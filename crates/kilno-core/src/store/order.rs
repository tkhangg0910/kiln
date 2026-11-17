//! The order you dragged sessions into (`order.json`): a rank per top-level session, lower
//! first. Branches follow their session; one never dragged has no rank and sits on top, newest
//! first, so a new session is where you look.

use super::names::IdMap;
use crate::paths::DataDir;

#[derive(Debug)]
pub struct SessionOrder(IdMap);

impl SessionOrder {
    /// Loads `<data>\order.json`, creating the folder.
    pub fn open(data: &DataDir) -> Self {
        Self(IdMap::open(data, "order.json"))
    }

    pub fn rank(&self, session_id: &str) -> Option<i64> {
        self.0.get(session_id).and_then(|v| v.parse().ok())
    }

    /// Nothing was ever dragged.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// `ids` in this order, ranked 0, 1, 2…: the sessions of one section after a drop. Ranks
    /// only matter between rows of the same section, so other sections' are left as they are.
    pub fn set(&mut self, ids: &[String]) {
        self.0.set_many(ids.iter().enumerate().map(|(i, id)| (id.clone(), i.to_string())));
    }
}

/// `ids` with `moved` taken out and put back just before (or after) `target`. Unchanged when
/// either is missing or they are the same.
pub fn move_to(ids: &[String], moved: &str, target: &str, after: bool) -> Vec<String> {
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    if same(moved, target) || !ids.iter().any(|i| same(i, moved)) {
        return ids.to_vec();
    }
    let mut out: Vec<String> = ids.iter().filter(|i| !same(i, moved)).cloned().collect();
    let Some(at) = out.iter().position(|i| same(i, target)) else {
        return ids.to_vec();
    };
    let moved = ids.iter().find(|i| same(i, moved)).expect("checked above").clone();
    out.insert(if after { at + 1 } else { at }, moved);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn moves_before_or_after() {
        let ids = v(&["a", "b", "c", "d"]);
        assert_eq!(move_to(&ids, "d", "b", false), v(&["a", "d", "b", "c"]));
        assert_eq!(move_to(&ids, "a", "c", true), v(&["b", "c", "a", "d"]));
        assert_eq!(move_to(&ids, "A", "d", true), v(&["b", "c", "d", "a"]), "ids ignore case");
        assert_eq!(move_to(&ids, "b", "b", true), ids, "onto itself");
        assert_eq!(move_to(&ids, "x", "b", true), ids, "not in the list");
        assert_eq!(move_to(&ids, "b", "x", true), ids, "no target");
    }

    #[test]
    fn ranks_persist() {
        let dir = tempfile::tempdir().unwrap();
        let data = DataDir::new(dir.path());
        let mut o = SessionOrder::open(&data);
        assert!(o.is_empty());
        o.set(&v(&["c", "a"]));
        let again = SessionOrder::open(&data);
        assert_eq!((again.rank("C"), again.rank("a"), again.rank("b")), (Some(0), Some(1), None));
    }
}
