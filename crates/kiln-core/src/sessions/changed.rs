//! The transcripts a watcher saw change since the last scan, so a scan reads only those.
//! Listing every transcript on each refresh cost ~16 ms of the UI thread at 220 sessions, and
//! grew with history.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Shared between the watcher's thread and the scanning thread.
#[derive(Debug)]
pub struct ChangedFiles {
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    /// Keyed case-insensitively, as Windows paths compare.
    paths: HashMap<String, PathBuf>,
    all: bool,
}

impl Default for ChangedFiles {
    fn default() -> Self {
        Self { inner: Mutex::new(Inner { paths: HashMap::new(), all: true }) }
    }
}

impl ChangedFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, path: &Path) {
        let key = path.to_string_lossy().to_lowercase();
        self.lock().paths.entry(key).or_insert_with(|| path.to_path_buf());
    }

    /// The watcher lost track (its buffer overflowed), so the next scan looks at every file.
    pub fn add_all(&self) {
        self.lock().all = true;
    }

    /// What a scan should look at now, as (every file?, these files): every file until a
    /// watcher runs, and once more on its first scan, which covers what changed before it
    /// started; after that, what it reported.
    pub fn take(&self, watching: bool) -> (bool, Vec<PathBuf>) {
        let mut inner = self.lock();
        let all = inner.all || !watching;
        let paths = inner.paths.drain().map(|(_, p)| p).collect();
        inner.all = !watching;
        (all, paths)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}
