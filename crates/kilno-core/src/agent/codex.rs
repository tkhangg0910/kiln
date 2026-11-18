//! Codex's command lines. Codex picks its own session ids, so a new pane or a fork is bound to
//! its transcript once that appears ([`crate::sessions::codex::find_started`]). No `-C`: the
//! pane's shell already starts in the folder.

use super::AgentKind;
use crate::launch::Agent;
use crate::paths::Env;
use crate::text::{shell_quote, shell_run};

/// Codex as one pane launches it. Build a fresh one for every launch, as for Claude.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexAgent {
    /// Full path where found; "codex" when nothing was found.
    pub exe: String,
}

impl CodexAgent {
    pub fn new(exe: impl Into<String>) -> Self {
        Self { exe: exe.into() }
    }

    pub fn for_this_user(env: &dyn Env, saved_path: Option<&str>) -> Self {
        let exe = super::detect::locate(env, AgentKind::Codex, saved_path);
        Self::new(exe.unwrap_or_else(|| "codex".to_string()))
    }

    fn exe(&self) -> String {
        shell_run(&self.exe)
    }
}

impl Agent for CodexAgent {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }

    fn knows_id_upfront(&self) -> bool {
        false
    }

    fn new_command(&self, _new_id: Option<&str>) -> String {
        self.exe()
    }

    // An id is read from transcript contents, so it is quoted like any other text.
    fn resume_command(&self, id: &str) -> String {
        format!("{} resume {}", self.exe(), shell_quote(id))
    }

    fn fork_command(&self, parent_id: &str, _new_id: Option<&str>) -> String {
        format!("{} fork {}", self.exe(), shell_quote(parent_id))
    }
}
