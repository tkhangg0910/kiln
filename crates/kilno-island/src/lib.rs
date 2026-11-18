//! The island at the top of the screen (PORTING.md Appendix D): a layered Win32 window that
//! never takes focus, on its own STA thread with its own message loop, painted with tiny-skia.
//! The app drives it through an [`IslandHandle`] and answers its questions through
//! [`IslandHost`]; the decisions themselves are `kilno_core::island::trigger`.
#![deny(unsafe_code)]

pub mod anim;
pub mod dib;
pub mod glow;
pub mod mascot;
pub mod shots;
pub mod sys;
pub mod text;
pub mod view;

#[cfg(windows)]
mod island;

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub use kilno_core::island::trigger::{IslandNote, NoteKind, NoteSource};
pub use kilno_core::pane_status::PaneStatus;

/// One open session the picker offers (MainWindow.PickChoice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickChoice {
    /// The app's pane id.
    pub pane: u64,
    /// The sidebar row's name, or "new session".
    pub name: String,
    /// The project folder's name.
    pub folder: String,
    pub status: PaneStatus,
}

/// What the island asks of the app. Called on the island thread: answer quickly and never call
/// [`IslandHandle::set_settings`] from inside (it waits for the island thread).
pub trait IslandHost: Send + Sync {
    /// Live panes, for "N sessions".
    fn open_session_count(&self) -> usize;
    /// Up to `max` open panes, most recently used first.
    fn recent_sessions(&self, max: usize) -> Vec<PickChoice>;
    /// Show the main window (restore, activate; Setup first if needed).
    fn reveal(&self);
    /// Bring this pane to the front of the main window.
    fn show_session(&self, pane: u64);
    /// A screenshot saved at `path` for this pane, or for a new session (None). The app
    /// writes `ESC[200~path ESC[201~` into the pane, or uses it as the new pane's prefill.
    fn send_image(&self, pane: Option<u64>, path: PathBuf);
    /// A new session in `folder`, revealed; `prefill` typed into the prompt once ready.
    fn adopt_folder(&self, folder: PathBuf, prefill: Option<String>);
    /// Quit Kilno (the island's menu holds the only Quit once the window is hidden).
    fn quit(&self);
    /// "Start at login": (on, can this build change it).
    fn login_state(&self) -> (bool, bool);
    fn set_login(&self, on: bool);
    /// Kilno's main window is what you are looking at. [`window_in_front`] answers this from
    /// the main window's handle.
    fn kilno_active(&self) -> bool;
    /// Where screenshots go: `%LOCALAPPDATA%\Kilno\shots` by default.
    fn shots_dir(&self) -> PathBuf {
        let data = kilno_core::paths::data_dir(&kilno_core::paths::OsEnv);
        kilno_core::screenshots::dir(&data)
    }
}

/// `hwnd` (the main window) is visible, not minimized and in front, an owned window of it
/// such as Setup included. For [`IslandHost::kilno_active`].
pub fn window_in_front(hwnd: isize) -> bool {
    #[cfg(windows)]
    {
        sys::win::window_in_front(hwnd)
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd;
        false
    }
}

/// The note a pane's status change raises (MainWindow): NeedsInput → NeedsYou, Error → Error,
/// Done only right after Working; anything else resolves what it said before (None).
pub fn note_kind_for(previous: PaneStatus, now: PaneStatus) -> Option<NoteKind> {
    match now {
        PaneStatus::NeedsInput => Some(NoteKind::NeedsYou),
        PaneStatus::Error => Some(NoteKind::Error),
        PaneStatus::Done if previous == PaneStatus::Working => Some(NoteKind::Done),
        _ => None,
    }
}

// Off Windows only Notify and Settings' reply are acted on (desktop notifications).
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Cmd {
    Notify(IslandNote),
    Resolve(NoteSource),
    Working(NoteSource, bool),
    Greet,
    Settings {
        hotkey: bool,
        offer: bool,
        reply: Sender<bool>,
    },
    Shutdown,
}

struct Inner {
    tx: Mutex<Sender<Cmd>>,
    #[cfg_attr(not(windows), allow(dead_code))] // the island window, woken on Windows
    hwnd: isize,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// The app's side of the island. Cheap to clone; every call only queues a command for the
/// island thread and wakes it.
#[derive(Clone)]
pub struct IslandHandle {
    inner: Arc<Inner>,
}

/// WM_APP: commands are waiting.
#[cfg(windows)]
pub(crate) const WM_WAKE: u32 = 0x8000;

impl IslandHandle {
    fn send(&self, cmd: Cmd) {
        if let Ok(tx) = self.inner.tx.lock() {
            if tx.send(cmd).is_ok() {
                #[cfg(windows)]
                sys::win::post(self.inner.hwnd, WM_WAKE);
            }
        }
    }

    /// Queues a note; the next poll decides whether it may show. A newer note from the same
    /// source replaces the older one.
    pub fn notify(&self, note: IslandNote) {
        self.send(Cmd::Notify(note));
    }

    /// Whatever this source said is stale (session moved on or closed).
    pub fn resolve(&self, source: NoteSource) {
        self.send(Cmd::Resolve(source));
    }

    /// A pane started or stopped working (the compact self-peek).
    pub fn set_working(&self, pane: u64, working: bool) {
        self.send(Cmd::Working(NoteSource::Pane(pane), working));
    }

    /// A pane's status moved from `previous` to `now`: queues or resolves its note and its
    /// working mark, as MainWindow did.
    pub fn pane_status(&self, pane: u64, name: &str, previous: PaneStatus, now: PaneStatus) {
        match note_kind_for(previous, now) {
            Some(kind) => self.notify(IslandNote::new(NoteSource::Pane(pane), name, kind)),
            None => self.resolve(NoteSource::Pane(pane)),
        }
        if now == PaneStatus::Working {
            self.set_working(pane, true);
        } else if previous == PaneStatus::Working {
            self.set_working(pane, false);
        }
    }

    /// The pane closed.
    pub fn pane_closed(&self, pane: u64) {
        self.resolve(NoteSource::Pane(pane));
        self.set_working(pane, false);
    }

    /// An account limit crossed the reserve: "5h at 82%", replacing an older one for it.
    pub fn reserve(&self, label: &str, percent: u32) {
        self.notify(IslandNote::new(
            NoteSource::Limit(label.to_string()),
            format!("{label} at {percent}%"),
            NoteKind::Reserve,
        ));
    }

    /// Kilno started at login: the compact pill waves once.
    pub fn greet(&self) {
        self.send(Cmd::Greet);
    }

    /// Applies the screenshot settings (Ctrl+Alt+S, clipboard offer). Returns whether the
    /// hotkey is wanted but taken by another app (for Settings). Waits for the island thread.
    pub fn set_settings(&self, screenshot_hotkey: bool, screenshot_offer: bool) -> bool {
        let (reply, rx) = mpsc::channel();
        self.send(Cmd::Settings {
            hotkey: screenshot_hotkey,
            offer: screenshot_offer,
            reply,
        });
        rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap_or(false)
    }

    /// Closes the island: timers, hotkeys and the clipboard listener go, then the windows.
    /// Waits for the thread.
    pub fn shutdown(&self) {
        self.send(Cmd::Shutdown);
        let thread = self.inner.thread.lock().ok().and_then(|mut t| t.take());
        if let Some(t) = thread {
            if t.thread().id() != std::thread::current().id() {
                let _ = t.join();
            }
        }
    }
}

/// Starts the island.
pub struct Island;

impl Island {
    /// Spawns the island thread and its windows. On a system where they can't be made (or off
    /// Windows) the handle still works and does nothing.
    pub fn start(host: Arc<dyn IslandHost>) -> IslandHandle {
        let (tx, rx) = mpsc::channel::<Cmd>();
        #[cfg(windows)]
        {
            let (ready_tx, ready_rx) = mpsc::channel::<isize>();
            let thread = std::thread::Builder::new()
                .name("island".into())
                .spawn(move || island::run(host, rx, ready_tx))
                .ok();
            let hwnd = ready_rx.recv().unwrap_or(0);
            IslandHandle {
                inner: Arc::new(Inner {
                    tx: Mutex::new(tx),
                    hwnd,
                    thread: Mutex::new(thread),
                }),
            }
        }
        #[cfg(not(windows))]
        {
            // No island window here yet (PORTING-LINUX.md LNX-041/042): its notes become the
            // desktop's notifications (LNX-043), which every desktop shows, GNOME's Wayland too.
            let _ = host;
            let thread = std::thread::Builder::new().name("island".into()).spawn(move || notifications(rx)).ok();
            IslandHandle {
                inner: Arc::new(Inner {
                    tx: Mutex::new(tx),
                    hwnd: 0,
                    thread: Mutex::new(thread),
                }),
            }
        }
    }
}

/// Off Windows: each note the island would show is sent as a desktop notification (`notify-send`,
/// part of libnotify, on every common desktop). Tips and reserve warnings included; the urgent
/// ones (a question, an error) as critical so they stay until dismissed.
#[cfg(not(windows))]
fn notifications(rx: mpsc::Receiver<Cmd>) {
    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::Notify(note) => {
                let (summary, urgency) = match note.kind {
                    kilno_core::island::trigger::NoteKind::NeedsYou => ("needs you", "critical"),
                    kilno_core::island::trigger::NoteKind::Error => ("hit an error", "critical"),
                    kilno_core::island::trigger::NoteKind::Done => ("is done", "normal"),
                    kilno_core::island::trigger::NoteKind::Reserve => ("is near its limit", "normal"),
                    kilno_core::island::trigger::NoteKind::Tip => ("", "low"),
                };
                let title = format!("{} {summary}", note.name).trim().to_string();
                let _ = std::process::Command::new("notify-send")
                    .args(["--app-name=Kilno", "--icon=kilno", &format!("--urgency={urgency}"), "Kilno", &title])
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            // No hotkey off Windows yet (LNX-051): never taken.
            Cmd::Settings { reply, .. } => {
                let _ = reply.send(false);
            }
            Cmd::Shutdown => break,
            Cmd::Resolve(_) | Cmd::Working(..) | Cmd::Greet => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_status_to_note() {
        use PaneStatus::*;
        assert_eq!(note_kind_for(Working, NeedsInput), Some(NoteKind::NeedsYou));
        assert_eq!(note_kind_for(Starting, Error), Some(NoteKind::Error));
        assert_eq!(note_kind_for(Working, Done), Some(NoteKind::Done));
        assert_eq!(note_kind_for(NeedsInput, Done), None);
        assert_eq!(note_kind_for(Done, Working), None);
        assert_eq!(note_kind_for(Working, Exited), None);
    }
}
