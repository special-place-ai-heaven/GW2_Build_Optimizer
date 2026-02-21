//! In-memory indexed game database.
//! Pre-indexes all cached game data into HashMaps for O(1) lookups.
//! This is the single source of truth for the optimizer — loaded once from
//! the file cache, then queried throughout the optimization pipeline.

use std::collections::HashMap;

use gw2_api::cache::DataCache;
use gw2_api::models::{
    Item, ItemStat, Legend, Profession, PvpAmulet, Skill, Specialization, Trait as GW2Trait,
};

/// In-memory indexed game database loaded from cache.
#[derive(Clone)]
pub struct GameDb {
    pub items: HashMap<u32, Item>,
    pub itemstats: HashMap<u32, ItemStat>,
    pub skills: HashMap<u32, Skill>,
    pub traits: HashMap<u32, GW2Trait>,
    pub specializations: HashMap<u32, Specialization>,
    pub professions: HashMap<String, Profession>,
    pub legends: HashMap<String, Legend>,
    pub pvp_amulets: HashMap<u32, PvpAmulet>,

    // Derived indexes for fast lookups
    pub skills_by_profession: HashMap<String, Vec<u32>>,
    pub traits_by_spec: HashMap<u32, Vec<u32>>,
    pub items_by_type: HashMap<String, Vec<u32>>,
    pub runes: Vec<u32>,
    pub sigils: Vec<u32>,
    pub relics: Vec<u32>,
    // Skill ↔ palette ID mapping (for build template chat codes)
    pub skill_to_palette: HashMap<u32, u32>,
    pub palette_to_skill: HashMap<u32, u32>,
}

impl GameDb {
    /// Load all game data from the file cache and build indexes.
    pub fn load(cache: &DataCache) -> Result<Self, String> {
        // Load raw vectors from cache
        let items_vec: Vec<Item> = cache
            .load("items")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let itemstats_vec: Vec<ItemStat> = cache
            .load("itemstats")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let skills_vec: Vec<Skill> = cache
            .load("skills")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let traits_vec: Vec<GW2Trait> = cache
            .load("traits")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let specs_vec: Vec<Specialization> = cache
            .load("specializations")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let professions_vec: Vec<Profession> = cache
            .load("professions")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let legends_vec: Vec<Legend> = cache
            .load("legends")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();
        let pvp_amulets_vec: Vec<PvpAmulet> = cache
            .load("pvp_amulets")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        // Build primary indexes (ID → data)
        let items: HashMap<u32, Item> = items_vec.into_iter().map(|i| (i.id, i)).collect();
        let itemstats: HashMap<u32, ItemStat> =
            itemstats_vec.into_iter().map(|i| (i.id, i)).collect();
        let skills: HashMap<u32, Skill> = skills_vec.into_iter().map(|s| (s.id, s)).collect();
        let traits: HashMap<u32, GW2Trait> = traits_vec.into_iter().map(|t| (t.id, t)).collect();
        let specializations: HashMap<u32, Specialization> =
            specs_vec.into_iter().map(|s| (s.id, s)).collect();
        let professions: HashMap<String, Profession> =
            professions_vec.into_iter().map(|p| (p.id.clone(), p)).collect();
        let legends: HashMap<String, Legend> =
            legends_vec.into_iter().map(|l| (l.id.clone(), l)).collect();
        let pvp_amulets: HashMap<u32, PvpAmulet> =
            pvp_amulets_vec.into_iter().map(|a| (a.id, a)).collect();

        // Build derived indexes
        let mut skills_by_profession: HashMap<String, Vec<u32>> = HashMap::new();
        for skill in skills.values() {
            for prof in &skill.professions {
                skills_by_profession
                    .entry(prof.clone())
                    .or_default()
                    .push(skill.id);
            }
        }

        let mut traits_by_spec: HashMap<u32, Vec<u32>> = HashMap::new();
        for t in traits.values() {
            traits_by_spec
                .entry(t.specialization)
                .or_default()
                .push(t.id);
        }

        let mut items_by_type: HashMap<String, Vec<u32>> = HashMap::new();
        let mut runes = Vec::new();
        let mut sigils = Vec::new();
        let mut relics = Vec::new();

        // Build skill ↔ palette ID maps from professions
        let mut skill_to_palette: HashMap<u32, u32> = HashMap::new();
        let mut palette_to_skill: HashMap<u32, u32> = HashMap::new();
        for prof in professions.values() {
            for pair in &prof.skills_by_palette {
                if pair.len() == 2 {
                    let palette_id = pair[0];
                    let skill_id = pair[1];
                    skill_to_palette.insert(skill_id, palette_id);
                    palette_to_skill.insert(palette_id, skill_id);
                }
            }
        }

        for item in items.values() {
            items_by_type
                .entry(item.item_type.clone())
                .or_default()
                .push(item.id);

            // Categorize upgrade components
            if item.item_type == "UpgradeComponent" {
                if let Some(ref details) = item.details {
                    match details.detail_type.as_deref() {
                        Some("Rune") => runes.push(item.id),
                        Some("Sigil") => sigils.push(item.id),
                        _ => {}
                    }
                }
            } else if item.item_type == "Relic" {
                relics.push(item.id);
            }
        }

        Ok(GameDb {
            items,
            itemstats,
            skills,
            traits,
            specializations,
            professions,
            legends,
            pvp_amulets,
            skills_by_profession,
            traits_by_spec,
            items_by_type,
            runes,
            sigils,
            relics,
            skill_to_palette,
            palette_to_skill,
        })
    }

    /// Get a profession by name.
    pub fn profession(&self, name: &str) -> Option<&Profession> {
        self.professions.get(name)
    }

    /// Get all skills for a profession.
    pub fn profession_skills(&self, profession: &str) -> Vec<&Skill> {
        self.skills_by_profession
            .get(profession)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.skills.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all traits in a specialization.
    pub fn spec_traits(&self, spec_id: u32) -> Vec<&GW2Trait> {
        self.traits_by_spec
            .get(&spec_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.traits.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get all rune items.
    pub fn all_runes(&self) -> Vec<&Item> {
        self.runes
            .iter()
            .filter_map(|id| self.items.get(id))
            .collect()
    }

    /// Get all sigil items.
    pub fn all_sigils(&self) -> Vec<&Item> {
        self.sigils
            .iter()
            .filter_map(|id| self.items.get(id))
            .collect()
    }

    /// Get all relics.
    pub fn all_relics(&self) -> Vec<&Item> {
        self.relics
            .iter()
            .filter_map(|id| self.items.get(id))
            .collect()
    }

    /// Get a specialization by ID.
    pub fn spec(&self, id: u32) -> Option<&Specialization> {
        self.specializations.get(&id)
    }

    /// Summary stats for logging.
    pub fn summary(&self) -> String {
        format!(
            "GameDb: {} items, {} itemstats, {} skills, {} traits, {} specs, {} professions, {} runes, {} sigils, {} relics",
            self.items.len(),
            self.itemstats.len(),
            self.skills.len(),
            self.traits.len(),
            self.specializations.len(),
            self.professions.len(),
            self.runes.len(),
            self.sigils.len(),
            self.relics.len(),
        )
    }
}
