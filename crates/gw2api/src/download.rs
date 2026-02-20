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
}

/// Download all game data, calling `on_progress` after each endpoint.
/// Skips endpoints that are already cached at the current build.
pub fn download_all(
    client: &Gw2Client,
    cache: &DataCache,
    mut on_progress: impl FnMut(DownloadProgress),
) -> Result<(), ApiError> {
    let build = client.get_build_number()?;
    let total = 8;
    let mut step = 0;

    let mut report = |name: &str, step: &mut usize| {
        *step += 1;
        on_progress(DownloadProgress {
            current_step: *step,
            total_steps: total,
            step_name: name.to_string(),
            done: *step >= total,
        });
    };

    // 1. Item stats
    if cache.is_stale("itemstats", build) {
        let data: Vec<models::ItemStat> = client.fetch_all("itemstats")?;
        cache.save("itemstats", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Item stats", &mut step);

    // 2. Specializations
    if cache.is_stale("specializations", build) {
        let data: Vec<models::Specialization> = client.fetch_all("specializations")?;
        cache.save("specializations", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Specializations", &mut step);

    // 3. Traits
    if cache.is_stale("traits", build) {
        let data: Vec<models::Trait> = client.fetch_all("traits")?;
        cache.save("traits", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Traits", &mut step);

    // 4. Skills
    if cache.is_stale("skills", build) {
        let data: Vec<models::Skill> = client.fetch_all("skills")?;
        cache.save("skills", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Skills", &mut step);

    // 5. Professions
    if cache.is_stale("professions", build) {
        let data: Vec<models::Profession> = client.fetch_all("professions")?;
        cache.save("professions", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Professions", &mut step);

    // 6. Legends
    if cache.is_stale("legends", build) {
        let data: Vec<models::Legend> = client.fetch_all("legends")?;
        cache.save("legends", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Legends", &mut step);

    // 7. PvP Amulets
    if cache.is_stale("pvp_amulets", build) {
        let data: Vec<models::PvpAmulet> = client.fetch_all("pvp/amulets")?;
        cache.save("pvp_amulets", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("PvP Amulets", &mut step);

    // 8. Items (filtered to equipment-relevant types)
    if cache.is_stale("items", build) {
        let data = fetch_equipment_items(client)?;
        cache.save("items", &data, build).map_err(|e| ApiError::Api {
            status: 0,
            message: e.to_string(),
        })?;
    }
    report("Items (equipment)", &mut step);

    Ok(())
}

/// Fetch only equipment-relevant items: Armor, Weapon, Trinket, Back,
/// UpgradeComponent, Relic of Exotic/Ascended/Legendary rarity.
fn fetch_equipment_items(client: &Gw2Client) -> Result<Vec<models::Item>, ApiError> {
    let relevant_types = ["Armor", "Weapon", "Trinket", "Back", "UpgradeComponent", "Relic"];
    let relevant_rarities = ["Exotic", "Ascended", "Legendary"];

    // Fetch all items via fetch_all (uses fetch_by_ids internally), then filter.
    let all_items: Vec<models::Item> = client.fetch_all("items")?;

    let equipment_items = all_items
        .into_iter()
        .filter(|item| {
            relevant_types.contains(&item.item_type.as_str())
                && relevant_rarities.contains(&item.rarity.as_str())
        })
        .collect();

    Ok(equipment_items)
}
