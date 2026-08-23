//! Full data download orchestration.
//! Downloads all game data endpoints and caches them locally.

use crate::cache::DataCache;
use crate::client::{ApiError, Gw2Client};
use crate::models;

/// Progress update sent during download.
#[derive(Debug, Clone)]
pub struct DownloadProgress {
    pub current_step: usize,
    pub total_steps: usize,
    pub step_name: String,
    pub done: bool,
    /// Optional sub-step detail (e.g. "batch 5/500")
    pub detail: Option<String>,
    /// Intra-step counts (items/icons). `0` total means no inner bar.
    pub inner_done: usize,
    pub inner_total: usize,
}

const TOTAL_STEPS: usize = 9;

fn report(
    on_progress: &mut impl FnMut(DownloadProgress),
    step: &mut usize,
    name: &str,
    detail: Option<String>,
) {
    *step += 1;
    on_progress(DownloadProgress {
        current_step: *step,
        total_steps: TOTAL_STEPS,
        step_name: name.to_string(),
        done: *step >= TOTAL_STEPS,
        detail,
        inner_done: 0,
        inner_total: 0,
    });
}

/// Download all game data, calling `on_progress` after each endpoint.
/// Skips endpoints that are already cached at the current build.
/// Returns the game build number on success.
pub fn download_all(
    client: &Gw2Client,
    cache: &DataCache,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<u32, ApiError> {
    let build = client.get_build_number()?;
    let mut step = 0;

    // 1. Item stats
    if cache.is_stale("itemstats", build) {
        let data: Vec<models::ItemStat> = client.fetch_all("itemstats")?;
        cache
            .save("itemstats", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Item stats", None);

    // 2. Specializations
    if cache.is_stale("specializations", build) {
        let data: Vec<models::Specialization> = client.fetch_all("specializations")?;
        cache
            .save("specializations", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Specializations", None);

    // 3. Traits
    if cache.is_stale("traits", build) {
        let data: Vec<models::Trait> = client.fetch_all("traits")?;
        cache
            .save("traits", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Traits", None);

    // 4. Skills
    if cache.is_stale("skills", build) {
        let data: Vec<models::Skill> = client.fetch_all("skills")?;
        cache
            .save("skills", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Skills", None);

    // 5. Professions (use schema version that includes skills_by_palette)
    if cache.is_stale("professions", build) {
        let data: Vec<models::Profession> = client.get_with_params(
            "professions",
            &[("ids", "all"), ("v", "2019-12-19T00:00:00.000Z")],
        )?;
        cache
            .save("professions", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Professions", None);

    // 6. Legends (schema that includes template `code`)
    if cache.is_stale("legends", build) {
        let data: Vec<models::Legend> = client.get_with_params(
            "legends",
            &[("ids", "all"), ("v", "2019-12-19T00:00:00.000Z")],
        )?;
        cache
            .save("legends", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Legends", None);

    // 7. PvP Amulets
    if cache.is_stale("pvp_amulets", build) {
        let data: Vec<models::PvpAmulet> = client.fetch_all("pvp/amulets")?;
        cache
            .save("pvp_amulets", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "PvP Amulets", None);

    // 8. Items (filtered to equipment-relevant types) — largest step, ~100k items
    // Uses fetch_by_ids (200-ID batches, 5 concurrent) with lenient per-item deserialization.
    if cache.is_stale("items", build) {
        let relevant_types = [
            "Armor",
            "Weapon",
            "Trinket",
            "Back",
            "UpgradeComponent",
            "Relic",
        ];
        let relevant_rarities = ["Exotic", "Ascended", "Legendary"];

        // Get all item IDs first
        on_progress(DownloadProgress {
            current_step: step,
            total_steps: TOTAL_STEPS,
            step_name: "Items (equipment)".to_string(),
            done: false,
            detail: Some("fetching item IDs...".into()),
            inner_done: 0,
            inner_total: 0,
        });
        let ids: Vec<serde_json::Value> = client.get("items")?;

        // Fetch all items as raw JSON values with live progress updates
        let raw_items: Vec<serde_json::Value> =
            client.fetch_by_ids_with_progress("items", &ids, |fetched, total| {
                on_progress(DownloadProgress {
                    current_step: step,
                    total_steps: TOTAL_STEPS,
                    step_name: "Items (equipment)".to_string(),
                    done: false,
                    detail: Some(format!("{} / {} items fetched", fetched, total)),
                    inner_done: fetched,
                    inner_total: total,
                });
            })?;

        // Filter to equipment-relevant items with lenient deserialization.
        // Consume `raw_items` by-value via into_iter so each rejected Value drops
        // before the next iteration — only the ~5k surviving Items are retained
        // for `cache.save`.
        let mut equipment_items: Vec<models::Item> = Vec::with_capacity(ids.len() / 20); // ~5% of items expected
        for val in raw_items {
            if let Ok(item) = serde_json::from_value::<models::Item>(val) {
                if relevant_types.contains(&item.item_type.as_str())
                    && relevant_rarities.contains(&item.rarity.as_str())
                {
                    equipment_items.push(item);
                }
            }
        }

        cache
            .save("items", &equipment_items, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Items (equipment)", None);

    // 9. Icons — separate from JSON. Skip files already on disk.
    let urls = crate::graphics::collect_from_cache(cache);
    let gfx = cache.graphics_dir();
    let _ = crate::graphics::download_missing(client, &gfx, &urls, |done, total| {
        on_progress(DownloadProgress {
            current_step: step,
            total_steps: TOTAL_STEPS,
            step_name: "Icons".into(),
            done: false,
            detail: Some(if total == 0 {
                "up to date".into()
            } else {
                format!("{done} / {total}")
            }),
            inner_done: done,
            inner_total: total,
        });
    });
    report(&mut on_progress, &mut step, "Icons", None);

    Ok(build)
}

/// Game data plus official name packs (de/es/fr/zh). One bar; skips packs that match `build`.
pub fn download_game_and_names(
    client: &Gw2Client,
    cache: &DataCache,
    cancelled: impl Fn() -> bool,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<u32, ApiError> {
    const NAME_STEPS: usize = 4;
    let total = TOTAL_STEPS + NAME_STEPS;
    let build = download_all(client, cache, |p| {
        on_progress(DownloadProgress {
            current_step: p.current_step,
            total_steps: total,
            step_name: p.step_name,
            done: false,
            detail: p.detail,
            inner_done: p.inner_done,
            inner_total: p.inner_total,
        });
    })?;
    for (i, lang) in crate::localize::API_LANGS.iter().enumerate() {
        if cancelled() {
            return Err(ApiError::Internal("cancelled".into()));
        }
        let step = TOTAL_STEPS + i + 1;
        on_progress(DownloadProgress {
            current_step: step,
            total_steps: total,
            step_name: format!("Names ({lang})"),
            done: false,
            detail: None,
            inner_done: 0,
            inner_total: 0,
        });
        if cache.is_stale(&crate::localize::cache_key(lang), build) {
            crate::localize::download(
                cache,
                lang,
                || cancelled(),
                |msg| {
                    let (inner_done, inner_total) = parse_items_progress(msg);
                    on_progress(DownloadProgress {
                        current_step: step,
                        total_steps: total,
                        step_name: format!("Names ({lang})"),
                        done: false,
                        detail: Some(msg.to_string()),
                        inner_done,
                        inner_total,
                    });
                },
            )?;
        }
    }
    on_progress(DownloadProgress {
        current_step: total,
        total_steps: total,
        step_name: "Done".into(),
        done: true,
        detail: None,
        inner_done: 0,
        inner_total: 0,
    });
    Ok(build)
}

fn parse_items_progress(msg: &str) -> (usize, usize) {
    let Some(rest) = msg.strip_prefix("items ") else {
        return (0, 0);
    };
    let mut parts = rest.split('/');
    let done = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let total = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    (done, total)
}
