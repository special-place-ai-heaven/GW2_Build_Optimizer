use nexus::imgui::{TreeNodeFlags, Ui};

use gw2_core::types::{ResolvedBuild, StatBlock};

pub fn render_build(ui: &Ui, build: &ResolvedBuild, stats: Option<&StatBlock>) {
    ui.text(&format!("{} — {}", build.character_name, build.profession));
    ui.separator();

    if ui.collapsing_header("Specializations", TreeNodeFlags::DEFAULT_OPEN) {
        render_specializations(ui, build);
    }

    if ui.collapsing_header("Skills", TreeNodeFlags::DEFAULT_OPEN) {
        render_skills(ui, build);
    }

    if ui.collapsing_header("Weapons", TreeNodeFlags::DEFAULT_OPEN) {
        render_weapons(ui, build);
    }

    if ui.collapsing_header("Armor", TreeNodeFlags::DEFAULT_OPEN) {
        render_armor(ui, build);
    }

    if ui.collapsing_header("Trinkets", TreeNodeFlags::DEFAULT_OPEN) {
        render_trinkets(ui, build);
    }

    if let Some(ref relic) = build.relic {
        if ui.collapsing_header("Relic", TreeNodeFlags::DEFAULT_OPEN) {
            ui.text(&format!("  {}", relic.name));
            if !relic.description.is_empty() {
                ui.text_colored([0.7, 0.7, 0.7, 1.0], &format!("  {}", relic.description));
            }
        }
    }

    if let Some(s) = stats {
        if ui.collapsing_header("Stats", TreeNodeFlags::DEFAULT_OPEN) {
            render_stats(ui, s);
        }
    }
}

fn render_specializations(ui: &Ui, build: &ResolvedBuild) {
    for spec in &build.specializations {
        let elite_marker = if spec.elite { " [Elite]" } else { "" };
        ui.text(&format!("  {}{}", spec.name, elite_marker));

        let trait_names: Vec<&str> = spec
            .traits_selected
            .iter()
            .map(|t| t.name.as_str())
            .collect();

        if !trait_names.is_empty() {
            ui.text_colored(
                [0.7, 0.85, 1.0, 1.0],
                &format!("    {}", trait_names.join(" | ")),
            );
        }
    }
}

fn render_skills(ui: &Ui, build: &ResolvedBuild) {
    let sk = &build.skills;

    if let Some(ref heal) = sk.heal {
        ui.text(&format!("  Heal: {}", heal.name));
    }

    let utils: Vec<String> = sk
        .utilities
        .iter()
        .filter_map(|u| u.as_ref().map(|s| s.name.clone()))
        .collect();
    if !utils.is_empty() {
        ui.text(&format!("  Utilities: {}", utils.join(", ")));
    }

    if let Some(ref elite) = sk.elite {
        ui.text(&format!("  Elite: {}", elite.name));
    }
}

fn render_weapons(ui: &Ui, build: &ResolvedBuild) {
    for set in &build.weapons {
        let mut parts = Vec::new();
        if let Some(ref mh) = set.main_hand {
            parts.push(mh.name.clone());
        }
        if let Some(ref oh) = set.off_hand {
            parts.push(oh.name.clone());
        }
        let weapons_str = parts.join(" / ");

        let sigil_names: Vec<&str> = set.sigils.iter().map(|s| s.name.as_str()).collect();
        let sigils_str = if sigil_names.is_empty() {
            String::new()
        } else {
            format!("  [{}]", sigil_names.join(", "))
        };

        ui.text(&format!("  {}: {}{}", set.label, weapons_str, sigils_str));
    }
}

fn render_armor(ui: &Ui, build: &ResolvedBuild) {
    if let Some(ref r) = build.rune {
        ui.text_colored([0.9, 0.8, 0.3, 1.0], &format!("  Rune: {}", r.name));
    }
    for piece in &build.armor {
        let prefix = if piece.stat_prefix.is_empty() {
            String::new()
        } else {
            format!(" ({})", piece.stat_prefix)
        };
        ui.text(&format!("  {}: {}{}", piece.slot, piece.name, prefix));
    }
}

fn render_trinkets(ui: &Ui, build: &ResolvedBuild) {
    for piece in &build.trinkets {
        let prefix = if piece.stat_prefix.is_empty() {
            String::new()
        } else {
            format!(" ({})", piece.stat_prefix)
        };
        ui.text(&format!("  {}: {}{}", piece.slot, piece.name, prefix));
    }
}

fn render_stats(ui: &Ui, s: &StatBlock) {
    ui.columns(2, "##stat_cols", true);

    let stat_rows = [
        ("Power", s.power),
        ("Precision", s.precision),
        ("Toughness", s.toughness),
        ("Vitality", s.vitality),
        ("Condi Dmg", s.condition_damage),
        ("Expertise", s.expertise),
        ("Concentration", s.concentration),
        ("Ferocity", s.ferocity),
        ("Healing", s.healing_power),
    ];

    for (name, val) in &stat_rows {
        ui.text(&format!("{}: {}", name, val));
        ui.next_column();
    }

    ui.columns(1, "##end_cols", false);
    ui.spacing();

    ui.text(&format!(
        "  Crit: {:.1}%  |  Crit Dmg: {:.1}%  |  Health: {}  |  Armor: {}",
        s.crit_chance, s.crit_damage, s.health, s.armor
    ));
}
