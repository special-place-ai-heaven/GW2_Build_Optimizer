//! Fight population per scale (`specs/007-trait-triggers`, FR-003a): how
//! many foes and allies a WvW fight has, so every target-facing effect is
//! counted onto everyone it is meant for within the record's cap. Counted,
//! not simulated: extra foes and allies carry no state. Loaded from
//! `data/formulas/fight_population.json` with the `include_str!` +
//! `OnceLock` pattern of the other formula files.

use crate::scenario::CombatTier;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const POPULATION_JSON: &str = include_str!("../../../../data/formulas/fight_population.json");

static TABLE: OnceLock<PopulationTable> = OnceLock::new();

/// Foes (including the primary target) and allies (excluding the player).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct FightPopulation {
    pub foes: u32,
    pub allies: u32,
}

impl FightPopulation {
    /// One foe, no allies: every direct constructor and every test that does
    /// not say otherwise.
    pub const fn solo() -> Self {
        Self { foes: 1, allies: 0 }
    }

    pub fn for_tier(tier: CombatTier) -> Self {
        let name = match tier {
            CombatTier::Solo => "Solo",
            CombatTier::Party => "Party",
            CombatTier::Squad => "Squad",
        };
        table().tiers.get(name).copied().unwrap_or_else(Self::solo)
    }
}

impl Default for FightPopulation {
    fn default() -> Self {
        Self::solo()
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct PopulationTable {
    pub source: String,
    pub tiers: HashMap<String, FightPopulation>,
}

pub fn table() -> &'static PopulationTable {
    TABLE.get_or_init(|| {
        let table: PopulationTable =
            serde_json::from_str(POPULATION_JSON).expect("fight_population.json parses");
        for (name, tier) in &table.tiers {
            assert!(tier.foes >= 1, "tier {name}: foes must be at least 1");
        }
        table
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_tiers_load_with_the_documented_counts() {
        assert_eq!(
            FightPopulation::for_tier(CombatTier::Solo),
            FightPopulation { foes: 1, allies: 0 }
        );
        assert_eq!(
            FightPopulation::for_tier(CombatTier::Party),
            FightPopulation { foes: 5, allies: 4 }
        );
        assert_eq!(
            FightPopulation::for_tier(CombatTier::Squad),
            FightPopulation {
                foes: 10,
                allies: 9
            }
        );
        assert!(table().source.contains("scenario tiers"));
        assert_eq!(FightPopulation::default(), FightPopulation::solo());
    }
}
