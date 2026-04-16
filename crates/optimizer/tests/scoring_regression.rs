//! Regression harness pinning the score outputs of canonical builds.
//!
//! Purpose: any future PR that shifts the empirical scoring constants
//! (`STRIKE_DPS_NORM`, `CONDI_DPS_NORM`, `WEIGHT_BUDGET`, the `*_TIERS` thresholds,
//! or `GEAR_PROFILES`) will trip these tests if a canonical build's score moves
//! outside its tolerance band.
//!
//! Why this matters: Epic 3 story P3-07 plans to migrate these constants from
//! hardcoded Rust values to JSON data files. Without this harness, a JSON read
//! that returns slightly different numbers would silently re-score every build
//! by a different amount per axis, with no signal until users complained.
//!
//! Pinning strategy:
//!   * Per the advisor: pin by MEASUREMENT, not by hand-derived expected values.
//!     Run the test once, read the printed score, encode it as the pinned value
//!     with a +/-5% band. If the math disagrees with the scorer, you'll end up
//!     "fixing" the test rather than measuring the true baseline.
//!   * Tolerance is +/-5% per axis -- a starting heuristic. Adjust per build if
//!     a particular axis is noisier (e.g., near a normalization clip boundary).
//!   * Each canonical build is a `CombatPerformance` fixture (hand-crafted,
//!     archetype-representative). We do NOT round-trip through stat sheets +
//!     `calculate_combat_performance` here -- that would couple the regression
//!     to combat-formula drift. Combat math has its own tests.
//!
//! What this catches:
//!   * `STRIKE_DPS_NORM` change         -> all power scores shift            -> trips power builds
//!   * `CONDI_DPS_NORM` change          -> all condition scores shift        -> trips Harbinger
//!   * `EFFECTIVE_HEALTH_NORM` change   -> all sustain scores shift          -> trips Druid + Daredevil
//!   * `HEALING_NORM` change            -> healing scores shift              -> trips Druid
//!   * `WEIGHT_BUDGET` change           -> high-sum custom weights re-clamp  -> trips Maxed Weights
//!   * Tier-table reshuffle (`POWER_TIERS` etc.) -> prefix count or set drift -> trips tier checks
//!   * `GEAR_PROFILES` reshuffle        -> cosine-sim winner changes         -> trips prefix checks
//!
//! When this test fails: the printed `assert_in_band!` panic message names the
//! build and the delta. Investigate WHY the constant moved before re-pinning.

use gw2_optimizer::combat::{CombatPerformance, ConditionTicks};
use gw2_optimizer::scoring::{
    score_with_weights, select_gear_prefix, select_prefixes_by_tiers, OptimizationWeights,
};

/// +/-5% band tolerance.
const TOLERANCE: f64 = 0.05;

/// Assert `actual` falls within +/-`TOLERANCE` of `expected`.
/// On failure, prints which build and axis tripped and the delta in percent.
#[track_caller]
fn assert_in_band(label: &str, actual: f64, expected: f64) {
    let lo = expected * (1.0 - TOLERANCE);
    let hi = expected * (1.0 + TOLERANCE);
    let delta_pct = if expected.abs() > f64::EPSILON {
        ((actual - expected) / expected) * 100.0
    } else {
        f64::INFINITY
    };
    assert!(
        actual >= lo && actual <= hi,
        "REGRESSION: {label} -- score={actual:.6} drifted {delta_pct:+.2}% from pinned {expected:.6} (band [{lo:.6}, {hi:.6}])",
    );
}

// ============================================================================
// Canonical build fixtures
// ============================================================================
//
// Each fixture is a `CombatPerformance` snapshot representative of the
// archetype. Numbers are realistic-ish but NOT gear-exact -- the labels are
// what the build is FOR, not a precise build export.

/// Build 1 -- Power DPS Dragonhunter (Guardian, PvE).
/// Heavy strike damage, modest condi (burns from traps), low boon/heal,
/// medium-armor sustain.
fn dragonhunter_power_pve() -> CombatPerformance {
    CombatPerformance {
        effective_power: 5200.0,
        strike_dps_index: 3100.0,
        condition_ticks: ConditionTicks::default(),
        condition_dps_index: 350.0, // some burn from traps
        total_dps_index: 3450.0,
        healing_power_index: 60.0,
        boon_duration_pct: 25.0, // group fury/aegis uptime
        condi_duration_pct: 5.0,
        crit_chance: 90.0,
        effective_health: 17000.0,
        damage_reduction_pct: 0.0,
    }
}

/// Build 2 -- Condi DPS Harbinger (Necromancer, PvE).
/// Heavy condition damage, lots of expertise (high condi duration),
/// medium HP, low strike.
fn harbinger_condi_pve() -> CombatPerformance {
    CombatPerformance {
        effective_power: 1400.0,
        strike_dps_index: 600.0,
        condition_ticks: ConditionTicks::default(),
        condition_dps_index: 3400.0,
        total_dps_index: 4000.0,
        healing_power_index: 80.0,
        boon_duration_pct: 20.0,
        condi_duration_pct: 90.0, // viper + traited expertise
        crit_chance: 50.0,
        effective_health: 19500.0,
        damage_reduction_pct: 0.0,
    }
}

/// Build 3 -- Heal Druid (Ranger, PvE).
/// High healing power, near-max boon duration (concentration + Grace of the Land),
/// minimal damage.
fn druid_heal_pve() -> CombatPerformance {
    CombatPerformance {
        effective_power: 700.0,
        strike_dps_index: 250.0,
        condition_ticks: ConditionTicks::default(),
        condition_dps_index: 90.0,
        total_dps_index: 340.0,
        healing_power_index: 1500.0,
        boon_duration_pct: 95.0, // near-cap with concentration runes/sigils
        condi_duration_pct: 15.0,
        crit_chance: 20.0,
        effective_health: 32000.0,
        damage_reduction_pct: 33.0, // protection uptime
    }
}

/// Build 4 -- WvW Roam Daredevil (Thief, WvW).
/// Mid-range strike DPS, modest sustain (dodges + vigor), low boon support,
/// some control via dazes.
fn daredevil_wvw_roam() -> CombatPerformance {
    CombatPerformance {
        effective_power: 3800.0,
        strike_dps_index: 2200.0,
        condition_ticks: ConditionTicks::default(),
        condition_dps_index: 250.0,
        total_dps_index: 2450.0,
        healing_power_index: 100.0,
        boon_duration_pct: 30.0,
        condi_duration_pct: 25.0, // immobs + bleeds via Steal traits
        crit_chance: 75.0,
        effective_health: 22000.0,
        damage_reduction_pct: 10.0,
    }
}

// Tank Chrono is intentionally omitted -- see NOTES in the report. The legacy
// chrono-tank archetype no longer maps cleanly to a single objective profile
// after the boonball shift, and constructing a representative CombatPerformance
// fixture for it would be guesswork. Three power+condi+heal+roam covers all
// six scoring axes between them.

// ============================================================================
// Pinned scores -- captured by measurement on the canonical baseline.
// If you change the constants in scoring.rs and these scores shift outside
// the +/-5% band, that is intentional iff you have user-facing justification.
// ============================================================================

#[test]
fn regression_dragonhunter_power_pve() {
    let perf = dragonhunter_power_pve();
    let weights = OptimizationWeights::preset_power_dps();
    let score = score_with_weights(&perf, &weights);
    println!("MEASURED dragonhunter_power_pve power_dps_preset = {score:.6}");
    // Pinned: 0.940000 -- power axis hits 1.0 cap, slight sustain dilution.
    assert_in_band("dragonhunter_power_pve / power_dps_preset", score, 0.940000);
}

#[test]
fn regression_harbinger_condi_pve() {
    let perf = harbinger_condi_pve();
    let weights = OptimizationWeights::preset_condi_dps();
    let score = score_with_weights(&perf, &weights);
    println!("MEASURED harbinger_condi_pve condi_dps_preset = {score:.6}");
    // Pinned: 0.802000 -- condi axis dominates, 90% expertise scales DPS index.
    assert_in_band("harbinger_condi_pve / condi_dps_preset", score, 0.802000);
}

#[test]
fn regression_druid_heal_pve() {
    let perf = druid_heal_pve();
    let weights = OptimizationWeights::preset_healer();
    let score = score_with_weights(&perf, &weights);
    println!("MEASURED druid_heal_pve healer_preset = {score:.6}");
    // Pinned: 0.986250 -- healing axis at 1.0 cap (1500 healing index), 95% boon
    // duration drives boon_support score, no penalty (low-weight axes are well-served).
    assert_in_band("druid_heal_pve / healer_preset", score, 0.986250);
}

#[test]
fn regression_daredevil_wvw_roam() {
    let perf = daredevil_wvw_roam();
    // WvW roam is power-dominant with sustain emphasis; use power_dps preset
    // (sustain=0.1 already baked in). A custom WvW preset doesn't yet exist.
    let weights = OptimizationWeights::preset_power_dps();
    let score = score_with_weights(&perf, &weights);
    println!("MEASURED daredevil_wvw_roam power_dps_preset = {score:.6}");
    // Pinned: 0.715758 -- power score around 0.733, sustain partial, mode-blended.
    assert_in_band("daredevil_wvw_roam / power_dps_preset", score, 0.715758);
}

/// Multi-axis assertion -- pins each of the 6 axes separately by scoring the
/// same fixture against single-axis weight vectors. This is the silent-drift
/// guard: if `STRIKE_DPS_NORM` shifts but `CONDI_DPS_NORM` doesn't, the power
/// row trips while the condition row stays green, immediately localizing the
/// regression to the strike normalizer.
#[test]
fn regression_per_axis_resolution_dragonhunter() {
    let perf = dragonhunter_power_pve();

    let single = |power, condition, boon_support, healing, sustain, control| {
        OptimizationWeights {
            power,
            condition,
            boon_support,
            healing,
            sustain,
            control,
        }
    };

    let s_power = score_with_weights(&perf, &single(1.0, 0.0, 0.0, 0.0, 0.0, 0.0));
    let s_condi = score_with_weights(&perf, &single(0.0, 1.0, 0.0, 0.0, 0.0, 0.0));
    let s_boon = score_with_weights(&perf, &single(0.0, 0.0, 1.0, 0.0, 0.0, 0.0));
    let s_heal = score_with_weights(&perf, &single(0.0, 0.0, 0.0, 1.0, 0.0, 0.0));
    let s_sust = score_with_weights(&perf, &single(0.0, 0.0, 0.0, 0.0, 1.0, 0.0));
    let s_ctrl = score_with_weights(&perf, &single(0.0, 0.0, 0.0, 0.0, 0.0, 1.0));

    println!(
        "MEASURED per-axis dragonhunter: power={s_power:.6} condi={s_condi:.6} \
         boon={s_boon:.6} heal={s_heal:.6} sust={s_sust:.6} ctrl={s_ctrl:.6}"
    );

    // High weight on a poorly-served axis triggers the misalignment penalty
    // (multiplier = 1 - w*0.7 when axis_score < 0.15). Pinned values bake that in.
    //  power: raw=1.0, no penalty                        -> 1.000000
    //  condi: raw=0.107..., < 0.15 + w=1.0 -> *0.3       -> 0.032250
    //  boon:  raw=0.25, no penalty                       -> 0.250000
    //  heal:  raw=0.04, < 0.15 + w=1.0 -> *0.3           -> 0.012000
    //  sust:  raw=0.34, no penalty                       -> 0.340000
    //  ctrl:  raw=0.13, < 0.15 + w=1.0 -> *0.3           -> 0.039000
    assert_in_band("dh axis power", s_power, 1.000000);
    assert_in_band("dh axis condi", s_condi, 0.032250);
    assert_in_band("dh axis boon", s_boon, 0.250000);
    assert_in_band("dh axis heal", s_heal, 0.012000);
    assert_in_band("dh axis sust", s_sust, 0.340000);
    assert_in_band("dh axis ctrl", s_ctrl, 0.039000);
}

/// `WEIGHT_BUDGET = 2.0` is enforced by `set_constrained()`, NOT by
/// `score_with_weights` (which only clamps per-axis to [0,1]).
///
/// To pin `WEIGHT_BUDGET`, we test the budget enforcement path directly:
/// set up a weight vector at the budget edge, then drag one axis up. Other
/// axes must scale down proportionally so the total still equals the budget.
/// If `WEIGHT_BUDGET` shifts from 2.0 to e.g. 3.0, the scale-down stops
/// happening and the resulting weight vector totals differently.
///
/// We also score a high-sum weight vector through `score_with_weights` to
/// pin the misalignment-penalty path -- a separate sanity check.
#[test]
fn regression_weight_budget_enforcement() {
    // Start at 5 axes * 0.4 = 2.0 (exactly at default budget).
    let mut w = OptimizationWeights {
        power: 0.4,
        condition: 0.4,
        boon_support: 0.4,
        healing: 0.4,
        sustain: 0.4,
        control: 0.0,
    };
    assert!(
        (w.total() - 2.0).abs() < 1e-9,
        "precondition: starting total must equal 2.0"
    );

    // Drag `control` up to 0.6. Total would become 2.6, exceeding budget.
    // Other axes must scale down so the new total stays at the budget.
    w.set_constrained(5, 0.6);
    let new_total = w.total();
    println!("MEASURED set_constrained -> total = {new_total:.6}");
    // Pinned: should equal `WEIGHT_BUDGET` (2.0). If WEIGHT_BUDGET changes,
    // this total moves with it -- catching the drift.
    assert_in_band("WEIGHT_BUDGET enforcement total", new_total, 2.0);
    assert!(
        (w.control - 0.6).abs() < 1e-9,
        "the dragged axis should be set exactly to the requested value"
    );

    // Bonus: pin the actual scaled-down value of one of the other axes so
    // a change in scaling math (not just the budget) also trips.
    // Original other_total = 2.0, excess = 0.6, scale = (2.0-0.6)/2.0 = 0.7.
    // Each non-control axis was 0.4, so post-scale = 0.28.
    println!("MEASURED post-scale power = {:.6}", w.power);
    assert_in_band("WEIGHT_BUDGET scaled axis", w.power, 0.28);
}

/// Pin a score under custom weights that exceed any preset's sum, exercising
/// the misalignment penalty path (axes weighted >= 0.4 with score < 0.15).
#[test]
fn regression_misalignment_penalty() {
    let perf = dragonhunter_power_pve();
    let raw = OptimizationWeights {
        power: 1.0,
        condition: 0.5, // DH condi score is ~0.147 -- < 0.15 -> penalty fires
        boon_support: 0.0,
        healing: 0.0,
        sustain: 1.0,
        control: 0.5, // DH ctrl score is 0.13 -- < 0.15 -> penalty fires
    };
    let score = score_with_weights(&perf, &raw);
    println!("MEASURED misalignment_penalty = {score:.6}");
    // Pinned: 0.205441 -- raw ~0.521, then * 0.65 (condi penalty) * 0.65 (ctrl penalty).
    assert_in_band("misalignment penalty", score, 0.205441);
}

/// Pin the gear-prefix selection (`select_gear_prefix`) for each archetype.
/// This catches `GEAR_PROFILES` drift -- the cosine-similarity weights inside
/// the const table that map weight vectors to gear set names.
#[test]
fn regression_gear_prefix_selection() {
    let dh = select_gear_prefix(&OptimizationWeights::preset_power_dps());
    let harb = select_gear_prefix(&OptimizationWeights::preset_condi_dps());
    let druid = select_gear_prefix(&OptimizationWeights::preset_healer());
    let tank = select_gear_prefix(&OptimizationWeights::preset_tank());

    println!(
        "MEASURED gear prefixes: power_dps -> {} | condi_dps -> {} | healer -> {} | tank -> {}",
        dh.primary, harb.primary, druid.primary, tank.primary
    );

    // These names ARE the canonical gear set GW2 players know each archetype by.
    // If any of these drift, GEAR_PROFILES has been edited -- intentional or not.
    assert_eq!(
        dh.primary, "Berserker's",
        "power_dps preset must select Berserker's gear"
    );
    assert_eq!(
        harb.primary, "Viper's",
        "condi_dps preset must select Viper's gear"
    );
    // Healer preset (boon_support=0.6, healing=1.0, sustain=0.2) cosine-matches
    // Harrier's most strongly (Pow/Heal/Concentration) under the current
    // GEAR_PROFILES -- the canonical meta-healer prefix in PvE, since healer
    // identity is defined by quickness/alacrity uptime, not raw sustain.
    assert_eq!(
        druid.primary, "Harrier's",
        "healer preset must cosine-match Harrier's gear"
    );
    // Tank preset (power=0.1, healing=0.3, sustain=1.0, control=0.2) cosine-matches
    // Nomad's most strongly (Tou/Vit/Heal pure-defensive).
    assert_eq!(
        tank.primary, "Nomad's",
        "tank preset must cosine-match Nomad's gear"
    );
}

/// Pin the COUNT of prefixes selected by `select_prefixes_by_tiers` for each
/// archetype. This catches `POWER_TIERS` / `CONDITION_TIERS` / etc. table
/// drift -- the tier thresholds named in the regression task.
///
/// We pin the count (not exact list) because reasonable maintenance might
/// reshuffle adjacent tiers without invalidating the search space coverage.
/// A drift in COUNT means a tier has been added, removed, or the weight->tier
/// boundary shifted.
#[test]
fn regression_tier_prefix_counts() {
    let n_power = select_prefixes_by_tiers(&OptimizationWeights::preset_power_dps()).len();
    let n_condi = select_prefixes_by_tiers(&OptimizationWeights::preset_condi_dps()).len();
    let n_heal = select_prefixes_by_tiers(&OptimizationWeights::preset_healer()).len();
    let n_tank = select_prefixes_by_tiers(&OptimizationWeights::preset_tank()).len();

    println!(
        "MEASURED tier counts: power={n_power} condi={n_condi} heal={n_heal} tank={n_tank}"
    );

    // Wider band on counts (+/-1) than on scores -- we want to catch table
    // restructure, not penalize a single prefix being added/removed within
    // an unchanged tier ladder.
    let in_band = |label: &str, n: usize, expected: usize| {
        let lo = expected.saturating_sub(1);
        let hi = expected + 1;
        assert!(
            n >= lo && n <= hi,
            "REGRESSION: tier_prefix count {label}={n} drifted from pinned {expected} (band [{lo}, {hi}])"
        );
    };
    in_band("power_dps", n_power, 3);
    in_band("condi_dps", n_condi, 11);
    in_band("healer", n_heal, 10);
    in_band("tank", n_tank, 8);
}
