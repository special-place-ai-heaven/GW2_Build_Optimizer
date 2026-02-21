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

use super::skill_timings::{HUMAN_DELAY_MS, MIN_SKILL_GAP_MS};
use super::{RotationSkill, SimulationResult, SkillEffect, SkillSlot, SkillUsage};

/// Tick resolution for the simulation (ms per step).
const TICK_MS: u32 = 100;

/// Default simulation duration (30 seconds — standard GW2 benchmark window).
pub const DEFAULT_DURATION_MS: u32 = 30_000;

/// GW2 weapon swap cooldown (10 seconds in-combat).
const WEAPON_SWAP_COOLDOWN_MS: u32 = 10_000;

/// Conditions tick every 1 second.
const CONDITION_TICK_INTERVAL_MS: u32 = 1000;

/// Reference armor value for damage calculation.
const REFERENCE_ARMOR: f64 = 2597.0;

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
    let duration = if duration_ms == 0 {
        DEFAULT_DURATION_MS
    } else {
        duration_ms
    };

    let mut sim = SimState::new(skills, duration);
    sim.run(power, condition_damage, weapon_strength);
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
    has_weapon_sets: bool, // true if any skill has weapon_set > 0

    // Tracking
    conditions: Vec<ConditionStack>,
    buffs: Vec<BuffInstance>,
    total_strike_damage: f64,
    total_condition_damage: f64,
    skill_casts: HashMap<u32, u32>,       // skill_id → cast count
    skill_damage: HashMap<u32, f64>,      // skill_id → total damage
    condition_ticks: HashMap<String, u32>, // condition → total ticks
    buff_active_ms: HashMap<String, u32>, // buff → total ms active
}

impl SimState {
    fn new(skills: &[RotationSkill], duration_ms: u32) -> Self {
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
            conditions: Vec::new(),
            buffs: Vec::new(),
            total_strike_damage: 0.0,
            total_condition_damage: 0.0,
            skill_casts: HashMap::new(),
            skill_damage: HashMap::new(),
            condition_ticks: HashMap::new(),
            buff_active_ms: HashMap::new(),
        }
    }

    fn run(&mut self, power: f64, condition_damage: f64, weapon_strength: f64) {
        while self.current_time_ms < self.duration_ms {
            // Tick conditions and buffs
            self.tick_conditions(condition_damage);
            self.tick_buffs();

            // Reduce all cooldowns by TICK_MS
            for state in &mut self.skill_states {
                state.cooldown_remaining_ms = state.cooldown_remaining_ms.saturating_sub(TICK_MS);
            }
            self.weapon_swap_cooldown_ms = self.weapon_swap_cooldown_ms.saturating_sub(TICK_MS);

            // Try to use a skill if the character is free
            if self.current_time_ms >= self.next_action_ms {
                // Check if weapon swap would be beneficial
                if self.has_weapon_sets && self.should_weapon_swap(power, condition_damage, weapon_strength) {
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

            // Auto-attack = filler (always available, pick last)
            if skill.slot == SkillSlot::Weapon1 && skill.cooldown_ms == 0 {
                if filler_idx.is_none() {
                    filler_idx = Some(i);
                }
                continue;
            }

            let dpct = skill_dps_efficiency(skill, power, condition_damage, weapon_strength);
            if dpct > best_dpct {
                best_dpct = dpct;
                best_idx = Some(i);
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
                && skill_dps_efficiency(s, power, condition_damage, weapon_strength) > 0.0
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
                    let damage =
                        weapon_strength * power / REFERENCE_ARMOR * dmg_multiplier * (*hit_count as f64);
                    self.total_strike_damage += damage;
                    *self.skill_damage.entry(skill_id).or_insert(0.0) += damage;
                }
                SkillEffect::ApplyCondition {
                    condition,
                    stacks,
                    duration_ms,
                } => {
                    for _ in 0..*stacks {
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
            }
        }

        // Set cooldown (0 for auto-attacks)
        self.skill_states[idx].cooldown_remaining_ms = cooldown;

        // Next action = now + cast_time + human delay
        self.next_action_ms = self.current_time_ms + cast_time + HUMAN_DELAY_MS + MIN_SKILL_GAP_MS;
    }

    /// Tick all active conditions — apply damage for each stack, remove expired.
    fn tick_conditions(&mut self, condition_damage: f64) {
        // Only tick on 1-second boundaries
        if self.current_time_ms % CONDITION_TICK_INTERVAL_MS != 0 {
            return;
        }

        for stack in &mut self.conditions {
            if stack.remaining_ms > 0 {
                let tick_dmg = condition_tick_damage(&stack.condition, condition_damage);
                self.total_condition_damage += tick_dmg;
                *self
                    .condition_ticks
                    .entry(stack.condition.clone())
                    .or_insert(0) += 1;
                stack.remaining_ms = stack.remaining_ms.saturating_sub(CONDITION_TICK_INTERVAL_MS);
            }
        }

        // Remove expired
        self.conditions.retain(|s| s.remaining_ms > 0);
    }

    /// Tick buffs — track uptime, remove expired.
    fn tick_buffs(&mut self) {
        for buff in &mut self.buffs {
            if buff.remaining_ms > 0 {
                *self
                    .buff_active_ms
                    .entry(buff.buff.clone())
                    .or_insert(0) += TICK_MS;
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
        let has_stability = self
            .skills
            .iter()
            .any(|s| {
                s.effects.iter().any(|e| matches!(e, SkillEffect::ApplyBuff { buff, .. } if buff == "Stability"))
            });
        let stability_uptime = buff_uptime.get("Stability").copied().unwrap_or(0.0);

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
                total_damage_value +=
                    weapon_strength * power / REFERENCE_ARMOR * dmg_multiplier * (*hit_count as f64);
            }
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                // Total condition damage over the full duration of all stacks
                let tick_dmg = condition_tick_damage(condition, condition_damage);
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
            // Each Might stack = +30 power, translated to extra DPS over buff duration.
            let extra_power = 30.0 * stacks as f64;
            extra_power * weapon_strength / REFERENCE_ARMOR * duration_s
        }
        "Fury" => {
            // Fury = +25% crit chance → roughly +15% DPS for the duration.
            let base_hit = power * weapon_strength / REFERENCE_ARMOR;
            base_hit * 0.15 * duration_s * (stacks.min(1) as f64)
        }
        "Quickness" => {
            // Quickness = +50% attack speed → massive DPS multiplier.
            let base_hit = power * weapon_strength / REFERENCE_ARMOR;
            base_hit * 0.5 * duration_s * (stacks.min(1) as f64)
        }
        _ => 0.0, // Stability, Resistance, etc. don't directly increase DPS
    }
}

/// Condition tick damage formula (GW2 level 80, per tick).
/// Uses the same formulas as combat.rs.
fn condition_tick_damage(condition: &str, condition_damage: f64) -> f64 {
    match condition {
        "Bleeding" => 0.06 * condition_damage + 22.0,
        "Burning" => 0.155 * condition_damage + 131.0,
        "Poison" => 0.06 * condition_damage + 33.5,
        "Torment" => 0.06 * condition_damage + 22.0,
        "Confusion" => 0.195 * condition_damage + 95.5,
        _ => 0.0,
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
        assert!(result.condition_dps > 0.0, "Should have condition DPS from Bleeding");
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
        let cd = 1000.0;
        assert!((condition_tick_damage("Bleeding", cd) - 82.0).abs() < 0.1);
        assert!((condition_tick_damage("Burning", cd) - 286.0).abs() < 0.1);
        assert!((condition_tick_damage("Poison", cd) - 93.5).abs() < 0.1);
        assert!((condition_tick_damage("Torment", cd) - 82.0).abs() < 0.1);
        assert!((condition_tick_damage("Confusion", cd) - 290.5).abs() < 0.1);
        assert_eq!(condition_tick_damage("Vulnerability", cd), 0.0);
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

        let high_dpct = skill_dps_efficiency(&high_dmg, 2000.0, 0.0, 1100.0);
        let low_dpct = skill_dps_efficiency(&low_dmg, 2000.0, 0.0, 1100.0);
        assert!(
            high_dpct > low_dpct,
            "High-damage skill ({:.1}) should have higher DPCT than slow weak skill ({:.1})",
            high_dpct, low_dpct
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

        let dpct = skill_dps_efficiency(&condi_skill, 1000.0, 1500.0, 1100.0);
        assert!(dpct > 0.0, "Condition skill should have positive DPCT");

        // Burning at 1500 CD = 0.155*1500+131 = 363.5 per tick
        // 2 stacks * 5 seconds = 3635.0 total damage
        // cast_time = (500 + 80 + 100) / 1000 = 0.68s
        // DPCT ≈ 3635 / 0.68 ≈ 5345
        assert!(dpct > 5000.0, "Burning DPCT should be substantial: {:.1}", dpct);
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

        let dpct = skill_dps_efficiency(&buff, 2000.0, 0.0, 1100.0);
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
        let used_names: Vec<&str> = result.skill_usage.iter().map(|su| su.name.as_str()).collect();
        assert!(
            used_names.contains(&"Axe Throw") || used_names.contains(&"Greatsword Swing"),
            "Should use skills from at least one weapon set: {:?}",
            used_names
        );
        assert!(result.total_dps > 0.0);

        // With 30s duration and 10s swap CD, should swap at least once
        // Both autos should appear (meaning both sets were active at some point)
        let has_set1 = used_names.iter().any(|n| *n == "Axe Throw" || *n == "Chop");
        let has_set2 = used_names.iter().any(|n| *n == "Greatsword Swing" || *n == "GS Auto");
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
        assert!(result.skill_usage.len() >= 2, "Should use at least AA + weapon skill");
    }

    #[test]
    fn test_no_weapon_set_backward_compat() {
        // All weapon_set=0 (legacy/untagged) — should work like before
        let skills = vec![auto_attack(), weapon_skill(), buff_skill()];
        let result = simulate(&skills, 10000, 2000.0, 1500.0, 1100.0);
        assert!(result.total_dps > 0.0);
        assert!(result.skill_usage.len() >= 2);
    }
}
