//! Localized id→name overlay from `/v2` `?lang=`.
//!
//! Official API locales are `de`, `es`, `fr`, `zh`. English GameDb stays the
//! optimizer source of truth — this cache is display-only.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::cache::DataCache;
use crate::client::{ApiError, Gw2Client};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocalizedNames {
    pub lang: String,
    #[serde(default)]
    pub skills: HashMap<u32, String>,
    #[serde(default)]
    pub traits: HashMap<u32, String>,
    #[serde(default)]
    pub specs: HashMap<u32, String>,
    #[serde(default)]
    pub items: HashMap<u32, String>,
    #[serde(default)]
    pub itemstats: HashMap<u32, String>,
    #[serde(default)]
    pub professions: HashMap<String, String>,
    #[serde(default)]
    pub legends: HashMap<String, String>,
    #[serde(default)]
    pub pvp_amulets: HashMap<u32, String>,
    /// Lowercase English name → localized name. Built when attached to GameDb.
    #[serde(skip)]
    pub by_english: HashMap<String, String>,
}

#[derive(Deserialize)]
struct NamedU32 {
    id: u32,
    #[serde(default)]
    name: String,
}

#[derive(Deserialize)]
struct NamedStr {
    id: String,
    #[serde(default)]
    name: String,
}

pub const API_LANGS: &[&str] = &["de", "es", "fr", "zh"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackStatus {
    /// Chrome-only language; GameDb names stay English.
    None,
    Ready,
    Missing,
    Stale,
}

/// Cache state for an official `/v2?lang=` name pack.
pub fn pack_status(cache: &DataCache, lang: &str, current_build: Option<u32>) -> PackStatus {
    if !API_LANGS.contains(&lang) {
        return PackStatus::None;
    }
    match cache.cached_build(&cache_key(lang)) {
        None => PackStatus::Missing,
        Some(b) => {
            if current_build.is_some_and(|c| c != b) {
                PackStatus::Stale
            } else {
                PackStatus::Ready
            }
        }
    }
}

pub fn cache_key(lang: &str) -> String {
    format!("loc_{lang}")
}

/// Load a previously downloaded overlay. `current_build` rejects a stale patch.
pub fn load(
    cache: &DataCache,
    lang: &str,
    current_build: Option<u32>,
) -> Result<Option<LocalizedNames>, String> {
    let key = cache_key(lang);
    if let Some(build) = current_build {
        if cache.is_stale(&key, build) {
            return Ok(None);
        }
    }
    cache.load(&key).map_err(|e| e.to_string())
}

/// Fetch id+name maps for the English cache's IDs, persist, return.
pub fn download(
    cache: &DataCache,
    lang: &str,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(&str),
) -> Result<LocalizedNames, ApiError> {
    if !matches!(lang, "de" | "es" | "fr" | "zh") {
        return Err(ApiError::Internal(format!("unsupported API lang {lang}")));
    }
    let client = Gw2Client::without_key()?.with_lang(Some(lang));
    let build = client.get_build_number()?;

    let check = || {
        if cancelled() {
            Err(ApiError::Internal("cancelled".into()))
        } else {
            Ok(())
        }
    };

    on_progress("itemstats");
    check()?;
    let itemstats = fetch_u32(&client, cache, "itemstats", "itemstats")?;
    on_progress("specializations");
    check()?;
    let specs = fetch_u32(&client, cache, "specializations", "specializations")?;
    on_progress("traits");
    check()?;
    let traits = fetch_u32(&client, cache, "traits", "traits")?;
    on_progress("skills");
    check()?;
    let skills = fetch_u32(&client, cache, "skills", "skills")?;
    on_progress("professions");
    check()?;
    let professions = fetch_str(&client, cache, "professions", "professions")?;
    on_progress("legends");
    check()?;
    let legends = fetch_str(&client, cache, "legends", "legends")?;
    on_progress("pvp amulets");
    check()?;
    let pvp_amulets = fetch_u32(&client, cache, "pvp/amulets", "pvp_amulets")?;
    on_progress("items");
    check()?;
    let items = fetch_u32_progress(&client, cache, "items", "items", &mut on_progress)?;

    let names = LocalizedNames {
        lang: lang.to_string(),
        skills,
        traits,
        specs,
        items,
        itemstats,
        professions,
        legends,
        pvp_amulets,
        by_english: HashMap::new(),
    };
    cache
        .save(&cache_key(lang), &names, build)
        .map_err(|e| ApiError::Cache(e.to_string()))?;
    Ok(names)
}

fn ids_from_cache_u32(cache: &DataCache, key: &str) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows: Vec<serde_json::Value> = cache
        .load(key)
        .map_err(|e| ApiError::Cache(e.to_string()))?
        .unwrap_or_default();
    Ok(rows
        .into_iter()
        .filter_map(|v| v.get("id").cloned())
        .collect())
}

fn ids_from_cache_str(cache: &DataCache, key: &str) -> Result<Vec<serde_json::Value>, ApiError> {
    ids_from_cache_u32(cache, key)
}

fn fetch_u32(
    client: &Gw2Client,
    cache: &DataCache,
    endpoint: &str,
    cache_key: &str,
) -> Result<HashMap<u32, String>, ApiError> {
    let ids = ids_from_cache_u32(cache, cache_key)?;
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<NamedU32> = client.fetch_by_ids(endpoint, &ids)?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| (r.id, r.name))
        .collect())
}

fn fetch_u32_progress(
    client: &Gw2Client,
    cache: &DataCache,
    endpoint: &str,
    cache_key: &str,
    on_progress: &mut impl FnMut(&str),
) -> Result<HashMap<u32, String>, ApiError> {
    let ids = ids_from_cache_u32(cache, cache_key)?;
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<NamedU32> =
        client.fetch_by_ids_with_progress(endpoint, &ids, |done, total| {
            on_progress(&format!("items {done}/{total}"));
        })?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| (r.id, r.name))
        .collect())
}

fn fetch_str(
    client: &Gw2Client,
    cache: &DataCache,
    endpoint: &str,
    cache_key: &str,
) -> Result<HashMap<String, String>, ApiError> {
    let ids = ids_from_cache_str(cache, cache_key)?;
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows: Vec<NamedStr> = client.fetch_by_ids(endpoint, &ids)?;
    Ok(rows
        .into_iter()
        .filter(|r| !r.name.is_empty())
        .map(|r| (r.id, r.name))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_u32_ignores_extra_item_fields() {
        let v =
            serde_json::json!({"id": 7, "name": "Sceau de malice", "type": "Skill", "facts": []});
        let n: NamedU32 = serde_json::from_value(v).unwrap();
        assert_eq!(n.id, 7);
        assert_eq!(n.name, "Sceau de malice");
    }

    #[test]
    fn named_str_defaults_missing_name() {
        let v = serde_json::json!({"id": "Legend1", "swap": 62891});
        let n: NamedStr = serde_json::from_value(v).unwrap();
        assert_eq!(n.id, "Legend1");
        assert!(n.name.is_empty());
    }

    #[test]
    fn ids_from_cache_reads_legend_without_name() {
        let cache = temp_cache();
        cache
            .save(
                "legends",
                &vec![serde_json::json!({"id": "Legend1", "swap": 62891})],
                1,
            )
            .unwrap();
        let ids = ids_from_cache_str(&cache, "legends").unwrap();
        assert_eq!(ids, vec![serde_json::json!("Legend1")]);
        let _ = cache.clear_all();
    }

    #[test]
    fn cache_key_is_lang_namespaced() {
        assert_eq!(cache_key("fr"), "loc_fr");
    }

    fn temp_cache() -> DataCache {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("gw2_loc_pack_{}_{}", std::process::id(), id));
        let _ = std::fs::remove_dir_all(&dir);
        DataCache::new(dir)
    }

    #[test]
    fn pack_status_en_is_none() {
        let cache = temp_cache();
        assert_eq!(pack_status(&cache, "en", Some(1)), PackStatus::None);
        let _ = cache.clear_all();
    }

    #[test]
    fn pack_status_fr_empty_is_missing() {
        let cache = temp_cache();
        assert_eq!(pack_status(&cache, "fr", Some(1)), PackStatus::Missing);
        let _ = cache.clear_all();
    }

    #[test]
    fn pack_status_fr_ready_and_stale() {
        let cache = temp_cache();
        cache
            .save(
                &cache_key("fr"),
                &LocalizedNames {
                    lang: "fr".into(),
                    ..Default::default()
                },
                100,
            )
            .unwrap();
        assert_eq!(pack_status(&cache, "fr", Some(100)), PackStatus::Ready);
        assert_eq!(pack_status(&cache, "fr", Some(101)), PackStatus::Stale);
        let _ = cache.clear_all();
    }
}
