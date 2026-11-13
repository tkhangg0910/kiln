//! The update check (Updater.NewerAsync + MainWindow's 12 h timer) on its own thread, so it
//! runs while the window is hidden and never blocks a frame. A found release reaches the app as
//! [`AppEvent::UpdateFound`]; installing it is the app's (UpdateWhenIdle).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::Duration;

use kiln_core::system::updater::{self, Version};

use crate::pane::{AppEvent, Waker};

pub struct Updater {
    on: Arc<AtomicBool>,
    kick: Sender<()>,
}

impl Updater {
    /// Checks at once (when on) and every 12 h after. A dev build never asks (updater::ENABLED).
    pub fn start(on: bool, waker: Waker) -> Self {
        let on = Arc::new(AtomicBool::new(on));
        let (kick, rx) = channel::<()>();
        let flag = on.clone();
        if updater::ENABLED {
            let _ = std::thread::Builder::new().name("kiln-update".into()).spawn(move || loop {
                if flag.load(Ordering::SeqCst) {
                    if let Some(v) = check() {
                        // Checked again after: the switch may have gone off while GitHub answered.
                        if flag.load(Ordering::SeqCst) {
                            waker.send(AppEvent::UpdateFound(v));
                        }
                    }
                }
                match rx.recv_timeout(updater::CHECK_EVERY) {
                    Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            });
        }
        Self { on, kick }
    }

    /// Settings' "Check for updates": on asks right away; off stops asking (the app forgets
    /// the offer).
    pub fn set_enabled(&self, on: bool) {
        let was = self.on.swap(on, Ordering::SeqCst);
        if on && !was {
            self.check_now();
        }
    }

    /// Another check now (after an update was called off).
    pub fn check_now(&self) {
        let _ = self.kick.send(());
    }
}

/// GitHub's latest release, when it is newer than this build. Any failure is "nothing new".
fn check() -> Option<Version> {
    #[cfg(windows)]
    {
        let (status, body) =
            crate::win::http::get(&updater::latest_url(), updater::USER_AGENT, Duration::from_secs(updater::TIMEOUT_SECS))
                .ok()?;
        if status != 200 {
            return None;
        }
        updater::newer_from_json(&String::from_utf8_lossy(&body), updater::current())
    }
    #[cfg(not(windows))]
    {
        let _ = Duration::ZERO;
        None
    }
}

/// What the Update button shows: the offer, then the wait once pressed.
#[derive(Default)]
pub struct UpdateState {
    /// The release the button installs, from the last check.
    pub to: Option<Version>,
    /// Pressed while a session was busy: it goes once none is.
    pub waiting: bool,
    /// The "Update Kiln" confirm box is up.
    pub confirming: bool,
}

impl UpdateState {
    /// The button's (text, tooltip), or None when there is nothing to offer.
    pub fn button(&self) -> Option<(String, String)> {
        let to = self.to?;
        Some(if self.waiting {
            (updater::WAITING_TEXT.to_string(), updater::WAITING_TOOLTIP.to_string())
        } else {
            (updater::button_text(to), updater::button_tooltip(to, updater::current()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_offers_then_waits() {
        let mut s = UpdateState::default();
        assert!(s.button().is_none());
        s.to = Some(Version::new(9, 1, 2));
        let (text, tip) = s.button().unwrap();
        assert_eq!(text, "Update to 9.1.2");
        assert!(tip.starts_with("Kiln 9.1.2 is out (you have "));
        s.waiting = true;
        assert_eq!(s.button().unwrap().0, "Update waits for running sessions");
    }
}
