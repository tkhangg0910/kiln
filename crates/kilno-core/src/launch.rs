//! What a pane runs: a PowerShell, then the agent's command typed into it. Spawning a shell
//! rather than the agent itself means that when the agent exits you land on a prompt instead
//! of the pane dying.

use crate::agent::AgentKind;
use crate::paths::Env;
use std::fmt;
use std::path::PathBuf;

/// What differs between Claude Code and Codex when starting one. The commands are typed into
/// the pane's PowerShell, so they are PowerShell command lines.
pub trait Agent {
    fn kind(&self) -> AgentKind;

    /// Claude takes the id we pick, so a new pane's row exists before the first message.
    /// Codex picks its own and Kilno finds it afterwards ([`crate::sessions::codex::find_started`]).
    /// When false, the commands get `None` for `new_id`.
    fn knows_id_upfront(&self) -> bool;

    fn new_command(&self, new_id: Option<&str>) -> String;

    fn resume_command(&self, id: &str) -> String;

    fn fork_command(&self, parent_id: &str, new_id: Option<&str>) -> String;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchMode {
    /// Fresh session in the folder.
    New,
    /// Continue an existing session. Never open the same id twice: transcripts interleave.
    Resume,
    /// Copy the conversation and diverge. The original is untouched.
    Fork,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchPlan {
    pub shell_command_line: String,
    pub working_directory: String,
    /// Typed into the shell (followed by Enter) once it is up.
    pub agent_command: String,
    /// The id the pane is keyed on: the resumed id, or the one Kilno picked (None for Codex
    /// new/fork, which picks its own).
    pub expected_session_id: Option<String>,
    pub agent: AgentKind,
}

/// Resume or fork asked for without a session id.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MissingSessionId(pub LaunchMode);

impl fmt::Display for MissingSessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} needs a session id.", self.0)
    }
}

impl std::error::Error for MissingSessionId {}

/// Builds the plan for a pane. `env` supplies the PATH that picks pwsh over Windows PowerShell.
pub fn plan(
    agent: &dyn Agent,
    mode: LaunchMode,
    working_directory: &str,
    session_id: Option<&str>,
    env: &dyn Env,
) -> Result<LaunchPlan, MissingSessionId> {
    let id = session_id.filter(|s| !s.trim().is_empty());
    let id = match (mode, id) {
        (LaunchMode::New, _) => "",
        (_, Some(id)) => id,
        (_, None) => return Err(MissingSessionId(mode)),
    };

    // Allocate the id up front when the agent lets us. Without it a brand-new pane has nothing
    // to key on until its transcript appears, so it cannot be renamed, tracked or told apart
    // from another new pane. For a fork it is the whole trick: the branch is drawn at once.
    let new_id = (agent.knows_id_upfront() && mode != LaunchMode::Resume).then(crate::text::new_guid);

    let agent_command = match mode {
        LaunchMode::New => agent.new_command(new_id.as_deref()),
        LaunchMode::Resume => agent.resume_command(id),
        LaunchMode::Fork => agent.fork_command(id, new_id.as_deref()),
    };

    // Resume allocates nothing: the existing id is the one the pane has to be keyed on, or its
    // sidebar row never learns it is open.
    let expected_session_id = if mode == LaunchMode::Resume { Some(id.to_string()) } else { new_id };

    Ok(LaunchPlan {
        shell_command_line: resolve_shell(env.var("PATH").as_deref()),
        working_directory: working_directory.to_string(),
        agent_command,
        expected_session_id,
        agent: agent.kind(),
    })
}

/// `"<pwsh.exe>" -NoLogo` when PowerShell 7 is on PATH, else Windows PowerShell.
#[cfg(windows)]
pub fn resolve_shell(path: Option<&str>) -> String {
    match find_on_path("pwsh.exe", path) {
        Some(pwsh) => format!("\"{}\" -NoLogo", pwsh.display()),
        None => "powershell.exe -NoLogo".to_string(),
    }
}

/// The user's login shell (`$SHELL`), else bash, else sh, started as a login shell (`-l`) so
/// it has the PATH the user's profile sets up, where the agents usually are. The program comes
/// first, single-quoted when it has a space ([`kilno_pty`] splits on that).
#[cfg(not(windows))]
pub fn resolve_shell(_path: Option<&str>) -> String {
    let shell = std::env::var("SHELL")
        .ok()
        .filter(|s| std::path::Path::new(s).is_file())
        .or_else(|| ["/bin/bash", "/usr/bin/bash", "/bin/sh"].iter().find(|s| std::path::Path::new(s).is_file()).map(|s| s.to_string()))
        .unwrap_or_else(|| "/bin/sh".to_string());
    format!("{} -l", crate::text::sh_quote(&shell))
}

/// The PATH list separator: `;` on Windows, `:` elsewhere.
pub const PATH_SEP: char = if cfg!(windows) { ';' } else { ':' };

/// The first PATH folder holding `exe`: entries split on [`PATH_SEP`], empty ones dropped,
/// quotes stripped.
pub fn find_on_path(exe: &str, path: Option<&str>) -> Option<PathBuf> {
    path?
        .split(PATH_SEP)
        .filter(|d| !d.is_empty())
        .map(|d| PathBuf::from(d.trim_matches('"')).join(exe))
        .find(|c| c.is_file())
}

/// Session markers Claude Code puts on its child processes.
pub const INHERITED_SESSION_VARS: [&str; 8] = [
    "CLAUDECODE",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
    "CLAUDE_CODE_MESSAGING_SOCKET",
    "CLAUDE_CODE_MESSAGING_TOKEN",
    "CLAUDE_PID",
];

/// What Claude Code sets for the commands its tools run, so their output is plain and they never
/// wait on a person: no colour, no editor for git, no git credential prompt. Right for a tool
/// call, wrong for an interactive pane (a colourless claude, `git commit` that never opens the
/// editor), so they go too, but only when Kilno itself was started from a Claude Code session:
/// a NO_COLOR the user set on purpose stays.
pub const INHERITED_TOOL_VARS: [&str; 4] = ["NO_COLOR", "GIT_EDITOR", "GIT_TERMINAL_PROMPT", "GIT_ASKPASS"];

/// Removes [`INHERITED_SESSION_VARS`] from this process's environment, and
/// [`INHERITED_TOOL_VARS`] when it came from a Claude Code session. Panes inherit it, so if
/// Kilno was started from inside a Claude Code session those markers would reach the claude it
/// spawns; CLAUDE_CODE_CHILD_SESSION alone switches transcript saving off, leaving the session
/// invisible to the sidebar and to --resume. Configuration (ANTHROPIC_*, CLAUDE_EFFORT) stays.
/// Call once at startup, before any other thread reads the environment.
pub fn scrub_inherited_session() {
    let from_claude = std::env::var_os("CLAUDECODE").is_some();
    for name in INHERITED_SESSION_VARS {
        std::env::remove_var(name);
    }
    if from_claude {
        for name in INHERITED_TOOL_VARS {
            std::env::remove_var(name);
        }
    }
}

/// Folder a resumed session opens in: its own if it still exists, else the app's.
pub fn resume_folder(session_folder: &str, current: &str) -> String {
    if std::path::Path::new(session_folder).is_dir() { session_folder } else { current }.to_string()
}

/// Pane title until the session has a name: `new · <folder leaf>`.
pub fn new_title(folder: &str) -> String {
    format!("new \u{b7} {}", crate::paths::leaf(folder))
}

/// Pane title for a branch until it has a name: `branch of <first 8 chars of the parent id>`.
pub fn branch_title(parent_id: &str) -> String {
    format!("branch of {}", parent_id.chars().take(8).collect::<String>())
}
