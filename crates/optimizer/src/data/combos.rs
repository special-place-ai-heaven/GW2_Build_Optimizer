//! Combo field x finisher table loader (`data/formulas/combos.json`).
//!
//! `include_str!` + `OnceLock` pattern (same as `boons.json`). SCHEMA = N —
//! this is sim/world state data, not ValidatedBuild.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

use super::{try_load, DataLoadError};

const COMBOS_JSON: &str = include_str!("../../../../data/formulas/combos.json");

static COMBOS: OnceLock<ComboTable> = OnceLock::new();

/// One field x finisher cell from the wiki table.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboCell {
    pub effect: String,
    pub target: String,
    #[serde(default)]
    pub duration_s: Option<f64>,
    #[serde(default)]
    pub stacks: Option<u32>,
    #[serde(default)]
    pub conditions_removed: Option<u32>,
    #[serde(default)]
    pub scales_with: Option<String>,
}

/// Finisher columns for one field type.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct FieldFinishers {
    pub blast: ComboCell,
    pub leap: ComboCell,
    pub projectile: ComboCell,
    pub whirl: ComboCell,
}

/// Rules block (max combatants, priority, projectile default, ...).
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboRules {
    pub max_combatants_per_field: ComboRuleValue,
    pub finisher_scales_with_finisher_user: ComboRuleBool,
    pub field_priority: ComboRuleStr,
    pub leap_first_field_wins: ComboRuleBool,
    pub projectile_partial_proc: ComboRuleF64,
    pub ordering: ComboRuleStr,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboRuleValue {
    pub value: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboRuleBool {
    pub value: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboRuleStr {
    pub value: String,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboRuleF64 {
    pub value: f64,
}

/// Core vs gated field/finisher types for one profession (wiki Combo page).
#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
pub struct ProfessionComboKit {
    #[serde(default)]
    pub core_finishers: Vec<String>,
    #[serde(default)]
    pub core_fields: Vec<String>,
    #[serde(default)]
    pub gated_finishers: HashMap<String, String>,
    #[serde(default)]
    pub gated_fields: HashMap<String, String>,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ComboTableFile {
    #[serde(rename = "_rules")]
    pub rules: ComboRules,
    pub fields: HashMap<String, FieldFinishers>,
    #[serde(default)]
    pub profession_combos: HashMap<String, ProfessionComboKit>,
}

/// Parsed combo table + rules.
#[derive(Debug, Clone, PartialEq)]
pub struct ComboTable {
    pub rules: ComboRules,
    pub fields: HashMap<String, FieldFinishers>,
    pub profession_combos: HashMap<String, ProfessionComboKit>,
}

#[derive(Debug, thiserror::Error)]
pub enum ComboLoadError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

/// Load + validate combo table JSON.
pub fn load_combo_table(json: &str) -> Result<ComboTable, ComboLoadError> {
    let file: ComboTableFile = serde_json::from_str(json)?;
    if file.fields.is_empty() {
        return Err(ComboLoadError::ValidationError(
            "fields map is empty".into(),
        ));
    }
    if file.rules.max_combatants_per_field.value == 0 {
        return Err(ComboLoadError::ValidationError(
            "max_combatants_per_field must be > 0".into(),
        ));
    }
    // Canonical field keys expected by the wiki table.
    for key in [
        "Dark",
        "Ethereal",
        "Fire",
        "Ice",
        "Light",
        "Lightning",
        "Poison",
        "Smoke",
        "Water",
    ] {
        if !file.fields.contains_key(key) {
            return Err(ComboLoadError::ValidationError(format!(
                "missing field key {key}"
            )));
        }
    }
    Ok(ComboTable {
        rules: file.rules,
        fields: file.fields,
        profession_combos: file.profession_combos,
    })
}

/// Globally loaded combo table (parse once).
///
/// # Panics
/// Panics if embedded JSON is malformed.
pub fn combos() -> &'static ComboTable {
    COMBOS.get_or_init(|| load_combo_table(COMBOS_JSON).expect("embedded combos.json is invalid"))
}

/// Health-check loader: does not store in OnceLock.
pub fn try_load_combos() -> Result<ComboTable, Vec<DataLoadError>> {
    try_load!("combos", load_combo_table(COMBOS_JSON), ComboLoadError)
}

/// Look up a cell by field + finisher (case-insensitive finisher).
pub fn lookup_cell<'a>(
    table: &'a ComboTable,
    field_type: &str,
    finisher_type: &str,
) -> Option<&'a ComboCell> {
    let field = canonical_field_name(field_type)
        .and_then(|k| table.fields.get(k))
        .or_else(|| {
            table
                .fields
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(field_type))
                .map(|(_, v)| v)
        })?;
    let fin = finisher_type.to_ascii_lowercase();
    if fin.contains("blast") {
        Some(&field.blast)
    } else if fin.contains("leap") {
        Some(&field.leap)
    } else if fin.contains("projectile") {
        Some(&field.projectile)
    } else if fin.contains("whirl") {
        Some(&field.whirl)
    } else {
        None
    }
}

/// Canonical wiki field key when `name` matches (including substring "Fire field").
pub fn canonical_field_name(name: &str) -> Option<&'static str> {
    const KEYS: &[&str] = &[
        "Dark",
        "Ethereal",
        "Fire",
        "Ice",
        "Light",
        "Lightning",
        "Poison",
        "Smoke",
        "Water",
    ];
    for key in KEYS {
        if name.eq_ignore_ascii_case(key) {
            return Some(*key);
        }
    }
    let lower = name.to_ascii_lowercase();
    for key in KEYS {
        if lower.contains(&key.to_ascii_lowercase()) {
            return Some(*key);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combos_json_loads_nine_fields() {
        let t = combos();
        assert_eq!(t.fields.len(), 9);
        assert_eq!(t.rules.max_combatants_per_field.value, 5);
        assert!((t.rules.projectile_partial_proc.value - 0.20).abs() < f64::EPSILON);
    }

    #[test]
    fn dark_blast_and_fire_projectile_cells() {
        let t = combos();
        let dark_blast = lookup_cell(t, "Dark", "Blast").expect("Dark blast");
        assert_eq!(dark_blast.effect, "Dark Aura");
        assert_eq!(dark_blast.duration_s, Some(3.0));
        let fire_proj = lookup_cell(t, "Fire", "Projectile").expect("Fire projectile");
        assert_eq!(fire_proj.effect, "Burning");
        assert_eq!(fire_proj.stacks, Some(1));
        assert_eq!(fire_proj.duration_s, Some(1.0));
    }

    #[test]
    fn try_load_combos_ok() {
        assert!(try_load_combos().is_ok());
    }

    #[test]
    fn malformed_json_errors() {
        assert!(load_combo_table("not json").is_err());
    }

    #[test]
    fn profession_combos_loaded() {
        let t = combos();
        assert!(t.profession_combos.contains_key("Necromancer"));
        assert!(t.profession_combos.contains_key("Guardian"));
        let necro = &t.profession_combos["Necromancer"];
        assert!(necro
            .gated_finishers
            .keys()
            .any(|k| k.eq_ignore_ascii_case("leap")));
    }

    #[test]
    fn profession_combos_optional_old_shape() {
        let mut v: serde_json::Value = serde_json::from_str(COMBOS_JSON).unwrap();
        v.as_object_mut().unwrap().remove("profession_combos");
        let t = load_combo_table(&v.to_string()).expect("old shape still loads");
        assert!(t.profession_combos.is_empty());
        assert_eq!(t.fields.len(), 9);
    }
}
