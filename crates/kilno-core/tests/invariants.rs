//! PORTING.md §3.1: Kilno only reads the Claude and Codex folders. Run everything that touches
//! them (scans, watching, transcript tails, the relay settings, rules listings, settings saves
//! with every option on) over temp copies and check not one byte, name or mtime changed.

use kilno_core::paths::{DataDir, ReadOnlyDir};
use kilno_core::security::{ask_rules, own_rules};
use kilno_core::sessions::claude::ClaudeStore;
use kilno_core::sessions::codex::CodexStore;
use kilno_core::sessions::tail::TranscriptTail;
use kilno_core::sessions::watch::SessionSource;
use kilno_core::settings::KilnoSettings;
use kilno_core::status::relay;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

fn snapshot(root: &Path) -> BTreeMap<PathBuf, (u64, SystemTime, Vec<u8>)> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).unwrap() {
            let e = e.unwrap();
            let md = e.metadata().unwrap();
            if md.is_dir() {
                stack.push(e.path());
                out.insert(e.path(), (0, SystemTime::UNIX_EPOCH, Vec::new()));
            } else {
                out.insert(e.path(), (md.len(), md.modified().unwrap(), fs::read(e.path()).unwrap()));
            }
        }
    }
    out
}

fn write(p: &Path, s: &str) {
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, s).unwrap();
}

#[test]
fn agent_folders_are_never_written() {
    let tmp = tempfile::tempdir().unwrap();
    let claude = tmp.path().join("claude-config");
    let codex = tmp.path().join("codex-home");
    let data = DataDir::new(tmp.path().join("kilno-data"));
    let work = tmp.path().join("work");
    fs::create_dir_all(&work).unwrap();

    let sid = "3f136615-e5ba-45b2-9766-6cc4d88ef5d9";
    let transcript = claude.join("projects").join("C--work").join(format!("{sid}.jsonl"));
    write(
        &transcript,
        &format!(
            "{{\"type\":\"user\",\"cwd\":\"{}\",\"sessionId\":\"{sid}\",\"timestamp\":\"2026-11-13T10:00:00Z\",\"message\":{{\"content\":\"hi\"}}}}\n{{\"type\":\"ai-title\",\"aiTitle\":\"Title\"}}\n",
            work.display().to_string().replace('\\', "\\\\")
        ),
    );
    write(
        &claude.join("settings.json"),
        r#"{"statusLine":{"type":"command","command":"pwsh line.ps1"},"permissions":{"allow":["Bash(git push*)"]}}"#,
    );
    write(
        &codex.join("sessions/2026/11/13/rollout-2026-11-13T10-00-00-a.jsonl"),
        "{\"timestamp\":\"2026-11-13T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"a\",\"timestamp\":\"2026-11-13T10:00:00Z\",\"cwd\":\"C:\\\\work\"}}\n",
    );
    write(&codex.join("session_index.jsonl"), "{\"id\":\"a\",\"thread_name\":\"Codex title\"}\n");
    write(&codex.join("rules/default.rules"), "prefix_rule(pattern=[\"git\", \"push\"], decision=\"allow\")\n");

    let before_claude = snapshot(&claude);
    let before_codex = snapshot(&codex);

    // Every option that ever wrote somewhere: ask rules on, Codex too.
    let s = KilnoSettings {
        ask_groups_on: "publishing".into(),
        also_for_codex: true,
        ..KilnoSettings::default()
    };
    s.save(&data.join("settings.json"));
    let _ = KilnoSettings::load(&data.join("settings.json"));

    let mut cs = ClaudeStore::new(ReadOnlyDir::new(claude.join("projects")));
    let mut xs = CodexStore::new(ReadOnlyDir::new(&codex));
    assert_eq!(cs.scan().len(), 1);
    assert_eq!(xs.scan().len(), 1);
    cs.start_watching(Box::new(|| {}));
    xs.start_watching(Box::new(|| {}));
    std::thread::sleep(Duration::from_millis(200));
    cs.stop_watching();
    xs.stop_watching();

    let mut tail = TranscriptTail::new(&work, chrono::Utc::now(), false);
    tail.read(&transcript);

    let path = relay::write_settings(&data, &ReadOnlyDir::new(&claude), "C:\\Kilno\\kilno.exe", &ask_rules::claude_ask(&s));
    assert!(path.is_some_and(|p| p.starts_with(data.path())));

    let _ = own_rules::claude(&claude.join("settings.json"), &s);
    let _ = own_rules::codex(&codex, &s);
    let _ = ask_rules::codex_rules(&s);
    let _ = ask_rules::codex_rules_file_still_active(&ReadOnlyDir::new(&codex), &s);

    assert_eq!(snapshot(&claude), before_claude, "the Claude folder changed");
    assert_eq!(snapshot(&codex), before_codex, "the Codex folder changed");
}
