//! Every condition-cleanse source in the game, by profession, specialization
//! and gear: `data/cleanse_sources.json`.
//!
//! The viability gate used to learn what cleanses from a text heuristic over
//! skill facts ("condit" + remove/cleanse/cure). Measured 2026-09-05 on the
//! live database: a WvW Reaper carrying "Suffer!" (Conditions Transferred),
//! with Consume Conditions, Plague Signet (Conditions Sent), Well of Power
//! (Conditions Converted to Boons) and Spectral Walk (consuming conditions)
//! all available, was judged to have NO cleanse at all, and the search served
//! a non-viable build. Each profession removes conditions through its own
//! verbs, so the answer is a table, not a smarter regex: one entry per game
//! id, catalogued from the API facts and cross-checked against the wiki, one
//! list per profession the search can pick from.
//!
//! The table is authoritative for every id it knows (including "known and it
//! only cleanses allies"). The text heuristic in `text_util` stays as the
//! safety net for ids the table does not know yet (a patch adds a skill).

use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;
use thiserror::Error;

use super::{try_load, DataLoadError};

/// Canonical JSON embedded at compile time from data/cleanse_sources.json.
const CLEANSE_SOURCES_JSON: &str = include_str!("../../../../data/cleanse_sources.json");

static REGISTRY: OnceLock<CleanseRegistry> = OnceLock::new();

/// The registry, parsed on first access.
///
/// # Panics
/// Panics if the embedded JSON is malformed (compile-time data; `cargo test`
/// catches it before a DLL is built).
pub fn registry() -> &'static CleanseRegistry {
    REGISTRY.get_or_init(|| {
        load_cleanse_sources(CLEANSE_SOURCES_JSON)
            .expect("embedded cleanse_sources.json is invalid")
    })
}

/// Health-check loader: parses and validates without touching the `OnceLock`.
pub fn try_load_cleanse_sources() -> Result<(), Vec<DataLoadError>> {
    try_load!(
        "cleanse_sources",
        load_cleanse_sources(CLEANSE_SOURCES_JSON).map(|_| ()),
        CleanseSourceError
    )
}

/// The text safety net the registry replaces, exposed so the audit example
/// (`examples/cleanse_registry_check.rs`) can list what the heuristic flags
/// that the table does not carry.
pub fn text_suggests_cleanse(text: &str) -> bool {
    crate::text_util::text_describes_condition_cleanse(text)
}

#[derive(Debug, Error)]
pub enum CleanseSourceError {
    #[error("JSON parse error: {0}")]
    ParseError(#[from] serde_json::Error),
    #[error("validation error: {0}")]
    ValidationError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
pub enum SourceKind {
    Skill,
    Trait,
    Sigil,
    Rune,
    Relic,
}

/// One game id that removes conditions from the caster and/or allies.
#[derive(Debug, Clone, Deserialize)]
pub struct CleanseSource {
    pub kind: SourceKind,
    pub id: u32,
    pub name: String,
    /// `None` for gear.
    #[serde(default)]
    pub profession: Option<String>,
    /// Specialization (elite or core line) the source belongs to; `None` for
    /// core skills every spec of the profession can slot, and for gear.
    #[serde(default)]
    pub specialization: Option<String>,
    #[serde(default)]
    pub slot: Option<String>,
    /// Remove | Transfer | Convert | Consume | Send | Cure: the game's verb.
    pub mechanism: String,
    /// Conditions removed from the caster per activation.
    pub self_count: u32,
    /// Conditions removed per ally per activation.
    #[serde(default)]
    pub ally_count: u32,
    /// Skill recharge / tooltip cooldown in seconds; `None` for event and
    /// interval triggers whose cadence the tooltip does not state.
    #[serde(default)]
    pub cooldown_s: Option<f64>,
    pub trigger: String,
    /// any | movement | nondamaging | damaging | specific:<names>
    #[serde(default = "default_filter")]
    pub filter: String,
    /// The skill only cleanses while this trait is equipped (Warrior bursts
    /// with Cleansing Ire, Mesmer shatters with Restorative Illusions,
    /// Signet of Midnight with Blurred Inscriptions). `None` = unconditional.
    #[serde(default)]
    pub requires_trait: Option<u32>,
    #[serde(default)]
    pub evidence: String,
    #[serde(default)]
    pub notes: String,
}

fn default_filter() -> String {
    "any".to_string()
}

impl CleanseSource {
    /// Conditions this source takes off the caster per activation, as the
    /// viability gate counts them. A movement-only cleanse (Relic of Febe)
    /// does not answer the burning and poison the gate exists for.
    pub fn gate_count(&self) -> u32 {
        if self.filter == "movement" {
            0
        } else {
            self.self_count
        }
    }

    /// `gate_count` for a build that has `equipped_traits`: zero when the
    /// cleanse needs a trait the build does not run.
    pub fn gate_count_with(&self, equipped_traits: &[u32]) -> u32 {
        match self.requires_trait {
            Some(t) if !equipped_traits.contains(&t) => 0,
            _ => self.gate_count(),
        }
    }

    /// Cleanses per 20 s: count x 20 / cooldown. Without a stated cooldown the
    /// trigger is an event (weapon swap, heal use, shroud enter) whose cadence
    /// is unknown here, so it is credited at ONE activation per 20 s, the same
    /// deliberately conservative convention as the tooltip fallback in
    /// `referee::cleanse_rate_from_text`.
    pub fn rate_per_20s(&self) -> f64 {
        let count = self.gate_count() as f64;
        match self.cooldown_s {
            Some(cd) if cd > 0.0 => count * 20.0 / cd,
            _ => count,
        }
    }
}

/// An id a cataloguer read and judged NOT to cleanse (boon corruption,
/// Resistance, "heals per condition removed", condition-damage text...).
#[derive(Debug, Clone, Deserialize)]
struct KnownNonCleanse {
    kind: SourceKind,
    id: u32,
}

#[derive(Debug, Deserialize)]
struct CleanseSourcesFile {
    schema: u32,
    #[serde(default)]
    game_build: u32,
    sources: Vec<CleanseSource>,
    #[serde(default)]
    rejected: Vec<KnownNonCleanse>,
}

#[derive(Debug)]
pub struct CleanseRegistry {
    sources: Vec<CleanseSource>,
    by_key: HashMap<(SourceKind, u32), usize>,
    rejected: HashSet<(SourceKind, u32)>,
    /// Game build the table was catalogued against.
    pub game_build: u32,
}

impl CleanseRegistry {
    fn get(&self, kind: SourceKind, id: u32) -> Option<&CleanseSource> {
        self.by_key.get(&(kind, id)).map(|&i| &self.sources[i])
    }

    /// The table has read this id, as a source or as a judged non-cleanse.
    /// Callers skip their text heuristic for anything the table knows.
    pub fn knows(&self, kind: SourceKind, id: u32) -> bool {
        self.by_key.contains_key(&(kind, id)) || self.rejected.contains(&(kind, id))
    }

    pub fn knows_skill(&self, id: u32) -> bool {
        self.knows(SourceKind::Skill, id)
    }

    pub fn knows_trait(&self, id: u32) -> bool {
        self.knows(SourceKind::Trait, id)
    }

    pub fn knows_item(&self, id: u32) -> bool {
        [SourceKind::Sigil, SourceKind::Rune, SourceKind::Relic]
            .iter()
            .any(|&k| self.knows(k, id))
    }

    pub fn skill(&self, id: u32) -> Option<&CleanseSource> {
        self.get(SourceKind::Skill, id)
    }

    pub fn trait_(&self, id: u32) -> Option<&CleanseSource> {
        self.get(SourceKind::Trait, id)
    }

    /// Sigil, rune or relic by item id.
    pub fn item(&self, id: u32) -> Option<&CleanseSource> {
        self.get(SourceKind::Sigil, id)
            .or_else(|| self.get(SourceKind::Rune, id))
            .or_else(|| self.get(SourceKind::Relic, id))
    }

    pub fn all(&self) -> &[CleanseSource] {
        &self.sources
    }

    /// Every skill and trait source of one profession (core and every spec).
    pub fn for_profession<'a>(
        &'a self,
        profession: &'a str,
    ) -> impl Iterator<Item = &'a CleanseSource> + 'a {
        self.sources
            .iter()
            .filter(move |s| s.profession.as_deref() == Some(profession))
    }
}

fn load_cleanse_sources(json: &str) -> Result<CleanseRegistry, CleanseSourceError> {
    let file: CleanseSourcesFile = serde_json::from_str(json)?;
    if file.schema != 1 {
        return Err(CleanseSourceError::ValidationError(format!(
            "unsupported schema {}",
            file.schema
        )));
    }
    let mut by_key = HashMap::with_capacity(file.sources.len());
    for (i, s) in file.sources.iter().enumerate() {
        if by_key.insert((s.kind, s.id), i).is_some() {
            return Err(CleanseSourceError::ValidationError(format!(
                "duplicate entry {:?} {} ({})",
                s.kind, s.id, s.name
            )));
        }
        let is_gear = matches!(
            s.kind,
            SourceKind::Sigil | SourceKind::Rune | SourceKind::Relic
        );
        if !is_gear && s.profession.is_none() {
            return Err(CleanseSourceError::ValidationError(format!(
                "{:?} {} ({}) has no profession",
                s.kind, s.id, s.name
            )));
        }
        // 0/0 entries document an amplifier (+N to other cleanses) or an
        // enabler trait whose count lives on the skills it unlocks.
        let documents_only = s.notes.contains("amplifier") || s.notes.contains("enabler");
        if s.self_count == 0 && s.ally_count == 0 && !documents_only {
            return Err(CleanseSourceError::ValidationError(format!(
                "{:?} {} ({}) removes nothing from anyone",
                s.kind, s.id, s.name
            )));
        }
    }
    let rejected: HashSet<(SourceKind, u32)> =
        file.rejected.iter().map(|r| (r.kind, r.id)).collect();
    if let Some(both) = rejected.iter().find(|k| by_key.contains_key(k)) {
        return Err(CleanseSourceError::ValidationError(format!(
            "{:?} {} is both a source and rejected",
            both.0, both.1
        )));
    }
    Ok(CleanseRegistry {
        sources: file.sources,
        by_key,
        rejected,
        game_build: file.game_build,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_registry_parses_and_validates() {
        assert!(try_load_cleanse_sources().is_ok());
        let _ = registry();
    }

    #[test]
    fn duplicate_ids_are_rejected() {
        let json = r#"{"schema":1,"sources":[
          {"kind":"Skill","id":1,"name":"A","profession":"Warrior","mechanism":"Remove","self_count":1,"trigger":"Active"},
          {"kind":"Skill","id":1,"name":"A","profession":"Warrior","mechanism":"Remove","self_count":1,"trigger":"Active"}]}"#;
        assert!(matches!(
            load_cleanse_sources(json),
            Err(CleanseSourceError::ValidationError(_))
        ));
    }

    #[test]
    fn gate_count_and_rate_follow_the_gate_conventions() {
        let json = r#"{"schema":1,"sources":[
          {"kind":"Skill","id":30670,"name":"\"Suffer!\"","profession":"Necromancer","specialization":"Reaper","slot":"Utility","mechanism":"Transfer","self_count":2,"cooldown_s":16,"trigger":"Active"},
          {"kind":"Relic","id":101116,"name":"Relic of Febe","mechanism":"Remove","self_count":0,"ally_count":1,"trigger":"OnHeal","filter":"movement"},
          {"kind":"Sigil","id":67340,"name":"Superior Sigil of Cleansing","mechanism":"Remove","self_count":3,"cooldown_s":9,"trigger":"OnWeaponSwap"},
          {"kind":"Trait","id":1922,"name":"Shrouded Removal","profession":"Necromancer","specialization":"Death Magic","slot":"Major","mechanism":"Remove","self_count":1,"trigger":"OnShroudEnter"}]}"#;
        let reg = load_cleanse_sources(json).unwrap();
        let suffer = reg.skill(30670).unwrap();
        assert_eq!(suffer.gate_count(), 2);
        assert_eq!(suffer.gate_count_with(&[]), 2, "unconditional");
        assert!((suffer.rate_per_20s() - 2.5).abs() < 1e-9);
        assert_eq!(reg.item(101116).unwrap().gate_count(), 0, "movement-only");
        assert!((reg.item(67340).unwrap().rate_per_20s() - 3.0 * 20.0 / 9.0).abs() < 1e-9);
        assert!(
            (reg.trait_(1922).unwrap().rate_per_20s() - 1.0).abs() < 1e-9,
            "event trigger: one per 20 s"
        );
        assert_eq!(reg.for_profession("Necromancer").count(), 2);
        assert!(reg.skill(1922).is_none(), "kinds do not collide");
    }

    /// Eviscerate cleanses only with Cleansing Ire equipped: a Warrior bar
    /// full of bursts must not read as covered without the trait.
    #[test]
    fn traited_cleanse_needs_its_trait() {
        let json = r#"{"schema":1,"sources":[
          {"kind":"Skill","id":14422,"name":"Eviscerate","profession":"Warrior","slot":"Profession_1","mechanism":"Remove","self_count":1,"cooldown_s":10,"trigger":"Active","requires_trait":1649}]}"#;
        let reg = load_cleanse_sources(json).unwrap();
        let ev = reg.skill(14422).unwrap();
        assert_eq!(ev.gate_count_with(&[]), 0);
        assert_eq!(ev.gate_count_with(&[1379, 1649]), 1);
    }
}
