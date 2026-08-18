//! One-page shopping-list loadout: prefix on every slot, nested upgrades, icons.

use gw2_core::i18n::{loc_weapon_types, t};
use gw2_core::types::{CombatMetrics, ResolvedBuild};
use gw2_optimizer::gamedb::GameDb;
use nexus::imgui::Ui;

use super::comparison::BuildSuggestion;
use super::gear_diff::parse_suggestion_weapons;
use super::theme;
use super::{comparison, icons};

const ARMOR_SLOTS: [&str; 6] = ["Helm", "Shoulders", "Coat", "Gloves", "Leggings", "Boots"];
const TRINKET_SLOTS: [&str; 6] = [
    "Backpack",
    "Accessory1",
    "Accessory2",
    "Amulet",
    "Ring1",
    "Ring2",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainTint {
    None,
    Up,
    Down,
}

pub fn slot_label(api: &str) -> String {
    t(match api {
        "Helm" => "slot.helm",
        "Shoulders" => "slot.shoulders",
        "Coat" => "slot.chest",
        "Gloves" => "slot.gloves",
        "Leggings" => "slot.legs",
        "Boots" => "slot.boots",
        "Backpack" => "slot.back",
        "Accessory1" | "Accessory2" => "slot.accessory",
        "Amulet" => "slot.amulet",
        "Ring1" | "Ring2" => "slot.ring",
        _ => return api.to_string(),
    })
}

/// Combat result delta — not Power. DPS first, then healing, then eHP.
pub fn combat_gain(cur: Option<&CombatMetrics>, opt: Option<&CombatMetrics>) -> i32 {
    let (Some(c), Some(o)) = (cur, opt) else {
        return 0;
    };
    let dps = o.total_dps_index - c.total_dps_index;
    if dps != 0 {
        return dps;
    }
    let heal = o.healing_index - c.healing_index;
    if heal != 0 {
        return heal;
    }
    o.effective_health - c.effective_health
}

pub fn slot_tint(changed: bool, viewing_optimized: bool, gain: i32) -> GainTint {
    if !changed || gain == 0 {
        return GainTint::None;
    }
    let opt_is_better = gain > 0;
    if viewing_optimized == opt_is_better {
        GainTint::Up
    } else {
        GainTint::Down
    }
}

fn icon_tint(gain: GainTint) -> [f32; 4] {
    match gain {
        GainTint::None => [1.0, 1.0, 1.0, 1.0],
        GainTint::Up => [0.78, 1.0, 0.78, 1.0],
        GainTint::Down => [1.0, 0.78, 0.78, 1.0],
    }
}

pub fn render_view_toggle(ui: &Ui, show_optimized: &mut bool) {
    if theme::pill(ui, &t("label.current"), !*show_optimized, "##view_current") {
        *show_optimized = false;
    }
    ui.same_line_with_spacing(0.0, 6.0);
    if theme::pill(ui, &t("label.optimized"), *show_optimized, "##view_optimized") {
        *show_optimized = true;
    }
}

pub fn render_current_sheet(
    ui: &Ui,
    build: &ResolvedBuild,
    suggestion: Option<&BuildSuggestion>,
    db: Option<&GameDb>,
    viewing_optimized: bool,
    gain: i32,
) {
    if viewing_optimized {
        if let Some(sug) = suggestion {
            render_suggestion_sheet(ui, build, sug, db, true, gain);
            return;
        }
    }
    render_resolved_sheet(ui, build, suggestion, db, false, gain);
}

fn render_resolved_sheet(
    ui: &Ui,
    build: &ResolvedBuild,
    suggestion: Option<&BuildSuggestion>,
    db: Option<&GameDb>,
    viewing_optimized: bool,
    gain: i32,
) {
    let rune_name = build.rune.as_ref().map(|r| r.name.as_str()).unwrap_or("");
    let rune_url = db.and_then(|d| {
        build
            .rune
            .as_ref()
            .and_then(|r| icons::item_url(d, r.id))
            .or_else(|| icons::upgrade_url(d, rune_name))
    });
    let sug_prefix = suggestion.map(|s| s.stat_prefix.as_str()).unwrap_or("");
    let sug_rune = suggestion.map(|s| s.rune.as_str()).unwrap_or("");

    gear_columns(
        ui,
        |ui| {
            section(ui, &t("section.armor"));
            for slot in ARMOR_SLOTS {
                let piece = build.armor.iter().find(|p| p.slot == slot);
                let prefix = piece.map(|p| p.stat_prefix.as_str()).unwrap_or("");
                let name = piece.map(|p| p.name.as_str()).unwrap_or("");
                let url = db.and_then(|d| piece.and_then(|p| icons::item_url(d, p.id)));
                let changed = suggestion.is_some()
                    && (prefix != sug_prefix && !sug_prefix.is_empty() || rune_name != sug_rune);
                let other = suggestion.map(|s| format!("{} {}", s.stat_prefix, slot_label(slot)));
                row(
                    ui,
                    db,
                    url,
                    prefix,
                    slot_label(slot),
                    name,
                    rune_name,
                    rune_url,
                    other.as_deref(),
                    slot_tint(changed, viewing_optimized, gain),
                );
            }
        },
        |ui| {
            section(ui, &t("section.trinkets"));
            for slot in TRINKET_SLOTS {
                let piece = build.trinkets.iter().find(|p| p.slot == slot);
                let prefix = piece.map(|p| p.stat_prefix.as_str()).unwrap_or("");
                let name = piece.map(|p| p.name.as_str()).unwrap_or("");
                let url = db.and_then(|d| piece.and_then(|p| icons::item_url(d, p.id)));
                let changed =
                    suggestion.is_some() && prefix != sug_prefix && !sug_prefix.is_empty();
                let other = suggestion.map(|s| format!("{} {}", s.stat_prefix, slot_label(slot)));
                row(
                    ui,
                    db,
                    url,
                    prefix,
                    slot_label(slot),
                    name,
                    "",
                    None,
                    other.as_deref(),
                    slot_tint(changed, viewing_optimized, gain),
                );
            }
            if let Some(relic) = &build.relic {
                let sug_relic = suggestion.map(|s| s.relic.as_str()).unwrap_or("");
                let url = db.and_then(|d| {
                    icons::item_url(d, relic.id).or_else(|| icons::upgrade_url(d, &relic.name))
                });
                let other = suggestion
                    .filter(|s| !s.relic.is_empty())
                    .map(|s| s.relic.as_str());
                row(
                    ui,
                    db,
                    url,
                    "",
                    "Relic",
                    &relic.name,
                    "",
                    None,
                    other,
                    slot_tint(
                        suggestion.is_some() && relic.name != sug_relic,
                        viewing_optimized,
                        gain,
                    ),
                );
            }
        },
        |ui| {
            section(ui, &t("section.weapons"));
            let sug_sets = suggestion
                .map(|s| parse_suggestion_weapons(&s.weapons))
                .unwrap_or_default();
            for (i, set) in build.weapons.iter().enumerate() {
                let parts: Vec<&str> = [set.main_hand.as_ref(), set.off_hand.as_ref()]
                    .into_iter()
                    .flatten()
                    .map(|w| {
                        if w.weapon_type.is_empty() {
                            w.name.as_str()
                        } else {
                            w.weapon_type.as_str()
                        }
                    })
                    .collect();
                let label = parts.join(" / ");
                let sug_line = sug_sets.get(i).map(|(_, v)| v.as_str()).unwrap_or("");
                let url = db.and_then(|d| {
                    set.main_hand
                        .as_ref()
                        .and_then(|w| icons::item_url(d, w.id))
                        .or_else(|| {
                            set.main_hand.as_ref().and_then(|w| {
                                icons::weapon_type_url(d, &build.profession, &w.weapon_type)
                            })
                        })
                });
                let sigils: Vec<String> = set.sigils.iter().map(|s| s.name.clone()).collect();
                weapon_row(
                    ui,
                    db,
                    url,
                    &set.stat_prefix,
                    &set.label,
                    &label,
                    &sigils,
                    if sug_line.is_empty() {
                        None
                    } else {
                        Some(sug_line)
                    },
                    slot_tint(
                        suggestion.is_some() && !sug_line.is_empty() && sug_line != label,
                        viewing_optimized,
                        gain,
                    ),
                );
            }
        },
    );
}

fn render_suggestion_sheet(
    ui: &Ui,
    current: &ResolvedBuild,
    sug: &BuildSuggestion,
    db: Option<&GameDb>,
    viewing_optimized: bool,
    gain: i32,
) {
    let rune_url = db.and_then(|d| icons::upgrade_url(d, &sug.rune));
    gear_columns(
        ui,
        |ui| {
            section(ui, &t("section.armor"));
            for slot in ARMOR_SLOTS {
                let cur = current.armor.iter().find(|p| p.slot == slot);
                let cur_prefix = cur.map(|p| p.stat_prefix.as_str()).unwrap_or("");
                let other = cur.map(|p| {
                    format!(
                        "{} {}",
                        if p.stat_prefix.is_empty() {
                            ""
                        } else {
                            p.stat_prefix.as_str()
                        },
                        slot_label(slot)
                    )
                });
                row(
                    ui,
                    db,
                    db.and_then(|d| cur.and_then(|p| icons::item_url(d, p.id))),
                    &sug.stat_prefix,
                    slot_label(slot),
                    cur.map(|p| p.name.as_str()).unwrap_or(""),
                    &sug.rune,
                    rune_url,
                    other.as_deref(),
                    slot_tint(
                        cur_prefix != sug.stat_prefix && !sug.stat_prefix.is_empty(),
                        viewing_optimized,
                        gain,
                    ),
                );
            }
        },
        |ui| {
            section(ui, &t("section.trinkets"));
            for slot in TRINKET_SLOTS {
                let cur = current.trinkets.iter().find(|p| p.slot == slot);
                let cur_prefix = cur.map(|p| p.stat_prefix.as_str()).unwrap_or("");
                let other = cur.map(|p| format!("{} {}", p.stat_prefix, slot_label(slot)));
                row(
                    ui,
                    db,
                    db.and_then(|d| cur.and_then(|p| icons::item_url(d, p.id))),
                    &sug.stat_prefix,
                    slot_label(slot),
                    cur.map(|p| p.name.as_str()).unwrap_or(""),
                    "",
                    None,
                    other.as_deref(),
                    slot_tint(
                        cur_prefix != sug.stat_prefix && !sug.stat_prefix.is_empty(),
                        viewing_optimized,
                        gain,
                    ),
                );
            }
            if !sug.relic.is_empty() {
                let cur_relic = current
                    .relic
                    .as_ref()
                    .map(|r| r.name.as_str())
                    .unwrap_or("");
                row(
                    ui,
                    db,
                    db.and_then(|d| icons::upgrade_url(d, &sug.relic)),
                    "",
                    "Relic",
                    &sug.relic,
                    "",
                    None,
                    if cur_relic.is_empty() {
                        None
                    } else {
                        Some(cur_relic)
                    },
                    slot_tint(cur_relic != sug.relic, viewing_optimized, gain),
                );
            }
        },
        |ui| {
            section(ui, &t("section.weapons"));
            let sets = parse_suggestion_weapons(&sug.weapons);
            let sigil_pairs = sug.sigils.chunks(2);
            for (i, ((label, weapons), sigils)) in sets.iter().zip(sigil_pairs).enumerate() {
                let url = db.and_then(|d| {
                    let wt = weapons.split(" / ").next().unwrap_or(weapons).trim();
                    icons::weapon_type_url(d, &current.profession, wt)
                });
                let cur = current.weapons.get(i).map(|w| {
                    let parts: Vec<&str> = [w.main_hand.as_ref(), w.off_hand.as_ref()]
                        .into_iter()
                        .flatten()
                        .map(|x| {
                            if x.weapon_type.is_empty() {
                                x.name.as_str()
                            } else {
                                x.weapon_type.as_str()
                            }
                        })
                        .collect();
                    parts.join(" / ")
                });
                let sigil_names: Vec<String> = sigils.to_vec();
                weapon_row(
                    ui,
                    db,
                    url,
                    &sug.stat_prefix,
                    label,
                    weapons,
                    &sigil_names,
                    cur.as_deref(),
                    slot_tint(
                        cur.as_deref() != Some(weapons.as_str()),
                        viewing_optimized,
                        gain,
                    ),
                );
            }
            if sets.is_empty() {
                for w in &sug.weapons {
                    ui.text_colored(theme::CREAM, w);
                }
            }
        },
    );
}

fn gear_columns(ui: &Ui, a: impl FnOnce(&Ui), b: impl FnOnce(&Ui), c: impl FnOnce(&Ui)) {
    ui.columns(3, "##gear_cols", false);
    a(ui);
    ui.next_column();
    b(ui);
    ui.next_column();
    c(ui);
    ui.columns(1, "##gear_end", false);
}

fn section(ui: &Ui, title: &str) {
    ui.spacing();
    ui.text_colored(theme::GOLD, title);
}

fn row(
    ui: &Ui,
    db: Option<&GameDb>,
    url: Option<&str>,
    prefix: &str,
    slot: impl AsRef<str>,
    name: &str,
    nested: &str,
    nested_url: Option<&str>,
    other: Option<&str>,
    tint: GainTint,
) {
    let slot = slot.as_ref();
    const ICON: f32 = 28.0;
    const GAP: f32 = 14.0;
    let p = ui.cursor_screen_pos();
    icons::draw(ui, url, ICON, icon_tint(tint));
    if ui.is_item_hovered() {
        let key = if name.is_empty() { slot } else { name };
        if db.and_then(|d| comparison::inspect_text(key, d)).is_none() {
            tooltip(ui, prefix, slot, name, nested, other);
        }
        comparison::inspect_if_hovered(ui, key, db);
    }
    ui.same_line();
    ui.set_cursor_screen_pos([p[0] + ICON + GAP, p[1]]);
    if !prefix.is_empty() {
        ui.text_colored(theme::GOLD, comparison::loc_name(db, prefix));
        ui.same_line();
    }
    ui.text_colored(theme::CREAM, slot);
    if !name.is_empty() && name != slot {
        ui.same_line();
        ui.text_colored(theme::MUTED, comparison::loc_name(db, name));
    }
    if ui.is_item_hovered() {
        tooltip(ui, prefix, slot, name, nested, other);
    }

    if !nested.is_empty() {
        ui.indent();
        let np = ui.cursor_screen_pos();
        icons::draw(ui, nested_url, 18.0, [1.0, 1.0, 1.0, 1.0]);
        ui.same_line();
        ui.set_cursor_screen_pos([np[0] + 18.0 + 10.0, np[1]]);
        ui.text_colored(theme::MUTED, comparison::loc_name(db, nested));
        comparison::inspect_if_hovered(ui, nested, db);
        ui.unindent();
    }
}

fn weapon_row(
    ui: &Ui,
    db: Option<&GameDb>,
    url: Option<&str>,
    prefix: &str,
    set_label: &str,
    weapons: &str,
    sigils: &[String],
    other: Option<&str>,
    tint: GainTint,
) {
    const ICON: f32 = 28.0;
    const GAP: f32 = 14.0;
    let p = ui.cursor_screen_pos();
    icons::draw(ui, url, ICON, icon_tint(tint));
    ui.same_line();
    ui.set_cursor_screen_pos([p[0] + ICON + GAP, p[1]]);
    if !prefix.is_empty() {
        ui.text_colored(theme::GOLD, comparison::loc_name(db, prefix));
        ui.same_line();
    }
    ui.text_colored(theme::CREAM, set_label);
    ui.same_line();
    ui.text_colored(theme::CREAM, loc_weapon_types(weapons));
    if ui.is_item_hovered() {
        tooltip(ui, prefix, set_label, weapons, &sigils.join(" · "), other);
    }
    for sig in sigils {
        if sig.is_empty() {
            continue;
        }
        ui.indent();
        let sp = ui.cursor_screen_pos();
        let surl = db.and_then(|d| icons::upgrade_url(d, sig));
        icons::draw(ui, surl, 18.0, [1.0, 1.0, 1.0, 1.0]);
        ui.same_line();
        ui.set_cursor_screen_pos([sp[0] + 18.0 + 10.0, sp[1]]);
        ui.text_colored(theme::MUTED, comparison::loc_name(db, sig));
        comparison::inspect_if_hovered(ui, sig, db);
        ui.unindent();
    }
}

fn tooltip(ui: &Ui, prefix: &str, slot: &str, name: &str, nested: &str, other: Option<&str>) {
    crate::ui::theme::wide_tooltip(ui, |ui| {
        let shown = format!("{} {} {}", prefix, slot, name);
        ui.text_colored(theme::GOLD, shown.trim());
        if !nested.is_empty() {
            ui.text_colored(theme::MUTED, nested);
        }
        if let Some(o) = other {
            ui.spacing();
            ui.text_colored(theme::MUTED, t("gear.other"));
            ui.text_colored(theme::CREAM, o);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coat_is_chest() {
        gw2_core::i18n::set_language("en");
        assert_eq!(slot_label("Coat"), "Chest");
        assert_eq!(slot_label("Helm"), "Helm");
    }

    #[test]
    fn tint_follows_combat_not_power() {
        assert_eq!(slot_tint(false, true, 100), GainTint::None);
        assert_eq!(slot_tint(true, true, 50), GainTint::Up);
        assert_eq!(slot_tint(true, false, 50), GainTint::Down);
        assert_eq!(slot_tint(true, true, -20), GainTint::Down);
    }
}
