//! Shared ComboEngine: live combo fields + finisher resolution from `combos.json`.
//!
//! Fields are world/sim state (not TargetState, not ValidatedBuild). One engine
//! is used by the flow simulator and the WvW timeline.
//!
//! Assumptions (Ada silent / Grove ledger):
//! - Positions are coarse [`ComboSite`] only (not 2D).
//! - Solo sims use a single combatant id; `max_combatants_per_field` refuses a
//!   6th *unique* id on a given field.
//! - Own-then-oldest priority is dormant until a second field owner exists.
//! - Phase 5 `is_finisher_for_field` predicates are out of scope (read-only
//!   `active_fields` / `field_under` may be exposed later).
//! - Missing coefficients keep existing WvW numbers (water heal, dark whirl
//!   leech); unread cells are [`ComboOutcomeEffect::Unmodeled`].

use crate::data::combos::{self, ComboCell, ComboTable};
use crate::rotation::CoverKind;

/// Coarse combo placement (not a 2D map).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ComboSite {
    /// Finisher user's location / self-centered field.
    OnSelf,
    /// Ground / well / AoE field.
    Ground,
    /// Field attached near the current target.
    Target,
}

/// Wiki `_target_key` for an outcome cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComboTarget {
    SelfOnly,
    AreaAllies,
    EnemyHit,
    Bolts,
    AllyNearTarget,
    AreaEnemies,
}

impl ComboTarget {
    fn parse(s: &str) -> Self {
        match s {
            "self" => Self::SelfOnly,
            "area_allies" => Self::AreaAllies,
            "enemy_hit" => Self::EnemyHit,
            "bolts" => Self::Bolts,
            "ally_near_target" => Self::AllyNearTarget,
            "area_enemies" => Self::AreaEnemies,
            _ => Self::SelfOnly,
        }
    }
}

/// Effect payload for sims to apply via existing owners.
#[derive(Debug, Clone, PartialEq)]
pub enum ComboOutcomeEffect {
    Buff {
        name: String,
        stacks: u32,
        duration_ms: u32,
    },
    Condition {
        name: String,
        stacks: u32,
        duration_ms: u32,
    },
    Healing {
        base: f64,
        healing_power_coef: f64,
    },
    LifeSteal {
        damage_base: f64,
        damage_power_coef: f64,
        heal_base: f64,
        heal_healing_power_coef: f64,
    },
    ConditionCleanse {
        count: u32,
    },
    Cover {
        kind: CoverKind,
        duration_ms: u32,
    },
    CrowdControl {
        duration_ms: u32,
    },
    Unmodeled {
        reason: String,
    },
}

/// Resolved field x finisher outcome (before sim apply).
#[derive(Debug, Clone, PartialEq)]
pub struct ComboOutcome {
    pub field_type: String,
    pub finisher_type: String,
    pub target: ComboTarget,
    pub effect: ComboOutcomeEffect,
    /// Deterministic EV scale from `SkillEffect::ComboFinisher.percent` / 100.
    pub proc_scale: f64,
}

/// One live field instance.
#[derive(Debug, Clone)]
pub struct LiveField {
    pub field_type: String,
    pub expires_at_ms: u32,
    pub site: ComboSite,
    pub owner_id: u32,
    created_seq: u64,
    /// Unique combatant ids that have successfully combo'd this field.
    combatants: Vec<u32>,
}

/// Shared combo field tracker + resolver.
#[derive(Debug, Clone)]
pub struct ComboEngine {
    fields: Vec<LiveField>,
    next_seq: u64,
    max_combatants: u32,
}

impl Default for ComboEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ComboEngine {
    pub fn new() -> Self {
        let max = combos::combos().rules.max_combatants_per_field.value;
        Self {
            fields: Vec::new(),
            next_seq: 0,
            max_combatants: max.max(1),
        }
    }

    /// Drop expired fields.
    pub fn tick_expiry(&mut self, now_ms: u32) {
        self.fields.retain(|f| f.expires_at_ms > now_ms);
    }

    /// Place a combo field (world/sim state).
    pub fn place_field(
        &mut self,
        field_type: &str,
        duration_ms: u32,
        now_ms: u32,
        site: ComboSite,
        owner_id: u32,
    ) {
        self.tick_expiry(now_ms);
        let canonical = combos::canonical_field_name(field_type)
            .unwrap_or(field_type)
            .to_string();
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);
        self.fields.push(LiveField {
            field_type: canonical,
            expires_at_ms: now_ms.saturating_add(duration_ms),
            site,
            owner_id,
            created_seq: seq,
            combatants: Vec::new(),
        });
    }

    /// Read-only live fields (Phase 5 may consume; no predicate DSL here).
    pub fn active_fields(&self, now_ms: u32) -> Vec<&LiveField> {
        self.fields
            .iter()
            .filter(|f| f.expires_at_ms > now_ms)
            .collect()
    }

    /// First live field at `site` under own-then-oldest for `combatant_id`.
    pub fn field_under(
        &self,
        site: ComboSite,
        now_ms: u32,
        combatant_id: u32,
    ) -> Option<&LiveField> {
        let idx = self.pick_field_index(now_ms, combatant_id, false, Some(site))?;
        self.fields.get(idx)
    }

    /// Resolve a finisher against a live field.
    ///
    /// - `interrupted`: leap (or any) finisher cancelled by CC → no effect.
    /// - `percent`: API finisher percent; deterministic EV via `proc_scale`.
    /// - Max 5 unique combatants per field: a 6th unique id is refused.
    pub fn try_finisher(
        &mut self,
        finisher_type: &str,
        percent: u32,
        now_ms: u32,
        combatant_id: u32,
        site: ComboSite,
        interrupted: bool,
    ) -> Option<ComboOutcome> {
        if interrupted || percent == 0 {
            return None;
        }
        self.tick_expiry(now_ms);
        let is_leap = finisher_type.to_ascii_lowercase().contains("leap");
        let idx = self.pick_field_index(
            now_ms,
            combatant_id,
            is_leap,
            if is_leap { None } else { Some(site) },
        )?;
        let field = &self.fields[idx];
        if !field.combatants.contains(&combatant_id)
            && field.combatants.len() as u32 >= self.max_combatants
        {
            // 6th unique combatant refused.
            return None;
        }

        let table = combos::combos();
        let cell = combos::lookup_cell(table, &field.field_type, finisher_type)?;
        let field_type = field.field_type.clone();
        let finisher_norm = normalize_finisher(finisher_type);

        // Record combatant on successful table hit (before building outcome).
        if !self.fields[idx].combatants.contains(&combatant_id) {
            self.fields[idx].combatants.push(combatant_id);
        }

        let proc_scale = (percent as f64 / 100.0).clamp(0.0, 1.0);
        let effect = cell_to_effect(table, &field_type, &finisher_norm, cell);
        Some(ComboOutcome {
            field_type,
            finisher_type: finisher_norm,
            target: ComboTarget::parse(&cell.target),
            effect,
            proc_scale,
        })
    }

    fn pick_field_index(
        &self,
        now_ms: u32,
        combatant_id: u32,
        leap: bool,
        site_filter: Option<ComboSite>,
    ) -> Option<usize> {
        let live: Vec<usize> = self
            .fields
            .iter()
            .enumerate()
            .filter(|(_, f)| f.expires_at_ms > now_ms)
            .filter(|(_, f)| site_filter.is_none_or(|s| f.site == s))
            .map(|(i, _)| i)
            .collect();
        if live.is_empty() {
            // Leap path: if site filter emptied the list, fall back to all live.
            if leap && site_filter.is_some() {
                return self.pick_field_index(now_ms, combatant_id, true, None);
            }
            return None;
        }
        let own: Vec<usize> = live
            .iter()
            .copied()
            .filter(|&i| self.fields[i].owner_id == combatant_id)
            .collect();
        // Own-then-oldest: with a single actor every field is "own"; priority
        // only diverges when a second owner exists.
        let pool: Vec<usize> = if own.is_empty() { live } else { own };

        if leap {
            // First live field on coarse path: OnSelf -> Ground -> Target,
            // oldest within each site.
            for path_site in [ComboSite::OnSelf, ComboSite::Ground, ComboSite::Target] {
                if let Some(idx) = pool
                    .iter()
                    .copied()
                    .filter(|&i| self.fields[i].site == path_site)
                    .min_by_key(|&i| self.fields[i].created_seq)
                {
                    return Some(idx);
                }
            }
        }
        pool.into_iter().min_by_key(|&i| self.fields[i].created_seq)
    }
}

fn normalize_finisher(finisher_type: &str) -> String {
    let f = finisher_type.to_ascii_lowercase();
    if f.contains("blast") {
        "Blast".into()
    } else if f.contains("leap") {
        "Leap".into()
    } else if f.contains("projectile") {
        "Projectile".into()
    } else if f.contains("whirl") {
        "Whirl".into()
    } else {
        finisher_type.to_string()
    }
}

fn duration_ms(cell: &ComboCell) -> u32 {
    cell.duration_s
        .map(|s| (s * 1000.0).round() as u32)
        .unwrap_or(0)
}

fn stacks(cell: &ComboCell) -> u32 {
    cell.stacks.unwrap_or(1)
}

/// Map a table cell to an apply-able effect.
///
/// Water heal + Dark whirl LifeSteal keep the existing WvW coefficients.
/// Dark projectile LifeSteal and other unread numeric outcomes are Unmodeled.
fn cell_to_effect(
    _table: &ComboTable,
    field_type: &str,
    finisher: &str,
    cell: &ComboCell,
) -> ComboOutcomeEffect {
    let effect = cell.effect.as_str();
    let dur = duration_ms(cell);

    // Conditions on the foe (must use TargetState::apply_condition in sims).
    const CONDITIONS: &[&str] = &[
        "Burning",
        "Confusion",
        "Chilled",
        "Poisoned",
        "Vulnerability",
        "Weakness",
        "Blinded",
        "Bleeding",
        "Torment",
        "Immobile",
        "Crippled",
        "Slow",
        "Fear",
        "Taunt",
    ];
    // Alias Poisoned from JSON "Poisoned".
    if CONDITIONS.iter().any(|c| effect.eq_ignore_ascii_case(c))
        || effect.eq_ignore_ascii_case("Poison")
    {
        let name = if effect.eq_ignore_ascii_case("Poison") {
            "Poisoned".to_string()
        } else {
            effect.to_string()
        };
        return ComboOutcomeEffect::Condition {
            name,
            stacks: stacks(cell),
            duration_ms: dur.max(1),
        };
    }

    match effect {
        "Might" | "Swiftness" | "Regeneration" | "Chaos Aura" | "Fire Aura" | "Frost Aura"
        | "Light Aura" | "Dark Aura" => ComboOutcomeEffect::Buff {
            name: effect.to_string(),
            stacks: if effect == "Might" { stacks(cell) } else { 1 },
            duration_ms: dur.max(1),
        },
        "Stealth" => ComboOutcomeEffect::Cover {
            kind: CoverKind::Stealth,
            duration_ms: dur.max(1),
        },
        "ConditionCleanse" => ComboOutcomeEffect::ConditionCleanse {
            count: cell.conditions_removed.unwrap_or(1),
        },
        "Daze" => ComboOutcomeEffect::CrowdControl {
            duration_ms: dur.max(1),
        },
        "Healing" => {
            // Kept WvW timeline coefficients (do not invent wiki numbers).
            let (base, coef) = match finisher {
                "Blast" => (1_320.0, 0.20),
                "Leap" => (1_300.0, 0.50),
                _ => {
                    return ComboOutcomeEffect::Unmodeled {
                        reason: format!("{field_type} field + {finisher} Healing (no coefficient)"),
                    };
                }
            };
            ComboOutcomeEffect::Healing {
                base,
                healing_power_coef: coef,
            }
        }
        "LifeSteal" => {
            // Dark whirl leeching bolts: kept WvW numbers.
            // Dark projectile life stealing: numbers unread → Unmodeled.
            if field_type.eq_ignore_ascii_case("Dark") && finisher == "Whirl" {
                ComboOutcomeEffect::LifeSteal {
                    damage_base: 198.0,
                    damage_power_coef: 0.03,
                    heal_base: 170.0,
                    heal_healing_power_coef: 0.05,
                }
            } else {
                ComboOutcomeEffect::Unmodeled {
                    reason: format!(
                        "{field_type} field + {finisher} finisher (life stealing unread)"
                    ),
                }
            }
        }
        other => ComboOutcomeEffect::Unmodeled {
            reason: format!("{field_type} field + {finisher} ({other} unmodeled)"),
        },
    }
}

/// Solo-sim combatant id (self).
pub const SELF_COMBATANT_ID: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_blast_gives_three_might_20s() {
        let mut eng = ComboEngine::new();
        eng.place_field("Fire", 5_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        let out = eng
            .try_finisher(
                "Blast",
                100,
                100,
                SELF_COMBATANT_ID,
                ComboSite::Ground,
                false,
            )
            .expect("combo");
        match out.effect {
            ComboOutcomeEffect::Buff {
                name,
                stacks,
                duration_ms,
            } => {
                assert_eq!(name, "Might");
                assert_eq!(stacks, 3);
                assert_eq!(duration_ms, 20_000);
            }
            other => panic!("expected Might buff, got {other:?}"),
        }
        assert!((out.proc_scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn no_field_no_outcome() {
        let mut eng = ComboEngine::new();
        assert!(eng
            .try_finisher("Blast", 100, 0, SELF_COMBATANT_ID, ComboSite::Ground, false)
            .is_none());
    }

    #[test]
    fn expired_field_no_outcome() {
        let mut eng = ComboEngine::new();
        eng.place_field("Fire", 1_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        assert!(eng
            .try_finisher(
                "Blast",
                100,
                1_001,
                SELF_COMBATANT_ID,
                ComboSite::Ground,
                false
            )
            .is_none());
    }

    #[test]
    fn interrupted_leap_no_outcome() {
        let mut eng = ComboEngine::new();
        eng.place_field("Fire", 5_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        assert!(eng
            .try_finisher("Leap", 100, 100, SELF_COMBATANT_ID, ComboSite::Ground, true)
            .is_none());
    }

    #[test]
    fn projectile_percent_scales_ev() {
        let mut eng = ComboEngine::new();
        eng.place_field("Fire", 5_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        let full = eng
            .try_finisher(
                "Projectile",
                100,
                100,
                SELF_COMBATANT_ID,
                ComboSite::Ground,
                false,
            )
            .expect("100%");
        // Fresh field combatant already recorded; place another for second resolve.
        let mut eng2 = ComboEngine::new();
        eng2.place_field("Fire", 5_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        let partial = eng2
            .try_finisher(
                "Projectile",
                20,
                100,
                SELF_COMBATANT_ID,
                ComboSite::Ground,
                false,
            )
            .expect("20%");
        assert!((full.proc_scale - 1.0).abs() < f64::EPSILON);
        assert!((partial.proc_scale - 0.20).abs() < 1e-9);
        assert!(matches!(
            full.effect,
            ComboOutcomeEffect::Condition { ref name, .. } if name == "Burning"
        ));
    }

    #[test]
    fn fifth_combatant_ok_sixth_refused() {
        let mut eng = ComboEngine::new();
        eng.place_field("Fire", 10_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        for id in 1u32..=5 {
            assert!(
                eng.try_finisher("Blast", 100, 100, id, ComboSite::Ground, false)
                    .is_some(),
                "combatant {id} should combo"
            );
        }
        assert!(
            eng.try_finisher("Blast", 100, 200, 6, ComboSite::Ground, false)
                .is_none(),
            "6th unique combatant refused"
        );
        // Repeat of an existing id still allowed.
        assert!(eng
            .try_finisher("Blast", 100, 300, 3, ComboSite::Ground, false)
            .is_some());
    }

    #[test]
    fn water_blast_healing_coefficients() {
        let mut eng = ComboEngine::new();
        eng.place_field("Water", 5_000, 0, ComboSite::Ground, SELF_COMBATANT_ID);
        let out = eng
            .try_finisher(
                "Blast",
                100,
                100,
                SELF_COMBATANT_ID,
                ComboSite::Ground,
                false,
            )
            .expect("water blast");
        match out.effect {
            ComboOutcomeEffect::Healing {
                base,
                healing_power_coef,
            } => {
                assert!((base - 1_320.0).abs() < f64::EPSILON);
                assert!((healing_power_coef - 0.20).abs() < f64::EPSILON);
            }
            other => panic!("expected Healing, got {other:?}"),
        }
    }
}
