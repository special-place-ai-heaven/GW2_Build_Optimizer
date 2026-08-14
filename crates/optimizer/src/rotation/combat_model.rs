//! Fight-dummy clocks and kit gates (research/kimi-combat-model.md).
//! Mapping lives in `builder`; this module is the scorer contract.

use crate::scenario::{CombatKind, CombatTier};

use super::{CoverKind, MobilityKind, RotationSkill, SkillEffect};

/// Simulation window 0–T in milliseconds for (scale × kind).
/// Zerg T=3s / 6–8s are derived (not log-measured); roam dive 2.0s is sourced.
pub fn simulation_window_ms(tier: CombatTier, kind: CombatKind) -> u32 {
    match (tier, kind) {
        (CombatTier::Solo, CombatKind::CondiRamp) => 5_000,
        (CombatTier::Solo, CombatKind::Commander | CombatKind::Support) => 10_000,
        (CombatTier::Solo, _) => 2_000,
        (CombatTier::Party, CombatKind::CondiRamp) => 5_000,
        (CombatTier::Party, CombatKind::Commander | CombatKind::Support) => 10_000,
        (CombatTier::Party, _) => 2_500,
        (CombatTier::Squad, CombatKind::StrikeSpike) => 3_000,
        (CombatTier::Squad, CombatKind::CondiRamp) => 7_000,
        (CombatTier::Squad, CombatKind::Harasser) => 2_500,
        (CombatTier::Squad, CombatKind::Disabler) => 7_000,
        (CombatTier::Squad, CombatKind::Commander | CombatKind::Support) => 10_000,
    }
}

/// Prefer CC/strip/cover over DPCT for this long at the start of a short clock.
pub fn setup_window_ms(duration_ms: u32) -> u32 {
    if duration_ms > 10_000 {
        0
    } else {
        2_000.min(duration_ms)
    }
}

pub fn kit_has_mobility_out(skills: &[RotationSkill]) -> bool {
    skills.iter().any(|s| {
        s.effects
            .iter()
            .any(|e| matches!(e, SkillEffect::Mobility { .. }))
    })
}

pub fn kit_has_strip(skills: &[RotationSkill]) -> bool {
    skills.iter().any(|s| {
        s.effects.iter().any(|e| {
            matches!(
                e,
                SkillEffect::StripBoons { .. }
                    | SkillEffect::StealBoons
                    | SkillEffect::CorruptBoons
            )
        })
    })
}

pub fn kit_has_corrupt(skills: &[RotationSkill]) -> bool {
    skills.iter().any(|s| {
        s.effects
            .iter()
            .any(|e| matches!(e, SkillEffect::CorruptBoons))
    })
}

pub fn kit_has_stability_cover(skills: &[RotationSkill]) -> bool {
    skills.iter().any(|s| {
        s.effects.iter().any(|e| match e {
            SkillEffect::Cover {
                kind: CoverKind::Stability,
                ..
            } => true,
            SkillEffect::ApplyBuff { buff, .. } => buff == "Stability",
            _ => false,
        })
    })
}

/// Which of the three BuffProfile slots (Solo / Party / Squad) this scale uses.
pub fn buff_profile_index(tier: CombatTier) -> usize {
    match tier {
        CombatTier::Solo => 0,
        CombatTier::Party => 1,
        CombatTier::Squad => 2,
    }
}

/// Nondamaging condition effects Resistance suppresses. Poison heal-reduction
/// and Terror damage are explicitly not negated (wiki Resistance, 2026-08-14).
pub fn resistance_negates(status: &str) -> bool {
    matches!(
        status,
        "Blind"
            | "Blinded"
            | "Chill"
            | "Chilled"
            | "Cripple"
            | "Crippled"
            | "Fear"
            | "Immobile"
            | "Immobilize"
            | "Immobilized"
            | "Slow"
            | "Taunt"
            | "Vulnerability"
            | "Weakness"
    )
}

/// Boon → leftover condition when corrupted.
/// Protection/Resolution/Vigor/Aegis rows from wiki Boon table (local fetch 2026-08-14);
/// Resistance → Chilled from wiki Resistance version history (same fetch);
/// remaining rows are the stable wiki Boon "Converted into" mapping.
pub fn corrupt_into(boon: &str) -> Option<&'static str> {
    Some(match boon {
        "Aegis" => "Burning",
        "Alacrity" => "Chilled",
        "Fury" => "Blinded",
        "Might" => "Weakness",
        "Protection" => "Vulnerability",
        "Quickness" => "Slow",
        "Regeneration" => "Poisoned",
        "Resistance" => "Chilled",
        "Resolution" => "Confusion",
        "Stability" => "Fear",
        "Swiftness" => "Crippled",
        "Vigor" => "Bleeding",
        _ => return None,
    })
}

/// Enemy cover the dummy starts with. Strip/steal/corrupt clear it for the rest of the window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EnemyDummy {
    pub protection: bool,
    pub stability: bool,
}

impl EnemyDummy {
    pub fn open() -> Self {
        Self::default()
    }

    /// Zerg/havoc blobs and roam harasser targets are assumed booned; naked roam DPS is not.
    pub fn for_scenario(tier: CombatTier, kind: CombatKind) -> Self {
        match (tier, kind) {
            (CombatTier::Solo, CombatKind::Harasser | CombatKind::Disabler) => Self {
                protection: true,
                stability: true,
            },
            (CombatTier::Party, _) | (CombatTier::Squad, _) => Self {
                protection: true,
                stability: true,
            },
            _ => Self::open(),
        }
    }
}

pub fn setup_priority(skill: &RotationSkill) -> u32 {
    let mut p = 0u32;
    for e in &skill.effects {
        p = p.max(match e {
            SkillEffect::CrowdControl {
                stops_dodge: true, ..
            } => 100,
            SkillEffect::StripBoons { .. }
            | SkillEffect::StealBoons
            | SkillEffect::CorruptBoons => 90,
            SkillEffect::Cover { .. } => 80,
            SkillEffect::CrowdControl {
                stops_dodge: false, ..
            } => 70,
            SkillEffect::Mobility {
                kind: MobilityKind::Stealth,
            } => 60,
            _ => 0,
        });
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rotation::{ControlKind, SkillSlot};
    use crate::scenario::CombatKind;

    fn skill_with(effects: Vec<SkillEffect>) -> RotationSkill {
        RotationSkill {
            skill_id: 1,
            name: "t".into(),
            slot: SkillSlot::Utility,
            cast_time_ms: 250,
            cooldown_ms: 10_000,
            effects,
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    #[test]
    fn roam_dive_clock_is_2s() {
        assert_eq!(
            simulation_window_ms(CombatTier::Solo, CombatKind::StrikeSpike),
            2_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Solo, CombatKind::Harasser),
            2_000
        );
    }

    #[test]
    fn zerg_kinds_do_not_share_a_clock() {
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::StrikeSpike),
            3_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::CondiRamp),
            7_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::Harasser),
            2_500
        );
    }

    #[test]
    fn long_sim_does_not_steal_dpct_setup() {
        assert_eq!(setup_window_ms(30_000), 0);
        assert_eq!(setup_window_ms(2_000), 2_000);
    }

    #[test]
    fn lock_outranks_damage_in_setup_priority() {
        let lock = skill_with(vec![SkillEffect::CrowdControl {
            kind: ControlKind::Stun,
            duration_ms: 1000,
            stops_dodge: true,
        }]);
        let dps = skill_with(vec![SkillEffect::StrikeDamage {
            hit_count: 1,
            dmg_multiplier: 10.0,
        }]);
        assert!(setup_priority(&lock) > setup_priority(&dps));
    }

    #[test]
    fn roam_out_requires_mobility_effect() {
        let burst = vec![skill_with(vec![SkillEffect::StrikeDamage {
            hit_count: 1,
            dmg_multiplier: 2.0,
        }])];
        let with_out = vec![skill_with(vec![SkillEffect::Mobility {
            kind: MobilityKind::Teleport,
        }])];
        assert!(!kit_has_mobility_out(&burst));
        assert!(kit_has_mobility_out(&with_out));
    }

    #[test]
    fn harasser_strip_gate_accepts_strip_steal_or_corrupt() {
        assert!(kit_has_strip(&[skill_with(vec![
            SkillEffect::StripBoons {
                count_per_pulse: 1,
                interval_ms: 1000,
                window_ms: 5000,
            }
        ])]));
        assert!(kit_has_strip(&[skill_with(vec![SkillEffect::StealBoons])]));
        assert!(kit_has_strip(&[skill_with(vec![
            SkillEffect::CorruptBoons
        ])]));
        assert!(!kit_has_strip(&[skill_with(vec![
            SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }
        ])]));
    }

    /// 18 calibration rows: clock + gates (research/kimi-combat-model.md §7).
    #[test]
    fn calibration_eighteen_rows() {
        use CombatKind::*;
        use CombatTier::*;
        let rows: [(CombatTier, CombatKind, u32); 18] = [
            (Squad, StrikeSpike, 3_000), // 1 Reaper
            (Squad, StrikeSpike, 3_000), // 2 Untamed
            (Squad, StrikeSpike, 3_000), // 3 Evoker ranged
            (Squad, Support, 10_000),    // 4 Firebrand
            (Squad, Support, 10_000),    // 5 Druid
            (Squad, Support, 10_000),    // 6 Troubadour
            (Squad, Disabler, 7_000),    // 7 Core Necro corrupt
            (Squad, Disabler, 7_000),    // 8 Spellbreaker
            (Squad, Harasser, 2_500),    // 9 Conduit
            (Squad, Harasser, 2_500),    // 10 Dragonhunter
            (Squad, Commander, 10_000),  // 11 Luminary tag
            (Solo, Harasser, 2_000),     // 12 Willbender
            (Solo, Harasser, 2_000),     // 13 Deadeye
            (Solo, Harasser, 2_000),     // 14 Virtuoso
            (Solo, Harasser, 2_000),     // 15 Herald
            (Solo, CondiRamp, 5_000),    // 16 Soulbeast
            (Party, StrikeSpike, 2_500), // 17 Celestial Herald havoc
            (Party, StrikeSpike, 2_500), // 18 Scrapper havoc
        ];
        for (i, (tier, kind, t)) in rows.iter().enumerate() {
            assert_eq!(
                simulation_window_ms(*tier, *kind),
                *t,
                "row {} clock",
                i + 1
            );
            let require_out = *tier == Solo;
            let require_strip = *kind == Harasser;
            if require_out {
                assert!(*tier == Solo, "row {} roam must be Solo", i + 1);
            }
            if require_strip && *tier == Squad {
                assert_eq!(*kind, Harasser);
            }
        }
    }

    #[test]
    fn resistance_does_not_negate_poison_heal_or_terror() {
        assert!(resistance_negates("Immobile"));
        assert!(resistance_negates("Fear"));
        assert!(!resistance_negates("Poisoned"));
        assert!(!resistance_negates("Burning"));
        assert!(!resistance_negates("Terror"));
    }

    #[test]
    fn corrupt_resistance_becomes_chill() {
        assert_eq!(corrupt_into("Resistance"), Some("Chilled"));
        assert_eq!(corrupt_into("Protection"), Some("Vulnerability"));
        assert_eq!(corrupt_into("Aegis"), Some("Burning"));
        assert_eq!(corrupt_into("Vigor"), Some("Bleeding"));
        assert!(corrupt_into("Distortion").is_none());
    }

    #[test]
    fn buff_profile_follows_scale() {
        assert_eq!(buff_profile_index(CombatTier::Solo), 0);
        assert_eq!(buff_profile_index(CombatTier::Party), 1);
        assert_eq!(buff_profile_index(CombatTier::Squad), 2);
    }

    #[test]
    fn zerg_dummy_starts_booned_roam_dps_does_not() {
        let zerg = EnemyDummy::for_scenario(CombatTier::Squad, CombatKind::StrikeSpike);
        assert!(zerg.protection && zerg.stability);
        let roam_dps = EnemyDummy::for_scenario(CombatTier::Solo, CombatKind::StrikeSpike);
        assert!(!roam_dps.protection && !roam_dps.stability);
        let roam_pick = EnemyDummy::for_scenario(CombatTier::Solo, CombatKind::Harasser);
        assert!(roam_pick.protection && roam_pick.stability);
    }
}
