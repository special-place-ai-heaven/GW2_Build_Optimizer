//! Read a rotation out of the prose a build site publishes.
//!
//! Every site writes the same shape — `Rifle 3 > Shred > Rifle 5 > 2 >
//! Demolish` — weapon plus slot number, a bare number that keeps the last
//! weapon, or a skill name. 362 of the 740 synced pages carry one. Nothing
//! parsed it before; the timeline improvised its own opener and the referee
//! judged the site's build on a rotation the site never wrote.
//!
//! Only the arrow notation is read. A page's operating instructions ("use X
//! whenever possible") stay prose.

use std::collections::HashMap;

use crate::gamedb::GameDb;

/// Skill ids in the order the page's rotation line names them. Empty when
/// the page has no arrow line, or the profession is unknown.
pub fn parse_rotation(prose: &str, profession_name: &str, db: &GameDb) -> Vec<u32> {
    let Some(profession) = db.profession(profession_name) else {
        return Vec::new();
    };
    let Some(line) = rotation_line(prose) else {
        return Vec::new();
    };
    let by_name: HashMap<String, u32> = db
        .skills
        .values()
        .filter(|s| s.professions.is_empty() || s.professions.iter().any(|p| p == &profession.name))
        .map(|s| (s.name.to_ascii_lowercase(), s.id))
        .collect();

    let mut ids = Vec::new();
    let mut last_weapon: Option<String> = None;
    for token in line.split('>') {
        let token = clean(token);
        if token.is_empty() {
            continue;
        }
        // `Rifle 3`, `Spear 2/5`, `2`
        let (prefix, slots) = split_slots(&token);
        if !slots.is_empty() {
            let weapon = if prefix.is_empty() {
                last_weapon.clone()
            } else {
                profession
                    .weapons
                    .keys()
                    .find(|w| w.eq_ignore_ascii_case(prefix))
                    .cloned()
            };
            if let Some(weapon) = weapon {
                if let Some(info) = profession.weapons.get(&weapon) {
                    for slot in &slots {
                        let want = format!("Weapon_{slot}");
                        if let Some(skill) = info.skills.iter().find(|s| s.slot == want) {
                            ids.push(skill.id);
                        }
                    }
                }
                last_weapon = Some(weapon);
                continue;
            }
        }
        if let Some(&id) = by_name.get(&token.to_ascii_lowercase()) {
            ids.push(id);
        }
    }
    ids
}

/// The sentence with the most arrows, normalised to `>` separators.
fn rotation_line(prose: &str) -> Option<String> {
    prose
        .replace(['→', '➜'], ">")
        .replace("->", ">")
        .split(['\n', '.', ';'])
        .filter(|s| s.matches('>').count() >= 2)
        .max_by_key(|s| s.matches('>').count())
        .map(str::to_string)
}

fn clean(token: &str) -> String {
    token
        .trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .trim_start_matches("then ")
        .trim_start_matches("and ")
        .trim()
        .to_string()
}

/// `Rifle 3` → ("Rifle", [3]); `Spear 2/5` → ("Spear", [2, 5]); `2` → ("", [2]);
/// `Shred` → ("Shred", []).
fn split_slots(token: &str) -> (&str, Vec<u8>) {
    let (prefix, tail) = match token.rsplit_once(' ') {
        Some((p, t)) => (p.trim(), t),
        None => ("", token),
    };
    let slots: Vec<u8> = tail
        .split('/')
        .filter_map(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=5).contains(n))
        .collect();
    if slots.is_empty() || slots.len() != tail.split('/').count() {
        return (token, Vec::new());
    }
    (prefix, slots)
}

#[cfg(test)]
mod tests {
    use super::{rotation_line, split_slots};

    #[test]
    fn the_arrow_sentence_wins_over_the_prose_around_it() {
        let prose =
            "Open on the bruiser. Rifle 3 > Shred > Rifle 5 > 2 > Demolish. Reset if it fails.";
        assert_eq!(
            rotation_line(prose).as_deref(),
            Some(" Rifle 3 > Shred > Rifle 5 > 2 > Demolish")
        );
        assert_eq!(rotation_line("no rotation here"), None);
    }

    #[test]
    fn slots_split_the_way_sites_write_them() {
        assert_eq!(split_slots("Rifle 3"), ("Rifle", vec![3]));
        assert_eq!(split_slots("Spear 2/5"), ("Spear", vec![2, 5]));
        assert_eq!(split_slots("2"), ("", vec![2]));
        assert_eq!(split_slots("Shred"), ("Shred", vec![]));
        assert_eq!(split_slots("Photon Forge 3"), ("Photon Forge", vec![3]));
    }
}
