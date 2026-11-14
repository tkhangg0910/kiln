//! Finds a newer Kilno on GitHub and installs it with `scripts/install.ps1`, the same script
//! the README's install command runs. The script waits for this Kilno to quit, swaps the folder
//! and starts the new one, so Kilno only has to start it and quit. The HTTP request itself is the
//! app's (WinHTTP); this is everything around it.

use std::fmt;
use std::path::Path;
use std::process::Command;

/// The GitHub repo releases come from, set at build time (PORTING.md D12).
pub const REPO: &str = match option_env!("KILNO_UPDATE_REPO") {
    Some(r) => r,
    None => "tkhangg0910/kilno",
};

/// GitHub refuses requests without a User-Agent.
pub const USER_AGENT: &str = "Kilno";
pub const TIMEOUT_SECS: u64 = 15;

/// A dev build would update its own target folder with the release, so it never checks.
pub const ENABLED: bool = !cfg!(debug_assertions);

/// How often a running Kilno asks again (MainWindow's 12 h timer).
pub const CHECK_EVERY: std::time::Duration = std::time::Duration::from_secs(12 * 60 * 60);

/// The Update button over New session (and its Settings twin), once a newer release is out.
pub fn button_text(to: Version) -> String {
    format!("Update to {to}")
}

pub fn button_tooltip(to: Version, current: Version) -> String {
    format!(
        "Kilno {to} is out (you have {current}). Kilno closes, updates and opens again, once no session is working."
    )
}

/// After the click, while a session is still busy.
pub const WAITING_TEXT: &str = "Update waits for running sessions";
pub const WAITING_TOOLTIP: &str =
    "Kilno updates as soon as no session is working. Press to call it off.";

/// The confirm box when sessions are open (OK / Cancel).
pub const CONFIRM_TITLE: &str = "Update Kilno";
pub const CONFIRM_TEXT: &str = "Kilno closes to update and opens again. Open sessions close and resume from the sidebar; any that are still working are waited for.";

/// This build's version, from Cargo.toml.
pub fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or(Version::new(0, 0, 0))
}

/// UpdateWhenIdle: the update may go once no pane is working or asking something.
pub fn idle(statuses: impl IntoIterator<Item = crate::pane_status::PaneStatus>) -> bool {
    use crate::pane_status::PaneStatus::{NeedsInput, Working};
    !statuses.into_iter().any(|s| matches!(s, Working | NeedsInput))
}

/// A release version, compared on its three parts: 1.0.0 and 1.0.0.0 are the same release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
    pub build: u32,
}

impl Version {
    pub const fn new(major: u32, minor: u32, build: u32) -> Self {
        Self {
            major,
            minor,
            build,
        }
    }

    /// Two to four dot-separated numbers, as .NET's `Version.TryParse` takes them; a fourth
    /// part is dropped.
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        if !(2..=4).contains(&parts.len()) {
            return None;
        }
        let mut n = [0u32; 4];
        for (slot, p) in n.iter_mut().zip(&parts) {
            let p = p.trim();
            if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                return None;
            }
            *slot = p.parse().ok().filter(|v| *v <= i32::MAX as u32)?;
        }
        Some(Self::new(n[0], n[1], n[2]))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.build)
    }
}

/// The latest-release endpoint, the only request Kilno makes.
pub fn latest_url() -> String {
    format!("https://api.github.com/repos/{REPO}/releases/latest")
}

/// The script as it was released with that version, not as main has it now: a push to main
/// never reaches anyone's Update button.
pub fn script_url(version: Version) -> String {
    format!("https://raw.githubusercontent.com/{REPO}/v{version}/scripts/install.ps1")
}

/// The release's page on GitHub: its notes are what's new in that version.
pub fn release_url(version: Version) -> String {
    format!("https://github.com/{REPO}/releases/tag/v{version}")
}

/// A tag like "v1.0.1", when it is a later version than `current`.
pub fn newer(tag: Option<&str>, current: Version) -> Option<Version> {
    let latest = Version::parse(tag?.trim_start_matches(['v', 'V']))?;
    (latest > current).then_some(latest)
}

/// `tag_name` of the latest-release JSON GitHub sent; None when it isn't that.
pub fn tag_from_json(body: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    v.get("tag_name")?.as_str().map(str::to_string)
}

/// The release in that JSON when it is newer than `current` (pass
/// `Version::parse(env!("CARGO_PKG_VERSION"))`).
pub fn newer_from_json(body: &str, current: Version) -> Option<Version> {
    newer(tag_from_json(body).as_deref(), current)
}

/// The install script in a PowerShell window you can watch; the caller then quits. On a failure
/// the window waits, or it would close before the error could be read. `KILNO_DIR` makes it
/// update this Kilno where it is, not a copy in the default folder.
pub fn install_command(version: Version, kilno_dir: &Path) -> Command {
    let mut c = Command::new("powershell.exe");
    c.args([
        "-NoProfile",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        &format!(
            "try {{ irm '{}' | iex }} catch {{ Write-Host $_ -ForegroundColor Red; pause }}",
            script_url(version)
        ),
    ]);
    c.env("KILNO_DIR", kilno_dir);
    c
}
