//! models.dev — the open database of what each model can do, for every
//! provider (`https://models.dev/api.json`, ~4.5 MB, 213 providers).
//!
//! OpenRouter's own catalog lists what a model accepts; OpenAI, Anthropic
//! and Google's do not, so until this existed the picker took every model
//! from those three on faith. models.dev answers, per model, the questions
//! the addon actually has: `tool_call`, `structured_output`, `reasoning`,
//! and the output limit. It also settled 2026-09-07's no-build runs in one
//! line — `minimax/minimax-m3:free` has `structured_output: false`.
//!
//! Fetched once a day into `models_dev.json` beside the usage counters, read
//! from disk otherwise, absent without complaint: a model the database does
//! not know keeps whatever its own provider said.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Deserialize;

const API_URL: &str = "https://models.dev/api.json";
const FILE_NAME: &str = "models_dev.json";
const REFRESH_AFTER: Duration = Duration::from_secs(24 * 3600);
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// What models.dev says about one model, reduced to what the addon asks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Facts {
    pub tool_call: bool,
    pub structured_output: bool,
    pub reasoning: bool,
    /// Largest reply, in tokens, when published.
    pub output_limit: Option<u32>,
    pub context: Option<u32>,
}

#[derive(Deserialize)]
struct RawModel {
    #[serde(default)]
    tool_call: bool,
    #[serde(default)]
    structured_output: bool,
    #[serde(default)]
    reasoning: bool,
    #[serde(default)]
    limit: RawLimit,
}

#[derive(Deserialize, Default)]
struct RawLimit {
    context: Option<u32>,
    output: Option<u32>,
}

#[derive(Deserialize)]
struct RawProvider {
    #[serde(default)]
    models: HashMap<String, RawModel>,
}

/// provider id -> model id -> facts
type Table = HashMap<String, HashMap<String, Facts>>;

static TABLE: OnceLock<Mutex<Option<Table>>> = OnceLock::new();

fn parse(text: &str) -> Option<Table> {
    let raw: HashMap<String, RawProvider> = serde_json::from_str(text).ok()?;
    Some(
        raw.into_iter()
            .map(|(provider, p)| {
                let models = p
                    .models
                    .into_iter()
                    .map(|(id, m)| {
                        (
                            id,
                            Facts {
                                tool_call: m.tool_call,
                                structured_output: m.structured_output,
                                reasoning: m.reasoning,
                                output_limit: m.limit.output,
                                context: m.limit.context,
                            },
                        )
                    })
                    .collect();
                (provider, models)
            })
            .collect(),
    )
}

/// Load from disk, refreshing from models.dev when the file is missing or a
/// day old. Safe to call from a background thread; never from the render
/// thread (it may download).
pub fn load(addon_dir: &Path) {
    let path = addon_dir.join(FILE_NAME);
    let fresh = std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age < REFRESH_AFTER);
    if !fresh {
        if let Ok(client) = reqwest::blocking::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .build()
        {
            if let Ok(text) = client.get(API_URL).send().and_then(|r| r.text()) {
                if parse(&text).is_some() {
                    let tmp = path.with_extension("json.tmp");
                    if std::fs::write(&tmp, &text).is_ok() {
                        let _ = std::fs::rename(&tmp, &path);
                    }
                }
            }
        }
    }
    let table = std::fs::read_to_string(&path).ok().and_then(|t| parse(&t));
    if let Ok(mut slot) = TABLE.get_or_init(|| Mutex::new(None)).lock() {
        *slot = table;
    }
}

/// What models.dev knows about `model_id` at `provider` (models.dev's
/// provider ids: `openrouter`, `openai`, `anthropic`, `google`).
pub fn facts(provider: &str, model_id: &str) -> Option<Facts> {
    let slot = TABLE.get()?.lock().ok()?;
    slot.as_ref()?.get(provider)?.get(model_id).copied()
}

/// Fold what models.dev knows into a provider's own model list: tool
/// support where the provider published none, structured-output support,
/// and an output limit where the provider published none.
pub fn enrich(provider: &str, models: &mut [super::ModelInfo]) {
    for m in models.iter_mut() {
        let Some(f) = facts(provider, &m.id) else {
            continue;
        };
        if m.supported_parameters.is_empty() {
            // The provider said nothing; models.dev is the only word.
            m.tools = f.tool_call;
        }
        m.structured_output = Some(f.structured_output);
        if m.max_completion_tokens.is_none() {
            m.max_completion_tokens = f.output_limit;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_shape_models_dev_publishes() {
        let text = r#"{
          "openrouter": {"id": "openrouter", "models": {
            "minimax/minimax-m3:free": {"tool_call": true, "structured_output": false, "reasoning": true,
                                        "limit": {"context": 1048576, "output": 943718}},
            "img/gen": {"tool_call": false}
          }},
          "openai": {"models": {"gpt-5": {"tool_call": true, "structured_output": true}}}
        }"#;
        let t = parse(text).unwrap();
        let mm = t["openrouter"]["minimax/minimax-m3:free"];
        assert!(mm.tool_call && mm.reasoning && !mm.structured_output);
        assert_eq!(mm.output_limit, Some(943_718));
        assert!(!t["openrouter"]["img/gen"].tool_call);
        assert!(t["openai"]["gpt-5"].structured_output);
    }

    #[test]
    fn enrich_only_speaks_where_the_provider_did_not() {
        if let Ok(mut slot) = TABLE.get_or_init(|| Mutex::new(None)).lock() {
            let mut models = HashMap::new();
            models.insert(
                "m".to_string(),
                Facts {
                    tool_call: false,
                    structured_output: true,
                    reasoning: false,
                    output_limit: Some(4096),
                    context: None,
                },
            );
            let mut table = HashMap::new();
            table.insert("openai".to_string(), models);
            *slot = Some(table);
        }
        let mut quiet = vec![super::super::ModelInfo {
            id: "m".into(),
            ..Default::default()
        }];
        enrich("openai", &mut quiet);
        assert!(
            !quiet[0].tools,
            "provider said nothing, models.dev says no tools"
        );
        assert_eq!(quiet[0].structured_output, Some(true));
        assert_eq!(quiet[0].max_completion_tokens, Some(4096));

        let mut loud = vec![super::super::ModelInfo {
            id: "m".into(),
            supported_parameters: vec!["tools".into()],
            ..Default::default()
        }];
        enrich("openai", &mut loud);
        assert!(
            loud[0].tools,
            "the provider's own catalog keeps the last word on tools"
        );
    }
}
