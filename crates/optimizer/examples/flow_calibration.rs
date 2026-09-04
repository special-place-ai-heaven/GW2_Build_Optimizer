//! Calibrate the realized-axis norms in `scoring.rs` against real builds.
//!
//! For every profession and a few radar presets, seed a build with the
//! synergy pipeline, run the 60s flow simulation, and print what it produced.
//! The norms are set so a well-built specialist lands near 1.0 on its axis.
//! Run:
//!   cargo run --release -p gw2-optimizer --example flow_calibration [PvE|WvW|PvP]
use std::time::Instant;

use gw2_api::cache::DataCache;
use gw2_core::types::{BuildLocks, GameMode};
use gw2_optimizer::balance::BalanceContext;
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::scenario::{CombatTier, ScenarioSpec};
use gw2_optimizer::scoring::OptimizationWeights;
use gw2_optimizer::{engine, synergy_pipeline};

const PROFESSIONS: [&str; 9] = [
    "Guardian",
    "Warrior",
    "Engineer",
    "Ranger",
    "Thief",
    "Elementalist",
    "Mesmer",
    "Necromancer",
    "Revenant",
];

fn main() {
    let cache = DataCache::new("C:/GAMES/Guild Wars 2/addons/gw2_build_optimizer/cache");
    let db = GameDb::load(&cache).expect("load real GameDb");
    let mode = match std::env::args().nth(1).as_deref() {
        Some("WvW") => GameMode::WvW,
        Some("PvP") => GameMode::PvP,
        _ => GameMode::PvE,
    };
    let ctx = BalanceContext::new(mode.clone());
    let presets: [(&str, OptimizationWeights); 4] = [
        ("power", OptimizationWeights::preset_power_dps()),
        ("condi", OptimizationWeights::preset_condi_dps()),
        ("healer", OptimizationWeights::preset_healer()),
        ("disrupt", OptimizationWeights::preset_disrupt()),
    ];
    println!(
        "{:<14}{:<8}{:>9}{:>9}{:>8}{:>7}{:>7}{:>7}{:>8}  seed",
        "profession", "preset", "strike", "condi", "hps", "boons", "might", "ctl", "flow_ms"
    );
    for prof in PROFESSIONS {
        for (label, weights) in &presets {
            let mut scenario = ScenarioSpec::from_balance_context(&ctx);
            scenario.combat_tier = CombatTier::Party;
            let prefix = gw2_optimizer::scoring::select_gear_prefix(weights).primary;
            let seed = match synergy_pipeline::optimize_synergy(
                &db,
                prof,
                weights,
                &ctx,
                prefix,
                &BuildLocks::default(),
                Some(&scenario),
                &mut |_| {},
            ) {
                Ok(s) => s,
                Err(e) => {
                    println!("{prof:<14}{label:<8} seed failed: {e}");
                    continue;
                }
            };
            let Some(prep) =
                engine::prepare_validated_rotation(&seed.validated, &db, &seed.stats, Some(&scenario))
            else {
                println!("{prof:<14}{label:<8} no rotation");
                continue;
            };
            let t0 = Instant::now();
            let flow = engine::simulate_flow(&prep, weights, Some(&scenario));
            let flow_ms = t0.elapsed().as_secs_f64() * 1000.0;
            let bar: Vec<String> = seed
                .validated
                .skills
                .utilities
                .iter()
                .flatten()
                .map(|(_, n)| n.clone())
                .collect();
            let name = |o: &Option<(u32, String)>| {
                o.as_ref().map(|(_, n)| n.clone()).unwrap_or_else(|| "-".into())
            };
            let weapons = format!(
                "{}/{} + {}/{}",
                seed.validated.weapons.set1.main_hand.as_deref().unwrap_or("-"),
                seed.validated.weapons.set1.off_hand.as_deref().unwrap_or("-"),
                seed.validated.weapons.set2.main_hand.as_deref().unwrap_or("-"),
                seed.validated.weapons.set2.off_hand.as_deref().unwrap_or("-"),
            );
            let specs: Vec<&str> = seed
                .validated
                .specializations
                .iter()
                .map(|s| s.name.as_str())
                .collect();
            println!(
                "{:<14}{:<8}{:>9.0}{:>9.0}{:>8.0}{:>7.2}{:>7.1}{:>7.3}{:>8.2}  {} | {} | H:{} U:{} E:{}",
                prof,
                label,
                flow.strike_dps,
                flow.condition_dps,
                flow.healing_per_second,
                flow.boon_equivalents,
                flow.might_stacks_avg,
                flow.control_uptime,
                flow_ms,
                specs.join("/"),
                weapons,
                name(&seed.validated.skills.heal),
                bar.join(", "),
                name(&seed.validated.skills.elite),
            );
        }
    }
}
