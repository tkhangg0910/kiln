//! A screenshot on its way to a session: the clipboard's image, written as a PNG the agent can
//! be handed by path. Kept a week, like the status files. Reading the clipboard is Win32 and
//! lives in the island crate; this is the naming and the pruning.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::NaiveDateTime;

use crate::paths::DataDir;

/// How long a screenshot is kept.
pub const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// `<data>\shots`.
pub fn dir(data: &DataDir) -> PathBuf {
    data.join("shots")
}

/// `yyyy-MM-dd_HH-mm-ss.png` in local time, with -2, -3 when that second already has one.
pub fn new_path(dir: &Path, now: NaiveDateTime) -> PathBuf {
    let stem = now.format("%Y-%m-%d_%H-%M-%S").to_string();
    let mut path = dir.join(format!("{stem}.png"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{stem}-{n}.png"));
        n += 1;
    }
    path
}

/// Deletes the screenshots (`*.png`) written before `before`. A missing folder, or a file that
/// can't be read or deleted, is left as it is.
pub fn prune(dir: &Path, before: SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let is_png = path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("png"));
        if !is_png {
            continue;
        }
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < before);
        if old && entry.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(&path);
        }
    }
}
