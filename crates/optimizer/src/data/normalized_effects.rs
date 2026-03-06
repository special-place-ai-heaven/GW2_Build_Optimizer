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
use crate::scoring::OptimizationWeights;
use crate::synergy;

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

// ─── Scoring ───

/// Score a NormalizedEffect using the new 23-category type system.
/// Produces values comparable to `synergy::score_normalized_effect()`.
///
/// Returns 0.0 for Unknown values (can't score what we don't know).
pub fn score_effect(effect: &NormalizedEffect, weights: &OptimizationWeights) -> f64 {
    let base_value = match effect.value {
        FactualValue::Resolved(v) => v,
        FactualValue::Unknown => return 0.0,
    };

    let uptime = effect_uptime(effect);

    let raw_score = match &effect.category {
        // Numeric modifier categories
        EffectCategory::FlatStat => {
            // +100 stat with weight 1.0 → ~0.033 (matching existing normalization)
            // Use average of power/condition as proxy for generic stat value
            let w = (weights.power + weights.condition) * 0.5;
            base_value / 3000.0 * w
        }
        EffectCategory::StatConversion => {
            // Conversion percent → value depends on source/target stat interaction
            // Conservative: treat as fractional stat bonus
            let w = (weights.power + weights.condition) * 0.4;
            base_value / 100.0 * w
        }
        EffectCategory::StrikeDamagePct => {
            // +5% strike damage → 0.05 * weight
            base_value / 100.0 * weights.power
        }
        EffectCategory::ConditionDamagePct => {
            base_value / 100.0 * weights.condition
        }
        EffectCategory::SpecificConditionDamagePct => {
            // Specific condition damage is less universally useful
            base_value / 100.0 * weights.condition * 0.8
        }
        EffectCategory::CritDamagePct => {
            // Crit damage primarily benefits strike builds
            base_value / 100.0 * weights.power * 0.8
        }
        EffectCategory::BoonDurationPct => {
            // Boon duration benefits boon support and healing
            base_value / 100.0 * (weights.boon_support * 0.4 + weights.control * 0.2 + weights.healing * 0.3)
        }
        EffectCategory::ConditionDurationPct => {
            // Condition duration benefits condition DPS and control
            base_value / 100.0 * (weights.condition * 0.8 + weights.control * 0.3)
        }
        EffectCategory::SpecificConditionDurationPct => {
            base_value / 100.0 * weights.condition * 0.5
        }
        EffectCategory::OutgoingHealingPct => {
            base_value / 100.0 * weights.healing
        }
        EffectCategory::IncomingStrikeMultiplier => {
            // Damage reduction: lower is better. Value < 1.0 = damage reduction.
            // Score the reduction amount (1.0 - multiplier) as sustain benefit.
            let reduction = (1.0 - base_value).max(0.0);
            reduction * weights.sustain * 2.0
        }
        EffectCategory::IncomingConditionMultiplier => {
            let reduction = (1.0 - base_value).max(0.0);
            reduction * weights.sustain * 1.5
        }

        // Status operation categories
        EffectCategory::AppliesBoon => {
            // Boon application: value is amount (stacks/duration)
            let boon_w = status_weight_for_scoring(effect, weights, false);
            base_value.min(5.0) * 0.02 * boon_w + boon_w * 0.05
        }
        EffectCategory::AppliesCondition => {
            // Condition application: value is stacks
            let cond_w = status_weight_for_scoring(effect, weights, true);
            base_value.min(5.0) * 0.02 * cond_w + cond_importance_from_op(effect) * 0.03 * weights.condition
        }
        EffectCategory::RemovesBoon => {
            // Boon removal is useful in PvP/WvW, modest in PvE
            base_value.min(5.0) * 0.02 * weights.control * 0.5
        }
        EffectCategory::StealsBoon => {
            // Boon theft = removal + self-application
            base_value.min(5.0) * 0.03 * weights.control
        }
        EffectCategory::CorruptsBoon => {
            // Boon corruption: strong in competitive modes
            base_value.min(5.0) * 0.03 * (weights.control * 0.5 + weights.condition * 0.3)
        }
        EffectCategory::RemovesCondition => {
            // Condition removal: sustain and healing value
            base_value.min(5.0) * 0.02 * (weights.sustain * 0.4 + weights.healing * 0.4)
        }
        EffectCategory::ConvertsConditionToBoon => {
            // Double value: removes condition AND applies boon
            base_value.min(5.0) * 0.04 * (weights.sustain * 0.3 + weights.healing * 0.3)
        }
        EffectCategory::TransfersCondition => {
            // Transfer: removes from self, applies to enemy
            base_value.min(5.0) * 0.03 * (weights.sustain * 0.3 + weights.condition * 0.3)
        }

        // Special categories
        EffectCategory::DefianceDamage => {
            // Defiance break value: primarily control axis
            base_value / 1000.0 * weights.control * 0.5
        }
        EffectCategory::ProcEffect => {
            // Proc scoring: the value is the inner magnitude
            // uptime applied below
            base_value / 3000.0 * weights.power
        }
        EffectCategory::TriggeredEffect => {
            // Triggered: score based on inner_category at reduced effective uptime
            let inner_w = match &effect.inner_category {
                Some(EffectCategory::StrikeDamagePct) => weights.power,
                Some(EffectCategory::ConditionDamagePct) => weights.condition,
                Some(EffectCategory::CritDamagePct) => weights.power * 0.8,
                Some(EffectCategory::FlatStat) => (weights.power + weights.condition) * 0.4,
                Some(EffectCategory::OutgoingHealingPct) => weights.healing,
                _ => 0.3, // Conservative default
            };
            base_value / 100.0 * inner_w * 0.5
        }
    };

    // Apply uptime for non-passive triggers
    match effect.trigger_rule {
        TriggerRule::Passive => raw_score,
        _ => raw_score * uptime,
    }
}

/// Compute effective uptime for an effect based on its uptime model.
fn effect_uptime(effect: &NormalizedEffect) -> f64 {
    match &effect.uptime_model.kind {
        UptimeModelKind::AlwaysOn => 1.0,
        UptimeModelKind::Estimated => {
            effect
                .uptime_model
                .uptime
                .as_ref()
                .and_then(|fv| match fv {
                    FactualValue::Resolved(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or(0.5)
        }
        UptimeModelKind::Derived => {
            // Use provided uptime if available, else 0.5 placeholder
            effect
                .uptime_model
                .uptime
                .as_ref()
                .and_then(|fv| match fv {
                    FactualValue::Resolved(v) => Some(*v),
                    _ => None,
                })
                .unwrap_or(0.5)
        }
        UptimeModelKind::Unknown => 0.3, // Conservative estimate
    }
}

/// Get weight for a boon/condition status application based on the StatusOperation payload.
fn status_weight_for_scoring(
    effect: &NormalizedEffect,
    weights: &OptimizationWeights,
    is_condition: bool,
) -> f64 {
    if is_condition {
        return weights.condition;
    }
    // For boons, check the specific boon name if available
    let status_kind = effect
        .status_operation
        .as_ref()
        .map(|op| op.status_kind.as_str())
        .unwrap_or("");
    match status_kind {
        "Might" => weights.power * 0.5 + weights.condition * 0.5,
        "Fury" => weights.power * 0.7,
        "Quickness" => weights.power * 0.5 + weights.condition * 0.3,
        "Alacrity" => weights.boon_support * 0.4 + weights.power * 0.2,
        "Protection" => weights.sustain * 0.6,
        "Resolution" => weights.sustain * 0.4,
        "Regeneration" => weights.healing * 0.4,
        "Vigor" => weights.sustain * 0.3,
        "Stability" => weights.control * 0.3 + weights.sustain * 0.3 + weights.boon_support * 0.2,
        "Resistance" => weights.sustain * 0.4,
        "Aegis" => weights.sustain * 0.5,
        _ => 0.05,
    }
}

/// Get condition importance from StatusOperation payload.
fn cond_importance_from_op(effect: &NormalizedEffect) -> f64 {
    let status_kind = effect
        .status_operation
        .as_ref()
        .map(|op| op.status_kind.as_str())
        .unwrap_or("");
    match status_kind {
        "Burning" => 1.0,
        "Vulnerability" => 0.8,
        "Bleeding" => 0.7,
        "Torment" => 0.6,
        "Poison" => 0.5,
        "Confusion" => 0.1,
        _ => 0.2,
    }
}

// ─── Legacy Mapping ───

/// Map a legacy 8-variant `synergy::NormalizedEffect` to the new 23-category
/// `NormalizedEffect`. Useful for Phase 2 migration from old extractors.
///
/// The `effect_index` disambiguates multiple effects from the same source.
pub fn map_legacy_effect(
    old: &synergy::NormalizedEffect,
    source_type: SourceType,
    source_id: u32,
    source_name: &str,
    effect_index: usize,
) -> NormalizedEffect {
    let source_prefix = match source_type {
        SourceType::Trait => "trait",
        SourceType::Skill => "skill",
        SourceType::Rune => "rune",
        SourceType::Sigil => "sigil",
        SourceType::Relic => "relic",
    };
    let effect_id = format!("{}:{}:{}", source_prefix, source_id, effect_index);

    match old {
        synergy::NormalizedEffect::StatBonus { stat, value } => NormalizedEffect {
            effect_id,
            source_type,
            source_id,
            source_name: source_name.to_string(),
            category: EffectCategory::FlatStat,
            value: FactualValue::Resolved(*value),
            stacking_rule: StackingRule::Additive,
            trigger_rule: TriggerRule::Passive,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::AlwaysOn,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Derived,
            source: None,
            effect_duration: None,
            internal_cooldown: None,
            max_stacks: None,
            status_operation: None,
            inner_category: Some(map_stat_type_to_hint(stat)),
        },
        synergy::NormalizedEffect::DamageModifier { category, percent } => {
            let (cat, inner) = map_damage_category(category);
            NormalizedEffect {
                effect_id,
                source_type,
                source_id,
                source_name: source_name.to_string(),
                category: cat,
                value: FactualValue::Resolved(*percent),
                stacking_rule: StackingRule::Multiplicative,
                trigger_rule: TriggerRule::Passive,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::AlwaysOn,
                    uptime: None,
                },
                evidence_level: EvidenceLevel::Derived,
                source: None,
                effect_duration: None,
                internal_cooldown: None,
                max_stacks: None,
                status_operation: None,
                inner_category: inner,
            }
        }
        synergy::NormalizedEffect::AppliesStatus {
            status,
            is_condition,
            duration_s,
            stacks,
        } => {
            let cat = if *is_condition {
                EffectCategory::AppliesCondition
            } else {
                EffectCategory::AppliesBoon
            };
            let op_type = if *is_condition {
                OperationType::AppliesCondition
            } else {
                OperationType::AppliesBoon
            };
            NormalizedEffect {
                effect_id,
                source_type,
                source_id,
                source_name: source_name.to_string(),
                category: cat,
                value: FactualValue::Resolved(*stacks as f64),
                stacking_rule: StackingRule::NonStacking,
                trigger_rule: TriggerRule::Passive,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::AlwaysOn,
                    uptime: None,
                },
                evidence_level: EvidenceLevel::Derived,
                source: None,
                effect_duration: Some(FactualValue::Resolved(*duration_s as f64)),
                internal_cooldown: None,
                max_stacks: None,
                status_operation: Some(StatusOperation {
                    operation_type: op_type,
                    target_side: if *is_condition {
                        TargetSide::Enemy
                    } else {
                        TargetSide::Self_
                    },
                    status_kind: status.clone(),
                    amount_mode: AmountMode::Stacks,
                    amount_value: FactualValue::Resolved(*stacks as f64),
                    base_duration_ms: Some(FactualValue::Resolved(duration_s * 1000)),
                    target_scope: if *is_condition {
                        TargetScope::SingleTarget
                    } else {
                        TargetScope::Self_
                    },
                    target_count: None,
                    internal_cooldown_ms: None,
                    source_duration_multiplier: None,
                }),
                inner_category: None,
            }
        }
        synergy::NormalizedEffect::BenefitsFromStatus { status, effect } => {
            // Map inner effect, then wrap as TriggeredEffect with Conditional trigger
            let inner = map_legacy_effect(effect, source_type.clone(), source_id, source_name, effect_index + 100);
            let inner_cat = inner.category.clone();
            NormalizedEffect {
                effect_id,
                source_type,
                source_id,
                source_name: source_name.to_string(),
                category: EffectCategory::TriggeredEffect,
                value: inner.value.clone(),
                stacking_rule: StackingRule::NonStacking,
                trigger_rule: TriggerRule::Conditional,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::Unknown,
                    uptime: None,
                },
                evidence_level: EvidenceLevel::Derived,
                source: Some(format!("Benefits from {}", status)),
                effect_duration: None,
                internal_cooldown: None,
                max_stacks: None,
                status_operation: None,
                inner_category: Some(inner_cat),
            }
        }
        synergy::NormalizedEffect::StatConversion {
            source: _,
            target: _,
            percent,
        } => NormalizedEffect {
            effect_id,
            source_type,
            source_id,
            source_name: source_name.to_string(),
            category: EffectCategory::StatConversion,
            value: FactualValue::Resolved(*percent),
            stacking_rule: StackingRule::Additive,
            trigger_rule: TriggerRule::Passive,
            uptime_model: UptimeModel {
                kind: UptimeModelKind::AlwaysOn,
                uptime: None,
            },
            evidence_level: EvidenceLevel::Derived,
            source: None,
            effect_duration: None,
            internal_cooldown: None,
            max_stacks: None,
            status_operation: None,
            inner_category: None,
        },
        synergy::NormalizedEffect::DurationBonus { kind, percent } => {
            let cat = match kind {
                synergy::DurationKind::AllCondition => EffectCategory::ConditionDurationPct,
                synergy::DurationKind::AllBoon => EffectCategory::BoonDurationPct,
                synergy::DurationKind::SpecificCondition(_) => {
                    EffectCategory::SpecificConditionDurationPct
                }
            };
            NormalizedEffect {
                effect_id,
                source_type,
                source_id,
                source_name: source_name.to_string(),
                category: cat,
                value: FactualValue::Resolved(*percent),
                stacking_rule: StackingRule::Additive,
                trigger_rule: TriggerRule::Passive,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::AlwaysOn,
                    uptime: None,
                },
                evidence_level: EvidenceLevel::Derived,
                source: None,
                effect_duration: None,
                internal_cooldown: None,
                max_stacks: None,
                status_operation: None,
                inner_category: None,
            }
        }
        synergy::NormalizedEffect::Conditional {
            requires_trait_id: _,
            overrides_index: _,
            effect,
        } => {
            // Map inner effect but mark as Conditional trigger
            let mut mapped = map_legacy_effect(
                effect,
                source_type,
                source_id,
                source_name,
                effect_index,
            );
            mapped.effect_id = effect_id;
            mapped.trigger_rule = TriggerRule::Conditional;
            mapped.evidence_level = EvidenceLevel::Derived;
            mapped
        }
        synergy::NormalizedEffect::ProcEffect {
            trigger,
            effect,
            estimated_uptime,
        } => {
            let inner = map_legacy_effect(
                effect,
                source_type.clone(),
                source_id,
                source_name,
                effect_index + 200,
            );
            let trigger_rule = match trigger {
                synergy::ProcTrigger::OnCrit => TriggerRule::OnCrit,
                synergy::ProcTrigger::OnHit => TriggerRule::OnHit,
                synergy::ProcTrigger::OnDodge | synergy::ProcTrigger::OnWeaponSwap => {
                    TriggerRule::OnSkillUse
                }
                synergy::ProcTrigger::OnKill => TriggerRule::OnHit,
                synergy::ProcTrigger::OnHealthThreshold => TriggerRule::OnHealthThreshold,
                synergy::ProcTrigger::Passive => TriggerRule::Passive,
            };
            NormalizedEffect {
                effect_id,
                source_type,
                source_id,
                source_name: source_name.to_string(),
                category: EffectCategory::ProcEffect,
                value: inner.value.clone(),
                stacking_rule: StackingRule::NonStacking,
                trigger_rule,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::Estimated,
                    uptime: Some(FactualValue::Resolved(*estimated_uptime)),
                },
                evidence_level: EvidenceLevel::Heuristic,
                source: None,
                effect_duration: inner.effect_duration.clone(),
                internal_cooldown: None,
                max_stacks: None,
                status_operation: inner.status_operation.clone(),
                inner_category: Some(inner.category.clone()),
            }
        }
    }
}

/// Map a legacy `StatType` to a hint category for `inner_category`.
fn map_stat_type_to_hint(stat: &synergy::StatType) -> EffectCategory {
    match stat {
        synergy::StatType::Power | synergy::StatType::Precision | synergy::StatType::Ferocity => {
            EffectCategory::StrikeDamagePct
        }
        synergy::StatType::ConditionDamage | synergy::StatType::Expertise => {
            EffectCategory::ConditionDamagePct
        }
        synergy::StatType::Concentration => EffectCategory::BoonDurationPct,
        synergy::StatType::HealingPower => EffectCategory::OutgoingHealingPct,
        synergy::StatType::Toughness | synergy::StatType::Vitality => {
            EffectCategory::IncomingStrikeMultiplier
        }
    }
}

/// Map a legacy `DamageCategory` to new category.
fn map_damage_category(
    category: &synergy::DamageCategory,
) -> (EffectCategory, Option<EffectCategory>) {
    match category {
        synergy::DamageCategory::Strike => (EffectCategory::StrikeDamagePct, None),
        synergy::DamageCategory::Condition => (EffectCategory::ConditionDamagePct, None),
        synergy::DamageCategory::SpecificCondition(_) => {
            (EffectCategory::SpecificConditionDamagePct, None)
        }
        synergy::DamageCategory::Crit => (EffectCategory::CritDamagePct, None),
        synergy::DamageCategory::Healing => (EffectCategory::OutgoingHealingPct, None),
    }
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
        // P3-10b populates baseline with representative effects
        assert!(
            data.effect_count() >= 20,
            "expected at least 20 total effects, got {}",
            data.effect_count(),
        );
    }

    #[test]
    fn test_try_load_returns_ok() {
        let result = try_load_normalized_effects();
        assert!(result.is_ok(), "try_load should succeed: {:?}", result.err());
    }

    #[test]
    fn test_effects_for_returns_populated_slices() {
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE");
        assert!(pve.is_some(), "PvE effects should exist");
        assert!(
            !pve.unwrap().is_empty(),
            "PvE should have populated effects after P3-10b"
        );

        let pvp = data.effects_for("2026-01-13", "PvP");
        assert!(pvp.is_some(), "PvP effects should exist");
        assert!(
            !pvp.unwrap().is_empty(),
            "PvP should have populated effects after P3-10b"
        );

        let wvw = data.effects_for("2026-01-13", "WvW");
        assert!(wvw.is_some(), "WvW effects should exist");
        assert!(
            !wvw.unwrap().is_empty(),
            "WvW should have populated effects after P3-10b"
        );
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

    // ─── P3-10b: score_effect tests ───

    #[test]
    fn test_score_effect_flat_stat() {
        let weights = OptimizationWeights {
            power: 1.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let mut effect = minimal_effect("score_flat_stat");
        effect.category = EffectCategory::FlatStat;
        effect.value = FactualValue::Resolved(150.0);
        let score = score_effect(&effect, &weights);
        // 150 / 3000 * (1.0 + 0.0) * 0.5 = 0.025
        assert!(score > 0.01, "FlatStat should produce positive score, got {}", score);
        assert!(score < 0.1, "FlatStat +150 should be modest, got {}", score);
    }

    #[test]
    fn test_score_effect_strike_damage() {
        let weights = OptimizationWeights {
            power: 1.0,
            condition: 0.0,
            boon_support: 0.0,
            healing: 0.0,
            sustain: 0.0,
            control: 0.0,
        };
        let mut effect = minimal_effect("score_strike");
        effect.category = EffectCategory::StrikeDamagePct;
        effect.value = FactualValue::Resolved(5.0);
        let score = score_effect(&effect, &weights);
        // 5.0 / 100 * 1.0 = 0.05
        assert!(
            (score - 0.05).abs() < 0.001,
            "StrikeDamagePct +5% should score ~0.05, got {}",
            score,
        );
    }

    #[test]
    fn test_score_effect_unknown_value_returns_zero() {
        let weights = OptimizationWeights {
            power: 1.0,
            condition: 0.5,
            boon_support: 0.2,
            healing: 0.3,
            sustain: 0.3,
            control: 0.3,
        };
        let mut effect = minimal_effect("score_unknown");
        effect.value = FactualValue::Unknown;
        let score = score_effect(&effect, &weights);
        assert!(
            (score - 0.0).abs() < f64::EPSILON,
            "Unknown value should score 0.0, got {}",
            score,
        );
    }

    // ─── P3-10b: map_legacy_effect tests ───

    #[test]
    fn test_map_legacy_stat_bonus() {
        let old = synergy::NormalizedEffect::StatBonus {
            stat: synergy::StatType::Power,
            value: 150.0,
        };
        let new = map_legacy_effect(&old, SourceType::Trait, 100, "Test Trait", 0);
        assert_eq!(new.category, EffectCategory::FlatStat);
        assert_eq!(new.value, FactualValue::Resolved(150.0));
        assert_eq!(new.trigger_rule, TriggerRule::Passive);
        assert_eq!(new.stacking_rule, StackingRule::Additive);
        assert_eq!(new.effect_id, "trait:100:0");
        assert_eq!(new.evidence_level, EvidenceLevel::Derived);
    }

    #[test]
    fn test_map_legacy_damage_modifier() {
        let old = synergy::NormalizedEffect::DamageModifier {
            category: synergy::DamageCategory::Strike,
            percent: 5.0,
        };
        let new = map_legacy_effect(&old, SourceType::Sigil, 24615, "Sigil of Force", 0);
        assert_eq!(new.category, EffectCategory::StrikeDamagePct);
        assert_eq!(new.value, FactualValue::Resolved(5.0));
        assert_eq!(new.stacking_rule, StackingRule::Multiplicative);
        assert_eq!(new.effect_id, "sigil:24615:0");
    }

    #[test]
    fn test_map_legacy_applies_status_condition() {
        let old = synergy::NormalizedEffect::AppliesStatus {
            status: "Bleeding".into(),
            is_condition: true,
            duration_s: 5,
            stacks: 3,
        };
        let new = map_legacy_effect(&old, SourceType::Sigil, 24560, "Sigil of Earth", 0);
        assert_eq!(new.category, EffectCategory::AppliesCondition);
        assert_eq!(new.value, FactualValue::Resolved(3.0));
        let op = new.status_operation.as_ref().expect("should have status_operation");
        assert_eq!(op.operation_type, OperationType::AppliesCondition);
        assert_eq!(op.status_kind, "Bleeding");
        assert_eq!(op.amount_mode, AmountMode::Stacks);
        assert_eq!(op.amount_value, FactualValue::Resolved(3.0));
        assert_eq!(op.base_duration_ms, Some(FactualValue::Resolved(5000)));
        assert_eq!(op.target_side, TargetSide::Enemy);
        assert_eq!(op.target_scope, TargetScope::SingleTarget);
    }

    #[test]
    fn test_map_legacy_applies_status_boon() {
        let old = synergy::NormalizedEffect::AppliesStatus {
            status: "Might".into(),
            is_condition: false,
            duration_s: 8,
            stacks: 1,
        };
        let new = map_legacy_effect(&old, SourceType::Trait, 214, "Phalanx Strength", 0);
        assert_eq!(new.category, EffectCategory::AppliesBoon);
        let op = new.status_operation.as_ref().expect("should have status_operation");
        assert_eq!(op.operation_type, OperationType::AppliesBoon);
        assert_eq!(op.status_kind, "Might");
        assert_eq!(op.target_side, TargetSide::Self_);
        assert_eq!(op.target_scope, TargetScope::Self_);
    }

    // ─── P3-10b: baseline data tests ───

    #[test]
    fn test_baseline_data_loads_and_validates() {
        // All three baseline files load and pass validation
        let data = effects();
        assert_eq!(data.file_count(), 3);

        // PvE should have the most entries
        let pve = data.effects_for("2026-01-13", "PvE").unwrap();
        assert!(
            pve.len() >= 20,
            "PvE should have at least 20 representative entries, got {}",
            pve.len(),
        );

        // All entries should have unique effect_ids (validation already ensures this,
        // but verify it held through deserialization)
        let mut ids: HashSet<&str> = HashSet::new();
        for effect in pve {
            assert!(
                ids.insert(&effect.effect_id),
                "duplicate effect_id in PvE baseline: {}",
                effect.effect_id,
            );
        }
    }

    #[test]
    fn test_mode_split_effect() {
        // Same source should have different values in PvE vs PvP
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE").unwrap();
        let pvp = data.effects_for("2026-01-13", "PvP").unwrap();

        // Find Sigil of Force in PvE (5% strike damage)
        let pve_force = pve
            .iter()
            .find(|e| e.effect_id == "sigil:24615:0")
            .expect("Sigil of Force should be in PvE baseline");
        // Find Sigil of Force in PvP (different value)
        let pvp_force = pvp
            .iter()
            .find(|e| e.effect_id == "sigil:24615:0")
            .expect("Sigil of Force should be in PvP baseline");

        // PvE Sigil of Force: +5% strike damage
        assert_eq!(pve_force.value, FactualValue::Resolved(5.0));
        // PvP Sigil of Force: +3% (split balance)
        assert_eq!(pvp_force.value, FactualValue::Resolved(3.0));
        // Values should differ between modes
        assert_ne!(pve_force.value, pvp_force.value);
    }

    #[test]
    fn test_proc_vs_triggered_boundary() {
        // Verify ProcEffect has inner_category and correct trigger
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE").unwrap();

        // Find a ProcEffect entry (Sigil of Fire)
        let proc_effect = pve
            .iter()
            .find(|e| e.category == EffectCategory::ProcEffect)
            .expect("should have at least one ProcEffect in PvE baseline");

        // ProcEffect should have inner_category
        assert!(
            proc_effect.inner_category.is_some(),
            "ProcEffect should have inner_category, effect: {}",
            proc_effect.effect_id,
        );
        // ProcEffect trigger should not be Passive
        assert_ne!(
            proc_effect.trigger_rule,
            TriggerRule::Passive,
            "ProcEffect should have non-passive trigger"
        );

        // Find a TriggeredEffect entry
        let triggered = pve
            .iter()
            .find(|e| e.category == EffectCategory::TriggeredEffect)
            .expect("should have at least one TriggeredEffect in PvE baseline");

        // TriggeredEffect must have inner_category (validation enforces this)
        assert!(
            triggered.inner_category.is_some(),
            "TriggeredEffect should have inner_category"
        );
        // TriggeredEffect typically uses Conditional or OnHealthThreshold
        assert!(
            matches!(
                triggered.trigger_rule,
                TriggerRule::Conditional | TriggerRule::OnHealthThreshold
            ),
            "TriggeredEffect should have Conditional or OnHealthThreshold trigger, got {:?}",
            triggered.trigger_rule,
        );
    }

    #[test]
    fn test_score_comparable_to_legacy() {
        // New scorer should produce similar magnitude as old for comparable inputs
        let weights = OptimizationWeights {
            power: 0.8,
            condition: 0.2,
            boon_support: 0.05,
            healing: 0.0,
            sustain: 0.1,
            control: 0.05,
        };

        // Old: StatBonus(Power, 150)
        let old_stat = synergy::NormalizedEffect::StatBonus {
            stat: synergy::StatType::Power,
            value: 150.0,
        };
        let old_score = synergy::score_normalized_effect(&old_stat, &weights);

        // New: equivalent FlatStat effect
        let mut new_eff = minimal_effect("compare_flat_stat");
        new_eff.category = EffectCategory::FlatStat;
        new_eff.value = FactualValue::Resolved(150.0);
        let new_score = score_effect(&new_eff, &weights);

        // Both should be positive and in similar ballpark (within 5x)
        assert!(old_score > 0.0, "old score should be positive");
        assert!(new_score > 0.0, "new score should be positive");
        let ratio = if old_score > new_score {
            old_score / new_score
        } else {
            new_score / old_score
        };
        assert!(
            ratio < 5.0,
            "scores should be comparable magnitude: old={}, new={}, ratio={}",
            old_score,
            new_score,
            ratio,
        );

        // Old: DamageModifier(Strike, 5%)
        let old_dmg = synergy::NormalizedEffect::DamageModifier {
            category: synergy::DamageCategory::Strike,
            percent: 5.0,
        };
        let old_dmg_score = synergy::score_normalized_effect(&old_dmg, &weights);

        let mut new_dmg = minimal_effect("compare_strike_dmg");
        new_dmg.category = EffectCategory::StrikeDamagePct;
        new_dmg.value = FactualValue::Resolved(5.0);
        let new_dmg_score = score_effect(&new_dmg, &weights);

        assert!(old_dmg_score > 0.0);
        assert!(new_dmg_score > 0.0);
        let dmg_ratio = if old_dmg_score > new_dmg_score {
            old_dmg_score / new_dmg_score
        } else {
            new_dmg_score / old_dmg_score
        };
        assert!(
            dmg_ratio < 5.0,
            "damage scores should be comparable: old={}, new={}, ratio={}",
            old_dmg_score,
            new_dmg_score,
            dmg_ratio,
        );
    }

    // ─── P3-10b: category coverage in baseline ───

    #[test]
    fn test_baseline_category_coverage() {
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE").unwrap();

        // Collect all categories present in PvE baseline
        let categories: HashSet<String> = pve
            .iter()
            .map(|e| format!("{:?}", e.category))
            .collect();

        // Must cover at least these core categories
        let required = [
            "FlatStat",
            "StrikeDamagePct",
            "ConditionDamagePct",
            "AppliesBoon",
            "AppliesCondition",
            "ProcEffect",
            "TriggeredEffect",
        ];
        for cat in &required {
            assert!(
                categories.contains(*cat),
                "PvE baseline must cover category {}, found: {:?}",
                cat,
                categories,
            );
        }
    }

    // ─── P3-10b: source type coverage ───

    #[test]
    fn test_baseline_source_type_coverage() {
        let data = effects();
        let pve = data.effects_for("2026-01-13", "PvE").unwrap();

        let source_types: HashSet<String> = pve
            .iter()
            .map(|e| format!("{:?}", e.source_type))
            .collect();

        // Should have Trait, Rune, Sigil at minimum
        for st in &["Trait", "Rune", "Sigil"] {
            assert!(
                source_types.contains(*st),
                "PvE baseline must include source type {}, found: {:?}",
                st,
                source_types,
            );
        }
    }
}
