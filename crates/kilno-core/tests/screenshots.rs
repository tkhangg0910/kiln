//! Port of checks/shots.cs: screenshot names and the week they are kept. A temp folder only.

use std::fs::{self, File};
use std::path::Path;
use std::time::{Duration, SystemTime};

use chrono::NaiveDate;
use kilno_core::screenshots::{new_path, prune, KEEP_FOR};

fn name(p: &Path) -> String {
    p.file_name().unwrap().to_string_lossy().into_owned()
}

fn set_mtime(p: &Path, t: SystemTime) {
    File::options()
        .write(true)
        .open(p)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

#[test]
fn named_by_the_second_with_a_counter() {
    let dir = tempfile::tempdir().unwrap();
    let at = NaiveDate::from_ymd_opt(2026, 10, 6)
        .unwrap()
        .and_hms_opt(19, 2, 11)
        .unwrap();
    let first = new_path(dir.path(), at);
    assert_eq!(name(&first), "2026-10-06_19-02-11.png");
    fs::write(&first, "").unwrap();
    let second = new_path(dir.path(), at);
    assert_eq!(name(&second), "2026-10-06_19-02-11-2.png");
    fs::write(&second, "").unwrap();
    assert_eq!(name(&new_path(dir.path(), at)), "2026-10-06_19-02-11-3.png");
}

#[test]
fn prune_deletes_screenshots_older_than_a_week() {
    let dir = tempfile::tempdir().unwrap();
    let now = SystemTime::now();
    let day = Duration::from_secs(86_400);
    let first = dir.path().join("2026-10-06_19-02-11.png");
    let second = dir.path().join("2026-10-06_19-02-11-2.png");
    let notes = dir.path().join("notes.txt");
    for p in [&first, &second, &notes] {
        fs::write(p, "").unwrap();
    }
    set_mtime(&first, now - KEEP_FOR - Duration::from_secs(3600));
    set_mtime(&second, now - day);
    set_mtime(&notes, now - day * 400);

    prune(dir.path(), now - KEEP_FOR);
    assert!(!first.exists(), "older than a week: deleted");
    assert!(second.exists(), "from yesterday: kept");
    assert!(notes.exists(), "not a screenshot: left alone");
}

#[test]
fn prune_with_no_folder_does_nothing() {
    let dir = tempfile::tempdir().unwrap();
    prune(&dir.path().join("none"), SystemTime::now());
}
