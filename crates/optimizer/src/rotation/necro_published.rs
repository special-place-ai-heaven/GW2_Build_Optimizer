//! A published GuildJen WvW Necromancer build as a fixture
//! (`specs/007-trait-triggers`, US3, T049).
//!
//! Source: <https://guildjen.com/power-spite-reaper-roaming-build/> "Power
//! Spite Reaper Roaming Build", scraped 2026-09-06 into
//! `<addons_dir>/gw2_build_optimizer/benchmarks/guildjen_necromancer_wvw.json`
//! (build code `[&DQg1HhMfIhvBEsES8RLxEvUA9QC9Ab0BAxMDEwAAAAAAAAAAAAAAAAAAAAA=]`,
//! decoded 2026-09-08). Real API ids, so it resolves only against the real
//! `GameDb` from the cache: every test that uses it is `#[ignore]` and reads
//! `gw2_api::dev_config::cache_dir()`.

use crate::gamedb::GameDb;
use crate::validation::ValidatedBuild;

/// Spite, Blood Magic, Reaper.
pub const SPEC_IDS: [u32; 3] = [53, 19, 34];
/// Malicious Swarm, Signets of Suffering, Dread; Blood Renewal, Vampiric
/// Presence, Blood Bank; Relentless Pursuit, Chilling Victory, Blighter's Boon.
pub const TRAIT_IDS: [[u32; 3]; 3] = [[916, 909, 919], [1876, 1844, 782], [2026, 2008, 1932]];
/// "Your Soul Is Mine!"; "You Are All Weaklings!", Plague Signet, Spectral
/// Walk; "Chilled to the Bone!" (palette ids 4801, 4849, 245, 445, 4867).
pub const HEAL_ID: u32 = 30_488;
pub const UTILITY_IDS: [u32; 3] = [29_414, 10_562, 10_685];
pub const ELITE_ID: u32 = 30_105;
/// Superior Rune of Resistance.
pub const RUNE_ID: u32 = 49_460;
/// Set 1 Axe / Sword: Battle, Celerity. Set 2 Dagger / Warhorn: Cleansing, Energy.
pub const SIGIL_IDS: [u32; 4] = [24_601, 24_865, 67_340, 24_607];
/// Relic of the Twin Generals.
pub const RELIC_ID: u32 = 101_767;
pub const PREFIX: &str = "Marauder";

/// The published build resolved against the real database through the same
/// plate path the Choya and the cached-build tests use, so gear slots, sigil
/// seats and trait names come from `validate_gemini_build`.
pub fn build(db: &GameDb) -> Result<ValidatedBuild, String> {
    let trait_name = |id: u32| {
        db.traits
            .get(&id)
            .map(|t| t.name.clone())
            .ok_or_else(|| format!("trait {id} missing from the cache"))
    };
    let skill_name = |id: u32| {
        db.skills
            .get(&id)
            .map(|s| s.name.clone())
            .ok_or_else(|| format!("skill {id} missing from the cache"))
    };
    let item_name = |id: u32| {
        db.items
            .get(&id)
            .map(|i| i.name.clone())
            .ok_or_else(|| format!("item {id} missing from the cache"))
    };
    let mut specs = Vec::new();
    for (spec_id, traits) in SPEC_IDS.iter().zip(TRAIT_IDS.iter()) {
        let name = db
            .specializations
            .get(spec_id)
            .map(|s| s.name.clone())
            .ok_or_else(|| format!("specialization {spec_id} missing"))?;
        let names = traits
            .iter()
            .map(|id| trait_name(*id))
            .collect::<Result<Vec<_>, _>>()?;
        specs.push(serde_json::json!({ "name": name, "traits": names }));
    }
    let utilities = UTILITY_IDS
        .iter()
        .map(|id| skill_name(*id))
        .collect::<Result<Vec<_>, _>>()?;
    let sigils = SIGIL_IDS
        .iter()
        .map(|id| item_name(*id))
        .collect::<Result<Vec<_>, _>>()?;
    let plate = serde_json::json!({
        "specializations": specs,
        "weapons": {
            "set1": { "main": "Axe", "off": "Sword" },
            "set2": { "main": "Dagger", "off": "Warhorn" },
        },
        "skills": {
            "heal": skill_name(HEAL_ID)?,
            "utilities": utilities,
            "elite": skill_name(ELITE_ID)?,
        },
        "rune": item_name(RUNE_ID)?,
        "sigils": sigils,
        "relic": item_name(RELIC_ID)?,
        "stat_prefix": PREFIX,
        "explanation": "GuildJen Power Spite Reaper (WvW roaming), scraped 2026-09-06",
    });
    let parsed = crate::prompts::parse_gemini_build(&plate.to_string())?;
    let validated = crate::validation::validate_gemini_build(&parsed, db, "Necromancer");
    if validated.errors.is_empty() {
        Ok(validated)
    } else {
        Err(format!("{:?}", validated.errors))
    }
}

/// The published build with one trait swapped for a line neighbour, for the
/// ranking-direction test (SC-003).
pub fn build_with_trait(db: &GameDb, from: u32, to: u32) -> Result<ValidatedBuild, String> {
    let mut validated = build(db)?;
    let to_name = db
        .traits
        .get(&to)
        .map(|t| t.name.clone())
        .ok_or_else(|| format!("trait {to} missing from the cache"))?;
    for spec in &mut validated.specializations {
        for (idx, id) in spec.trait_ids.clone().iter().enumerate() {
            if *id == from {
                spec.trait_ids[idx] = to;
                spec.trait_names[idx] = to_name.clone();
                if let Some(slot) = spec.all_trait_ids.iter_mut().find(|t| **t == from) {
                    *slot = to;
                }
            }
        }
    }
    Ok(validated)
}
