//! Rotation simulator for GW2 builds.
//! Simulates a time-step skill rotation to estimate real DPS, condition uptime,
//! and buff uptime — validating AI build reasoning with concrete numbers.

pub mod builder;
pub mod combat_model;
pub mod simulator;
pub mod skill_timings;

use std::collections::HashMap;

/// A skill prepared for rotation simulation, with all timing and effect data extracted.
#[derive(Debug, Clone)]
pub struct RotationSkill {
    pub skill_id: u32,
    pub name: String,
    pub slot: SkillSlot,
    /// Cast time in milliseconds (animation lock).
    pub cast_time_ms: u32,
    /// Cooldown in milliseconds.
    pub cooldown_ms: u32,
    /// All effects this skill applies.
    pub effects: Vec<SkillEffect>,
    /// Next skill in auto-attack chain (if any).
    pub next_chain: Option<u32>,
    /// Whether this skill is a stunbreak.
    pub is_stunbreak: bool,
    /// Weapon set this skill belongs to (0=always available, 1=set1, 2=set2).
    /// Non-weapon skills (heal/utility/elite) use 0.
    pub weapon_set: u8,
}

/// Skill slot classification — determines priority and auto-attack behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkillSlot {
    Weapon1,
    Weapon2,
    Weapon3,
    Weapon4,
    Weapon5,
    Heal,
    Utility,
    Elite,
    Profession,
}

impl SkillSlot {
    /// Parse from GW2 API slot string.
    pub fn from_api(slot: &str) -> Option<Self> {
        match slot {
            "Weapon_1" => Some(Self::Weapon1),
            "Weapon_2" => Some(Self::Weapon2),
            "Weapon_3" => Some(Self::Weapon3),
            "Weapon_4" => Some(Self::Weapon4),
            "Weapon_5" => Some(Self::Weapon5),
            "Heal" => Some(Self::Heal),
            "Utility" => Some(Self::Utility),
            "Elite" => Some(Self::Elite),
            s if s.starts_with("Profession_") => Some(Self::Profession),
            _ => None,
        }
    }

    /// Is this a weapon skill (auto-attackable)?
    pub fn is_weapon(&self) -> bool {
        matches!(
            self,
            Self::Weapon1 | Self::Weapon2 | Self::Weapon3 | Self::Weapon4 | Self::Weapon5
        )
    }
}

/// Hard control vs interrupt-only. `stops_dodge` is the lock/not-lock split:
/// Daze interrupts casts but does not stop dodges; Immobilize is a condition
/// that does stop dodges (wiki Control effect / Dodge).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Stun,
    Knockdown,
    Launch,
    Knockback,
    Pull,
    Fear,
    Taunt,
    Daze,
    Float,
    Sink,
    Immobilize,
}

/// Cover that can eat the alpha answer. Boons are strippable; true invuln is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverKind {
    Invulnerability,
    Stealth,
    Aegis,
    Stability,
    /// Suppresses nondamaging condition *effects* (Immobile, Fear, Taunt, …).
    /// Condi damage still ticks. Poison heal-reduction and Terror are exempt.
    /// Corrupts into Chill (wiki Resistance, fetched 2026-08-14).
    Resistance,
    Protection,
    Blind,
    Block,
}

/// Roam "out" — at least one of these is the mobility gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MobilityKind {
    Teleport,
    Stealth,
    Superspeed,
    Leap,
}

/// An effect that a skill produces when used.
#[derive(Debug, Clone, PartialEq)]
pub enum SkillEffect {
    /// Direct strike damage.
    StrikeDamage {
        hit_count: u32,
        dmg_multiplier: f64,
    },
    /// Applies a damaging condition to the target.
    ApplyCondition {
        condition: String,
        stacks: u32,
        duration_ms: u32,
    },
    /// Applies a boon or self-buff.
    ApplyBuff {
        buff: String,
        stacks: u32,
        duration_ms: u32,
    },
    /// Combo field placement.
    ComboField {
        field_type: String,
    },
    /// Removes one or more conditions from self (cleanse).
    /// `conditions_removed` is the number of conditions removed per use.
    RemovesCondition {
        conditions_removed: u32,
    },
    /// Crowd control. `stops_dodge` false = interrupt only (Daze).
    CrowdControl {
        kind: ControlKind,
        duration_ms: u32,
        stops_dodge: bool,
    },
    /// Boon strip. Pair Number "Boons Removed" with Time Interval/Pulse/Duration.
    /// WoD: count_per_pulse=1, interval_ms=1000, window_ms=5000 → 5 strips, not 1.
    StripBoons {
        count_per_pulse: u32,
        interval_ms: u32,
        window_ms: u32,
    },
    /// Boon → condition. Often missing from API facts; parsed from description.
    CorruptBoons,
    /// Condition → boon (convert cleanse).
    ConvertConditions,
    /// Boon steal / transfer to self.
    StealBoons,
    Cover {
        kind: CoverKind,
        duration_ms: u32,
        strippable: bool,
    },
    Mobility {
        kind: MobilityKind,
    },
}

/// Total boons removed over a strip effect's window (not the per-pulse Number).
pub fn strip_total(effect: &SkillEffect) -> u32 {
    match effect {
        SkillEffect::StripBoons {
            count_per_pulse,
            interval_ms,
            window_ms,
        } => {
            if *interval_ms == 0 {
                *count_per_pulse
            } else {
                let window = (*window_ms).max(*interval_ms);
                *count_per_pulse * (window / *interval_ms)
            }
        }
        _ => 0,
    }
}

/// Full simulation result from running a rotation.
///
/// Design philosophy: Pure damage output is NOT the only goal.
/// Build quality = ability to DELIVER damage (DPS * uptime * control).
/// A build that can CC the enemy, maintain stability, and survive
/// delivers more REAL damage than a glass cannon that gets interrupted.
#[derive(Debug, Clone)]
pub struct SimulationResult {
    /// Duration of the simulation in milliseconds.
    pub duration_ms: u32,
    /// Estimated strike DPS (direct damage per second).
    pub strike_dps: f64,
    /// Estimated condition DPS from tick damage.
    pub condition_dps: f64,
    /// Total DPS (strike + condition).
    pub total_dps: f64,
    /// Average condition stacks over the simulation.
    pub condition_uptime: HashMap<String, f64>,
    /// Boon uptime as fraction (0.0 to 1.0).
    pub buff_uptime: HashMap<String, f64>,
    /// Per-skill usage: (name, cast_count, dps_contribution).
    pub skill_usage: Vec<SkillUsage>,
    /// Number of stunbreak skills available in the build.
    pub stunbreak_count: u32,
    /// Whether the build has access to Stability (from skills or traits).
    pub has_stability: bool,
    /// Estimated self-Stability uptime (fraction 0.0 to 1.0).
    pub stability_uptime: f64,
    /// Number of equipped skills that have at least one cleanse effect.
    pub cleanse_count: u32,
    /// Estimated conditions removed per 20 seconds (sum of conditions_removed × uptime_factor).
    pub cleanse_rate_per_20s: f64,
    /// Kit has teleport/stealth/superspeed/leap (roam out gate).
    pub has_mobility_out: bool,
    /// Kit has strip, steal, or corrupt (harasser cover-crack gate).
    pub has_strip: bool,
    /// Kit converts enemy boons to conditions (disabler identity).
    pub has_corrupt: bool,
}

/// Per-skill breakdown in a simulation result.
#[derive(Debug, Clone)]
pub struct SkillUsage {
    pub name: String,
    pub cast_count: u32,
    pub dps_contribution: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skill_slot_from_api() {
        assert_eq!(SkillSlot::from_api("Weapon_1"), Some(SkillSlot::Weapon1));
        assert_eq!(SkillSlot::from_api("Heal"), Some(SkillSlot::Heal));
        assert_eq!(
            SkillSlot::from_api("Profession_1"),
            Some(SkillSlot::Profession)
        );
        assert_eq!(SkillSlot::from_api("Unknown"), None);
    }

    #[test]
    fn test_skill_slot_is_weapon() {
        assert!(SkillSlot::Weapon1.is_weapon());
        assert!(SkillSlot::Weapon5.is_weapon());
        assert!(!SkillSlot::Heal.is_weapon());
        assert!(!SkillSlot::Elite.is_weapon());
    }
}
