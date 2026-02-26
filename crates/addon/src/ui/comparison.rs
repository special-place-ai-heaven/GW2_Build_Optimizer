//! Side-by-side build comparison view.
//! Shows current build vs optimized build with full stat tables, bonuses,
//! effects/resistances, and LLM explanation.

use nexus::imgui::{Selectable, TreeNodeFlags, Ui};

use gw2_core::types::{CombatMetrics, ResolvedBuild, RotationBreakdown, StatBlock};

use super::gear_diff::{compute_build_diff, ChangeStatus, SlotDiff};

/// A build suggestion from the optimizer + LLM.
#[derive(Debug, Clone, Default)]
pub struct BuildSuggestion {
    pub label: String,
    pub build_summary: String,
    pub stat_prefix: String,
    pub specializations: Vec<(String, Vec<String>)>, // (spec_name, [trait1, trait2, trait3])
    pub weapons: Vec<String>,
    pub skills: Vec<String>,
    pub rune: String,
    pub sigils: Vec<String>,
    pub relic: String,
    pub explanation: String,
    /// Synergy-focused explanation from the new pipeline (preferred over `explanation`).
    pub synergy_explanation: String,
    pub changes_made: Vec<String>,
    pub estimated_stats: Option<StatBlock>,
    /// Combat metrics under Solo profile (gear+traits only).
    pub combat_solo: Option<CombatMetrics>,
    /// Combat metrics under Party profile (Might x15, Fury).
    pub combat_party: Option<CombatMetrics>,
    /// Combat metrics under Full Squad profile (Might x25, Fury, Vulnerability x25).
    pub combat_squad: Option<CombatMetrics>,
    /// Rotation simulation breakdown (if simulation was run).
    pub rotation: Option<RotationBreakdown>,
}

/// State for the comparison view.
#[derive(Default)]
pub struct ComparisonState {
    pub suggestions: Vec<BuildSuggestion>,
    pub selected_suggestion: usize,
    pub loading: bool,
    pub error: Option<String>,
    /// Combat metrics for the current build under each profile.
    pub current_combat_solo: Option<CombatMetrics>,
    pub current_combat_party: Option<CombatMetrics>,
    pub current_combat_squad: Option<CombatMetrics>,
}

/// Render the comparison view: current build on left, suggestion on right.
/// Returns Some(index) if the user clicked a different suggestion tab.
pub fn render_comparison(
    ui: &Ui,
    current_build: &ResolvedBuild,
    current_stats: Option<&StatBlock>,
    comparison: &ComparisonState,
) -> Option<usize> {
    let mut new_selection = None;
    if comparison.loading {
        ui.text("Optimizing build...");
        ui.text("Running AI synergy analysis...");
        return None;
    }

    if let Some(ref err) = comparison.error {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("Error: {}", err));
        return None;
    }

    if comparison.suggestions.is_empty() {
        ui.text("No suggestions available. Run the optimizer first.");
        return None;
    }

    // Suggestion tabs
    let tab_count = comparison.suggestions.len();
    if tab_count > 1 {
        for (i, suggestion) in comparison.suggestions.iter().enumerate() {
            let selected = comparison.selected_suggestion == i;
            let label = if suggestion.label.is_empty() {
                format!("Build {}", i + 1)
            } else if suggestion.label.starts_with("Score:") {
                format!("Option {} ({})", i + 1, suggestion.stat_prefix)
            } else {
                suggestion.label.clone()
            };
            if Selectable::new(&format!("{}##sug_{}", label, i))
                .selected(selected)
                .size([0.0, 0.0])
                .build(ui)
            {
                new_selection = Some(i);
            }
            if i < tab_count - 1 {
                ui.same_line();
            }
        }
        ui.separator();
    }

    let suggestion = &comparison.suggestions[comparison.selected_suggestion.min(tab_count - 1)];

    // ═══ Equipment Comparison ═══
    render_gear_comparison(ui, current_build, suggestion);

    // ═══ Stats Table ═══
    ui.spacing();
    if ui.collapsing_header("Primary Attributes", TreeNodeFlags::DEFAULT_OPEN) {
        render_primary_stats(ui, current_stats, suggestion.estimated_stats.as_ref());
    }

    // ═══ Combat Performance (3 tiers) ═══
    if ui.collapsing_header("Combat Performance", TreeNodeFlags::DEFAULT_OPEN) {
        render_combat_performance(ui, comparison, suggestion);
    }

    // ═══ Defenses & Resistances ═══
    if ui.collapsing_header("Defenses & Resistances", TreeNodeFlags::DEFAULT_OPEN) {
        render_defenses(ui, comparison, current_stats, suggestion);
    }

    // ═══ Rotation Breakdown (from simulation) ═══
    if let Some(ref rotation) = suggestion.rotation {
        if ui.collapsing_header("Rotation Breakdown", TreeNodeFlags::DEFAULT_OPEN) {
            render_rotation_breakdown(ui, rotation);
        }
    }

    // ═══ LLM Explanation ═══
    // Prefer synergy_explanation (from new pipeline), fall back to explanation (from old pipeline)
    let explanation_text = if !suggestion.synergy_explanation.is_empty() {
        &suggestion.synergy_explanation
    } else {
        &suggestion.explanation
    };
    if !explanation_text.is_empty() {
        if ui.collapsing_header("Why This Build?", TreeNodeFlags::DEFAULT_OPEN) {
            ui.text_wrapped(explanation_text);
        }
    }

    // ═══ Changes Made ═══
    if !suggestion.changes_made.is_empty() {
        if ui.collapsing_header("Changes Made", TreeNodeFlags::DEFAULT_OPEN) {
            for change in &suggestion.changes_made {
                ui.bullet_text(change);
            }
        }
    }

    new_selection
}

/// Render gear comparison table: per-slot diff between current and optimized build.
fn render_gear_comparison(ui: &Ui, current: &ResolvedBuild, suggestion: &BuildSuggestion) {
    let diff = compute_build_diff(current, suggestion);

    // ── Specializations ──
    if ui.collapsing_header("Specializations", TreeNodeFlags::DEFAULT_OPEN) {
        ui.columns(4, "##spec_diff", true);
        diff_header(ui);
        for (spec_diff, trait_diff) in &diff.specializations {
            render_diff_row(ui, spec_diff);
            render_diff_row(ui, trait_diff);
        }
        ui.columns(1, "##spec_diff_end", false);
    }

    // ── Skills ──
    if ui.collapsing_header("Skills", TreeNodeFlags::DEFAULT_OPEN) {
        ui.columns(4, "##skill_diff", true);
        diff_header(ui);
        for skill in &diff.skills {
            render_diff_row(ui, skill);
        }
        ui.columns(1, "##skill_diff_end", false);
    }

    // ── Weapons & Sigils ──
    if ui.collapsing_header("Weapons & Sigils", TreeNodeFlags::DEFAULT_OPEN) {
        ui.columns(4, "##weapon_diff", true);
        diff_header(ui);
        for (weapon_diff, sigil_diff) in &diff.weapon_sets {
            render_diff_row(ui, weapon_diff);
            render_diff_row(ui, sigil_diff);
        }
        ui.columns(1, "##weapon_diff_end", false);
    }

    // ── Upgrades ──
    if ui.collapsing_header("Upgrades", TreeNodeFlags::DEFAULT_OPEN) {
        ui.columns(4, "##upgrade_diff", true);
        diff_header(ui);
        render_diff_row(ui, &diff.gear_prefix);
        render_diff_row(ui, &diff.rune);
        render_diff_row(ui, &diff.relic);
        ui.columns(1, "##upgrade_diff_end", false);
    }
}

/// Render the 4-column header for diff tables.
fn diff_header(ui: &Ui) {
    ui.text_colored([0.8, 0.8, 0.2, 1.0], "Slot");
    ui.next_column();
    ui.text_colored([0.6, 0.8, 1.0, 1.0], "Current");
    ui.next_column();
    ui.text_colored([0.3, 1.0, 0.3, 1.0], "Optimized");
    ui.next_column();
    ui.text("");
    ui.next_column();
    ui.separator();
}

/// Render one row in a 4-column diff table.
fn render_diff_row(ui: &Ui, diff: &SlotDiff) {
    let (indicator, indicator_color) = match diff.status {
        ChangeStatus::Unchanged => ("=", [0.4, 0.4, 0.4, 0.6]),
        ChangeStatus::Changed => ("*", [1.0, 0.85, 0.3, 1.0]),
    };

    let (cur_color, opt_color) = match diff.status {
        ChangeStatus::Unchanged => ([0.5, 0.5, 0.5, 1.0], [0.5, 0.5, 0.5, 1.0]),
        ChangeStatus::Changed => ([0.6, 0.8, 1.0, 1.0], [1.0, 0.85, 0.3, 1.0]),
    };

    // Slot label — indent sub-rows
    let is_sub = diff.slot_label.starts_with("  ");
    if is_sub {
        ui.text_colored([0.6, 0.6, 0.6, 1.0], &diff.slot_label);
    } else {
        ui.text(&diff.slot_label);
    }
    ui.next_column();

    ui.text_colored(cur_color, &diff.current_value);
    ui.next_column();

    ui.text_colored(opt_color, &diff.proposed_value);
    ui.next_column();

    ui.text_colored(indicator_color, indicator);
    ui.next_column();
}

/// Render all 9 primary attributes in a comparison table.
fn render_primary_stats(ui: &Ui, current: Option<&StatBlock>, suggested: Option<&StatBlock>) {
    let cur = current.cloned().unwrap_or_default();
    let sug = suggested.cloned().unwrap_or_default();

    let stats = [
        ("Power",          cur.power,            sug.power),
        ("Precision",      cur.precision,         sug.precision),
        ("Ferocity",       cur.ferocity,          sug.ferocity),
        ("Condition Dmg",  cur.condition_damage,  sug.condition_damage),
        ("Expertise",      cur.expertise,         sug.expertise),
        ("Concentration",  cur.concentration,     sug.concentration),
        ("Toughness",      cur.toughness,         sug.toughness),
        ("Vitality",       cur.vitality,          sug.vitality),
        ("Healing Power",  cur.healing_power,     sug.healing_power),
    ];

    render_stat_table(ui, "##primary_stats", &stats);
}

/// Render combat performance metrics with three tiers: Solo, Party, Full Squad.
fn render_combat_performance(ui: &Ui, comparison: &ComparisonState, suggestion: &BuildSuggestion) {
    let tiers: Vec<(&str, [f32; 4], Option<&CombatMetrics>, Option<&CombatMetrics>)> = vec![
        ("Solo (Gear + Traits)", [0.7, 0.85, 1.0, 1.0], comparison.current_combat_solo.as_ref(), suggestion.combat_solo.as_ref()),
        ("Party (Might x15, Fury)", [1.0, 0.85, 0.4, 1.0], comparison.current_combat_party.as_ref(), suggestion.combat_party.as_ref()),
        ("Full Squad (Might x25, Fury, Vuln x25)", [0.3, 1.0, 0.3, 1.0], comparison.current_combat_squad.as_ref(), suggestion.combat_squad.as_ref()),
    ];

    for (label, color, cur_combat, sug_combat) in &tiers {
        ui.text_colored(*color, *label);

        if let Some(sug) = sug_combat {
            ui.columns(4, &format!("##{}_cols", label), true);
            bonus_header(ui);

            let cur = *cur_combat;
            render_int_row_opt(ui, "Effective Power", cur.map(|c| c.effective_power), sug.effective_power);
            render_pct_row_opt(ui, "Crit Chance", cur.map(|c| c.crit_chance), sug.crit_chance);
            render_int_row_opt(ui, "Strike DPS Index", cur.map(|c| c.strike_dps_index), sug.strike_dps_index);
            render_int_row_opt(ui, "Condi DPS Index", cur.map(|c| c.condition_dps_index), sug.condition_dps_index);
            render_int_row_opt(ui, "Total DPS Index", cur.map(|c| c.total_dps_index), sug.total_dps_index);
            render_pct_row_opt(ui, "Boon Duration", cur.map(|c| c.boon_duration_pct), sug.boon_duration_pct);
            render_pct_row_opt(ui, "Condi Duration", cur.map(|c| c.condi_duration_pct), sug.condi_duration_pct);
            if sug.healing_index > 0 || cur.map_or(false, |c| c.healing_index > 0) {
                render_int_row_opt(ui, "Healing Index", cur.map(|c| c.healing_index), sug.healing_index);
            }
            render_int_row_opt(ui, "Effective HP", cur.map(|c| c.effective_health), sug.effective_health);
            render_pct_row_opt(ui, "Dmg Reduction", cur.map(|c| c.damage_reduction_pct), sug.damage_reduction_pct);

            ui.columns(1, &format!("##{}_end", label), false);
        } else {
            ui.text_colored([0.5, 0.5, 0.5, 1.0], "  (not computed)");
        }

        ui.spacing();
    }

    // ─── Condition Tick Breakdown ───
    if let Some(sug) = suggestion.combat_solo.as_ref() {
        let cur = comparison.current_combat_solo.as_ref();
        let ticks = [
            ("Bleeding", cur.map_or(0, |c| c.bleeding_tick), sug.bleeding_tick, "per stack/sec"),
            ("Burning", cur.map_or(0, |c| c.burning_tick), sug.burning_tick, "per stack/sec"),
            ("Poison", cur.map_or(0, |c| c.poison_tick), sug.poison_tick, "per stack/sec"),
            ("Torment", cur.map_or(0, |c| c.torment_tick), sug.torment_tick, "stationary"),
            ("Confusion", cur.map_or(0, |c| c.confusion_tick), sug.confusion_tick, "on skill use"),
        ];
        let ticks_to_show: Vec<_> = ticks.iter()
            .filter(|(_, cur_v, sug_v, _)| *cur_v > 0 || *sug_v > 0)
            .collect();

        if !ticks_to_show.is_empty() {
            ui.text_colored([0.9, 0.6, 0.2, 1.0], "Condition Ticks (per tick, Solo)");
            ui.columns(4, "##condi_ticks", true);

            ui.text_colored([0.8, 0.8, 0.2, 1.0], "Condition");
            ui.next_column();
            ui.text_colored([0.6, 0.8, 1.0, 1.0], "Current");
            ui.next_column();
            ui.text_colored([0.3, 1.0, 0.3, 1.0], "Optimized");
            ui.next_column();
            ui.text("Info");
            ui.next_column();
            ui.separator();

            for (name, cur_val, sug_val, info) in &ticks_to_show {
                ui.text(*name);
                ui.next_column();
                if *cur_val > 0 {
                    ui.text(&format!("{}", cur_val));
                } else {
                    ui.text_colored([0.5, 0.5, 0.5, 1.0], "-");
                }
                ui.next_column();
                let diff = *sug_val - *cur_val;
                ui.text(&format!("{}", sug_val));
                if *cur_val > 0 && diff != 0 {
                    ui.same_line();
                    let color = if diff > 0 { [0.0, 1.0, 0.0, 1.0] } else { [1.0, 0.0, 0.0, 1.0] };
                    ui.text_colored(color, &format!("({}{})", if diff > 0 { "+" } else { "" }, diff));
                }
                ui.next_column();
                ui.text_colored([0.5, 0.5, 0.5, 1.0], *info);
                ui.next_column();
            }

            ui.columns(1, "##condi_ticks_end", false);
        }
    }
}

fn bonus_header(ui: &Ui) {
    ui.text_colored([0.8, 0.8, 0.2, 1.0], "Metric");
    ui.next_column();
    ui.text_colored([0.6, 0.8, 1.0, 1.0], "Current");
    ui.next_column();
    ui.text_colored([0.3, 1.0, 0.3, 1.0], "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();
}

/// Render defenses: Health and Armor (static stats that don't change with buff profile).
/// Effective HP and Damage Reduction are shown per-tier in Combat Performance.
fn render_defenses(ui: &Ui, _comparison: &ComparisonState, current_stats: Option<&StatBlock>, suggestion: &BuildSuggestion) {
    let sug_stats = suggestion.estimated_stats.clone().unwrap_or_default();
    let cur = current_stats.cloned().unwrap_or_default();

    let stats = [
        ("Health", cur.health, sug_stats.health),
        ("Armor", cur.armor, sug_stats.armor),
    ];

    ui.columns(4, "##defense_cols", true);
    ui.text_colored([0.8, 0.8, 0.2, 1.0], "Defense");
    ui.next_column();
    ui.text_colored([0.6, 0.8, 1.0, 1.0], "Current");
    ui.next_column();
    ui.text_colored([0.3, 1.0, 0.3, 1.0], "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();

    for (name, cur_val, sug_val) in &stats {
        render_int_row(ui, name, *cur_val, *sug_val);
    }

    ui.columns(1, "##end_defense", false);
}

/// Render a table row for integer stats with diff.
fn render_int_row(ui: &Ui, name: &str, cur: i32, sug: i32) {
    ui.text(name);
    ui.next_column();
    ui.text(&format!("{}", cur));
    ui.next_column();
    ui.text(&format!("{}", sug));
    ui.next_column();
    let diff = sug - cur;
    let color = diff_color(diff as f64);
    let sign = if diff > 0 { "+" } else { "" };
    ui.text_colored(color, &format!("{}{}", sign, diff));
    ui.next_column();
}


/// Render an integer row where the current value may be absent (shows "—" for current and diff).
fn render_int_row_opt(ui: &Ui, name: &str, cur: Option<i32>, sug: i32) {
    ui.text(name);
    ui.next_column();
    if let Some(c) = cur {
        ui.text(&format!("{}", c));
        ui.next_column();
        ui.text(&format!("{}", sug));
        ui.next_column();
        let diff = sug - c;
        let color = diff_color(diff as f64);
        let sign = if diff > 0 { "+" } else { "" };
        ui.text_colored(color, &format!("{}{}", sign, diff));
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "\u{2014}"); // em-dash
        ui.next_column();
        ui.text(&format!("{}", sug));
        ui.next_column();
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "\u{2014}");
    }
    ui.next_column();
}

/// Render a percentage row where the current value may be absent.
fn render_pct_row_opt(ui: &Ui, name: &str, cur: Option<f64>, sug: f64) {
    ui.text(name);
    ui.next_column();
    if let Some(c) = cur {
        ui.text(&format!("{:.1}%", c));
        ui.next_column();
        ui.text(&format!("{:.1}%", sug));
        ui.next_column();
        let diff = sug - c;
        let color = diff_color(diff);
        let sign = if diff > 0.0 { "+" } else { "" };
        ui.text_colored(color, &format!("{}{:.1}%", sign, diff));
    } else {
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "\u{2014}");
        ui.next_column();
        ui.text(&format!("{:.1}%", sug));
        ui.next_column();
        ui.text_colored([0.5, 0.5, 0.5, 1.0], "\u{2014}");
    }
    ui.next_column();
}

/// Render a stat comparison table with 4 columns.
fn render_stat_table(ui: &Ui, id: &str, stats: &[(&str, i32, i32)]) {
    ui.columns(4, id, true);

    ui.text_colored([0.8, 0.8, 0.2, 1.0], "Attribute");
    ui.next_column();
    ui.text_colored([0.6, 0.8, 1.0, 1.0], "Current");
    ui.next_column();
    ui.text_colored([0.3, 1.0, 0.3, 1.0], "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();

    for (name, cur, sug) in stats {
        render_int_row(ui, name, *cur, *sug);
    }

    ui.columns(1, &format!("{}_end", id), false);
}

/// Color for a diff value: green=positive, red=negative, gray=zero.
fn diff_color(diff: f64) -> [f32; 4] {
    if diff > 0.5 {
        [0.0, 1.0, 0.0, 1.0]
    } else if diff < -0.5 {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        [0.7, 0.7, 0.7, 1.0]
    }
}


/// Render rotation simulation breakdown: simulated DPS, condition uptimes, skill usage.
fn render_rotation_breakdown(ui: &Ui, rotation: &RotationBreakdown) {
    // DPS summary row
    ui.text(format!(
        "Simulated DPS: {} (Strike: {}, Condition: {})",
        rotation.simulated_dps, rotation.strike_dps, rotation.condition_dps
    ));
    ui.spacing();

    // Control metrics
    if rotation.stunbreak_count > 0 || rotation.has_stability {
        let mut parts = Vec::new();
        if rotation.stunbreak_count > 0 {
            parts.push(format!("Stunbreaks: {}", rotation.stunbreak_count));
        }
        if rotation.has_stability {
            parts.push(format!("Stability: {:.0}%", rotation.stability_uptime * 100.0));
        }
        ui.text_colored([0.4, 0.9, 0.4, 1.0], parts.join("  |  "));
        ui.spacing();
    }

    // Condition uptime
    if !rotation.condition_uptime.is_empty() {
        ui.text("Condition Uptime (avg stacks):");
        for (name, stacks) in &rotation.condition_uptime {
            if *stacks > 0.01 {
                ui.text(format!("  {}: {:.1}", name, stacks));
            }
        }
        ui.spacing();
    }

    // Buff uptime
    if !rotation.buff_uptime.is_empty() {
        ui.text("Buff Uptime:");
        for (name, pct) in &rotation.buff_uptime {
            if *pct > 0.01 {
                ui.text(format!("  {}: {:.0}%", name, pct * 100.0));
            }
        }
        ui.spacing();
    }

    // Skill usage table
    if !rotation.skill_usage.is_empty() {
        ui.text("Skill Usage:");
        for (name, casts, dps) in &rotation.skill_usage {
            if *casts > 0 {
                ui.text(format!("  {} x{} ({} DPS)", name, casts, dps));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_suggestion_default() {
        let s = BuildSuggestion::default();
        assert!(s.label.is_empty());
        assert!(s.specializations.is_empty());
        assert!(s.weapons.is_empty());
    }

    #[test]
    fn test_diff_color() {
        let green = diff_color(100.0);
        assert_eq!(green, [0.0, 1.0, 0.0, 1.0]);
        let red = diff_color(-50.0);
        assert_eq!(red, [1.0, 0.0, 0.0, 1.0]);
        let gray = diff_color(0.0);
        assert_eq!(gray, [0.7, 0.7, 0.7, 1.0]);
    }
}
