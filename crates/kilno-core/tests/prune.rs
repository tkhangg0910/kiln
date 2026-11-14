//! checks/prune.cs: old status files go, recent ones stay. A temp folder, never the real one.

use chrono::{Duration, Utc};
use kilno_core::status::store::{prune, KEEP_FOR};

#[test]
fn prune_status_files() {
    let dir = tempfile::tempdir().unwrap();
    let now = Utc::now();
    let write = |name: &str, at: chrono::DateTime<Utc>| {
        let path = dir.path().join(name);
        std::fs::write(&path, "{}").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(at.into())
            .unwrap();
        path
    };
    let keep_for = Duration::from_std(KEEP_FOR).unwrap();
    let old = write("old.json", now - keep_for - Duration::hours(1));
    let recent = write("recent.json", now - Duration::days(1));
    let other = write("notes.txt", now - Duration::days(400));

    prune(dir.path(), now - keep_for);
    assert!(!old.exists(), "older than a week: deleted");
    assert!(recent.exists(), "from yesterday: kept");
    assert!(other.exists(), "not a status file: left alone");
}
