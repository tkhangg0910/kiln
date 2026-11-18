//! A page an agent wrote, opened in the default browser (PORTING.md D3): a sealed copy under
//! `%LOCALAPPDATA%\Kilno\pages\<hash>\` with Kilno's page assets beside it.

use kilno_core::pages;
use std::path::{Path, PathBuf};

macro_rules! asset {
    ($name:literal) => {
        ($name, include_bytes!(concat!("../../../assets/pages/", $name)) as &[u8])
    };
}

/// What [`kilno_core::pages::ASSET_FILES`] names, built into the exe.
const ASSETS: [(&str, &[u8]); 4] =
    [asset!("clayo-page.css"), asset!("clayo-page.js"), asset!("chart.umd.min.js"), asset!("mermaid.min.js")];

/// Writes the copy and opens it, off the UI thread (the first copy writes ~5.7 MB of assets).
pub fn open(copies: PathBuf, page: &Path) {
    let page = page.to_path_buf();
    let _ = std::thread::Builder::new().name("open page".into()).spawn(move || {
        if let Ok(copy) = pages::browser_copy(&copies, &page, &ASSETS) {
            crate::os::shell::open(&copy.to_string_lossy());
            #[cfg(not(windows))]
            let _ = copy;
        }
    });
}

/// Copies older than a week go, at start (as the screenshots do).
pub fn prune(copies: PathBuf) {
    let _ = std::thread::Builder::new().name("prune pages".into()).spawn(move || {
        pages::prune_copies(&copies, std::time::SystemTime::now() - pages::KEEP_FOR);
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_asset_a_page_links_is_built_in() {
        let names: Vec<&str> = super::ASSETS.iter().map(|a| a.0).collect();
        assert_eq!(names, kilno_core::pages::ASSET_FILES);
        assert!(super::ASSETS.iter().all(|a| !a.1.is_empty()));
    }
}
