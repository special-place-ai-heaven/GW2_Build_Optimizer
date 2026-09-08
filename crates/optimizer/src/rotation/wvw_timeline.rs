//! Counterplay-aware WvW combat timeline.
//!
//! The legacy rotation simulator answers "what is the average damage of this
//! skill roster against a dummy?"  This module answers the WvW question: "can
//! the build establish control of a real exchange long enough to finish its
//! chain, survive the answer, recover, and do it again?"

use std::collections::{HashMap, HashSet, VecDeque};

use crate::data::normalized_effects::{
    EffectCategory, NormalizedEffect, OperationType, SourceType, TargetSide, TriggerRule,
};
use crate::data::quality::FactualValue;
use crate::scenario::{CombatKind, CombatTier, ScenarioSpec};

use super::combat_model::{corrupt_into, EnemyDummy};
use super::simulator::{
    alacrity_cd_advance_ms, condition_tick_damage, crit_chance_fraction, reference_armor,
    strike_crit_factor_with_bonus, SimParams,
};
use super::skill_timings::{HUMAN_DELAY_MS, MIN_SKILL_GAP_MS};
use super::{CoverKind, MobilityKind, RotationSkill, SkillEffect, SkillSlot};

const TIMELINE_TICK_MS: u32 = 50;
pub const MIN_PROTECTED_WINDOW_MS: u32 = 2_000;

/// How often the enemy can open on you again.
///
/// The event script below is a burst: control, strike, condition, boon strip,
/// control, a bigger strike, an unblockable finisher, over 4.4 s. It used to
/// repeat every 5 s, which left 600 ms between the finisher and the next
/// opener - a metronome, not a fight. Pressure in WvW is never sustained. It
/// oscillates: someone spends their cooldowns on you, and then they do not
/// have them for a while.
///
/// That is not a cosmetic difference, it decides what the sustain gates can
/// measure. Under a metronome the only thing that survives is raw mitigation
/// per second, because a heal on a 25 s cooldown can never catch up with a
/// drip - so the test became an accumulation race that longer windows always
/// won. Support and CondiRamp get a 20 s window against StrikeSpike's 5 s, so
/// they ate four uninterrupted bursts and died: measured on the synced corpus
/// 2026-09-07, `SustainRecovery` passed 100% of StrikeSpike, 93% of Harasser
/// (10 s), 50% of CondiRamp and 14% of Support - monotonic in window length,
/// which means the gate was measuring the clock.
///
/// With a real lull the same window measures the thing a support build is
/// actually for: eat the spike, recover before the next one.
const BURST_PERIOD_MS: u32 = 10_000;

/// The burst's shape, as multiples of the base strike: damage ramps up,
/// reaches a peak, and whatever is not evaded or blocked at the peak has to be
/// healed before the next one.
///
/// These four sum to 2.90, which is exactly what the flat 1.00/1.20/0.70
/// script totalled, so this redistributes the burst rather than sharpening it.
/// The peak is now 4.6x the opening chip instead of 1.2x. That ratio is the
/// point: `receive_strike` drops a blockable hit entirely on evade, block or
/// invulnerability, so a peak barely above the chip made active defence worth
/// almost nothing and left raw mitigation per second as the only thing the
/// sustain gates could see.
/// The peak magnitude is the one free number here, and it is calibrated, not
/// chosen. Swept against the synced WvW corpus 2026-09-07, `SustainRecovery`
/// pass rate per combat kind:
///
/// | peak | Support | StrikeSpike | Harasser | spread |
/// |---|---|---|---|---|
/// | flat 1.20 (old) | 14% | 100% | 93% | 86 pts |
/// | 1.60 | 100% | 100% | 100% | 0, but nothing fails |
/// | **2.60** | **93%** | **91%** | **99%** | **8 pts** |
/// | 4.00 | 57% | 70% | 96% | 39 pts |
/// | 5.50 | 7% | 43% | 82% | 75 pts |
///
/// The old script's spread was the window length, not the build. 2.60 is
/// where that bias disappears while the gate still refuses real builds - five
/// of the corpus - so it is measuring sustain rather than the clock or
/// nothing at all. Too high and the bias returns inverted, because a support
/// carries less active defence than a roamer and starts eating peaks.
const RAMP_OPEN: f64 = 0.35;
const RAMP_BUILD: f64 = 0.55;
const PEAK: f64 = 2.60;
const RESIDUAL: f64 = 0.40;

/// How long an applied condition sits on you.
///
/// This was 4,000 ms against a burst period that is now 10,000 ms, which
/// meant every condition expired during the lull. Nothing had to be cleansed:
/// waiting was a complete answer, so `CleanseRate` was a rule about the skill
/// bar with no consequence anywhere in the simulation, and a build that
/// brought no cleanse at all measured the same as one built around it.
///
/// Condition damage is not strike damage. Armour does not reduce it,
/// Protection does not reduce it, and an evade or a block cannot avoid what
/// is already ticking - the tick is `(base + coefficient x Condition Damage)`
/// per stack per second, for as long as the duration the attacker's Expertise
/// bought. Removal is the only counter. So the duration has to reach the next
/// burst: uncleansed stacks then overlap the new ones and the pressure
/// compounds, which is the thing that makes cleansing worth a utility slot.
const CONDITION_DURATION_MS: u32 = 10_000;

/// Chilled: skills recharge at 34% of the normal rate.
///
/// The enemy's answer to Alacrity, and the reason a condition that deals no
/// damage at all can still be the one that kills you.
// Wiki `Chilled` (read 2026-09-07): "for every 1.66 seconds chilled, only 1
// second of cooldown will have expired" — a 60% recharge rate. The tooltip's
// "cooldown increased by 66%" is the same fact from the other side; it is
// not a 34% rate.
const CHILLED_RECHARGE_PERCENT: u32 = 60;

/// How long the opener's Chill sits on you.
///
/// Shorter than [`CONDITION_DURATION_MS`]: a damaging condition ticks for as
/// long as it lasts, but Chill only has to cover the recovery window to do its
/// job, and a chill that outlasted the lull would mean the heal never comes
/// back at all rather than comes back late.
const CHILL_DURATION_MS: u32 = 4_000;
pub const TARGET_PROTECTED_WINDOW_MS: u32 = 5_000;

/// Wiki Barrier: disappears 5s after applied; WvW cap is 25% of max health.
const BARRIER_LIFETIME_MS: u32 = 5_000;
const WVW_BARRIER_HEALTH_FRACTION: f64 = 0.25;
/// Wiki Interrupt: interrupted skills get a 5 second cooldown.
// Wiki `Activation time` (edited 2026-07-30) and `Channeled skill` say an
// interrupted activation puts the skill on a 4-second recharge; `Skill` and
// `Interrupt` (edited 2026-02-19) say 5. The wiki disagrees with itself; the
// newest page wins until someone measures it.
const INTERRUPT_COOLDOWN_MS: u32 = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ResourceKind {
    #[default]
    Initiative,
    Energy,
    Adrenaline,
    Illusions,
    Blades,
    /// Necromancer life force, in absolute units; the cap is 69 % of max
    /// health (`data/formulas/shroud.json`).
    LifeForce,
}

#[derive(Debug, Clone, Default)]
pub struct SkillResourceRule {
    pub skill_id: u32,
    pub kind: ResourceKind,
    pub cost: f64,
    pub gain_on_hit: f64,
    pub spend_all: bool,
    // Sprint 2 (specs/005-wvw-proc-sites): life force and shroud
    /// Resource credited when the cast resolves (Percent fact "Life Force").
    pub gain_on_use: f64,
    /// Minimum pool to start the cast (shroud entry: 10 % of the cap).
    pub entry_floor: f64,
    /// Pool lost per second while this skill's shroud is active.
    pub drain_per_second: f64,
    /// Incoming damage taken by the pool while in this shroud, as a
    /// fraction after reduction (WvW: 0.5).
    pub shroud_damage_factor: f64,
    /// This skill enters / exits shroud.
    pub enters_shroud: bool,
    pub exits_shroud: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WvwCombatReport {
    pub duration_ms: u32,
    pub target_health: Option<f64>,
    pub target_reached_at_ms: Option<u32>,
    pub longest_protected_window_ms: u32,
    pub protected_action_count: u32,
    pub successful_action_count: u32,
    pub interrupted_casts: u32,
    pub protected_damage: f64,
    pub peak_protected_damage_2s: f64,
    pub peak_protected_damage_5s: f64,
    pub total_damage: f64,
    pub control_landed_ms: u32,
    pub incoming_damage: f64,
    pub avoided_damage: f64,
    pub healing: f64,
    pub barrier_absorbed: f64,
    pub conditions_cleansed: u32,
    pub combo_activations: u32,
    pub remaining_health_ratio: f64,
    /// Positive means recovery/avoidance exceeded incoming pressure.
    pub sustain_margin: f64,
    pub player_survived: bool,
    pub target_reached: bool,
    pub chain_completed: bool,
    /// Pressure and control that occurred inside the qualifying secured
    /// sequence, not totals collected from unrelated moments.
    pub secured_sequence_damage: f64,
    pub secured_sequence_control_ms: u32,
    pub repeatable: bool,
    pub resource_blocked_actions: u32,
    pub resource_legal: bool,
    /// False when the active profession mechanic needs a state model that this
    /// bounded resource ledger does not yet provide.
    pub resource_model_complete: bool,
    /// Every equipped or triggered effect source the timeline did not
    /// simulate, as `"{name} ({why})"` — `no record`, `on-crit`,
    /// `on-skill-use`, `on-health-threshold`, `conditional`, `unresolved
    /// value`, `unsupported proc`, `partial combo`, `dark field`. Deduplicated.
    /// This never silently becomes verified data: the referee turns it into
    /// the `wvw_timeline.effects` coverage reason.
    pub unmodeled_sources: Vec<String>,
    /// Bounded event trace. Empty unless [`WvwTimelineInput::trace`] was set;
    /// capped at [`TRACE_CAP`] events.
    pub trace: Vec<TraceEvent>,
    /// True when an event past the cap was dropped.
    pub trace_truncated: bool,
    /// Seeded on-crit trials, one entry per proc source. Empty unless
    /// [`WvwTimelineInput::trace`] was set.
    pub proc_trials: Vec<ProcTrial>,
    /// Readable reasons a shroud entry was refused, e.g. `Reaper's Shroud
    /// needs 10% life force, had 4%`. The referee appends them to the
    /// quality reasons.
    pub shroud_refusals: Vec<String>,
}

/// Upper bound on [`WvwCombatReport::trace`]. The 513th event is dropped and
/// `trace_truncated` is set.
pub const TRACE_CAP: usize = 512;

/// One observable runtime event, for the causal experiments in
/// `specs/004-simulator-trust`. Never serialized into a prompt.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceEvent {
    pub t_ms: u32,
    pub kind: TraceKind,
    /// The skill, trait, sigil or relic name the event belongs to.
    pub source: String,
    /// Kind-specific detail: landed damage, proc category, hits lost, set.
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceKind {
    HitLanded,
    ProcFired,
    ProcSkippedIcd,
    ProcUnmodeled,
    CastInterrupted,
    WeaponSwap,
    // Sprint 2 (specs/005-wvw-proc-sites)
    ConditionalActivated,
    ConditionalExpired,
    StackGained,
    ComboResolved,
    ShroudEntered,
    ShroudExited,
    ShroudRefused,
    LifeForceGained,
}

/// Seeded-trial summary for one proc source, trace mode only
/// (`specs/005-wvw-proc-sites`, R1): how often the proc fired across the
/// fixed seeds, beside the expected-value count the ranking uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProcTrial {
    pub source: String,
    pub mean: f64,
    pub min: u32,
    pub max: u32,
}

pub struct WvwTimelineInput<'a> {
    pub skills: &'a [RotationSkill],
    /// The published rotation, as skill ids in press order. Followed while
    /// it can be; the timeline improvises once it runs out or cannot comply.
    pub opener: &'a [u32],
    pub duration_ms: u32,
    pub params: &'a SimParams,
    pub enemy: EnemyDummy,
    pub scenario: &'a ScenarioSpec,
    pub active_effects: &'a [&'a NormalizedEffect],
    pub resource_rules: &'a [SkillResourceRule],
    pub resource_model_complete: bool,
    /// Equipped sources with no record for this mode, already named by the
    /// caller (`engine::active_normalized_effects`).
    pub unmodeled_sources: Vec<String>,
    /// Each socketed sigil's weapon set (1 or 2; 0 = on both), so a sigil's
    /// procs fire only while its set is held (CONN-00-07).
    pub sigil_sets: HashMap<u32, u8>,
    /// Exact in-combat weapon swap cooldown for this profession. `None`
    /// means the active specialization cannot swap weapons in combat.
    pub weapon_swap_cooldown_ms: Option<u32>,
    /// Record a bounded [`TraceEvent`] log on the report. Diagnostics only:
    /// no production caller sets this — `engine::simulate_prepared` passes
    /// `false`, the search and the Choya tools go through it, and nothing
    /// appends the trace to a prompt (`trace_is_empty_unless_requested`).
    pub trace: bool,
}

#[derive(Debug, Clone)]
enum EnemyEventKind {
    Strike {
        damage: f64,
        unblockable: bool,
    },
    Control {
        duration_ms: u32,
        unblockable: bool,
    },
    Condition {
        condition: String,
        stacks: u32,
        duration_ms: u32,
    },
    BoonStrip {
        count: u32,
    },
}

#[derive(Debug, Clone)]
struct EnemyEvent {
    at_ms: u32,
    kind: EnemyEventKind,
}

#[derive(Debug, Clone)]
struct WvwProfile {
    duration_ms: u32,
    target_health: Option<f64>,
    enemy_events: VecDeque<EnemyEvent>,
    required_window_ms: u32,
    desired_window_ms: u32,
}

impl WvwProfile {
    fn for_scenario(
        scenario: &ScenarioSpec,
        enemy: &EnemyDummy,
        params: &SimParams,
        duration_ms: u32,
    ) -> Self {
        let duration_ms = duration_ms.max(MIN_PROTECTED_WINDOW_MS);
        let tier_pressure = match scenario.combat_tier {
            CombatTier::Solo => 1.0,
            CombatTier::Party => 1.20,
            CombatTier::Squad => 1.45,
        };
        let kind_pressure = match scenario.combat_kind {
            CombatKind::StrikeSpike | CombatKind::Harasser | CombatKind::Disabler => 1.10,
            CombatKind::CondiRamp => 1.0,
            CombatKind::Support | CombatKind::Commander | CombatKind::Staller => 0.90,
        };
        let pressure = tier_pressure * kind_pressure;
        let enemy_power = 2_500.0 * pressure;
        let strike = 1_100.0 * enemy_power / params.armor.max(1_000.0) * 2.0;
        let mut events = Vec::new();
        let mut cycle = 0;
        while cycle < duration_ms {
            events.push(EnemyEvent {
                at_ms: cycle + 450,
                kind: EnemyEventKind::Control {
                    duration_ms: 900,
                    unblockable: false,
                },
            });
            // The ramp. Chip damage while they build to the thing that
            // actually kills you - small enough that a real build's healing
            // covers it, which is what makes healing worth having.
            events.push(EnemyEvent {
                at_ms: cycle + 850,
                kind: EnemyEventKind::Strike {
                    damage: strike * RAMP_OPEN,
                    unblockable: false,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 1_650,
                kind: EnemyEventKind::Condition {
                    condition: if scenario.combat_kind == CombatKind::CondiRamp {
                        "Burning".into()
                    } else {
                        "Bleeding".into()
                    },
                    stacks: if scenario.combat_kind == CombatKind::CondiRamp {
                        3
                    } else {
                        2
                    },
                    duration_ms: CONDITION_DURATION_MS,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 2_350,
                kind: EnemyEventKind::Strike {
                    damage: strike * RAMP_BUILD,
                    unblockable: false,
                },
            });
            // Strip the cover, then land the CC, then hit. That order is the
            // whole of a WvW opener and it is why the peak is worth spending
            // an evade on.
            events.push(EnemyEvent {
                at_ms: cycle + 2_600,
                kind: EnemyEventKind::BoonStrip { count: 1 },
            });
            // Chill goes on before the peak, so the recovery window after it
            // is spent waiting rather than healing. It deals no damage; it
            // costs you the answer to the damage.
            events.push(EnemyEvent {
                at_ms: cycle + 2_800,
                kind: EnemyEventKind::Condition {
                    condition: "Chilled".into(),
                    stacks: 1,
                    duration_ms: CHILL_DURATION_MS,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 3_050,
                kind: EnemyEventKind::Control {
                    duration_ms: 1_100,
                    unblockable: false,
                },
            });
            // The peak. Blockable on purpose: this is the hit evades, blocks
            // and invulnerability exist for, and a model where the biggest
            // number of the fight cannot be answered cannot tell a build that
            // brought an answer from one that did not.
            events.push(EnemyEvent {
                at_ms: cycle + 3_550,
                kind: EnemyEventKind::Strike {
                    damage: strike * PEAK,
                    unblockable: false,
                },
            });
            // What is left after the peak, unblockable, so surviving is never
            // purely a matter of holding one button at the right moment.
            events.push(EnemyEvent {
                at_ms: cycle + 4_400,
                kind: EnemyEventKind::Strike {
                    damage: strike * RESIDUAL,
                    unblockable: true,
                },
            });
            cycle += BURST_PERIOD_MS;
        }
        events.retain(|event| event.at_ms < duration_ms);
        events.sort_by_key(|event| event.at_ms);

        Self {
            duration_ms,
            target_health: enemy.hp,
            enemy_events: events.into(),
            required_window_ms: MIN_PROTECTED_WINDOW_MS,
            desired_window_ms: TARGET_PROTECTED_WINDOW_MS.min(duration_ms),
        }
    }
}

#[derive(Debug, Clone)]
struct TimedDefense {
    kind: CoverKind,
    expires_at_ms: u32,
    stacks: u32,
    strippable: bool,
    /// First application. Extensions do not touch it — strips go by this.
    applied_at_ms: u32,
}

/// One hit of a cast in flight, landing at `at_ms` with its share of the
/// skill's damage. Cancelled with the cast if the cast is interrupted.
#[derive(Debug, Clone)]
struct ScheduledHit {
    at_ms: u32,
    skill_id: u32,
    dmg_multiplier: f64,
}

#[derive(Debug, Clone)]
struct TimedBuff {
    name: String,
    stacks: u32,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct TimedCondition {
    name: String,
    stacks: u32,
    expires_at_ms: u32,
    next_tick_ms: u32,
}

struct BarrierLayer {
    amount: f64,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct ComboFieldState {
    field_type: String,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct PendingCast {
    skill_idx: usize,
    started_at_ms: u32,
    resolves_at_ms: u32,
    protected_at_start: bool,
    saved_by_charge: bool,
}

#[derive(Debug, Clone)]
struct DamageEvent {
    at_ms: u32,
    amount: f64,
    protected: bool,
}

#[derive(Debug, Clone)]
struct ProtectedActionEvent {
    at_ms: u32,
    skill_id: u32,
    control_ms: u32,
    applies_condition: bool,
    /// Healed, barriered, cleansed, or handed out a boon.
    ///
    /// A protected window is worth having because something happened inside
    /// it. For a damage build that is damage; for a healer it is the healing
    /// and the cleansing, which are not lesser outcomes — they are the job.
    supports_allies: bool,
}

#[derive(Debug, Clone, Default)]
struct SecuredSequenceSummary {
    completed: bool,
    damage: f64,
    control_ms: u32,
    skill_ids: HashSet<u32>,
}

#[derive(Debug, Clone)]
struct ProcSpec {
    source_type: SourceType,
    source_id: u32,
    source_name: String,
    trigger: TriggerRule,
    category: EffectCategory,
    value: f64,
    duration_ms: u32,
    internal_cooldown_ms: u32,
    next_ready_ms: u32,
    operation: Option<crate::data::normalized_effects::StatusOperation>,
    // Sprint 2 (specs/005-wvw-proc-sites)
    /// 0 for traits, runes, relics and skills; 1 or 2 for a sigil's seat.
    /// A sigil fires only while its set is held.
    weapon_set: u8,
    /// Record proc chance, 1.0 when absent.
    proc_chance: f64,
    /// Expected-value probability mass accumulated since the last cooldown
    /// start (R1): the cooldown begins when it reaches 1.0.
    mass: f64,
    scope: crate::data::normalized_effects::TriggerScope,
}

/// A rune or relic strike bonus that holds only while its prerequisite does
/// (specs/005-wvw-proc-sites, US3): a health threshold read against the
/// regular health pool, or a stack count fed by qualifying hits.
struct ConditionalSpec {
    source_name: String,
    kind: ConditionalKind,
    /// Percent per activation or per stack (record `value`).
    percent: f64,
    /// Threshold state as of the last evaluation.
    active: bool,
    stacks: u32,
    expires_at_ms: u32,
}

enum ConditionalKind {
    Threshold {
        above: bool,
        percent: f64,
    },
    Stacking {
        max: u32,
        duration_ms: u32,
        scope: crate::data::normalized_effects::TriggerScope,
    },
}

/// Unequipped weapon strength (wiki `Weapon strength`, read 2026-09-08): the
/// value sigil flame blasts and similar procs use instead of the held weapon.
const UNEQUIPPED_WEAPON_STRENGTH: f64 = 690.5;

/// How an on-crit proc's chance enters the result (R1): expected value in
/// ranking, Bernoulli draws from a fixed seed in the diagnostic trials.
enum CritMode {
    Expected,
    Seeded(XorShift64Star),
}

/// Ten-line PRNG for the trace-only trials; no dependency.
struct XorShift64Star(u64);

impl XorShift64Star {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }

    fn next_f64(&mut self) -> f64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Fixed trial seeds: reproducible, and eight is enough to bracket the
/// expected value on a 30 s opener.
const TRIAL_SEEDS: [u64; 8] = [
    0x9E37_79B9_7F4A_7C15,
    0x9E37_79B9_7F4A_7C16,
    0x9E37_79B9_7F4A_7C17,
    0x9E37_79B9_7F4A_7C18,
    0x9E37_79B9_7F4A_7C19,
    0x9E37_79B9_7F4A_7C1A,
    0x9E37_79B9_7F4A_7C1B,
    0x9E37_79B9_7F4A_7C1C,
];

struct Timeline<'a> {
    skills: &'a [RotationSkill],
    params: &'a SimParams,
    profile: WvwProfile,
    now_ms: u32,
    next_action_ms: u32,
    disabled_until_ms: u32,
    active_weapon_set: u8,
    weapon_swap_ready_ms: u32,
    weapon_swap_cooldown_ms: Option<u32>,
    opener: &'a [u32],
    opener_cursor: usize,
    cooldown_ready_ms: Vec<u32>,
    pending: Option<PendingCast>,
    scheduled_hits: Vec<ScheduledHit>,
    defenses: Vec<TimedDefense>,
    buffs: Vec<TimedBuff>,
    outgoing_conditions: Vec<TimedCondition>,
    incoming_conditions: Vec<TimedCondition>,
    combo_field: Option<ComboFieldState>,
    enemy_protection: bool,
    enemy_stability: bool,
    enemy_disabled_until_ms: u32,
    enemy_health: f64,
    target_reached_at_ms: Option<u32>,
    player_health: f64,
    barrier: VecDeque<BarrierLayer>,
    protected_run_ms: u32,
    longest_protected_window_ms: u32,
    charge_cover_consumed_this_tick: bool,
    secured_tick_times: Vec<u32>,
    protected_actions: Vec<ProtectedActionEvent>,
    damage_events: Vec<DamageEvent>,
    protected_action_count: u32,
    successful_action_count: u32,
    interrupted_casts: u32,
    control_landed_ms: u32,
    incoming_damage: f64,
    avoided_damage: f64,
    healing: f64,
    barrier_absorbed: f64,
    conditions_cleansed: u32,
    combo_activations: u32,
    proc_specs: Vec<ProcSpec>,
    conditional_specs: Vec<ConditionalSpec>,
    unmodeled_proc_keys: HashSet<(u8, u32)>,
    /// Every source the timeline itself could not simulate, `"{name} ({why})"`,
    /// deduplicated. Reported first: these are the mechanics a player would
    /// expect to see executed.
    unmodeled_names: Vec<String>,
    /// Equipped sources with no record for this mode, named by the caller.
    /// Reported after `unmodeled_names`: most traits and every weapon skill
    /// have no proc record, so this list is long and least specific
    /// (CONN-01-06).
    no_record_names: Vec<String>,
    trace_enabled: bool,
    trace: Vec<TraceEvent>,
    trace_truncated: bool,
    shroud_refusals: Vec<String>,
    crit_mode: CritMode,
    /// `ProcFired` count per proc source, for the trials (trace only).
    proc_fire_counts: HashMap<String, u32>,
    protection_multiplier: f64,
    resource_rules: HashMap<u32, SkillResourceRule>,
    resources: HashMap<ResourceKind, f64>,
    resource_blocked_skills: HashSet<u32>,
    resource_model_complete: bool,
}

/// Run the WvW exchange model. Effects resolve at cast completion, so incoming
/// control can genuinely cancel the action instead of merely lowering a score.
pub fn evaluate_wvw_timeline(input: WvwTimelineInput<'_>) -> WvwCombatReport {
    let WvwTimelineInput {
        skills,
        opener,
        duration_ms,
        params,
        enemy,
        scenario,
        active_effects,
        resource_rules,
        resource_model_complete,
        unmodeled_sources,
        sigil_sets,
        weapon_swap_cooldown_ms,
        trace,
    } = input;
    let profile = WvwProfile::for_scenario(scenario, &enemy, params, duration_ms);
    let mut timeline = Timeline::new(
        skills,
        params,
        profile,
        enemy,
        active_effects,
        resource_rules,
        resource_model_complete,
        unmodeled_sources,
    );
    timeline.weapon_swap_cooldown_ms = weapon_swap_cooldown_ms;
    timeline.assign_sigil_sets(&sigil_sets);
    timeline.opener = opener;
    timeline.trace_enabled = trace;
    timeline.trace_loaded_unmodeled();
    timeline.run();
    let mut report = timeline.report();
    if trace {
        // Diagnostics only (R1): the same fight eight more times with real
        // on-crit draws, so the trace can say how far the expected-value
        // process is from a proc that either fires or does not.
        let sources: Vec<String> = timeline
            .proc_specs
            .iter()
            .map(|spec| spec.source_name.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        let mut counts: HashMap<String, Vec<u32>> = HashMap::new();
        for seed in TRIAL_SEEDS {
            let mut trial = Timeline::new(
                skills,
                params,
                timeline.profile.clone(),
                enemy,
                active_effects,
                resource_rules,
                resource_model_complete,
                Vec::new(),
            );
            trial.weapon_swap_cooldown_ms = weapon_swap_cooldown_ms;
            trial.assign_sigil_sets(&sigil_sets);
            trial.opener = opener;
            trial.crit_mode = CritMode::Seeded(XorShift64Star::new(seed));
            trial.run();
            for source in &sources {
                counts
                    .entry(source.clone())
                    .or_default()
                    .push(trial.proc_fire_counts.get(source).copied().unwrap_or(0));
            }
        }
        let mut sources = sources;
        sources.sort();
        report.proc_trials = sources
            .into_iter()
            .map(|source| {
                let runs = &counts[&source];
                ProcTrial {
                    mean: runs.iter().map(|&n| f64::from(n)).sum::<f64>() / runs.len() as f64,
                    min: runs.iter().copied().min().unwrap_or(0),
                    max: runs.iter().copied().max().unwrap_or(0),
                    source,
                }
            })
            .collect();
    }
    report
}

impl<'a> Timeline<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        skills: &'a [RotationSkill],
        params: &'a SimParams,
        profile: WvwProfile,
        enemy: EnemyDummy,
        active_effects: &[&NormalizedEffect],
        resource_rules: &[SkillResourceRule],
        resource_model_complete: bool,
        unmodeled_sources: Vec<String>,
    ) -> Self {
        let mut state = Self {
            skills,
            params,
            // ponytail: no-target dummy uses +inf so events keep firing and target_reached stays false
            enemy_health: profile.target_health.unwrap_or(f64::INFINITY),
            player_health: params.max_health,
            enemy_protection: enemy.protection,
            enemy_stability: enemy.stability,
            profile,
            now_ms: 0,
            next_action_ms: 0,
            disabled_until_ms: 0,
            active_weapon_set: 1,
            weapon_swap_ready_ms: 0,
            opener: &[],
            opener_cursor: 0,
            weapon_swap_cooldown_ms: Some(10_000),
            cooldown_ready_ms: vec![0; skills.len()],
            pending: None,
            scheduled_hits: Vec::new(),
            defenses: Vec::new(),
            buffs: Vec::new(),
            outgoing_conditions: Vec::new(),
            incoming_conditions: Vec::new(),
            combo_field: None,
            enemy_disabled_until_ms: 0,
            target_reached_at_ms: None,
            barrier: VecDeque::new(),
            protected_run_ms: 0,
            longest_protected_window_ms: 0,
            charge_cover_consumed_this_tick: false,
            secured_tick_times: Vec::new(),
            protected_actions: Vec::new(),
            damage_events: Vec::new(),
            protected_action_count: 0,
            successful_action_count: 0,
            interrupted_casts: 0,
            control_landed_ms: 0,
            incoming_damage: 0.0,
            avoided_damage: 0.0,
            healing: 0.0,
            barrier_absorbed: 0.0,
            conditions_cleansed: 0,
            combo_activations: 0,
            proc_specs: Vec::new(),
            conditional_specs: Vec::new(),
            unmodeled_proc_keys: HashSet::new(),
            unmodeled_names: Vec::new(),
            no_record_names: unmodeled_sources,
            trace_enabled: false,
            trace: Vec::new(),
            trace_truncated: false,
            shroud_refusals: Vec::new(),
            crit_mode: CritMode::Expected,
            proc_fire_counts: HashMap::new(),
            protection_multiplier: crate::data::boon_condition_formulas::boons()
                .protection_multiplier(),
            resource_rules: resource_rules
                .iter()
                .map(|rule| (rule.skill_id, rule.clone()))
                .collect(),
            resources: initial_resources(resource_rules),
            resource_blocked_skills: HashSet::new(),
            resource_model_complete,
        };
        state.load_normalized_effects(active_effects);
        state
    }

    fn load_normalized_effects(&mut self, effects: &[&NormalizedEffect]) {
        for effect in effects {
            if matches!(effect.trigger_rule, TriggerRule::Passive) {
                // Standing modifiers are already folded into SimParams by the
                // shared combat parser. Applying them here would count the same
                // trait/rune/sigil a second time.
                continue;
            }

            if matches!(effect.source_type, SourceType::Skill)
                && self.skill_directly_models_effect(effect)
            {
                continue;
            }

            // Conditional strike bonuses (US3): a health threshold or a
            // stacking bonus on strike damage becomes a ConditionalSpec.
            let strike_bonus = matches!(effect.category, EffectCategory::StrikeDamagePct)
                || matches!(effect.inner_category, Some(EffectCategory::StrikeDamagePct));
            if strike_bonus && matches!(effect.trigger_rule, TriggerRule::OnHealthThreshold) {
                let (Some(threshold), Some(&percent)) =
                    (effect.health_threshold.as_ref(), resolved(&effect.value))
                else {
                    self.note_unmodeled(format!("{} (unresolved value)", effect.source_name));
                    continue;
                };
                let Some(&line) = resolved(&threshold.percent) else {
                    self.note_unmodeled(format!("{} (unresolved value)", effect.source_name));
                    continue;
                };
                self.conditional_specs.push(ConditionalSpec {
                    source_name: effect.source_name.clone(),
                    kind: ConditionalKind::Threshold {
                        above: threshold.above,
                        percent: line,
                    },
                    percent,
                    active: false,
                    stacks: 0,
                    expires_at_ms: 0,
                });
                continue;
            }
            if strike_bonus
                && matches!(effect.trigger_rule, TriggerRule::OnHit)
                && effect.max_stacks.is_some()
            {
                let (Some(&max), Some(&duration), Some(&percent)) = (
                    effect.max_stacks.as_ref().and_then(resolved),
                    effect.effect_duration.as_ref().and_then(resolved),
                    resolved(&effect.value),
                ) else {
                    self.note_unmodeled(format!("{} (unresolved value)", effect.source_name));
                    continue;
                };
                self.conditional_specs.push(ConditionalSpec {
                    source_name: effect.source_name.clone(),
                    kind: ConditionalKind::Stacking {
                        max,
                        duration_ms: (duration * 1_000.0).round() as u32,
                        scope: effect.trigger_scope.clone().unwrap_or_default(),
                    },
                    percent,
                    active: false,
                    stacks: 0,
                    expires_at_ms: 0,
                });
                continue;
            }

            let supported = matches!(
                effect.trigger_rule,
                TriggerRule::OnHit | TriggerRule::OnCrit
            ) || (matches!(effect.trigger_rule, TriggerRule::OnSkillUse)
                && matches!(effect.source_type, SourceType::Skill));
            if !supported {
                self.note_unmodeled(format!(
                    "{} ({})",
                    effect.source_name,
                    trigger_label(&effect.trigger_rule)
                ));
                continue;
            }
            let Some(&value) = resolved(&effect.value) else {
                self.note_unmodeled(format!("{} (unresolved value)", effect.source_name));
                continue;
            };

            self.proc_specs.push(ProcSpec {
                source_type: effect.source_type.clone(),
                source_id: effect.source_id,
                source_name: effect.source_name.clone(),
                trigger: effect.trigger_rule.clone(),
                category: effect
                    .inner_category
                    .clone()
                    .unwrap_or_else(|| effect.category.clone()),
                value,
                duration_ms: effect
                    .effect_duration
                    .as_ref()
                    .and_then(resolved)
                    .map(|seconds| (seconds * 1_000.0).round() as u32)
                    .unwrap_or(0),
                internal_cooldown_ms: effect
                    .internal_cooldown
                    .as_ref()
                    .and_then(resolved)
                    .map(|seconds| (seconds * 1_000.0).round() as u32)
                    .or_else(|| {
                        effect
                            .status_operation
                            .as_ref()
                            .and_then(|op| op.internal_cooldown_ms.as_ref())
                            .and_then(resolved)
                            .copied()
                    })
                    .unwrap_or(0),
                next_ready_ms: 0,
                operation: effect.status_operation.clone(),
                weapon_set: 0,
                proc_chance: effect
                    .proc_chance
                    .as_ref()
                    .and_then(resolved)
                    .copied()
                    .unwrap_or(1.0),
                mass: 0.0,
                scope: effect.trigger_scope.clone().unwrap_or_default(),
            });
        }
    }

    fn run(&mut self) {
        while self.now_ms < self.profile.duration_ms && self.player_health > 0.0 {
            self.charge_cover_consumed_this_tick = false;
            self.expire_timed_state();
            self.regenerate_resources();
            self.land_scheduled_hits();
            self.resolve_pending_cast();
            self.tick_conditions();
            self.process_enemy_events();
            self.track_protected_window();

            if self.pending.is_none()
                && self.now_ms >= self.next_action_ms
                && self.now_ms >= self.disabled_until_ms
            {
                if let Some(skill_idx) = self.pick_skill() {
                    self.start_cast(skill_idx);
                } else {
                    self.try_weapon_swap();
                }
            } else if self.pending.is_none() {
                self.try_stunbreak();
            }

            self.now_ms = self.now_ms.saturating_add(TIMELINE_TICK_MS);
            self.tick_recharge_rate();
        }
        // A cast that finishes as the window closes still delivered its hits.
        if self.player_health > 0.0 {
            self.now_ms = self.now_ms.min(self.profile.duration_ms);
            self.land_scheduled_hits();
        }
    }

    /// How fast skills come back this tick.
    ///
    /// Dummy clock: 100ms wall consumes 125ms CD under Alacrity, and 34ms
    /// under Chilled. Apply *4/5 at set is the leftover snapshot.
    ///
    /// Only the Alacrity half of this existed. Nothing the enemy did could
    /// touch skill availability, which leaves out the thing that actually
    /// kills a support: Chilled does not have to out-damage your healing, it
    /// only has to keep your heal on cooldown until the next burst lands. It
    /// is also what makes cleansing existential rather than a damage tax -
    /// the cleanse is buying back the heal, not the 130/s tick.
    fn tick_recharge_rate(&mut self) {
        if !self.now_ms.is_multiple_of(100) {
            return;
        }
        // Chilled wins: the wiki is explicit that Alacrity and Chilled are
        // both recharge-rate modifiers and the slow applies to the already
        // hastened rate, but a build that is Chilled through its whole
        // recovery window is in the situation this models either way.
        if self.has_condition("Chilled") {
            let lost = 100 - CHILLED_RECHARGE_PERCENT;
            for ready in &mut self.cooldown_ready_ms {
                if *ready > self.now_ms {
                    *ready = ready.saturating_add(lost);
                }
            }
            return;
        }
        let extra = alacrity_cd_advance_ms(100, self.has_buff("Alacrity")).saturating_sub(100);
        if extra == 0 {
            return;
        }
        for ready in &mut self.cooldown_ready_ms {
            if *ready > self.now_ms {
                *ready = ready.saturating_sub(extra);
            }
        }
    }

    /// Whether a condition the enemy applied is currently on the player.
    fn has_condition(&self, name: &str) -> bool {
        self.incoming_conditions
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(name) && c.expires_at_ms > self.now_ms)
    }

    fn expire_timed_state(&mut self) {
        self.update_conditionals();
        self.defenses
            .retain(|defense| defense.expires_at_ms > self.now_ms);
        self.buffs.retain(|buff| buff.expires_at_ms > self.now_ms);
        self.barrier
            .retain(|layer| layer.expires_at_ms > self.now_ms);
        if self
            .combo_field
            .as_ref()
            .is_some_and(|field| field.expires_at_ms <= self.now_ms)
        {
            self.combo_field = None;
        }
    }

    /// Land every scheduled hit that is due, in the order they were queued.
    /// Protection is judged when the hit lands, the same way a resolved cast
    /// judges it, so a channel that starts inside a window and runs out of it
    /// splits its hits between the two.
    fn land_scheduled_hits(&mut self) {
        if self.scheduled_hits.is_empty() {
            return;
        }
        let protected = self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.protected_at_start || pending.saved_by_charge)
            || self.control_owned();
        let mut i = 0;
        while i < self.scheduled_hits.len() {
            if self.scheduled_hits[i].at_ms > self.now_ms {
                i += 1;
                continue;
            }
            let hit = self.scheduled_hits.remove(i);
            self.apply_skill_effect(
                hit.skill_id,
                &SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: hit.dmg_multiplier,
                },
                protected,
            );
        }
    }

    fn resolve_pending_cast(&mut self) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if pending.resolves_at_ms > self.now_ms {
            return;
        }
        // The final hit is scheduled for this very tick; land it before the
        // cast's other effects so the order matches the game.
        self.land_scheduled_hits();
        self.pending = None;
        // Wiki `Skill` (read 2026-09-07): "Once the activation is complete a
        // skill will enter a recharge time before it may be used again."
        // ponytail: channels really start recharging at the start of their
        // active phase (wiki `Channeled skill`); we have no phase data, so a
        // channel's recharge runs late by its channel length here.
        {
            let skill = &self.skills[pending.skill_idx];
            let (skill_id, cooldown_ms) = (skill.skill_id, skill.cooldown_ms);
            self.set_skill_cooldown(skill_id, cooldown_ms);
        }
        self.successful_action_count += 1;
        let protected_before = pending.protected_at_start || pending.saved_by_charge;
        let first_damage_event = self.damage_events.len();
        let control_before = self.control_landed_ms;
        let skill_id = self.skills[pending.skill_idx].skill_id;
        let effects = self.skills[pending.skill_idx].effects.clone();
        let applies_condition = effects
            .iter()
            .any(|effect| matches!(effect, SkillEffect::ApplyCondition { .. }));
        let supports_allies = effects.iter().any(|effect| {
            matches!(
                effect,
                SkillEffect::Healing { .. }
                    | SkillEffect::Barrier { .. }
                    | SkillEffect::RemovesCondition { .. }
                    | SkillEffect::ConvertConditions
                    | SkillEffect::ApplyBuff { .. }
            )
        });
        for effect in effects {
            // Strikes were scheduled at cast start and have landed by now.
            if matches!(effect, SkillEffect::StrikeDamage { .. }) {
                continue;
            }
            self.apply_skill_effect(skill_id, &effect, protected_before);
        }
        self.trigger_procs(
            TriggerRule::OnSkillUse,
            Some(skill_id),
            protected_before,
            1.0,
        );
        let protected = protected_before || self.control_owned();
        if protected {
            self.protected_action_count += 1;
            for event in &mut self.damage_events[first_damage_event..] {
                event.protected = true;
            }
            self.protected_actions.push(ProtectedActionEvent {
                at_ms: self.now_ms,
                skill_id,
                control_ms: self.control_landed_ms.saturating_sub(control_before),
                applies_condition,
                supports_allies,
            });
        }
    }

    fn start_cast(&mut self, skill_idx: usize) {
        let skill = &self.skills[skill_idx];
        self.pay_resource(skill.skill_id);
        self.resource_blocked_skills.remove(&skill.skill_id);
        self.apply_incoming_confusion_on_skill_use();
        let quickness = self.has_buff("Quickness");
        let cast_ms = if quickness {
            (skill.cast_time_ms * 2 + 1) / 3
        } else {
            skill.cast_time_ms
        }
        .max(TIMELINE_TICK_MS);
        // Strikes land across the activation, not as one lump at the end:
        // measured spacing where data/formulas/hit_timing.json has it, an
        // even spread otherwise. Everything else the skill does resolves at
        // cast end as before.
        let hits: Vec<ScheduledHit> = skill
            .effects
            .iter()
            .filter_map(|effect| match effect {
                SkillEffect::StrikeDamage {
                    hit_count,
                    dmg_multiplier,
                } => Some((*hit_count, *dmg_multiplier)),
                _ => None,
            })
            .flat_map(|(hit_count, dmg_multiplier)| {
                let per_hit = dmg_multiplier / hit_count.max(1) as f64;
                crate::data::hit_timing::hit_schedule(&skill.name, cast_ms, hit_count)
                    .into_iter()
                    .map(move |offset| (offset, per_hit))
            })
            .map(|(offset, per_hit)| ScheduledHit {
                at_ms: self.at(offset),
                skill_id: skill.skill_id,
                dmg_multiplier: per_hit,
            })
            .collect();
        self.scheduled_hits.extend(hits);
        // Recharge starts when the cast resolves, not here — see
        // `resolve_pending_cast`. An interrupted cast gets the short interrupt
        // recharge instead, which only works if the full one is not yet set.
        self.pending = Some(PendingCast {
            skill_idx,
            started_at_ms: self.now_ms,
            resolves_at_ms: self.at(cast_ms),
            protected_at_start: self.control_owned(),
            saved_by_charge: false,
        });
        self.next_action_ms = self.at(cast_ms
            .saturating_add(HUMAN_DELAY_MS)
            .saturating_add(MIN_SKILL_GAP_MS));
    }

    fn pick_skill(&mut self) -> Option<usize> {
        // Follow the published rotation while it can be followed. A skill
        // already on recharge was pressed; one on the other set asks for a
        // swap when the swap is ready and is skipped when it is not; one the
        // build cannot pay for hands over to the scorer below.
        while let Some(&want) = self.opener.get(self.opener_cursor) {
            let Some(idx) = self.skills.iter().position(|s| s.skill_id == want) else {
                self.opener_cursor += 1;
                continue;
            };
            if self.cooldown_ready_ms[idx] > self.now_ms {
                self.opener_cursor += 1;
                continue;
            }
            if !self.skill_available(&self.skills[idx]) {
                if self.weapon_swap_cooldown_ms.is_some()
                    && self.now_ms >= self.weapon_swap_ready_ms
                {
                    return None;
                }
                self.opener_cursor += 1;
                continue;
            }
            if !self.can_pay_resource(want) {
                break;
            }
            self.opener_cursor += 1;
            return Some(idx);
        }
        let health_ratio = self.player_health / self.params.max_health.max(1.0);
        let cover_remaining = self.control_cover_remaining_ms();
        let enemy_event_soon = self
            .profile
            .enemy_events
            .front()
            .is_some_and(|event| event.at_ms <= self.at(900));

        let mut best: Option<(usize, f64)> = None;
        let mut filler = None;
        let mut highest_unpaid: Option<(u32, f64)> = None;
        for (idx, skill) in self.skills.iter().enumerate() {
            if self.cooldown_ready_ms[idx] > self.now_ms || !self.skill_available(skill) {
                continue;
            }
            if skill.slot == SkillSlot::Weapon1 && skill.cooldown_ms == 0 {
                filler = Some(idx);
                continue;
            }
            let has_heal = skill
                .effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::Healing { .. }));
            if has_heal && health_ratio > 0.72 {
                continue;
            }
            let has_cover = skill.effects.iter().any(is_control_cover);
            let has_strip = skill.effects.iter().any(|effect| {
                matches!(
                    effect,
                    SkillEffect::StripBoons { .. }
                        | SkillEffect::StealBoons
                        | SkillEffect::CorruptBoons
                )
            });
            let has_control = skill
                .effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::CrowdControl { .. }));

            let mut priority = self.skill_damage_value(skill);
            if has_heal {
                priority += (1.0 - health_ratio) * 1_000_000.0;
            }
            if has_cover && (cover_remaining < self.profile.required_window_ms || enemy_event_soon)
            {
                priority += 900_000.0;
            }
            // Strip Stability/Protection before trying to CC or dump damage.
            if has_strip && (self.enemy_stability || self.enemy_protection) {
                priority += 800_000.0;
            }
            if has_control && !self.enemy_stability {
                priority += 700_000.0;
            }
            if has_control && self.enemy_stability {
                priority -= 500_000.0;
            }
            if !self.can_pay_resource(skill.skill_id) {
                if highest_unpaid
                    .as_ref()
                    .is_none_or(|(_, blocked_priority)| priority > *blocked_priority)
                {
                    highest_unpaid = Some((skill.skill_id, priority));
                }
                continue;
            }
            if best.as_ref().is_none_or(|(_, score)| priority > *score) {
                best = Some((idx, priority));
            }
        }
        let legal_priority = best.as_ref().map(|(_, score)| *score).unwrap_or(0.0);
        if let Some((skill_id, blocked_priority)) = highest_unpaid {
            if blocked_priority > legal_priority {
                self.resource_blocked_skills.insert(skill_id);
            }
        }
        best.map(|(idx, _)| idx).or(filler)
    }

    fn skill_available(&self, skill: &RotationSkill) -> bool {
        skill.weapon_set == 0 || skill.weapon_set == self.active_weapon_set
    }

    fn try_weapon_swap(&mut self) {
        let Some(cooldown_ms) = self.weapon_swap_cooldown_ms else {
            return;
        };
        if self.now_ms < self.weapon_swap_ready_ms
            || !self.skills.iter().any(|skill| skill.weapon_set > 0)
        {
            return;
        }
        let other = if self.active_weapon_set == 1 { 2 } else { 1 };
        if self.skills.iter().enumerate().any(|(idx, skill)| {
            skill.weapon_set == other && self.cooldown_ready_ms[idx] <= self.now_ms
        }) {
            self.active_weapon_set = other;
            self.weapon_swap_ready_ms = self.at(cooldown_ms);
            self.next_action_ms = self.at(MIN_SKILL_GAP_MS);
            self.trace(TraceKind::WeaponSwap, "weapon swap", format!("set {other}"));
        }
    }

    fn try_stunbreak(&mut self) {
        if self.now_ms >= self.disabled_until_ms {
            return;
        }
        let candidates: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(idx, skill)| {
                skill.is_stunbreak
                    && self.cooldown_ready_ms[*idx] <= self.now_ms
                    && self.skill_available(skill)
            })
            .map(|(idx, _)| idx)
            .collect();
        let Some(idx) = candidates.into_iter().find(|idx| {
            let skill_id = self.skills[*idx].skill_id;
            let can_pay = self.can_pay_resource(skill_id);
            if !can_pay {
                self.resource_blocked_skills.insert(skill_id);
            }
            can_pay
        }) else {
            return;
        };
        let skill_id = self.skills[idx].skill_id;
        self.pay_resource(skill_id);
        self.resource_blocked_skills.remove(&skill_id);
        self.set_skill_cooldown(skill_id, self.skills[idx].cooldown_ms);
        self.disabled_until_ms = self.now_ms;
        self.next_action_ms = self.at(MIN_SKILL_GAP_MS);
        self.successful_action_count += 1;
        let first_damage_event = self.damage_events.len();
        let control_before = self.control_landed_ms;
        let applies_condition = self.skills[idx]
            .effects
            .iter()
            .any(|effect| matches!(effect, SkillEffect::ApplyCondition { .. }));
        let supports_allies = self.skills[idx].effects.iter().any(|effect| {
            matches!(
                effect,
                SkillEffect::Healing { .. }
                    | SkillEffect::Barrier { .. }
                    | SkillEffect::RemovesCondition { .. }
                    | SkillEffect::ConvertConditions
                    | SkillEffect::ApplyBuff { .. }
            )
        });
        for effect in self.skills[idx].effects.clone() {
            self.apply_skill_effect(skill_id, &effect, true);
        }
        self.trigger_procs(TriggerRule::OnSkillUse, Some(skill_id), true, 1.0);
        for event in &mut self.damage_events[first_damage_event..] {
            event.protected = true;
        }
        self.protected_action_count += 1;
        self.protected_actions.push(ProtectedActionEvent {
            at_ms: self.now_ms,
            skill_id,
            control_ms: self.control_landed_ms.saturating_sub(control_before),
            applies_condition,
            supports_allies,
        });
    }

    fn process_enemy_events(&mut self) {
        if self.enemy_health <= 0.0 {
            return;
        }
        while self
            .profile
            .enemy_events
            .front()
            .is_some_and(|event| event.at_ms <= self.now_ms)
        {
            let event = self
                .profile
                .enemy_events
                .pop_front()
                .expect("front checked");
            // A disabled opponent cannot continue a queued attack/cast. Existing
            // conditions still tick separately, but new strikes, CC, strips and
            // condition applications are lost during the control window.
            if self.enemy_disabled_until_ms > self.now_ms {
                continue;
            }
            match event.kind {
                EnemyEventKind::Strike {
                    damage,
                    unblockable,
                } => self.receive_strike(damage, unblockable),
                EnemyEventKind::Control {
                    duration_ms,
                    unblockable,
                } => self.receive_control(duration_ms, unblockable),
                EnemyEventKind::Condition {
                    condition,
                    stacks,
                    duration_ms,
                } => self.receive_condition(condition, stacks, duration_ms),
                EnemyEventKind::BoonStrip { count } => self.receive_boon_strip(count),
            }
        }
    }

    fn receive_strike(&mut self, raw_damage: f64, unblockable: bool) {
        if self.avoids_attack(unblockable) {
            self.avoided_damage += raw_damage;
            return;
        }
        let mut damage = raw_damage;
        if self.has_defense(CoverKind::Protection) {
            damage *= self.protection_multiplier;
        }
        self.absorb_damage(damage);
    }

    fn receive_control(&mut self, duration_ms: u32, unblockable: bool) {
        if self.avoids_attack(unblockable) || self.consume_stability() {
            return;
        }
        if let Some(pending) = self.pending.take() {
            if pending.started_at_ms < self.now_ms {
                self.interrupted_casts += 1;
            }
            // Hits that had not landed yet die with the cast.
            let lost = self.scheduled_hits.len();
            self.scheduled_hits.clear();
            let skill_id = self.skills[pending.skill_idx].skill_id;
            if self.trace_enabled {
                let name = self.skill_name(skill_id);
                self.trace(
                    TraceKind::CastInterrupted,
                    &name,
                    format!("{lost} hits lost"),
                );
            }
            self.set_skill_cooldown(skill_id, INTERRUPT_COOLDOWN_MS);
        }
        self.disabled_until_ms = self.disabled_until_ms.max(self.at(duration_ms));
        self.protected_run_ms = 0;
    }

    fn receive_condition(&mut self, condition: String, stacks: u32, duration_ms: u32) {
        if self.has_defense(CoverKind::Invulnerability)
            || (self.has_defense(CoverKind::Resistance) && !condition_is_damaging(&condition))
        {
            return;
        }
        self.incoming_conditions.push(TimedCondition {
            name: condition,
            stacks,
            expires_at_ms: self.at(duration_ms),
            next_tick_ms: self.at(1_000),
        });
    }

    fn receive_boon_strip(&mut self, count: u32) {
        // Generic strips are last-in-first-out over first application
        // (community-tested, not dev-stated: MetaBattle WvW Firebrand guide,
        // forum 153106, read 2026-09-07). Corrupts are random since the
        // 2015-06-23 patch; the enemy script only strips, so no random path.
        for _ in 0..count {
            let Some((idx, _)) = self
                .defenses
                .iter()
                .enumerate()
                .filter(|(_, defense)| defense.strippable)
                .max_by_key(|(_, defense)| defense.applied_at_ms)
            else {
                break;
            };
            self.defenses.remove(idx);
        }
    }

    fn remove_enemy_boons(&mut self, count: u32) -> Vec<&'static str> {
        let mut stripped = Vec::new();
        for _ in 0..count {
            if self.enemy_stability {
                self.enemy_stability = false;
                stripped.push("Stability");
            } else if self.enemy_protection {
                self.enemy_protection = false;
                stripped.push("Protection");
            } else {
                break;
            }
        }
        stripped
    }

    fn avoids_attack(&mut self, unblockable: bool) -> bool {
        if self.has_defense(CoverKind::Invulnerability) || self.has_defense(CoverKind::Evade) {
            return true;
        }
        if !unblockable {
            if self.consume_defense(CoverKind::Aegis) {
                self.mark_charge_cover_consumed();
                return true;
            }
            if self.has_defense(CoverKind::Block) {
                return true;
            }
        }
        if !unblockable && self.consume_defense(CoverKind::Blind) {
            self.mark_charge_cover_consumed();
            return true;
        }
        false
    }

    fn mark_charge_cover_consumed(&mut self) {
        self.charge_cover_consumed_this_tick = true;
        if let Some(pending) = self.pending.as_mut() {
            pending.saved_by_charge = true;
        }
    }

    fn apply_incoming_confusion_on_skill_use(&mut self) {
        let stacks: u32 = self
            .incoming_conditions
            .iter()
            .filter(|c| c.name.eq_ignore_ascii_case("Confusion") && c.expires_at_ms > self.now_ms)
            .map(|c| c.stacks)
            .sum();
        if stacks == 0 {
            return;
        }
        let dmg = crate::data::conditions().confusion_tick(1_800.0, self.params.mode.clone(), true)
            * stacks as f64;
        self.absorb_damage(dmg);
    }

    fn absorb_damage(&mut self, damage: f64) {
        let mut remaining = damage;
        let mut absorbed = 0.0;
        while remaining > 0.0 {
            let Some(layer) = self.barrier.front_mut() else {
                break;
            };
            let take = layer.amount.min(remaining);
            layer.amount -= take;
            remaining -= take;
            absorbed += take;
            if layer.amount <= 0.0 {
                self.barrier.pop_front();
            }
        }
        self.barrier_absorbed += absorbed;
        self.incoming_damage += remaining;
        self.player_health = (self.player_health - remaining).max(0.0);
    }

    fn apply_barrier(&mut self, amount: f64) {
        if amount <= 0.0 {
            return;
        }
        let cap = self.params.max_health * WVW_BARRIER_HEALTH_FRACTION;
        let current: f64 = self.barrier.iter().map(|layer| layer.amount).sum();
        let applied = amount.min((cap - current).max(0.0));
        if applied > 0.0 {
            self.barrier.push_back(BarrierLayer {
                amount: applied,
                expires_at_ms: self.now_ms.saturating_add(BARRIER_LIFETIME_MS),
            });
        }
    }

    fn tick_conditions(&mut self) {
        let mut outgoing_damage = 0.0;
        let might = self.buff_stacks("Might") as f64;
        let condition_damage = self.params.condition_damage
            + might * crate::data::boon_condition_formulas::boons().might_condi_per_stack();
        for condition in &mut self.outgoing_conditions {
            if condition.next_tick_ms <= self.now_ms
                && condition.next_tick_ms <= condition.expires_at_ms
            {
                let tick =
                    condition_tick_damage(&condition.name, condition_damage, &self.params.mode)
                        * condition.stacks as f64
                        * self.params.condition_mult;
                outgoing_damage += tick;
                condition.next_tick_ms += 1_000;
            }
            if condition.expires_at_ms <= self.now_ms {
                let frac = leftover_condition_fraction(condition);
                if frac > 0.0 {
                    outgoing_damage +=
                        condition_tick_damage(&condition.name, condition_damage, &self.params.mode)
                            * condition.stacks as f64
                            * self.params.condition_mult
                            * frac;
                }
            }
        }
        if outgoing_damage > 0.0 {
            self.record_damage(outgoing_damage, self.control_owned());
        }

        let mut incoming_damage = 0.0;
        for condition in &mut self.incoming_conditions {
            if condition.next_tick_ms <= self.now_ms
                && condition.next_tick_ms <= condition.expires_at_ms
            {
                incoming_damage +=
                    condition_tick_damage(&condition.name, 1_800.0, &self.params.mode)
                        * condition.stacks as f64;
                condition.next_tick_ms += 1_000;
            }
            if condition.expires_at_ms <= self.now_ms {
                let frac = leftover_condition_fraction(condition);
                if frac > 0.0 {
                    incoming_damage +=
                        condition_tick_damage(&condition.name, 1_800.0, &self.params.mode)
                            * condition.stacks as f64
                            * frac;
                }
            }
        }
        if incoming_damage > 0.0 {
            self.absorb_damage(incoming_damage);
        }
        self.outgoing_conditions
            .retain(|condition| condition.expires_at_ms > self.now_ms);
        self.incoming_conditions
            .retain(|condition| condition.expires_at_ms > self.now_ms);
    }

    fn apply_skill_effect(&mut self, skill_id: u32, effect: &SkillEffect, protected: bool) {
        match effect {
            SkillEffect::StrikeDamage {
                hit_count,
                dmg_multiplier,
            } => {
                let might = self.buff_stacks("Might") as f64;
                let power = self.params.power
                    + might * crate::data::boon_condition_formulas::boons().might_power_per_stack();
                let fury_bonus = if self.has_buff("Fury") {
                    self.params.fury_crit_chance_bonus
                } else {
                    0.0
                };
                let mut damage = self.params.weapon_strength * power / reference_armor()
                    * dmg_multiplier
                    * *hit_count as f64
                    * strike_crit_factor_with_bonus(
                        self.params.precision,
                        self.params.ferocity,
                        self.params.crit_chance_bonus + fury_bonus,
                    )
                    * self.params.strike_mult;
                if self.enemy_protection {
                    damage *= self.protection_multiplier;
                }
                // Conditional bonuses (US3), evaluated at the strike.
                self.update_conditionals();
                damage *= self.strike_conditional_mult();
                self.record_damage(damage, protected);
                if self.trace_enabled {
                    let name = self.skill_name(skill_id);
                    self.trace(TraceKind::HitLanded, &name, format!("{damage:.1}"));
                }
                self.remove_defense(CoverKind::Stealth);
                self.gain_resource_on_hit(skill_id);
                self.gain_conditional_stacks(skill_id);
                self.trigger_procs(TriggerRule::OnHit, Some(skill_id), protected, 1.0);
                // On-crit procs (CONN-00-06): one chance per landed hit at the
                // crit chance the damage line above already priced in.
                let crit = crit_chance_fraction(
                    self.params.precision,
                    self.params.crit_chance_bonus + fury_bonus,
                );
                if crit > 0.0 {
                    for _ in 0..*hit_count {
                        self.trigger_procs(TriggerRule::OnCrit, Some(skill_id), protected, crit);
                    }
                }
            }
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                let duration =
                    (*duration_ms as f64 * self.params.condition_duration_mult).round() as u32;
                self.outgoing_conditions.push(TimedCondition {
                    name: condition.clone(),
                    stacks: *stacks,
                    expires_at_ms: self.at(duration),
                    next_tick_ms: self.at(1_000),
                });
            }
            SkillEffect::ApplyBuff {
                buff,
                stacks,
                duration_ms,
            } => self.apply_buff(buff, *stacks, *duration_ms, true),
            SkillEffect::ComboField {
                field_type,
                duration_ms,
            } => {
                self.combo_field = Some(ComboFieldState {
                    field_type: field_type.clone(),
                    expires_at_ms: self.now_ms.saturating_add(*duration_ms),
                });
            }
            SkillEffect::ComboFinisher {
                finisher_type,
                percent,
            } => self.resolve_combo(finisher_type, *percent),
            SkillEffect::Healing { hit_count } => {
                let amount = (1_200.0 + self.params.healing_power * 0.45)
                    * *hit_count as f64
                    * self.params.healing_mult;
                self.heal(amount);
            }
            SkillEffect::Barrier { amount } => {
                self.apply_barrier(amount + self.params.healing_power * 0.30);
            }
            SkillEffect::RemovesCondition { conditions_removed } => {
                self.cleanse(*conditions_removed)
            }
            SkillEffect::CrowdControl { duration_ms, .. } => {
                if !self.enemy_stability {
                    let previous_end = self.enemy_disabled_until_ms.max(self.now_ms);
                    let new_end = self.enemy_disabled_until_ms.max(self.at(*duration_ms));
                    self.enemy_disabled_until_ms = new_end;
                    self.control_landed_ms += new_end.saturating_sub(previous_end);
                }
            }
            SkillEffect::StripBoons {
                count_per_pulse,
                interval_ms,
                window_ms,
            } => {
                let _ = self.remove_enemy_boons(if *interval_ms == 0 {
                    // Zero interval = one immediate pulse, not a division.
                    *count_per_pulse
                } else {
                    *count_per_pulse
                        * ((*window_ms).max(*interval_ms))
                            .checked_div(*interval_ms)
                            .unwrap_or(0)
                });
            }
            SkillEffect::CorruptBoons => {
                for boon in self.remove_enemy_boons(1) {
                    if let Some(condition) = corrupt_into(boon) {
                        self.outgoing_conditions.push(TimedCondition {
                            name: condition.into(),
                            stacks: 1,
                            expires_at_ms: self.now_ms.saturating_add(1_000),
                            next_tick_ms: self.now_ms.saturating_add(1_000),
                        });
                    }
                }
            }
            SkillEffect::StealBoons => {
                for boon in self.remove_enemy_boons(1) {
                    self.apply_buff(boon, 1, 1_000, true);
                }
            }
            SkillEffect::ConvertConditions => {
                let count = self.incoming_conditions.len() as u32;
                self.cleanse(count.max(1));
                self.apply_buff("Protection", 1, 3_000, true);
            }
            SkillEffect::Cover {
                kind,
                duration_ms,
                strippable,
            } => self.apply_defense(*kind, *duration_ms, 1, *strippable),
            // Mobility is a capability tag, not a duration source. Quantitative
            // cover must arrive as a mode-aware `Cover` fact.
            SkillEffect::Mobility { .. } => {}
        }
    }

    fn resolve_combo(&mut self, finisher_type: &str, percent: u32) {
        if percent < 100 {
            if percent > 0 {
                self.note_unmodeled(format!("{finisher_type} finisher (partial combo)"));
            }
            return;
        }
        let Some(field) = self.combo_field.as_ref() else {
            return;
        };
        let field_name = field.field_type.clone();
        let field_type = field_name.to_lowercase();
        let finisher = finisher_type.to_lowercase();
        let unmodeled =
            |why: &str| format!("{field_name} field + {finisher_type} finisher ({why})");
        self.combo_activations += 1;
        if field_type.contains("smoke") {
            if finisher.contains("blast") || finisher.contains("leap") {
                self.apply_defense(CoverKind::Stealth, 3_000, 1, false);
            } else if finisher.contains("projectile") || finisher.contains("whirl") {
                self.apply_defense(CoverKind::Blind, 3_000, 1, false);
            } else {
                self.note_unmodeled(unmodeled("unmodeled combo"));
            }
        } else if field_type.contains("water") {
            if finisher.contains("blast") {
                self.heal(1_320.0 + self.params.healing_power * 0.20);
            } else if finisher.contains("leap") {
                self.heal(1_300.0 + self.params.healing_power * 0.50);
            } else {
                // Projectile/whirl apply regeneration; periodic regeneration
                // is not represented by this timeline yet.
                self.note_unmodeled(unmodeled("unmodeled combo"));
            }
        } else if field_type.contains("light") {
            if finisher.contains("blast") {
                self.cleanse(1);
            } else {
                self.note_unmodeled(unmodeled("unmodeled combo"));
            }
        } else if field_type.contains("fire") {
            if finisher.contains("blast") {
                self.apply_buff("Might", 3, 20_000, true);
            } else {
                self.note_unmodeled(unmodeled("unmodeled combo"));
            }
        } else if field_type.contains("dark") {
            // Dark finishers produce auras or life-steal effects, not Blind.
            self.note_unmodeled(unmodeled("dark field"));
        } else {
            self.note_unmodeled(unmodeled("unmodeled combo"));
        }
    }

    fn apply_buff(&mut self, name: &str, stacks: u32, duration_ms: u32, scale_duration: bool) {
        let duration = if scale_duration {
            (duration_ms as f64 * self.params.boon_duration_mult).round() as u32
        } else {
            duration_ms
        };
        // Wiki `Effect stacking`: most boons cap at 30 s remaining, Swiftness
        // at 60 s, Might/Aegis/Regeneration uncapped — data/formulas/boons.json.
        let duration = match crate::data::boons().get(name).and_then(|b| b.max_duration) {
            Some(cap_s) => duration.min(cap_s * 1_000),
            None => duration,
        };
        self.buffs.push(TimedBuff {
            name: name.into(),
            stacks,
            expires_at_ms: self.at(duration),
        });
        if let Some(kind) = boon_cover_kind(name) {
            self.apply_defense(kind, duration, stacks, true);
        }
    }

    fn apply_defense(&mut self, kind: CoverKind, duration_ms: u32, stacks: u32, strippable: bool) {
        if let Some(existing) = self
            .defenses
            .iter_mut()
            .find(|defense| defense.kind == kind)
        {
            existing.expires_at_ms = existing
                .expires_at_ms
                .max(self.now_ms.saturating_add(duration_ms));
            existing.stacks = existing.stacks.max(stacks);
            existing.strippable &= strippable;
        } else {
            self.defenses.push(TimedDefense {
                kind,
                expires_at_ms: self.at(duration_ms),
                stacks,
                strippable,
                applied_at_ms: self.now_ms,
            });
        }
    }

    /// `scale` is the expected-value weight of the firing (1.0 for a certain
    /// proc). Stacks and counts are whole numbers in the game, so the scaled
    /// amount rounds to nearest and a rounding to zero applies nothing.
    // ponytail: fractional boon stacks would need a fractional ledger; the
    // rounding is the approximation the trace's trials measure against.
    fn apply_operation(
        &mut self,
        operation: Option<&crate::data::normalized_effects::StatusOperation>,
        scale: f64,
    ) {
        let Some(operation) = operation else {
            return;
        };
        let amount = (resolved(&operation.amount_value)
            .copied()
            .unwrap_or(1.0)
            .max(1.0)
            * scale)
            .round() as u32;
        if amount == 0 {
            return;
        }
        let duration = operation
            .base_duration_ms
            .as_ref()
            .and_then(resolved)
            .copied()
            .unwrap_or(1_000);
        match (&operation.operation_type, &operation.target_side) {
            (OperationType::AppliesBoon, TargetSide::Self_ | TargetSide::Ally) => {
                self.apply_buff(&operation.status_kind, amount, duration, true)
            }
            (OperationType::AppliesCondition, TargetSide::Enemy) => {
                self.outgoing_conditions.push(TimedCondition {
                    name: operation.status_kind.clone(),
                    stacks: amount,
                    expires_at_ms: self.at(duration),
                    next_tick_ms: self.at(1_000),
                })
            }
            (OperationType::RemovesCondition, TargetSide::Self_ | TargetSide::Ally)
            | (OperationType::ConvertsConditionToBoon, TargetSide::Self_ | TargetSide::Ally) => {
                self.cleanse(amount)
            }
            (OperationType::RemovesBoon | OperationType::CorruptsBoon, TargetSide::Enemy) => {
                self.enemy_stability = false;
                self.enemy_protection = false;
            }
            _ => {}
        }
    }

    fn skill_directly_models_effect(&self, effect: &NormalizedEffect) -> bool {
        let Some(skill) = self
            .skills
            .iter()
            .find(|skill| skill.skill_id == effect.source_id)
        else {
            return false;
        };
        match effect.category {
            EffectCategory::RemovesCondition => skill
                .effects
                .iter()
                .any(|item| matches!(item, SkillEffect::RemovesCondition { .. })),
            _ => false,
        }
    }

    fn at(&self, offset_ms: u32) -> u32 {
        self.now_ms.saturating_add(offset_ms)
    }

    fn note_unmodeled_proc(&mut self, source_type: &SourceType, source_id: u32, name: &str) {
        let tag = match source_type {
            SourceType::Trait => 0,
            SourceType::Skill => 1,
            SourceType::Rune => 2,
            SourceType::Sigil => 3,
            SourceType::Relic => 4,
        };
        // Deduplicated by source key, not by name: two sources may share a
        // name and still be two lines.
        if self.unmodeled_proc_keys.insert((tag, source_id)) {
            self.unmodeled_names
                .push(format!("{name} (unsupported proc)"));
            self.trace(TraceKind::ProcUnmodeled, name, "unsupported proc");
        }
    }

    /// Record one source the timeline is not simulating. Deduplicated by the
    /// full `"{name} ({why})"` string, so a combo that resolves every cast
    /// or a proc that is skipped every hit is one line, not one per tick.
    fn note_unmodeled(&mut self, name: String) {
        if !self.unmodeled_names.contains(&name) {
            self.unmodeled_names.push(name);
        }
    }

    /// Append one trace event when tracing is on; the 513th sets
    /// `trace_truncated` and is dropped.
    fn trace(&mut self, kind: TraceKind, source: &str, detail: impl Into<String>) {
        if !self.trace_enabled {
            return;
        }
        if self.trace.len() >= TRACE_CAP {
            self.trace_truncated = true;
            return;
        }
        self.trace.push(TraceEvent {
            t_ms: self.now_ms,
            kind,
            source: source.into(),
            detail: detail.into(),
        });
    }

    /// Re-read every threshold against the regular health pool and expire
    /// stacks, tracing each state change (US3).
    fn update_conditionals(&mut self) {
        let ratio = self.player_health / self.params.max_health.max(1.0);
        let now = self.now_ms;
        let mut changes = Vec::new();
        for spec in &mut self.conditional_specs {
            match spec.kind {
                ConditionalKind::Threshold { above, percent } => {
                    let holds = if above {
                        ratio > percent / 100.0
                    } else {
                        ratio < percent / 100.0
                    };
                    if holds != spec.active {
                        spec.active = holds;
                        changes.push((
                            spec.source_name.clone(),
                            holds,
                            format!("health {:.0}%", ratio * 100.0),
                        ));
                    }
                }
                ConditionalKind::Stacking { .. } => {
                    if spec.stacks > 0 && spec.expires_at_ms <= now {
                        spec.stacks = 0;
                        changes.push((
                            spec.source_name.clone(),
                            false,
                            "stacks expired".to_string(),
                        ));
                    }
                }
            }
        }
        for (name, on, detail) in changes {
            let kind = if on {
                TraceKind::ConditionalActivated
            } else {
                TraceKind::ConditionalExpired
            };
            self.trace(kind, &name, detail);
        }
    }

    /// The strike multiplier of every conditional bonus that holds now.
    fn strike_conditional_mult(&self) -> f64 {
        self.conditional_specs
            .iter()
            .map(|spec| match spec.kind {
                ConditionalKind::Threshold { .. } if spec.active => 1.0 + spec.percent / 100.0,
                ConditionalKind::Stacking { .. } => 1.0 + spec.stacks as f64 * spec.percent / 100.0,
                _ => 1.0,
            })
            .product()
    }

    /// Whether `skill_id` counts for a trigger scope.
    fn scope_admits(
        &self,
        scope: &crate::data::normalized_effects::TriggerScope,
        skill_id: Option<u32>,
    ) -> bool {
        match scope {
            crate::data::normalized_effects::TriggerScope::Any => true,
            crate::data::normalized_effects::TriggerScope::WeaponSkillWithRecharge => skill_id
                .and_then(|id| self.skills.iter().find(|s| s.skill_id == id))
                .is_some_and(|s| s.weapon_set != 0 && s.cooldown_ms > 0),
        }
    }

    /// A landed hit from `skill_id` feeds every stacking bonus in scope.
    fn gain_conditional_stacks(&mut self, skill_id: u32) {
        let now = self.now_ms;
        let mut gained = Vec::new();
        for idx in 0..self.conditional_specs.len() {
            let ConditionalKind::Stacking {
                max,
                duration_ms,
                ref scope,
            } = self.conditional_specs[idx].kind
            else {
                continue;
            };
            if !self.scope_admits(scope, Some(skill_id)) {
                continue;
            }
            let spec = &mut self.conditional_specs[idx];
            spec.stacks = (spec.stacks + 1).min(max);
            spec.expires_at_ms = now.saturating_add(duration_ms);
            gained.push((spec.source_name.clone(), format!("{}/{max}", spec.stacks)));
        }
        for (name, detail) in gained {
            self.trace(TraceKind::StackGained, &name, detail);
        }
    }

    /// Tag each sigil's procs with the weapon set it is socketed on.
    fn assign_sigil_sets(&mut self, sigil_sets: &HashMap<u32, u8>) {
        for spec in &mut self.proc_specs {
            if matches!(spec.source_type, SourceType::Sigil) {
                spec.weapon_set = sigil_sets.get(&spec.source_id).copied().unwrap_or(0);
            }
        }
    }

    /// The set a hit from `skill_id` was cast on: a weapon skill's own set,
    /// which is also right for a channel that finishes after a swap; the
    /// held set for everything else.
    fn held_set_for(&self, skill_id: Option<u32>) -> u8 {
        skill_id
            .and_then(|id| self.skills.iter().find(|s| s.skill_id == id))
            .map(|s| s.weapon_set)
            .filter(|set| matches!(set, 1 | 2))
            .unwrap_or(self.active_weapon_set)
    }

    /// The sources `load_normalized_effects` could not model are known before
    /// tracing is switched on; replay them as events once it is.
    fn trace_loaded_unmodeled(&mut self) {
        if !self.trace_enabled {
            return;
        }
        for name in self.unmodeled_names.clone() {
            self.trace(TraceKind::ProcUnmodeled, &name, "no firing site");
        }
        for name in self.no_record_names.clone() {
            self.trace(TraceKind::ProcUnmodeled, &name, "no record");
        }
    }

    fn skill_name(&self, skill_id: u32) -> String {
        self.skills
            .iter()
            .find(|skill| skill.skill_id == skill_id)
            .map(|skill| skill.name.clone())
            .unwrap_or_else(|| format!("skill {skill_id}"))
    }

    /// `weight` is the probability that this event is the proc's trigger:
    /// 1.0 for a landed hit or a skill use, the critical chance for the
    /// on-crit call. Expected-value mode applies `weight × proc_chance` of
    /// the effect and starts the cooldown once a whole proc's worth of mass
    /// has accumulated (R1); seeded mode draws.
    fn trigger_procs(
        &mut self,
        trigger: TriggerRule,
        activating_skill_id: Option<u32>,
        protected: bool,
        weight: f64,
    ) {
        let mut ready = Vec::new();
        let mut on_cooldown = Vec::new();
        let held_set = self.held_set_for(activating_skill_id);
        for (idx, proc_spec) in self.proc_specs.iter().enumerate() {
            let source_matches = !matches!(proc_spec.source_type, SourceType::Skill)
                || activating_skill_id == Some(proc_spec.source_id);
            let set_held = proc_spec.weapon_set == 0 || proc_spec.weapon_set == held_set;
            let scope_ok = self.scope_admits(&proc_spec.scope, activating_skill_id);
            if same_trigger(&proc_spec.trigger, &trigger) && source_matches && set_held && scope_ok
            {
                if proc_spec.next_ready_ms <= self.now_ms {
                    ready.push(idx);
                } else {
                    on_cooldown.push(idx);
                }
            }
        }
        for idx in on_cooldown {
            let (name, ready_at) = (
                self.proc_specs[idx].source_name.clone(),
                self.proc_specs[idx].next_ready_ms,
            );
            self.trace(
                TraceKind::ProcSkippedIcd,
                &name,
                format!("ready at {ready_at} ms"),
            );
        }
        let on_crit = matches!(trigger, TriggerRule::OnCrit);
        for idx in ready {
            let chance = weight * self.proc_specs[idx].proc_chance;
            let p = match (&mut self.crit_mode, on_crit) {
                (CritMode::Seeded(rng), true) => {
                    if rng.next_f64() < chance {
                        1.0
                    } else {
                        0.0
                    }
                }
                _ => chance,
            };
            if p <= 0.0 {
                continue;
            }
            let (category, value, duration_ms, operation, name) = {
                let proc_spec = &mut self.proc_specs[idx];
                proc_spec.mass += p;
                if proc_spec.mass >= 1.0 - 1e-9 {
                    proc_spec.mass -= 1.0;
                    proc_spec.next_ready_ms =
                        self.now_ms.saturating_add(proc_spec.internal_cooldown_ms);
                }
                (
                    proc_spec.category.clone(),
                    proc_spec.value,
                    proc_spec.duration_ms,
                    proc_spec.operation.clone(),
                    proc_spec.source_name.clone(),
                )
            };
            let fired = match category {
                EffectCategory::StrikeDamagePct => {
                    // A coefficient (≤ 2.0) is a flame-blast style proc on
                    // the unequipped weapon strength that cannot crit (wiki
                    // `Superior Sigil of Fire`, read 2026-09-08); a percent
                    // is a share of the held weapon's strike as before.
                    let proc_damage = if value <= 2.0 {
                        UNEQUIPPED_WEAPON_STRENGTH * self.params.power / reference_armor()
                            * value
                            * self.params.strike_mult
                    } else {
                        self.params.weapon_strength * self.params.power / reference_armor()
                            * as_ratio(value)
                    };
                    self.record_damage(proc_damage * p, protected);
                    true
                }
                EffectCategory::AppliesBoon
                | EffectCategory::AppliesCondition
                | EffectCategory::RemovesBoon
                | EffectCategory::CorruptsBoon
                | EffectCategory::RemovesCondition
                | EffectCategory::ConvertsConditionToBoon
                | EffectCategory::TransfersCondition => {
                    self.apply_operation(operation.as_ref(), p);
                    true
                }
                EffectCategory::OutgoingHealingPct if duration_ms > 0 => {
                    self.heal(value.max(0.0) * p);
                    true
                }
                _ => {
                    let source_type = self.proc_specs[idx].source_type.clone();
                    let source_id = self.proc_specs[idx].source_id;
                    self.note_unmodeled_proc(&source_type, source_id, &name);
                    false
                }
            };
            if fired {
                *self.proc_fire_counts.entry(name.clone()).or_default() += 1;
                self.trace(TraceKind::ProcFired, &name, format!("{category:?} ×{p:.2}"));
            }
        }
    }

    fn track_protected_window(&mut self) {
        if self.control_owned() || self.charge_cover_consumed_this_tick {
            self.secured_tick_times.push(self.now_ms);
            self.protected_run_ms += TIMELINE_TICK_MS;
            self.longest_protected_window_ms =
                self.longest_protected_window_ms.max(self.protected_run_ms);
        } else {
            self.protected_run_ms = 0;
        }
    }

    fn control_owned(&self) -> bool {
        self.enemy_disabled_until_ms > self.now_ms
            || self.has_defense(CoverKind::Stability)
            || self.has_defense(CoverKind::Invulnerability)
            || self.has_defense(CoverKind::Evade)
            || self.has_defense(CoverKind::Stealth)
            || self.has_defense(CoverKind::Block)
    }

    fn control_cover_remaining_ms(&self) -> u32 {
        let defense = self
            .defenses
            .iter()
            .filter(|defense| is_interrupt_cover_kind(&defense.kind))
            .map(|defense| defense.expires_at_ms.saturating_sub(self.now_ms))
            .max()
            .unwrap_or(0);
        defense.max(self.enemy_disabled_until_ms.saturating_sub(self.now_ms))
    }

    fn has_defense(&self, kind: CoverKind) -> bool {
        self.defenses
            .iter()
            .any(|defense| defense.kind == kind && defense.expires_at_ms > self.now_ms)
    }

    fn consume_defense(&mut self, kind: CoverKind) -> bool {
        let Some(idx) = self
            .defenses
            .iter()
            .position(|defense| defense.kind == kind && defense.expires_at_ms > self.now_ms)
        else {
            return false;
        };
        if self.defenses[idx].stacks > 1 {
            self.defenses[idx].stacks -= 1;
        } else {
            self.defenses.remove(idx);
        }
        true
    }

    fn remove_defense(&mut self, kind: CoverKind) {
        self.defenses.retain(|defense| defense.kind != kind);
    }

    /// The game tracks recharge by skill, not by rendered bar position. The
    /// same skill equipped in both weapon sets therefore shares one timer.
    fn set_skill_cooldown(&mut self, skill_id: u32, cooldown_ms: u32) {
        let ready_ms = self.at(cooldown_ms);
        for (idx, skill) in self.skills.iter().enumerate() {
            if skill.skill_id == skill_id {
                self.cooldown_ready_ms[idx] = ready_ms;
            }
        }
    }

    fn consume_stability(&mut self) -> bool {
        self.consume_defense(CoverKind::Stability)
    }

    fn has_buff(&self, name: &str) -> bool {
        self.buffs
            .iter()
            .any(|buff| buff.name.eq_ignore_ascii_case(name) && buff.expires_at_ms > self.now_ms)
    }

    fn buff_stacks(&self, name: &str) -> u32 {
        let total: u32 = self
            .buffs
            .iter()
            .filter(|buff| buff.name.eq_ignore_ascii_case(name) && buff.expires_at_ms > self.now_ms)
            .map(|buff| buff.stacks)
            .sum();
        // Stack caps come from data/formulas/boons.json (wiki: Might and
        // Stability 25, duration-stacking boons 1) rather than an inline 25.
        match crate::data::boons().get(name) {
            Some(def) => total.min(def.max_stacks),
            None => total,
        }
    }

    fn cleanse(&mut self, count: u32) {
        let removed = count.min(self.incoming_conditions.len() as u32);
        for _ in 0..removed {
            self.incoming_conditions.pop();
        }
        self.conditions_cleansed += removed;
    }

    fn heal(&mut self, amount: f64) {
        let before = self.player_health;
        self.player_health = (self.player_health + amount).min(self.params.max_health);
        self.healing += self.player_health - before;
    }

    fn record_damage(&mut self, amount: f64, protected: bool) {
        if amount <= 0.0 || self.enemy_health <= 0.0 {
            return;
        }
        let applied = amount.min(self.enemy_health);
        self.enemy_health -= applied;
        if self.enemy_health <= 0.0 && self.target_reached_at_ms.is_none() {
            self.target_reached_at_ms = Some(self.now_ms);
        }
        self.damage_events.push(DamageEvent {
            at_ms: self.now_ms,
            amount: applied,
            protected,
        });
    }

    fn skill_damage_value(&self, skill: &RotationSkill) -> f64 {
        let mut value = 0.0;
        for effect in &skill.effects {
            match effect {
                SkillEffect::StrikeDamage {
                    hit_count,
                    dmg_multiplier,
                } => {
                    value += self.params.weapon_strength * self.params.power / reference_armor()
                        * *dmg_multiplier
                        * *hit_count as f64;
                }
                SkillEffect::ApplyCondition {
                    condition,
                    stacks,
                    duration_ms,
                } => {
                    value += condition_tick_damage(
                        condition,
                        self.params.condition_damage,
                        &self.params.mode,
                    ) * *stacks as f64
                        * (*duration_ms as f64 / 1_000.0);
                }
                _ => {}
            }
        }
        value / (skill.cast_time_ms.max(100) as f64 / 1_000.0)
    }

    fn can_pay_resource(&self, skill_id: u32) -> bool {
        let Some(rule) = self.resource_rules.get(&skill_id) else {
            return true;
        };
        self.resources.get(&rule.kind).copied().unwrap_or(0.0) >= rule.cost
    }

    fn pay_resource(&mut self, skill_id: u32) {
        let Some(rule) = self.resource_rules.get(&skill_id) else {
            return;
        };
        let resource = self.resources.entry(rule.kind).or_default();
        if *resource >= rule.cost {
            if rule.spend_all {
                *resource = 0.0;
            } else {
                *resource -= rule.cost;
            }
        }
    }

    fn gain_resource_on_hit(&mut self, skill_id: u32) {
        if let Some(rule) = self.resource_rules.get(&skill_id) {
            if rule.gain_on_hit > 0.0 {
                let resource = self.resources.entry(rule.kind).or_default();
                *resource = (*resource + rule.gain_on_hit)
                    .min(resource_cap(rule.kind, self.params.max_health));
            }
        }
        if self.resources.contains_key(&ResourceKind::Adrenaline) {
            let adrenaline = self.resources.entry(ResourceKind::Adrenaline).or_default();
            *adrenaline = (*adrenaline + 5.0).min(30.0);
        }
    }

    fn regenerate_resources(&mut self) {
        let seconds = TIMELINE_TICK_MS as f64 / 1_000.0;
        if let Some(initiative) = self.resources.get_mut(&ResourceKind::Initiative) {
            *initiative = (*initiative + seconds).min(12.0);
        }
        if let Some(energy) = self.resources.get_mut(&ResourceKind::Energy) {
            *energy = (*energy + 5.0 * seconds).min(100.0);
        }
    }

    fn report(&self) -> WvwCombatReport {
        let protected_damage: f64 = self
            .damage_events
            .iter()
            .filter(|event| event.protected)
            .map(|event| event.amount)
            .sum();
        let peak_2s = peak_damage(&self.damage_events, MIN_PROTECTED_WINDOW_MS, true);
        let peak_5s = peak_damage(&self.damage_events, self.profile.desired_window_ms, true);
        let total_damage: f64 = self.damage_events.iter().map(|event| event.amount).sum();
        let remaining_health_ratio = self.player_health / self.params.max_health.max(1.0);
        let sustain_margin = (self.healing + self.barrier_absorbed + self.avoided_damage
            - self.incoming_damage)
            / (self.profile.duration_ms as f64 / 1_000.0).max(1.0);
        let target_reached = self.profile.target_health.is_some() && self.enemy_health <= 0.0;
        let sequence = secured_sequence_summary(
            &self.secured_tick_times,
            &self.protected_actions,
            &self.damage_events,
            self.profile.desired_window_ms,
            self.profile.required_window_ms,
        );
        let chain_completed = sequence.completed;
        // Repeatable = the player leaves the exchange able to fight again:
        // alive, resources back, and either the target went down, the fight
        // was net-positive, or half the bar is left. Skill cooldowns are NOT
        // part of it: the timeline already refuses to recast a skill that is
        // on cooldown inside the fight, and demanding every skill of the best
        // secured window be ready again 5s after the window closed failed
        // every heal (20-30s) and every elite (60-180s) in the game. In-game
        // 2026-09-05 a Roam/Support Scourge at 87% health after the fight was
        // non-viable on that rule alone and 32k search evaluations found
        // nothing viable, because nothing could be.
        let resource_recovery = self.sequence_resources_recovered(&sequence.skill_ids);
        let repeatable = self.player_health > 0.0
            && chain_completed
            && resource_recovery
            && (target_reached || sustain_margin >= 0.0 || remaining_health_ratio >= 0.50);

        WvwCombatReport {
            duration_ms: self.profile.duration_ms,
            target_health: self.profile.target_health,
            target_reached_at_ms: self.target_reached_at_ms,
            longest_protected_window_ms: self.longest_protected_window_ms,
            protected_action_count: self.protected_action_count,
            successful_action_count: self.successful_action_count,
            interrupted_casts: self.interrupted_casts,
            protected_damage,
            peak_protected_damage_2s: peak_2s,
            peak_protected_damage_5s: peak_5s,
            total_damage,
            control_landed_ms: self.control_landed_ms,
            incoming_damage: self.incoming_damage,
            avoided_damage: self.avoided_damage,
            healing: self.healing,
            barrier_absorbed: self.barrier_absorbed,
            conditions_cleansed: self.conditions_cleansed,
            combo_activations: self.combo_activations,
            remaining_health_ratio,
            sustain_margin,
            player_survived: self.player_health > 0.0,
            target_reached,
            chain_completed,
            secured_sequence_damage: sequence.damage,
            secured_sequence_control_ms: sequence.control_ms,
            repeatable,
            resource_blocked_actions: self.resource_blocked_skills.len() as u32,
            resource_legal: self.resource_blocked_skills.is_empty(),
            resource_model_complete: self.resource_model_complete,
            unmodeled_sources: self
                .unmodeled_names
                .iter()
                .chain(&self.no_record_names)
                .cloned()
                .collect(),
            trace: self.trace.clone(),
            trace_truncated: self.trace_truncated,
            proc_trials: Vec::new(),
            shroud_refusals: self.shroud_refusals.clone(),
        }
    }

    fn sequence_resources_recovered(&self, skill_ids: &HashSet<u32>) -> bool {
        let mut required: HashMap<ResourceKind, f64> = HashMap::new();
        for skill_id in skill_ids {
            let Some(rule) = self.resource_rules.get(skill_id) else {
                continue;
            };
            let entry = required.entry(rule.kind).or_default();
            if rule.spend_all {
                *entry = (*entry).max(rule.cost);
            } else {
                *entry += rule.cost;
            }
        }
        required
            .into_iter()
            .all(|(kind, cost)| self.resources.get(&kind).copied().unwrap_or(0.0) >= cost)
    }
}

fn secured_sequence_summary(
    secured_tick_times: &[u32],
    protected_actions: &[ProtectedActionEvent],
    damage_events: &[DamageEvent],
    max_span_ms: u32,
    required_secured_ms: u32,
) -> SecuredSequenceSummary {
    let mut summary = SecuredSequenceSummary::default();
    for (left, start_ms) in secured_tick_times.iter().copied().enumerate() {
        let end_ms = start_ms + max_span_ms;
        let secured_ticks = secured_tick_times[left..]
            .iter()
            .take_while(|at_ms| **at_ms <= end_ms)
            .count() as u32;
        if secured_ticks * TIMELINE_TICK_MS < required_secured_ms {
            continue;
        }
        let actions: Vec<&ProtectedActionEvent> = protected_actions
            .iter()
            .filter(|action| action.at_ms >= start_ms && action.at_ms <= end_ms)
            .collect();
        if actions.len() < 2 {
            continue;
        }
        let damage: f64 = damage_events
            .iter()
            .filter(|event| event.protected && event.at_ms >= start_ms && event.at_ms <= end_ms)
            .map(|event| event.amount)
            .sum();
        let control_ms = actions.iter().map(|action| action.control_ms).sum();
        let applies_condition = actions.iter().any(|action| action.applies_condition);
        let supports_allies = actions.iter().any(|action| action.supports_allies);
        // A window in which nothing happened is not a sequence. A window in
        // which the player healed and cleansed under pressure is — and it
        // used to be discarded, because the test asked only for damage,
        // control or a condition. That made every support build unable to
        // complete a chain and therefore unable to pass ProtectedExecution,
        // whatever else it did: a WvW healer failed the gate by doing its job.
        if damage <= 0.0 && control_ms == 0 && !applies_condition && !supports_allies {
            continue;
        }
        if !summary.completed
            || damage > summary.damage
            || (damage == summary.damage && control_ms > summary.control_ms)
        {
            summary.completed = true;
            summary.damage = damage;
            summary.control_ms = control_ms;
            summary.skill_ids = actions.iter().map(|action| action.skill_id).collect();
        }
    }
    summary
}

fn peak_damage(events: &[DamageEvent], window_ms: u32, protected_only: bool) -> f64 {
    let mut best: f64 = 0.0;
    let mut left = 0usize;
    let mut total = 0.0;
    for right in 0..events.len() {
        if !protected_only || events[right].protected {
            total += events[right].amount;
        }
        while events[right].at_ms.saturating_sub(events[left].at_ms) > window_ms {
            if !protected_only || events[left].protected {
                total -= events[left].amount;
            }
            left += 1;
        }
        best = best.max(total);
    }
    best
}

fn is_control_cover(effect: &SkillEffect) -> bool {
    matches!(
        effect,
        SkillEffect::Cover {
            kind: CoverKind::Stability
                | CoverKind::Invulnerability
                | CoverKind::Evade
                | CoverKind::Stealth
                | CoverKind::Aegis
                | CoverKind::Blind
                | CoverKind::Block,
            ..
        } | SkillEffect::Mobility {
            kind: MobilityKind::Evade | MobilityKind::Stealth
        }
    )
}

fn is_interrupt_cover_kind(kind: &CoverKind) -> bool {
    matches!(
        kind,
        CoverKind::Stability
            | CoverKind::Invulnerability
            | CoverKind::Evade
            | CoverKind::Stealth
            | CoverKind::Block
    )
}

fn boon_cover_kind(name: &str) -> Option<CoverKind> {
    if name.eq_ignore_ascii_case("Stability") {
        Some(CoverKind::Stability)
    } else if name.eq_ignore_ascii_case("Aegis") {
        Some(CoverKind::Aegis)
    } else if name.eq_ignore_ascii_case("Protection") {
        Some(CoverKind::Protection)
    } else if name.eq_ignore_ascii_case("Resistance") {
        Some(CoverKind::Resistance)
    } else {
        None
    }
}

fn resolved<T>(value: &FactualValue<T>) -> Option<&T> {
    match value {
        FactualValue::Resolved(value) => Some(value),
        FactualValue::Unknown => None,
    }
}

fn as_ratio(value: f64) -> f64 {
    if value.abs() > 2.0 {
        value / 100.0
    } else {
        value
    }
}

fn leftover_condition_fraction(condition: &TimedCondition) -> f64 {
    let period_start = condition.next_tick_ms.saturating_sub(1_000);
    if condition.expires_at_ms <= period_start {
        return 0.0;
    }
    let remaining_ms = condition.expires_at_ms - period_start;
    if remaining_ms >= 1_000 {
        return 0.0;
    }
    remaining_ms as f64 / 1_000.0
}

/// Human label for the coverage line: `Superior Sigil of Fire (on-crit)`.
fn trigger_label(trigger: &TriggerRule) -> &'static str {
    match trigger {
        TriggerRule::Passive => "passive",
        TriggerRule::OnCrit => "on-crit",
        TriggerRule::OnHit => "on-hit",
        TriggerRule::OnSkillUse => "on-skill-use",
        TriggerRule::OnHealthThreshold => "on-health-threshold",
        TriggerRule::Conditional => "conditional",
    }
}

fn same_trigger(left: &TriggerRule, right: &TriggerRule) -> bool {
    matches!(
        (left, right),
        (TriggerRule::Passive, TriggerRule::Passive)
            | (TriggerRule::OnCrit, TriggerRule::OnCrit)
            | (TriggerRule::OnHit, TriggerRule::OnHit)
            | (TriggerRule::OnSkillUse, TriggerRule::OnSkillUse)
            | (
                TriggerRule::OnHealthThreshold,
                TriggerRule::OnHealthThreshold
            )
            | (TriggerRule::Conditional, TriggerRule::Conditional)
    )
}

fn initial_resources(rules: &[SkillResourceRule]) -> HashMap<ResourceKind, f64> {
    let mut resources = HashMap::new();
    for rule in rules {
        resources
            .entry(rule.kind)
            .or_insert_with(|| match rule.kind {
                ResourceKind::Initiative => 12.0,
                ResourceKind::Energy => 50.0,
                ResourceKind::Adrenaline => 10.0,
                ResourceKind::Illusions | ResourceKind::Blades | ResourceKind::LifeForce => 0.0,
            });
    }
    resources
}

fn resource_cap(kind: ResourceKind, max_health: f64) -> f64 {
    match kind {
        ResourceKind::Initiative => 12.0,
        ResourceKind::Energy => 100.0,
        ResourceKind::Adrenaline => 30.0,
        ResourceKind::Illusions => 3.0,
        ResourceKind::Blades => 5.0,
        ResourceKind::LifeForce => crate::data::shroud::table().pool_for(max_health),
    }
}

fn condition_is_damaging(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "bleeding" | "burning" | "confusion" | "poison" | "torment"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::CombatPerformance;
    use crate::referee::evaluate_viability_gates;
    use crate::rotation::{ControlKind, SkillSlot};
    use crate::scenario::{OptimizationTarget, TargetProfile};
    use gw2_core::types::GameMode;

    fn skill(
        skill_id: u32,
        slot: SkillSlot,
        cast_time_ms: u32,
        cooldown_ms: u32,
        effects: Vec<SkillEffect>,
    ) -> RotationSkill {
        RotationSkill {
            skill_id,
            name: format!("test-{skill_id}"),
            slot,
            cast_time_ms,
            cooldown_ms,
            effects,
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    fn params() -> SimParams {
        let mut params = SimParams::basic(2_000.0, 1_500.0, 1_100.0);
        params.max_health = 20_000.0;
        params.armor = 2_500.0;
        params.mode = GameMode::WvW;
        params
    }

    fn profile(duration_ms: u32, events: Vec<EnemyEvent>) -> WvwProfile {
        WvwProfile {
            duration_ms,
            target_health: Some(18_000.0),
            enemy_events: events.into(),
            required_window_ms: MIN_PROTECTED_WINDOW_MS,
            desired_window_ms: TARGET_PROTECTED_WINDOW_MS.min(duration_ms),
        }
    }

    fn run_report(
        skills: &[RotationSkill],
        rules: &[SkillResourceRule],
        enemy: EnemyDummy,
        profile: WvwProfile,
        params: &SimParams,
    ) -> WvwCombatReport {
        let mut timeline =
            Timeline::new(skills, params, profile, enemy, &[], rules, true, Vec::new());
        timeline.run();
        timeline.report()
    }

    fn open_enemy(stability: bool) -> EnemyDummy {
        EnemyDummy {
            protection: false,
            stability,
            hp: Some(18_000.0),
        }
    }

    fn rule(
        skill_id: u32,
        kind: ResourceKind,
        cost: f64,
        gain_on_hit: f64,
        spend_all: bool,
    ) -> SkillResourceRule {
        SkillResourceRule {
            skill_id,
            kind,
            cost,
            gain_on_hit,
            spend_all,
            ..Default::default()
        }
    }

    #[test]
    fn short_block_does_not_complete_minimum_window() {
        let skills = vec![
            skill(
                1,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Block,
                    duration_ms: 1_000,
                    strippable: false,
                }],
            ),
            skill(
                2,
                SkillSlot::Weapon2,
                200,
                250,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 4.0,
                }],
            ),
        ];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(3_000, vec![]),
            &params,
        );

        assert!(report.longest_protected_window_ms < MIN_PROTECTED_WINDOW_MS);
        assert!(!report.chain_completed);
    }

    #[test]
    fn charge_cover_only_preserves_the_consumed_event_tick() {
        let skills = vec![skill(
            1,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::Cover {
                kind: CoverKind::Aegis,
                duration_ms: 5_000,
                strippable: true,
            }],
        )];
        let events = vec![EnemyEvent {
            at_ms: 450,
            kind: EnemyEventKind::Strike {
                damage: 2_000.0,
                unblockable: false,
            },
        }];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(1_000, events),
            &params,
        );

        assert_eq!(report.longest_protected_window_ms, TIMELINE_TICK_MS);
        assert!(!report.chain_completed);
        assert_eq!(report.incoming_damage, 0.0);
    }

    #[test]
    fn target_stability_requires_strip_before_control() {
        let control = skill(
            2,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::CrowdControl {
                kind: ControlKind::Stun,
                duration_ms: 1_000,
                stops_dodge: true,
            }],
        );
        let params = params();
        let blocked = run_report(
            std::slice::from_ref(&control),
            &[],
            open_enemy(true),
            profile(1_000, vec![]),
            &params,
        );
        assert_eq!(blocked.control_landed_ms, 0);

        let strip = skill(
            1,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::StripBoons {
                count_per_pulse: 1,
                interval_ms: 0,
                window_ms: 0,
            }],
        );
        let opened = run_report(
            &[strip, control],
            &[],
            open_enemy(true),
            profile(1_000, vec![]),
            &params,
        );
        assert_eq!(opened.control_landed_ms, 1_000);
    }

    #[test]
    fn incoming_control_cancels_a_pending_cast() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 8.0,
            }],
        )];
        let events = vec![EnemyEvent {
            at_ms: 450,
            kind: EnemyEventKind::Control {
                duration_ms: 900,
                unblockable: false,
            },
        }];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(2_000, events),
            &params,
        );

        assert_eq!(report.interrupted_casts, 1);
        assert_eq!(report.total_damage, 0.0);
    }

    /// Chilled has to cost the player skill uptime, or it is just a word.
    ///
    /// It deals no damage at all, so nothing in the damage model can see it.
    /// What it does is keep the heal on cooldown until the next burst lands,
    /// which is the actual way a support dies, and the reason cleansing is
    /// worth a utility slot rather than a damage tax.
    #[test]
    fn chilled_keeps_a_skill_on_cooldown_longer() {
        // One skill, short cooldown, nothing else to do but recast it.
        let bar = vec![skill(1, SkillSlot::Weapon1, 200, 2_000, vec![])];
        let params = params();

        let clear = run_report(
            &bar,
            &[],
            open_enemy(false),
            profile(12_000, vec![]),
            &params,
        );
        let chilled = run_report(
            &bar,
            &[],
            open_enemy(false),
            profile(
                12_000,
                vec![EnemyEvent {
                    at_ms: 100,
                    kind: EnemyEventKind::Condition {
                        condition: "Chilled".into(),
                        stacks: 1,
                        duration_ms: 10_000,
                    },
                }],
            ),
            &params,
        );

        assert!(
            chilled.successful_action_count < clear.successful_action_count,
            "Chilled must cost casts over the same window - clear {}, chilled {}",
            clear.successful_action_count,
            chilled.successful_action_count
        );
    }

    /// Conditions have to outlast the lull, or cleansing is decoration.
    ///
    /// The applied duration used to be 4,000 ms against a burst period that is
    /// now 10,000 ms, so every condition expired on its own before the next
    /// burst. Waiting was a complete answer and a build with no cleanse
    /// measured exactly the same as one built around cleansing, which is the
    /// opposite of how condition damage works: armour does not reduce it,
    /// Protection does not reduce it, and an evade cannot dodge what is
    /// already ticking. Removal is the only counter.
    #[test]
    fn an_uncleansed_condition_costs_health_a_cleanse_saves() {
        let events = vec![EnemyEvent {
            at_ms: 200,
            kind: EnemyEventKind::Condition {
                condition: "Bleeding".into(),
                stacks: 8,
                duration_ms: CONDITION_DURATION_MS,
            },
        }];
        let params = params();

        let exposed = run_report(
            &[],
            &[],
            open_enemy(false),
            profile(CONDITION_DURATION_MS, events.clone()),
            &params,
        );
        assert!(
            exposed.incoming_damage > 0.0,
            "a condition nobody removes has to actually tick: {exposed:?}"
        );

        let cleanse = vec![skill(
            1,
            SkillSlot::Utility,
            50,
            600,
            vec![SkillEffect::RemovesCondition {
                conditions_removed: 3,
            }],
        )];
        let cleansed = run_report(
            &cleanse,
            &[],
            open_enemy(false),
            profile(CONDITION_DURATION_MS, events),
            &params,
        );

        assert!(
            cleansed.incoming_damage < exposed.incoming_damage,
            "cleansing has to cost the enemy damage - uncleansed {:.0}, cleansed {:.0}",
            exposed.incoming_damage,
            cleansed.incoming_damage
        );
        assert!(
            cleansed.remaining_health_ratio > exposed.remaining_health_ratio,
            "and has to show up as health left on the bar - uncleansed {:.0}%, cleansed {:.0}%",
            exposed.remaining_health_ratio * 100.0,
            cleansed.remaining_health_ratio * 100.0
        );
    }

    /// Pressure oscillates: ramp, peak, then a lull long enough to recover in.
    ///
    /// A flat script makes raw mitigation per second the only thing the
    /// sustain gates can see, and turns a longer window into nothing but more
    /// total damage - which is how a 20 s support window came to fail
    /// `SustainRecovery` for 86% of the published corpus while a 5 s DPS
    /// window passed 100% of it.
    #[test]
    fn the_generated_burst_peaks_and_then_lets_go() {
        let scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Solo,
            combat_kind: CombatKind::Support,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "burst shape".into(),
            },
            patch_id: None,
            objective_profile_id: None,
        };
        let params = params();
        let profile = WvwProfile::for_scenario(&scenario, &open_enemy(false), &params, 20_000);

        let strikes: Vec<(u32, f64)> = profile
            .enemy_events
            .iter()
            .filter_map(|e| match e.kind {
                EnemyEventKind::Strike { damage, .. } => Some((e.at_ms, damage)),
                _ => None,
            })
            .collect();
        assert!(
            strikes.len() >= 4,
            "a 20 s window has to carry more than one burst: {strikes:?}"
        );

        let biggest = strikes.iter().map(|(_, d)| *d).fold(0.0_f64, f64::max);
        let smallest = strikes.iter().map(|(_, d)| *d).fold(f64::MAX, f64::min);
        assert!(
            biggest >= smallest * 4.0,
            "the peak has to dwarf the chip or an evade spent on it buys nothing:              {smallest:.0} .. {biggest:.0}"
        );

        // The lull. Consecutive event gaps must include one long enough to
        // land a heal and have it matter.
        let mut times: Vec<u32> = profile.enemy_events.iter().map(|e| e.at_ms).collect();
        times.sort_unstable();
        let longest_gap = times.windows(2).map(|w| w[1] - w[0]).max().unwrap_or(0);
        assert!(
            longest_gap >= MIN_PROTECTED_WINDOW_MS,
            "no recovery window in the script: longest gap {longest_gap}ms"
        );
    }

    #[test]
    fn recovery_kit_outlasts_the_same_pressure_script() {
        let events = vec![
            EnemyEvent {
                at_ms: 400,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
            EnemyEvent {
                at_ms: 900,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
            EnemyEvent {
                at_ms: 1_400,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
        ];
        let params = params();
        let exposed = run_report(
            &[],
            &[],
            open_enemy(false),
            profile(2_000, events.clone()),
            &params,
        );
        assert!(!exposed.player_survived);

        let recovery = vec![
            skill(
                1,
                SkillSlot::Utility,
                50,
                600,
                vec![SkillEffect::Barrier { amount: 8_000.0 }],
            ),
            skill(
                2,
                SkillSlot::Heal,
                50,
                600,
                vec![
                    SkillEffect::Healing { hit_count: 2 },
                    SkillEffect::RemovesCondition {
                        conditions_removed: 2,
                    },
                ],
            ),
        ];
        let recovered = run_report(
            &recovery,
            &[],
            open_enemy(false),
            profile(2_000, events),
            &params,
        );
        assert!(recovered.player_survived);
        assert!(recovered.sustain_margin > 0.0);
    }

    fn published_fixture_profile() -> WvwProfile {
        profile(
            5_000,
            vec![
                EnemyEvent {
                    at_ms: 450,
                    kind: EnemyEventKind::Control {
                        duration_ms: 900,
                        unblockable: false,
                    },
                },
                EnemyEvent {
                    at_ms: 850,
                    kind: EnemyEventKind::Strike {
                        damage: 3_000.0,
                        unblockable: false,
                    },
                },
                EnemyEvent {
                    at_ms: 1_650,
                    kind: EnemyEventKind::Condition {
                        condition: "Bleeding".into(),
                        stacks: 2,
                        duration_ms: 3_000,
                    },
                },
                EnemyEvent {
                    at_ms: 2_350,
                    kind: EnemyEventKind::BoonStrip { count: 1 },
                },
                EnemyEvent {
                    at_ms: 3_050,
                    kind: EnemyEventKind::Control {
                        duration_ms: 1_100,
                        unblockable: false,
                    },
                },
            ],
        )
    }

    fn paper_pressure_sibling(skills: &[RotationSkill]) -> Vec<RotationSkill> {
        skills
            .iter()
            .cloned()
            .filter_map(|mut skill| {
                skill.effects.retain(|effect| {
                    !matches!(
                        effect,
                        SkillEffect::Cover { .. }
                            | SkillEffect::Mobility { .. }
                            | SkillEffect::CrowdControl { .. }
                    )
                });
                (!skill.effects.is_empty()).then_some(skill)
            })
            .collect()
    }

    fn assert_published_fixture(
        label: &str,
        skills: Vec<RotationSkill>,
        rules: Vec<SkillResourceRule>,
        target_starts_stable: bool,
    ) {
        let params = params();
        let report = run_report(
            &skills,
            &rules,
            open_enemy(target_starts_stable),
            published_fixture_profile(),
            &params,
        );
        let paper = paper_pressure_sibling(&skills);
        let paper_report = run_report(
            &paper,
            &rules,
            open_enemy(target_starts_stable),
            published_fixture_profile(),
            &params,
        );

        assert!(
            report.chain_completed,
            "{label} should complete its secured sequence: {report:?}"
        );
        assert!(
            report.resource_legal,
            "{label} should obey its resource ledger"
        );
        assert!(
            report.protected_damage > paper_report.protected_damage,
            "{label} should outperform its unprotected pressure sibling"
        );

        let rotation = super::super::SimulationResult {
            duration_ms: report.duration_ms,
            strike_dps: report.total_damage / 5.0,
            condition_dps: 0.0,
            total_dps: report.total_damage / 5.0,
            condition_uptime: HashMap::new(),
            buff_uptime: HashMap::new(),
            skill_usage: Vec::new(),
            stunbreak_count: 1,
            has_stability: false,
            stability_uptime: 0.0,
            cleanse_count: 1,
            cleanse_rate_per_20s: 4.0,
            healing_per_second: 0.0,
            control_uptime: 0.0,
            might_stacks_avg: 0.0,
            boon_equivalents: 0.0,
            has_mobility_out: true,
            escape_kinds: 1,
            has_strip: true,
            has_corrupt: false,
            downed: report.target_reached,
            finished: report.target_reached,
            has_interrupt: true,
            has_cover_answer: true,
            wvw: Some(report),
        };
        let combat = CombatPerformance {
            effective_health: 20_000.0,
            ..CombatPerformance::default()
        };
        let scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Solo,
            combat_kind: CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: label.into(),
            },
            patch_id: None,
            objective_profile_id: None,
        };
        let viability = evaluate_viability_gates(Some(&rotation), &combat, &scenario);
        assert!(
            viability.is_viable,
            "{label} should pass the WvW gates: {:?}",
            viability.gates
        );
    }

    #[test]
    fn mobility_label_without_timed_cover_does_not_complete_sequence() {
        // A mobility tag says what a skill can do, not how long it protects an
        // action. The sourced D/P fixture belongs after its explicit WvW facts
        // and instant-during-cast ordering are represented.
        let skills = vec![
            skill(
                100,
                SkillSlot::Profession,
                50,
                3_000,
                vec![
                    SkillEffect::StealBoons,
                    SkillEffect::CrowdControl {
                        kind: ControlKind::Daze,
                        duration_ms: 750,
                        stops_dodge: false,
                    },
                ],
            ),
            skill(
                101,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Mobility {
                    kind: MobilityKind::Stealth,
                }],
            ),
            skill(
                102,
                SkillSlot::Weapon2,
                200,
                1_200,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
        ];
        let report = run_report(
            &skills,
            &[rule(102, ResourceKind::Initiative, 3.0, 0.0, false)],
            open_enemy(false),
            published_fixture_profile(),
            &params(),
        );
        assert!(!report.chain_completed);
    }

    #[test]
    fn published_spellbreaker_fixture_beats_unprotected_pressure() {
        // MetaBattle Magebane/Spearbreaker roamers: remove target cover, then
        // combine Full Counter's block/control with weapon pressure.
        let skills = vec![
            skill(
                200,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::StripBoons {
                    count_per_pulse: 2,
                    interval_ms: 0,
                    window_ms: 0,
                }],
            ),
            skill(
                201,
                SkillSlot::Profession,
                50,
                5_000,
                vec![
                    SkillEffect::Cover {
                        kind: CoverKind::Block,
                        duration_ms: 2_500,
                        strippable: false,
                    },
                    SkillEffect::CrowdControl {
                        kind: ControlKind::Stun,
                        duration_ms: 1_000,
                        stops_dodge: true,
                    },
                    SkillEffect::StrikeDamage {
                        hit_count: 1,
                        dmg_multiplier: 8.0,
                    },
                ],
            ),
            skill(
                202,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
        ];
        assert_published_fixture(
            "Spellbreaker",
            skills,
            vec![rule(201, ResourceKind::Adrenaline, 10.0, 0.0, false)],
            true,
        );
    }

    #[test]
    fn long_cooldown_in_the_secured_sequence_is_still_repeatable() {
        // Seen in-game 2026-09-05 on 1.11.25: a Roam/Support Scourge left the
        // exchange alive at 87% health and was still judged not repeatable,
        // because its heal (25s) and elite (120s) sat inside the best secured
        // window and were not off cooldown 5s after a 20s fight. Every heal and
        // elite in the game trips that rule. Repeatable is about the player
        // coming out of the exchange able to fight again, not about the burst
        // being castable again 5s later.
        let skills = vec![
            skill(
                201,
                SkillSlot::Elite,
                50,
                120_000,
                vec![
                    SkillEffect::Cover {
                        kind: CoverKind::Block,
                        duration_ms: 2_500,
                        strippable: false,
                    },
                    SkillEffect::CrowdControl {
                        kind: ControlKind::Stun,
                        duration_ms: 1_000,
                        stops_dodge: true,
                    },
                    SkillEffect::StrikeDamage {
                        hit_count: 1,
                        dmg_multiplier: 8.0,
                    },
                ],
            ),
            skill(
                202,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
        ];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            published_fixture_profile(),
            &params,
        );
        assert!(report.chain_completed, "{report:?}");
        assert!(report.player_survived, "{report:?}");
        assert!(report.remaining_health_ratio >= 0.5, "{report:?}");
        assert!(
            report.repeatable,
            "alive at {:.0}% after a completed sequence must be repeatable: {report:?}",
            report.remaining_health_ratio * 100.0
        );
    }

    #[test]
    fn published_mirage_fixture_beats_unprotected_pressure() {
        // MetaBattle Shatter/Celestial Mirage roamers: generate an illusion,
        // use Distortion as duration cover, then apply the weapon sequence.
        let skills = vec![
            skill(
                300,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
            skill(
                301,
                SkillSlot::Profession,
                50,
                5_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Invulnerability,
                    duration_ms: 2_500,
                    strippable: false,
                }],
            ),
        ];
        assert_published_fixture(
            "Mirage",
            skills,
            vec![
                rule(300, ResourceKind::Illusions, 0.0, 1.0, false),
                rule(301, ResourceKind::Illusions, 1.0, 0.0, true),
            ],
            false,
        );
    }

    #[test]
    fn published_virtuoso_fixture_beats_unprotected_pressure() {
        // MetaBattle Power Speartuoso: invulnerability provides the opening;
        // a blade builder makes the bladesong legal before the pressure lands.
        let skills = vec![
            skill(
                400,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Invulnerability,
                    duration_ms: 2_500,
                    strippable: false,
                }],
            ),
            skill(
                401,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
            skill(
                402,
                SkillSlot::Profession,
                200,
                5_000,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 10.0,
                }],
            ),
        ];
        assert_published_fixture(
            "Virtuoso",
            skills,
            vec![
                rule(401, ResourceKind::Blades, 0.0, 1.0, false),
                rule(402, ResourceKind::Blades, 1.0, 0.0, true),
            ],
            false,
        );
    }

    #[test]
    fn initiative_stops_after_the_available_pool_is_spent() {
        let rules = [rule(1, ResourceKind::Initiative, 4.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        for _ in 0..3 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
    }

    #[test]
    fn exact_duration_conditions_receive_their_final_tick() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(5_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.outgoing_conditions.push(TimedCondition {
            name: "Bleeding".into(),
            stacks: 1,
            expires_at_ms: 4_000,
            next_tick_ms: 1_000,
        });
        let one_tick = condition_tick_damage("Bleeding", params.condition_damage, &params.mode);
        for second in 1..=4 {
            timeline.now_ms = second * 1_000;
            timeline.tick_conditions();
        }

        let total: f64 = timeline
            .damage_events
            .iter()
            .map(|event| event.amount)
            .sum();
        assert!((total - one_tick * 4.0).abs() < 0.001);
        assert!(timeline.outgoing_conditions.is_empty());
    }

    #[test]
    fn resistance_does_not_remove_damaging_conditions() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.apply_defense(CoverKind::Resistance, 2_000, 1, false);
        timeline.receive_condition("Burning".into(), 1, 1_000);
        timeline.receive_condition("Crippled".into(), 1, 1_000);

        assert_eq!(timeline.incoming_conditions.len(), 1);
        assert_eq!(timeline.incoming_conditions[0].name, "Burning");
    }

    #[test]
    fn reactive_stunbreak_must_pay_its_resource_cost() {
        let mut stunbreak = skill(1, SkillSlot::Utility, 0, 10_000, vec![]);
        stunbreak.is_stunbreak = true;
        let skills = [stunbreak];
        let rules = [rule(1, ResourceKind::Energy, 60.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        timeline.disabled_until_ms = 1_000;
        timeline.try_stunbreak();

        assert_eq!(timeline.disabled_until_ms, 1_000);
        assert_eq!(timeline.resources[&ResourceKind::Energy], 50.0);
        assert!(timeline.resource_blocked_skills.contains(&1));
    }

    #[test]
    fn skill_owned_proc_only_runs_for_its_source_skill() {
        let data = crate::data::normalized_effects::effects();
        let effect = data
            .effects_for_mode("WvW")
            .iter()
            .find(|effect| effect.source_id == 9120)
            .expect("Virtue of Resolve normalized effect");
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[effect],
            &[],
            true,
            Vec::new(),
        );
        timeline.incoming_conditions.push(TimedCondition {
            name: "Burning".into(),
            stacks: 1,
            expires_at_ms: 2_000,
            next_tick_ms: 1_000,
        });

        timeline.trigger_procs(TriggerRule::OnSkillUse, Some(1), false, 1.0);
        assert_eq!(timeline.incoming_conditions.len(), 1);
        timeline.trigger_procs(TriggerRule::OnSkillUse, Some(9120), false, 1.0);
        assert!(timeline.incoming_conditions.is_empty());
    }

    #[test]
    fn unsupported_normalized_trigger_degrades_coverage() {
        // A `Conditional` record without a health threshold has no firing
        // site (on-crit gained one in Sprint 2): it is named, never fired.
        let data = crate::data::normalized_effects::effects();
        let mut effect = data
            .effects_for_mode("WvW")
            .iter()
            .find(|effect| matches!(effect.trigger_rule, TriggerRule::OnCrit))
            .expect("OnCrit normalized effect")
            .clone();
        effect.trigger_rule = TriggerRule::Conditional;
        effect.health_threshold = None;
        let params = params();
        let timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[&effect],
            &[],
            true,
            Vec::new(),
        );

        assert_eq!(timeline.unmodeled_names.len(), 1);
        assert!(timeline.proc_specs.is_empty());
    }

    fn test_proc_effect(
        source_id: u32,
        category: EffectCategory,
        duration: Option<f64>,
    ) -> NormalizedEffect {
        use crate::data::normalized_effects::{StackingRule, UptimeModel, UptimeModelKind};
        use crate::data::EvidenceLevel;
        NormalizedEffect {
            effect_id: format!("test-proc-{source_id}"),
            source_type: SourceType::Relic,
            source_id,
            source_name: "test".into(),
            category,
            value: FactualValue::Resolved(10.0),
            stacking_rule: StackingRule::NonStacking,
            trigger_rule: TriggerRule::OnHit,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::Unknown,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Unknown,
            source: None,
            effect_duration: duration.map(FactualValue::Resolved),
            internal_cooldown: None,
            max_stacks: None,
            status_operation: None,
            inner_category: None,
            health_threshold: None,
            proc_chance: None,
            trigger_scope: None,
        }
    }

    #[test]
    fn unsupported_proc_category_counts_once_per_source() {
        let flat = test_proc_effect(11, EffectCategory::FlatStat, None);
        let heal = test_proc_effect(22, EffectCategory::OutgoingHealingPct, None);
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[&flat, &heal],
            &[],
            true,
            Vec::new(),
        );
        assert_eq!(timeline.unmodeled_names.len(), 0);
        assert_eq!(timeline.proc_specs.len(), 2);

        for _ in 0..5 {
            timeline.trigger_procs(TriggerRule::OnHit, None, false, 1.0);
        }

        assert_eq!(timeline.unmodeled_names.len(), 2);
        assert_eq!(timeline.healing, 0.0);
    }

    #[test]
    fn timeline_at_saturates_near_u32_max() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.now_ms = u32::MAX - 10;
        assert_eq!(timeline.at(20), u32::MAX);
        assert_eq!(timeline.at(5), u32::MAX - 5);
    }

    #[test]
    fn protection_uses_formula_multiplier() {
        let params = params();
        let mut incoming = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        incoming.apply_skill_effect(
            1,
            &SkillEffect::Cover {
                kind: CoverKind::Protection,
                duration_ms: 5_000,
                strippable: true,
            },
            false,
        );
        incoming.receive_strike(1_000.0, true);
        let expected = 1_000.0 * incoming.protection_multiplier;
        assert!((incoming.incoming_damage - expected).abs() < 1e-9);

        let mut open = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        let mut prot = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            EnemyDummy {
                protection: true,
                stability: false,
                hp: Some(18_000.0),
            },
            &[],
            &[],
            true,
            Vec::new(),
        );
        let strike = SkillEffect::StrikeDamage {
            hit_count: 1,
            dmg_multiplier: 1.0,
        };
        open.apply_skill_effect(1, &strike, false);
        prot.apply_skill_effect(1, &strike, false);
        let ratio = prot.damage_events[0].amount / open.damage_events[0].amount;
        assert!((ratio - prot.protection_multiplier).abs() < 1e-9);
    }

    #[test]
    fn energy_respects_its_cap_and_cost() {
        let rules = [rule(1, ResourceKind::Energy, 25.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        for _ in 0..2 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
        timeline.resources.insert(ResourceKind::Energy, 99.9);
        timeline.regenerate_resources();
        assert_eq!(timeline.resources[&ResourceKind::Energy], 100.0);
    }

    #[test]
    fn adrenaline_action_waits_for_landed_hits() {
        let rules = [rule(1, ResourceKind::Adrenaline, 10.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        timeline.resources.insert(ResourceKind::Adrenaline, 0.0);
        assert!(!timeline.can_pay_resource(1));
        timeline.gain_resource_on_hit(99);
        timeline.gain_resource_on_hit(99);
        assert!(timeline.can_pay_resource(1));
    }

    #[test]
    fn illusion_action_spends_the_current_stack() {
        let rules = [
            rule(1, ResourceKind::Illusions, 0.0, 1.0, false),
            rule(2, ResourceKind::Illusions, 1.0, 0.0, true),
        ];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        assert!(!timeline.can_pay_resource(2));
        timeline.gain_resource_on_hit(1);
        assert!(timeline.can_pay_resource(2));
        timeline.pay_resource(2);
        assert!(!timeline.can_pay_resource(2));
    }

    #[test]
    fn blade_action_spends_the_current_stack() {
        let rules = [
            rule(1, ResourceKind::Blades, 0.0, 1.0, false),
            rule(2, ResourceKind::Blades, 1.0, 0.0, true),
        ];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            Vec::new(),
        );
        assert!(!timeline.can_pay_resource(2));
        timeline.gain_resource_on_hit(1);
        assert!(timeline.can_pay_resource(2));
        timeline.pay_resource(2);
        assert!(!timeline.can_pay_resource(2));
    }

    #[test]
    fn duplicate_skill_ids_share_one_recharge_timer() {
        let mut first = skill(42, SkillSlot::Weapon2, 100, 5_000, vec![]);
        first.weapon_set = 1;
        let mut second = first.clone();
        second.weapon_set = 2;
        let skills = [first, second];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );

        timeline.set_skill_cooldown(42, 5_000);

        assert_eq!(timeline.cooldown_ready_ms, vec![5_000, 5_000]);
    }

    #[test]
    fn profession_swap_policy_controls_the_timeline_timer() {
        let mut first = skill(1, SkillSlot::Weapon2, 100, 1_000, vec![]);
        first.weapon_set = 1;
        let mut second = skill(2, SkillSlot::Weapon2, 100, 1_000, vec![]);
        second.weapon_set = 2;
        let skills = [first, second];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(6_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.weapon_swap_cooldown_ms = Some(5_000);

        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 2);
        assert_eq!(timeline.weapon_swap_ready_ms, 5_000);
        timeline.now_ms = 4_999;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 2);
        timeline.now_ms = 5_000;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 1);

        timeline.weapon_swap_cooldown_ms = None;
        timeline.now_ms = 10_000;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 1);
    }

    #[test]
    fn stealth_breaks_on_landed_strike_but_is_not_full_immunity() {
        let strike = skill(
            1,
            SkillSlot::Weapon2,
            100,
            1_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        );
        let skills = [strike];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.apply_defense(CoverKind::Stealth, 3_000, 1, false);

        timeline.receive_strike(1_000.0, false);
        assert!(timeline.incoming_damage > 0.0);
        assert!(timeline.has_defense(CoverKind::Stealth));

        timeline.apply_skill_effect(1, &skills[0].effects[0], true);
        assert!(!timeline.has_defense(CoverKind::Stealth));
    }

    #[test]
    fn alacrity_shortens_recharge_to_eighty_percent() {
        // Wiki Alacrity (2026-08-29): 10s CD recharges in 8s while Alacrity is up.
        let skills = [skill(
            1,
            SkillSlot::Utility,
            200,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(20_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.apply_buff("Alacrity", 1, 30_000, false);
        timeline.set_skill_cooldown(1, 10_000);
        assert_eq!(
            timeline.cooldown_ready_ms[0], 10_000,
            "store full CD; dummy clock consumes 1.25x per 100ms wall"
        );
        for _ in 0..160 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_recharge_rate();
        }
        assert!(
            timeline.cooldown_ready_ms[0] <= timeline.now_ms,
            "10s CD ready after 8s wall with Alacrity, ready={} now={}",
            timeline.cooldown_ready_ms[0],
            timeline.now_ms
        );
    }

    #[test]
    fn alacrity_mid_cooldown_still_uses_dummy_clock() {
        let skills = [skill(
            1,
            SkillSlot::Utility,
            200,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(20_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.set_skill_cooldown(1, 10_000);
        for _ in 0..40 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_recharge_rate();
        }
        assert_eq!(timeline.cooldown_ready_ms[0], 10_000);
        timeline.apply_buff("Alacrity", 1, 30_000, false);
        for _ in 0..128 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_recharge_rate();
        }
        assert!(
            timeline.cooldown_ready_ms[0] <= timeline.now_ms,
            "Alacrity after 2s must still eat the remaining 8s in 6.4s wall"
        );
    }

    #[test]
    fn confusion_on_skill_use_is_not_the_one_second_pulse() {
        // Wiki Confusion (2026-08-29): DoT each second AND extra on skill activation.
        let skills = [skill(
            1,
            SkillSlot::Weapon2,
            200,
            5_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(4_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.receive_condition("Confusion".into(), 1, 10_000);
        let before = timeline.incoming_damage;
        timeline.start_cast(0);
        let on_use = crate::data::conditions().confusion_tick(1_800.0, GameMode::WvW, true);
        assert!(
            (timeline.incoming_damage - before - on_use).abs() < 0.01,
            "start_cast must apply on-skill-use Confusion, expected {on_use}, got {}",
            timeline.incoming_damage - before
        );
        let after_cast = timeline.incoming_damage;
        timeline.now_ms = 1_000;
        timeline.tick_conditions();
        let pulse = timeline.incoming_damage - after_cast;
        let dot = crate::data::conditions().confusion_tick(1_800.0, GameMode::WvW, false);
        assert!(
            (pulse - dot).abs() < 0.01,
            "1s pulse must be DoT {dot}, not on-skill-use {on_use}; got {pulse}"
        );
    }

    #[test]
    fn barrier_expires_after_five_seconds_and_caps_at_quarter_health() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(10_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        let cap = params.max_health * WVW_BARRIER_HEALTH_FRACTION;
        timeline.apply_barrier(params.max_health);
        let total: f64 = timeline.barrier.iter().map(|layer| layer.amount).sum();
        assert!((total - cap).abs() < 0.001, "cap {cap}, got {total}");
        timeline.apply_barrier(10_000.0);
        let total: f64 = timeline.barrier.iter().map(|layer| layer.amount).sum();
        assert!(total <= cap + 0.001);

        timeline.now_ms = 8_000;
        timeline.expire_timed_state();
        let absorbed_before = timeline.barrier_absorbed;
        let incoming_before = timeline.incoming_damage;
        timeline.absorb_damage(1_000.0);
        assert_eq!(timeline.barrier_absorbed, absorbed_before);
        assert!((timeline.incoming_damage - incoming_before - 1_000.0).abs() < 0.001);
    }

    #[test]
    fn interrupt_sets_four_second_cooldown() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.start_cast(0);
        assert_eq!(
            timeline.cooldown_ready_ms[0], 0,
            "no recharge until the cast resolves"
        );
        timeline.now_ms = 200;
        timeline.receive_control(900, false);
        assert_eq!(timeline.interrupted_casts, 1);
        assert!(timeline.pending.is_none());
        assert_eq!(timeline.cooldown_ready_ms[0], 4_200);
    }

    #[test]
    fn a_channel_lands_its_hits_across_the_cast_and_loses_the_rest_on_interrupt() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 4,
                dmg_multiplier: 2.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.start_cast(0);
        assert_eq!(timeline.scheduled_hits.len(), 4);
        timeline.now_ms = 500;
        timeline.land_scheduled_hits();
        assert_eq!(timeline.damage_events.len(), 2, "hits at 250 and 500 ms");
        timeline.receive_control(900, true);
        assert!(timeline.pending.is_none());
        assert!(
            timeline.scheduled_hits.is_empty(),
            "the last two hits died with the cast"
        );
        assert_eq!(timeline.damage_events.len(), 2);
    }

    #[test]
    fn the_published_opener_is_pressed_in_order_then_the_scorer_takes_over() {
        let strike = |id: u32, slot: SkillSlot, mult: f64| {
            skill(
                id,
                slot,
                500,
                10_000,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: mult,
                }],
            )
        };
        // The scorer alone would open with the 3.0x skill.
        let skills = vec![
            strike(1, SkillSlot::Weapon2, 3.0),
            strike(2, SkillSlot::Weapon3, 1.0),
            strike(3, SkillSlot::Weapon4, 1.0),
        ];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(5_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        let opener = [3u32, 2];
        timeline.opener = &opener;
        assert_eq!(timeline.pick_skill(), Some(2), "page says skill 3 first");
        timeline.set_skill_cooldown(3, 10_000);
        assert_eq!(timeline.pick_skill(), Some(1), "then skill 2");
        timeline.set_skill_cooldown(2, 10_000);
        assert_eq!(
            timeline.pick_skill(),
            Some(0),
            "opener spent: scorer picks the 3.0x"
        );
    }

    #[test]
    fn recharge_starts_when_the_cast_resolves() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.start_cast(0);
        timeline.now_ms = 1_000;
        timeline.resolve_pending_cast();
        assert!(timeline.pending.is_none());
        assert_eq!(timeline.cooldown_ready_ms[0], 31_000);
    }

    #[test]
    fn a_strip_takes_the_most_recently_applied_boon() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        // Stability first with a long duration, Protection second and short:
        // the soonest-expiring rule would take Protection, LIFO takes it too —
        // so extend Stability afterwards to prove extension does not re-sort.
        timeline.apply_buff("Stability", 3, 20_000, false);
        timeline.now_ms = 1_000;
        timeline.apply_buff("Protection", 1, 5_000, false);
        timeline.now_ms = 2_000;
        timeline.apply_buff("Stability", 3, 20_000, false);
        timeline.receive_boon_strip(1);
        let kinds: Vec<CoverKind> = timeline.defenses.iter().map(|d| d.kind).collect();
        assert!(kinds.contains(&CoverKind::Stability), "{kinds:?}");
        assert!(!kinds.contains(&CoverKind::Protection), "{kinds:?}");
    }

    #[test]
    fn boon_duration_caps_follow_the_wiki() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.apply_buff("Fury", 1, 90_000, false);
        timeline.apply_buff("Swiftness", 1, 90_000, false);
        timeline.apply_buff("Might", 1, 90_000, false);
        let expires = |t: &Timeline, name: &str| {
            t.buffs
                .iter()
                .find(|b| b.name == name)
                .map(|b| b.expires_at_ms)
                .unwrap()
        };
        assert_eq!(expires(&timeline, "Fury"), 30_000);
        assert_eq!(expires(&timeline, "Swiftness"), 60_000);
        assert_eq!(expires(&timeline, "Might"), 90_000);
    }

    #[test]
    fn might_stacks_cap_at_the_wiki_twenty_five() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            30_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.apply_buff("Might", 20, 10_000, false);
        timeline.apply_buff("Might", 10, 10_000, false);
        assert_eq!(timeline.buff_stacks("Might"), 25);
        timeline.apply_buff("Fury", 1, 10_000, false);
        timeline.apply_buff("Fury", 1, 10_000, false);
        assert_eq!(timeline.buff_stacks("Fury"), 1);
    }

    #[test]
    fn killed_dummy_stops_incoming_after_one_second() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            50,
            1_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 8.0,
            }],
        )];
        let events = vec![
            EnemyEvent {
                at_ms: 2_000,
                kind: EnemyEventKind::Strike {
                    damage: 3_000.0,
                    unblockable: true,
                },
            },
            EnemyEvent {
                at_ms: 2_500,
                kind: EnemyEventKind::Control {
                    duration_ms: 1_100,
                    unblockable: true,
                },
            },
            EnemyEvent {
                at_ms: 3_000,
                kind: EnemyEventKind::Condition {
                    condition: "Bleeding".into(),
                    stacks: 5,
                    duration_ms: 4_000,
                },
            },
            EnemyEvent {
                at_ms: 3_500,
                kind: EnemyEventKind::BoonStrip { count: 1 },
            },
        ];
        let mut fight = profile(5_000, events);
        fight.target_health = Some(1.0);
        let params = params();
        let report = run_report(&skills, &[], open_enemy(false), fight, &params);
        assert!(report.target_reached);
        assert!(report.target_reached_at_ms.expect("kill time") <= 1_000);
        assert_eq!(report.incoming_damage, 0.0);
        assert_eq!(report.interrupted_casts, 0);
    }

    #[test]
    fn fractional_condition_pays_half_tick_on_expiry() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(5_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.outgoing_conditions.push(TimedCondition {
            name: "Bleeding".into(),
            stacks: 1,
            expires_at_ms: 1_500,
            next_tick_ms: 1_000,
        });
        let one_tick = condition_tick_damage("Bleeding", params.condition_damage, &params.mode);
        timeline.now_ms = 1_000;
        timeline.tick_conditions();
        timeline.now_ms = 1_500;
        timeline.tick_conditions();
        let total: f64 = timeline
            .damage_events
            .iter()
            .map(|event| event.amount)
            .sum();
        assert!(
            (total - one_tick * 1.5).abs() < 0.001,
            "expected 1.5 ticks ({}) got {total}",
            one_tick * 1.5
        );
        assert!(timeline.outgoing_conditions.is_empty());
    }

    #[test]
    fn no_outcome_target_never_reaches() {
        let params = params();
        let enemy = EnemyDummy {
            protection: false,
            stability: false,
            hp: None,
        };
        let scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Squad,
            combat_kind: CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: "no-target".into(),
            },
            patch_id: None,
            objective_profile_id: None,
        };
        let built = WvwProfile::for_scenario(&scenario, &enemy, &params, 5_000);
        assert!(built.target_health.is_none());
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            50,
            200,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 20.0,
            }],
        )];
        let report = run_report(&skills, &[], enemy, built, &params);
        assert!(!report.target_reached);
        assert!(report.target_reached_at_ms.is_none());
        assert!(report.total_damage > 0.0);
    }

    #[test]
    fn unknown_strike_pct_is_unmodeled_not_one_percent() {
        use crate::data::normalized_effects::{StackingRule, UptimeModel, UptimeModelKind};
        use crate::data::EvidenceLevel;
        let effect = NormalizedEffect {
            effect_id: "test-unknown-strike".into(),
            source_type: SourceType::Relic,
            source_id: 1,
            source_name: "test".into(),
            category: EffectCategory::StrikeDamagePct,
            value: FactualValue::Unknown,
            stacking_rule: StackingRule::NonStacking,
            trigger_rule: TriggerRule::OnHit,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::Unknown,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Unknown,
            source: None,
            effect_duration: None,
            internal_cooldown: None,
            max_stacks: None,
            status_operation: None,
            inner_category: None,
            health_threshold: None,
            proc_chance: None,
            trigger_scope: None,
        };
        let params = params();
        let timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[&effect],
            &[],
            true,
            Vec::new(),
        );
        assert_eq!(timeline.unmodeled_names.len(), 1);
        assert!(timeline.proc_specs.is_empty());
    }

    #[test]
    fn corrupt_maps_condition_steal_grants_boon() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(true),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.enemy_protection = true;
        timeline.apply_skill_effect(1, &SkillEffect::CorruptBoons, false);
        assert!(!timeline.enemy_stability);
        assert_eq!(timeline.outgoing_conditions[0].name, "Fear");
        timeline.apply_skill_effect(1, &SkillEffect::StealBoons, false);
        assert!(!timeline.enemy_protection);
        assert!(timeline.has_buff("Protection"));
        let conditions = timeline.outgoing_conditions.len();
        let buffs = timeline.buffs.len();
        timeline.apply_skill_effect(1, &SkillEffect::CorruptBoons, false);
        timeline.apply_skill_effect(1, &SkillEffect::StealBoons, false);
        assert_eq!(timeline.outgoing_conditions.len(), conditions);
        assert_eq!(timeline.buffs.len(), buffs);
    }
}

/// Causal experiments on the hand-authored Reaper slice
/// (`specs/004-simulator-trust`, audit section 6). Each test names its kind.
/// Every one was seen failing once under the disabling change recorded in
/// `docs/simulator-connection-audit.md`.
#[cfg(test)]
mod reaper_experiments {
    use super::*;
    use crate::engine;
    use crate::rotation::reaper_fixture as fx;
    use crate::sigil_slots::SigilSlots;
    use crate::validation::ValidatedBuild;

    fn prepared() -> engine::PreparedRotation {
        let db = fx::db();
        let build = fx::build();
        let (ctx, scenario) = fx::scenario();
        let (stats, _) = engine::calculate_validated_stats(&build, &db, "Necromancer", &ctx);
        let mut prepared = engine::prepare_validated_rotation(&build, &db, &stats, Some(&scenario))
            .expect("the fixture prepares a rotation");
        prepared.opener = fx::opener();
        prepared
    }

    /// The fixture through the production entry point, with the trace on.
    fn traced(build: &ValidatedBuild, precision: Option<f64>) -> WvwCombatReport {
        traced_opener(build, fx::opener(), precision)
    }

    /// `traced` with a custom press order.
    fn traced_opener(
        build: &ValidatedBuild,
        opener: Vec<u32>,
        precision: Option<f64>,
    ) -> WvwCombatReport {
        let db = fx::db();
        let (ctx, scenario) = fx::scenario();
        let (stats, _) = engine::calculate_validated_stats(build, &db, "Necromancer", &ctx);
        let mut prepared = engine::prepare_validated_rotation(build, &db, &stats, Some(&scenario))
            .expect("the fixture prepares a rotation");
        prepared.opener = opener;
        if let Some(precision) = precision {
            prepared.params.precision = precision;
        }
        engine::simulate_prepared_traced(&prepared, build, &db, Some(&scenario))
            .wvw
            .expect("WvW scenario runs the timeline")
    }

    fn first_swap_ms(report: &WvwCombatReport) -> Option<u32> {
        report
            .trace
            .iter()
            .find(|event| event.kind == TraceKind::WeaponSwap)
            .map(|event| event.t_ms)
    }

    // ── US2: swapping weapons swaps sigils (T020, T021) ─────────────────────

    /// US2 positive and negative control: a sigil on set 2 fires only after
    /// the swap; moved to set 1 it fires only before.
    #[test]
    fn reaper_swap_loads_set_two_sigils() {
        let on_two = traced_opener(
            &fx::build_with_set_two_fire(),
            fx::opener_with_swap(),
            Some(3_000.0),
        );
        let swap = first_swap_ms(&on_two).expect("the opener crosses to set 2");
        let fired = events(&on_two, TraceKind::ProcFired, "Superior Sigil of Fire");
        assert!(
            !fired.is_empty() && fired.iter().all(|e| e.t_ms >= swap),
            "set-2 sigil fires only after the swap at {swap} ms: {fired:?}"
        );

        let on_one = traced_opener(&fx::build(), fx::opener_with_swap(), Some(3_000.0));
        let swap = first_swap_ms(&on_one).expect("the opener crosses to set 2");
        let fired = events(&on_one, TraceKind::ProcFired, "Superior Sigil of Fire");
        assert!(
            !fired.is_empty() && fired.iter().all(|e| e.t_ms < swap),
            "set-1 sigil fires only before the swap at {swap} ms: {fired:?}"
        );
    }

    /// US2 timing control: the same sigil on both sets keeps one cooldown
    /// across the swap.
    #[test]
    fn reaper_swap_keeps_icd_across_sets() {
        let mut both = fx::build();
        both.set_sigil_seats([
            Some(crate::validation::ValidatedItem {
                id: fx::SIGIL_OF_FIRE,
                name: "Superior Sigil of Fire".into(),
            }),
            None,
            Some(crate::validation::ValidatedItem {
                id: fx::SIGIL_OF_FIRE,
                name: "Superior Sigil of Fire".into(),
            }),
            None,
        ]);
        let report = traced_opener(&both, fx::opener_with_swap(), Some(3_000.0));
        let swap = first_swap_ms(&report).expect("the opener crosses to set 2");
        let fired = events(&report, TraceKind::ProcFired, "Superior Sigil of Fire");
        let skipped = events(&report, TraceKind::ProcSkippedIcd, "Superior Sigil of Fire");
        assert!(
            fired.iter().any(|e| e.t_ms < swap),
            "fires on set 1 before the swap: {fired:?}"
        );
        assert!(
            skipped
                .iter()
                .any(|e| e.t_ms >= swap && e.t_ms < swap + 5_000),
            "a set-2 hit inside the cooldown started on set 1 is skipped: {skipped:?}"
        );
        assert!(
            !fired
                .iter()
                .any(|e| e.t_ms >= swap && e.t_ms < fired[0].t_ms + 5_000),
            "no second fire inside the first 5 s cooldown because the set changed: {fired:?}"
        );
    }

    /// US2 scenario 4: a set-2 sigil with a record is modeled, not listed.
    #[test]
    fn reaper_set_two_sigil_leaves_coverage_line() {
        let report = traced(&fx::build_with_set_two_fire(), None);
        assert!(
            !report
                .unmodeled_sources
                .iter()
                .any(|s| s.contains("Sigil of Fire")),
            "the stowed sigil's record loads and it leaves the coverage line: {:?}",
            report.unmodeled_sources
        );
        assert!(
            report
                .proc_trials
                .iter()
                .any(|trial| trial.source == "Superior Sigil of Fire"),
            "the stowed sigil's record is loaded (it has a trial entry): {:?}",
            report.proc_trials
        );
    }

    fn open_profile(duration_ms: u32, events: Vec<EnemyEvent>) -> WvwProfile {
        WvwProfile {
            duration_ms,
            target_health: None,
            enemy_events: events.into(),
            required_window_ms: MIN_PROTECTED_WINDOW_MS,
            desired_window_ms: TARGET_PROTECTED_WINDOW_MS.min(duration_ms),
        }
    }

    fn still_enemy() -> EnemyDummy {
        EnemyDummy {
            protection: false,
            stability: true,
            hp: None,
        }
    }

    /// A pinned run: `opener` pressed in order, no enemy pressure unless the
    /// profile says so, trace on.
    fn run(
        skills: &[RotationSkill],
        params: &SimParams,
        opener: &[u32],
        effects: &[&NormalizedEffect],
        profile: WvwProfile,
    ) -> WvwCombatReport {
        let mut timeline = Timeline::new(
            skills,
            params,
            profile,
            still_enemy(),
            effects,
            &[],
            false,
            Vec::new(),
        );
        timeline.opener = opener;
        timeline.trace_enabled = true;
        timeline.trace_loaded_unmodeled();
        timeline.run();
        timeline.report()
    }

    fn only(skills: &[RotationSkill], ids: &[u32]) -> Vec<RotationSkill> {
        skills
            .iter()
            .filter(|skill| ids.contains(&skill.skill_id))
            .cloned()
            .collect()
    }

    fn events<'a>(
        report: &'a WvwCombatReport,
        kind: TraceKind,
        source: &str,
    ) -> Vec<&'a TraceEvent> {
        report
            .trace
            .iter()
            .filter(|event| event.kind == kind && event.source.starts_with(source))
            .collect()
    }

    fn landed(report: &WvwCombatReport, skill: &str) -> Vec<f64> {
        events(report, TraceKind::HitLanded, skill)
            .iter()
            .map(|event| event.detail.parse::<f64>().expect("damage detail"))
            .collect()
    }

    // ── Sprint 2 fixture variants (specs/005-wvw-proc-sites, T002) ──────────

    #[test]
    fn sprint2_fixture_variants_are_consistent() {
        let set_two = fx::build_with_set_two_fire();
        assert_eq!(set_two.active_sigil_ids(), vec![fx::SIGIL_OF_FORCE]);
        assert_eq!(set_two.sigils.len(), 2);

        let db = fx::db();
        let (ctx, _) = fx::scenario();
        let (marauder, _) =
            engine::calculate_validated_stats(&fx::build(), &db, "Necromancer", &ctx);
        let (soldier, _) = engine::calculate_validated_stats(
            &fx::build_with_zero_precision(),
            &db,
            "Necromancer",
            &ctx,
        );
        assert!(
            soldier.get("Precision") < marauder.get("Precision"),
            "Soldier carries no precision"
        );

        let records = fx::records_with_threshold_and_stack();
        assert!(records[0].health_threshold.is_some());
        assert_eq!(
            records[1].trigger_scope,
            Some(crate::data::normalized_effects::TriggerScope::WeaponSkillWithRecharge)
        );
        assert_eq!(
            records[1].max_stacks,
            Some(crate::data::FactualValue::Resolved(5))
        );

        let p = prepared();
        let sets: Vec<u8> = fx::opener_with_swap()
            .iter()
            .map(|id| {
                p.skills
                    .iter()
                    .find(|s| s.skill_id == *id)
                    .unwrap()
                    .weapon_set
            })
            .collect();
        assert_eq!(
            sets,
            vec![1, 2],
            "the swap opener crosses from set 1 to set 2"
        );
    }

    // ── US3: conditional bonuses (T027, T028) ───────────────────────────────

    fn strike_at(at_ms: u32, damage: f64) -> EnemyEvent {
        EnemyEvent {
            at_ms,
            kind: EnemyEventKind::Strike {
                damage,
                unblockable: true,
            },
        }
    }

    /// US3 scenarios 1 and 2: the Scholar bonus applies from t = 0 while the
    /// player stays above 90 % health and drops out at the crossing.
    #[test]
    fn reaper_scholar_applies_only_above_threshold() {
        let p = prepared();
        let records = fx::records_with_threshold_and_stack();
        let scholar = &records[0];
        let opener = [fx::GRAVEDIGGER, fx::DEATH_SPIRAL];
        let bare = run(
            &p.skills,
            &p.params,
            &opener,
            &[],
            open_profile(3_000, vec![]),
        );
        let healthy = run(
            &p.skills,
            &p.params,
            &opener,
            &[scholar],
            open_profile(3_000, vec![]),
        );
        let activated = events(
            &healthy,
            TraceKind::ConditionalActivated,
            "Superior Rune of the Scholar",
        );
        assert!(
            activated.first().is_some_and(|e| e.t_ms == 0),
            "the threshold is true at the start of the fight: {activated:?}"
        );
        let bare_hits = landed(&bare, "Gravedigger");
        let boosted_hits = landed(&healthy, "Gravedigger");
        assert_eq!(bare_hits.len(), boosted_hits.len());
        for (bare_hit, boosted) in bare_hits.iter().zip(&boosted_hits) {
            // The trace prints one decimal.
            assert!(
                (boosted - bare_hit * 1.05).abs() < 0.06,
                "every strike above the threshold carries +5 %: {bare_hit} → {boosted}"
            );
        }

        // Incoming damage takes the player to 75 % at 1 000 ms.
        let hit = p.params.max_health * 0.25;
        let wounded = run(
            &p.skills,
            &p.params,
            &opener,
            &[scholar],
            open_profile(3_000, vec![strike_at(1_000, hit)]),
        );
        let expired = events(
            &wounded,
            TraceKind::ConditionalExpired,
            "Superior Rune of the Scholar",
        );
        assert!(
            expired
                .first()
                .is_some_and(|e| (1_000..=1_000 + TIMELINE_TICK_MS).contains(&e.t_ms)),
            "the bonus expires at the crossing: {expired:?}"
        );
        let crossing = expired[0].t_ms;
        let late_bare: Vec<f64> = events(&bare, TraceKind::HitLanded, "Death Spiral")
            .iter()
            .filter(|e| e.t_ms >= crossing)
            .map(|e| e.detail.parse::<f64>().unwrap())
            .collect();
        let late_wounded: Vec<f64> = events(&wounded, TraceKind::HitLanded, "Death Spiral")
            .iter()
            .filter(|e| e.t_ms >= crossing)
            .map(|e| e.detail.parse::<f64>().unwrap())
            .collect();
        assert!(!late_wounded.is_empty(), "hits land after the crossing");
        for (a, b) in late_bare.iter().zip(&late_wounded) {
            assert!(
                (a - b).abs() < 1e-6,
                "strikes below the threshold carry no bonus: {a} vs {b}"
            );
        }
    }

    /// US3 scenario 3: the Thief stacks cap at five, a sixth qualifying hit
    /// refreshes, and the stacks expire 6 s after the last one.
    #[test]
    fn reaper_thief_stacks_cap_and_expire() {
        let p = prepared();
        let records = fx::records_with_threshold_and_stack();
        let thief = &records[1];
        let skills = only(
            &p.skills,
            &[
                fx::GRAVEDIGGER,
                fx::DEATH_SPIRAL,
                fx::GRASPING_DARKNESS,
                fx::GS_AUTO,
            ],
        );
        let report = run(
            &skills,
            &p.params,
            &[fx::GRAVEDIGGER, fx::DEATH_SPIRAL, fx::GRASPING_DARKNESS],
            &[thief],
            open_profile(12_000, vec![]),
        );
        let gained = events(&report, TraceKind::StackGained, "Relic of the Thief");
        assert!(
            gained.len() >= 6,
            "six qualifying weapon-skill hits gain or refresh: {gained:?}"
        );
        assert!(
            gained.iter().all(|e| !e.detail.starts_with("6/")),
            "never a sixth stack: {gained:?}"
        );
        assert!(
            gained
                .iter()
                .filter(|e| e.detail.starts_with("5/5"))
                .count()
                >= 2,
            "the cap is reached and then refreshed: {gained:?}"
        );
        let expired = events(&report, TraceKind::ConditionalExpired, "Relic of the Thief");
        assert!(
            expired.iter().any(|e| {
                let last_gain_before = gained
                    .iter()
                    .filter(|g| g.t_ms < e.t_ms)
                    .map(|g| g.t_ms)
                    .max()
                    .unwrap_or(0);
                (6_000 - TIMELINE_TICK_MS..=6_000 + TIMELINE_TICK_MS)
                    .contains(&(e.t_ms - last_gain_before))
            }),
            "stacks expire 6 s after the last qualifying hit: {gained:?}, {expired:?}"
        );
        assert!(
            events(&report, TraceKind::StackGained, "Relic of the Thief")
                .iter()
                .all(|e| e.source != "Dusk Strike"),
            "auto-attacks without a recharge do not stack"
        );
    }

    /// US3 scenario 4 (FR-008): an unresolved threshold stays on the
    /// coverage line and never executes.
    #[test]
    fn reaper_unresolved_conditional_stays_named() {
        let p = prepared();
        let mut records = fx::records_with_threshold_and_stack();
        records[0].health_threshold = Some(crate::data::normalized_effects::HealthThreshold {
            above: true,
            percent: FactualValue::Unknown,
        });
        let report = run(
            &p.skills,
            &p.params,
            &[fx::GRAVEDIGGER],
            &[&records[0]],
            open_profile(2_000, vec![]),
        );
        assert!(
            report
                .unmodeled_sources
                .iter()
                .any(|s| s == "Superior Rune of the Scholar (unresolved value)"),
            "named as unresolved: {:?}",
            report.unmodeled_sources
        );
        assert!(
            events(
                &report,
                TraceKind::ConditionalActivated,
                "Superior Rune of the Scholar"
            )
            .is_empty(),
            "never executed with an invented number"
        );
    }

    // ── Diagnostics (T022) ──────────────────────────────────────────────────

    #[test]
    fn trace_is_empty_unless_requested() {
        let db = fx::db();
        let build = fx::build();
        let (_, scenario) = fx::scenario();
        let prepared = prepared();
        let production = engine::simulate_prepared(&prepared, &build, &db, Some(&scenario))
            .wvw
            .expect("WvW");
        assert!(
            production.trace.is_empty() && !production.trace_truncated,
            "the production entry point never traces"
        );
        let traced = engine::simulate_prepared_traced(&prepared, &build, &db, Some(&scenario))
            .wvw
            .expect("WvW");
        assert!(!traced.trace.is_empty(), "the test entry point does");
    }

    #[test]
    fn trace_caps_at_512_and_flags_truncation() {
        let params = SimParams::basic(2_000.0, 0.0, 1_100.0);
        let mut timeline = Timeline::new(
            &[],
            &params,
            open_profile(1_000, vec![]),
            still_enemy(),
            &[],
            &[],
            true,
            Vec::new(),
        );
        timeline.trace_enabled = true;
        for _ in 0..600 {
            timeline.trace(TraceKind::HitLanded, "x", "");
        }
        assert_eq!(timeline.trace.len(), TRACE_CAP);
        assert!(timeline.trace_truncated);
        let report = timeline.report();
        assert_eq!(report.trace.len(), TRACE_CAP);
        assert!(report.trace_truncated);
    }

    // ── Positive control ────────────────────────────────────────────────────

    #[test]
    fn reaper_positive_control_onhit_proc_changes_events() {
        let p = prepared();
        let record = fx::path_of_corruption();
        let with = run(
            &p.skills,
            &p.params,
            &p.opener,
            &[&record],
            open_profile(15_000, vec![]),
        );
        let without = run(
            &p.skills,
            &p.params,
            &p.opener,
            &[],
            open_profile(15_000, vec![]),
        );

        let fired = events(&with, TraceKind::ProcFired, "Path of Corruption");
        assert!(
            !fired.is_empty(),
            "Path of Corruption fires on the opener's landed hits when its record is active; trace: {:?}",
            with.trace.iter().take(12).collect::<Vec<_>>()
        );
        assert!(
            events(&without, TraceKind::ProcFired, "Path of Corruption").is_empty(),
            "nothing fires without the record"
        );
        assert!(
            events(&with, TraceKind::ProcUnmodeled, "Path of Corruption").is_empty(),
            "an executed OnHit record is never reported as unmodeled"
        );
        assert_eq!(with.unmodeled_sources, without.unmodeled_sources);
    }

    // ── Negative control ────────────────────────────────────────────────────

    #[test]
    fn reaper_negative_control_wrong_mode_and_stowed_set() {
        // (a) A record that exists only in the PvE file is not selected for
        // the WvW scenario. Signet of Undeath (skill 10611, OnSkillUse) is
        // such a record on the 2026-01-13 manifest.
        let data = crate::data::normalized_effects::effects();
        assert!(
            data.effects_for_mode("PvE")
                .iter()
                .any(|e| e.source_id == 10611),
            "control premise: the record exists in the PvE file"
        );
        assert!(
            !data
                .effects_for_mode("WvW")
                .iter()
                .any(|e| e.source_id == 10611),
            "control premise: and not in the WvW file"
        );
        let mut db = fx::db();
        let signet: gw2_api::models::Skill = serde_json::from_value(serde_json::json!({
            "id": 10611, "name": "Signet of Undeath", "slot": "Utility",
            "professions": ["Necromancer"],
            "facts": [{"type": "Recharge", "value": 60.0}]
        }))
        .expect("fixture skill");
        db.skills.insert(10611, signet);
        let mut build = fx::build();
        build.skills.utilities[1] = Some((10611, "Signet of Undeath".into()));
        let (ctx, scenario) = fx::scenario();
        let (stats, _) = engine::calculate_validated_stats(&build, &db, "Necromancer", &ctx);
        let mut prepared = engine::prepare_validated_rotation(&build, &db, &stats, Some(&scenario))
            .expect("prepares");
        prepared.opener = vec![10611, fx::GRAVEDIGGER];
        let fight = engine::simulate_prepared_traced(&prepared, &build, &db, Some(&scenario))
            .wvw
            .expect("WvW");
        let record_events: Vec<&TraceEvent> = fight
            .trace
            .iter()
            .filter(|e| {
                e.source.starts_with("Signet of Undeath")
                    && (matches!(e.kind, TraceKind::ProcFired | TraceKind::ProcSkippedIcd)
                        || (e.kind == TraceKind::ProcUnmodeled && e.detail == "unsupported proc"))
            })
            .collect();
        assert!(
            record_events.is_empty(),
            "a PvE-only record is never loaded as a proc under WvW: {record_events:?}"
        );
        assert!(
            fight
                .unmodeled_sources
                .iter()
                .any(|s| s == "Signet of Undeath (no record)"),
            "under WvW the skill is an equipped source with no record, not a loaded proc: {:?}",
            fight.unmodeled_sources
        );

        // (b) Sigil of Fire stowed on weapon set 2 while set 1 is worn is
        // absent from the fight entirely: not fired, not counted, not named.
        let worn = traced(&fx::build(), None);
        let mut stowed_build = fx::build();
        stowed_build.sigil_seats = SigilSlots::new([
            None,
            Some(fx::SIGIL_OF_FORCE),
            Some(fx::SIGIL_OF_FIRE),
            None,
        ]);
        let stowed = traced(&stowed_build, None);
        assert!(
            !events(&worn, TraceKind::ProcFired, "Superior Sigil of Fire").is_empty(),
            "worn: the on-crit sigil fires (Sprint 2)"
        );
        let first_swap = first_swap_ms(&stowed).unwrap_or(u32::MAX);
        assert!(
            events(&stowed, TraceKind::ProcUnmodeled, "Superior Sigil of Fire").is_empty()
                && events(&stowed, TraceKind::ProcFired, "Superior Sigil of Fire")
                    .iter()
                    .all(|e| e.t_ms >= first_swap),
            "stowed: the sigil is absent from the fight before a swap"
        );
        assert!(
            !worn
                .unmodeled_sources
                .iter()
                .chain(&stowed.unmodeled_sources)
                .any(|s| s.starts_with("Superior Sigil of Fire")),
            "the sigil is modeled worn and stowed alike; neither names it: {:?} vs {:?}",
            stowed.unmodeled_sources,
            worn.unmodeled_sources
        );
    }

    // ── Timing ──────────────────────────────────────────────────────────────

    #[test]
    fn reaper_timing_icd_interrupt_and_late_buff() {
        let p = prepared();

        // (1) Internal cooldown: consecutive procs are at least 10 s apart.
        let record = fx::path_of_corruption();
        let report = run(
            &p.skills,
            &p.params,
            &p.opener,
            &[&record],
            open_profile(30_000, vec![]),
        );
        let fired: Vec<u32> = events(&report, TraceKind::ProcFired, "Path of Corruption")
            .iter()
            .map(|e| e.t_ms)
            .collect();
        assert!(
            fired.len() >= 2,
            "need two procs in 30 s to test spacing: {fired:?}"
        );
        for pair in fired.windows(2) {
            assert!(
                pair[1] - pair[0] >= 10_000,
                "procs {} ms and {} ms are inside the 10 s ICD",
                pair[0],
                pair[1]
            );
        }
        assert!(
            !events(&report, TraceKind::ProcSkippedIcd, "Path of Corruption").is_empty(),
            "hits inside the ICD are recorded as skipped, not silently dropped"
        );

        // (2) Interrupt: a control landing mid-channel loses the later hit.
        // Gravedigger (fixture): 500 ms cast, two hits at 250 and 500 ms.
        let gravedigger = only(&p.skills, &[fx::GRAVEDIGGER]);
        let calm = run(
            &gravedigger,
            &p.params,
            &[fx::GRAVEDIGGER],
            &[],
            open_profile(1_000, vec![]),
        );
        let control = EnemyEvent {
            at_ms: 300,
            kind: EnemyEventKind::Control {
                duration_ms: 1_000,
                unblockable: true,
            },
        };
        let cut = run(
            &gravedigger,
            &p.params,
            &[fx::GRAVEDIGGER],
            &[],
            open_profile(1_000, vec![control]),
        );
        assert!(
            !events(&cut, TraceKind::CastInterrupted, "Gravedigger").is_empty(),
            "the control interrupted the cast: {:?}",
            cut.trace
        );
        assert!(
            landed(&cut, "Gravedigger").len() < landed(&calm, "Gravedigger").len(),
            "interrupted {} hits vs calm {} hits",
            landed(&cut, "Gravedigger").len(),
            landed(&calm, "Gravedigger").len()
        );

        // (3) Late buff: Might applied after a hit does not raise that hit;
        // it raises the next one.
        let pair = only(&p.skills, &[fx::GRAVEDIGGER, fx::YOU_ARE_ALL_WEAKLINGS]);
        let plain = run(
            &gravedigger,
            &p.params,
            &[fx::GRAVEDIGGER],
            &[],
            open_profile(12_000, vec![]),
        );
        let late = run(
            &pair,
            &p.params,
            &[fx::GRAVEDIGGER, fx::YOU_ARE_ALL_WEAKLINGS],
            &[],
            open_profile(12_000, vec![]),
        );
        let plain_hits = landed(&plain, "Gravedigger");
        let late_hits = landed(&late, "Gravedigger");
        assert!(
            plain_hits.len() >= 3 && late_hits.len() >= 3,
            "{plain_hits:?} {late_hits:?}"
        );
        assert_eq!(
            plain_hits[..2],
            late_hits[..2],
            "the hits before the buff are priced the same"
        );
        assert!(
            late_hits.last().unwrap() > plain_hits.last().unwrap(),
            "the recast under Might hits harder: {late_hits:?} vs {plain_hits:?}"
        );
    }

    // ── Ablation ────────────────────────────────────────────────────────────

    #[test]
    fn reaper_ablation_enabler_and_payoff() {
        let p = prepared();
        let record = fx::path_of_corruption();
        let complete_kit = only(&p.skills, &[fx::GRAVEDIGGER, fx::WELL_OF_DARKNESS]);
        let no_enabler_kit = only(&p.skills, &[fx::WELL_OF_DARKNESS]);
        let opener = [fx::WELL_OF_DARKNESS, fx::GRAVEDIGGER];
        // Expected event difference, stated before the assertion: the
        // complete kit lands Gravedigger and the OnHit record fires at least
        // once; without the strike (enabler) there is no landed hit to fire
        // on; without the record (payoff) there is nothing to fire. Equal
        // counts are allowed only when the ICD saturates, which needs at
        // least one proc on both sides — impossible for either ablation.
        let complete = run(
            &complete_kit,
            &p.params,
            &opener,
            &[&record],
            open_profile(5_000, vec![]),
        );
        let missing_enabler = run(
            &no_enabler_kit,
            &p.params,
            &opener,
            &[&record],
            open_profile(5_000, vec![]),
        );
        let missing_payoff = run(
            &complete_kit,
            &p.params,
            &opener,
            &[],
            open_profile(5_000, vec![]),
        );
        let procs =
            |r: &WvwCombatReport| events(r, TraceKind::ProcFired, "Path of Corruption").len();
        assert!(
            procs(&complete) >= 1,
            "complete mechanism fires: {:?}",
            complete.trace
        );
        assert!(
            procs(&complete) > procs(&missing_enabler),
            "complete {} vs missing enabler {}",
            procs(&complete),
            procs(&missing_enabler)
        );
        assert!(
            procs(&complete) > procs(&missing_payoff),
            "complete {} vs missing payoff {}",
            procs(&complete),
            procs(&missing_payoff)
        );
    }

    // ── Unsupported control (regression test of the coverage remedy) ────────

    /// US1 positive control (was Sprint 1's `reaper_unsupported_oncrit_is_named_not_zeroed`,
    /// inverted): with the on-crit firing site the shipped Sigil of Fire
    /// record fires from the fixture's critical hits and leaves the coverage
    /// line.
    #[test]
    fn reaper_oncrit_positive_control_fires_from_crits() {
        // Precision forced to a 100 % critical chance.
        let with_fire = traced(&fx::build(), Some(3_000.0));
        assert!(
            !events(&with_fire, TraceKind::ProcFired, "Superior Sigil of Fire").is_empty(),
            "the on-crit sigil fires from the opener's critical hits; trace: {:?}",
            with_fire.trace.iter().take(16).collect::<Vec<_>>()
        );
        assert!(
            !with_fire
                .unmodeled_sources
                .iter()
                .any(|s| s.contains("Sigil of Fire")),
            "an executed on-crit record leaves the coverage line: {:?}",
            with_fire.unmodeled_sources
        );

        let mut bare_build = fx::build();
        bare_build.sigils.retain(|s| s.id != fx::SIGIL_OF_FIRE);
        bare_build.sigil_seats = SigilSlots::new([None, Some(fx::SIGIL_OF_FORCE), None, None]);
        let bare = traced(&bare_build, Some(3_000.0));
        assert!(
            with_fire.total_damage > bare.total_damage,
            "the sigil now adds to the fight: {} vs {}",
            with_fire.total_damage,
            bare.total_damage
        );
    }

    /// US1 negative control: no critical chance, no on-crit proc, and the
    /// sigil is worth nothing.
    #[test]
    fn reaper_oncrit_zero_precision_never_fires() {
        let with_fire = traced(&fx::build_with_zero_precision(), Some(0.0));
        assert!(
            events(&with_fire, TraceKind::ProcFired, "Superior Sigil of Fire").is_empty(),
            "no crit chance, no proc: {:?}",
            events(&with_fire, TraceKind::ProcFired, "Superior Sigil of Fire")
        );
        let mut bare_build = fx::build_with_zero_precision();
        bare_build.sigils.retain(|s| s.id != fx::SIGIL_OF_FIRE);
        bare_build.sigil_seats = SigilSlots::new([None, Some(fx::SIGIL_OF_FORCE), None, None]);
        let bare = traced(&bare_build, Some(0.0));
        assert!(
            (with_fire.total_damage - bare.total_damage).abs() < 1e-9,
            "the sigil adds nothing without crits: {} vs {}",
            with_fire.total_damage,
            bare.total_damage
        );
    }

    /// US1 timing control: the internal cooldown bounds the rate. With a
    /// certain crit, the first hit fires and every hit inside the next 5 s
    /// is skipped for the cooldown.
    #[test]
    fn reaper_oncrit_icd_bounds_rate() {
        let p = prepared();
        let mut params = p.params.clone();
        params.precision = 3_000.0;
        let record = fx::sigil_of_fire();
        let report = run(
            &p.skills,
            &params,
            &[fx::GRAVEDIGGER, fx::DEATH_SPIRAL, fx::GS_AUTO],
            &[&record],
            open_profile(4_500, vec![]),
        );
        let fired = events(&report, TraceKind::ProcFired, "Superior Sigil of Fire");
        let skipped = events(&report, TraceKind::ProcSkippedIcd, "Superior Sigil of Fire");
        assert_eq!(
            fired.len(),
            1,
            "one fire inside one 5 s cooldown window; trace: {:?}",
            report.trace
        );
        assert!(
            !skipped.is_empty(),
            "the later hits are skipped for the cooldown and say so"
        );
        assert!(
            landed(&report, "Gravedigger").len() + landed(&report, "Death Spiral").len() >= 3,
            "the opener landed several hits inside the window"
        );
    }

    /// US1 scenario 5 (SC-009): the trace's seeded trials bracket the
    /// expected-value count the ranking uses.
    #[test]
    fn reaper_oncrit_trials_bracket_expected_value() {
        let report = traced(&fx::build(), Some(3_000.0));
        let expected: f64 = events(&report, TraceKind::ProcFired, "Superior Sigil of Fire")
            .iter()
            .map(|event| {
                event
                    .detail
                    .rsplit('×')
                    .next()
                    .and_then(|p| p.trim().parse::<f64>().ok())
                    .unwrap_or(1.0)
            })
            .sum();
        let trial = report
            .proc_trials
            .iter()
            .find(|trial| trial.source == "Superior Sigil of Fire")
            .unwrap_or_else(|| panic!("trials exist for the sigil: {:?}", report.proc_trials));
        assert!(
            (trial.mean - expected).abs() <= 1.0,
            "trial mean {} within one proc of the expected count {}",
            trial.mean,
            expected
        );
        assert!(f64::from(trial.min) <= trial.mean && trial.mean <= f64::from(trial.max));
    }
    // ── Fixture records against the runtime's own semantics ─────────────────

    #[test]
    fn reaper_fixture_records_follow_runtime_semantics() {
        let p = prepared();
        let records = fx::records();
        let refs: Vec<&NormalizedEffect> = records.iter().collect();
        let report = run(
            &p.skills,
            &p.params,
            &p.opener,
            &refs,
            open_profile(15_000, vec![]),
        );
        assert!(
            !events(&report, TraceKind::ProcFired, "Path of Corruption").is_empty(),
            "the OnHit record fires"
        );
        assert!(
            !events(&report, TraceKind::ProcFired, "Superior Sigil of Fire").is_empty()
                && !report
                    .unmodeled_sources
                    .iter()
                    .any(|s| s.starts_with("Superior Sigil of Fire")),
            "the OnCrit record fires (Sprint 2) and is not named: {:?}",
            report.unmodeled_sources
        );
        assert!(
            report
                .unmodeled_sources
                .iter()
                .all(|s| !s.starts_with("Superior Sigil of Force")),
            "the passive record is folded into SimParams upstream and never listed: {:?}",
            report.unmodeled_sources
        );
    }
}
