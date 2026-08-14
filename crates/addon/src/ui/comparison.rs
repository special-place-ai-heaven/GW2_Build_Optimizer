//! Side-by-side build comparison view.
//! Shows current build vs optimized build with full stat tables, bonuses,
//! effects/resistances, and LLM explanation.

use nexus::imgui::{Selectable, TreeNodeFlags, Ui};

use gw2_core::types::{CombatMetrics, ResolvedBuild, RotationBreakdown, StatBlock};
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::ViabilityReport;

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
    /// Generated GW2 build-template chat code for this suggestion, when all required IDs are known.
    pub chat_code: Option<String>,
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
    /// Viability gate report from the referee (None for legacy/LLM paths that skip the referee).
    /// Populated for S07 Trust UI rendering.
    pub viability: Option<ViabilityReport>,
    /// Benchmark delta vs closest community reference build.
    /// None when no benchmark data has been scraped yet.
    pub benchmark_delta: Option<gw2_optimizer::benchmark::BenchmarkDelta>,
    /// Data quality assessment from the optimizer pipeline.
    pub data_quality: gw2_optimizer::data::DataQuality,
    /// Human-readable reasons for quality degradation (empty when Verified).
    pub quality_reasons: Vec<String>,
}

/// Which result view is showing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ResultPane {
    #[default]
    Build,
    Stats,
}

impl ResultPane {
    const ALL: [(ResultPane, &'static str); 2] =
        [(ResultPane::Build, "Build"), (ResultPane::Stats, "Stats")];
}

/// Compact section tabs as gold pills. Wraps instead of clipping on a narrow overlay.
pub fn render_result_pane_tabs(ui: &Ui, pane: &mut ResultPane) {
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0;
    for (i, (tab, label)) in ResultPane::ALL.iter().enumerate() {
        let pill_w = ui.calc_text_size(label)[0] + 20.0;
        if i > 0 {
            if row_x + pill_w + 6.0 > avail {
                row_x = 0.0;
            } else {
                ui.same_line_with_spacing(0.0, 6.0);
            }
        }
        let selected = *pane == *tab;
        if crate::ui::theme::pill(ui, label, selected, &format!("##pane_{}", label)) {
            *pane = *tab;
        }
        row_x += pill_w + 6.0;
    }
    ui.spacing();
}

/// Hover a skill/trait/upgrade name to show live GameDb description + facts.
pub fn inspect_if_hovered(ui: &Ui, name: &str, db: Option<&GameDb>) {
    if !ui.is_item_hovered() {
        return;
    }
    let Some(db) = db else {
        return;
    };
    let Some(tip) = inspect_text(name, db) else {
        return;
    };
    ui.tooltip(|| {
        let mut lines = tip.lines();
        if let Some(title) = lines.next() {
            ui.text_colored(crate::ui::theme::GOLD, title);
        }
        for line in lines {
            ui.text_wrapped(line);
        }
    });
}

pub(crate) fn compact_stance_name(name: &str) -> String {
    name.trim_start_matches("Legendary ")
        .trim_end_matches(" Stance")
        .to_string()
}

fn fact_line(fact: &gw2_api::models::facts::Fact) -> Option<String> {
    use gw2_api::models::facts::Fact;
    match fact {
        Fact::AttributeAdjust {
            target: Some(t),
            value: Some(v),
            ..
        } => Some(format!("{t}: {v:+}")),
        Fact::Buff {
            status: Some(s),
            duration,
            apply_count,
            ..
        } => {
            let dur = duration.map(|d| format!(" {d}s")).unwrap_or_default();
            let stacks = apply_count
                .filter(|&c| c > 1)
                .map(|c| format!(" x{c}"))
                .unwrap_or_default();
            Some(format!("Applies {s}{dur}{stacks}"))
        }
        Fact::PrefixedBuff {
            status: Some(s),
            duration,
            apply_count,
            prefix,
            ..
        } => {
            let dur = duration.map(|d| format!(" {d}s")).unwrap_or_default();
            let stacks = apply_count
                .filter(|&c| c > 1)
                .map(|c| format!(" x{c}"))
                .unwrap_or_default();
            let pfx = prefix
                .as_ref()
                .and_then(|p| p.status.as_ref())
                .map(|ps| format!(" (on {ps})"))
                .unwrap_or_default();
            Some(format!("Applies {s}{dur}{stacks}{pfx}"))
        }
        Fact::Damage {
            hit_count,
            dmg_multiplier,
            ..
        } => Some(format!(
            "Damage: {}\u{00d7} (coeff {:.2})",
            hit_count.unwrap_or(1),
            dmg_multiplier.unwrap_or(1.0)
        )),
        Fact::Heal { hit_count, .. } | Fact::HealingAdjust { hit_count, .. } => {
            Some(format!("Healing: {}\u{00d7}", hit_count.unwrap_or(1)))
        }
        Fact::Percent {
            text: Some(t),
            percent: Some(p),
            ..
        } => Some(format!("{t}: {p}%")),
        Fact::Recharge { value: Some(v), .. } => Some(format!("Recharge: {v}s")),
        Fact::Range { value: Some(v), .. } => Some(format!("Range: {v}")),
        Fact::Radius {
            distance: Some(d), ..
        } => Some(format!("Radius: {d}")),
        Fact::BuffConversion {
            source: Some(s),
            target: Some(t),
            percent: Some(p),
            ..
        } => Some(format!("Convert {p}% {s} \u{2192} {t}")),
        Fact::StunBreak {
            value: Some(true), ..
        } => Some("Stun break".to_string()),
        Fact::Unblockable {
            value: Some(true), ..
        } => Some("Unblockable".to_string()),
        Fact::ComboField {
            field_type: Some(ft),
            ..
        } => Some(format!("Combo field: {ft}")),
        Fact::ComboFinisher {
            finisher_type: Some(ft),
            percent,
            ..
        } => {
            let pct = percent.map(|p| format!(" ({p}%)")).unwrap_or_default();
            Some(format!("Combo finisher: {ft}{pct}"))
        }
        Fact::Number {
            text: Some(t),
            value: Some(v),
            ..
        } => Some(format!("{t}: {v}")),
        Fact::Duration {
            text: Some(t),
            duration: Some(d),
            ..
        } => Some(format!("{t}: {d}s")),
        _ => None,
    }
}

fn format_inspect_entry(
    name: &str,
    description: Option<&str>,
    facts: &[gw2_api::models::facts::Fact],
    traited_n: usize,
) -> String {
    let mut lines = vec![name.to_string()];
    if let Some(d) = description.filter(|d| !d.is_empty()) {
        lines.push(d.to_string());
    }
    let mut fact_lines: Vec<String> = facts.iter().filter_map(fact_line).collect();
    const MAX_FACTS: usize = 10;
    let extra = fact_lines.len().saturating_sub(MAX_FACTS);
    fact_lines.truncate(MAX_FACTS);
    lines.extend(fact_lines);
    if extra > 0 {
        lines.push(format!("(+{extra} more)"));
    }
    if traited_n > 0 {
        lines.push("Some numbers change with traits.".to_string());
    }
    lines.join("\n")
}

fn format_inspect_item(item: &gw2_api::models::Item) -> String {
    let mut lines = vec![item.name.clone()];
    if let Some(d) = item.description.as_deref().filter(|d| !d.is_empty()) {
        lines.push(d.to_string());
    }
    if let Some(details) = &item.details {
        for bonus in &details.bonuses {
            if !bonus.is_empty() {
                lines.push(bonus.clone());
            }
        }
    }
    lines.join("\n")
}

fn find_upgrade_item<'a>(db: &'a GameDb, name: &str) -> Option<&'a gw2_api::models::Item> {
    for id in db.runes.iter().chain(&db.sigils).chain(&db.relics) {
        let Some(item) = db.items.get(id) else {
            continue;
        };
        if item.name.eq_ignore_ascii_case(name) {
            return Some(item);
        }
        let stripped = item
            .name
            .strip_prefix("Superior ")
            .or_else(|| item.name.strip_prefix("Minor "))
            .unwrap_or(&item.name);
        if stripped.eq_ignore_ascii_case(name) {
            return Some(item);
        }
    }
    None
}

fn inspect_one(name: &str, db: &GameDb) -> Option<String> {
    let lookup = name.strip_suffix(" [E]").unwrap_or(name);
    let mut candidates = vec![lookup.to_string()];
    for prefix in ["Superior ", "Minor "] {
        if let Some(rest) = lookup.strip_prefix(prefix) {
            candidates.push(rest.to_string());
        }
    }
    candidates.push(format!("Legendary {lookup} Stance"));

    for c in &candidates {
        if let Some(skill) = db.skills.values().find(|s| {
            s.name.eq_ignore_ascii_case(c) || compact_stance_name(&s.name).eq_ignore_ascii_case(c)
        }) {
            return Some(format_inspect_entry(
                &skill.name,
                skill.description.as_deref(),
                &skill.facts,
                skill.traited_facts.len(),
            ));
        }
    }
    for c in &candidates {
        if let Some(tr) = db.traits.values().find(|t| t.name.eq_ignore_ascii_case(c)) {
            return Some(format_inspect_entry(
                &tr.name,
                tr.description.as_deref(),
                &tr.facts,
                tr.traited_facts.len(),
            ));
        }
    }
    for c in &candidates {
        if let Some(item) = find_upgrade_item(db, c) {
            return Some(format_inspect_item(item));
        }
    }
    None
}

pub(crate) fn inspect_text(name: &str, db: &GameDb) -> Option<String> {
    let name = name.trim();
    if name.is_empty() || name == "\u{2014}" || name == "-" || name == "(none)" || name == "(empty)"
    {
        return None;
    }
    if name.contains(" / ") {
        let tips: Vec<String> = name
            .split(" / ")
            .filter_map(|part| inspect_one(part.trim(), db))
            .collect();
        if !tips.is_empty() {
            return Some(tips.join("\n\n"));
        }
    }
    inspect_one(name, db)
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
    /// Which result section is visible (one at a time — density / no clip).
    pub result_pane: ResultPane,
    /// Build tab shows Optimized when a suggestion exists. Default true.
    pub show_optimized: bool,
    /// Frames remaining for "Copied" on the suggestion chat-code button.
    pub copy_feedback_frames: u32,
}

/// Render the comparison view: current build on left, suggestion on right.
/// Mutates suggestion tab + result pane. `db` is used for hover inspect.
pub fn render_comparison(
    ui: &Ui,
    current_build: &ResolvedBuild,
    current_stats: Option<&StatBlock>,
    comparison: &mut ComparisonState,
    db: Option<&GameDb>,
) {
    if comparison.loading {
        ui.text("Optimizing build...");
        ui.text("Running AI synergy analysis...");
        return;
    }

    if let Some(ref err) = comparison.error {
        ui.text_colored(crate::ui::theme::ERR, format!("Error: {}", err));
        return;
    }

    if comparison.suggestions.is_empty() {
        ui.text("No suggestions available. Run the optimizer first.");
        return;
    }

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
                comparison.selected_suggestion = i;
            }
            if i < tab_count - 1 {
                ui.same_line();
            }
        }
        ui.separator();
    }

    let idx = comparison.selected_suggestion.min(tab_count - 1);
    comparison.selected_suggestion = idx;

    render_data_quality_badge(ui, &comparison.suggestions[idx]);
    let chat_code = comparison.suggestions[idx].chat_code.clone();
    render_chat_code_copy(
        ui,
        chat_code.as_deref(),
        &format!("comparison_{}", idx),
        &mut comparison.copy_feedback_frames,
    );
    render_result_pane_tabs(ui, &mut comparison.result_pane);

    let pane = comparison.result_pane;
    let suggestion = comparison.suggestions[idx].clone();
    match pane {
        ResultPane::Build => {
            crate::ui::gear_sheet::render_view_toggle(ui, &mut comparison.show_optimized);
            let gain = crate::ui::gear_sheet::combat_gain(
                comparison.current_combat_solo.as_ref(),
                suggestion.combat_solo.as_ref(),
            );
            crate::ui::gear_sheet::render_current_sheet(
                ui,
                current_build,
                Some(&suggestion),
                db,
                comparison.show_optimized,
                gain,
            );
            ui.spacing();
            if comparison.show_optimized {
                render_skill_diff(ui, current_build, &suggestion, db);
            } else {
                crate::ui::main_view::build_display::render_build_skills(ui, current_build, db);
            }
            let explanation_text = if !suggestion.synergy_explanation.is_empty() {
                &suggestion.synergy_explanation
            } else {
                &suggestion.explanation
            };
            if !explanation_text.is_empty() {
                ui.spacing();
                ui.text_colored(
                    crate::ui::theme::MUTED,
                    "How to play (advisory — scores are the authority).",
                );
                ui.spacing();
                ui.text_wrapped(explanation_text);
            }
        }
        ResultPane::Stats => {
            render_stats_pane(ui, current_stats, comparison, &suggestion, db);
        }
    }
}

/// Rotation sim stores buff uptime as 0.0–1.0; some saves used 0–100.
fn display_uptime_pct(v: f64) -> f64 {
    if v <= 1.0 {
        v * 100.0
    } else {
        v
    }
}

pub(crate) fn render_stats_pane(
    ui: &Ui,
    current_stats: Option<&StatBlock>,
    comparison: &ComparisonState,
    suggestion: &BuildSuggestion,
    db: Option<&GameDb>,
) {
    ui.text_colored(crate::ui::theme::GOLD, "ATTRIBUTES");
    ui.text_colored(
        crate::ui::theme::MUTED,
        "Power and Condition Damage are damage flavors — not DPS.",
    );
    ui.spacing();
    render_primary_stats(ui, current_stats, suggestion.estimated_stats.as_ref());
    ui.spacing();
    render_defenses(ui, comparison, current_stats, suggestion);

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, "DAMAGE APPLICATION");
    ui.text_colored(
        crate::ui::theme::MUTED,
        "Strike (Power) or condition ticks. The scenario is the target's boons and conditions, not your attributes.",
    );
    ui.spacing();
    render_combat_performance(ui, comparison, suggestion);

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, "MODIFIERS");
    ui.text_colored(
        crate::ui::theme::MUTED,
        "Stacked from traits, runes, sigils, relics, and skills — already inside the damage numbers. Duration is the part shown separately.",
    );
    ui.spacing();
    if let Some(sug) = suggestion.combat_solo.as_ref() {
        let cur = comparison.current_combat_solo.as_ref();
        ui.columns(4, "##mod_dur", true);
        bonus_header(ui);
        render_pct_row_opt(
            ui,
            "Boon Duration",
            cur.map(|c| c.boon_duration_pct),
            sug.boon_duration_pct,
        );
        render_pct_row_opt(
            ui,
            "Condi Duration",
            cur.map(|c| c.condi_duration_pct),
            sug.condi_duration_pct,
        );
        ui.columns(1, "##mod_dur_end", false);
    }

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, "BOONS");
    ui.text_colored(
        crate::ui::theme::MUTED,
        "Application on you or allies. Strip and conversion on enemies are not scored yet.",
    );
    match suggestion.rotation.as_ref() {
        Some(rotation) => {
            if rotation.has_stability || rotation.stunbreak_count > 0 {
                ui.spacing();
                ui.text_colored(
                    crate::ui::theme::CREAM,
                    format!(
                        "Stability: {} ({:.0}%)  ·  stunbreaks: {}",
                        if rotation.has_stability { "yes" } else { "no" },
                        rotation.stability_uptime * 100.0,
                        rotation.stunbreak_count
                    ),
                );
            }
            if rotation.buff_uptime.is_empty() {
                ui.spacing();
                ui.text_colored(crate::ui::theme::MUTED, "No boon uptime in this rotation.");
            } else {
                ui.spacing();
                ui.text_colored(crate::ui::theme::GOLD, "Uptime");
                for (name, frac) in rotation.buff_uptime.iter().take(8) {
                    ui.text(format!("  {name}: {:.0}%", display_uptime_pct(*frac)));
                }
            }
        }
        None => {
            ui.spacing();
            ui.text_colored(crate::ui::theme::MUTED, "No rotation simulation.");
        }
    }

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, "CONDITIONS");
    ui.text_colored(
        crate::ui::theme::MUTED,
        "Application on the target. Cleanse removes them. Incoming strike reduction (Protection) is the mitigation we score; Resistance is not.",
    );
    if let Some(rotation) = suggestion.rotation.as_ref() {
        ui.spacing();
        ui.text_colored(
            crate::ui::theme::CREAM,
            format!(
                "Cleanse skills: {}  ·  conditions removed / 20s: {:.1}",
                rotation.cleanse_count, rotation.cleanse_rate_per_20s
            ),
        );
        if !rotation.condition_uptime.is_empty() {
            ui.spacing();
            ui.text_colored(crate::ui::theme::GOLD, "Application (avg stacks)");
            for (name, stacks) in rotation.condition_uptime.iter().take(8) {
                ui.text(format!("  {name}: {stacks:.1}"));
            }
        }
    }

    if let Some(ref rotation) = suggestion.rotation {
        ui.spacing();
        ui.text_colored(crate::ui::theme::GOLD, "ROTATION");
        render_rotation_breakdown(ui, rotation, db);
    }
    if let Some(ref viability) = suggestion.viability {
        render_viability_report(ui, viability);
    }
    render_tradeoff_analysis(ui, comparison, suggestion);
    render_benchmark_delta(ui, suggestion);
    if !suggestion.changes_made.is_empty() {
        ui.spacing();
        ui.text_colored(crate::ui::theme::GOLD, "Changes");
        for change in &suggestion.changes_made {
            ui.bullet_text(change);
        }
    }
}

/// Sticky copy strip for a GW2 build-template chat code.
pub fn render_chat_code_copy(
    ui: &Ui,
    chat_code: Option<&str>,
    id_suffix: &str,
    copied_frames: &mut u32,
) {
    let Some(code) = chat_code else {
        return;
    };

    if *copied_frames > 0 {
        *copied_frames = copied_frames.saturating_sub(1);
    }

    ui.spacing();
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect(
            [start[0] - 2.0, start[1] - 4.0],
            [start[0] + width + 2.0, start[1] + 48.0],
            crate::ui::theme::PLATE,
        )
        .filled(true)
        .rounding(6.0)
        .build();
        dl.add_rect(
            [start[0] - 2.0, start[1] - 4.0],
            [start[0] + width + 2.0, start[1] + 48.0],
            crate::ui::theme::GOLD_DIM,
        )
        .rounding(6.0)
        .build();
    }

    ui.dummy([4.0, 2.0]);
    ui.text_colored(crate::ui::theme::GOLD, "  Chat code");
    ui.same_line();
    if crate::ui::theme::gold_button(ui, format!("Copy##suggestion_chat_code_{}", id_suffix)) {
        ui.set_clipboard_text(code);
        *copied_frames = 120;
    }
    if *copied_frames > 0 {
        ui.same_line();
        ui.text_colored(
            crate::ui::theme::OPTIMIZED,
            "Copied \u{2014} paste in GW2 chat",
        );
    } else {
        ui.same_line();
        ui.text_colored(crate::ui::theme::MUTED, "Paste in GW2 chat to apply");
    }
    ui.set_next_item_width(-8.0);
    let mut code_buf = code.to_string();
    ui.input_text(
        &format!("##suggestion_chat_code_display_{}", id_suffix),
        &mut code_buf,
    )
    .read_only(true)
    .build();
    ui.dummy([0.0, 6.0]);
}

/// Render gear comparison table: per-slot diff between current and optimized build.
fn render_spec_diff(
    ui: &Ui,
    current: &ResolvedBuild,
    suggestion: &BuildSuggestion,
    db: Option<&GameDb>,
) {
    let diff = compute_build_diff(current, suggestion);
    ui.columns(4, "##spec_diff", true);
    diff_header(ui);
    for (spec_diff, trait_diff) in &diff.specializations {
        render_diff_row(ui, spec_diff, db);
        render_diff_row(ui, trait_diff, db);
    }
    ui.columns(1, "##spec_diff_end", false);
}

fn render_skill_diff(
    ui: &Ui,
    current: &ResolvedBuild,
    suggestion: &BuildSuggestion,
    db: Option<&GameDb>,
) {
    let diff = compute_build_diff(current, suggestion);
    ui.columns(4, "##skill_diff", true);
    diff_header(ui);
    for skill in &diff.skills {
        render_diff_row(ui, skill, db);
    }
    ui.columns(1, "##skill_diff_end", false);
}

fn render_weapon_diff(
    ui: &Ui,
    current: &ResolvedBuild,
    suggestion: &BuildSuggestion,
    db: Option<&GameDb>,
) {
    let diff = compute_build_diff(current, suggestion);
    ui.columns(4, "##weapon_diff", true);
    diff_header(ui);
    for (weapon_diff, sigil_diff) in &diff.weapon_sets {
        render_diff_row(ui, weapon_diff, db);
        render_diff_row(ui, sigil_diff, db);
    }
    ui.columns(1, "##weapon_diff_end", false);
}

fn render_upgrade_diff(
    ui: &Ui,
    current: &ResolvedBuild,
    suggestion: &BuildSuggestion,
    db: Option<&GameDb>,
) {
    let diff = compute_build_diff(current, suggestion);
    ui.columns(4, "##upgrade_diff", true);
    diff_header(ui);
    render_diff_row(ui, &diff.gear_prefix, db);
    render_diff_row(ui, &diff.rune, db);
    render_diff_row(ui, &diff.relic, db);
    ui.columns(1, "##upgrade_diff_end", false);
}

/// Render the 4-column header for diff tables.
fn diff_header(ui: &Ui) {
    ui.text_colored(crate::ui::theme::GOLD, "Slot");
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, "Current");
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, "Optimized");
    ui.next_column();
    ui.text("");
    ui.next_column();
    ui.separator();
}

/// Render one row in a 4-column diff table.
fn render_diff_row(ui: &Ui, diff: &SlotDiff, db: Option<&GameDb>) {
    let (badge, badge_col, badge_fill) = match diff.status {
        ChangeStatus::Unchanged => ("same", crate::ui::theme::MUTED, [0.14, 0.13, 0.11, 0.9]),
        ChangeStatus::Changed => ("new", crate::ui::theme::GOLD, crate::ui::theme::GOLD_HOVER),
    };

    let (cur_color, opt_color) = match diff.status {
        ChangeStatus::Unchanged => (crate::ui::theme::MUTED, crate::ui::theme::MUTED),
        ChangeStatus::Changed => (crate::ui::theme::CURRENT, crate::ui::theme::GOLD),
    };

    let is_sub = diff.slot_label.starts_with("  ");
    if is_sub {
        ui.text_colored(crate::ui::theme::MUTED, &diff.slot_label);
    } else {
        ui.text_colored(crate::ui::theme::CREAM, &diff.slot_label);
    }
    ui.next_column();

    ui.text_colored(cur_color, &diff.current_value);
    inspect_if_hovered(ui, &diff.current_value, db);
    ui.next_column();

    ui.text_colored(opt_color, &diff.proposed_value);
    inspect_if_hovered(ui, &diff.proposed_value, db);
    ui.next_column();

    let p = ui.cursor_screen_pos();
    let tw = ui.calc_text_size(badge)[0] + 10.0;
    let th = ui.text_line_height() + 2.0;
    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + tw, p[1] + th], badge_fill)
            .filled(true)
            .rounding(th * 0.4)
            .build();
        dl.add_text(
            [p[0] + 5.0, p[1] + 1.0],
            crate::ui::color_u32(badge_col),
            badge,
        );
    }
    ui.dummy([tw, th]);
    ui.next_column();
}

/// Render all 9 primary attributes in a comparison table.
fn render_primary_stats(ui: &Ui, current: Option<&StatBlock>, suggested: Option<&StatBlock>) {
    let cur = current.cloned().unwrap_or_default();
    let sug = suggested.cloned().unwrap_or_default();

    let stats = [
        ("Power", cur.power, sug.power),
        ("Precision", cur.precision, sug.precision),
        ("Ferocity", cur.ferocity, sug.ferocity),
        ("Condition Dmg", cur.condition_damage, sug.condition_damage),
        ("Expertise", cur.expertise, sug.expertise),
        ("Concentration", cur.concentration, sug.concentration),
        ("Toughness", cur.toughness, sug.toughness),
        ("Vitality", cur.vitality, sug.vitality),
        ("Healing Power", cur.healing_power, sug.healing_power),
    ];

    render_stat_table(ui, "##primary_stats", &stats);
}

/// One combat-performance row: (label, color, current metrics, suggested metrics).
type CombatTier<'a> = (
    &'a str,
    [f32; 4],
    Option<&'a CombatMetrics>,
    Option<&'a CombatMetrics>,
);

/// Render combat performance metrics with three tiers: Solo, Party, Full Squad.
fn render_combat_performance(ui: &Ui, comparison: &ComparisonState, suggestion: &BuildSuggestion) {
    let tiers: Vec<CombatTier> = vec![
        (
            "Solo (Gear + Traits)",
            [0.7, 0.85, 1.0, 1.0],
            comparison.current_combat_solo.as_ref(),
            suggestion.combat_solo.as_ref(),
        ),
        (
            "Party (Might x15, Fury)",
            [1.0, 0.85, 0.4, 1.0],
            comparison.current_combat_party.as_ref(),
            suggestion.combat_party.as_ref(),
        ),
        (
            "Full Squad (Might x25, Fury, Vuln x25)",
            [0.3, 1.0, 0.3, 1.0],
            comparison.current_combat_squad.as_ref(),
            suggestion.combat_squad.as_ref(),
        ),
    ];

    for (label, color, cur_combat, sug_combat) in &tiers {
        ui.text_colored(*color, *label);

        if let Some(sug) = sug_combat {
            ui.columns(4, format!("##{}_cols", label), true);
            bonus_header(ui);

            let cur = *cur_combat;
            render_int_row_opt(
                ui,
                "Effective Power",
                cur.map(|c| c.effective_power),
                sug.effective_power,
            );
            render_pct_row_opt(
                ui,
                "Crit Chance",
                cur.map(|c| c.crit_chance),
                sug.crit_chance,
            );
            render_int_row_opt(
                ui,
                "Strike DPS",
                cur.map(|c| c.strike_dps_index),
                sug.strike_dps_index,
            );
            render_int_row_opt(
                ui,
                "Condi DPS",
                cur.map(|c| c.condition_dps_index),
                sug.condition_dps_index,
            );
            render_int_row_opt(
                ui,
                "Total DPS",
                cur.map(|c| c.total_dps_index),
                sug.total_dps_index,
            );
            if sug.healing_index > 0 || cur.is_some_and(|c| c.healing_index > 0) {
                render_int_row_opt(
                    ui,
                    "Healing Index",
                    cur.map(|c| c.healing_index),
                    sug.healing_index,
                );
            }
            render_pct_row_opt(
                ui,
                "Incoming strike (Protection)",
                cur.map(|c| c.damage_reduction_pct),
                sug.damage_reduction_pct,
            );

            ui.columns(1, format!("##{}_end", label), false);
        } else {
            ui.text_colored(crate::ui::theme::MUTED, "  (not computed)");
        }

        ui.spacing();
    }

    if let Some(sug) = suggestion.combat_solo.as_ref() {
        let cur = comparison.current_combat_solo.as_ref();
        let ticks = [
            (
                "Bleeding",
                cur.map_or(0, |c| c.bleeding_tick),
                sug.bleeding_tick,
                "per stack/sec",
            ),
            (
                "Burning",
                cur.map_or(0, |c| c.burning_tick),
                sug.burning_tick,
                "per stack/sec",
            ),
            (
                "Poison",
                cur.map_or(0, |c| c.poison_tick),
                sug.poison_tick,
                "per stack/sec",
            ),
            (
                "Torment",
                cur.map_or(0, |c| c.torment_tick),
                sug.torment_tick,
                "stationary",
            ),
            (
                "Confusion",
                cur.map_or(0, |c| c.confusion_tick),
                sug.confusion_tick,
                "on skill use",
            ),
        ];
        let ticks_to_show: Vec<_> = ticks
            .iter()
            .filter(|(_, cur_v, sug_v, _)| *cur_v > 0 || *sug_v > 0)
            .collect();

        if !ticks_to_show.is_empty() {
            ui.text_colored(
                [0.9, 0.6, 0.2, 1.0],
                "Condition ticks (how condi damage applies, Solo)",
            );
            ui.columns(4, "##condi_ticks", true);

            ui.text_colored(crate::ui::theme::GOLD, "Condition");
            ui.next_column();
            ui.text_colored(crate::ui::theme::CURRENT, "Current");
            ui.next_column();
            ui.text_colored(crate::ui::theme::OPTIMIZED, "Optimized");
            ui.next_column();
            ui.text("Info");
            ui.next_column();
            ui.separator();

            for (name, cur_val, sug_val, info) in &ticks_to_show {
                ui.text(*name);
                ui.next_column();
                if *cur_val > 0 {
                    ui.text(format!("{}", cur_val));
                } else {
                    ui.text_colored(crate::ui::theme::MUTED, "-");
                }
                ui.next_column();
                let diff = *sug_val - *cur_val;
                ui.text(format!("{}", sug_val));
                if *cur_val > 0 && diff != 0 {
                    ui.same_line();
                    let color = if diff > 0 {
                        [0.0, 1.0, 0.0, 1.0]
                    } else {
                        [1.0, 0.0, 0.0, 1.0]
                    };
                    ui.text_colored(
                        color,
                        format!("({}{})", if diff > 0 { "+" } else { "" }, diff),
                    );
                }
                ui.next_column();
                ui.text_colored(crate::ui::theme::MUTED, *info);
                ui.next_column();
            }

            ui.columns(1, "##condi_ticks_end", false);
        }
    }
}

fn bonus_header(ui: &Ui) {
    ui.text_colored(crate::ui::theme::GOLD, "Metric");
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, "Current");
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();
}

/// Render defenses: Health and Armor (static stats that don't change with buff profile).
/// Effective HP and Damage Reduction are shown per-tier in Combat Performance.
fn render_defenses(
    ui: &Ui,
    comparison: &ComparisonState,
    current_stats: Option<&StatBlock>,
    suggestion: &BuildSuggestion,
) {
    let sug_stats = suggestion.estimated_stats.clone().unwrap_or_default();
    let cur = current_stats.cloned().unwrap_or_default();

    let stats = [
        ("Health", cur.health, sug_stats.health),
        ("Armor", cur.armor, sug_stats.armor),
    ];

    ui.columns(4, "##defense_cols", true);
    ui.text_colored(crate::ui::theme::GOLD, "Defense");
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, "Current");
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();

    for (name, cur_val, sug_val) in &stats {
        render_int_row(ui, name, *cur_val, *sug_val);
    }
    if let Some(sug_hp) = suggestion.combat_solo.as_ref().map(|c| c.effective_health) {
        render_int_row_opt(
            ui,
            "Effective HP",
            comparison
                .current_combat_solo
                .as_ref()
                .map(|c| c.effective_health),
            sug_hp,
        );
    }

    ui.columns(1, "##end_defense", false);
}

/// Render a table row for integer stats with diff.
fn render_int_row(ui: &Ui, name: &str, cur: i32, sug: i32) {
    ui.text(name);
    ui.next_column();
    ui.text(format!("{}", cur));
    ui.next_column();
    ui.text(format!("{}", sug));
    ui.next_column();
    let diff = sug - cur;
    let color = diff_color(diff as f64);
    let sign = if diff > 0 { "+" } else { "" };
    ui.text_colored(color, format!("{}{}", sign, diff));
    ui.next_column();
}

/// Render an integer row where the current value may be absent (shows "—" for current and diff).
fn render_int_row_opt(ui: &Ui, name: &str, cur: Option<i32>, sug: i32) {
    ui.text(name);
    ui.next_column();
    if let Some(c) = cur {
        ui.text(format!("{}", c));
        ui.next_column();
        ui.text(format!("{}", sug));
        ui.next_column();
        let diff = sug - c;
        let color = diff_color(diff as f64);
        let sign = if diff > 0 { "+" } else { "" };
        ui.text_colored(color, format!("{}{}", sign, diff));
    } else {
        ui.text_colored(crate::ui::theme::MUTED, "\u{2014}"); // em-dash
        ui.next_column();
        ui.text(format!("{}", sug));
        ui.next_column();
        ui.text_colored(crate::ui::theme::MUTED, "\u{2014}");
    }
    ui.next_column();
}

/// Render a percentage row where the current value may be absent.
fn render_pct_row_opt(ui: &Ui, name: &str, cur: Option<f64>, sug: f64) {
    ui.text(name);
    ui.next_column();
    if let Some(c) = cur {
        ui.text(format!("{:.1}%", c));
        ui.next_column();
        ui.text(format!("{:.1}%", sug));
        ui.next_column();
        let diff = sug - c;
        let color = diff_color(diff);
        let sign = if diff > 0.0 { "+" } else { "" };
        ui.text_colored(color, format!("{}{:.1}%", sign, diff));
    } else {
        ui.text_colored(crate::ui::theme::MUTED, "\u{2014}");
        ui.next_column();
        ui.text(format!("{:.1}%", sug));
        ui.next_column();
        ui.text_colored(crate::ui::theme::MUTED, "\u{2014}");
    }
    ui.next_column();
}

/// Render a stat comparison table with 4 columns.
fn render_stat_table(ui: &Ui, id: &str, stats: &[(&str, i32, i32)]) {
    ui.columns(4, id, true);

    ui.text_colored(crate::ui::theme::GOLD, "Attribute");
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, "Current");
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, "Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();
    ui.separator();

    for (name, cur, sug) in stats {
        render_int_row(ui, name, *cur, *sug);
    }

    ui.columns(1, format!("{}_end", id), false);
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
fn render_rotation_breakdown(ui: &Ui, rotation: &RotationBreakdown, db: Option<&GameDb>) {
    ui.text_colored(
        [0.55, 0.55, 0.55, 1.0],
        "Simulated — not a live combat log.",
    );
    ui.text(format!(
        "Simulated DPS: {} (Strike: {}, Condition: {})",
        rotation.simulated_dps, rotation.strike_dps, rotation.condition_dps
    ));
    ui.spacing();

    if !rotation.skill_usage.is_empty() {
        ui.text("Skill Usage:");
        for (name, casts, dps) in &rotation.skill_usage {
            if *casts > 0 {
                ui.text(format!("  {} x{} ({} DPS)", name, casts, dps));
                inspect_if_hovered(ui, name, db);
            }
        }
    }
}

// ─── Trust UI helpers ────────────────────────────────────────────────────────

/// Render data quality badge in comparison header.
fn render_data_quality_badge(ui: &Ui, suggestion: &BuildSuggestion) {
    use gw2_optimizer::data::DataQuality;
    let (label, col, tooltip_header): (&str, [f32; 4], &str) = match suggestion.data_quality {
        DataQuality::Verified => (
            "\u{25cf} Verified",
            [0.3, 0.9, 0.3, 1.0],
            "All input data is source-backed and verified.",
        ),
        DataQuality::Provisional => (
            "\u{25cf} Provisional",
            [0.95, 0.75, 0.15, 1.0],
            "Some data is estimated or provisional. Results are usable but less certain.",
        ),
        DataQuality::Blocked => (
            "\u{25cf} Blocked",
            [1.0, 0.3, 0.2, 1.0],
            "Critical data is missing. Results may not be reliable.",
        ),
    };

    ui.text_colored(col, label);
    if ui.is_item_hovered() {
        ui.tooltip(|| {
            ui.text(tooltip_header);
            if !suggestion.quality_reasons.is_empty() {
                ui.spacing();
                ui.text("Reasons:");
                for reason in &suggestion.quality_reasons {
                    ui.bullet_text(reason);
                }
            }
        });
    }
    ui.spacing();
}

/// Render benchmark delta vs community reference.
fn render_benchmark_delta(ui: &Ui, suggestion: &BuildSuggestion) {
    match &suggestion.benchmark_delta {
        None => {
            // No data — show subtle hint in collapsed section
            if ui.collapsing_header("vs Community Meta", TreeNodeFlags::empty()) {
                ui.text_colored(crate::ui::theme::MUTED, "  No benchmark data available.");
                ui.text_colored(
                    crate::ui::theme::MUTED,
                    "  Go to Settings \u{2192} Sync Benchmarks to download reference builds.",
                );
            }
        }
        Some(delta) => {
            let pct = delta.pct_of_ref;
            let (col, status): ([f32; 4], &str) = if pct >= 95.0 {
                ([0.3, 0.9, 0.3, 1.0], "on-par")
            } else if pct >= 80.0 {
                ([0.9, 0.8, 0.2, 1.0], "close")
            } else if pct >= 65.0 {
                ([0.9, 0.55, 0.1, 1.0], "below")
            } else {
                ([1.0, 0.3, 0.2, 1.0], "far below")
            };

            let header = format!(
                "vs {} meta: {:.0}% [{}]",
                title_case(&delta.source),
                pct,
                status
            );

            if ui.collapsing_header(&header, TreeNodeFlags::DEFAULT_OPEN) {
                ui.text_colored(col, format!("  {:.0}% of reference score", pct));
                ui.spacing();

                // Score bar
                let bar_width = ui.content_region_avail()[0] - 16.0;
                let filled = (bar_width * (pct / 100.0).min(1.0) as f32).max(0.0);
                let pos = ui.cursor_screen_pos();
                let draw = ui.get_window_draw_list();
                // Background
                draw.add_rect(
                    [pos[0] + 8.0, pos[1] + 2.0],
                    [pos[0] + bar_width + 8.0, pos[1] + 14.0],
                    [0.2, 0.2, 0.2, 0.8],
                )
                .filled(true)
                .build();
                // Fill
                if filled > 0.0 {
                    draw.add_rect(
                        [pos[0] + 8.0, pos[1] + 2.0],
                        [pos[0] + 8.0 + filled, pos[1] + 14.0],
                        col,
                    )
                    .filled(true)
                    .build();
                }
                ui.dummy([0.0, 18.0]);

                ui.spacing();
                ui.text(format!(
                    "  Reference: {} {} ({}) \u{2014} {}",
                    delta.profession, delta.role, delta.ref_gear_prefix, delta.source
                ));
                if !delta.ref_url.is_empty() {
                    ui.text_colored([0.4, 0.6, 0.9, 1.0], format!("  {}", delta.ref_url));
                }
            }
        }
    }
}

fn title_case(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Render the viability gate breakdown: pass/fail per gate with notes.
fn render_viability_report(ui: &Ui, report: &ViabilityReport) {
    let header_col = if report.is_viable {
        [0.3, 0.9, 0.3, 1.0]
    } else {
        [1.0, 0.35, 0.2, 1.0]
    };
    let status = if report.is_viable {
        "VIABLE"
    } else {
        "NON-VIABLE"
    };

    if ui.collapsing_header(
        format!("Viability Report [{}]", status),
        if report.is_viable {
            TreeNodeFlags::empty()
        } else {
            TreeNodeFlags::DEFAULT_OPEN
        },
    ) {
        ui.text_colored(header_col, format!("  Status: {}", status));
        ui.spacing();
        for gate in &report.gates {
            let (icon, col): (&str, [f32; 4]) = if gate.passed {
                ("\u{2705}", [0.4, 0.9, 0.4, 1.0])
            } else {
                ("\u{274C}", [1.0, 0.3, 0.2, 1.0])
            };
            let gate_name = format!("{:?}", gate.gate);
            ui.text_colored(col, format!("  {} {} — {}", icon, gate_name, gate.note));
        }
        if !report.is_viable {
            ui.spacing();
            ui.text_colored(
                [1.0, 0.7, 0.2, 1.0],
                "  Build scored -1.0 (non-viable). It won't rank against viable builds.",
            );
        }
    }
}

/// Render tradeoff analysis: what this build gains/loses vs current stats.
fn render_tradeoff_analysis(ui: &Ui, comparison: &ComparisonState, suggestion: &BuildSuggestion) {
    let Some(ref new_metrics) = suggestion.combat_solo else {
        return;
    };
    let Some(ref cur_metrics) = comparison.current_combat_solo else {
        // No current build data — show absolute metrics only
        if ui.collapsing_header("Performance Estimate", TreeNodeFlags::empty()) {
            ui.text_colored([0.8, 0.8, 0.8, 1.0], "  (No current build for comparison)");
            ui.text(format!(
                "  Strike DPS index: {:.0}",
                new_metrics.strike_dps_index
            ));
            ui.text(format!(
                "  Condi DPS index:  {:.0}",
                new_metrics.condition_dps_index
            ));
            ui.text(format!(
                "  Effective HP:     {:.0}",
                new_metrics.effective_health
            ));
        }
        return;
    };

    if ui.collapsing_header("Tradeoff Analysis", TreeNodeFlags::DEFAULT_OPEN) {
        ui.text_colored([0.8, 0.8, 0.5, 1.0], "  vs your current build:");
        ui.spacing();

        let tradeoffs = [
            (
                "Strike DPS",
                cur_metrics.strike_dps_index as f64,
                new_metrics.strike_dps_index as f64,
            ),
            (
                "Condi DPS",
                cur_metrics.condition_dps_index as f64,
                new_metrics.condition_dps_index as f64,
            ),
            (
                "Effective HP",
                cur_metrics.effective_health as f64,
                new_metrics.effective_health as f64,
            ),
            (
                "Healing Index",
                cur_metrics.healing_index as f64,
                new_metrics.healing_index as f64,
            ),
        ];

        for (label, old_val, new_val) in &tradeoffs {
            if *old_val < 0.01 && *new_val < 0.01 {
                continue; // Skip zero/negligible axes
            }
            let delta = new_val - old_val;
            let pct = if *old_val > 0.01 {
                delta / old_val * 100.0
            } else {
                0.0
            };
            let (arrow, col): (&str, [f32; 4]) = if delta > old_val * 0.02 {
                ("\u{2191}", [0.3, 0.9, 0.3, 1.0]) // ↑ green
            } else if delta < -old_val * 0.02 {
                ("\u{2193}", [1.0, 0.4, 0.3, 1.0]) // ↓ red
            } else {
                ("\u{2194}", [0.7, 0.7, 0.7, 1.0]) // ↔ neutral
            };
            ui.text_colored(
                col,
                format!("  {} {:14} {:+.0}  ({:+.1}%)", arrow, label, delta, pct),
            );
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
    fn build_suggestion_can_carry_copyable_chat_code() {
        let s = BuildSuggestion {
            chat_code: Some("[&DQIEAAA=]".to_string()),
            ..Default::default()
        };

        assert_eq!(s.chat_code.as_deref(), Some("[&DQIEAAA=]"));
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

    fn inspect_db_with_skill() -> GameDb {
        let mut db = gw2_optimizer::gamedb::GameDb::empty_for_tests();
        let skill: gw2_api::models::Skill = serde_json::from_value(serde_json::json!({
            "id": 1,
            "name": "Legendary Assassin Stance",
            "description": "Swap to this legend.",
            "facts": [
                {"type": "Recharge", "value": 10.0},
                {"type": "Range", "value": 900}
            ]
        }))
        .expect("skill fixture");
        db.skills.insert(1, skill);
        let item: gw2_api::models::Item = serde_json::from_value(serde_json::json!({
            "id": 2,
            "name": "Superior Rune of the Scholar",
            "type": "UpgradeComponent",
            "rarity": "Exotic",
            "level": 60,
            "details": {
                "bonuses": ["+25 Power", "+5% Strike Damage"]
            }
        }))
        .expect("item fixture");
        db.items.insert(2, item);
        db.runes.push(2);
        db
    }

    #[test]
    fn inspect_text_includes_facts_and_compact_stance() {
        let db = inspect_db_with_skill();
        let tip = inspect_text("Assassin", &db).expect("stance lookup");
        assert!(tip.contains("Legendary Assassin Stance"), "{tip}");
        assert!(tip.contains("Recharge: 10"), "{tip}");
        assert!(tip.contains("Range: 900"), "{tip}");
    }

    #[test]
    fn inspect_text_finds_rune_bonuses() {
        let db = inspect_db_with_skill();
        let tip = inspect_text("Rune of the Scholar", &db).expect("rune lookup");
        assert!(tip.contains("Superior Rune of the Scholar"), "{tip}");
        assert!(tip.contains("+5% Strike Damage"), "{tip}");
    }
}
