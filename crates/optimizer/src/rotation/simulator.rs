//! Time-step rotation simulator with DPCT-optimal skill scheduling.
//!
//! Runs a skill rotation over a configurable duration, selecting skills by
//! **damage per cast time (DPCT)** — the skill that deals the most damage per
//! second of animation lock is always used first. Auto-attacks fill gaps.
//!
//! Supports weapon swapping: skills tagged with `weapon_set` 1 or 2 are only
//! usable from the active weapon set, with a 10-second swap cooldown.
//!
//! Design philosophy: Pure damage output is NOT the only goal. Build quality
//! is measured by the ability to DELIVER damage (CC, survivability, control)
//! as much as the raw DPS number. The simulator captures:
//! - Raw DPS potential (strike + condition)
//! - Self-buff uptime (can you maintain your own damage buffs?)
//! - Condition uptime (are conditions actually applied consistently?)
//! - CC availability (stunbreaks, stability sources)

use std::collections::HashMap;

use gw2_core::types::GameMode;

use super::combat_model::{
    kit_has_corrupt, kit_has_interrupt, kit_has_mobility_out, kit_has_stability_cover,
    kit_has_strip, setup_priority, setup_window_ms, EnemyDummy,
};
use super::skill_timings::{HUMAN_DELAY_MS, MIN_SKILL_GAP_MS};
use super::{RotationSkill, SimulationResult, SkillEffect, SkillSlot, SkillUsage};

/// Tick resolution for the simulation (ms per step).
const TICK_MS: u32 = 100;

/// Default simulation duration (30 seconds — standard GW2 benchmark window).
pub const DEFAULT_DURATION_MS: u32 = 30_000;

/// GW2 weapon swap cooldown (10 seconds in-combat).
const WEAPON_SWAP_COOLDOWN_MS: u32 = 10_000;

/// Brief invulnerability on falling down (wiki Downed / Invulnerability).
const DOWNED_INVULN_MS: u32 = 1_000;

/// WvW/PvP finisher channel. Interruptible; Quickness/Slow do not apply.
const STOMP_MS: u32 = 3_500;

/// Conditions tick every 1 second.
const CONDITION_TICK_INTERVAL_MS: u32 = 1000;

/// Reference armor value for damage calculation, loaded from data.
/// Source: https://wiki.guildwars2.com/wiki/Damage
fn reference_armor() -> f64 {
    crate::data::universal_formulas::formulas().tooltip_reference_armor
}

/// Active condition stack being tracked.
#[derive(Debug, Clone)]
struct ConditionStack {
    condition: String,
    remaining_ms: u32,
}

/// Active buff being tracked.
#[derive(Debug, Clone)]
struct BuffInstance {
    buff: String,
    remaining_ms: u32,
}

/// Internal state for a skill's cooldown.
#[derive(Debug, Clone)]
struct SkillState {
    /// Milliseconds remaining before this skill can be used again.
    cooldown_remaining_ms: u32,
}

/// Combat inputs for a rotation sim. `precision == 0` skips the crit term so
/// existing CC-only tests keep the same strike numbers.
#[derive(Debug, Clone)]
pub struct SimParams {
    pub power: f64,
    pub condition_damage: f64,
    pub weapon_strength: f64,
    pub precision: f64,
    pub ferocity: f64,
    pub strike_mult: f64,
    pub mode: GameMode,
}

impl SimParams {
    pub fn basic(power: f64, condition_damage: f64, weapon_strength: f64) -> Self {
        Self {
            power,
            condition_damage,
            weapon_strength,
            precision: 0.0,
            ferocity: 0.0,
            strike_mult: 1.0,
            mode: GameMode::PvE,
        }
    }
}

/// Run a rotation simulation with the given skills and parameters.
///
/// Skills should have their `weapon_set` field set:
/// - 0 = always available (heal, utility, elite, profession)
/// - 1 = weapon set 1
/// - 2 = weapon set 2
///
/// The simulator uses DPCT (damage per cast time) to optimally schedule skills,
/// and automatically weapon-swaps when the active set's skills are on cooldown.
pub fn simulate(
    skills: &[RotationSkill],
    duration_ms: u32,
    power: f64,
    condition_damage: f64,
    weapon_strength: f64,
) -> SimulationResult {
    simulate_with(
        skills,
        duration_ms,
        &SimParams::basic(power, condition_damage, weapon_strength),
        EnemyDummy::open(),
    )
}

/// Like [`simulate`], but the dummy can start with Protection/Stability.
pub fn simulate_against(
    skills: &[RotationSkill],
    duration_ms: u32,
    power: f64,
    condition_damage: f64,
    weapon_strength: f64,
    enemy: EnemyDummy,
) -> SimulationResult {
    simulate_with(
        skills,
        duration_ms,
        &SimParams::basic(power, condition_damage, weapon_strength),
        enemy,
    )
}

/// Full combat-aware simulation (mode, crit, strike modifiers).
pub fn simulate_with(
    skills: &[RotationSkill],
    duration_ms: u32,
    params: &SimParams,
    enemy: EnemyDummy,
) -> SimulationResult {
    let duration = if duration_ms == 0 {
        DEFAULT_DURATION_MS
    } else {
        duration_ms
    };

    let mut sim = SimState::new(skills, duration, enemy, params.clone());
    sim.setup_until_ms = setup_window_ms(duration);
    sim.run();
    sim.into_result()
}

/// Internal simulation state.
struct SimState {
    skills: Vec<RotationSkill>,
    skill_states: Vec<SkillState>,
    duration_ms: u32,
    current_time_ms: u32,
    /// Time when the character is free to use the next skill.
    next_action_ms: u32,

    // Weapon swap
    active_weapon_set: u8,
    weapon_swap_cooldown_ms: u32,
    has_weapon_sets: bool,
    /// Prefer CC/strip/cover over DPCT until this time (0 = never).
    setup_until_ms: u32,
    enemy: EnemyDummy,
    remaining_hp: Option<f64>,
    downed: bool,
    invuln_until_ms: u32,
    stomp_ends_ms: u32,

    // Tracking
    conditions: Vec<ConditionStack>,
    buffs: Vec<BuffInstance>,
    total_strike_damage: f64,
    total_condition_damage: f64,
    skill_casts: HashMap<u32, u32>,        // skill_id → cast count
    skill_damage: HashMap<u32, f64>,       // skill_id → total damage
    condition_ticks: HashMap<String, u32>, // condition → total ticks
    buff_active_ms: HashMap<String, u32>,  // buff → total ms active
    params: SimParams,
}

impl SimState {
    fn new(skills: &[RotationSkill], duration_ms: u32, enemy: EnemyDummy, params: SimParams) -> Self {
        let skill_states = skills
            .iter()
            .map(|_| SkillState {
                cooldown_remaining_ms: 0,
            })
            .collect();

        let has_weapon_sets = skills.iter().any(|s| s.weapon_set > 0);

        Self {
            skills: skills.to_vec(),
            skill_states,
            duration_ms,
            current_time_ms: 0,
            next_action_ms: 0,
            active_weapon_set: 1,
            weapon_swap_cooldown_ms: 0,
            has_weapon_sets,
            setup_until_ms: 0,
            remaining_hp: enemy.hp,
            downed: false,
            invuln_until_ms: 0,
            stomp_ends_ms: 0,
            enemy,
            conditions: Vec::new(),
            buffs: Vec::new(),
            total_strike_damage: 0.0,
            total_condition_damage: 0.0,
            skill_casts: HashMap::new(),
            skill_damage: HashMap::new(),
            condition_ticks: HashMap::new(),
            buff_active_ms: HashMap::new(),
            params,
        }
    }

    fn run(&mut self) {
        let power = self.params.power;
        let condition_damage = self.params.condition_damage;
        let weapon_strength = self.params.weapon_strength;
        while self.current_time_ms < self.duration_ms {
            // Tick conditions and buffs
            self.tick_conditions(condition_damage);
            self.tick_buffs();

            // Alacrity: skills recharge 33% faster → 100ms wall = 133ms CD.
            let cd_tick = if self.buffs.iter().any(|b| b.buff == "Alacrity") {
                TICK_MS + TICK_MS / 3
            } else {
                TICK_MS
            };
            for state in &mut self.skill_states {
                state.cooldown_remaining_ms = state.cooldown_remaining_ms.saturating_sub(cd_tick);
            }
            self.weapon_swap_cooldown_ms = self.weapon_swap_cooldown_ms.saturating_sub(TICK_MS);

            // Try to use a skill if the character is free
            if self.current_time_ms >= self.next_action_ms {
                // Check if weapon swap would be beneficial
                if self.has_weapon_sets
                    && self.should_weapon_swap(power, condition_damage, weapon_strength)
                {
                    self.weapon_swap();
                }

                if let Some(idx) = self.pick_skill(power, condition_damage, weapon_strength) {
                    self.use_skill(idx, power, weapon_strength);
                }
            }

            self.current_time_ms += TICK_MS;
        }
    }

    /// Pick the skill with the highest DPS-per-cast-time (DPCT) that is available.
    ///
    /// Availability means: off cooldown AND (weapon_set==0 OR weapon_set==active_set).
    /// Auto-attack (Weapon1 with cooldown 0) is used as filler when nothing else is ready.
    fn pick_skill(&self, power: f64, condition_damage: f64, weapon_strength: f64) -> Option<usize> {
        let mut best_idx = None;
        let mut best_dpct = 0.0f64;
        let mut filler_idx = None;

        for (i, skill) in self.skills.iter().enumerate() {
            // Check cooldown
            if self.skill_states[i].cooldown_remaining_ms > 0 {
                continue;
            }

            // Check weapon set restriction
            if !self.is_skill_available(skill) {
                continue;
            }

            // Auto-attack = filler (always available, pick last).
            // Prefer the filler from the active weapon set over weapon_set==0.
            if skill.slot == SkillSlot::Weapon1 && skill.cooldown_ms == 0 {
                match filler_idx {
                    None => filler_idx = Some(i),
                    Some(prev) => {
                        // Upgrade to active-set filler if current filler is generic
                        if self.skills[prev].weapon_set == 0
                            && skill.weapon_set == self.active_weapon_set
                        {
                            filler_idx = Some(i);
                        }
                    }
                }
                continue;
            }

            let dpct = skill_dps_efficiency(
                skill,
                power,
                condition_damage,
                weapon_strength,
                &self.params.mode,
            );
            if dpct > best_dpct {
                best_dpct = dpct;
                best_idx = Some(i);
            }
        }

        if self.current_time_ms < self.setup_until_ms {
            let mut best_setup = None;
            let mut best_p = 0u32;
            for (i, skill) in self.skills.iter().enumerate() {
                if self.skill_states[i].cooldown_remaining_ms > 0 || !self.is_skill_available(skill)
                {
                    continue;
                }
                if skill.slot == SkillSlot::Weapon1 && skill.cooldown_ms == 0 {
                    continue;
                }
                let p = setup_priority(skill);
                if p > best_p {
                    best_p = p;
                    best_setup = Some(i);
                }
            }
            if best_p > 0 {
                return best_setup.or(filler_idx);
            }
        }

        // Use highest DPCT skill, or fall back to auto-attack filler
        best_idx.or(filler_idx)
    }

    /// Check if a skill is usable given the current weapon set.
    fn is_skill_available(&self, skill: &RotationSkill) -> bool {
        if !self.has_weapon_sets || skill.weapon_set == 0 {
            return true; // non-weapon skill or no weapon sets in this sim
        }
        skill.weapon_set == self.active_weapon_set
    }

    /// Decide if we should weapon swap: all active weapon skills on CD,
    /// swap is available, and the other set has usable skills.
    fn should_weapon_swap(&self, power: f64, condition_damage: f64, weapon_strength: f64) -> bool {
        if self.weapon_swap_cooldown_ms > 0 {
            return false;
        }

        let other_set = if self.active_weapon_set == 1 { 2 } else { 1 };

        // Check: are all active set weapon skills on cooldown?
        let has_active_weapon_ready = self.skills.iter().enumerate().any(|(i, s)| {
            s.weapon_set == self.active_weapon_set
                && s.slot != SkillSlot::Weapon1
                && self.skill_states[i].cooldown_remaining_ms == 0
        });

        if has_active_weapon_ready {
            return false; // still have skills to use on current set
        }

        // Check: does the other set have off-cooldown skills with DPCT > 0?
        self.skills.iter().enumerate().any(|(i, s)| {
            s.weapon_set == other_set
                && s.slot != SkillSlot::Weapon1
                && self.skill_states[i].cooldown_remaining_ms == 0
                && skill_dps_efficiency(s, power, condition_damage, weapon_strength, &self.params.mode) > 0.0
        })
    }

    /// Perform a weapon swap.
    fn weapon_swap(&mut self) {
        self.active_weapon_set = if self.active_weapon_set == 1 { 2 } else { 1 };
        self.weapon_swap_cooldown_ms = WEAPON_SWAP_COOLDOWN_MS;
        // Weapon swap is instant in GW2 (no cast time), but add minimal delay
        self.next_action_ms = self.current_time_ms + MIN_SKILL_GAP_MS;
    }

    /// Use a skill: apply effects, set cooldown, advance next_action time.
    fn use_skill(&mut self, idx: usize, power: f64, weapon_strength: f64) {
        let skill = &self.skills[idx];
        let skill_id = skill.skill_id;
        let cast_time = skill.cast_time_ms;
        let cooldown = skill.cooldown_ms;
        let effects = skill.effects.clone();

        // Record the cast
        *self.skill_casts.entry(skill_id).or_insert(0) += 1;

        // Apply effects
        for effect in &effects {
            match effect {
                SkillEffect::StrikeDamage {
                    hit_count,
                    dmg_multiplier,
                } => {
                    let mut damage = weapon_strength * power / reference_armor()
                        * dmg_multiplier
                        * (*hit_count as f64);
                    damage *= strike_crit_factor(
                        self.params.precision,
                        self.params.ferocity,
                    ) * self.params.strike_mult;
                    if self.enemy.protection {
                        damage *=
                            crate::data::boon_condition_formulas::boons().protection_multiplier();
                    }
                    self.total_strike_damage += damage;
                    *self.skill_damage.entry(skill_id).or_insert(0.0) += damage;
                    self.apply_dummy_damage(damage);
                }
                SkillEffect::ApplyCondition {
                    condition,
                    stacks,
                    duration_ms,
                } => {
                    let cap = condition_stack_cap(&condition, &self.params.mode);
                    let current = self
                        .conditions
                        .iter()
                        .filter(|s| s.condition == *condition)
                        .count();
                    let can_apply = (*stacks as usize).min(cap.saturating_sub(current));
                    for _ in 0..can_apply {
                        self.conditions.push(ConditionStack {
                            condition: condition.clone(),
                            remaining_ms: *duration_ms,
                        });
                    }
                }
                SkillEffect::ApplyBuff {
                    buff,
                    stacks,
                    duration_ms,
                } => {
                    for _ in 0..*stacks {
                        self.buffs.push(BuffInstance {
                            buff: buff.clone(),
                            remaining_ms: *duration_ms,
                        });
                    }
                }
                SkillEffect::ComboField { .. } => {
                    // Combo fields tracked but not simulated for damage
                }
                SkillEffect::RemovesCondition { .. } => {
                    // Cleanse effects are tracked at the roster level (cleanse_count / cleanse_rate_per_20s),
                    // not as in-sim condition deletion (no enemy condi bar).
                }
                SkillEffect::CrowdControl { .. }
                | SkillEffect::ConvertConditions
                | SkillEffect::Cover { .. }
                | SkillEffect::Mobility { .. } => {}
                SkillEffect::StripBoons { .. }
                | SkillEffect::CorruptBoons
                | SkillEffect::StealBoons => {
                    self.enemy.protection = false;
                    self.enemy.stability = false;
                }
            }
        }

        // Set cooldown (0 for auto-attacks)
        self.skill_states[idx].cooldown_remaining_ms = cooldown;

        // Quickness reduces cast time: skills execute at 1.5× speed (66.7% of normal time).
        // GW2 mechanic: "skills and actions execute 50% faster" = cast * 2/3.
        // Quickness does NOT reduce cooldowns.
        // Round-half-up via (n*2 + 1)/3 to avoid systematic floor bias from integer division.
        let quickness_active = self.buffs.iter().any(|b| b.buff == "Quickness");
        let effective_cast = if quickness_active {
            (cast_time * 2 + 1) / 3
        } else {
            cast_time
        };

        // Next action = now + effective_cast + human delay
        self.next_action_ms =
            self.current_time_ms + effective_cast + HUMAN_DELAY_MS + MIN_SKILL_GAP_MS;
    }

    /// Tick all active conditions — apply damage for each stack, remove expired.
    fn tick_conditions(&mut self, condition_damage: f64) {
        // Only tick on 1-second boundaries
        if !self
            .current_time_ms
            .is_multiple_of(CONDITION_TICK_INTERVAL_MS)
        {
            return;
        }

        let mut tick_total = 0.0;
        for stack in &mut self.conditions {
            if stack.remaining_ms > 0 {
                let tick_dmg =
                    condition_tick_damage(&stack.condition, condition_damage, &self.params.mode);
                self.total_condition_damage += tick_dmg;
                tick_total += tick_dmg;
                // Skip the per-tick `stack.condition.clone()` once the key is
                // already present in the map. Hash + lookup is cheaper than
                // String allocation across thousands of ticks per sim.
                if let Some(count) = self.condition_ticks.get_mut(&stack.condition) {
                    *count += 1;
                } else {
                    self.condition_ticks.insert(stack.condition.clone(), 1);
                }
                stack.remaining_ms = stack
                    .remaining_ms
                    .saturating_sub(CONDITION_TICK_INTERVAL_MS);
            }
        }
        self.apply_dummy_damage(tick_total);

        // Remove expired
        self.conditions.retain(|s| s.remaining_ms > 0);
    }

    fn apply_dummy_damage(&mut self, damage: f64) {
        if self.downed || self.current_time_ms < self.invuln_until_ms {
            return;
        }
        let Some(hp) = self.remaining_hp.as_mut() else {
            return;
        };
        *hp -= damage;
        if *hp <= 0.0 {
            *hp = 0.0;
            self.downed = true;
            self.invuln_until_ms = self.current_time_ms.saturating_add(DOWNED_INVULN_MS);
            self.stomp_ends_ms = self.invuln_until_ms.saturating_add(STOMP_MS);
        }
    }

    /// Tick buffs — track uptime, remove expired.
    fn tick_buffs(&mut self) {
        for buff in &mut self.buffs {
            if buff.remaining_ms > 0 {
                // Avoid `buff.buff.clone()` once the buff name is interned in
                // the uptime map. Sim runs ~thousands of ticks; clone savings
                // compound across long simulations.
                if let Some(ms) = self.buff_active_ms.get_mut(&buff.buff) {
                    *ms += TICK_MS;
                } else {
                    self.buff_active_ms.insert(buff.buff.clone(), TICK_MS);
                }
                buff.remaining_ms = buff.remaining_ms.saturating_sub(TICK_MS);
            }
        }

        // Remove expired
        self.buffs.retain(|b| b.remaining_ms > 0);
    }

    /// Convert internal state into SimulationResult.
    fn into_result(self) -> SimulationResult {
        let duration_secs = self.duration_ms as f64 / 1000.0;
        let strike_dps = self.total_strike_damage / duration_secs;
        let condition_dps = self.total_condition_damage / duration_secs;

        // Average condition stacks: total ticks / duration in seconds
        let condition_uptime: HashMap<String, f64> = self
            .condition_ticks
            .iter()
            .map(|(name, ticks)| (name.clone(), *ticks as f64 / duration_secs))
            .collect();

        // Buff uptime as fraction of total duration
        let buff_uptime: HashMap<String, f64> = self
            .buff_active_ms
            .iter()
            .map(|(name, ms)| {
                let fraction = *ms as f64 / self.duration_ms as f64;
                (name.clone(), fraction.min(1.0))
            })
            .collect();

        // Per-skill usage
        let skill_usage: Vec<SkillUsage> = self
            .skills
            .iter()
            .filter_map(|s| {
                let casts = self.skill_casts.get(&s.skill_id).copied().unwrap_or(0);
                if casts == 0 {
                    return None;
                }
                let dmg = self.skill_damage.get(&s.skill_id).copied().unwrap_or(0.0);
                Some(SkillUsage {
                    name: s.name.clone(),
                    cast_count: casts,
                    dps_contribution: dmg / duration_secs,
                })
            })
            .collect();

        // Control/survivability metrics
        let stunbreak_count = self.skills.iter().filter(|s| s.is_stunbreak).count() as u32;
        let has_stability = kit_has_stability_cover(&self.skills);
        let stability_uptime = buff_uptime.get("Stability").copied().unwrap_or(0.0);

        // Cleanse metrics: count skills with ≥1 RemovesCondition effect; estimate rate per 20s.
        // Rate per 20s: for each cleanse skill, sum conditions_removed × (20s / cooldown_s).
        // Skills with cooldown=0 (auto-attacks) are excluded (would be infinite — ignore them).
        let cleanse_count = self
            .skills
            .iter()
            .filter(|s| {
                s.effects
                    .iter()
                    .any(|e| matches!(e, SkillEffect::RemovesCondition { .. }))
            })
            .count() as u32;

        let cleanse_rate_per_20s: f64 = self
            .skills
            .iter()
            .filter_map(|s| {
                let conditions_removed: u32 = s
                    .effects
                    .iter()
                    .filter_map(|e| {
                        if let SkillEffect::RemovesCondition { conditions_removed } = e {
                            Some(*conditions_removed)
                        } else {
                            None
                        }
                    })
                    .sum();
                if conditions_removed == 0 || s.cooldown_ms == 0 {
                    return None;
                }
                let cooldown_s = s.cooldown_ms as f64 / 1000.0;
                // uptime_factor = 20s / cooldown_s (capped at 1 use per cooldown)
                let casts_in_20s = 20.0 / cooldown_s;
                Some(conditions_removed as f64 * casts_in_20s)
            })
            .sum();

        SimulationResult {
            duration_ms: self.duration_ms,
            strike_dps,
            condition_dps,
            total_dps: strike_dps + condition_dps,
            condition_uptime,
            buff_uptime,
            skill_usage,
            stunbreak_count,
            has_stability,
            stability_uptime,
            cleanse_count,
            cleanse_rate_per_20s,
            has_mobility_out: kit_has_mobility_out(&self.skills),
            has_strip: kit_has_strip(&self.skills),
            has_corrupt: kit_has_corrupt(&self.skills),
            downed: self.downed,
            finished: self.downed && self.duration_ms >= self.stomp_ends_ms,
            has_interrupt: kit_has_interrupt(&self.skills),
        }
    }
}

/// Calculate damage per second of cast time (DPCT) for a skill.
///
/// This is the core metric for optimal skill scheduling: the skill with the
/// highest DPCT should always be used first when multiple are off cooldown.
///
/// Accounts for:
/// - **Strike damage**: direct weapon damage per hit
/// - **Condition damage**: total tick damage over the condition's full duration
/// - **Buff value**: estimated DPS increase from Might, Fury, Quickness
fn skill_dps_efficiency(
    skill: &RotationSkill,
    power: f64,
    condition_damage: f64,
    weapon_strength: f64,
    mode: &GameMode,
) -> f64 {
    let cast_time_s = (skill.cast_time_ms + HUMAN_DELAY_MS + MIN_SKILL_GAP_MS) as f64 / 1000.0;
    if cast_time_s <= 0.0 {
        return 0.0;
    }

    let mut total_damage_value = 0.0;

    for effect in &skill.effects {
        match effect {
            SkillEffect::StrikeDamage {
                hit_count,
                dmg_multiplier,
            } => {
                total_damage_value += weapon_strength * power / reference_armor()
                    * dmg_multiplier
                    * (*hit_count as f64);
            }
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                // Total condition damage over the full duration of all stacks
                let tick_dmg = condition_tick_damage(condition, condition_damage, mode);
                let duration_s = *duration_ms as f64 / 1000.0;
                total_damage_value += tick_dmg * (*stacks as f64) * duration_s;
            }
            SkillEffect::ApplyBuff {
                buff,
                stacks,
                duration_ms,
            } => {
                total_damage_value +=
                    estimate_buff_dps_value(buff, *stacks, *duration_ms, power, weapon_strength);
            }
            _ => {}
        }
    }

    total_damage_value / cast_time_s
}

/// Estimate the total DPS value of applying a buff.
///
/// Buffs don't deal direct damage, but they increase DPS over their duration.
/// This heuristic lets the scheduler properly value buff skills alongside
/// damage skills in the DPCT priority.
fn estimate_buff_dps_value(
    buff: &str,
    stacks: u32,
    duration_ms: u32,
    power: f64,
    weapon_strength: f64,
) -> f64 {
    let duration_s = duration_ms as f64 / 1000.0;
    match buff {
        "Might" => {
            // Might per-stack values loaded from data/formulas/boons.json.
            let b = crate::data::boons();
            let stacks_f = stacks as f64;
            let might_power = b.might_power_per_stack();
            let might_condi = b.might_condi_per_stack();
            // Power contribution: extra_power * weapon_strength / armor
            let power_value =
                might_power * stacks_f * weapon_strength / reference_armor() * duration_s;
            // Condition Damage contribution: condi_per_stack * CD_coefficient * duration.
            // Use bleeding coefficient (0.06) as a conservative typical-condition estimate.
            let condi_value = might_condi * stacks_f * 0.06 * duration_s;
            power_value + condi_value
        }
        "Fury" => {
            // Fury: mode-dependent crit chance bonus (loaded from data).
            // Using PvE default here; TODO: P3-XX thread BalanceContext through simulator.
            let base_hit = power * weapon_strength / reference_armor();
            let fury = crate::data::boons().fury_crit_bonus(GameMode::PvE);
            base_hit * fury * duration_s * (stacks.min(1) as f64)
        }
        "Quickness" => {
            // Quickness = +50% attack speed → massive DPS multiplier.
            let base_hit = power * weapon_strength / reference_armor();
            base_hit * 0.5 * duration_s * (stacks.min(1) as f64)
        }
        _ => 0.0, // Stability, Resistance, etc. don't directly increase DPS
    }
}

/// Expected-value crit multiplier. Precision 0 → 1.0 (no crit term).
fn strike_crit_factor(precision: f64, ferocity: f64) -> f64 {
    if precision <= 0.0 {
        return 1.0;
    }
    let f = crate::data::universal_formulas::formulas();
    let chance = (f.crit_chance(precision) / 100.0).clamp(0.0, 1.0);
    let crit_mult = f.crit_damage(ferocity) / 100.0;
    1.0 + chance * (crit_mult - 1.0)
}

/// Intensity-stack cap from conditions.json, with competitive 100-stack ceiling.
/// Vulnerability is 25 in every mode. Duration conditions cap at 1.
pub(crate) fn condition_stack_cap(condition: &str, mode: &GameMode) -> usize {
    let conds = crate::data::conditions();
    let canonical = crate::data::boon_condition_formulas::canonical_condition_name(condition);
    let pve_cap = conds.max_stacks(canonical).unwrap_or(1500) as usize;
    if canonical.eq_ignore_ascii_case("Vulnerability") {
        return pve_cap.min(25);
    }
    match mode {
        GameMode::PvE => pve_cap,
        GameMode::PvP | GameMode::WvW => pve_cap.min(100),
    }
}

/// Condition tick damage formula (GW2 level 80, per tick).
/// Torment uses stationary baseline; Confusion uses on_skill_use baseline.
fn condition_tick_damage(condition: &str, condition_damage: f64, mode: &GameMode) -> f64 {
    let conds = crate::data::conditions();
    let mode = mode.clone();
    match condition {
        "Torment" => conds.torment_tick(condition_damage, mode, false),
        "Confusion" => conds.confusion_tick(condition_damage, mode, true),
        _ => conds.tick_damage(condition, condition_damage, mode),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rotation::{RotationSkill, SkillEffect, SkillSlot};

    fn auto_attack() -> RotationSkill {
        RotationSkill {
            skill_id: 1,
            name: "Auto Attack".into(),
            slot: SkillSlot::Weapon1,
            cast_time_ms: 500,
            cooldown_ms: 0,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    fn weapon_skill() -> RotationSkill {
        RotationSkill {
            skill_id: 2,
            name: "Whirling Axe".into(),
            slot: SkillSlot::Weapon2,
            cast_time_ms: 750,
            cooldown_ms: 8000,
            effects: vec![
                SkillEffect::StrikeDamage {
                    hit_count: 5,
                    dmg_multiplier: 0.5,
                },
                SkillEffect::ApplyCondition {
                    condition: "Bleeding".into(),
                    stacks: 3,
                    duration_ms: 6000,
                },
            ],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    fn buff_skill() -> RotationSkill {
        RotationSkill {
            skill_id: 3,
            name: "For Great Justice!".into(),
            slot: SkillSlot::Utility,
            cast_time_ms: 250,
            cooldown_ms: 25000,
            effects: vec![SkillEffect::ApplyBuff {
                buff: "Might".into(),
                stacks: 6,
                duration_ms: 10000,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    #[test]
    fn test_simulate_auto_attack_only() {
        let skills = vec![auto_attack()];
        let result = simulate(&skills, 5000, 2000.0, 0.0, 1100.0);

        assert_eq!(result.duration_ms, 5000);
        assert!(result.strike_dps > 0.0, "Should have non-zero strike DPS");
        assert_eq!(result.condition_dps, 0.0, "No conditions in auto-attack");
        assert_eq!(result.skill_usage.len(), 1);
        assert!(result.skill_usage[0].cast_count > 0);
    }

    #[test]
    fn test_simulate_with_conditions() {
        let skills = vec![auto_attack(), weapon_skill()];
        let result = simulate(&skills, 10000, 2000.0, 1500.0, 1100.0);

        assert!(result.strike_dps > 0.0);
        assert!(
            result.condition_dps > 0.0,
            "Should have condition DPS from Bleeding"
        );
        assert!(
            result.condition_uptime.contains_key("Bleeding"),
            "Bleeding uptime should be tracked"
        );
        assert!(*result.condition_uptime.get("Bleeding").unwrap() > 0.0);
    }

    #[test]
    fn test_simulate_with_buffs() {
        let skills = vec![auto_attack(), buff_skill()];
        let result = simulate(&skills, 10000, 2000.0, 0.0, 1100.0);

        assert!(
            result.buff_uptime.contains_key("Might"),
            "Might should be tracked"
        );
        assert!(*result.buff_uptime.get("Might").unwrap() > 0.0);
    }

    #[test]
    fn test_simulate_default_duration() {
        let skills = vec![auto_attack()];
        let result = simulate(&skills, 0, 2000.0, 0.0, 1100.0);
        assert_eq!(result.duration_ms, DEFAULT_DURATION_MS);
    }

    #[test]
    fn test_condition_tick_damage_formulas() {
        // Formulas loaded from data/formulas/conditions.json (PvE default)
        let cd = 1000.0;
        let pve = GameMode::PvE;
        assert!((condition_tick_damage("Bleeding", cd, &pve) - 82.0).abs() < 0.1);
        // Burning: 0.155*1000 + 131.0 = 286.0 (L1: base=131.0)
        assert!((condition_tick_damage("Burning", cd, &pve) - 286.0).abs() < 0.1);
        assert!((condition_tick_damage("Poison", cd, &pve) - 93.5).abs() < 0.1);
        // Torment PvE stationary: 0.09*1000 + 31.8 = 121.8 (L2 verified)
        assert!((condition_tick_damage("Torment", cd, &pve) - 121.8).abs() < 0.1);
        // Confusion PvE on-skill-use: 0.0325*1000 + 16.24 = 48.74 (L3 verified)
        assert!((condition_tick_damage("Confusion", cd, &pve) - 48.74).abs() < 0.1);
        assert_eq!(condition_tick_damage("Vulnerability", cd, &pve), 0.0);
    }

    #[test]
    fn test_condition_stack_caps() {
        assert_eq!(condition_stack_cap("Bleeding", &GameMode::PvE), 1500);
        assert_eq!(condition_stack_cap("Bleeding", &GameMode::PvP), 100);
        assert_eq!(condition_stack_cap("Bleeding", &GameMode::WvW), 100);
        assert_eq!(condition_stack_cap("Vulnerability", &GameMode::PvE), 25);
        assert_eq!(condition_stack_cap("Vulnerability", &GameMode::PvP), 25);
        assert_eq!(condition_stack_cap("Burning", &GameMode::PvP), 100);
    }

    #[test]
    fn test_dpct_prefers_high_damage_skill() {
        // A high-damage weapon skill should be picked over a low-damage utility
        let high_dmg = RotationSkill {
            skill_id: 10,
            name: "Big Hit".into(),
            slot: SkillSlot::Weapon2,
            cast_time_ms: 500,
            cooldown_ms: 5000,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 3,
                dmg_multiplier: 2.0,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };
        let low_dmg = RotationSkill {
            skill_id: 11,
            name: "Weak Poke".into(),
            slot: SkillSlot::Elite, // Elite slot but low damage
            cast_time_ms: 1500,
            cooldown_ms: 30000,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 0.1,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };

        let high_dpct = skill_dps_efficiency(&high_dmg, 2000.0, 0.0, 1100.0, &GameMode::PvE);
        let low_dpct = skill_dps_efficiency(&low_dmg, 2000.0, 0.0, 1100.0, &GameMode::PvE);
        assert!(
            high_dpct > low_dpct,
            "High-damage skill ({:.1}) should have higher DPCT than slow weak skill ({:.1})",
            high_dpct,
            low_dpct
        );
    }

    #[test]
    fn test_dpct_values_condition_skills() {
        // A condition skill's DPCT should account for total lifetime damage
        let condi_skill = RotationSkill {
            skill_id: 20,
            name: "Burning Blade".into(),
            slot: SkillSlot::Weapon3,
            cast_time_ms: 500,
            cooldown_ms: 8000,
            effects: vec![SkillEffect::ApplyCondition {
                condition: "Burning".into(),
                stacks: 2,
                duration_ms: 5000,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };

        let dpct = skill_dps_efficiency(&condi_skill, 1000.0, 1500.0, 1100.0, &GameMode::PvE);
        assert!(dpct > 0.0, "Condition skill should have positive DPCT");

        // Burning at 1500 CD = 0.155*1500+131 = 363.5 per tick
        // 2 stacks * 5 seconds = 3635.0 total damage
        // cast_time = (500 + 80 + 100) / 1000 = 0.68s
        // DPCT ≈ 3635 / 0.68 ≈ 5345
        assert!(
            dpct > 5000.0,
            "Burning DPCT should be substantial: {:.1}",
            dpct
        );
    }

    #[test]
    fn test_dpct_values_buff_skills() {
        let buff = RotationSkill {
            skill_id: 30,
            name: "For Great Justice!".into(),
            slot: SkillSlot::Utility,
            cast_time_ms: 250,
            cooldown_ms: 25000,
            effects: vec![SkillEffect::ApplyBuff {
                buff: "Might".into(),
                stacks: 6,
                duration_ms: 10000,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };

        let dpct = skill_dps_efficiency(&buff, 2000.0, 0.0, 1100.0, &GameMode::PvE);
        assert!(dpct > 0.0, "Might buff should have positive DPCT value");
    }

    #[test]
    fn test_weapon_swap() {
        // Set 1: fast skill, Set 2: different fast skill, plus auto + utility
        let set1_skill = RotationSkill {
            skill_id: 100,
            name: "Axe Throw".into(),
            slot: SkillSlot::Weapon2,
            cast_time_ms: 500,
            cooldown_ms: 5000,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.5,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 1,
        };
        let set1_auto = RotationSkill {
            skill_id: 101,
            name: "Chop".into(),
            slot: SkillSlot::Weapon1,
            cast_time_ms: 500,
            cooldown_ms: 0,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 1,
        };
        let set2_skill = RotationSkill {
            skill_id: 200,
            name: "Greatsword Swing".into(),
            slot: SkillSlot::Weapon2,
            cast_time_ms: 600,
            cooldown_ms: 6000,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 2,
                dmg_multiplier: 1.2,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 2,
        };
        let set2_auto = RotationSkill {
            skill_id: 201,
            name: "GS Auto".into(),
            slot: SkillSlot::Weapon1,
            cast_time_ms: 500,
            cooldown_ms: 0,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 0.9,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 2,
        };

        let skills = vec![set1_skill, set1_auto, set2_skill, set2_auto];
        let result = simulate(&skills, 30000, 2000.0, 0.0, 1100.0);

        // Both weapon sets should be used
        let used_names: Vec<&str> = result
            .skill_usage
            .iter()
            .map(|su| su.name.as_str())
            .collect();
        assert!(
            used_names.contains(&"Axe Throw") || used_names.contains(&"Greatsword Swing"),
            "Should use skills from at least one weapon set: {:?}",
            used_names
        );
        assert!(result.total_dps > 0.0);

        // With 30s duration and 10s swap CD, should swap at least once
        // Both autos should appear (meaning both sets were active at some point)
        let has_set1 = used_names.iter().any(|n| *n == "Axe Throw" || *n == "Chop");
        let has_set2 = used_names
            .iter()
            .any(|n| *n == "Greatsword Swing" || *n == "GS Auto");
        assert!(
            has_set1 && has_set2,
            "Should use both weapon sets in 30s: {:?}",
            used_names
        );
    }

    #[test]
    fn test_full_rotation() {
        let skills = vec![auto_attack(), weapon_skill(), buff_skill()];
        let result = simulate(&skills, 30000, 2500.0, 1500.0, 1100.0);

        // Full 30s rotation should produce meaningful DPS
        assert!(result.total_dps > 100.0, "Total DPS should be non-trivial");
        assert!(result.strike_dps > 0.0);
        assert!(result.condition_dps > 0.0);

        // Should have used all skills
        assert!(
            result.skill_usage.len() >= 2,
            "Should use at least AA + weapon skill"
        );
    }

    #[test]
    fn test_no_weapon_set_backward_compat() {
        // All weapon_set=0 (legacy/untagged) — should work like before
        let skills = vec![auto_attack(), weapon_skill(), buff_skill()];
        let result = simulate(&skills, 10000, 2000.0, 1500.0, 1100.0);
        assert!(result.total_dps > 0.0);
        assert!(result.skill_usage.len() >= 2);
    }

    // ─── Cleanse detection tests ───

    fn cleanse_skill(cooldown_ms: u32, conditions: u32) -> RotationSkill {
        RotationSkill {
            skill_id: 9999,
            name: "Mending".into(),
            slot: SkillSlot::Heal,
            cast_time_ms: 750,
            cooldown_ms,
            effects: vec![SkillEffect::RemovesCondition {
                conditions_removed: conditions,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    #[test]
    fn test_cleanse_count_zero_without_cleanse_skill() {
        // No cleanse skill → cleanse_count == 0, cleanse_rate_per_20s == 0.0
        let skills = vec![auto_attack()];
        let result = simulate(&skills, 5000, 2000.0, 0.0, 1100.0);
        assert_eq!(result.cleanse_count, 0);
        assert_eq!(result.cleanse_rate_per_20s, 0.0);
    }

    #[test]
    fn test_cleanse_count_detects_cleanse_skill() {
        // One cleanse skill with 20s CD removing 3 conditions.
        let skills = vec![auto_attack(), cleanse_skill(20000, 3)];
        let result = simulate(&skills, 5000, 2000.0, 0.0, 1100.0);
        assert_eq!(result.cleanse_count, 1, "one skill has cleanse effect");
        // 3 conditions × (20s / 20s) = 3.0 per 20s
        assert!(
            (result.cleanse_rate_per_20s - 3.0).abs() < 0.01,
            "cleanse_rate_per_20s should be ~3.0, got {}",
            result.cleanse_rate_per_20s
        );
    }

    #[test]
    fn test_cleanse_rate_scales_with_cooldown() {
        // 10s CD, 2 conditions → 2 × (20/10) = 4.0 per 20s
        let skills = vec![auto_attack(), cleanse_skill(10000, 2)];
        let result = simulate(&skills, 5000, 2000.0, 0.0, 1100.0);
        assert_eq!(result.cleanse_count, 1);
        assert!(
            (result.cleanse_rate_per_20s - 4.0).abs() < 0.01,
            "cleanse_rate_per_20s should be ~4.0, got {}",
            result.cleanse_rate_per_20s
        );
    }

    #[test]
    fn test_cleanse_count_multiple_cleanse_skills() {
        // Two cleanse skills → cleanse_count = 2
        let mut second_cleanse = cleanse_skill(30000, 1);
        second_cleanse.skill_id = 9998;
        let skills = vec![auto_attack(), cleanse_skill(20000, 2), second_cleanse];
        let result = simulate(&skills, 5000, 2000.0, 0.0, 1100.0);
        assert_eq!(result.cleanse_count, 2, "both cleanse skills counted");
        // 2×(20/20) + 1×(20/30) ≈ 2.0 + 0.667 = 2.667
        let expected = 2.0 + 20.0_f64 / 30.0;
        assert!(
            (result.cleanse_rate_per_20s - expected).abs() < 0.01,
            "cleanse_rate_per_20s should be ~{:.3}, got {}",
            expected,
            result.cleanse_rate_per_20s
        );
    }

    #[test]
    fn test_cleanse_auto_attack_excluded_from_rate() {
        // A cleanse on an auto-attack (cooldown=0) should not blow up the rate.
        // The rate calculation skips skills with cooldown=0.
        let auto_with_cleanse = RotationSkill {
            skill_id: 1,
            name: "Cleansing Auto".into(),
            slot: SkillSlot::Weapon1,
            cast_time_ms: 500,
            cooldown_ms: 0,
            effects: vec![
                SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 1.0,
                },
                SkillEffect::RemovesCondition {
                    conditions_removed: 1,
                },
            ],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };
        let result = simulate(&[auto_with_cleanse], 5000, 2000.0, 0.0, 1100.0);
        // cleanse_count = 1 (skill has cleanse effect), but rate = 0 (cooldown=0 excluded)
        assert_eq!(
            result.cleanse_count, 1,
            "cleanse_count counts the auto-attack"
        );
        assert_eq!(
            result.cleanse_rate_per_20s, 0.0,
            "rate excludes auto-attacks to avoid division by zero"
        );
    }

    #[test]
    fn protection_dummy_cuts_strike_by_a_third() {
        let skills = vec![auto_attack()];
        let open = simulate_against(&skills, 2000, 2000.0, 0.0, 1100.0, EnemyDummy::open());
        let prot = simulate_against(
            &skills,
            2000,
            2000.0,
            0.0,
            1100.0,
            EnemyDummy {
                protection: true,
                stability: true,
                hp: None,
            },
        );
        let ratio = prot.strike_dps / open.strike_dps;
        assert!((ratio - 0.67).abs() < 0.02, "expected ~0.67, got {ratio}");
    }

    #[test]
    fn strip_clears_protection_so_later_hits_are_full() {
        let strip = RotationSkill {
            skill_id: 90,
            name: "Strip".into(),
            slot: SkillSlot::Utility,
            cast_time_ms: 250,
            cooldown_ms: 10_000,
            effects: vec![SkillEffect::StripBoons {
                count_per_pulse: 1,
                interval_ms: 1000,
                window_ms: 1000,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };
        let dummy = EnemyDummy {
            protection: true,
            stability: true,
            hp: None,
        };
        let with_strip =
            simulate_against(&[auto_attack(), strip], 2000, 2000.0, 0.0, 1100.0, dummy);
        let no_strip = simulate_against(&[auto_attack()], 2000, 2000.0, 0.0, 1100.0, dummy);
        assert!(
            with_strip.strike_dps > no_strip.strike_dps,
            "strip should raise delivered DPS vs a Protection dummy"
        );
    }

    #[test]
    fn dummy_hp_downs_then_stomps_after_invuln() {
        let burst = RotationSkill {
            skill_id: 99,
            name: "Burst".into(),
            slot: SkillSlot::Weapon2,
            cast_time_ms: 250,
            cooldown_ms: 10_000,
            effects: vec![SkillEffect::StrikeDamage {
                hit_count: 20,
                dmg_multiplier: 10.0,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };
        let dummy = EnemyDummy {
            hp: Some(500.0),
            ..EnemyDummy::open()
        };
        let short = simulate_against(
            &[auto_attack(), burst.clone()],
            2_000,
            2500.0,
            0.0,
            1100.0,
            dummy,
        );
        assert!(short.downed, "500 HP dummy should drop in a 2s burst");
        assert!(
            !short.finished,
            "2s window cannot fit 1s invuln + 3.5s stomp"
        );

        let long = simulate_against(&[auto_attack(), burst], 10_000, 2500.0, 0.0, 1100.0, dummy);
        assert!(long.downed);
        assert!(long.finished, "10s window covers stomp after invuln");
    }

    #[test]
    fn open_dummy_does_not_track_downstate() {
        let result = simulate(&[auto_attack()], 5_000, 2000.0, 0.0, 1100.0);
        assert!(!result.downed);
        assert!(!result.finished);
    }

    #[test]
    fn crowd_control_sets_has_interrupt() {
        let cc = RotationSkill {
            skill_id: 7,
            name: "Daze".into(),
            slot: SkillSlot::Utility,
            cast_time_ms: 0,
            cooldown_ms: 10_000,
            effects: vec![SkillEffect::CrowdControl {
                kind: crate::rotation::ControlKind::Daze,
                duration_ms: 500,
                stops_dodge: false,
            }],
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        };
        let result = simulate(&[auto_attack(), cc], 2_000, 2000.0, 0.0, 1100.0);
        assert!(result.has_interrupt);
        assert!(!simulate(&[auto_attack()], 2_000, 2000.0, 0.0, 1100.0).has_interrupt);
    }
}
