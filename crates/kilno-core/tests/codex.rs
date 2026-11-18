//! checks/codex.cs: a fake CODEX_HOME in a temp folder. The lines are cut from real codex-cli
//! 0.160.0 transcripts (2026-10-02), with base_instructions and other bulk left out. The two
//! "account: newest report wins" cases belong to the status store's tests.

use chrono::{DateTime, Utc};
use kilno_core::agent::AgentKind;
use kilno_core::paths::{MapEnv, ReadOnlyDir};
use kilno_core::sessions::codex::{context, find_started, CodexStore};
use kilno_core::sessions::watch::SessionSource;
use kilno_core::sessions::{SessionInfo, TurnState};
use kilno_core::status::Limit;
use kilno_core::time::{from_unix, parse_iso};
use std::collections::{HashMap, HashSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};

const A: &str = "01a0fd44-440a-7fb1-ba60-9ebb7a28ae46";
const B: &str = "01a0fd49-f4bd-7d51-a1f3-305c520b8ad1";
const FORK: &str = "01a0fd5a-4ae7-7e12-9be4-5cd951206df2";
const NO_INDEX: &str = "01a0fd5f-5ca5-7150-aac4-1f57a2afb465";
const WORKING: &str = "01a0fd70-0000-7000-8000-000000000001";
const MONTHLY: &str = "01a0fd70-0000-7000-8000-000000000002";
const OLDER: &str = "01a0fd70-0000-7000-8000-000000000003";

fn t(iso: &str) -> DateTime<Utc> {
    parse_iso(iso).unwrap()
}

fn meta(id: &str, cwd: &str, started: &str, parent: Option<&str>) -> String {
    let fork = parent
        .map(|p| format!("\"forked_from_id\":\"{p}\",\"forked_from_ordinal_exclusive\":67,"))
        .unwrap_or_default();
    format!(
        "{{\"timestamp\":\"{started}\",\"ordinal\":0,\"type\":\"session_meta\",\"payload\":{{\"session_id\":\"{id}\",\"id\":\"{id}\",{fork}\"timestamp\":\"{started}\",\"cwd\":\"{}\",\"originator\":\"codex-tui\",\"cli_version\":\"0.160.0\",\"base_instructions\":{{\"text\":\"You are Codex...\"}}}}}}",
        cwd.replace('\\', "\\\\")
    )
}

fn user(at: &str, text: &str) -> String {
    format!(
        "{{\"timestamp\":\"{at}\",\"ordinal\":8,\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"{text}\"}}]}}}}"
    )
}

fn context_msg(at: &str, cwd: &str) -> String {
    user(
        at,
        &format!(
            "<environment_context>\\n  <cwd>{}</cwd>\\n  <shell>powershell</shell>\\n</environment_context>",
            cwd.replace('\\', r"\\\\")
        ),
    )
}

fn event(at: &str, kind: &str) -> String {
    format!("{{\"timestamp\":\"{at}\",\"ordinal\":20,\"type\":\"event_msg\",\"payload\":{{\"type\":\"{kind}\"}}}}")
}

fn turn_context(at: &str, model: &str, effort: Option<&str>) -> String {
    let effort = effort.map(|e| format!("\"{e}\"")).unwrap_or_else(|| "null".into());
    format!(
        "{{\"timestamp\":\"{at}\",\"type\":\"turn_context\",\"payload\":{{\"model\":\"{model}\",\"collaboration_mode\":{{\"mode\":\"default\",\"settings\":{{\"model\":\"{model}\",\"reasoning_effort\":{effort}}}}}}}}}"
    )
}

fn token_count(at: &str, total: i64, window: i64, limits: &str) -> String {
    format!(
        "{{\"timestamp\":\"{at}\",\"type\":\"event_msg\",\"payload\":{{\"type\":\"token_count\",\"info\":{{\"last_token_usage\":{{\"input_tokens\":{},\"output_tokens\":268,\"total_tokens\":{total}}},\"model_context_window\":{window}}},\"rate_limits\":{{\"limit_id\":\"codex\",{limits}}}}}}}",
        total - 268
    )
}

fn window(percent: i32, minutes: i32, resets: i64) -> String {
    format!("{{\"used_percent\":{percent}.5,\"window_minutes\":{minutes},\"resets_at\":{resets}}}")
}

struct Home {
    _dir: tempfile::TempDir,
    home: PathBuf,
    day: PathBuf,
}

impl Home {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("codex");
        let day = home.join("sessions").join("2026").join("10").join("02");
        Self { _dir: dir, home, day }
    }

    fn write(&self, name: &str, lines: &[String]) -> PathBuf {
        std::fs::create_dir_all(&self.day).unwrap();
        let path = self.day.join(name);
        let text = if lines.is_empty() { String::new() } else { lines.join("\n") + "\n" };
        std::fs::write(&path, text).unwrap();
        path
    }

    fn store(&self) -> CodexStore {
        CodexStore::new(ReadOnlyDir::new(&self.home))
    }
}

fn append(path: &Path, text: &str) {
    std::fs::OpenOptions::new().append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
}

fn set_mtime(path: &Path, at: DateTime<Utc>) {
    std::fs::File::options().write(true).open(path).unwrap().set_modified(at.into()).unwrap();
}

fn by_id(rows: Vec<SessionInfo>) -> HashMap<String, SessionInfo> {
    rows.into_iter().map(|s| (s.session_id.clone(), s)).collect()
}

/// The four sessions of the C# check, plus the non-sessions beside them.
fn populate(h: &Home) -> PathBuf {
    // A plain session.
    h.write(
        &format!("rollout-2026-10-02T17-38-24-{A}.jsonl"),
        &[
            meta(A, r"C:\Yogi\Kilno", "2026-10-02T15:38:24.825Z", None),
            event("2026-10-02T15:38:54.300Z", "task_started"),
            context_msg("2026-10-02T15:38:54.310Z", r"C:\Yogi\Kilno"),
            user("2026-10-02T15:38:54.320Z", "hai are you ready"),
            event("2026-10-02T15:40:01.565Z", "task_complete"),
        ],
    );
    // Resumed later: the file grew, its time stayed old.
    let resumed = h.write(
        &format!("rollout-2026-10-02T17-44-37-{B}.jsonl"),
        &[
            meta(B, r"C:\Users\you", "2026-10-02T15:44:37.570Z", None),
            context_msg("2026-10-02T15:44:45.990Z", r"C:\Users\you"),
            user("2026-10-02T15:44:46.000Z", "checking what we can do"),
            event("2026-10-02T15:44:49.000Z", "task_complete"),
            user("2026-10-02T16:11:40.000Z", "just testing resume, reply with ok"),
            event("2026-10-02T16:11:45.384Z", "task_complete"),
        ],
    );
    set_mtime(&resumed, t("2026-10-02T15:44:49.000Z"));
    // A fork: its own file and id, the parent named in session_meta, no copied history.
    h.write(
        &format!("rollout-2026-10-02T18-02-28-{FORK}.jsonl"),
        &[
            meta(FORK, r"C:\Yogi\Kilno", "2026-10-02T16:02:28.375Z", Some(A)),
            user("2026-10-02T16:02:30.000Z", "so this is the fork?"),
            event("2026-10-02T16:02:44.792Z", "task_complete"),
        ],
    );
    // Not in session_index yet.
    h.write(
        &format!("rollout-2026-10-02T18-08-00-{NO_INDEX}.jsonl"),
        &[
            meta(NO_INDEX, r"C:\Temp\codex-approval", "2026-10-02T16:08:00.429Z", None),
            user("2026-10-02T16:08:20.000Z", "Create a file called hello.txt here containing the word hi."),
            event("2026-10-02T16:08:34.924Z", "turn_aborted"),
        ],
    );
    // Not sessions.
    h.write("other.jsonl", &[meta("nope", r"C:\x", "2026-10-02T10:00:00Z", None)]);
    h.write("rollout-broken.jsonl", &["{not json".to_string()]);
    h.write("rollout-empty.jsonl", &[]);

    let index = [
        format!(r#"{{"id":"{A}","thread_name":"Confirm readiness","updated_at":"2026-10-02T15:38:57Z"}}"#),
        format!(r#"{{"id":"{B}","thread_name":"Check options","updated_at":"2026-10-02T15:44:49Z"}}"#),
        format!(r#"{{"id":"{FORK}","thread_name":"Confirm readiness","updated_at":"2026-10-02T16:02:28Z"}}"#),
        format!(r#"{{"id":"{B}","thread_name":"Check available options","updated_at":"2026-10-02T15:50:00Z"}}"#),
    ];
    std::fs::write(h.home.join("session_index.jsonl"), index.join("\n") + "\n").unwrap();
    resumed
}

#[test]
fn no_sessions_folder() {
    let h = Home::new();
    let mut store = h.store();
    assert_eq!((store.root_exists(), store.scan().len()), (false, 0), "no sessions folder: nothing");
}

#[test]
fn rows() {
    let h = Home::new();
    let resumed = populate(&h);
    let mut store = h.store();
    let rows = by_id(store.scan());
    assert_eq!(rows.len(), 4, "rollout files only");
    assert_eq!(store.scan()[0].session_id, B, "newest first");

    let plain = &rows[A];
    assert_eq!(plain.agent, AgentKind::Codex, "plain: codex");
    assert_eq!(plain.project_dir, r"C:\Yogi\Kilno", "plain: folder from session_meta");
    assert_eq!(plain.ai_title.as_deref(), Some("Confirm readiness"), "plain: title from session_index");
    assert_eq!(plain.preview, "hai are you ready", "plain: preview skips Codex's own context");
    assert_eq!(plain.last_activity, t("2026-10-02T15:40:01.565Z"), "plain: last active is the last line");
    assert_eq!(plain.parent_id, None, "plain: no parent");

    assert_eq!(rows[B].last_activity, t("2026-10-02T16:11:45.384Z"), "resumed: last line beats the file time");
    assert_eq!(rows[B].last_prompt.as_deref(), Some("just testing resume, reply with ok"), "resumed: last prompt");
    assert_eq!(rows[B].ai_title.as_deref(), Some("Check available options"), "resumed: a later rename wins");

    assert_eq!(rows[FORK].parent_id.as_deref(), Some(A), "fork: parent from session_meta");
    assert_eq!(rows[FORK].preview, "so this is the fork?", "fork: its own first prompt");
    assert_eq!(rows[FORK].ai_title.as_deref(), Some("Confirm readiness"), "fork: the parent's title, as Codex gives it");

    assert_eq!(rows[NO_INDEX].ai_title, None, "no index entry: no title");
    assert_eq!(
        rows[NO_INDEX].preview,
        "Create a file called hello.txt here containing the word hi.",
        "no index entry: the prompt to fall back on"
    );

    // Grows while Kilno watches: the same length means unchanged, a new length is read again.
    append(
        &resumed,
        &format!("{}\n{}\n", user("2026-10-02T17:00:00.000Z", "one more"), event("2026-10-02T17:00:05.000Z", "task_complete")),
    );
    set_mtime(&resumed, t("2026-10-02T15:44:49.000Z"));
    let again = by_id(store.scan());
    assert_eq!(again[B].last_prompt.as_deref(), Some("one more"), "grown: new last prompt");
    assert_eq!(again[B].last_activity, t("2026-10-02T17:00:05.000Z"), "grown: new last active");

    // A line half written: the one before it counts.
    append(&resumed, r#"{"timestamp":"2026-10-02T18:00:00.000Z","type":"event_ms"#);
    assert_eq!(by_id(store.scan())[B].last_activity, t("2026-10-02T17:00:05.000Z"), "half-written last line ignored");

    assert_eq!(rows[A].started, t("2026-10-02T15:38:24.825Z"), "started: from session_meta");
    assert_eq!(rows[A].turn, Some(TurnState::Done), "turn: complete is done");
    assert_eq!(rows[NO_INDEX].turn, Some(TurnState::Stopped), "turn: aborted is stopped");
    assert_eq!(rows[A].status, None, "no token_count, no model: nothing for the strip");
}

/// Binding a pane Kilno started to the transcript Codex then writes.
// Folders match case-insensitively, a trailing backslash ignored: Windows only.
#[cfg(windows)]
#[test]
fn find_started_binds_panes() {
    let h = Home::new();
    populate(&h);
    let all = h.store().scan();
    let none = HashSet::new();
    let find = |folder: &str, parent: Option<&str>, since: &str, taken: &HashSet<String>, until: Option<&str>| {
        find_started(&all, folder, parent, t(since), taken, until.map(t)).map(|s| s.session_id.clone())
    };
    assert_eq!(
        find(r"C:\Yogi\Kilno\", None, "2026-10-02T15:30:00Z", &none, None).as_deref(),
        Some(A),
        "new pane: the session started in its folder after it"
    );
    assert_eq!(find(r"C:\Yogi\Kilno", None, "2026-10-02T15:39:00Z", &none, None), None, "new pane: not one started before it");
    let taken: HashSet<String> = [A.to_string()].into();
    assert_eq!(find(r"c:\yogi\kilno", None, "2026-10-02T15:30:00Z", &taken, None), None, "new pane: not one another pane has");
    assert_eq!(find(r"C:\Yogi\Kilno", None, "2026-10-02T16:00:00Z", &none, None), None, "new pane: not a fork in the same folder");
    assert_eq!(find(r"C:\Other", None, "2026-10-02T15:30:00Z", &none, None), None, "new pane: not another folder's");
    assert_eq!(
        find(r"C:\Elsewhere", Some(A), "2026-10-02T16:00:00Z", &none, None).as_deref(),
        Some(FORK),
        "fork pane: the session naming its parent"
    );
    // Two waiting panes in one folder: the older one must not take the newer one's session.
    assert_eq!(
        find(r"C:\Yogi\Kilno", None, "2026-10-02T15:30:00Z", &none, Some("2026-10-02T15:38:00Z")),
        None,
        "new pane: not a session started after the next pane opened"
    );
    assert_eq!(
        find(r"C:\Yogi\Kilno", None, "2026-10-02T15:38:00Z", &none, Some("2026-10-02T15:48:00Z")).as_deref(),
        Some(A),
        "new pane: the newer pane gets it"
    );
    assert_eq!(find(r"C:\Yogi\Kilno", Some(B), "2026-10-02T16:00:00Z", &none, None), None, "fork pane: nothing forked from another");
}

/// The turn's state, and the strip's numbers from turn_context and token_count.
#[test]
fn strip_from_the_transcript() {
    let h = Home::new();
    h.write(
        &format!("rollout-2026-10-02T19-00-00-{WORKING}.jsonl"),
        &[
            meta(WORKING, r"C:\Yogi\Kilno", "2026-10-02T17:00:00.000Z", None),
            turn_context("2026-10-02T17:00:01.000Z", "gpt-6-luna", None),
            event("2026-10-02T17:00:02.000Z", "task_started"),
            user("2026-10-02T17:00:02.100Z", "write the tests"),
            token_count(
                "2026-10-02T17:00:05.000Z",
                46800,
                258400,
                &format!("\"primary\":{},\"secondary\":{}", window(12, 300, 1790000000), window(40, 10080, 1790500000)),
            ),
            turn_context("2026-10-02T17:00:06.000Z", "gpt-6-luna", Some("high")),
            event("2026-10-02T17:00:07.000Z", "task_complete"),
            event("2026-10-02T17:00:08.000Z", "task_started"),
        ],
    );
    h.write(
        &format!("rollout-2026-10-02T19-10-00-{MONTHLY}.jsonl"),
        &[
            meta(MONTHLY, r"C:\Yogi\Kilno", "2026-10-02T17:10:00.000Z", None),
            token_count(
                "2026-10-02T17:10:05.000Z",
                1000,
                258400,
                &format!("\"primary\":{},\"secondary\":null", window(3, 43200, 1793547537)),
            ),
            event("2026-10-02T17:10:06.000Z", "task_complete"),
        ],
    );
    h.write(
        &format!("rollout-2026-10-02T18-00-00-{OLDER}.jsonl"),
        &[
            meta(OLDER, r"C:\Yogi\Kilno", "2026-10-02T16:00:00.000Z", None),
            token_count(
                "2026-10-02T16:00:05.000Z",
                1000,
                258400,
                &format!("\"primary\":{},\"secondary\":{}", window(80, 300, 1789990000), window(70, 10080, 1790500000)),
            ),
            event("2026-10-02T16:00:06.000Z", "task_complete"),
        ],
    );

    let now = by_id(h.store().scan());
    let st = now[WORKING].status.clone().unwrap();
    assert_eq!(now[WORKING].turn, Some(TurnState::Working), "turn: started after the last complete is working");
    assert_eq!(st.model.as_deref(), Some("gpt-6-luna"), "strip: model from the latest turn_context");
    assert_eq!(st.effort, "high", "strip: effort from the latest turn_context");
    // As Codex shows it: 12k baseline off both sides, 86% left (codex-rs TokenUsage).
    assert_eq!((st.context_used, st.context_size, st.context_percent), (34800, 246400, 14), "strip: context as Codex shows it");
    assert_eq!(context(49323, 258400), (37323, 246400, 15), "strip: a real session, Codex said 85% left");
    assert_eq!(context(11008, 258400), (0, 246400, 0), "strip: just after the first prompt, Codex says 100% left");
    assert_eq!(st.five_hour, Some(Limit::new(12, from_unix(1790000000))), "strip: a 5-hour window, floored");
    assert_eq!(st.seven_day, Some(Limit::new(40, from_unix(1790500000))), "strip: a 7-day window");
    assert_eq!(st.cwd.as_deref(), Some(r"C:\Yogi\Kilno"), "strip: folder");
    assert_eq!(st.updated, t("2026-10-02T17:00:08.000Z"));
    let mo = now[MONTHLY].status.clone().unwrap();
    assert_eq!((mo.five_hour, mo.seven_day), (None, None), "strip: a 30-day window fits neither");
    assert_eq!(mo.effort, "", "strip: no turn_context, no effort");
    assert_eq!(context(500, 12000), (500, 12000, 100));
    assert_eq!(context(0, 0), (0, 0, 0));
}

#[test]
fn home_from_env() {
    let h = Home::new();
    populate(&h);
    let env = MapEnv::default().with("CODEX_HOME", h.home.to_string_lossy());
    assert_eq!(CodexStore::for_this_user(&env).scan().len(), 4);
}

#[test]
fn watcher_sees_a_new_session_and_a_rename() {
    let h = Home::new();
    populate(&h);
    let mut store = h.store();
    store.start_watching(Box::new(|| {}));
    assert!(store.is_watching());
    assert_eq!(store.scan().len(), 4, "first scan while watching reads everything");

    let id = "01a0fd99-0000-7000-8000-000000000009";
    h.write(&format!("rollout-2026-10-02T20-00-00-{id}.jsonl"), &[meta(id, r"C:\n", "2026-10-02T18:00:00Z", None)]);
    append(&h.home.join("session_index.jsonl"), &format!("{{\"id\":\"{A}\",\"thread_name\":\"Renamed\"}}\n"));
    let mut rows = store.scan();
    for _ in 0..50 {
        if rows.len() == 5 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
        rows = store.scan();
    }
    assert_eq!(rows.len(), 5, "a new session appears");
    assert_eq!(by_id(rows)[A].ai_title.as_deref(), Some("Renamed"), "a rename shows on the next scan");
}
