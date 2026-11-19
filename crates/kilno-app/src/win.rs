//! The Win32 wrapper layer of the app: the only module (with kilno-pty and kilno-island) where
//! `unsafe` is allowed. Each block says why it is sound.

use std::path::{Path, PathBuf};

#[cfg(windows)]
pub mod http;
#[cfg(windows)]
pub mod instance;
#[cfg(windows)]
pub mod shell;
#[cfg(windows)]
pub mod tray;

/// Map a font file read-only and keep it mapped for the life of the process. The bytes are
/// file-backed, so they count as shared, not private, memory (PORTING.md §1 fonts).
pub fn map_font(path: &Path) -> Option<&'static [u8]> {
    let file = std::fs::File::open(path).ok()?;
    // SAFETY: system font files are not modified while Windows has them installed; the map is
    // read-only and leaked, so the slice stays valid for 'static.
    let map = unsafe { memmap2::Mmap::map(&file) }.ok()?;
    let map: &'static memmap2::Mmap = Box::leak(Box::new(map));
    Some(&map[..])
}

/// `C:\Windows\Fonts`.
pub fn fonts_dir() -> PathBuf {
    let windir = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    windir.join("Fonts")
}

#[cfg(windows)]
mod imp {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWINDOWATTRIBUTE};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalUnlock};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};

    const CF_UNICODETEXT: u32 = 13;
    const CF_HDROP: u32 = 15;

    /// Dark caption, acrylic backdrop and the hairline border colour, as the C# window had.
    /// Failures (older Windows builds) are ignored.
    pub fn dark_caption(hwnd: isize) {
        let hwnd = HWND(hwnd as *mut _);
        let set = |attr: i32, value: u32| {
            // SAFETY: `value` lives for the call; size matches a DWORD attribute.
            let _ = unsafe {
                DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(attr), &value as *const u32 as *const _, 4)
            };
        };
        set(20, 1); // DWMWA_USE_IMMERSIVE_DARK_MODE
        set(38, 3); // DWMWA_SYSTEMBACKDROP_TYPE: transient (acrylic)
        set(34, 0x0035_2C26); // DWMWA_BORDER_COLOR #262C35 as COLORREF (BGR)
    }

    /// The caption's colour (None: Windows' dark one, with the hairline border), its text
    /// colour and the backdrop (DWMSBT_*: 1 none, 2 mica, 3 acrylic, 4 tabbed). Windows 11;
    /// older builds ignore the attributes.
    pub fn caption_colors(hwnd: isize, color: Option<[u8; 3]>, text: [u8; 3], backdrop: u32) {
        let hwnd = HWND(hwnd as *mut _);
        let set = |attr: i32, value: u32| {
            // SAFETY: `value` lives for the call; size matches a DWORD attribute.
            let _ = unsafe {
                DwmSetWindowAttribute(hwnd, DWMWINDOWATTRIBUTE(attr), &value as *const u32 as *const _, 4)
            };
        };
        let bgr = |[r, g, b]: [u8; 3]| u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16;
        const DEFAULT: u32 = 0xFFFF_FFFF; // DWMWA_COLOR_DEFAULT
        set(38, backdrop); // DWMWA_SYSTEMBACKDROP_TYPE
        set(35, color.map_or(DEFAULT, bgr)); // DWMWA_CAPTION_COLOR
        set(36, if color.is_some() { bgr(text) } else { DEFAULT }); // DWMWA_TEXT_COLOR
        set(34, color.map_or(0x0035_2C26, bgr)); // DWMWA_BORDER_COLOR, #262C35 by default
    }

    /// What the clipboard holds that a paste can use.
    pub enum Clip {
        Text(String),
        Files(Vec<String>),
        Nothing,
    }

    struct Open;
    impl Open {
        fn new() -> Option<Self> {
            // Another app may hold the clipboard for a moment: retry a few times.
            for _ in 0..5 {
                // SAFETY: no owner window needed for reading.
                if unsafe { OpenClipboard(None) }.is_ok() {
                    return Some(Open);
                }
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            None
        }
    }
    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: paired with the successful OpenClipboard in `new`.
            let _ = unsafe { CloseClipboard() };
        }
    }

    /// Text first (a text paste wins), else a file list copied in Explorer.
    pub fn read_clipboard() -> Clip {
        let Some(_open) = Open::new() else { return Clip::Nothing };
        // SAFETY: the clipboard is open; the handle stays valid until it is closed, and is
        // locked while read.
        unsafe {
            if IsClipboardFormatAvailable(CF_UNICODETEXT).is_ok() {
                if let Ok(h) = GetClipboardData(CF_UNICODETEXT) {
                    let g = HGLOBAL(h.0);
                    let p = GlobalLock(g) as *const u16;
                    if !p.is_null() {
                        let mut len = 0;
                        while *p.add(len) != 0 {
                            len += 1;
                        }
                        let s = String::from_utf16_lossy(std::slice::from_raw_parts(p, len));
                        let _ = GlobalUnlock(g);
                        if !s.is_empty() {
                            return Clip::Text(s);
                        }
                    }
                }
            }
            if IsClipboardFormatAvailable(CF_HDROP).is_ok() {
                if let Ok(h) = GetClipboardData(CF_HDROP) {
                    let drop = HDROP(h.0);
                    let n = DragQueryFileW(drop, u32::MAX, None);
                    let mut files = Vec::new();
                    for i in 0..n {
                        let len = DragQueryFileW(drop, i, None) as usize;
                        let mut buf = vec![0u16; len + 1];
                        DragQueryFileW(drop, i, Some(&mut buf));
                        files.push(String::from_utf16_lossy(&buf[..len]));
                    }
                    if !files.is_empty() {
                        return Clip::Files(files);
                    }
                }
            }
        }
        Clip::Nothing
    }

    /// The primary monitor's work area in logical points (x, y, w, h), for first placement:
    /// WPF's CenterScreen misplaced windows on mixed-DPI setups, so the C# build centred on
    /// the primary work area itself and clamped the size to it.
    pub fn primary_work_area() -> Option<(f32, f32, f32, f32)> {
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::HiDpi::GetDpiForSystem;
        use windows::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETWORKAREA, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS};
        let mut r = RECT::default();
        // SAFETY: SPI_GETWORKAREA writes one RECT into the live local.
        unsafe { SystemParametersInfoW(SPI_GETWORKAREA, 0, Some(&mut r as *mut RECT as *mut _), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) }.ok()?;
        // SAFETY: no arguments; returns the system DPI.
        let scale = unsafe { GetDpiForSystem() } as f32 / 96.0;
        Some((r.left as f32 / scale, r.top as f32 / scale, (r.right - r.left) as f32 / scale, (r.bottom - r.top) as f32 / scale))
    }

    #[allow(dead_code)]
    pub(crate) fn _unused(_: HANDLE) {}
}

#[cfg(windows)]
pub use imp::*;
