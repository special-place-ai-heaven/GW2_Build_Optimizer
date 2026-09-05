//! Developer settings for `cargo run --example …` and the live-cache tests:
//! `dev.cfg` at the workspace root (copy `dev.cfg.example`). Machine-specific
//! paths never live in code. The one setting is the game's `addons` directory,
//! where the DLL is deployed; everything else is relative to it, exactly as
//! Nexus resolves the addon's own directory at runtime. The addon never reads
//! this file.
//!
//! Format: `key = value` lines; `#`, `;` and `[section]` lines are ignored.

use std::collections::HashMap;
use std::path::PathBuf;

/// Workspace root `dev.cfg`, resolved from this crate's manifest directory.
const FILE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../dev.cfg");

pub fn load() -> Result<HashMap<String, String>, String> {
    let text = std::fs::read_to_string(FILE).map_err(|e| {
        format!("{FILE}: {e}. Copy dev.cfg.example to dev.cfg and set addons_dir.")
    })?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with(['#', ';', '[']))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().trim_matches('"').to_string()))
        .collect())
}

/// `addons_dir`: the game's `addons` directory, home of
/// `gw2_build_optimizer.dll`.
pub fn addons_dir() -> Result<PathBuf, String> {
    load()?
        .get("addons_dir")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("{FILE}: missing addons_dir"))
}

/// The addon's cache directory, `<addons_dir>/<addon dir>/cache`, the same
/// layout Nexus gives the running DLL.
pub fn cache_dir() -> Result<PathBuf, String> {
    Ok(addons_dir()?
        .join(gw2_core::ADDON_DIR_NAME)
        .join("cache"))
}

/// `cache_dir`, or exit 2 with the reason on stderr. For examples.
pub fn cache_dir_or_exit() -> PathBuf {
    cache_dir().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2)
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn example_file_names_the_addons_dir() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../dev.cfg.example"
        ))
        .expect("dev.cfg.example is committed");
        let keys: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with(['#', ';', '[']))
            .filter_map(|l| l.split_once('=').map(|(k, _)| k.trim()))
            .collect();
        assert_eq!(keys, ["addons_dir"]);
    }
}
