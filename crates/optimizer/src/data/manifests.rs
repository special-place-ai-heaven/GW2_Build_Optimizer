use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use thiserror::Error;

/// Canonical JSON embedded at compile time from data/manifests/2026-01-13.json.
const MANIFEST_2026_01_13_JSON: &str = include_str!("../../../../data/manifests/2026-01-13.json");

static MANIFESTS: OnceLock<Vec<PatchManifest>> = OnceLock::new();

/// Returns all globally loaded patch manifests, parsing on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data, should never happen).
pub fn manifests() -> &'static [PatchManifest] {
    MANIFESTS.get_or_init(|| {
        let all = vec![load_manifest(MANIFEST_2026_01_13_JSON)
            .expect("embedded 2026-01-13 manifest is invalid")];
        validate_manifest_set(&all).expect("embedded manifest set validation failed");
        all
    })
}

/// Returns the latest active manifest (highest patch_id with status "active").
///
/// # Panics
/// Panics if no active manifests exist.
pub fn latest_manifest() -> &'static PatchManifest {
    manifests()
        .iter()
        .filter(|m| m.status == "active")
        .max_by(|a, b| a.patch_id.cmp(&b.patch_id))
        .expect("no active manifests found")
}

/// Returns a staleness warning message if the live game build doesn't match
/// the latest manifest's game_build_id, or None if they match.
pub fn check_staleness(live_build_id: u64) -> Option<String> {
    let manifest = latest_manifest();
    if manifest.game_build_id != live_build_id {
        Some(format!(
            "Balance data verified for build {}, but game is running build {}",
            manifest.game_build_id, live_build_id
        ))
    } else {
        None
    }
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Deserialize)]
pub struct PatchManifest {
    pub patch_id: String,
    pub game_build_id: u64,
    pub release_date: String,
    pub inherits_from: Option<String>,
    pub sources: Vec<ManifestSource>,
    pub supported_modes: Vec<String>,
    pub status: String,
    #[serde(default)]
    pub authoring_notes: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ManifestSource {
    pub kind: String,
    pub url: String,
}

/// Known valid statuses for a patch manifest.
const VALID_STATUSES: &[&str] = &["active", "superseded", "draft"];

/// Parse and validate a single patch manifest from JSON text.
pub fn load_manifest(json: &str) -> Result<PatchManifest, ManifestError> {
    let manifest: PatchManifest = serde_json::from_str(json)?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(m: &PatchManifest) -> Result<(), ManifestError> {
    if m.patch_id.is_empty() {
        return Err(ManifestError::ValidationError(
            "patch_id is empty".to_string(),
        ));
    }
    if m.game_build_id == 0 {
        return Err(ManifestError::ValidationError(
            "game_build_id must be > 0".to_string(),
        ));
    }
    if m.sources.is_empty() {
        return Err(ManifestError::ValidationError(
            "at least one source is required".to_string(),
        ));
    }
    for (i, source) in m.sources.iter().enumerate() {
        if source.url.is_empty() {
            return Err(ManifestError::ValidationError(format!(
                "source[{}] has empty URL",
                i
            )));
        }
    }
    if m.supported_modes.is_empty() {
        return Err(ManifestError::ValidationError(
            "supported_modes is empty".to_string(),
        ));
    }
    if !VALID_STATUSES.contains(&m.status.as_str()) {
        return Err(ManifestError::ValidationError(format!(
            "unknown status '{}', expected one of: {:?}",
            m.status, VALID_STATUSES
        )));
    }
    Ok(())
}

/// Validate a set of manifests for cross-manifest invariants.
pub fn validate_manifest_set(manifests: &[PatchManifest]) -> Result<(), ManifestError> {
    // No duplicate patch_ids
    let mut seen_ids = HashSet::new();
    for m in manifests {
        if !seen_ids.insert(&m.patch_id) {
            return Err(ManifestError::ValidationError(format!(
                "duplicate patch_id: {}",
                m.patch_id
            )));
        }
    }

    // Build id→manifest lookup for inheritance checks
    let id_map: HashMap<&str, &PatchManifest> =
        manifests.iter().map(|m| (m.patch_id.as_str(), m)).collect();

    // Every inherits_from must reference an existing patch_id
    for m in manifests {
        if let Some(ref parent) = m.inherits_from {
            if !id_map.contains_key(parent.as_str()) {
                return Err(ManifestError::ValidationError(format!(
                    "manifest '{}' inherits_from '{}' which does not exist",
                    m.patch_id, parent
                )));
            }
        }
    }

    // No circular inherits_from references
    for m in manifests {
        let mut visited = HashSet::new();
        let mut current = Some(m.patch_id.as_str());
        while let Some(id) = current {
            if !visited.insert(id) {
                return Err(ManifestError::ValidationError(format!(
                    "circular inheritance detected involving '{}'",
                    id
                )));
            }
            current = id_map.get(id).and_then(|m| m.inherits_from.as_deref());
        }
    }

    // No two "active" manifests in the same inheritance lineage
    let active_ids: Vec<&str> = manifests
        .iter()
        .filter(|m| m.status == "active")
        .map(|m| m.patch_id.as_str())
        .collect();

    // For each active manifest, collect its full lineage
    for &active_id in &active_ids {
        let mut lineage = HashSet::new();
        let mut current = Some(active_id);
        while let Some(id) = current {
            lineage.insert(id);
            current = id_map.get(id).and_then(|m| m.inherits_from.as_deref());
        }

        // Check if any other active manifest is in this lineage
        for &other_id in &active_ids {
            if other_id != active_id && lineage.contains(other_id) {
                return Err(ManifestError::ValidationError(format!(
                    "two active manifests in the same lineage: '{}' and '{}'",
                    active_id, other_id
                )));
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_embedded_manifest_loads_successfully() {
        let ms = manifests();
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].patch_id, "2026-01-13");
        assert_eq!(ms[0].status, "active");
        assert!(ms[0].game_build_id > 0);
    }

    #[test]
    fn test_latest_manifest_returns_active() {
        let m = latest_manifest();
        assert_eq!(m.status, "active");
        assert_eq!(m.patch_id, "2026-01-13");
    }

    #[test]
    fn test_staleness_matching_build_returns_none() {
        let m = latest_manifest();
        assert_eq!(check_staleness(m.game_build_id), None);
    }

    #[test]
    fn test_staleness_mismatched_build_returns_some() {
        let result = check_staleness(999999);
        assert!(result.is_some());
        let msg = result.unwrap();
        assert!(
            msg.contains("999999"),
            "message should mention live build: {}",
            msg
        );
        assert!(
            msg.contains(&latest_manifest().game_build_id.to_string()),
            "message should mention manifest build: {}",
            msg
        );
    }

    #[test]
    fn test_manifest_has_authoring_notes() {
        let m = latest_manifest();
        assert!(
            m.authoring_notes.is_some(),
            "baseline manifest should have authoring_notes"
        );
        assert!(
            m.authoring_notes.as_ref().unwrap().contains("baseline"),
            "authoring_notes should mention baseline"
        );
    }

    #[test]
    fn test_validation_rejects_empty_patch_id() {
        let json = r#"{
            "patch_id": "",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("patch_id is empty"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_zero_game_build_id() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 0,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("game_build_id must be > 0"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_missing_sources() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("at least one source"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_empty_source_url() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": ""}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("empty URL"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_empty_supported_modes() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": [],
            "status": "active"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("supported_modes is empty"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_validation_rejects_unknown_status() {
        let json = r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "expired"
        }"#;
        let err = load_manifest(json).unwrap_err();
        assert!(
            err.to_string().contains("unknown status"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_set_validation_rejects_duplicate_patch_id() {
        let m1 = load_manifest(
            r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        let m2 = load_manifest(
            r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175219,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "draft"
        }"#,
        )
        .unwrap();
        let err = validate_manifest_set(&[m1, m2]).unwrap_err();
        assert!(
            err.to_string().contains("duplicate patch_id"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_set_validation_rejects_dangling_inherits_from() {
        let m = load_manifest(
            r#"{
            "patch_id": "2026-02-01",
            "game_build_id": 175300,
            "release_date": "2026-02-01",
            "inherits_from": "2026-01-13",
            "sources": [{"kind": "wiki_patch_notes", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        let err = validate_manifest_set(&[m]).unwrap_err();
        assert!(
            err.to_string().contains("does not exist"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_set_validation_rejects_circular_inheritance() {
        // Two manifests that point to each other
        let mut m1 = load_manifest(
            r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "superseded"
        }"#,
        )
        .unwrap();
        let m2 = load_manifest(
            r#"{
            "patch_id": "2026-02-01",
            "game_build_id": 175300,
            "release_date": "2026-02-01",
            "inherits_from": "2026-01-13",
            "sources": [{"kind": "wiki_patch_notes", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        // Create the cycle: m1 -> m2 -> m1
        m1.inherits_from = Some("2026-02-01".to_string());
        let err = validate_manifest_set(&[m1, m2]).unwrap_err();
        assert!(
            err.to_string().contains("circular inheritance"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_set_validation_rejects_two_active_in_same_lineage() {
        let m1 = load_manifest(
            r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        let m2 = load_manifest(
            r#"{
            "patch_id": "2026-02-01",
            "game_build_id": 175300,
            "release_date": "2026-02-01",
            "inherits_from": "2026-01-13",
            "sources": [{"kind": "wiki_patch_notes", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        let err = validate_manifest_set(&[m1, m2]).unwrap_err();
        assert!(
            err.to_string().contains("two active manifests"),
            "unexpected error: {}",
            err
        );
    }

    #[test]
    fn test_set_validation_allows_valid_inheritance_chain() {
        let m1 = load_manifest(
            r#"{
            "patch_id": "2026-01-13",
            "game_build_id": 175218,
            "release_date": "2026-01-13",
            "inherits_from": null,
            "sources": [{"kind": "baseline", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "superseded"
        }"#,
        )
        .unwrap();
        let m2 = load_manifest(
            r#"{
            "patch_id": "2026-02-01",
            "game_build_id": 175300,
            "release_date": "2026-02-01",
            "inherits_from": "2026-01-13",
            "sources": [{"kind": "wiki_patch_notes", "url": "https://example.com"}],
            "supported_modes": ["PvE"],
            "status": "active"
        }"#,
        )
        .unwrap();
        assert!(validate_manifest_set(&[m1, m2]).is_ok());
    }
}
