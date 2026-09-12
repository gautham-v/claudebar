//! The last good limits snapshot, kept on disk.
//!
//! The usage endpoint rate-limits, and a fresh launch used to start with an
//! empty ring until its first fetch came back — or, after a 429, with nothing
//! at all. Writing each successful [`Usage`] to a small JSON file means the
//! menu bar and the popover show the previous numbers the moment the app
//! starts, and the first fetch merely refreshes them. The file holds nothing
//! secret: percentages and reset times, never the token.

use std::fs;
use std::path::{Path, PathBuf};

use crate::model::Usage;

/// `~/Library/Caches/claudebar/usage.json`.
pub fn path() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("claudebar").join("usage.json"))
}

/// The cached snapshot, if there is one and it still parses. A missing or
/// unreadable file is simply "no cache": the fetch that follows is what
/// matters, and nothing here should ever stop the app starting.
pub fn load() -> Option<Usage> {
    load_from(&path()?)
}

/// Write the snapshot. Errors are logged and otherwise ignored for the same
/// reason: the cache is a convenience, not the source of truth.
pub fn save(usage: &Usage) {
    let Some(path) = path() else {
        return;
    };
    if let Err(error) = save_to(&path, usage) {
        eprintln!("claudebar: could not write {}: {error}", path.display());
    }
}

pub fn load_from(path: &Path) -> Option<Usage> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn save_to(path: &Path, usage: &Usage) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = serde_json::to_string_pretty(usage).map_err(std::io::Error::other)?;
    fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::provider::fixture_usage;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "claudebar-cache-test-{}-{name}",
                std::process::id()
            ))
            .join("usage.json")
    }

    #[test]
    fn a_saved_snapshot_reads_back_the_same() {
        let path = temp_path("roundtrip");
        let usage = fixture_usage();
        save_to(&path, &usage).unwrap();
        let back = load_from(&path).expect("the cache parses");
        assert_eq!(back.limits, usage.limits);
        assert_eq!(back.plan, usage.plan);
        assert_eq!(back.fetched_at.timestamp(), usage.fetched_at.timestamp());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn a_missing_or_broken_file_is_no_cache() {
        let path = temp_path("broken");
        assert!(load_from(&path).is_none());
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "not json").unwrap();
        assert!(load_from(&path).is_none());
        let _ = fs::remove_dir_all(path.parent().unwrap());
    }
}
