//! Card-style build display using DrawList for visual card sections.

use nexus::imgui::Ui;

use gw2_core::types::{CombatMetrics, ResolvedBuild, RotationBreakdown, StatBlock};

// ─── Color palette ───

const SECTION_HEADER_BG: [f32; 4] = [0.28, 0.24, 0.12, 0.95]; // warm dark gold header
                                                              // Body bg is not drawn (would cover text due to DrawList ordering).
                                                              // Card look comes from header + border + accent line.
const SPEC_COLOR: [f32; 4] = [0.85, 0.65, 1.0, 1.0]; // purple spec names
const TRAIT_COLOR: [f32; 4] = [0.65, 0.8, 1.0, 1.0]; // light blue traits
const GEAR_COLOR: [f32; 4] = [1.0, 0.85, 0.3, 1.0]; // bright gold for gear
const DIM_COLOR: [f32; 4] = [0.55, 0.55, 0.55, 1.0]; // dim gray
const LABEL_COLOR: [f32; 4] = [0.7, 0.7, 0.75, 1.0]; // cool gray labels
const VALUE_COLOR: [f32; 4] = [0.95, 0.95, 0.95, 1.0]; // near-white values
const SECTION_TITLE_COLOR: [f32; 4] = [1.0, 0.88, 0.35, 1.0]; // bright gold titles
const ACCENT_LINE_COLOR: [f32; 4] = [0.7, 0.55, 0.15, 0.5]; // subtle gold accent
const CARD_BORDER_COLOR: [f32; 4] = [0.35, 0.3, 0.15, 0.4]; // dim gold border

const STAT_BETTER: [f32; 4] = [0.3, 1.0, 0.4, 1.0]; // green — this stat is higher
const STAT_WORSE: [f32; 4] = [1.0, 0.35, 0.3, 1.0]; // red — this stat is lower

/// Pick color for a stat value based on comparison: green if better, red if worse, default if equal.
fn stat_color(val: i32, compare: i32) -> [f32; 4] {
    if val > compare {
        STAT_BETTER
    } else if val < compare {
        STAT_WORSE
    } else {
        VALUE_COLOR
    }
}

fn stat_color_f64(val: f64, compare: f64) -> [f32; 4] {
    if val > compare + 0.1 {
        STAT_BETTER
    } else if val < compare - 0.1 {
        STAT_WORSE
    } else {
        VALUE_COLOR
    }
}

const CARD_ROUNDING: f32 = 5.0;
const CARD_PAD: f32 = 4.0;
const TITLE_HEIGHT: f32 = 20.0;
const CARD_GAP: f32 = 8.0;

/// Render a prominent panel header (e.g. "CURRENT BUILD") with colored text.
pub fn render_card_header(ui: &Ui, title: &str, color: [f32; 4]) {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [start[0] - 1.0, start[1]],
                [start[0] + width + 1.0, start[1] + 22.0],
                [0.15, 0.13, 0.08, 0.9],
            )
            .filled(true)
            .rounding(CARD_ROUNDING)
            .round_bot_left(false)
            .round_bot_right(false)
            .build();
        draw_list.add_text([start[0] + 8.0, start[1] + 3.0], color, title);
    }
    ui.dummy([0.0, 24.0]);
}

/// Render a section card: header bar with title + body with content.
/// DrawListMut borrows are scoped to avoid the "already loaded" panic.
fn render_card_section(ui: &Ui, title: &str, content: impl FnOnce(&Ui)) {
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let hdr_top = start[1];
    let hdr_bottom = hdr_top + CARD_PAD + TITLE_HEIGHT;

    // ── Phase 1: Draw header background + title ──
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [start[0] - 1.0, hdr_top],
                [start[0] + width + 1.0, hdr_bottom],
                SECTION_HEADER_BG,
            )
            .filled(true)
            .rounding(CARD_ROUNDING)
            .round_bot_left(false)
            .round_bot_right(false)
            .build();
        draw_list.add_text(
            [start[0] + 10.0, hdr_top + CARD_PAD],
            SECTION_TITLE_COLOR,
            title,
        );
    } // DrawListMut dropped here

    // Reserve space for the header
    ui.dummy([0.0, CARD_PAD + TITLE_HEIGHT + 2.0]);

    // ── Phase 2: Render content (ImGui widgets) ──
    let body_top = ui.cursor_screen_pos()[1];
    ui.dummy([0.0, 2.0]);
    content(ui);
    ui.dummy([0.0, CARD_PAD]);
    let body_bottom = ui.cursor_screen_pos()[1];

    // ── Phase 3: Draw accent line + border ──
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_line(
                [start[0] - 1.0, body_top],
                [start[0] + width + 1.0, body_top],
                ACCENT_LINE_COLOR,
            )
            .thickness(1.0)
            .build();
        draw_list
            .add_rect(
                [start[0] - 1.0, hdr_top],
                [start[0] + width + 1.0, body_bottom],
                CARD_BORDER_COLOR,
            )
            .rounding(CARD_ROUNDING)
            .build();
    } // DrawListMut dropped here

    ui.dummy([0.0, CARD_GAP]);
}

/// Render a compact build card suitable for a narrow panel.
pub fn render_build_card(ui: &Ui, build: &ResolvedBuild, stats: Option<&StatBlock>) {
    // ── Specializations Card ──
    render_card_section(ui, "SPECIALIZATIONS", |ui| {
        for spec in &build.specializations {
            let elite = if spec.elite { " [Elite]" } else { "" };
            ui.text_colored(SPEC_COLOR, &format!("  {}{}", spec.name, elite));
            let traits: Vec<&str> = spec
                .traits_selected
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            if !traits.is_empty() {
                ui.text_colored(TRAIT_COLOR, &format!("    {}", traits.join(" | ")));
            }
        }
    });

    // ── Skills Card ──
    render_card_section(ui, "SKILLS", |ui| {
        if let Some(ref h) = build.skills.heal {
            render_label_value(ui, "Heal", &h.name);
        }
        let utils: Vec<String> = build
            .skills
            .utilities
            .iter()
            .filter_map(|u| u.as_ref().map(|s| s.name.clone()))
            .collect();
        for (i, name) in utils.iter().enumerate() {
            render_label_value(ui, &format!("Util {}", i + 1), name);
        }
        if let Some(ref e) = build.skills.elite {
            render_label_value(ui, "Elite", &e.name);
        }
    });

    // ── Weapons Card ──
    render_card_section(ui, "WEAPONS", |ui| {
        for set in &build.weapons {
            let mut parts = Vec::new();
            if let Some(ref mh) = set.main_hand {
                parts.push(mh.weapon_type.as_str());
            }
            if let Some(ref oh) = set.off_hand {
                parts.push(oh.weapon_type.as_str());
            }
            if !parts.is_empty() {
                render_label_value(ui, &set.label, &parts.join(" / "));
            }
            if !set.sigils.is_empty() {
                let names: Vec<&str> = set.sigils.iter().map(|s| s.name.as_str()).collect();
                ui.text_colored(DIM_COLOR, &format!("    Sigils: {}", names.join(", ")));
            }
        }
    });

    // ── Gear Card ──
    render_card_section(ui, "GEAR", |ui| {
        if !build.armor.is_empty() {
            let mut prefixes: Vec<&str> = build
                .armor
                .iter()
                .map(|a| a.stat_prefix.as_str())
                .filter(|p| !p.is_empty())
                .collect();
            prefixes.sort();
            prefixes.dedup();
            let prefix_str = if prefixes.len() <= 1 {
                prefixes.first().copied().unwrap_or("(none)").to_string()
            } else {
                format!("Mixed ({})", prefixes.join(", "))
            };
            render_label_value(ui, "Prefix", &prefix_str);
        }
        if let Some(ref r) = build.rune {
            render_label_value_colored(ui, "Rune", &r.name, GEAR_COLOR);
        }
        if let Some(ref r) = build.relic {
            render_label_value_colored(ui, "Relic", &r.name, GEAR_COLOR);
        }
        if let Some(ref a) = build.pvp_amulet {
            render_label_value_colored(ui, "Amulet", &a.name, GEAR_COLOR);
        }
    });

    // ── Stats Card ──
    if let Some(s) = stats {
        render_card_section(ui, "STATS", |ui| {
            ui.columns(2, "##card_stats", false);
            let rows = [
                ("Power", s.power),
                ("Precision", s.precision),
                ("Toughness", s.toughness),
                ("Vitality", s.vitality),
                ("Condi Dmg", s.condition_damage),
                ("Ferocity", s.ferocity),
                ("Healing", s.healing_power),
                ("Expertise", s.expertise),
            ];
            for (name, val) in &rows {
                ui.text_colored(LABEL_COLOR, &format!("  {}: ", name));
                ui.same_line();
                ui.text_colored(VALUE_COLOR, &format!("{}", val));
                ui.next_column();
            }
            ui.columns(1, "##card_stats_end", false);
            ui.text_colored(
                DIM_COLOR,
                &format!(
                    "  Crit {:.1}% | HP {} | Armor {}",
                    s.crit_chance, s.health, s.armor
                ),
            );
        });
    }
}

/// Render a build card without the Specializations section (used when lock panel replaces it).
/// `compare_stats`: if provided, stat values are colored green/red relative to these.
pub fn render_build_card_no_specs(
    ui: &Ui,
    build: &ResolvedBuild,
    stats: Option<&StatBlock>,
    compare_stats: Option<&StatBlock>,
) {
    render_build_skills(ui, build);
    render_build_weapons(ui, build);
    render_build_gear(ui, build);
    render_build_stats(ui, stats, compare_stats);
}

/// Render a compact suggestion card (right panel) from a BuildSuggestion.
#[allow(dead_code)]
pub fn render_suggestion_card(ui: &Ui, suggestion: &super::super::comparison::BuildSuggestion) {
    // ── Specializations Card ──
    render_card_section(ui, "SPECIALIZATIONS", |ui| {
        for (name, traits) in &suggestion.specializations {
            ui.text_colored(SPEC_COLOR, &format!("  {}", name));
            if !traits.is_empty() {
                ui.text_colored(TRAIT_COLOR, &format!("    {}", traits.join(" | ")));
            }
        }
    });

    // ── Skills Card ──
    render_card_section(ui, "SKILLS", |ui| {
        for skill in &suggestion.skills {
            ui.text_colored(VALUE_COLOR, &format!("  {}", skill));
        }
    });

    // ── Weapons Card ──
    render_card_section(ui, "WEAPONS", |ui| {
        for weapon in &suggestion.weapons {
            ui.text_colored(VALUE_COLOR, &format!("  {}", weapon));
        }
        if !suggestion.sigils.is_empty() {
            ui.text_colored(
                DIM_COLOR,
                &format!("  Sigils: {}", suggestion.sigils.join(", ")),
            );
        }
    });

    // ── Gear Card ──
    render_card_section(ui, "GEAR", |ui| {
        if !suggestion.stat_prefix.is_empty() {
            render_label_value(ui, "Prefix", &suggestion.stat_prefix);
        }
        if !suggestion.rune.is_empty() {
            render_label_value_colored(ui, "Rune", &suggestion.rune, GEAR_COLOR);
        }
        if !suggestion.relic.is_empty() {
            render_label_value_colored(ui, "Relic", &suggestion.relic, GEAR_COLOR);
        }
    });

    // ── Stats Card ──
    if let Some(ref s) = suggestion.estimated_stats {
        render_card_section(ui, "STATS", |ui| {
            ui.columns(2, "##sug_stats", false);
            let rows = [
                ("Power", s.power),
                ("Precision", s.precision),
                ("Toughness", s.toughness),
                ("Vitality", s.vitality),
                ("Condi Dmg", s.condition_damage),
                ("Ferocity", s.ferocity),
                ("Healing", s.healing_power),
                ("Expertise", s.expertise),
            ];
            for (name, val) in &rows {
                ui.text_colored(LABEL_COLOR, &format!("  {}: ", name));
                ui.same_line();
                ui.text_colored(VALUE_COLOR, &format!("{}", val));
                ui.next_column();
            }
            ui.columns(1, "##sug_stats_end", false);
            ui.text_colored(
                DIM_COLOR,
                &format!(
                    "  Crit {:.1}% | HP {} | Armor {}",
                    s.crit_chance, s.health, s.armor
                ),
            );
        });
    }
}

/// Render a suggestion card without the Specializations section (used when optimized specs panel replaces it).
/// `compare_stats`: if provided, stat values are colored green/red relative to these (the current build's stats).
#[allow(dead_code)]
pub fn render_suggestion_card_no_specs(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    compare_stats: Option<&StatBlock>,
) {
    render_suggestion_skills(ui, suggestion);
    render_suggestion_weapons(ui, suggestion);
    render_suggestion_gear(ui, suggestion);
    render_suggestion_stats(ui, suggestion, compare_stats);
}

// ─── Helpers ───

fn render_label_value(ui: &Ui, label: &str, value: &str) {
    ui.text_colored(LABEL_COLOR, &format!("  {}: ", label));
    ui.same_line();
    ui.text_colored(VALUE_COLOR, value);
}

fn render_label_value_colored(ui: &Ui, label: &str, value: &str, color: [f32; 4]) {
    ui.text_colored(LABEL_COLOR, &format!("  {}: ", label));
    ui.same_line();
    ui.text_colored(color, value);
}

// ─── Individual section renderers (for column-aligned layouts) ───

/// Render the SKILLS section for the current build.
pub fn render_build_skills(ui: &Ui, build: &ResolvedBuild) {
    render_card_section(ui, "SKILLS", |ui| {
        if let Some(ref h) = build.skills.heal {
            render_label_value(ui, "Heal", &h.name);
        }
        let utils: Vec<String> = build
            .skills
            .utilities
            .iter()
            .filter_map(|u| u.as_ref().map(|s| s.name.clone()))
            .collect();
        for (i, name) in utils.iter().enumerate() {
            render_label_value(ui, &format!("Util {}", i + 1), name);
        }
        if let Some(ref e) = build.skills.elite {
            render_label_value(ui, "Elite", &e.name);
        }
    });
}

/// Render the WEAPONS section for the current build.
pub fn render_build_weapons(ui: &Ui, build: &ResolvedBuild) {
    render_card_section(ui, "WEAPONS", |ui| {
        for set in &build.weapons {
            let mut parts = Vec::new();
            if let Some(ref mh) = set.main_hand {
                parts.push(mh.weapon_type.as_str());
            }
            if let Some(ref oh) = set.off_hand {
                parts.push(oh.weapon_type.as_str());
            }
            if !parts.is_empty() {
                render_label_value(ui, &set.label, &parts.join(" / "));
            }
            if !set.sigils.is_empty() {
                let names: Vec<&str> = set.sigils.iter().map(|s| s.name.as_str()).collect();
                ui.text_colored(DIM_COLOR, &format!("    Sigils: {}", names.join(", ")));
            }
        }
    });
}

/// Render the GEAR section for the current build.
pub fn render_build_gear(ui: &Ui, build: &ResolvedBuild) {
    render_card_section(ui, "GEAR", |ui| {
        if !build.armor.is_empty() {
            let mut prefixes: Vec<&str> = build
                .armor
                .iter()
                .map(|a| a.stat_prefix.as_str())
                .filter(|p| !p.is_empty())
                .collect();
            prefixes.sort();
            prefixes.dedup();
            let prefix_str = if prefixes.len() <= 1 {
                prefixes.first().copied().unwrap_or("(none)").to_string()
            } else {
                format!("Mixed ({})", prefixes.join(", "))
            };
            render_label_value(ui, "Prefix", &prefix_str);
        }
        if let Some(ref r) = build.rune {
            render_label_value_colored(ui, "Rune", &r.name, GEAR_COLOR);
        }
        if let Some(ref r) = build.relic {
            render_label_value_colored(ui, "Relic", &r.name, GEAR_COLOR);
        }
        if let Some(ref a) = build.pvp_amulet {
            render_label_value_colored(ui, "Amulet", &a.name, GEAR_COLOR);
        }
    });
}

/// Render the STATS section for the current build (single-column, no inner columns).
/// Color-coded against compare_stats if provided.
pub fn render_build_stats(ui: &Ui, stats: Option<&StatBlock>, compare_stats: Option<&StatBlock>) {
    if let Some(s) = stats {
        render_card_section(ui, "STATS", |ui| {
            let cmp = compare_stats.cloned().unwrap_or_default();
            let rows = [
                ("Power", s.power, cmp.power),
                ("Precision", s.precision, cmp.precision),
                ("Toughness", s.toughness, cmp.toughness),
                ("Vitality", s.vitality, cmp.vitality),
                ("Condi Dmg", s.condition_damage, cmp.condition_damage),
                ("Ferocity", s.ferocity, cmp.ferocity),
                ("Healing", s.healing_power, cmp.healing_power),
                ("Expertise", s.expertise, cmp.expertise),
            ];
            for (name, val, cmp_val) in &rows {
                ui.text_colored(LABEL_COLOR, &format!("  {}: ", name));
                ui.same_line();
                let color = if compare_stats.is_some() {
                    stat_color(*val, *cmp_val)
                } else {
                    VALUE_COLOR
                };
                ui.text_colored(color, &format!("{}", val));
            }
            ui.text_colored(
                DIM_COLOR,
                &format!(
                    "  Crit {:.1}% | HP {} | Armor {}",
                    s.crit_chance, s.health, s.armor
                ),
            );
        });
    }
}

/// Render the SKILLS section for the suggestion.
pub fn render_suggestion_skills(ui: &Ui, suggestion: &super::super::comparison::BuildSuggestion) {
    render_card_section(ui, "SKILLS", |ui| {
        for skill in &suggestion.skills {
            ui.text_colored(VALUE_COLOR, &format!("  {}", skill));
        }
    });
}

/// Render the WEAPONS section for the suggestion.
pub fn render_suggestion_weapons(ui: &Ui, suggestion: &super::super::comparison::BuildSuggestion) {
    render_card_section(ui, "WEAPONS", |ui| {
        for weapon in &suggestion.weapons {
            ui.text_colored(VALUE_COLOR, &format!("  {}", weapon));
        }
        if !suggestion.sigils.is_empty() {
            ui.text_colored(
                DIM_COLOR,
                &format!("  Sigils: {}", suggestion.sigils.join(", ")),
            );
        }
    });
}

/// Render the GEAR section for the suggestion.
pub fn render_suggestion_gear(ui: &Ui, suggestion: &super::super::comparison::BuildSuggestion) {
    render_card_section(ui, "GEAR", |ui| {
        if !suggestion.stat_prefix.is_empty() {
            render_label_value(ui, "Prefix", &suggestion.stat_prefix);
        }
        if !suggestion.rune.is_empty() {
            render_label_value_colored(ui, "Rune", &suggestion.rune, GEAR_COLOR);
        }
        if !suggestion.relic.is_empty() {
            render_label_value_colored(ui, "Relic", &suggestion.relic, GEAR_COLOR);
        }
    });
}

/// Render the STATS section for the suggestion (single-column, no inner columns).
/// Color-coded against compare_stats if provided.
pub fn render_suggestion_stats(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    compare_stats: Option<&StatBlock>,
) {
    if let Some(ref s) = suggestion.estimated_stats {
        render_card_section(ui, "STATS", |ui| {
            let cmp = compare_stats.cloned().unwrap_or_default();
            let rows = [
                ("Power", s.power, cmp.power),
                ("Precision", s.precision, cmp.precision),
                ("Toughness", s.toughness, cmp.toughness),
                ("Vitality", s.vitality, cmp.vitality),
                ("Condi Dmg", s.condition_damage, cmp.condition_damage),
                ("Ferocity", s.ferocity, cmp.ferocity),
                ("Healing", s.healing_power, cmp.healing_power),
                ("Expertise", s.expertise, cmp.expertise),
            ];
            for (name, val, cmp_val) in &rows {
                ui.text_colored(LABEL_COLOR, &format!("  {}: ", name));
                ui.same_line();
                let color = if compare_stats.is_some() {
                    stat_color(*val, *cmp_val)
                } else {
                    VALUE_COLOR
                };
                ui.text_colored(color, &format!("{}", val));
            }
            ui.text_colored(
                DIM_COLOR,
                &format!(
                    "  Crit {:.1}% | HP {} | Armor {}",
                    s.crit_chance, s.health, s.armor
                ),
            );
        });
    }
}

// ─── Combat performance section renderers ───

fn render_combat_int(ui: &Ui, label: &str, val: i32, cmp_val: i32, has_cmp: bool) {
    ui.text_colored(LABEL_COLOR, &format!("  {}: ", label));
    ui.same_line();
    let color = if has_cmp {
        stat_color(val, cmp_val)
    } else {
        VALUE_COLOR
    };
    ui.text_colored(color, &format!("{}", val));
}

fn render_combat_pct(ui: &Ui, label: &str, val: f64, cmp_val: f64, has_cmp: bool) {
    ui.text_colored(LABEL_COLOR, &format!("  {}: ", label));
    ui.same_line();
    let color = if has_cmp {
        stat_color_f64(val, cmp_val)
    } else {
        VALUE_COLOR
    };
    ui.text_colored(color, &format!("{:.1}%", val));
}

fn render_combat_metrics_inner(ui: &Ui, c: &CombatMetrics, cmp: &CombatMetrics, has_cmp: bool) {
    render_combat_int(
        ui,
        "Effective Power",
        c.effective_power,
        cmp.effective_power,
        has_cmp,
    );
    render_combat_pct(ui, "Crit Chance", c.crit_chance, cmp.crit_chance, has_cmp);
    render_combat_int(
        ui,
        "Strike DPS",
        c.strike_dps_index,
        cmp.strike_dps_index,
        has_cmp,
    );
    render_combat_int(
        ui,
        "Condi DPS",
        c.condition_dps_index,
        cmp.condition_dps_index,
        has_cmp,
    );
    render_combat_int(
        ui,
        "Total DPS",
        c.total_dps_index,
        cmp.total_dps_index,
        has_cmp,
    );
    if c.boon_duration_pct > 0.1 || cmp.boon_duration_pct > 0.1 {
        render_combat_pct(
            ui,
            "Boon Duration",
            c.boon_duration_pct,
            cmp.boon_duration_pct,
            has_cmp,
        );
    }
    if c.condi_duration_pct > 0.1 || cmp.condi_duration_pct > 0.1 {
        render_combat_pct(
            ui,
            "Condi Duration",
            c.condi_duration_pct,
            cmp.condi_duration_pct,
            has_cmp,
        );
    }
    if c.healing_index > 0 || cmp.healing_index > 0 {
        render_combat_int(
            ui,
            "Healing Index",
            c.healing_index,
            cmp.healing_index,
            has_cmp,
        );
    }
    render_combat_int(
        ui,
        "Effective HP",
        c.effective_health,
        cmp.effective_health,
        has_cmp,
    );
}

/// Render COMBAT PERFORMANCE section for the current build.
/// Color-coded against compare metrics (suggestion's combat) if provided.
pub fn render_build_combat(
    ui: &Ui,
    combat: Option<&CombatMetrics>,
    compare: Option<&CombatMetrics>,
) {
    render_card_section(ui, "COMBAT PERFORMANCE", |ui| {
        if let Some(c) = combat {
            let cmp = compare.cloned().unwrap_or_default();
            render_combat_metrics_inner(ui, c, &cmp, compare.is_some());
        } else {
            ui.text_colored(DIM_COLOR, "  (not computed)");
        }
    });
}

/// Render COMBAT PERFORMANCE section for the suggestion.
/// Color-coded against compare metrics (current build's combat) if provided.
pub fn render_suggestion_combat(
    ui: &Ui,
    suggestion: &super::super::comparison::BuildSuggestion,
    compare: Option<&CombatMetrics>,
) {
    render_card_section(ui, "COMBAT PERFORMANCE", |ui| {
        if let Some(ref c) = suggestion.combat_solo {
            let cmp = compare.cloned().unwrap_or_default();
            render_combat_metrics_inner(ui, c, &cmp, compare.is_some());
        } else {
            ui.text_colored(DIM_COLOR, "  (not computed)");
        }
    });
}

/// Render rotation breakdown section (full-width).
pub fn render_rotation_section(ui: &Ui, rotation: &RotationBreakdown) {
    render_card_section(ui, "ROTATION BREAKDOWN", |ui| {
        ui.text_colored(
            VALUE_COLOR,
            &format!(
                "  Simulated DPS: {}  (Strike: {} | Condi: {})",
                rotation.simulated_dps, rotation.strike_dps, rotation.condition_dps
            ),
        );
        if rotation.stunbreak_count > 0 || rotation.has_stability {
            let mut parts = Vec::new();
            if rotation.stunbreak_count > 0 {
                parts.push(format!("Stunbreaks: {}", rotation.stunbreak_count));
            }
            if rotation.has_stability {
                parts.push(format!(
                    "Stability: {:.0}%",
                    rotation.stability_uptime * 100.0
                ));
            }
            ui.text_colored([0.4, 0.9, 0.4, 1.0], &format!("  {}", parts.join("  |  ")));
        }
        if !rotation.condition_uptime.is_empty() {
            let uptimes: Vec<String> = rotation
                .condition_uptime
                .iter()
                .filter(|(_, s)| *s > 0.01)
                .map(|(name, stacks)| format!("{}: {:.1}", name, stacks))
                .collect();
            if !uptimes.is_empty() {
                ui.text_colored(LABEL_COLOR, "  Condition Stacks:");
                ui.text_colored(VALUE_COLOR, &format!("    {}", uptimes.join("  |  ")));
            }
        }
        if !rotation.skill_usage.is_empty() {
            ui.text_colored(LABEL_COLOR, "  Key Skills:");
            for (name, casts, dps) in &rotation.skill_usage {
                if *casts > 0 {
                    ui.text_colored(DIM_COLOR, &format!("    {} x{} ({} DPS)", name, casts, dps));
                }
            }
        }
    });
}

/// Render the "WHY THIS BUILD" section (full-width, different background).
pub fn render_why_section(ui: &Ui, explanation: &str, changes: &[String]) {
    if explanation.is_empty() && changes.is_empty() {
        return;
    }
    ui.spacing();

    let pos = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    {
        let draw_list = ui.get_window_draw_list();
        draw_list
            .add_rect(
                [pos[0] - 2.0, pos[1]],
                [pos[0] + width + 2.0, pos[1] + 22.0],
                [0.18, 0.14, 0.06, 0.95],
            )
            .filled(true)
            .rounding(3.0)
            .build();
        draw_list.add_text(
            [pos[0] + 8.0, pos[1] + 3.0],
            SECTION_TITLE_COLOR,
            "WHY THIS BUILD",
        );
    }
    ui.dummy([width, 24.0]);

    if !explanation.is_empty() {
        ui.text_wrapped(explanation);
    }

    if !changes.is_empty() {
        ui.spacing();
        ui.text_colored(SECTION_TITLE_COLOR, "  CHANGES");
        for change in changes {
            ui.text_colored(GEAR_COLOR, &format!("    * {}", change));
        }
    }
    ui.spacing();
}
