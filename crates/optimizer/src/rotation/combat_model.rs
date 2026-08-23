//! Fight-dummy clocks and kit gates used by the rotation scorer.
//! Mapping lives in `builder`; this module is the scorer contract.

use crate::scenario::{CombatKind, CombatTier};

use super::{CoverKind, MobilityKind, RotationSkill, SkillEffect};

/// Simulation window 0–T in milliseconds for (scale × kind).
/// Zerg T=3s / 6–8s are derived (not log-measured).
/// WvW needs enough wall-clock for a protected opener and the enemy's answer.
/// A two-second spike remains a reported sub-window, but the exchange itself is
/// never truncated at the instant the minimum chain completes.
pub fn simulation_window_ms(tier: CombatTier, kind: CombatKind) -> u32 {
    match (tier, kind) {
        (CombatTier::Solo, CombatKind::StrikeSpike) => 5_000,
        (CombatTier::Solo, CombatKind::CondiRamp) => 20_000,
        (CombatTier::Solo, CombatKind::Harasser | CombatKind::Disabler) => 10_000,
        (CombatTier::Solo, CombatKind::Commander | CombatKind::Support | CombatKind::Staller) => {
            20_000
        }
        (CombatTier::Party, CombatKind::StrikeSpike) => 5_000,
        (CombatTier::Party, CombatKind::CondiRamp) => 20_000,
        (CombatTier::Party, CombatKind::Harasser | CombatKind::Disabler) => 10_000,
        (CombatTier::Party, CombatKind::Commander | CombatKind::Support | CombatKind::Staller) => {
            20_000
        }
        (CombatTier::Squad, CombatKind::StrikeSpike) => 5_000,
        (CombatTier::Squad, CombatKind::CondiRamp) => 20_000,
        (CombatTier::Squad, CombatKind::Harasser | CombatKind::Disabler) => 10_000,
        (CombatTier::Squad, CombatKind::Commander | CombatKind::Support | CombatKind::Staller) => {
            20_000
        }
    }
}

/// Prefer CC/strip/cover over DPCT for this long at the start of a short clock.
pub fn setup_window_ms(duration_ms: u32) -> u32 {
    2_000.min(duration_ms)
}

pub fn kit_has_mobility_out(skills: &[RotationSkill]) -> bool {
    kit_escape_kinds(skills) > 0
}

/// Distinct roam-out categories: mobility, stealth, block, invuln/aegis.
pub fn kit_escape_kinds(skills: &[RotationSkill]) -> u32 {
    let mut mobility = false;
    let mut stealth = false;
    let mut block = false;
    let mut cover = false;
    for skill in skills {
        for e in &skill.effects {
            match e {
                SkillEffect::Mobility {
                    kind: MobilityKind::Stealth,
                } => stealth = true,
                SkillEffect::Mobility { .. } => mobility = true,
                SkillEffect::Cover {
                    kind: CoverKind::Stealth,
                    ..
                } => stealth = true,
                SkillEffect::Cover {
                    kind: CoverKind::Block,
                    ..
                } => block = true,
                SkillEffect::Cover {
                    kind: CoverKind::Invulnerability | CoverKind::Aegis | CoverKind::Evade,
                    ..
                } => cover = true,
                _ => {}
            }
        }
    }
    u32::from(mobility) + u32::from(stealth) + u32::from(block) + u32::from(cover)
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

pub fn kit_has_interrupt(skills: &[RotationSkill]) -> bool {
    skills.iter().any(|s| {
        s.effects
            .iter()
            .any(|e| matches!(e, SkillEffect::CrowdControl { .. }))
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

/// Cover that eats the incoming alpha: Stability, evade, block, invuln/aegis, stealth, or blind.
/// Leap/teleport is an *out*, not cover — that is `kit_has_mobility_out`.
pub fn kit_has_cover_answer(skills: &[RotationSkill]) -> bool {
    kit_has_stability_cover(skills)
        || skills.iter().any(|s| {
            s.effects.iter().any(|e| {
                matches!(
                    e,
                    SkillEffect::Mobility {
                        kind: MobilityKind::Evade | MobilityKind::Stealth,
                    } | SkillEffect::Cover {
                        kind: CoverKind::Stealth
                            | CoverKind::Evade
                            | CoverKind::Block
                            | CoverKind::Invulnerability
                            | CoverKind::Aegis
                            | CoverKind::Blind,
                        ..
                    }
                )
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

/// Glass roam pick / havoc bruiser. Support and large-scale pressure have no solo outcome target.
pub fn dummy_hp(tier: CombatTier, kind: CombatKind) -> Option<f64> {
    if matches!(
        kind,
        CombatKind::Support | CombatKind::Commander | CombatKind::Staller
    ) {
        return None;
    }
    match (tier, kind) {
        (CombatTier::Solo, _) => Some(13_000.0),
        (CombatTier::Party, _) => Some(20_000.0),
        (CombatTier::Squad, CombatKind::Harasser) => Some(13_000.0),
        (CombatTier::Squad, _) => None,
    }
}

/// Enemy cover the dummy starts with. Strip/steal/corrupt clear it for the rest of the window.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EnemyDummy {
    pub protection: bool,
    pub stability: bool,
    /// `None` = open dummy (no encounter-outcome tracking).
    pub hp: Option<f64>,
}

impl EnemyDummy {
    pub fn open() -> Self {
        Self::default()
    }

    /// Zerg/havoc blobs and roam harasser targets are assumed booned; naked roam DPS is not.
    pub fn for_scenario(tier: CombatTier, kind: CombatKind) -> Self {
        let hp = dummy_hp(tier, kind);
        match (tier, kind) {
            (CombatTier::Solo, CombatKind::Harasser | CombatKind::Disabler) => Self {
                protection: true,
                stability: true,
                hp,
            },
            (CombatTier::Party, _) | (CombatTier::Squad, _) => Self {
                protection: true,
                stability: true,
                hp,
            },
            _ => Self { hp, ..Self::open() },
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
    fn roam_exchange_includes_the_minimum_chain_and_response() {
        assert_eq!(
            simulation_window_ms(CombatTier::Solo, CombatKind::StrikeSpike),
            5_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Solo, CombatKind::Harasser),
            10_000
        );
    }

    #[test]
    fn zerg_kinds_do_not_share_a_clock() {
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::StrikeSpike),
            5_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::CondiRamp),
            20_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::Harasser),
            10_000
        );
    }

    #[test]
    fn setup_window_keeps_the_two_second_opening() {
        assert_eq!(setup_window_ms(30_000), 2_000);
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
        let with_block = vec![skill_with(vec![SkillEffect::Cover {
            kind: CoverKind::Block,
            duration_ms: 1000,
            strippable: false,
        }])];
        assert!(!kit_has_mobility_out(&burst));
        assert!(kit_has_mobility_out(&with_out));
        assert!(kit_has_mobility_out(&with_block));
    }

    #[test]
    fn cover_answer_evade_not_leap() {
        let leap = vec![skill_with(vec![SkillEffect::Mobility {
            kind: MobilityKind::Leap,
        }])];
        let evade = vec![skill_with(vec![SkillEffect::Mobility {
            kind: MobilityKind::Evade,
        }])];
        assert!(!kit_has_cover_answer(&leap));
        assert!(kit_has_cover_answer(&evade));
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

    /// 18 calibration rows covering the supported combat clocks and kit gates.
    #[test]
    fn calibration_eighteen_rows() {
        use CombatKind::*;
        use CombatTier::*;
        let rows: [(CombatTier, CombatKind, u32); 18] = [
            (Squad, StrikeSpike, 5_000), // 1 Reaper
            (Squad, StrikeSpike, 5_000), // 2 Untamed
            (Squad, StrikeSpike, 5_000), // 3 Evoker ranged
            (Squad, Support, 20_000),    // 4 Firebrand
            (Squad, Support, 20_000),    // 5 Druid
            (Squad, Support, 20_000),    // 6 Troubadour
            (Squad, Disabler, 10_000),   // 7 Core Necro corrupt
            (Squad, Disabler, 10_000),   // 8 Spellbreaker
            (Squad, Harasser, 10_000),   // 9 Conduit
            (Squad, Harasser, 10_000),   // 10 Dragonhunter
            (Squad, Commander, 20_000),  // 11 Luminary tag
            (Solo, Harasser, 10_000),    // 12 Willbender
            (Solo, Harasser, 10_000),    // 13 Deadeye
            (Solo, Harasser, 10_000),    // 14 Virtuoso
            (Solo, Harasser, 10_000),    // 15 Herald
            (Solo, CondiRamp, 20_000),   // 16 Soulbeast
            (Party, StrikeSpike, 5_000), // 17 Celestial Herald havoc
            (Party, StrikeSpike, 5_000), // 18 Scrapper havoc
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
        assert!(zerg.hp.is_none());
        let roam_dps = EnemyDummy::for_scenario(CombatTier::Solo, CombatKind::StrikeSpike);
        assert!(!roam_dps.protection && !roam_dps.stability);
        assert_eq!(roam_dps.hp, Some(13_000.0));
        let roam_pick = EnemyDummy::for_scenario(CombatTier::Solo, CombatKind::Harasser);
        assert!(roam_pick.protection && roam_pick.stability);
        assert_eq!(roam_pick.hp, Some(13_000.0));
        let havoc = EnemyDummy::for_scenario(CombatTier::Party, CombatKind::StrikeSpike);
        assert_eq!(havoc.hp, Some(20_000.0));
        let support = EnemyDummy::for_scenario(CombatTier::Solo, CombatKind::Support);
        assert!(support.hp.is_none());
        let troll = EnemyDummy::for_scenario(CombatTier::Squad, CombatKind::Staller);
        assert!(troll.hp.is_none());
        assert_eq!(
            simulation_window_ms(CombatTier::Solo, CombatKind::Staller),
            20_000
        );
        assert_eq!(
            simulation_window_ms(CombatTier::Squad, CombatKind::Staller),
            20_000
        );
    }

    #[test]
    fn interrupt_is_any_crowd_control() {
        assert!(kit_has_interrupt(&[skill_with(vec![
            SkillEffect::CrowdControl {
                kind: ControlKind::Daze,
                duration_ms: 500,
                stops_dodge: false,
            }
        ])]));
        assert!(!kit_has_interrupt(&[skill_with(vec![
            SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }
        ])]));
    }
}
