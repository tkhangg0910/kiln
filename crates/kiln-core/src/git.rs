//! A folder's branch and its uncommitted `+added −deleted`, as the user's status line shows them.
//! Read through the git CLI (same flags as the C# build), never for a folder outside a repo, at
//! most once per interval per folder, off the UI thread.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Branch plus unstaged line counts (working tree vs index; staged changes are not counted).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct GitStatus {
    pub branch: String,
    pub added: i32,
    pub deleted: i32,
}

/// How long one git call may take before it is killed.
pub const TIMEOUT: Duration = Duration::from_secs(5);

/// The app's poll interval per folder.
pub const INTERVAL: Duration = Duration::from_secs(3);

/// None when the folder is not in a repo or git cannot be run. `rev-parse --abbrev-ref HEAD`
/// gives "HEAD" when detached.
pub fn read(folder: &Path) -> Option<GitStatus> {
    if !is_repo(folder) {
        return None;
    }
    let branch = git(folder, &["rev-parse", "--abbrev-ref", "HEAD"])?
        .trim()
        .to_string();
    if branch.is_empty() {
        return None;
    }
    let numstat = git(
        folder,
        &["diff", "--numstat", "--no-ext-diff", "--no-textconv"],
    )
    .unwrap_or_default();
    let (added, deleted) = parse_numstat(&numstat);
    Some(GitStatus {
        branch,
        added,
        deleted,
    })
}

/// Looks for `.git` up the tree rather than asking git, so a folder outside any repo costs no
/// process at all. `.git` is a file in a worktree or submodule, so either counts.
pub fn is_repo(folder: &Path) -> bool {
    let Ok(start) = std::path::absolute(folder) else {
        return false;
    };
    start.ancestors().any(|dir| dir.join(".git").exists())
}

/// `added<TAB>deleted<TAB>path` per file; a binary file shows `-` for both and counts nothing.
pub fn parse_numstat(numstat: &str) -> (i32, i32) {
    let (mut added, mut deleted) = (0i32, 0i32);
    for line in numstat.split('\n').filter(|l| !l.is_empty()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 {
            continue;
        }
        if let Ok(a) = parts[0].parse::<i32>() {
            added = added.wrapping_add(a);
        }
        if let Ok(d) = parts[1].parse::<i32>() {
            deleted = deleted.wrapping_add(d);
        }
    }
    (added, deleted)
}

/// Git's stdout, or None if it failed, is missing, or took longer than [`TIMEOUT`].
fn git(folder: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new("git");
    // A folder may be anything you opened, and its .git/config can name a program to run
    // (core.fsmonitor, or a diff driver, which the diff call turns off). Not from our timer.
    cmd.args(["-c", "core.fsmonitor=false", "--no-optional-locks", "-C"])
        .arg(folder)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    no_window(&mut cmd);
    let mut child = cmd.spawn().ok()?;

    // Drain both, or a chatty stderr fills its pipe and git never exits.
    let out = drain(child.stdout.take());
    let _err = drain(child.stderr.take());
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                kill_tree(&mut child);
                return None;
            }
        }
    };
    let stdout = out.join().ok()?;
    status
        .success()
        .then(|| String::from_utf8_lossy(&stdout).into_owned())
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    })
}

#[cfg(windows)]
fn no_window(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(windows))]
fn no_window(_: &mut Command) {}

/// Kills git and whatever it started (as .NET's `Kill(entireProcessTree: true)`).
fn kill_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        let mut tk = Command::new("taskkill");
        tk.args(["/F", "/T", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        no_window(&mut tk);
        let _ = tk.status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

type OnChange = dyn Fn(&str, Option<&GitStatus>) + Send + Sync;

/// Per-folder cache of [`read`], refreshed in the background. Folders compare ignoring case.
/// Cheap to clone and to call from any thread.
#[derive(Clone)]
pub struct GitCache {
    inner: Arc<CacheInner>,
}

struct CacheInner {
    interval: Duration,
    state: Mutex<CacheState>,
    on_change: Box<OnChange>,
}

#[derive(Default)]
struct CacheState {
    known: HashMap<String, (Instant, Option<GitStatus>)>,
    reading: HashSet<String>,
}

impl GitCache {
    /// `on_change(folder, status)` runs on a background thread when a folder's status differs
    /// from its last read.
    pub fn new(
        interval: Duration,
        on_change: impl Fn(&str, Option<&GitStatus>) + Send + Sync + 'static,
    ) -> Self {
        Self {
            inner: Arc::new(CacheInner {
                interval,
                state: Mutex::new(CacheState::default()),
                on_change: Box::new(on_change),
            }),
        }
    }

    /// The last status read for this folder.
    pub fn get(&self, folder: &str) -> Option<GitStatus> {
        let state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        state.known.get(&key(folder)).and_then(|(_, s)| s.clone())
    }

    /// Starts a read in the background if this folder is due one: none in flight and none in
    /// the last interval. Returns the thread so callers (tests) can wait on it.
    pub fn refresh(&self, folder: &str) -> Option<std::thread::JoinHandle<()>> {
        let k = key(folder);
        {
            let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
            if state.reading.contains(&k) {
                return None;
            }
            if state
                .known
                .get(&k)
                .is_some_and(|(at, _)| at.elapsed() < self.inner.interval)
            {
                return None;
            }
            state.reading.insert(k.clone());
        }
        let inner = Arc::clone(&self.inner);
        let folder = folder.to_string();
        Some(std::thread::spawn(move || {
            let status = read(&PathBuf::from(&folder));
            let changed = {
                let mut state = inner.state.lock().unwrap_or_else(|e| e.into_inner());
                let changed = state.known.get(&k).and_then(|(_, s)| s.as_ref()) != status.as_ref();
                state
                    .known
                    .insert(k.clone(), (Instant::now(), status.clone()));
                state.reading.remove(&k);
                changed
            };
            if changed {
                (inner.on_change)(&folder, status.as_ref());
            }
        }))
    }
}

fn key(folder: &str) -> String {
    folder.to_lowercase()
}
