//! The status wiring (MainWindow's ShowStatusBar / WarnReserve and its timers), off the UI
//! thread: the status store follows `%LOCALAPPDATA%\Kiln\status` on its watcher; one clock
//! thread polls git every 3 s for the open panes' folders while the window shows, and checks
//! the reserve every 30 s (hidden too: a script's cached limits change between reports) and on
//! every status change. The UI only reads what these hold.

use kiln_core::git::{GitCache, INTERVAL};
use kiln_core::paths::{DataDir, OsEnv};
use kiln_core::settings::KilnSettings;
use kiln_core::status::reserve::ReserveWatch;
use kiln_core::status::store::StatusStore;
use kiln_core::status::strip::Meter;
use kiln_core::status::{parse, relay, AccountLimits};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The clock: countdowns and staleness move, and the reserve is looked at, this often.
pub const CLOCK: Duration = Duration::from_secs(30);

enum Poke {
    /// A status file brought something new.
    Status,
    /// The window was shown, or a setting changed.
    Wake,
    Stop,
}

struct Shared {
    shown: AtomicBool,
    folders: Mutex<Vec<String>>,
    settings: Mutex<KilnSettings>,
    /// Claude's account limits as of the last status change or clock tick: reading them reads
    /// the user's script's cache file, so the UI never does that per frame.
    limits: Mutex<Option<AccountLimits>>,
}

pub struct Status {
    pub store: Arc<StatusStore>,
    pub git: GitCache,
    shared: Arc<Shared>,
    tx: Sender<Poke>,
}

impl Status {
    /// Starts the store (on its own thread: it reads and prunes the folder first) and the
    /// clock. `changed` runs on a background thread when something the strip shows changed;
    /// `reserve` for each limit that crossed the reserve.
    pub fn start(
        data: &DataDir,
        settings: KilnSettings,
        shown: bool,
        changed: impl Fn() + Send + Sync + 'static,
        reserve: impl Fn(&Meter) + Send + 'static,
    ) -> Self {
        let dir = relay::status_dir(data);
        let store = Arc::new(StatusStore::new(dir, || parse::default_effort(&OsEnv), parse::usage_cache_path(&OsEnv)));
        Self::with_store(store, settings, shown, changed, reserve)
    }

    pub fn with_store(
        store: Arc<StatusStore>,
        settings: KilnSettings,
        shown: bool,
        changed: impl Fn() + Send + Sync + 'static,
        reserve: impl Fn(&Meter) + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            shown: AtomicBool::new(shown),
            folders: Mutex::new(Vec::new()),
            settings: Mutex::new(settings),
            limits: Mutex::new(None),
        });
        let changed = Arc::new(changed);
        let (tx, rx) = channel::<Poke>();
        {
            let tx = Mutex::new(tx.clone());
            store.on_change(move |_| {
                let _ = lock(&tx).send(Poke::Status);
            });
        }
        let git = {
            let (changed, shared) = (changed.clone(), shared.clone());
            GitCache::new(INTERVAL, move |_, _| {
                if shared.shown.load(Ordering::Relaxed) {
                    changed();
                }
            })
        };
        {
            let (store, tx) = (store.clone(), tx.clone());
            let _ = std::thread::Builder::new().name("status store".into()).spawn(move || {
                let _ = store.start();
                let _ = tx.send(Poke::Status);
            });
        }
        {
            let (store, git, shared) = (store.clone(), git.clone(), shared.clone());
            let _ = std::thread::Builder::new().name("status clock".into()).spawn(move || {
                let mut watch = ReserveWatch::new();
                // Fresh limits, the reserve (Claude's limits only, as MainWindow.WarnReserve),
                // and a redraw while the window shows.
                let mut warn = |store: &StatusStore| {
                    let settings = lock(&shared.settings).clone();
                    let limits = store.limits();
                    for m in watch.crossed(limits.as_ref(), &settings, chrono::Utc::now()) {
                        reserve(&m);
                    }
                    *lock(&shared.limits) = limits;
                    if shared.shown.load(Ordering::Relaxed) {
                        changed();
                    }
                };
                let mut next_git = Instant::now();
                let mut next_clock = Instant::now() + CLOCK;
                loop {
                    let now = Instant::now();
                    let shown = shared.shown.load(Ordering::Relaxed);
                    // A hidden window shows no branch, so it reads none.
                    if shown && now >= next_git {
                        for f in lock(&shared.folders).iter() {
                            git.refresh(f);
                        }
                        next_git = now + INTERVAL;
                    }
                    if now >= next_clock {
                        warn(&store);
                        next_clock = now + CLOCK;
                    }
                    let until = if shown { next_git.min(next_clock) } else { next_clock };
                    match rx.recv_timeout(until.saturating_duration_since(Instant::now())) {
                        Ok(Poke::Status) => warn(&store),
                        Ok(Poke::Wake) => {}
                        Ok(Poke::Stop) | Err(RecvTimeoutError::Disconnected) => break,
                        Err(RecvTimeoutError::Timeout) => {}
                    }
                }
            });
        }
        Self { store, git, shared, tx }
    }

    /// The window was shown or hidden: the git poll runs only while it shows.
    pub fn set_shown(&self, shown: bool) {
        if self.shared.shown.swap(shown, Ordering::Relaxed) != shown && shown {
            let _ = self.tx.send(Poke::Wake);
        }
    }

    /// Claude's account limits, as of the last status change or clock tick.
    pub fn claude_limits(&self) -> Option<AccountLimits> {
        lock(&self.shared.limits).clone()
    }

    /// The open panes' working folders, polled for git.
    pub fn set_folders(&self, folders: Vec<String>) {
        *lock(&self.shared.folders) = folders;
    }

    /// Settings changed: the reserve is checked against them at once.
    #[allow(dead_code)] // The Settings page (phase 7) changes settings.
    pub fn set_settings(&self, s: KilnSettings) {
        *lock(&self.shared.settings) = s;
        let _ = self.tx.send(Poke::Status);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Poke::Stop);
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A status file past the reserve warns once, from the store's watcher, with no UI.
    #[test]
    fn reserve_crossing_reaches_the_island_once() {
        let dir = tempfile::tempdir().unwrap();
        let status = dir.path().join("status");
        let store = Arc::new(StatusStore::new(&status, || "medium".to_string(), dir.path().join("none.json")));
        let settings = KilnSettings { reserve_at: 80, ..KilnSettings::default() };
        let (tx, rx) = channel::<(String, i32)>();
        let tx = Mutex::new(tx);
        let s = Status::with_store(store, settings, false, || {}, move |m| {
            let _ = lock(&tx).send((m.label.clone(), m.percent));
        });
        std::thread::sleep(Duration::from_millis(300));
        let resets = chrono::Utc::now().timestamp() + 3600;
        let json = format!(
            r#"{{"session_id":"0f8e4c2a-1b3d-4e5f-8a9b-0c1d2e3f4a5b","rate_limits":{{"five_hour":{{"used_percentage":85,"resets_at":{resets}}}}}}}"#
        );
        std::fs::create_dir_all(&status).unwrap();
        std::fs::write(status.join("0f8e4c2a-1b3d-4e5f-8a9b-0c1d2e3f4a5b.json"), &json).unwrap();
        let got = rx.recv_timeout(Duration::from_secs(10)).expect("reserve warning");
        assert_eq!(got, ("5h".to_string(), 85));
        // A rewrite in the same window says nothing new. Only the percentage changes: a blanket
        // replace of "85" could also hit the resets_at timestamp and open a new window.
        std::fs::write(status.join("0f8e4c2a-1b3d-4e5f-8a9b-0c1d2e3f4a5b.json"), json.replace(r#""used_percentage":85"#, r#""used_percentage":86"#)).unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(1500)).is_err());
        s.stop();
    }
}
