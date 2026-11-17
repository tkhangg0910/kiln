//! Reads the status files the relay leaves in `<data dir>\status`, one per session, and keeps
//! them fresh. Kilno only shows what Claude hands its status line (and, for the limits, what
//! the user's own status line script saved, and the usage reply Claude Code saved for itself).

use super::model::{AccountLimits, SessionStatus};
use super::{claude_usage, parse};
use chrono::{DateTime, Utc};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// A status file is written for every session ever opened; one quiet this long is gone.
pub const KEEP_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// A file's (modified time, length): changed when either is.
type Stamp = (SystemTime, u64);

type Listener = Arc<dyn Fn(&SessionStatus) + Send + Sync>;
type LimitListener = Arc<dyn Fn() + Send + Sync>;

/// Per-session statuses by id (ignoring case), following the folder once started.
pub struct StatusStore {
    inner: Arc<Inner>,
    watcher: Mutex<Option<RecommendedWatcher>>,
    /// Follows the folder holding `.claude.json`, when one is set.
    claude_json_watcher: Mutex<Option<RecommendedWatcher>>,
}

struct Inner {
    dir: PathBuf,
    fallback_effort: Box<dyn Fn() -> String + Send + Sync>,
    usage_cache: PathBuf,
    /// Claude Code's `.claude.json` ([`claude_usage`]), when set.
    claude_json: Option<PathBuf>,
    /// Its limits as of its last (modified time, length): a 200 kB file is not parsed again
    /// every clock tick when Claude Code has not touched it.
    claude_json_seen: Mutex<Option<(Stamp, Option<AccountLimits>)>>,
    /// The last live usage reply ([`super::live`]), while live usage is on.
    live: Mutex<Option<AccountLimits>>,
    by_id: Mutex<HashMap<String, SessionStatus>>,
    listeners: Mutex<Vec<Listener>>,
    limit_listeners: Mutex<Vec<LimitListener>>,
}

impl StatusStore {
    /// `fallback_effort` is asked when a payload has no effort ([`parse::default_effort`] in the
    /// app); `usage_cache` is the user's script's cache ([`parse::usage_cache_path`]).
    pub fn new(
        dir: impl Into<PathBuf>,
        fallback_effort: impl Fn() -> String + Send + Sync + 'static,
        usage_cache: impl Into<PathBuf>,
    ) -> Self {
        Self {
            inner: Arc::new(Inner {
                dir: dir.into(),
                fallback_effort: Box::new(fallback_effort),
                usage_cache: usage_cache.into(),
                claude_json: None,
                claude_json_seen: Mutex::new(None),
                live: Mutex::new(None),
                by_id: Mutex::new(HashMap::new()),
                listeners: Mutex::new(Vec::new()),
                limit_listeners: Mutex::new(Vec::new()),
            }),
            watcher: Mutex::new(None),
            claude_json_watcher: Mutex::new(None),
        }
    }

    /// Also read the usage reply Claude Code saves in `path` ([`claude_usage::path`]): the only
    /// place the per-model weekly limits are found, and a fresher 5-hour and 7-day than a quiet
    /// session's.
    pub fn with_claude_json(mut self, path: impl Into<PathBuf>) -> Self {
        let inner = Arc::get_mut(&mut self.inner).expect("set before the store is shared");
        inner.claude_json = Some(path.into());
        self
    }

    /// Called (on a background thread when it comes from the watcher) whenever a session's file
    /// brings a new status.
    pub fn on_change(&self, f: impl Fn(&SessionStatus) + Send + Sync + 'static) {
        lock(&self.inner.listeners).push(Arc::new(f));
    }

    pub fn get(&self, session_id: &str) -> Option<SessionStatus> {
        lock(&self.inner.by_id)
            .get(&session_id.to_lowercase())
            .cloned()
    }

    /// The account's limits: for each of the 5-hour and 7-day, the newest of the session
    /// reports, the user's script's cache, Claude Code's saved usage reply and the live one (on a
    /// tie, in that order; [`parse::merge`]), except that a live reply under [`super::live::FRESH`]
    /// old wins outright. Some accounts' Claude Code sends none in its reports at all. The
    /// per-model weekly limits come from the newer usage reply, as only those have them.
    pub fn limits(&self) -> Option<AccountLimits> {
        let cached = read_usage_cache(&self.inner.usage_cache);
        let saved = self.inner.claude_json();
        let live = lock(&self.inner.live).clone();
        // The model rows come whole from the newer of the two replies that have them.
        let replies = [saved.clone(), live.clone()].into_iter().flatten();
        let newest_reply = replies.reduce(|a, b| if b.updated > a.updated { b } else { a });
        let models = newest_reply.as_ref().map(|s| s.models.clone()).unwrap_or_default();
        let sessions: Vec<_> = lock(&self.inner.by_id)
            .values()
            .map(|s| (s.five_hour.clone(), s.seven_day.clone(), s.updated))
            .collect();
        let live_now = live.clone();
        let files = [cached, saved, live]
            .into_iter()
            .flatten()
            .map(|l| (l.five_hour, l.seven_day, l.updated));
        match parse::merge(sessions.into_iter().chain(files)) {
            Some(mut best) => {
                best.models = models;
                // A recent live reply is what `/usage` says, so it beats a status line report even
                // when that file is newer: Claude Code rewrites it with the same lagging numbers.
                let fresh = chrono::Duration::from_std(super::live::FRESH).unwrap_or_default();
                if let Some(l) = live_now.filter(|l| Utc::now() - l.updated <= fresh) {
                    best.five_hour = l.five_hour.or(best.five_hour);
                    best.seven_day = l.seven_day.or(best.seven_day);
                    best.updated = best.updated.max(l.updated);
                }
                Some(best)
            }
            // A saved reply with only a model's limit still shows it.
            None => newest_reply.filter(|s| !s.models.is_empty()),
        }
    }

    /// The live usage reply, or None to forget it (live usage switched off).
    pub fn set_live(&self, limits: Option<AccountLimits>) {
        *lock(&self.inner.live) = limits;
    }

    /// Calls `f` (on the watcher's thread) when Claude Code's `.claude.json` changes, so a fresh
    /// usage reply shows at once rather than on the next clock tick. Set before [`Self::start`].
    pub fn on_limits_change(&self, f: impl Fn() + Send + Sync + 'static) {
        lock(&self.inner.limit_listeners).push(Arc::new(f));
    }

    /// Creates the folder, starts following `*.json` in it, prunes files quiet for
    /// [`KEEP_FOR`], then reads what is there. A second call does nothing.
    pub fn start(&self) -> std::io::Result<()> {
        let mut slot = lock(&self.watcher);
        if slot.is_some() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.inner.dir)?;

        // The relay renames a finished file into place, so a rename is the usual event.
        let inner = Arc::clone(&self.inner);
        let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Remove(_) | EventKind::Access(_)) {
                return;
            }
            for path in event.paths.iter().filter(|p| is_json(p)) {
                inner.load(path);
            }
        })
        .map_err(std::io::Error::other)?;
        watcher
            .watch(&self.inner.dir, RecursiveMode::NonRecursive)
            .map_err(std::io::Error::other)?;
        *slot = Some(watcher);
        drop(slot);
        self.watch_claude_json();

        prune(&self.inner.dir, Utc::now() - KEEP_FOR);
        for file in json_files(&self.inner.dir) {
            self.inner.load(&file);
        }
        Ok(())
    }

    /// Follows the folder holding `.claude.json` (only that file's events count; Claude Code may
    /// write it through a temp file and a rename). Best effort: without it the clock still
    /// picks a new reply up.
    fn watch_claude_json(&self) {
        let Some(path) = self.inner.claude_json.clone() else { return };
        let (Some(dir), Some(name)) = (path.parent(), path.file_name().map(|n| n.to_os_string())) else {
            return;
        };
        let inner = Arc::clone(&self.inner);
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let Ok(event) = res else { return };
            if matches!(event.kind, EventKind::Access(_)) {
                return;
            }
            if !event.paths.iter().any(|p| p.file_name() == Some(name.as_os_str())) {
                return;
            }
            let listeners: Vec<LimitListener> = lock(&inner.limit_listeners).clone();
            for f in listeners {
                f();
            }
        });
        let Ok(mut watcher) = watcher else { return };
        if watcher.watch(dir, RecursiveMode::NonRecursive).is_ok() {
            *lock(&self.claude_json_watcher) = Some(watcher);
        }
    }

    /// Reads one file. Anything unreadable keeps the last good status.
    pub fn load(&self, path: &Path) {
        self.inner.load(path);
    }
}

impl Inner {
    /// The saved usage reply's limits, parsed again only when the file changed.
    fn claude_json(&self) -> Option<AccountLimits> {
        let path = self.claude_json.as_deref()?;
        let meta = std::fs::metadata(path).ok()?;
        let stamp = (meta.modified().ok()?, meta.len());
        let mut seen = lock(&self.claude_json_seen);
        if let Some((s, limits)) = seen.as_ref() {
            if *s == stamp {
                return limits.clone();
            }
        }
        // Claude Code rewrites this file often. One caught half-written keeps the last good
        // limits and is not marked seen, so the next tick reads it again.
        let read = std::fs::read(path).ok().map(|b| claude_usage::read(&String::from_utf8_lossy(&b)));
        match read {
            Some(Ok(limits)) => {
                *seen = Some((stamp, limits.clone()));
                limits
            }
            _ => seen.as_ref().and_then(|(_, l)| l.clone()),
        }
    }

    fn load(&self, path: &Path) {
        // std opens with share read|write|delete, so the relay's rename over this file still
        // succeeds while we read it.
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };
        let Some(updated) = crate::time::mtime(path) else {
            return;
        };
        let Some(status) = parse::parse(
            &String::from_utf8_lossy(&bytes),
            updated,
            &self.fallback_effort,
        ) else {
            return;
        };
        {
            let mut by_id = lock(&self.by_id);
            let key = status.session_id.to_lowercase();
            if by_id.get(&key) == Some(&status) {
                return;
            }
            by_id.insert(key, status.clone());
        }
        let listeners: Vec<Listener> = lock(&self.listeners).clone();
        for f in listeners {
            f(&status);
        }
    }
}

/// Deletes the `*.json` files in `dir` last written before `before`. Errors are ignored.
pub fn prune(dir: &Path, before: DateTime<Utc>) {
    for file in json_files(dir) {
        if crate::time::mtime(&file).is_some_and(|t| t < before) {
            let _ = std::fs::remove_file(&file);
        }
    }
}

fn read_usage_cache(path: &Path) -> Option<AccountLimits> {
    let bytes = std::fs::read(path).ok()?;
    let updated = crate::time::mtime(path)?;
    parse::parse_usage_cache(&String::from_utf8_lossy(&bytes), updated)
}

fn json_files(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .filter(|p| is_json(p) && p.is_file())
                .collect()
        })
        .unwrap_or_default()
}

fn is_json(p: &Path) -> bool {
    p.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("json"))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}
