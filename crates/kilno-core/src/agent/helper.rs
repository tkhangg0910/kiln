//! What the setup window's Install and Sign in run: the agent's own installer or login, in a
//! window the user can watch and answer (PowerShell on Windows, a terminal elsewhere). Never
//! elevated, never hidden. The window waits for Enter, so an error stays readable.

use super::{AgentInfo, AgentKind};
use crate::text::shell_run;
use std::process::Command;

pub use crate::text::ps_quote as quote;

/// None when it can't be installed from here: Codex or OpenCode without npm.
pub fn install_command(agent: &AgentInfo) -> Option<String> {
    match agent.kind {
        AgentKind::Claude if cfg!(windows) => Some("irm https://claude.ai/install.ps1 | iex".to_string()),
        AgentKind::Claude => Some("curl -fsSL https://claude.ai/install.sh | bash".to_string()),
        AgentKind::Codex => agent.npm.as_deref().map(|npm| format!("{} install -g @openai/codex", shell_run(npm))),
        AgentKind::OpenCode => agent.npm.as_deref().map(|npm| format!("{} install -g opencode-ai", shell_run(npm))),
    }
}

/// None when the agent wasn't found.
pub fn sign_in_command(agent: &AgentInfo) -> Option<String> {
    let exe = shell_run(agent.exe.as_deref()?);
    Some(match agent.kind {
        AgentKind::Claude => format!("{exe} auth login"),
        AgentKind::Codex => format!("{exe} login"),
        AgentKind::OpenCode => format!("{exe} auth login"),
    })
}

/// powershell.exe, which every Windows has, in its own visible console. Each argument is
/// quoted for us, so the command reaches -Command exactly as written.
#[cfg(windows)]
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

/// The first terminal emulator found (Debian's x-terminal-emulator, then GNOME's, KDE's,
/// Xfce's, xterm), running `command` in sh and waiting for Enter. `--wait` keeps GNOME
/// Terminal's process until its window closes, so the caller can check again after it.
#[cfg(not(windows))]
pub fn window(command: &str) -> Command {
    let script = format!("{command}; printf '\\nDone. Press Enter to close'; read _");
    let found = |p: &str| crate::launch::find_on_path(p, std::env::var("PATH").ok().as_deref()).is_some();
    let mut cmd = if found("x-terminal-emulator") {
        let mut c = Command::new("x-terminal-emulator");
        c.args(["-e", "sh", "-c", &script]);
        c
    } else if found("gnome-terminal") {
        let mut c = Command::new("gnome-terminal");
        c.args(["--wait", "--", "sh", "-c", &script]);
        c
    } else if found("konsole") {
        let mut c = Command::new("konsole");
        c.args(["-e", "sh", "-c", &script]);
        c
    } else if found("xfce4-terminal") {
        let mut c = Command::new("xfce4-terminal");
        c.args(["--disable-server", "-x", "sh", "-c", &script]);
        c
    } else {
        let mut c = Command::new("xterm");
        c.args(["-e", "sh", "-c", &script]);
        c
    };
    cmd.stdin(std::process::Stdio::null());
    cmd
}

/// Runs `command` in [`window`] and returns when the window is closed. Blocks; call it off the
/// UI thread.
pub fn run_in_window(command: &str) -> std::io::Result<()> {
    window(command).spawn()?.wait().map(|_| ())
}
