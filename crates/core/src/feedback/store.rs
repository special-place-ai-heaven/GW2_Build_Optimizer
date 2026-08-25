//! On-disk persistence for the feedback feature: `messages.json` (the player's local
//! message list) and `feedback_taxonomy.json` (the cached server copy of the taxonomy).
//! Writes are crash-safe (`.tmp` + rename); reads never modify or delete a file.

use std::path::{Path, PathBuf};

use crate::feedback::message::{now_unix, FailReason, MessageStatus, MessagesFile};
use crate::feedback::taxonomy::FeedbackTaxonomy;

/// Reads and writes the feedback files under the addon directory.
#[derive(Debug, Clone)]
pub struct FeedbackStore {
    dir: PathBuf,
}

impl FeedbackStore {
    pub fn new(addon_dir: &Path) -> Self {
        Self {
            dir: addon_dir.to_path_buf(),
        }
    }

    /// `{addon_dir}/messages.json`
    pub fn messages_path(&self) -> PathBuf {
        self.dir.join("messages.json")
    }

    /// `{addon_dir}/feedback_taxonomy.json`
    pub fn taxonomy_path(&self) -> PathBuf {
        self.dir.join("feedback_taxonomy.json")
    }

    /// Load `messages.json`. A missing file yields the empty default; an unreadable
    /// or unparseable file logs a warning and yields the empty default without
    /// touching the file on disk. Any message still `Sending` was interrupted by
    /// an unload and is mapped to `Failed(Interrupted)` (in memory only; the caller
    /// persists it on the next `save`).
    pub fn load(&self) -> MessagesFile {
        let path = self.messages_path();
        if !path.exists() {
            return MessagesFile::default();
        }
        let mut file = match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|s| serde_json::from_str::<MessagesFile>(&s).map_err(|e| e.to_string()))
        {
            Ok(file) => file,
            Err(e) => {
                eprintln!("Warning: messages.json unreadable, starting empty: {e}");
                return MessagesFile::default();
            }
        };

        let now = now_unix();
        for m in file
            .messages
            .iter_mut()
            .filter(|m| m.status == MessageStatus::Sending)
        {
            m.status = MessageStatus::Failed;
            m.last_error = Some(FailReason::Interrupted);
            m.failed_at = Some(now);
        }
        file
    }

    /// Write `messages.json` crash-safely (`messages.json.tmp` then rename).
    pub fn save(&self, file: &MessagesFile) -> Result<(), String> {
        let json = serde_json::to_string_pretty(file)
            .map_err(|e| format!("Failed to serialize messages.json: {}", e))?;
        self.write_atomic(&self.messages_path(), json.as_bytes())
    }

    /// The cached server copy of the taxonomy, if present and parseable.
    pub fn load_taxonomy(&self) -> Option<FeedbackTaxonomy> {
        let path = self.taxonomy_path();
        let raw = std::fs::read_to_string(&path).ok()?;
        match FeedbackTaxonomy::parse(&raw) {
            Ok(tax) => Some(tax),
            Err(e) => {
                eprintln!(
                    "Warning: cached taxonomy {} unparseable, ignoring: {}",
                    path.display(),
                    e
                );
                None
            }
        }
    }

    /// Cache the server's taxonomy JSON verbatim (crash-safe write).
    pub fn save_taxonomy(&self, raw_json: &str) -> Result<(), String> {
        self.write_atomic(&self.taxonomy_path(), raw_json.as_bytes())
    }

    /// Write `bytes` to `<path>.tmp` then rename over `path`. The orphan `.tmp` is
    /// removed on either failure so repeated failed saves do not accumulate.
    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Failed to create {}: {}", self.dir.display(), e))?;

        let mut tmp_name = path
            .file_name()
            .map(|n| n.to_os_string())
            .ok_or_else(|| format!("Invalid path {}", path.display()))?;
        tmp_name.push(".tmp");
        let tmp_path = path.with_file_name(tmp_name);

        if let Err(e) = std::fs::write(&tmp_path, bytes) {
            let _ = std::fs::remove_file(&tmp_path);
            return Err(format!("Failed to write {}: {}", tmp_path.display(), e));
        }
        std::fs::rename(&tmp_path, path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            format!(
                "Failed to rename {} → {}: {}",
                tmp_path.display(),
                path.display(),
                e
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feedback::message::{FailReason, LastPath, LocalMessage, MessageStatus};

    /// Fresh temp dir per test; removed before and after so reruns start clean.
    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("gw2_feedback_test_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn msg(report_id: &str, status: MessageStatus) -> LocalMessage {
        LocalMessage {
            report_id: report_id.into(),
            short_id: None,
            sent_at: 1_000,
            category: "bug".into(),
            path: vec!["area_screen".into()],
            title: "Title".into(),
            body: "Body".into(),
            status,
            reply: None,
            replied_at: None,
            closing_note: None,
            last_error: None,
            failed_at: None,
            failed_payload: Some("{\"replay\":true}".into()),
            context_summary: "v1.6.0".into(),
        }
    }

    #[test]
    fn round_trip() {
        let dir = temp_dir("round_trip");
        let store = FeedbackStore::new(&dir);

        let file = MessagesFile {
            last_path: Some(LastPath {
                category: "bug".into(),
                path: vec!["area_screen".into(), "severity".into()],
            }),
            messages: vec![
                msg("a", MessageStatus::Received),
                LocalMessage {
                    last_error: Some(FailReason::RateLimited {
                        retry_after_secs: 30,
                    }),
                    failed_at: Some(5),
                    ..msg("b", MessageStatus::Failed)
                },
            ],
        };
        store.save(&file).unwrap();
        assert!(store.messages_path().exists());
        assert_eq!(store.messages_path(), dir.join("messages.json"));

        let loaded = store.load();
        assert_eq!(loaded, file);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_empty() {
        let dir = temp_dir("missing");
        let store = FeedbackStore::new(&dir);

        let loaded = store.load();
        assert_eq!(loaded, MessagesFile::default());
        assert!(
            !store.messages_path().exists(),
            "load must not create the file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unreadable_file_is_empty_and_untouched() {
        let dir = temp_dir("unreadable");
        std::fs::create_dir_all(&dir).unwrap();
        let store = FeedbackStore::new(&dir);

        let garbage = b"{not json";
        std::fs::write(store.messages_path(), garbage).unwrap();

        let loaded = store.load();
        assert_eq!(loaded, MessagesFile::default());

        let after = std::fs::read(store.messages_path()).unwrap();
        assert_eq!(
            after, garbage,
            "load must never rewrite or delete a bad file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sending_becomes_failed_interrupted_on_load() {
        let dir = temp_dir("interrupted");
        let store = FeedbackStore::new(&dir);

        let file = MessagesFile {
            last_path: None,
            messages: vec![
                msg("in_flight", MessageStatus::Sending),
                msg("done", MessageStatus::Received),
            ],
        };
        store.save(&file).unwrap();

        let before = now_unix();
        let loaded = store.load();
        let after = now_unix();

        let in_flight = &loaded.messages[0];
        assert_eq!(in_flight.report_id, "in_flight");
        assert_eq!(in_flight.status, MessageStatus::Failed);
        assert_eq!(in_flight.last_error, Some(FailReason::Interrupted));
        let failed_at = in_flight.failed_at.expect("failed_at is stamped");
        assert!(failed_at >= before && failed_at <= after);
        assert!(in_flight.resend_allowed(after));
        // The payload is kept so Resend can replay it.
        assert_eq!(
            in_flight.failed_payload.as_deref(),
            Some("{\"replay\":true}")
        );

        // Untouched message stays as it was.
        assert_eq!(loaded.messages[1], file.messages[1]);

        // Load only maps in memory; the file on disk still says "sending".
        let raw = std::fs::read_to_string(store.messages_path()).unwrap();
        assert!(raw.contains("\"sending\""));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn taxonomy_cache_round_trip() {
        let dir = temp_dir("taxonomy");
        let store = FeedbackStore::new(&dir);

        assert!(store.load_taxonomy().is_none(), "no cache yet");

        store
            .save_taxonomy(crate::feedback::taxonomy::EMBEDDED)
            .unwrap();
        assert_eq!(store.taxonomy_path(), dir.join("feedback_taxonomy.json"));
        // Written verbatim.
        let raw = std::fs::read_to_string(store.taxonomy_path()).unwrap();
        assert_eq!(raw, crate::feedback::taxonomy::EMBEDDED);

        let tax = store.load_taxonomy().expect("cached taxonomy parses");
        assert_eq!(tax.taxonomy_version, 1);

        // A corrupt cache is ignored, not deleted.
        std::fs::write(store.taxonomy_path(), "{oops").unwrap();
        assert!(store.load_taxonomy().is_none());
        assert_eq!(
            std::fs::read_to_string(store.taxonomy_path()).unwrap(),
            "{oops"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_is_atomic_no_tmp_left() {
        let dir = temp_dir("atomic");
        let store = FeedbackStore::new(&dir);

        store.save(&MessagesFile::default()).unwrap();
        // Overwrite an existing file too (rename over a target).
        store
            .save(&MessagesFile {
                last_path: None,
                messages: vec![msg("x", MessageStatus::Local)],
            })
            .unwrap();
        store
            .save_taxonomy(crate::feedback::taxonomy::EMBEDDED)
            .unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "no .tmp files after save: {leftovers:?}"
        );
        assert!(store.messages_path().exists());
        assert!(store.taxonomy_path().exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
