//! Where things live. Every path under the agents' own folders is a [`ReadOnlyDir`]: it can be
//! joined and read, never written. Kiln's own data lives in [`DataDir`].

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

/// Kiln's own folder, `%LOCALAPPDATA%\Kiln` in the app (a temp folder in tests).
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

fn home(env: &dyn Env) -> PathBuf {
    PathBuf::from(env.var("USERPROFILE").unwrap_or_default())
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

/// `%LOCALAPPDATA%\Kiln`.
pub fn data_dir(env: &dyn Env) -> DataDir {
    DataDir(PathBuf::from(env.var("LOCALAPPDATA").unwrap_or_default()).join("Kiln"))
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
    let s = p.to_string_lossy().replace('/', "\\");
    s.trim_end_matches('\\').to_lowercase()
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
    c == d || c.starts_with(&(d + "\\"))
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
    fn leaf_and_inside() {
        assert_eq!(leaf(r"C:\work\api\"), "api");
        assert_eq!(leaf("api"), "api");
        assert!(is_inside(Path::new(r"C:\Work"), Path::new(r"c:\work\a.md")));
        assert!(!is_inside(Path::new(r"C:\Work"), Path::new(r"C:\Workshop\a.md")));
        assert_eq!(normalize(Path::new(r"C:\a\b\..\c.md")), PathBuf::from(r"C:\a\c.md"));
    }

    #[test]
    fn claude_dir_honours_config_dir() {
        let e = MapEnv::default().with("USERPROFILE", r"C:\Users\x");
        assert_eq!(claude_dir(&e).path(), Path::new(r"C:\Users\x\.claude"));
        let e = e.with("CLAUDE_CONFIG_DIR", r"D:\cfg");
        assert_eq!(claude_dir(&e).path(), Path::new(r"D:\cfg"));
        let e = e.with("CLAUDE_CONFIG_DIR", "");
        assert_eq!(claude_dir(&e).path(), Path::new(r"C:\Users\x\.claude"));
    }
}
