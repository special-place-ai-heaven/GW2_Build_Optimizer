//! Time-step rotation simulator.
//! Runs a priority-based skill rotation over a configurable duration,
//! tracking strike damage, condition stacks, buff uptime, and skill usage.
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

/// Condition DPS formulas (per tick, level 80).
/// These are base tick values; actual damage is: tick_base + condition_damage * scaling.
/// We use reference values here (assuming the combat model provides the real numbers).
const CONDITION_TICK_INTERVAL_MS: u32 = 1000; // conditions tick every 1 second

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
    /// Current position in auto-attack chain (for Weapon1 chain skills).
    _chain_position: u32,
}

/// Run a rotation simulation with the given skills and parameters.
///
/// `condition_damage` and `power` are needed to convert condition stacks and
/// strike hits into actual DPS estimates.
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
                _chain_position: 0,
            })
            .collect();

        Self {
            skills: skills.to_vec(),
            skill_states,
            duration_ms,
            current_time_ms: 0,
            next_action_ms: 0,
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

            // Try to use a skill if the character is free
            if self.current_time_ms >= self.next_action_ms {
                if let Some(idx) = self.pick_skill() {
                    self.use_skill(idx, power, weapon_strength);
                }
            }

            self.current_time_ms += TICK_MS;
        }
    }

    /// Pick the highest-priority skill that is off cooldown.
    /// Priority order: Elite > Utility > Profession > Weapon2-5 > Weapon1 (auto-attack fallback).
    fn pick_skill(&self) -> Option<usize> {
        let mut best_idx = None;
        let mut best_priority = -1i32;

        for (i, skill) in self.skills.iter().enumerate() {
            if self.skill_states[i].cooldown_remaining_ms > 0 {
                continue;
            }

            let priority = slot_priority(skill.slot);
            if priority > best_priority {
                best_priority = priority;
                best_idx = Some(i);
            }
        }

        best_idx
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
                    // Simplified strike damage: weapon_strength * power / armor_reference * multiplier * hits
                    let damage =
                        weapon_strength * power / 2597.0 * dmg_multiplier * (*hit_count as f64);
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
                    // Combo fields tracked but not simulated for damage (would need finisher pairing)
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

/// Priority for skill slot — higher = used first when multiple are off cooldown.
/// Elite and utility skills are highest priority (use on cooldown for max value).
/// Weapon1 (auto-attack) is lowest — used as filler.
fn slot_priority(slot: SkillSlot) -> i32 {
    match slot {
        SkillSlot::Elite => 100,
        SkillSlot::Profession => 80,
        SkillSlot::Utility => 70,
        SkillSlot::Weapon5 => 50,
        SkillSlot::Weapon4 => 45,
        SkillSlot::Weapon3 => 40,
        SkillSlot::Weapon2 => 35,
        SkillSlot::Heal => 20, // heal used reactively, low sim priority
        SkillSlot::Weapon1 => 0, // auto-attack filler
    }
}

/// Condition tick damage formula (GW2 level 80, per tick).
/// Uses the same formulas as combat.rs but simplified for simulation.
fn condition_tick_damage(condition: &str, condition_damage: f64) -> f64 {
    match condition {
        "Bleeding" => 0.06 * condition_damage + 22.0,
        "Burning" => 0.155 * condition_damage + 131.0,
        "Poison" => 0.06 * condition_damage + 33.5,
        "Torment" => 0.06 * condition_damage + 22.0,
        "Confusion" => 0.0725 * condition_damage + 49.5, // passive tick only
        _ => 0.0, // non-damaging conditions don't contribute DPS
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
        assert_eq!(condition_tick_damage("Vulnerability", cd), 0.0); // non-damaging
    }

    #[test]
    fn test_slot_priority_ordering() {
        assert!(slot_priority(SkillSlot::Elite) > slot_priority(SkillSlot::Utility));
        assert!(slot_priority(SkillSlot::Utility) > slot_priority(SkillSlot::Weapon2));
        assert!(slot_priority(SkillSlot::Weapon2) > slot_priority(SkillSlot::Weapon1));
        assert!(slot_priority(SkillSlot::Weapon1) < slot_priority(SkillSlot::Heal));
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
}
