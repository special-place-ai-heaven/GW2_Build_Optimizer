//! Reproduce: WvW Roam / Support optimize with Scourge locked returns a
//! NON-VIABLE build — SustainRecovery fails with `survived=true, health=87%,
//! repeatable=false` (seen in-game 2026-09-05 on 1.11.25). Run:
//!   cargo run --release -p gw2-optimizer --example scourge_support_check
//! Reads the cache directory from dev.cfg (copy dev.cfg.example).
use gw2_api::cache::DataCache;
use gw2_core::types::{BuildLocks, GameMode};
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::scenario::{
    CombatTier, OptimizationTarget, RoleObjective, ScenarioSpec, TargetProfile,
};
use gw2_optimizer::scoring::OptimizationWeights;
use gw2_optimizer::validation::ValidatedBuild;
use gw2_optimizer::{referee, search_v2, synergy_pipeline};

fn describe(v: &ValidatedBuild) -> String {
    let specs: Vec<String> = v
        .specializations
        .iter()
        .map(|s| format!("{}{}", s.name, if s.elite { "[E]" } else { "" }))
        .collect();
    let name = |o: &Option<(u32, String)>| {
        o.as_ref()
            .map(|(_, n)| n.clone())
            .unwrap_or_else(|| "<EMPTY>".into())
    };
    let utils: Vec<String> = v.skills.utilities.iter().map(name).collect();
    let hand = |h: &Option<String>| h.clone().unwrap_or_else(|| "-".into());
    format!(
        "specs={} | heal={} | utils=[{}] | elite={} | weapons={}/{} + {}/{}",
        specs.join("/"),
        name(&v.skills.heal),
        utils.join(", "),
        name(&v.skills.elite),
        hand(&v.weapons.set1.main_hand),
        hand(&v.weapons.set1.off_hand),
        hand(&v.weapons.set2.main_hand),
        hand(&v.weapons.set2.off_hand),
    )
}

fn print_report(tag: &str, v: &ValidatedBuild, db: &GameDb, prof: &str, w: &OptimizationWeights, ctx: &BalanceContext, sc: &ScenarioSpec) {
    let rep = referee::evaluate_validated_build(v, db, prof, w, ctx, sc);
    println!("{tag} {}\n       rank={:?} viable={}", describe(v), referee::search_rank(&rep), rep.viability.is_viable);
    for g in &rep.viability.gates {
        println!("       gate {:?} passed={} {}", g.gate, g.passed, g.note);
    }
    // Every rotation skill vs the fight window: which ones cannot be back off
    // cooldown within window + 5s (the `repeatable` rule in wvw_timeline)?
    let window = gw2_optimizer::rotation::combat_model::simulation_window_ms_for_mode(
        &sc.game_mode,
        sc.combat_tier,
        sc.combat_kind,
    );
    let (stats, _) = gw2_optimizer::engine::calculate_validated_stats(v, db, prof, ctx);
    let Some(prep) = gw2_optimizer::engine::prepare_validated_rotation(v, db, &stats, Some(sc)) else {
        println!("       (no rotation)");
        return;
    };
    println!("       window={}ms, recovery deadline={}ms", window, window + 5_000);
    for s in &prep.skills {
        let flag = if s.cooldown_ms > window + 5_000 {
            "  <-- can never be ready by the deadline if cast at t>0"
        } else if s.cooldown_ms + 2_000 > window + 5_000 {
            "  <-- ready only if cast in the first seconds"
        } else {
            ""
        };
        println!(
            "       skill {:<28} slot={:<11?} cast={:>5}ms cd={:>6}ms{}",
            s.name, s.slot, s.cast_time_ms, s.cooldown_ms, flag
        );
    }
}

fn main() {
    let cache = DataCache::new(gw2_api::dev_config::cache_dir_or_exit());
    let db = GameDb::load(&cache).expect("load real GameDb");
    let prof = "Necromancer";
    // Radar in the in-game screenshot: Power 0, Condition 7, Control 16,
    // Sustain 61, Boon 52, Heal 64 (sums to the 2.0 budget).
    let weights = OptimizationWeights {
        power: 0.0,
        condition: 0.07,
        boon_support: 0.52,
        healing: 0.64,
        sustain: 0.61,
        control: 0.16,
    };
    let mode = GameMode::WvW;
    let tier = CombatTier::Solo;
    let ctx = BalanceContext::new(mode.clone());
    let role = RoleObjective::Buffer; // play label "Support"
    let scenario = ScenarioSpec {
        game_mode: mode.clone(),
        combat_tier: tier,
        combat_kind: role.combat_kind_for_weights(&weights),
        target_profile: TargetProfile::Single,
        optimization_target: OptimizationTarget {
            label: mode.label().to_string(),
        },
        patch_id: Some(ctx.patch_id.clone()),
        objective_profile_id: Some(role.profile_id_for(&mode, tier).to_string()),
    };
    println!("scenario kind={:?} tier={:?} profile={:?}", scenario.combat_kind, tier, scenario.objective_profile_id);
    let scourge = db
        .specializations
        .values()
        .find(|s| s.name == "Scourge")
        .expect("Scourge in GameDb")
        .id;
    let mut locks = BuildLocks::default();
    locks.specs[2] = Some(scourge);
    let prefix = gw2_optimizer::scoring::select_gear_prefix(&weights).primary;
    println!("prefix={prefix}");

    let seed = synergy_pipeline::optimize_synergy(
        &db,
        prof,
        &weights,
        &ctx,
        prefix,
        &locks,
        Some(&scenario),
        &mut |_| {},
    )
    .expect("seed");
    print_report("SEED  ", &seed.validated, &db, prof, &weights, &ctx, &scenario);

    let result = search_v2::optimize_v2_search(
        &db,
        prof,
        &weights,
        &ctx,
        &scenario,
        &locks,
        &search_v2::SearchConfig::default(),
        &mut |p| {
            if p.stage.starts_with("search_v2") {
                println!("  {}", p.stage);
            }
        },
        &|| false,
    )
    .expect("search");
    print_report("RESULT", &result, &db, prof, &weights, &ctx, &scenario);
    for w in &result.warnings {
        println!("       warning: {w}");
    }
}
