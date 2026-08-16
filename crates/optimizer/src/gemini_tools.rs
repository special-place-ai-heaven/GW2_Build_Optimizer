//! Gemini function-calling tools — gives the LLM access to game data and calculations.
//! Each tool maps to a Gemini `functionDeclaration` and an execution handler.
//! Tools return concise JSON (<500 tokens) to stay within Gemini context limits.

use std::collections::HashMap;

use serde_json::{json, Value};

use gw2_api::models::{Fact, ItemStat, Trait as GW2Trait};

use crate::balance::BalanceContext;
use crate::combat::{self, CombatPerformance, DamageModifiers};
use crate::data;
use crate::engine::BuildCandidate;
use crate::gamedb::GameDb;
use crate::gemini::{FunctionDeclaration, Tool};
use crate::scoring::{self, OptimizationWeights};
use crate::stats;

/// Named record indexable by the deterministic lookup helper below.
/// Implemented for any indexed game entity (ItemStat, Specialization, GW2Trait) with
/// `id` + `name` fields.
trait NamedRecord {
    fn rec_id(&self) -> u32;
    fn rec_name(&self) -> &str;
}

impl NamedRecord for ItemStat {
    fn rec_id(&self) -> u32 {
        self.id
    }
    fn rec_name(&self) -> &str {
        &self.name
    }
}

impl NamedRecord for gw2_api::models::Specialization {
    fn rec_id(&self) -> u32 {
        self.id
    }
    fn rec_name(&self) -> &str {
        &self.name
    }
}

impl NamedRecord for GW2Trait {
    fn rec_id(&self) -> u32 {
        self.id
    }
    fn rec_name(&self) -> &str {
        &self.name
    }
}

/// Deterministic record lookup by name across any `HashMap<u32, T>` of `NamedRecord`.
///
/// `HashMap::values()` iteration order is unspecified, so a naive `.find(contains)`
/// can pick a different record across runs given identical input. Gemini tool execs
/// are LLM-driven and must be reproducible: the same name string must always
/// resolve to the same record.
///
/// Policy (matches `validation::validate_gear_prefix`):
///   1. Exact case-insensitive name match wins (lower id tiebreak).
///   2. Otherwise: shortest substring match wins (lower id tiebreak).
fn find_named<'a, T, I>(records: I, needle: &str) -> Option<&'a T>
where
    T: NamedRecord + 'a,
    I: IntoIterator<Item = &'a T>,
{
    if needle.is_empty() {
        return None;
    }
    let needle_lower = needle.to_lowercase();
    let mut exact: Option<&T> = None;
    let mut fuzzy: Option<(usize, u32, &T)> = None;
    for r in records {
        let lower = r.rec_name().to_lowercase();
        if lower == needle_lower {
            match exact {
                Some(prev) if prev.rec_id() <= r.rec_id() => {}
                _ => exact = Some(r),
            }
        } else if lower.contains(&needle_lower) {
            let key = (r.rec_name().len(), r.rec_id());
            match fuzzy {
                Some((plen, pid, _)) if (plen, pid) <= key => {}
                _ => fuzzy = Some((key.0, key.1, r)),
            }
        }
    }
    exact.or(fuzzy.map(|(_, _, r)| r))
}

fn find_itemstat_by_name<'a>(db: &'a GameDb, needle: &str) -> Option<&'a ItemStat> {
    // Delegate to GameDb's centralized deterministic helper. Kept as a thin
    // wrapper so existing call sites and tests continue to compile unchanged.
    db.itemstat_by_name(needle)
}

/// Runtime context for tool execution — holds references to all game data
/// and optimizer state needed by the tools.
pub struct ToolContext<'a> {
    pub db: &'a GameDb,
    pub profession_name: &'a str,
    pub candidates: &'a [BuildCandidate],
    pub current_build_summary: Option<&'a str>,
    pub weights: OptimizationWeights,
    pub balance_ctx: &'a BalanceContext,
}

/// Build the Gemini tool declarations for all available tools.
pub fn tool_declarations() -> Vec<Tool> {
    vec![Tool {
        function_declarations: vec![
            decl_get_profession_info(),
            decl_get_spec_traits(),
            decl_get_trait_details(),
            decl_get_skill_info(),
            decl_list_runes(),
            decl_list_sigils(),
            decl_list_relics(),
            decl_search_upgrades(),
            decl_upgrade_synergies(),
            decl_calculate_stats(),
            decl_simulate_combat(),
            decl_score_build(),
            decl_get_current_build(),
            decl_get_optimizer_results(),
            // Synergy discovery tools
            decl_search_traits_by_effect(),
            decl_find_condition_sources(),
            decl_search_skills_by_effect(),
            decl_find_synergies(),
            decl_get_build_synergy_report(),
            // Rotation simulator
            decl_simulate_rotation(),
        ],
    }]
}

/// Execute a tool call by name, dispatching to the appropriate handler.
pub fn execute_tool(name: &str, args: &Value, ctx: &ToolContext) -> Value {
    match name {
        "get_profession_info" => exec_get_profession_info(args, ctx),
        "get_spec_traits" => exec_get_spec_traits(args, ctx),
        "get_trait_details" => exec_get_trait_details(args, ctx),
        "get_skill_info" => exec_get_skill_info(args, ctx),
        "list_runes" => exec_list_runes(ctx),
        "list_sigils" => exec_list_sigils(ctx),
        "list_relics" => exec_list_relics(ctx),
        "search_upgrades" => exec_search_upgrades(args, ctx),
        "upgrade_synergies" => exec_upgrade_synergies(args, ctx),
        "calculate_stats" => exec_calculate_stats(args, ctx),
        "simulate_combat" => exec_simulate_combat(args, ctx),
        "score_build" => exec_score_build(args, ctx),
        "get_current_build" => exec_get_current_build(ctx),
        "get_optimizer_results" => exec_get_optimizer_results(ctx),
        "search_traits_by_effect" => exec_search_traits_by_effect(args, ctx),
        "find_condition_sources" => exec_find_condition_sources(args, ctx),
        "search_skills_by_effect" => exec_search_skills_by_effect(args, ctx),
        "find_synergies" => exec_find_synergies(args, ctx),
        "get_build_synergy_report" => exec_get_build_synergy_report(args, ctx),
        "simulate_rotation" => exec_simulate_rotation(args, ctx),
        _ => json!({ "error": format!("Unknown tool: {}", name) }),
    }
}

// ─── Tool Declarations ───

fn decl_get_profession_info() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_profession_info".into(),
        description: "Get profession details: specializations, available weapons (per hand), and resource type.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "profession": {
                    "type": "string",
                    "description": "Profession name (e.g. 'Warrior', 'Elementalist')"
                }
            },
            "required": ["profession"]
        }),
    }
}

fn decl_get_spec_traits() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_spec_traits".into(),
        description: "Get a specialization's trait layout: 3 minor traits (always active) and 9 major traits (3 columns × 3 choices, pick 1 per column).".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "spec_name": {
                    "type": "string",
                    "description": "Specialization name (e.g. 'Berserker', 'Arms', 'Firebrand')"
                }
            },
            "required": ["spec_name"]
        }),
    }
}

fn decl_get_trait_details() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_trait_details".into(),
        description: "Get detailed info about a trait: description, stat bonuses, damage modifiers, buffs, and conditions.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "trait_name": {
                    "type": "string",
                    "description": "Trait name (e.g. 'Forceful Greatsword', 'Empowered')"
                }
            },
            "required": ["trait_name"]
        }),
    }
}

fn decl_get_skill_info() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_skill_info".into(),
        description: "Get skill details: type, slot, cost, recharge, damage facts, conditions applied, weapon type.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "skill_name": {
                    "type": "string",
                    "description": "Skill name (e.g. 'Hundred Blades', 'Fireball')"
                }
            },
            "required": ["skill_name"]
        }),
    }
}

fn decl_list_runes() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "list_runes".into(),
        description: "Top Superior runes for the current 6-axis radar (Power, Condition, Boon Support, Heal, Sustain, Control). Not A–Z. Use search_upgrades for another focus.".into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn decl_list_sigils() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "list_sigils".into(),
        description:
            "Top Superior sigils for the current 6-axis radar. Not A–Z. Use search_upgrades for another focus.".into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn decl_list_relics() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "list_relics".into(),
        description: "Top relics for the current 6-axis radar. Not A–Z. Use search_upgrades for another focus.".into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn decl_search_upgrades() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "search_upgrades".into(),
        description: "Search runes/sigils/relics on the full 6-axis matrix. focus is power|condition|boon_support|healing|sustain|control. Optional kind and tag (burning, crit, cc, heal, kill). Returns a short ranked list with rely + all 6 axis scores.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "focus": {
                    "type": "string",
                    "description": "Axis: power, condition, boon_support, healing, sustain, or control"
                },
                "kind": {
                    "type": "string",
                    "description": "rune, sigil, or relic"
                },
                "tag": {
                    "type": "string",
                    "description": "Optional tag such as burning, crit, swap, evade, cc, heal, kill"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results (default 12)"
                }
            }
        }),
    }
}

fn decl_upgrade_synergies() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "upgrade_synergies".into(),
        description: "Neighbors of a rune/sigil/relic in the upgrade graph (shared tags and duration↔apply pairs) with the full 6-axis scores.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Upgrade name (partial match ok)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Max neighbors (default 8)"
                }
            },
            "required": ["name"]
        }),
    }
}

fn decl_calculate_stats() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "calculate_stats".into(),
        description: "Calculate the full 9-stat block for a gear prefix on the current profession. Shows what stats you'd get.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gear_prefix": {
                    "type": "string",
                    "description": "Stat prefix name (e.g. 'Berserker\\'s', 'Viper\\'s', 'Celestial')"
                }
            },
            "required": ["gear_prefix"]
        }),
    }
}

fn decl_simulate_combat() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "simulate_combat".into(),
        description: "Simulate combat performance for a gear prefix with optional trait IDs. Returns DPS indexes, healing, survivability under Solo/Party/Squad buff profiles.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gear_prefix": {
                    "type": "string",
                    "description": "Stat prefix name (e.g. 'Berserker\\'s')"
                },
                "trait_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Optional list of equipped trait IDs for damage modifier extraction"
                }
            },
            "required": ["gear_prefix"]
        }),
    }
}

fn decl_score_build() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "score_build".into(),
        description: "Score a gear prefix against the player's optimization weights. Returns the combat-based score and breakdown.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "gear_prefix": {
                    "type": "string",
                    "description": "Stat prefix name (e.g. 'Berserker\\'s')"
                }
            },
            "required": ["gear_prefix"]
        }),
    }
}

fn decl_get_current_build() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_current_build".into(),
        description: "Get a summary of the player's currently equipped build (if available)."
            .into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

fn decl_get_optimizer_results() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_optimizer_results".into(),
        description: "Get the top candidates from the deterministic optimizer. Shows best gear/spec combos found.".into(),
        parameters: json!({
            "type": "object",
            "properties": {}
        }),
    }
}

// ─── Synergy Tool Declarations ───

fn decl_search_traits_by_effect() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "search_traits_by_effect".into(),
        description: "Search for traits by effect type. Finds traits that deal strike damage, apply conditions, grant boons, boost stats, etc. Use to discover synergy candidates.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "effect_type": {
                    "type": "string",
                    "description": "Effect to search for: 'strike_damage', 'condition_damage', 'healing', 'boon_duration', 'condition_duration', 'crit', 'survivability'"
                },
                "profession": {
                    "type": "string",
                    "description": "Optional: filter to traits available to this profession"
                }
            },
            "required": ["effect_type"]
        }),
    }
}

fn decl_find_condition_sources() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "find_condition_sources".into(),
        description: "Find all skills and traits that apply a specific condition. Essential for building condition-focused builds.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "condition": {
                    "type": "string",
                    "description": "Condition name: 'Bleeding', 'Burning', 'Poison', 'Torment', 'Confusion', 'Vulnerability', 'Weakness'"
                },
                "profession": {
                    "type": "string",
                    "description": "Optional: filter to this profession's skills/traits"
                }
            },
            "required": ["condition"]
        }),
    }
}

fn decl_search_skills_by_effect() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "search_skills_by_effect".into(),
        description: "Search for skills that apply a condition, grant a boon, or create a combo field. Use to find skills that synergize with traits and gear.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "effect": {
                    "type": "string",
                    "description": "Condition, boon, or combo field type: 'Bleeding', 'Burning', 'Might', 'Fury', 'Fire', 'Light', etc."
                },
                "profession": {
                    "type": "string",
                    "description": "Optional: filter to this profession"
                },
                "weapon_type": {
                    "type": "string",
                    "description": "Optional: filter to this weapon (e.g. 'Greatsword', 'Scepter')"
                }
            },
            "required": ["effect"]
        }),
    }
}

fn decl_find_synergies() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "find_synergies".into(),
        description: "Analyze interactions between selected traits. Shows activated traited_facts (conditional bonuses that unlock when specific trait combinations are equipped).".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "trait_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Trait IDs to analyze for cross-references"
                }
            },
            "required": ["trait_ids"]
        }),
    }
}

fn decl_get_build_synergy_report() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "get_build_synergy_report".into(),
        description: "Generate a comprehensive synergy report for a complete build. Analyzes trait interactions, condition chains, damage modifier stacking, and duration bonuses.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "trait_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "All equipped trait IDs"
                },
                "gear_prefix": {
                    "type": "string",
                    "description": "Gear stat prefix (e.g. 'Berserker\\'s')"
                },
                "rune_name": {
                    "type": "string",
                    "description": "Optional: equipped rune name"
                }
            },
            "required": ["trait_ids"]
        }),
    }
}

fn decl_simulate_rotation() -> FunctionDeclaration {
    FunctionDeclaration {
        name: "simulate_rotation".into(),
        description: "Simulate a skill rotation to estimate real DPS, condition uptime, buff uptime, and control metrics. Validates whether a build's skills actually work together over time.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "skill_ids": {
                    "type": "array",
                    "items": { "type": "integer" },
                    "description": "Skill IDs to include in the rotation (weapon skills + heal + utilities + elite)"
                },
                "gear_prefix": {
                    "type": "string",
                    "description": "Gear stat prefix (e.g. 'Berserker\\'s') for stat calculation"
                },
                "duration_seconds": {
                    "type": "integer",
                    "description": "Optional simulation duration (default 30 seconds)"
                }
            },
            "required": ["skill_ids"]
        }),
    }
}

// ─── Tool Execution Handlers ───

fn exec_get_profession_info(args: &Value, ctx: &ToolContext) -> Value {
    let prof_name = args["profession"].as_str().unwrap_or(ctx.profession_name);

    let Some(prof) = ctx.db.profession(prof_name) else {
        return json!({ "error": format!("Profession '{}' not found", prof_name) });
    };

    // Gather specializations
    let specs: Vec<Value> = prof
        .specializations
        .iter()
        .filter_map(|&id| {
            ctx.db.spec(id).map(|s| {
                json!({
                    "id": s.id,
                    "name": &s.name,
                    "elite": s.elite
                })
            })
        })
        .collect();

    // Gather weapons with hand info
    let weapons: Vec<Value> = prof
        .weapons
        .iter()
        .map(|(name, info)| {
            let mut w = json!({
                "name": name,
                "flags": &info.flags
            });
            if let Some(spec_id) = info.specialization {
                if let Some(spec) = ctx.db.spec(spec_id) {
                    w["requires_elite"] = json!(&spec.name);
                }
            }
            w
        })
        .collect();

    json!({
        "profession": &prof.name,
        "specializations": specs,
        "weapons": weapons
    })
}

fn exec_get_spec_traits(args: &Value, ctx: &ToolContext) -> Value {
    let spec_name = args["spec_name"].as_str().unwrap_or("");

    let spec = find_named(ctx.db.specializations.values(), spec_name);

    let Some(spec) = spec else {
        return json!({ "error": format!("Specialization '{}' not found", spec_name) });
    };

    // Minor traits (always active)
    let minors: Vec<Value> = spec
        .minor_traits
        .iter()
        .filter_map(|&id| {
            ctx.db.traits.get(&id).map(|t| {
                json!({
                    "id": t.id,
                    "name": &t.name,
                    "tier": t.tier,
                    "description": t.description.as_deref().unwrap_or("")
                })
            })
        })
        .collect();

    // Major traits organized by column (tier)
    let mut columns: HashMap<u32, Vec<Value>> = HashMap::new();
    for &id in &spec.major_traits {
        if let Some(t) = ctx.db.traits.get(&id) {
            columns.entry(t.tier).or_default().push(json!({
                "id": t.id,
                "name": &t.name,
                "order": t.order,
                "description": t.description.as_deref().unwrap_or(""),
                "key_effects": summarize_trait_facts(t)
            }));
        }
    }

    // Sort choices within each column by order
    for choices in columns.values_mut() {
        choices.sort_by_key(|v| v["order"].as_u64().unwrap_or(0));
    }

    json!({
        "specialization": &spec.name,
        "elite": spec.elite,
        "profession": &spec.profession,
        "minor_traits": minors,
        "columns": {
            "adept": columns.get(&1).unwrap_or(&vec![]),
            "master": columns.get(&2).unwrap_or(&vec![]),
            "grandmaster": columns.get(&3).unwrap_or(&vec![])
        }
    })
}

fn exec_get_trait_details(args: &Value, ctx: &ToolContext) -> Value {
    let trait_name = args["trait_name"].as_str().unwrap_or("");

    let t = find_named(ctx.db.traits.values(), trait_name);

    let Some(t) = t else {
        return json!({ "error": format!("Trait '{}' not found", trait_name) });
    };

    let spec_name = ctx
        .db
        .spec(t.specialization)
        .map(|s| s.name.as_str())
        .unwrap_or("Unknown");

    let facts: Vec<Value> = t.facts.iter().map(format_fact).collect();
    let traited: Vec<Value> = t
        .traited_facts
        .iter()
        .map(|tf| {
            let mut v = format_fact(&tf.fact);
            if let Some(req) = ctx.db.traits.get(&tf.requires_trait) {
                v["requires_trait"] = json!(&req.name);
            }
            if let Some(idx) = tf.overrides {
                v["overrides_fact_index"] = json!(idx);
            }
            v
        })
        .collect();

    // Synergy summaries
    let conditions_applied = extract_conditions(&t.facts);
    let buffs_applied = extract_buffs(&t.facts);
    let stat_bonuses = extract_stat_bonuses(&t.facts);
    let damage_modifiers = extract_damage_modifiers_from_facts(&t.facts);
    let proc_triggers = detect_proc_triggers(&t.facts, t.description.as_deref());

    json!({
        "id": t.id,
        "name": &t.name,
        "specialization": spec_name,
        "tier": t.tier,
        "slot": &t.slot,
        "description": t.description.as_deref().unwrap_or(""),
        "facts": facts,
        "traited_facts": traited,
        "conditions_applied": conditions_applied,
        "buffs_applied": buffs_applied,
        "stat_bonuses": stat_bonuses,
        "damage_modifiers": damage_modifiers,
        "proc_triggers": proc_triggers
    })
}

fn exec_get_skill_info(args: &Value, ctx: &ToolContext) -> Value {
    let skill_name = args["skill_name"].as_str().unwrap_or("");

    // Find matching skills, preferring profession match
    let mut matches: Vec<_> = ctx
        .db
        .skills
        .values()
        .filter(|s| s.name.to_lowercase().contains(&skill_name.to_lowercase()))
        .collect();

    // Sort: profession-specific first, then by name length (shorter = more exact),
    // then by id so ties are broken deterministically across runs.
    // `HashMap::values()` order is unspecified — without an id tiebreak the
    // same skill_name argument could resolve to different skills on different
    // machines or runs, making LLM tool results non-reproducible.
    matches.sort_by(|a, b| {
        let a_prof = a.professions.contains(&ctx.profession_name.to_string());
        let b_prof = b.professions.contains(&ctx.profession_name.to_string());
        b_prof
            .cmp(&a_prof)
            .then(a.name.len().cmp(&b.name.len()))
            .then(a.id.cmp(&b.id))
    });

    let Some(skill) = matches.first() else {
        return json!({ "error": format!("Skill '{}' not found", skill_name) });
    };

    let facts: Vec<Value> = skill.facts.iter().map(format_fact).collect();
    let conditions_applied = extract_conditions(&skill.facts);
    let buffs_applied = extract_buffs(&skill.facts);

    json!({
        "id": skill.id,
        "name": &skill.name,
        "description": skill.description.as_deref().unwrap_or(""),
        "type": skill.skill_type.as_deref().unwrap_or(""),
        "slot": skill.slot.as_deref().unwrap_or(""),
        "weapon_type": skill.weapon_type.as_deref().unwrap_or(""),
        "cost": skill.cost,
        "initiative": skill.initiative,
        "professions": &skill.professions,
        "categories": &skill.categories,
        "next_chain": skill.next_chain,
        "prev_chain": skill.prev_chain,
        "flip_skill": skill.flip_skill,
        "conditions_applied": conditions_applied,
        "buffs_applied": buffs_applied,
        "facts": facts
    })
}

fn exec_list_runes(ctx: &ToolContext) -> Value {
    exec_list_kind(ctx, crate::upgrade_graph::UpgradeKind::Rune)
}

fn exec_list_sigils(ctx: &ToolContext) -> Value {
    exec_list_kind(ctx, crate::upgrade_graph::UpgradeKind::Sigil)
}

fn exec_list_relics(ctx: &ToolContext) -> Value {
    exec_list_kind(ctx, crate::upgrade_graph::UpgradeKind::Relic)
}

fn graph(ctx: &ToolContext) -> crate::upgrade_graph::UpgradeGraph {
    crate::upgrade_graph::UpgradeGraph::from_db(ctx.db, ctx.balance_ctx)
}

fn exec_list_kind(ctx: &ToolContext, kind: crate::upgrade_graph::UpgradeKind) -> Value {
    let g = graph(ctx);
    let hits: Vec<Value> = g
        .search(None, Some(kind), None, Some(&ctx.weights), 12)
        .iter()
        .map(|n| n.to_json())
        .collect();
    match kind {
        crate::upgrade_graph::UpgradeKind::Rune => json!({ "runes": hits }),
        crate::upgrade_graph::UpgradeKind::Sigil => json!({ "sigils": hits }),
        crate::upgrade_graph::UpgradeKind::Relic => json!({ "relics": hits }),
    }
}

fn exec_search_upgrades(args: &Value, ctx: &ToolContext) -> Value {
    let g = graph(ctx);
    let focus = args["focus"].as_str();
    let kind = args["kind"]
        .as_str()
        .and_then(crate::upgrade_graph::parse_kind);
    let tag = args["tag"].as_str();
    let limit = args["limit"].as_u64().unwrap_or(12) as usize;
    let hits = g.search(focus, kind, tag, Some(&ctx.weights), limit.min(24));
    json!({
        "focus": focus,
        "kind": kind.map(|k| k.as_str()),
        "axes": ["power", "condition", "boon_support", "healing", "sustain", "control"],
        "upgrades": hits.iter().map(|n| n.to_json()).collect::<Vec<_>>()
    })
}

fn exec_upgrade_synergies(args: &Value, ctx: &ToolContext) -> Value {
    let Some(name) = args["name"].as_str() else {
        return json!({ "error": "name is required" });
    };
    let limit = args["limit"].as_u64().unwrap_or(8) as usize;
    graph(ctx).synergies(name, limit.min(16))
}

fn exec_calculate_stats(args: &Value, ctx: &ToolContext) -> Value {
    let gear_prefix = args["gear_prefix"].as_str().unwrap_or("");

    let itemstat = find_itemstat_by_name(ctx.db, gear_prefix);

    let Some(itemstat) = itemstat else {
        return json!({ "error": format!("Stat prefix '{}' not found", gear_prefix) });
    };

    let gear_stats = calculate_full_set_stats(itemstat);
    let mut full_stats = stats::base_stats();
    full_stats += &gear_stats;
    let derived = stats::compute_derived(&full_stats, ctx.profession_name);

    json!({
        "prefix": &itemstat.name,
        "stats": {
            "power": full_stats.power.round() as i32,
            "precision": full_stats.precision.round() as i32,
            "toughness": full_stats.toughness.round() as i32,
            "vitality": full_stats.vitality.round() as i32,
            "condition_damage": full_stats.condition_damage.round() as i32,
            "expertise": full_stats.expertise.round() as i32,
            "concentration": full_stats.concentration.round() as i32,
            "ferocity": full_stats.ferocity.round() as i32,
            "healing_power": full_stats.healing_power.round() as i32
        },
        "derived": {
            "crit_chance": format!("{:.1}%", derived.crit_chance),
            "crit_damage": format!("{:.1}%", derived.crit_damage),
            "effective_power": derived.effective_power.round() as i32,
            "health": derived.health.round() as i32,
            "armor": derived.armor.round() as i32
        }
    })
}

fn exec_simulate_combat(args: &Value, ctx: &ToolContext) -> Value {
    let gear_prefix = args["gear_prefix"].as_str().unwrap_or("");

    let itemstat = find_itemstat_by_name(ctx.db, gear_prefix);

    let Some(itemstat) = itemstat else {
        return json!({ "error": format!("Stat prefix '{}' not found", gear_prefix) });
    };

    let gear_stats = calculate_full_set_stats(itemstat);
    let mut full_stats = stats::base_stats();
    full_stats += &gear_stats;
    let derived = stats::compute_derived(&full_stats, ctx.profession_name);

    // Extract damage modifiers from traits if provided
    let trait_ids: Vec<u32> = args
        .get("trait_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();

    let modifiers = if trait_ids.is_empty() {
        DamageModifiers::default()
    } else {
        combat::extract_damage_modifiers(
            &trait_ids,
            None,
            &[],
            None,
            &ctx.db.traits,
            &ctx.db.items,
            ctx.balance_ctx,
        )
    };

    // Simulate under all 3 buff profiles using profession-specific rotation profile data
    let profiles = combat::buff_profiles_for_profession(ctx.profession_name, ctx.balance_ctx);
    let cw = combat::condition_weights_for_profession(ctx.profession_name, ctx.balance_ctx);
    let results: Vec<Value> = profiles
        .iter()
        .map(|bp| {
            let perf = combat::calculate_combat_performance(
                &full_stats,
                &derived,
                &modifiers,
                bp,
                &cw,
                ctx.profession_name,
                ctx.balance_ctx,
            );
            format_combat_performance(&perf, &bp.label)
        })
        .collect();

    json!({
        "prefix": &itemstat.name,
        "combat_profiles": results
    })
}

fn exec_score_build(args: &Value, ctx: &ToolContext) -> Value {
    let gear_prefix = args["gear_prefix"].as_str().unwrap_or("");

    let itemstat = find_itemstat_by_name(ctx.db, gear_prefix);

    let Some(itemstat) = itemstat else {
        return json!({ "error": format!("Stat prefix '{}' not found", gear_prefix) });
    };

    let gear_stats = calculate_full_set_stats(itemstat);
    let mut full_stats = stats::base_stats();
    full_stats += &gear_stats;
    let derived = stats::compute_derived(&full_stats, ctx.profession_name);

    let mods = DamageModifiers::default();
    let solo = &combat::buff_profiles_for_profession(ctx.profession_name, ctx.balance_ctx)[0];
    let perf = combat::calculate_combat_performance(
        &full_stats,
        &derived,
        &mods,
        solo,
        &combat::condition_weights_for_profession(ctx.profession_name, ctx.balance_ctx),
        ctx.profession_name,
        ctx.balance_ctx,
    );

    let score = scoring::score_with_weights(&perf, &ctx.weights);

    json!({
        "prefix": &itemstat.name,
        "weights_summary": ctx.weights.summary_label(),
        "score": format!("{:.4}", score),
        "combat_summary": {
            "effective_power": perf.effective_power.round() as i32,
            "strike_dps_index": perf.strike_dps_index.round() as i32,
            "condition_dps_index": perf.condition_dps_index.round() as i32,
            "total_dps_index": perf.total_dps_index.round() as i32,
            "healing_power_index": perf.healing_power_index.round() as i32,
            "boon_duration_pct": format!("{:.1}%", perf.boon_duration_pct),
            "effective_health": perf.effective_health.round() as i32
        }
    })
}

fn exec_get_current_build(ctx: &ToolContext) -> Value {
    match ctx.current_build_summary {
        Some(summary) => json!({
            "has_build": true,
            "summary": summary
        }),
        None => json!({
            "has_build": false,
            "message": "No current build equipped or build could not be resolved"
        }),
    }
}

fn exec_get_optimizer_results(ctx: &ToolContext) -> Value {
    if ctx.candidates.is_empty() {
        return json!({ "candidates": [], "message": "No optimizer results available" });
    }

    let results: Vec<Value> = ctx
        .candidates
        .iter()
        .take(5)
        .map(|c| {
            let spec_names: Vec<String> = c
                .core_specs
                .iter()
                .chain(c.elite_spec.iter())
                .filter_map(|&id| ctx.db.spec(id).map(|s| s.name.clone()))
                .collect();

            let trait_names: Vec<String> = c
                .equipped_traits
                .iter()
                .filter_map(|&id| ctx.db.traits.get(&id).map(|t| t.name.clone()))
                .collect();

            json!({
                "gear_prefix": &c.gear.stat_prefix_name,
                "score": format!("{:.4}", c.score),
                "specializations": spec_names,
                "equipped_traits": trait_names,
                "stats": {
                    "power": c.stats.power.round() as i32,
                    "precision": c.stats.precision.round() as i32,
                    "toughness": c.stats.toughness.round() as i32,
                    "vitality": c.stats.vitality.round() as i32,
                    "condition_damage": c.stats.condition_damage.round() as i32,
                    "ferocity": c.stats.ferocity.round() as i32,
                    "expertise": c.stats.expertise.round() as i32,
                    "concentration": c.stats.concentration.round() as i32,
                    "healing_power": c.stats.healing_power.round() as i32
                },
                "combat": {
                    "effective_power": c.combat.effective_power.round() as i32,
                    "crit_chance": format!("{:.1}%", c.combat.crit_chance),
                    "strike_dps_index": c.combat.strike_dps_index.round() as i32,
                    "condition_dps_index": c.combat.condition_dps_index.round() as i32,
                    "total_dps_index": c.combat.total_dps_index.round() as i32,
                    "healing_power_index": c.combat.healing_power_index.round() as i32,
                    "boon_duration": format!("{:.1}%", c.combat.boon_duration_pct),
                    "condi_duration": format!("{:.1}%", c.combat.condi_duration_pct),
                    "effective_health": c.combat.effective_health.round() as i32
                }
            })
        })
        .collect();

    json!({ "candidates": results })
}

// ─── Synergy Tool Execution Handlers ───

fn exec_search_traits_by_effect(args: &Value, ctx: &ToolContext) -> Value {
    let effect_type = args["effect_type"].as_str().unwrap_or("");
    let profession_filter = args.get("profession").and_then(|v| v.as_str());

    // Iterate by id so the "first 20" surfaced to the LLM are stable across
    // runs. `db.traits.values()` HashMap iteration order is unspecified.
    let mut trait_ids: Vec<u32> = ctx.db.traits.keys().copied().collect();
    trait_ids.sort_unstable();

    let mut results: Vec<Value> = Vec::new();

    for &tid in &trait_ids {
        let Some(t) = ctx.db.traits.get(&tid) else {
            continue;
        };
        // Filter by profession if specified
        if let Some(prof) = profession_filter {
            let spec = ctx.db.spec(t.specialization);
            if let Some(spec) = spec {
                if spec.profession != prof {
                    continue;
                }
            }
        }

        let matches = match effect_type {
            "strike_damage" => t.facts.iter().any(|f| matches!(f,
                Fact::Percent { text: Some(text), .. } if text.to_lowercase().contains("damage")
            )),
            "condition_damage" => t.facts.iter().any(|f| matches!(f,
                Fact::Buff { status: Some(s), .. } if ["Bleeding", "Burning", "Poisoned", "Torment", "Confusion"].contains(
                    &crate::data::boon_condition_formulas::canonical_condition_name(s)
                )
            )),
            "healing" => t.facts.iter().any(|f| matches!(f,
                Fact::AttributeAdjust { target: Some(t), .. } if t.contains("Healing")
            ) || matches!(f, Fact::HealingAdjust { .. })),
            "boon_duration" => t.facts.iter().any(|f| matches!(f,
                Fact::Percent { text: Some(text), .. } if text.to_lowercase().contains("boon duration")
            ) || matches!(f, Fact::AttributeAdjust { target: Some(t), .. } if t == "Concentration")),
            "condition_duration" => t.facts.iter().any(|f| matches!(f,
                Fact::Percent { text: Some(text), .. } if text.to_lowercase().contains("duration") && !text.to_lowercase().contains("boon")
            ) || matches!(f, Fact::AttributeAdjust { target: Some(t), .. } if t == "Expertise" || t == "ConditionDuration")),
            "crit" => {
                let desc = t.description.as_deref().unwrap_or("").to_lowercase();
                desc.contains("crit") || t.facts.iter().any(|f| matches!(f,
                    Fact::AttributeAdjust { target: Some(t), .. } if t == "Precision" || t == "CritDamage" || t == "Ferocity"
                ))
            }
            "survivability" => t.facts.iter().any(|f| matches!(f,
                Fact::AttributeAdjust { target: Some(t), .. } if t == "Toughness" || t == "Vitality"
            ) || matches!(f, Fact::Buff { status: Some(s), .. } if s == "Protection" || s == "Regeneration" || s == "Resistance")),
            _ => false,
        };

        if matches {
            let spec_name = ctx
                .db
                .spec(t.specialization)
                .map(|s| s.name.as_str())
                .unwrap_or("Unknown");
            let effects = summarize_trait_facts(t);
            results.push(json!({
                "id": t.id,
                "name": &t.name,
                "spec": spec_name,
                "tier": t.tier,
                "key_effects": effects
            }));
            if results.len() >= 20 {
                break;
            }
        }
    }

    json!({ "effect_type": effect_type, "count": results.len(), "traits": results })
}

fn exec_find_condition_sources(args: &Value, ctx: &ToolContext) -> Value {
    let condition = args["condition"].as_str().unwrap_or("");
    let profession_filter = args
        .get("profession")
        .and_then(|v| v.as_str())
        .unwrap_or(ctx.profession_name);

    // Find traits that apply this condition
    let trait_sources: Vec<Value> = ctx
        .db
        .traits_applying_condition(condition)
        .iter()
        .filter(|t| {
            ctx.db
                .spec(t.specialization)
                .map(|s| s.profession == profession_filter)
                .unwrap_or(false)
        })
        .take(15)
        .map(|t| {
            let spec_name = ctx
                .db
                .spec(t.specialization)
                .map(|s| s.name.as_str())
                .unwrap_or("Unknown");
            // Find the specific Buff fact for this condition
            let detail = t.facts.iter().find_map(|f| {
                if let Fact::Buff {
                    status: Some(s),
                    duration,
                    apply_count,
                    ..
                } = f
                {
                    if s == condition {
                        return Some(json!({
                            "stacks": apply_count.unwrap_or(1),
                            "duration_s": duration.unwrap_or(0)
                        }));
                    }
                }
                None
            });
            json!({
                "id": t.id,
                "name": &t.name,
                "spec": spec_name,
                "tier": t.tier,
                "application": detail
            })
        })
        .collect();

    // Find skills that apply this condition
    let skill_sources: Vec<Value> = ctx
        .db
        .skills_applying_condition(condition)
        .iter()
        .filter(|s| s.professions.contains(&profession_filter.to_string()))
        .take(20)
        .map(|skill| {
            let detail = skill.facts.iter().find_map(|f| {
                if let Fact::Buff {
                    status: Some(s),
                    duration,
                    apply_count,
                    ..
                } = f
                {
                    if s == condition {
                        return Some(json!({
                            "stacks": apply_count.unwrap_or(1),
                            "duration_s": duration.unwrap_or(0)
                        }));
                    }
                }
                None
            });
            json!({
                "id": skill.id,
                "name": &skill.name,
                "slot": skill.slot.as_deref().unwrap_or(""),
                "weapon_type": skill.weapon_type.as_deref().unwrap_or(""),
                "application": detail
            })
        })
        .collect();

    json!({
        "condition": condition,
        "profession": profession_filter,
        "trait_sources": trait_sources,
        "skill_sources": skill_sources
    })
}

fn exec_search_skills_by_effect(args: &Value, ctx: &ToolContext) -> Value {
    let effect = args["effect"].as_str().unwrap_or("");
    let profession_filter = args
        .get("profession")
        .and_then(|v| v.as_str())
        .unwrap_or(ctx.profession_name);
    let weapon_filter = args.get("weapon_type").and_then(|v| v.as_str());

    // Hoist the needle lowercase once outside the per-skill/per-fact loop.
    // Also walk the pre-built `skills_by_profession` index so iteration order
    // is stable across runs — the previous `db.skills.values()` scan picked
    // nondeterministic first-20 to surface to the LLM.
    let needle = effect.to_lowercase();
    let prof_skill_ids = ctx.db.skills_by_profession.get(profession_filter);

    let mut results: Vec<Value> = Vec::new();

    if let Some(ids) = prof_skill_ids {
        for &id in ids {
            let Some(skill) = ctx.db.skills.get(&id) else {
                continue;
            };
            if let Some(wt) = weapon_filter {
                if skill.weapon_type.as_deref() != Some(wt) {
                    continue;
                }
            }

            let matches = skill.facts.iter().any(|f| match f {
                Fact::Buff {
                    status: Some(s), ..
                }
                | Fact::PrefixedBuff {
                    status: Some(s), ..
                } => s.to_lowercase().contains(&needle),
                Fact::ComboField {
                    field_type: Some(ft),
                    ..
                } => ft.to_lowercase().contains(&needle),
                Fact::ComboFinisher {
                    finisher_type: Some(ft),
                    ..
                } => ft.to_lowercase().contains(&needle),
                _ => false,
            });

            if matches {
                let conditions = extract_conditions(&skill.facts);
                let buffs = extract_buffs(&skill.facts);
                results.push(json!({
                    "id": skill.id,
                    "name": &skill.name,
                    "slot": skill.slot.as_deref().unwrap_or(""),
                    "weapon_type": skill.weapon_type.as_deref().unwrap_or(""),
                    "conditions_applied": conditions,
                    "buffs_applied": buffs
                }));
                if results.len() >= 20 {
                    break;
                }
            }
        }
    }

    json!({ "effect": effect, "count": results.len(), "skills": results })
}

fn exec_find_synergies(args: &Value, ctx: &ToolContext) -> Value {
    let trait_ids: Vec<u32> = args
        .get("trait_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();

    let trait_id_set: std::collections::HashSet<u32> = trait_ids.iter().copied().collect();
    let mut activated_synergies: Vec<Value> = Vec::new();

    for &tid in &trait_ids {
        let Some(t) = ctx.db.traits.get(&tid) else {
            continue;
        };

        for tf in &t.traited_facts {
            if trait_id_set.contains(&tf.requires_trait) {
                let req_name = ctx
                    .db
                    .traits
                    .get(&tf.requires_trait)
                    .map(|r| r.name.as_str())
                    .unwrap_or("Unknown");
                let fact_json = format_fact(&tf.fact);
                activated_synergies.push(json!({
                    "trait": &t.name,
                    "trait_id": t.id,
                    "requires": req_name,
                    "requires_id": tf.requires_trait,
                    "activated_effect": fact_json,
                    "overrides_base_fact": tf.overrides
                }));
            }
        }
    }

    // Also check what conditions the selected traits apply
    let mut conditions_from_traits: Vec<Value> = Vec::new();
    for &tid in &trait_ids {
        let Some(t) = ctx.db.traits.get(&tid) else {
            continue;
        };
        let condis = extract_conditions(&t.facts);
        if !condis.is_empty() {
            conditions_from_traits.push(json!({
                "trait": &t.name,
                "conditions": condis
            }));
        }
    }

    json!({
        "trait_count": trait_ids.len(),
        "activated_synergies": activated_synergies,
        "conditions_from_traits": conditions_from_traits
    })
}

fn exec_get_build_synergy_report(args: &Value, ctx: &ToolContext) -> Value {
    let trait_ids: Vec<u32> = args
        .get("trait_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();
    let gear_prefix = args
        .get("gear_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let rune_name = args.get("rune_name").and_then(|v| v.as_str());

    // 1. Trait synergies (activated traited_facts)
    let synergies_result = exec_find_synergies(args, ctx);

    // 2. Damage modifiers from traits
    let modifiers = combat::extract_damage_modifiers(
        &trait_ids,
        None,
        &[],
        None,
        &ctx.db.traits,
        &ctx.db.items,
        ctx.balance_ctx,
    );

    // 3. All conditions the build can apply
    let mut all_conditions: HashMap<String, Vec<String>> = HashMap::new();
    for &tid in &trait_ids {
        if let Some(t) = ctx.db.traits.get(&tid) {
            for f in &t.facts {
                if let Fact::Buff {
                    status: Some(s), ..
                } = f
                {
                    let conditions = ["Bleeding", "Burning", "Poisoned", "Torment", "Confusion"];
                    let canonical =
                        crate::data::boon_condition_formulas::canonical_condition_name(s);
                    if conditions.contains(&canonical) {
                        all_conditions
                            .entry(canonical.to_string())
                            .or_default()
                            .push(t.name.clone());
                    }
                }
            }
        }
    }

    // 4. Rune synergy (if specified)
    let rune_info = rune_name.and_then(|name| {
        ctx.db
            .all_runes()
            .iter()
            .find(|r| r.name.to_lowercase().contains(&name.to_lowercase()))
            .map(|item| {
                let bonuses = item
                    .details
                    .as_ref()
                    .map(|d| &d.bonuses)
                    .cloned()
                    .unwrap_or_default();
                let parsed: Vec<Value> = bonuses
                    .iter()
                    .map(|b| parse_rune_bonus_structured(b))
                    .collect();
                json!({ "name": &item.name, "bonuses": parsed })
            })
    });

    json!({
        "gear_prefix": gear_prefix,
        "trait_synergies": synergies_result["activated_synergies"],
        "damage_modifiers": {
            "total_strike_mult": format!("{:.3}", modifiers.total_strike_mult()),
            "total_condi_mult": format!("{:.3}", modifiers.total_condi_mult()),
            "total_healing_mult": format!("{:.3}", modifiers.total_healing_mult()),
            "crit_damage_bonus": format!("{:.1}%", modifiers.total_crit_damage_bonus()),
            "condi_duration_bonus": format!("{:.1}%", modifiers.total_condi_duration_bonus()),
            "boon_duration_bonus": format!("{:.1}%", modifiers.total_boon_duration_bonus())
        },
        "condition_sources": all_conditions,
        "rune": rune_info
    })
}

fn exec_simulate_rotation(args: &Value, ctx: &ToolContext) -> Value {
    use crate::rotation;

    let skill_ids: Vec<u32> = args
        .get("skill_ids")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_u64().map(|n| n as u32))
                .collect()
        })
        .unwrap_or_default();

    if skill_ids.is_empty() {
        return json!({ "error": "No skill IDs provided" });
    }

    let duration_s = args
        .get("duration_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(30) as u32;

    // Get stats from gear prefix (or use sensible defaults)
    let gear_prefix = args
        .get("gear_prefix")
        .and_then(|v| v.as_str())
        .unwrap_or("Berserker's");
    // Use the deterministic, case-insensitive lookup so LLM-supplied prefixes
    // like "berserker's" don't silently fall through to hardcoded defaults
    // and run the simulation against the wrong stats.
    let (power, condition_damage, weapon_strength) =
        if let Some(istat) = find_itemstat_by_name(ctx.db, gear_prefix) {
            let gear_stats = calculate_full_set_stats(istat);
            let base = stats::base_stats();
            let power = base.power + gear_stats.power;
            let condition_damage = base.condition_damage + gear_stats.condition_damage;
            (power, condition_damage, 1100.0)
        } else {
            (2000.0, 1000.0, 1100.0) // fallback defaults
        };

    // Build rotation skills from the provided IDs
    let rotation_skills = rotation::builder::build_rotation_skills(&skill_ids, ctx.db);

    if rotation_skills.is_empty() {
        return json!({ "error": "No valid skills found for the provided IDs" });
    }

    // Run simulation
    let result = rotation::simulator::simulate(
        &rotation_skills,
        duration_s * 1000,
        power,
        condition_damage,
        weapon_strength,
    );

    // Format condition uptimes
    let mut condition_uptimes: Vec<Value> = result
        .condition_uptime
        .iter()
        .map(|(name, avg_stacks)| {
            json!({
                "condition": name,
                "avg_stacks": format!("{:.1}", avg_stacks)
            })
        })
        .collect();
    condition_uptimes.sort_by(|a, b| a["condition"].as_str().cmp(&b["condition"].as_str()));

    // Format buff uptimes
    let mut buff_uptimes: Vec<Value> = result
        .buff_uptime
        .iter()
        .map(|(name, fraction)| {
            json!({
                "buff": name,
                "uptime_pct": format!("{:.0}%", fraction * 100.0)
            })
        })
        .collect();
    buff_uptimes.sort_by(|a, b| a["buff"].as_str().cmp(&b["buff"].as_str()));

    // Format skill usage
    let skill_usage: Vec<Value> = result
        .skill_usage
        .iter()
        .map(|su| {
            json!({
                "skill": &su.name,
                "casts": su.cast_count,
                "dps": format!("{:.0}", su.dps_contribution)
            })
        })
        .collect();

    json!({
        "duration_s": duration_s,
        "dps": {
            "strike": format!("{:.0}", result.strike_dps),
            "condition": format!("{:.0}", result.condition_dps),
            "total": format!("{:.0}", result.total_dps)
        },
        "condition_uptime": condition_uptimes,
        "buff_uptime": buff_uptimes,
        "skill_usage": skill_usage,
        "control_metrics": {
            "stunbreak_count": result.stunbreak_count,
            "has_stability": result.has_stability,
            "stability_uptime_pct": format!("{:.0}%", result.stability_uptime * 100.0)
        },
        "skills_simulated": rotation_skills.len()
    })
}

// ─── Helpers ───

/// Calculate gear stats for a full set of one prefix using slot budget data.
/// Iterates all 16 equipment slots and applies budget-based major/minor values.
fn calculate_full_set_stats(itemstat: &ItemStat) -> stats::StatBlock {
    let mut gear_stats = stats::StatBlock::default();
    let budgets = data::slot_budgets::slot_budgets();
    let shape = data::stat_shape_from_attr_count(itemstat.attributes.len());
    for &(slot_type, _) in data::EQUIPMENT_SLOTS {
        if let Some(budget) = budgets.get(slot_type, shape) {
            crate::engine::add_budget_stats_for_itemstat(&mut gear_stats, itemstat, budget);
        }
    }
    gear_stats
}

/// Summarize a trait's key mechanical effects (for trait column overview).
fn summarize_trait_facts(t: &GW2Trait) -> Vec<String> {
    let mut effects = Vec::new();
    for fact in &t.facts {
        match fact {
            Fact::AttributeAdjust { text, value, .. } => {
                if let (Some(text), Some(val)) = (text, value) {
                    effects.push(format!("{}: {}", text, val));
                }
            }
            Fact::Buff {
                status, duration, ..
            }
            | Fact::PrefixedBuff {
                status, duration, ..
            } => {
                if let Some(status) = status {
                    let dur = duration.map(|d| format!(" ({}s)", d)).unwrap_or_default();
                    effects.push(format!("Applies {}{}", status, dur));
                }
            }
            Fact::Damage {
                hit_count,
                dmg_multiplier,
                ..
            } => {
                let hits = hit_count.unwrap_or(1);
                let mult = dmg_multiplier.unwrap_or(1.0);
                effects.push(format!("Damage: {}x {:.2}", hits, mult));
            }
            Fact::Percent { text, percent, .. } => {
                if let (Some(text), Some(pct)) = (text, percent) {
                    effects.push(format!("{}: {}%", text, pct));
                }
            }
            _ => {}
        }
        if effects.len() >= 5 {
            break;
        }
    }
    effects
}

/// Extract conditions applied by a set of facts.
///
/// API `Fact::Buff.status` may emit either verb-form (Poison) or canonical
/// (Poisoned) names. Inputs are normalized via `canonical_condition_name`
/// before filtering, and the canonical form is shipped in JSON so the LLM
/// sees one consistent name per condition. `Immobilized` stays in the
/// list — the resolver only knows `Immobilize→Immobile`.
fn extract_conditions(facts: &[Fact]) -> Vec<Value> {
    use crate::data::boon_condition_formulas::canonical_condition_name;
    let conditions = [
        "Bleeding",
        "Burning",
        "Poisoned",
        "Torment",
        "Confusion",
        "Vulnerability",
        "Weakness",
        "Blinded",
        "Chilled",
        "Crippled",
        "Fear",
        "Immobile",
        "Immobilized",
        "Slow",
        "Taunt",
    ];
    facts
        .iter()
        .filter_map(|f| match f {
            Fact::Buff {
                status: Some(s),
                duration,
                apply_count,
                ..
            }
            | Fact::PrefixedBuff {
                status: Some(s),
                duration,
                apply_count,
                ..
            } => {
                let canonical = canonical_condition_name(s);
                if conditions.contains(&canonical) {
                    Some(json!({
                        "condition": canonical,
                        "stacks": apply_count.unwrap_or(1),
                        "duration_s": duration.unwrap_or(0)
                    }))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

/// Extract boons/buffs applied by a set of facts.
fn extract_buffs(facts: &[Fact]) -> Vec<Value> {
    let boons = [
        "Might",
        "Fury",
        "Quickness",
        "Alacrity",
        "Protection",
        "Resolution",
        "Regeneration",
        "Vigor",
        "Stability",
        "Swiftness",
        "Resistance",
        "Aegis",
    ];
    facts
        .iter()
        .filter_map(|f| match f {
            Fact::Buff {
                status: Some(s),
                duration,
                apply_count,
                ..
            }
            | Fact::PrefixedBuff {
                status: Some(s),
                duration,
                apply_count,
                ..
            } if boons.contains(&s.as_str()) => Some(json!({
                "buff": s,
                "stacks": apply_count.unwrap_or(1),
                "duration_s": duration.unwrap_or(0)
            })),
            _ => None,
        })
        .collect()
}

/// Extract stat bonuses from AttributeAdjust facts.
fn extract_stat_bonuses(facts: &[Fact]) -> Vec<Value> {
    facts
        .iter()
        .filter_map(|f| {
            if let Fact::AttributeAdjust {
                target,
                value: Some(val),
                ..
            } = f
            {
                Some(json!({
                    "stat": target.as_deref().unwrap_or("unknown"),
                    "value": val
                }))
            } else {
                None
            }
        })
        .collect()
}

/// Extract damage modifier percentages from Percent facts.
fn extract_damage_modifiers_from_facts(facts: &[Fact]) -> Vec<Value> {
    facts
        .iter()
        .filter_map(|f| {
            if let Fact::Percent {
                text,
                percent: Some(pct),
                ..
            } = f
            {
                Some(json!({
                    "description": text.as_deref().unwrap_or(""),
                    "percent": pct
                }))
            } else {
                None
            }
        })
        .collect()
}

/// Detect proc triggers by scanning fact descriptions.
fn detect_proc_triggers(facts: &[Fact], description: Option<&str>) -> Vec<String> {
    let mut triggers = Vec::new();
    let trigger_patterns = [
        ("on crit", "on_critical_hit"),
        ("critical hit", "on_critical_hit"),
        ("on hit", "on_hit"),
        ("when you apply", "on_condition_apply"),
        ("when applying", "on_condition_apply"),
        ("on dodge", "on_dodge"),
        ("when you dodge", "on_dodge"),
        ("when struck", "when_struck"),
        ("when you are struck", "when_struck"),
        ("when health", "health_threshold"),
        ("above 90%", "health_above_90"),
        ("below 50%", "health_below_50"),
        ("on weapon swap", "on_weapon_swap"),
        ("on kill", "on_kill"),
        ("when you use a heal", "on_heal_skill"),
        ("on interval", "periodic"),
        ("recharge of 20", "long_recharge"),
        ("20 seconds or more", "long_recharge"),
        ("stunned", "on_cc"),
        ("disabled foe", "on_cc"),
        ("hitting a disabled", "on_cc"),
    ];

    let mut check_text = |text: &str| {
        let lower = text.to_lowercase();
        for &(pattern, trigger) in &trigger_patterns {
            if lower.contains(pattern) && !triggers.contains(&trigger.to_string()) {
                triggers.push(trigger.to_string());
            }
        }
    };

    // Check fact descriptions
    for fact in facts {
        match fact {
            Fact::Buff { text: Some(t), .. }
            | Fact::Percent { text: Some(t), .. }
            | Fact::AttributeAdjust { text: Some(t), .. }
            | Fact::Damage { text: Some(t), .. } => check_text(t),
            _ => {}
        }
    }

    // Check main description
    if let Some(desc) = description {
        check_text(desc);
    }

    triggers
}

/// Parse a rune bonus string into structured JSON.
fn parse_rune_bonus_structured(bonus: &str) -> Value {
    let lower = bonus.to_lowercase();

    // Stat bonus: "+125 Power", "+180 Precision", etc.
    if let Some(captures) = extract_stat_bonus(bonus) {
        return captures;
    }

    // Condition duration: "+10% Burning Duration"
    // Search terms stay in verb form (GW2 tooltip text uses "Poison Duration",
    // not "Poisoned Duration"); the emitted `condition` key is canonicalized
    // so LLM / downstream consumers see a single spelling per condition.
    for condi in &["Bleeding", "Burning", "Poison", "Torment", "Confusion"] {
        if lower.contains(&condi.to_lowercase()) && lower.contains("duration") {
            if let Some(pct) = extract_number(bonus) {
                let canonical =
                    crate::data::boon_condition_formulas::canonical_condition_name(condi);
                return json!({
                    "type": "condition_duration",
                    "condition": canonical,
                    "value_pct": pct,
                    "raw": bonus
                });
            }
        }
    }

    // Boon duration
    if lower.contains("boon duration") {
        if let Some(pct) = extract_number(bonus) {
            return json!({ "type": "boon_duration", "value_pct": pct, "raw": bonus });
        }
    }

    // Condition damage bonus
    if lower.contains("condition damage") {
        if let Some(val) = extract_number(bonus) {
            return json!({ "type": "stat", "stat": "ConditionDamage", "value": val, "raw": bonus });
        }
    }

    // Damage modifier: "+5% damage", "+7% Condition Damage"
    if lower.contains("% damage") || lower.contains("% all damage") {
        if let Some(pct) = extract_number(bonus) {
            return json!({ "type": "damage_modifier", "value_pct": pct, "raw": bonus });
        }
    }

    // Fallback: return raw text
    json!({ "type": "other", "raw": bonus })
}

/// Extract a stat bonus like "+125 Power" from text.
fn extract_stat_bonus(text: &str) -> Option<Value> {
    let stats = [
        "Power",
        "Precision",
        "Toughness",
        "Vitality",
        "Ferocity",
        "Condition Damage",
        "Expertise",
        "Concentration",
        "Healing Power",
    ];
    for stat in &stats {
        if text.contains(stat) {
            if let Some(val) = extract_number(text) {
                return Some(json!({
                    "type": "stat",
                    "stat": stat,
                    "value": val,
                    "raw": text
                }));
            }
        }
    }
    None
}

/// Extract the first number from text.
fn extract_number(text: &str) -> Option<f64> {
    let mut num_str = String::new();
    let mut found_digit = false;
    for ch in text.chars() {
        if ch.is_ascii_digit()
            || (ch == '.' && found_digit)
            || (ch == '+' && !found_digit)
            || (ch == '-' && !found_digit)
        {
            if ch != '+' {
                num_str.push(ch);
            }
            if ch.is_ascii_digit() {
                found_digit = true;
            }
        } else if found_digit {
            break;
        }
    }
    num_str.parse::<f64>().ok()
}

/// Format a single Fact into a JSON value for tool responses.
fn format_fact(fact: &Fact) -> Value {
    match fact {
        Fact::AttributeAdjust {
            text,
            target,
            value,
            ..
        } => json!({
            "type": "AttributeAdjust",
            "text": text,
            "target": target,
            "value": value
        }),
        Fact::Buff {
            text,
            status,
            duration,
            apply_count,
            ..
        } => json!({
            "type": "Buff",
            "text": text,
            "status": status,
            "duration": duration,
            "apply_count": apply_count
        }),
        Fact::BuffConversion {
            text,
            source,
            target,
            percent,
            ..
        } => json!({
            "type": "BuffConversion",
            "text": text,
            "source": source,
            "target": target,
            "percent": percent
        }),
        Fact::Damage {
            text,
            hit_count,
            dmg_multiplier,
            ..
        } => json!({
            "type": "Damage",
            "text": text,
            "hit_count": hit_count,
            "dmg_multiplier": dmg_multiplier
        }),
        Fact::Percent { text, percent, .. } => json!({
            "type": "Percent",
            "text": text,
            "percent": percent
        }),
        Fact::Recharge { text, value, .. } => json!({
            "type": "Recharge",
            "text": text,
            "value": value
        }),
        Fact::Number { text, value, .. } => json!({
            "type": "Number",
            "text": text,
            "value": value
        }),
        Fact::Duration { text, duration, .. } => json!({
            "type": "Duration",
            "text": text,
            "duration": duration
        }),
        Fact::ComboField {
            text, field_type, ..
        } => json!({
            "type": "ComboField",
            "text": text,
            "field_type": field_type
        }),
        Fact::ComboFinisher {
            text,
            finisher_type,
            percent,
            ..
        } => json!({
            "type": "ComboFinisher",
            "text": text,
            "finisher_type": finisher_type,
            "percent": percent
        }),
        Fact::Distance { text, distance, .. } => json!({
            "type": "Distance",
            "text": text,
            "distance": distance
        }),
        Fact::Radius { text, distance, .. } => json!({
            "type": "Radius",
            "text": text,
            "distance": distance
        }),
        Fact::Range { text, value, .. } => json!({
            "type": "Range",
            "text": text,
            "value": value
        }),
        Fact::Time { text, duration, .. } => json!({
            "type": "Time",
            "text": text,
            "duration": duration
        }),
        Fact::NoData { text, .. } => json!({
            "type": "NoData",
            "text": text
        }),
        Fact::PrefixedBuff {
            text,
            status,
            duration,
            apply_count,
            ..
        } => json!({
            "type": "PrefixedBuff",
            "text": text,
            "status": status,
            "duration": duration,
            "apply_count": apply_count
        }),
        Fact::StunBreak { text, value, .. } => json!({
            "type": "StunBreak",
            "text": text,
            "value": value
        }),
        Fact::HealingAdjust {
            text, hit_count, ..
        } => json!({
            "type": "HealingAdjust",
            "text": text,
            "hit_count": hit_count
        }),
        _ => json!({ "type": "Other" }),
    }
}

/// Format CombatPerformance into a concise JSON value.
fn format_combat_performance(perf: &CombatPerformance, label: &str) -> Value {
    json!({
        "profile": label,
        "effective_power": perf.effective_power.round() as i32,
        "strike_dps_index": perf.strike_dps_index.round() as i32,
        "condition_dps_index": perf.condition_dps_index.round() as i32,
        "total_dps_index": perf.total_dps_index.round() as i32,
        "healing_power_index": perf.healing_power_index.round() as i32,
        "crit_chance": format!("{:.1}%", perf.crit_chance),
        "boon_duration": format!("{:.1}%", perf.boon_duration_pct),
        "condi_duration": format!("{:.1}%", perf.condi_duration_pct),
        "effective_health": perf.effective_health.round() as i32,
        "damage_reduction": format!("{:.1}%", perf.damage_reduction_pct),
        "condition_ticks": {
            "bleeding": perf.condition_ticks.bleeding.round() as i32,
            "burning": perf.condition_ticks.burning.round() as i32,
            "poison": perf.condition_ticks.poison.round() as i32,
            "torment": perf.condition_ticks.torment.round() as i32,
            "confusion": perf.condition_ticks.confusion.round() as i32
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_declarations_count() {
        let tools = tool_declarations();
        assert_eq!(tools.len(), 1); // Single tool block
        assert_eq!(tools[0].function_declarations.len(), 20);
    }

    #[test]
    fn test_tool_declarations_have_names() {
        let tools = tool_declarations();
        let names: Vec<&str> = tools[0]
            .function_declarations
            .iter()
            .map(|d| d.name.as_str())
            .collect();
        assert!(names.contains(&"get_profession_info"));
        assert!(names.contains(&"get_spec_traits"));
        assert!(names.contains(&"simulate_combat"));
        assert!(names.contains(&"list_runes"));
        assert!(names.contains(&"search_upgrades"));
        assert!(names.contains(&"upgrade_synergies"));
        assert!(names.contains(&"get_optimizer_results"));
        // Synergy tools
        assert!(names.contains(&"search_traits_by_effect"));
        assert!(names.contains(&"find_condition_sources"));
        assert!(names.contains(&"search_skills_by_effect"));
        assert!(names.contains(&"find_synergies"));
        assert!(names.contains(&"get_build_synergy_report"));
        assert!(names.contains(&"simulate_rotation"));
    }

    #[test]
    fn test_format_combat_performance() {
        let perf = CombatPerformance {
            effective_power: 12345.6,
            strike_dps_index: 5000.0,
            condition_dps_index: 3000.0,
            total_dps_index: 8000.0,
            healing_power_index: 500.0,
            boon_duration_pct: 45.5,
            condi_duration_pct: 30.0,
            effective_health: 20000.0,
            damage_reduction_pct: 35.5,
            ..Default::default()
        };
        let result = format_combat_performance(&perf, "Solo");
        assert_eq!(result["profile"], "Solo");
        assert_eq!(result["effective_power"], 12346);
        assert_eq!(result["total_dps_index"], 8000);
    }

    // ── find_itemstat_by_name() determinism ──────────────────────────────────

    fn db_with_itemstats(stats: Vec<(u32, &str)>) -> GameDb {
        let mut itemstats = HashMap::new();
        for (id, name) in stats {
            itemstats.insert(
                id,
                ItemStat {
                    id,
                    name: name.into(),
                    attributes: vec![],
                },
            );
        }
        GameDb {
            items: HashMap::new(),
            itemstats,
            skills: HashMap::new(),
            traits: HashMap::new(),
            specializations: HashMap::new(),
            professions: HashMap::new(),
            legends: HashMap::new(),
            pvp_amulets: HashMap::new(),
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
        }
    }

    #[test]
    fn test_find_itemstat_exact_match_wins_over_substring() {
        let db = db_with_itemstats(vec![
            (10, "Marauder's Berserker Combo"),
            (11, "Berserker's"),
        ]);
        let is = find_itemstat_by_name(&db, "Berserker's").expect("match");
        assert_eq!(is.id, 11);
    }

    #[test]
    fn test_find_itemstat_fuzzy_prefers_shortest_name() {
        // Three substring matches. Run repeatedly — HashMap iteration would
        // otherwise leak through. Shortest name (Viper's = 7) must always win.
        let db = db_with_itemstats(vec![
            (20, "Trailblazer's Viper Combo"),
            (21, "Viper's"),
            (22, "Carrion-Viper Hybrid"),
        ]);
        for _ in 0..20 {
            let is = find_itemstat_by_name(&db, "Viper").expect("match");
            assert_eq!(is.id, 21, "Viper's must win every run");
        }
    }

    #[test]
    fn test_find_itemstat_empty_needle_returns_none() {
        let db = db_with_itemstats(vec![(30, "Berserker's")]);
        assert!(find_itemstat_by_name(&db, "").is_none());
    }

    #[test]
    fn test_find_itemstat_id_tiebreak_when_lengths_equal() {
        let db = db_with_itemstats(vec![(50, "Foo Alpha Bar"), (40, "Foo Beta Baz")]);
        // Both length 13, both contain "Foo". Lower id (40) must win.
        let is = find_itemstat_by_name(&db, "Foo").expect("match");
        assert_eq!(is.id, 40);
    }

    // ── find_named() generic lookup: Specialization + Trait ──────────────────

    fn make_spec(id: u32, name: &str) -> gw2_api::models::Specialization {
        gw2_api::models::Specialization {
            id,
            name: name.into(),
            profession: "Test".into(),
            elite: false,
            minor_traits: vec![],
            major_traits: vec![],
            weapon_trait: None,
            icon: None,
            background: None,
            profession_icon: None,
            profession_icon_big: None,
        }
    }

    fn make_trait(id: u32, name: &str) -> GW2Trait {
        GW2Trait {
            id,
            name: name.into(),
            icon: None,
            description: None,
            specialization: 0,
            tier: 1,
            order: 0,
            slot: "Major".into(),
            facts: vec![],
            traited_facts: vec![],
            skills: vec![],
        }
    }

    #[test]
    fn test_find_named_spec_exact_beats_substring() {
        let specs = vec![
            make_spec(1, "Berserker"),
            make_spec(2, "Berserker Variant Spec"),
        ];
        let s = find_named(&specs, "Berserker").expect("match");
        assert_eq!(s.id, 1);
    }

    #[test]
    fn test_find_named_spec_fuzzy_shortest_wins_deterministically() {
        // Two substring matches across many repeats — must be stable.
        let specs = vec![
            make_spec(10, "Soulbeast Special Edition"),
            make_spec(11, "Soulbeast"),
            make_spec(12, "Soulbeast Long Name Variant"),
        ];
        for _ in 0..20 {
            let s = find_named(&specs, "Soulbeast").expect("match");
            assert_eq!(s.id, 11, "shortest name must win every iteration");
        }
    }

    #[test]
    fn test_find_named_trait_fuzzy_id_tiebreak() {
        // Equal-length names containing needle → lower id wins.
        let traits = vec![
            make_trait(200, "Big Trait Alpha"),
            make_trait(100, "Big Trait Bravo"),
        ];
        let t = find_named(&traits, "Big").expect("match");
        assert_eq!(t.id, 100);
    }

    #[test]
    fn test_find_named_empty_needle_returns_none() {
        let traits = vec![make_trait(300, "Anything")];
        assert!(find_named(&traits, "").is_none());
    }
}
