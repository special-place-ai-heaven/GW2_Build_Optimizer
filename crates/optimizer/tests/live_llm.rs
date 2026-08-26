//! Live integration tests for all LLM providers.
//!
//! These tests make REAL API calls — they are `#[ignore]`d by default.
//! Run one provider at a time with the appropriate env var set:
//!
//! ```bash
//! # Gemini
//! GEMINI_API_KEY=your-key cargo test -p gw2-optimizer --test live_llm -- --ignored --nocapture test_gemini
//!
//! # OpenAI
//! OPENAI_API_KEY=your-key cargo test -p gw2-optimizer --test live_llm -- --ignored --nocapture test_openai
//!
//! # Anthropic
//! ANTHROPIC_API_KEY=your-key cargo test -p gw2-optimizer --test live_llm -- --ignored --nocapture test_anthropic
//!
//! # All at once (needs all three keys)
//! GEMINI_API_KEY=... OPENAI_API_KEY=... ANTHROPIC_API_KEY=... \
//!   cargo test -p gw2-optimizer --test live_llm -- --ignored --nocapture
//!
//! # Canonical-build smoke suite — one command, all three providers,
//! # asserts the response validates against a real GameDb.
//! # Also set GW2_OPTIMIZER_CACHE_DIR to a populated cache directory
//! # (e.g. the addon's cache/ folder). Per-provider sections skip
//! # independently when their API key env var is missing.
//! GW2_OPTIMIZER_CACHE_DIR=~/AppData/Roaming/Guild\ Wars\ 2/addons/gw2_build_optimizer/cache \
//!   GEMINI_API_KEY=... OPENAI_API_KEY=... ANTHROPIC_API_KEY=... \
//!   cargo test -p gw2-optimizer --test live_llm -- --ignored --nocapture canonical_build_smoke
//! ```

use gw2_api::cache::DataCache;
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::llm::{LlmClient, LlmError, ToolDefinition};
use gw2_optimizer::prompts::{new_build_prompt_with_tools, parse_gemini_build};
use gw2_optimizer::scoring::OptimizationWeights;
use gw2_optimizer::validation::validate_gemini_build;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::time::Instant;

// ─── Helpers ───

fn gemini_key() -> String {
    std::env::var("GEMINI_API_KEY").expect("Set GEMINI_API_KEY env var to run this test")
}

fn openai_key() -> String {
    std::env::var("OPENAI_API_KEY").expect("Set OPENAI_API_KEY env var to run this test")
}

fn anthropic_key() -> String {
    std::env::var("ANTHROPIC_API_KEY").expect("Set ANTHROPIC_API_KEY env var to run this test")
}

fn openrouter_key() -> String {
    std::env::var("OPENROUTER_API_KEY").expect("Set OPENROUTER_API_KEY env var to run this test")
}

/// A simple tool for testing tool-calling: takes a number, returns its square.
fn test_tools() -> Vec<ToolDefinition> {
    vec![ToolDefinition {
        name: "square_number".to_string(),
        description: "Returns the square of the given number.".to_string(),
        parameters: json!({
            "type": "object",
            "properties": {
                "number": {
                    "type": "number",
                    "description": "The number to square"
                }
            },
            "required": ["number"]
        }),
    }]
}

fn execute_test_tool(_name: &str, args: &Value) -> Value {
    let n = args.get("number").and_then(|v| v.as_f64()).unwrap_or(0.0);
    json!({ "result": n * n })
}

/// Run the standard test suite against any LlmClient implementation.
fn run_provider_tests(client: &dyn LlmClient) {
    let provider = client.provider_name();

    // 1. validate_key
    print!("  [{}] validate_key... ", provider);
    let start = Instant::now();
    match client.validate_key() {
        Ok(()) => println!("OK ({:.1}s)", start.elapsed().as_secs_f64()),
        Err(e) => {
            println!("FAILED: {}", e);
            panic!("{} validate_key failed: {}", provider, e);
        }
    }

    // 2. Simple generate
    print!("  [{}] generate (simple prompt)... ", provider);
    let start = Instant::now();
    let prompt = "Reply with exactly the word 'PONG' and nothing else.";
    match client.generate(prompt) {
        Ok(response) => {
            let trimmed = response.trim().to_uppercase();
            println!(
                "OK ({:.1}s) — got '{}' (len={})",
                start.elapsed().as_secs_f64(),
                &response.chars().take(80).collect::<String>(),
                response.len()
            );
            assert!(
                trimmed.contains("PONG"),
                "{} generate: expected response containing 'PONG', got: {}",
                provider,
                response
            );
        }
        Err(e) => {
            println!("FAILED: {}", e);
            panic!("{} generate failed: {}", provider, e);
        }
    }

    // 3. generate_cached (should return same result and be faster on second call)
    print!("  [{}] generate_cached (first call)... ", provider);
    let start = Instant::now();
    let cache_prompt = "What is 2+2? Reply with just the number.";
    let first = client
        .generate_cached(cache_prompt)
        .expect("generate_cached first call");
    let first_time = start.elapsed();
    println!("OK ({:.1}s) — '{}'", first_time.as_secs_f64(), first.trim());

    print!(
        "  [{}] generate_cached (second call, should be cached)... ",
        provider
    );
    let start = Instant::now();
    let second = client
        .generate_cached(cache_prompt)
        .expect("generate_cached second call");
    let second_time = start.elapsed();
    println!(
        "OK ({:.3}s) — '{}'",
        second_time.as_secs_f64(),
        second.trim()
    );
    assert_eq!(first, second, "Cached response should be identical");
    assert!(
        second_time < first_time || second_time.as_millis() < 100,
        "Cached call should be faster (first={:.1}s, second={:.3}s)",
        first_time.as_secs_f64(),
        second_time.as_secs_f64()
    );

    // 4. generate_with_tools_progress
    print!("  [{}] generate_with_tools (square 7)... ", provider);
    let start = Instant::now();
    let tools = test_tools();
    let tool_prompt =
        "What is 7 squared? Use the square_number tool to compute it, then tell me the result.";
    let mut turns_seen = 0usize;
    let result = client.generate_with_tools_progress(
        tool_prompt,
        &tools,
        &mut |name: &str, args: &Value| {
            println!("    [tool call] {}({})", name, args);
            execute_test_tool(name, args)
        },
        5,
        &mut |turn: usize, max_turns: usize, tool_names: &[String]| {
            turns_seen = turn;
            println!(
                "    [progress] turn {}/{} tools={:?}",
                turn, max_turns, tool_names
            );
        },
    );
    match result {
        Ok(response) => {
            println!(
                "  OK ({:.1}s, {} tool turns) — '{}'",
                start.elapsed().as_secs_f64(),
                turns_seen,
                &response.chars().take(120).collect::<String>()
            );
            assert!(
                response.contains("49"),
                "{} tool calling: expected '49' in response, got: {}",
                provider,
                response
            );
        }
        Err(e) => {
            println!("FAILED: {}", e);
            panic!("{} generate_with_tools failed: {}", provider, e);
        }
    }

    // 5. remaining_quota (should have decremented from calls above)
    let quota = client.remaining_quota();
    println!("  [{}] remaining_quota: {}", provider, quota);

    // 6. Invalid key test
    println!("  [{}] All live tests PASSED", provider);
}

// ─── Gemini Tests ───

#[test]
#[ignore]
fn test_gemini_validate_and_generate() {
    let key = gemini_key();
    let client = gw2_optimizer::llm::gemini::GeminiLlmClient::new(&key, "gemini-2.5-flash")
        .expect("Failed to create Gemini client");
    println!("\n=== Gemini Live Tests ===");
    run_provider_tests(&client);
}

#[test]
#[ignore]
fn test_gemini_invalid_key() {
    let client =
        gw2_optimizer::llm::gemini::GeminiLlmClient::new("invalid-key-12345", "gemini-2.5-flash")
            .expect("Client construction should not fail");
    let result = client.validate_key();
    println!("Gemini invalid key result: {:?}", result);
    assert!(
        matches!(
            result,
            Err(LlmError::InvalidKey) | Err(LlmError::Api { .. })
        ),
        "Expected InvalidKey or Api error for bad key, got: {:?}",
        result
    );
}

// ─── OpenAI Tests ───

#[test]
#[ignore]
fn test_openai_validate_and_generate() {
    let key = openai_key();
    let client = gw2_optimizer::llm::openai::OpenAiClient::new(&key, "gpt-4o-mini")
        .expect("Failed to create OpenAI client");
    println!("\n=== OpenAI Live Tests ===");
    run_provider_tests(&client);
}

#[test]
#[ignore]
fn test_openai_invalid_key() {
    let client =
        gw2_optimizer::llm::openai::OpenAiClient::new("sk-invalid-key-12345", "gpt-4o-mini")
            .expect("Client construction should not fail");
    let result = client.validate_key();
    println!("OpenAI invalid key result: {:?}", result);
    assert!(
        matches!(
            result,
            Err(LlmError::InvalidKey) | Err(LlmError::Api { .. })
        ),
        "Expected InvalidKey or Api error for bad key, got: {:?}",
        result
    );
}

// ─── Anthropic Tests ───

#[test]
#[ignore]
fn test_anthropic_validate_and_generate() {
    let key = anthropic_key();
    let client =
        gw2_optimizer::llm::anthropic::AnthropicClient::new(&key, "claude-haiku-4-5-20251001")
            .expect("Failed to create Anthropic client");
    println!("\n=== Anthropic Live Tests ===");
    run_provider_tests(&client);
}

#[test]
#[ignore]
fn test_anthropic_invalid_key() {
    let client = gw2_optimizer::llm::anthropic::AnthropicClient::new(
        "sk-ant-invalid-12345",
        "claude-haiku-4-5-20251001",
    )
    .expect("Client construction should not fail");
    let result = client.validate_key();
    println!("Anthropic invalid key result: {:?}", result);
    assert!(
        matches!(
            result,
            Err(LlmError::InvalidKey) | Err(LlmError::Api { .. })
        ),
        "Expected InvalidKey or Api error for bad key, got: {:?}",
        result
    );
}

// ─── OpenRouter Tests ───

#[test]
#[ignore]
fn test_openrouter_validate_and_generate() {
    let key = openrouter_key();
    let client = gw2_optimizer::llm::openrouter::OpenRouterClient::new(&key, "z-ai/glm-5.3-flash")
        .expect("Failed to create OpenRouter client");
    println!("\n=== OpenRouter Live Tests (streamed) ===");
    run_provider_tests(&client);
}

#[test]
#[ignore]
fn test_openrouter_invalid_key() {
    let client = gw2_optimizer::llm::openrouter::OpenRouterClient::new(
        "sk-or-invalid-12345",
        "z-ai/glm-5.3-flash",
    )
    .expect("Client construction should not fail");
    let result = client.validate_key();
    println!("OpenRouter invalid key result: {:?}", result);
    assert!(
        matches!(
            result,
            Err(LlmError::InvalidKey) | Err(LlmError::Api { .. })
        ),
        "Expected InvalidKey or Api error for bad key, got: {:?}",
        result
    );
}

// ─── Factory Test ───

#[test]
#[ignore]
fn test_create_client_factory_gemini() {
    let key = gemini_key();
    let config = gw2_core::config::AppConfig {
        gemini_api_key: Some(key),
        active_provider: gw2_core::config::LlmProvider::Gemini,
        ..Default::default()
    };
    let tmp = std::env::temp_dir().join("gw2_llm_test_factory");
    std::fs::create_dir_all(&tmp).ok();

    let client = gw2_optimizer::llm::create_client(&config, &tmp)
        .expect("create_client should succeed with valid Gemini config");
    assert_eq!(client.provider_name(), "Gemini");
    client
        .validate_key()
        .expect("Key should be valid via factory");
    println!("[OK] create_client factory → Gemini validate_key passed");

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
#[ignore]
fn test_create_client_factory_openai() {
    let key = openai_key();
    let config = gw2_core::config::AppConfig {
        openai_api_key: Some(key),
        active_provider: gw2_core::config::LlmProvider::OpenAI,
        ..Default::default()
    };
    let tmp = std::env::temp_dir().join("gw2_llm_test_factory_openai");
    std::fs::create_dir_all(&tmp).ok();

    let client = gw2_optimizer::llm::create_client(&config, &tmp)
        .expect("create_client should succeed with valid OpenAI config");
    assert_eq!(client.provider_name(), "OpenAI");
    client
        .validate_key()
        .expect("Key should be valid via factory");
    println!("[OK] create_client factory → OpenAI validate_key passed");

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
#[ignore]
fn test_create_client_factory_anthropic() {
    let key = anthropic_key();
    let config = gw2_core::config::AppConfig {
        anthropic_api_key: Some(key),
        active_provider: gw2_core::config::LlmProvider::Anthropic,
        ..Default::default()
    };
    let tmp = std::env::temp_dir().join("gw2_llm_test_factory_anthropic");
    std::fs::create_dir_all(&tmp).ok();

    let client = gw2_optimizer::llm::create_client(&config, &tmp)
        .expect("create_client should succeed with valid Anthropic config");
    assert_eq!(client.provider_name(), "Anthropic");
    client
        .validate_key()
        .expect("Key should be valid via factory");
    println!("[OK] create_client factory → Anthropic validate_key passed");

    std::fs::remove_dir_all(&tmp).ok();
}

// ─── Canonical Build Smoke Suite ─────────────────────────────────────────
//
// One command, all three providers: send the canonical new-build prompt
// and assert the response parses and validates against a real GameDb.
// Gated on GW2_OPTIMIZER_CACHE_DIR *and* each provider's API key env var.
// Per-provider sections skip independently if a key is missing; the whole
// test skips if the cache dir is absent.

fn load_game_db_for_smoke() -> Option<GameDb> {
    let dir = std::env::var("GW2_OPTIMIZER_CACHE_DIR").ok()?;
    let cache = DataCache::new(PathBuf::from(dir));
    match GameDb::load(&cache) {
        Ok(db) => Some(db),
        Err(e) => {
            eprintln!("[skip] GameDb::load failed: {}", e);
            None
        }
    }
}

fn run_canonical_build_smoke(client: &dyn LlmClient, db: &GameDb) {
    let provider = client.provider_name();
    let weights = OptimizationWeights::preset_power_dps();
    let prompt = new_build_prompt_with_tools("Warrior", &weights, "PvE");

    println!("  [{}] canonical build smoke...", provider);
    let start = Instant::now();
    // Pass an empty tool set to short-circuit the tool-calling loop: the
    // LLM sees no tools, so it must answer with the final JSON directly.
    let mut no_tools: Vec<ToolDefinition> = Vec::new();
    let response = client
        .generate_with_tools_progress(
            &prompt,
            &no_tools,
            &mut |_name, _args| json!({}),
            1,
            &mut |_, _, _| {},
        )
        .or_else(|_| {
            no_tools.clear();
            client.generate(&prompt)
        })
        .unwrap_or_else(|e| panic!("{}: generate failed: {}", provider, e));
    println!(
        "    ({:.1}s, {} chars)",
        start.elapsed().as_secs_f64(),
        response.len()
    );

    let parsed = parse_gemini_build(&response)
        .unwrap_or_else(|e| panic!("{}: parse_gemini_build failed: {}", provider, e));
    let validated = validate_gemini_build(&parsed, db, "Warrior");

    assert!(
        !validated.specializations.is_empty(),
        "{}: validated build has no specializations. errors: {:?}",
        provider,
        validated.errors
    );
    assert!(
        validated.errors.is_empty(),
        "{}: validation hard errors: {:?}",
        provider,
        validated.errors
    );
    println!(
        "  [{}] OK — {} specs, stat_prefix={:?}, warnings={}",
        provider,
        validated.specializations.len(),
        validated.gear_prefix.as_ref().map(|g| g.name.as_str()),
        validated.warnings.len()
    );
}

#[test]
#[ignore]
fn test_all_providers_canonical_build_smoke() {
    let Some(db) = load_game_db_for_smoke() else {
        eprintln!(
            "[skip] set GW2_OPTIMIZER_CACHE_DIR to a populated cache dir \
             (addon's cache/ folder works)"
        );
        return;
    };

    let mut ran = 0;

    match std::env::var("GEMINI_API_KEY") {
        Ok(key) => {
            let client = gw2_optimizer::llm::gemini::GeminiLlmClient::new(&key, "gemini-2.5-flash")
                .expect("create Gemini client");
            run_canonical_build_smoke(&client, &db);
            ran += 1;
        }
        Err(_) => println!("[skip] GEMINI_API_KEY not set"),
    }

    match std::env::var("OPENAI_API_KEY") {
        Ok(key) => {
            let client = gw2_optimizer::llm::openai::OpenAiClient::new(&key, "gpt-4o-mini")
                .expect("create OpenAI client");
            run_canonical_build_smoke(&client, &db);
            ran += 1;
        }
        Err(_) => println!("[skip] OPENAI_API_KEY not set"),
    }

    match std::env::var("ANTHROPIC_API_KEY") {
        Ok(key) => {
            let client = gw2_optimizer::llm::anthropic::AnthropicClient::new(
                &key,
                "claude-haiku-4-5-20251001",
            )
            .expect("create Anthropic client");
            run_canonical_build_smoke(&client, &db);
            ran += 1;
        }
        Err(_) => println!("[skip] ANTHROPIC_API_KEY not set"),
    }

    assert!(
        ran > 0,
        "no provider API keys set — at least one required for canonical-build smoke"
    );
}
