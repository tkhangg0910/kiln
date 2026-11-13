//! Font files memory-mapped for the life of the process.

use std::path::Path;

/// The file mapped read-only and leaked, or None when it can't be opened.
pub fn map_font(path: &Path) -> Option<&'static [u8]> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: installed system font files are not modified while Windows has them in use; the
    // map is read-only and leaked, so the slice stays valid for 'static.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let map: &'static memmap2::Mmap = Box::leak(Box::new(map));
    Some(&map[..])
}
