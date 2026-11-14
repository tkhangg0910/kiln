//! Names you typed yourself, keyed by session id (`names.json`). The agents' generated titles
//! describe what a conversation was about, not what you were trying to do or which of four
//! similar branches this is. Kept in Kilno's own folder: the agents' folders are never written.

use crate::paths::DataDir;
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// A JSON object of id -> text in Kilno's folder: ids compare case-insensitively, saving writes
/// beside the file then swaps it in, and a corrupt file starts empty rather than failing.
#[derive(Debug)]
pub(crate) struct IdMap {
    path: PathBuf,
    /// In file order; small enough that a linear lookup is fine.
    entries: Vec<(String, String)>,
}

impl IdMap {
    pub(crate) fn open(data: &DataDir, file: &str) -> Self {
        let _ = std::fs::create_dir_all(data.path());
        let path = data.join(file);
        let entries = load(&path).unwrap_or_default();
        Self { path, entries }
    }

    fn find(&self, id: &str) -> Option<usize> {
        self.entries.iter().position(|(k, _)| k.to_lowercase() == id.to_lowercase())
    }

    pub(crate) fn get(&self, id: &str) -> Option<&str> {
        self.find(id).map(|i| self.entries[i].1.as_str()).filter(|v| !v.is_empty())
    }

    pub(crate) fn set(&mut self, id: &str, value: &str) {
        match self.find(id) {
            Some(i) => self.entries[i].1 = value.to_string(),
            None => self.entries.push((id.to_string(), value.to_string())),
        }
        self.save();
    }

    pub(crate) fn remove(&mut self, id: &str) {
        if let Some(i) = self.find(id) {
            self.entries.remove(i);
        }
        self.save();
    }

    /// Written beside the target then swapped in, so a crash mid-write can't leave a truncated
    /// file. Failures are ignored: the map in memory still holds.
    fn save(&self) {
        let map: Map<String, Value> =
            self.entries.iter().map(|(k, v)| (k.clone(), Value::String(v.clone()))).collect();
        let Ok(json) = serde_json::to_string_pretty(&Value::Object(map)) else { return };
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &self.path);
        }
    }
}

/// None for a missing, unreadable or corrupt file (any value that is not a string).
fn load(path: &Path) -> Option<Vec<(String, String)>> {
    let text = std::fs::read_to_string(path).ok()?;
    let Value::Object(map) = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()? else {
        return None;
    };
    let mut entries: Vec<(String, String)> = Vec::with_capacity(map.len());
    for (k, v) in map {
        let v = v.as_str()?.to_string();
        // Ids differing only in case are one id; the later one wins.
        entries.retain(|(e, _)| e.to_lowercase() != k.to_lowercase());
        entries.push((k, v));
    }
    Some(entries)
}

/// `names.json`: your own names for sessions.
#[derive(Debug)]
pub struct SessionNames(IdMap);

impl SessionNames {
    /// Loads `<data>\names.json`, creating the folder.
    pub fn open(data: &DataDir) -> Self {
        Self(IdMap::open(data, "names.json"))
    }

    pub fn get(&self, session_id: &str) -> Option<&str> {
        self.0.get(session_id)
    }

    /// Blank or whitespace clears the override, so the generated title shows again.
    pub fn set(&mut self, session_id: &str, name: Option<&str>) {
        match name.map(str::trim).filter(|n| !n.is_empty()) {
            Some(n) => self.0.set(session_id, n),
            None => self.0.remove(session_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path().join("Kilno"));
        let mut n = SessionNames::open(&data);
        n.set("AbC", Some("  my name "));
        assert_eq!(n.get("abc"), Some("my name"));
        let again = SessionNames::open(&data);
        assert_eq!(again.get("ABC"), Some("my name"));
        n.set("abc", Some("   "));
        assert_eq!(n.get("abc"), None);
        assert_eq!(SessionNames::open(&data).get("abc"), None);
        assert!(!data.join("names.json.tmp").exists());
    }

    #[test]
    fn corrupt_file_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        std::fs::write(data.join("names.json"), "{ nope").unwrap();
        assert_eq!(SessionNames::open(&data).get("a"), None);
        std::fs::write(data.join("names.json"), r#"{"a": 3, "b": "x"}"#).unwrap();
        assert_eq!(SessionNames::open(&data).get("b"), None);
        std::fs::write(data.join("names.json"), "{\n  \"a\": \"x\"\n}").unwrap();
        assert_eq!(SessionNames::open(&data).get("A"), Some("x"));
    }
}
