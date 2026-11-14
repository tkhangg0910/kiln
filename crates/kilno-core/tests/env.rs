//! checks/env.cs: scrubbing Claude Code's session markers from Kilno's own environment, so the
//! shells (and claudes) it spawns don't inherit them. One test: it changes this process's
//! environment.

#![cfg(windows)]

use std::process::Command;

fn child_sees(name: &str) -> String {
    let out = Command::new("cmd.exe").args(["/c", &format!("echo %{name}%")]).output().unwrap();
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    // cmd echoes %NAME% verbatim when it is unset.
    if s == format!("%{name}%") { String::new() } else { s }
}

#[test]
fn scrub_inherited_session() {
    // Planted here rather than relying on how the test was launched.
    std::env::set_var("CLAUDE_CODE_CHILD_SESSION", "1");
    std::env::set_var("ANTHROPIC_MODEL", "keep-me");

    // What Claude Code sets for its tools' commands, Kilno having been started from one.
    std::env::set_var("CLAUDECODE", "1");
    std::env::set_var("NO_COLOR", "1");
    std::env::set_var("GIT_EDITOR", "true");

    assert_eq!(child_sees("CLAUDE_CODE_CHILD_SESSION"), "1", "child inherits the marker before scrubbing");
    kilno_core::launch::scrub_inherited_session();
    assert_eq!(child_sees("CLAUDE_CODE_CHILD_SESSION"), "", "child cannot see the marker after scrubbing");
    assert_eq!(child_sees("NO_COLOR"), "", "a pane gets its colours back");
    assert_eq!(child_sees("GIT_EDITOR"), "", "git commit opens the editor again");
    assert_eq!(child_sees("ANTHROPIC_MODEL"), "keep-me", "configuration is left untouched");

    // Not started from Claude Code: a NO_COLOR the user set on purpose is theirs.
    std::env::set_var("NO_COLOR", "1");
    kilno_core::launch::scrub_inherited_session();
    assert_eq!(child_sees("NO_COLOR"), "1", "the user's own NO_COLOR stays");
}
