//! Images pasted into a session, for the pane's image chip. Claude Code keeps each one as
//! `%TEMP%\claude\<project>\<session>\images\<n>.png`, n being its `[Image #n]` tag
//! (`CLAUDE_CODE_TMPDIR` moves the `%TEMP%` part). Codex keeps no folder per session: a pasted
//! image is a loose `%TEMP%\codex-clipboard-*.png`. Kilno only reads these; the agents own them.

use crate::paths::{temp_dir, Env};
use chrono::{DateTime, Utc};
use std::path::{Path, PathBuf};

const KINDS: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "bmp"];

/// Codex's pasted images in [`codex_dir`] (the name read from codex.exe 0.160.0).
pub const CODEX_PATTERN: &str = "codex-clipboard-*";

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| KINDS.iter().any(|k| e.eq_ignore_ascii_case(k)))
}

/// `<CLAUDE_CODE_TMPDIR or %TEMP%>\claude`.
pub fn claude_root(env: &dyn Env) -> PathBuf {
    env.var("CLAUDE_CODE_TMPDIR").map(PathBuf::from).unwrap_or_else(|| temp_dir(env)).join("claude")
}

/// Where Codex's pasted images land: `%TEMP%`.
pub fn codex_dir(env: &dyn Env) -> PathBuf {
    temp_dir(env)
}

/// The session's images folder under any project in `root`, or None until its first paste
/// makes one. An id that is not a plain name (it is read from a transcript) finds nothing.
pub fn find_dir(root: &Path, session_id: &str) -> Option<PathBuf> {
    if session_id.is_empty() || session_id.contains(['\\', '/', ':']) || session_id == ".." {
        return None;
    }
    std::fs::read_dir(root)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.path().join(session_id).join("images"))
        .find(|d| d.is_dir())
}

/// The images in `dir` matching `pattern` (`*` wildcards, case-insensitive) written after
/// `since`: in tag order (numeric name), the rest oldest first. A missing folder gives none.
pub fn since(dir: &Path, since: DateTime<Utc>, pattern: &str) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut found: Vec<(i64, DateTime<Utc>, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| is_image(p) && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| matches(pattern, n)))
        .filter_map(|p| {
            let at = crate::time::mtime(&p).filter(|t| *t > since)?;
            let tag = p
                .file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.parse::<i32>().ok())
                .map_or(i64::from(i32::MAX), i64::from);
            Some((tag, at, p))
        })
        .collect();
    found.sort_by_key(|f| (f.0, f.1));
    found.into_iter().map(|(_, _, p)| p).collect()
}

/// A file-name wildcard: `*` matches any run, everything else literally, ignoring case.
fn matches(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_lowercase(), name.to_lowercase());
    let mut parts = p.split('*');
    let first = parts.next().unwrap_or("");
    let Some(mut rest) = n.strip_prefix(first) else { return false };
    let parts: Vec<&str> = parts.collect();
    let Some((last, middle)) = parts.split_last() else { return rest.is_empty() };
    for m in middle {
        match rest.find(m) {
            Some(i) => rest = &rest[i + m.len()..],
            None => return false,
        }
    }
    rest.ends_with(last)
}

/// The images still in the prompt, each with its `[Image #n]` tag (MainWindow:1364-1369):
/// Claude names each file by its tag; Codex numbers a prompt's images from 1, so a file without
/// a number is tagged by its place. Only tags still on screen count, so an image you deleted
/// from the prompt drops out.
pub fn tagged(files: &[PathBuf], on_screen: &std::collections::BTreeSet<u32>) -> Vec<(PathBuf, u32)> {
    files
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let n = p.file_stem().and_then(|s| s.to_str()).and_then(|s| s.parse::<u32>().ok());
            (p.clone(), n.unwrap_or(i as u32 + 1))
        })
        .filter(|(_, tag)| on_screen.contains(tag))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::matches;

    #[test]
    fn only_tags_on_screen() {
        use std::path::PathBuf;
        let tags = [1, 3].into_iter().collect();
        let claude = [PathBuf::from("1.png"), PathBuf::from("2.png"), PathBuf::from("3.png")];
        let got: Vec<u32> = super::tagged(&claude, &tags).into_iter().map(|t| t.1).collect();
        assert_eq!(got, [1, 3]);
        let codex = [PathBuf::from("codex-clipboard-a.png"), PathBuf::from("codex-clipboard-b.png")];
        let got: Vec<PathBuf> = super::tagged(&codex, &tags).into_iter().map(|t| t.0).collect();
        assert_eq!(got, [PathBuf::from("codex-clipboard-a.png")]);
    }

    #[test]
    fn wildcards() {
        assert!(matches("*", "a.png"));
        assert!(matches("codex-clipboard-*", "Codex-Clipboard-x.png"));
        assert!(!matches("codex-clipboard-*", "other.png"));
        assert!(matches("a*b*c", "aXbYc"));
        assert!(!matches("a*b*c", "aXcYb"));
        assert!(matches("abc", "ABC"));
        assert!(!matches("abc", "abcd"));
    }
}
