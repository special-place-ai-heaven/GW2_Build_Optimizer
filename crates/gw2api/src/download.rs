//! Full data download orchestration.
//! Downloads all game data endpoints and caches them locally.

use crate::cache::DataCache;
use crate::client::{with_cancel_bridge, ApiError, Gw2Client};
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

const TOTAL_STEPS: usize = 10;

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
///
/// `cancelled` is checked between steps *and* bridged into `client`'s cancel
/// flag (see `with_cancel_bridge`), because the waits worth interrupting —
/// retry backoff, rate-limit sleeps — happen inside `client` while this thread
/// is blocked and cannot poll anything.
pub fn download_all(
    client: &Gw2Client,
    cache: &DataCache,
    cancelled: impl Fn() -> bool + Sync,
    on_progress: impl FnMut(DownloadProgress),
) -> Result<u32, ApiError> {
    // Fail before the first request rather than after it, and arm the client
    // synchronously so an already-cancelled caller needs no watchdog at all.
    if cancelled() {
        client.cancel();
        return Err(ApiError::Cancelled);
    }
    with_cancel_bridge(client, &cancelled, || {
        download_steps(client, cache, &cancelled, on_progress)
    })
}

/// The ten download steps. Split out so `download_all` reads as "arm
/// cancellation, run the steps, disarm".
fn download_steps(
    client: &Gw2Client,
    cache: &DataCache,
    cancelled: &impl Fn() -> bool,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<u32, ApiError> {
    let check = || {
        if cancelled() {
            Err(ApiError::Cancelled)
        } else {
            Ok(())
        }
    };

    let build = client.get_build_number()?;
    let mut step = 0;

    // 1. Item stats
    check()?;
    if cache.is_stale("itemstats", build) {
        let data: Vec<models::ItemStat> = client.fetch_all("itemstats")?;
        cache
            .save("itemstats", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Item stats", None);

    // 2. Specializations
    check()?;
    if cache.is_stale("specializations", build) {
        let data: Vec<models::Specialization> = client.fetch_all("specializations")?;
        cache
            .save("specializations", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Specializations", None);

    // 3. Traits
    check()?;
    if cache.is_stale("traits", build) {
        let data: Vec<models::Trait> = client.fetch_all("traits")?;
        cache
            .save("traits", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Traits", None);

    // 4. Skills
    check()?;
    if cache.is_stale("skills", build) {
        let data: Vec<models::Skill> = client.fetch_all("skills")?;
        cache
            .save("skills", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Skills", None);

    // 5. Professions (use schema version that includes skills_by_palette)
    check()?;
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
    check()?;
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

    // 7. Pets (ranger terrestrial/aquatic companions)
    check()?;
    if cache.is_stale("pets", build) {
        let data: Vec<models::Pet> = client.fetch_all("pets")?;
        cache
            .save("pets", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Pets", None);

    // 8. PvP Amulets
    check()?;
    if cache.is_stale("pvp_amulets", build) {
        let data: Vec<models::PvpAmulet> = client.fetch_all("pvp/amulets")?;
        cache
            .save("pvp_amulets", &data, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "PvP Amulets", None);

    // 9. Items (filtered to equipment-relevant types) — largest step, ~100k items
    // Uses fetch_by_ids (200-ID batches, 5 concurrent) with lenient per-item deserialization.
    check()?;
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

        // Fetch all items as raw JSON values with live progress updates.
        // Singleton 5xx ids are skip-listed so a hole is not written as success.
        let (raw_items, skipped): (Vec<serde_json::Value>, Vec<serde_json::Value>) = client
            .fetch_by_ids_with_skips("items", &ids, |fetched, total| {
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
        cache
            .save("items.skipped", &skipped, build)
            .map_err(|e| ApiError::Cache(e.to_string()))?;
    }
    report(&mut on_progress, &mut step, "Items (equipment)", None);

    // 10. Icons — separate from JSON. Skip files already on disk.
    check()?;
    let urls = crate::graphics::collect_from_cache(cache);
    let gfx = cache.graphics_dir();
    // A single bad icon must not abort a refresh, but cancellation must.
    if let Err(ApiError::Cancelled) =
        crate::graphics::download_missing(client, &gfx, &urls, |done, total| {
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
        })
    {
        return Err(ApiError::Cancelled);
    }
    report(&mut on_progress, &mut step, "Icons", None);

    Ok(build)
}

/// Game data plus official name packs (de/es/fr/zh). One bar; skips packs that match `build`.
pub fn download_game_and_names(
    client: &Gw2Client,
    cache: &DataCache,
    cancelled: impl Fn() -> bool + Sync,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<u32, ApiError> {
    const NAME_STEPS: usize = 4;
    let total = TOTAL_STEPS + NAME_STEPS;
    let build = download_all(client, cache, &cancelled, |p| {
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
            client.cancel();
            return Err(ApiError::Cancelled);
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
            crate::localize::download(cache, lang, &cancelled, |msg| {
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
            })?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    fn temp_cache_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "gw2_download_test_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    /// A caller that is already cancelled must not reach the network, must not
    /// report a step, and must leave the client cancelled so any wait it is
    /// asked for later aborts instead of sleeping out a retry ladder.
    #[test]
    fn download_all_observes_cancel() {
        let dir = temp_cache_dir("cancel");
        let cache = DataCache::new(&dir);
        let client = Gw2Client::without_key().unwrap();
        let token = Arc::new(AtomicBool::new(true));
        let watched = Arc::clone(&token);

        let mut steps = 0usize;
        let started = Instant::now();
        let err = download_all(
            &client,
            &cache,
            move || watched.load(Ordering::Relaxed),
            |_| steps += 1,
        )
        .expect_err("a cancelled download must not succeed");
        let elapsed = started.elapsed();
        let _ = std::fs::remove_dir_all(&dir);

        assert!(matches!(err, ApiError::Cancelled), "got {err:?}");
        assert_eq!(steps, 0, "no step should have been reported");
        assert!(
            client.is_cancelled(),
            "cancellation must reach the client's own waits"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "cancelled download took {elapsed:?} — it went to the network"
        );
    }

    /// Live smoke test for the whole download path. Lives here rather than in
    /// `tests/` for the same reason the `test_live_fetch_*` tests live in
    /// `client.rs`: this crate keeps its `#[ignore]` network tests beside the
    /// code they exercise, and a second test target costs a full extra link
    /// for tests that never run by default.
    ///
    /// Run with:
    /// `cargo test -p gw2-api test_full_download_pipeline -- --ignored --nocapture`
    #[test]
    #[ignore] // Requires network
    fn test_full_download_pipeline() {
        let client = Gw2Client::without_key().unwrap();

        // Build number
        let build = client.get_build_number().unwrap();
        println!("[OK] Build: {}", build);

        // Traits
        let start = Instant::now();
        let traits: Vec<models::Trait> = client.fetch_all("traits").unwrap();
        println!(
            "[OK] Traits: {} in {:.1}s",
            traits.len(),
            start.elapsed().as_secs_f64()
        );
        assert!(
            traits.len() > 100,
            "Expected >100 traits, got {}",
            traits.len()
        );

        // Skills
        let start = Instant::now();
        let skills: Vec<models::Skill> = client.fetch_all("skills").unwrap();
        println!(
            "[OK] Skills: {} in {:.1}s",
            skills.len(),
            start.elapsed().as_secs_f64()
        );
        assert!(
            skills.len() > 100,
            "Expected >100 skills, got {}",
            skills.len()
        );

        // Specializations
        let start = Instant::now();
        let specs: Vec<models::Specialization> = client.fetch_all("specializations").unwrap();
        println!(
            "[OK] Specs: {} in {:.1}s",
            specs.len(),
            start.elapsed().as_secs_f64()
        );
        assert!(specs.len() > 30, "Expected >30 specs, got {}", specs.len());

        // Itemstats
        let start = Instant::now();
        let itemstats: Vec<models::ItemStat> = client.fetch_all("itemstats").unwrap();
        println!(
            "[OK] Itemstats: {} in {:.1}s",
            itemstats.len(),
            start.elapsed().as_secs_f64()
        );
        assert!(
            itemstats.len() > 50,
            "Expected >50 itemstats, got {}",
            itemstats.len()
        );

        // Items (first 2000 only — full download too slow for test)
        let start = Instant::now();
        let all_ids: Vec<serde_json::Value> = client.get("items").unwrap();
        println!(
            "[OK] Item IDs: {} in {:.1}s",
            all_ids.len(),
            start.elapsed().as_secs_f64()
        );

        let subset = &all_ids[..2000.min(all_ids.len())];
        let start = Instant::now();
        let items: Vec<serde_json::Value> = client.fetch_by_ids("items", subset).unwrap();
        println!(
            "[OK] Items (2000 subset): {} in {:.1}s",
            items.len(),
            start.elapsed().as_secs_f64()
        );
        assert!(
            items.len() > 1000,
            "Expected >1000 items from 2000 IDs, got {}",
            items.len()
        );

        // Professions
        let start = Instant::now();
        let profs: Vec<models::Profession> = client
            .get_with_params(
                "professions",
                &[("ids", "all"), ("v", "2019-12-19T00:00:00.000Z")],
            )
            .unwrap();
        println!(
            "[OK] Professions: {} in {:.1}s",
            profs.len(),
            start.elapsed().as_secs_f64()
        );
        assert_eq!(
            profs.len(),
            9,
            "Expected 9 professions, got {}",
            profs.len()
        );

        println!(
            "
=== ALL ENDPOINTS OK ==="
        );
    }

    #[test]
    fn parse_items_progress_reads_done_and_total() {
        assert_eq!(parse_items_progress("items 40/200"), (40, 200));
        assert_eq!(parse_items_progress("specializations"), (0, 0));
    }
}
