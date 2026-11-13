//! Saving the clipboard's screenshot (Screenshots.SaveFromClipboard): only when a session was
//! picked, as `shots\yyyy-MM-dd_HH-mm-ss[-n].png`, alpha dropped.

use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

/// Writes `dib` as a new PNG in `dir` named for `now`. None when the bitmap can't be read or
/// the file can't be written.
pub fn save_dib(dir: &Path, now: NaiveDateTime, dib: &[u8]) -> Option<PathBuf> {
    let png = crate::dib::dib_to_png(dib).ok()?;
    std::fs::create_dir_all(dir).ok()?;
    let path = kiln_core::screenshots::new_path(dir, now);
    std::fs::write(&path, png).ok()?;
    Some(path)
}

/// The clipboard's image saved in `dir`, or None if there is none (or it stays locked).
#[cfg(windows)]
pub(crate) fn save_from_clipboard(owner: crate::sys::win::Window, dir: &Path) -> Option<PathBuf> {
    if !crate::sys::win::clipboard_has_image() {
        return None;
    }
    let dib = crate::sys::win::clipboard_dib(owner)?;
    save_dib(dir, chrono::Local::now().naive_local(), &dib)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_a_png_named_for_the_second() {
        let dir = std::env::temp_dir().join(format!("kiln-island-shots-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut dib = vec![0u8; 40];
        dib[0] = 40;
        dib[4] = 1; // width 1
        dib[8] = 1; // height 1
        dib[12] = 1; // planes
        dib[14] = 32; // bpp
        dib.extend_from_slice(&[1, 2, 3, 0]);
        let now = NaiveDateTime::parse_from_str("2026-10-06 19:02:11", "%Y-%m-%d %H:%M:%S").expect("date");
        let a = save_dib(&dir, now, &dib).expect("saved");
        let b = save_dib(&dir, now, &dib).expect("saved");
        assert_eq!(a.file_name().and_then(|n| n.to_str()), Some("2026-10-06_19-02-11.png"));
        assert_eq!(b.file_name().and_then(|n| n.to_str()), Some("2026-10-06_19-02-11-2.png"));
        let bytes = std::fs::read(&a).expect("read");
        assert_eq!(&bytes[1..4], b"PNG");
        assert!(save_dib(&dir, now, &[0, 1]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
