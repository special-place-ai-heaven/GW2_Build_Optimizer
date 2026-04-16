use serde::Deserialize;
use std::sync::OnceLock;
use thiserror::Error;

use super::{try_load, DataLoadError, EvidenceLevel};

/// Canonical JSON embedded at compile time from data/formulas/universal.json.
const UNIVERSAL_FORMULAS_JSON: &str = include_str!("../../../../data/formulas/universal.json");

static FORMULAS: OnceLock<UniversalFormulas> = OnceLock::new();

/// Returns the globally loaded universal formulas, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn formulas() -> &'static UniversalFormulas {
    FORMULAS.get_or_init(|| {
        load_universal_formulas(UNIVERSAL_FORMULAS_JSON)
            .expect("embedded universal.json is invalid")
    })
}

/// Try to load universal formulas from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_universal_formulas() -> Result<UniversalFormulas, Vec<DataLoadError>> {
    try_load!(
        "universal_formulas",
        load_universal_formulas(UNIVERSAL_FORMULAS_JSON),
        UniversalFormulaError
    )
}

#[derive(Debug, Error)]
pub enum UniversalFormulaError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

/// Universal formula constants for GW2 attribute and strike damage calculations.
/// These are mode-invariant (same in PvE, PvP, WvW).
///
/// Sources:
/// - https://wiki.guildwars2.com/wiki/Attribute
/// - https://wiki.guildwars2.com/wiki/Critical_Chance
/// - https://wiki.guildwars2.com/wiki/Ferocity
/// - https://wiki.guildwars2.com/wiki/Damage
/// - https://wiki.guildwars2.com/wiki/Health
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UniversalFormulas {
    /// Base value for primary attributes at level 80 (Power, Precision, Toughness, Vitality).
    pub base_primary_attribute: f64,
    /// Multiplier: health = base_health + vitality * vitality_to_health.
    pub vitality_to_health: f64,
    /// Offset subtracted from Precision before dividing: crit% = (Prec - offset) / divisor.
    pub precision_offset: f64,
    /// Divisor: each `precision_per_crit_pct` points of (Precision - offset) = 1% crit chance.
    pub precision_per_crit_pct: f64,
    /// Divisor: each `ferocity_per_crit_damage_pct` points of Ferocity = 1% crit damage.
    pub ferocity_per_crit_damage_pct: f64,
    /// Base critical damage percentage with zero Ferocity.
    pub base_crit_damage_pct: f64,
    /// Divisor: each `expertise_per_condition_duration_pct` Expertise = 1% condition duration.
    pub expertise_per_condition_duration_pct: f64,
    /// Divisor: each `concentration_per_boon_duration_pct` Concentration = 1% boon duration.
    pub concentration_per_boon_duration_pct: f64,
    /// Maximum condition duration bonus (as a ratio, 1.0 = 100%).
    pub condition_duration_cap: f64,
    /// Maximum boon duration bonus (as a ratio, 1.0 = 100%).
    pub boon_duration_cap: f64,
    /// Reference armor value used in tooltip damage calculations (2597 at level 80).
    pub tooltip_reference_armor: f64,
    /// Evidence level for all constants in this file.
    pub evidence_level: EvidenceLevel,
    /// Wiki source URLs documenting these constants.
    pub sources: Vec<String>,
}

impl UniversalFormulas {
    /// Critical chance from Precision (percentage points, 0-100).
    /// Formula: (precision - precision_offset) / precision_per_crit_pct
    /// Does NOT clamp — caller is responsible for clamping to [0, 100].
    /// Source: https://wiki.guildwars2.com/wiki/Critical_Chance
    pub fn crit_chance(&self, precision: f64) -> f64 {
        (precision - self.precision_offset) / self.precision_per_crit_pct
    }

    /// Critical damage from Ferocity (percentage, e.g. 170.0 for 170%).
    /// Formula: base_crit_damage_pct + ferocity / ferocity_per_crit_damage_pct
    /// Source: https://wiki.guildwars2.com/wiki/Ferocity
    pub fn crit_damage(&self, ferocity: f64) -> f64 {
        self.base_crit_damage_pct + ferocity / self.ferocity_per_crit_damage_pct
    }

    /// Health from Vitality + profession base health.
    /// Formula: base_health + vitality * vitality_to_health
    /// Source: https://wiki.guildwars2.com/wiki/Health
    pub fn health(&self, vitality: f64, base_health: f64) -> f64 {
        base_health + vitality * self.vitality_to_health
    }

    /// Strike damage term.
    /// Formula: skill_damage * (power / base_primary_attribute)
    ///          * (tooltip_reference_armor / target_armor)
    /// Source: https://wiki.guildwars2.com/wiki/Damage
    pub fn strike_damage(&self, skill_damage: f64, power: f64, target_armor: f64) -> f64 {
        skill_damage
            * (power / self.base_primary_attribute)
            * (self.tooltip_reference_armor / target_armor)
    }
}

/// Parse and validate universal formulas from JSON text.
pub fn load_universal_formulas(json: &str) -> Result<UniversalFormulas, UniversalFormulaError> {
    let formulas: UniversalFormulas = serde_json::from_str(json)?;
    validate_formulas(&formulas)?;
    Ok(formulas)
}

fn validate_formulas(f: &UniversalFormulas) -> Result<(), UniversalFormulaError> {
    // All numeric fields must be positive
    if f.base_primary_attribute <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "base_primary_attribute must be positive".into(),
        ));
    }
    if f.vitality_to_health <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "vitality_to_health must be positive".into(),
        ));
    }
    if f.precision_offset <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "precision_offset must be positive".into(),
        ));
    }
    if f.precision_per_crit_pct <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "precision_per_crit_pct must be positive".into(),
        ));
    }
    if f.ferocity_per_crit_damage_pct <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "ferocity_per_crit_damage_pct must be positive".into(),
        ));
    }
    if f.base_crit_damage_pct <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "base_crit_damage_pct must be positive".into(),
        ));
    }
    if f.expertise_per_condition_duration_pct <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "expertise_per_condition_duration_pct must be positive".into(),
        ));
    }
    if f.concentration_per_boon_duration_pct <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "concentration_per_boon_duration_pct must be positive".into(),
        ));
    }
    if f.tooltip_reference_armor <= 0.0 {
        return Err(UniversalFormulaError::ValidationError(
            "tooltip_reference_armor must be positive".into(),
        ));
    }

    // Duration caps must be exactly 1.0 for universal formulas
    if (f.condition_duration_cap - 1.0).abs() > f64::EPSILON {
        return Err(UniversalFormulaError::ValidationError(format!(
            "condition_duration_cap must be 1.0, got {}",
            f.condition_duration_cap
        )));
    }
    if (f.boon_duration_cap - 1.0).abs() > f64::EPSILON {
        return Err(UniversalFormulaError::ValidationError(format!(
            "boon_duration_cap must be 1.0, got {}",
            f.boon_duration_cap
        )));
    }

    // Evidence level must be Factual for universal formulas
    if f.evidence_level != EvidenceLevel::Factual {
        return Err(UniversalFormulaError::ValidationError(
            "universal formulas must have Factual evidence level".into(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_formulas_load_successfully() {
        let f = formulas();
        assert_eq!(f.base_primary_attribute, 1000.0);
        assert_eq!(f.precision_offset, 895.0);
        assert_eq!(f.tooltip_reference_armor, 2597.0);
        assert_eq!(f.evidence_level, EvidenceLevel::Factual);
    }

    // Source: https://wiki.guildwars2.com/wiki/Critical_Chance
    // At level 80, base Precision is 1000. Crit chance = (1000 - 895) / 21 = 5.0%
    #[test]
    fn test_crit_chance_base_precision() {
        let f = formulas();
        let cc = f.crit_chance(1000.0);
        assert!(
            (cc - 5.0).abs() < 0.01,
            "Precision 1000: expected ~5.0%, got {}",
            cc
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Critical_Chance
    // Precision 2000: (2000 - 895) / 21 = 52.619%
    #[test]
    fn test_crit_chance_high_precision() {
        let f = formulas();
        let cc = f.crit_chance(2000.0);
        assert!(
            (cc - 52.619).abs() < 0.01,
            "Precision 2000: expected ~52.619%, got {}",
            cc
        );
    }

    // Precision 5000: raw = (5000 - 895) / 21 = 195.47..., capped at 100.0
    #[test]
    fn test_crit_chance_capped() {
        let f = formulas();
        let cc = f.crit_chance(5000.0).clamp(0.0, 100.0);
        assert_eq!(cc, 100.0, "Crit chance should be capped at 100.0");
    }

    // Source: https://wiki.guildwars2.com/wiki/Ferocity
    // Ferocity 0: crit damage = 150.0 + 0/15 = 150.0%
    #[test]
    fn test_crit_damage_zero_ferocity() {
        let f = formulas();
        let cd = f.crit_damage(0.0);
        assert!(
            (cd - 150.0).abs() < 0.01,
            "Ferocity 0: expected 150.0%, got {}",
            cd
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Ferocity
    // Ferocity 300: crit damage = 150.0 + 300/15 = 170.0%
    #[test]
    fn test_crit_damage_with_ferocity() {
        let f = formulas();
        let cd = f.crit_damage(300.0);
        assert!(
            (cd - 170.0).abs() < 0.01,
            "Ferocity 300: expected 170.0%, got {}",
            cd
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Health
    // Vitality 1000, base_health 9212: 9212 + 1000*10 = 19212
    #[test]
    fn test_health_from_vitality() {
        let f = formulas();
        let hp = f.health(1000.0, 9212.0);
        assert!(
            (hp - 19212.0).abs() < 0.01,
            "Vitality 1000, base 9212: expected 19212.0, got {}",
            hp
        );
    }

    // Source: https://wiki.guildwars2.com/wiki/Damage
    // Power 2000, skill_damage 500, target_armor 2597:
    // 500 * (2000/1000) * (2597/2597) = 1000.0
    #[test]
    fn test_strike_damage_formula() {
        let f = formulas();
        let dmg = f.strike_damage(500.0, 2000.0, 2597.0);
        assert!((dmg - 1000.0).abs() < 0.01, "Expected 1000.0, got {}", dmg);
    }

    // Power 2000, skill_damage 500, target_armor 2000:
    // 500 * (2000/1000) * (2597/2000) = 1298.5
    #[test]
    fn test_strike_damage_with_different_armor() {
        let f = formulas();
        let dmg = f.strike_damage(500.0, 2000.0, 2000.0);
        assert!((dmg - 1298.5).abs() < 0.01, "Expected 1298.5, got {}", dmg);
    }

    // Source: https://wiki.guildwars2.com/wiki/Damage
    #[test]
    fn test_tooltip_reference_armor_value() {
        assert_eq!(formulas().tooltip_reference_armor, 2597.0);
    }

    // Source: https://wiki.guildwars2.com/wiki/Attribute
    #[test]
    fn test_base_primary_attribute_value() {
        assert_eq!(formulas().base_primary_attribute, 1000.0);
    }

    #[test]
    fn test_validation_rejects_negative_values() {
        let json = r#"{
            "base_primary_attribute": 1000,
            "vitality_to_health": 10,
            "precision_offset": -895,
            "precision_per_crit_pct": 21,
            "ferocity_per_crit_damage_pct": 15,
            "base_crit_damage_pct": 150,
            "expertise_per_condition_duration_pct": 15,
            "concentration_per_boon_duration_pct": 15,
            "condition_duration_cap": 1.0,
            "boon_duration_cap": 1.0,
            "tooltip_reference_armor": 2597,
            "evidence_level": "Factual",
            "sources": []
        }"#;
        let err = load_universal_formulas(json).unwrap_err();
        assert!(
            err.to_string()
                .contains("precision_offset must be positive"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_non_factual() {
        let json = r#"{
            "base_primary_attribute": 1000,
            "vitality_to_health": 10,
            "precision_offset": 895,
            "precision_per_crit_pct": 21,
            "ferocity_per_crit_damage_pct": 15,
            "base_crit_damage_pct": 150,
            "expertise_per_condition_duration_pct": 15,
            "concentration_per_boon_duration_pct": 15,
            "condition_duration_cap": 1.0,
            "boon_duration_cap": 1.0,
            "tooltip_reference_armor": 2597,
            "evidence_level": "Heuristic",
            "sources": []
        }"#;
        let err = load_universal_formulas(json).unwrap_err();
        assert!(
            err.to_string().contains("Factual evidence level"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_wrong_caps() {
        let json = r#"{
            "base_primary_attribute": 1000,
            "vitality_to_health": 10,
            "precision_offset": 895,
            "precision_per_crit_pct": 21,
            "ferocity_per_crit_damage_pct": 15,
            "base_crit_damage_pct": 150,
            "expertise_per_condition_duration_pct": 15,
            "concentration_per_boon_duration_pct": 15,
            "condition_duration_cap": 2.0,
            "boon_duration_cap": 1.0,
            "tooltip_reference_armor": 2597,
            "evidence_level": "Factual",
            "sources": []
        }"#;
        let err = load_universal_formulas(json).unwrap_err();
        assert!(
            err.to_string()
                .contains("condition_duration_cap must be 1.0"),
            "unexpected error: {}",
            err
        );
    }
}
