//! The Files panel's file work: a folder's listing, what a file is, reading a text file to edit
//! and saving it back. Nothing here runs a file.

use std::io;
use std::path::{Path, PathBuf};

/// Folders not listed: tool output and history nobody browses by hand.
pub const SKIPPED: [&str; 7] = [".git", "node_modules", "target", ".venv", "__pycache__", ".next", "dist"];

/// The editor opens files up to this size.
pub const TEXT_LIMIT: u64 = 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// `dir`'s entries: folders first, then files, each by name ignoring case; [`SKIPPED`] folders
/// left out. Dot files (`.env`) are listed. An unreadable folder lists nothing.
pub fn list(dir: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<Entry> = read
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            // A link to a folder counts as a folder: is_dir follows it.
            let is_dir = e.path().is_dir();
            (!(is_dir && SKIPPED.contains(&name.as_str()))).then(|| Entry { name, path: e.path(), is_dir })
        })
        .collect();
    out.sort_by_cached_key(|e| (!e.is_dir, e.name.to_lowercase()));
    out
}

/// Files under `root` whose name contains `query` (ignoring case), breadth first, at most
/// `limit` of them; [`SKIPPED`] folders are not entered.
pub fn find(root: &Path, query: &str, limit: usize) -> Vec<Entry> {
    let q = query.to_lowercase();
    let mut out = Vec::new();
    let mut dirs = std::collections::VecDeque::from([root.to_path_buf()]);
    // shortcut: a bounded walk on the UI thread (20 000 entries looked at); thread it if huge trees stall.
    let mut seen = 0;
    while let Some(dir) = dirs.pop_front() {
        for e in list(&dir) {
            seen += 1;
            if e.is_dir {
                dirs.push_back(e.path);
            } else if e.name.to_lowercase().contains(&q) {
                out.push(e);
                if out.len() >= limit {
                    return out;
                }
            }
            if seen > 20_000 {
                return out;
            }
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Markdown,
    Image,
    Text,
    TooBig,
    Binary,
}

/// What the panel does with `path`: by extension for markdown and pictures, else by looking at
/// it (a NUL byte or bytes that are not UTF-8 make it binary).
pub fn kind(path: &Path) -> Kind {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "md" | "markdown" => return Kind::Markdown,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" => return Kind::Image,
        _ => {}
    }
    match std::fs::metadata(path) {
        Ok(m) if m.len() > TEXT_LIMIT => Kind::TooBig,
        Ok(_) => match std::fs::read(path) {
            Ok(bytes) if !bytes.contains(&0) && std::str::from_utf8(&bytes).is_ok() => Kind::Text,
            _ => Kind::Binary,
        },
        Err(_) => Kind::Binary,
    }
}

/// A text file to edit, as it is (a BOM and line endings kept, so saving changes only edits).
pub fn read_text(path: &Path) -> io::Result<String> {
    if std::fs::metadata(path)?.len() > TEXT_LIMIT {
        return Err(io::Error::other("too big to edit here"));
    }
    String::from_utf8(std::fs::read(path)?).map_err(|_| io::Error::other("not a text file"))
}

/// Writes `text` beside `path` and swaps it in, so a failure midway leaves the old file whole.
pub fn save(path: &Path, text: &str) -> io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".kilno-save");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_first_by_name_skipping_tool_output() {
        let d = tempfile::tempdir().unwrap();
        for f in ["b.txt", "A.md", ".env"] {
            std::fs::write(d.path().join(f), "x").unwrap();
        }
        for dir in ["src", "node_modules", ".git", "Docs"] {
            std::fs::create_dir(d.path().join(dir)).unwrap();
        }
        let names: Vec<_> = list(d.path()).into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Docs", "src", ".env", "A.md", "b.txt"]);
        assert!(list(&d.path().join("nope")).is_empty());
    }

    #[test]
    fn find_walks_down_but_not_into_skipped_folders() {
        let d = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(d.path().join("a/b")).unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/x")).unwrap();
        std::fs::write(d.path().join("a/b/.env.local"), "").unwrap();
        std::fs::write(d.path().join("node_modules/x/.env"), "").unwrap();
        std::fs::write(d.path().join("README.md"), "").unwrap();
        let found: Vec<_> = find(d.path(), "ENV", 10).into_iter().map(|e| e.name).collect();
        assert_eq!(found, [".env.local"]);
        assert_eq!(find(d.path(), "", 1).len(), 1, "the limit holds");
    }

    #[test]
    fn kinds() {
        let d = tempfile::tempdir().unwrap();
        let w = |n: &str, b: &[u8]| {
            std::fs::write(d.path().join(n), b).unwrap();
            kind(&d.path().join(n))
        };
        assert_eq!(w("a.MD", b"# x"), Kind::Markdown);
        assert_eq!(w("a.png", b"not even png"), Kind::Image);
        assert_eq!(w(".env", b"A=1\r\nB=\xc4\x91\n"), Kind::Text);
        assert_eq!(w("a.bin", b"\x00\x01"), Kind::Binary);
        assert_eq!(w("latin1.txt", b"caf\xe9"), Kind::Binary);
        assert_eq!(w("big.log", &vec![b'a'; TEXT_LIMIT as usize + 1]), Kind::TooBig);
    }

    #[test]
    fn save_keeps_what_it_is_given_and_leaves_no_temp() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join(".env");
        std::fs::write(&p, "\u{feff}A=1\r\n").unwrap();
        let text = read_text(&p).unwrap();
        assert_eq!(text, "\u{feff}A=1\r\n", "BOM and CRLF kept");
        save(&p, &text.replace("A=1", "A=2")).unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "\u{feff}A=2\r\n");
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }
}
