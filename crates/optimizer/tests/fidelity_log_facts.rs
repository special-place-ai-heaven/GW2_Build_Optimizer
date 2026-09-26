//! Log-side facts the WvW Reaper's Onslaught record (`trait:2021:1`) cites.
//! Plain CI, no game data: the numbers are facts of the committed fixture,
//! not simulator output. No interval value matches them (the gap is shroud
//! dwell and skill Quickness), so they pin the log, not a budget.

use gw2_optimizer::fidelity::compare;
use gw2_optimizer::fidelity::ei_log;
use gw2_optimizer::gamedb::GameDb;

const ABTD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/ei_logs/aBtd-20260604-211449_wvw.json"
);
const ENTER_REAPER_SHROUD: i64 = 30792;
const EXIT_REAPER_SHROUD: i64 = 30961;

#[test]
fn abtd_reaper_self_quickness_and_shroud_dwell() {
    let log = ei_log::load(std::path::Path::new(ABTD)).expect("aBtd fixture");
    let db = GameDb::empty_for_tests();
    let player = |name: &str| {
        log.squad()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("aBtd has no {name}"))
    };
    let casts = |name: &str, id: i64| -> Vec<i64> {
        player(name)
            .rotation
            .iter()
            .filter(|r| r.id == id)
            .flat_map(|r| r.skills.iter().map(|c| c.cast_time))
            .collect()
    };
    // Enter to exit, summed over the fight.
    let shroud_ms = |name: &str| -> i64 {
        let (enter, exit) = (
            casts(name, ENTER_REAPER_SHROUD),
            casts(name, EXIT_REAPER_SHROUD),
        );
        assert_eq!(enter.len(), exit.len(), "{name}");
        enter.iter().zip(&exit).map(|(a, b)| b - a).sum()
    };
    let self_quickness =
        |name: &str| compare::observe(&log, player(name), &db).uptime_self["Quickness"];

    let akatosh = self_quickness("Ak\u{e4}tosh").expect("a source map");
    assert!((akatosh - 0.1425).abs() < 1e-9, "{akatosh}");
    assert_eq!(shroud_ms("Ak\u{e4}tosh"), 13_914);

    // A condition Reaper who did not run Onslaught (wrong-archetype corpus kit).
    assert_eq!(self_quickness("Piero Rosso"), Some(0.0));
    assert_eq!(shroud_ms("Piero Rosso"), 14_339);
}
