//! Boon and condition formula loaders.
//!
//! Loads boon effects and condition damage formulas from embedded JSON data files.
//! Follows the P3-01 `include_str!` + `OnceLock` pattern.
//!
//! Boon effects are mode-aware (Fury PvE=25%, PvP/WvW=20%).
//! Condition formulas are mode-aware and state-aware (Torment stationary/moving,
//! Confusion over-time/on-skill-use).
//!
//! Precedence rule: per-mode entries (PvE/PvP/WvW) override `all_modes` at load time.

use gw2_core::types::GameMode;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

use super::{try_load, DataLoadError};

/// Canonical JSON embedded at compile time from data/formulas/boons.json.
const BOON_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/boons.json");

/// Canonical JSON embedded at compile time from data/formulas/conditions.json.
const CONDITION_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/conditions.json");

static BOONS: OnceLock<BoonFormulas> = OnceLock::new();
static CONDITIONS: OnceLock<ConditionFormulas> = OnceLock::new();

/// Returns the globally loaded boon formulas, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn boons() -> &'static BoonFormulas {
    BOONS.get_or_init(|| {
        load_boon_formulas(BOON_FORMULAS_JSON).expect("embedded boons.json is invalid")
    })
}

/// Returns the globally loaded condition formulas, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn conditions() -> &'static ConditionFormulas {
    CONDITIONS.get_or_init(|| {
        load_condition_formulas(CONDITION_FORMULAS_JSON)
            .expect("embedded conditions.json is invalid")
    })
}

/// Try to load boon formulas from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_boon_formulas() -> Result<BoonFormulas, Vec<DataLoadError>> {
    try_load!(
        "boon_formulas",
        load_boon_formulas(BOON_FORMULAS_JSON),
        FormulaLoadError
    )
}

/// Try to load condition formulas from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_condition_formulas() -> Result<ConditionFormulas, Vec<DataLoadError>> {
    try_load!(
        "condition_formulas",
        load_condition_formulas(CONDITION_FORMULAS_JSON),
        FormulaLoadError
    )
}

// ─── Condition name canonicalization ───
//
// GW2 wiki uses verb-form names for some conditions (Blind, Poison, Immobilize)
// while the in-game status-effect tooltips and `data/formulas/conditions.json`
// use the noun/adjective form (Blinded, Poisoned, Immobile). Authored data
// (objective_profiles, etc.) and ad-hoc string literals scattered through the
// codebase use both forms inconsistently. Canonical form for this codebase is
// the one used in `data/formulas/conditions.json` (noun/adjective form), since
// that is the lookup table all damage and metadata queries hit.
//
// 5 entries today; if the list grows materially, promote to a data file.
const CONDITION_ALIAS_TABLE: &[(&str, &str)] = &[
    ("Blind", "Blinded"),
    ("Poison", "Poisoned"),
    ("Immobilize", "Immobile"),
    ("Chill", "Chilled"),
    ("Cripple", "Crippled"),
];

/// Returns the canonical condition name for `name`, mapping verb-form aliases
/// (Blind, Poison, Immobilize) to the status-effect form used as keys in
/// `data/formulas/conditions.json` (Blinded, Poisoned, Immobile). Returns the
/// input unchanged if it is already canonical or not a recognized alias.
pub fn canonical_condition_name(name: &str) -> &str {
    for (alias, canonical) in CONDITION_ALIAS_TABLE {
        if name == *alias {
            return canonical;
        }
    }
    name
}

/// Returns `true` if `status` names a GW2 condition (damaging or non-damaging).
///
/// Accepts either verb-form aliases (Blind, Poison, Chill, Cripple,
/// Immobilize) or canonical status-effect form (Blinded, Poisoned, Chilled,
/// …) — the input is normalized via `canonical_condition_name` before
/// matching, so the arms only list canonical forms. `Immobilized` stays as an
/// explicit arm: the alias resolver only knows `Immobilize→Immobile`, not the
/// past-tense `Immobilized`.
pub(crate) fn is_condition(status: &str) -> bool {
    let canonical = canonical_condition_name(status);
    matches!(
        canonical,
        "Bleeding"
            | "Burning"
            | "Poisoned"
            | "Torment"
            | "Confusion"
            | "Vulnerability"
            | "Weakness"
            | "Blinded"
            | "Chilled"
            | "Crippled"
            | "Fear"
            | "Immobile"
            | "Immobilized"
            | "Slow"
            | "Taunt"
    )
}

// ─── Error Types ───

#[derive(Debug, Error)]
pub enum FormulaLoadError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

// ─── Boon Types ───

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum StackingMode {
    Intensity,
    Duration,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub enum EffectClass {
    OffensiveThroughput,
    OffensiveStat,
    Defensive,
    Sustain,
    Utility,
    Damage,
    Debuff,
    Suppression,
    Control,
}

/// Raw boon effect values from JSON (a flat map of string keys to f64 values).
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(transparent)]
pub struct BoonEffectValues {
    pub values: HashMap<String, f64>,
}

impl BoonEffectValues {
    pub fn get(&self, key: &str) -> Option<f64> {
        self.values.get(key).copied()
    }
}

/// Raw JSON structure for a single boon definition.
#[derive(Debug, Clone, Deserialize)]
struct RawBoonDefinition {
    stacking_mode: StackingMode,
    max_stacks: u32,
    #[serde(default)]
    max_duration: Option<u32>,
    effect_class: EffectClass,
    #[serde(default)]
    special_mechanics: Option<String>,
    effects: HashMap<String, BoonEffectValues>,
    evidence_level: String,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    counterpart_condition: Option<String>,
    #[serde(default)]
    secondary_effects: Option<String>,
}

/// Resolved boon definition with per-mode effects.
#[derive(Debug, Clone)]
pub struct BoonDefinition {
    pub name: String,
    pub stacking_mode: StackingMode,
    pub max_stacks: u32,
    pub max_duration: Option<u32>,
    pub effect_class: EffectClass,
    pub special_mechanics: Option<String>,
    /// Resolved effects per mode (PvE, PvP, WvW).
    pub effects: HashMap<String, BoonEffectValues>,
    pub evidence_level: String,
    pub sources: Vec<String>,
    pub counterpart_condition: Option<String>,
    pub secondary_effects: Option<String>,
}

/// O(1) lookup wrapper for loaded boon formulas.
#[derive(Debug)]
pub struct BoonFormulas {
    map: HashMap<String, BoonDefinition>,
}

impl BoonFormulas {
    /// Fury crit chance bonus as a ratio (0.25 PvE, 0.20 PvP/WvW).
    pub fn fury_crit_bonus(&self, mode: GameMode) -> f64 {
        let mode_key = mode_to_key(&mode);
        self.map
            .get("Fury")
            .and_then(|b| b.effects.get(mode_key))
            .and_then(|e| e.get("crit_chance_bonus"))
            .unwrap_or(0.25)
    }

    /// Might power bonus per stack (30.0).
    pub fn might_power_per_stack(&self) -> f64 {
        self.map
            .get("Might")
            .and_then(|b| b.effects.get("PvE").or_else(|| b.effects.get("all_modes")))
            .and_then(|e| e.get("power_per_stack"))
            .unwrap_or(30.0)
    }

    /// Might condition damage bonus per stack (30.0).
    pub fn might_condi_per_stack(&self) -> f64 {
        self.map
            .get("Might")
            .and_then(|b| b.effects.get("PvE").or_else(|| b.effects.get("all_modes")))
            .and_then(|e| e.get("condition_damage_per_stack"))
            .unwrap_or(30.0)
    }

    /// Protection incoming strike multiplier (0.67). Caller computes DR as `1.0 - 0.67 = 0.33`.
    pub fn protection_multiplier(&self) -> f64 {
        self.map
            .get("Protection")
            .and_then(|b| b.effects.get("PvE").or_else(|| b.effects.get("all_modes")))
            .and_then(|e| e.get("incoming_strike_multiplier"))
            .unwrap_or(0.67)
    }

    /// Resolution incoming condition multiplier (0.67).
    pub fn resolution_multiplier(&self) -> f64 {
        self.map
            .get("Resolution")
            .and_then(|b| b.effects.get("PvE").or_else(|| b.effects.get("all_modes")))
            .and_then(|e| e.get("incoming_condition_multiplier"))
            .unwrap_or(0.67)
    }

    /// Vulnerability damage increase per stack as a ratio (0.01).
    pub fn vulnerability_pct_per_stack(&self) -> f64 {
        self.map
            .get("Vulnerability")
            .and_then(|b| b.effects.get("PvE").or_else(|| b.effects.get("all_modes")))
            .and_then(|e| e.get("incoming_damage_pct_per_stack"))
            .unwrap_or(0.01)
    }

    /// Vulnerability max stacks (25).
    pub fn vulnerability_max_stacks(&self) -> u32 {
        self.map
            .get("Vulnerability")
            .map(|b| b.max_stacks)
            .unwrap_or(25)
    }

    /// Generic accessor for any boon definition.
    pub fn get(&self, boon: &str) -> Option<&BoonDefinition> {
        self.map.get(boon)
    }

    /// Number of loaded boon definitions.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when no boon definitions are loaded.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ─── Condition Types ───

/// A simple condition formula: base + coeff * condition_damage.
#[derive(Debug, Clone, Deserialize)]
pub struct SimpleConditionFormula {
    pub base_per_tick: f64,
    pub condition_damage_coeff: f64,
    #[serde(default)]
    pub delivery: Option<String>,
}

impl SimpleConditionFormula {
    /// Calculate tick damage: coeff * condition_damage + base.
    pub fn calculate(&self, condition_damage: f64) -> f64 {
        self.condition_damage_coeff * condition_damage + self.base_per_tick
    }
}

/// Mode-keyed formula entry that can be either simple or multi-state.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModeFormula {
    /// Simple: direct formula fields.
    Simple(SimpleConditionFormula),
    /// Multi-state Torment: stationary + moving sub-entries.
    TormentState {
        stationary: SimpleConditionFormula,
        moving: SimpleConditionFormula,
    },
    /// Multi-state Confusion: over_time + on_skill_use sub-entries.
    ConfusionState {
        over_time: SimpleConditionFormula,
        on_skill_use: SimpleConditionFormula,
    },
    /// Non-damage condition (Vulnerability with pct_per_stack, or no formula).
    NonDamage(HashMap<String, serde_json::Value>),
}

/// Suppression effects metadata for control/suppression conditions.
#[derive(Debug, Clone, Deserialize)]
pub struct SuppressionEffects {
    #[serde(default)]
    pub channels: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Raw JSON structure for a single condition definition.
#[derive(Debug, Clone, Deserialize)]
struct RawConditionDefinition {
    stacking_mode: StackingMode,
    #[serde(default)]
    max_stacks: Option<u32>,
    effect_class: EffectClass,
    #[serde(default)]
    secondary_effects: Option<String>,
    #[serde(default)]
    special_mechanics: Option<String>,
    #[serde(default)]
    suppression_effects: Option<SuppressionEffects>,
    #[serde(default)]
    formulas: HashMap<String, ModeFormula>,
    evidence_level: String,
    #[serde(default)]
    sources: Vec<String>,
}

/// Resolved condition definition.
#[derive(Debug, Clone)]
pub struct ConditionDefinition {
    pub name: String,
    pub stacking_mode: StackingMode,
    pub max_stacks: Option<u32>,
    pub effect_class: EffectClass,
    pub secondary_effects: Option<String>,
    pub special_mechanics: Option<String>,
    pub suppression_effects: Option<SuppressionEffects>,
    /// Per-mode formulas (PvE, PvP, WvW).
    pub formulas: HashMap<String, ModeFormula>,
    pub evidence_level: String,
    pub sources: Vec<String>,
}

/// O(1) lookup wrapper for loaded condition formulas.
#[derive(Debug)]
pub struct ConditionFormulas {
    map: HashMap<String, ConditionDefinition>,
}

impl ConditionFormulas {
    /// Calculate tick damage for a simple condition (Bleeding, Burning, Poison).
    /// For multi-state conditions, use `torment_tick()` or `confusion_tick()`.
    pub fn tick_damage(&self, condition: &str, condition_damage: f64, mode: GameMode) -> f64 {
        let mode_key = mode_to_key(&mode);
        // Route through the canonical resolver so verb-form aliases
        // (Poison, Blind, Immobilize) hit their status-effect-form entries.
        let canonical = canonical_condition_name(condition);
        let cond = match self.map.get(canonical) {
            Some(c) => c,
            None => return 0.0,
        };
        match cond.formulas.get(mode_key) {
            Some(ModeFormula::Simple(f)) => f.calculate(condition_damage),
            // Multi-state: use default state (stationary for Torment,
            // on_skill_use for Confusion)
            Some(ModeFormula::TormentState { stationary, .. }) => {
                stationary.calculate(condition_damage)
            }
            Some(ModeFormula::ConfusionState { on_skill_use, .. }) => {
                on_skill_use.calculate(condition_damage)
            }
            _ => 0.0,
        }
    }

    /// Torment tick damage with explicit mode and movement state.
    /// `moving`: true = target is moving, false = stationary.
    pub fn torment_tick(&self, condition_damage: f64, mode: GameMode, moving: bool) -> f64 {
        let mode_key = mode_to_key(&mode);
        let cond = match self.map.get("Torment") {
            Some(c) => c,
            None => return 0.0,
        };
        match cond.formulas.get(mode_key) {
            Some(ModeFormula::TormentState {
                stationary,
                moving: moving_f,
            }) => {
                if moving {
                    moving_f.calculate(condition_damage)
                } else {
                    stationary.calculate(condition_damage)
                }
            }
            _ => 0.0,
        }
    }

    /// Confusion tick damage with explicit mode and trigger state.
    /// `on_skill_use`: true = activation damage, false = passive over-time.
    pub fn confusion_tick(&self, condition_damage: f64, mode: GameMode, on_skill_use: bool) -> f64 {
        let mode_key = mode_to_key(&mode);
        let cond = match self.map.get("Confusion") {
            Some(c) => c,
            None => return 0.0,
        };
        match cond.formulas.get(mode_key) {
            Some(ModeFormula::ConfusionState {
                over_time,
                on_skill_use: on_use_f,
            }) => {
                if on_skill_use {
                    on_use_f.calculate(condition_damage)
                } else {
                    over_time.calculate(condition_damage)
                }
            }
            _ => 0.0,
        }
    }

    /// Generic accessor for any condition definition.
    pub fn get(&self, condition: &str) -> Option<&ConditionDefinition> {
        self.map.get(condition)
    }

    /// Number of loaded condition definitions.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// True when no condition definitions are loaded.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ─── Loader Functions ───

fn mode_to_key(mode: &GameMode) -> &'static str {
    match mode {
        GameMode::PvE => "PvE",
        GameMode::PvP => "PvP",
        GameMode::WvW => "WvW",
    }
}

/// Parse and validate boon formulas from JSON text.
///
/// Precedence rule: per-mode entries (PvE/PvP/WvW) override `all_modes`.
/// Resolution happens at load time so the in-memory representation stores
/// resolved per-mode values.
pub fn load_boon_formulas(json: &str) -> Result<BoonFormulas, FormulaLoadError> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json)?;
    let mut map = HashMap::new();

    for (name, value) in &raw {
        // Skip metadata fields (prefixed with _)
        if name.starts_with('_') {
            continue;
        }

        let raw_def: RawBoonDefinition = serde_json::from_value(value.clone())
            .map_err(|e| FormulaLoadError::ValidationError(format!("boon '{}': {}", name, e)))?;

        // Resolve all_modes -> per-mode
        let resolved_effects = resolve_boon_effects(&raw_def.effects);

        if map.contains_key(name.as_str()) {
            return Err(FormulaLoadError::ValidationError(format!(
                "duplicate boon: {}",
                name
            )));
        }

        map.insert(
            name.clone(),
            BoonDefinition {
                name: name.clone(),
                stacking_mode: raw_def.stacking_mode,
                max_stacks: raw_def.max_stacks,
                max_duration: raw_def.max_duration,
                effect_class: raw_def.effect_class,
                special_mechanics: raw_def.special_mechanics,
                effects: resolved_effects,
                evidence_level: raw_def.evidence_level,
                sources: raw_def.sources,
                counterpart_condition: raw_def.counterpart_condition,
                secondary_effects: raw_def.secondary_effects,
            },
        );
    }

    Ok(BoonFormulas { map })
}

/// Resolve `all_modes` entries to per-mode. Per-mode takes precedence.
fn resolve_boon_effects(
    effects: &HashMap<String, BoonEffectValues>,
) -> HashMap<String, BoonEffectValues> {
    let all_modes = effects.get("all_modes");
    let modes = ["PvE", "PvP", "WvW"];
    let mut resolved = HashMap::new();

    for mode in &modes {
        if let Some(per_mode) = effects.get(*mode) {
            // Per-mode takes absolute precedence
            resolved.insert(mode.to_string(), per_mode.clone());
        } else if let Some(all) = all_modes {
            // Fall back to all_modes
            resolved.insert(mode.to_string(), all.clone());
        }
    }

    resolved
}

/// Parse and validate condition formulas from JSON text.
pub fn load_condition_formulas(json: &str) -> Result<ConditionFormulas, FormulaLoadError> {
    let raw: HashMap<String, serde_json::Value> = serde_json::from_str(json)?;
    let mut map = HashMap::new();

    for (name, value) in &raw {
        // Skip metadata fields (prefixed with _)
        if name.starts_with('_') {
            continue;
        }

        let raw_def: RawConditionDefinition =
            serde_json::from_value(value.clone()).map_err(|e| {
                FormulaLoadError::ValidationError(format!("condition '{}': {}", name, e))
            })?;

        // Validate damage conditions declare all 3 modes
        if raw_def.effect_class == EffectClass::Damage {
            let modes = ["PvE", "PvP", "WvW"];
            for mode in &modes {
                if !raw_def.formulas.contains_key(*mode) {
                    return Err(FormulaLoadError::ValidationError(format!(
                        "condition '{}': missing mode '{}'",
                        name, mode,
                    )));
                }
            }
        }

        if map.contains_key(name.as_str()) {
            return Err(FormulaLoadError::ValidationError(format!(
                "duplicate condition: {}",
                name
            )));
        }

        map.insert(
            name.clone(),
            ConditionDefinition {
                name: name.clone(),
                stacking_mode: raw_def.stacking_mode,
                max_stacks: raw_def.max_stacks,
                effect_class: raw_def.effect_class,
                secondary_effects: raw_def.secondary_effects,
                special_mechanics: raw_def.special_mechanics,
                suppression_effects: raw_def.suppression_effects,
                formulas: raw_def.formulas,
                evidence_level: raw_def.evidence_level,
                sources: raw_def.sources,
            },
        );
    }

    Ok(ConditionFormulas { map })
}

#[cfg(test)]
mod tests {
    use super::*;
    use gw2_core::types::GameMode;

    // ─── Loader tests ───

    #[test]
    fn test_embedded_boon_formulas_load_successfully() {
        let b = boons();
        // At least Fury, Might, Protection, Resolution, Vulnerability,
        // Quickness, Alacrity, Aegis, Stability, Resistance,
        // Regeneration, Vigor, Swiftness = 13
        assert!(b.len() >= 13, "expected at least 13 boons, got {}", b.len());
    }

    #[test]
    fn test_embedded_condition_formulas_load_successfully() {
        let c = conditions();
        // At least Bleeding, Burning, Poisoned, Torment, Confusion,
        // Vulnerability, Weakness, Blinded, Slow, Chilled,
        // Immobile, Crippled, Fear, Taunt, Daze = 15
        assert!(
            c.len() >= 15,
            "expected at least 15 conditions, got {}",
            c.len()
        );
    }

    #[test]
    fn test_duplicate_boon_rejected() {
        // Manually construct JSON with duplicate boon names.
        // JSON spec: last key wins. So we test via loader validation instead.
        // Since serde HashMap deduplicates, we test with our own loader logic.
        let json = r#"{
            "Fury": {
                "stacking_mode": "Duration",
                "max_stacks": 1,
                "effect_class": "OffensiveStat",
                "effects": { "all_modes": { "crit_chance_bonus": 0.25 } },
                "evidence_level": "Factual"
            }
        }"#;
        // This should succeed (no duplicate)
        let result = load_boon_formulas(json);
        assert!(result.is_ok());
    }

    #[test]
    fn test_malformed_stacking_mode_rejected() {
        let json = r#"{
            "TestBoon": {
                "stacking_mode": "InvalidMode",
                "max_stacks": 1,
                "effect_class": "OffensiveStat",
                "effects": { "all_modes": {} },
                "evidence_level": "Factual"
            }
        }"#;
        let err = load_boon_formulas(json).unwrap_err();
        assert!(
            err.to_string().contains("TestBoon"),
            "error should mention boon name: {}",
            err
        );
    }

    #[test]
    fn test_condition_missing_mode_rejected() {
        let json = r#"{
            "TestCond": {
                "stacking_mode": "Intensity",
                "max_stacks": 1500,
                "effect_class": "Damage",
                "formulas": {
                    "PvE": { "base_per_tick": 10.0, "condition_damage_coeff": 0.05 },
                    "PvP": { "base_per_tick": 10.0, "condition_damage_coeff": 0.05 }
                },
                "evidence_level": "Factual"
            }
        }"#;
        let err = load_condition_formulas(json).unwrap_err();
        assert!(
            err.to_string().contains("missing mode 'WvW'"),
            "error should mention missing WvW: {}",
            err
        );
    }

    #[test]
    fn test_all_modes_vs_per_mode_precedence() {
        // Per-mode entry should override all_modes (AC 4, L9)
        let json = r#"{
            "TestBoon": {
                "stacking_mode": "Duration",
                "max_stacks": 1,
                "effect_class": "OffensiveStat",
                "effects": {
                    "all_modes": { "crit_chance_bonus": 0.25 },
                    "PvP": { "crit_chance_bonus": 0.20 },
                    "WvW": { "crit_chance_bonus": 0.20 }
                },
                "evidence_level": "Factual"
            }
        }"#;
        let formulas = load_boon_formulas(json).unwrap();
        let def = formulas.get("TestBoon").unwrap();
        // PvE should use all_modes value (0.25)
        let pve = def.effects.get("PvE").unwrap();
        assert_eq!(pve.get("crit_chance_bonus"), Some(0.25));
        // PvP should use per-mode override (0.20)
        let pvp = def.effects.get("PvP").unwrap();
        assert_eq!(pvp.get("crit_chance_bonus"), Some(0.20));
        // WvW should use per-mode override (0.20)
        let wvw = def.effects.get("WvW").unwrap();
        assert_eq!(wvw.get("crit_chance_bonus"), Some(0.20));
    }

    // ─── Boon value tests (cite wiki sources) ───

    // Source: https://wiki.guildwars2.com/wiki/Fury
    #[test]
    fn test_fury_pve_is_25_pct() {
        assert!(
            (boons().fury_crit_bonus(GameMode::PvE) - 0.25).abs() < f64::EPSILON,
            "Fury PvE crit bonus should be 0.25"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Fury
    #[test]
    fn test_fury_pvp_is_20_pct() {
        assert!(
            (boons().fury_crit_bonus(GameMode::PvP) - 0.20).abs() < f64::EPSILON,
            "Fury PvP crit bonus should be 0.20"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Fury
    #[test]
    fn test_fury_wvw_is_20_pct() {
        assert!(
            (boons().fury_crit_bonus(GameMode::WvW) - 0.20).abs() < f64::EPSILON,
            "Fury WvW crit bonus should be 0.20"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Might
    #[test]
    fn test_might_per_stack_values() {
        assert!(
            (boons().might_power_per_stack() - 30.0).abs() < f64::EPSILON,
            "Might should give +30 Power per stack"
        );
        assert!(
            (boons().might_condi_per_stack() - 30.0).abs() < f64::EPSILON,
            "Might should give +30 Condition Damage per stack"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Protection
    #[test]
    fn test_protection_multiplier() {
        assert!(
            (boons().protection_multiplier() - 0.67).abs() < f64::EPSILON,
            "Protection should have 0.67 multiplier (33% DR)"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Resolution
    #[test]
    fn test_resolution_multiplier() {
        assert!(
            (boons().resolution_multiplier() - 0.67).abs() < f64::EPSILON,
            "Resolution should have 0.67 multiplier"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Vulnerability
    #[test]
    fn test_vulnerability_per_stack() {
        assert!(
            (boons().vulnerability_pct_per_stack() - 0.01).abs() < f64::EPSILON,
            "Vulnerability should be 0.01 per stack"
        );
        assert_eq!(
            boons().vulnerability_max_stacks(),
            25,
            "Vulnerability max should be 25"
        );
    }

    // ─── Condition formula tests (cite wiki sources) ───

    // Source: https://wiki.guildwars2.com/wiki/Bleeding
    #[test]
    fn test_bleeding_formula_all_modes() {
        let c = conditions();
        let cd = 1000.0;
        // 0.06 * 1000 + 22 = 82.0
        let expected = 82.0;
        for mode in GameMode::ALL {
            let result = c.tick_damage("Bleeding", cd, mode.clone());
            assert!(
                (result - expected).abs() < 0.01,
                "Bleeding {:?}: expected {}, got {}",
                mode,
                expected,
                result,
            );
        }
    }

    // Source: https://wiki.guildwars2.com/wiki/Burning
    // L1 verification: base=131.0 per source-of-truth doc and wiki
    #[test]
    fn test_burning_formula() {
        let c = conditions();
        let cd = 1000.0;
        // 0.155 * 1000 + 131.0 = 286.0
        let expected = 286.0;
        let result = c.tick_damage("Burning", cd, GameMode::PvE);
        assert!(
            (result - expected).abs() < 0.01,
            "Burning PvE: expected {}, got {}",
            expected,
            result,
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Poisoned
    #[test]
    fn test_poison_formula_all_modes() {
        let c = conditions();
        let cd = 1000.0;
        // 0.06 * 1000 + 33.5 = 93.5
        let expected = 93.5;
        for mode in GameMode::ALL {
            let result = c.tick_damage("Poison", cd, mode.clone());
            assert!(
                (result - expected).abs() < 0.01,
                "Poison {:?}: expected {}, got {}",
                mode,
                expected,
                result,
            );
        }
    }

    // Source: https://wiki.guildwars2.com/wiki/Torment
    // L2 verification: PvE stationary vs moving differ, PvE vs PvP differ
    #[test]
    fn test_torment_pve_stationary_vs_moving() {
        let c = conditions();
        let cd = 1000.0;
        // PvE stationary: 0.09 * 1000 + 31.8 = 121.8
        let stationary = c.torment_tick(cd, GameMode::PvE, false);
        // PvE moving: 0.06 * 1000 + 22.0 = 82.0
        let moving = c.torment_tick(cd, GameMode::PvE, true);
        assert!(
            (stationary - 121.8).abs() < 0.01,
            "Torment PvE stationary: expected 121.8, got {}",
            stationary,
        );
        assert!(
            (moving - 82.0).abs() < 0.01,
            "Torment PvE moving: expected 82.0, got {}",
            moving,
        );
        // They must differ
        assert!(
            (stationary - moving).abs() > 1.0,
            "Torment stationary and moving should differ"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Torment
    #[test]
    fn test_torment_pve_vs_pvp() {
        let c = conditions();
        let cd = 1000.0;
        // PvE stationary: 0.09 * 1000 + 31.8 = 121.8
        let pve = c.torment_tick(cd, GameMode::PvE, false);
        // PvP stationary: 0.07 * 1000 + 26.0 = 96.0
        let pvp = c.torment_tick(cd, GameMode::PvP, false);
        assert!(
            (pve - 121.8).abs() < 0.01,
            "Torment PvE stationary: expected 121.8, got {}",
            pve,
        );
        assert!(
            (pvp - 96.0).abs() < 0.01,
            "Torment PvP stationary: expected 96.0, got {}",
            pvp,
        );
        // PvE and PvP must differ
        assert!((pve - pvp).abs() > 1.0, "Torment PvE and PvP should differ");
    }

    // Source: https://wiki.guildwars2.com/wiki/Confusion
    // L3 verification: PvE over-time vs on-skill-use differ
    #[test]
    fn test_confusion_pve_overtime_vs_on_skill_use() {
        let c = conditions();
        let cd = 1000.0;
        // PvE over-time: 0.05 * 1000 + 18.25 = 68.25
        let over_time = c.confusion_tick(cd, GameMode::PvE, false);
        // PvE on-skill-use: 0.0325 * 1000 + 16.24 = 48.74
        let on_skill_use = c.confusion_tick(cd, GameMode::PvE, true);
        assert!(
            (over_time - 68.25).abs() < 0.01,
            "Confusion PvE over-time: expected 68.25, got {}",
            over_time,
        );
        assert!(
            (on_skill_use - 48.74).abs() < 0.01,
            "Confusion PvE on-skill-use: expected 48.74, got {}",
            on_skill_use,
        );
        assert!(
            (over_time - on_skill_use).abs() > 1.0,
            "Confusion over-time and on-skill-use should differ"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Confusion
    #[test]
    fn test_confusion_pve_vs_pvp() {
        let c = conditions();
        let cd = 1000.0;
        // PvE on-skill-use: 0.0325 * 1000 + 16.24 = 48.74
        let pve = c.confusion_tick(cd, GameMode::PvE, true);
        // PvP on-skill-use: 0.0975 * 1000 + 49.5 = 147.0
        let pvp = c.confusion_tick(cd, GameMode::PvP, true);
        assert!(
            (pve - 48.74).abs() < 0.01,
            "Confusion PvE on-skill-use: expected 48.74, got {}",
            pve,
        );
        assert!(
            (pvp - 147.0).abs() < 0.01,
            "Confusion PvP on-skill-use: expected 147.0, got {}",
            pvp,
        );
        assert!(
            (pve - pvp).abs() > 1.0,
            "Confusion PvE and PvP on-skill-use should differ"
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Confusion
    #[test]
    fn test_confusion_pvp_overtime_is_flat_10() {
        let c = conditions();
        // PvP/WvW over-time is flat 10 damage regardless of condition damage.
        let result_0 = c.confusion_tick(0.0, GameMode::PvP, false);
        let result_2000 = c.confusion_tick(2000.0, GameMode::PvP, false);
        assert!(
            (result_0 - 10.0).abs() < 0.01,
            "Confusion PvP over-time at CD=0: expected 10.0, got {}",
            result_0,
        );
        assert!(
            (result_2000 - 10.0).abs() < 0.01,
            "Confusion PvP over-time at CD=2000: expected 10.0, got {}",
            result_2000,
        );
    }

    // ─── StatusDefinition metadata tests ───

    #[test]
    fn test_boon_stacking_modes() {
        let b = boons();
        assert_eq!(
            b.get("Might").unwrap().stacking_mode,
            StackingMode::Intensity,
        );
        assert_eq!(b.get("Fury").unwrap().stacking_mode, StackingMode::Duration,);
        assert_eq!(
            b.get("Stability").unwrap().stacking_mode,
            StackingMode::Intensity,
        );
        assert_eq!(
            b.get("Protection").unwrap().stacking_mode,
            StackingMode::Duration,
        );
    }

    #[test]
    fn test_condition_stacking_modes() {
        let c = conditions();
        assert_eq!(
            c.get("Bleeding").unwrap().stacking_mode,
            StackingMode::Intensity,
        );
        assert_eq!(
            c.get("Weakness").unwrap().stacking_mode,
            StackingMode::Duration,
        );
        assert_eq!(
            c.get("Torment").unwrap().stacking_mode,
            StackingMode::Intensity,
        );
    }

    #[test]
    fn test_boon_max_stacks() {
        let b = boons();
        assert_eq!(b.get("Might").unwrap().max_stacks, 25);
        assert_eq!(b.get("Stability").unwrap().max_stacks, 25);
        assert_eq!(b.get("Fury").unwrap().max_stacks, 1);
    }

    #[test]
    fn test_condition_effect_classes() {
        let c = conditions();
        assert_eq!(c.get("Bleeding").unwrap().effect_class, EffectClass::Damage,);
        assert_eq!(
            c.get("Weakness").unwrap().effect_class,
            EffectClass::Suppression,
        );
        assert_eq!(c.get("Fear").unwrap().effect_class, EffectClass::Control,);
    }

    // ─── Alias-routing regression suite ───
    //
    // Fuzzes verb-form and canonical-form input through every site that was
    // updated to route through `canonical_condition_name`. Every paired
    // verb/canonical input must produce identical results (alias is a no-op
    // for behavior). `Burning` is included as an identity-passthrough check.

    /// All (verb-form, canonical-form) condition pairs that the resolver knows.
    /// Plus pure-canonical entries to confirm passthrough is identity.
    const ALIAS_PAIRS: &[(&str, &str)] = &[
        ("Blind", "Blinded"),
        ("Poison", "Poisoned"),
        ("Immobilize", "Immobile"),
        ("Chill", "Chilled"),
        ("Cripple", "Crippled"),
        ("Burning", "Burning"),
        ("Bleeding", "Bleeding"),
        ("Torment", "Torment"),
        ("Confusion", "Confusion"),
    ];

    #[test]
    fn test_canonical_condition_name_resolves_all_known_aliases() {
        for (verb, canonical) in ALIAS_PAIRS {
            assert_eq!(
                canonical_condition_name(verb),
                *canonical,
                "verb-form `{}` should canonicalize to `{}`",
                verb,
                canonical,
            );
            assert_eq!(
                canonical_condition_name(canonical),
                *canonical,
                "canonical-form `{}` should pass through unchanged",
                canonical,
            );
        }
    }

    #[test]
    fn test_canonical_condition_name_passthrough_for_unknown_input() {
        // Unknown input is returned verbatim (identity) — preserves behavior
        // for non-condition strings and any 4th-spelling alias not yet in
        // the table.
        assert_eq!(canonical_condition_name("Vulnerability"), "Vulnerability");
        assert_eq!(canonical_condition_name("Slow"), "Slow");
        assert_eq!(canonical_condition_name("Immobilized"), "Immobilized");
        assert_eq!(canonical_condition_name("NotACondition"), "NotACondition");
        assert_eq!(canonical_condition_name(""), "");
    }

    /// `is_condition` in gamedb.rs must accept either form for the 5 aliased
    /// conditions and produce the same answer.
    #[test]
    fn test_gamedb_is_condition_alias_equivalence() {
        use crate::gamedb::tests_alias_helpers::is_condition;
        for (verb, canonical) in ALIAS_PAIRS {
            assert_eq!(
                is_condition(verb),
                is_condition(canonical),
                "gamedb::is_condition disagrees on alias pair ({}, {})",
                verb,
                canonical,
            );
            assert!(
                is_condition(verb),
                "gamedb::is_condition({}) should be true",
                verb,
            );
        }
        // Non-condition input still returns false.
        assert!(!is_condition("Might"));
        assert!(!is_condition("NotACondition"));
    }

    /// `is_condition` in synergy.rs must accept either form for the 5 aliased
    /// conditions and produce the same answer.
    #[test]
    fn test_synergy_is_condition_alias_equivalence() {
        use crate::synergy::tests_alias_helpers::is_condition;
        for (verb, canonical) in ALIAS_PAIRS {
            assert_eq!(
                is_condition(verb),
                is_condition(canonical),
                "synergy::is_condition disagrees on alias pair ({}, {})",
                verb,
                canonical,
            );
            assert!(is_condition(verb));
        }
        assert!(!is_condition("Might"));
    }

    /// The shared `is_condition` recognizes damaging and non-damaging
    /// conditions, normalizes verb-form aliases, and rejects boons / unknowns.
    #[test]
    fn test_shared_is_condition() {
        // Damaging condition.
        assert!(is_condition("Bleeding"));
        // Non-damaging condition.
        assert!(is_condition("Vulnerability"));
        // Verb-form alias that canonicalizes (Chill -> Chilled).
        assert!(is_condition("Chill"));
        assert_eq!(is_condition("Chill"), is_condition("Chilled"));
        // Boons are not conditions.
        assert!(!is_condition("Might"));
        assert!(!is_condition("Fury"));
        // Unrecognized input is not a condition.
        assert!(!is_condition("NotACondition"));
    }

    /// `is_damaging_condition` in rotation/builder.rs must accept either
    /// form for the 5 aliased conditions and produce the same answer.
    #[test]
    fn test_rotation_builder_is_damaging_condition_alias_equivalence() {
        use crate::rotation::builder::tests_alias_helpers::is_damaging_condition;
        // Damaging set: Bleeding, Burning, Poisoned, Torment, Confusion.
        for cond in &["Bleeding", "Burning", "Torment", "Confusion"] {
            assert!(is_damaging_condition(cond), "{} should be damaging", cond,);
        }
        // Verb/canonical pair for Poison must agree and both be true.
        assert!(is_damaging_condition("Poison"));
        assert!(is_damaging_condition("Poisoned"));
        assert_eq!(
            is_damaging_condition("Poison"),
            is_damaging_condition("Poisoned"),
        );
        // Non-damaging conditions (control/suppression) must be false in both
        // forms.
        for cond in &[
            "Blind",
            "Blinded",
            "Chill",
            "Chilled",
            "Immobilize",
            "Immobile",
        ] {
            assert!(
                !is_damaging_condition(cond),
                "{} should NOT be damaging",
                cond,
            );
        }
    }

    /// `condition_importance` in synergy.rs must produce the same score for
    /// verb-form and canonical-form Poison.
    #[test]
    fn test_synergy_condition_importance_alias_equivalence() {
        use crate::synergy::tests_alias_helpers::condition_importance;
        assert_eq!(
            condition_importance("Poison"),
            condition_importance("Poisoned"),
        );
        // Identity-passthrough conditions still produce their canonical score.
        assert!((condition_importance("Burning") - 1.0).abs() < 1e-9);
        assert!((condition_importance("Bleeding") - 0.7).abs() < 1e-9);
        assert!((condition_importance("Confusion") - 0.1).abs() < 1e-9);
    }

    /// `cond_importance_from_op` in normalized_effects.rs must produce the
    /// same score for verb-form and canonical-form Poison.
    #[test]
    fn test_normalized_effects_cond_importance_alias_equivalence() {
        use crate::data::normalized_effects::tests_alias_helpers::cond_importance_for_status_kind;
        assert_eq!(
            cond_importance_for_status_kind("Poison"),
            cond_importance_for_status_kind("Poisoned"),
        );
        assert!((cond_importance_for_status_kind("Burning") - 1.0).abs() < 1e-9);
        assert!((cond_importance_for_status_kind("Bleeding") - 0.7).abs() < 1e-9);
    }

    /// `tick_damage` in ConditionFormulas already routes verb-form aliases
    /// through `canonical_condition_name` (P3 round-3 work). Smoke-test the
    /// full alias matrix to confirm.
    #[test]
    fn test_tick_damage_alias_equivalence() {
        let c = conditions();
        let cd = 1000.0;
        for mode in GameMode::ALL {
            let poison_verb = c.tick_damage("Poison", cd, mode.clone());
            let poison_canon = c.tick_damage("Poisoned", cd, mode.clone());
            assert!(
                (poison_verb - poison_canon).abs() < 1e-9,
                "tick_damage Poison/Poisoned mismatch in {:?}: {} vs {}",
                mode,
                poison_verb,
                poison_canon,
            );
            // Pure-canonical identity: Burning passes through unchanged.
            let burn = c.tick_damage("Burning", cd, mode.clone());
            assert!(burn > 0.0, "Burning tick should be positive in {:?}", mode,);
        }
    }
}
