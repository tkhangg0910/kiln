//! Claude Code transcripts (`<claude dir>\projects\<mangled cwd>\<id>.jsonl`) as sidebar rows,
//! kept fresh by a watcher. Never writes into that tree.

use super::changed::ChangedFiles;
use super::watch::{OnChange, SessionSource, Watcher};
use super::model::NO_PROMPT;
use super::SessionInfo;
use crate::agent::detect::has_ext;
use crate::agent::AgentKind;
use crate::paths::{claude_dir, Env, ReadOnlyDir};
use crate::text::{clean, shorten};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Window read off the end of a transcript: every one of 177 real transcripts that has a title
/// has it inside the final 96 KiB.
pub(crate) const TAIL_WINDOW: u64 = 96 * 1024;

struct Cached {
    size: u64,
    mtime: Option<DateTime<Utc>>,
    info: SessionInfo,
}

pub struct ClaudeStore {
    root: ReadOnlyDir,
    /// Keyed by lowercased path.
    cache: HashMap<String, Cached>,
    changed: Arc<ChangedFiles>,
    watcher: Option<Watcher>,
}

impl ClaudeStore {
    /// `root` is the `projects` folder.
    pub fn new(root: ReadOnlyDir) -> Self {
        Self { root, cache: HashMap::new(), changed: Arc::default(), watcher: None }
    }

    /// `<CLAUDE_CONFIG_DIR or ~\.claude>\projects` (PORTING.md D2).
    pub fn for_this_user(env: &dyn Env) -> Self {
        Self::new(ReadOnlyDir::new(claude_dir(env).join("projects")))
    }

    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }
}

impl SessionSource for ClaudeStore {
    fn root_exists(&self) -> bool {
        self.root.path().is_dir()
    }

    fn scan(&mut self) -> Vec<SessionInfo> {
        if !self.root_exists() {
            return Vec::new();
        }
        let (all, mut files) = self.changed.take(self.watcher.is_some());
        if all {
            files.clear();
            let _ = walk(self.root.path(), &mut files);
            files.retain(|f| has_ext(f, "jsonl"));
            let present: HashSet<String> = files.iter().map(|f| key(f)).collect();
            self.cache.retain(|k, _| present.contains(k));
        }

        for file in files {
            // subagents\agent-*.jsonl are tool transcripts, not sessions you can resume.
            let parent = file.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str());
            if parent.is_some_and(|n| n.eq_ignore_ascii_case("subagents")) {
                continue;
            }
            let k = key(&file);
            let meta = match std::fs::metadata(&file) {
                Ok(m) if m.is_file() && m.len() > 0 => m,
                _ => {
                    self.cache.remove(&k);
                    continue;
                }
            };
            // Cheap invalidation: size + mtime. Transcripts are append-only in practice.
            let mtime = meta.modified().ok().map(DateTime::<Utc>::from);
            if self.cache.get(&k).is_some_and(|c| c.size == meta.len() && c.mtime == mtime) {
                continue;
            }
            match parse(&file, mtime.unwrap_or_else(Utc::now)) {
                Some(info) => {
                    self.cache.insert(k, Cached { size: meta.len(), mtime, info });
                }
                // Unreadable mid-write: keep the row as it was and read it again next scan.
                None => self.changed.add(&file),
            }
        }

        let mut rows: Vec<SessionInfo> = self.cache.values().map(|c| c.info.clone()).collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.last_activity));
        rows
    }

    fn start_watching(&mut self, on_change: OnChange) {
        if !self.root_exists() || self.watcher.is_some() {
            return;
        }
        self.watcher = Watcher::start(self.root.path(), self.changed.clone(), on_change).ok();
    }

    fn stop_watching(&mut self) {
        self.watcher = None;
    }
}

fn key(p: &Path) -> String {
    p.to_string_lossy().to_lowercase()
}

/// Every file under `dir`, recursively. Links are not followed.
pub(crate) fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let t = entry.file_type()?;
        if t.is_dir() {
            walk(&entry.path(), out)?;
        } else if t.is_file() {
            out.push(entry.path());
        }
    }
    Ok(())
}

/// Lines of a file as text: invalid UTF-8 replaced, the line break (and a `\r` before it) cut.
pub(crate) fn lines(r: impl BufRead) -> impl Iterator<Item = io::Result<String>> {
    r.split(b'\n').map(|l| {
        l.map(|mut b| {
            if b.last() == Some(&b'\r') {
                b.pop();
            }
            String::from_utf8_lossy(&b).into_owned()
        })
    })
}

/// A reader `window` bytes from the end, past the first (partial) line when not at the start.
pub(crate) fn tail_reader(path: &Path, window: u64) -> io::Result<BufReader<File>> {
    let mut f = File::open(path)?;
    let start = f.metadata()?.len().saturating_sub(window);
    f.seek(SeekFrom::Start(start))?;
    let mut r = BufReader::new(f);
    if start > 0 {
        // Seeking lands mid-line and mid-UTF-8. Throw the fragment away.
        r.skip_until(b'\n')?;
    }
    Ok(r)
}

/// `name` when `v` is an object holding a string there.
pub(crate) fn get_str<'a>(v: &'a Value, name: &str) -> Option<&'a str> {
    v.as_object()?.get(name)?.as_str()
}

fn parse(file: &Path, mtime: DateTime<Utc>) -> Option<SessionInfo> {
    let mut cwd: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut preview = String::new();

    // Head: cwd and sessionId appear on the first real line. The first human prompt is usually
    // within the first few, but tool noise can push it down. std opens with share
    // read|write|delete, so a transcript Claude is writing still reads.
    let head = BufReader::new(File::open(file).ok()?);
    for line in lines(head).take(40) {
        let line = line.ok()?;
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        if cwd.is_none() {
            cwd = get_str(&v, "cwd").map(str::to_string);
        }
        if session_id.is_none() {
            session_id = get_str(&v, "sessionId").map(str::to_string);
        }
        if preview.is_empty() && get_str(&v, "type") == Some("user") {
            let text = user_text(&v);
            if !text.trim().is_empty() {
                preview = shorten(&text, 120);
            }
        }
        if cwd.is_some() && session_id.is_some() && !preview.is_empty() {
            break;
        }
    }

    let (ai_title, last_prompt) = read_tail(file);
    let session_id = session_id.unwrap_or_else(|| {
        file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    });
    // Last resort: the mangled folder name. Lossy (a real dash in a path can't be told apart
    // from a separator), so it's display only.
    let project_dir = cwd.filter(|c| !c.trim().is_empty()).unwrap_or_else(|| {
        file.parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "(unknown)".to_string())
    });

    Some(SessionInfo {
        session_id,
        agent: AgentKind::Claude,
        parent_id: None,
        file: Some(file.to_path_buf()),
        project_dir,
        preview: if preview.is_empty() { NO_PROMPT.to_string() } else { preview },
        ai_title,
        last_prompt,
        last_activity: mtime,
        started: mtime,
        turn: None,
        status: None,
    })
}

/// ai-title and last-prompt are rewritten as the conversation grows, so the last copy counts.
fn read_tail(file: &Path) -> (Option<String>, Option<String>) {
    let mut title: Option<String> = None;
    let mut prompt: Option<String> = None;
    if let Ok(r) = tail_reader(file, TAIL_WINDOW) {
        for line in lines(r).map_while(Result::ok) {
            // Substring test first: only a handful of lines are the two types wanted.
            if !line.contains("\"ai-title\"") && !line.contains("\"last-prompt\"") {
                continue;
            }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            match get_str(&v, "type") {
                Some("ai-title") => title = get_str(&v, "aiTitle").map(str::to_string).or(title),
                Some("last-prompt") => prompt = get_str(&v, "lastPrompt").map(str::to_string).or(prompt),
                _ => {}
            }
        }
    }
    let pick = |s: Option<String>, max| {
        let c = clean(s.as_deref().unwrap_or(""));
        (!c.is_empty()).then(|| shorten(&c, max))
    };
    (pick(title, 90), pick(prompt, 120))
}

/// `message.content` is a plain string or an array of blocks. Tool results, meta lines and
/// slash-command wrappers give "", so the preview shows something a human typed.
fn user_text(line: &Value) -> String {
    if line.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return String::new();
    }
    let Some(content) = line.get("message").filter(|m| m.is_object()).and_then(|m| m.get("content")) else {
        return String::new();
    };
    match content {
        Value::String(s) => clean(s),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| get_str(b, "type") == Some("text"))
            .map(|b| clean(get_str(b, "text").unwrap_or("")))
            .find(|t| !t.is_empty())
            .unwrap_or_default(),
        _ => String::new(),
    }
}
