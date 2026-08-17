use gw2_core::types::GameMode;

use crate::balance::BalanceContext;
use crate::scoring::OptimizationWeights;

/// Scenario is part of the optimization input, not implicit engine state.
/// A build can only be called "best" relative to a declared scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioSpec {
    pub game_mode: GameMode,
    pub combat_tier: CombatTier,
    pub combat_kind: CombatKind,
    pub target_profile: TargetProfile,
    pub optimization_target: OptimizationTarget,
    pub patch_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CombatTier {
    Solo,
    Party,
    #[default]
    Squad,
}

/// What job the dummy is scoring. Independent of [`CombatTier`] (scale).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CombatKind {
    #[default]
    StrikeSpike,
    CondiRamp,
    Harasser,
    Support,
    Disabler,
    Commander,
    /// Occupy / escape — survive until backup. Not a kill job.
    Staller,
}

impl CombatKind {
    pub fn label(&self) -> &'static str {
        match self {
            CombatKind::StrikeSpike => "Strike spike",
            CombatKind::CondiRamp => "Condi ramp",
            CombatKind::Harasser => "Harasser",
            CombatKind::Support => "Support",
            CombatKind::Disabler => "Disabler",
            CombatKind::Commander => "Commander",
            CombatKind::Staller => "Staller",
        }
    }
}

impl CombatTier {
    /// Human-readable label used in UI and logs.
    pub fn label(&self) -> &'static str {
        match self {
            CombatTier::Solo => "Roam",
            CombatTier::Party => "Havoc",
            CombatTier::Squad => "Cloud/Zerg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProfile {
    Single,
    Cleave,
    AoE,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationTarget {
    pub label: String,
}

impl ScenarioSpec {
    pub fn from_balance_context(ctx: &BalanceContext) -> Self {
        Self {
            game_mode: ctx.game_mode.clone(),
            combat_tier: CombatTier::Solo,
            combat_kind: CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: ctx.game_mode.label().to_string(),
            },
            patch_id: Some(ctx.patch_id.clone()),
        }
    }

    /// Construct scenario with explicit combat tier (used by UI when WvW sub-role is selected).
    pub fn with_combat_tier(mut self, tier: CombatTier) -> Self {
        self.combat_tier = tier;
        self
    }

    pub fn with_combat_kind(mut self, kind: CombatKind) -> Self {
        self.combat_kind = kind;
        self
    }
}

// ─── Role Objectives ─────────────────────────────────────────────────────────

/// Job the player picked. Mode (PvE/PvP/WvW) remaps weights via [`profile_id`].
/// Scale (Roam/Havoc/Cloud/Zerg) is independent — see [`CombatTier`].
///
/// Overlay chips use [`PLAY_ROLES`] + [`play_label`]. Legacy WvW/PvP variants
/// stay so old mappings and tests still compile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleObjective {
    PowerDps,
    CondiDps,
    Sustain,
    Tank,
    Healer,
    Disabler,
    Buffer,
    /// Celestial-style all-rounder. Occupies; not a specialist.
    Hybrid,
    /// Stall until backup. Survive via tank/heal/evade/port/stealth — not DPS.
    Staller,
    WvWRoamer,
    WvWZergDps,
    WvWZergSupport,
    WvWDisruptor,
    PvPBurst,
    PvPSustain,
    PvPDisruptor,
}

impl RoleObjective {
    /// Shared overlay chips. Same jobs in every mode; [`profile_id`] changes the build.
    pub const PLAY_ROLES: [RoleObjective; 10] = [
        RoleObjective::WvWRoamer,
        RoleObjective::PowerDps,
        RoleObjective::CondiDps,
        RoleObjective::Hybrid,
        RoleObjective::Sustain,
        RoleObjective::Staller,
        RoleObjective::Healer,
        RoleObjective::Buffer,
        RoleObjective::Disabler,
        RoleObjective::Tank,
    ];

    /// Display label shown in the UI.
    pub fn label(&self) -> &'static str {
        match self {
            RoleObjective::PowerDps => "Power DPS",
            RoleObjective::CondiDps => "Condi DPS",
            RoleObjective::Sustain => "Sustain / Bruiser",
            RoleObjective::Tank => "Tank",
            RoleObjective::Healer => "Healer",
            RoleObjective::Disabler => "Disabler / CC",
            RoleObjective::Buffer => "Buffer / Support",
            RoleObjective::Hybrid => "Hybrid",
            RoleObjective::Staller => "Troll",
            RoleObjective::WvWRoamer => "Roamer",
            RoleObjective::WvWZergDps => "WvW Zerg DPS",
            RoleObjective::WvWZergSupport => "WvW Zerg Support",
            RoleObjective::WvWDisruptor => "WvW Disruptor",
            RoleObjective::PvPBurst => "PvP Burst",
            RoleObjective::PvPSustain => "PvP Sustain",
            RoleObjective::PvPDisruptor => "PvP Disruptor",
        }
    }

    /// Short chip label (same as [`label`] for play roles).
    pub fn play_label(&self) -> &'static str {
        match self {
            RoleObjective::PowerDps => "Power",
            RoleObjective::CondiDps => "Condi",
            RoleObjective::Sustain => "Bruiser",
            RoleObjective::Tank => "Commander",
            RoleObjective::Healer => "Heal",
            RoleObjective::Disabler => "Disabler",
            RoleObjective::Buffer => "Support",
            RoleObjective::Hybrid => "Hybrid",
            RoleObjective::Staller => "Troll",
            RoleObjective::WvWRoamer => "Roamer",
            other => other.label(),
        }
    }

    /// The `objective_profile_id` this role maps to for the given game mode.
    /// Falls back to mode default if the role has no direct profile for that mode.
    pub fn profile_id(&self, game_mode: &GameMode) -> &'static str {
        match (self, game_mode) {
            (RoleObjective::WvWRoamer, GameMode::WvW) => "WvW_Roamer",
            (RoleObjective::WvWRoamer, GameMode::PvP) => "PvP_Harasser",
            (RoleObjective::WvWRoamer, GameMode::PvE) => "PvE_Harasser",
            (RoleObjective::PowerDps, GameMode::PvE) => "PvE_Power_DPS",
            (RoleObjective::PowerDps, GameMode::WvW) => "WvW_Zerg_DPS",
            (RoleObjective::PowerDps, GameMode::PvP) => "PvP_Burst",
            (RoleObjective::CondiDps, GameMode::PvE) => "PvE_Condi_DPS",
            (RoleObjective::CondiDps, GameMode::WvW) => "WvW_Condi",
            (RoleObjective::CondiDps, GameMode::PvP) => "PvP_Condi",
            (RoleObjective::Hybrid, GameMode::PvE) => "PvE_Hybrid",
            (RoleObjective::Hybrid, GameMode::WvW) => "WvW_Hybrid",
            (RoleObjective::Hybrid, GameMode::PvP) => "PvP_Hybrid",
            (RoleObjective::Sustain, GameMode::PvE) => "PvE_Bruiser",
            (RoleObjective::Sustain, GameMode::WvW) => "WvW_Bruiser",
            (RoleObjective::Sustain, GameMode::PvP) => "PvP_Bruiser",
            (RoleObjective::Staller, GameMode::PvE) => "PvE_Staller",
            (RoleObjective::Staller, GameMode::WvW) => "WvW_Staller",
            (RoleObjective::Staller, GameMode::PvP) => "PvP_Staller",
            (RoleObjective::Healer, GameMode::PvE) => "PvE_Healer",
            (RoleObjective::Healer, GameMode::WvW) => "WvW_Heal",
            (RoleObjective::Healer, GameMode::PvP) => "PvP_Sustain",
            (RoleObjective::Buffer, GameMode::PvE) => "PvE_Boon_Support",
            (RoleObjective::Buffer, GameMode::WvW) => "WvW_Zerg_Support",
            (RoleObjective::Buffer, GameMode::PvP) => "PvP_Boon_Pressure",
            (RoleObjective::Disabler, GameMode::PvE) => "PvE_Disabler",
            (RoleObjective::Disabler, GameMode::WvW) => "WvW_Disruptor",
            (RoleObjective::Disabler, GameMode::PvP) => "PvP_Control_Disruptor",
            (RoleObjective::Tank, GameMode::PvE) => "PvE_Commander",
            (RoleObjective::Tank, GameMode::WvW) => "WvW_Commander",
            (RoleObjective::Tank, GameMode::PvP) => "PvP_Commander",
            (RoleObjective::WvWZergDps, _) => "WvW_Zerg_DPS",
            (RoleObjective::WvWZergSupport, _) => "WvW_Zerg_Support",
            (RoleObjective::WvWDisruptor, _) => "WvW_Disruptor",
            (RoleObjective::PvPBurst, _) => "PvP_Burst",
            (RoleObjective::PvPSustain, _) => "PvP_Sustain",
            (RoleObjective::PvPDisruptor, _) => "PvP_Control_Disruptor",
        }
    }

    /// The combat tier this role implies (used to construct ScenarioSpec).
    pub fn combat_tier(&self) -> CombatTier {
        match self {
            RoleObjective::WvWRoamer => CombatTier::Solo,
            RoleObjective::WvWDisruptor => CombatTier::Solo,
            RoleObjective::WvWZergDps => CombatTier::Squad,
            RoleObjective::WvWZergSupport => CombatTier::Squad,
            RoleObjective::PvPBurst => CombatTier::Solo,
            RoleObjective::PvPSustain => CombatTier::Solo,
            RoleObjective::PvPDisruptor => CombatTier::Solo,
            RoleObjective::Hybrid | RoleObjective::Staller => CombatTier::Solo,
            // Generic archetypes default to Party (sensible middle ground)
            _ => CombatTier::Party,
        }
    }

    /// Task axis — independent of [`combat_tier`] (scale).
    pub fn combat_kind(&self) -> CombatKind {
        match self {
            RoleObjective::CondiDps => CombatKind::CondiRamp,
            RoleObjective::Healer | RoleObjective::Buffer | RoleObjective::WvWZergSupport => {
                CombatKind::Support
            }
            RoleObjective::Disabler | RoleObjective::WvWDisruptor | RoleObjective::PvPDisruptor => {
                CombatKind::Disabler
            }
            RoleObjective::WvWRoamer => CombatKind::StrikeSpike,
            RoleObjective::Hybrid | RoleObjective::Staller => CombatKind::Staller,
            RoleObjective::Tank => CombatKind::Commander,
            RoleObjective::PowerDps
            | RoleObjective::Sustain
            | RoleObjective::WvWZergDps
            | RoleObjective::PvPBurst
            | RoleObjective::PvPSustain => CombatKind::StrikeSpike,
        }
    }

    /// Roamers are three jobs, not one 1v1 dummy.
    /// Dive: 2s port-in burst. Trickster: 5s peck/attrition. Bruiser: 10s cover fight.
    pub fn combat_kind_for_weights(&self, weights: &OptimizationWeights) -> CombatKind {
        if !matches!(self, RoleObjective::WvWRoamer) {
            return self.combat_kind();
        }
        let power = weights.power;
        let condi = weights.condition;
        let sustain = weights.sustain;
        if sustain > power && sustain > condi {
            CombatKind::Staller
        } else if condi > power {
            CombatKind::CondiRamp
        } else {
            CombatKind::StrikeSpike
        }
    }


    /// Convert this role to `OptimizationWeights` by reading the objective profile data.
    /// Falls back to mode default weights if the profile is not found.
    pub fn to_weights(&self, game_mode: &GameMode) -> OptimizationWeights {
        let id = self.profile_id(game_mode);
        let profiles = crate::data::objective_profiles::objective_profiles();
        if let Some(profile) = profiles.profile_by_id(id) {
            let aw = &profile.axis_weights;
            OptimizationWeights {
                power: aw.power,
                condition: aw.condition,
                boon_support: aw.boon_support,
                healing: aw.healing,
                sustain: aw.sustain,
                control: aw.control,
            }
        } else {
            OptimizationWeights::default_for_mode(game_mode.label())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CombatKind, CombatTier, RoleObjective, ScenarioSpec, TargetProfile};
    use crate::balance::BalanceContext;
    use gw2_core::types::GameMode;

    #[test]
    fn scenario_from_balance_context_captures_mode_and_patch() {
        let ctx = BalanceContext::new(GameMode::WvW);
        let scenario = ScenarioSpec::from_balance_context(&ctx);
        assert_eq!(scenario.game_mode, GameMode::WvW);
        assert_eq!(scenario.combat_tier, CombatTier::Solo);
        assert_eq!(scenario.target_profile, TargetProfile::Single);
        assert_eq!(scenario.patch_id.as_deref(), Some(ctx.patch_id.as_str()));
        assert_eq!(scenario.optimization_target.label, "WvW");
    }

    // ─── RoleObjective profile mapping ──────────────────────────────────────

    #[test]
    fn role_wvw_roamer_maps_to_correct_profile_and_tier() {
        assert_eq!(
            RoleObjective::WvWRoamer.profile_id(&GameMode::WvW),
            "WvW_Roamer"
        );
        assert_eq!(RoleObjective::WvWRoamer.combat_tier(), CombatTier::Solo);
    }

    #[test]
    fn role_wvw_zerg_dps_maps_to_squad_tier() {
        assert_eq!(
            RoleObjective::WvWZergDps.profile_id(&GameMode::WvW),
            "WvW_Zerg_DPS"
        );
        assert_eq!(RoleObjective::WvWZergDps.combat_tier(), CombatTier::Squad);
    }

    #[test]
    fn role_wvw_zerg_support_maps_to_squad_tier() {
        assert_eq!(
            RoleObjective::WvWZergSupport.profile_id(&GameMode::WvW),
            "WvW_Zerg_Support"
        );
        assert_eq!(
            RoleObjective::WvWZergSupport.combat_tier(),
            CombatTier::Squad
        );
    }

    #[test]
    fn role_wvw_disruptor_maps_to_solo_tier() {
        assert_eq!(
            RoleObjective::WvWDisruptor.profile_id(&GameMode::WvW),
            "WvW_Disruptor"
        );
        assert_eq!(RoleObjective::WvWDisruptor.combat_tier(), CombatTier::Solo);
    }

    #[test]
    fn role_pve_power_dps_profile_id() {
        assert_eq!(
            RoleObjective::PowerDps.profile_id(&GameMode::PvE),
            "PvE_Power_DPS"
        );
    }

    #[test]
    fn role_to_weights_wvw_roamer_differs_from_pve_power_dps() {
        let roamer_w = RoleObjective::WvWRoamer.to_weights(&GameMode::WvW);
        let pve_w = RoleObjective::PowerDps.to_weights(&GameMode::PvE);
        // Roamer should have meaningful sustain; PvE power DPS should be pure power
        assert!(
            roamer_w.sustain > pve_w.sustain,
            "WvW Roamer sustain ({}) should exceed PvE Power DPS sustain ({})",
            roamer_w.sustain,
            pve_w.sustain
        );
        assert!(
            pve_w.power > roamer_w.power,
            "PvE Power DPS power ({}) should exceed WvW Roamer power ({})",
            pve_w.power,
            roamer_w.power
        );
    }

    #[test]
    fn role_to_weights_uses_profile_data_not_defaults() {
        // WvW Roamer profile has specific weights in wvw.json — verify they loaded correctly
        let w = RoleObjective::WvWRoamer.to_weights(&GameMode::WvW);
        // From wvw.json WvW_Roamer: power=0.5, sustain=0.5, control=0.5
        assert!(
            (w.power - 0.5).abs() < 0.01,
            "WvW_Roamer power should be 0.5, got {}",
            w.power
        );
        assert!(
            (w.sustain - 0.5).abs() < 0.01,
            "WvW_Roamer sustain should be 0.5, got {}",
            w.sustain
        );
        assert!(
            (w.control - 0.5).abs() < 0.01,
            "WvW_Roamer control should be 0.5, got {}",
            w.control
        );
    }

    #[test]
    fn combat_tier_label_is_non_empty() {
        for tier in [CombatTier::Solo, CombatTier::Party, CombatTier::Squad] {
            assert!(!tier.label().is_empty());
        }
    }

    #[test]
    fn role_combat_kind_is_independent_of_scale() {
        assert_eq!(
            RoleObjective::WvWRoamer.combat_kind(),
            CombatKind::StrikeSpike
        );
        assert_eq!(
            RoleObjective::WvWZergSupport.combat_kind(),
            CombatKind::Support
        );
        assert_eq!(
            RoleObjective::WvWDisruptor.combat_kind(),
            CombatKind::Disabler
        );
        assert_eq!(RoleObjective::Tank.combat_kind(), CombatKind::Commander);
        assert_eq!(RoleObjective::CondiDps.combat_kind(), CombatKind::CondiRamp);
        assert_eq!(
            RoleObjective::WvWZergDps.combat_kind(),
            CombatKind::StrikeSpike
        );
    }

    #[test]
    fn roam_weights_pick_dive_trickster_or_bruiser_clock() {
        let dive = crate::scoring::OptimizationWeights {
            power: 0.8,
            condition: 0.2,
            sustain: 0.3,
            ..crate::scoring::OptimizationWeights::default()
        };
        assert_eq!(
            RoleObjective::WvWRoamer.combat_kind_for_weights(&dive),
            CombatKind::StrikeSpike
        );
        let trickster = crate::scoring::OptimizationWeights {
            power: 0.2,
            condition: 0.8,
            sustain: 0.3,
            ..crate::scoring::OptimizationWeights::default()
        };
        assert_eq!(
            RoleObjective::WvWRoamer.combat_kind_for_weights(&trickster),
            CombatKind::CondiRamp
        );
        let bruiser = crate::scoring::OptimizationWeights {
            power: 0.4,
            condition: 0.2,
            sustain: 0.9,
            ..crate::scoring::OptimizationWeights::default()
        };
        assert_eq!(
            RoleObjective::WvWRoamer.combat_kind_for_weights(&bruiser),
            CombatKind::Staller
        );
    }


    #[test]
    fn play_roles_are_shared_and_remap_per_mode() {
        assert_eq!(RoleObjective::PLAY_ROLES.len(), 10);
        assert_eq!(RoleObjective::Healer.play_label(), "Heal");
        assert_eq!(RoleObjective::Staller.play_label(), "Troll");
        assert_eq!(RoleObjective::WvWRoamer.play_label(), "Roamer");
        assert_eq!(
            RoleObjective::Healer.profile_id(&GameMode::PvE),
            "PvE_Healer"
        );
        assert_eq!(RoleObjective::Healer.profile_id(&GameMode::WvW), "WvW_Heal");
        assert_eq!(
            RoleObjective::Healer.profile_id(&GameMode::PvP),
            "PvP_Sustain"
        );
        assert_eq!(
            RoleObjective::Hybrid.profile_id(&GameMode::WvW),
            "WvW_Hybrid"
        );
        assert_eq!(
            RoleObjective::Staller.profile_id(&GameMode::WvW),
            "WvW_Staller"
        );
        let pve = RoleObjective::Healer.to_weights(&GameMode::PvE);
        let wvw = RoleObjective::Healer.to_weights(&GameMode::WvW);
        assert!(
            (pve.healing - wvw.healing).abs() > 0.01 || (pve.sustain - wvw.sustain).abs() > 0.01,
            "Heal in PvE vs WvW should not share the same weight vector"
        );
    }

    #[test]
    fn play_roles_have_unique_weights_in_every_mode() {
        for mode in [GameMode::PvE, GameMode::PvP, GameMode::WvW] {
            let mut seen: Vec<(RoleObjective, [i32; 6])> = Vec::new();
            for role in RoleObjective::PLAY_ROLES {
                let w = role.to_weights(&mode);
                let key = [
                    (w.power * 100.0).round() as i32,
                    (w.condition * 100.0).round() as i32,
                    (w.boon_support * 100.0).round() as i32,
                    (w.healing * 100.0).round() as i32,
                    (w.sustain * 100.0).round() as i32,
                    (w.control * 100.0).round() as i32,
                ];
                if let Some((other, _)) = seen.iter().find(|(_, k)| *k == key) {
                    panic!(
                        "{:?} and {:?} share weights {:?} in {:?}",
                        role, other, key, mode
                    );
                }
                seen.push((role, key));
            }
        }
    }

    #[test]
    fn hybrid_and_troll_are_occupy_jobs_not_kills() {
        assert_eq!(RoleObjective::Hybrid.combat_kind(), CombatKind::Staller);
        assert_eq!(RoleObjective::Staller.combat_kind(), CombatKind::Staller);
        let hybrid = RoleObjective::Hybrid.to_weights(&GameMode::WvW);
        let troll = RoleObjective::Staller.to_weights(&GameMode::WvW);
        assert!(
            troll.power + troll.condition < hybrid.power + hybrid.condition,
            "troll should dump damage vs hybrid celestial"
        );
        assert!(troll.sustain >= 0.9, "troll sustain {}", troll.sustain);
    }
}
