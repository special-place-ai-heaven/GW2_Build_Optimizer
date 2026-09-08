//! When each hit of a multi-hit skill lands inside its activation.
//!
//! The API gives a skill one damage fact with a hit count and one activation
//! time; it says nothing about spacing. `data/formulas/hit_timing.json` has
//! measured spacing for the skills someone has measured (gw2combat, Guardian
//! and Ranger); everything else spreads its hits evenly across the cast,
//! which is the honest zero-data model and already better than one lump at
//! the end.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

const HIT_TIMING_JSON: &str = include_str!("../../../../data/formulas/hit_timing.json");

static TABLE: OnceLock<HashMap<String, Vec<f64>>> = OnceLock::new();

#[derive(Deserialize)]
struct RawTiming {
    hits: u32,
    at: Vec<f64>,
}

#[derive(Deserialize)]
struct RawFile {
    skills: HashMap<String, RawTiming>,
}

fn table() -> &'static HashMap<String, Vec<f64>> {
    TABLE.get_or_init(|| {
        let raw: RawFile =
            serde_json::from_str(HIT_TIMING_JSON).expect("embedded hit_timing.json is invalid");
        raw.skills
            .into_iter()
            .filter(|(_, t)| t.hits as usize == t.at.len())
            .map(|(name, t)| (name, t.at))
            .collect()
    })
}

/// Offsets from cast start, in ms, at which each of `hit_count` hits lands.
///
/// Measured spacing wins when the table knows the skill AND agrees with the
/// API on the hit count; otherwise hits are spread evenly, the last one at
/// the end of the cast. A single hit lands at cast end; an instant lands now.
pub fn hit_schedule(skill_name: &str, cast_ms: u32, hit_count: u32) -> Vec<u32> {
    let hits = hit_count.max(1);
    if let Some(at) = table().get(skill_name) {
        if at.len() == hits as usize {
            return at
                .iter()
                .map(|f| (cast_ms as f64 * f).round() as u32)
                .collect();
        }
    }
    (1..=hits)
        .map(|k| ((cast_ms as u64 * k as u64) / hits as u64) as u32)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::hit_schedule;

    #[test]
    fn unknown_skills_spread_evenly_and_end_on_the_cast() {
        assert_eq!(
            hit_schedule("nothing", 1_000, 4),
            vec![250, 500, 750, 1_000]
        );
        assert_eq!(hit_schedule("nothing", 1_000, 1), vec![1_000]);
        assert_eq!(hit_schedule("nothing", 0, 3), vec![0, 0, 0]);
    }

    #[test]
    fn rapid_fire_uses_its_measured_ten_shots() {
        let ticks = hit_schedule("Rapid Fire", 2_400, 10);
        assert_eq!(ticks.len(), 10);
        assert_eq!(ticks[0], 240);
        assert_eq!(ticks[9], 2_400);
    }

    #[test]
    fn a_hit_count_mismatch_falls_back_to_even_spread() {
        assert_eq!(hit_schedule("Rapid Fire", 2_000, 2), vec![1_000, 2_000]);
    }
}
