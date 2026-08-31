//! Counterplay-aware WvW combat timeline.
//!
//! The legacy rotation simulator answers "what is the average damage of this
//! skill roster against a dummy?"  This module answers the WvW question: "can
//! the build establish control of a real exchange long enough to finish its
//! chain, survive the answer, recover, and do it again?"

use std::collections::{HashMap, HashSet, VecDeque};

use crate::data::normalized_effects::{
    EffectCategory, NormalizedEffect, OperationType, SourceType, TargetSide, TriggerRule,
};
use crate::data::quality::FactualValue;
use crate::scenario::{CombatKind, CombatTier, ScenarioSpec};

use super::combat_model::{corrupt_into, EnemyDummy};
use super::skill_timings::{HUMAN_DELAY_MS, MIN_SKILL_GAP_MS};
use super::simulator::{
    alacrity_cd_advance_ms, condition_tick_damage, reference_armor, strike_crit_factor_with_bonus,
    SimParams,
};
use super::{CoverKind, MobilityKind, RotationSkill, SkillEffect, SkillSlot};

const TIMELINE_TICK_MS: u32 = 50;
pub const MIN_PROTECTED_WINDOW_MS: u32 = 2_000;
pub const TARGET_PROTECTED_WINDOW_MS: u32 = 5_000;

/// Wiki Barrier: disappears 5s after applied; WvW cap is 25% of max health.
const BARRIER_LIFETIME_MS: u32 = 5_000;
const WVW_BARRIER_HEALTH_FRACTION: f64 = 0.25;
/// Wiki Interrupt: interrupted skills get a 5 second cooldown.
const INTERRUPT_COOLDOWN_MS: u32 = 5_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceKind {
    Initiative,
    Energy,
    Adrenaline,
    Illusions,
    Blades,
}

#[derive(Debug, Clone)]
pub struct SkillResourceRule {
    pub skill_id: u32,
    pub kind: ResourceKind,
    pub cost: f64,
    pub gain_on_hit: f64,
    pub spend_all: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WvwCombatReport {
    pub duration_ms: u32,
    pub target_health: Option<f64>,
    pub target_reached_at_ms: Option<u32>,
    pub longest_protected_window_ms: u32,
    pub protected_action_count: u32,
    pub successful_action_count: u32,
    pub interrupted_casts: u32,
    pub protected_damage: f64,
    pub peak_protected_damage_2s: f64,
    pub peak_protected_damage_5s: f64,
    pub total_damage: f64,
    pub control_landed_ms: u32,
    pub incoming_damage: f64,
    pub avoided_damage: f64,
    pub healing: f64,
    pub barrier_absorbed: f64,
    pub conditions_cleansed: u32,
    pub combo_activations: u32,
    pub remaining_health_ratio: f64,
    /// Positive means recovery/avoidance exceeded incoming pressure.
    pub sustain_margin: f64,
    pub player_survived: bool,
    pub target_reached: bool,
    pub chain_completed: bool,
    /// Pressure and control that occurred inside the qualifying secured
    /// sequence, not totals collected from unrelated moments.
    pub secured_sequence_damage: f64,
    pub secured_sequence_control_ms: u32,
    pub repeatable: bool,
    pub resource_blocked_actions: u32,
    pub resource_legal: bool,
    /// False when the active profession mechanic needs a state model that this
    /// bounded resource ledger does not yet provide.
    pub resource_model_complete: bool,
    /// Number of equipped effect sources for which the timeline had no timed
    /// normalized record. This never silently becomes verified data.
    pub unmodeled_effect_sources: u32,
}

pub struct WvwTimelineInput<'a> {
    pub skills: &'a [RotationSkill],
    pub duration_ms: u32,
    pub params: &'a SimParams,
    pub enemy: EnemyDummy,
    pub scenario: &'a ScenarioSpec,
    pub active_effects: &'a [&'a NormalizedEffect],
    pub resource_rules: &'a [SkillResourceRule],
    pub resource_model_complete: bool,
    pub unmodeled_effect_sources: u32,
    /// Exact in-combat weapon swap cooldown for this profession. `None`
    /// means the active specialization cannot swap weapons in combat.
    pub weapon_swap_cooldown_ms: Option<u32>,
}

#[derive(Debug, Clone)]
enum EnemyEventKind {
    Strike {
        damage: f64,
        unblockable: bool,
    },
    Control {
        duration_ms: u32,
        unblockable: bool,
    },
    Condition {
        condition: String,
        stacks: u32,
        duration_ms: u32,
    },
    BoonStrip {
        count: u32,
    },
}

#[derive(Debug, Clone)]
struct EnemyEvent {
    at_ms: u32,
    kind: EnemyEventKind,
}

#[derive(Debug, Clone)]
struct WvwProfile {
    duration_ms: u32,
    target_health: Option<f64>,
    enemy_events: VecDeque<EnemyEvent>,
    required_window_ms: u32,
    desired_window_ms: u32,
}

impl WvwProfile {
    fn for_scenario(
        scenario: &ScenarioSpec,
        enemy: &EnemyDummy,
        params: &SimParams,
        duration_ms: u32,
    ) -> Self {
        let duration_ms = duration_ms.max(MIN_PROTECTED_WINDOW_MS);
        let tier_pressure = match scenario.combat_tier {
            CombatTier::Solo => 1.0,
            CombatTier::Party => 1.20,
            CombatTier::Squad => 1.45,
        };
        let kind_pressure = match scenario.combat_kind {
            CombatKind::StrikeSpike | CombatKind::Harasser | CombatKind::Disabler => 1.10,
            CombatKind::CondiRamp => 1.0,
            CombatKind::Support | CombatKind::Commander | CombatKind::Staller => 0.90,
        };
        let pressure = tier_pressure * kind_pressure;
        let enemy_power = 2_500.0 * pressure;
        let strike = 1_100.0 * enemy_power / params.armor.max(1_000.0) * 2.0;
        let mut events = Vec::new();
        let mut cycle = 0;
        while cycle < duration_ms {
            events.push(EnemyEvent {
                at_ms: cycle + 450,
                kind: EnemyEventKind::Control {
                    duration_ms: 900,
                    unblockable: false,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 850,
                kind: EnemyEventKind::Strike {
                    damage: strike,
                    unblockable: false,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 1_650,
                kind: EnemyEventKind::Condition {
                    condition: if scenario.combat_kind == CombatKind::CondiRamp {
                        "Burning".into()
                    } else {
                        "Bleeding".into()
                    },
                    stacks: if scenario.combat_kind == CombatKind::CondiRamp {
                        3
                    } else {
                        2
                    },
                    duration_ms: 4_000,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 2_350,
                kind: EnemyEventKind::BoonStrip { count: 1 },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 3_050,
                kind: EnemyEventKind::Control {
                    duration_ms: 1_100,
                    unblockable: false,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 3_550,
                kind: EnemyEventKind::Strike {
                    damage: strike * 1.20,
                    unblockable: false,
                },
            });
            events.push(EnemyEvent {
                at_ms: cycle + 4_400,
                kind: EnemyEventKind::Strike {
                    damage: strike * 0.70,
                    unblockable: true,
                },
            });
            cycle += 5_000;
        }
        events.retain(|event| event.at_ms < duration_ms);
        events.sort_by_key(|event| event.at_ms);

        Self {
            duration_ms,
            target_health: enemy.hp,
            enemy_events: events.into(),
            required_window_ms: MIN_PROTECTED_WINDOW_MS,
            desired_window_ms: TARGET_PROTECTED_WINDOW_MS.min(duration_ms),
        }
    }
}

#[derive(Debug, Clone)]
struct TimedDefense {
    kind: CoverKind,
    expires_at_ms: u32,
    stacks: u32,
    strippable: bool,
}

#[derive(Debug, Clone)]
struct TimedBuff {
    name: String,
    stacks: u32,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct TimedCondition {
    name: String,
    stacks: u32,
    expires_at_ms: u32,
    next_tick_ms: u32,
}

struct BarrierLayer {
    amount: f64,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct ComboFieldState {
    field_type: String,
    expires_at_ms: u32,
}

#[derive(Debug, Clone)]
struct PendingCast {
    skill_idx: usize,
    started_at_ms: u32,
    resolves_at_ms: u32,
    protected_at_start: bool,
    saved_by_charge: bool,
}

#[derive(Debug, Clone)]
struct DamageEvent {
    at_ms: u32,
    amount: f64,
    protected: bool,
}

#[derive(Debug, Clone)]
struct ProtectedActionEvent {
    at_ms: u32,
    skill_id: u32,
    control_ms: u32,
    applies_condition: bool,
}

#[derive(Debug, Clone, Default)]
struct SecuredSequenceSummary {
    completed: bool,
    damage: f64,
    control_ms: u32,
    skill_ids: HashSet<u32>,
}

#[derive(Debug, Clone)]
struct ProcSpec {
    source_type: SourceType,
    source_id: u32,
    trigger: TriggerRule,
    category: EffectCategory,
    value: f64,
    duration_ms: u32,
    internal_cooldown_ms: u32,
    next_ready_ms: u32,
    operation: Option<crate::data::normalized_effects::StatusOperation>,
}

struct Timeline<'a> {
    skills: &'a [RotationSkill],
    params: &'a SimParams,
    profile: WvwProfile,
    now_ms: u32,
    next_action_ms: u32,
    disabled_until_ms: u32,
    active_weapon_set: u8,
    weapon_swap_ready_ms: u32,
    weapon_swap_cooldown_ms: Option<u32>,
    cooldown_ready_ms: Vec<u32>,
    pending: Option<PendingCast>,
    defenses: Vec<TimedDefense>,
    buffs: Vec<TimedBuff>,
    outgoing_conditions: Vec<TimedCondition>,
    incoming_conditions: Vec<TimedCondition>,
    combo_field: Option<ComboFieldState>,
    enemy_protection: bool,
    enemy_stability: bool,
    enemy_disabled_until_ms: u32,
    enemy_health: f64,
    target_reached_at_ms: Option<u32>,
    player_health: f64,
    barrier: VecDeque<BarrierLayer>,
    protected_run_ms: u32,
    longest_protected_window_ms: u32,
    charge_cover_consumed_this_tick: bool,
    secured_tick_times: Vec<u32>,
    protected_actions: Vec<ProtectedActionEvent>,
    damage_events: Vec<DamageEvent>,
    protected_action_count: u32,
    successful_action_count: u32,
    interrupted_casts: u32,
    control_landed_ms: u32,
    incoming_damage: f64,
    avoided_damage: f64,
    healing: f64,
    barrier_absorbed: f64,
    conditions_cleansed: u32,
    combo_activations: u32,
    proc_specs: Vec<ProcSpec>,
    passive_strike_mult: f64,
    passive_condition_mult: f64,
    passive_healing_mult: f64,
    incoming_strike_mult: f64,
    incoming_condition_mult: f64,
    bonus_boon_duration: f64,
    bonus_condition_duration: f64,
    unmodeled_effect_sources: u32,
    resource_rules: HashMap<u32, SkillResourceRule>,
    resources: HashMap<ResourceKind, f64>,
    resource_blocked_skills: HashSet<u32>,
    resource_model_complete: bool,
}

/// Run the WvW exchange model. Effects resolve at cast completion, so incoming
/// control can genuinely cancel the action instead of merely lowering a score.
pub fn evaluate_wvw_timeline(input: WvwTimelineInput<'_>) -> WvwCombatReport {
    let WvwTimelineInput {
        skills,
        duration_ms,
        params,
        enemy,
        scenario,
        active_effects,
        resource_rules,
        resource_model_complete,
        unmodeled_effect_sources,
        weapon_swap_cooldown_ms,
    } = input;
    let profile = WvwProfile::for_scenario(scenario, &enemy, params, duration_ms);
    let mut timeline = Timeline::new(
        skills,
        params,
        profile,
        enemy,
        active_effects,
        resource_rules,
        resource_model_complete,
        unmodeled_effect_sources,
    );
    timeline.weapon_swap_cooldown_ms = weapon_swap_cooldown_ms;
    timeline.run();
    timeline.report()
}

impl<'a> Timeline<'a> {
    #[allow(clippy::too_many_arguments)]
    fn new(
        skills: &'a [RotationSkill],
        params: &'a SimParams,
        profile: WvwProfile,
        enemy: EnemyDummy,
        active_effects: &[&NormalizedEffect],
        resource_rules: &[SkillResourceRule],
        resource_model_complete: bool,
        unmodeled_effect_sources: u32,
    ) -> Self {
        let mut state = Self {
            skills,
            params,
            // ponytail: no-target dummy uses +inf so events keep firing and target_reached stays false
            enemy_health: profile.target_health.unwrap_or(f64::INFINITY),
            player_health: params.max_health,
            enemy_protection: enemy.protection,
            enemy_stability: enemy.stability,
            profile,
            now_ms: 0,
            next_action_ms: 0,
            disabled_until_ms: 0,
            active_weapon_set: 1,
            weapon_swap_ready_ms: 0,
            weapon_swap_cooldown_ms: Some(10_000),
            cooldown_ready_ms: vec![0; skills.len()],
            pending: None,
            defenses: Vec::new(),
            buffs: Vec::new(),
            outgoing_conditions: Vec::new(),
            incoming_conditions: Vec::new(),
            combo_field: None,
            enemy_disabled_until_ms: 0,
            target_reached_at_ms: None,
            barrier: VecDeque::new(),
            protected_run_ms: 0,
            longest_protected_window_ms: 0,
            charge_cover_consumed_this_tick: false,
            secured_tick_times: Vec::new(),
            protected_actions: Vec::new(),
            damage_events: Vec::new(),
            protected_action_count: 0,
            successful_action_count: 0,
            interrupted_casts: 0,
            control_landed_ms: 0,
            incoming_damage: 0.0,
            avoided_damage: 0.0,
            healing: 0.0,
            barrier_absorbed: 0.0,
            conditions_cleansed: 0,
            combo_activations: 0,
            proc_specs: Vec::new(),
            passive_strike_mult: 1.0,
            passive_condition_mult: 1.0,
            passive_healing_mult: 1.0,
            incoming_strike_mult: 1.0,
            incoming_condition_mult: 1.0,
            bonus_boon_duration: 0.0,
            bonus_condition_duration: 0.0,
            unmodeled_effect_sources,
            resource_rules: resource_rules
                .iter()
                .map(|rule| (rule.skill_id, rule.clone()))
                .collect(),
            resources: initial_resources(resource_rules),
            resource_blocked_skills: HashSet::new(),
            resource_model_complete,
        };
        state.load_normalized_effects(active_effects);
        state
    }

    fn load_normalized_effects(&mut self, effects: &[&NormalizedEffect]) {
            for effect in effects {
                if matches!(effect.trigger_rule, TriggerRule::Passive) {
                    // Standing modifiers are already folded into SimParams by the
                    // shared combat parser. Applying them here would count the same
                    // trait/rune/sigil a second time.
                    continue;
                }

                if matches!(effect.source_type, SourceType::Skill)
                    && self.skill_directly_models_effect(effect)
                {
                    continue;
                }

                let supported = matches!(effect.trigger_rule, TriggerRule::OnHit)
                    || (matches!(effect.trigger_rule, TriggerRule::OnSkillUse)
                        && matches!(effect.source_type, SourceType::Skill));
                if !supported {
                    self.unmodeled_effect_sources += 1;
                    continue;
                }
                let Some(&value) = resolved(&effect.value) else {
                    self.unmodeled_effect_sources += 1;
                    continue;
                };

                self.proc_specs.push(ProcSpec {
                    source_type: effect.source_type.clone(),
                    source_id: effect.source_id,
                    trigger: effect.trigger_rule.clone(),
                    category: effect
                        .inner_category
                        .clone()
                        .unwrap_or_else(|| effect.category.clone()),
                    value,
                    duration_ms: effect
                        .effect_duration
                        .as_ref()
                        .and_then(resolved)
                        .map(|seconds| (seconds * 1_000.0).round() as u32)
                        .unwrap_or(0),
                    internal_cooldown_ms: effect
                        .internal_cooldown
                        .as_ref()
                        .and_then(resolved)
                        .map(|seconds| (seconds * 1_000.0).round() as u32)
                        .or_else(|| {
                            effect
                                .status_operation
                                .as_ref()
                                .and_then(|op| op.internal_cooldown_ms.as_ref())
                                .and_then(resolved)
                                .copied()
                        })
                        .unwrap_or(0),
                    next_ready_ms: 0,
                    operation: effect.status_operation.clone(),
                });
            }
        }

    fn run(&mut self) {
        while self.now_ms < self.profile.duration_ms && self.player_health > 0.0 {
            self.charge_cover_consumed_this_tick = false;
            self.expire_timed_state();
            self.regenerate_resources();
            self.resolve_pending_cast();
            self.tick_conditions();
            self.process_enemy_events();
            self.track_protected_window();

            if self.pending.is_none()
                && self.now_ms >= self.next_action_ms
                && self.now_ms >= self.disabled_until_ms
            {
                if let Some(skill_idx) = self.pick_skill() {
                    self.start_cast(skill_idx);
                } else {
                    self.try_weapon_swap();
                }
            } else if self.pending.is_none() {
                self.try_stunbreak();
            }

            self.now_ms += TIMELINE_TICK_MS;
            self.tick_alacrity_recharge();
        }
    }

    /// Dummy clock: 100ms wall consumes 125ms CD. Apply *4/5 at set is the leftover snapshot.
    fn tick_alacrity_recharge(&mut self) {
        if !self.now_ms.is_multiple_of(100) {
            return;
        }
        let extra = alacrity_cd_advance_ms(100, self.has_buff("Alacrity")).saturating_sub(100);
        if extra == 0 {
            return;
        }
        for ready in &mut self.cooldown_ready_ms {
            if *ready > self.now_ms {
                *ready = ready.saturating_sub(extra);
            }
        }
    }

    fn expire_timed_state(&mut self) {
            self.defenses
                .retain(|defense| defense.expires_at_ms > self.now_ms);
            self.buffs.retain(|buff| buff.expires_at_ms > self.now_ms);
            self.barrier
                .retain(|layer| layer.expires_at_ms > self.now_ms);
            if self
                .combo_field
                .as_ref()
                .is_some_and(|field| field.expires_at_ms <= self.now_ms)
            {
                self.combo_field = None;
            }
        }

    fn resolve_pending_cast(&mut self) {
        let Some(pending) = self.pending.clone() else {
            return;
        };
        if pending.resolves_at_ms > self.now_ms {
            return;
        }
        self.pending = None;
        self.successful_action_count += 1;
        let protected_before = pending.protected_at_start || pending.saved_by_charge;
        let first_damage_event = self.damage_events.len();
        let control_before = self.control_landed_ms;
        let skill_id = self.skills[pending.skill_idx].skill_id;
        let effects = self.skills[pending.skill_idx].effects.clone();
        let applies_condition = effects
            .iter()
            .any(|effect| matches!(effect, SkillEffect::ApplyCondition { .. }));
        for effect in effects {
            self.apply_skill_effect(skill_id, &effect, protected_before);
        }
        self.trigger_procs(TriggerRule::OnSkillUse, Some(skill_id), protected_before);
        let protected = protected_before || self.control_owned();
        if protected {
            self.protected_action_count += 1;
            for event in &mut self.damage_events[first_damage_event..] {
                event.protected = true;
            }
            self.protected_actions.push(ProtectedActionEvent {
                at_ms: self.now_ms,
                skill_id,
                control_ms: self.control_landed_ms.saturating_sub(control_before),
                applies_condition,
            });
        }
    }

    fn start_cast(&mut self, skill_idx: usize) {
        let skill = &self.skills[skill_idx];
        self.pay_resource(skill.skill_id);
        self.resource_blocked_skills.remove(&skill.skill_id);
        self.apply_incoming_confusion_on_skill_use();
        let quickness = self.has_buff("Quickness");
        let cast_ms = if quickness {
            (skill.cast_time_ms * 2 + 1) / 3
        } else {
            skill.cast_time_ms
        }
        .max(TIMELINE_TICK_MS);
        self.set_skill_cooldown(skill.skill_id, skill.cooldown_ms);
        self.pending = Some(PendingCast {
            skill_idx,
            started_at_ms: self.now_ms,
            resolves_at_ms: self.now_ms + cast_ms,
            protected_at_start: self.control_owned(),
            saved_by_charge: false,
        });
        self.next_action_ms = self.now_ms + cast_ms + HUMAN_DELAY_MS + MIN_SKILL_GAP_MS;
    }

    fn pick_skill(&mut self) -> Option<usize> {
        let health_ratio = self.player_health / self.params.max_health.max(1.0);
        let cover_remaining = self.control_cover_remaining_ms();
        let enemy_event_soon = self
            .profile
            .enemy_events
            .front()
            .is_some_and(|event| event.at_ms <= self.now_ms + 900);

        let mut best: Option<(usize, f64)> = None;
        let mut filler = None;
        let mut highest_unpaid: Option<(u32, f64)> = None;
        for (idx, skill) in self.skills.iter().enumerate() {
            if self.cooldown_ready_ms[idx] > self.now_ms || !self.skill_available(skill) {
                continue;
            }
            if skill.slot == SkillSlot::Weapon1 && skill.cooldown_ms == 0 {
                filler = Some(idx);
                continue;
            }
            let has_heal = skill
                .effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::Healing { .. }));
            if has_heal && health_ratio > 0.72 {
                continue;
            }
            let has_cover = skill.effects.iter().any(is_control_cover);
            let has_strip = skill.effects.iter().any(|effect| {
                matches!(
                    effect,
                    SkillEffect::StripBoons { .. }
                        | SkillEffect::StealBoons
                        | SkillEffect::CorruptBoons
                )
            });
            let has_control = skill
                .effects
                .iter()
                .any(|effect| matches!(effect, SkillEffect::CrowdControl { .. }));

            let mut priority = self.skill_damage_value(skill);
            if has_heal {
                priority += (1.0 - health_ratio) * 1_000_000.0;
            }
            if has_cover && (cover_remaining < self.profile.required_window_ms || enemy_event_soon)
            {
                priority += 900_000.0;
            }
            // Strip Stability/Protection before trying to CC or dump damage.
            if has_strip && (self.enemy_stability || self.enemy_protection) {
                priority += 800_000.0;
            }
            if has_control && !self.enemy_stability {
                priority += 700_000.0;
            }
            if has_control && self.enemy_stability {
                priority -= 500_000.0;
            }
            if !self.can_pay_resource(skill.skill_id) {
                if highest_unpaid
                    .as_ref()
                    .is_none_or(|(_, blocked_priority)| priority > *blocked_priority)
                {
                    highest_unpaid = Some((skill.skill_id, priority));
                }
                continue;
            }
            if best.as_ref().is_none_or(|(_, score)| priority > *score) {
                best = Some((idx, priority));
            }
        }
        let legal_priority = best.as_ref().map(|(_, score)| *score).unwrap_or(0.0);
        if let Some((skill_id, blocked_priority)) = highest_unpaid {
            if blocked_priority > legal_priority {
                self.resource_blocked_skills.insert(skill_id);
            }
        }
        best.map(|(idx, _)| idx).or(filler)
    }

    fn skill_available(&self, skill: &RotationSkill) -> bool {
        skill.weapon_set == 0 || skill.weapon_set == self.active_weapon_set
    }

    fn try_weapon_swap(&mut self) {
        let Some(cooldown_ms) = self.weapon_swap_cooldown_ms else {
            return;
        };
        if self.now_ms < self.weapon_swap_ready_ms
            || !self.skills.iter().any(|skill| skill.weapon_set > 0)
        {
            return;
        }
        let other = if self.active_weapon_set == 1 { 2 } else { 1 };
        if self.skills.iter().enumerate().any(|(idx, skill)| {
            skill.weapon_set == other && self.cooldown_ready_ms[idx] <= self.now_ms
        }) {
            self.active_weapon_set = other;
            self.weapon_swap_ready_ms = self.now_ms + cooldown_ms;
            self.next_action_ms = self.now_ms + MIN_SKILL_GAP_MS;
        }
    }

    fn try_stunbreak(&mut self) {
        if self.now_ms >= self.disabled_until_ms {
            return;
        }
        let candidates: Vec<usize> = self
            .skills
            .iter()
            .enumerate()
            .filter(|(idx, skill)| {
                skill.is_stunbreak
                    && self.cooldown_ready_ms[*idx] <= self.now_ms
                    && self.skill_available(skill)
            })
            .map(|(idx, _)| idx)
            .collect();
        let Some(idx) = candidates.into_iter().find(|idx| {
            let skill_id = self.skills[*idx].skill_id;
            let can_pay = self.can_pay_resource(skill_id);
            if !can_pay {
                self.resource_blocked_skills.insert(skill_id);
            }
            can_pay
        }) else {
            return;
        };
        let skill_id = self.skills[idx].skill_id;
        self.pay_resource(skill_id);
        self.resource_blocked_skills.remove(&skill_id);
        self.set_skill_cooldown(skill_id, self.skills[idx].cooldown_ms);
        self.disabled_until_ms = self.now_ms;
        self.next_action_ms = self.now_ms + MIN_SKILL_GAP_MS;
        self.successful_action_count += 1;
        let first_damage_event = self.damage_events.len();
        let control_before = self.control_landed_ms;
        let applies_condition = self.skills[idx]
            .effects
            .iter()
            .any(|effect| matches!(effect, SkillEffect::ApplyCondition { .. }));
        for effect in self.skills[idx].effects.clone() {
            self.apply_skill_effect(skill_id, &effect, true);
        }
        self.trigger_procs(TriggerRule::OnSkillUse, Some(skill_id), true);
        for event in &mut self.damage_events[first_damage_event..] {
            event.protected = true;
        }
        self.protected_action_count += 1;
        self.protected_actions.push(ProtectedActionEvent {
            at_ms: self.now_ms,
            skill_id,
            control_ms: self.control_landed_ms.saturating_sub(control_before),
            applies_condition,
        });
    }

    fn process_enemy_events(&mut self) {
            if self.enemy_health <= 0.0 {
                return;
            }
            while self
                .profile
                .enemy_events
                .front()
                .is_some_and(|event| event.at_ms <= self.now_ms)
            {
                let event = self
                    .profile
                    .enemy_events
                    .pop_front()
                    .expect("front checked");
                // A disabled opponent cannot continue a queued attack/cast. Existing
                // conditions still tick separately, but new strikes, CC, strips and
                // condition applications are lost during the control window.
                if self.enemy_disabled_until_ms > self.now_ms {
                    continue;
                }
                match event.kind {
                    EnemyEventKind::Strike {
                        damage,
                        unblockable,
                    } => self.receive_strike(damage, unblockable),
                    EnemyEventKind::Control {
                        duration_ms,
                        unblockable,
                    } => self.receive_control(duration_ms, unblockable),
                    EnemyEventKind::Condition {
                        condition,
                        stacks,
                        duration_ms,
                    } => self.receive_condition(condition, stacks, duration_ms),
                    EnemyEventKind::BoonStrip { count } => self.receive_boon_strip(count),
                }
            }
        }

    fn receive_strike(&mut self, raw_damage: f64, unblockable: bool) {
        if self.avoids_attack(unblockable) {
            self.avoided_damage += raw_damage;
            return;
        }
        let mut damage = raw_damage * self.incoming_strike_mult;
        if self.has_defense(CoverKind::Protection) {
            damage *= 0.67;
        }
        self.absorb_damage(damage);
    }

    fn receive_control(&mut self, duration_ms: u32, unblockable: bool) {
            if self.avoids_attack(unblockable) || self.consume_stability() {
                return;
            }
            if let Some(pending) = self.pending.take() {
                if pending.started_at_ms < self.now_ms {
                    self.interrupted_casts += 1;
                }
                let skill_id = self.skills[pending.skill_idx].skill_id;
                self.set_skill_cooldown(skill_id, INTERRUPT_COOLDOWN_MS);
            }
            self.disabled_until_ms = self.disabled_until_ms.max(self.now_ms + duration_ms);
            self.protected_run_ms = 0;
        }

    fn receive_condition(&mut self, condition: String, stacks: u32, duration_ms: u32) {
        if self.has_defense(CoverKind::Invulnerability)
            || (self.has_defense(CoverKind::Resistance) && !condition_is_damaging(&condition))
        {
            return;
        }
        self.incoming_conditions.push(TimedCondition {
            name: condition,
            stacks,
            expires_at_ms: self.now_ms + duration_ms,
            next_tick_ms: self.now_ms + 1_000,
        });
    }

    fn receive_boon_strip(&mut self, count: u32) {
        for _ in 0..count {
            let Some((idx, _)) = self
                .defenses
                .iter()
                .enumerate()
                .filter(|(_, defense)| defense.strippable)
                .min_by_key(|(_, defense)| defense.expires_at_ms)
            else {
                break;
            };
            self.defenses.remove(idx);
        }
    }

    fn remove_enemy_boons(&mut self, count: u32) -> Vec<&'static str> {
            let mut stripped = Vec::new();
            for _ in 0..count {
                if self.enemy_stability {
                    self.enemy_stability = false;
                    stripped.push("Stability");
                } else if self.enemy_protection {
                    self.enemy_protection = false;
                    stripped.push("Protection");
                } else {
                    break;
                }
            }
            stripped
        }

    fn avoids_attack(&mut self, unblockable: bool) -> bool {
        if self.has_defense(CoverKind::Invulnerability) || self.has_defense(CoverKind::Evade) {
            return true;
        }
        if !unblockable {
            if self.consume_defense(CoverKind::Aegis) {
                self.mark_charge_cover_consumed();
                return true;
            }
            if self.has_defense(CoverKind::Block) {
                return true;
            }
        }
        if !unblockable && self.consume_defense(CoverKind::Blind) {
            self.mark_charge_cover_consumed();
            return true;
        }
        false
    }

    fn mark_charge_cover_consumed(&mut self) {
        self.charge_cover_consumed_this_tick = true;
        if let Some(pending) = self.pending.as_mut() {
            pending.saved_by_charge = true;
        }
    }

    fn apply_incoming_confusion_on_skill_use(&mut self) {
        let stacks: u32 = self
            .incoming_conditions
            .iter()
            .filter(|c| c.name.eq_ignore_ascii_case("Confusion") && c.expires_at_ms > self.now_ms)
            .map(|c| c.stacks)
            .sum();
        if stacks == 0 {
            return;
        }
        let dmg = crate::data::conditions().confusion_tick(1_800.0, self.params.mode.clone(), true)
            * stacks as f64;
        self.absorb_damage(dmg * self.incoming_condition_mult);
    }

    fn absorb_damage(&mut self, damage: f64) {
            let mut remaining = damage;
            let mut absorbed = 0.0;
            while remaining > 0.0 {
                let Some(layer) = self.barrier.front_mut() else {
                    break;
                };
                let take = layer.amount.min(remaining);
                layer.amount -= take;
                remaining -= take;
                absorbed += take;
                if layer.amount <= 0.0 {
                    self.barrier.pop_front();
                }
            }
            self.barrier_absorbed += absorbed;
            self.incoming_damage += remaining;
            self.player_health = (self.player_health - remaining).max(0.0);
        }

    fn apply_barrier(&mut self, amount: f64) {
            if amount <= 0.0 {
                return;
            }
            let cap = self.params.max_health * WVW_BARRIER_HEALTH_FRACTION;
            let current: f64 = self.barrier.iter().map(|layer| layer.amount).sum();
            let applied = amount.min((cap - current).max(0.0));
            if applied > 0.0 {
                self.barrier.push_back(BarrierLayer {
                    amount: applied,
                    expires_at_ms: self.now_ms.saturating_add(BARRIER_LIFETIME_MS),
                });
            }
        }

    fn tick_conditions(&mut self) {
            let mut outgoing_damage = 0.0;
            let might = self.buff_stacks("Might").min(25) as f64;
            let condition_damage = self.params.condition_damage
                + might * crate::data::boon_condition_formulas::boons().might_condi_per_stack();
            for condition in &mut self.outgoing_conditions {
                if condition.next_tick_ms <= self.now_ms
                    && condition.next_tick_ms <= condition.expires_at_ms
                {
                    let tick =
                        condition_tick_damage(&condition.name, condition_damage, &self.params.mode)
                            * condition.stacks as f64
                            * self.params.condition_mult
                            * self.passive_condition_mult;
                    outgoing_damage += tick;
                    condition.next_tick_ms += 1_000;
                }
                if condition.expires_at_ms <= self.now_ms {
                    let frac = leftover_condition_fraction(condition);
                    if frac > 0.0 {
                        outgoing_damage += condition_tick_damage(
                            &condition.name,
                            condition_damage,
                            &self.params.mode,
                        ) * condition.stacks as f64
                            * self.params.condition_mult
                            * self.passive_condition_mult
                            * frac;
                    }
                }
            }
            if outgoing_damage > 0.0 {
                self.record_damage(outgoing_damage, self.control_owned());
            }

            let mut incoming_damage = 0.0;
            for condition in &mut self.incoming_conditions {
                if condition.next_tick_ms <= self.now_ms
                    && condition.next_tick_ms <= condition.expires_at_ms
                {
                    incoming_damage +=
                        condition_tick_damage(&condition.name, 1_800.0, &self.params.mode)
                            * condition.stacks as f64;
                    condition.next_tick_ms += 1_000;
                }
                if condition.expires_at_ms <= self.now_ms {
                    let frac = leftover_condition_fraction(condition);
                    if frac > 0.0 {
                        incoming_damage +=
                            condition_tick_damage(&condition.name, 1_800.0, &self.params.mode)
                                * condition.stacks as f64
                                * frac;
                    }
                }
            }
            if incoming_damage > 0.0 {
                self.absorb_damage(incoming_damage * self.incoming_condition_mult);
            }
            self.outgoing_conditions
                .retain(|condition| condition.expires_at_ms > self.now_ms);
            self.incoming_conditions
                .retain(|condition| condition.expires_at_ms > self.now_ms);
        }

    fn apply_skill_effect(&mut self, skill_id: u32, effect: &SkillEffect, protected: bool) {
        match effect {
            SkillEffect::StrikeDamage {
                hit_count,
                dmg_multiplier,
            } => {
                let might = self.buff_stacks("Might").min(25) as f64;
                let power = self.params.power
                    + might * crate::data::boon_condition_formulas::boons().might_power_per_stack();
                let fury_bonus = if self.has_buff("Fury") {
                    self.params.fury_crit_chance_bonus
                } else {
                    0.0
                };
                let mut damage = self.params.weapon_strength * power / reference_armor()
                    * dmg_multiplier
                    * *hit_count as f64
                    * strike_crit_factor_with_bonus(
                        self.params.precision,
                        self.params.ferocity,
                        self.params.crit_chance_bonus + fury_bonus,
                    )
                    * self.params.strike_mult
                    * self.passive_strike_mult;
                if self.enemy_protection {
                    damage *= 0.67;
                }
                self.record_damage(damage, protected);
                self.remove_defense(CoverKind::Stealth);
                self.gain_resource_on_hit(skill_id);
                self.trigger_procs(TriggerRule::OnHit, Some(skill_id), protected);
            }
            SkillEffect::ApplyCondition {
                condition,
                stacks,
                duration_ms,
            } => {
                let duration = (*duration_ms as f64
                    * self.params.condition_duration_mult
                    * (1.0 + self.bonus_condition_duration))
                    .round() as u32;
                self.outgoing_conditions.push(TimedCondition {
                    name: condition.clone(),
                    stacks: *stacks,
                    expires_at_ms: self.now_ms + duration,
                    next_tick_ms: self.now_ms + 1_000,
                });
            }
            SkillEffect::ApplyBuff {
                buff,
                stacks,
                duration_ms,
            } => self.apply_buff(buff, *stacks, *duration_ms, true),
            SkillEffect::ComboField {
                field_type,
                duration_ms,
            } => {
                self.combo_field = Some(ComboFieldState {
                    field_type: field_type.clone(),
                    expires_at_ms: self.now_ms.saturating_add(*duration_ms),
                });
            }
            SkillEffect::ComboFinisher {
                finisher_type,
                percent,
            } => self.resolve_combo(finisher_type, *percent),
            SkillEffect::Healing { hit_count } => {
                let amount = (1_200.0 + self.params.healing_power * 0.45)
                    * *hit_count as f64
                    * self.params.healing_mult
                    * self.passive_healing_mult;
                self.heal(amount);
            }
            SkillEffect::Barrier { amount } => {
                self.apply_barrier(amount + self.params.healing_power * 0.30);
            }
            SkillEffect::RemovesCondition { conditions_removed } => {
                self.cleanse(*conditions_removed)
            }
            SkillEffect::CrowdControl { duration_ms, .. } => {
                if !self.enemy_stability {
                    let previous_end = self.enemy_disabled_until_ms.max(self.now_ms);
                    let new_end = self.enemy_disabled_until_ms.max(self.now_ms + *duration_ms);
                    self.enemy_disabled_until_ms = new_end;
                    self.control_landed_ms += new_end.saturating_sub(previous_end);
                }
            }
            SkillEffect::StripBoons {
                count_per_pulse,
                interval_ms,
                window_ms,
            } => {
                let _ = self.remove_enemy_boons(if *interval_ms == 0 {
                    // Zero interval = one immediate pulse, not a division.
                    *count_per_pulse
                } else {
                    *count_per_pulse
                        * ((*window_ms).max(*interval_ms))
                            .checked_div(*interval_ms)
                            .unwrap_or(0)
                });
            }
            SkillEffect::CorruptBoons => {
                for boon in self.remove_enemy_boons(1) {
                    if let Some(condition) = corrupt_into(boon) {
                        self.outgoing_conditions.push(TimedCondition {
                            name: condition.into(),
                            stacks: 1,
                            expires_at_ms: self.now_ms.saturating_add(1_000),
                            next_tick_ms: self.now_ms.saturating_add(1_000),
                        });
                    }
                }
            }
            SkillEffect::StealBoons => {
                for boon in self.remove_enemy_boons(1) {
                    self.apply_buff(boon, 1, 1_000, true);
                }
            }
            SkillEffect::ConvertConditions => {
                let count = self.incoming_conditions.len() as u32;
                self.cleanse(count.max(1));
                self.apply_buff("Protection", 1, 3_000, true);
            }
            SkillEffect::Cover {
                kind,
                duration_ms,
                strippable,
            } => self.apply_defense(*kind, *duration_ms, 1, *strippable),
            // Mobility is a capability tag, not a duration source. Quantitative
            // cover must arrive as a mode-aware `Cover` fact.
            SkillEffect::Mobility { .. } => {}
        }
    }

    fn resolve_combo(&mut self, finisher_type: &str, percent: u32) {
        if percent < 100 {
            if percent > 0 {
                self.unmodeled_effect_sources += 1;
            }
            return;
        }
        let Some(field) = self.combo_field.as_ref() else {
            return;
        };
        let field_type = field.field_type.to_lowercase();
        let finisher = finisher_type.to_lowercase();
        self.combo_activations += 1;
        if field_type.contains("smoke") {
            if finisher.contains("blast") || finisher.contains("leap") {
                self.apply_defense(CoverKind::Stealth, 3_000, 1, false);
            } else if finisher.contains("projectile") || finisher.contains("whirl") {
                self.apply_defense(CoverKind::Blind, 3_000, 1, false);
            } else {
                self.unmodeled_effect_sources += 1;
            }
        } else if field_type.contains("water") {
            if finisher.contains("blast") {
                self.heal(1_320.0 + self.params.healing_power * 0.20);
            } else if finisher.contains("leap") {
                self.heal(1_300.0 + self.params.healing_power * 0.50);
            } else {
                // Projectile/whirl apply regeneration; periodic regeneration
                // is not represented by this timeline yet.
                self.unmodeled_effect_sources += 1;
            }
        } else if field_type.contains("light") {
            if finisher.contains("blast") {
                self.cleanse(1);
            } else {
                self.unmodeled_effect_sources += 1;
            }
        } else if field_type.contains("fire") {
            if finisher.contains("blast") {
                self.apply_buff("Might", 3, 20_000, true);
            } else {
                self.unmodeled_effect_sources += 1;
            }
        } else if field_type.contains("dark") {
            // Dark finishers produce auras or life-steal effects, not Blind.
            self.unmodeled_effect_sources += 1;
        } else {
            self.unmodeled_effect_sources += 1;
        }
    }

    fn apply_buff(&mut self, name: &str, stacks: u32, duration_ms: u32, scale_duration: bool) {
        let duration = if scale_duration {
            (duration_ms as f64 * self.params.boon_duration_mult * (1.0 + self.bonus_boon_duration))
                .round() as u32
        } else {
            duration_ms
        };
        self.buffs.push(TimedBuff {
            name: name.into(),
            stacks,
            expires_at_ms: self.now_ms + duration,
        });
        if let Some(kind) = boon_cover_kind(name) {
            self.apply_defense(kind, duration, stacks, true);
        }
    }

    fn apply_defense(&mut self, kind: CoverKind, duration_ms: u32, stacks: u32, strippable: bool) {
        if let Some(existing) = self
            .defenses
            .iter_mut()
            .find(|defense| defense.kind == kind)
        {
            existing.expires_at_ms = existing.expires_at_ms.max(self.now_ms + duration_ms);
            existing.stacks = existing.stacks.max(stacks);
            existing.strippable &= strippable;
        } else {
            self.defenses.push(TimedDefense {
                kind,
                expires_at_ms: self.now_ms + duration_ms,
                stacks,
                strippable,
            });
        }
    }

    fn apply_operation(
        &mut self,
        operation: Option<&crate::data::normalized_effects::StatusOperation>,
    ) {
        let Some(operation) = operation else {
            return;
        };
        let amount = resolved(&operation.amount_value)
            .copied()
            .unwrap_or(1.0)
            .max(1.0) as u32;
        let duration = operation
            .base_duration_ms
            .as_ref()
            .and_then(resolved)
            .copied()
            .unwrap_or(1_000);
        match (&operation.operation_type, &operation.target_side) {
            (OperationType::AppliesBoon, TargetSide::Self_ | TargetSide::Ally) => {
                self.apply_buff(&operation.status_kind, amount, duration, true)
            }
            (OperationType::AppliesCondition, TargetSide::Enemy) => {
                self.outgoing_conditions.push(TimedCondition {
                    name: operation.status_kind.clone(),
                    stacks: amount,
                    expires_at_ms: self.now_ms + duration,
                    next_tick_ms: self.now_ms + 1_000,
                })
            }
            (OperationType::RemovesCondition, TargetSide::Self_ | TargetSide::Ally)
            | (OperationType::ConvertsConditionToBoon, TargetSide::Self_ | TargetSide::Ally) => {
                self.cleanse(amount)
            }
            (OperationType::RemovesBoon | OperationType::CorruptsBoon, TargetSide::Enemy) => {
                self.enemy_stability = false;
                self.enemy_protection = false;
            }
            _ => {}
        }
    }

    fn skill_directly_models_effect(&self, effect: &NormalizedEffect) -> bool {
        let Some(skill) = self
            .skills
            .iter()
            .find(|skill| skill.skill_id == effect.source_id)
        else {
            return false;
        };
        match effect.category {
            EffectCategory::RemovesCondition => skill
                .effects
                .iter()
                .any(|item| matches!(item, SkillEffect::RemovesCondition { .. })),
            _ => false,
        }
    }

    fn trigger_procs(
        &mut self,
        trigger: TriggerRule,
        activating_skill_id: Option<u32>,
        protected: bool,
    ) {
        let mut ready = Vec::new();
        for (idx, proc_spec) in self.proc_specs.iter().enumerate() {
            let source_matches = !matches!(proc_spec.source_type, SourceType::Skill)
                || activating_skill_id == Some(proc_spec.source_id);
            if same_trigger(&proc_spec.trigger, &trigger)
                && source_matches
                && proc_spec.next_ready_ms <= self.now_ms
            {
                ready.push(idx);
            }
        }
        for idx in ready {
            let (category, value, duration_ms, operation, cooldown) = {
                let proc_spec = &mut self.proc_specs[idx];
                proc_spec.next_ready_ms = self.now_ms + proc_spec.internal_cooldown_ms;
                (
                    proc_spec.category.clone(),
                    proc_spec.value,
                    proc_spec.duration_ms,
                    proc_spec.operation.clone(),
                    proc_spec.internal_cooldown_ms,
                )
            };
            match category {
                EffectCategory::StrikeDamagePct => {
                    let proc_damage = self.params.weapon_strength * self.params.power
                        / reference_armor()
                        * as_ratio(value);
                    self.record_damage(proc_damage, protected);
                }
                EffectCategory::AppliesBoon
                | EffectCategory::AppliesCondition
                | EffectCategory::RemovesBoon
                | EffectCategory::CorruptsBoon
                | EffectCategory::RemovesCondition
                | EffectCategory::ConvertsConditionToBoon
                | EffectCategory::TransfersCondition => self.apply_operation(operation.as_ref()),
                EffectCategory::OutgoingHealingPct if duration_ms > 0 => self.heal(value.max(0.0)),
                _ => {
                    let _ = cooldown;
                }
            }
        }
    }

    fn track_protected_window(&mut self) {
        if self.control_owned() || self.charge_cover_consumed_this_tick {
            self.secured_tick_times.push(self.now_ms);
            self.protected_run_ms += TIMELINE_TICK_MS;
            self.longest_protected_window_ms =
                self.longest_protected_window_ms.max(self.protected_run_ms);
        } else {
            self.protected_run_ms = 0;
        }
    }

    fn control_owned(&self) -> bool {
        self.enemy_disabled_until_ms > self.now_ms
            || self.has_defense(CoverKind::Stability)
            || self.has_defense(CoverKind::Invulnerability)
            || self.has_defense(CoverKind::Evade)
            || self.has_defense(CoverKind::Stealth)
            || self.has_defense(CoverKind::Block)
    }

    fn control_cover_remaining_ms(&self) -> u32 {
        let defense = self
            .defenses
            .iter()
            .filter(|defense| is_interrupt_cover_kind(&defense.kind))
            .map(|defense| defense.expires_at_ms.saturating_sub(self.now_ms))
            .max()
            .unwrap_or(0);
        defense.max(self.enemy_disabled_until_ms.saturating_sub(self.now_ms))
    }

    fn has_defense(&self, kind: CoverKind) -> bool {
        self.defenses
            .iter()
            .any(|defense| defense.kind == kind && defense.expires_at_ms > self.now_ms)
    }

    fn consume_defense(&mut self, kind: CoverKind) -> bool {
        let Some(idx) = self
            .defenses
            .iter()
            .position(|defense| defense.kind == kind && defense.expires_at_ms > self.now_ms)
        else {
            return false;
        };
        if self.defenses[idx].stacks > 1 {
            self.defenses[idx].stacks -= 1;
        } else {
            self.defenses.remove(idx);
        }
        true
    }

    fn remove_defense(&mut self, kind: CoverKind) {
        self.defenses.retain(|defense| defense.kind != kind);
    }

    /// The game tracks recharge by skill, not by rendered bar position. The
    /// same skill equipped in both weapon sets therefore shares one timer.
    fn set_skill_cooldown(&mut self, skill_id: u32, cooldown_ms: u32) {
        let ready_ms = self.now_ms + cooldown_ms;
        for (idx, skill) in self.skills.iter().enumerate() {
            if skill.skill_id == skill_id {
                self.cooldown_ready_ms[idx] = ready_ms;
            }
        }
    }

    fn consume_stability(&mut self) -> bool {
        self.consume_defense(CoverKind::Stability)
    }

    fn has_buff(&self, name: &str) -> bool {
        self.buffs
            .iter()
            .any(|buff| buff.name.eq_ignore_ascii_case(name) && buff.expires_at_ms > self.now_ms)
    }

    fn buff_stacks(&self, name: &str) -> u32 {
        self.buffs
            .iter()
            .filter(|buff| buff.name.eq_ignore_ascii_case(name) && buff.expires_at_ms > self.now_ms)
            .map(|buff| buff.stacks)
            .sum()
    }

    fn cleanse(&mut self, count: u32) {
        let removed = count.min(self.incoming_conditions.len() as u32);
        for _ in 0..removed {
            self.incoming_conditions.pop();
        }
        self.conditions_cleansed += removed;
    }

    fn heal(&mut self, amount: f64) {
        let before = self.player_health;
        self.player_health = (self.player_health + amount).min(self.params.max_health);
        self.healing += self.player_health - before;
    }

    fn record_damage(&mut self, amount: f64, protected: bool) {
        if amount <= 0.0 || self.enemy_health <= 0.0 {
            return;
        }
        let applied = amount.min(self.enemy_health);
        self.enemy_health -= applied;
        if self.enemy_health <= 0.0 && self.target_reached_at_ms.is_none() {
            self.target_reached_at_ms = Some(self.now_ms);
        }
        self.damage_events.push(DamageEvent {
            at_ms: self.now_ms,
            amount: applied,
            protected,
        });
    }

    fn skill_damage_value(&self, skill: &RotationSkill) -> f64 {
        let mut value = 0.0;
        for effect in &skill.effects {
            match effect {
                SkillEffect::StrikeDamage {
                    hit_count,
                    dmg_multiplier,
                } => {
                    value += self.params.weapon_strength * self.params.power / reference_armor()
                        * *dmg_multiplier
                        * *hit_count as f64;
                }
                SkillEffect::ApplyCondition {
                    condition,
                    stacks,
                    duration_ms,
                } => {
                    value += condition_tick_damage(
                        condition,
                        self.params.condition_damage,
                        &self.params.mode,
                    ) * *stacks as f64
                        * (*duration_ms as f64 / 1_000.0);
                }
                _ => {}
            }
        }
        value / (skill.cast_time_ms.max(100) as f64 / 1_000.0)
    }

    fn can_pay_resource(&self, skill_id: u32) -> bool {
        let Some(rule) = self.resource_rules.get(&skill_id) else {
            return true;
        };
        self.resources.get(&rule.kind).copied().unwrap_or(0.0) >= rule.cost
    }

    fn pay_resource(&mut self, skill_id: u32) {
        let Some(rule) = self.resource_rules.get(&skill_id) else {
            return;
        };
        let resource = self.resources.entry(rule.kind).or_default();
        if *resource >= rule.cost {
            if rule.spend_all {
                *resource = 0.0;
            } else {
                *resource -= rule.cost;
            }
        }
    }

    fn gain_resource_on_hit(&mut self, skill_id: u32) {
        if let Some(rule) = self.resource_rules.get(&skill_id) {
            if rule.gain_on_hit > 0.0 {
                let resource = self.resources.entry(rule.kind).or_default();
                *resource = (*resource + rule.gain_on_hit).min(resource_cap(rule.kind));
            }
        }
        if self.resources.contains_key(&ResourceKind::Adrenaline) {
            let adrenaline = self.resources.entry(ResourceKind::Adrenaline).or_default();
            *adrenaline = (*adrenaline + 5.0).min(30.0);
        }
    }

    fn regenerate_resources(&mut self) {
        let seconds = TIMELINE_TICK_MS as f64 / 1_000.0;
        if let Some(initiative) = self.resources.get_mut(&ResourceKind::Initiative) {
            *initiative = (*initiative + seconds).min(12.0);
        }
        if let Some(energy) = self.resources.get_mut(&ResourceKind::Energy) {
            *energy = (*energy + 5.0 * seconds).min(100.0);
        }
    }

    fn report(&self) -> WvwCombatReport {
        let protected_damage: f64 = self
            .damage_events
            .iter()
            .filter(|event| event.protected)
            .map(|event| event.amount)
            .sum();
        let peak_2s = peak_damage(&self.damage_events, MIN_PROTECTED_WINDOW_MS, true);
        let peak_5s = peak_damage(&self.damage_events, self.profile.desired_window_ms, true);
        let total_damage: f64 = self.damage_events.iter().map(|event| event.amount).sum();
        let remaining_health_ratio = self.player_health / self.params.max_health.max(1.0);
        let sustain_margin = (self.healing + self.barrier_absorbed + self.avoided_damage
            - self.incoming_damage)
            / (self.profile.duration_ms as f64 / 1_000.0).max(1.0);
        let target_reached = self.profile.target_health.is_some() && self.enemy_health <= 0.0;
        let sequence = secured_sequence_summary(
            &self.secured_tick_times,
            &self.protected_actions,
            &self.damage_events,
            self.profile.desired_window_ms,
            self.profile.required_window_ms,
        );
        let chain_completed = sequence.completed;
        let cooldown_recovery = !sequence.skill_ids.is_empty()
            && sequence.skill_ids.iter().all(|skill_id| {
                self.skills
                    .iter()
                    .enumerate()
                    .filter(|(_, skill)| skill.skill_id == *skill_id)
                    .all(|(idx, _)| self.cooldown_ready_ms[idx] <= self.profile.duration_ms + 5_000)
            });
        let resource_recovery = self.sequence_resources_recovered(&sequence.skill_ids);
        let repeatable = self.player_health > 0.0
            && cooldown_recovery
            && resource_recovery
            && (target_reached || sustain_margin >= 0.0 || remaining_health_ratio >= 0.50);

        WvwCombatReport {
            duration_ms: self.profile.duration_ms,
            target_health: self.profile.target_health,
            target_reached_at_ms: self.target_reached_at_ms,
            longest_protected_window_ms: self.longest_protected_window_ms,
            protected_action_count: self.protected_action_count,
            successful_action_count: self.successful_action_count,
            interrupted_casts: self.interrupted_casts,
            protected_damage,
            peak_protected_damage_2s: peak_2s,
            peak_protected_damage_5s: peak_5s,
            total_damage,
            control_landed_ms: self.control_landed_ms,
            incoming_damage: self.incoming_damage,
            avoided_damage: self.avoided_damage,
            healing: self.healing,
            barrier_absorbed: self.barrier_absorbed,
            conditions_cleansed: self.conditions_cleansed,
            combo_activations: self.combo_activations,
            remaining_health_ratio,
            sustain_margin,
            player_survived: self.player_health > 0.0,
            target_reached,
            chain_completed,
            secured_sequence_damage: sequence.damage,
            secured_sequence_control_ms: sequence.control_ms,
            repeatable,
            resource_blocked_actions: self.resource_blocked_skills.len() as u32,
            resource_legal: self.resource_blocked_skills.is_empty(),
            resource_model_complete: self.resource_model_complete,
            unmodeled_effect_sources: self.unmodeled_effect_sources,
        }
    }

    fn sequence_resources_recovered(&self, skill_ids: &HashSet<u32>) -> bool {
        let mut required: HashMap<ResourceKind, f64> = HashMap::new();
        for skill_id in skill_ids {
            let Some(rule) = self.resource_rules.get(skill_id) else {
                continue;
            };
            let entry = required.entry(rule.kind).or_default();
            if rule.spend_all {
                *entry = (*entry).max(rule.cost);
            } else {
                *entry += rule.cost;
            }
        }
        required
            .into_iter()
            .all(|(kind, cost)| self.resources.get(&kind).copied().unwrap_or(0.0) >= cost)
    }
}

fn secured_sequence_summary(
    secured_tick_times: &[u32],
    protected_actions: &[ProtectedActionEvent],
    damage_events: &[DamageEvent],
    max_span_ms: u32,
    required_secured_ms: u32,
) -> SecuredSequenceSummary {
    let mut summary = SecuredSequenceSummary::default();
    for (left, start_ms) in secured_tick_times.iter().copied().enumerate() {
        let end_ms = start_ms + max_span_ms;
        let secured_ticks = secured_tick_times[left..]
            .iter()
            .take_while(|at_ms| **at_ms <= end_ms)
            .count() as u32;
        if secured_ticks * TIMELINE_TICK_MS < required_secured_ms {
            continue;
        }
        let actions: Vec<&ProtectedActionEvent> = protected_actions
            .iter()
            .filter(|action| action.at_ms >= start_ms && action.at_ms <= end_ms)
            .collect();
        if actions.len() < 2 {
            continue;
        }
        let damage: f64 = damage_events
            .iter()
            .filter(|event| event.protected && event.at_ms >= start_ms && event.at_ms <= end_ms)
            .map(|event| event.amount)
            .sum();
        let control_ms = actions.iter().map(|action| action.control_ms).sum();
        let applies_condition = actions.iter().any(|action| action.applies_condition);
        if damage <= 0.0 && control_ms == 0 && !applies_condition {
            continue;
        }
        if !summary.completed
            || damage > summary.damage
            || (damage == summary.damage && control_ms > summary.control_ms)
        {
            summary.completed = true;
            summary.damage = damage;
            summary.control_ms = control_ms;
            summary.skill_ids = actions.iter().map(|action| action.skill_id).collect();
        }
    }
    summary
}

fn peak_damage(events: &[DamageEvent], window_ms: u32, protected_only: bool) -> f64 {
    let mut best: f64 = 0.0;
    let mut left = 0usize;
    let mut total = 0.0;
    for right in 0..events.len() {
        if !protected_only || events[right].protected {
            total += events[right].amount;
        }
        while events[right].at_ms.saturating_sub(events[left].at_ms) > window_ms {
            if !protected_only || events[left].protected {
                total -= events[left].amount;
            }
            left += 1;
        }
        best = best.max(total);
    }
    best
}

fn is_control_cover(effect: &SkillEffect) -> bool {
    matches!(
        effect,
        SkillEffect::Cover {
            kind: CoverKind::Stability
                | CoverKind::Invulnerability
                | CoverKind::Evade
                | CoverKind::Stealth
                | CoverKind::Aegis
                | CoverKind::Blind
                | CoverKind::Block,
            ..
        } | SkillEffect::Mobility {
            kind: MobilityKind::Evade | MobilityKind::Stealth
        }
    )
}

fn is_interrupt_cover_kind(kind: &CoverKind) -> bool {
    matches!(
        kind,
        CoverKind::Stability
            | CoverKind::Invulnerability
            | CoverKind::Evade
            | CoverKind::Stealth
            | CoverKind::Block
    )
}

fn boon_cover_kind(name: &str) -> Option<CoverKind> {
    if name.eq_ignore_ascii_case("Stability") {
        Some(CoverKind::Stability)
    } else if name.eq_ignore_ascii_case("Aegis") {
        Some(CoverKind::Aegis)
    } else if name.eq_ignore_ascii_case("Protection") {
        Some(CoverKind::Protection)
    } else if name.eq_ignore_ascii_case("Resistance") {
        Some(CoverKind::Resistance)
    } else {
        None
    }
}

fn resolved<T>(value: &FactualValue<T>) -> Option<&T> {
    match value {
        FactualValue::Resolved(value) => Some(value),
        FactualValue::Unknown => None,
    }
}

fn as_ratio(value: f64) -> f64 {
    if value.abs() > 2.0 {
        value / 100.0
    } else {
        value
    }
}

fn leftover_condition_fraction(condition: &TimedCondition) -> f64 {
    let period_start = condition.next_tick_ms.saturating_sub(1_000);
    if condition.expires_at_ms <= period_start {
        return 0.0;
    }
    let remaining_ms = condition.expires_at_ms - period_start;
    if remaining_ms >= 1_000 {
        return 0.0;
    }
    remaining_ms as f64 / 1_000.0
}

fn same_trigger(left: &TriggerRule, right: &TriggerRule) -> bool {
    matches!(
        (left, right),
        (TriggerRule::Passive, TriggerRule::Passive)
            | (TriggerRule::OnCrit, TriggerRule::OnCrit)
            | (TriggerRule::OnHit, TriggerRule::OnHit)
            | (TriggerRule::OnSkillUse, TriggerRule::OnSkillUse)
            | (
                TriggerRule::OnHealthThreshold,
                TriggerRule::OnHealthThreshold
            )
            | (TriggerRule::Conditional, TriggerRule::Conditional)
    )
}

fn initial_resources(rules: &[SkillResourceRule]) -> HashMap<ResourceKind, f64> {
    let mut resources = HashMap::new();
    for rule in rules {
        resources
            .entry(rule.kind)
            .or_insert_with(|| match rule.kind {
                ResourceKind::Initiative => 12.0,
                ResourceKind::Energy => 50.0,
                ResourceKind::Adrenaline => 10.0,
                ResourceKind::Illusions | ResourceKind::Blades => 0.0,
            });
    }
    resources
}

fn resource_cap(kind: ResourceKind) -> f64 {
    match kind {
        ResourceKind::Initiative => 12.0,
        ResourceKind::Energy => 100.0,
        ResourceKind::Adrenaline => 30.0,
        ResourceKind::Illusions => 3.0,
        ResourceKind::Blades => 5.0,
    }
}

fn condition_is_damaging(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "bleeding" | "burning" | "confusion" | "poison" | "torment"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::CombatPerformance;
    use crate::referee::evaluate_viability_gates;
    use crate::rotation::{ControlKind, SkillSlot};
    use crate::scenario::{OptimizationTarget, TargetProfile};
    use gw2_core::types::GameMode;

    fn skill(
        skill_id: u32,
        slot: SkillSlot,
        cast_time_ms: u32,
        cooldown_ms: u32,
        effects: Vec<SkillEffect>,
    ) -> RotationSkill {
        RotationSkill {
            skill_id,
            name: format!("test-{skill_id}"),
            slot,
            cast_time_ms,
            cooldown_ms,
            effects,
            next_chain: None,
            is_stunbreak: false,
            weapon_set: 0,
        }
    }

    fn params() -> SimParams {
        let mut params = SimParams::basic(2_000.0, 1_500.0, 1_100.0);
        params.max_health = 20_000.0;
        params.armor = 2_500.0;
        params.mode = GameMode::WvW;
        params
    }

    fn profile(duration_ms: u32, events: Vec<EnemyEvent>) -> WvwProfile {
        WvwProfile {
            duration_ms,
            target_health: Some(18_000.0),
            enemy_events: events.into(),
            required_window_ms: MIN_PROTECTED_WINDOW_MS,
            desired_window_ms: TARGET_PROTECTED_WINDOW_MS.min(duration_ms),
        }
    }

    fn run_report(
        skills: &[RotationSkill],
        rules: &[SkillResourceRule],
        enemy: EnemyDummy,
        profile: WvwProfile,
        params: &SimParams,
    ) -> WvwCombatReport {
        let mut timeline = Timeline::new(skills, params, profile, enemy, &[], rules, true, 0);
        timeline.run();
        timeline.report()
    }

    fn open_enemy(stability: bool) -> EnemyDummy {
        EnemyDummy {
            protection: false,
            stability,
            hp: Some(18_000.0),
        }
    }

    fn rule(
        skill_id: u32,
        kind: ResourceKind,
        cost: f64,
        gain_on_hit: f64,
        spend_all: bool,
    ) -> SkillResourceRule {
        SkillResourceRule {
            skill_id,
            kind,
            cost,
            gain_on_hit,
            spend_all,
        }
    }

    #[test]
    fn short_block_does_not_complete_minimum_window() {
        let skills = vec![
            skill(
                1,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Block,
                    duration_ms: 1_000,
                    strippable: false,
                }],
            ),
            skill(
                2,
                SkillSlot::Weapon2,
                200,
                250,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 4.0,
                }],
            ),
        ];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(3_000, vec![]),
            &params,
        );

        assert!(report.longest_protected_window_ms < MIN_PROTECTED_WINDOW_MS);
        assert!(!report.chain_completed);
    }

    #[test]
    fn charge_cover_only_preserves_the_consumed_event_tick() {
        let skills = vec![skill(
            1,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::Cover {
                kind: CoverKind::Aegis,
                duration_ms: 5_000,
                strippable: true,
            }],
        )];
        let events = vec![EnemyEvent {
            at_ms: 450,
            kind: EnemyEventKind::Strike {
                damage: 2_000.0,
                unblockable: false,
            },
        }];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(1_000, events),
            &params,
        );

        assert_eq!(report.longest_protected_window_ms, TIMELINE_TICK_MS);
        assert!(!report.chain_completed);
        assert_eq!(report.incoming_damage, 0.0);
    }

    #[test]
    fn target_stability_requires_strip_before_control() {
        let control = skill(
            2,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::CrowdControl {
                kind: ControlKind::Stun,
                duration_ms: 1_000,
                stops_dodge: true,
            }],
        );
        let params = params();
        let blocked = run_report(
            std::slice::from_ref(&control),
            &[],
            open_enemy(true),
            profile(1_000, vec![]),
            &params,
        );
        assert_eq!(blocked.control_landed_ms, 0);

        let strip = skill(
            1,
            SkillSlot::Utility,
            50,
            10_000,
            vec![SkillEffect::StripBoons {
                count_per_pulse: 1,
                interval_ms: 0,
                window_ms: 0,
            }],
        );
        let opened = run_report(
            &[strip, control],
            &[],
            open_enemy(true),
            profile(1_000, vec![]),
            &params,
        );
        assert_eq!(opened.control_landed_ms, 1_000);
    }

    #[test]
    fn incoming_control_cancels_a_pending_cast() {
        let skills = vec![skill(
            1,
            SkillSlot::Weapon2,
            1_000,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 8.0,
            }],
        )];
        let events = vec![EnemyEvent {
            at_ms: 450,
            kind: EnemyEventKind::Control {
                duration_ms: 900,
                unblockable: false,
            },
        }];
        let params = params();
        let report = run_report(
            &skills,
            &[],
            open_enemy(false),
            profile(2_000, events),
            &params,
        );

        assert_eq!(report.interrupted_casts, 1);
        assert_eq!(report.total_damage, 0.0);
    }

    #[test]
    fn recovery_kit_outlasts_the_same_pressure_script() {
        let events = vec![
            EnemyEvent {
                at_ms: 400,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
            EnemyEvent {
                at_ms: 900,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
            EnemyEvent {
                at_ms: 1_400,
                kind: EnemyEventKind::Strike {
                    damage: 7_000.0,
                    unblockable: false,
                },
            },
        ];
        let params = params();
        let exposed = run_report(
            &[],
            &[],
            open_enemy(false),
            profile(2_000, events.clone()),
            &params,
        );
        assert!(!exposed.player_survived);

        let recovery = vec![
            skill(
                1,
                SkillSlot::Utility,
                50,
                600,
                vec![SkillEffect::Barrier { amount: 8_000.0 }],
            ),
            skill(
                2,
                SkillSlot::Heal,
                50,
                600,
                vec![
                    SkillEffect::Healing { hit_count: 2 },
                    SkillEffect::RemovesCondition {
                        conditions_removed: 2,
                    },
                ],
            ),
        ];
        let recovered = run_report(
            &recovery,
            &[],
            open_enemy(false),
            profile(2_000, events),
            &params,
        );
        assert!(recovered.player_survived);
        assert!(recovered.sustain_margin > 0.0);
    }

    fn published_fixture_profile() -> WvwProfile {
        profile(
            5_000,
            vec![
                EnemyEvent {
                    at_ms: 450,
                    kind: EnemyEventKind::Control {
                        duration_ms: 900,
                        unblockable: false,
                    },
                },
                EnemyEvent {
                    at_ms: 850,
                    kind: EnemyEventKind::Strike {
                        damage: 3_000.0,
                        unblockable: false,
                    },
                },
                EnemyEvent {
                    at_ms: 1_650,
                    kind: EnemyEventKind::Condition {
                        condition: "Bleeding".into(),
                        stacks: 2,
                        duration_ms: 3_000,
                    },
                },
                EnemyEvent {
                    at_ms: 2_350,
                    kind: EnemyEventKind::BoonStrip { count: 1 },
                },
                EnemyEvent {
                    at_ms: 3_050,
                    kind: EnemyEventKind::Control {
                        duration_ms: 1_100,
                        unblockable: false,
                    },
                },
            ],
        )
    }

    fn paper_pressure_sibling(skills: &[RotationSkill]) -> Vec<RotationSkill> {
        skills
            .iter()
            .cloned()
            .filter_map(|mut skill| {
                skill.effects.retain(|effect| {
                    !matches!(
                        effect,
                        SkillEffect::Cover { .. }
                            | SkillEffect::Mobility { .. }
                            | SkillEffect::CrowdControl { .. }
                    )
                });
                (!skill.effects.is_empty()).then_some(skill)
            })
            .collect()
    }

    fn assert_published_fixture(
        label: &str,
        skills: Vec<RotationSkill>,
        rules: Vec<SkillResourceRule>,
        target_starts_stable: bool,
    ) {
        let params = params();
        let report = run_report(
            &skills,
            &rules,
            open_enemy(target_starts_stable),
            published_fixture_profile(),
            &params,
        );
        let paper = paper_pressure_sibling(&skills);
        let paper_report = run_report(
            &paper,
            &rules,
            open_enemy(target_starts_stable),
            published_fixture_profile(),
            &params,
        );

        assert!(
            report.chain_completed,
            "{label} should complete its secured sequence: {report:?}"
        );
        assert!(
            report.resource_legal,
            "{label} should obey its resource ledger"
        );
        assert!(
            report.protected_damage > paper_report.protected_damage,
            "{label} should outperform its unprotected pressure sibling"
        );

        let rotation = super::super::SimulationResult {
            duration_ms: report.duration_ms,
            strike_dps: report.total_damage / 5.0,
            condition_dps: 0.0,
            total_dps: report.total_damage / 5.0,
            condition_uptime: HashMap::new(),
            buff_uptime: HashMap::new(),
            skill_usage: Vec::new(),
            stunbreak_count: 1,
            has_stability: false,
            stability_uptime: 0.0,
            cleanse_count: 1,
            cleanse_rate_per_20s: 4.0,
            has_mobility_out: true,
            escape_kinds: 1,
            has_strip: true,
            has_corrupt: false,
            downed: report.target_reached,
            finished: report.target_reached,
            has_interrupt: true,
            has_cover_answer: true,
            wvw: Some(report),
        };
        let combat = CombatPerformance {
            effective_health: 20_000.0,
            ..CombatPerformance::default()
        };
        let scenario = ScenarioSpec {
            game_mode: GameMode::WvW,
            combat_tier: CombatTier::Solo,
            combat_kind: CombatKind::StrikeSpike,
            target_profile: TargetProfile::Single,
            optimization_target: OptimizationTarget {
                label: label.into(),
            },
            patch_id: None,
            objective_profile_id: None,
        };
        let viability = evaluate_viability_gates(Some(&rotation), &combat, &scenario);
        assert!(
            viability.is_viable,
            "{label} should pass the WvW gates: {:?}",
            viability.gates
        );
    }

    #[test]
    fn mobility_label_without_timed_cover_does_not_complete_sequence() {
        // A mobility tag says what a skill can do, not how long it protects an
        // action. The sourced D/P fixture belongs after its explicit WvW facts
        // and instant-during-cast ordering are represented.
        let skills = vec![
            skill(
                100,
                SkillSlot::Profession,
                50,
                3_000,
                vec![
                    SkillEffect::StealBoons,
                    SkillEffect::CrowdControl {
                        kind: ControlKind::Daze,
                        duration_ms: 750,
                        stops_dodge: false,
                    },
                ],
            ),
            skill(
                101,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Mobility {
                    kind: MobilityKind::Stealth,
                }],
            ),
            skill(
                102,
                SkillSlot::Weapon2,
                200,
                1_200,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
        ];
        let report = run_report(
            &skills,
            &[rule(102, ResourceKind::Initiative, 3.0, 0.0, false)],
            open_enemy(false),
            published_fixture_profile(),
            &params(),
        );
        assert!(!report.chain_completed);
    }

    #[test]
    fn published_spellbreaker_fixture_beats_unprotected_pressure() {
        // MetaBattle Magebane/Spearbreaker roamers: remove target cover, then
        // combine Full Counter's block/control with weapon pressure.
        let skills = vec![
            skill(
                200,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::StripBoons {
                    count_per_pulse: 2,
                    interval_ms: 0,
                    window_ms: 0,
                }],
            ),
            skill(
                201,
                SkillSlot::Profession,
                50,
                5_000,
                vec![
                    SkillEffect::Cover {
                        kind: CoverKind::Block,
                        duration_ms: 2_500,
                        strippable: false,
                    },
                    SkillEffect::CrowdControl {
                        kind: ControlKind::Stun,
                        duration_ms: 1_000,
                        stops_dodge: true,
                    },
                    SkillEffect::StrikeDamage {
                        hit_count: 1,
                        dmg_multiplier: 8.0,
                    },
                ],
            ),
            skill(
                202,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
        ];
        assert_published_fixture(
            "Spellbreaker",
            skills,
            vec![rule(201, ResourceKind::Adrenaline, 10.0, 0.0, false)],
            true,
        );
    }

    #[test]
    fn published_mirage_fixture_beats_unprotected_pressure() {
        // MetaBattle Shatter/Celestial Mirage roamers: generate an illusion,
        // use Distortion as duration cover, then apply the weapon sequence.
        let skills = vec![
            skill(
                300,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
            skill(
                301,
                SkillSlot::Profession,
                50,
                5_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Invulnerability,
                    duration_ms: 2_500,
                    strippable: false,
                }],
            ),
        ];
        assert_published_fixture(
            "Mirage",
            skills,
            vec![
                rule(300, ResourceKind::Illusions, 0.0, 1.0, false),
                rule(301, ResourceKind::Illusions, 1.0, 0.0, true),
            ],
            false,
        );
    }

    #[test]
    fn published_virtuoso_fixture_beats_unprotected_pressure() {
        // MetaBattle Power Speartuoso: invulnerability provides the opening;
        // a blade builder makes the bladesong legal before the pressure lands.
        let skills = vec![
            skill(
                400,
                SkillSlot::Utility,
                50,
                10_000,
                vec![SkillEffect::Cover {
                    kind: CoverKind::Invulnerability,
                    duration_ms: 2_500,
                    strippable: false,
                }],
            ),
            skill(
                401,
                SkillSlot::Weapon2,
                200,
                700,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            ),
            skill(
                402,
                SkillSlot::Profession,
                200,
                5_000,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 10.0,
                }],
            ),
        ];
        assert_published_fixture(
            "Virtuoso",
            skills,
            vec![
                rule(401, ResourceKind::Blades, 0.0, 1.0, false),
                rule(402, ResourceKind::Blades, 1.0, 0.0, true),
            ],
            false,
        );
    }

    #[test]
    fn initiative_stops_after_the_available_pool_is_spent() {
        let rules = [rule(1, ResourceKind::Initiative, 4.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        for _ in 0..3 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
    }

    #[test]
    fn exact_duration_conditions_receive_their_final_tick() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(5_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.outgoing_conditions.push(TimedCondition {
            name: "Bleeding".into(),
            stacks: 1,
            expires_at_ms: 4_000,
            next_tick_ms: 1_000,
        });
        let one_tick = condition_tick_damage("Bleeding", params.condition_damage, &params.mode);
        for second in 1..=4 {
            timeline.now_ms = second * 1_000;
            timeline.tick_conditions();
        }

        let total: f64 = timeline
            .damage_events
            .iter()
            .map(|event| event.amount)
            .sum();
        assert!((total - one_tick * 4.0).abs() < 0.001);
        assert!(timeline.outgoing_conditions.is_empty());
    }

    #[test]
    fn resistance_does_not_remove_damaging_conditions() {
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.apply_defense(CoverKind::Resistance, 2_000, 1, false);
        timeline.receive_condition("Burning".into(), 1, 1_000);
        timeline.receive_condition("Crippled".into(), 1, 1_000);

        assert_eq!(timeline.incoming_conditions.len(), 1);
        assert_eq!(timeline.incoming_conditions[0].name, "Burning");
    }

    #[test]
    fn reactive_stunbreak_must_pay_its_resource_cost() {
        let mut stunbreak = skill(1, SkillSlot::Utility, 0, 10_000, vec![]);
        stunbreak.is_stunbreak = true;
        let skills = [stunbreak];
        let rules = [rule(1, ResourceKind::Energy, 60.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        timeline.disabled_until_ms = 1_000;
        timeline.try_stunbreak();

        assert_eq!(timeline.disabled_until_ms, 1_000);
        assert_eq!(timeline.resources[&ResourceKind::Energy], 50.0);
        assert!(timeline.resource_blocked_skills.contains(&1));
    }

    #[test]
    fn skill_owned_proc_only_runs_for_its_source_skill() {
        let data = crate::data::normalized_effects::effects();
        let effect = data
            .effects_for_mode("WvW")
            .iter()
            .find(|effect| effect.source_id == 9120)
            .expect("Virtue of Resolve normalized effect");
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[effect],
            &[],
            true,
            0,
        );
        timeline.incoming_conditions.push(TimedCondition {
            name: "Burning".into(),
            stacks: 1,
            expires_at_ms: 2_000,
            next_tick_ms: 1_000,
        });

        timeline.trigger_procs(TriggerRule::OnSkillUse, Some(1), false);
        assert_eq!(timeline.incoming_conditions.len(), 1);
        timeline.trigger_procs(TriggerRule::OnSkillUse, Some(9120), false);
        assert!(timeline.incoming_conditions.is_empty());
    }

    #[test]
    fn unsupported_normalized_trigger_degrades_coverage() {
        let data = crate::data::normalized_effects::effects();
        let effect = data
            .effects_for_mode("WvW")
            .iter()
            .find(|effect| matches!(effect.trigger_rule, TriggerRule::OnCrit))
            .expect("OnCrit normalized effect");
        let params = params();
        let timeline = Timeline::new(
            &[],
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[effect],
            &[],
            true,
            0,
        );

        assert_eq!(timeline.unmodeled_effect_sources, 1);
        assert!(timeline.proc_specs.is_empty());
    }

    #[test]
    fn energy_respects_its_cap_and_cost() {
        let rules = [rule(1, ResourceKind::Energy, 25.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        for _ in 0..2 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
        timeline.resources.insert(ResourceKind::Energy, 99.9);
        timeline.regenerate_resources();
        assert_eq!(timeline.resources[&ResourceKind::Energy], 100.0);
    }

    #[test]
    fn adrenaline_action_waits_for_landed_hits() {
        let rules = [rule(1, ResourceKind::Adrenaline, 10.0, 0.0, false)];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        timeline.resources.insert(ResourceKind::Adrenaline, 0.0);
        assert!(!timeline.can_pay_resource(1));
        timeline.gain_resource_on_hit(99);
        timeline.gain_resource_on_hit(99);
        assert!(timeline.can_pay_resource(1));
    }

    #[test]
    fn illusion_action_spends_the_current_stack() {
        let rules = [
            rule(1, ResourceKind::Illusions, 0.0, 1.0, false),
            rule(2, ResourceKind::Illusions, 1.0, 0.0, true),
        ];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        assert!(!timeline.can_pay_resource(2));
        timeline.gain_resource_on_hit(1);
        assert!(timeline.can_pay_resource(2));
        timeline.pay_resource(2);
        assert!(!timeline.can_pay_resource(2));
    }

    #[test]
    fn blade_action_spends_the_current_stack() {
        let rules = [
            rule(1, ResourceKind::Blades, 0.0, 1.0, false),
            rule(2, ResourceKind::Blades, 1.0, 0.0, true),
        ];
        let params = params();
        let mut timeline = Timeline::new(
            &[],
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &rules,
            true,
            0,
        );
        assert!(!timeline.can_pay_resource(2));
        timeline.gain_resource_on_hit(1);
        assert!(timeline.can_pay_resource(2));
        timeline.pay_resource(2);
        assert!(!timeline.can_pay_resource(2));
    }

    #[test]
    fn duplicate_skill_ids_share_one_recharge_timer() {
        let mut first = skill(42, SkillSlot::Weapon2, 100, 5_000, vec![]);
        first.weapon_set = 1;
        let mut second = first.clone();
        second.weapon_set = 2;
        let skills = [first, second];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(1_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );

        timeline.set_skill_cooldown(42, 5_000);

        assert_eq!(timeline.cooldown_ready_ms, vec![5_000, 5_000]);
    }

    #[test]
    fn profession_swap_policy_controls_the_timeline_timer() {
        let mut first = skill(1, SkillSlot::Weapon2, 100, 1_000, vec![]);
        first.weapon_set = 1;
        let mut second = skill(2, SkillSlot::Weapon2, 100, 1_000, vec![]);
        second.weapon_set = 2;
        let skills = [first, second];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(6_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.weapon_swap_cooldown_ms = Some(5_000);

        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 2);
        assert_eq!(timeline.weapon_swap_ready_ms, 5_000);
        timeline.now_ms = 4_999;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 2);
        timeline.now_ms = 5_000;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 1);

        timeline.weapon_swap_cooldown_ms = None;
        timeline.now_ms = 10_000;
        timeline.try_weapon_swap();
        assert_eq!(timeline.active_weapon_set, 1);
    }

    #[test]
    fn stealth_breaks_on_landed_strike_but_is_not_full_immunity() {
        let strike = skill(
            1,
            SkillSlot::Weapon2,
            100,
            1_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        );
        let skills = [strike];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(2_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.apply_defense(CoverKind::Stealth, 3_000, 1, false);

        timeline.receive_strike(1_000.0, false);
        assert!(timeline.incoming_damage > 0.0);
        assert!(timeline.has_defense(CoverKind::Stealth));

        timeline.apply_skill_effect(1, &skills[0].effects[0], true);
        assert!(!timeline.has_defense(CoverKind::Stealth));
    }

    #[test]
    fn alacrity_shortens_recharge_to_eighty_percent() {
        // Wiki Alacrity (2026-08-29): 10s CD recharges in 8s while Alacrity is up.
        let skills = [skill(
            1,
            SkillSlot::Utility,
            200,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(20_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.apply_buff("Alacrity", 1, 30_000, false);
        timeline.set_skill_cooldown(1, 10_000);
        assert_eq!(
            timeline.cooldown_ready_ms[0], 10_000,
            "store full CD; dummy clock consumes 1.25x per 100ms wall"
        );
        for _ in 0..160 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_alacrity_recharge();
        }
        assert!(
            timeline.cooldown_ready_ms[0] <= timeline.now_ms,
            "10s CD ready after 8s wall with Alacrity, ready={} now={}",
            timeline.cooldown_ready_ms[0],
            timeline.now_ms
        );
    }

    #[test]
    fn alacrity_mid_cooldown_still_uses_dummy_clock() {
        let skills = [skill(
            1,
            SkillSlot::Utility,
            200,
            10_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(20_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.set_skill_cooldown(1, 10_000);
        for _ in 0..40 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_alacrity_recharge();
        }
        assert_eq!(timeline.cooldown_ready_ms[0], 10_000);
        timeline.apply_buff("Alacrity", 1, 30_000, false);
        for _ in 0..128 {
            timeline.now_ms += TIMELINE_TICK_MS;
            timeline.tick_alacrity_recharge();
        }
        assert!(
            timeline.cooldown_ready_ms[0] <= timeline.now_ms,
            "Alacrity after 2s must still eat the remaining 8s in 6.4s wall"
        );
    }

    #[test]
    fn confusion_on_skill_use_is_not_the_one_second_pulse() {
        // Wiki Confusion (2026-08-29): DoT each second AND extra on skill activation.
        let skills = [skill(
            1,
            SkillSlot::Weapon2,
            200,
            5_000,
            vec![SkillEffect::StrikeDamage {
                hit_count: 1,
                dmg_multiplier: 1.0,
            }],
        )];
        let params = params();
        let mut timeline = Timeline::new(
            &skills,
            &params,
            profile(4_000, vec![]),
            open_enemy(false),
            &[],
            &[],
            true,
            0,
        );
        timeline.receive_condition("Confusion".into(), 1, 10_000);
        let before = timeline.incoming_damage;
        timeline.start_cast(0);
        let on_use = crate::data::conditions().confusion_tick(1_800.0, GameMode::WvW, true);
        assert!(
            (timeline.incoming_damage - before - on_use).abs() < 0.01,
            "start_cast must apply on-skill-use Confusion, expected {on_use}, got {}",
            timeline.incoming_damage - before
        );
        let after_cast = timeline.incoming_damage;
        timeline.now_ms = 1_000;
        timeline.tick_conditions();
        let pulse = timeline.incoming_damage - after_cast;
        let dot = crate::data::conditions().confusion_tick(1_800.0, GameMode::WvW, false);
        assert!(
            (pulse - dot).abs() < 0.01,
            "1s pulse must be DoT {dot}, not on-skill-use {on_use}; got {pulse}"
        );
    }

    #[test]
        fn barrier_expires_after_five_seconds_and_caps_at_quarter_health() {
            let params = params();
            let mut timeline = Timeline::new(
                &[],
                &params,
                profile(10_000, vec![]),
                open_enemy(false),
                &[],
                &[],
                true,
                0,
            );
            let cap = params.max_health * WVW_BARRIER_HEALTH_FRACTION;
            timeline.apply_barrier(params.max_health);
            let total: f64 = timeline.barrier.iter().map(|layer| layer.amount).sum();
            assert!((total - cap).abs() < 0.001, "cap {cap}, got {total}");
            timeline.apply_barrier(10_000.0);
            let total: f64 = timeline.barrier.iter().map(|layer| layer.amount).sum();
            assert!(total <= cap + 0.001);

            timeline.now_ms = 8_000;
            timeline.expire_timed_state();
            let absorbed_before = timeline.barrier_absorbed;
            let incoming_before = timeline.incoming_damage;
            timeline.absorb_damage(1_000.0);
            assert_eq!(timeline.barrier_absorbed, absorbed_before);
            assert!((timeline.incoming_damage - incoming_before - 1_000.0).abs() < 0.001);
        }

        #[test]
        fn interrupt_sets_five_second_cooldown() {
            let skills = vec![skill(
                1,
                SkillSlot::Weapon2,
                1_000,
                30_000,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 1.0,
                }],
            )];
            let params = params();
            let mut timeline = Timeline::new(
                &skills,
                &params,
                profile(2_000, vec![]),
                open_enemy(false),
                &[],
                &[],
                true,
                0,
            );
            timeline.start_cast(0);
            assert_eq!(timeline.cooldown_ready_ms[0], 30_000);
            timeline.now_ms = 200;
            timeline.receive_control(900, false);
            assert_eq!(timeline.interrupted_casts, 1);
            assert!(timeline.pending.is_none());
            assert_eq!(timeline.cooldown_ready_ms[0], 5_200);
        }

        #[test]
        fn killed_dummy_stops_incoming_after_one_second() {
            let skills = vec![skill(
                1,
                SkillSlot::Weapon2,
                50,
                1_000,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 8.0,
                }],
            )];
            let events = vec![
                EnemyEvent {
                    at_ms: 2_000,
                    kind: EnemyEventKind::Strike {
                        damage: 3_000.0,
                        unblockable: true,
                    },
                },
                EnemyEvent {
                    at_ms: 2_500,
                    kind: EnemyEventKind::Control {
                        duration_ms: 1_100,
                        unblockable: true,
                    },
                },
                EnemyEvent {
                    at_ms: 3_000,
                    kind: EnemyEventKind::Condition {
                        condition: "Bleeding".into(),
                        stacks: 5,
                        duration_ms: 4_000,
                    },
                },
                EnemyEvent {
                    at_ms: 3_500,
                    kind: EnemyEventKind::BoonStrip { count: 1 },
                },
            ];
            let mut fight = profile(5_000, events);
            fight.target_health = Some(1.0);
            let params = params();
            let report = run_report(&skills, &[], open_enemy(false), fight, &params);
            assert!(report.target_reached);
            assert!(report.target_reached_at_ms.expect("kill time") <= 1_000);
            assert_eq!(report.incoming_damage, 0.0);
            assert_eq!(report.interrupted_casts, 0);
        }

        #[test]
        fn fractional_condition_pays_half_tick_on_expiry() {
            let params = params();
            let mut timeline = Timeline::new(
                &[],
                &params,
                profile(5_000, vec![]),
                open_enemy(false),
                &[],
                &[],
                true,
                0,
            );
            timeline.outgoing_conditions.push(TimedCondition {
                name: "Bleeding".into(),
                stacks: 1,
                expires_at_ms: 1_500,
                next_tick_ms: 1_000,
            });
            let one_tick = condition_tick_damage("Bleeding", params.condition_damage, &params.mode);
            timeline.now_ms = 1_000;
            timeline.tick_conditions();
            timeline.now_ms = 1_500;
            timeline.tick_conditions();
            let total: f64 = timeline.damage_events.iter().map(|event| event.amount).sum();
            assert!(
                (total - one_tick * 1.5).abs() < 0.001,
                "expected 1.5 ticks ({}) got {total}",
                one_tick * 1.5
            );
            assert!(timeline.outgoing_conditions.is_empty());
        }

        #[test]
        fn no_outcome_target_never_reaches() {
            let params = params();
            let enemy = EnemyDummy {
                protection: false,
                stability: false,
                hp: None,
            };
            let scenario = ScenarioSpec {
                game_mode: GameMode::WvW,
                combat_tier: CombatTier::Squad,
                combat_kind: CombatKind::StrikeSpike,
                target_profile: TargetProfile::Single,
                optimization_target: OptimizationTarget {
                    label: "no-target".into(),
                },
                patch_id: None,
                objective_profile_id: None,
            };
            let built = WvwProfile::for_scenario(&scenario, &enemy, &params, 5_000);
            assert!(built.target_health.is_none());
            let skills = vec![skill(
                1,
                SkillSlot::Weapon2,
                50,
                200,
                vec![SkillEffect::StrikeDamage {
                    hit_count: 1,
                    dmg_multiplier: 20.0,
                }],
            )];
            let report = run_report(&skills, &[], enemy, built, &params);
            assert!(!report.target_reached);
            assert!(report.target_reached_at_ms.is_none());
            assert!(report.total_damage > 0.0);
        }

        #[test]
        fn unknown_strike_pct_is_unmodeled_not_one_percent() {
            use crate::data::normalized_effects::{StackingRule, UptimeModel, UptimeModelKind};
            use crate::data::EvidenceLevel;
            let effect = NormalizedEffect {
                effect_id: "test-unknown-strike".into(),
                source_type: SourceType::Relic,
                source_id: 1,
                source_name: "test".into(),
                category: EffectCategory::StrikeDamagePct,
                value: FactualValue::Unknown,
                stacking_rule: StackingRule::NonStacking,
                trigger_rule: TriggerRule::OnHit,
                uptime_model: UptimeModel {
                    kind: UptimeModelKind::Unknown,
                    uptime: None,
                },
                evidence_level: EvidenceLevel::Unknown,
                source: None,
                effect_duration: None,
                internal_cooldown: None,
                max_stacks: None,
                status_operation: None,
                inner_category: None,
            };
            let params = params();
            let timeline = Timeline::new(
                &[],
                &params,
                profile(1_000, vec![]),
                open_enemy(false),
                &[&effect],
                &[],
                true,
                0,
            );
            assert_eq!(timeline.unmodeled_effect_sources, 1);
            assert!(timeline.proc_specs.is_empty());
        }

        #[test]
        fn corrupt_maps_condition_steal_grants_boon() {
            let params = params();
            let mut timeline = Timeline::new(
                &[],
                &params,
                profile(2_000, vec![]),
                open_enemy(true),
                &[],
                &[],
                true,
                0,
            );
            timeline.enemy_protection = true;
            timeline.apply_skill_effect(1, &SkillEffect::CorruptBoons, false);
            assert!(!timeline.enemy_stability);
            assert_eq!(timeline.outgoing_conditions[0].name, "Fear");
            timeline.apply_skill_effect(1, &SkillEffect::StealBoons, false);
            assert!(!timeline.enemy_protection);
            assert!(timeline.has_buff("Protection"));
            let conditions = timeline.outgoing_conditions.len();
            let buffs = timeline.buffs.len();
            timeline.apply_skill_effect(1, &SkillEffect::CorruptBoons, false);
            timeline.apply_skill_effect(1, &SkillEffect::StealBoons, false);
            assert_eq!(timeline.outgoing_conditions.len(), conditions);
            assert_eq!(timeline.buffs.len(), buffs);
        }
}
