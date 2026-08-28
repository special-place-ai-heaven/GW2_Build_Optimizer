//! Sigil slots that keep their holes.
//!
//! A build has four sigil slots — `[set 1 main, set 1 off, set 2 main,
//! set 2 off]` — and any of them can be empty: a two-handed weapon has no
//! off-hand, an unfinished build has no set 2, and an unsocketed weapon has no
//! sigil at all.
//!
//! Collapsing that array into a dense list of ids destroys which slot each
//! sigil came from, and the damage is silent. A build with a bare main-hand on
//! set 1 and two sigils on set 2 collapses to `[set2_main, set2_off]`, and a
//! consumer that reads "the first two entries" as the active set then reads the
//! *inactive* set's sigils and applies them to combat maths. Every seat stays
//! addressable here, and an empty seat stays [`None`].

use gw2_core::types::GearSlot;

/// Sigil seats in a build: two weapon sets, main-hand and off-hand each.
pub const SIGIL_SLOT_COUNT: usize = 4;

/// One sigil seat, in canonical `[set 1 main, set 1 off, set 2 main, set 2 off]`
/// order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SigilSlot {
    Set1Main,
    Set1Off,
    Set2Main,
    Set2Off,
}

impl SigilSlot {
    /// All four seats in canonical order.
    pub const ALL: [SigilSlot; SIGIL_SLOT_COUNT] = [
        SigilSlot::Set1Main,
        SigilSlot::Set1Off,
        SigilSlot::Set2Main,
        SigilSlot::Set2Off,
    ];

    /// Position of this seat in the canonical array.
    pub const fn index(self) -> usize {
        match self {
            SigilSlot::Set1Main => 0,
            SigilSlot::Set1Off => 1,
            SigilSlot::Set2Main => 2,
            SigilSlot::Set2Off => 3,
        }
    }

    /// True for the seats on weapon set 1 — the set the optimizer treats as
    /// worn, and the only one whose sigils are on the character.
    pub const fn is_active_set(self) -> bool {
        matches!(self, SigilSlot::Set1Main | SigilSlot::Set1Off)
    }

    /// The weapon this sigil is socketed into.
    pub const fn gear_slot(self) -> GearSlot {
        match self {
            SigilSlot::Set1Main => GearSlot::WeaponSet1Main,
            SigilSlot::Set1Off => GearSlot::WeaponSet1Off,
            SigilSlot::Set2Main => GearSlot::WeaponSet2Main,
            SigilSlot::Set2Off => GearSlot::WeaponSet2Off,
        }
    }

    /// The sigil seat inside a weapon slot, or `None` for a slot that holds no
    /// weapon (armour and trinkets take runes and infusions, not sigils).
    pub const fn from_gear_slot(slot: GearSlot) -> Option<SigilSlot> {
        match slot {
            GearSlot::WeaponSet1Main => Some(SigilSlot::Set1Main),
            GearSlot::WeaponSet1Off => Some(SigilSlot::Set1Off),
            GearSlot::WeaponSet2Main => Some(SigilSlot::Set2Main),
            GearSlot::WeaponSet2Off => Some(SigilSlot::Set2Off),
            _ => None,
        }
    }
}

/// The four sigil seats of a build, holes included.
///
/// Holds sigil item ids. An absent id means "this seat has no sigil", which is
/// a fact worth keeping: it is the difference between a two-handed set and a
/// main-hand-only draft, and between "set 2 is empty" and "set 2 is missing".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SigilSlots {
    slots: [Option<u32>; SIGIL_SLOT_COUNT],
}

impl SigilSlots {
    /// Build from the canonical array `[set 1 main, set 1 off, set 2 main,
    /// set 2 off]`.
    pub const fn new(slots: [Option<u32>; SIGIL_SLOT_COUNT]) -> Self {
        Self { slots }
    }

    /// The sigil in one seat, if any.
    pub const fn get(&self, slot: SigilSlot) -> Option<u32> {
        self.slots[slot.index()]
    }

    /// Put a sigil in one seat, or clear it with `None`.
    pub fn set(&mut self, slot: SigilSlot, sigil_id: Option<u32>) {
        self.slots[slot.index()] = sigil_id;
    }

    /// The canonical array, holes and all.
    pub const fn as_array(&self) -> [Option<u32>; SIGIL_SLOT_COUNT] {
        self.slots
    }

    /// The two seats on weapon set 1 — the sigils actually on the character.
    ///
    /// Positional, so `[None, Some(id)]` still says "no main-hand sigil, an
    /// off-hand one": the caller cannot mistake an off-hand sigil for a
    /// main-hand one.
    pub const fn active_set(&self) -> [Option<u32>; 2] {
        [
            self.slots[SigilSlot::Set1Main.index()],
            self.slots[SigilSlot::Set1Off.index()],
        ]
    }

    /// The two seats on weapon set 2 — carried, not worn.
    pub const fn second_set(&self) -> [Option<u32>; 2] {
        [
            self.slots[SigilSlot::Set2Main.index()],
            self.slots[SigilSlot::Set2Off.index()],
        ]
    }

    /// Every socketed sigil, each still tagged with the seat it sits in.
    ///
    /// Tagged on purpose: an untagged id list is exactly the lossy shape this
    /// type exists to replace.
    pub fn equipped(self) -> impl Iterator<Item = (SigilSlot, u32)> {
        SigilSlot::ALL
            .into_iter()
            .filter_map(move |slot| self.get(slot).map(|sigil_id| (slot, sigil_id)))
    }

    /// How many of the four seats hold a sigil.
    pub fn count_equipped(self) -> usize {
        self.equipped().count()
    }

    /// True when no seat holds a sigil.
    pub fn is_empty(self) -> bool {
        self.count_equipped() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_seat_stays_a_hole() {
        // Bare main-hand on set 1, both sigils on set 2. A dense id list would
        // read `[7, 9]` and hand set 2's sigils to the active set.
        let slots = SigilSlots::new([Some(5), None, Some(7), Some(9)]);

        assert_eq!(slots.active_set(), [Some(5), None]);
        assert_eq!(slots.second_set(), [Some(7), Some(9)]);
        assert_eq!(slots.get(SigilSlot::Set1Off), None);
        assert_eq!(slots.get(SigilSlot::Set2Main), Some(7));
    }

    #[test]
    fn a_leading_hole_does_not_shift_the_off_hand_into_the_main_hand() {
        let slots = SigilSlots::new([None, Some(42), None, None]);

        assert_eq!(slots.active_set(), [None, Some(42)]);
        assert_eq!(slots.get(SigilSlot::Set1Main), None);
        assert_eq!(slots.count_equipped(), 1);
    }

    #[test]
    fn equipped_keeps_the_seat_with_the_sigil() {
        let slots = SigilSlots::new([None, Some(42), Some(7), None]);

        let seats: Vec<(SigilSlot, u32)> = slots.equipped().collect();

        assert_eq!(
            seats,
            vec![(SigilSlot::Set1Off, 42), (SigilSlot::Set2Main, 7)]
        );
        assert_eq!(slots.count_equipped(), 2);
        assert!(!slots.is_empty());
    }

    #[test]
    fn a_build_with_no_sigils_is_empty_not_absent() {
        let slots = SigilSlots::default();

        assert!(slots.is_empty());
        assert_eq!(slots.count_equipped(), 0);
        assert_eq!(slots.as_array(), [None, None, None, None]);
        assert_eq!(slots.active_set(), [None, None]);
        assert_eq!(slots.equipped().count(), 0);
    }

    #[test]
    fn setting_a_seat_touches_only_that_seat() {
        let mut slots = SigilSlots::new([Some(1), Some(2), Some(3), Some(4)]);

        slots.set(SigilSlot::Set1Off, None);
        assert_eq!(slots.as_array(), [Some(1), None, Some(3), Some(4)]);

        slots.set(SigilSlot::Set2Off, Some(99));
        assert_eq!(slots.as_array(), [Some(1), None, Some(3), Some(99)]);
    }

    #[test]
    fn seat_order_and_indexes_are_canonical() {
        assert_eq!(SigilSlot::ALL.len(), SIGIL_SLOT_COUNT);
        for (position, slot) in SigilSlot::ALL.into_iter().enumerate() {
            assert_eq!(slot.index(), position, "{slot:?}");
        }
        assert!(SigilSlot::Set1Main.is_active_set());
        assert!(SigilSlot::Set1Off.is_active_set());
        assert!(!SigilSlot::Set2Main.is_active_set());
        assert!(!SigilSlot::Set2Off.is_active_set());
    }

    #[test]
    fn seats_round_trip_through_their_weapon_slot() {
        for slot in SigilSlot::ALL {
            assert_eq!(SigilSlot::from_gear_slot(slot.gear_slot()), Some(slot));
        }
        assert_eq!(SigilSlot::from_gear_slot(GearSlot::Helm), None);
        assert_eq!(SigilSlot::from_gear_slot(GearSlot::Ring1), None);
        assert_eq!(SigilSlot::from_gear_slot(GearSlot::Back), None);
    }
}
