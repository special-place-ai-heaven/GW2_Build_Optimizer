//! What the model spends its first tool rounds fetching, and what it would
//! cost to hand it that in the prompt instead.
//!
//!   cargo run -p gw2-optimizer --example prompt_prefill_size -- Necromancer

use gw2_optimizer::gemini_tools::{execute_tool, ToolContext};

fn main() {
    let profession = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "Necromancer".into());
    let addon_dir = match gw2_api::dev_config::addons_dir() {
        Ok(dir) => dir.join("gw2_build_optimizer"),
        Err(e) => {
            eprintln!("no addons_dir in dev.cfg ({e})");
            std::process::exit(2);
        }
    };
    let cache = gw2_api::cache::DataCache::new(addon_dir.join("cache"));
    let db = gw2_optimizer::gamedb::GameDb::load(&cache).expect("GameDb");
    let candidates = vec![];
    let ctx = ToolContext {
        db: &db,
        profession_name: &profession,
        candidates: &candidates,
        current_build_summary: None,
        weights: gw2_optimizer::scoring::OptimizationWeights::preset_power_dps(),
        balance_ctx: &gw2_optimizer::balance::BalanceContext::new(gw2_core::types::GameMode::WvW),
    };

    let est = |s: &str| s.bytes().filter(|b| b.is_ascii()).count() / 4;

    let info = execute_tool("get_profession_info", &serde_json::json!({}), &ctx);
    let info_s = serde_json::to_string(&info).unwrap();
    println!(
        "get_profession_info {:>8} B  ~{:>6} tok",
        info_s.len(),
        est(&info_s)
    );

    let specs: Vec<String> = info["specializations"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| s["name"].as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut total = info_s.len();
    for name in &specs {
        let v = execute_tool(
            "get_spec_traits",
            &serde_json::json!({ "spec_name": name }),
            &ctx,
        );
        let s = serde_json::to_string(&v).unwrap();
        total += s.len();
        println!("  {name:<24} {:>8} B  ~{:>6} tok", s.len(), est(&s));
    }
    println!(
        "\n{} specs + profession info: {} B, ~{} tokens",
        specs.len(),
        total,
        total / 4
    );

    // The slot-skill palette, which no tool lists: the model guesses names and
    // verifies them one at a time with get_skill_info.
    let mut names = String::new();
    let mut with_desc = String::new();
    let mut n = 0;
    for s in db.skills.values() {
        if !s.professions.iter().any(|p| p == &profession) {
            continue;
        }
        if !matches!(s.slot.as_deref(), Some("Heal" | "Utility" | "Elite")) {
            continue;
        }
        n += 1;
        names.push_str(&format!(
            "{} ({})\n",
            s.name,
            s.slot.as_deref().unwrap_or("")
        ));
        with_desc.push_str(&format!(
            "{} ({}): {}\n",
            s.name,
            s.slot.as_deref().unwrap_or(""),
            s.description.as_deref().unwrap_or("")
        ));
    }
    println!("\n{n} slot skills (Heal/Utility/Elite)");
    println!(
        "  names only      {:>8} B  ~{:>6} tok",
        names.len(),
        est(&names)
    );
    println!(
        "  with description{:>8} B  ~{:>6} tok",
        with_desc.len(),
        est(&with_desc)
    );
}
