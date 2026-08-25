//! Report payload for the feedback server (contract v1). Pure builder: it only sees the plain
//! inputs below, never `AppConfig` or `ResolvedBuild`, so keys and character names cannot leak.

use crate::types::GearPrefixGroups;
use serde::{Deserialize, Serialize};

/// Wire schema version sent as `schema_version`.
pub const SCHEMA_VERSION: u16 = 1;
/// Maximum body length in chars (server rejects longer).
pub const MAX_BODY_CHARS: usize = 4000;
/// Maximum title length in chars (server rejects longer).
pub const MAX_TITLE_CHARS: usize = 120;
/// Maximum compact-JSON size of `build_snapshot` in bytes.
pub const MAX_SNAPSHOT_BYTES: usize = 6 * 1024;
/// Maximum compact-JSON size of the whole request in bytes (server cap is 16384; keep headroom).
pub const MAX_REQUEST_BYTES: usize = 16_000;

/// Where the player was when they wrote the message. Server reads only
/// `addon_version` and `game_build`; the rest is stored verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReportContext {
    pub addon_version: String,
    pub game_build: Option<u32>,
    pub locale: String,
    pub mode: String,
    pub scale: String,
    pub role: String,
    pub profession: String,
    pub elite: String,
    pub llm_provider: String,
}

/// Opt-in slim build snapshot. Deliberately has no character name, account, or key fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BuildSnapshot {
    pub stat_prefix: String,
    pub gear_prefixes: GearPrefixGroups,
    /// `(specialization name, chosen trait names)` per spec line.
    pub specializations: Vec<(String, Vec<String>)>,
    pub weapons: Vec<String>,
    pub sigils: Vec<String>,
    pub skills: Vec<String>,
    pub rune: String,
    pub relic: String,
    pub chat_code: Option<String>,
}

/// The `POST /v1/reports` body. Field names are the wire names — do not rename.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u16,
    pub report_id: String,
    pub client_id: String,
    pub category: String,
    pub path: Vec<String>,
    pub title: String,
    pub body: String,
    pub contact: Option<String>,
    pub account: Option<String>,
    pub context: ReportContext,
    pub build_snapshot: Option<BuildSnapshot>,
}

/// Title shown in the server admin list: the first `MAX_TITLE_CHARS` chars of a body that is
/// at least 20 chars long, otherwise the choice labels joined with ` › `, otherwise the short
/// body itself, otherwise `(no title)`. Never empty, never longer than `MAX_TITLE_CHARS` chars.
pub fn title_for(body: &str, choice_labels: &[String]) -> String {
    // One line: newlines and runs of whitespace collapse to a single space.
    let body = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let cap = |s: &str| -> String {
        s.chars()
            .take(MAX_TITLE_CHARS)
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    if body.chars().count() >= 20 {
        return cap(&body);
    }
    if !choice_labels.is_empty() {
        return cap(&choice_labels.join(" › "));
    }
    if !body.is_empty() {
        return body;
    }
    "(no title)".to_string()
}

/// Compact-JSON size of the snapshot, the quantity the `MAX_SNAPSHOT_BYTES` gate is measured in.
pub fn snapshot_bytes(s: &BuildSnapshot) -> usize {
    to_compact(s).len()
}

/// Compact-JSON size of the whole report, the quantity the `MAX_REQUEST_BYTES` gate is measured in.
pub fn request_bytes(r: &Report) -> usize {
    to_json(r).len()
}

/// The exact bytes posted to the server (compact JSON).
pub fn to_json(r: &Report) -> String {
    to_compact(r)
}

fn to_compact<T: Serialize>(value: &T) -> String {
    // Only plain strings, numbers, vecs, and options: serialization cannot fail.
    serde_json::to_string(value).expect("report types serialize to JSON")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::taxonomy::FeedbackTaxonomy;
    use crate::types::GearPrefixGroups;
    use serde_json::Value;
    use std::collections::BTreeSet;

    fn sample_context() -> ReportContext {
        ReportContext {
            addon_version: "1.6.0".to_string(),
            game_build: Some(174122),
            locale: "en".to_string(),
            mode: "WvW".to_string(),
            scale: "Roam".to_string(),
            role: "Damage".to_string(),
            profession: "Ranger".to_string(),
            elite: "Untamed".to_string(),
            llm_provider: "gemini".to_string(),
        }
    }

    fn sample_report() -> Report {
        Report {
            schema_version: SCHEMA_VERSION,
            report_id: "7aabc139-7506-4d21-8956-222ebe78ee39".to_string(),
            client_id: "11111111-1111-4111-8111-111111111111".to_string(),
            category: "bug".to_string(),
            path: vec!["optimize".to_string(), "wrong".to_string()],
            title: "Optimize picks Trident on land".to_string(),
            body: "Expected a land weapon, got Trident on a Ranger in WvW roam.".to_string(),
            contact: None,
            account: None,
            context: sample_context(),
            build_snapshot: None,
        }
    }

    fn sample_snapshot() -> BuildSnapshot {
        BuildSnapshot {
            stat_prefix: "Marauder".to_string(),
            gear_prefixes: GearPrefixGroups {
                armor: "Marauder".to_string(),
                trinkets: "Berserker".to_string(),
                weapons: "Marauder".to_string(),
            },
            specializations: vec![
                (
                    "Skirmishing".to_string(),
                    vec!["Sharpened Edges".to_string()],
                ),
                ("Untamed".to_string(), vec!["Fervent Force".to_string()]),
            ],
            weapons: vec!["Hammer".to_string(), "Greatsword".to_string()],
            sigils: vec!["Force".to_string(), "Impact".to_string()],
            skills: vec!["Troll Unguent".to_string(), "Unleash".to_string()],
            rune: "Scholar".to_string(),
            relic: "Thief".to_string(),
            chat_code: Some("[&DQQ...]".to_string()),
        }
    }

    fn keys(v: &Value) -> BTreeSet<String> {
        v.as_object().expect("object").keys().cloned().collect()
    }

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn title_is_first_120_chars_when_body_long() {
        let body: String = "é".repeat(300);
        let title = title_for(&body, &["Bug".to_string()]);
        assert_eq!(title.chars().count(), MAX_TITLE_CHARS);
        assert_eq!(title, "é".repeat(120));
    }

    #[test]
    fn title_is_choice_labels_when_body_short() {
        let labels = vec!["Bug".to_string(), "Optimize".to_string()];
        assert_eq!(title_for("ok", &labels), "Bug › Optimize");
    }

    #[test]
    fn title_never_empty() {
        assert_eq!(title_for("", &[]), "(no title)");
        assert_eq!(title_for("   ", &[]), "(no title)");
        assert_eq!(title_for("short", &[]), "short");
        for body in ["", "   ", "short", &"x".repeat(500)] {
            let title = title_for(body, &[]);
            assert!(!title.is_empty());
            assert!(title.chars().count() <= MAX_TITLE_CHARS);
        }
    }

    #[test]
    fn snapshot_bytes_measures_compact_json() {
        let snap = sample_snapshot();
        let compact = serde_json::to_string(&snap).unwrap();
        assert_eq!(snapshot_bytes(&snap), compact.len());
        assert!(!compact.contains('\n'));
    }

    #[test]
    fn request_bytes_measures_compact_json() {
        let mut report = sample_report();
        report.build_snapshot = Some(sample_snapshot());
        let compact = serde_json::to_string(&report).unwrap();
        assert_eq!(request_bytes(&report), compact.len());
        assert_eq!(to_json(&report), compact);
    }

    #[test]
    fn report_serializes_to_contract_shape() {
        let report = sample_report();
        let v: Value = serde_json::from_str(&to_json(&report)).unwrap();

        assert_eq!(
            keys(&v),
            set(&[
                "schema_version",
                "report_id",
                "client_id",
                "category",
                "path",
                "title",
                "body",
                "contact",
                "account",
                "context",
                "build_snapshot",
            ])
        );
        assert_eq!(v["contact"], Value::Null);
        assert_eq!(v["account"], Value::Null);
        assert_eq!(v["build_snapshot"], Value::Null);
        assert_eq!(v["schema_version"], Value::from(1));
        assert_eq!(v["path"], serde_json::json!(["optimize", "wrong"]));

        assert_eq!(
            keys(&v["context"]),
            set(&[
                "addon_version",
                "game_build",
                "locale",
                "mode",
                "scale",
                "role",
                "profession",
                "elite",
                "llm_provider",
            ])
        );
        assert_eq!(v["context"]["game_build"], Value::from(174122));
    }

    // T009 — privacy guards.

    #[test]
    fn optional_fields_are_null_for_every_category() {
        // `Report` has no field that could carry an API key, a character name, or the
        // key's `TokenInfo.name`; the structural guard is the key-set assertion in
        // `report_serializes_to_contract_shape`. The addon-side context builder is
        // tested separately (T021). Here: every category with nothing opted in ships
        // `account` and `build_snapshot` as null.
        let taxonomy = FeedbackTaxonomy::embedded();
        assert!(!taxonomy.categories.is_empty());

        for category in &taxonomy.categories {
            let report = Report {
                schema_version: SCHEMA_VERSION,
                report_id: "7aabc139-7506-4d21-8956-222ebe78ee39".to_string(),
                client_id: "11111111-1111-4111-8111-111111111111".to_string(),
                category: category.id.clone(),
                path: vec!["optimize".to_string()],
                title: "A plain title".to_string(),
                body: "A plain body with nothing sensitive in it.".to_string(),
                contact: None,
                account: None,
                context: sample_context(),
                build_snapshot: None,
            };
            let json = to_json(&report);
            assert!(
                json.contains("\"account\":null"),
                "{}: account not null",
                category.id
            );
            assert!(
                json.contains("\"build_snapshot\":null"),
                "{}: build_snapshot not null",
                category.id
            );
        }
    }

    #[test]
    fn title_caps_label_branch_and_flattens_newlines() {
        let long_label = "x".repeat(200);
        let t = title_for("ok", &[long_label]);
        assert_eq!(t.chars().count(), MAX_TITLE_CHARS);

        let t = title_for("first line of the body\nsecond   line here", &[]);
        assert_eq!(t, "first line of the body second line here");
    }

    #[test]
    fn snapshot_has_no_character_name() {
        let v = serde_json::to_value(sample_snapshot()).unwrap();
        assert_eq!(
            keys(&v),
            set(&[
                "stat_prefix",
                "gear_prefixes",
                "specializations",
                "weapons",
                "sigils",
                "skills",
                "rune",
                "relic",
                "chat_code",
            ])
        );
    }
}
