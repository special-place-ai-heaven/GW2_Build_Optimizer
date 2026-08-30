//! In-memory indexed game database.
//! Pre-indexes all cached game data into HashMaps for O(1) lookups.
//! This is the single source of truth for the optimizer — loaded once from
//! the file cache, then queried throughout the optimization pipeline.

use std::collections::HashMap;

use gw2_api::cache::DataCache;
use gw2_api::models::{
    Item, ItemStat, Legend, Pet, Profession, PvpAmulet, Skill, Specialization, Trait as GW2Trait,
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
    /// Ranger pets. Empty until the cache has a `pets` download.
    pub pets: HashMap<u32, Pet>,

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

    // Reverse indexes for synergy queries (condition/buff name → IDs that apply it)
    pub traits_by_condition: HashMap<String, Vec<u32>>,
    pub skills_by_condition: HashMap<String, Vec<u32>>,
    pub traits_by_buff: HashMap<String, Vec<u32>>,
    pub skills_by_buff: HashMap<String, Vec<u32>>,

    /// Official API names for the current UI language. Optimizer still uses English `.name`.
    pub localized: Option<std::sync::Arc<gw2_api::localize::LocalizedNames>>,
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
        let pets_vec: Vec<Pet> = cache
            .load("pets")
            .map_err(|e| e.to_string())?
            .unwrap_or_default();

        // Validate critical data is non-empty
        if professions_vec.is_empty() {
            return Err("No professions found in cache — game data may not be downloaded".into());
        }
        if specs_vec.is_empty() {
            return Err(
                "No specializations found in cache — game data may not be downloaded".into(),
            );
        }
        if itemstats_vec.is_empty() {
            return Err("No item stats found in cache — game data may not be downloaded".into());
        }
        if skills_vec.is_empty() {
            return Err("No skills found in cache — game data may not be downloaded".into());
        }
        if traits_vec.is_empty() {
            return Err("No traits found in cache — game data may not be downloaded".into());
        }
        if items_vec.is_empty() {
            return Err("No items found in cache — game data may not be downloaded".into());
        }

        // Build primary indexes (ID → data)
        let items: HashMap<u32, Item> = items_vec.into_iter().map(|i| (i.id, i)).collect();
        let itemstats: HashMap<u32, ItemStat> =
            itemstats_vec.into_iter().map(|i| (i.id, i)).collect();
        let skills: HashMap<u32, Skill> = skills_vec.into_iter().map(|s| (s.id, s)).collect();
        let traits: HashMap<u32, GW2Trait> = traits_vec.into_iter().map(|t| (t.id, t)).collect();
        let specializations: HashMap<u32, Specialization> =
            specs_vec.into_iter().map(|s| (s.id, s)).collect();
        let professions: HashMap<String, Profession> = professions_vec
            .into_iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        let legends: HashMap<String, Legend> =
            legends_vec.into_iter().map(|l| (l.id.clone(), l)).collect();
        let pvp_amulets: HashMap<u32, PvpAmulet> =
            pvp_amulets_vec.into_iter().map(|a| (a.id, a)).collect();
        let pets: HashMap<u32, Pet> = pets_vec.into_iter().map(|p| (p.id, p)).collect();

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

        // Build skill ↔ palette ID maps from professions.
        //
        // Iterate professions in a deterministic order (sorted by name) so
        // that when a skill_id or palette_id is shared across professions
        // (e.g. racial elites, downed-state skills), the last-writer-wins
        // resolution is stable across runs and machines. `HashMap::values()`
        // ordering is unspecified — without this sort the same input cache
        // could yield two different `skill_to_palette` / `palette_to_skill`
        // mappings, then break weapon-skill-by-palette lookups depending on
        // which profession won the insert.
        let mut prof_names: Vec<&String> = professions.keys().collect();
        prof_names.sort_unstable();
        let mut skill_to_palette: HashMap<u32, u32> = HashMap::new();
        let mut palette_to_skill: HashMap<u32, u32> = HashMap::new();
        for prof_name in prof_names {
            if let Some(prof) = professions.get(prof_name) {
                for pair in &prof.skills_by_palette {
                    if pair.len() == 2 {
                        let palette_id = pair[0];
                        let skill_id = pair[1];
                        skill_to_palette.insert(skill_id, palette_id);
                        palette_to_skill.insert(palette_id, skill_id);
                    }
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

        // Build reverse indexes for synergy queries
        let mut traits_by_condition: HashMap<String, Vec<u32>> = HashMap::new();
        let mut traits_by_buff: HashMap<String, Vec<u32>> = HashMap::new();
        for t in traits.values() {
            for fact in &t.facts {
                if let gw2_api::models::facts::Fact::Buff {
                    status: Some(s), ..
                } = fact
                {
                    if is_condition(s) {
                        traits_by_condition
                            .entry(condition_index_key(s).to_string())
                            .or_default()
                            .push(t.id);
                    } else if is_boon(s) {
                        traits_by_buff.entry(s.clone()).or_default().push(t.id);
                    }
                }
            }
        }

        let mut skills_by_condition: HashMap<String, Vec<u32>> = HashMap::new();
        let mut skills_by_buff: HashMap<String, Vec<u32>> = HashMap::new();
        for skill in skills.values() {
            for fact in &skill.facts {
                if let gw2_api::models::facts::Fact::Buff {
                    status: Some(s), ..
                } = fact
                {
                    if is_condition(s) {
                        skills_by_condition
                            .entry(condition_index_key(s).to_string())
                            .or_default()
                            .push(skill.id);
                    } else if is_boon(s) {
                        skills_by_buff.entry(s.clone()).or_default().push(skill.id);
                    }
                }
            }
        }

        // Deduplicate (a trait/skill may have multiple Buff facts for same condition).
        // Also sort the profession/spec/type indexes so downstream consumers
        // (`profession_skills()`, `section_profession_skills()`, LLM tool execs)
        // get a stable order across runs — these Vecs were previously populated
        // from `HashMap::values()` iteration which has non-deterministic order.
        for ids in traits_by_condition.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
        for ids in traits_by_buff.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
        for ids in skills_by_condition.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
        for ids in skills_by_buff.values_mut() {
            ids.sort_unstable();
            ids.dedup();
        }
        for ids in skills_by_profession.values_mut() {
            ids.sort_unstable();
        }
        for ids in traits_by_spec.values_mut() {
            ids.sort_unstable();
        }
        for ids in items_by_type.values_mut() {
            ids.sort_unstable();
        }

        // Sort the upgrade-item id vecs so `all_runes()`/`all_sigils()`/
        // `all_relics()` iterate in stable order. Populated from
        // `items.values()` HashMap iteration, which is unspecified — beam
        // search neighbor order (swap_rune, swap_relic, swap_sigil_slots)
        // inherited the nondeterminism without these sorts.
        runes.sort_unstable();
        sigils.sort_unstable();
        relics.sort_unstable();

        Ok(GameDb {
            items,
            itemstats,
            skills,
            traits,
            specializations,
            professions,
            legends,
            pvp_amulets,
            pets,
            skills_by_profession,
            traits_by_spec,
            items_by_type,
            runes,
            sigils,
            relics,
            skill_to_palette,
            palette_to_skill,
            traits_by_condition,
            skills_by_condition,
            traits_by_buff,
            skills_by_buff,
            localized: None,
        })
    }

    /// Get a profession by name.
    pub fn profession(&self, name: &str) -> Option<&Profession> {
        self.professions.get(name)
    }

    /// Deterministic itemstat lookup by name (case-insensitive). Returns the
    /// exact-name match if any, otherwise the shortest substring match
    /// (lower id wins ties). Exact ties do not lowest-id across distinct
    /// multiplier shapes: wiki three-stat Giver's (Toughness / Healing /
    /// BoonDuration) outranks other Giver's templates, then lower id.
    ///
    /// Centralizes the policy used by validation, Gemini tool execs, and
    /// LLM-context builders — a raw `itemstats.values().find(contains)` is
    /// non-deterministic because `HashMap::values()` iteration order is
    /// unspecified, so the same input could resolve to different itemstats
    /// across runs and machines.
    pub fn itemstat_by_name(&self, needle: &str) -> Option<&ItemStat> {
        if needle.is_empty() {
            return None;
        }
        let needle_lower = needle.to_lowercase();
        let needle_key = gw2_core::i18n::alnum_key(needle);
        let mut exact: Option<&ItemStat> = None;
        let mut fuzzy: Option<(usize, u32, &ItemStat)> = None;
        for is in self.itemstats.values() {
            let lower = is.name.to_lowercase();
            let alnum_hit =
                !needle_key.is_empty() && gw2_core::i18n::alnum_key(&is.name) == needle_key;
            if lower == needle_lower || alnum_hit {
                match exact {
                    Some(prev) if !Self::exact_name_outranks(is, prev) => {}
                    _ => exact = Some(is),
                }
            } else if needle_lower.len() >= 5 && lower.contains(&needle_lower) {
                let key = (is.name.len(), is.id);
                match fuzzy {
                    Some((plen, pid, _)) if (plen, pid) <= key => {}
                    _ => fuzzy = Some((key.0, key.1, is)),
                }
            }
        }
        exact.or(fuzzy.map(|(_, _, is)| is))
    }

    /// Exact-name ties: wiki three-stat Giver's outranks other multiplier
    /// shapes of that English name; otherwise lower id wins.
    fn exact_name_outranks(candidate: &ItemStat, incumbent: &ItemStat) -> bool {
        match (
            crate::itemstat_pool::is_wiki_givers_three_stat(candidate),
            crate::itemstat_pool::is_wiki_givers_three_stat(incumbent),
        ) {
            (true, false) => true,
            (false, true) => false,
            _ => candidate.id < incumbent.id,
        }
    }

    pub fn attach_localized(&mut self, mut names: gw2_api::localize::LocalizedNames) {
        names.by_english.clear();
        let mut add = |en: &str, loc: &str| {
            if !en.is_empty() && !loc.is_empty() {
                names
                    .by_english
                    .insert(en.to_ascii_lowercase(), loc.to_string());
            }
        };
        for (id, loc) in &names.skills {
            if let Some(s) = self.skills.get(id) {
                add(&s.name, loc);
            }
        }
        for (id, loc) in &names.traits {
            if let Some(t) = self.traits.get(id) {
                add(&t.name, loc);
            }
        }
        for (id, loc) in &names.specs {
            if let Some(s) = self.specializations.get(id) {
                add(&s.name, loc);
            }
        }
        for (id, loc) in &names.items {
            if let Some(i) = self.items.get(id) {
                add(&i.name, loc);
            }
        }
        for (id, loc) in &names.itemstats {
            if let Some(s) = self.itemstats.get(id) {
                add(&s.name, loc);
            }
        }
        for (id, loc) in &names.professions {
            add(id, loc);
            if let Some(p) = self.professions.get(id) {
                add(&p.name, loc);
            }
        }
        for (id, loc) in &names.legends {
            add(id, loc);
        }
        for (id, loc) in &names.pvp_amulets {
            if let Some(a) = self.pvp_amulets.get(id) {
                add(&a.name, loc);
            }
        }
        for (id, loc) in &names.pets {
            if let Some(p) = self.pets.get(id) {
                add(&p.name, loc);
                let compact = p.name.trim_start_matches("Juvenile ");
                if compact != p.name {
                    add(compact, loc);
                }
            }
        }
        self.localized = Some(std::sync::Arc::new(names));
    }

    pub fn loc_skill<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.localized
            .as_ref()
            .and_then(|l| l.skills.get(&id))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    pub fn loc_trait<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.localized
            .as_ref()
            .and_then(|l| l.traits.get(&id))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    pub fn loc_spec<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.localized
            .as_ref()
            .and_then(|l| l.specs.get(&id))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    pub fn loc_item<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.localized
            .as_ref()
            .and_then(|l| l.items.get(&id))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    pub fn loc_pet<'a>(&'a self, id: u32, fallback: &'a str) -> &'a str {
        self.localized
            .as_ref()
            .and_then(|l| l.pets.get(&id))
            .map(String::as_str)
            .unwrap_or(fallback)
    }

    /// English display name for a ranger pet id, or `#id` when the catalog
    /// has not been downloaded yet.
    pub fn pet_display_name(&self, id: u32) -> String {
        self.pets
            .get(&id)
            .map(|p| self.loc_pet(id, &p.name).to_string())
            .unwrap_or_else(|| format!("#{id}"))
    }

    pub fn pet_by_name(&self, needle: &str) -> Option<&Pet> {
        let needle = needle.trim();
        if needle.is_empty() {
            return None;
        }
        if let Some(id) = needle.strip_prefix('#').and_then(|s| s.parse::<u32>().ok()) {
            return self.pets.get(&id);
        }
        let compact = needle.trim_start_matches("Juvenile ");
        self.pets.values().find(|p| {
            p.name.eq_ignore_ascii_case(needle)
                || p.name
                    .trim_start_matches("Juvenile ")
                    .eq_ignore_ascii_case(compact)
        })
    }

    pub fn loc_prefix<'a>(&'a self, english: &'a str) -> &'a str {
        self.itemstat_by_name(english)
            .and_then(|s| {
                self.localized
                    .as_ref()
                    .and_then(|l| l.itemstats.get(&s.id))
                    .map(String::as_str)
            })
            .unwrap_or(english)
    }

    pub fn loc_name<'a>(&'a self, english: &'a str) -> &'a str {
        let Some(loc) = &self.localized else {
            return english;
        };
        loc.by_english
            .get(&english.to_ascii_lowercase())
            .map(String::as_str)
            .unwrap_or(english)
    }

    /// Palette ID for a build-template skill slot.
    ///
    /// Revenant legend skills share one palette per slot; `/v2/professions`
    /// `skills_by_palette` only lists the latest legend's skill IDs. A miss on
    /// an older legend skill is mapped through that shared slot palette.
    pub fn skill_palette_id(&self, skill_id: u32) -> u32 {
        if let Some(&p) = self.skill_to_palette.get(&skill_id) {
            return p;
        }
        let Some((heal_p, util_p, elite_p)) = self.revenant_shared_palettes() else {
            return 0;
        };
        for legend in self.legends.values() {
            if skill_id == legend.heal {
                return heal_p;
            }
            if skill_id == legend.elite {
                return elite_p;
            }
            if let Some(i) = legend.utilities.iter().position(|&u| u == skill_id) {
                if i < 3 {
                    return util_p[i];
                }
            }
        }
        0
    }

    /// Template byte for a revenant legend id (`Legend1`…), from `/v2/legends.code`.
    pub fn legend_template_code(&self, legend_id: &str) -> u8 {
        if let Some(c) = self.legends.get(legend_id).and_then(|l| l.code) {
            return c.min(255) as u8;
        }
        legend_id
            .strip_prefix("Legend")
            .and_then(|n| n.parse::<u8>().ok())
            .unwrap_or(0)
    }

    /// True when this legend's swap skill is ungated, or its elite spec is equipped.
    pub fn legend_available(&self, legend_id: &str, spec_ids: &[u32]) -> bool {
        let Some(legend) = self.legends.get(legend_id) else {
            return false;
        };
        match self.skills.get(&legend.swap).and_then(|s| s.specialization) {
            None => true,
            Some(spec_id) => spec_ids.contains(&spec_id),
        }
    }

    fn revenant_shared_palettes(&self) -> Option<(u32, [u32; 3], u32)> {
        for legend in self.legends.values() {
            let Some(&heal_p) = self.skill_to_palette.get(&legend.heal) else {
                continue;
            };
            let Some(&elite_p) = self.skill_to_palette.get(&legend.elite) else {
                continue;
            };
            if legend.utilities.len() < 3 {
                continue;
            }
            let Some(&u0) = self.skill_to_palette.get(&legend.utilities[0]) else {
                continue;
            };
            let Some(&u1) = self.skill_to_palette.get(&legend.utilities[1]) else {
                continue;
            };
            let Some(&u2) = self.skill_to_palette.get(&legend.utilities[2]) else {
                continue;
            };
            return Some((heal_p, [u0, u1, u2], elite_p));
        }
        None
    }

    /// Get all skills for a profession.
    pub fn profession_skills(&self, profession: &str) -> Vec<&Skill> {
        self.skills_by_profession
            .get(profession)
            .map(|ids| ids.iter().filter_map(|id| self.skills.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get all traits in a specialization.
    pub fn spec_traits(&self, spec_id: u32) -> Vec<&GW2Trait> {
        self.traits_by_spec
            .get(&spec_id)
            .map(|ids| ids.iter().filter_map(|id| self.traits.get(id)).collect())
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

    /// Get trait IDs that apply a specific condition.
    pub fn traits_applying_condition(&self, condition: &str) -> Vec<&GW2Trait> {
        self.traits_by_condition
            .get(condition_index_key(condition))
            .map(|ids| ids.iter().filter_map(|id| self.traits.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get skill IDs that apply a specific condition.
    pub fn skills_applying_condition(&self, condition: &str) -> Vec<&Skill> {
        self.skills_by_condition
            .get(condition_index_key(condition))
            .map(|ids| ids.iter().filter_map(|id| self.skills.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get trait IDs that grant a specific boon.
    pub fn traits_granting_buff(&self, buff: &str) -> Vec<&GW2Trait> {
        self.traits_by_buff
            .get(buff)
            .map(|ids| ids.iter().filter_map(|id| self.traits.get(id)).collect())
            .unwrap_or_default()
    }

    /// Get skill IDs that grant a specific boon.
    pub fn skills_granting_buff(&self, buff: &str) -> Vec<&Skill> {
        self.skills_by_buff
            .get(buff)
            .map(|ids| ids.iter().filter_map(|id| self.skills.get(id)).collect())
            .unwrap_or_default()
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

    /// Empty indexed db for unit tests (optimizer + addon).
    pub fn empty_for_tests() -> Self {
        use std::collections::HashMap;
        Self {
            items: HashMap::new(),
            itemstats: HashMap::new(),
            skills: HashMap::new(),
            traits: HashMap::new(),
            specializations: HashMap::new(),
            professions: HashMap::new(),
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
            pets: HashMap::new(),
            skills_by_profession: HashMap::new(),
            traits_by_spec: HashMap::new(),
            items_by_type: HashMap::new(),
            runes: vec![],
            sigils: vec![],
            relics: vec![],
            skill_to_palette: HashMap::new(),
            palette_to_skill: HashMap::new(),
            traits_by_condition: HashMap::new(),
            skills_by_condition: HashMap::new(),
            traits_by_buff: HashMap::new(),
            skills_by_buff: HashMap::new(),
            localized: None,
        }
    }
}

use crate::data::boon_condition_formulas::is_condition;

fn condition_index_key(status: &str) -> &str {
    crate::data::boon_condition_formulas::canonical_condition_name(status)
}

/// GW2 boons.
fn is_boon(status: &str) -> bool {
    matches!(
        status,
        "Might"
            | "Fury"
            | "Quickness"
            | "Alacrity"
            | "Protection"
            | "Resolution"
            | "Regeneration"
            | "Vigor"
            | "Stability"
            | "Swiftness"
            | "Resistance"
            | "Aegis"
    )
}

/// Test-only thin wrapper preserved so the alias-routing regression suite in
/// `data::boon_condition_formulas::tests` keeps exercising the shared
/// `is_condition` helper through the path the gamedb module consumes it by.
#[cfg(test)]
pub(crate) mod tests_alias_helpers {
    pub(crate) fn is_condition(status: &str) -> bool {
        crate::data::boon_condition_formulas::is_condition(status)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gw2_api::models::Legend;

    fn legend(
        id: &str,
        code: u32,
        heal: u32,
        elite: u32,
        utilities: [u32; 3],
        swap: u32,
    ) -> Legend {
        Legend {
            id: id.into(),
            code: Some(code),
            swap,
            heal,
            elite,
            utilities: utilities.to_vec(),
        }
    }

    #[test]
    fn itemstat_by_name_ignores_apostrophe() {
        let mut db = GameDb::empty_for_tests();
        db.itemstats.insert(
            1,
            gw2_api::models::ItemStat {
                id: 1,
                name: "Knight's".into(),
                attributes: vec![],
            },
        );
        assert_eq!(db.itemstat_by_name("Knights").unwrap().name, "Knight's");
        assert_eq!(db.itemstat_by_name("Knight's").unwrap().name, "Knight's");
    }

    #[test]
    fn itemstat_short_needle_does_not_fuzzy() {
        let mut db = GameDb::empty_for_tests();
        db.itemstats.insert(
            1,
            gw2_api::models::ItemStat {
                id: 1,
                name: "Berserker's".into(),
                attributes: vec![],
            },
        );
        assert!(db.itemstat_by_name("a").is_none());
        assert!(db.itemstat_by_name("sig").is_none());
        assert_eq!(db.itemstat_by_name("Berserker's").map(|s| s.id), Some(1));
    }

    #[test]
    fn load_rejects_empty_skills_traits_or_items() {
        let dir = std::env::temp_dir().join(format!(
            "gw2bo-hollow-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let cache = gw2_api::cache::DataCache::new(&dir);
        let prof = gw2_api::models::Profession {
            id: "Guardian".into(),
            name: "Guardian".into(),
            code: Some(1),
            specializations: vec![],
            weapons: std::collections::HashMap::new(),
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        };
        let spec = gw2_api::models::Specialization {
            id: 1,
            name: "Zeal".into(),
            profession: "Guardian".into(),
            elite: false,
            minor_traits: vec![],
            major_traits: vec![],
            weapon_trait: None,
            icon: None,
            background: None,
            profession_icon: None,
            profession_icon_big: None,
        };
        let stat = gw2_api::models::ItemStat {
            id: 161,
            name: "Berserker's".into(),
            attributes: vec![],
        };
        cache
            .save("professions", &vec![prof], 1)
            .expect("save professions");
        cache
            .save("specializations", &vec![spec], 1)
            .expect("save specs");
        cache
            .save("itemstats", &vec![stat], 1)
            .expect("save itemstats");
        let err = match GameDb::load(&cache) {
            Ok(_) => panic!("hollow skills must fail"),
            Err(e) => e,
        };
        assert!(
            err.contains("skills"),
            "expected skills fail-closed, got {err}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Live `/v2/itemstats` ships several Giver's multiplier shapes under one
    /// English name. Lowest-id exact match is Toughness-only 627; wiki L80
    /// Giver's (Attribute combinations, retrieved 2026-08-29) is Toughness /
    /// Healing Power / Concentration (API: Healing, BoonDuration) at
    /// 628/1070/1430. Math must resolve the three-stat template.
    #[test]
    fn itemstat_by_name_givers_prefers_three_stat_not_lowest_id() {
        let mut db = GameDb::empty_for_tests();
        let attr = |attribute: &str, multiplier: f64, value: i32| {
            gw2_api::models::itemstats::StatAttribute {
                attribute: attribute.into(),
                multiplier,
                value,
            }
        };
        let row = |id: u32, attributes: Vec<gw2_api::models::itemstats::StatAttribute>| {
            gw2_api::models::ItemStat {
                id,
                name: "Giver's".into(),
                attributes,
            }
        };
        db.itemstats
            .insert(627, row(627, vec![attr("Toughness", 0.35, 0)]));
        db.itemstats.insert(
            628,
            row(
                628,
                vec![
                    attr("Toughness", 0.35, 0),
                    attr("Healing", 0.25, 0),
                    attr("BoonDuration", 0.25, 0),
                ],
            ),
        );
        db.itemstats.insert(
            629,
            row(
                629,
                vec![attr("Toughness", 0.35, 0), attr("Healing", 0.25, 0)],
            ),
        );
        db.itemstats.insert(
            1070,
            row(
                1070,
                vec![
                    attr("Toughness", 0.35, 0),
                    attr("Healing", 0.25, 0),
                    attr("BoonDuration", 0.25, 0),
                ],
            ),
        );
        db.itemstats.insert(
            1430,
            row(
                1430,
                vec![
                    attr("Toughness", 0.35, 32),
                    attr("Healing", 0.25, 18),
                    attr("BoonDuration", 0.25, 18),
                ],
            ),
        );

        let got = db
            .itemstat_by_name("Giver's")
            .expect("Giver's must resolve");
        assert_ne!(
            got.id, 627,
            "Giver's must not resolve to Toughness-only 627"
        );
        assert_eq!(got.id, 628, "lowest wiki three-stat Giver's id");
        let mut attrs: Vec<&str> = got
            .attributes
            .iter()
            .filter(|a| a.multiplier > 0.0)
            .map(|a| a.attribute.as_str())
            .collect();
        attrs.sort_unstable();
        assert_eq!(
            attrs.as_slice(),
            ["BoonDuration", "Healing", "Toughness"].as_slice()
        );
        assert_eq!(db.itemstat_by_name("Givers").map(|s| s.id), Some(628));
    }

    #[test]
    fn revenant_older_legend_skill_uses_shared_palette() {
        let mut db = GameDb::empty_for_tests();
        db.legends.insert(
            "Legend1".into(),
            legend("Legend1", 1, 27220, 27760, [28379, 27014, 26644], 28085),
        );
        db.legends.insert(
            "Legend8".into(),
            legend("Legend8", 8, 77043, 76968, [77243, 77291, 76805], 76610),
        );
        db.skill_to_palette.insert(77043, 4572);
        db.skill_to_palette.insert(76968, 4554);
        db.skill_to_palette.insert(77243, 4614);
        db.skill_to_palette.insert(77291, 4651);
        db.skill_to_palette.insert(76805, 4564);

        assert_eq!(db.skill_palette_id(77043), 4572);
        assert_eq!(
            db.skill_palette_id(27220),
            4572,
            "Shiro/Dragon heal shares Conduit heal palette"
        );
        assert_eq!(db.skill_palette_id(27760), 4554);
        assert_eq!(db.skill_palette_id(28379), 4614);
        assert_eq!(db.skill_palette_id(26644), 4564);
        assert_eq!(db.skill_palette_id(99999), 0);
    }

    #[test]
    fn legend_template_code_prefers_api_code() {
        let mut db = GameDb::empty_for_tests();
        db.legends
            .insert("Legend5".into(), legend("Legend5", 5, 1, 2, [3, 4, 5], 6));
        assert_eq!(db.legend_template_code("Legend5"), 5);
        assert_eq!(db.legend_template_code("Legend9"), 9);
    }

    #[test]
    fn legend_available_respects_swap_specialization() {
        let mut db = GameDb::empty_for_tests();
        db.legends.insert(
            "Legend1".into(),
            legend("Legend1", 1, 1, 2, [3, 4, 5], 28085),
        );
        db.skills.insert(
            28085,
            Skill {
                id: 28085,
                name: "Legendary Dragon Stance".into(),
                description: None,
                icon: None,
                chat_link: None,
                skill_type: Some("Profession".into()),
                weapon_type: None,
                professions: vec!["Revenant".into()],
                slot: Some("Profession_1".into()),
                facts: vec![],
                traited_facts: vec![],
                categories: vec![],
                attunement: None,
                cost: None,
                dual_wield: None,
                flip_skill: None,
                initiative: None,
                next_chain: None,
                prev_chain: None,
                transform_skills: vec![],
                bundle_skills: vec![],
                toolbelt_skill: None,
                flags: vec![],
                specialization: Some(52),
            },
        );
        assert!(!db.legend_available("Legend1", &[3, 9]));
        assert!(db.legend_available("Legend1", &[3, 9, 52]));
    }

    #[test]
    fn loc_skill_falls_back_and_uses_overlay() {
        let mut db = GameDb::empty_for_tests();
        let skill: Skill = serde_json::from_value(serde_json::json!({
            "id": 1,
            "name": "Signet of Malice"
        }))
        .unwrap();
        db.skills.insert(1, skill);
        assert_eq!(db.loc_skill(1, "Signet of Malice"), "Signet of Malice");
        assert_eq!(db.loc_name("Signet of Malice"), "Signet of Malice");
        let mut names = gw2_api::localize::LocalizedNames {
            lang: "fr".into(),
            ..Default::default()
        };
        names.skills.insert(1, "Sceau de malice".into());
        db.attach_localized(names);
        assert_eq!(db.loc_skill(1, "Signet of Malice"), "Sceau de malice");
        assert_eq!(db.loc_name("Signet of Malice"), "Sceau de malice");
    }

    #[test]
    fn pet_display_name_uses_catalog_or_hash_id() {
        let mut db = GameDb::empty_for_tests();
        assert_eq!(db.pet_display_name(66), "#66");
        db.pets.insert(
            66,
            Pet {
                id: 66,
                name: "Juvenile Smokescale".into(),
                description: None,
                icon: None,
                skills: vec![],
            },
        );
        assert_eq!(db.pet_display_name(66), "Juvenile Smokescale");
        assert_eq!(db.pet_by_name("Smokescale").map(|p| p.id), Some(66));
        assert_eq!(db.pet_by_name("#66").map(|p| p.id), Some(66));
    }

    #[test]
    fn immobilized_aliases_hit_immobile_index() {
        let mut db = GameDb::empty_for_tests();
        let skill: Skill = serde_json::from_value(serde_json::json!({
            "id": 7,
            "name": "Test Immobilize"
        }))
        .unwrap();
        db.skills.insert(7, skill);
        db.skills_by_condition.insert("Immobile".into(), vec![7]);
        for name in ["Immobilized", "Immobilize", "Immobile"] {
            let hits = db.skills_applying_condition(name);
            assert_eq!(hits.len(), 1, "{name}");
            assert_eq!(hits[0].id, 7, "{name}");
        }
    }
}
