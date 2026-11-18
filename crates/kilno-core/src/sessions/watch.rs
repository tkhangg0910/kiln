//! Watching a transcript tree. The agents write a line at a time, so events are collected into
//! [`ChangedFiles`] and reported once things settle (600 ms debounce), on a background thread.

use super::changed::ChangedFiles;
use super::SessionInfo;
use notify::{RecursiveMode, Watcher as _};
use std::path::Path;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long the tree must be quiet before a change is reported.
pub const DEBOUNCE: Duration = Duration::from_millis(600);

/// Called on a background thread after the transcripts settle.
pub type OnChange = Box<dyn Fn() + Send + 'static>;

/// An agent's transcripts, read only, as rows for the sidebar.
pub trait SessionSource {
    fn root_exists(&self) -> bool;

    fn scan(&mut self) -> Vec<SessionInfo>;

    /// No-op when the root is missing or already watched.
    fn start_watching(&mut self, on_change: OnChange);

    fn stop_watching(&mut self);
}

/// A running watcher. Dropping it stops watching (a pending report is dropped too).
pub struct Watcher {
    _inner: notify::RecommendedWatcher,
}

impl Watcher {
    /// Watches `root` recursively for `*.jsonl` files: each one created, written, deleted or
    /// renamed (both names) goes into `changed`. When the watcher loses track (an error or an
    /// overflow) the next scan reads everything. `on_change` runs [`DEBOUNCE`] after the last
    /// event.
    pub fn start(root: &Path, changed: Arc<ChangedFiles>, on_change: OnChange) -> notify::Result<Self> {
        let (bump, bumps) = mpsc::channel::<()>();
        let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            match res {
                Ok(e) if e.need_rescan() => changed.add_all(),
                Ok(e) => {
                    for p in e.paths.iter().filter(|p| crate::agent::detect::has_ext(p, "jsonl")) {
                        changed.add(p);
                    }
                }
                Err(_) => changed.add_all(),
            }
            let _ = bump.send(());
        })?;
        inner.watch(root, RecursiveMode::Recursive)?;
        std::thread::Builder::new()
            .name("kilno-transcript-watch".into())
            .spawn(move || debounce(bumps, on_change))
            .map_err(notify::Error::io)?;
        Ok(Self { _inner: inner })
    }
}

impl Watcher {
    /// Watches only `dir` itself (not below it) and reports, [`DEBOUNCE`] after the last event,
    /// whenever a file `wanted` accepts changes. For a store kept in one file, such as
    /// OpenCode's database and its `-wal`.
    pub fn start_files(dir: &Path, wanted: fn(&Path) -> bool, on_change: OnChange) -> notify::Result<Self> {
        let (bump, bumps) = mpsc::channel::<()>();
        let mut inner = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            let hit = match res {
                Ok(e) => e.need_rescan() || e.paths.iter().any(|p| wanted(p)),
                Err(_) => true,
            };
            if hit {
                let _ = bump.send(());
            }
        })?;
        inner.watch(dir, RecursiveMode::NonRecursive)?;
        std::thread::Builder::new()
            .name("kilno-store-watch".into())
            .spawn(move || debounce(bumps, on_change))
            .map_err(notify::Error::io)?;
        Ok(Self { _inner: inner })
    }
}

/// Re-arms on every bump; fires once the bumps stop for [`DEBOUNCE`]. Ends when the watcher
/// (the sender) is dropped.
fn debounce(bumps: mpsc::Receiver<()>, on_change: OnChange) {
    let mut due: Option<Instant> = None;
    loop {
        let got = match due {
            None => bumps.recv().map_err(|_| RecvTimeoutError::Disconnected),
            Some(at) => bumps.recv_timeout(at.saturating_duration_since(Instant::now())),
        };
        match got {
            Ok(()) => due = Some(Instant::now() + DEBOUNCE),
            Err(RecvTimeoutError::Timeout) => {
                due = None;
                on_change();
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
