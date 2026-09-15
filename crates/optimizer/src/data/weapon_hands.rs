//! Per-hand land-weapon gates from the wiki usability table, not the API dump.
//!
//! Elite names are that spec **or** Weaponmaster Training. `expanded_soto` /
//! `spear_jw` are account unlocks (SotO Expanded Weapon Proficiency, JW
//! Lowland Spear Training). Aquatic-only types are omitted.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

use gw2_core::i18n::{canonical_weapon_type, weapon_type_key};

const WEAPON_HANDS_JSON: &str = include_str!("../../../../data/weapon_hands.json");

/// Land weapons Choya may be told about, sorted, wiki/API names normalized.
const LAND_WEAPONS: &[&str] = &[
    "Axe",
    "Dagger",
    "Focus",
    "Greatsword",
    "Hammer",
    "Longbow",
    "Mace",
    "Pistol",
    "Rifle",
    "Scepter",
    "Shield",
    "Shortbow",
    "Spear",
    "Staff",
    "Sword",
    "Torch",
    "Warhorn",
];

static TABLE: OnceLock<Table> = OnceLock::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hand {
    Main,
    Off,
    TwoHand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponAccess {
    None,
    Core,
    Elite(&'static str),
    ExpandedSoto,
    SpearJw,
}

impl WeaponAccess {
    /// Wire token: `"core"` | elite name | `"expanded_soto"` | `"spear_jw"`.
    pub fn json_token(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Core => Some("core"),
            Self::Elite(name) => Some(name),
            Self::ExpandedSoto => Some("expanded_soto"),
            Self::SpearJw => Some("spear_jw"),
        }
    }
}

/// Gate for one profession / weapon / hand. Unknown names are `None`.
pub fn access(profession: &str, weapon: &str, hand: Hand) -> WeaponAccess {
    let Some(prof) = table().get(&prof_key(profession)) else {
        return WeaponAccess::None;
    };
    let Some(row) = prof.get(&weapon_type_key(weapon)) else {
        return WeaponAccess::None;
    };
    match hand {
        Hand::Main => row.main,
        Hand::Off => row.off,
        Hand::TwoHand => row.two_hand,
    }
}

/// Parenthetical Choya sees after a hand name. `None` if the hand is illegal.
/// Core is `Some("")` — no extra gate text.
pub fn choya_label(access: WeaponAccess) -> Option<String> {
    match access {
        WeaponAccess::None => None,
        WeaponAccess::Core => Some(String::new()),
        WeaponAccess::Elite(spec) => Some(format!(" (requires {spec} or Weaponmaster Training)")),
        WeaponAccess::ExpandedSoto => Some(" (requires SotO Expanded Weapon Proficiency)".into()),
        WeaponAccess::SpearJw => Some(" (requires Janthir Wilds Lowland Spear Training)".into()),
    }
}

/// Sorted land weapons with at least one legal hand for `profession`.
pub fn land_weapons(profession: &str) -> Vec<&'static str> {
    LAND_WEAPONS
        .iter()
        .copied()
        .filter(|weapon| {
            [Hand::Main, Hand::Off, Hand::TwoHand]
                .into_iter()
                .any(|hand| access(profession, weapon, hand) != WeaponAccess::None)
        })
        .collect()
}

type Table = HashMap<String, HashMap<String, Hands>>;

#[derive(Clone, Copy)]
struct Hands {
    main: WeaponAccess,
    off: WeaponAccess,
    two_hand: WeaponAccess,
}

#[derive(Deserialize, Default)]
struct GateFile {
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    off: Option<String>,
    #[serde(default)]
    two_hand: Option<String>,
}

fn table() -> &'static Table {
    TABLE.get_or_init(|| {
        load_table(WEAPON_HANDS_JSON).expect("embedded weapon_hands.json is invalid")
    })
}

fn load_table(json: &str) -> Result<Table, String> {
    let raw: HashMap<String, HashMap<String, GateFile>> =
        serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    for (prof, weapons) in raw {
        let mut rows = HashMap::new();
        for (weapon, gates) in weapons {
            let display = canonical_weapon_type(&weapon);
            if matches!(weapon_type_key(&display).as_str(), "trident" | "speargun") {
                return Err(format!("{prof} lists aquatic-only {display}"));
            }
            let row = Hands {
                main: parse_gate(gates.main.as_deref())?,
                off: parse_gate(gates.off.as_deref())?,
                two_hand: parse_gate(gates.two_hand.as_deref())?,
            };
            if row.main == WeaponAccess::None
                && row.off == WeaponAccess::None
                && row.two_hand == WeaponAccess::None
            {
                continue;
            }
            rows.insert(weapon_type_key(&display), row);
        }
        out.insert(prof_key(&prof), rows);
    }
    Ok(out)
}

fn parse_gate(raw: Option<&str>) -> Result<WeaponAccess, String> {
    match raw {
        None => Ok(WeaponAccess::None),
        Some("core") => Ok(WeaponAccess::Core),
        Some("expanded_soto") => Ok(WeaponAccess::ExpandedSoto),
        Some("spear_jw") => Ok(WeaponAccess::SpearJw),
        Some(name) => intern_elite(name)
            .map(WeaponAccess::Elite)
            .ok_or_else(|| format!("unknown weapon gate {name}")),
    }
}

fn intern_elite(name: &str) -> Option<&'static str> {
    const SPECS: &[&str] = &[
        "Berserker",
        "Bladesworn",
        "Catalyst",
        "Chronomancer",
        "Daredevil",
        "Deadeye",
        "Dragonhunter",
        "Druid",
        "Firebrand",
        "Harbinger",
        "Herald",
        "Holosmith",
        "Mechanist",
        "Mirage",
        "Reaper",
        "Renegade",
        "Scourge",
        "Scrapper",
        "Soulbeast",
        "Specter",
        "Spellbreaker",
        "Tempest",
        "Untamed",
        "Vindicator",
        "Virtuoso",
        "Weaver",
        "Willbender",
    ];
    SPECS
        .iter()
        .copied()
        .find(|spec| spec.eq_ignore_ascii_case(name))
}

fn prof_key(name: &str) -> String {
    name.trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardian_sword_is_per_hand() {
        assert_eq!(access("Guardian", "Sword", Hand::Main), WeaponAccess::Core);
        assert_eq!(
            access("Guardian", "Sword", Hand::Off),
            WeaponAccess::Elite("Willbender")
        );
        assert_eq!(
            access("Guardian", "Sword", Hand::TwoHand),
            WeaponAccess::None
        );
    }

    #[test]
    fn ranger_dagger_soulbeast_main_only() {
        assert_eq!(
            access("Ranger", "Dagger", Hand::Main),
            WeaponAccess::Elite("Soulbeast")
        );
        assert_eq!(access("Ranger", "Dagger", Hand::Off), WeaponAccess::Core);
    }

    #[test]
    fn revenant_sword_both_core() {
        assert_eq!(access("Revenant", "Sword", Hand::Main), WeaponAccess::Core);
        assert_eq!(access("Revenant", "Sword", Hand::Off), WeaponAccess::Core);
    }

    #[test]
    fn aquatic_only_is_none() {
        assert_eq!(
            access("Guardian", "Trident", Hand::TwoHand),
            WeaponAccess::None
        );
        assert_eq!(
            access("Warrior", "Speargun", Hand::TwoHand),
            WeaponAccess::None
        );
    }

    #[test]
    fn spear_is_jw_unlock() {
        assert_eq!(
            access("Guardian", "Harpoon", Hand::TwoHand),
            WeaponAccess::SpearJw
        );
    }
}
