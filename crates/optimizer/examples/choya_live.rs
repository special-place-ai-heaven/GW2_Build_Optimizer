//! The acceptance test for one model: does it produce a plate?
//!
//! Runs the real contract from `docs/llm-requirements.md` against the keys in
//! the addon's own `config.json` (found through `dev.cfg`'s `addons_dir`):
//! handshake probe, the real prompt with the full kitchen and all tools, the
//! tool loop, the same repairs the chat makes, parse, validate. One line per
//! model, PASS or FAIL, plus the request count and wall clock.
//!
//!   cargo run -p gw2-optimizer --example choya_live -- openrouter inclusionai/ling-3.0-flash-sante:free
//!   cargo run -p gw2-optimizer --example choya_live -- gemini gemini-3.8-flash
//!
//! Spends the player's quota: one run is 5–10 requests. Exit code 1 if any
//! model failed.

use std::time::Instant;

use gw2_core::config::{AppConfig, LlmProvider};
use gw2_optimizer::gemini_tools::{execute_tool, profession_reference, ToolContext};
use gw2_optimizer::llm::profile;

const PROFESSION: &str = "Necromancer";
const REQUEST: &str = "WvW roaming power build";

fn main() {
    let mut args = std::env::args().skip(1);
    let provider = match args.next().as_deref() {
        Some("gemini") => LlmProvider::Gemini,
        Some("openai") => LlmProvider::OpenAI,
        Some("anthropic") => LlmProvider::Anthropic,
        Some("openrouter") => LlmProvider::OpenRouter,
        _ => {
            eprintln!("usage: choya_live <gemini|openai|anthropic|openrouter> <model id> [more model ids]");
            std::process::exit(2);
        }
    };
    let models: Vec<String> = args.collect();
    if models.is_empty() {
        eprintln!("give at least one model id");
        std::process::exit(2);
    }

    let addon_dir = match gw2_api::dev_config::addons_dir() {
        Ok(dir) => dir.join("gw2_build_optimizer"),
        Err(e) => {
            eprintln!("no addons_dir in dev.cfg ({e})");
            std::process::exit(2);
        }
    };
    let (mut config, _) = AppConfig::load(&AppConfig::config_path(&addon_dir));
    config.active_provider = provider;
    let cache = gw2_api::cache::DataCache::new(addon_dir.join("cache"));
    let db = match gw2_optimizer::gamedb::GameDb::load(&cache) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("game data not cached ({e}) - sync it in-game first");
            std::process::exit(2);
        }
    };
    gw2_optimizer::llm::models_dev::load(&addon_dir);

    // The kitchen the chat builds, minus the deterministic reference build
    // (that lives in the addon crate). The profession reference is the part
    // that matters for the contract: ~26 KB the model must read, not fetch.
    let mut kitchen =
        String::from("Mode: WvW. Scale: Roam. Role: Roamer.\nCurrent build: none equipped.\n");
    kitchen.push_str(&profession_reference(&db, PROFESSION));
    let prompt = gw2_optimizer::prompts::chat_refinement_prompt_with_tools(
        PROFESSION, "WvW", REQUEST, &kitchen, "English",
    );
    let tools = gw2_optimizer::llm::tools::tool_definitions();
    let balance = gw2_optimizer::balance::BalanceContext::new(gw2_core::types::GameMode::WvW);
    let ctx = ToolContext {
        db: &db,
        profession_name: PROFESSION,
        candidates: &[],
        current_build_summary: Some(kitchen.as_str()),
        weights: gw2_optimizer::scoring::OptimizationWeights::preset_power_dps(),
        balance_ctx: &balance,
    };
    println!(
        "prompt {} chars (~{} tokens), {} tools",
        prompt.len(),
        prompt.len() / 4,
        tools.len()
    );

    let mut failed = false;
    for model in &models {
        config.set_active_model_id(model.clone());
        let started = Instant::now();
        let mut requests = 0usize;
        let outcome = (|| -> Result<String, String> {
            let client = gw2_optimizer::llm::create_client(&config, &addon_dir)
                .map_err(|e| e.to_string())?;
            let profile = match profile::probe(client.as_ref(), model, profile::now_secs()) {
                Ok(p) => {
                    requests += if p.tools == profile::ToolSupport::None {
                        1
                    } else {
                        2
                    };
                    println!("  handshake: {}", p.summary());
                    p
                }
                Err(e) => {
                    requests += 1;
                    println!("  handshake got no answer ({e}); assuming the full job");
                    profile::ModelProfile::assumed(model)
                }
            };
            let response = if profile.max_turns() == 0 {
                requests += 1;
                client
                    .generate_brief(&prompt, 8_192)
                    .map_err(|e| e.to_string())?
            } else {
                let mut round_started = Instant::now();
                client
                    .generate_with_tools_progress(
                        &prompt,
                        &tools,
                        &mut |name, args| execute_tool(name, args, &ctx),
                        profile.max_turns(),
                        &mut |turn, max, names| {
                            requests += 1;
                            println!(
                                "  round {turn}/{max} {:.1}s: {}",
                                round_started.elapsed().as_secs_f32(),
                                if names.is_empty() {
                                    "writing the build".to_string()
                                } else {
                                    names.join(", ")
                                }
                            );
                            round_started = Instant::now();
                        },
                    )
                    .map_err(|e| e.to_string())?
            };
            let parsed = match gw2_optimizer::prompts::parse_gemini_build(&response) {
                Ok(p) if !p.specializations.is_empty() => p,
                other => {
                    // The chat's repair request, verbatim in spirit.
                    let why = match &other {
                        Ok(_) => "no specializations",
                        Err(_) => "prose",
                    };
                    println!("  reply was {why}; one repair request");
                    requests += 1;
                    let repair = format!(
                        "You answered without a plate:\n\n{response}\n\nServe that as the \
                         plate now: ONLY the JSON build object from your instructions - \
                         \"specializations\" as three objects with \"name\", \"elite\" and \
                         \"traits\" (three each), \"weapons\", \"skills\", \"rune\", \
                         \"sigils\", \"relic\", \"stat_prefix\", \"explanation\". No text \
                         outside the JSON."
                    );
                    let again = client
                        .generate_brief(&repair, 8_192)
                        .map_err(|e| e.to_string())?;
                    gw2_optimizer::prompts::parse_gemini_build(&again)
                        .map_err(|e| format!("no plate after repair: {e}"))?
                }
            };
            if parsed.specializations.len() != 3 {
                return Err(format!(
                    "plate has {} specializations, not 3",
                    parsed.specializations.len()
                ));
            }
            let validated =
                gw2_optimizer::validation::validate_gemini_build(&parsed, &db, PROFESSION);
            if validated.specializations.len() != 3 {
                return Err(format!(
                    "only {} of 3 specializations validate against the game data: {}",
                    validated.specializations.len(),
                    parsed
                        .specializations
                        .iter()
                        .map(|(n, _)| n.as_str())
                        .collect::<Vec<_>>()
                        .join(" | ")
                ));
            }
            Ok(parsed
                .specializations
                .iter()
                .map(|(n, t)| format!("{n} [{}]", t.join(", ")))
                .collect::<Vec<_>>()
                .join(" | "))
        })();
        let secs = started.elapsed().as_secs_f32();
        match outcome {
            Ok(plate) => println!("PASS  {model:<48} {requests:>2} req {secs:6.1}s  {plate}"),
            Err(e) => {
                failed = true;
                let e: String = e.chars().take(200).collect();
                println!("FAIL  {model:<48} {requests:>2} req {secs:6.1}s  {e}");
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
