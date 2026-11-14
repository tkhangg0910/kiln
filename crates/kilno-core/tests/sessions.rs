//! checks/sessions.cs: a fake `projects` folder in a temp dir. Once the watcher runs, a scan
//! reads only the transcripts it reported, so these check that a new, a changed and a deleted
//! transcript still reach the sidebar. Plus the Claude transcript parsing itself.

use kilno_core::paths::{MapEnv, ReadOnlyDir};
use kilno_core::sessions::claude::ClaudeStore;
use kilno_core::sessions::watch::SessionSource;
use kilno_core::sessions::SessionInfo;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

fn user(id: &str, text: &str) -> String {
    format!(
        "{{\"type\":\"user\",\"cwd\":\"C:\\\\x\",\"sessionId\":\"{id}\",\"message\":{{\"role\":\"user\",\"content\":\"{text}\"}}}}\n"
    )
}

/// The watcher reports a moment after the write, so give it up to 5 s to show in a scan.
fn scan_until(store: &mut ClaudeStore, done: impl Fn(&[SessionInfo]) -> bool) -> Vec<SessionInfo> {
    let mut rows = store.scan();
    for _ in 0..50 {
        if done(&rows) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
        rows = store.scan();
    }
    rows
}

fn last_prompt(rows: &[SessionInfo], id: &str) -> Option<String> {
    rows.iter().find(|s| s.session_id == id).and_then(|s| s.last_prompt.clone())
}

#[test]
fn watcher_keeps_the_list_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let project = root.join("C--x");
    let path = |id: &str| project.join(format!("{id}.jsonl"));
    std::fs::create_dir_all(project.join("subagents")).unwrap();
    std::fs::write(path("a"), user("a", "first")).unwrap();
    std::fs::write(project.join("subagents").join("agent-1.jsonl"), user("agent-1", "tool")).unwrap();

    let mut store = ClaudeStore::new(ReadOnlyDir::new(root));
    assert_eq!(store.scan().len(), 1, "before watching: every file is read");
    assert!(!store.scan().iter().any(|s| s.session_id == "agent-1"), "subagent transcripts are not sessions");

    let fired = Arc::new(AtomicUsize::new(0));
    let f = fired.clone();
    store.start_watching(Box::new(move || {
        f.fetch_add(1, Ordering::SeqCst);
    }));
    assert!(store.is_watching());
    assert_eq!(store.scan().len(), 1, "first scan while watching");

    std::fs::write(path("b"), user("b", "second")).unwrap();
    assert_eq!(scan_until(&mut store, |r| r.len() == 2).len(), 2, "a new transcript appears");

    let mut f = std::fs::OpenOptions::new().append(true).open(path("a")).unwrap();
    std::io::Write::write_all(&mut f, b"{\"type\":\"last-prompt\",\"lastPrompt\":\"later\"}\n").unwrap();
    drop(f);
    let rows = scan_until(&mut store, |r| last_prompt(r, "a").as_deref() == Some("later"));
    assert_eq!(last_prompt(&rows, "a").as_deref(), Some("later"), "a changed transcript is read again");

    std::fs::remove_file(path("b")).unwrap();
    assert_eq!(scan_until(&mut store, |r| r.len() == 1).len(), 1, "a deleted transcript goes");

    std::fs::write(project.join("subagents").join("agent-2.jsonl"), user("agent-2", "tool")).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    assert_eq!(store.scan().len(), 1, "a new subagent transcript still is not a session");
    // The 600 ms debounce fires on a background thread; under load it can take longer than 1 s.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while fired.load(Ordering::SeqCst) == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(fired.load(Ordering::SeqCst) > 0, "changes are reported after the debounce");

    store.stop_watching();
    assert!(!store.is_watching());
}

#[test]
fn no_root_is_no_rows() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = ClaudeStore::new(ReadOnlyDir::new(dir.path().join("missing")));
    assert!(!store.root_exists());
    assert!(store.scan().is_empty());
    store.start_watching(Box::new(|| {}));
    assert!(!store.is_watching());
}

#[test]
fn root_honours_claude_config_dir() {
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("projects").join("C--y");
    std::fs::create_dir_all(&project).unwrap();
    std::fs::write(project.join("s.jsonl"), user("s", "hi")).unwrap();
    let env = MapEnv::default().with("CLAUDE_CONFIG_DIR", dir.path().to_string_lossy());
    assert_eq!(ClaudeStore::for_this_user(&env).scan().len(), 1);
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(name), text).unwrap();
}

#[test]
fn row_contents() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("C--work-api");
    let lines = [
        r#"{"type":"user","isMeta":true,"cwd":"C:\\work\\api","sessionId":"s1","message":{"content":"meta"}}"#,
        r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#,
        r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"x"},{"type":"text","text":"  Caveat: no"},{"type":"text","text":"  fix   the\nbug  "}]}}"#,
        r#"{"type":"ai-title","aiTitle":"Old title"}"#,
        r#"{"type":"ai-title","aiTitle":"Fix the bug"}"#,
        r#"{"type":"last-prompt","lastPrompt":"and the tests"}"#,
    ];
    write(&p, "s1.jsonl", &(lines.join("\n") + "\n"));
    // No cwd, no sessionId, no prompt: fall back to the folder and the file name.
    write(&p, "bare.jsonl", "{\"type\":\"system\"}\n");
    write(&p, "empty.jsonl", "");

    let rows = ClaudeStore::new(ReadOnlyDir::new(dir.path())).scan();
    assert_eq!(rows.len(), 2, "an empty transcript is no row");
    let s1 = rows.iter().find(|r| r.session_id == "s1").unwrap();
    assert_eq!(s1.project_dir, r"C:\work\api");
    assert_eq!(s1.preview, "fix the bug");
    assert_eq!(s1.ai_title.as_deref(), Some("Fix the bug"));
    assert_eq!(s1.last_prompt.as_deref(), Some("and the tests"));
    assert_eq!(s1.project_name(), "api");
    let bare = rows.iter().find(|r| r.session_id == "bare").unwrap();
    assert_eq!(bare.project_dir, "C--work-api");
    assert_eq!(bare.preview, "(no prompt yet)");
    assert_eq!(bare.ai_title, None);
}

#[test]
fn title_is_read_from_the_last_96_kib() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("C--big");
    let mut text = String::from("{\"type\":\"ai-title\",\"aiTitle\":\"too early\"}\n");
    let filler = format!("{{\"type\":\"assistant\",\"pad\":\"{}\"}}\n", "x".repeat(1000));
    for _ in 0..120 {
        text += &filler;
    }
    text += "{\"type\":\"last-prompt\",\"lastPrompt\":\"near the end\"}\n";
    write(&p, "big.jsonl", &text);
    let rows = ClaudeStore::new(ReadOnlyDir::new(dir.path())).scan();
    assert_eq!(rows[0].ai_title, None);
    assert_eq!(rows[0].last_prompt.as_deref(), Some("near the end"));
}
