//! The status wiring (MainWindow's ShowStatusBar / WarnReserve and its timers), off the UI
//! thread: the status store follows `%LOCALAPPDATA%\Kilno\status` on its watcher; one clock
//! thread polls git every 3 s for the open panes' folders while the window shows, and checks
//! the reserve every 30 s (hidden too: a script's cached limits change between reports) and on
//! every status change. With live usage on, a third thread asks Anthropic's usage endpoint
//! ([`kilno_core::status::live`]) on a click, after a Claude reply and on its interval while
//! the window shows. The UI only reads what these hold.

use kilno_core::git::{GitCache, INTERVAL};
use kilno_core::paths::{DataDir, OsEnv};
use kilno_core::settings::KilnoSettings;
use kilno_core::status::reserve::ReserveWatch;
use kilno_core::status::store::StatusStore;
use kilno_core::status::strip::Meter;
use kilno_core::status::{claude_usage, live, parse, relay, AccountLimits};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The clock: countdowns and staleness move, and the reserve is looked at, this often.
pub const CLOCK: Duration = Duration::from_secs(30);

/// What the live usage thread is told.
enum Ask {
    /// A Claude session reported (it replied).
    Reply,
    /// The limits were clicked.
    Click,
    /// The window was shown, or a setting changed.
    Wake,
    Stop,
}

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
    settings: Mutex<KilnoSettings>,
    /// Claude's account limits as of the last status change or clock tick: reading them reads
    /// the user's script's cache file, so the UI never does that per frame.
    limits: Mutex<Option<AccountLimits>>,
}

pub struct Status {
    pub store: Arc<StatusStore>,
    pub git: GitCache,
    shared: Arc<Shared>,
    tx: Sender<Poke>,
    live: Sender<Ask>,
}

impl Status {
    /// Starts the store (on its own thread: it reads and prunes the folder first) and the
    /// clock. `changed` runs on a background thread when something the strip shows changed;
    /// `reserve` for each limit that crossed the reserve.
    pub fn start(
        data: &DataDir,
        settings: KilnoSettings,
        shown: bool,
        changed: impl Fn() + Send + Sync + 'static,
        reserve: impl Fn(&Meter) + Send + 'static,
    ) -> Self {
        let dir = relay::status_dir(data);
        let store = StatusStore::new(dir, || parse::default_effort(&OsEnv), parse::usage_cache_path(&OsEnv))
            .with_claude_json(claude_usage::path(&OsEnv));
        let store = Arc::new(store);
        Self::with_store(store, settings, shown, changed, reserve)
    }

    pub fn with_store(
        store: Arc<StatusStore>,
        settings: KilnoSettings,
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
        let (live_tx, live_rx) = channel::<Ask>();
        {
            let tx = Mutex::new(tx.clone());
            let live_tx = Mutex::new(live_tx.clone());
            store.on_change(move |_| {
                let _ = lock(&tx).send(Poke::Status);
                let _ = lock(&live_tx).send(Ask::Reply);
            });
        }
        {
            let (store, shared, tx) = (store.clone(), shared.clone(), tx.clone());
            let _ = std::thread::Builder::new()
                .name("live usage".into())
                .spawn(move || live_loop(&store, &shared, &live_rx, &tx));
        }
        {
            // Claude Code saved a new usage reply: show it now, not on the next tick.
            let tx = Mutex::new(tx.clone());
            store.on_limits_change(move || {
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
        Self { store, git, shared, tx, live: live_tx }
    }

    /// The window was shown or hidden: the git poll runs only while it shows.
    pub fn set_shown(&self, shown: bool) {
        if self.shared.shown.swap(shown, Ordering::Relaxed) != shown && shown {
            let _ = self.tx.send(Poke::Wake);
            let _ = self.live.send(Ask::Wake);
        }
    }

    /// Reads the limits again now (the footer was clicked).
    pub fn refresh(&self) {
        let _ = self.tx.send(Poke::Status);
        let _ = self.live.send(Ask::Click);
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
    pub fn set_settings(&self, s: KilnoSettings) {
        *lock(&self.shared.settings) = s;
        let _ = self.tx.send(Poke::Status);
        let _ = self.live.send(Ask::Wake);
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Poke::Stop);
        let _ = self.live.send(Ask::Stop);
    }
}

/// Live usage: asks the endpoint when switched on, on a click (at most every
/// [`live::AFTER_CLICK`]), after a reply ([`live::AFTER_REPLY`]) and every
/// `live_usage_minutes` while the window shows. After a failure it waits
/// [`live::AFTER_FAILURE`] unless clicked. Switched off, it forgets the last reply.
fn live_loop(store: &StatusStore, shared: &Shared, rx: &Receiver<Ask>, poke: &Sender<Poke>) {
    let mut last: Option<Instant> = None;
    let mut failed = false;
    let mut was_on = false;
    loop {
        let s = lock(&shared.settings).clone();
        let every = Duration::from_secs(u64::try_from(s.live_usage_minutes.max(1)).unwrap_or(1) * 60);
        let gap = if failed { every.max(live::AFTER_FAILURE) } else { every };
        let due_at = last.map(|t| t + gap);
        let shown = shared.shown.load(Ordering::Relaxed);
        // Off, or hidden: nothing on a timer, only what it is told.
        let ask = if s.live_usage && shown {
            let wait = due_at.map_or(Duration::ZERO, |d| d.saturating_duration_since(Instant::now()));
            match rx.recv_timeout(wait) {
                Ok(a) => Some(a),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(a) => Some(a),
                Err(_) => return,
            }
        };
        let on = lock(&shared.settings).live_usage;
        if was_on && !on {
            store.set_live(None);
            last = None;
            failed = false;
            let _ = poke.send(Poke::Status);
        }
        was_on = on;
        let since = last.map(|t| t.elapsed());
        let go = on
            && match ask {
                Some(Ask::Stop) => return,
                None => true,
                Some(Ask::Wake) => due_at.is_none_or(|d| Instant::now() >= d),
                Some(Ask::Reply) => !failed && since.is_none_or(|d| d >= live::AFTER_REPLY),
                Some(Ask::Click) => since.is_none_or(|d| d >= live::AFTER_CLICK),
            };
        if !go {
            continue;
        }
        last = Some(Instant::now());
        match fetch_live() {
            Some(l) => {
                failed = false;
                store.set_live(Some(l));
                let _ = poke.send(Poke::Status);
            }
            None => failed = true,
        }
    }
}

/// One request to the usage endpoint with the Claude sign-in's token. None when signed out, the
/// token has expired, or the request or its reply fails.
#[cfg(windows)]
fn fetch_live() -> Option<AccountLimits> {
    let token = {
        let json = std::fs::read(live::credentials_path(&OsEnv)).ok()?;
        live::read_token(&String::from_utf8_lossy(&json), chrono::Utc::now())?
    };
    let (status, body) = crate::win::http::get_with(
        live::URL,
        kilno_core::system::updater::USER_AGENT,
        &token.headers(),
        Duration::from_secs(15),
    )
    .ok()?;
    drop(token);
    (status == 200).then(|| live::parse_reply(&String::from_utf8_lossy(&body), chrono::Utc::now()))?
}

#[cfg(not(windows))]
fn fetch_live() -> Option<AccountLimits> {
    None
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
        let settings = KilnoSettings { reserve_at: 80, ..KilnoSettings::default() };
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
