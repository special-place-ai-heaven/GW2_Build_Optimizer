use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;
use thiserror::Error;

use super::{DataLoadError, EvidenceLevel};

// ─── Embedded baseline JSON (compile-time) ───

const PVE_OVERRIDES_JSON: &str =
    include_str!("../../../../data/balance_overrides/2026-01-13/pve.json");
const PVP_OVERRIDES_JSON: &str =
    include_str!("../../../../data/balance_overrides/2026-01-13/pvp.json");
const WVW_OVERRIDES_JSON: &str =
    include_str!("../../../../data/balance_overrides/2026-01-13/wvw.json");

static OVERRIDES: OnceLock<BalanceOverrides> = OnceLock::new();

/// Returns the globally loaded balance overrides, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn overrides() -> &'static BalanceOverrides {
    OVERRIDES.get_or_init(|| {
        load_all_overrides().expect("embedded balance_overrides JSON is invalid")
    })
}

/// Try to load all balance overrides from the embedded JSON, returning typed errors
/// on failure. Does NOT store in OnceLock — used for health-check validation.
pub fn try_load_balance_overrides() -> Result<BalanceOverrides, Vec<DataLoadError>> {
    load_all_overrides().map_err(|e| {
        vec![match e {
            BalanceOverrideError::ParseError(pe) => DataLoadError::ParseError {
                source: "balance_overrides".into(),
                detail: pe.to_string(),
            },
            BalanceOverrideError::ValidationError(msg) => DataLoadError::ValidationError {
                source: "balance_overrides".into(),
                field: String::new(),
                reason: msg,
            },
        }]
    })
}

// ─── Error type ───

#[derive(Debug, Error)]
pub enum BalanceOverrideError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

// ─── Override file schema ───

/// A single override file for one game mode in a specific patch.
#[derive(Debug, Clone, Deserialize)]
pub struct OverrideFile {
    pub patch_id: String,
    pub mode: String,
    pub entities: Vec<OverrideEntity>,
}

/// An entity (skill, trait, profession mechanic, etc.) with field overrides.
#[derive(Debug, Clone, Deserialize)]
pub struct OverrideEntity {
    /// Type of the source entity (e.g., "Skill", "Trait", "Profession").
    pub source_type: String,
    /// GW2 API ID of the entity.
    pub source_id: u32,
    /// Human-readable name for logging/debugging.
    pub name: String,
    /// Per-field overrides. Keys are field names (e.g., "coefficient", "base_damage").
    pub overrides: HashMap<String, OverrideEntry>,
}

/// A single field override with its value, evidence level, and optional source.
#[derive(Debug, Clone, Deserialize)]
pub struct OverrideEntry {
    /// The override value. `None` means explicitly Unknown (value cannot be determined).
    /// `Some(f64)` means a known override value.
    pub value: Option<f64>,
    /// Evidence level for this override.
    pub evidence_level: EvidenceLevel,
    /// Optional source citation (wiki URL, patch notes, etc.).
    #[serde(default)]
    pub source: Option<String>,
}

// ─── Lookup result ───

/// Result of looking up an override. Distinguishes between:
/// - No override exists (None from lookup) → no quality degradation
/// - Override exists with a known value → OverrideResult::Value
/// - Override exists but value is Unknown → OverrideResult::Unknown
#[derive(Debug, Clone, PartialEq)]
pub enum OverrideResult {
    /// A known override value with its evidence level.
    Value {
        value: f64,
        evidence_level: EvidenceLevel,
    },
    /// The value is explicitly unknown — degrades DataQuality.
    Unknown {
        evidence_level: EvidenceLevel,
    },
}

// ─── BalanceOverrides container ───

/// Container for all loaded balance overrides, keyed by (patch_id, mode).
#[derive(Debug)]
pub struct BalanceOverrides {
    /// Map from (patch_id, mode) to parsed override file.
    files: HashMap<(String, String), OverrideFile>,
}

impl BalanceOverrides {
    /// Look up a specific override.
    ///
    /// Returns:
    /// - `None` if no override exists for this entity/field combination.
    ///   This means the default value should be used with no quality degradation.
    /// - `Some(OverrideResult::Value { .. })` if an override with a known value exists.
    /// - `Some(OverrideResult::Unknown { .. })` if the value is explicitly unknown.
    pub fn lookup(
        &self,
        patch_id: &str,
        mode: &str,
        source_type: &str,
        source_id: u32,
        field: &str,
    ) -> Option<OverrideResult> {
        let file = self.files.get(&(patch_id.to_string(), mode.to_string()))?;
        let entity = file
            .entities
            .iter()
            .find(|e| e.source_type == source_type && e.source_id == source_id)?;
        let entry = entity.overrides.get(field)?;

        Some(match entry.value {
            Some(v) => OverrideResult::Value {
                value: v,
                evidence_level: entry.evidence_level.clone(),
            },
            None => OverrideResult::Unknown {
                evidence_level: entry.evidence_level.clone(),
            },
        })
    }

    /// Number of loaded override files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Total number of entities across all override files.
    pub fn entity_count(&self) -> usize {
        self.files.values().map(|f| f.entities.len()).sum()
    }
}

// ─── Loading ───

/// Parse and validate a single override file from JSON text.
pub fn load_override_file(json: &str) -> Result<OverrideFile, BalanceOverrideError> {
    let file: OverrideFile = serde_json::from_str(json)?;
    validate_override_file(&file)?;
    Ok(file)
}

fn validate_override_file(file: &OverrideFile) -> Result<(), BalanceOverrideError> {
    if file.patch_id.is_empty() {
        return Err(BalanceOverrideError::ValidationError(
            "patch_id must not be empty".into(),
        ));
    }

    let valid_modes = ["PvE", "PvP", "WvW"];
    if !valid_modes.contains(&file.mode.as_str()) {
        return Err(BalanceOverrideError::ValidationError(format!(
            "invalid mode '{}', expected one of: PvE, PvP, WvW",
            file.mode
        )));
    }

    // Validate no duplicate entity (source_type, source_id) pairs
    let mut seen = std::collections::HashSet::new();
    for entity in &file.entities {
        let key = (entity.source_type.clone(), entity.source_id);
        if !seen.insert(key) {
            return Err(BalanceOverrideError::ValidationError(format!(
                "duplicate entity: {} (id {})",
                entity.source_type, entity.source_id
            )));
        }
        if entity.name.is_empty() {
            return Err(BalanceOverrideError::ValidationError(format!(
                "entity {} (id {}) has empty name",
                entity.source_type, entity.source_id
            )));
        }
    }

    Ok(())
}

/// Load and merge all three baseline override files.
fn load_all_overrides() -> Result<BalanceOverrides, BalanceOverrideError> {
    let mut files = HashMap::new();

    for (json, expected_mode) in [
        (PVE_OVERRIDES_JSON, "PvE"),
        (PVP_OVERRIDES_JSON, "PvP"),
        (WVW_OVERRIDES_JSON, "WvW"),
    ] {
        let file = load_override_file(json)?;

        // Validate mode matches expected
        if file.mode != expected_mode {
            return Err(BalanceOverrideError::ValidationError(format!(
                "expected mode '{}', got '{}'",
                expected_mode, file.mode
            )));
        }

        files.insert((file.patch_id.clone(), file.mode.clone()), file);
    }

    Ok(BalanceOverrides { files })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Embedded baseline loads successfully ───

    #[test]
    fn test_embedded_overrides_load_successfully() {
        let o = overrides();
        assert_eq!(o.file_count(), 3, "expected 3 override files (PvE, PvP, WvW)");
        assert_eq!(o.entity_count(), 0, "baseline has no entities");
    }

    #[test]
    fn test_try_load_returns_ok() {
        let result = try_load_balance_overrides();
        assert!(result.is_ok(), "try_load should succeed: {:?}", result.err());
    }

    // ─── Lookup on empty baseline returns None ───

    #[test]
    fn test_lookup_empty_baseline_returns_none() {
        let o = overrides();
        assert_eq!(
            o.lookup("2026-01-13", "PvE", "Skill", 1234, "coefficient"),
            None,
            "empty baseline should return None for any lookup"
        );
    }

    #[test]
    fn test_lookup_unknown_patch_returns_none() {
        let o = overrides();
        assert_eq!(
            o.lookup("9999-99-99", "PvE", "Skill", 1234, "coefficient"),
            None,
        );
    }

    #[test]
    fn test_lookup_unknown_mode_returns_none() {
        let o = overrides();
        assert_eq!(
            o.lookup("2026-01-13", "UnknownMode", "Skill", 1234, "coefficient"),
            None,
        );
    }

    // ─── Parsing tests with inline JSON ───

    #[test]
    fn test_parse_valid_override_file() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvP",
            "entities": [
                {
                    "source_type": "Skill",
                    "source_id": 5678,
                    "name": "Fireball",
                    "overrides": {
                        "coefficient": {
                            "value": 0.75,
                            "evidence_level": "Factual",
                            "source": "https://wiki.guildwars2.com/wiki/Fireball"
                        }
                    }
                }
            ]
        }"#;
        let file = load_override_file(json).expect("should parse");
        assert_eq!(file.patch_id, "2026-01-13");
        assert_eq!(file.mode, "PvP");
        assert_eq!(file.entities.len(), 1);
        assert_eq!(file.entities[0].name, "Fireball");
        assert_eq!(file.entities[0].source_id, 5678);
        let coeff = file.entities[0].overrides.get("coefficient").unwrap();
        assert_eq!(coeff.value, Some(0.75));
        assert_eq!(coeff.evidence_level, EvidenceLevel::Factual);
    }

    #[test]
    fn test_parse_unknown_value_override() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "WvW",
            "entities": [
                {
                    "source_type": "Trait",
                    "source_id": 999,
                    "name": "Unknown Trait",
                    "overrides": {
                        "coefficient": {
                            "value": null,
                            "evidence_level": "Unknown"
                        }
                    }
                }
            ]
        }"#;
        let file = load_override_file(json).expect("should parse");
        let coeff = file.entities[0].overrides.get("coefficient").unwrap();
        assert_eq!(coeff.value, None);
        assert_eq!(coeff.evidence_level, EvidenceLevel::Unknown);
    }

    // ─── Lookup with populated overrides ───

    #[test]
    fn test_lookup_value_override() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvP",
            "entities": [
                {
                    "source_type": "Skill",
                    "source_id": 5678,
                    "name": "Fireball",
                    "overrides": {
                        "coefficient": {
                            "value": 0.75,
                            "evidence_level": "Factual"
                        }
                    }
                }
            ]
        }"#;
        let file = load_override_file(json).unwrap();
        let mut files = HashMap::new();
        files.insert(
            (file.patch_id.clone(), file.mode.clone()),
            file,
        );
        let overrides = BalanceOverrides { files };

        // Existing override returns Value
        let result = overrides.lookup("2026-01-13", "PvP", "Skill", 5678, "coefficient");
        assert_eq!(
            result,
            Some(OverrideResult::Value {
                value: 0.75,
                evidence_level: EvidenceLevel::Factual,
            }),
        );

        // Different field returns None (no override, no degradation)
        assert_eq!(
            overrides.lookup("2026-01-13", "PvP", "Skill", 5678, "base_damage"),
            None,
        );

        // Different entity returns None
        assert_eq!(
            overrides.lookup("2026-01-13", "PvP", "Skill", 9999, "coefficient"),
            None,
        );
    }

    #[test]
    fn test_lookup_unknown_override() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "WvW",
            "entities": [
                {
                    "source_type": "Trait",
                    "source_id": 999,
                    "name": "Mystery Trait",
                    "overrides": {
                        "coefficient": {
                            "value": null,
                            "evidence_level": "Unknown"
                        }
                    }
                }
            ]
        }"#;
        let file = load_override_file(json).unwrap();
        let mut files = HashMap::new();
        files.insert(
            (file.patch_id.clone(), file.mode.clone()),
            file,
        );
        let overrides = BalanceOverrides { files };

        // Unknown override returns OverrideResult::Unknown (degrades quality)
        let result = overrides.lookup("2026-01-13", "WvW", "Trait", 999, "coefficient");
        assert_eq!(
            result,
            Some(OverrideResult::Unknown {
                evidence_level: EvidenceLevel::Unknown,
            }),
        );
    }

    // ─── Error paths ───

    #[test]
    fn test_malformed_json_returns_error() {
        let result = load_override_file("not valid json");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("JSON parse error"),
            "expected parse error, got: {}",
            err,
        );
    }

    #[test]
    fn test_empty_patch_id_rejected() {
        let json = r#"{
            "patch_id": "",
            "mode": "PvE",
            "entities": []
        }"#;
        let result = load_override_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("patch_id must not be empty"));
    }

    #[test]
    fn test_invalid_mode_rejected() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "Ranked",
            "entities": []
        }"#;
        let result = load_override_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid mode"));
    }

    #[test]
    fn test_duplicate_entity_rejected() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvE",
            "entities": [
                {
                    "source_type": "Skill",
                    "source_id": 100,
                    "name": "Skill A",
                    "overrides": {}
                },
                {
                    "source_type": "Skill",
                    "source_id": 100,
                    "name": "Skill A Duplicate",
                    "overrides": {}
                }
            ]
        }"#;
        let result = load_override_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("duplicate entity"));
    }

    #[test]
    fn test_empty_entity_name_rejected() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvE",
            "entities": [
                {
                    "source_type": "Skill",
                    "source_id": 100,
                    "name": "",
                    "overrides": {}
                }
            ]
        }"#;
        let result = load_override_file(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty name"));
    }

    // ─── None vs Unknown semantics ───

    #[test]
    fn test_none_vs_unknown_semantics() {
        // None from lookup = no override exists, use default, no quality degradation
        // Some(Unknown) = override exists but value can't be determined, degrades quality
        let json = r#"{
            "patch_id": "2026-01-13",
            "mode": "PvP",
            "entities": [
                {
                    "source_type": "Skill",
                    "source_id": 42,
                    "name": "Test Skill",
                    "overrides": {
                        "known_field": {
                            "value": 1.5,
                            "evidence_level": "Factual"
                        },
                        "unknown_field": {
                            "value": null,
                            "evidence_level": "Unknown"
                        }
                    }
                }
            ]
        }"#;
        let file = load_override_file(json).unwrap();
        let mut files = HashMap::new();
        files.insert(
            (file.patch_id.clone(), file.mode.clone()),
            file,
        );
        let o = BalanceOverrides { files };

        // Known override → Value
        assert!(matches!(
            o.lookup("2026-01-13", "PvP", "Skill", 42, "known_field"),
            Some(OverrideResult::Value { value: 1.5, .. }),
        ));

        // Explicitly unknown override → Unknown (degrades quality)
        assert!(matches!(
            o.lookup("2026-01-13", "PvP", "Skill", 42, "unknown_field"),
            Some(OverrideResult::Unknown { .. }),
        ));

        // No override at all → None (no degradation, use default)
        assert_eq!(
            o.lookup("2026-01-13", "PvP", "Skill", 42, "unset_field"),
            None,
        );
    }
}
