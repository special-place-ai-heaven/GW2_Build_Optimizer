//! Side-by-side build comparison view.
//! Shows current build vs optimized build with stat diffs and LLM explanation.

use nexus::imgui::{ChildWindow, Selectable, TreeNodeFlags, Ui};

use gw2_core::types::{ResolvedBuild, StatBlock};

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
    pub changes_made: Vec<String>,
    pub estimated_stats: Option<StatBlock>,
}

/// State for the comparison view.
#[derive(Default)]
pub struct ComparisonState {
    pub suggestions: Vec<BuildSuggestion>,
    pub selected_suggestion: usize,
    pub loading: bool,
    pub error: Option<String>,
}

/// Render the comparison view: current build on left, suggestion on right.
pub fn render_comparison(
    ui: &Ui,
    current_build: &ResolvedBuild,
    current_stats: Option<&StatBlock>,
    comparison: &ComparisonState,
) {
    if comparison.loading {
        ui.text("Optimizing build...");
        ui.text("Consulting Gemini for synergy analysis...");
        return;
    }

    if let Some(ref err) = comparison.error {
        ui.text_colored([1.0, 0.3, 0.0, 1.0], &format!("Error: {}", err));
        return;
    }

    if comparison.suggestions.is_empty() {
        ui.text("No suggestions available. Run the optimizer first.");
        return;
    }

    // Suggestion tabs
    let tab_count = comparison.suggestions.len();
    if tab_count > 1 {
        ui.text("Suggestions:");
        ui.same_line();
        for (i, suggestion) in comparison.suggestions.iter().enumerate() {
            let selected = comparison.selected_suggestion == i;
            let label = if suggestion.label.is_empty() {
                format!("Build {}", i + 1)
            } else {
                suggestion.label.clone()
            };
            if Selectable::new(&format!("[{}]", label))
                .selected(selected)
                .size([0.0, 0.0])
                .build(ui)
            {
                // Selection handled by caller via return value
            }
            if i < tab_count - 1 {
                ui.same_line();
            }
        }
        ui.separator();
    }

    let suggestion = &comparison.suggestions[comparison.selected_suggestion.min(tab_count - 1)];

    // Two-column layout
    let avail = ui.content_region_avail();
    let col_width = (avail[0] - 20.0) / 2.0;

    // Left column: Current Build
    ChildWindow::new("##current_col")
        .size([col_width, 0.0])
        .build(ui, || {
            ui.text_colored([0.6, 0.8, 1.0, 1.0], "CURRENT BUILD");
            ui.separator();
            render_current_build_summary(ui, current_build);
        });

    ui.same_line();

    // Divider
    ui.text("|");
    ui.same_line();

    // Right column: Suggested Build
    ChildWindow::new("##suggested_col")
        .size([col_width, 0.0])
        .build(ui, || {
            ui.text_colored([0.3, 1.0, 0.3, 1.0], "OPTIMIZED BUILD");
            ui.separator();
            render_suggestion_summary(ui, suggestion);
        });

    // Stat diff panel (below both columns)
    ui.spacing();
    ui.separator();

    if let Some(ref est_stats) = suggestion.estimated_stats {
        if let Some(cur_stats) = current_stats {
            if ui.collapsing_header("Stat Comparison", TreeNodeFlags::DEFAULT_OPEN) {
                render_stat_diff(ui, cur_stats, est_stats);
            }
        }
    }

    // LLM explanation
    if !suggestion.explanation.is_empty() {
        if ui.collapsing_header("Why This Build?", TreeNodeFlags::DEFAULT_OPEN) {
            ui.text_wrapped(&suggestion.explanation);
        }
    }

    // Changes made
    if !suggestion.changes_made.is_empty() {
        if ui.collapsing_header("Changes Made", TreeNodeFlags::DEFAULT_OPEN) {
            for change in &suggestion.changes_made {
                ui.bullet_text(change);
            }
        }
    }
}

fn render_current_build_summary(ui: &Ui, build: &ResolvedBuild) {
    // Specs
    for spec in &build.specializations {
        let elite = if spec.elite { " [E]" } else { "" };
        ui.text(&format!("{}{}", spec.name, elite));
        let traits: Vec<&str> = spec.traits_selected.iter().map(|t| t.name.as_str()).collect();
        if !traits.is_empty() {
            ui.text_colored([0.7, 0.7, 0.7, 1.0], &format!("  {}", traits.join(" | ")));
        }
    }
    ui.spacing();

    // Skills
    if let Some(ref h) = build.skills.heal {
        ui.text(&format!("Heal: {}", h.name));
    }
    let utils: Vec<String> = build.skills.utilities.iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone())).collect();
    if !utils.is_empty() {
        ui.text(&format!("Utils: {}", utils.join(", ")));
    }
    if let Some(ref e) = build.skills.elite {
        ui.text(&format!("Elite: {}", e.name));
    }
    ui.spacing();

    // Weapons
    for set in &build.weapons {
        let mut parts = Vec::new();
        if let Some(ref mh) = set.main_hand { parts.push(mh.name.clone()); }
        if let Some(ref oh) = set.off_hand { parts.push(oh.name.clone()); }
        ui.text(&format!("{}: {}", set.label, parts.join(" / ")));
    }
    ui.spacing();

    // Gear summary
    if let Some(ref r) = build.rune {
        ui.text(&format!("Rune: {}", r.name));
    }
    if let Some(ref r) = build.relic {
        ui.text(&format!("Relic: {}", r.name));
    }
    if !build.armor.is_empty() {
        let prefix = &build.armor[0].stat_prefix;
        if !prefix.is_empty() {
            ui.text(&format!("Gear: {}", prefix));
        }
    }
}

fn render_suggestion_summary(ui: &Ui, suggestion: &BuildSuggestion) {
    // Specs
    for (spec_name, traits) in &suggestion.specializations {
        ui.text(spec_name);
        if !traits.is_empty() {
            ui.text_colored([0.7, 0.7, 0.7, 1.0], &format!("  {}", traits.join(" | ")));
        }
    }
    ui.spacing();

    // Skills
    for skill in &suggestion.skills {
        ui.text(skill);
    }
    ui.spacing();

    // Weapons
    for weapon in &suggestion.weapons {
        ui.text(weapon);
    }
    ui.spacing();

    // Gear
    if !suggestion.rune.is_empty() {
        ui.text(&format!("Rune: {}", suggestion.rune));
    }
    if !suggestion.relic.is_empty() {
        ui.text(&format!("Relic: {}", suggestion.relic));
    }
    if !suggestion.stat_prefix.is_empty() {
        ui.text(&format!("Gear: {}", suggestion.stat_prefix));
    }
    if !suggestion.sigils.is_empty() {
        ui.text(&format!("Sigils: {}", suggestion.sigils.join(", ")));
    }
}

fn render_stat_diff(ui: &Ui, current: &StatBlock, suggested: &StatBlock) {
    let stats = [
        ("Power", current.power, suggested.power),
        ("Precision", current.precision, suggested.precision),
        ("Toughness", current.toughness, suggested.toughness),
        ("Vitality", current.vitality, suggested.vitality),
        ("Condi Dmg", current.condition_damage, suggested.condition_damage),
        ("Expertise", current.expertise, suggested.expertise),
        ("Concentration", current.concentration, suggested.concentration),
        ("Ferocity", current.ferocity, suggested.ferocity),
        ("Healing", current.healing_power, suggested.healing_power),
    ];

    ui.columns(4, "##stat_diff_cols", true);

    ui.text("Stat");
    ui.next_column();
    ui.text("Current");
    ui.next_column();
    ui.text("Optimized");
    ui.next_column();
    ui.text("Diff");
    ui.next_column();

    ui.separator();

    for (name, cur, sug) in &stats {
        ui.text(name);
        ui.next_column();

        ui.text(&format!("{:.0}", cur));
        ui.next_column();

        ui.text(&format!("{:.0}", sug));
        ui.next_column();

        let diff = *sug - *cur;
        let color = if diff > 0 {
            [0.0, 1.0, 0.0, 1.0] // green = better
        } else if diff < 0 {
            [1.0, 0.0, 0.0, 1.0] // red = worse
        } else {
            [0.7, 0.7, 0.7, 1.0] // gray = same
        };
        let sign = if diff > 0 { "+" } else { "" };
        ui.text_colored(color, &format!("{}{}", sign, diff));
        ui.next_column();
    }

    ui.columns(1, "##end_diff", false);
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
}
