//! Skill timing data (cast times) for rotation simulation.
//! The GW2 API does NOT provide cast/aftercast times — these come from the wiki
//! and community testing. We provide slot-based defaults as fallback.

use super::SkillSlot;

/// Human reaction/input delay between consecutive skill activations.
/// Players can't perfectly chain skills — there's always a small gap.
/// Range: 50ms (fast) to 150ms (average), we use 80ms as a reasonable default.
pub const HUMAN_DELAY_MS: u32 = 80;

/// Minimum global cooldown between any two skill activations (GW2 internal).
/// GW2 has a hidden ~100ms minimum between skill activations.
pub const MIN_SKILL_GAP_MS: u32 = 100;

/// Wiki activation times. The API has no cast field.
/// IDs from live api.guildwars2.com; times from wiki skill pages (2026-08-14).
pub fn timing_for(skill_id: u32, slot: SkillSlot) -> SkillTiming {
    match skill_id {
        13005 => SkillTiming::new(250, 0),  // Backstab ¼ s
        13097 => SkillTiming::new(750, 0),  // Heartseeker ¾ s
        13115 => SkillTiming::new(0, 0),    // Sneak Attack (no activation)
        29887 => SkillTiming::new(750, 0),  // Spear of Justice ¾ s
        30229 => SkillTiming::new(750, 0),  // True Shot ¾ s
        30364 => SkillTiming::new(500, 0),  // Procession of Blades ½ s
        30628 => SkillTiming::new(1000, 0), // Hunter's Ward 1 s channel
        30557 => SkillTiming::new(250, 0),  // Executioner's Scythe ¼ s
        29855 => SkillTiming::new(500, 0),  // Nightfall ½ s
        30825 => SkillTiming::new(250, 0),  // Death's Charge ¼ s
        9080 => SkillTiming::new(500, 0),   // Leap of Faith ½ s
        13002 => SkillTiming::new(0, 0),    // Shadowstep instant
        44165 => SkillTiming::new(500, 0),  // Full Counter ½ s
        44364 => SkillTiming::new(0, 0),    // Tome of Justice open
        10192 => SkillTiming::new(0, 0),    // Distortion shatter
        10671 => SkillTiming::new(500, 0),  // Well of Corruption
        10674 => SkillTiming::new(500, 0),  // Well of Suffering
        45333 => SkillTiming::new(1500, 0), // Winds of Disenchantment 1½ s
        _ => default_timing(slot),
    }
}

/// Cast time for a specific skill.
#[derive(Debug, Clone, Copy)]
pub struct SkillTiming {
    /// Cast time in milliseconds (animation lock before the skill fires).
    pub cast_ms: u32,
    /// Aftercast in milliseconds (recovery before next skill can start).
    pub aftercast_ms: u32,
}

impl SkillTiming {
    pub const fn new(cast_ms: u32, aftercast_ms: u32) -> Self {
        Self {
            cast_ms,
            aftercast_ms,
        }
    }

    /// Total time this skill occupies (cast + aftercast).
    pub const fn total_ms(&self) -> u32 {
        self.cast_ms + self.aftercast_ms
    }
}

/// Default cast times by skill slot when no specific timing data is available.
/// These are conservative averages based on typical GW2 skill animations.
pub fn default_timing(slot: SkillSlot) -> SkillTiming {
    match slot {
        // Auto-attacks are generally fast (around 1/2 second per chain step)
        SkillSlot::Weapon1 => SkillTiming::new(400, 100),
        // Weapon skills 2-5 have moderate cast times
        SkillSlot::Weapon2 => SkillTiming::new(500, 250),
        SkillSlot::Weapon3 => SkillTiming::new(600, 250),
        SkillSlot::Weapon4 => SkillTiming::new(600, 300),
        SkillSlot::Weapon5 => SkillTiming::new(750, 300),
        // Heal skills tend to have longer cast/aftercast
        SkillSlot::Heal => SkillTiming::new(750, 250),
        // Utility skills are generally instant or fast
        SkillSlot::Utility => SkillTiming::new(250, 250),
        // Elite skills tend to have longer animations
        SkillSlot::Elite => SkillTiming::new(1000, 500),
        // Profession-specific (F1-F5) — moderate
        SkillSlot::Profession => SkillTiming::new(500, 250),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_timings_reasonable() {
        // Auto-attack should be fastest
        let aa = default_timing(SkillSlot::Weapon1);
        assert!(aa.total_ms() <= 600);

        // Elite should be slowest
        let elite = default_timing(SkillSlot::Elite);
        assert!(elite.total_ms() >= 1000);

        // All timings should be positive
        for slot in &[
            SkillSlot::Weapon1,
            SkillSlot::Weapon2,
            SkillSlot::Weapon3,
            SkillSlot::Weapon4,
            SkillSlot::Weapon5,
            SkillSlot::Heal,
            SkillSlot::Utility,
            SkillSlot::Elite,
            SkillSlot::Profession,
        ] {
            let t = default_timing(*slot);
            assert!(t.cast_ms > 0, "cast_ms should be positive for {:?}", slot);
            assert!(t.total_ms() > 0);
        }
    }

    #[test]
    fn test_skill_timing_total() {
        let t = SkillTiming::new(500, 250);
        assert_eq!(t.total_ms(), 750);
    }

    #[test]
    fn wod_cast_is_not_elite_slot_average() {
        let wod = timing_for(45333, SkillSlot::Elite);
        let slot = default_timing(SkillSlot::Elite);
        assert_eq!(wod.cast_ms, 1500);
        assert_ne!(wod.cast_ms, slot.cast_ms);
    }

    #[test]
    fn backstab_is_faster_than_weapon1_average() {
        let backstab = timing_for(13005, SkillSlot::Weapon1);
        assert_eq!(backstab.cast_ms, 250);
        assert!(backstab.total_ms() < default_timing(SkillSlot::Weapon1).total_ms());
    }

    #[test]
    fn spear_of_justice_is_not_profession_slot_average() {
        let spear = timing_for(29887, SkillSlot::Profession);
        assert_eq!(spear.cast_ms, 750);
        assert_ne!(spear.cast_ms, default_timing(SkillSlot::Profession).cast_ms);
    }

    #[test]
    fn hunter_ward_channel_is_one_second() {
        assert_eq!(timing_for(30628, SkillSlot::Weapon5).cast_ms, 1000);
    }

    #[test]
    fn instant_outs_are_zero_cast() {
        assert_eq!(timing_for(13002, SkillSlot::Utility).cast_ms, 0);
        assert_eq!(timing_for(10192, SkillSlot::Profession).cast_ms, 0);
    }
}
