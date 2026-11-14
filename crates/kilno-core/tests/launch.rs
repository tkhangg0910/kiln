//! checks/launch.cs: the commands a pane types, for new / resume / fork. Nothing is run; the
//! relay settings path is fixed.

use kilno_core::agent::claude::ClaudeAgent;
use kilno_core::agent::codex::CodexAgent;
use kilno_core::agent::helper::quote;
use kilno_core::agent::AgentKind;
use kilno_core::launch::{self, plan, Agent, LaunchMode, MissingSessionId};
use kilno_core::paths::MapEnv;
use std::cell::RefCell;
use std::path::Path;

const EXE: &str = r"& 'C:\Users\O''Neil\.local\bin\claude.exe'";
const RELAY: &str = r" --settings 'C:\s\relay.json'";
const CX: &str = r"& 'C:\Users\O''Neil\AppData\Roaming\npm\codex.cmd'";

fn claude() -> ClaudeAgent {
    ClaudeAgent::new(r"C:\Users\O'Neil\.local\bin\claude.exe", Some(Path::new(r"C:\s\relay.json")))
}

fn bare() -> ClaudeAgent {
    ClaudeAgent::new("claude", None)
}

fn codex() -> CodexAgent {
    CodexAgent::new(r"C:\Users\O'Neil\AppData\Roaming\npm\codex.cmd")
}

fn go(agent: &dyn Agent, mode: LaunchMode, id: Option<&str>) -> launch::LaunchPlan {
    plan(agent, mode, r"C:\work", id, &MapEnv::default()).unwrap()
}

fn is_guid(s: Option<&str>) -> bool {
    s.and_then(kilno_core::text::guid).is_some()
}

#[test]
fn claude_new() {
    let p = go(&claude(), LaunchMode::New, None);
    let id = p.expected_session_id.clone();
    assert!(is_guid(id.as_deref()), "new: picks an id");
    assert_eq!(p.agent_command, format!("{EXE} --session-id '{}'{RELAY}", id.clone().unwrap()), "new: passes it to claude");
    assert_eq!(p.working_directory, r"C:\work", "new: in the folder");
    assert_ne!(go(&claude(), LaunchMode::New, None).expected_session_id, id, "new: a fresh id each time");
    assert_eq!(p.agent, AgentKind::Claude, "claude: says whose pane");
    assert_eq!(p.shell_command_line, "powershell.exe -NoLogo");
}

#[test]
fn claude_resume_and_fork() {
    let p = go(&claude(), LaunchMode::Resume, Some("abc"));
    assert_eq!(p.expected_session_id.as_deref(), Some("abc"), "resume: keyed on the existing id");
    assert_eq!(p.agent_command, format!("{EXE} --resume 'abc'{RELAY}"), "resume: command");

    let p = go(&claude(), LaunchMode::Fork, Some("abc"));
    let id = p.expected_session_id.clone();
    assert!(is_guid(id.as_deref()), "fork: a new id for the child");
    assert_eq!(
        p.agent_command,
        format!("{EXE} --resume 'abc' --fork-session --session-id '{}'{RELAY}", id.unwrap()),
        "fork: command"
    );
}

#[test]
fn quoting() {
    assert_eq!(go(&bare(), LaunchMode::Resume, Some("abc")).agent_command, "& 'claude' --resume 'abc'", "no relay settings: no --settings");
    assert_eq!(
        go(&bare(), LaunchMode::Resume, Some("x'; calc; '")).agent_command,
        "& 'claude' --resume 'x''; calc; '''",
        "an id is quoted, so a crafted transcript types no command"
    );
    assert_eq!(
        quote("C:\\Users\\O\u{2019}Brien\u{2018}\u{201A}\u{201B}"),
        "'C:\\Users\\O\u{2019}\u{2019}Brien\u{2018}\u{2018}\u{201A}\u{201A}\u{201B}\u{201B}'",
        "typographic quotes are doubled, as PowerShell reads them as quotes"
    );
}

/// An agent that picks its own ids (Codex): Kilno expects none, and never the parent's.
struct PicksOwnId {
    last_new_id: RefCell<Option<Option<String>>>,
}

impl Agent for PicksOwnId {
    fn kind(&self) -> AgentKind {
        AgentKind::Codex
    }
    fn knows_id_upfront(&self) -> bool {
        false
    }
    fn new_command(&self, new_id: Option<&str>) -> String {
        *self.last_new_id.borrow_mut() = Some(new_id.map(str::to_string));
        "new".into()
    }
    fn resume_command(&self, id: &str) -> String {
        format!("resume {id}")
    }
    fn fork_command(&self, parent_id: &str, _new_id: Option<&str>) -> String {
        format!("fork {parent_id}")
    }
}

#[test]
fn own_ids() {
    let a = PicksOwnId { last_new_id: RefCell::new(None) };
    assert_eq!(go(&a, LaunchMode::New, None).expected_session_id, None, "own ids, new: none expected");
    assert_eq!(*a.last_new_id.borrow(), Some(None), "own ids, new: no id passed");
    assert_eq!(go(&a, LaunchMode::Fork, Some("abc")).expected_session_id, None, "own ids, fork: not the parent's id");
    assert_eq!(go(&a, LaunchMode::Resume, Some("abc")).expected_session_id.as_deref(), Some("abc"), "own ids, resume: the existing id");
}

#[test]
fn codex_commands() {
    let p = go(&codex(), LaunchMode::New, None);
    assert_eq!(p.agent_command, CX, "codex new: command");
    assert_eq!(p.expected_session_id, None, "codex new: no id yet");
    assert_eq!(p.agent, AgentKind::Codex, "codex new: says whose pane");
    assert_eq!(go(&codex(), LaunchMode::Resume, Some("abc")).agent_command, format!("{CX} resume 'abc'"), "codex resume: command");
    let p = go(&codex(), LaunchMode::Fork, Some("abc"));
    assert_eq!(p.agent_command, format!("{CX} fork 'abc'"), "codex fork: command");
    assert_eq!(p.expected_session_id, None, "codex fork: no id yet");
}

#[test]
fn fork_without_a_parent_is_refused() {
    let e = plan(&claude(), LaunchMode::Fork, r"C:\work", None, &MapEnv::default());
    assert_eq!(e, Err(MissingSessionId(LaunchMode::Fork)));
    assert!(plan(&claude(), LaunchMode::Resume, r"C:\work", Some("  "), &MapEnv::default()).is_err());
}

#[test]
fn pwsh_on_path_wins() {
    let dir = tempfile::tempdir().unwrap();
    let pwsh = dir.path().join("pwsh.exe");
    std::fs::write(&pwsh, "").unwrap();
    let path = format!("\"{}\";;C:\\nowhere", dir.path().display());
    let env = MapEnv::default().with("PATH", path);
    let p = plan(&bare(), LaunchMode::New, r"C:\work", None, &env).unwrap();
    assert_eq!(p.shell_command_line, format!("\"{}\" -NoLogo", pwsh.display()));
}

#[test]
fn locating_falls_back_to_the_bare_name() {
    let env = MapEnv::default().with("PATH", r"C:\nowhere");
    assert_eq!(ClaudeAgent::for_this_user(&env, None, None).exe, "claude");
    assert_eq!(CodexAgent::for_this_user(&env, None).exe, "codex");
}

#[test]
fn pane_titles_and_folder() {
    assert_eq!(launch::new_title(r"C:\work\api\"), "new \u{b7} api");
    assert_eq!(launch::branch_title("0123456789abcdef"), "branch of 01234567");
    assert_eq!(launch::branch_title("abc"), "branch of abc");
    let here = std::env::temp_dir();
    let here = here.to_string_lossy();
    assert_eq!(launch::resume_folder(&here, r"C:\cur"), here);
    assert_eq!(launch::resume_folder(r"Z:\gone\folder\x", r"C:\cur"), r"C:\cur");
}
