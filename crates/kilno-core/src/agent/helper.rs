//! What the setup window's Install and Sign in run: the agent's own installer or login, in a
//! PowerShell window the user can watch and answer. Never elevated, never hidden. The window
//! waits for Enter, so an error stays readable.

use super::{AgentInfo, AgentKind};
use std::process::Command;

pub use crate::text::ps_quote as quote;

/// None when it can't be installed from here: Codex or OpenCode without npm.
pub fn install_command(agent: &AgentInfo) -> Option<String> {
    match agent.kind {
        AgentKind::Claude => Some("irm https://claude.ai/install.ps1 | iex".to_string()),
        AgentKind::Codex => agent.npm.as_deref().map(|npm| format!("& {} install -g @openai/codex", quote(npm))),
        AgentKind::OpenCode => agent.npm.as_deref().map(|npm| format!("& {} install -g opencode-ai", quote(npm))),
    }
}

/// None when the agent wasn't found.
pub fn sign_in_command(agent: &AgentInfo) -> Option<String> {
    let exe = quote(agent.exe.as_deref()?);
    Some(match agent.kind {
        AgentKind::Claude => format!("& {exe} auth login"),
        AgentKind::Codex => format!("& {exe} login"),
        AgentKind::OpenCode => format!("& {exe} auth login"),
    })
}

/// powershell.exe, which every Windows has, in its own visible console. Each argument is
/// quoted for us, so the command reaches -Command exactly as written.
pub fn window(command: &str) -> Command {
    let mut cmd = Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &format!("{command}; Read-Host 'Done. Press Enter to close'"),
    ]);
    cmd
}

/// Runs `command` in [`window`] and returns when the window is closed. Blocks; call it off the
/// UI thread.
pub fn run_in_window(command: &str) -> std::io::Result<()> {
    window(command).spawn()?.wait().map(|_| ())
}
