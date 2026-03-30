use gw2_core::types::GameMode;

use crate::balance::BalanceContext;

/// Scenario is part of the optimization input, not implicit engine state.
/// A build can only be called "best" relative to a declared scenario.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioSpec {
    pub game_mode: GameMode,
    pub combat_tier: CombatTier,
    pub target_profile: TargetProfile,
    pub optimization_target: OptimizationTarget,
    pub patch_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombatTier {
    Solo,
    Party,
    Squad,
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
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: ctx.game_mode.label().to_string(),
            },
            patch_id: Some(ctx.patch_id.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CombatTier, ScenarioSpec, TargetProfile};
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
}
