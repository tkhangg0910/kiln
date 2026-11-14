//! Codex transcripts (`<codex home>\sessions\YYYY\MM\DD\rollout-*.jsonl`) as sidebar rows, the
//! way [`super::claude`] does for Claude Code. Never writes there. Times are compared in UTC
//! (PORTING.md D11).

use super::changed::ChangedFiles;
use super::claude::{get_str, lines, tail_reader, walk, TAIL_WINDOW};
use super::watch::{OnChange, SessionSource, Watcher};
use super::model::NO_PROMPT;
use super::{SessionInfo, TurnState};
use crate::agent::detect::is_rollout;
use crate::agent::AgentKind;
use crate::paths::{codex_home, is_inside, same_dir, Env, ReadOnlyDir};
use crate::status::{Limit, SessionStatus};
use crate::text::{clean, shorten};
use crate::time::{from_unix, mtime, parse_iso};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct CodexStore {
    home: ReadOnlyDir,
    /// Keyed by lowercased path. A transcript only grows, and resuming one keeps its file
    /// time, so its length alone says it changed.
    cache: HashMap<String, (u64, SessionInfo)>,
    changed: Arc<ChangedFiles>,
    watcher: Option<Watcher>,
}

impl CodexStore {
    pub fn new(home: ReadOnlyDir) -> Self {
        Self { home, cache: HashMap::new(), changed: Arc::default(), watcher: None }
    }

    /// `%CODEX_HOME%` if set, else `~\.codex`.
    pub fn for_this_user(env: &dyn Env) -> Self {
        Self::new(codex_home(env))
    }

    fn sessions_dir(&self) -> PathBuf {
        self.home.join("sessions")
    }

    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }

    /// session_index.jsonl: one line per naming, so the last line for an id wins.
    fn titles(&self) -> HashMap<String, String> {
        let mut titles = HashMap::new();
        let Ok(f) = File::open(self.home.join("session_index.jsonl")) else { return titles };
        for line in lines(BufReader::new(f)).map_while(Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if let Some(id) = get_str(&v, "id") {
                let name = clean(get_str(&v, "thread_name").unwrap_or(""));
                if !name.is_empty() {
                    titles.insert(id.to_lowercase(), shorten(&name, 90));
                }
            }
        }
        titles
    }
}

impl SessionSource for CodexStore {
    fn root_exists(&self) -> bool {
        self.sessions_dir().is_dir()
    }

    fn scan(&mut self) -> Vec<SessionInfo> {
        if !self.root_exists() {
            return Vec::new();
        }
        let sessions = self.sessions_dir();
        let (all, mut files) = self.changed.take(self.watcher.is_some());
        if all {
            files.clear();
            let _ = walk(&sessions, &mut files);
            files.retain(|f| is_rollout(f));
            let present: HashSet<String> = files.iter().map(|f| key(f)).collect();
            self.cache.retain(|k, _| present.contains(k));
        }

        for file in files {
            // The watcher covers all of home: session_index.jsonl, archived sessions and such.
            if !is_rollout(&file) || !is_inside(&sessions, &file) || same_dir(&sessions, &file) {
                continue;
            }
            let k = key(&file);
            let size = std::fs::metadata(&file).map(|m| if m.is_file() { m.len() } else { 0 }).unwrap_or(0);
            if size == 0 {
                self.cache.remove(&k);
                continue;
            }
            if self.cache.get(&k).is_some_and(|(s, _)| *s == size) {
                continue;
            }
            match parse(&file) {
                Some(info) => {
                    self.cache.insert(k, (size, info));
                }
                // Unreadable mid-write: keep the row as it was and read it again next scan.
                None => self.changed.add(&file),
            }
        }

        // Codex keeps names apart from the transcript; a rename changes no transcript, so the
        // title is looked up every scan.
        let titles = self.titles();
        let mut rows: Vec<SessionInfo> = self
            .cache
            .values_mut()
            .map(|(_, info)| {
                info.ai_title = titles.get(&info.session_id.to_lowercase()).cloned();
                info.clone()
            })
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.last_activity));
        rows
    }

    /// Watches all of home: transcripts and session_index.jsonl (renames) are both `*.jsonl`.
    fn start_watching(&mut self, on_change: OnChange) {
        if !self.home.path().is_dir() || self.watcher.is_some() {
            return;
        }
        self.watcher = Watcher::start(self.home.path(), self.changed.clone(), on_change).ok();
    }

    fn stop_watching(&mut self) {
        self.watcher = None;
    }
}

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Line 0 is session_meta: id, folder, and for a fork the session it came from. It also carries
/// Codex's whole instructions (~19 KB), so only the head and a window off the end are read.
fn parse(file: &Path) -> Option<SessionInfo> {
    let mut head = lines(BufReader::new(File::open(file).ok()?));
    let first = head.next()?.ok()?;
    let meta: Value = serde_json::from_str(&first).ok()?;
    if get_str(&meta, "type") != Some("session_meta") {
        return None;
    }
    let p = meta.get("payload")?;
    let id = get_str(p, "id")?.to_string();
    let cwd = get_str(p, "cwd").map(str::to_string);
    let parent_id = get_str(p, "forked_from_id").map(str::to_string);
    let started = get_str(p, "timestamp")
        .and_then(parse_iso)
        .or_else(|| mtime(file))
        .unwrap_or_else(Utc::now);

    // The first thing you typed. Codex puts its own context in front as user messages too,
    // wrapped in pseudo-XML, which clean() drops. The model comes first, in turn_context.
    let mut preview = String::new();
    let (mut model, mut effort) = (None, None);
    for line in head.take(60) {
        let line = line.ok()?;
        preview = user_text(&line).unwrap_or_default();
        if model.is_none() && line.contains("\"turn_context\"") {
            if let Some((m, e)) = turn_context(&line) {
                (model, effort) = (m, e);
            }
        }
        if !preview.is_empty() {
            break;
        }
    }

    let tail = read_tail(file);
    if tail.model.is_some() {
        (model, effort) = (tail.model.clone(), tail.effort.clone());
    }
    let updated = tail.last.unwrap_or(started);
    // What the strip shows, from the same lines: Codex has no status line to relay.
    let status = (tail.context_size > 0 || model.is_some()).then(|| {
        let (used, size, percent) = context(tail.context_used, tail.context_size);
        SessionStatus {
            session_id: id.clone(),
            model: model.clone(),
            cwd: cwd.clone(),
            context_used: used,
            context_size: size,
            context_percent: percent,
            effort: effort.clone().unwrap_or_default(),
            five_hour: tail.five_hour.clone(),
            seven_day: tail.seven_day.clone(),
            updated,
        }
    });

    Some(SessionInfo {
        session_id: id,
        agent: AgentKind::Codex,
        parent_id,
        file: Some(file.to_path_buf()),
        project_dir: cwd.unwrap_or_else(|| {
            file.parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_else(|| "(unknown)".into())
        }),
        preview: if preview.is_empty() { NO_PROMPT.to_string() } else { shorten(&preview, 120) },
        ai_title: None,
        last_prompt: tail.prompt.as_deref().map(|p| shorten(p, 120)),
        last_activity: updated,
        started,
        turn: tail.turn,
        status,
    })
}

/// What every Codex request carries before you type (system prompt and tools). Codex leaves it
/// out of both sides of its "% context left" (codex-rs TokenUsage).
const BASELINE: i64 = 12_000;

/// The context as Codex itself shows it, as (used, size, percent), so the strip and Codex's own
/// footer agree: the baseline comes off what is used and off the window, and the percentage is
/// 100 minus Codex's rounded "% left".
pub fn context(used: i64, window: i64) -> (i64, i64, i32) {
    if window <= BASELINE {
        return (used, window, if window > 0 { 100 } else { 0 });
    }
    let size = window - BASELINE;
    let mine = (used - BASELINE).max(0);
    // f64::round rounds half away from zero, as Codex does.
    let left = ((size - mine) as f64 * 100.0 / size as f64).clamp(0.0, 100.0).round() as i32;
    (mine, size, 100 - left)
}

/// The session a pane Kilno started turned out to be: for a fork, the one naming `parent_id`;
/// else a non-fork in the pane's `folder`. Either way started in `[since, until)` and not in
/// `taken` (other panes' ids). Oldest first, so two panes in one folder bind in start order.
///
/// session_meta's time is when codex started, not the first message, so a session started
/// before `until` (the next pane like this one opening, or since + 10 min) is this pane's.
/// Without that bound an older pane took a newer pane's session.
pub fn find_started<'a>(
    sessions: &'a [SessionInfo],
    folder: &str,
    parent_id: Option<&str>,
    since: DateTime<Utc>,
    taken: &HashSet<String>,
    until: Option<DateTime<Utc>>,
) -> Option<&'a SessionInfo> {
    sessions
        .iter()
        .filter(|s| s.started >= since && until.is_none_or(|u| s.started < u) && !taken.contains(&s.session_id))
        .filter(|s| match parent_id {
            None => s.parent_id.is_none() && same_dir(Path::new(&s.project_dir), Path::new(folder)),
            Some(p) => s.parent_id.as_deref() == Some(p),
        })
        .min_by_key(|s| s.started)
}

/// What the end of a transcript says. Context and limits are from the last token_count.
#[derive(Default)]
struct Tail {
    last: Option<DateTime<Utc>>,
    prompt: Option<String>,
    turn: Option<TurnState>,
    model: Option<String>,
    effort: Option<String>,
    context_used: i64,
    context_size: i64,
    five_hour: Option<Limit>,
    seven_day: Option<Limit>,
}

/// Last active is the last line's own timestamp: a resumed session's file can keep its old time.
fn read_tail(file: &Path) -> Tail {
    let mut t = Tail::default();
    let Ok(r) = tail_reader(file, TAIL_WINDOW) else { return t };
    for line in lines(r).map_while(Result::ok) {
        // A line still being written doesn't parse; the one before it stands.
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if let Some(at) = get_str(&v, "timestamp").and_then(parse_iso) {
            t.last = Some(at);
        }
        match get_str(&v, "type") {
            Some("turn_context") => {
                if let Some((m, e)) = turn_context(&line) {
                    (t.model, t.effort) = (m, e);
                }
            }
            Some("event_msg") => {
                if let Some(p) = v.get("payload") {
                    match get_str(p, "type") {
                        Some("task_started") => t.turn = Some(TurnState::Working),
                        Some("task_complete") => t.turn = Some(TurnState::Done),
                        Some("turn_aborted") => t.turn = Some(TurnState::Stopped),
                        Some("token_count") => tokens(p, &mut t),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        if let Some(text) = user_text(&line) {
            t.prompt = Some(text);
        }
    }
    t
}

/// The model and reasoning effort a turn ran with. Effort is None when Codex left it to its
/// default.
fn turn_context(line: &str) -> Option<(Option<String>, Option<String>)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let p = v.get("payload")?;
    let effort = p
        .get("collaboration_mode")
        .filter(|m| m.is_object())
        .and_then(|m| m.get("settings"))
        .and_then(|s| get_str(s, "reasoning_effort"))
        .map(str::to_string);
    Some((get_str(p, "model").map(str::to_string), effort))
}

/// token_count: the context in use is the last request's total against the model's window.
/// Codex's limits come as primary and secondary windows; only a 5-hour (300 min) or 7-day
/// (10080 min) one maps onto the strip, so another length is left out.
fn tokens(p: &Value, t: &mut Tail) {
    if let Some(info) = p.get("info").filter(|i| i.is_object()) {
        if let Some(total) = info.get("last_token_usage").and_then(|u| u.get("total_tokens")).and_then(Value::as_i64) {
            t.context_used = total;
        }
        if let Some(window) = info.get("model_context_window").and_then(Value::as_i64) {
            t.context_size = window;
        }
    }
    let Some(limits) = p.get("rate_limits").filter(|l| l.is_object()) else { return };
    for name in ["primary", "secondary"] {
        let Some(l) = limits.get(name).filter(|l| l.is_object()) else { continue };
        let (Some(percent), Some(minutes)) = (
            l.get("used_percent").and_then(Value::as_f64),
            l.get("window_minutes").and_then(Value::as_i64).and_then(|m| i32::try_from(m).ok()),
        ) else {
            continue;
        };
        let resets = l.get("resets_at").and_then(Value::as_i64).and_then(from_unix);
        let limit = Limit::new(percent.floor() as i32, resets);
        match minutes {
            300 => t.five_hour = Some(limit),
            10080 => t.seven_day = Some(limit),
            _ => {}
        }
    }
}

/// A response_item message from the user, as typed; None for anything else (including the
/// context Codex sends as user messages of its own).
pub fn user_text(line: &str) -> Option<String> {
    // Substring test first: most lines are tool calls and reasoning.
    if !line.contains("\"role\":\"user\"") {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    if get_str(&v, "type") != Some("response_item") {
        return None;
    }
    let p = v.get("payload")?;
    if get_str(p, "type") != Some("message") {
        return None;
    }
    p.get("content")?
        .as_array()?
        .iter()
        .map(|b| clean(get_str(b, "text").unwrap_or("")))
        .find(|t| !t.is_empty())
}
