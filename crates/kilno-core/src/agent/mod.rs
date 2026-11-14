//! The two agents Kilno runs, and how to find and start them.

pub mod claude;
pub mod codex;
pub mod detect;
pub mod helper;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AgentKind {
    Claude,
    Codex,
}

/// What setup found for one agent. `exe` is None when nothing was found or what was found does
/// not run. `npm` is for Codex's Install only (it installs through npm); None for Claude.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentInfo {
    pub kind: AgentKind,
    pub exe: Option<String>,
    pub version: Option<String>,
    pub signed_in: bool,
    pub data_folder: String,
    pub sessions: u32,
    pub projects: u32,
    pub npm: Option<String>,
}

impl AgentKind {
    pub fn name(self) -> &'static str {
        match self {
            AgentKind::Claude => "Claude Code",
            AgentKind::Codex => "Codex",
        }
    }

    /// Sidebar tag when both agents are in use.
    pub fn tag(self) -> &'static str {
        match self {
            AgentKind::Claude => "CC",
            AgentKind::Codex => "CX",
        }
    }
}
