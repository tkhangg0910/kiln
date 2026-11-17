//! Sessions hidden from the sidebar, keyed by id, with when (`hidden.json`). Hiding never
//! touches the transcript: the agents' folders are never written, so `claude --resume` still
//! lists it, and unhiding brings the row back as it was.

use super::names::IdMap;
use crate::paths::DataDir;
use chrono::{DateTime, Utc};

#[derive(Debug)]
pub struct HiddenSessions(IdMap);

impl HiddenSessions {
    /// Loads `<data>\hidden.json`, creating the folder.
    pub fn open(data: &DataDir) -> Self {
        Self(IdMap::open(data, "hidden.json"))
    }

    pub fn is_hidden(&self, session_id: &str) -> bool {
        self.0.get(session_id).is_some()
    }

    /// Blank ids are ignored.
    pub fn hide(&mut self, session_id: &str, when: DateTime<Utc>) {
        if session_id.trim().is_empty() || self.is_hidden(session_id) {
            return;
        }
        self.0.set(session_id, &when.to_rfc3339());
    }

    pub fn unhide(&mut self, session_id: &str) {
        if self.is_hidden(session_id) {
            self.0.remove(session_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_persists_and_unhides() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let mut h = HiddenSessions::open(&data);
        h.hide("Abc", Utc::now());
        h.hide(" ", Utc::now());
        let mut again = HiddenSessions::open(&data);
        assert!(again.is_hidden("abc"), "ids ignore case and survive a restart");
        assert!(!again.is_hidden(" "));
        again.unhide("ABC");
        assert!(!HiddenSessions::open(&data).is_hidden("abc"));
    }
}
