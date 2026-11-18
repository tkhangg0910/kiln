//! OpenCode sessions as sidebar rows. OpenCode (1.2 and later) keeps them in one SQLite file,
//! `<data>\opencode.db`, not in transcripts: the `session` table has the id, parent (a fork),
//! folder, title and times; `message` and `part` hold the turns as JSON. Kilno opens it read
//! only and never writes there, like the other agents' folders. The database is in WAL mode, so
//! it reads while OpenCode writes.
//!
//! The tables are OpenCode's own and undocumented: if they change shape the rows simply stop
//! showing, nothing fails.

use super::model::NO_PROMPT;
use super::watch::{OnChange, SessionSource, Watcher};
use super::{SessionInfo, TurnState};
use crate::agent::AgentKind;
use crate::paths::{opencode_data, Env, ReadOnlyDir};
use crate::status::SessionStatus;
use crate::text::{clean, shorten};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The database's file name in OpenCode's data folder.
pub const DB_FILE: &str = "opencode.db";

/// The first prompt and the last one are cut to this, as for Codex.
const PREVIEW_MAX: usize = 300;

pub struct OpenCodeStore {
    data: ReadOnlyDir,
    /// By session id: its `time_updated` when read, and the row. A session that has not changed
    /// is not read again.
    cache: HashMap<String, (i64, SessionInfo)>,
    watcher: Option<Watcher>,
}

impl OpenCodeStore {
    pub fn new(data: ReadOnlyDir) -> Self {
        Self { data, cache: HashMap::new(), watcher: None }
    }

    /// `%XDG_DATA_HOME%\opencode`, else `~\.local\share\opencode`.
    pub fn for_this_user(env: &dyn Env) -> Self {
        Self::new(opencode_data(env))
    }

    fn db(&self) -> PathBuf {
        self.data.join(DB_FILE)
    }

    pub fn is_watching(&self) -> bool {
        self.watcher.is_some()
    }
}

impl SessionSource for OpenCodeStore {
    fn root_exists(&self) -> bool {
        self.db().is_file()
    }

    fn scan(&mut self) -> Vec<SessionInfo> {
        let Some(conn) = open(&self.db()) else {
            // Unreadable right now (locked mid-checkpoint, or gone): keep what was shown.
            return self.cache.values().map(|(_, i)| i.clone()).collect();
        };
        let Ok(rows) = sessions(&conn) else {
            return self.cache.values().map(|(_, i)| i.clone()).collect();
        };
        let mut next = HashMap::with_capacity(rows.len());
        for r in rows {
            let info = match self.cache.remove(&r.id) {
                Some((at, info)) if at == r.updated => info,
                _ => r.to_info(&conn),
            };
            next.insert(r.id, (r.updated, info));
        }
        self.cache = next;
        self.cache.values().map(|(_, i)| i.clone()).collect()
    }

    fn start_watching(&mut self, on_change: OnChange) {
        if self.watcher.is_some() || !self.data.path().is_dir() {
            return;
        }
        // The database, its write-ahead log and the shared memory file all change with a write.
        fn wanted(p: &Path) -> bool {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(DB_FILE))
        }
        self.watcher = Watcher::start_files(self.data.path(), wanted, on_change).ok();
    }

    fn stop_watching(&mut self) {
        self.watcher = None;
    }
}

/// How many sessions and folders OpenCode has, for the setup window. (0, 0) without a database.
pub fn count(data: &Path) -> (u32, u32) {
    let Some(conn) = open(&data.join(DB_FILE)) else { return (0, 0) };
    conn.query_row(
        "SELECT COUNT(*), COUNT(DISTINCT directory) FROM session WHERE time_archived IS NULL",
        [],
        |r| Ok((r.get::<_, i64>(0)? as u32, r.get::<_, i64>(1)? as u32)),
    )
    .unwrap_or((0, 0))
}

/// The database, read only. Should OpenCode not be running, its shared-memory file can be
/// missing, which a read-only connection to a WAL database cannot create: then the file is read
/// as it is (`immutable`), which is whole once OpenCode has closed it.
pub fn open(db: &Path) -> Option<Connection> {
    if !db.is_file() {
        return None;
    }
    let ro = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let tried = Connection::open_with_flags(db, ro).and_then(|c| {
        c.query_row("SELECT COUNT(*) FROM sqlite_master", [], |_| Ok(()))?;
        Ok(c)
    });
    let conn = tried.ok().or_else(|| {
        let uri = format!("file:{}?immutable=1", db.to_string_lossy().replace('\\', "/").replace('?', "%3f").replace('#', "%23"));
        Connection::open_with_flags(uri, ro | OpenFlags::SQLITE_OPEN_URI).ok()
    })?;
    // OpenCode may hold a write lock for a moment: wait a little rather than fail the scan.
    let _ = conn.busy_timeout(std::time::Duration::from_millis(500));
    Some(conn)
}

/// One `session` row.
struct Row {
    id: String,
    parent: Option<String>,
    dir: String,
    title: String,
    /// The model the session last used (`session.model`'s `id`).
    model: Option<String>,
    created: i64,
    updated: i64,
}

fn sessions(conn: &Connection) -> rusqlite::Result<Vec<Row>> {
    let mut q = conn.prepare(
        "SELECT id, parent_id, directory, title, json_extract(model, '$.id'), time_created, time_updated \
         FROM session WHERE time_archived IS NULL",
    )?;
    let rows = q.query_map([], |r| {
        Ok(Row {
            id: r.get(0)?,
            parent: r.get(1)?,
            dir: r.get(2)?,
            title: r.get(3)?,
            // An odd value in this column must not drop the row.
            model: r.get::<_, Option<String>>(4).ok().flatten(),
            created: r.get(5)?,
            updated: r.get(6)?,
        })
    })?;
    Ok(rows.filter_map(Result::ok).filter(|r| !r.id.is_empty()).collect())
}

impl Row {
    fn to_info(&self, conn: &Connection) -> SessionInfo {
        let first = prompt(conn, &self.id, "ASC");
        let last = prompt(conn, &self.id, "DESC");
        SessionInfo {
            session_id: self.id.clone(),
            agent: AgentKind::OpenCode,
            parent_id: self.parent.clone().filter(|p| !p.is_empty()),
            file: None,
            project_dir: display_dir(&self.dir),
            preview: first.clone().unwrap_or_else(|| NO_PROMPT.to_string()),
            ai_title: title(&self.title),
            last_prompt: last,
            last_activity: millis(self.updated),
            started: millis(self.created),
            turn: turn(conn, &self.id),
            status: Some(self.status(conn)),
        }
    }

    /// What the pane header shows: the model, and the context the last answer used. OpenCode
    /// stores no window size, so there is no meter (`context_size` 0) and no account limits.
    fn status(&self, conn: &Connection) -> SessionStatus {
        let used: i64 = conn
            .query_row(
                "SELECT COALESCE(json_extract(data, '$.tokens.input'), 0) \
                 + COALESCE(json_extract(data, '$.tokens.cache.read'), 0) \
                 + COALESCE(json_extract(data, '$.tokens.cache.write'), 0) \
                 FROM message WHERE session_id = ?1 AND json_extract(data, '$.role') = 'assistant' \
                 ORDER BY time_created DESC LIMIT 1",
                [&self.id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        SessionStatus {
            session_id: self.id.clone(),
            model: self.model.clone().filter(|m| !m.is_empty()),
            cwd: Some(display_dir(&self.dir)),
            context_used: used,
            context_size: 0,
            context_percent: 0,
            effort: String::new(),
            five_hour: None,
            seven_day: None,
            updated: millis(self.updated),
        }
    }
}

/// OpenCode names a session itself once it has a prompt; until then the title is its
/// placeholder ("New session - <date>"), which says nothing.
fn title(t: &str) -> Option<String> {
    let t = clean(t);
    (!t.is_empty() && !t.starts_with("New session")).then(|| shorten(&t, 90))
}

/// The session's first (`ASC`) or last (`DESC`) prompt you typed: a text part of a user message
/// that OpenCode did not add itself.
fn prompt(conn: &Connection, id: &str, order: &str) -> Option<String> {
    let sql = format!(
        "SELECT json_extract(p.data, '$.text') FROM part p JOIN message m ON m.id = p.message_id \
         WHERE p.session_id = ?1 AND json_extract(m.data, '$.role') = 'user' \
         AND json_extract(p.data, '$.type') = 'text' \
         AND COALESCE(json_extract(p.data, '$.synthetic'), 0) = 0 \
         ORDER BY p.time_created {order} LIMIT 1"
    );
    let text: String = conn.query_row::<Option<String>, _, _>(&sql, [id], |r| r.get(0)).optional().ok()???;
    // `opencode run "…"` keeps the shell's quotes in the prompt.
    let text = clean(text.trim().trim_matches('"'));
    (!text.is_empty()).then(|| shorten(&text, PREVIEW_MAX))
}

/// Where the turn stands, from the newest message: your prompt with no answer yet, or an answer
/// still being written, is working; a finished answer is done; one that ended in an error
/// (stopped with Esc, or failed) is stopped.
fn turn(conn: &Connection, id: &str) -> Option<TurnState> {
    let last: Option<(Option<String>, Option<i64>, Option<String>)> = conn
        .query_row(
            "SELECT json_extract(data, '$.role'), json_extract(data, '$.time.completed'), \
             json_extract(data, '$.error') FROM message WHERE session_id = ?1 \
             ORDER BY time_created DESC LIMIT 1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .ok()?;
    let (role, completed, error) = last?;
    Some(match (role.as_deref(), completed, error) {
        (Some("assistant"), Some(_), Some(_)) => TurnState::Stopped,
        (Some("assistant"), Some(_), None) => TurnState::Done,
        _ => TurnState::Working,
    })
}

fn millis(ms: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default()
}

/// OpenCode writes `C:/Users/x/proj`; shown and compared the way the other agents' folders are.
fn display_dir(dir: &str) -> String {
    if cfg!(windows) {
        dir.replace('/', "\\")
    } else {
        dir.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database with OpenCode's tables (the columns Kilno reads) and one session per case.
    fn db(dir: &Path) -> PathBuf {
        let path = dir.join(DB_FILE);
        let c = Connection::open(&path).unwrap();
        c.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE session (id TEXT PRIMARY KEY, project_id TEXT, parent_id TEXT, directory TEXT NOT NULL,
               title TEXT NOT NULL, model TEXT, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, time_archived INTEGER);
             CREATE TABLE message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             CREATE TABLE part (id TEXT PRIMARY KEY, message_id TEXT NOT NULL, session_id TEXT NOT NULL, time_created INTEGER NOT NULL, data TEXT NOT NULL);
             INSERT INTO session VALUES ('ses_done', 'p', NULL, 'C:/work/app', 'Fix the parser', '{\"id\":\"big-pickle\",\"providerID\":\"opencode\"}', 1000, 5000, NULL);
             INSERT INTO message VALUES ('m1', 'ses_done', 1100, '{\"role\":\"user\",\"time\":{\"created\":1100}}');
             INSERT INTO part VALUES ('p0', 'm1', 'ses_done', 1100, '{\"type\":\"text\",\"text\":\"injected\",\"synthetic\":true}');
             INSERT INTO part VALUES ('p1', 'm1', 'ses_done', 1101, '{\"type\":\"text\",\"text\":\"Fix   the\\nparser\"}');
             INSERT INTO message VALUES ('m2', 'ses_done', 1200, '{\"role\":\"assistant\",\"time\":{\"created\":1200,\"completed\":1300}}');
             INSERT INTO message VALUES ('m3', 'ses_done', 1400, '{\"role\":\"user\",\"time\":{\"created\":1400}}');
             INSERT INTO part VALUES ('p2', 'm3', 'ses_done', 1400, '{\"type\":\"text\",\"text\":\"and the tests\"}');
             INSERT INTO message VALUES ('m4', 'ses_done', 1500, '{\"role\":\"assistant\",\"tokens\":{\"input\":100,\"cache\":{\"read\":20,\"write\":3}},\"time\":{\"created\":1500,\"completed\":1600}}');
             INSERT INTO session VALUES ('ses_fork', 'p', 'ses_done', 'C:/work/app', 'New session - 2026-11-18', NULL, 2000, 2100, NULL);
             INSERT INTO message VALUES ('m5', 'ses_fork', 2050, '{\"role\":\"assistant\",\"time\":{\"created\":2050}}');
             INSERT INTO session VALUES ('ses_stop', 'p', NULL, 'C:/work/other', 'Stopped one', NULL, 3000, 3100, NULL);
             INSERT INTO message VALUES ('m6', 'ses_stop', 3050, '{\"role\":\"assistant\",\"time\":{\"created\":3050,\"completed\":3060},\"error\":{\"name\":\"MessageAbortedError\"}}');
             INSERT INTO session VALUES ('ses_gone', 'p', NULL, 'C:/work/app', 'Archived', NULL, 1, 2, 9);",
        )
        .unwrap();
        path
    }

    #[test]
    fn reads_sessions_as_rows() {
        let dir = tempfile::tempdir().unwrap();
        db(dir.path());
        let mut store = OpenCodeStore::new(ReadOnlyDir::new(dir.path()));
        assert!(store.root_exists());
        let rows = store.scan();
        let get = |id: &str| rows.iter().find(|r| r.session_id == id).cloned().unwrap();
        assert_eq!(rows.len(), 3, "the archived session is left out");

        let done = get("ses_done");
        assert_eq!(done.agent, AgentKind::OpenCode);
        assert_eq!(done.preview, "Fix the parser", "first typed prompt, synthetic text skipped");
        assert_eq!(done.last_prompt.as_deref(), Some("and the tests"));
        assert_eq!(done.ai_title.as_deref(), Some("Fix the parser"));
        assert_eq!(done.turn, Some(TurnState::Done));
        let st = done.status.clone().unwrap();
        assert_eq!((st.model.as_deref(), st.context_used, st.context_size), (Some("big-pickle"), 123, 0), "model and the last answer's context");
        assert_eq!((done.started.timestamp_millis(), done.last_activity.timestamp_millis()), (1000, 5000));
        if cfg!(windows) {
            assert_eq!(done.project_dir, r"C:\work\app");
        }

        let fork = get("ses_fork");
        assert_eq!(fork.parent_id.as_deref(), Some("ses_done"));
        assert_eq!(fork.ai_title, None, "the placeholder title is not a name");
        assert_eq!(fork.preview, NO_PROMPT);
        assert_eq!(fork.turn, Some(TurnState::Working), "an answer still being written");

        assert_eq!(get("ses_stop").turn, Some(TurnState::Stopped));
        assert_eq!(count(dir.path()), (3, 2));
    }

    #[test]
    fn nothing_without_a_database() {
        let dir = tempfile::tempdir().unwrap();
        let mut store = OpenCodeStore::new(ReadOnlyDir::new(dir.path()));
        assert!(!store.root_exists());
        assert!(store.scan().is_empty());
        assert_eq!(count(dir.path()), (0, 0));
    }

    #[test]
    fn never_writes_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let path = db(dir.path());
        let conn = open(&path).unwrap();
        assert!(conn.execute("DELETE FROM session", []).is_err(), "opened read only");
    }
}
