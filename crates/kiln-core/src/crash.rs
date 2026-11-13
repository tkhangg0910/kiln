//! `crash.log` in Kiln's data folder: a fatal error leaves a record of why the window went
//! away (App.LogFatal). One older file is kept, so the log never grows past about 2 MB.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

pub const LOG: &str = "crash.log";
pub const OLD: &str = "crash.old.log";
/// Above this the log is moved to [`OLD`] (replacing it) before the next entry.
pub const MAX_LEN: u64 = 1024 * 1024;

/// One entry: `yyyy-MM-dd HH:mm:ss  <what>` and a blank line, with Windows line ends as
/// .NET's `Environment.NewLine` wrote them.
pub fn entry(at: NaiveDateTime, what: &str) -> String {
    format!("{}  {what}\r\n\r\n", at.format("%Y-%m-%d %H:%M:%S"))
}

/// Appends an entry to `<dir>\crash.log`, rotating first when the log is over [`MAX_LEN`].
/// Returns the log's path.
pub fn append(dir: &Path, at: NaiveDateTime, what: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let log = dir.join(LOG);
    if std::fs::metadata(&log).is_ok_and(|m| m.len() > MAX_LEN) {
        // On Windows `rename` replaces an existing target (MOVEFILE_REPLACE_EXISTING).
        let old = dir.join(OLD);
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&log, &old)?;
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&log)?;
    f.write_all(entry(at, what).as_bytes())?;
    Ok(log)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 1, 2).unwrap().and_hms_opt(3, 4, 5).unwrap()
    }

    #[test]
    fn entry_has_the_time_two_spaces_and_a_blank_line() {
        assert_eq!(entry(at(), "boom"), "2026-01-02 03:04:05  boom\r\n\r\n");
    }

    #[test]
    fn appends_and_creates_the_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("Kiln");
        append(&dir, at(), "one").unwrap();
        append(&dir, at(), "two").unwrap();
        let text = std::fs::read_to_string(dir.join(LOG)).unwrap();
        assert_eq!(text, format!("{}{}", entry(at(), "one"), entry(at(), "two")));
        assert!(!dir.join(OLD).exists());
    }

    #[test]
    fn a_log_over_one_mib_moves_to_the_old_file_replacing_it() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(OLD), "older").unwrap();
        let big = "x".repeat(MAX_LEN as usize + 1);
        std::fs::write(dir.join(LOG), &big).unwrap();
        append(dir, at(), "new").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(OLD)).unwrap(), big);
        assert_eq!(std::fs::read_to_string(dir.join(LOG)).unwrap(), entry(at(), "new"));
    }

    #[test]
    fn exactly_one_mib_is_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        std::fs::write(dir.join(LOG), "x".repeat(MAX_LEN as usize)).unwrap();
        append(dir, at(), "new").unwrap();
        assert!(!dir.join(OLD).exists());
    }
}
