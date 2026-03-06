use serde::Deserialize;
use std::sync::OnceLock;
use thiserror::Error;

use super::EvidenceLevel;

/// Canonical YAML embedded at compile time from data/patch_ledgers/2026-01-13.yaml.
const LEDGER_2026_01_13_YAML: &str =
    include_str!("../../../../data/patch_ledgers/2026-01-13.yaml");

static LEDGERS: OnceLock<Vec<PatchLedger>> = OnceLock::new();

/// Returns all globally loaded patch ledgers, parsing on first access.
///
/// # Panics
/// Panics if the embedded YAML is malformed (compile-time data, should never happen).
pub fn ledgers() -> &'static [PatchLedger] {
    LEDGERS.get_or_init(|| {
        vec![
            load_ledger(LEDGER_2026_01_13_YAML)
                .expect("embedded 2026-01-13 ledger is invalid"),
        ]
    })
}

/// Returns the ledger for a specific patch_id, or None.
pub fn ledger_for_patch(patch_id: &str) -> Option<&'static PatchLedger> {
    ledgers().iter().find(|l| l.patch_id == patch_id)
}

#[derive(Debug, Error)]
pub enum LedgerError {
    #[error("YAML parse error: {0}")]
    ParseError(#[from] serde_yaml::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchLedger {
    pub patch_id: String,
    pub inherits_from: Option<String>,
    pub changes: Vec<LedgerChange>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LedgerChange {
    pub source_type: String,
    pub source_id: u32,
    pub source_name: String,
    pub mode: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub evidence_level: EvidenceLevel,
    pub source: String,
}

/// Parse and validate a single patch ledger from YAML text.
pub fn load_ledger(yaml: &str) -> Result<PatchLedger, LedgerError> {
    let ledger: PatchLedger = serde_yaml::from_str(yaml)?;
    validate_ledger(&ledger)?;
    Ok(ledger)
}

fn validate_ledger(ledger: &PatchLedger) -> Result<(), LedgerError> {
    if ledger.patch_id.is_empty() {
        return Err(LedgerError::ValidationError(
            "patch_id is empty".to_string(),
        ));
    }
    for (i, change) in ledger.changes.iter().enumerate() {
        if change.source.is_empty() {
            return Err(LedgerError::ValidationError(format!(
                "change[{}] ('{}') has empty source URL",
                i, change.source_name
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_ledger_loads_successfully() {
        let ls = ledgers();
        assert_eq!(ls.len(), 1);
        assert_eq!(ls[0].patch_id, "2026-01-13");
        assert!(ls[0].changes.is_empty(), "baseline ledger should have no changes");
    }

    #[test]
    fn test_ledger_for_patch_found() {
        let l = ledger_for_patch("2026-01-13");
        assert!(l.is_some());
        assert_eq!(l.unwrap().patch_id, "2026-01-13");
    }

    #[test]
    fn test_ledger_for_patch_not_found() {
        let l = ledger_for_patch("1999-01-01");
        assert!(l.is_none());
    }

    #[test]
    fn test_baseline_ledger_has_no_inherits_from() {
        let l = ledger_for_patch("2026-01-13").unwrap();
        assert!(l.inherits_from.is_none());
    }

    #[test]
    fn test_validation_rejects_empty_patch_id() {
        let yaml = r#"
patch_id: ""
inherits_from: null
changes: []
"#;
        let err = load_ledger(yaml).unwrap_err();
        assert!(
            err.to_string().contains("patch_id is empty"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_empty_source_url_in_change() {
        let yaml = r#"
patch_id: "2026-02-01"
inherits_from: "2026-01-13"
changes:
  - source_type: skill
    source_id: 12345
    source_name: "Fireball"
    mode: PvE
    field: damage_coefficient
    old_value: "0.8"
    new_value: "0.9"
    evidence_level: Factual
    source: ""
"#;
        let err = load_ledger(yaml).unwrap_err();
        assert!(
            err.to_string().contains("empty source URL"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_unknown_evidence_level_preserved() {
        let yaml = r#"
patch_id: "2026-02-01"
inherits_from: null
changes:
  - source_type: skill
    source_id: 99999
    source_name: "Mystery Skill"
    mode: PvE
    field: damage_coefficient
    old_value: "???"
    new_value: "???"
    evidence_level: Unknown
    source: "https://wiki.guildwars2.com/wiki/Game_updates/2026-02-01"
"#;
        let ledger = load_ledger(yaml).unwrap();
        assert_eq!(ledger.changes.len(), 1);
        assert_eq!(ledger.changes[0].evidence_level, EvidenceLevel::Unknown);
    }

    #[test]
    fn test_ledger_with_valid_changes_loads() {
        let yaml = r#"
patch_id: "2026-02-01"
inherits_from: "2026-01-13"
changes:
  - source_type: skill
    source_id: 12345
    source_name: "Fireball"
    mode: PvE
    field: damage_coefficient
    old_value: "0.8"
    new_value: "0.9"
    evidence_level: Factual
    source: "https://wiki.guildwars2.com/wiki/Game_updates/2026-02-01"
  - source_type: trait
    source_id: 67890
    source_name: "Burning Precision"
    mode: WvW
    field: proc_chance
    old_value: "0.33"
    new_value: "0.25"
    evidence_level: Factual
    source: "https://wiki.guildwars2.com/wiki/Game_updates/2026-02-01"
"#;
        let ledger = load_ledger(yaml).unwrap();
        assert_eq!(ledger.patch_id, "2026-02-01");
        assert_eq!(ledger.inherits_from, Some("2026-01-13".to_string()));
        assert_eq!(ledger.changes.len(), 2);
        assert_eq!(ledger.changes[0].source_type, "skill");
        assert_eq!(ledger.changes[1].source_type, "trait");
    }

    #[test]
    fn test_malformed_yaml_rejected() {
        let yaml = "this is not valid yaml: [[[";
        let err = load_ledger(yaml);
        assert!(err.is_err());
        assert!(matches!(err.unwrap_err(), LedgerError::ParseError(_)));
    }
}
