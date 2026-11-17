//! The status line relay. Kilno's Claude panes start with a status line of their own:
//!
//! ```text
//! 'C:/…/kilno.exe' --statusline --pass | {
//! their own statusLine command
//! }
//! ```
//!
//! Claude Code pipes the session's JSON to it on every refresh. We keep a copy for Kilno and
//! pass the same bytes on to the user's command, which the shell starts alongside us, so the
//! line inside the pane looks as it always has and our start-up overlaps theirs. With no
//! command of theirs it is only `kilno.exe --statusline`, which prints nothing. The user's
//! settings.json is only ever read.
//!
//! [`run`] is on the path of every status refresh: it must stay cheap (no UI, no logging).

use crate::paths::{DataDir, ReadOnlyDir};
use crate::pane_status::Hook;
use crate::settings::push_json_string;
use serde_json::{Map, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const ARG: &str = "--statusline";
pub const PASS_ARG: &str = "--pass";

/// `<data dir>\status`, one `<guid>.json` per session.
pub fn status_dir(data: &DataDir) -> PathBuf {
    data.join("status")
}

/// The file panes pass to `--settings`. A dev build has its own, so its panes relay through the
/// dev exe and the installed Kilno's through the installed one.
pub fn settings_path(data: &DataDir) -> PathBuf {
    data.join(if cfg!(debug_assertions) {
        "statusline.dev.json"
    } else {
        "statusline.json"
    })
}

/// `kilno --statusline [--pass]`: reads all of stdin, then (with `pass`) writes the same bytes
/// to stdout first, since the user's command is waiting on them, then saves. Always 0: this
/// must never break the user's line.
pub fn run(pass: bool, mut stdin: impl Read, mut stdout: impl Write, status_dir: &Path) -> i32 {
    let mut bytes = Vec::new();
    let _ = stdin.read_to_end(&mut bytes);
    if pass {
        // Their command may not read it (a broken pipe); ours still saves.
        let _ = stdout.write_all(&bytes).and_then(|()| stdout.flush());
    }
    save(&bytes, status_dir);
    0
}

/// Written whole and renamed into place, so a reader never sees half a file. Anything odd is
/// dropped silently.
fn save(json: &[u8], dir: &Path) {
    let body = json.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(json);
    let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(body) else {
        return;
    };
    // The id becomes a file name.
    let Some(guid) = root
        .get("session_id")
        .and_then(Value::as_str)
        .and_then(super::parse::dotnet_guid)
    else {
        return;
    };
    let guid = guid.hyphenated().to_string();
    let path = dir.join(format!("{guid}.json"));
    let tmp = dir.join(format!("{guid}.{}.tmp", std::process::id()));
    let result = std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::write(&tmp, json))
        .and_then(|()| std::fs::rename(&tmp, &path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// The `--settings` file's contents (compact, escaped as the C# build wrote it). The rest of
/// the user's statusLine (padding and the like) is copied as it is; `type` and `command` are
/// set in place or appended. Ask rules go in `permissions.ask`, which only adds to the user's
/// own (Claude Code merges the arrays, and ask wins over allow); none, no permissions key.
pub fn settings_json(
    exe_path: &str,
    user_status_line: Option<&Map<String, Value>>,
    ask: &[String],
) -> String {
    let mut status_line = user_status_line.cloned().unwrap_or_default();
    let theirs = status_line
        .get("command")
        .and_then(Value::as_str)
        .filter(|c| !c.is_empty())
        .map(str::to_string);

    // Claude Code runs status lines through Git Bash, where a backslash is an escape. The
    // braces give their whole command our output, even a `cd x && ./line.sh`; the newlines keep
    // a trailing # comment of theirs from eating the closing brace. Single quotes, so a $ or `
    // in the path stays text.
    let exe = format!("'{}'", exe_path.replace('\\', "/").replace('\'', "'\\''"));
    let relay = format!("{exe} {ARG}");
    status_line.insert("type".into(), Value::from("command"));
    let command = match theirs {
        Some(c) => format!("{relay} {PASS_ARG} | {{\n{c}\n}}"),
        None => relay,
    };
    status_line.insert("command".into(), Value::from(command));

    let mut settings = Map::new();
    settings.insert("statusLine".into(), Value::Object(status_line));
    if !ask.is_empty() {
        let mut permissions = Map::new();
        permissions.insert("ask".into(), Value::from(ask.to_vec()));
        settings.insert("permissions".into(), Value::Object(permissions));
    }
    // The turn, told exactly ([`super::hooks`]). Added to the user's own hooks, not instead.
    let mut hooks = Map::new();
    for (event, hook) in [("UserPromptSubmit", Hook::Working), ("Stop", Hook::Done), ("Notification", Hook::Ask)] {
        let command = format!("{exe} {} {}", super::hooks::ARG, super::hooks::name(hook));
        let mut cmd = Map::new();
        cmd.insert("type".into(), Value::from("command"));
        cmd.insert("command".into(), Value::from(command));
        let mut group = Map::new();
        group.insert("hooks".into(), Value::Array(vec![Value::Object(cmd)]));
        hooks.insert(event.into(), Value::Array(vec![Value::Object(group)]));
    }
    settings.insert("hooks".into(), Value::Object(hooks));
    let mut out = String::new();
    push_compact(&mut out, &Value::Object(settings));
    out
}

/// The user's `statusLine` object from `<claude dir>\settings.json`; None when the file is
/// missing, broken, or it isn't an object. Only read.
pub fn user_status_line(claude_dir: &ReadOnlyDir) -> Option<Map<String, Value>> {
    let bytes = std::fs::read(claude_dir.join("settings.json")).ok()?;
    let text = String::from_utf8_lossy(&bytes);
    let root: Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    root.get("statusLine")?.as_object().cloned()
}

/// Writes the `--settings` file for a pane about to start and returns its path, or None if it
/// could not be written (then the pane starts without `--settings`). Read fresh each time, so a
/// pane follows the user's current statusLine and Kilno's current ask rules; rewritten only when
/// its content changes.
pub fn write_settings(
    data: &DataDir,
    claude_dir: &ReadOnlyDir,
    exe_path: &str,
    ask: &[String],
) -> Option<PathBuf> {
    let json = settings_json(exe_path, user_status_line(claude_dir).as_ref(), ask);
    let path = settings_path(data);
    std::fs::create_dir_all(data.path()).ok()?;
    if std::fs::read_to_string(&path).ok().as_deref() != Some(json.as_str()) {
        std::fs::write(&path, json).ok()?;
    }
    Some(path)
}

/// Compact JSON with System.Text.Json's escaping, so the file matches the C# build's byte for
/// byte and switching builds does not rewrite it.
fn push_compact(out: &mut String, v: &Value) {
    match v {
        Value::Object(m) => {
            out.push('{');
            for (i, (k, v)) in m.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_string(out, k);
                out.push(':');
                push_compact(out, v);
            }
            out.push('}');
        }
        Value::Array(a) => {
            out.push('[');
            for (i, v) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_compact(out, v);
            }
            out.push(']');
        }
        Value::String(s) => push_json_string(out, s),
        other => out.push_str(&other.to_string()),
    }
}
