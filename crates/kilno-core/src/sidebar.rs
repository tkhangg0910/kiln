//! The sidebar's rows and their order, without UI. Port of MainWindow's SessionRow, RefreshSessions,
//! ApplyFilter, LinkThreads, RecentFolderList, RecentSessions and BindCodexPanes' time window.
//!
//! "Now" and the local time zone are parameters: everything is stored in UTC, and only "Today"
//! and the "d MMM" age read the local calendar.

use crate::agent::AgentKind;
use crate::pane_status::PaneStatus;
use crate::sessions::model::NO_PROMPT;
use crate::sessions::SessionInfo;
use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Deep enough for any real branching, and what stops a parents.json describing a cycle from
/// spinning the walk forever.
pub const MAX_BRANCH_DEPTH: usize = 8;

/// Shown when neither agent's transcript folder exists.
pub const EMPTY_STATE: &str =
    "No transcripts found under ~\\.claude or ~\\.codex.\nStart a session and it will show up here.";

/// Codex starts within seconds of its pane; generous, as an update or sign-in screen can hold it up.
pub const CODEX_START_LIMIT: TimeDelta = TimeDelta::minutes(10);

/// A live pane as the sidebar sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneRef {
    /// The app's handle for the pane.
    pub id: u64,
    pub agent: AgentKind,
    pub status: PaneStatus,
    pub working_dir: String,
}

/// Top-level sections, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Bucket {
    /// Has a live pane: the whole reason the app exists.
    Open,
    /// Last activity today (local calendar).
    Today,
    Earlier,
}

impl Bucket {
    pub fn label(self) -> &'static str {
        match self {
            Bucket::Open => "Open",
            Bucket::Today => "Today",
            Bucket::Earlier => "Earlier",
        }
    }

    /// Open and Today start expanded, Earlier folded away.
    pub fn expanded_by_default(self) -> bool {
        self != Bucket::Earlier
    }
}

/// Sort combo: Newest (default), Oldest, Name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortMode {
    #[default]
    Newest,
    Oldest,
    /// Alphabetical and nothing else: branches keep marker and indent but are not threaded.
    Name,
}

/// What the list shows: the search text, the group combo (Folder = true, default) and the sort.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewOptions {
    pub filter: String,
    pub group_by_folder: bool,
    pub sort: SortMode,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            filter: String::new(),
            group_by_folder: true,
            sort: SortMode::Newest,
        }
    }
}

/// One row: backed by a transcript, a live pane, or both (a session started in this window has
/// a pane at once and grows a transcript a moment later).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRow {
    /// A placeholder guid for a Codex pane until its transcript appears.
    pub session_id: String,
    /// None until the transcript shows up on disk.
    pub info: Option<SessionInfo>,
    /// Some while the session is open in this window.
    pub pane: Option<PaneRef>,
    /// Label before any transcript exists.
    pub fallback_title: String,
    pub custom_name: Option<String>,
    pub folder: String,
    /// Set when both agents are in use, so the tag only shows when it tells rows apart.
    pub show_agent: bool,
    /// Session this one was branched from (set by [`Sidebar::link_threads`]).
    pub parent_id: Option<String>,
    /// Parent's name, for the tooltip; None when the parent is not in the list.
    pub branch_of: Option<String>,
    /// Branch hops from the top-level session counting only parents in the list; indents 15 px each.
    pub depth: usize,
    /// Sort key landing a branch directly beneath its parent: a parent's key prefixes its children's.
    pub thread_order: String,
}

impl SessionRow {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            info: None,
            pane: None,
            fallback_title: "new session".into(),
            custom_name: None,
            folder: String::new(),
            show_agent: false,
            parent_id: None,
            branch_of: None,
            depth: 0,
            thread_order: String::new(),
        }
    }

    /// Your name, then the agent's generated title, then the first prompt, then a placeholder.
    pub fn name(&self) -> &str {
        if let Some(n) = &self.custom_name {
            return n;
        }
        if let Some(info) = &self.info {
            if let Some(t) = &info.ai_title {
                return t;
            }
            if !info.preview.is_empty() && info.preview != NO_PROMPT {
                return &info.preview;
            }
        }
        &self.fallback_title
    }

    /// A row with no transcript yet is a pane Kilno started, which knows its agent.
    pub fn agent(&self) -> AgentKind {
        self.info
            .as_ref()
            .map(|i| i.agent)
            .or(self.pane.as_ref().map(|p| p.agent))
            .unwrap_or(AgentKind::Claude)
    }

    /// "CC"/"CX" at the right of the name, only when both agents are in use.
    pub fn agent_tag(&self) -> Option<&'static str> {
        self.show_agent.then(|| self.agent().tag())
    }

    /// Badge colour for the tag: clay for Claude Code, blue for Codex.
    pub fn agent_color(&self) -> &'static str {
        match self.agent() {
            AgentKind::Codex => "#7DBBEB",
            AgentKind::Claude => "#E39A7E",
        }
    }

    pub fn is_open(&self) -> bool {
        self.pane.is_some()
    }

    pub fn is_branch(&self) -> bool {
        self.parent_id.is_some()
    }

    /// The live pane's status; None without a pane.
    pub fn status(&self) -> Option<PaneStatus> {
        self.pane.as_ref().map(|p| p.status)
    }

    /// Transcript's project name, else the folder's leaf.
    pub fn project(&self) -> String {
        match &self.info {
            Some(i) => i.project_name(),
            None => crate::paths::leaf(&self.folder),
        }
    }

    /// Last activity, or now for a pane with no transcript yet.
    pub fn when(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.info.as_ref().map_or(now, |i| i.last_activity)
    }

    pub fn age<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        age_text(self.when(now), now, tz)
    }

    pub fn bucket<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> Bucket {
        if self.is_open() {
            Bucket::Open
        } else if self.when(now).with_timezone(tz).date_naive()
            == now.with_timezone(tz).date_naive()
        {
            Bucket::Today
        } else {
            Bucket::Earlier
        }
    }

    /// Open rows say what the pane is doing, others how long ago they were active.
    pub fn subtitle<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        if self.is_open() {
            self.status_text(now, tz)
        } else {
            self.age(now, tz)
        }
    }

    pub fn status_text<Tz: TimeZone>(&self, now: DateTime<Utc>, tz: &Tz) -> String
    where
        Tz::Offset: std::fmt::Display,
    {
        match status_label(self.status()) {
            Some(s) => s.to_string(),
            None => self.age(now, tz),
        }
    }

    /// Theme brush key of the status light (shared with the pane header's dot).
    pub fn status_key(&self) -> &'static str {
        status_key(self.status())
    }

    /// `Name\n<project dir or folder>\n<id>` plus the parent's name and the last prompt.
    pub fn tooltip(&self) -> String {
        let dir = self
            .info
            .as_ref()
            .map_or(self.folder.as_str(), |i| i.project_dir.as_str());
        let mut tip = format!("{}\n{}\n{}", self.name(), dir, self.session_id);
        if let Some(b) = self.branch_of.as_deref().filter(|b| !b.is_empty()) {
            tip += &format!("\n\nBranched from: {b}");
        }
        if let Some(lp) = self
            .info
            .as_ref()
            .and_then(|i| i.last_prompt.as_deref())
            .filter(|s| !s.is_empty())
        {
            tip += &format!("\n\nLast: {lp}");
        }
        tip
    }

    /// Text the rename box starts with.
    pub fn edit_text(&self) -> String {
        self.custom_name
            .clone()
            .unwrap_or_else(|| self.name().to_string())
    }

    /// The custom name a rename commits: trimmed; empty, or the generated title typed back in,
    /// is no override.
    pub fn rename_to(&self, text: &str) -> Option<String> {
        let t = text.trim();
        let ai = self
            .info
            .as_ref()
            .and_then(|i| i.ai_title.as_deref())
            .unwrap_or("");
        (!t.is_empty() && t != ai).then(|| t.to_string())
    }
}

/// "just now", "N min ago", "N h ago", "N d ago" (under a week), else "d MMM" in local time.
pub fn age_text<Tz: TimeZone>(when: DateTime<Utc>, now: DateTime<Utc>, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let d = now - when;
    let mins = d.num_milliseconds() as f64 / 60_000.0;
    if mins < 1.0 {
        "just now".into()
    } else if mins < 60.0 {
        format!("{} min ago", mins as i64)
    } else if mins < 24.0 * 60.0 {
        format!("{} h ago", (mins / 60.0) as i64)
    } else if mins < 7.0 * 24.0 * 60.0 {
        format!("{} d ago", (mins / (24.0 * 60.0)) as i64)
    } else {
        when.with_timezone(tz).format("%-d %b").to_string()
    }
}

/// The word for a pane's status; None (no pane) means the row shows its age instead.
pub fn status_label(status: Option<PaneStatus>) -> Option<&'static str> {
    Some(match status? {
        PaneStatus::Working => "running",
        PaneStatus::NeedsInput => "needs you",
        PaneStatus::Done => "done",
        PaneStatus::Error => "api error",
        PaneStatus::Exited => "shell closed",
        PaneStatus::Starting => "starting",
    })
}

/// Status to theme brush key. None means no pane: "Dormant", near invisible.
pub fn status_key(status: Option<PaneStatus>) -> &'static str {
    match status {
        Some(PaneStatus::Working | PaneStatus::Starting) => "Working",
        Some(PaneStatus::NeedsInput) => "NeedsInput",
        Some(PaneStatus::Done) => "Done",
        Some(PaneStatus::Error) => "Failed",
        Some(PaneStatus::Exited) => "Ended",
        None => "Dormant",
    }
}

/// Fallback title of a new session's row.
pub fn new_title(folder: &str) -> String {
    format!("new · {}", crate::paths::leaf(folder))
}

/// Fallback title of a branch's row: the parent's first 8 characters.
pub fn branch_title(parent_id: &str) -> String {
    format!(
        "branch of {}",
        parent_id.chars().take(8).collect::<String>()
    )
}

/// .NET ticks (100 ns since 0001-01-01), 19 digits, reversed for newest-first, so comparing the
/// strings orders them the way the times do.
pub fn stamp(when: DateTime<Utc>, newest_first: bool) -> String {
    const EPOCH_TICKS: i64 = 621_355_968_000_000_000;
    const MAX_TICKS: i64 = 3_155_378_975_999_999_999;
    let ticks = (when.timestamp() as i128 * 10_000_000
        + when.timestamp_subsec_nanos() as i128 / 100
        + EPOCH_TICKS as i128)
        .clamp(0, MAX_TICKS as i128) as i64;
    format!(
        "{:019}",
        if newest_first {
            MAX_TICKS - ticks
        } else {
            ticks
        }
    )
}

fn ci_contains(hay: &str, needle: &str) -> bool {
    hay.to_lowercase().contains(&needle.to_lowercase())
}

fn ci_cmp(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// One entry of the New session folder popup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderChoice {
    /// The leaf, to read.
    pub name: String,
    /// The full path, to launch in.
    pub full_path: String,
}

/// One entry of the screenshot picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickChoice {
    pub pane_id: u64,
    pub name: String,
    pub folder: String,
    pub status: PaneStatus,
}

/// All rows, keyed by session id case-insensitively. Rows are kept across refreshes so a live
/// status light does not flicker.
#[derive(Clone, Debug, Default)]
pub struct Sidebar {
    rows: HashMap<String, SessionRow>,
}

fn key(id: &str) -> String {
    id.to_lowercase()
}

impl Sidebar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn rows(&self) -> impl Iterator<Item = &SessionRow> {
        self.rows.values()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&SessionRow> {
        self.rows.get(&key(id))
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut SessionRow> {
        self.rows.get_mut(&key(id))
    }

    pub fn by_pane(&self, pane_id: u64) -> Option<&SessionRow> {
        self.rows
            .values()
            .find(|r| r.pane.as_ref().is_some_and(|p| p.id == pane_id))
    }

    fn by_pane_mut(&mut self, pane_id: u64) -> Option<&mut SessionRow> {
        self.rows
            .values_mut()
            .find(|r| r.pane.as_ref().is_some_and(|p| p.id == pane_id))
    }

    /// A new pane gets its row at once, before any transcript exists, so it shows under Open the
    /// instant it starts. `id` is the expected session id, or a placeholder guid for Codex.
    pub fn open_pane(
        &mut self,
        id: &str,
        pane: PaneRef,
        title: &str,
        custom_name: Option<String>,
    ) -> &mut SessionRow {
        let row = self.rows.entry(key(id)).or_insert_with(|| {
            let mut r = SessionRow::new(id);
            r.folder = pane.working_dir.clone();
            r.custom_name = custom_name;
            r.fallback_title = title.to_string();
            r
        });
        row.folder = pane.working_dir.clone();
        row.pane = Some(pane);
        row
    }

    /// The pane's status changed.
    pub fn set_pane_status(&mut self, pane_id: u64, status: PaneStatus) {
        if let Some(p) = self.by_pane_mut(pane_id).and_then(|r| r.pane.as_mut()) {
            p.status = status;
        }
    }

    /// The pane closed: its row leaves Open. A Codex pane closed before its transcript appeared
    /// (`unbound`) has no session to list, so its row goes.
    pub fn close_pane(&mut self, pane_id: u64, unbound: bool) {
        let Some(row) = self.by_pane_mut(pane_id) else {
            return;
        };
        row.pane = None;
        if unbound {
            let k = key(&row.session_id);
            self.rows.remove(&k);
        }
    }

    /// Re-keys a Codex pane's row from its placeholder to the transcript it turned out to be.
    pub fn rekey(&mut self, old_id: &str, new_id: &str) {
        if let Some(mut row) = self.rows.remove(&key(old_id)) {
            row.session_id = new_id.to_string();
            self.rows.insert(key(new_id), row);
        }
    }

    /// Folds a scan's transcripts into the rows (both agents' lists, only agents in use), so
    /// live panes keep their identity. Rows whose transcript vanished go unless open.
    /// `show_agent` = both agents in use; `name_of` reads names.json.
    pub fn fold(
        &mut self,
        infos: Vec<SessionInfo>,
        show_agent: bool,
        name_of: impl Fn(&str) -> Option<String>,
    ) {
        let mut seen = HashSet::new();
        for info in infos {
            let k = key(&info.session_id);
            seen.insert(k.clone());
            let custom = name_of(&info.session_id);
            match self.rows.get_mut(&k) {
                Some(row) => {
                    row.info = Some(info);
                    row.custom_name = custom;
                    row.show_agent = show_agent;
                }
                None => {
                    let mut row = SessionRow::new(info.session_id.clone());
                    row.folder = info.project_dir.clone();
                    row.custom_name = custom;
                    row.show_agent = show_agent;
                    row.info = Some(info);
                    self.rows.insert(k, row);
                }
            }
        }
        self.rows.retain(|k, r| seen.contains(k) || r.is_open());
    }

    /// Hangs every branch off the session it came from: the parent (the transcript's for Codex,
    /// else Kilno's parents.json via `recorded_parent`), the depth and the thread sort key.
    pub fn link_threads(
        &mut self,
        recorded_parent: impl Fn(&str) -> Option<String>,
        newest_first: bool,
        now: DateTime<Utc>,
    ) {
        let keys: Vec<String> = self.rows.keys().cloned().collect();
        for k in &keys {
            let row = &self.rows[k];
            let parent = row
                .info
                .as_ref()
                .and_then(|i| i.parent_id.clone())
                .or_else(|| recorded_parent(&row.session_id));
            let branch_of = parent
                .as_deref()
                .and_then(|p| self.get(p))
                .map(|p| p.name().to_string());
            let row = self.rows.get_mut(k).expect("key from map");
            row.parent_id = parent;
            row.branch_of = branch_of;
        }
        for k in &keys {
            // Climb to the top-level session; a parent not in the list stops the walk.
            let mut chain = vec![&self.rows[k]];
            while chain.len() <= MAX_BRANCH_DEPTH {
                let Some(p) = chain
                    .last()
                    .and_then(|r| r.parent_id.as_deref())
                    .and_then(|p| self.get(p))
                else {
                    break;
                };
                chain.push(p);
            }
            let depth = chain.len() - 1;
            let order: String = chain
                .iter()
                .rev()
                .map(|r| format!("{}{}/", stamp(r.when(now), newest_first), r.session_id))
                .collect();
            let row = self.rows.get_mut(k).expect("key from map");
            row.depth = depth;
            row.thread_order = order;
        }
    }

    /// Newest activity per project (case-insensitive): folder groups sort by it, not by name.
    fn folder_ranks(&self, now: DateTime<Utc>) -> HashMap<String, DateTime<Utc>> {
        let mut ranks: HashMap<String, DateTime<Utc>> = HashMap::new();
        for r in self.rows.values() {
            let e = ranks
                .entry(r.project().to_lowercase())
                .or_insert(r.when(now));
            *e = (*e).max(r.when(now));
        }
        ranks
    }

    /// The rows to show, filtered and in display order (groups form in this order). Call
    /// [`Self::link_threads`] first with `newest_first = sort != Oldest`.
    pub fn view<Tz: TimeZone>(
        &self,
        opts: &ViewOptions,
        now: DateTime<Utc>,
        tz: &Tz,
    ) -> Vec<&SessionRow> {
        let q = opts.filter.trim();
        let q_low = q.to_lowercase();
        let mut rows: Vec<&SessionRow> = self
            .rows
            .values()
            .filter(|r| {
                q.is_empty()
                    || ci_contains(r.name(), q)
                    || ci_contains(&r.project(), q)
                    || r.session_id.to_lowercase().starts_with(&q_low)
            })
            .collect();
        let ranks = self.folder_ranks(now);
        let rank = |r: &SessionRow| ranks[&r.project().to_lowercase()];
        rows.sort_by(|a, b| {
            let mut o = a.bucket(now, tz).cmp(&b.bucket(now, tz));
            if opts.group_by_folder {
                o = o.then_with(|| match opts.sort {
                    SortMode::Name => Ordering::Equal,
                    SortMode::Newest => rank(b).cmp(&rank(a)),
                    SortMode::Oldest => rank(a).cmp(&rank(b)),
                });
                o = o.then_with(|| ci_cmp(&a.project(), &b.project()));
            }
            o.then_with(|| match opts.sort {
                SortMode::Name => ci_cmp(a.name(), b.name()),
                _ => a.thread_order.cmp(&b.thread_order),
            })
            .then_with(|| a.session_id.cmp(&b.session_id))
        });
        rows
    }

    /// Recent folders for New session: `current` pinned first, then rows' folders by most recent
    /// activity; deduplicated case-insensitively, existing folders only, at most `max` (8).
    pub fn recent_folders(
        &self,
        current: &str,
        max: usize,
        now: DateTime<Utc>,
        exists: impl Fn(&str) -> bool,
    ) -> Vec<FolderChoice> {
        let mut rows: Vec<&SessionRow> = self.rows.values().collect();
        rows.sort_by(|a, b| {
            b.when(now)
                .cmp(&a.when(now))
                .then_with(|| a.session_id.cmp(&b.session_id))
        });
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for folder in std::iter::once(current).chain(rows.iter().map(|r| r.folder.as_str())) {
            if out.len() >= max {
                break;
            }
            if folder.is_empty() || !seen.insert(folder.to_lowercase()) || !exists(folder) {
                continue;
            }
            out.push(FolderChoice {
                name: crate::paths::leaf(folder),
                full_path: folder.to_string(),
            });
        }
        out
    }

    /// Up to `max` open panes for the screenshot picker, the one used last first. `used` is the
    /// most-recently-used pane ids, `open` the live panes in creation order.
    pub fn recent_sessions(&self, used: &[u64], open: &[PaneRef], max: usize) -> Vec<PickChoice> {
        let mut seen = HashSet::new();
        used.iter()
            .filter_map(|id| open.iter().find(|p| p.id == *id))
            .chain(open.iter())
            .filter(|p| seen.insert(p.id))
            .take(max)
            .map(|p| match self.by_pane(p.id) {
                Some(row) => PickChoice {
                    pane_id: p.id,
                    name: row.name().to_string(),
                    folder: row.project(),
                    status: p.status,
                },
                None => PickChoice {
                    pane_id: p.id,
                    name: "new session".into(),
                    folder: file_name(&p.working_dir).to_string(),
                    status: p.status,
                },
            })
            .collect()
    }
}

/// Path.GetFileName: the text after the last separator (empty for a trailing separator).
fn file_name(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

/// What the list currently shows, per row `id|bucket|project-if-grouped|depth|parent`. The list
/// is only rebuilt when this changes (and not while a rename box is open), so it does not blink
/// on every transcript write.
pub fn layout_signature<Tz: TimeZone>(
    rows: &[&SessionRow],
    group_by_folder: bool,
    now: DateTime<Utc>,
    tz: &Tz,
) -> String {
    rows.iter()
        .map(|r| {
            format!(
                "{}|{}|{}|{}|{}",
                r.session_id,
                r.bucket(now, tz).label(),
                if group_by_folder {
                    r.project()
                } else {
                    String::new()
                },
                r.depth,
                r.parent_id.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Codex pane waiting for its transcript (placeholder id), in start order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnboundPane {
    pub folder: String,
    /// The parent, for a fork.
    pub parent_id: Option<String>,
    /// When the pane started.
    pub since: DateTime<Utc>,
}

/// The `[since, until)` window in which `unbound[i]`'s transcript may have started. A session
/// started after the next pane like this one (same parent, and for a new session the same
/// folder) opened is that pane's; one started much later is from outside Kilno. Recompute after
/// each bind, as a bound pane leaves the list.
pub fn codex_bind_window(unbound: &[UnboundPane], i: usize) -> (DateTime<Utc>, DateTime<Utc>) {
    let wait = &unbound[i];
    let until = unbound
        .iter()
        .filter(|w| {
            w.since > wait.since
                && w.parent_id == wait.parent_id
                && (wait.parent_id.is_some()
                    || crate::paths::same_dir(
                        std::path::Path::new(&w.folder),
                        std::path::Path::new(&wait.folder),
                    ))
        })
        .map(|w| w.since)
        .chain(std::iter::once(wait.since + CODEX_START_LIMIT))
        .min()
        .expect("chain has one element");
    (wait.since, until)
}

/// A Codex pane waiting for its transcript, under the placeholder id its row is keyed on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waiting {
    pub placeholder: String,
    pub pane: UnboundPane,
}

/// BindCodexPanes: each waiting pane (in start order) takes the session that
/// [`crate::sessions::codex::find_started`] finds for it within [`codex_bind_window`]. Bound panes
/// leave `waiting` and their new id joins `taken` (the ids live panes already hold). Returns
/// (placeholder, session id) per bind, for the caller to re-key the row and the pane.
pub fn bind_codex_panes(
    waiting: &mut Vec<Waiting>,
    codex: &[SessionInfo],
    taken: &mut HashSet<String>,
) -> Vec<(String, String)> {
    let mut bound = Vec::new();
    for w in waiting.clone() {
        let Some(i) = waiting.iter().position(|x| x.placeholder == w.placeholder) else {
            continue;
        };
        let unbound: Vec<UnboundPane> = waiting.iter().map(|w| w.pane.clone()).collect();
        let (since, until) = codex_bind_window(&unbound, i);
        let Some(found) = crate::sessions::codex::find_started(
            codex,
            &w.pane.folder,
            w.pane.parent_id.as_deref(),
            since,
            taken,
            Some(until),
        ) else {
            continue;
        };
        let id = found.session_id.clone();
        waiting.remove(i);
        taken.insert(id.clone());
        bound.push((w.placeholder, id));
    }
    bound
}

#[cfg(test)]
mod bind_tests {
    use super::*;

    fn at(min: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap() + TimeDelta::minutes(min)
    }

    fn session(id: &str, folder: &str, parent: Option<&str>, started: i64) -> SessionInfo {
        SessionInfo {
            session_id: id.into(),
            agent: AgentKind::Codex,
            parent_id: parent.map(String::from),
            file: None,
            project_dir: folder.into(),
            preview: String::new(),
            ai_title: None,
            last_prompt: None,
            last_activity: at(started),
            started: at(started),
            turn: None,
            status: None,
        }
    }

    fn wait(ph: &str, folder: &str, parent: Option<&str>, since: i64) -> Waiting {
        Waiting {
            placeholder: ph.into(),
            pane: UnboundPane { folder: folder.into(), parent_id: parent.map(String::from), since: at(since) },
        }
    }

    #[test]
    fn two_panes_in_one_folder_bind_in_start_order() {
        let mut w = vec![wait("p1", r"C:\a", None, 0), wait("p2", r"C:\a", None, 2)];
        let codex = [session("s2", r"C:\a\", None, 3), session("s1", r"C:\A", None, 1)];
        let mut taken = HashSet::new();
        let b = bind_codex_panes(&mut w, &codex, &mut taken);
        assert_eq!(b, vec![("p1".into(), "s1".into()), ("p2".into(), "s2".into())]);
        assert!(w.is_empty());
        assert!(taken.contains("s1") && taken.contains("s2"));
    }

    #[test]
    fn later_pane_session_is_not_taken_by_earlier_pane() {
        // Only the second pane's session exists so far: it started after pane 2 opened.
        let mut w = vec![wait("p1", r"C:\a", None, 0), wait("p2", r"C:\a", None, 2)];
        let codex = [session("s2", r"C:\a", None, 3)];
        let b = bind_codex_panes(&mut w, &codex, &mut HashSet::new());
        assert_eq!(b, vec![("p2".into(), "s2".into())]);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].placeholder, "p1");
    }

    #[test]
    fn fork_binds_by_parent_and_skips_taken_and_late() {
        let mut w = vec![wait("f", r"C:\x", Some("par"), 0)];
        let codex = [
            session("other", r"C:\x", None, 1),
            session("held", r"C:\x", Some("par"), 1),
            session("late", r"C:\x", Some("par"), 11),
            session("mine", r"C:\y", Some("par"), 2),
        ];
        let mut taken = HashSet::from(["held".to_string()]);
        let b = bind_codex_panes(&mut w, &codex, &mut taken);
        assert_eq!(b, vec![("f".into(), "mine".into())]);
    }
}
