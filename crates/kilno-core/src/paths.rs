//! Where things live. Every path under the agents' own folders is a [`ReadOnlyDir`]: it can be
//! joined and read, never written. Kilno's own data lives in [`DataDir`].

use std::path::{Path, PathBuf};

/// A folder that belongs to Claude Code or Codex. Only read from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadOnlyDir(PathBuf);

impl ReadOnlyDir {
    pub fn new(p: impl Into<PathBuf>) -> Self {
        Self(p.into())
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.0.join(p)
    }
}

/// Kilno's own folder, `%LOCALAPPDATA%\Kilno` in the app (a temp folder in tests).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDir(pub PathBuf);

impl DataDir {
    pub fn new(p: impl Into<PathBuf>) -> Self {
        Self(p.into())
    }
    pub fn path(&self) -> &Path {
        &self.0
    }
    pub fn join(&self, p: impl AsRef<Path>) -> PathBuf {
        self.0.join(p)
    }
}

/// The process environment, injectable so tests never read the real one.
pub trait Env: Send + Sync {
    fn var(&self, name: &str) -> Option<String>;
}

/// The real environment. Empty values count as unset.
pub struct OsEnv;

impl Env for OsEnv {
    fn var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
}

/// A fixed environment for tests. Empty values count as unset.
#[derive(Default, Clone, Debug)]
pub struct MapEnv(pub std::collections::HashMap<String, String>);

impl MapEnv {
    pub fn with(mut self, k: &str, v: impl Into<String>) -> Self {
        self.0.insert(k.to_string(), v.into());
        self
    }
}

impl Env for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.0.get(name).filter(|v| !v.is_empty()).cloned()
    }
}

/// The user's home: `%USERPROFILE%` on Windows, `$HOME` elsewhere.
pub fn home(env: &dyn Env) -> PathBuf {
    PathBuf::from(env.var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).unwrap_or_default())
}

/// `%CLAUDE_CONFIG_DIR%`, else `%USERPROFILE%\.claude` (PORTING.md D2: honoured everywhere).
pub fn claude_dir(env: &dyn Env) -> ReadOnlyDir {
    match env.var("CLAUDE_CONFIG_DIR") {
        Some(d) => ReadOnlyDir::new(d),
        None => ReadOnlyDir::new(home(env).join(".claude")),
    }
}

/// `%CODEX_HOME%`, else `%USERPROFILE%\.codex`.
pub fn codex_home(env: &dyn Env) -> ReadOnlyDir {
    match env.var("CODEX_HOME") {
        Some(d) => ReadOnlyDir::new(d),
        None => ReadOnlyDir::new(home(env).join(".codex")),
    }
}

/// OpenCode's data folder, which holds `opencode.db`: `%XDG_DATA_HOME%\opencode`, else
/// `%USERPROFILE%\.local\share\opencode` (OpenCode keeps that layout on Windows too).
pub fn opencode_data(env: &dyn Env) -> ReadOnlyDir {
    match env.var("XDG_DATA_HOME") {
        Some(d) => ReadOnlyDir::new(PathBuf::from(d).join("opencode")),
        None => ReadOnlyDir::new(home(env).join(".local").join("share").join("opencode")),
    }
}

/// `%LOCALAPPDATA%\Kilno`.
pub fn data_dir(env: &dyn Env) -> DataDir {
    if cfg!(windows) {
        return DataDir(PathBuf::from(env.var("LOCALAPPDATA").unwrap_or_default()).join("Kilno"));
    }
    // Elsewhere: `$XDG_DATA_HOME/kilno`, else `~/.local/share/kilno`.
    let base = env.var("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home(env).join(".local").join("share"));
    DataDir(base.join("kilno"))
}

/// `%TEMP%` (falls back to `TMP`, then the OS temp dir).
pub fn temp_dir(env: &dyn Env) -> PathBuf {
    env.var("TEMP")
        .or_else(|| env.var("TMP"))
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
}

/// Path as a comparison key: `/` as `\`, trailing separators trimmed, lowercase (Windows paths
/// compare case-insensitively).
pub fn norm_key(p: &Path) -> String {
    if cfg!(windows) {
        let s = p.to_string_lossy().replace('/', "\\");
        return s.trim_end_matches('\\').to_lowercase();
    }
    // Elsewhere names are case-sensitive: only trailing slashes are dropped.
    let s = p.to_string_lossy();
    match s.trim_end_matches('/') {
        "" if s.starts_with('/') => "/".to_string(),
        t => t.to_string(),
    }
}

/// Two paths name the same folder (case-insensitive, trailing separators ignored).
pub fn same_dir(a: &Path, b: &Path) -> bool {
    norm_key(a) == norm_key(b)
}

/// Last segment of a path with trailing separators trimmed; the input itself if it has none.
pub fn leaf(p: &str) -> String {
    let t = p.trim_end_matches(['\\', '/']);
    match t.rfind(['\\', '/']) {
        Some(i) if i + 1 < t.len() => t[i + 1..].to_string(),
        _ if !t.is_empty() => t.to_string(),
        _ => p.to_string(),
    }
}

/// `child` is `dir` itself or inside it (case-insensitive prefix match on full paths).
pub fn is_inside(dir: &Path, child: &Path) -> bool {
    let d = norm_key(dir);
    let c = norm_key(child);
    let sep = if cfg!(windows) { '\\' } else { '/' };
    // The Unix root's key already ends in the separator.
    let prefix = if d.ends_with(sep) { d.clone() } else { format!("{d}{sep}") };
    c == d || c.starts_with(&prefix)
}

/// Lexically resolve `.` and `..` (like .NET GetFullPath, without touching the disk).
pub fn normalize(p: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaf_of_either_separator() {
        assert_eq!(leaf(r"C:\work\api\"), "api");
        assert_eq!(leaf("/work/api/"), "api");
        assert_eq!(leaf("api"), "api");
    }

    #[cfg(not(windows))]
    #[test]
    fn inside_on_unix() {
        assert!(is_inside(Path::new("/work"), Path::new("/work/a.md")));
        assert!(is_inside(Path::new("/work/"), Path::new("/work")));
        assert!(!is_inside(Path::new("/Work"), Path::new("/work/a.md")), "case-sensitive");
        assert!(!is_inside(Path::new("/work"), Path::new("/workshop/a.md")));
        assert!(is_inside(Path::new("/"), Path::new("/etc")));
        assert_eq!(normalize(Path::new("/a/b/../c.md")), PathBuf::from("/a/c.md"));
    }

    #[cfg(windows)]
    #[test]
    fn inside_on_windows() {
        assert!(is_inside(Path::new(r"C:\Work"), Path::new(r"c:\work\a.md")));
        assert!(!is_inside(Path::new(r"C:\Work"), Path::new(r"C:\Workshop\a.md")));
        assert_eq!(normalize(Path::new(r"C:\a\b\..\c.md")), PathBuf::from(r"C:\a\c.md"));
    }

    #[test]
    fn claude_dir_honours_config_dir() {
        let e = MapEnv::default().with("USERPROFILE", r"C:\Users\x").with("HOME", r"C:\Users\x");
        assert_eq!(claude_dir(&e).path(), Path::new(r"C:\Users\x").join(".claude"));
        let e = e.with("CLAUDE_CONFIG_DIR", r"D:\cfg");
        assert_eq!(claude_dir(&e).path(), Path::new(r"D:\cfg"));
        let e = e.with("CLAUDE_CONFIG_DIR", "");
        assert_eq!(claude_dir(&e).path(), Path::new(r"C:\Users\x").join(".claude"));
    }
}
