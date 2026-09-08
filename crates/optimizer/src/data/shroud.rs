//! Shroud table: life force pool, entry floor and per-shroud drain and
//! damage reduction, one shared shape for every Necromancer specialisation
//! (`specs/005-wvw-proc-sites`, R6). Loaded from `data/formulas/shroud.json`
//! with the `include_str!` + `OnceLock` pattern of the other formula files.
//!
//! A `null` drain or reduction means the wiki number was not read yet: the
//! caller must treat the resource model as incomplete, never invent a value.

use gw2_core::types::GameMode;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

const SHROUD_JSON: &str = include_str!("../../../../data/formulas/shroud.json");

static TABLE: OnceLock<ShroudTable> = OnceLock::new();

/// Per-mode percentages.
#[derive(Debug, Clone, Deserialize)]
pub struct PerMode {
    pub pve: f64,
    pub wvw: f64,
    pub pvp: f64,
}

impl PerMode {
    pub fn for_mode(&self, mode: GameMode) -> f64 {
        match mode {
            GameMode::PvE => self.pve,
            GameMode::WvW => self.wvw,
            GameMode::PvP => self.pvp,
        }
    }
}

/// One shroud entry skill's numbers. `None` is unresolved.
#[derive(Debug, Clone, Deserialize)]
pub struct ShroudRow {
    pub name: String,
    pub drain_pct_per_s: Option<PerMode>,
    pub damage_reduction_pct: Option<PerMode>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ShroudTable {
    pub source: String,
    pub life_force_pool_pct_of_health: f64,
    pub entry_floor_pct: f64,
    pub recharge_on_exit_s: f64,
    /// Keyed by the entry skill id.
    pub shrouds: HashMap<u32, ShroudRow>,
}

impl ShroudTable {
    /// The row for a shroud entry skill, if the table knows it.
    pub fn row(&self, entry_skill_id: u32) -> Option<&ShroudRow> {
        self.shrouds.get(&entry_skill_id)
    }

    /// Life force capacity for a health pool (wiki `Life force`: 69 %).
    pub fn pool_for(&self, max_health: f64) -> f64 {
        max_health * self.life_force_pool_pct_of_health / 100.0
    }
}

/// The embedded table, parsed once.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data).
pub fn table() -> &'static ShroudTable {
    TABLE
        .get_or_init(|| serde_json::from_str(SHROUD_JSON).expect("embedded shroud.json is invalid"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_shrouds_load_and_unread_rows_are_none() {
        let t = table();
        assert_eq!(t.shrouds.len(), 4);
        let reaper = t.row(30792).expect("Reaper's Shroud");
        assert_eq!(
            reaper
                .drain_pct_per_s
                .as_ref()
                .unwrap()
                .for_mode(GameMode::WvW),
            5.0
        );
        assert_eq!(
            reaper
                .damage_reduction_pct
                .as_ref()
                .unwrap()
                .for_mode(GameMode::PvE),
            33.0
        );
        let death = t.row(10574).expect("Death Shroud");
        assert_eq!(
            death
                .drain_pct_per_s
                .as_ref()
                .unwrap()
                .for_mode(GameMode::WvW),
            3.0
        );
        for id in [62567, 77238] {
            let row = t.row(id).expect("row exists");
            assert!(row.drain_pct_per_s.is_none() && row.damage_reduction_pct.is_none());
        }
        assert!(t.row(1).is_none());
        assert_eq!(t.entry_floor_pct, 10.0);
        assert!((t.pool_for(20_000.0) - 13_800.0).abs() < 1e-9);
        assert!(t.source.contains("(read 20"));
    }
}
