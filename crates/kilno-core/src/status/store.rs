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

/// Per-session statuses by id (ignoring case), following the folder once started.
pub struct StatusStore {
    inner: Arc<Inner>,
    watcher: Mutex<Option<RecommendedWatcher>>,
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
    by_id: Mutex<HashMap<String, SessionStatus>>,
    listeners: Mutex<Vec<Listener>>,
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
                by_id: Mutex::new(HashMap::new()),
                listeners: Mutex::new(Vec::new()),
            }),
            watcher: Mutex::new(None),
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

    /// The account's limits: the newest of a session report, the user's script's cache and
    /// Claude Code's saved usage reply (on a tie, in that order). Some accounts' Claude Code
    /// sends none in its reports at all. The per-model weekly limits come from the saved reply
    /// whichever wins, as only it has them.
    pub fn limits(&self) -> Option<AccountLimits> {
        let from_claude = parse::newest(lock(&self.inner.by_id).values());
        let cached = read_usage_cache(&self.inner.usage_cache);
        let saved = self.inner.claude_json();
        let models = saved.as_ref().map(|s| s.models.clone()).unwrap_or_default();
        let mut best = [from_claude, cached, saved]
            .into_iter()
            .flatten()
            .reduce(|best, l| if l.updated > best.updated { l } else { best })?;
        best.models = models;
        Some(best)
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

        prune(&self.inner.dir, Utc::now() - KEEP_FOR);
        for file in json_files(&self.inner.dir) {
            self.inner.load(&file);
        }
        Ok(())
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
