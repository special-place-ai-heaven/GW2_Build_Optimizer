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
    name: String,
}

#[derive(Deserialize)]
struct NamedStr {
    id: String,
    name: String,
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
    let rows: Vec<NamedU32> = cache
        .load(key)
        .map_err(|e| ApiError::Cache(e.to_string()))?
        .unwrap_or_default();
    Ok(rows.into_iter().map(|r| serde_json::json!(r.id)).collect())
}

fn ids_from_cache_str(cache: &DataCache, key: &str) -> Result<Vec<serde_json::Value>, ApiError> {
    let rows: Vec<NamedStr> = cache
        .load(key)
        .map_err(|e| ApiError::Cache(e.to_string()))?
        .unwrap_or_default();
    Ok(rows.into_iter().map(|r| serde_json::json!(r.id)).collect())
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
        let v = serde_json::json!({"id": 7, "name": "Sceau de malice", "type": "Skill", "facts": []});
        let n: NamedU32 = serde_json::from_value(v).unwrap();
        assert_eq!(n.id, 7);
        assert_eq!(n.name, "Sceau de malice");
    }

    #[test]
    fn cache_key_is_lang_namespaced() {
        assert_eq!(cache_key("fr"), "loc_fr");
    }
}
