//! The app's side of the island (App.xaml.cs / MainWindow's island members): what it asks of
//! the main window. Called on the island thread, so anything that touches panes is handed to
//! the UI as an [`AppEvent::Island`], handled in `App::logic` (which runs while hidden too).

use crate::app::Revealer;
use crate::model::{lock, Shared};
use crate::pane::{AppEvent, Waker};
use kilno_core::paths::DataDir;
use kilno_core::system::login_startup::LoginStartup;
use kilno_island::{IslandHost, PickChoice};
use std::path::PathBuf;

/// What the island asks the UI thread to do.
#[derive(Debug)]
pub enum IslandCmd {
    ShowSession(u64),
    SendImage(Option<u64>, PathBuf),
    Adopt(PathBuf, Option<String>),
    Quit,
}

pub struct Host {
    pub model: Shared,
    pub revealer: Revealer,
    pub waker: Waker,
    pub data: DataDir,
    /// The main window, for "Kilno is in front".
    pub hwnd: isize,
}

impl Host {
    fn ask(&self, cmd: IslandCmd) {
        self.waker.send(AppEvent::Island(cmd));
    }
}

impl IslandHost for Host {
    fn open_session_count(&self) -> usize {
        lock(&self.model).panes.len()
    }

    fn recent_sessions(&self, max: usize) -> Vec<PickChoice> {
        let m = lock(&self.model);
        let open: Vec<_> = m.panes.iter().filter_map(|p| m.sidebar.by_pane(p.id).and_then(|r| r.pane.clone())).collect();
        m.sidebar
            .recent_sessions(&m.used, &open, max)
            .into_iter()
            .map(|c| PickChoice { pane: c.pane_id, name: c.name, folder: c.folder, status: c.status })
            .collect()
    }

    fn reveal(&self) {
        self.revealer.reveal();
    }

    fn show_session(&self, pane: u64) {
        self.ask(IslandCmd::ShowSession(pane));
    }

    fn send_image(&self, pane: Option<u64>, path: PathBuf) {
        self.ask(IslandCmd::SendImage(pane, path));
    }

    fn adopt_folder(&self, folder: PathBuf, prefill: Option<String>) {
        self.ask(IslandCmd::Adopt(folder, prefill));
    }

    fn quit(&self) {
        self.ask(IslandCmd::Quit);
    }

    fn login_state(&self) -> (bool, bool) {
        let s = LoginStartup::for_this_user(&self.data);
        let on = s.is_on();
        (on, s.can_write())
    }

    fn set_login(&self, on: bool) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = LoginStartup::for_this_user(&self.data).set(on, &exe);
        }
    }

    fn kilno_active(&self) -> bool {
        kilno_island::window_in_front(self.hwnd)
    }
}
