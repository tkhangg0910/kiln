//! Claude Code's command lines. Single quotes throughout, for the pane's shell (PowerShell on
//! Windows, a POSIX shell elsewhere: [`crate::text::shell_quote`]).

use super::AgentKind;
use crate::launch::Agent;
use crate::paths::Env;
use crate::text::{shell_quote, shell_run};
use std::path::{Path, PathBuf};

/// Claude Code as one pane launches it. Build a fresh one for every launch, so an install or a
/// Locate… made while Kilno runs is the one used, and the relay settings follow the user's own
/// status line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClaudeAgent {
    /// Full path where found, so a PATH that differs between Explorer and the shell can't break
    /// a pane; "claude" when nothing was found.
    pub exe: String,
    /// The `--settings` file for Kilno's status line relay and ask rules (this pane only; the
    /// user's own settings.json is never written), or None.
    pub relay_settings: Option<PathBuf>,
}

impl ClaudeAgent {
    pub fn new(exe: impl Into<String>, relay_settings: Option<&Path>) -> Self {
        Self { exe: exe.into(), relay_settings: relay_settings.map(Path::to_path_buf) }
    }

    /// Located only, not run: setup is what checks that it works.
    pub fn for_this_user(env: &dyn Env, saved_path: Option<&str>, relay_settings: Option<&Path>) -> Self {
        let exe = super::detect::locate(env, AgentKind::Claude, saved_path);
        Self::new(exe.unwrap_or_else(|| "claude".to_string()), relay_settings)
    }

    fn command(&self, args: &str) -> String {
        let mut command = format!("{} {args}", shell_run(&self.exe));
        if let Some(settings) = &self.relay_settings {
            command += &format!(" --settings {}", shell_quote(&settings.to_string_lossy()));
        }
        command
    }
}

/// An id is read from transcript contents, so it is quoted like any other text.
fn id(id: Option<&str>) -> String {
    shell_quote(id.unwrap_or(""))
}

impl Agent for ClaudeAgent {
    fn kind(&self) -> AgentKind {
        AgentKind::Claude
    }

    fn knows_id_upfront(&self) -> bool {
        true
    }

    fn new_command(&self, new_id: Option<&str>) -> String {
        self.command(&format!("--session-id {}", id(new_id)))
    }

    fn resume_command(&self, session_id: &str) -> String {
        self.command(&format!("--resume {}", id(Some(session_id))))
    }

    fn fork_command(&self, parent_id: &str, new_id: Option<&str>) -> String {
        self.command(&format!("--resume {} --fork-session --session-id {}", id(Some(parent_id)), id(new_id)))
    }
}
