//! One tool loop above four providers.
//!
//! Each provider used to run its own copy of the loop, and every fix landed
//! in one copy: the deadline salvage and the narration nudge reached OpenAI
//! and OpenRouter, the capped closing request never reached Anthropic, the
//! Gemini loop kept its own "no text response". This module owns the loop;
//! a provider implements [`TurnDriver`] — open a conversation, take one
//! turn, push tool results — in its own wire format, keeping whatever the
//! provider needs carried between turns (OpenAI reasoning details,
//! Anthropic content blocks, Gemini parts), and the loop supplies:
//!
//! - cancellation between turns, a lookup budget judged before a round from
//!   how long the last one took, deadline salvage after the first round;
//! - `tool_choice: required` on the first round where the model takes it;
//! - argument validation against each tool's schema, so a call missing a
//!   required field is answered with a message the model can act on rather
//!   than executed and guessed at;
//! - repeated-call detection: the same call again gets the same result and
//!   a note; a whole round repeated twice ends gathering;
//! - the narration nudge, once; the closing request with the plate held to
//!   whatever the provider can enforce; and the last text seen as the
//!   answer of last resort when the closing request itself fails.

use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::openai_compat::{is_narration, CLOSING_TURN, CONTINUE_TURN, TOOL_PHASE_BUDGET};
use super::ToolDefinition;

/// One tool call the model made, in provider-neutral form.
#[derive(Debug, Clone)]
pub struct ToolCall {
    /// The provider's id for the call, echoed back with the result. Gemini
    /// has none; its driver uses the function name.
    pub id: String,
    pub name: String,
    /// Parsed arguments. Arguments the driver could not parse arrive as
    /// `{"error": "unparseable arguments: ..."}` and are not executed; the
    /// object is returned to the model as the result, so it can retry.
    pub args: Value,
}

/// What one model turn contained.
#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub text: Option<String>,
    pub calls: Vec<ToolCall>,
}

/// How the driver should shape a turn's request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnMode {
    /// A lookup round: tools on, the usual budget. `force_tool` asks for
    /// `tool_choice: required` where the provider supports it.
    Explore { force_tool: bool },
    /// The request that writes the answer: no tools, small cap, no
    /// reasoning budget, the plate's schema where the model can hold it.
    Closing,
}

/// What the loop may ask of a provider.
#[derive(Debug, Clone, Copy, Default)]
pub struct LoopCaps {
    /// `tool_choice: required` is understood.
    pub tool_choice: bool,
}

/// A provider's side of the loop: its conversation, its wire format, its
/// error type. The driver pushes every assistant turn into the conversation
/// itself, so provider-specific fields (reasoning details, signatures) are
/// never flattened through this module.
pub(crate) trait TurnDriver {
    type Conv;
    type Err;

    fn open(&self, prompt: &str) -> Self::Conv;
    /// Keep the conversation inside the prompt budget.
    fn trim(&self, conv: &mut Self::Conv);
    /// One request. Must append the assistant's turn to `conv` before
    /// returning, in the provider's own representation.
    fn turn(
        &self,
        conv: &mut Self::Conv,
        tools: Option<&[ToolDefinition]>,
        mode: TurnMode,
    ) -> Result<Turn, Self::Err>;
    fn push_tool_results(&self, conv: &mut Self::Conv, results: &[(ToolCall, Value)]);
    fn push_user(&self, conv: &mut Self::Conv, text: &str);
    fn caps(&self) -> LoopCaps;

    fn cancelled(&self) -> Self::Err;
    fn no_answer(&self, detail: String) -> Self::Err;
    /// The request ran out its deadline (a round that took too long).
    fn is_deadline(&self, err: &Self::Err) -> bool;
    /// The model cannot produce a usable function call at all.
    fn is_function_call_failure(&self, err: &Self::Err) -> bool;
}

/// Run a tool-driven conversation to an answer.
pub(crate) fn run<D: TurnDriver>(
    driver: &D,
    prompt: &str,
    tools: &[ToolDefinition],
    execute_tool: &mut dyn FnMut(&str, &Value) -> Value,
    max_turns: usize,
    on_progress: &mut dyn FnMut(usize, usize, &[String]),
) -> Result<String, D::Err> {
    let mut conv = driver.open(prompt);
    let caps = driver.caps();
    let gathering_until = Instant::now() + TOOL_PHASE_BUDGET;
    let mut last_round = Duration::ZERO;
    let mut nudged = false;
    let mut last_text: Option<String> = None;
    // Every call made so far, so a repeat costs nothing and is named as one.
    let mut seen: HashMap<String, Value> = HashMap::new();
    let mut last_round_key: Option<String> = None;
    let mut repeated_rounds = 0u32;

    for turn_index in 0..max_turns {
        if super::cancel::is_cancelled() {
            return Err(driver.cancelled());
        }
        // Out of clock for lookups — or about to be, if the next round
        // takes what the last one did.
        if turn_index > 0 && Instant::now() + last_round >= gathering_until {
            break;
        }
        let round_started = Instant::now();
        driver.trim(&mut conv);
        let mode = TurnMode::Explore {
            force_tool: turn_index == 0 && caps.tool_choice && !tools.is_empty(),
        };
        let turn = match driver.turn(&mut conv, Some(tools), mode) {
            Ok(turn) => turn,
            // The model cannot drive our tools at all; answer without them.
            Err(e) if driver.is_function_call_failure(&e) => break,
            // One round ran out its deadline. Earlier rounds gathered real
            // results; answer from them rather than report a stopwatch.
            Err(e) if driver.is_deadline(&e) && turn_index > 0 => break,
            Err(e) => return Err(e),
        };

        if turn.calls.is_empty() {
            let text = turn.text.unwrap_or_default();
            // Narration is not an answer: tell it to act, once.
            if !nudged && turn_index + 1 < max_turns && is_narration(&text) {
                nudged = true;
                driver.push_user(&mut conv, CONTINUE_TURN);
                last_round = round_started.elapsed();
                continue;
            }
            if text.is_empty() {
                return Err(driver.no_answer(format!("No response text on turn {turn_index}")));
            }
            return Ok(text);
        }
        if let Some(text) = turn.text.as_ref().filter(|t| !t.trim().is_empty()) {
            last_text = Some(text.clone());
        }

        let names: Vec<String> = turn.calls.iter().map(|c| c.name.clone()).collect();
        on_progress(turn_index + 1, max_turns, &names);

        let mut results = Vec::with_capacity(turn.calls.len());
        let mut round_keys: Vec<String> = Vec::with_capacity(turn.calls.len());
        for call in turn.calls {
            let key = call_key(&call);
            round_keys.push(key.clone());
            let result = if super::unparseable_tool_input(&call.args) {
                call.args.clone()
            } else if let Some(problem) = validate_args(tools, &call.name, &call.args) {
                json!({ "error": problem })
            } else if let Some(previous) = seen.get(&key) {
                let mut again = previous.clone();
                if let Some(obj) = again.as_object_mut() {
                    obj.insert(
                        "note".to_string(),
                        Value::String(
                            "You already made this exact call; this is the same result. \
                             Do not call it again - use it."
                                .to_string(),
                        ),
                    );
                }
                again
            } else {
                let value = execute_tool(&call.name, &call.args);
                seen.insert(key, value.clone());
                value
            };
            results.push((call, result));
        }
        driver.push_tool_results(&mut conv, &results);

        // A round identical to the last one, twice, is a model going in
        // circles; the answer it can give, it can give now.
        round_keys.sort();
        let round_key = round_keys.join("\n");
        if last_round_key.as_deref() == Some(round_key.as_str()) {
            repeated_rounds += 1;
            if repeated_rounds >= 2 {
                break;
            }
        } else {
            repeated_rounds = 0;
        }
        last_round_key = Some(round_key);
        last_round = round_started.elapsed();
    }

    // Gathering is over — turns, clock, circles, or a model that cannot call
    // tools — and there is no answer yet. Every result is in the
    // conversation; one request with the tools withheld writes the answer
    // from them.
    if super::cancel::is_cancelled() {
        return Err(driver.cancelled());
    }
    on_progress(max_turns, max_turns, &[]);
    driver.push_user(&mut conv, CLOSING_TURN);
    driver.trim(&mut conv);
    match driver.turn(&mut conv, None, TurnMode::Closing) {
        Ok(turn) => turn
            .text
            .filter(|t| !t.is_empty())
            .or(last_text)
            .ok_or_else(|| {
                driver.no_answer(format!(
                    "Tool loop exceeded {max_turns} turns with no answer"
                ))
            }),
        // The closing request is the last chance, not the only evidence:
        // text from an earlier turn still beats an error.
        Err(e) => last_text.ok_or(e),
    }
}

/// A stable identity for one call: name plus arguments with keys sorted.
fn call_key(call: &ToolCall) -> String {
    fn canonical(v: &Value) -> Value {
        match v {
            Value::Object(map) => Value::Object(
                map.iter()
                    .map(|(k, v)| (k.clone(), canonical(v)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            Value::Array(items) => Value::Array(items.iter().map(canonical).collect()),
            other => other.clone(),
        }
    }
    format!("{}:{}", call.name, canonical(&call.args))
}

/// Whether `args` satisfies the tool's declared schema well enough to run:
/// every `required` property present, each declared property of the type
/// the schema names. Returns what is wrong, for the model, or `None`.
pub fn validate_args(tools: &[ToolDefinition], name: &str, args: &Value) -> Option<String> {
    let def = tools.iter().find(|t| t.name == name)?;
    let schema = &def.parameters;
    let Some(object) = args.as_object() else {
        return Some(format!("{name}: arguments must be a JSON object"));
    };
    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let missing: Vec<&str> = required
            .iter()
            .filter_map(Value::as_str)
            .filter(|k| !object.contains_key(*k))
            .collect();
        if !missing.is_empty() {
            return Some(format!(
                "{name}: missing required argument(s) {}",
                missing.join(", ")
            ));
        }
    }
    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (key, value) in object {
            let Some(expected) = props
                .get(key)
                .and_then(|p| p.get("type"))
                .and_then(Value::as_str)
            else {
                continue;
            };
            let ok = match expected {
                "string" => value.is_string(),
                "integer" => value.is_i64() || value.is_u64(),
                "number" => value.is_number(),
                "boolean" => value.is_boolean(),
                "array" => value.is_array(),
                "object" => value.is_object(),
                _ => true,
            };
            if !ok && !value.is_null() {
                return Some(format!("{name}: argument \"{key}\" must be a {expected}"));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    /// A scripted model: each entry is one turn's reply.
    struct Script {
        turns: RefCell<Vec<Turn>>,
        log: RefCell<Vec<String>>,
        caps: LoopCaps,
    }

    impl Script {
        fn new(turns: Vec<Turn>) -> Self {
            Self {
                turns: RefCell::new(turns),
                log: RefCell::new(Vec::new()),
                caps: LoopCaps { tool_choice: true },
            }
        }
    }

    impl TurnDriver for Script {
        type Conv = Vec<String>;
        type Err = String;
        fn open(&self, prompt: &str) -> Vec<String> {
            vec![format!("user:{prompt}")]
        }
        fn trim(&self, _conv: &mut Vec<String>) {}
        fn turn(
            &self,
            conv: &mut Vec<String>,
            tools: Option<&[ToolDefinition]>,
            mode: TurnMode,
        ) -> Result<Turn, String> {
            self.log
                .borrow_mut()
                .push(format!("turn tools={} mode={mode:?}", tools.is_some()));
            // The closing request never sees the script: it answers.
            let turn = if mode == TurnMode::Closing || self.turns.borrow().is_empty() {
                Turn {
                    text: Some("closing answer".into()),
                    calls: vec![],
                }
            } else {
                self.turns.borrow_mut().remove(0)
            };
            conv.push(format!("assistant:{:?}", turn.text));
            Ok(turn)
        }
        fn push_tool_results(&self, conv: &mut Vec<String>, results: &[(ToolCall, Value)]) {
            for (call, value) in results {
                conv.push(format!("tool:{}={value}", call.name));
            }
        }
        fn push_user(&self, conv: &mut Vec<String>, text: &str) {
            conv.push(format!("user:{text}"));
        }
        fn caps(&self) -> LoopCaps {
            self.caps
        }
        fn cancelled(&self) -> String {
            "cancelled".into()
        }
        fn no_answer(&self, detail: String) -> String {
            detail
        }
        fn is_deadline(&self, e: &String) -> bool {
            e.contains("deadline")
        }
        fn is_function_call_failure(&self, e: &String) -> bool {
            e.contains("MALFORMED")
        }
    }

    fn tool() -> ToolDefinition {
        ToolDefinition {
            name: "lookup".into(),
            description: "".into(),
            parameters: json!({
                "type": "object",
                "properties": { "id": { "type": "integer" }, "name": { "type": "string" } },
                "required": ["id"]
            }),
        }
    }

    fn call(args: Value) -> ToolCall {
        ToolCall {
            id: "c".into(),
            name: "lookup".into(),
            args,
        }
    }

    #[test]
    fn a_missing_required_argument_is_answered_not_executed() {
        let script = Script::new(vec![
            Turn {
                text: None,
                calls: vec![call(json!({ "name": "x" }))],
            },
            Turn {
                text: Some("done".into()),
                calls: vec![],
            },
        ]);
        let mut executed = 0;
        let out = run(
            &script,
            "p",
            &[tool()],
            &mut |_, _| {
                executed += 1;
                json!({})
            },
            8,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out, "done");
        assert_eq!(
            executed, 0,
            "the tool must not run without its required argument"
        );
    }

    #[test]
    fn the_same_call_twice_runs_once_and_says_so() {
        let script = Script::new(vec![
            Turn {
                text: None,
                calls: vec![call(json!({ "id": 1 }))],
            },
            Turn {
                text: None,
                calls: vec![call(json!({ "id": 1 }))],
            },
            Turn {
                text: Some("done".into()),
                calls: vec![],
            },
        ]);
        let mut executed = 0;
        run(
            &script,
            "p",
            &[tool()],
            &mut |_, _| {
                executed += 1;
                json!({ "v": 1 })
            },
            8,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(executed, 1);
    }

    #[test]
    fn three_identical_rounds_end_gathering_and_close() {
        let round = || Turn {
            text: None,
            calls: vec![call(json!({ "id": 2 }))],
        };
        let script = Script::new(vec![round(), round(), round(), round(), round()]);
        let out = run(
            &script,
            "p",
            &[tool()],
            &mut |_, _| json!({}),
            8,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out, "closing answer");
        let log = script.log.borrow();
        assert_eq!(
            log.iter().filter(|l| l.contains("Explore")).count(),
            3,
            "two repeats after the first round, then the closing request"
        );
        assert!(log.last().unwrap().contains("Closing"));
    }

    #[test]
    fn narration_is_nudged_once_and_the_first_round_forces_a_tool() {
        let script = Script::new(vec![
            Turn {
                text: Some("I'll start by checking the traits.".into()),
                calls: vec![],
            },
            Turn {
                text: Some("Here is the answer.".into()),
                calls: vec![],
            },
        ]);
        let out = run(
            &script,
            "p",
            &[tool()],
            &mut |_, _| json!({}),
            8,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out, "Here is the answer.");
        let log = script.log.borrow();
        assert!(log[0].contains("force_tool: true"));
        assert!(log[1].contains("force_tool: false"));
    }

    #[test]
    fn the_last_text_seen_beats_a_failed_closing_request() {
        struct Failing(Script);
        impl TurnDriver for Failing {
            type Conv = Vec<String>;
            type Err = String;
            fn open(&self, p: &str) -> Vec<String> {
                self.0.open(p)
            }
            fn trim(&self, c: &mut Vec<String>) {
                self.0.trim(c)
            }
            fn turn(
                &self,
                c: &mut Vec<String>,
                t: Option<&[ToolDefinition]>,
                m: TurnMode,
            ) -> Result<Turn, String> {
                if m == TurnMode::Closing {
                    return Err("deadline".into());
                }
                self.0.turn(c, t, m)
            }
            fn push_tool_results(&self, c: &mut Vec<String>, r: &[(ToolCall, Value)]) {
                self.0.push_tool_results(c, r)
            }
            fn push_user(&self, c: &mut Vec<String>, t: &str) {
                self.0.push_user(c, t)
            }
            fn caps(&self) -> LoopCaps {
                self.0.caps()
            }
            fn cancelled(&self) -> String {
                self.0.cancelled()
            }
            fn no_answer(&self, d: String) -> String {
                self.0.no_answer(d)
            }
            fn is_deadline(&self, e: &String) -> bool {
                self.0.is_deadline(e)
            }
            fn is_function_call_failure(&self, e: &String) -> bool {
                self.0.is_function_call_failure(e)
            }
        }
        let driver = Failing(Script::new(vec![Turn {
            text: Some("draft".into()),
            calls: vec![call(json!({ "id": 3 }))],
        }]));
        let out = run(
            &driver,
            "p",
            &[tool()],
            &mut |_, _| json!({}),
            1,
            &mut |_, _, _| {},
        )
        .unwrap();
        assert_eq!(out, "draft");
    }

    #[test]
    fn validate_args_reads_the_schema() {
        let t = [tool()];
        assert_eq!(validate_args(&t, "lookup", &json!({ "id": 1 })), None);
        assert!(validate_args(&t, "lookup", &json!({}))
            .unwrap()
            .contains("id"));
        assert!(validate_args(&t, "lookup", &json!({ "id": "one" }))
            .unwrap()
            .contains("integer"));
        assert_eq!(validate_args(&t, "unknown", &json!({})), None);
    }
}
