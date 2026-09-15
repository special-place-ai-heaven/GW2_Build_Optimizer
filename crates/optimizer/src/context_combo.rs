//! Compact wiki combo matrix + per-profession field/finisher gates for Gemini.

use crate::data::combos::{combos, ComboCell, ComboTable, ProfessionComboKit};

const FIELD_ORDER: &[&str] = &[
    "Dark",
    "Ethereal",
    "Fire",
    "Ice",
    "Light",
    "Lightning",
    "Poison",
    "Smoke",
    "Water",
];
const FINISHER_ORDER: &[&str] = &["blast", "leap", "projectile", "whirl"];

/// Wiki combo outcomes plus this profession's core vs gated kit.
///
/// `equipped_elite` is optional; when absent, gates stay labeled (do not assume an elite).
pub fn section_combo_reference(profession: &str, equipped_elite: Option<&str>) -> String {
    let table = combos();
    let mut out = String::from("=== COMBO REFERENCE ===\n");
    out.push_str("Outcomes (field × finisher):\n");
    for name in FIELD_ORDER {
        let Some(ff) = table.fields.get(*name) else {
            continue;
        };
        out.push_str(&format!(
            "{name}: {}; {}; {}; {}\n",
            format_finisher_cell("blast", &ff.blast),
            format_finisher_cell("leap", &ff.leap),
            format_finisher_cell("projectile", &ff.projectile),
            format_finisher_cell("whirl", &ff.whirl),
        ));
    }
    out.push('\n');
    out.push_str(&profession_digest(table, profession, equipped_elite));
    out
}

fn format_finisher_cell(kind: &str, cell: &ComboCell) -> String {
    let mut bits = vec![cell.effect.clone()];
    if let Some(s) = cell.stacks.filter(|&s| s > 0) {
        bits.push(format!("x{s}"));
    }
    if let Some(n) = cell.conditions_removed.filter(|&n| n > 0) {
        bits.push(format!("{n} cond"));
    }
    if let Some(d) = cell.duration_s {
        if (d - d.round()).abs() < 1e-9 {
            bits.push(format!("{}s", d as i64));
        } else {
            bits.push(format!("{d}s"));
        }
    }
    let tgt = match cell.target.as_str() {
        "area_allies" => "allies",
        "self" => "self",
        "bolts" => "bolts",
        "area_enemies" => "enemies",
        "ally_near_target" => "ally near target",
        _ => "",
    };
    if !tgt.is_empty() {
        bits.push(tgt.to_string());
    }
    format!("{kind}={}", bits.join(" "))
}

fn kit_for<'a>(table: &'a ComboTable, profession: &str) -> Option<&'a ProfessionComboKit> {
    table
        .profession_combos
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(profession))
        .map(|(_, v)| v)
}

fn gate_for<'a>(
    gates: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Option<&'a str> {
    gates
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(key))
        .map(|(_, v)| v.as_str())
}

fn elite_unlocks(elite: Option<&str>, via: &str) -> bool {
    let Some(elite) = elite.filter(|s| !s.is_empty()) else {
        return false;
    };
    let via_l = via.to_ascii_lowercase();
    let elite_l = elite.to_ascii_lowercase();
    via_l
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|w| w == elite_l)
}

fn availability_line(name: &str, unlocked: bool, via: &str, elite: Option<&str>) -> String {
    if unlocked {
        if let Some(e) = elite {
            format!("  {name}: yes ({e})\n")
        } else {
            format!("  {name}: yes\n")
        }
    } else {
        format!("  {name}: no ({via})\n")
    }
}

fn profession_digest(table: &ComboTable, profession: &str, equipped_elite: Option<&str>) -> String {
    let mut out = format!("This profession ({profession}):\n");
    let Some(kit) = kit_for(table, profession) else {
        out.push_str("  (no compact combo kit on file)\n");
        return out;
    };

    for fin in FINISHER_ORDER {
        if kit
            .core_finishers
            .iter()
            .any(|f| f.eq_ignore_ascii_case(fin))
        {
            out.push_str(&format!("  {fin}: yes\n"));
            continue;
        }
        if let Some(via) = gate_for(&kit.gated_finishers, fin) {
            let unlocked = elite_unlocks(equipped_elite, via);
            out.push_str(&availability_line(fin, unlocked, via, equipped_elite));
        } else {
            out.push_str(&format!("  {fin}: no\n"));
        }
    }

    let fields: Vec<&str> = FIELD_ORDER
        .iter()
        .copied()
        .filter(|f| kit.core_fields.iter().any(|c| c.eq_ignore_ascii_case(f)))
        .collect();
    if fields.is_empty() {
        out.push_str("  fields: (none on core kit)\n");
    } else {
        out.push_str(&format!("  fields: {}\n", fields.join(", ")));
    }

    for field in FIELD_ORDER {
        let Some(via) = gate_for(&kit.gated_fields, field) else {
            continue;
        };
        let unlocked = elite_unlocks(equipped_elite, via);
        out.push_str(&availability_line(field, unlocked, via, equipped_elite));
    }

    for note in &kit.notes {
        if !note.is_empty() {
            out.push_str(&format!("  note: {note}\n"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn necro_core_leap_locked() {
        let s = section_combo_reference("Necromancer", None);
        let lower = s.to_ascii_lowercase();
        assert!(lower.contains("leap"), "{s}");
        assert!(
            lower.contains("leap: no") || lower.contains("locked"),
            "leap should be locked on core necro:\n{s}"
        );
        assert!(lower.contains("reaper"), "{s}");
        assert!(lower.contains("sword"), "{s}");
    }

    #[test]
    fn guardian_projectile_locked() {
        let s = section_combo_reference("Guardian", None);
        let lower = s.to_ascii_lowercase();
        assert!(
            lower.contains("projectile: no") || lower.contains("locked"),
            "projectile should be locked on core guardian:\n{s}"
        );
        assert!(lower.contains("longbow"), "{s}");
        assert!(lower.contains("pistol"), "{s}");
    }

    #[test]
    fn fire_blast_line_contains_might() {
        let s = section_combo_reference("Warrior", None);
        let fire = s.lines().find(|l| l.starts_with("Fire:")).unwrap_or("");
        assert!(fire.contains("Might"), "Fire line: {fire}\nfull:\n{s}");
        assert!(fire.contains("blast="), "{fire}");
    }

    #[test]
    fn necro_reaper_unlocks_leap() {
        let s = section_combo_reference("Necromancer", Some("Reaper"));
        assert!(s.to_ascii_lowercase().contains("leap: yes"), "{s}");
    }
}
