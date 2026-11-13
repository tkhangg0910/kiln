//! Which session each branch came from, keyed by the child's id (`parents.json`).
//!
//! This has to be stored because it cannot be recovered: `--fork-session` copies the parent's
//! messages into the child and rewrites `sessionId` on every copied line, so a forked Claude
//! transcript holds no reference to its origin. A link is only ever written at the moment
//! Kiln forks, so branches made outside Kiln read as ordinary top-level sessions.

use super::names::IdMap;
use crate::paths::DataDir;

#[derive(Debug)]
pub struct SessionParents(IdMap);

impl SessionParents {
    /// Loads `<data>\parents.json`, creating the folder.
    pub fn open(data: &DataDir) -> Self {
        Self(IdMap::open(data, "parents.json"))
    }

    pub fn get(&self, session_id: &str) -> Option<&str> {
        self.0.get(session_id)
    }

    /// Blank ids are ignored. A session cannot branch from itself: that would put a cycle in
    /// the chain walk that draws the indents.
    pub fn set(&mut self, child_id: &str, parent_id: &str) {
        if child_id.trim().is_empty() || parent_id.trim().is_empty() || child_id.eq_ignore_ascii_case(parent_id) {
            return;
        }
        self.0.set(child_id, parent_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_persist_and_refuse_self() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let mut p = SessionParents::open(&data);
        p.set("child", "parent");
        p.set("Self", "self");
        p.set(" ", "x");
        let again = SessionParents::open(&data);
        assert_eq!(again.get("CHILD"), Some("parent"));
        assert_eq!(again.get("self"), None);
        assert_eq!(again.get(" "), None);
    }
}
