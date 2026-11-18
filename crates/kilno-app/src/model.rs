//! The sidebar's data, shared between the UI and the sessions thread.
//!
//! The sessions thread owns the agents' session stores and their watchers. A settled change
//! rescans on that thread and folds the result in here (RefreshSessions), so rows, Codex pane
//! binding and Codex turn status keep up while the window is hidden and eframe calls no `ui`.

use crate::pane::PaneShared;
use kilno_core::agent::AgentKind;
use kilno_core::paths::{DataDir, OsEnv};
use kilno_core::sessions::claude::ClaudeStore;
use kilno_core::sessions::codex::CodexStore;
use kilno_core::sessions::opencode::OpenCodeStore;
use kilno_core::sessions::watch::SessionSource;
use kilno_core::sessions::SessionInfo;
use kilno_core::sidebar::{bind_codex_panes, Sidebar, Waiting};
use kilno_core::store::names::SessionNames;
use kilno_core::store::hidden::HiddenSessions;
use kilno_core::store::order::SessionOrder;
use kilno_core::store::parents::SessionParents;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Model {
    pub sidebar: Sidebar,
    pub names: SessionNames,
    pub parents: SessionParents,
    /// Sessions the user hid from the list (`hidden.json`).
    pub hidden: HiddenSessions,
    /// The order sessions were dragged into (`order.json`).
    pub order: SessionOrder,
    /// Codex and OpenCode panes waiting for their session, in start order.
    pub waiting: Vec<Waiting>,
    /// Open panes, the one used last first (the screenshot picker's order).
    pub used: Vec<u64>,
    /// Live panes, in start order.
    pub panes: Vec<Arc<PaneShared>>,
    /// Some agent's sessions exist (else the list shows the empty-state text).
    pub roots_exist: bool,
    /// The agents in use (Settings › Agents), in sidebar order.
    pub agents: Vec<AgentKind>,
    /// Bumped on every change that can move rows, so the UI rebuilds its view only then.
    pub generation: u64,
}

impl Model {
    pub fn new(data: &DataDir, agents: Vec<AgentKind>) -> Self {
        Self {
            sidebar: Sidebar::new(),
            names: SessionNames::open(data),
            parents: SessionParents::open(data),
            hidden: HiddenSessions::open(data),
            order: SessionOrder::open(data),
            waiting: Vec::new(),
            used: Vec::new(),
            panes: Vec::new(),
            roots_exist: true,
            agents,
            generation: 0,
        }
    }

    pub fn pane(&self, id: u64) -> Option<&Arc<PaneShared>> {
        self.panes.iter().find(|p| p.id == id)
    }

    /// RefreshSessions: bind waiting Codex and OpenCode panes first, so a session lands on its
    /// pane's row instead of growing a second one; feed their turns to their panes; fold
    /// everything in. `scans` holds the sessions of the agents in use only.
    pub fn apply_scan(&mut self, scans: Vec<SessionInfo>, roots_exist: bool) {
        let mut taken: HashSet<String> = self.panes.iter().filter_map(|p| p.session_id()).collect();
        let picks_own_id: Vec<SessionInfo> = scans.iter().filter(|i| i.agent != AgentKind::Claude).cloned().collect();
        for (old, new) in bind_codex_panes(&mut self.waiting, &picks_own_id, &mut taken) {
            let pane_id = self.sidebar.get(&old).and_then(|r| r.pane.as_ref()).map(|p| p.id);
            self.sidebar.rekey(&old, &new);
            if let Some(p) = pane_id.and_then(|id| self.pane(id)) {
                p.set_session_id(Some(new));
            }
        }
        // A Codex or OpenCode pane's working / done is exact in what the agent stored.
        for info in &picks_own_id {
            let Some(turn) = info.turn else { continue };
            let pane = self.sidebar.get(&info.session_id).and_then(|r| r.pane.as_ref()).map(|p| p.id);
            if let Some(p) = pane.and_then(|id| self.pane(id)) {
                p.show_turn(turn, info.last_activity);
            }
        }
        // The agent's mark on each row only tells rows apart when more than one is in use.
        let several = self.agents.len() > 1;
        let names = &self.names;
        self.sidebar.fold(scans, several, |id| names.get(id).map(String::from));
        self.roots_exist = roots_exist;
        self.generation += 1;
    }
}

pub type Shared = Arc<Mutex<Model>>;

pub fn lock(m: &Shared) -> MutexGuard<'_, Model> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the sessions thread is asked to do.
pub enum Job {
    Refresh,
    Stop,
}

/// Starts the sessions thread: an immediate scan, then one per settled watcher change or
/// [`Job::Refresh`]. `changed` runs after each fold (it wakes the UI).
pub fn start_worker(model: Shared, changed: impl Fn() + Send + 'static) -> Sender<Job> {
    let (tx, rx) = channel::<Job>();
    let me = tx.clone();
    std::thread::Builder::new()
        .name("kilno-sessions".into())
        .spawn(move || {
            let env = OsEnv;
            let mut sources: Vec<(AgentKind, Box<dyn SessionSource>)> = vec![
                (AgentKind::Claude, Box::new(ClaudeStore::for_this_user(&env))),
                (AgentKind::Codex, Box::new(CodexStore::for_this_user(&env))),
                (AgentKind::OpenCode, Box::new(OpenCodeStore::for_this_user(&env))),
            ];
            let _ = me.send(Job::Refresh);
            for (_, source) in &mut sources {
                let me = me.clone();
                source.start_watching(Box::new(move || {
                    let _ = me.send(Job::Refresh);
                }));
            }
            while let Ok(job) = rx.recv() {
                if matches!(job, Job::Stop) {
                    break;
                }
                // Several changes queued while scanning are one refresh.
                let mut stop = false;
                while let Ok(more) = rx.try_recv() {
                    stop |= matches!(more, Job::Stop);
                }
                if stop {
                    break;
                }
                let agents = lock(&model).agents.clone();
                let mut scans = Vec::new();
                for (kind, source) in &mut sources {
                    if agents.contains(kind) {
                        scans.extend(source.scan());
                    }
                }
                let roots = sources.iter().any(|(_, s)| s.root_exists());
                lock(&model).apply_scan(scans, roots);
                // Each open pane's transcript, from where the last read stopped, off the lock.
                let reads: Vec<(Arc<PaneShared>, std::path::PathBuf)> = {
                    let m = lock(&model);
                    m.panes
                        .iter()
                        .filter_map(|p| {
                            let file = m.sidebar.by_pane(p.id)?.info.as_ref()?.file.clone()?;
                            Some((p.clone(), file))
                        })
                        .collect()
                };
                for (pane, file) in reads {
                    pane.read_transcript(&file);
                }
                changed();
            }
            for (_, source) in &mut sources {
                source.stop_watching();
            }
        })
        .expect("spawn sessions thread");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// The sessions thread scans and folds with no UI at all (a hidden window calls no `ui`),
    /// and a transcript written later reaches the rows through the watcher.
    #[test]
    fn worker_refreshes_rows_without_the_ui() {
        let dir = tempfile::tempdir().unwrap();
        let projects = dir.path().join("claude").join("projects").join("C--x");
        std::fs::create_dir_all(&projects).unwrap();
        let line = |id: &str| {
            format!(r#"{{"type":"user","cwd":"C:\\x","sessionId":"{id}","message":{{"content":"hello {id}"}}}}"#)
        };
        std::fs::write(projects.join("a.jsonl"), line("aaaa")).unwrap();
        // The only test in this crate touching the environment.
        std::env::set_var("CLAUDE_CONFIG_DIR", dir.path().join("claude"));
        std::env::set_var("CODEX_HOME", dir.path().join("codex"));
        let model: Shared = Arc::new(Mutex::new(Model::new(&DataDir::new(dir.path().join("data")), vec![kilno_core::agent::AgentKind::Claude, kilno_core::agent::AgentKind::Codex])));
        let (tx, rx) = channel();
        let jobs = start_worker(model.clone(), move || {
            let _ = tx.send(());
        });
        rx.recv_timeout(Duration::from_secs(10)).expect("first scan");
        assert_eq!(lock(&model).sidebar.get("aaaa").map(|r| r.name().to_string()).as_deref(), Some("hello aaaa"));
        assert!(lock(&model).roots_exist);
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(projects.join("b.jsonl"), line("bbbb")).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while lock(&model).sidebar.get("bbbb").is_none() {
            assert!(std::time::Instant::now() < deadline, "watcher refresh");
            let _ = rx.recv_timeout(Duration::from_millis(500));
        }
        let _ = jobs.send(Job::Stop);
    }
}
