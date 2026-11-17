//! Claude Code's hooks report a pane's turn exactly: a prompt sent (`UserPromptSubmit`), the
//! turn over (`Stop`), a permission asked (`Notification`). The `--settings` file Kilno passes
//! each Claude pane runs `kilno --hook <event>` on them ([`super::relay::settings_json`]); that
//! writes `<data dir>\turns\<session>.txt` with the event, which the app follows. Without them
//! (hooks switched off, an older Claude Code) the pane's status is read from its output as before.

use crate::pane_status::Hook;
use crate::paths::DataDir;
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

/// `kilno --hook <event>`.
pub const ARG: &str = "--hook";

/// `<data dir>\turns`, one `<guid>.txt` per session holding its last event.
pub fn turns_dir(data: &DataDir) -> PathBuf {
    data.join("turns")
}

/// The word on the command line and in the file for each event.
pub fn name(h: Hook) -> &'static str {
    match h {
        Hook::Working => "working",
        Hook::Done => "done",
        Hook::Ask => "ask",
    }
}

pub fn parse_name(s: &str) -> Option<Hook> {
    match s.trim() {
        "working" => Some(Hook::Working),
        "done" => Some(Hook::Done),
        "ask" => Some(Hook::Ask),
        _ => None,
    }
}

/// `kilno --hook <event>`: reads the hook's JSON from stdin and saves the event for its
/// session. Prints nothing (a UserPromptSubmit hook's output would reach Claude) and always
/// exits 0, so it can never block or break a turn.
pub fn run(event: &str, mut stdin: impl Read, dir: &Path) -> i32 {
    let mut bytes = Vec::new();
    let _ = stdin.read_to_end(&mut bytes);
    if let Some(h) = parse_name(event) {
        if let Some(id) = session_of(&bytes, h) {
            save(dir, &id, h);
        }
    }
    0
}

/// The payload's session id (as a file name), or None when it has none or, for a notification,
/// it is not a question: "Claude is waiting for your input" after a minute idle is not one.
fn session_of(json: &[u8], h: Hook) -> Option<String> {
    let body = json.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(json);
    let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(body) else {
        return None;
    };
    if h == Hook::Ask {
        let kind = root.get("notification_type").and_then(Value::as_str);
        let message = root.get("message").and_then(Value::as_str).unwrap_or_default();
        let asks = match kind {
            Some(k) => matches!(k, "permission_prompt" | "elicitation_dialog"),
            None => message.to_lowercase().contains("permission"),
        };
        if !asks {
            return None;
        }
    }
    let id = root.get("session_id").and_then(Value::as_str).and_then(super::parse::dotnet_guid)?;
    Some(id.hyphenated().to_string())
}

/// Written whole and renamed into place, like the status files.
fn save(dir: &Path, id: &str, h: Hook) {
    let path = dir.join(format!("{id}.txt"));
    let tmp = dir.join(format!("{id}.{}.tmp", std::process::id()));
    let ok = std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&tmp, name(h)))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if ok.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// A turns file: its session id and event. None for anything else in the folder.
pub fn read(path: &Path) -> Option<(String, Hook)> {
    if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt")) {
        return None;
    }
    let id = path.file_stem()?.to_str()?;
    super::parse::dotnet_guid(id)?;
    let h = parse_name(&std::fs::read_to_string(path).ok()?)?;
    Some((id.to_lowercase(), h))
}

/// Follows `dir` (created if missing; files older than a day pruned first) and calls `f` with each
/// event as it is written, on the watcher's thread. None when the folder cannot be followed:
/// panes then go on reading their status from the output alone. Keep the returned handle: dropping
/// it stops the watching.
pub fn watch(dir: &Path, f: impl Fn(String, Hook) + Send + 'static) -> Option<Box<dyn std::any::Any + Send>> {
    use notify::{EventKind, RecursiveMode, Watcher};
    std::fs::create_dir_all(dir).ok()?;
    if let Ok(entries) = std::fs::read_dir(dir) {
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(24 * 60 * 60);
        for e in entries.flatten() {
            let stale = e.metadata().and_then(|m| m.modified()).is_ok_and(|t| t < old);
            if stale {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if matches!(event.kind, EventKind::Remove(_) | EventKind::Access(_)) {
            return;
        }
        for p in &event.paths {
            if let Some((id, h)) = read(p) {
                f(id, h);
            }
        }
    })
    .ok()?;
    w.watch(dir, RecursiveMode::NonRecursive).ok()?;
    Some(Box::new(w))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "3f136615-e5ba-45b2-9766-6cc4d88ef5d9";

    #[test]
    fn saves_and_reads_events() {
        let dir = tempfile::tempdir().unwrap();
        let payload = format!(r#"{{"session_id":"{ID}","hook_event_name":"Stop","stop_hook_active":false}}"#);
        assert_eq!(run("done", payload.as_bytes(), dir.path()), 0);
        let file = dir.path().join(format!("{ID}.txt"));
        assert_eq!(read(&file), Some((ID.to_string(), Hook::Done)));

        run("working", payload.as_bytes(), dir.path());
        assert_eq!(read(&file).map(|r| r.1), Some(Hook::Working), "the last event wins");
    }

    #[test]
    fn only_questions_count_as_ask() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(format!("{ID}.txt"));
        let idle = format!(r#"{{"session_id":"{ID}","notification_type":"idle_prompt","message":"Claude is waiting for your input"}}"#);
        run("ask", idle.as_bytes(), dir.path());
        assert!(!file.exists(), "an idle reminder is not a question");
        let old = format!(r#"{{"session_id":"{ID}","message":"Claude needs your permission to use Bash"}}"#);
        run("ask", old.as_bytes(), dir.path());
        assert_eq!(read(&file).map(|r| r.1), Some(Hook::Ask), "no type: told by the message");
        let typed = format!(r#"{{"session_id":"{ID}","notification_type":"permission_prompt","message":"x"}}"#);
        run("ask", typed.as_bytes(), dir.path());
        assert_eq!(read(&file).map(|r| r.1), Some(Hook::Ask));
    }

    #[test]
    fn ignores_what_it_cannot_use() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run("done", &b"not json"[..], dir.path()), 0);
        assert_eq!(run("done", &br#"{"session_id":"../../evil"}"#[..], dir.path()), 0);
        assert_eq!(run("bogus", format!(r#"{{"session_id":"{ID}"}}"#).as_bytes(), dir.path()), 0);
        assert_eq!(std::fs::read_dir(dir.path()).map(|d| d.count()).unwrap_or(0), 0, "nothing written");
        assert_eq!(read(Path::new("C:\\x\\notes.txt")), None);
    }
}
