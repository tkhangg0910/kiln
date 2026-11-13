//! An open pane's transcript, read as it grows, a few new lines at a time. Three things come
//! out of it: the last markdown file the session wrote inside its folder (the doc chip), the
//! pages it wrote in `.kiln\pages` (the pages chip), and when you last sent a prompt (which
//! clears the image chip). Prompts from before the pane opened are skipped. Never writes.

use super::claude::get_str;
use crate::paths::{is_inside, normalize};
use crate::time::parse_iso;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// A resumed session's transcript can run to a few MB. Its first read starts this far from the
/// end: the lines since the pane opened are all there, and the timestamp skips the rest.
const FIRST_WINDOW: u64 = 512 * 1024;

pub struct TranscriptTail {
    folder: PathBuf,
    page_folder: PathBuf,
    pages: Vec<PathBuf>,
    since: DateTime<Utc>,
    codex: bool,
    file: Option<PathBuf>,
    offset: u64,
    doc: Option<PathBuf>,
    page_writes: u32,
    prompt_at: Option<DateTime<Utc>>,
}

impl TranscriptTail {
    /// `folder` is the pane's working folder; `since` is when the pane opened.
    pub fn new(folder: &Path, since: DateTime<Utc>, codex: bool) -> Self {
        let folder = normalize(&std::path::absolute(folder).unwrap_or_else(|_| folder.to_path_buf()));
        let page_folder = folder.join(".kiln").join("pages");
        Self {
            folder,
            page_folder,
            pages: Vec::new(),
            since,
            codex,
            file: None,
            offset: 0,
            doc: None,
            page_writes: 0,
            prompt_at: None,
        }
    }

    /// The newest .md the session wrote or edited inside its folder, if any.
    pub fn doc(&self) -> Option<&Path> {
        self.doc.as_deref()
    }

    /// The newest page the session wrote or edited in .kiln\pages, if any.
    pub fn page(&self) -> Option<&Path> {
        self.pages.last().map(PathBuf::as_path)
    }

    /// Every page the session wrote, the newest last.
    pub fn pages(&self) -> &[PathBuf] {
        &self.pages
    }

    /// How many times a page was written, so a caller can tell a read that saw one.
    pub fn page_writes(&self) -> u32 {
        self.page_writes
    }

    /// When you last sent a prompt since the pane opened; None until you do.
    pub fn prompt_at(&self) -> Option<DateTime<Utc>> {
        self.prompt_at
    }

    /// Reads what was added since the last call. True when a doc or a page was written meanwhile.
    pub fn read(&mut self, file: &Path) -> bool {
        self.read_new(file).unwrap_or(false)
    }

    fn read_new(&mut self, file: &Path) -> std::io::Result<bool> {
        // std opens with share read|write|delete, so the agent keeps writing meanwhile.
        let mut f = File::open(file)?;
        let len = f.metadata()?.len();
        let mut mid = false;
        let same = self.file.as_deref().is_some_and(|p| p.to_string_lossy().eq_ignore_ascii_case(&file.to_string_lossy()));
        if !same {
            self.file = Some(file.to_path_buf());
            self.offset = len.saturating_sub(FIRST_WINDOW);
            mid = self.offset > 0;
        }
        if len <= self.offset {
            return Ok(false);
        }
        f.seek(SeekFrom::Start(self.offset))?;
        let mut bytes = Vec::with_capacity((len - self.offset) as usize);
        f.take(len - self.offset).read_to_end(&mut bytes)?;

        // Only whole lines: the last one may still be being written.
        let Some(end) = bytes.iter().rposition(|&b| b == b'\n') else { return Ok(false) };
        self.offset += end as u64 + 1;
        let text = String::from_utf8_lossy(&bytes[..end]);
        let mut wrote = false;
        for line in text.split('\n').skip(usize::from(mid)) {
            wrote |= self.read_line(line);
        }
        Ok(wrote)
    }

    fn read_line(&mut self, line: &str) -> bool {
        // Most lines are neither; only these few are parsed.
        let lower = line.to_ascii_lowercase();
        let doc = lower.contains(".md") || lower.contains(".html");
        // A prompt you sent: Claude's user line that is not a tool result or Claude's own meta
        // line, or Codex's user message (user_text drops Codex's own context messages).
        let prompt = if self.codex {
            line.contains("\"role\":\"user\"")
        } else {
            line.contains("\"type\":\"user\"") && !line.contains("\"tool_result\"") && !line.contains("\"isMeta\":true")
        };
        if !doc && !prompt {
            return false;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else { return false };

        // Only a prompt sent since the pane opened clears its images. A doc written before then
        // still counts: a resumed session shows the last doc it wrote, if it is still there.
        if prompt {
            if let Some(at) = get_str(&v, "timestamp").and_then(parse_iso) {
                if at >= self.since && (!self.codex || super::codex::user_text(line).is_some()) {
                    self.prompt_at = Some(at);
                }
            }
        }
        if !doc {
            return false;
        }

        let paths = if self.codex { codex_paths(line, &v) } else { claude_paths(&v) };
        let mut wrote = false;
        for path in paths {
            if let Some(md) = self.inside(&path, ".md", &self.folder) {
                self.doc = Some(md);
                wrote = true;
            } else if let Some(page) = self.inside(&path, ".html", &self.page_folder) {
                // Written again: it moves to the end, as the newest.
                let key = page.to_string_lossy().to_lowercase();
                self.pages.retain(|p| p.to_string_lossy().to_lowercase() != key);
                self.pages.push(page);
                self.page_writes += 1;
                wrote = true;
            }
        }
        wrote
    }

    /// The full path when it has that extension, is inside `within` and still exists. Relative
    /// paths are against the pane's folder; `..` that leaves it is rejected.
    fn inside(&self, path: &str, extension: &str, within: &Path) -> Option<PathBuf> {
        if !path.to_ascii_lowercase().ends_with(extension) {
            return None;
        }
        let full = normalize(&self.folder.join(path));
        (is_inside(within, &full) && !crate::paths::same_dir(within, &full) && full.is_file()).then_some(full)
    }
}

/// The file_path of each Write, Edit or MultiEdit in an assistant line.
fn claude_paths(v: &Value) -> Vec<String> {
    let Some(content) = v.get("message").filter(|m| m.is_object()).and_then(|m| m.get("content")).and_then(Value::as_array)
    else {
        return Vec::new();
    };
    content
        .iter()
        .filter(|b| get_str(b, "type") == Some("tool_use"))
        .filter(|b| matches!(get_str(b, "name"), Some("Write" | "Edit" | "MultiEdit")))
        .filter_map(|b| b.get("input").and_then(|i| get_str(i, "file_path")).map(str::to_string))
        .collect()
}

/// An apply_patch names each file on its own line. Read off the raw line, so it does not matter
/// which field of Codex's JSON carries the patch. The path is taken a JSON escape at a time, so
/// the `\\` in a Windows path such as `C:\\notes` is not mistaken for the line break.
fn patch_file() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"\*\*\* (?:Add|Update) File: ((?:[^\\"]|\\.)+?)\\n"#).expect("valid regex"))
}

fn codex_paths(line: &str, v: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    // In code mode (0.162) the patch runs inside exec's JS, escaped twice over, so the regex
    // misses it; the FileChange item that follows names each file plainly.
    let item = v.get("payload").filter(|p| p.is_object()).and_then(|p| p.get("item")).filter(|i| i.is_object());
    if let Some(item) = item.filter(|i| get_str(i, "type") == Some("FileChange")) {
        if let Some(changes) = item.get("changes").and_then(Value::as_object) {
            paths.extend(changes.keys().cloned());
        }
    }
    for m in patch_file().captures_iter(line) {
        if let Ok(path) = serde_json::from_str::<String>(&format!("\"{}\"", &m[1])) {
            paths.push(path);
        }
    }
    paths
}
