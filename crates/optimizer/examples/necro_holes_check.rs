//! Reproduce: PvE Necromancer optimize hands back a build with empty utility
//! and elite slots (seen in-game 2026-09-04 on 1.11.22). Run:
//!   cargo run --release -p gw2-optimizer --example necro_holes_check
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
    format!(
        "specs={} | heal={} | utils=[{}] (len {}) | elite={}",
        specs.join("/"),
        name(&v.skills.heal),
        utils.join(", "),
        v.skills.utilities.len(),
        name(&v.skills.elite)
    )
}

fn main() {
    let cache = DataCache::new("C:/GAMES/Guild Wars 2/addons/gw2_build_optimizer/cache");
    let db = GameDb::load(&cache).expect("load real GameDb");
    let prof = "Necromancer";
    let weights = OptimizationWeights {
        power: 0.7,
        condition: 0.2,
        boon_support: 0.0,
        healing: 0.1,
        sustain: 0.4,
        control: 0.6,
    };
    let ctx = BalanceContext::new(GameMode::PvE);
    let role = RoleObjective::WvWRoamer;
    let scenario = ScenarioSpec {
        game_mode: GameMode::PvE,
        combat_tier: CombatTier::Party,
        combat_kind: role.combat_kind_for_weights(&weights),
        target_profile: TargetProfile::Single,
        optimization_target: OptimizationTarget {
            label: "PvE".into(),
        },
        patch_id: Some(ctx.patch_id.clone()),
        objective_profile_id: Some(
            role.profile_id_for(&GameMode::PvE, CombatTier::Party)
                .to_string(),
        ),
    };
    let locks = BuildLocks::default();
    let prefix = gw2_optimizer::scoring::select_gear_prefix(&weights).primary;

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
    let seed_rep = referee::evaluate_validated_build(&seed.validated, &db, prof, &weights, &ctx, &scenario);
    println!("SEED   {}\n       rank={:?}", describe(&seed.validated), referee::search_rank(&seed_rep));

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
    let rep = referee::evaluate_validated_build(&result, &db, prof, &weights, &ctx, &scenario);
    println!("RESULT {}\n       rank={:?}", describe(&result), referee::search_rank(&rep));

    // Does filling a hole move the rank at all?
    let equipped: Vec<u32> = result.specializations.iter().map(|s| s.spec_id).collect();
    let mut probes = 0;
    let mut moved = 0;
    for (slot, u) in result.skills.utilities.iter().enumerate() {
        if u.is_some() {
            continue;
        }
        for id in db.skills_by_profession.get(prof).into_iter().flatten() {
            let Some(s) = db.skills.get(id) else { continue };
            if s.slot.as_deref() != Some("Utility")
                || db.skill_palette_id(s.id) == 0
                || s.specialization.is_some_and(|r| !equipped.contains(&r))
            {
                continue;
            }
            let mut b = result.clone();
            b.skills.utilities[slot] = Some((s.id, s.name.clone()));
            let r = referee::evaluate_validated_build(&b, &db, prof, &weights, &ctx, &scenario);
            probes += 1;
            if referee::search_rank(&r) != referee::search_rank(&rep) {
                moved += 1;
            }
        }
        break;
    }
    println!("FILL PROBE: {probes} utilities tried in first empty slot, {moved} changed the rank");
    let holes = result.skills.utilities.iter().filter(|u| u.is_none()).count()
        + usize::from(result.skills.heal.is_none())
        + usize::from(result.skills.elite.is_none());
    assert_eq!(holes, 0, "optimized bar has {holes} empty slot(s)");

    // Simulator cost on the winning kit: gate window vs 60s flow window.
    let prep = gw2_optimizer::engine::prepare_validated_rotation(&result, &db, &rep.stats, Some(&scenario))
        .expect("rotation");
    let t0 = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(gw2_optimizer::engine::simulate_prepared(&prep, &result, &db, Some(&scenario)));
    }
    let gate_ms = t0.elapsed().as_secs_f64() * 1000.0 / 200.0;
    let t0 = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(gw2_optimizer::engine::simulate_flow(&prep, &weights, Some(&scenario)));
    }
    let flow_ms = t0.elapsed().as_secs_f64() * 1000.0 / 200.0;
    let t0 = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(gw2_optimizer::engine::prepare_validated_rotation(&result, &db, &rep.stats, Some(&scenario)));
    }
    let prep_ms = t0.elapsed().as_secs_f64() * 1000.0 / 200.0;
    let t0 = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(referee::evaluate_validated_build(&result, &db, prof, &weights, &ctx, &scenario));
    }
    let eval_ms = t0.elapsed().as_secs_f64() * 1000.0 / 200.0;
    println!("COST prepare={prep_ms:.3}ms gate_sim={gate_ms:.3}ms flow_sim={flow_ms:.3}ms full_eval={eval_ms:.3}ms skills={}", prep.skills.len());

    // The objective itself must see the bar: emptying it has to cost rank.
    let mut emptied = result.clone();
    emptied.skills.utilities = vec![None, None, None];
    emptied.skills.elite = None;
    let rep_empty = referee::evaluate_validated_build(&emptied, &db, prof, &weights, &ctx, &scenario);
    println!(
        "EMPTIED rank={:?}\n        realized full={:?}\n        realized empty={:?}",
        referee::search_rank(&rep_empty),
        rep.realized,
        rep_empty.realized
    );
    assert!(
        referee::search_rank(&rep) > referee::search_rank(&rep_empty),
        "a full bar must outrank an empty one"
    );
}
