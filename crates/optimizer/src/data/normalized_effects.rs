//! Normalized effect type system for the GW2 Build Optimizer.
//!
//! Defines the structured representation of every modifier that traits, skills,
//! runes, sigils, and relics produce. This is the **type system and schema only** —
//! data population happens in P3-10b and heuristic uptime modeling in P3-14.
//!
//! Each `NormalizedEffect` captures:
//! - What source produces it (trait, skill, rune, sigil, relic)
//! - What category of effect it is (23 variants from flat stat to triggered effect)
//! - How it stacks with other effects of the same category
//! - When it triggers (passive, on-crit, on-hit, etc.)
//! - Uptime modeling metadata
//! - Optional `StatusOperation` payload for boon/condition interaction categories

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use thiserror::Error;

use super::quality::FactualValue;
use super::{DataLoadError, EvidenceLevel};

/// Serde helper for `Option<FactualValue<T>>` with 3-state JSON mapping:
/// - field absent → None (not applicable)
/// - null → Some(Unknown) (applicable but value not yet sourced)
/// - value → Some(Resolved(v)) (factually known)
mod optional_factual {
    use super::super::quality::FactualValue;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<T: Serialize, S: Serializer>(
        value: &Option<FactualValue<T>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match value {
            None => serializer.serialize_none(),
            Some(fv) => fv.serialize(serializer),
        }
    }

    pub fn deserialize<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<FactualValue<T>>, D::Error> {
        // When this is called, the field IS present in JSON
        let opt = Option::<T>::deserialize(deserializer)?;
        Ok(Some(match opt {
            Some(v) => FactualValue::Resolved(v),
            None => FactualValue::Unknown,
        }))
    }
}

// ─── Embedded baseline JSON (compile-time) ───

const PVE_EFFECTS_JSON: &str =
    include_str!("../../../../data/normalized_effects/2026-01-13/pve.json");
const PVP_EFFECTS_JSON: &str =
    include_str!("../../../../data/normalized_effects/2026-01-13/pvp.json");
const WVW_EFFECTS_JSON: &str =
    include_str!("../../../../data/normalized_effects/2026-01-13/wvw.json");

static EFFECTS: OnceLock<NormalizedEffectsData> = OnceLock::new();

/// Returns the globally loaded normalized effects, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn effects() -> &'static NormalizedEffectsData {
    EFFECTS.get_or_init(|| {
        load_all_effects().expect("embedded normalized_effects JSON is invalid")
    })
}

/// Try to load all normalized effects from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_normalized_effects() -> Result<(), Vec<DataLoadError>> {
    load_all_effects()
        .map(|_| ())
        .map_err(|e| {
            vec![match e {
                NormalizedEffectError::ParseError(pe) => DataLoadError::ParseError {
                    source: "normalized_effects".into(),
                    detail: pe.to_string(),
                },
                NormalizedEffectError::ValidationError(msg) => DataLoadError::ValidationError {
                    source: "normalized_effects".into(),
                    field: String::new(),
                    reason: msg,
                },
            }]
        })
}

// ─── Error type ───

#[derive(Debug, Error)]
pub enum NormalizedEffectError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

// ─── Enums ───

/// The type of game entity that produces this effect.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SourceType {
    Trait,
    Skill,
    Rune,
    Sigil,
    Relic,
}

/// Category of effect — determines how the optimizer interprets and applies the value.
///
/// Categories 0-11: numeric modifiers (stat bonuses, damage multipliers, duration bonuses).
/// Categories 12-19: status operations (boon/condition application, removal, conversion).
/// Categories 20-22: special (defiance damage, proc effects, triggered effects).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EffectCategory {
    FlatStat,
    StatConversion,
    StrikeDamagePct,
    ConditionDamagePct,
    SpecificConditionDamagePct,
    CritDamagePct,
    BoonDurationPct,
    ConditionDurationPct,
    SpecificConditionDurationPct,
    OutgoingHealingPct,
    IncomingStrikeMultiplier,
    IncomingConditionMultiplier,
    AppliesBoon,
    AppliesCondition,
    RemovesBoon,
    StealsBoon,
    CorruptsBoon,
    RemovesCondition,
    ConvertsConditionToBoon,
    TransfersCondition,
    DefianceDamage,
    ProcEffect,
    TriggeredEffect,
}

impl EffectCategory {
    /// Returns true if this category represents a status operation
    /// (boon/condition application, removal, conversion, or transfer).
    /// These categories should have a `status_operation` payload.
    pub fn is_status_operation(&self) -> bool {
        matches!(
            self,
            EffectCategory::AppliesBoon
                | EffectCategory::AppliesCondition
                | EffectCategory::RemovesBoon
                | EffectCategory::StealsBoon
                | EffectCategory::CorruptsBoon
                | EffectCategory::RemovesCondition
                | EffectCategory::ConvertsConditionToBoon
                | EffectCategory::TransfersCondition
        )
    }
}

/// How multiple instances of this effect stack with each other.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum StackingRule {
    /// Effects multiply together: (1 + a) * (1 + b).
    Multiplicative,
    /// Effects add together: a + b.
    Additive,
    /// Only the highest value applies.
    Highest,
    /// Effect does not stack — only one instance active at a time.
    NonStacking,
}

/// When this effect activates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TriggerRule {
    /// Always active while the source is equipped/traited.
    Passive,
    /// Triggers on critical hit.
    OnCrit,
    /// Triggers on any hit (strike or condition tick).
    OnHit,
    /// Triggers on skill use.
    OnSkillUse,
    /// Triggers when health crosses a threshold.
    OnHealthThreshold,
    /// Triggers based on a custom condition (e.g., "while above 90% health").
    Conditional,
}

// ─── Uptime model ───

/// How the uptime value was determined.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UptimeModelKind {
    /// Effect is always active (100% uptime). Typical for passive traits.
    AlwaysOn,
    /// Uptime was estimated based on typical gameplay patterns.
    Estimated,
    /// Uptime is derived from other known values (e.g., ICD + proc chance).
    Derived,
    /// Uptime is unknown — no data available.
    Unknown,
}

/// Metadata about how often an effect is active.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UptimeModel {
    pub kind: UptimeModelKind,
    /// Fractional uptime (0.0 to 1.0). Only meaningful for `Estimated` or `Derived` kinds.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub uptime: Option<FactualValue<f64>>,
}

// ─── StatusOperation ───

/// The type of boon/condition operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OperationType {
    AppliesBoon,
    RemovesBoon,
    StealsBoon,
    CorruptsBoon,
    AppliesCondition,
    RemovesCondition,
    ConvertsConditionToBoon,
    TransfersCondition,
}

/// Which side the operation targets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TargetSide {
    #[serde(rename = "self")]
    Self_,
    Ally,
    Enemy,
}

/// How the amount is measured.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AmountMode {
    Stacks,
    DurationMs,
    Charges,
    Count,
}

/// How many targets the operation affects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TargetScope {
    #[serde(rename = "self")]
    Self_,
    SingleTarget,
    NearbyAllies,
    Party,
    Squad,
    Area,
}

/// Describes a boon or condition operation (application, removal, conversion, etc.).
///
/// Used as a payload for status-interaction effect categories (AppliesBoon through
/// TransfersCondition).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusOperation {
    /// The type of operation being performed.
    pub operation_type: OperationType,
    /// Which side the operation targets (self, ally, enemy).
    pub target_side: TargetSide,
    /// The boon or condition name (e.g., "Might", "Burning", "Protection").
    pub status_kind: String,
    /// How the amount value is interpreted.
    pub amount_mode: AmountMode,
    /// Numeric amount (stacks, duration, charges, or count depending on mode).
    pub amount_value: FactualValue<f64>,
    /// Base duration of the applied status in milliseconds, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub base_duration_ms: Option<FactualValue<u32>>,
    /// How many/what kind of targets are affected.
    pub target_scope: TargetScope,
    /// Maximum number of targets affected. `None` means unlimited or N/A.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub target_count: Option<FactualValue<u32>>,
    /// Internal cooldown of this specific operation in milliseconds.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub internal_cooldown_ms: Option<FactualValue<u32>>,
    /// Multiplier applied to the source's boon/condition duration stat.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub source_duration_multiplier: Option<FactualValue<f64>>,
}

// ─── NormalizedEffect ───

/// A single normalized effect — the structured representation of one modifier
/// produced by a trait, skill, rune, sigil, or relic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedEffect {
    /// Unique identifier for this effect (e.g., "trait_214_might_on_crit").
    pub effect_id: String,
    /// Type of source entity producing this effect.
    pub source_type: SourceType,
    /// GW2 API ID of the source entity.
    pub source_id: u32,
    /// Human-readable name of the source entity.
    pub source_name: String,
    /// Category of effect — determines interpretation and stacking behavior.
    pub category: EffectCategory,
    /// Primary numeric value of the effect (meaning depends on category).
    pub value: FactualValue<f64>,
    /// How this effect stacks with other effects of the same category.
    pub stacking_rule: StackingRule,
    /// When this effect activates.
    pub trigger_rule: TriggerRule,
    /// Uptime model metadata.
    pub uptime_model: UptimeModel,
    /// Evidence level for this effect's data.
    pub evidence_level: EvidenceLevel,
    /// Optional source citation (wiki URL, patch notes, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    // Timer/cap metadata

    /// Duration of the effect in seconds, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub effect_duration: Option<FactualValue<f64>>,
    /// Internal cooldown in seconds, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub internal_cooldown: Option<FactualValue<f64>>,
    /// Maximum number of stacks this effect can have.
    #[serde(default, skip_serializing_if = "Option::is_none", with = "optional_factual")]
    pub max_stacks: Option<FactualValue<u32>>,

    // Interaction payload (for status operation categories)

    /// Detailed boon/condition operation payload. Required for categories
    /// AppliesBoon through TransfersCondition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_operation: Option<StatusOperation>,

    // TriggeredEffect inner category

    /// For `TriggeredEffect` category: the inner effect category that is triggered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inner_category: Option<EffectCategory>,
}

// ─── File wrapper ───

/// A single normalized effects file for one game mode in a specific patch.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedEffectsFile {
    pub patch_id: String,
    pub mode: String,
    pub effects: Vec<NormalizedEffect>,
}

// ─── Container ───

/// Container for all loaded normalized effects, keyed by (patch_id, mode).
#[derive(Debug)]
pub struct NormalizedEffectsData {
    /// Map from (patch_id, mode) to parsed effects file.
    files: HashMap<(String, String), NormalizedEffectsFile>,
}

impl NormalizedEffectsData {
    /// Look up all effects for a given patch and mode.
    pub fn effects_for(&self, patch_id: &str, mode: &str) -> Option<&[NormalizedEffect]> {
        self.files
            .get(&(patch_id.to_string(), mode.to_string()))
            .map(|f| f.effects.as_slice())
    }

    /// Number of loaded effects files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total number of effects across all files.
    pub fn effect_count(&self) -> usize {
        self.files.values().map(|f| f.effects.len()).sum()
    }
}

// ─── Loading ───

/// Parse and validate a single normalized effects file from JSON text.
pub fn load_effects_file(json: &str) -> Result<NormalizedEffectsFile, NormalizedEffectError> {
    let file: NormalizedEffectsFile = serde_json::from_str(json)?;
    validate_effects_file(&file)?;
    Ok(file)
}

fn validate_effects_file(file: &NormalizedEffectsFile) -> Result<(), NormalizedEffectError> {
    // 1. patch_id must not be empty
    if file.patch_id.is_empty() {
        return Err(NormalizedEffectError::ValidationError(
            "patch_id must not be empty".into(),
        ));
    }

    // 2. mode must be valid
    let valid_modes = ["PvE", "PvP", "WvW"];
    if !valid_modes.contains(&file.mode.as_str()) {
        return Err(NormalizedEffectError::ValidationError(format!(
            "invalid mode '{}', expected one of: PvE, PvP, WvW",
            file.mode
        )));
    }

    // 3. No duplicate effect_id within a file
    let mut seen_ids = HashSet::new();
    for effect in &file.effects {
        if !seen_ids.insert(&effect.effect_id) {
            return Err(NormalizedEffectError::ValidationError(format!(
                "duplicate effect_id: '{}'",
                effect.effect_id
            )));
        }

        // 4. If uptime_model.kind == Estimated AND evidence_level != Heuristic → error
        if effect.uptime_model.kind == UptimeModelKind::Estimated
            && effect.evidence_level != EvidenceLevel::Heuristic
        {
            return Err(NormalizedEffectError::ValidationError(format!(
                "effect '{}': Estimated uptime requires Heuristic evidence_level, got {:?}",
                effect.effect_id, effect.evidence_level
            )));
        }

        // 5. If trigger_rule == Passive AND internal_cooldown.is_some() → error
        if effect.trigger_rule == TriggerRule::Passive && effect.internal_cooldown.is_some() {
            return Err(NormalizedEffectError::ValidationError(format!(
                "effect '{}': Passive trigger_rule should not have internal_cooldown",
                effect.effect_id
            )));
        }

        // 6. Status operation categories should have status_operation payload
        if effect.category.is_status_operation() && effect.status_operation.is_none() {
            return Err(NormalizedEffectError::ValidationError(format!(
                "effect '{}': category {:?} requires status_operation payload",
                effect.effect_id, effect.category
            )));
        }

        // 7. TriggeredEffect must have inner_category
        if effect.category == EffectCategory::TriggeredEffect && effect.inner_category.is_none() {
            return Err(NormalizedEffectError::ValidationError(format!(
                "effect '{}': TriggeredEffect category requires inner_category",
                effect.effect_id
            )));
        }
    }

    Ok(())
}

/// Load and validate all three baseline effects files.
fn load_all_effects() -> Result<NormalizedEffectsData, NormalizedEffectError> {
    let mut files = HashMap::new();

    for (json, expected_mode) in [
        (PVE_EFFECTS_JSON, "PvE"),
        (PVP_EFFECTS_JSON, "PvP"),
        (WVW_EFFECTS_JSON, "WvW"),
    ] {
        let file = load_effects_file(json)?;

        // Validate mode matches expected
        if file.mode != expected_mode {
            return Err(NormalizedEffectError::ValidationError(format!(
                "expected mode '{}', got '{}'",
                expected_mode, file.mode
            )));
        }

        files.insert((file.patch_id.clone(), file.mode.clone()), file);
    }

    Ok(NormalizedEffectsData { files })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Helper to build a minimal NormalizedEffect for testing ───

    fn minimal_effect(effect_id: &str) -> NormalizedEffect {
        NormalizedEffect {
            effect_id: effect_id.to_string(),
            source_type: SourceType::Trait,
            source_id: 100,
            source_name: "Test Trait".to_string(),
            category: EffectCategory::FlatStat,
            value: FactualValue::Resolved(150.0),
            stacking_rule: StackingRule::Additive,
            trigger_rule: TriggerRule::Passive,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::AlwaysOn,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Factual,
            source: None,
            effect_duration: None,
            internal_cooldown: None,
            max_stacks: None,
            status_operation: None,
            inner_category: None,
        }
    }

    fn full_effect() -> NormalizedEffect {
        NormalizedEffect {
            effect_id: "trait_214_might_on_crit".to_string(),
            source_type: SourceType::Trait,
            source_id: 214,
            source_name: "Signet of Fury".to_string(),
            category: EffectCategory::AppliesBoon,
            value: FactualValue::Resolved(1.0),
            stacking_rule: StackingRule::NonStacking,
            trigger_rule: TriggerRule::OnCrit,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::Estimated,
                uptime: Some(FactualValue::Resolved(0.6)),
            },
            evidence_level: EvidenceLevel::Heuristic,
            source: Some("https://wiki.guildwars2.com/wiki/Signet_of_Fury".to_string()),
            effect_duration: Some(FactualValue::Resolved(10.0)),
            internal_cooldown: Some(FactualValue::Resolved(1.0)),
            max_stacks: Some(FactualValue::Resolved(25)),
            status_operation: Some(StatusOperation {
                operation_type: OperationType::AppliesBoon,
                target_side: TargetSide::Self_,
                status_kind: "Might".to_string(),
                amount_mode: AmountMode::Stacks,
                amount_value: FactualValue::Resolved(1.0),
                base_duration_ms: Some(FactualValue::Resolved(8000)),
                target_scope: TargetScope::Self_,
                target_count: None,
                internal_cooldown_ms: Some(FactualValue::Resolved(1000)),
                source_duration_multiplier: Some(FactualValue::Resolved(1.0)),
            }),
            inner_category: None,
        }
    }

    // ─── 1. Serde round-trip for each enum ───

    #[test]
    fn test_serde_roundtrip_source_type() {
        let variants = vec![
            SourceType::Trait,
            SourceType::Skill,
            SourceType::Rune,
            SourceType::Sigil,
            SourceType::Relic,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: SourceType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_all_23_effect_categories() {
        let variants = vec![
            EffectCategory::FlatStat,
            EffectCategory::StatConversion,
            EffectCategory::StrikeDamagePct,
            EffectCategory::ConditionDamagePct,
            EffectCategory::SpecificConditionDamagePct,
            EffectCategory::CritDamagePct,
            EffectCategory::BoonDurationPct,
            EffectCategory::ConditionDurationPct,
            EffectCategory::SpecificConditionDurationPct,
            EffectCategory::OutgoingHealingPct,
            EffectCategory::IncomingStrikeMultiplier,
            EffectCategory::IncomingConditionMultiplier,
            EffectCategory::AppliesBoon,
            EffectCategory::AppliesCondition,
            EffectCategory::RemovesBoon,
            EffectCategory::StealsBoon,
            EffectCategory::CorruptsBoon,
            EffectCategory::RemovesCondition,
            EffectCategory::ConvertsConditionToBoon,
            EffectCategory::TransfersCondition,
            EffectCategory::DefianceDamage,
            EffectCategory::ProcEffect,
            EffectCategory::TriggeredEffect,
        ];
        assert_eq!(variants.len(), 23, "must test all 23 EffectCategory variants");
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: EffectCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed, "roundtrip failed for {:?}", v);
        }
    }

    #[test]
    fn test_serde_roundtrip_stacking_rule() {
        let variants = vec![
            StackingRule::Multiplicative,
            StackingRule::Additive,
            StackingRule::Highest,
            StackingRule::NonStacking,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: StackingRule = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_trigger_rule() {
        let variants = vec![
            TriggerRule::Passive,
            TriggerRule::OnCrit,
            TriggerRule::OnHit,
            TriggerRule::OnSkillUse,
            TriggerRule::OnHealthThreshold,
            TriggerRule::Conditional,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: TriggerRule = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_uptime_model_kind() {
        let variants = vec![
            UptimeModelKind::AlwaysOn,
            UptimeModelKind::Estimated,
            UptimeModelKind::Derived,
            UptimeModelKind::Unknown,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: UptimeModelKind = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_operation_type() {
        let variants = vec![
            OperationType::AppliesBoon,
            OperationType::RemovesBoon,
            OperationType::StealsBoon,
            OperationType::CorruptsBoon,
            OperationType::AppliesCondition,
            OperationType::RemovesCondition,
            OperationType::ConvertsConditionToBoon,
            OperationType::TransfersCondition,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: OperationType = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_target_side() {
        let variants = vec![TargetSide::Self_, TargetSide::Ally, TargetSide::Enemy];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: TargetSide = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_amount_mode() {
        let variants = vec![
            AmountMode::Stacks,
            AmountMode::DurationMs,
            AmountMode::Charges,
            AmountMode::Count,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: AmountMode = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_target_scope() {
        let variants = vec![
            TargetScope::Self_,
            TargetScope::SingleTarget,
            TargetScope::NearbyAllies,
            TargetScope::Party,
            TargetScope::Squad,
            TargetScope::Area,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: TargetScope = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    #[test]
    fn test_serde_roundtrip_evidence_level() {
        let variants = vec![
            EvidenceLevel::Factual,
            EvidenceLevel::Derived,
            EvidenceLevel::Heuristic,
            EvidenceLevel::Unknown,
        ];
        for v in variants {
            let json = serde_json::to_string(&v).unwrap();
            let parsed: EvidenceLevel = serde_json::from_str(&json).unwrap();
            assert_eq!(v, parsed);
        }
    }

    // ─── 2. Serde round-trip for NormalizedEffect with all fields ───

    #[test]
    fn test_serde_roundtrip_full_effect() {
        let effect = full_effect();
        let json = serde_json::to_string_pretty(&effect).unwrap();
        let parsed: NormalizedEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);
    }

    // ─── 3. Serde round-trip for NormalizedEffect with minimal fields ───

    #[test]
    fn test_serde_roundtrip_minimal_effect() {
        let effect = minimal_effect("test_minimal");
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: NormalizedEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);

        // Verify optional fields are absent from JSON
        let json_value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(json_value.get("source").is_none(), "source should be skipped");
        assert!(
            json_value.get("effect_duration").is_none(),
            "effect_duration should be skipped"
        );
        assert!(
            json_value.get("internal_cooldown").is_none(),
            "internal_cooldown should be skipped"
        );
        assert!(json_value.get("max_stacks").is_none(), "max_stacks should be skipped");
        assert!(
            json_value.get("status_operation").is_none(),
            "status_operation should be skipped"
        );
        assert!(
            json_value.get("inner_category").is_none(),
            "inner_category should be skipped"
        );
    }

    // ─── 4. NormalizedEffectsFile with empty effects array ───

    #[test]
    fn test_effects_file_empty_effects() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvE",
            "effects": []
        }"#;
        let file = load_effects_file(json).expect("should parse");
        assert_eq!(file.patch_id, "2026-01-13");
        assert_eq!(file.mode, "PvE");
        assert!(file.effects.is_empty());
    }

    // ─── 5. Validation: duplicate effect_id → error ───

    #[test]
    fn test_validation_duplicate_effect_id() {
        let effect1 = minimal_effect("dup_id");
        let effect2 = minimal_effect("dup_id");
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect1, effect2],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("duplicate effect_id"),
            "expected duplicate error, got: {}",
            err
        );
    }

    // ─── 6. Validation: Estimated uptime with Factual evidence → error ───

    #[test]
    fn test_validation_estimated_uptime_requires_heuristic() {
        let mut effect = minimal_effect("bad_uptime");
        effect.uptime_model = UptimeModel {
            kind: UptimeModelKind::Estimated,
            uptime: Some(FactualValue::Resolved(0.5)),
        };
        effect.evidence_level = EvidenceLevel::Factual; // Wrong! Should be Heuristic.
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("Estimated uptime requires Heuristic"),
            "expected uptime/evidence error, got: {}",
            err
        );
    }

    // ─── 7. Validation: Passive trigger with ICD → error ───

    #[test]
    fn test_validation_passive_with_icd() {
        let mut effect = minimal_effect("passive_icd");
        effect.trigger_rule = TriggerRule::Passive;
        effect.internal_cooldown = Some(FactualValue::Resolved(1.0));
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("Passive trigger_rule should not have internal_cooldown"),
            "expected passive/ICD error, got: {}",
            err
        );
    }

    // ─── 8. Validation: TriggeredEffect without inner_category → error ───

    #[test]
    fn test_validation_triggered_effect_requires_inner_category() {
        let mut effect = minimal_effect("bad_triggered");
        effect.category = EffectCategory::TriggeredEffect;
        effect.inner_category = None; // Missing!
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("TriggeredEffect category requires inner_category"),
            "expected inner_category error, got: {}",
            err
        );
    }

    // ─── 9. Validation: AppliesBoon without status_operation → warning (error) ───

    #[test]
    fn test_validation_applies_boon_requires_status_operation() {
        let mut effect = minimal_effect("boon_no_op");
        effect.category = EffectCategory::AppliesBoon;
        effect.status_operation = None; // Missing!
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("requires status_operation payload"),
            "expected status_operation error, got: {}",
            err
        );
    }

    #[test]
    fn test_validation_all_status_categories_require_operation() {
        let status_categories = vec![
            EffectCategory::AppliesBoon,
            EffectCategory::AppliesCondition,
            EffectCategory::RemovesBoon,
            EffectCategory::StealsBoon,
            EffectCategory::CorruptsBoon,
            EffectCategory::RemovesCondition,
            EffectCategory::ConvertsConditionToBoon,
            EffectCategory::TransfersCondition,
        ];
        for cat in status_categories {
            let mut effect = minimal_effect("status_test");
            effect.category = cat.clone();
            effect.status_operation = None;
            let file = NormalizedEffectsFile {
                patch_id: "2026-01-13".to_string(),
                mode: "PvE".to_string(),
                effects: vec![effect],
            };
            let result = validate_effects_file(&file);
            assert!(
                result.is_err(),
                "category {:?} should require status_operation",
                cat
            );
        }
    }

    // ─── 10. Loader: baseline files parse successfully ───

    #[test]
    fn test_embedded_effects_load_successfully() {
        let data = effects();
        assert_eq!(data.file_count(), 3, "expected 3 effects files (PvE, PvP, WvW)");
        assert_eq!(data.effect_count(), 0, "baseline has no effects");
    }

    #[test]
    fn test_try_load_returns_ok() {
        let result = try_load_normalized_effects();
        assert!(result.is_ok(), "try_load should succeed: {:?}", result.err());
    }

    #[test]
    fn test_effects_for_returns_empty_slice() {
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE");
        assert!(pve.is_some(), "PvE effects should exist");
        assert!(pve.unwrap().is_empty(), "baseline PvE should have no effects");

        let pvp = data.effects_for("2026-01-13", "PvP");
        assert!(pvp.is_some(), "PvP effects should exist");

        let wvw = data.effects_for("2026-01-13", "WvW");
        assert!(wvw.is_some(), "WvW effects should exist");
    }

    #[test]
    fn test_effects_for_unknown_returns_none() {
        let data = effects();
        assert!(data.effects_for("9999-99-99", "PvE").is_none());
        assert!(data.effects_for("2026-01-13", "Ranked").is_none());
    }

    // ─── 11. Loader: malformed JSON → DataLoadError ───

    #[test]
    fn test_malformed_json_returns_error() {
        let result = load_effects_file("not valid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("JSON parse error"),
            "expected parse error, got: {}",
            err,
        );
    }

    // ─── 12. Full NormalizedEffect with StatusOperation deserialization ───

    #[test]
    fn test_full_effect_with_status_operation_from_json() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvE",
            "effects": [
                {
                    "effect_id": "trait_214_might_on_crit",
                    "source_type": "Trait",
                    "source_id": 214,
                    "source_name": "Signet of Fury",
                    "category": "AppliesBoon",
                    "value": 1.0,
                    "stacking_rule": "NonStacking",
                    "trigger_rule": "OnCrit",
                    "uptime_model": {
                        "kind": "Estimated",
                        "uptime": 0.6
                    },
                    "evidence_level": "Heuristic",
                    "source": "https://wiki.guildwars2.com/wiki/Signet_of_Fury",
                    "effect_duration": 10.0,
                    "internal_cooldown": 1.0,
                    "max_stacks": 25,
                    "status_operation": {
                        "operation_type": "AppliesBoon",
                        "target_side": "self",
                        "status_kind": "Might",
                        "amount_mode": "Stacks",
                        "amount_value": 1.0,
                        "base_duration_ms": 8000,
                        "target_scope": "self",
                        "target_count": null,
                        "internal_cooldown_ms": 1000,
                        "source_duration_multiplier": 1.0
                    }
                }
            ]
        }"#;
        let file = load_effects_file(json).expect("should parse");
        assert_eq!(file.effects.len(), 1);

        let effect = &file.effects[0];
        assert_eq!(effect.effect_id, "trait_214_might_on_crit");
        assert_eq!(effect.source_type, SourceType::Trait);
        assert_eq!(effect.source_id, 214);
        assert_eq!(effect.category, EffectCategory::AppliesBoon);
        assert_eq!(effect.trigger_rule, TriggerRule::OnCrit);
        assert_eq!(effect.evidence_level, EvidenceLevel::Heuristic);
        assert_eq!(effect.max_stacks, Some(FactualValue::Resolved(25)));

        let op = effect.status_operation.as_ref().expect("should have status_operation");
        assert_eq!(op.operation_type, OperationType::AppliesBoon);
        assert_eq!(op.target_side, TargetSide::Self_);
        assert_eq!(op.status_kind, "Might");
        assert_eq!(op.amount_mode, AmountMode::Stacks);
        assert_eq!(op.amount_value, FactualValue::Resolved(1.0));
        assert_eq!(op.base_duration_ms, Some(FactualValue::Resolved(8000)));
        assert_eq!(op.target_scope, TargetScope::Self_);
        assert_eq!(op.target_count, Some(FactualValue::Unknown));
        assert_eq!(op.internal_cooldown_ms, Some(FactualValue::Resolved(1000)));
    }

    // ─── 13. TargetSide/TargetScope "self" rename ───

    #[test]
    fn test_self_rename_in_json() {
        // TargetSide::Self_ serializes to "self" (Rust keyword workaround)
        let json = serde_json::to_string(&TargetSide::Self_).unwrap();
        assert_eq!(json, r#""self""#);
        let parsed: TargetSide = serde_json::from_str(r#""self""#).unwrap();
        assert_eq!(parsed, TargetSide::Self_);

        // TargetScope::Self_ serializes to "self"
        let json = serde_json::to_string(&TargetScope::Self_).unwrap();
        assert_eq!(json, r#""self""#);
        let parsed: TargetScope = serde_json::from_str(r#""self""#).unwrap();
        assert_eq!(parsed, TargetScope::Self_);
    }

    // ─── Validation: valid effects pass ───

    #[test]
    fn test_validation_valid_effect_passes() {
        let effect = minimal_effect("valid_effect");
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_ok(), "valid effect should pass: {:?}", result.err());
    }

    #[test]
    fn test_validation_estimated_with_heuristic_passes() {
        let mut effect = minimal_effect("estimated_ok");
        effect.uptime_model = UptimeModel {
            kind: UptimeModelKind::Estimated,
            uptime: Some(FactualValue::Resolved(0.75)),
        };
        effect.evidence_level = EvidenceLevel::Heuristic; // Correct!
        // Non-passive to avoid ICD conflict
        effect.trigger_rule = TriggerRule::OnCrit;
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_ok(), "Estimated + Heuristic should pass: {:?}", result.err());
    }

    #[test]
    fn test_validation_triggered_effect_with_inner_passes() {
        let mut effect = minimal_effect("triggered_ok");
        effect.category = EffectCategory::TriggeredEffect;
        effect.inner_category = Some(EffectCategory::FlatStat); // Present!
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_ok(), "TriggeredEffect with inner should pass: {:?}", result.err());
    }

    // ─── is_status_operation helper ───

    #[test]
    fn test_is_status_operation() {
        // These 8 categories are status operations
        assert!(EffectCategory::AppliesBoon.is_status_operation());
        assert!(EffectCategory::AppliesCondition.is_status_operation());
        assert!(EffectCategory::RemovesBoon.is_status_operation());
        assert!(EffectCategory::StealsBoon.is_status_operation());
        assert!(EffectCategory::CorruptsBoon.is_status_operation());
        assert!(EffectCategory::RemovesCondition.is_status_operation());
        assert!(EffectCategory::ConvertsConditionToBoon.is_status_operation());
        assert!(EffectCategory::TransfersCondition.is_status_operation());

        // These are NOT status operations
        assert!(!EffectCategory::FlatStat.is_status_operation());
        assert!(!EffectCategory::StrikeDamagePct.is_status_operation());
        assert!(!EffectCategory::DefianceDamage.is_status_operation());
        assert!(!EffectCategory::ProcEffect.is_status_operation());
        assert!(!EffectCategory::TriggeredEffect.is_status_operation());
    }

    // ─── Error path: empty patch_id and invalid mode ───

    #[test]
    fn test_empty_patch_id_rejected() {
        let json = r#"{
            "patch_id": "",
            "mode": "PvE",
            "effects": []
        }"#;
        let result = load_effects_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("patch_id must not be empty"));
    }

    #[test]
    fn test_invalid_mode_rejected() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "Ranked",
            "effects": []
        }"#;
        let result = load_effects_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid mode"));
    }

    // ─── StatusOperation serde round-trip ───

    #[test]
    fn test_serde_roundtrip_status_operation() {
        let op = StatusOperation {
            operation_type: OperationType::CorruptsBoon,
            target_side: TargetSide::Enemy,
            status_kind: "Stability".to_string(),
            amount_mode: AmountMode::Stacks,
            amount_value: FactualValue::Resolved(2.0),
            base_duration_ms: None,
            target_scope: TargetScope::Area,
            target_count: Some(FactualValue::Resolved(5)),
            internal_cooldown_ms: Some(FactualValue::Resolved(3000)),
            source_duration_multiplier: None,
        };
        let json = serde_json::to_string(&op).unwrap();
        let parsed: StatusOperation = serde_json::from_str(&json).unwrap();
        assert_eq!(op, parsed);
    }

    // ─── UptimeModel serde round-trip ───

    #[test]
    fn test_serde_roundtrip_uptime_model_always_on() {
        let model = UptimeModel {
            kind: UptimeModelKind::AlwaysOn,
            uptime: None,
        };
        let json = serde_json::to_string(&model).unwrap();
        let parsed: UptimeModel = serde_json::from_str(&json).unwrap();
        assert_eq!(model, parsed);
    }

    #[test]
    fn test_serde_roundtrip_uptime_model_estimated() {
        let model = UptimeModel {
            kind: UptimeModelKind::Estimated,
            uptime: Some(FactualValue::Resolved(0.85)),
        };
        let json = serde_json::to_string(&model).unwrap();
        let parsed: UptimeModel = serde_json::from_str(&json).unwrap();
        assert_eq!(model, parsed);
    }

    // ─── Deserialization from various source types ───

    #[test]
    fn test_all_source_types_in_json() {
        for (source_type_str, expected) in [
            ("Trait", SourceType::Trait),
            ("Skill", SourceType::Skill),
            ("Rune", SourceType::Rune),
            ("Sigil", SourceType::Sigil),
            ("Relic", SourceType::Relic),
        ] {
            let json = format!(
                r#"{{
                    "effect_id": "test_{source_type_str}",
                    "source_type": "{source_type_str}",
                    "source_id": 1,
                    "source_name": "Test",
                    "category": "FlatStat",
                    "value": 100.0,
                    "stacking_rule": "Additive",
                    "trigger_rule": "Passive",
                    "uptime_model": {{ "kind": "AlwaysOn" }},
                    "evidence_level": "Factual"
                }}"#
            );
            let effect: NormalizedEffect =
                serde_json::from_str(&json).expect(&format!("should parse {}", source_type_str));
            assert_eq!(effect.source_type, expected);
        }
    }

    // ─── TriggeredEffect with inner_category round-trip ───

    #[test]
    fn test_triggered_effect_with_inner_category_roundtrip() {
        let mut effect = minimal_effect("triggered_roundtrip");
        effect.category = EffectCategory::TriggeredEffect;
        effect.inner_category = Some(EffectCategory::AppliesBoon);
        // Add status_operation since we claim inner is AppliesBoon but validation
        // only checks the outer category — TriggeredEffect doesn't require status_operation
        let json = serde_json::to_string(&effect).unwrap();
        let parsed: NormalizedEffect = serde_json::from_str(&json).unwrap();
        assert_eq!(effect, parsed);
        assert_eq!(parsed.inner_category, Some(EffectCategory::AppliesBoon));
    }

    // ─── Non-passive trigger with ICD is valid ───

    #[test]
    fn test_on_crit_with_icd_is_valid() {
        let mut effect = minimal_effect("crit_icd");
        effect.trigger_rule = TriggerRule::OnCrit;
        effect.internal_cooldown = Some(FactualValue::Resolved(1.0));
        let file = NormalizedEffectsFile {
            patch_id: "2026-01-13".to_string(),
            mode: "PvE".to_string(),
            effects: vec![effect],
        };
        let result = validate_effects_file(&file);
        assert!(result.is_ok(), "OnCrit + ICD should be valid: {:?}", result.err());
    }

    // ─── FactualValue deserialization: null → Unknown ───

    #[test]
    fn test_value_null_deserializes_to_unknown() {
        let json = r#"{
            "effect_id": "test_unknown_value",
            "source_type": "Trait",
            "source_id": 1,
            "source_name": "Test",
            "category": "FlatStat",
            "value": null,
            "stacking_rule": "Additive",
            "trigger_rule": "Passive",
            "uptime_model": { "kind": "AlwaysOn" },
            "evidence_level": "Unknown"
        }"#;
        let effect: NormalizedEffect = serde_json::from_str(json).unwrap();
        assert_eq!(effect.value, FactualValue::Unknown);
    }

    // ─── 3-state Option<FactualValue<T>> test ───

    #[test]
    fn test_three_state_option_factual_value() {
        // State 1: field absent → None
        let json = r#"{
            "effect_id": "test_absent",
            "source_type": "Trait",
            "source_id": 1,
            "source_name": "Test",
            "category": "FlatStat",
            "value": 100.0,
            "stacking_rule": "Additive",
            "trigger_rule": "Passive",
            "uptime_model": { "kind": "AlwaysOn" },
            "evidence_level": "Factual"
        }"#;
        let effect: NormalizedEffect = serde_json::from_str(json).unwrap();
        assert_eq!(effect.effect_duration, None, "absent field → None");
        assert_eq!(effect.max_stacks, None, "absent field → None");

        // State 2: field = null → Some(Unknown)
        let json = r#"{
            "effect_id": "test_null",
            "source_type": "Trait",
            "source_id": 1,
            "source_name": "Test",
            "category": "FlatStat",
            "value": 100.0,
            "stacking_rule": "Additive",
            "trigger_rule": "Passive",
            "uptime_model": { "kind": "AlwaysOn" },
            "evidence_level": "Factual",
            "effect_duration": null,
            "max_stacks": null
        }"#;
        let effect: NormalizedEffect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect.effect_duration,
            Some(FactualValue::Unknown),
            "null → Some(Unknown)"
        );
        assert_eq!(
            effect.max_stacks,
            Some(FactualValue::Unknown),
            "null → Some(Unknown)"
        );

        // State 3: field = value → Some(Resolved(v))
        let json = r#"{
            "effect_id": "test_resolved",
            "source_type": "Trait",
            "source_id": 1,
            "source_name": "Test",
            "category": "FlatStat",
            "value": 100.0,
            "stacking_rule": "Additive",
            "trigger_rule": "Passive",
            "uptime_model": { "kind": "AlwaysOn" },
            "evidence_level": "Factual",
            "effect_duration": 5.0,
            "max_stacks": 10
        }"#;
        let effect: NormalizedEffect = serde_json::from_str(json).unwrap();
        assert_eq!(
            effect.effect_duration,
            Some(FactualValue::Resolved(5.0)),
            "value → Some(Resolved)"
        );
        assert_eq!(
            effect.max_stacks,
            Some(FactualValue::Resolved(10)),
            "value → Some(Resolved)"
        );
    }

    // ─── StatusOperation with FactualValue fields ───

    #[test]
    fn test_status_operation_amount_value_unknown() {
        let json = r#"{
            "operation_type": "AppliesBoon",
            "target_side": "self",
            "status_kind": "Might",
            "amount_mode": "Stacks",
            "amount_value": null,
            "target_scope": "self"
        }"#;
        let op: StatusOperation = serde_json::from_str(json).unwrap();
        assert_eq!(op.amount_value, FactualValue::Unknown);
    }
}
