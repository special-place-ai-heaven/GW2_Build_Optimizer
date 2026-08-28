//! The land weapon budget: which slot budgets the active weapon set spends.
//!
//! A build carries two land weapon sets, but only one is worn at a time, so
//! only one set's stats are on the character. Summing both sets is worth
//! 376 stat points instead of 251 and makes a two-set build strictly beat a
//! one-set build for free. Every caller that turns gear into stats therefore
//! asks the same question about the *active* set only, and this module is the
//! one place that answers it.
//!
//! The other half of the model is what "two-handed" means. It is a property of
//! the weapon *type*: a Greatsword occupies both hands, a Sword does not.
//! Reading it off an empty off-hand slot instead — `(Some(main), None) =>
//! two-handed` — inflates every single-weapon draft to a two-hander's 251
//! points, and quietly misprices any mid-edit build.

use gw2_api::models::Profession;

use crate::data::slot_budgets::SlotType;

/// Every two-handed weapon type in the game, in [`gw2_core::i18n::weapon_type_key`]
/// form (ASCII-alphanumeric lowercase, with the item-API spellings folded in:
/// `Harpoon` -> `spear`, `HarpoonGun` -> `speargun`).
///
/// This is an enumeration of the whole weapon universe, not a heuristic — GW2
/// has exactly these hand counts. It is the fallback for when no profession is
/// on hand; [`is_two_handed`] prefers the profession's own `TwoHand` flag so a
/// weapon type added by a future patch is right before this list is edited.
///
/// Spear is deliberately here and deliberately land-legal: it keeps the API's
/// `Aquatic` flag (that flag marks the underwater skill palette) but has been a
/// terrestrial two-hander since Janthir Wilds. Speargun and Trident are
/// two-handed too; they simply never belong on a land set, which is the
/// caller's business, not the budget's.
const TWO_HANDED_WEAPON_TYPES: &[&str] = &[
    "greatsword",
    "hammer",
    "longbow",
    "rifle",
    "shortbow",
    "spear",
    "speargun",
    "staff",
    "trident",
];

/// What the active land weapon set costs, in budget slots.
///
/// Deliberately not a stat total: the point value depends on the prefix's stat
/// shape (a ThreeStat two-hander is 251 major, a CelestialLike one is 118), so
/// this names the slots and the caller looks the shape up in
/// [`crate::data::slot_budgets`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LandWeaponBudget {
    /// Nothing equipped on the active set — zero weapon points, not a default
    /// weapon's worth.
    Empty,
    /// One two-handed weapon filling both hands: a single `WeaponTwoHand`
    /// budget.
    TwoHand,
    /// A single one-handed weapon, in either hand: one `WeaponOneHand` budget.
    OneHand,
    /// Main-hand plus off-hand: two `WeaponOneHand` budgets.
    OneHandPair,
}

impl LandWeaponBudget {
    /// The budget slots this set spends, in main-hand-then-off-hand order.
    pub fn slots(self) -> &'static [SlotType] {
        match self {
            LandWeaponBudget::Empty => &[],
            LandWeaponBudget::TwoHand => &[SlotType::WeaponTwoHand],
            LandWeaponBudget::OneHand => &[SlotType::WeaponOneHand],
            LandWeaponBudget::OneHandPair => &[SlotType::WeaponOneHand, SlotType::WeaponOneHand],
        }
    }

    /// Number of budget slots spent (0, 1, or 2).
    pub fn slot_count(self) -> usize {
        self.slots().len()
    }

    /// True when the set is a single weapon held in both hands.
    pub fn is_two_handed(self) -> bool {
        matches!(self, LandWeaponBudget::TwoHand)
    }
}

/// Is this weapon type held in both hands?
///
/// Answered from the profession's own weapon table when one is supplied — that
/// is patch-aware and survives a new weapon type — and from the module's
/// two-handed type list otherwise, or when the profession simply does
/// not train that weapon (whether a Rifle needs both hands does not depend on
/// who is holding it).
///
/// Accepts any API spelling: `Short Bow`, `ShortBow`, and `Shortbow` are one
/// weapon, as are `Harpoon` and `Spear`.
///
/// A weapon type nobody recognises counts as one-handed. That under-counts by
/// 126 points in the worst case, which is the safe direction: this module
/// exists to stop free stat points, not to invent them.
pub fn is_two_handed(weapon_type: &str, profession: Option<&Profession>) -> bool {
    let profession_key = gw2_core::i18n::canonical_weapon_type(weapon_type);
    if let Some(info) = profession.and_then(|prof| prof.weapons.get(&profession_key)) {
        return info
            .flags
            .iter()
            .any(|flag| flag.eq_ignore_ascii_case("TwoHand"));
    }
    let key = gw2_core::i18n::weapon_type_key(weapon_type);
    TWO_HANDED_WEAPON_TYPES.contains(&key.as_str())
}

/// The budget spent by **one** land weapon set — the active one.
///
/// Takes a single set on purpose: there is no argument shape that lets a caller
/// accidentally bill both sets. Pass weapon *type* names (`"Greatsword"`,
/// `"Sword"`); `None` or blank means the slot is empty, and an empty slot is
/// worth nothing.
///
/// A two-handed main hand wins outright and the off-hand field is ignored: a
/// Greatsword occupies both hands, so a weapon recorded beside it is stale data,
/// not a second budget. A two-handed weapon found in the *off-hand* field is
/// likewise impossible, and is billed as the one one-hand slot it sits in
/// rather than being rewarded with a two-hander's points.
///
/// "Land" here names *which* set (set 1, not the aquatic set, not set 2); it is
/// not a filter on weapon types. Keeping an underwater weapon off a land set is
/// the weapon-selection code's job, and doing it here would silently zero a
/// real weapon's stats instead of reporting the bad data.
pub fn land_weapon_budget(
    main_hand: Option<&str>,
    off_hand: Option<&str>,
    profession: Option<&Profession>,
) -> LandWeaponBudget {
    let main = equipped(main_hand);
    let off = equipped(off_hand);

    if main.is_some_and(|weapon| is_two_handed(weapon, profession)) {
        return LandWeaponBudget::TwoHand;
    }

    match (main, off) {
        (Some(_), Some(_)) => LandWeaponBudget::OneHandPair,
        (Some(_), None) | (None, Some(_)) => LandWeaponBudget::OneHand,
        (None, None) => LandWeaponBudget::Empty,
    }
}

/// A slot holds a weapon only if it names one — a blank string is an empty
/// slot, not a weapon called "".
fn equipped(slot: Option<&str>) -> Option<&str> {
    slot.map(str::trim).filter(|weapon| !weapon.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::slot_budgets::{slot_budgets, StatShape};
    use gw2_api::models::professions::WeaponInfo;
    use std::collections::HashMap;

    fn profession_with(weapons: &[(&str, &[&str])]) -> Profession {
        Profession {
            id: "Test".into(),
            name: "Test".into(),
            code: None,
            specializations: vec![],
            weapons: weapons
                .iter()
                .map(|(name, flags)| {
                    (
                        (*name).to_string(),
                        WeaponInfo {
                            specialization: None,
                            flags: flags.iter().map(|f| (*f).to_string()).collect(),
                            skills: vec![],
                        },
                    )
                })
                .collect::<HashMap<_, _>>(),
            training: vec![],
            skills_by_palette: vec![],
            icon: None,
            icon_big: None,
        }
    }

    #[test]
    fn an_empty_off_hand_does_not_make_a_one_hander_two_handed() {
        // The whole point. A lone Sword is 125 points, not a Greatsword's 251.
        assert_eq!(
            land_weapon_budget(Some("Sword"), None, None),
            LandWeaponBudget::OneHand
        );
        assert_eq!(
            land_weapon_budget(Some("Scepter"), None, None),
            LandWeaponBudget::OneHand
        );
    }

    #[test]
    fn two_handed_comes_from_the_weapon_type() {
        for weapon in ["Greatsword", "Hammer", "Longbow", "Rifle", "Staff"] {
            assert_eq!(
                land_weapon_budget(Some(weapon), None, None),
                LandWeaponBudget::TwoHand,
                "{weapon} should be two-handed"
            );
        }
        for weapon in ["Sword", "Axe", "Dagger", "Mace", "Pistol", "Scepter"] {
            assert_eq!(
                land_weapon_budget(Some(weapon), None, None),
                LandWeaponBudget::OneHand,
                "{weapon} should be one-handed"
            );
        }
    }

    #[test]
    fn land_spear_is_a_two_hander_despite_the_aquatic_flag() {
        assert!(is_two_handed("Spear", None));
        assert!(is_two_handed("Harpoon", None));
        assert_eq!(
            land_weapon_budget(Some("Spear"), None, None),
            LandWeaponBudget::TwoHand
        );
    }

    #[test]
    fn main_plus_off_hand_spends_two_one_hand_slots() {
        assert_eq!(
            land_weapon_budget(Some("Sword"), Some("Focus"), None),
            LandWeaponBudget::OneHandPair
        );
        assert_eq!(
            land_weapon_budget(Some("Axe"), Some("Torch"), None),
            LandWeaponBudget::OneHandPair
        );
    }

    #[test]
    fn a_two_hander_ignores_a_stale_off_hand_entry() {
        assert_eq!(
            land_weapon_budget(Some("Greatsword"), Some("Focus"), None),
            LandWeaponBudget::TwoHand
        );
    }

    #[test]
    fn a_two_hander_in_the_off_hand_field_is_billed_as_one_slot() {
        // Impossible gear, so it must not pay a two-hander's points.
        assert_eq!(
            land_weapon_budget(None, Some("Greatsword"), None),
            LandWeaponBudget::OneHand
        );
    }

    #[test]
    fn an_off_hand_only_set_still_spends_one_slot() {
        assert_eq!(
            land_weapon_budget(None, Some("Warhorn"), None),
            LandWeaponBudget::OneHand
        );
    }

    #[test]
    fn an_empty_or_blank_set_spends_nothing() {
        assert_eq!(
            land_weapon_budget(None, None, None),
            LandWeaponBudget::Empty
        );
        assert_eq!(
            land_weapon_budget(Some(""), Some("   "), None),
            LandWeaponBudget::Empty
        );
    }

    #[test]
    fn api_spellings_resolve_to_the_same_weapon() {
        for spelling in ["Shortbow", "ShortBow", "Short Bow"] {
            assert!(is_two_handed(spelling, None), "{spelling}");
        }
        assert!(is_two_handed("HarpoonGun", None));
        assert!(is_two_handed("Speargun", None));
    }

    #[test]
    fn the_profession_table_outranks_the_static_list() {
        // A patch that made Sword two-handed would land in the profession
        // weapons table first; the budget must follow the API, not the list.
        let prof = profession_with(&[("Sword", &["TwoHand"]), ("Greatsword", &["Mainhand"])]);

        assert!(is_two_handed("Sword", Some(&prof)));
        assert_eq!(
            land_weapon_budget(Some("Sword"), Some("Focus"), Some(&prof)),
            LandWeaponBudget::TwoHand
        );
        assert!(!is_two_handed("Greatsword", Some(&prof)));
    }

    #[test]
    fn a_weapon_the_profession_cannot_train_falls_back_to_the_type() {
        // Guardians have no Rifle entry; a Rifle is still two-handed.
        let prof = profession_with(&[("Sword", &["Mainhand"])]);
        assert!(is_two_handed("Rifle", Some(&prof)));
        assert!(!is_two_handed("Focus", Some(&prof)));
    }

    #[test]
    fn an_unrecognised_weapon_type_is_billed_as_one_handed() {
        // Safe direction: a type nobody knows must not mint a two-hander's points.
        assert!(!is_two_handed("Scythe", None));
        assert_eq!(
            land_weapon_budget(Some("Scythe"), None, None),
            LandWeaponBudget::OneHand
        );
    }

    #[test]
    fn budget_slots_map_to_the_real_slot_budget_values() {
        let budgets = slot_budgets();
        let major = |budget: LandWeaponBudget| -> i32 {
            budget
                .slots()
                .iter()
                .map(|slot| {
                    budgets
                        .get(*slot, StatShape::ThreeStat)
                        .expect("weapon slot budgets are loaded")
                        .major
                })
                .sum()
        };

        assert_eq!(major(LandWeaponBudget::TwoHand), 251);
        assert_eq!(major(LandWeaponBudget::OneHandPair), 250);
        assert_eq!(major(LandWeaponBudget::OneHand), 125);
        assert_eq!(major(LandWeaponBudget::Empty), 0);
    }

    #[test]
    fn slot_count_matches_the_slots_listed() {
        assert_eq!(LandWeaponBudget::Empty.slot_count(), 0);
        assert_eq!(LandWeaponBudget::TwoHand.slot_count(), 1);
        assert_eq!(LandWeaponBudget::OneHand.slot_count(), 1);
        assert_eq!(LandWeaponBudget::OneHandPair.slot_count(), 2);
        assert!(LandWeaponBudget::TwoHand.is_two_handed());
        assert!(!LandWeaponBudget::OneHandPair.is_two_handed());
    }
}
