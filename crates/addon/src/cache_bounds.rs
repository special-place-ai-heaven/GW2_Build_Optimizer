//! Shared caps for Nexus textures and on-disk image caches.
//!
//! Nexus never frees textures until game exit (nexus-rs issue #138), so a
//! session must not upload without bound. Disk folders grow for the life of
//! the install unless writes evict oldest-by-mtime.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Admit `id` into the session texture set. Known ids stay creatable; a new
/// id past `cap` is refused (letter plate / no still).
pub(crate) fn admit_texture(set: &mut HashSet<String>, id: &str, cap: usize) -> bool {
    if set.contains(id) {
        return true;
    }
    if set.len() >= cap {
        return false;
    }
    set.insert(id.to_string());
    true
}

/// Keep a cache folder bounded: after a write, delete the oldest files (by
/// mtime) beyond `cap`. Worker thread only.
pub(crate) fn evict_oldest(dir: &Path, cap: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let files: Vec<(SystemTime, PathBuf)> = rd
        .filter_map(|e| {
            let e = e.ok()?;
            let md = e.metadata().ok()?;
            if !md.is_file() {
                return None;
            }
            Some((md.modified().ok()?, e.path()))
        })
        .collect();
    for path in evict_victims(files, cap) {
        let _ = std::fs::remove_file(&path);
    }
}

/// Pure eviction pick: everything beyond `cap`, oldest first.
pub(crate) fn evict_victims(mut files: Vec<(SystemTime, PathBuf)>, cap: usize) -> Vec<PathBuf> {
    if files.len() <= cap {
        return Vec::new();
    }
    files.sort_by_key(|(t, _)| *t);
    let n = files.len() - cap;
    files.truncate(n);
    files.into_iter().map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn admit_texture_caps_distinct_ids() {
        let mut set = HashSet::new();
        assert!(admit_texture(&mut set, "a", 2));
        assert!(admit_texture(&mut set, "b", 2));
        assert!(!admit_texture(&mut set, "c", 2), "over budget");
        assert!(admit_texture(&mut set, "a", 2), "known id stays creatable");
        assert!(admit_texture(&mut set, "b", 2));
    }

    #[test]
    fn evict_victims_drops_oldest_beyond_cap() {
        let t = |s: u64| UNIX_EPOCH + Duration::from_secs(s);
        let files = vec![
            (t(30), PathBuf::from("c")),
            (t(10), PathBuf::from("a")),
            (t(20), PathBuf::from("b")),
        ];
        assert_eq!(evict_victims(files.clone(), 2), vec![PathBuf::from("a")]);
        assert_eq!(
            evict_victims(files.clone(), 1),
            vec![PathBuf::from("a"), PathBuf::from("b")]
        );
        assert!(evict_victims(files, 3).is_empty());
        assert!(evict_victims(Vec::new(), 0).is_empty());
    }
}
