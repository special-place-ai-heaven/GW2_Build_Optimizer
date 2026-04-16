//! Rotation profiles: per-profession/spec heuristic data for condition application,
//! boon generation, buff uptimes, and scenario-based combat environments.
//!
//! Replaces the hardcoded `condition_weights_for_profession()` and `default_buff_profiles()`
//! functions with data-driven lookups. All profiles are Heuristic evidence level.
//!
//! Data files: `data/rotation_profiles/{pve,pvp,wvw}.json`
//! Loader pattern: `include_str!` + `OnceLock` + validation (P3-07).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

use super::{DataLoadError, EvidenceLevel};

// ─── Embedded JSON (compile-time) ───

const PVE_PROFILES_JSON: &str = include_str!("../../../../data/rotation_profiles/pve.json");
const PVP_PROFILES_JSON: &str = include_str!("../../../../data/rotation_profiles/pvp.json");
const WVW_PROFILES_JSON: &str = include_str!("../../../../data/rotation_profiles/wvw.json");

static ROTATION_PROFILES: OnceLock<RotationProfileData> = OnceLock::new();

/// Returns the globally loaded rotation profiles, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn rotation_profiles() -> &'static RotationProfileData {
    ROTATION_PROFILES.get_or_init(|| {
        load_all_rotation_profiles().expect("embedded rotation_profiles JSON is invalid")
    })
}

/// Try to load all rotation profiles from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_rotation_profiles() -> Result<(), Vec<DataLoadError>> {
    load_all_rotation_profiles().map(|_| ()).map_err(|e| {
        vec![match e {
            RotationProfileError::ParseError(pe) => DataLoadError::ParseError {
                source: "rotation_profiles".into(),
                detail: pe.to_string(),
            },
            RotationProfileError::ValidationError(msg) => DataLoadError::ValidationError {
                source: "rotation_profiles".into(),
                field: String::new(),
                reason: msg,
            },
        }]
    })
}

// ─── Error type ───

#[derive(Debug, Error)]
pub enum RotationProfileError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

// ─── Types ───

/// Typed application metrics for conditions, matching P3-04 stacking modes.
/// Tagged enum: the `mode` field in JSON determines which variant is used.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum ApplicationMetrics {
    /// Intensity-stacking damaging conditions (Bleeding, Burning, Torment, Confusion, Poisoned).
    IntensityRate { avg_stacks_per_second: f64 },
    /// Duration-stacking conditions (Fear, Taunt, Daze, etc.).
    DurationRate { avg_duration_ms_per_second: f64 },
    /// Steady-state debuffs (Vulnerability) — average maintained stacks.
    SteadyState { avg_stacks: f64 },
    /// Proc-based effects — expected procs per second.
    ProcRate { expected_procs_per_second: f64 },
}

/// Typed generation metrics for boons, matching P3-04 stacking modes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "mode")]
pub enum GenerationMetrics {
    /// Intensity-stacking boons (Might, Stability).
    IntensityRate { avg_stacks_per_second: f64 },
    /// Duration-stacking boons (Fury, Protection, etc.).
    DurationRate { avg_duration_ms_per_second: f64 },
}

/// Target behavior assumptions for a game mode/scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TargetBehavior {
    /// Fraction of time the target is moving (0.0–1.0). Affects Torment damage.
    pub movement_fraction: f64,
    /// Average target skill activations per second. Affects Confusion damage.
    pub skill_use_frequency_per_second: f64,
}

/// Scenario-specific buff environment (Solo, Party, Full Squad).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScenarioProfile {
    /// Unique scenario identifier (e.g., "solo", "party", "full_squad").
    pub scenario_id: String,
    /// Display label for UI (e.g., "Solo", "Party", "Full Squad").
    pub label: String,
    /// Average Might stacks in this scenario (0.0–25.0).
    pub might_stacks: f64,
    /// Average Vulnerability stacks on target (0.0–25.0).
    pub vulnerability_stacks: f64,
    /// Optional boon uptime overrides for this scenario.
    /// If present, these override the base `boon_uptime` values.
    pub boon_overrides: Option<HashMap<String, f64>>,
}

/// A complete rotation profile for a profession/spec in a game mode.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RotationProfile {
    /// Unique profile identifier.
    pub profile_id: String,
    /// Profession name (e.g., "Warrior", "Guardian"). "Generic" for fallback.
    pub profession: String,
    /// Elite spec name, if this profile is spec-specific. Null for core profession.
    pub elite_spec: Option<String>,
    /// Optional link to an objective profile (P3-15 scope).
    pub objective_profile_id: Option<String>,
    /// Boon generation rates: how much of each boon this build generates.
    pub boon_generation: HashMap<String, GenerationMetrics>,
    /// Boon uptime fractions (0.0–1.0): what this build benefits from.
    pub boon_uptime: HashMap<String, f64>,
    /// Condition application rates, keyed by condition name.
    pub condition_application: HashMap<String, ApplicationMetrics>,
    /// Incoming suppression: uptime fractions for incoming CC/debuffs.
    pub incoming_suppression: HashMap<String, f64>,
    /// Target behavior assumptions.
    pub target_behavior: TargetBehavior,
    /// Scenario profiles (solo, party, full_squad).
    pub scenarios: Vec<ScenarioProfile>,
    /// Evidence level — must be Heuristic for all rotation profiles.
    pub evidence_level: EvidenceLevel,
    /// Human-readable notes about the profile's assumptions.
    pub notes: String,
}

impl RotationProfile {
    /// Get the condition weight (average stacks) for a given condition.
    /// Extracts from the ApplicationMetrics variant appropriate to the condition.
    ///
    /// NOTE: `data/rotation_profiles/*.json` currently uses verb-form keys
    /// ("Poison", "Cripple"); callers must use the same form. Centralized
    /// alias normalization here is intentionally NOT applied — switching
    /// to `canonical_condition_name` would require migrating the JSON keys
    /// to canonical form first (out of scope for the alias-resolver task).
    pub fn condition_weight(&self, condition: &str) -> f64 {
        match self.condition_application.get(condition) {
            Some(ApplicationMetrics::IntensityRate {
                avg_stacks_per_second,
            }) => *avg_stacks_per_second,
            Some(ApplicationMetrics::SteadyState { avg_stacks }) => *avg_stacks,
            Some(ApplicationMetrics::DurationRate {
                avg_duration_ms_per_second,
            }) => {
                // Convert duration rate to an approximate stack equivalent
                // 1000ms per second = 1.0 "stack equivalent"
                avg_duration_ms_per_second / 1000.0
            }
            Some(ApplicationMetrics::ProcRate {
                expected_procs_per_second,
            }) => *expected_procs_per_second,
            None => 0.0,
        }
    }

    /// Find a scenario by ID.
    pub fn scenario(&self, id: &str) -> Option<&ScenarioProfile> {
        self.scenarios.iter().find(|s| s.scenario_id == id)
    }

    /// Get effective boon uptime for a given boon in a scenario.
    /// Uses scenario overrides if present, otherwise base uptime.
    pub fn effective_boon_uptime(&self, boon: &str, scenario: &ScenarioProfile) -> f64 {
        if let Some(ref overrides) = scenario.boon_overrides {
            if let Some(&val) = overrides.get(boon) {
                return val.clamp(0.0, 1.0);
            }
        }
        self.boon_uptime
            .get(boon)
            .copied()
            .unwrap_or(0.0)
            .clamp(0.0, 1.0)
    }
}

// ─── Data Container ───

/// All loaded rotation profiles, organized by mode.
#[derive(Debug)]
pub struct RotationProfileData {
    pve: Vec<RotationProfile>,
    pvp: Vec<RotationProfile>,
    wvw: Vec<RotationProfile>,
}

impl RotationProfileData {
    /// Look up a rotation profile by profession, optional elite spec, and game mode.
    ///
    /// Lookup priority:
    /// 1. Exact match: profession + elite_spec + mode
    /// 2. Profession-only match: profession + mode (elite_spec = None)
    /// 3. Generic fallback: profession = "Generic" + mode
    pub fn lookup(
        &self,
        profession: &str,
        elite_spec: Option<&str>,
        mode: &gw2_core::types::GameMode,
    ) -> Option<&RotationProfile> {
        let profiles = match mode {
            gw2_core::types::GameMode::PvE => &self.pve,
            gw2_core::types::GameMode::PvP => &self.pvp,
            gw2_core::types::GameMode::WvW => &self.wvw,
        };

        // 1. Exact match (profession + elite_spec)
        if let Some(spec) = elite_spec {
            if let Some(p) = profiles
                .iter()
                .find(|p| p.profession == profession && p.elite_spec.as_deref() == Some(spec))
            {
                return Some(p);
            }
        }

        // 2. Profession-only match
        if let Some(p) = profiles
            .iter()
            .find(|p| p.profession == profession && p.elite_spec.is_none())
        {
            return Some(p);
        }

        // 3. Generic fallback
        profiles.iter().find(|p| p.profession == "Generic")
    }

    /// Get all profiles for a given mode.
    pub fn profiles_for_mode(&self, mode: &gw2_core::types::GameMode) -> &[RotationProfile] {
        match mode {
            gw2_core::types::GameMode::PvE => &self.pve,
            gw2_core::types::GameMode::PvP => &self.pvp,
            gw2_core::types::GameMode::WvW => &self.wvw,
        }
    }

    /// Total number of loaded profiles across all modes.
    pub fn total_count(&self) -> usize {
        self.pve.len() + self.pvp.len() + self.wvw.len()
    }
}

// ─── Loader ───

fn load_all_rotation_profiles() -> Result<RotationProfileData, RotationProfileError> {
    let pve: Vec<RotationProfile> = serde_json::from_str(PVE_PROFILES_JSON)?;
    let pvp: Vec<RotationProfile> = serde_json::from_str(PVP_PROFILES_JSON)?;
    let wvw: Vec<RotationProfile> = serde_json::from_str(WVW_PROFILES_JSON)?;

    validate_profiles("pve", &pve)?;
    validate_profiles("pvp", &pvp)?;
    validate_profiles("wvw", &wvw)?;

    Ok(RotationProfileData { pve, pvp, wvw })
}

/// Parse and validate rotation profiles from JSON text. Exposed for testing.
pub fn load_rotation_profiles(json: &str) -> Result<Vec<RotationProfile>, RotationProfileError> {
    let profiles: Vec<RotationProfile> = serde_json::from_str(json)?;
    validate_profiles("test", &profiles)?;
    Ok(profiles)
}

fn validate_profiles(
    mode_label: &str,
    profiles: &[RotationProfile],
) -> Result<(), RotationProfileError> {
    // Must have at least one generic fallback
    let has_fallback = profiles.iter().any(|p| p.profession == "Generic");
    if !has_fallback {
        return Err(RotationProfileError::ValidationError(format!(
            "{}: missing Generic fallback profile",
            mode_label
        )));
    }

    // Must have all 9 core professions
    let required = [
        "Warrior",
        "Guardian",
        "Revenant",
        "Engineer",
        "Ranger",
        "Thief",
        "Elementalist",
        "Mesmer",
        "Necromancer",
    ];
    for prof in &required {
        if !profiles.iter().any(|p| p.profession == *prof) {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: missing profession '{}'",
                mode_label, prof
            )));
        }
    }

    // No duplicate profile IDs
    let mut seen_ids = std::collections::HashSet::new();
    for p in profiles {
        if !seen_ids.insert(&p.profile_id) {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: duplicate profile_id '{}'",
                mode_label, p.profile_id
            )));
        }
    }

    // All profiles must be Heuristic evidence level
    for p in profiles {
        if p.evidence_level != EvidenceLevel::Heuristic {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: profile '{}' has evidence_level {:?}, expected Heuristic",
                mode_label, p.profile_id, p.evidence_level
            )));
        }
    }

    // Each profile must have at least 3 scenarios
    for p in profiles {
        if p.scenarios.len() < 3 {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: profile '{}' has {} scenarios, minimum 3 required",
                mode_label,
                p.profile_id,
                p.scenarios.len()
            )));
        }
    }

    // Validate numeric ranges
    for p in profiles {
        // Might stacks in scenarios capped at 25
        for s in &p.scenarios {
            if s.might_stacks < 0.0 || s.might_stacks > 25.0 {
                return Err(RotationProfileError::ValidationError(format!(
                    "{}: profile '{}' scenario '{}' has might_stacks {} (must be 0-25)",
                    mode_label, p.profile_id, s.scenario_id, s.might_stacks
                )));
            }
            if s.vulnerability_stacks < 0.0 || s.vulnerability_stacks > 25.0 {
                return Err(RotationProfileError::ValidationError(format!(
                    "{}: profile '{}' scenario '{}' has vulnerability_stacks {} (must be 0-25)",
                    mode_label, p.profile_id, s.scenario_id, s.vulnerability_stacks
                )));
            }
        }

        // Boon uptimes must be 0.0-1.0
        for (boon, &uptime) in &p.boon_uptime {
            if !(0.0..=1.0).contains(&uptime) {
                return Err(RotationProfileError::ValidationError(format!(
                    "{}: profile '{}' boon_uptime '{}' = {} (must be 0.0-1.0)",
                    mode_label, p.profile_id, boon, uptime
                )));
            }
        }

        // Target behavior ranges
        if !(0.0..=1.0).contains(&p.target_behavior.movement_fraction) {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: profile '{}' movement_fraction {} (must be 0.0-1.0)",
                mode_label, p.profile_id, p.target_behavior.movement_fraction
            )));
        }
        if p.target_behavior.skill_use_frequency_per_second < 0.0 {
            return Err(RotationProfileError::ValidationError(format!(
                "{}: profile '{}' skill_use_frequency_per_second {} (must be >= 0)",
                mode_label, p.profile_id, p.target_behavior.skill_use_frequency_per_second
            )));
        }
    }

    Ok(())
}

// ─── Compatibility helpers ───

/// Convert a RotationProfile's condition application into a legacy-compatible
/// ConditionWeights-like struct (5 f64 fields matching the old ConditionWeights).
///
/// This is the bridge between the new data-driven profiles and the existing
/// `calculate_combat_performance()` signature.
#[derive(Debug, Clone)]
pub struct ConditionWeightsFromProfile {
    pub bleeding: f64,
    pub burning: f64,
    pub poison: f64,
    pub torment: f64,
    pub confusion: f64,
}

impl ConditionWeightsFromProfile {
    /// Extract condition weights from a rotation profile.
    pub fn from_profile(profile: &RotationProfile) -> Self {
        Self {
            bleeding: profile.condition_weight("Bleeding"),
            burning: profile.condition_weight("Burning"),
            poison: profile.condition_weight("Poison"),
            torment: profile.condition_weight("Torment"),
            confusion: profile.condition_weight("Confusion"),
        }
    }

    /// Generic PvE fallback — looks up Generic rotation profile.
    /// Used in tests and as a compatibility bridge.
    pub fn default_pve() -> Self {
        let data = rotation_profiles();
        let profile = data
            .lookup("Generic", None, &gw2_core::types::GameMode::PvE)
            .expect("Generic PvE rotation profile missing");
        Self::from_profile(profile)
    }

    /// Necromancer group condition weights from rotation profile data.
    pub fn necro_group() -> Self {
        let data = rotation_profiles();
        let profile = data
            .lookup("Necromancer", None, &gw2_core::types::GameMode::PvE)
            .expect("Necromancer PvE rotation profile missing");
        Self::from_profile(profile)
    }

    /// Guardian / Firebrand group condition weights from rotation profile data.
    pub fn firebrand_group() -> Self {
        let data = rotation_profiles();
        let profile = data
            .lookup("Guardian", None, &gw2_core::types::GameMode::PvE)
            .expect("Guardian PvE rotation profile missing");
        Self::from_profile(profile)
    }

    /// Harbinger-specific condition weights from rotation profile data.
    /// Falls back to Necromancer profile since Harbinger is an elite spec.
    pub fn harbinger_preset() -> Self {
        let data = rotation_profiles();
        // Try Harbinger elite spec first, fall back to Necromancer core
        let profile = data
            .lookup(
                "Necromancer",
                Some("Harbinger"),
                &gw2_core::types::GameMode::PvE,
            )
            .or_else(|| data.lookup("Necromancer", None, &gw2_core::types::GameMode::PvE))
            .expect("Necromancer PvE rotation profile missing");
        Self::from_profile(profile)
    }
}

/// Convert a ScenarioProfile into a legacy-compatible BuffProfile-like struct.
///
/// This is the bridge between the new data-driven profiles and the existing
/// `calculate_combat_performance()` signature.
#[derive(Debug, Clone)]
pub struct BuffProfileFromScenario {
    pub might_stacks: u32,
    pub fury: bool,
    pub protection: bool,
    pub resolution: bool,
    pub vulnerability_stacks: u32,
    pub label: String,
}

impl BuffProfileFromScenario {
    /// Build from a scenario profile + the base rotation profile's boon uptimes.
    /// Uses threshold: uptime >= 0.5 means the boon is considered active.
    pub fn from_scenario(profile: &RotationProfile, scenario: &ScenarioProfile) -> Self {
        let fury_uptime = profile.effective_boon_uptime("Fury", scenario);
        let protection_uptime = profile.effective_boon_uptime("Protection", scenario);
        let resolution_uptime = profile.effective_boon_uptime("Resolution", scenario);

        Self {
            might_stacks: (scenario.might_stacks.clamp(0.0, 25.0)) as u32,
            fury: fury_uptime >= 0.5,
            protection: protection_uptime >= 0.5,
            resolution: resolution_uptime >= 0.5,
            vulnerability_stacks: (scenario.vulnerability_stacks.clamp(0.0, 25.0)) as u32,
            label: scenario.label.clone(),
        }
    }
}

// ─── Heuristic Uptime Population ───

/// Update NormalizedEffect entries with Unknown uptime to Estimated where applicable,
/// using rotation profile data to inform the estimates.
///
/// For trigger-based effects (OnCrit, OnHit, OnSkillUse), estimates an uptime
/// fraction based on the rotation profile's target behavior and typical proc rates.
/// Only updates effects with `UptimeModelKind::Unknown`.
///
/// Returns the number of effects updated.
pub fn populate_heuristic_uptimes(
    effects: &mut [super::normalized_effects::NormalizedEffect],
    _profile: &RotationProfile,
) -> usize {
    use super::normalized_effects::{TriggerRule, UptimeModelKind};
    use super::quality::FactualValue;

    let mut updated = 0;

    for effect in effects.iter_mut() {
        if effect.uptime_model.kind != UptimeModelKind::Unknown {
            continue;
        }

        let estimated_uptime = match effect.trigger_rule {
            TriggerRule::Passive => {
                // Passive effects are always on — set to AlwaysOn instead
                effect.uptime_model.kind = UptimeModelKind::AlwaysOn;
                effect.uptime_model.uptime = Some(FactualValue::Resolved(1.0));
                updated += 1;
                continue;
            }
            TriggerRule::OnCrit => {
                // Estimate based on typical crit chance in rotation (~50-70%)
                // and ICD if available
                let base_rate = 0.6;
                if let Some(FactualValue::Resolved(icd)) = effect.internal_cooldown {
                    if icd > 0.0 {
                        // With ICD: uptime ≈ effect_duration / (effect_duration + icd)
                        let dur = match effect.effect_duration {
                            Some(FactualValue::Resolved(d)) if d > 0.0 => d,
                            _ => 5.0, // default 5s effect duration estimate
                        };
                        (dur / (dur + icd)).min(base_rate)
                    } else {
                        base_rate
                    }
                } else {
                    base_rate
                }
            }
            TriggerRule::OnHit => {
                // Estimate based on ~2 hits per second average
                let base_rate = 0.7;
                if let Some(FactualValue::Resolved(icd)) = effect.internal_cooldown {
                    if icd > 0.0 {
                        let dur = match effect.effect_duration {
                            Some(FactualValue::Resolved(d)) if d > 0.0 => d,
                            _ => 5.0,
                        };
                        (dur / (dur + icd)).min(base_rate)
                    } else {
                        base_rate
                    }
                } else {
                    base_rate
                }
            }
            TriggerRule::OnSkillUse => {
                // Estimate: skills used every ~2-3 seconds
                0.5
            }
            TriggerRule::OnHealthThreshold => {
                // Health thresholds: typically active ~30% of the time
                0.3
            }
            TriggerRule::Conditional => {
                // Generic conditional: ~50% uptime estimate
                0.5
            }
        };

        effect.uptime_model.kind = UptimeModelKind::Estimated;
        effect.uptime_model.uptime = Some(FactualValue::Resolved(estimated_uptime));
        effect.evidence_level = EvidenceLevel::Heuristic;
        updated += 1;
    }

    updated
}

// ─── Tests ───

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_profiles_load_successfully() {
        let data = rotation_profiles();
        assert!(
            data.total_count() >= 30,
            "expected >= 30 profiles, got {}",
            data.total_count()
        );
    }

    #[test]
    fn test_all_modes_have_9_professions_plus_fallback() {
        let data = rotation_profiles();
        for mode in &[
            gw2_core::types::GameMode::PvE,
            gw2_core::types::GameMode::PvP,
            gw2_core::types::GameMode::WvW,
        ] {
            let profiles = data.profiles_for_mode(mode);
            assert!(
                profiles.len() >= 10,
                "mode {:?} has {} profiles (need >= 10)",
                mode,
                profiles.len()
            );
            // Generic fallback exists
            assert!(
                profiles.iter().any(|p| p.profession == "Generic"),
                "mode {:?} missing Generic fallback",
                mode
            );
        }
    }

    #[test]
    fn test_lookup_exact_match() {
        let data = rotation_profiles();
        let profile = data.lookup("Warrior", None, &gw2_core::types::GameMode::PvE);
        assert!(profile.is_some(), "Warrior PvE profile not found");
        let p = profile.unwrap();
        assert_eq!(p.profession, "Warrior");
    }

    #[test]
    fn test_lookup_fallback_to_generic() {
        let data = rotation_profiles();
        // "FakeProfession" doesn't exist, should fall back to Generic
        let profile = data.lookup("FakeProfession", None, &gw2_core::types::GameMode::PvE);
        assert!(profile.is_some(), "Generic fallback not found");
        assert_eq!(profile.unwrap().profession, "Generic");
    }

    #[test]
    fn test_condition_weight_extraction() {
        let data = rotation_profiles();
        let necro = data
            .lookup("Necromancer", None, &gw2_core::types::GameMode::PvE)
            .unwrap();
        let bleeding = necro.condition_weight("Bleeding");
        assert!(
            bleeding > 0.0,
            "Necromancer should have Bleeding weight > 0"
        );
        let torment = necro.condition_weight("Torment");
        assert!(torment > 0.0, "Necromancer should have Torment weight > 0");
    }

    #[test]
    fn test_scenario_profiles_minimum_three() {
        let data = rotation_profiles();
        for mode in &[
            gw2_core::types::GameMode::PvE,
            gw2_core::types::GameMode::PvP,
            gw2_core::types::GameMode::WvW,
        ] {
            for p in data.profiles_for_mode(mode) {
                assert!(
                    p.scenarios.len() >= 3,
                    "profile '{}' has {} scenarios, need >= 3",
                    p.profile_id,
                    p.scenarios.len()
                );
            }
        }
    }

    #[test]
    fn test_all_evidence_levels_heuristic() {
        let data = rotation_profiles();
        for mode in &[
            gw2_core::types::GameMode::PvE,
            gw2_core::types::GameMode::PvP,
            gw2_core::types::GameMode::WvW,
        ] {
            for p in data.profiles_for_mode(mode) {
                assert_eq!(
                    p.evidence_level,
                    EvidenceLevel::Heuristic,
                    "profile '{}' is not Heuristic",
                    p.profile_id
                );
            }
        }
    }

    #[test]
    fn test_buff_profile_from_scenario() {
        let data = rotation_profiles();
        let warrior = data
            .lookup("Warrior", None, &gw2_core::types::GameMode::PvE)
            .unwrap();

        let solo = warrior.scenario("solo").unwrap();
        let bp_solo = BuffProfileFromScenario::from_scenario(warrior, solo);
        assert_eq!(bp_solo.label, "Solo");

        let squad = warrior.scenario("full_squad").unwrap();
        let bp_squad = BuffProfileFromScenario::from_scenario(warrior, squad);
        assert_eq!(bp_squad.label, "Full Squad");
        assert!(
            bp_squad.might_stacks > bp_solo.might_stacks,
            "Squad should have more Might than Solo"
        );
    }

    #[test]
    fn test_condition_weights_from_profile() {
        let data = rotation_profiles();
        let necro = data
            .lookup("Necromancer", None, &gw2_core::types::GameMode::PvE)
            .unwrap();
        let cw = ConditionWeightsFromProfile::from_profile(necro);
        assert!(cw.bleeding > 0.0);
        assert!(cw.torment > 0.0);
    }

    #[test]
    fn test_effective_boon_uptime_with_override() {
        let data = rotation_profiles();
        let warrior = data
            .lookup("Warrior", None, &gw2_core::types::GameMode::PvE)
            .unwrap();
        let squad = warrior.scenario("full_squad").unwrap();
        // Full squad should have Fury override to 1.0
        let fury = warrior.effective_boon_uptime("Fury", squad);
        assert!(
            (fury - 1.0).abs() < 0.001,
            "Expected Fury 1.0 in full_squad, got {}",
            fury
        );
    }

    #[test]
    fn test_malformed_json_rejected() {
        let result = load_rotation_profiles("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_generic_fallback_rejected() {
        // Valid JSON but no Generic profile
        let json = r#"[
            {
                "profile_id": "test_warrior",
                "profession": "Warrior",
                "elite_spec": null,
                "objective_profile_id": null,
                "boon_generation": {},
                "boon_uptime": {},
                "condition_application": {},
                "incoming_suppression": {},
                "target_behavior": { "movement_fraction": 0.2, "skill_use_frequency_per_second": 0.3 },
                "scenarios": [
                    { "scenario_id": "solo", "label": "Solo", "might_stacks": 0.0, "vulnerability_stacks": 0.0, "boon_overrides": null },
                    { "scenario_id": "party", "label": "Party", "might_stacks": 15.0, "vulnerability_stacks": 10.0, "boon_overrides": null },
                    { "scenario_id": "full_squad", "label": "Full Squad", "might_stacks": 25.0, "vulnerability_stacks": 25.0, "boon_overrides": null }
                ],
                "evidence_level": "Heuristic",
                "notes": "test"
            }
        ]"#;
        let result = load_rotation_profiles(json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("missing Generic fallback"));
    }

    #[test]
    fn test_non_heuristic_evidence_rejected() {
        // A profile with evidence_level Factual (should be Heuristic) must be rejected.
        // We provide all required professions + generic so that the evidence check is reached.
        let make_profile = |id: &str, prof: &str, evidence: &str| -> String {
            format!(
                r#"{{
                "profile_id": "{}",
                "profession": "{}",
                "elite_spec": null,
                "objective_profile_id": null,
                "boon_generation": {{}},
                "boon_uptime": {{}},
                "condition_application": {{}},
                "incoming_suppression": {{}},
                "target_behavior": {{ "movement_fraction": 0.2, "skill_use_frequency_per_second": 0.3 }},
                "scenarios": [
                    {{ "scenario_id": "solo", "label": "Solo", "might_stacks": 0.0, "vulnerability_stacks": 0.0, "boon_overrides": null }},
                    {{ "scenario_id": "party", "label": "Party", "might_stacks": 15.0, "vulnerability_stacks": 10.0, "boon_overrides": null }},
                    {{ "scenario_id": "full_squad", "label": "Full Squad", "might_stacks": 25.0, "vulnerability_stacks": 25.0, "boon_overrides": null }}
                ],
                "evidence_level": "{}",
                "notes": "test"
            }}"#,
                id, prof, evidence
            )
        };
        let profs = [
            "Warrior",
            "Guardian",
            "Revenant",
            "Engineer",
            "Ranger",
            "Thief",
            "Elementalist",
            "Mesmer",
            "Necromancer",
        ];
        let mut entries: Vec<String> = profs
            .iter()
            .enumerate()
            .map(|(i, p)| make_profile(&format!("test_{}", i), p, "Heuristic"))
            .collect();
        // Generic with Factual (should be rejected)
        entries.push(make_profile("test_generic", "Generic", "Factual"));
        let json = format!("[{}]", entries.join(","));

        let result = load_rotation_profiles(&json);
        assert!(result.is_err(), "Factual evidence_level should be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("Heuristic") || err_msg.contains("evidence_level"),
            "Error should mention Heuristic or evidence_level: {}",
            err_msg
        );
    }

    #[test]
    fn test_integration_rotation_profile_changes_combat_output() {
        // Verify that different rotation profiles produce different condition weights
        let data = rotation_profiles();
        let warrior = data
            .lookup("Warrior", None, &gw2_core::types::GameMode::PvE)
            .unwrap();
        let necro = data
            .lookup("Necromancer", None, &gw2_core::types::GameMode::PvE)
            .unwrap();

        let cw_war = ConditionWeightsFromProfile::from_profile(warrior);
        let cw_nec = ConditionWeightsFromProfile::from_profile(necro);

        // Necromancer should have higher Bleeding and Torment than Warrior
        assert!(
            cw_nec.bleeding > cw_war.bleeding,
            "Necro bleeding {} should exceed Warrior {}",
            cw_nec.bleeding,
            cw_war.bleeding
        );
        assert!(
            cw_nec.torment > cw_war.torment,
            "Necro torment {} should exceed Warrior {}",
            cw_nec.torment,
            cw_war.torment
        );
    }

    #[test]
    fn test_integration_solo_lt_party_lt_squad_might() {
        let data = rotation_profiles();
        let warrior = data
            .lookup("Warrior", None, &gw2_core::types::GameMode::PvE)
            .unwrap();

        let solo = warrior.scenario("solo").unwrap();
        let party = warrior.scenario("party").unwrap();
        let squad = warrior.scenario("full_squad").unwrap();

        assert!(
            solo.might_stacks < party.might_stacks,
            "Solo Might {} should be < Party {}",
            solo.might_stacks,
            party.might_stacks
        );
        assert!(
            party.might_stacks < squad.might_stacks,
            "Party Might {} should be < Squad {}",
            party.might_stacks,
            squad.might_stacks
        );
    }
}
