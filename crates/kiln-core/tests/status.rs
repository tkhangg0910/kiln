//! checks/status.cs (+ the two "account: the newest report wins" checks of checks/codex.cs).

use chrono::{DateTime, Duration, TimeZone, Utc};
use kiln_core::git;
use kiln_core::status::parse::{effort, newest, parse, parse_usage_cache};
use kiln_core::status::store::StatusStore;
use kiln_core::status::{Limit, SessionStatus};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;

// Captured from Claude Code 2.1.288 through the relay, on start-up before any prompt, with the
// user's paths swapped for C:\work. So no rate_limits yet and current_usage is null.
const FRESH: &str = r#"{"session_id":"3f136615-e5ba-45b2-9766-6cc4d88ef5d9","transcript_path":"C:\\Users\\me\\.claude\\projects\\C--work\\3f136615-e5ba-45b2-9766-6cc4d88ef5d9.jsonl","cwd":"C:\\work","effort":{"level":"medium"},"model":{"id":"claude-opus-5-5[1m]","display_name":"Opus 5.5 (1M context)"},"workspace":{"current_dir":"C:\\work","project_dir":"C:\\work","added_dirs":[]},"version":"2.1.288","output_style":{"name":"default"},"cost":{"total_cost_usd":0,"total_duration_ms":902,"total_api_duration_ms":0,"total_lines_added":0,"total_lines_removed":0},"context_window":{"total_input_tokens":0,"total_output_tokens":0,"context_window_size":1000000,"current_usage":null,"used_percentage":null,"remaining_percentage":null},"exceeds_200k_tokens":false,"fast_mode":false,"thinking":{"enabled":true}}"#;

// The same session after a reply: current_usage and rate_limits filled in.
const BUSY: &str = r#"{"session_id":"3f136615-e5ba-45b2-9766-6cc4d88ef5d9","cwd":"C:\\work","effort":{"level":"xhigh"},"model":{"id":"claude-opus-5-5[1m]","display_name":"Opus 5.5 (1M context)"},"context_window":{"context_window_size":1000000,"current_usage":{"input_tokens":12,"output_tokens":900,"cache_creation_input_tokens":40000,"cache_read_input_tokens":300000}},"rate_limits":{"five_hour":{"used_percentage":42.7,"resets_at":1790000000},"seven_day":{"used_percentage":9,"resets_at":"2026-10-08T10:00:00Z"}}}"#;

const ID: &str = "3f136615-e5ba-45b2-9766-6cc4d88ef5d9";
const A: &str = "aaaaaaaa-0000-0000-0000-000000000000";
const B: &str = "bbbbbbbb-0000-0000-0000-000000000000";
const C: &str = "cccccccc-0000-0000-0000-000000000000";
const D: &str = "dddddddd-0000-0000-0000-000000000000";

// The user's statusline.ps1 cache: utilization rather than used_percentage, ISO reset times.
const CACHE: &str = r#"{
    "five_hour":  { "utilization":  2.0, "resets_at":  "2026-10-03T13:39:59.976874+00:00", "limit_dollars":  null },
    "seven_day":  { "utilization":  16.0, "resets_at":  "2026-10-07T01:59:59.976900+00:00" },
    "extra_usage": { "is_enabled": false }
}"#;

/// Today, not a fixed day: the store prunes files older than a week.
fn at() -> DateTime<Utc> {
    Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
}

fn unused() -> String {
    panic!("fallback read although the payload had an effort")
}

fn write(path: &Path, json: &str, when: DateTime<Utc>) -> PathBuf {
    std::fs::write(path, json).unwrap();
    set_mtime(path, when);
    path.to_path_buf()
}

fn set_mtime(path: &Path, when: DateTime<Utc>) {
    let f = std::fs::File::options().write(true).open(path).unwrap();
    f.set_modified(when.into()).unwrap();
}

#[test]
fn real_payload() {
    let s = parse(FRESH, at(), unused).unwrap();
    assert_eq!(s.session_id, ID, "session id");
    assert_eq!(s.model.as_deref(), Some("Opus 5.5 (1M context)"), "model");
    assert_eq!(s.cwd.as_deref(), Some(r"C:\work"), "cwd");
    assert_eq!(s.context_size, 1_000_000, "context size");
    assert_eq!(s.context_used, 0, "no usage yet is 0 used");
    assert_eq!(s.context_percent, 0, "and 0%");
    assert_eq!(s.effort, "medium", "effort from the payload");
    assert_eq!(s.five_hour, None, "missing rate_limits: no 5h");
    assert_eq!(s.seven_day, None, "missing rate_limits: no 7d");
    assert_eq!(s.updated, at(), "last update is the file's time");
}

#[test]
fn busy_payload() {
    let s = parse(BUSY, at(), unused).unwrap();
    assert_eq!(
        s.context_used, 340_012,
        "context used is input + cache creation + cache read"
    );
    assert_eq!(s.context_percent, 34, "context percent is floored");
    let h5 = s.five_hour.unwrap();
    assert_eq!(h5.percent, 42, "5h percent is floored");
    assert_eq!(
        h5.resets_at,
        Utc.timestamp_opt(1_790_000_000, 0).single(),
        "5h reset from epoch seconds"
    );
    let d7 = s.seven_day.unwrap();
    assert_eq!(d7.percent, 9, "7d percent");
    assert_eq!(
        d7.resets_at,
        Some(Utc.with_ymd_and_hms(2026, 10, 8, 10, 0, 0).unwrap()),
        "7d reset from an ISO time"
    );
}

#[test]
fn effort_order() {
    assert_eq!(
        effort(Some("high"), Some("low"), Some("max")),
        "high",
        "payload first"
    );
    assert_eq!(
        effort(None, Some("low"), Some("max")),
        "low",
        "then the environment"
    );
    assert_eq!(
        effort(None, Some(""), Some("max")),
        "max",
        "then settings.json"
    );
    assert_eq!(effort(None, None, None), "medium", "else medium");
    let s = parse(
        r#"{"session_id":"a1b2c3d4-0000-0000-0000-000000000000"}"#,
        at(),
        || "low".into(),
    );
    assert_eq!(
        s.unwrap().effort,
        "low",
        "payload without an effort uses the fallback"
    );
}

#[test]
fn bad_payloads_are_ignored() {
    assert_eq!(
        parse(&BUSY[..60], at(), unused),
        None,
        "half a file is ignored"
    );
    assert_eq!(parse("Claude", at(), unused), None, "not JSON is ignored");
    assert_eq!(parse("[1,2]", at(), unused), None, "an array is ignored");
    assert_eq!(
        parse(r#"{"cwd":"C:\\work"}"#, at(), unused),
        None,
        "no session id is ignored"
    );
    assert_eq!(
        parse(r#"{"session_id":"..\\..\\evil"}"#, at(), unused),
        None,
        "a session id that is not a guid is ignored"
    );
}

#[test]
fn store_over_a_folder() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let at = at();
    write(&d.join(format!("{A}.json")), &BUSY.replace(ID, A), at);
    write(
        &d.join(format!("{B}.json")),
        &BUSY.replace(ID, B).replace("42.7", "77"),
        at + Duration::minutes(5),
    );
    // Newest of all, but fresh: no limits yet, so it must not blank the account's.
    write(
        &d.join(format!("{C}.json")),
        &FRESH.replace(ID, C),
        at + Duration::minutes(9),
    );
    write(
        &d.join("broken.json"),
        &BUSY[..80],
        at + Duration::minutes(10),
    );

    // A usage cache that is not there: the real one must not leak into these checks.
    let store = StatusStore::new(d, || "medium".into(), d.join("no-usage-cache.json"));
    store.start().unwrap();
    assert_eq!(
        store.get(A).map(|s| s.context_percent),
        Some(34),
        "each session's file is read"
    );
    assert_eq!(
        store
            .get(&B.to_uppercase())
            .and_then(|s| s.five_hour)
            .map(|l| l.percent),
        Some(77),
        "lookup ignores case"
    );
    assert_eq!(
        five_hour(&store),
        Some(77),
        "the newest file with limits wins"
    );
    assert_eq!(store.get("broken"), None, "a bad file is skipped");

    // A rewrite of an older session makes it the newest.
    store.load(&write(
        &d.join(format!("{A}.json")),
        &BUSY.replace(ID, A).replace("42.7", "51"),
        at + Duration::minutes(20),
    ));
    assert_eq!(
        five_hour(&store),
        Some(51),
        "an updated file takes over the limits"
    );

    // A file cut short mid-write keeps what we had.
    store.load(&write(
        &d.join(format!("{B}.json")),
        &format!("{{\"session_id\":\"{B}\",\"cwd\":"),
        at + Duration::minutes(30),
    ));
    assert_eq!(
        store.get(B).and_then(|s| s.five_hour).map(|l| l.percent),
        Some(77),
        "partial JSON keeps the last good status"
    );

    // The watcher picks up a new file the way the relay writes it: a temp file renamed into place.
    let (tx, rx) = mpsc::channel();
    let tx = std::sync::Mutex::new(tx);
    store.on_change(move |s| {
        if s.session_id == D {
            let _ = tx.lock().unwrap().send(());
        }
    });
    let tmp = d.join("x.tmp");
    std::fs::write(&tmp, FRESH.replace(ID, D)).unwrap();
    std::fs::rename(&tmp, d.join(format!("{D}.json"))).unwrap();
    assert!(
        rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok(),
        "the watcher sees a renamed-in file"
    );
}

fn five_hour(store: &StatusStore) -> Option<i32> {
    store.limits().and_then(|l| l.five_hour).map(|l| l.percent)
}

#[test]
fn usage_cache() {
    let c = parse_usage_cache(CACHE, at()).unwrap();
    let h5 = c.five_hour.unwrap();
    assert_eq!(h5.percent, 2, "cache: 5h percent");
    let want =
        Utc.with_ymd_and_hms(2026, 10, 3, 13, 39, 59).unwrap() + Duration::microseconds(976_874);
    assert_eq!(h5.resets_at, Some(want), "cache: 5h reset");
    assert_eq!(
        c.seven_day.map(|l| l.percent),
        Some(16),
        "cache: 7d percent"
    );
    assert_eq!(c.updated, at(), "cache: updated is the file's time");
    assert_eq!(
        parse_usage_cache(r#"{"extra_usage":{}}"#, at()),
        None,
        "cache: no limits in it is none"
    );
    assert_eq!(
        parse_usage_cache("{\"five_hour\":", at()),
        None,
        "cache: bad JSON is none"
    );
}

#[test]
fn store_with_usage_cache() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let at = at();
    let cache_path = d.join("usage.json");
    let store = StatusStore::new(d, || "medium".into(), &cache_path);
    store.start().unwrap();
    assert_eq!(
        store.limits(),
        None,
        "no sessions and no cache is no limits"
    );

    write(&cache_path, CACHE, at);
    let seven_day = |s: &StatusStore| s.limits().and_then(|l| l.seven_day).map(|l| l.percent);
    assert_eq!(
        seven_day(&store),
        Some(16),
        "the cache fills in when Claude sends none"
    );

    // Claude's own report, newer than the cache, wins; an older one does not.
    let s = write(
        &d.join(format!("{A}.json")),
        &BUSY.replace(ID, A),
        at + Duration::minutes(1),
    );
    store.load(&s);
    assert_eq!(
        seven_day(&store),
        Some(9),
        "a newer report from Claude wins"
    );
    set_mtime(&cache_path, at + Duration::minutes(2));
    assert_eq!(seven_day(&store), Some(16), "a newer cache wins");
}

/// checks/codex.cs: account limits over Codex sessions come from the newest that has them.
#[test]
fn account_newest_report_wins() {
    let now = at();
    let status =
        |id: &str, h5: Option<Limit>, d7: Option<Limit>, updated: DateTime<Utc>| SessionStatus {
            session_id: id.into(),
            model: None,
            cwd: None,
            context_used: 0,
            context_size: 1,
            context_percent: 0,
            effort: String::new(),
            five_hour: h5,
            seven_day: d7,
            updated,
        };
    let reset = Utc.timestamp_opt(1_790_000_000, 0).single();
    let sessions = [
        status(
            "older",
            Some(Limit::new(70, reset)),
            Some(Limit::new(90, reset)),
            now - Duration::hours(2),
        ),
        status(
            "working",
            Some(Limit::new(12, reset)),
            Some(Limit::new(40, reset)),
            now - Duration::minutes(5),
        ),
        // Newest, but a 30-day window fits neither: it must not blank them.
        status("monthly", None, None, now),
    ];
    let account = newest(sessions.iter());
    assert_eq!(
        account
            .as_ref()
            .and_then(|a| a.five_hour.as_ref())
            .map(|l| l.percent),
        Some(12),
        "account: the newest report wins, not an older session's"
    );
    assert_eq!(
        account
            .as_ref()
            .and_then(|a| a.seven_day.as_ref())
            .map(|l| l.percent),
        Some(40),
        "account: a session without 5h/7d windows does not blank them"
    );
}

#[test]
fn numstat() {
    let n = "3\t1\ta.cs\n10\t0\tb.cs\n-\t-\tlogo.png\n";
    assert_eq!(git::parse_numstat(n).0, 13, "numstat added");
    assert_eq!(git::parse_numstat(n).1, 1, "numstat deleted");
    assert_eq!(git::parse_numstat(""), (0, 0), "numstat empty");
}

#[test]
fn git_outside_a_repo() {
    let dir = tempfile::tempdir().unwrap();
    // The system temp folder could sit inside a repo on some machine; check that first.
    if git::is_repo(&std::env::temp_dir()) {
        return;
    }
    assert!(!git::is_repo(dir.path()), "the temp folder is not a repo");
    assert_eq!(
        git::read(dir.path()),
        None,
        "a folder outside any repo reads nothing"
    );
}

#[test]
fn git_inside_a_repo() {
    let dir = tempfile::tempdir().unwrap();
    let d = dir.path();
    let git_cmd = |args: &[&str]| Command::new("git").arg("-C").arg(d).args(args).output();
    match git_cmd(&["init", "-q", "-b", "main"]) {
        Ok(o) if o.status.success() => {}
        _ => {
            eprintln!("git not available; skipping");
            return;
        }
    }
    assert!(git::is_repo(d), "a git init folder is a repo");
    assert!(
        git::is_repo(&d.join("sub").join("deeper")),
        "and so is any folder under it"
    );

    // An unborn branch: rev-parse fails, so nothing until the first commit.
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    for args in [
        &["add", "a.txt"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-q",
            "-m",
            "x",
        ][..],
    ] {
        assert!(git_cmd(args).unwrap().status.success());
    }
    std::fs::write(d.join("a.txt"), "one\nthree\nfour\n").unwrap();
    let s = git::read(d).expect("a repo reads");
    assert_eq!(s.branch, "main", "and has a branch");
    assert_eq!((s.added, s.deleted), (2, 1), "uncommitted lines");
}

#[test]
fn git_cache_reads_in_the_background() {
    let dir = tempfile::tempdir().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    let tx = std::sync::Mutex::new(tx);
    let cache = git::GitCache::new(git::INTERVAL, move |folder, _| {
        let _ = tx.lock().unwrap().send(folder.to_string());
    });
    let folder = dir.path().to_string_lossy().into_owned();
    cache
        .refresh(&folder)
        .expect("first refresh reads")
        .join()
        .unwrap();
    assert!(
        cache.refresh(&folder.to_uppercase()).is_none(),
        "throttled per folder, ignoring case"
    );
    assert_eq!(cache.get(&folder), None, "not a repo");
    assert!(
        rx.try_recv().is_err(),
        "unchanged (None to None) fires nothing"
    );
}

/// A writer whose reader went away (the user's command did not read).
struct BrokenPipe;

impl std::io::Write for BrokenPipe {
    fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
        Err(std::io::ErrorKind::BrokenPipe.into())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn relay_passes_and_saves() {
    use kiln_core::status::relay::run;
    let dir = tempfile::tempdir().unwrap();
    let status_dir = dir.path().join("status");
    let input = BUSY.replace(ID, &ID.to_uppercase());

    let mut out = Vec::new();
    assert_eq!(
        run(true, input.as_bytes(), &mut out, &status_dir),
        0,
        "always 0"
    );
    assert_eq!(out, input.as_bytes(), "--pass hands on the same bytes");
    let saved = status_dir.join(format!("{ID}.json"));
    assert_eq!(
        std::fs::read(&saved).unwrap(),
        input.as_bytes(),
        "saved raw, under the lowercase guid"
    );
    assert_eq!(
        std::fs::read_dir(&status_dir).unwrap().count(),
        1,
        "no temp file left"
    );

    let mut out = Vec::new();
    run(false, FRESH.as_bytes(), &mut out, &status_dir);
    assert!(out.is_empty(), "without --pass nothing is printed");
    assert_eq!(
        std::fs::read_to_string(&saved).unwrap(),
        FRESH,
        "and the file is replaced"
    );

    assert_eq!(
        run(true, BUSY.as_bytes(), BrokenPipe, &status_dir),
        0,
        "a broken pipe is not an error"
    );
    assert_eq!(
        std::fs::read_to_string(&saved).unwrap(),
        BUSY,
        "and it still saves"
    );

    let empty = dir.path().join("empty");
    for bad in [
        "",
        "Claude",
        "[1]",
        r#"{"session_id":3}"#,
        r#"{"session_id":"..\\..\\evil"}"#,
    ] {
        run(false, bad.as_bytes(), Vec::new(), &empty);
    }
    assert!(!empty.exists(), "nothing saved for anything odd");
}

#[test]
fn relay_settings_file() {
    use kiln_core::paths::{DataDir, ReadOnlyDir};
    use kiln_core::status::relay::{settings_path, write_settings};
    let dir = tempfile::tempdir().unwrap();
    let claude = dir.path().join("claude");
    std::fs::create_dir_all(&claude).unwrap();
    let theirs =
        r#"{"statusLine":{"type":"command","command":"bash ~/line.sh","padding":0},"model":"x"}"#;
    std::fs::write(claude.join("settings.json"), theirs).unwrap();
    let data = DataDir::new(dir.path().join("data"));

    let ask = vec!["Bash(git push*)".to_string()];
    let path = write_settings(
        &data,
        &ReadOnlyDir::new(&claude),
        r"C:\Kiln\kiln.exe",
        &ask,
    )
    .unwrap();
    assert_eq!(path, settings_path(&data));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        name,
        if cfg!(debug_assertions) {
            "statusline.dev.json"
        } else {
            "statusline.json"
        }
    );
    let v: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(
        v["statusLine"]["command"],
        "'C:/Kiln/kiln.exe' --statusline --pass | {\nbash ~/line.sh\n}"
    );
    assert_eq!(v["permissions"]["ask"][0], "Bash(git push*)");
    assert_eq!(
        std::fs::read_to_string(claude.join("settings.json")).unwrap(),
        theirs,
        "theirs is only read"
    );

    // Unchanged content is not rewritten.
    let old =
        std::fs::metadata(&path).unwrap().modified().unwrap() - std::time::Duration::from_secs(60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    write_settings(
        &data,
        &ReadOnlyDir::new(&claude),
        r"C:\Kiln\kiln.exe",
        &ask,
    )
    .unwrap();
    assert_eq!(
        std::fs::metadata(&path).unwrap().modified().unwrap(),
        old,
        "same content, not rewritten"
    );
}
