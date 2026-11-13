//! Drives the island with fake sessions: the login hello, a session that needs you, one that
//! finishes, a session at work, and a reserve warning. Rest the cursor at the top centre of the
//! screen to peek; Ctrl+Alt+S with an image on the clipboard brings the picker; drag a file to
//! the top edge for the drop target.
//!
//! `cargo run -p kiln-island --example island_demo [seconds]` (default 20).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use kiln_island::{IslandHost, IslandNote, Island, NoteKind, NoteSource, PaneStatus, PickChoice};

struct FakeHost;

impl IslandHost for FakeHost {
    fn open_session_count(&self) -> usize {
        2
    }
    fn recent_sessions(&self, max: usize) -> Vec<PickChoice> {
        vec![
            PickChoice { pane: 1, name: "api-refactor".into(), folder: "kiln".into(), status: PaneStatus::Working },
            PickChoice { pane: 2, name: "docs-pass".into(), folder: "website".into(), status: PaneStatus::Done },
        ]
        .into_iter()
        .take(max)
        .collect()
    }
    fn reveal(&self) {
        println!("host: reveal");
    }
    fn show_session(&self, pane: u64) {
        println!("host: show session {pane}");
    }
    fn send_image(&self, pane: Option<u64>, path: PathBuf) {
        println!("host: send image {} to {pane:?}", path.display());
    }
    fn adopt_folder(&self, folder: PathBuf, prefill: Option<String>) {
        println!("host: adopt {} prefill {prefill:?}", folder.display());
    }
    fn quit(&self) {
        println!("host: quit");
    }
    fn login_state(&self) -> (bool, bool) {
        (true, true)
    }
    fn set_login(&self, on: bool) {
        println!("host: start at login = {on}");
    }
    fn kiln_active(&self) -> bool {
        false
    }
    fn shots_dir(&self) -> PathBuf {
        std::env::temp_dir().join("kiln-island-demo-shots")
    }
}

fn main() {
    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(20);
    let island = Island::start(Arc::new(FakeHost));
    let taken = island.set_settings(true, false);
    println!("Ctrl+Alt+S taken: {taken}");
    island.greet();

    let steps: Vec<(u64, Box<dyn Fn()>)> = vec![
        (3, Box::new({
            let i = island.clone();
            move || i.notify(IslandNote::new(NoteSource::Pane(1), "api-refactor", NoteKind::NeedsYou))
        })),
        (7, Box::new({
            let i = island.clone();
            move || i.pane_status(2, "docs-pass", PaneStatus::Working, PaneStatus::Done)
        })),
        (9, Box::new({
            let i = island.clone();
            move || i.resolve(NoteSource::Pane(1))
        })),
        (14, Box::new({
            let i = island.clone();
            move || i.reserve("5h", 82)
        })),
        (16, Box::new({
            let i = island.clone();
            move || i.resolve(NoteSource::Limit("5h".into()))
        })),
        (17, Box::new({
            let i = island.clone();
            move || i.set_working(3, true)
        })),
    ];
    let mut at = 0;
    for (t, step) in steps {
        if t >= seconds {
            break;
        }
        std::thread::sleep(Duration::from_secs(t - at));
        at = t;
        step();
    }
    std::thread::sleep(Duration::from_secs(seconds.saturating_sub(at)));
    island.shutdown();
    println!("island closed");
}
