//! checks/agents.cs (AgentDetector) and the AgentHelper cases of checks/setup.cs. A fake machine
//! in a temp folder: the environment points into it and the runner answers from a table keyed
//! "exe|args", so nothing real is run, read or installed (except the real-runner cases).

// Some cases are Windows-only (paths, PowerShell); their helpers sit unused elsewhere.
#![cfg_attr(not(windows), allow(unused_imports, dead_code))]

use kilno_core::agent::detect::{claude_signed_in, codex_signed_in, AgentDetector};
use kilno_core::agent::helper::{install_command, sign_in_command, window};
use kilno_core::agent::{AgentInfo, AgentKind};
use kilno_core::paths::MapEnv;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// The real outputs, as printed on 2026-10-03 by Claude Code 2.1.288 and codex-cli 0.160.0.
const CLAUDE_IN: &str = "{\n  \"loggedIn\": true,\n  \"authMethod\": \"claude.ai\",\n  \"apiProvider\": \"firstParty\"\n}";
const CLAUDE_OUT: &str = r#"{ "loggedIn": false }"#;

struct Machine {
    dir: tempfile::TempDir,
    env: MapEnv,
    answers: HashMap<String, String>,
}

impl Machine {
    fn fresh() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut m = Self { dir, env: MapEnv::default(), answers: HashMap::new() };
        let p = |parts: &[&str]| m.p(parts).to_string_lossy().into_owned();
        let env = MapEnv::default()
            .with("USERPROFILE", p(&["home"]))
            .with("HOME", p(&["home"]))
            .with("APPDATA", p(&["home", "AppData", "Roaming"]))
            .with("LOCALAPPDATA", p(&["home", "AppData", "Local"]))
            .with("ProgramFiles", p(&["pf"]))
            .with("PATH", format!("{}{}{}", p(&["bin1"]), if cfg!(windows) { ';' } else { ':' }, p(&["bin2"])));
        m.env = env;
        m
    }

    fn p(&self, parts: &[&str]) -> PathBuf {
        let mut p = self.dir.path().to_path_buf();
        p.extend(parts);
        p
    }

    fn s(&self, parts: &[&str]) -> String {
        self.p(parts).to_string_lossy().into_owned()
    }

    fn touch(&self, parts: &[&str], text: &str) -> String {
        let path = self.p(parts);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, text).unwrap();
        path.to_string_lossy().into_owned()
    }

    fn works(&mut self, exe: &str, version: &str, status: Option<(&str, &str)>) {
        self.answer(exe, "--version", &format!("{version}\n"));
        if let Some((args, out)) = status {
            self.answer(exe, args, out);
        }
    }

    fn answer(&mut self, exe: &str, args: &str, out: &str) {
        self.answers.insert(format!("{exe}|{args}").to_lowercase(), out.to_string());
    }

    fn detector(&self) -> AgentDetector {
        let answers = self.answers.clone();
        AgentDetector::new(self.env.clone(), move |exe: &str, args: &str| {
            answers.get(&format!("{exe}|{args}").to_lowercase()).cloned()
        })
    }

    fn detect(&self, kind: AgentKind, saved: Option<&str>) -> AgentInfo {
        self.detector().detect(kind, saved)
    }
}

#[cfg(not(windows))]
#[test]
fn found_in_order_on_unix() {
    let mut m = Machine::fresh();
    let none = m.detect(AgentKind::Claude, None);
    assert_eq!(none.exe, None);
    assert_eq!(none.data_folder, m.s(&["home", ".claude"]), "default data folder is ~/.claude");
    // ~/.local/bin without PATH, then PATH (colon-separated) beats it, then a saved path wins.
    let native = m.touch(&["home", ".local", "bin", "claude"], "");
    m.works(&native, "2.1.288 (Claude Code)", Some(("auth status", CLAUDE_IN)));
    assert_eq!(m.detect(AgentKind::Claude, None).exe.as_deref(), Some(native.as_str()));
    let on_path = m.touch(&["bin2", "claude"], "");
    m.works(&on_path, "2.1.288 (Claude Code)", Some(("auth status", CLAUDE_OUT)));
    assert_eq!(m.detect(AgentKind::Claude, None).exe.as_deref(), Some(on_path.as_str()));
    let saved = m.touch(&["custom", "claude"], "");
    m.works(&saved, "2.1.200 (Claude Code)", None);
    assert_eq!(m.detect(AgentKind::Claude, Some(&saved)).exe.as_deref(), Some(saved.as_str()));
    // npm's global bin for Codex.
    let codex = m.touch(&["home", ".npm-global", "bin", "codex"], "");
    assert_eq!(kilno_core::agent::detect::locate(&m.env, AgentKind::Codex, None).as_deref(), Some(codex.as_str()));
}

#[cfg(windows)]
#[test]
fn nothing_installed() {
    let m = Machine::fresh();
    let none = m.detect(AgentKind::Claude, None);
    assert_eq!(none.exe, None, "nothing installed: no exe");
    assert!(!none.signed_in, "nothing installed: not signed in");
    assert_eq!(none.sessions, 0, "nothing installed: no sessions");
    assert_eq!(none.data_folder, m.s(&["home", ".claude"]), "default data folder is ~\\.claude");
}

#[cfg(windows)]
#[test]
fn claude_found_in_order() {
    let mut m = Machine::fresh();
    // Known place found when PATH has nothing (an install a moment ago, PATH not yet updated).
    let native = m.touch(&["home", ".local", "bin", "claude.exe"], "");
    m.works(&native, "2.1.288 (Claude Code)", Some(("auth status", CLAUDE_IN)));
    let c = m.detect(AgentKind::Claude, None);
    assert_eq!(c.exe.as_deref(), Some(native.as_str()), "known place found without PATH");
    assert_eq!(c.version.as_deref(), Some("2.1.288 (Claude Code)"), "version is the first line");
    assert!(c.signed_in, "claude signed in from JSON");

    // PATH beats the known place; a .cmd counts.
    let cmd = m.touch(&["bin2", "claude.cmd"], "");
    m.works(&cmd, "2.1.288 (Claude Code)", Some(("auth status", CLAUDE_OUT)));
    let c = m.detect(AgentKind::Claude, None);
    assert_eq!(c.exe.as_deref(), Some(cmd.as_str()), "PATH beats a known place");
    assert!(!c.signed_in, "claude signed out from JSON");

    // A saved path beats PATH.
    let saved = m.touch(&["custom", "claude.exe"], "");
    m.works(&saved, "2.1.200 (Claude Code)", None);
    assert_eq!(m.detect(AgentKind::Claude, Some(&saved)).exe.as_deref(), Some(saved.as_str()), "saved path beats PATH");
    let gone = m.s(&["gone", "claude.exe"]);
    assert_eq!(
        m.detect(AgentKind::Claude, Some(&gone)).exe.as_deref(),
        Some(cmd.as_str()),
        "a saved path that's gone falls back to PATH"
    );

    // Found but doesn't run (or timed out: the runner returns None for both).
    m.answers.remove(&format!("{cmd}|--version").to_lowercase());
    let c = m.detect(AgentKind::Claude, None);
    assert_eq!(c.exe, None, "found but --version fails: not usable");
    assert_eq!(c.version, None, "found but --version fails: no version");
}

fn meta(cwd: &str) -> String {
    format!(
        "{{\"timestamp\":\"2026-10-02T17:44:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"x\",\"cwd\":\"{}\"}}}}\n",
        cwd.replace('\\', "\\\\")
    )
}

#[cfg(windows)]
#[test]
fn codex_through_npm() {
    let mut m = Machine::fresh();
    let codex = m.touch(&["home", "AppData", "Roaming", "npm", "codex.cmd"], "");
    m.works(&codex, "codex-cli 0.160.0", Some(("login status", "Logged in using ChatGPT\r\n")));
    let npm = m.touch(&["pf", "nodejs", "npm.cmd"], "");
    m.env = m.env.clone().with("CODEX_HOME", m.s(&["codexhome"]));
    let day = |d: &str, f: &str| ["codexhome", "sessions", "2026", "10", d, f].map(str::to_string);
    let t = |parts: [String; 6], text: &str| {
        m.touch(&parts.each_ref().map(String::as_str), text);
    };
    t(day("01", "rollout-a.jsonl"), &meta(r"C:\work\api"));
    t(day("02", "rollout-b.jsonl"), &meta(r"C:\Work\API\"));
    t(day("02", "rollout-c.jsonl"), &meta(r"C:\work\web"));
    t(day("02", "rollout-d.jsonl"), "not json\n");
    t(day("02", "other.jsonl"), &meta(r"C:\elsewhere"));

    let x = m.detect(AgentKind::Codex, None);
    assert_eq!(x.exe.as_deref(), Some(codex.as_str()), "codex in %APPDATA%\\npm");
    assert_eq!(x.version.as_deref(), Some("codex-cli 0.160.0"), "codex version");
    assert!(x.signed_in, "codex signed in");
    assert_eq!(x.data_folder, m.s(&["codexhome"]), "CODEX_HOME is the data folder");
    assert_eq!(x.sessions, 4, "codex sessions: rollout files only");
    assert_eq!(x.projects, 2, "codex projects: same folder in any case or trailing slash counts once");
    assert_eq!(x.npm.as_deref(), Some(npm.as_str()), "npm found in Program Files");
    assert!(!codex_signed_in(Some("Not logged in\n")), "codex signed out");
}

#[test]
fn claude_counts_and_config_dir() {
    let mut m = Machine::fresh();
    m.env = m.env.clone().with("CLAUDE_CONFIG_DIR", m.s(&["cfg"]));
    m.touch(&["cfg", "projects", "C--work-api", "1.jsonl"], "");
    m.touch(&["cfg", "projects", "C--work-api", "2.jsonl"], "");
    m.touch(&["cfg", "projects", "C--work-web", "3.jsonl"], "");
    m.touch(&["cfg", "projects", "C--work-web", "subagents", "agent-1.jsonl"], "");
    std::fs::create_dir_all(m.p(&["cfg", "projects", "C--empty"])).unwrap();
    let c = m.detect(AgentKind::Claude, None);
    assert_eq!(c.data_folder, m.s(&["cfg"]), "CLAUDE_CONFIG_DIR is the data folder");
    assert_eq!(c.sessions, 3, "claude sessions");
    assert_eq!(c.projects, 2, "claude projects: empty folders don't count");
    assert_eq!(c.npm, None, "no npm for claude");
}

#[test]
fn detects_both_off_thread() {
    let m = Machine::fresh();
    let d = m.detector();
    let [claude, codex] = std::thread::spawn(move || d.detect_both(None, None)).join().unwrap();
    assert_eq!((claude.kind, codex.kind), (AgentKind::Claude, AgentKind::Codex));
}

#[test]
fn odd_outputs() {
    assert!(claude_signed_in(Some(&format!("note\n{CLAUDE_IN}"))), "claude status with a line before the JSON");
    assert!(!claude_signed_in(Some("oops")), "claude status not JSON");
    assert!(!claude_signed_in(None), "no output at all");
}

#[cfg(windows)]
mod real_runner {
    use kilno_core::agent::detect::run;
    use std::time::{Duration, Instant};

    const T: Duration = Duration::from_millis(3000);

    #[test]
    fn returns_output() {
        assert_eq!(run("cmd.exe", "/c echo hi", T).map(|s| s.trim().to_string()).as_deref(), Some("hi"));
    }

    #[test]
    fn non_zero_exit_is_none() {
        assert_eq!(run("cmd.exe", "/c exit 3", T), None);
    }

    #[test]
    fn too_slow_is_none_and_gives_up_on_time() {
        let started = Instant::now();
        assert_eq!(run("cmd.exe", "/c ping -n 6 127.0.0.1 >nul", Duration::from_millis(500)), None);
        assert!(started.elapsed() < Duration::from_secs(3), "runner: gives up on time");
    }

    #[test]
    fn missing_exe_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(run(&dir.path().join("nope.exe").to_string_lossy(), "", T), None);
    }

    #[test]
    fn runs_a_cmd_shim() {
        // codex is a .cmd shim from npm: it must run with its arguments as given.
        let dir = tempfile::tempdir().unwrap();
        let shim = dir.path().join("codex.cmd");
        std::fs::write(&shim, "@echo off\r\necho codex-cli %1\r\n").unwrap();
        let out = run(&shim.to_string_lossy(), "--version", T);
        assert_eq!(out.map(|s| s.trim().to_string()).as_deref(), Some("codex-cli --version"));
    }
}

// checks/setup.cs: what Install and Sign in run.
fn info(kind: AgentKind, exe: Option<&str>, npm: Option<&str>) -> AgentInfo {
    AgentInfo {
        kind,
        exe: exe.map(str::to_string),
        version: None,
        signed_in: false,
        data_folder: String::new(),
        sessions: 0,
        projects: 0,
        npm: npm.map(str::to_string),
    }
}

#[cfg(windows)]
#[test]
fn install_and_sign_in_commands() {
    let claude_missing = info(AgentKind::Claude, None, None);
    assert_eq!(install_command(&claude_missing).as_deref(), Some("irm https://claude.ai/install.ps1 | iex"), "install claude");
    assert_eq!(
        install_command(&info(AgentKind::Codex, None, Some(r"C:\Program Files\nodejs\npm.cmd"))).as_deref(),
        Some(r"& 'C:\Program Files\nodejs\npm.cmd' install -g @openai/codex"),
        "install codex through its npm"
    );
    assert_eq!(install_command(&info(AgentKind::Codex, None, None)), None, "no npm, no codex install");
    assert_eq!(
        sign_in_command(&info(AgentKind::Claude, Some(r"C:\c.exe"), None)).as_deref(),
        Some(r"& 'C:\c.exe' auth login"),
        "sign in to claude"
    );
    assert_eq!(
        sign_in_command(&info(AgentKind::Codex, Some(r"C:\x.cmd"), Some(r"C:\npm.cmd"))).as_deref(),
        Some(r"& 'C:\x.cmd' login"),
        "sign in to codex"
    );
    assert_eq!(sign_in_command(&claude_missing), None, "nothing to sign in to");
}

#[cfg(windows)]
#[test]
fn a_visible_powershell_window_that_waits() {
    let w = window("x");
    assert_eq!(w.get_program(), "powershell.exe", "a visible PowerShell window");
    let args: Vec<_> = w.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args, ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", "x; Read-Host 'Done. Press Enter to close'"]);
}

#[cfg(windows)]
#[test]
fn locate_is_cheap_and_needs_no_runner() {
    let m = Machine::fresh();
    let exe = m.touch(&["bin1", "codex.cmd"], "");
    assert_eq!(kilno_core::agent::detect::locate(&m.env, AgentKind::Codex, None).as_deref(), Some(exe.as_str()));
    assert!(Path::new(&exe).is_file());
}

#[cfg(unix)]
#[test]
fn install_and_sign_in_commands_for_sh() {
    assert_eq!(
        install_command(&info(AgentKind::Claude, None, None)).as_deref(),
        Some("curl -fsSL https://claude.ai/install.sh | bash")
    );
    assert_eq!(
        install_command(&info(AgentKind::OpenCode, None, Some("/usr/bin/npm"))).as_deref(),
        Some("'/usr/bin/npm' install -g opencode-ai")
    );
    assert_eq!(sign_in_command(&info(AgentKind::Codex, Some("/home/me/.local/bin/codex"), None)).as_deref(), Some("'/home/me/.local/bin/codex' login"));
    let w = window("echo hi");
    let args: Vec<_> = w.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert!(args.last().unwrap().starts_with("echo hi; printf"), "the command, then a wait for Enter: {args:?}");
}
