//! Finds Claude Code and Codex: where the program is, whether it runs, whether you are signed
//! in, and how many sessions it has. The environment and the process runner are injected so
//! tests fake both; the file system is real (tests point the environment at a temp folder).

use super::{AgentInfo, AgentKind};
use crate::launch::find_on_path;
use crate::paths::{claude_dir, codex_home, norm_key, opencode_data, Env, OsEnv};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Runs a program and returns what it printed, or None if it failed, exited non-zero or took
/// too long.
pub trait Runner: Send + Sync {
    fn run(&self, exe: &str, args: &str) -> Option<String>;
}

impl<F: Fn(&str, &str) -> Option<String> + Send + Sync> Runner for F {
    fn run(&self, exe: &str, args: &str) -> Option<String> {
        self(exe, args)
    }
}

/// The real runner: [`run`] with a timeout.
pub struct ProcessRunner {
    pub timeout: Duration,
}

impl Default for ProcessRunner {
    fn default() -> Self {
        Self { timeout: DEFAULT_TIMEOUT }
    }
}

impl Runner for ProcessRunner {
    fn run(&self, exe: &str, args: &str) -> Option<String> {
        run(exe, args, self.timeout)
    }
}

/// How long `--version` and the status commands get.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(3000);

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Runs `exe` with `args` (one command line, passed as is) without a console window. Returns
/// stdout then stderr (Codex reports its login on stderr) when it exits 0 in time; None when
/// it cannot start, exits non-zero, or outlives `timeout`, in which case its whole process
/// tree is killed.
pub fn run(exe: &str, args: &str, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
        if !args.is_empty() {
            cmd.raw_arg(args);
        }
    }
    #[cfg(not(windows))]
    cmd.args(args.split_whitespace());
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;

    let drain = |r: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut r) = r {
                let _ = r.read_to_end(&mut buf);
            }
            String::from_utf8_lossy(&buf).into_owned()
        })
    };
    let out = drain(child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>));
    let err = drain(child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>));

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(15)),
            _ => {
                // The readers are left to finish on their own once the tree is gone.
                kill_tree(&mut child);
                return None;
            }
        }
    };
    let out = out.join().unwrap_or_default();
    let err = err.join().unwrap_or_default();
    status.success().then(|| out + &err)
}

/// Kills the child and everything it started (a .cmd shim runs node, which runs the agent).
fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Detects agents for setup. Send + Sync, so it can run on a background thread.
pub struct AgentDetector {
    env: Box<dyn Env>,
    runner: Box<dyn Runner>,
}

impl AgentDetector {
    pub fn new(env: impl Env + 'static, runner: impl Runner + 'static) -> Self {
        Self { env: Box::new(env), runner: Box::new(runner) }
    }

    /// The real environment and processes.
    pub fn for_this_machine() -> Self {
        Self::new(OsEnv, ProcessRunner::default())
    }

    /// Both agents at once (each runs a few slow processes). Blocks; call it off the UI
    /// thread. Saved paths come from "Locate…".
    /// Claude Code, Codex and OpenCode at once, in that order. `paths` are their Locate… picks.
    pub fn detect_all(&self, paths: [Option<&str>; 3]) -> [AgentInfo; 3] {
        let [claude_path, codex_path, opencode_path] = paths;
        std::thread::scope(|s| {
            let opencode = s.spawn(|| self.detect(AgentKind::OpenCode, opencode_path));
            let [claude, codex] = self.detect_both(claude_path, codex_path);
            let opencode = opencode.join().unwrap_or_else(|_| self.missing(AgentKind::OpenCode));
            [claude, codex, opencode]
        })
    }

    pub fn detect_both(&self, claude_path: Option<&str>, codex_path: Option<&str>) -> [AgentInfo; 2] {
        std::thread::scope(|s| {
            let codex = s.spawn(|| self.detect(AgentKind::Codex, codex_path));
            let claude = self.detect(AgentKind::Claude, claude_path);
            let codex = codex.join().unwrap_or_else(|_| self.missing(AgentKind::Codex));
            [claude, codex]
        })
    }

    pub fn detect(&self, kind: AgentKind, saved_path: Option<&str>) -> AgentInfo {
        let env = self.env.as_ref();
        let claude = kind == AgentKind::Claude;

        // Found is not enough: a half-removed install still leaves its shim behind.
        let mut exe = locate(env, kind, saved_path);
        let version = exe.as_deref().and_then(|e| first_line(self.runner.run(e, "--version").as_deref()));
        if version.is_none() {
            exe = None;
        }

        let signed_in = match (exe.as_deref(), kind) {
            (Some(e), AgentKind::Claude) => claude_signed_in(self.runner.run(e, "auth status").as_deref()),
            (Some(e), AgentKind::Codex) => codex_signed_in(self.runner.run(e, "login status").as_deref()),
            // OpenCode runs its free models with no sign-in; a provider login is optional.
            (Some(_), AgentKind::OpenCode) => true,
            (None, _) => false,
        };

        let data = data_folder(env, kind);
        // A folder we may not list counts as none, rather than failing the whole setup window.
        let (sessions, projects) = match kind {
            AgentKind::Claude => count_claude(&data),
            AgentKind::Codex => count_codex(&data),
            AgentKind::OpenCode => Ok(crate::sessions::opencode::count(&data)),
        }
        .unwrap_or((0, 0));

        let npm = if claude {
            None
        } else {
            find(env, None, &["npm.cmd"], &[within(env, "ProgramFiles", &["nodejs", "npm.cmd"])])
        };
        AgentInfo {
            kind,
            exe,
            version,
            signed_in,
            data_folder: data.to_string_lossy().into_owned(),
            sessions,
            projects,
            npm,
        }
    }

    /// Where the agent is, without running it.
    pub fn locate(&self, kind: AgentKind, saved_path: Option<&str>) -> Option<String> {
        locate(self.env.as_ref(), kind, saved_path)
    }

    fn missing(&self, kind: AgentKind) -> AgentInfo {
        AgentInfo {
            kind,
            exe: None,
            version: None,
            signed_in: false,
            data_folder: data_folder(self.env.as_ref(), kind).to_string_lossy().into_owned(),
            sessions: 0,
            projects: 0,
            npm: None,
        }
    }
}

/// Where the agent is, without running it: cheap enough for every pane launch. Saved path
/// (if it still exists), then PATH (each name across the whole PATH before the next name),
/// then the places installers put it.
pub fn locate(env: &dyn Env, kind: AgentKind, saved_path: Option<&str>) -> Option<String> {
    match kind {
        AgentKind::Claude => find(
            env,
            saved_path,
            &["claude.exe", "claude.cmd"],
            &[
                Some(home(env, &[".local", "bin", "claude.exe"])),
                within(env, "APPDATA", &["npm", "claude.cmd"]),
                within(env, "LOCALAPPDATA", &["Microsoft", "WinGet", "Links", "claude.exe"]),
            ],
        ),
        AgentKind::Codex => find(
            env,
            saved_path,
            &["codex.cmd", "codex.exe"],
            &[
                within(env, "APPDATA", &["npm", "codex.cmd"]),
                within(env, "LOCALAPPDATA", &["Microsoft", "WinGet", "Links", "codex.exe"]),
            ],
        ),
        AgentKind::OpenCode => find(
            env,
            saved_path,
            &["opencode.cmd", "opencode.exe"],
            &[
                within(env, "APPDATA", &["npm", "opencode.cmd"]),
                Some(home(env, &[".opencode", "bin", "opencode.exe"])),
                within(env, "LOCALAPPDATA", &["Microsoft", "WinGet", "Links", "opencode.exe"]),
            ],
        ),
    }
}

fn find(env: &dyn Env, saved: Option<&str>, names: &[&str], known: &[Option<PathBuf>]) -> Option<String> {
    if let Some(s) = saved.filter(|s| Path::new(s).is_file()) {
        return Some(s.to_string());
    }
    let path = env.var("PATH");
    names
        .iter()
        .find_map(|n| find_on_path(n, path.as_deref()))
        .or_else(|| known.iter().flatten().find(|p| p.is_file()).cloned())
        .map(|p| p.to_string_lossy().into_owned())
}

fn data_folder(env: &dyn Env, kind: AgentKind) -> PathBuf {
    match kind {
        AgentKind::Claude => claude_dir(env).path().to_path_buf(),
        AgentKind::Codex => codex_home(env).path().to_path_buf(),
        AgentKind::OpenCode => opencode_data(env).path().to_path_buf(),
    }
}

fn home(env: &dyn Env, parts: &[&str]) -> PathBuf {
    join(PathBuf::from(env.var("USERPROFILE").unwrap_or_default()), parts)
}

/// `%variable%\parts…`, or None when the variable is unset or empty.
fn within(env: &dyn Env, variable: &str, parts: &[&str]) -> Option<PathBuf> {
    env.var(variable).map(|root| join(PathBuf::from(root), parts))
}

fn join(mut p: PathBuf, parts: &[&str]) -> PathBuf {
    p.extend(parts);
    p
}

fn first_line(text: Option<&str>) -> Option<String> {
    text?.split('\n').map(str::trim).find(|l| !l.is_empty()).map(str::to_string)
}

/// `claude auth status` prints JSON with `loggedIn`, possibly after other text.
pub fn claude_signed_in(output: Option<&str>) -> bool {
    let Some(start) = output.and_then(|o| o.find('{').map(|i| &o[i..])) else {
        return false;
    };
    serde_json::from_str::<serde_json::Value>(start)
        .ok()
        .and_then(|v| v.get("loggedIn").and_then(|b| b.as_bool()))
        .unwrap_or(false)
}

/// `codex login status` prints "Logged in using ChatGPT" (or an API key), else "Not logged in".
pub fn codex_signed_in(output: Option<&str>) -> bool {
    output.is_some_and(|o| o.split('\n').any(|l| l.trim().starts_with("Logged in")))
}

/// `projects\<folder>\<id>.jsonl`: one folder per project, one file per session (subfolders
/// such as `subagents` are not counted).
fn count_claude(data: &Path) -> std::io::Result<(u32, u32)> {
    let root = data.join("projects");
    if !root.is_dir() {
        return Ok((0, 0));
    }
    let (mut sessions, mut projects) = (0, 0);
    for dir in std::fs::read_dir(root)? {
        let dir = dir?;
        if !dir.file_type()?.is_dir() {
            continue;
        }
        let mut n = 0;
        for f in std::fs::read_dir(dir.path())? {
            let f = f?;
            if f.file_type()?.is_file() && has_ext(&f.path(), "jsonl") {
                n += 1;
            }
        }
        sessions += n;
        if n > 0 {
            projects += 1;
        }
    }
    Ok((sessions, projects))
}

/// `sessions\YYYY\MM\DD\rollout-*.jsonl`; the project is session_meta's cwd on line 0. A file
/// that can't be read or parsed still counts as a session.
fn count_codex(data: &Path) -> std::io::Result<(u32, u32)> {
    let root = data.join("sessions");
    if !root.is_dir() {
        return Ok((0, 0));
    }
    let mut files = Vec::new();
    crate::sessions::claude::walk(&root, &mut files)?;
    let mut sessions = 0;
    let mut folders = HashSet::new();
    for file in files.iter().filter(|f| is_rollout(f)) {
        sessions += 1;
        if let Some(cwd) = first_cwd(file) {
            folders.insert(norm_key(Path::new(&cwd)));
        }
    }
    Ok((sessions, folders.len() as u32))
}

fn first_cwd(file: &Path) -> Option<String> {
    // std opens with share read|write|delete, so a transcript Codex is writing still reads.
    let mut line = String::new();
    BufReader::new(std::fs::File::open(file).ok()?).read_line(&mut line).ok()?;
    let v: serde_json::Value = serde_json::from_str(&line).ok()?;
    let cwd = v.get("payload")?.get("cwd")?.as_str()?;
    (!cwd.is_empty()).then(|| cwd.to_string())
}

/// `rollout-*.jsonl`, case-insensitive.
pub(crate) fn is_rollout(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .and_then(|n| n.get(..8))
        .is_some_and(|p| p.eq_ignore_ascii_case("rollout-"))
        && has_ext(p, "jsonl")
}

pub(crate) fn has_ext(p: &Path, ext: &str) -> bool {
    p.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case(ext))
}
