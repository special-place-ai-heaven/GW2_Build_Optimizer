//! Counterplay-aware WvW combat timeline.
//!
//! The legacy rotation simulator answers "what is the average damage of this
//! skill roster against a dummy?"  This module answers the WvW question: "can
//! the build establish control of a real exchange long enough to finish its
//! chain, survive the answer, recover, and do it again?"

use std::collections::{HashMap, HashSet, VecDeque};

use crate::data::normalized_effects::{
    EffectCategory, NormalizedEffect, OperationType, TargetSide, TriggerRule,
};
use crate::data::quality::FactualValue;
use crate::scenario::{CombatKind, CombatTier, ScenarioSpec};

use super::combat_model::EnemyDummy;
use super::simulator::{
    condition_tick_damage, reference_armor, strike_crit_factor_with_bonus, SimParams,
};
use super::{CoverKind, MobilityKind, RotationSkill, SkillEffect, SkillSlot};

const TIMELINE_TICK_MS: u32 = 50;
pub const MIN_PROTECTED_WINDOW_MS: u32 = 2_000;
pub const TARGET_PROTECTED_WINDOW_MS: u32 = 5_000;

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
    pub target_health: f64,
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
    pub repeatable: bool,
    pub resource_blocked_actions: u32,
    pub resource_legal: bool,
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
    pub unmodeled_effect_sources: u32,
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
    target_health: f64,
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
            target_health: enemy.hp.unwrap_or(match scenario.combat_tier {
                CombatTier::Solo => 18_000.0,
                CombatTier::Party => 24_000.0,
                CombatTier::Squad => 35_000.0,
            }),
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
struct ProcSpec {
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
    barrier: f64,
    protected_run_ms: u32,
    longest_protected_window_ms: u32,
    charge_cover_consumed_this_tick: bool,
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
        unmodeled_effect_sources,
    } = input;
    let profile = WvwProfile::for_scenario(scenario, &enemy, params, duration_ms);
    let mut timeline = Timeline::new(
        skills,
        params,
        profile,
        enemy,
        active_effects,
        resource_rules,
        unmodeled_effect_sources,
    );
    timeline.run();
    timeline.report()
}

impl<'a> Timeline<'a> {
    fn new(
        skills: &'a [RotationSkill],
        params: &'a SimParams,
        profile: WvwProfile,
        enemy: EnemyDummy,
        active_effects: &[&NormalizedEffect],
        resource_rules: &[SkillResourceRule],
        unmodeled_effect_sources: u32,
    ) -> Self {
        let mut state = Self {
            skills,
            params,
            enemy_health: profile.target_health,
            player_health: params.max_health,
            enemy_protection: enemy.protection,
            enemy_stability: enemy.stability,
            profile,
            now_ms: 0,
            next_action_ms: 0,
            disabled_until_ms: 0,
            active_weapon_set: 1,
            weapon_swap_ready_ms: 0,
            cooldown_ready_ms: vec![0; skills.len()],
            pending: None,
            defenses: Vec::new(),
            buffs: Vec::new(),
            outgoing_conditions: Vec::new(),
            incoming_conditions: Vec::new(),
            combo_field: None,
            enemy_disabled_until_ms: 0,
            target_reached_at_ms: None,
            barrier: 0.0,
            protected_run_ms: 0,
            longest_protected_window_ms: 0,
            charge_cover_consumed_this_tick: false,
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
        };
        state.load_normalized_effects(active_effects);
        state
    }

    fn load_normalized_effects(&mut self, effects: &[&NormalizedEffect]) {
        for effect in effects {
            let value = resolved(&effect.value).copied().unwrap_or(0.0);
            let ratio = as_ratio(value);
            if matches!(effect.trigger_rule, TriggerRule::Passive) {
                match effect.category {
                    EffectCategory::StrikeDamagePct => self.passive_strike_mult *= 1.0 + ratio,
                    EffectCategory::ConditionDamagePct => {
                        self.passive_condition_mult *= 1.0 + ratio
                    }
                    EffectCategory::OutgoingHealingPct => self.passive_healing_mult *= 1.0 + ratio,
                    EffectCategory::IncomingStrikeMultiplier => {
                        self.incoming_strike_mult *= value.max(0.0)
                    }
                    EffectCategory::IncomingConditionMultiplier => {
                        self.incoming_condition_mult *= value.max(0.0)
                    }
                    EffectCategory::BoonDurationPct => self.bonus_boon_duration += ratio,
                    EffectCategory::ConditionDurationPct
                    | EffectCategory::SpecificConditionDurationPct => {
                        self.bonus_condition_duration += ratio
                    }
                    _ => self.apply_operation(effect.status_operation.as_ref()),
                }
                continue;
            }

            self.proc_specs.push(ProcSpec {
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
        }
    }

    fn expire_timed_state(&mut self) {
        self.defenses
            .retain(|defense| defense.expires_at_ms > self.now_ms);
        self.buffs.retain(|buff| buff.expires_at_ms > self.now_ms);
        self.outgoing_conditions
            .retain(|condition| condition.expires_at_ms > self.now_ms);
        self.incoming_conditions
            .retain(|condition| condition.expires_at_ms > self.now_ms);
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
        let skill_id = self.skills[pending.skill_idx].skill_id;
        let effects = self.skills[pending.skill_idx].effects.clone();
        for effect in effects {
            self.apply_skill_effect(skill_id, &effect, protected_before);
        }
        self.trigger_procs(TriggerRule::OnSkillUse, protected_before);
        let protected = protected_before || self.control_owned();
        if protected {
            self.protected_action_count += 1;
            for event in &mut self.damage_events[first_damage_event..] {
                event.protected = true;
            }
        }
    }

    fn start_cast(&mut self, skill_idx: usize) {
        let skill = &self.skills[skill_idx];
        self.pay_resource(skill.skill_id);
        self.resource_blocked_skills.remove(&skill.skill_id);
        let quickness = self.has_buff("Quickness");
        let cast_ms = if quickness {
            (skill.cast_time_ms * 2 + 1) / 3
        } else {
            skill.cast_time_ms
        }
        .max(TIMELINE_TICK_MS);
        self.cooldown_ready_ms[skill_idx] = self.now_ms + skill.cooldown_ms;
        self.pending = Some(PendingCast {
            skill_idx,
            started_at_ms: self.now_ms,
            resolves_at_ms: self.now_ms + cast_ms,
            protected_at_start: self.control_owned(),
            saved_by_charge: false,
        });
        self.next_action_ms = self.now_ms + cast_ms + 180;
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
            self.weapon_swap_ready_ms = self.now_ms + 9_000;
            self.next_action_ms = self.now_ms + 100;
        }
    }

    fn try_stunbreak(&mut self) {
        if self.now_ms >= self.disabled_until_ms {
            return;
        }
        let Some((idx, _)) = self.skills.iter().enumerate().find(|(idx, skill)| {
            skill.is_stunbreak
                && self.cooldown_ready_ms[*idx] <= self.now_ms
                && self.skill_available(skill)
        }) else {
            return;
        };
        self.cooldown_ready_ms[idx] = self.now_ms + self.skills[idx].cooldown_ms;
        self.disabled_until_ms = self.now_ms;
        self.next_action_ms = self.now_ms + 100;
    }

    fn process_enemy_events(&mut self) {
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
        }
        self.disabled_until_ms = self.disabled_until_ms.max(self.now_ms + duration_ms);
        self.protected_run_ms = 0;
    }

    fn receive_condition(&mut self, condition: String, stacks: u32, duration_ms: u32) {
        if self.avoids_attack(false)
            || self.has_defense(CoverKind::Resistance)
            || self.has_defense(CoverKind::Invulnerability)
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

    fn avoids_attack(&mut self, unblockable: bool) -> bool {
        if self.has_defense(CoverKind::Invulnerability)
            || self.has_defense(CoverKind::Evade)
            || self.has_defense(CoverKind::Stealth)
        {
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
        if self.consume_defense(CoverKind::Blind) {
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

    fn absorb_damage(&mut self, damage: f64) {
        let absorbed = self.barrier.min(damage);
        self.barrier -= absorbed;
        self.barrier_absorbed += absorbed;
        let health_damage = damage - absorbed;
        self.incoming_damage += health_damage;
        self.player_health = (self.player_health - health_damage).max(0.0);
    }

    fn tick_conditions(&mut self) {
        let mut outgoing_damage = 0.0;
        for condition in &mut self.outgoing_conditions {
            if condition.next_tick_ms <= self.now_ms {
                let tick = condition_tick_damage(
                    &condition.name,
                    self.params.condition_damage,
                    &self.params.mode,
                ) * condition.stacks as f64
                    * self.params.condition_mult
                    * self.passive_condition_mult;
                outgoing_damage += tick;
                condition.next_tick_ms += 1_000;
            }
        }
        if outgoing_damage > 0.0 {
            self.record_damage(outgoing_damage, self.control_owned());
        }

        let mut incoming_damage = 0.0;
        for condition in &mut self.incoming_conditions {
            if condition.next_tick_ms <= self.now_ms {
                incoming_damage +=
                    condition_tick_damage(&condition.name, 1_800.0, &self.params.mode)
                        * condition.stacks as f64;
                condition.next_tick_ms += 1_000;
            }
        }
        if incoming_damage > 0.0 {
            self.absorb_damage(incoming_damage * self.incoming_condition_mult);
        }
    }

    fn apply_skill_effect(&mut self, skill_id: u32, effect: &SkillEffect, protected: bool) {
        match effect {
            SkillEffect::StrikeDamage {
                hit_count,
                dmg_multiplier,
            } => {
                let might = self.buff_stacks("Might").min(25) as f64;
                let power = self.params.power + might * 30.0;
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
                self.gain_resource_on_hit(skill_id);
                self.trigger_procs(TriggerRule::OnHit, protected);
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
            SkillEffect::ComboField { field_type } => {
                self.combo_field = Some(ComboFieldState {
                    field_type: field_type.clone(),
                    expires_at_ms: self.now_ms + 5_000,
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
                self.barrier += amount + self.params.healing_power * 0.30;
            }
            SkillEffect::RemovesCondition { conditions_removed } => {
                self.cleanse(*conditions_removed)
            }
            SkillEffect::CrowdControl { duration_ms, .. } => {
                if !self.enemy_stability {
                    self.enemy_disabled_until_ms =
                        self.enemy_disabled_until_ms.max(self.now_ms + *duration_ms);
                    self.control_landed_ms += *duration_ms;
                }
            }
            SkillEffect::StripBoons { .. }
            | SkillEffect::CorruptBoons
            | SkillEffect::StealBoons => {
                self.enemy_stability = false;
                self.enemy_protection = false;
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
            SkillEffect::Mobility { kind } => match kind {
                MobilityKind::Evade => self.apply_defense(CoverKind::Evade, 750, 1, false),
                MobilityKind::Stealth => self.apply_defense(CoverKind::Stealth, 3_000, 1, false),
                _ => {}
            },
        }
    }

    fn resolve_combo(&mut self, finisher_type: &str, percent: u32) {
        if percent == 0 {
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
            } else {
                self.apply_defense(CoverKind::Blind, 3_000, 1, false);
            }
        } else if field_type.contains("water") {
            self.heal(1_000.0 + self.params.healing_power * 0.25);
        } else if field_type.contains("light") {
            self.cleanse(1);
        } else if field_type.contains("fire") {
            self.apply_buff("Might", 3, 10_000, true);
        } else if field_type.contains("dark") {
            self.apply_defense(CoverKind::Blind, 2_000, 1, false);
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

    fn trigger_procs(&mut self, trigger: TriggerRule, protected: bool) {
        let mut ready = Vec::new();
        for (idx, proc_spec) in self.proc_specs.iter().enumerate() {
            if same_trigger(&proc_spec.trigger, &trigger) && proc_spec.next_ready_ms <= self.now_ms
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
                        * as_ratio(value).max(0.01);
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
        let target_reached = self.enemy_health <= 0.0;
        let chain_completed = self.longest_protected_window_ms >= self.profile.required_window_ms
            && self.protected_action_count >= 2
            && (protected_damage > 0.0 || self.control_landed_ms > 0);
        let cooldown_recovery = self
            .cooldown_ready_ms
            .iter()
            .filter(|ready| **ready <= self.profile.duration_ms + 5_000)
            .count()
            >= self.cooldown_ready_ms.len().saturating_div(2);
        let repeatable = self.player_health > 0.0
            && cooldown_recovery
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
            repeatable,
            resource_blocked_actions: self.resource_blocked_skills.len() as u32,
            resource_legal: self.resource_blocked_skills.is_empty(),
            unmodeled_effect_sources: self.unmodeled_effect_sources,
        }
    }
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
                ResourceKind::Energy => 100.0,
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
            target_health: 18_000.0,
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
        let mut timeline = Timeline::new(skills, params, profile, enemy, &[], rules, 0);
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
            "{label} should complete its secured sequence"
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
        };
        let viability = evaluate_viability_gates(Some(&rotation), &combat, &scenario);
        assert!(
            viability.is_viable,
            "{label} should pass the WvW gates: {:?}",
            viability.gates
        );
    }

    #[test]
    fn published_daredevil_fixture_beats_unprotected_pressure() {
        // MetaBattle Power S/D and SA D/P roamers: control or stealth/evade
        // creates the opening for initiative-limited weapon pressure.
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
        assert_published_fixture(
            "Daredevil",
            skills,
            vec![rule(102, ResourceKind::Initiative, 3.0, 0.0, false)],
            false,
        );
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
            0,
        );
        for _ in 0..3 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
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
            0,
        );
        for _ in 0..4 {
            assert!(timeline.can_pay_resource(1));
            timeline.pay_resource(1);
        }
        assert!(!timeline.can_pay_resource(1));
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
            0,
        );
        assert!(!timeline.can_pay_resource(2));
        timeline.gain_resource_on_hit(1);
        assert!(timeline.can_pay_resource(2));
        timeline.pay_resource(2);
        assert!(!timeline.can_pay_resource(2));
    }
}
