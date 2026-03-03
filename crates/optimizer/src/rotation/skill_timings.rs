//! Skill timing data (cast times) for rotation simulation.
//! The GW2 API does NOT provide cast/aftercast times — these come from the wiki
//! and community testing. We provide slot-based defaults as fallback.

use super::SkillSlot;

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

/// Human reaction/input delay between consecutive skill activations.
/// Players can't perfectly chain skills — there's always a small gap.
/// Range: 50ms (fast) to 150ms (average), we use 80ms as a reasonable default.
pub const HUMAN_DELAY_MS: u32 = 80;

/// Minimum global cooldown between any two skill activations (GW2 internal).
/// GW2 has a hidden ~100ms minimum between skill activations.
pub const MIN_SKILL_GAP_MS: u32 = 100;

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
}
