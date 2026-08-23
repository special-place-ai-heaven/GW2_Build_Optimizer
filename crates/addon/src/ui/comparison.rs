//! Side-by-side build comparison view.
//! Shows current build vs optimized build with full stat tables, bonuses,
//! effects/resistances, and LLM explanation.

use nexus::imgui::{Selectable, TreeNodeFlags, Ui};

use gw2_core::types::{CombatMetrics, ResolvedBuild, RotationBreakdown, StatBlock};
use gw2_optimizer::gamedb::GameDb;
use gw2_optimizer::ViabilityReport;

use gw2_core::i18n::{t, tf};

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
    const ALL: [(ResultPane, &'static str); 2] = [
        (ResultPane::Build, "pane.build"),
        (ResultPane::Stats, "pane.stats"),
    ];
}

/// Compact section tabs as gold pills. Wraps instead of clipping on a narrow overlay.
pub fn render_result_pane_tabs(ui: &Ui, pane: &mut ResultPane) {
    let avail = ui.content_region_avail()[0];
    let mut row_x = 0.0;
    for (i, (tab, key)) in ResultPane::ALL.iter().enumerate() {
        let label = t(key);
        let pill_w = ui.calc_text_size(&label)[0] + 20.0;
        if i > 0 {
            if row_x + pill_w + 6.0 > avail {
                row_x = 0.0;
            } else {
                ui.same_line_with_spacing(0.0, 6.0);
            }
        }
        let selected = *pane == *tab;
        if crate::ui::theme::pill(ui, &label, selected, &format!("##pane_{i}")) {
            *pane = *tab;
        }
        row_x += pill_w + 6.0;
    }
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
    crate::ui::theme::wide_tooltip(ui, |ui| {
        let mut lines = tip.lines();
        if let Some(title) = lines.next() {
            ui.text_colored(crate::ui::theme::GOLD, title);
        }
        for line in lines {
            ui.text(line);
        }
    });
}

pub fn loc_name<'a>(db: Option<&'a GameDb>, english: &'a str) -> &'a str {
    db.map(|d| d.loc_name(english)).unwrap_or(english)
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
            text,
            target: Some(t),
            value: Some(v),
            ..
        } => {
            if gw2_optimizer::stats::is_permanent_stat_adjust(text.as_deref()) {
                Some(format!("{t}: {v:+}"))
            } else {
                text.as_ref().map(|label| format!("{label}: {v}"))
            }
        }
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
}

/// Which build the top Chat strip is copying.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatSource {
    Character,
    Optimized,
}

impl ChatSource {
    pub fn label(self) -> String {
        t(self.i18n_key())
    }

    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::Character => "label.character",
            Self::Optimized => "label.optimized",
        }
    }

    pub fn color(self) -> [f32; 4] {
        match self {
            Self::Character => crate::ui::theme::CURRENT,
            Self::Optimized => crate::ui::theme::OPTIMIZED,
        }
    }
}

impl ComparisonState {
    /// Chat code for the Current / Optimized focus. One strip at the top follows this.
    pub fn chat_focus(&self, current_code: Option<&str>) -> (ChatSource, Option<String>) {
        if self.show_optimized && !self.suggestions.is_empty() {
            let idx = self.selected_suggestion.min(self.suggestions.len() - 1);
            (
                ChatSource::Optimized,
                self.suggestions[idx].chat_code.clone(),
            )
        } else {
            (ChatSource::Character, current_code.map(str::to_string))
        }
    }
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
        ui.text(t("cmp.optimizing"));
        ui.text(t("cmp.ai"));
        return;
    }

    if let Some(ref err) = comparison.error {
        ui.text_colored(crate::ui::theme::ERR, tf("setup.error", &[("msg", err)]));
        return;
    }

    if comparison.suggestions.is_empty() {
        ui.text(t("cmp.none"));
        return;
    }

    let tab_count = comparison.suggestions.len();
    if tab_count > 1 {
        for (i, suggestion) in comparison.suggestions.iter().enumerate() {
            let selected = comparison.selected_suggestion == i;
            let label = if suggestion.label.is_empty() {
                tf("fmt.build_n", &[("n", &(i + 1).to_string())])
            } else if suggestion.label.starts_with("Score:") {
                tf(
                    "fmt.option_n",
                    &[
                        ("n", &(i + 1).to_string()),
                        ("prefix", &suggestion.stat_prefix),
                    ],
                )
            } else {
                suggestion.label.clone()
            };
            if Selectable::new(&format!("{}##sug_{}", label, i))
                .selected(selected)
                .size([0.0, 0.0])
                .build(ui)
            {
                comparison.selected_suggestion = i;
                comparison.show_optimized = true;
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
    render_result_pane_tabs(ui, &mut comparison.result_pane);
    if comparison.result_pane == ResultPane::Build {
        ui.same_line_with_spacing(0.0, 16.0);
        crate::ui::gear_sheet::render_view_toggle(ui, &mut comparison.show_optimized);
    }
    ui.spacing();

    let pane = comparison.result_pane;
    let suggestion = comparison.suggestions[idx].clone();
    match pane {
        ResultPane::Build => {
            let viewing = comparison.show_optimized;
            if viewing {
                crate::ui::main_view::build_display::render_suggestion_skills(ui, &suggestion, db);
            } else {
                crate::ui::main_view::build_display::render_build_skills(ui, current_build, db);
            }
            ui.spacing();
            if viewing {
                crate::ui::main_view::lock_panel::render_optimized_specs_panel(
                    ui,
                    db,
                    &suggestion.specializations,
                    &t("section.optimized_specs"),
                );
            } else {
                let current_specs = spec_pairs_from_build(current_build);
                crate::ui::main_view::lock_panel::render_optimized_specs_panel(
                    ui,
                    db,
                    &current_specs,
                    &t("section.specs"),
                );
            }
            let gain = crate::ui::gear_sheet::combat_gain(
                comparison.current_combat_solo.as_ref(),
                suggestion.combat_solo.as_ref(),
            );
            crate::ui::gear_sheet::render_current_sheet(
                ui,
                current_build,
                Some(&suggestion),
                db,
                viewing,
                gain,
            );
            let explanation_text = if !suggestion.synergy_explanation.is_empty() {
                &suggestion.synergy_explanation
            } else {
                &suggestion.explanation
            };
            if !explanation_text.is_empty() {
                ui.spacing();
                ui.text_colored(crate::ui::theme::MUTED, t("note.how_to_play"));
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
    ui.text_colored(crate::ui::theme::GOLD, t("section.attributes"));
    ui.text_colored(crate::ui::theme::MUTED, t("note.attributes"));
    ui.text_colored(crate::ui::theme::MUTED, t("tier.solo"));
    ui.spacing();
    render_primary_stats(ui, current_stats, suggestion.estimated_stats.as_ref());
    ui.spacing();
    render_defenses(ui, comparison, current_stats, suggestion);

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, t("section.boons"));
    ui.text_colored(crate::ui::theme::MUTED, t("note.boons"));
    match suggestion.rotation.as_ref() {
        Some(rotation) => {
            if rotation.has_stability || rotation.stunbreak_count > 0 {
                ui.spacing();
                ui.text_colored(
                    crate::ui::theme::CREAM,
                    tf(
                        "fmt.stability",
                        &[
                            (
                                "yn",
                                &if rotation.has_stability {
                                    t("label.yes")
                                } else {
                                    t("label.no")
                                },
                            ),
                            ("pct", &format!("{:.0}", rotation.stability_uptime * 100.0)),
                            ("n", &rotation.stunbreak_count.to_string()),
                        ],
                    ),
                );
            }
            if rotation.buff_uptime.is_empty() {
                ui.spacing();
                ui.text_colored(crate::ui::theme::MUTED, t("note.no_boons"));
            } else {
                ui.spacing();
                ui.text_colored(crate::ui::theme::GOLD, t("label.uptime"));
                for (name, frac) in rotation.buff_uptime.iter().take(8) {
                    ui.text(format!("  {name}: {:.0}%", display_uptime_pct(*frac)));
                }
            }
        }
        None => {
            ui.spacing();
            ui.text_colored(crate::ui::theme::MUTED, t("note.no_rotation"));
        }
    }

    ui.spacing();
    ui.text_colored(crate::ui::theme::GOLD, t("section.conditions"));
    ui.text_colored(crate::ui::theme::MUTED, t("note.conditions_full"));
    if let Some(rotation) = suggestion.rotation.as_ref() {
        ui.spacing();
        ui.text_colored(
            crate::ui::theme::CREAM,
            tf(
                "fmt.cleanse",
                &[
                    ("n", &rotation.cleanse_count.to_string()),
                    ("rate", &format!("{:.1}", rotation.cleanse_rate_per_20s)),
                ],
            ),
        );
        if !rotation.condition_uptime.is_empty() {
            ui.spacing();
            ui.text_colored(crate::ui::theme::GOLD, t("label.stacks"));
            for (name, stacks) in rotation.condition_uptime.iter().take(8) {
                ui.text(format!("  {name}: {stacks:.1}"));
            }
        }
    }

    if let Some(ref rotation) = suggestion.rotation {
        ui.spacing();
        ui.text_colored(crate::ui::theme::GOLD, t("section.rotation"));
        render_rotation_breakdown(ui, rotation, db);
    }
    if let Some(ref viability) = suggestion.viability {
        render_viability_report(ui, viability);
    }
    render_benchmark_delta(ui, suggestion);
    if !suggestion.changes_made.is_empty() {
        ui.spacing();
        ui.text_colored(crate::ui::theme::GOLD, t("section.changes"));
        for change in &suggestion.changes_made {
            ui.bullet_text(change);
        }
    }
}

fn spec_pairs_from_build(build: &ResolvedBuild) -> Vec<(String, Vec<String>)> {
    build
        .specializations
        .iter()
        .map(|s| {
            let name = if s.elite {
                format!("{} [E]", s.name)
            } else {
                s.name.clone()
            };
            let traits = s
                .traits_selected
                .iter()
                .filter(|t| t.selected)
                .map(|t| t.name.clone())
                .collect();
            (name, traits)
        })
        .collect()
}

/// Sticky copy strip for a GW2 build-template chat code.
/// Click the line to copy onto the Windows clipboard (GW2 paste reads that).
/// Rim and label follow [ChatSource]: blue = loaded character, green = optimized.
/// One line — lives on the tab row so the left panel keeps that height.
pub fn render_chat_code_copy(
    ui: &Ui,
    source: ChatSource,
    chat_code: Option<&str>,
    id_suffix: &str,
    copied_frames: &mut u32,
) {
    if *copied_frames > 0 {
        *copied_frames = copied_frames.saturating_sub(1);
    }

    let remain = ui.content_region_avail()[0];
    if remain < 96.0 {
        ui.dummy([0.0, 0.0]);
        return;
    }

    let accent = source.color();
    let h = ui.frame_height().max(ui.text_line_height() + 6.0);
    let w = (remain - 4.0).max(80.0);
    let p = ui.cursor_screen_pos();
    let clicked = ui.invisible_button(&format!("##chat_copy_{}", id_suffix), [w, h]);
    let hovered = ui.is_item_hovered();
    if clicked {
        if let Some(code) = chat_code {
            if crate::clipboard::copy_text(code) {
                *copied_frames = 120;
            }
        }
    }
    if hovered {
        ui.tooltip_text(if chat_code.is_some() {
            t("tip.copy_chat")
        } else {
            t("tip.no_chat")
        });
    }

    let fill = if *copied_frames > 0 {
        match source {
            ChatSource::Character => [0.10, 0.16, 0.26, 0.95],
            ChatSource::Optimized => [0.10, 0.22, 0.12, 0.95],
        }
    } else if hovered {
        match source {
            ChatSource::Character => [0.16, 0.20, 0.28, 0.95],
            ChatSource::Optimized => [0.14, 0.22, 0.14, 0.95],
        }
    } else {
        crate::ui::theme::PLATE
    };
    let rim = if chat_code.is_some() {
        accent
    } else {
        crate::ui::theme::GOLD_DIM
    };
    let text_col = if chat_code.is_some() {
        crate::ui::theme::CREAM
    } else {
        crate::ui::theme::MUTED
    };

    let src = source.label();
    let prefix = if *copied_frames > 0 {
        tf("fmt.chat_copied", &[("source", &src)])
    } else {
        tf("fmt.chat_source", &[("source", &src)])
    };
    let fallback = match source {
        ChatSource::Character => t("chat.load_character"),
        ChatSource::Optimized => t("chat.no_result_code"),
    };
    let code_part = chat_code.unwrap_or(fallback.as_str());

    let pad = 8.0;
    let inner_w = (w - pad * 2.0).max(20.0);
    let prefix_w = ui.calc_text_size(&prefix)[0];
    let gap = 8.0;
    let code_w = (inner_w - prefix_w - gap).max(12.0);
    let shown_code = if *copied_frames > 0 {
        String::new()
    } else {
        truncate_ui_text(ui, code_part, code_w)
    };

    {
        let dl = ui.get_window_draw_list();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], fill)
            .filled(true)
            .rounding(crate::ui::theme::ICON_ROUNDING)
            .build();
        dl.add_rect([p[0], p[1]], [p[0] + w, p[1] + h], rim)
            .rounding(crate::ui::theme::ICON_ROUNDING)
            .build();
        let ty = p[1] + ((h - ui.text_line_height()) * 0.5).round();
        dl.add_text([p[0] + pad, ty], crate::ui::color_u32(accent), &prefix);
        if !shown_code.is_empty() {
            dl.add_text(
                [p[0] + pad + prefix_w + gap, ty],
                crate::ui::color_u32(text_col),
                &shown_code,
            );
        }
    }
}

fn truncate_ui_text(ui: &Ui, text: &str, width: f32) -> String {
    if ui.calc_text_size(text)[0] <= width {
        return text.to_string();
    }
    let ellipsis = "...";
    let budget = (width - ui.calc_text_size(ellipsis)[0]).max(0.0);
    let mut s = String::new();
    for ch in text.chars() {
        let mut next = s.clone();
        next.push(ch);
        if ui.calc_text_size(&next)[0] > budget {
            s.push_str(ellipsis);
            return s;
        }
        s = next;
    }
    s
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
    ui.text_colored(crate::ui::theme::GOLD, t("table.slot"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, t("label.current"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, t("label.optimized"));
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

    ui.text_colored(cur_color, loc_name(db, &diff.current_value));
    inspect_if_hovered(ui, &diff.current_value, db);
    ui.next_column();

    ui.text_colored(opt_color, loc_name(db, &diff.proposed_value));
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

    let names = [
        t("stat.power"),
        t("stat.precision"),
        t("stat.ferocity"),
        t("stat.condi_dmg_full"),
        t("stat.expertise"),
        t("stat.concentration"),
        t("stat.toughness"),
        t("stat.vitality"),
        t("stat.heal_power"),
    ];
    let stats = [
        (names[0].as_str(), cur.power, sug.power),
        (names[1].as_str(), cur.precision, sug.precision),
        (names[2].as_str(), cur.ferocity, sug.ferocity),
        (
            names[3].as_str(),
            cur.condition_damage,
            sug.condition_damage,
        ),
        (names[4].as_str(), cur.expertise, sug.expertise),
        (names[5].as_str(), cur.concentration, sug.concentration),
        (names[6].as_str(), cur.toughness, sug.toughness),
        (names[7].as_str(), cur.vitality, sug.vitality),
        (names[8].as_str(), cur.healing_power, sug.healing_power),
    ];

    render_stat_table(ui, "##primary_stats", &stats);
}

/// Legacy synthetic combat metrics retained for internal diagnostics only.
/// Player-facing build comparisons use observable Hero-panel attributes and
/// the explicitly labelled rotation report instead.
#[allow(dead_code)]
fn render_combat_performance(ui: &Ui, comparison: &ComparisonState, suggestion: &BuildSuggestion) {
    let solo = t("tier.solo");
    let party = t("tier.party");
    let squad = t("tier.squad");
    let ep = t("stat.effective_power");
    let crit = t("stat.crit");
    let strike = t("stat.strike_dps");
    let condi = t("stat.condi_dps");
    let total = t("stat.total_dps");
    let heal = t("stat.heal_index");
    let incoming = t("stat.incoming_strike");
    let not_computed = t("note.not_computed");
    let tiers = [
        (
            solo.as_str(),
            [0.7, 0.85, 1.0, 1.0],
            comparison.current_combat_solo.as_ref(),
            suggestion.combat_solo.as_ref(),
            "solo",
        ),
        (
            party.as_str(),
            [1.0, 0.85, 0.4, 1.0],
            comparison.current_combat_party.as_ref(),
            suggestion.combat_party.as_ref(),
            "party",
        ),
        (
            squad.as_str(),
            [0.3, 1.0, 0.3, 1.0],
            comparison.current_combat_squad.as_ref(),
            suggestion.combat_squad.as_ref(),
            "squad",
        ),
    ];

    for (label, color, cur_combat, sug_combat, id) in &tiers {
        ui.text_colored(*color, *label);

        if let Some(sug) = sug_combat {
            ui.columns(4, format!("##{id}_cols"), true);
            bonus_header(ui);

            let cur = *cur_combat;
            render_int_row_opt(ui, &ep, cur.map(|c| c.effective_power), sug.effective_power);
            render_pct_row_opt(ui, &crit, cur.map(|c| c.crit_chance), sug.crit_chance);
            render_int_row_opt(
                ui,
                &strike,
                cur.map(|c| c.strike_dps_index),
                sug.strike_dps_index,
            );
            render_int_row_opt(
                ui,
                &condi,
                cur.map(|c| c.condition_dps_index),
                sug.condition_dps_index,
            );
            render_int_row_opt(
                ui,
                &total,
                cur.map(|c| c.total_dps_index),
                sug.total_dps_index,
            );
            if sug.healing_index > 0 || cur.is_some_and(|c| c.healing_index > 0) {
                render_int_row_opt(ui, &heal, cur.map(|c| c.healing_index), sug.healing_index);
            }
            render_pct_row_opt(
                ui,
                &incoming,
                cur.map(|c| c.damage_reduction_pct),
                sug.damage_reduction_pct,
            );

            ui.columns(1, format!("##{id}_end"), false);
        } else {
            ui.text_colored(crate::ui::theme::MUTED, format!("  {not_computed}"));
        }

        ui.spacing();
    }

    if let Some(sug) = suggestion.combat_solo.as_ref() {
        let cur = comparison.current_combat_solo.as_ref();
        let per_stack = t("info.per_stack");
        let stationary = t("info.stationary");
        let on_skill = t("info.on_skill");
        let ticks = [
            (
                "Bleeding",
                cur.map_or(0, |c| c.bleeding_tick),
                sug.bleeding_tick,
                per_stack.as_str(),
            ),
            (
                "Burning",
                cur.map_or(0, |c| c.burning_tick),
                sug.burning_tick,
                per_stack.as_str(),
            ),
            (
                "Poison",
                cur.map_or(0, |c| c.poison_tick),
                sug.poison_tick,
                per_stack.as_str(),
            ),
            (
                "Torment",
                cur.map_or(0, |c| c.torment_tick),
                sug.torment_tick,
                stationary.as_str(),
            ),
            (
                "Confusion",
                cur.map_or(0, |c| c.confusion_tick),
                sug.confusion_tick,
                on_skill.as_str(),
            ),
        ];
        let ticks_to_show: Vec<_> = ticks
            .iter()
            .filter(|(_, cur_v, sug_v, _)| *cur_v > 0 || *sug_v > 0)
            .collect();

        if !ticks_to_show.is_empty() {
            ui.text_colored([0.9, 0.6, 0.2, 1.0], t("note.condi_ticks"));
            ui.columns(4, "##condi_ticks", true);

            ui.text_colored(crate::ui::theme::GOLD, t("table.condition"));
            ui.next_column();
            ui.text_colored(crate::ui::theme::CURRENT, t("label.current"));
            ui.next_column();
            ui.text_colored(crate::ui::theme::OPTIMIZED, t("label.optimized"));
            ui.next_column();
            ui.text(t("table.info"));
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
    ui.text_colored(crate::ui::theme::GOLD, t("table.metric"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, t("label.current"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, t("label.optimized"));
    ui.next_column();
    ui.text(t("table.diff"));
    ui.next_column();
    ui.separator();
}

/// Render defenses: Health and Armor (static stats that don't change with buff profile).
/// Effective HP and Damage Reduction are shown per-tier in Combat Performance.
fn render_defenses(
    ui: &Ui,
    _comparison: &ComparisonState,
    current_stats: Option<&StatBlock>,
    suggestion: &BuildSuggestion,
) {
    let sug_stats = suggestion.estimated_stats.clone().unwrap_or_default();
    let cur = current_stats.cloned().unwrap_or_default();

    let health = t("stat.health");
    let armor = t("stat.armor");
    let stats = [
        (health.as_str(), cur.health, sug_stats.health),
        (armor.as_str(), cur.armor, sug_stats.armor),
    ];

    ui.columns(4, "##defense_cols", true);
    ui.text_colored(crate::ui::theme::GOLD, t("table.defense"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, t("label.current"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, t("label.optimized"));
    ui.next_column();
    ui.text(t("table.diff"));
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

    ui.text_colored(crate::ui::theme::GOLD, t("table.attribute"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::CURRENT, t("label.current"));
    ui.next_column();
    ui.text_colored(crate::ui::theme::OPTIMIZED, t("label.optimized"));
    ui.next_column();
    ui.text(t("table.diff"));
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
    ui.text_colored([0.55, 0.55, 0.55, 1.0], t("note.rotation_sim"));
    ui.text(tf(
        "fmt.sim_dps",
        &[
            ("dps", &rotation.simulated_dps.to_string()),
            ("strike", &rotation.strike_dps.to_string()),
            ("condi", &rotation.condition_dps.to_string()),
        ],
    ));
    ui.spacing();

    if !rotation.skill_usage.is_empty() {
        ui.text(t("label.skill_usage"));
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
    let (label, col, tooltip_header) = match suggestion.data_quality {
        DataQuality::Verified => (
            format!("* {}", t("settings.verified")),
            [0.3, 0.9, 0.3, 1.0],
            t("quality.verified_tip"),
        ),
        DataQuality::Provisional => (
            format!("* {}", t("settings.provisional")),
            [0.95, 0.75, 0.15, 1.0],
            t("quality.provisional_tip"),
        ),
        DataQuality::Blocked => (
            format!("* {}", t("settings.blocked")),
            [1.0, 0.3, 0.2, 1.0],
            t("quality.blocked_tip"),
        ),
    };

    ui.text_colored(col, &label);
    if ui.is_item_hovered() {
        crate::ui::theme::wide_tooltip(ui, |ui| {
            ui.text(&tooltip_header);
            if !suggestion.quality_reasons.is_empty() {
                ui.spacing();
                ui.text(t("label.reasons"));
                for reason in &suggestion.quality_reasons {
                    ui.bullet_text(reason);
                }
            }
        });
    }
}

/// Render benchmark delta vs community reference.
fn render_benchmark_delta(ui: &Ui, suggestion: &BuildSuggestion) {
    match &suggestion.benchmark_delta {
        None => {
            // No data — show subtle hint in collapsed section
            if ui.collapsing_header(t("bench.header"), TreeNodeFlags::empty()) {
                ui.text_colored(crate::ui::theme::MUTED, format!("  {}", t("bench.none")));
                ui.text_colored(
                    crate::ui::theme::MUTED,
                    format!("  {}", t("bench.sync_hint")),
                );
            }
        }
        Some(delta) => {
            let pct = delta.pct_of_ref;
            let (col, status_key): ([f32; 4], &str) = if pct >= 95.0 {
                ([0.3, 0.9, 0.3, 1.0], "bench.on_par")
            } else if pct >= 80.0 {
                ([0.9, 0.8, 0.2, 1.0], "bench.close")
            } else if pct >= 65.0 {
                ([0.9, 0.55, 0.1, 1.0], "bench.below")
            } else {
                ([1.0, 0.3, 0.2, 1.0], "bench.far_below")
            };
            let status = t(status_key);

            let header = tf(
                "fmt.vs_meta",
                &[
                    ("src", &title_case(&delta.source)),
                    ("pct", &format!("{:.0}", pct)),
                    ("status", &status),
                ],
            );

            if ui.collapsing_header(&header, TreeNodeFlags::DEFAULT_OPEN) {
                ui.text_colored(
                    col,
                    format!(
                        "  {}",
                        tf("fmt.pct_ref", &[("pct", &format!("{:.0}", pct))])
                    ),
                );
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
                    "  {}",
                    tf(
                        "fmt.reference",
                        &[
                            ("prof", &delta.profession),
                            ("role", &delta.role),
                            ("gear", &delta.ref_gear_prefix),
                            ("src", &delta.source),
                        ],
                    )
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
        t("viable.yes")
    } else {
        t("viable.no")
    };

    if ui.collapsing_header(
        tf("fmt.viability", &[("status", &status)]),
        if report.is_viable {
            TreeNodeFlags::empty()
        } else {
            TreeNodeFlags::DEFAULT_OPEN
        },
    ) {
        ui.text_colored(
            header_col,
            format!("  {}", tf("fmt.viable_status", &[("status", &status)])),
        );
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
            ui.text_colored([1.0, 0.7, 0.2, 1.0], format!("  {}", t("note.nonviable")));
        }
    }
}

/// Legacy synthetic tradeoff report retained for internal diagnostics only.
#[allow(dead_code)]
fn render_tradeoff_analysis(ui: &Ui, comparison: &ComparisonState, suggestion: &BuildSuggestion) {
    let Some(ref new_metrics) = suggestion.combat_solo else {
        return;
    };
    let Some(ref cur_metrics) = comparison.current_combat_solo else {
        // No current build data — show absolute metrics only
        if ui.collapsing_header(t("tradeoff.estimate"), TreeNodeFlags::empty()) {
            ui.text_colored(
                [0.8, 0.8, 0.8, 1.0],
                format!("  {}", t("tradeoff.no_current")),
            );
            ui.text(format!(
                "  {}",
                tf(
                    "fmt.strike_idx",
                    &[("n", &format!("{:.0}", new_metrics.strike_dps_index))],
                )
            ));
            ui.text(format!(
                "  {}",
                tf(
                    "fmt.condi_idx",
                    &[("n", &format!("{:.0}", new_metrics.condition_dps_index))],
                )
            ));
            ui.text(format!(
                "  {}",
                tf(
                    "fmt.ehp",
                    &[("n", &format!("{:.0}", new_metrics.effective_health))],
                )
            ));
        }
        return;
    };

    if ui.collapsing_header(t("tradeoff.header"), TreeNodeFlags::DEFAULT_OPEN) {
        ui.text_colored([0.8, 0.8, 0.5, 1.0], format!("  {}", t("tradeoff.vs")));
        ui.spacing();

        let strike = t("stat.strike_dps");
        let condi = t("stat.condi_dps");
        let ehp = t("stat.effective_hp");
        let heal = t("stat.heal_index");
        let tradeoffs = [
            (
                strike.as_str(),
                cur_metrics.strike_dps_index as f64,
                new_metrics.strike_dps_index as f64,
            ),
            (
                condi.as_str(),
                cur_metrics.condition_dps_index as f64,
                new_metrics.condition_dps_index as f64,
            ),
            (
                ehp.as_str(),
                cur_metrics.effective_health as f64,
                new_metrics.effective_health as f64,
            ),
            (
                heal.as_str(),
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
    fn chat_code_follows_current_vs_optimized() {
        let mut c = ComparisonState::default();
        let (src, code) = c.chat_focus(Some("[&CUR]"));
        assert_eq!(src, ChatSource::Character);
        assert_eq!(code.as_deref(), Some("[&CUR]"));

        c.show_optimized = true;
        c.suggestions.push(BuildSuggestion {
            chat_code: Some("[&OPT]".into()),
            ..Default::default()
        });
        let (src, code) = c.chat_focus(Some("[&CUR]"));
        assert_eq!(src, ChatSource::Optimized);
        assert_eq!(code.as_deref(), Some("[&OPT]"));

        c.suggestions[0].chat_code = None;
        let (src, code) = c.chat_focus(Some("[&CUR]"));
        assert_eq!(src, ChatSource::Optimized);
        assert_eq!(code, None);

        c.show_optimized = false;
        let (src, code) = c.chat_focus(Some("[&CUR]"));
        assert_eq!(src, ChatSource::Character);
        assert_eq!(code.as_deref(), Some("[&CUR]"));
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

    #[test]
    fn fact_line_preserves_tooltip_effect_label() {
        let fact = gw2_api::models::facts::Fact::AttributeAdjust {
            text: Some("Life Siphon Damage".into()),
            icon: None,
            value: Some(3517),
            target: Some("Power".into()),
        };

        assert_eq!(
            fact_line(&fact).as_deref(),
            Some("Life Siphon Damage: 3517")
        );
    }
}
