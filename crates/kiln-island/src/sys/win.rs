//! Safe wrappers over the Win32 calls the island makes (PORTING.md §4.2, §4.3, Appendix D
//! HARD POINTS). Each `unsafe` block names why it is sound.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;

use kiln_core::island::trigger::PxRect;
use tiny_skia::Pixmap;
use windows::core::{implement, w, Ref, BOOL, PCWSTR};
use windows::Win32::Foundation::{
    COLORREF, HGLOBAL, HINSTANCE, HWND, LPARAM, LRESULT, POINT, POINTL, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, GetMonitorInfoW,
    MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardSequenceNumber,
    IsClipboardFormatAvailable, OpenClipboard, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::{
    IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize, RegisterDragDrop,
    ReleaseStgMedium, RevokeDragDrop, CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP, DROPEFFECT,
    DROPEFFECT_COPY, DROPEFFECT_NONE,
};
use windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS;
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};
use windows::Win32::UI::Shell::{
    DragQueryFileW, SHQueryUserNotificationState, HDROP, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_QUIET_TIME, QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetAncestor, GetClassNameW,
    GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetMessageW, GetSystemMetrics,
    GetWindowLongPtrW, GetWindowRect, IsIconic, IsWindowVisible, KillTimer, LoadCursorW,
    PostMessageW, PostQuitMessage, RegisterClassExW, SetCursor, SetTimer, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, SystemParametersInfoW, UpdateLayeredWindow, GA_ROOTOWNER,
    GUITHREADINFO, GUI_INMOVESIZE, GWL_EXSTYLE, HWND_TOPMOST, IDC_ARROW, IDC_HAND, MSG,
    SM_SWAPBUTTON, SPI_GETCLIENTAREAANIMATION, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SWP_SHOWWINDOW, SW_HIDE, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, ULW_ALPHA, WINDOW_EX_STYLE,
    WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
    WS_EX_TRANSPARENT, WS_POPUP,
};

/// A window this thread created. Plain handle; only used on the island thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window(HWND);

impl Window {
    pub fn raw(self) -> isize {
        self.0 .0 as isize
    }
}

/// Posts `msg` to a window by its raw handle (any thread may).
pub fn post(hwnd: isize, msg: u32) -> bool {
    // SAFETY: PostMessageW only queues a message; a stale handle makes it fail, nothing more.
    unsafe { PostMessageW(Some(HWND(hwnd as *mut c_void)), msg, WPARAM(0), LPARAM(0)) }.is_ok()
}

/// What the window procedure and the drop target hand to the island.
pub trait Handler {
    /// A window message; Some(result) when handled.
    fn message(&mut self, w: Window, msg: u32, wp: usize, lp: isize) -> Option<isize>;
    /// A file drag over the island (`Some(first path)` for a file drop, None for anything
    /// else). Returns whether a drop would be taken.
    fn drag_over(&mut self, path: Option<&std::path::Path>) -> bool;
    fn drag_leave(&mut self);
    fn drop_path(&mut self, path: Option<PathBuf>);
}

thread_local! {
    static HANDLER: RefCell<Option<Box<dyn Handler>>> = const { RefCell::new(None) };
}

/// Installs the handler for this thread's island windows.
pub fn set_handler(h: Option<Box<dyn Handler>>) {
    HANDLER.with(|c| {
        if let Ok(mut slot) = c.try_borrow_mut() {
            *slot = h;
        }
    });
}

/// Runs `f` on the handler unless it is already busy (a re-entrant message): then None.
fn with_handler<R>(f: impl FnOnce(&mut dyn Handler) -> R) -> Option<R> {
    HANDLER.with(|c| {
        let mut slot = c.try_borrow_mut().ok()?;
        let h = slot.as_mut()?;
        Some(f(h.as_mut()))
    })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if let Some(Some(r)) = with_handler(|h| h.message(Window(hwnd), msg, wp.0, lp.0)) {
        return LRESULT(r);
    }
    // SAFETY: the default procedure for our own window, with the arguments Windows passed us.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// The island thread: per-monitor DPI v2 (placement is in physical px) and OLE for drops.
pub fn init_thread() -> bool {
    // SAFETY: plain calls on the current thread; OleInitialize is balanced by `uninit_thread`.
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        OleInitialize(None).is_ok()
    }
}

pub fn uninit_thread() {
    // SAFETY: balances the OleInitialize in `init_thread` on the same thread.
    unsafe { OleUninitialize() }
}

const CLASS: PCWSTR = w!("KilnIsland");

/// A layered popup that never activates and stays out of Alt+Tab; `transparent` makes it
/// click-through as a whole (the glow).
pub fn create_window(title: &str, transparent: bool) -> Option<Window> {
    // SAFETY: registering a class with a static name and our wndproc; a second registration
    // fails harmlessly. CreateWindowExW gets valid null-terminated strings that outlive it.
    unsafe {
        let instance = GetModuleHandleW(None).ok()?;
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: HINSTANCE(instance.0),
            lpszClassName: CLASS,
            ..Default::default()
        };
        RegisterClassExW(&class);
        let mut ex = WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        if transparent {
            ex |= WS_EX_TRANSPARENT;
        }
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let hwnd = CreateWindowExW(
            ex,
            CLASS,
            PCWSTR(title.as_ptr()),
            WS_POPUP,
            0,
            0,
            1,
            1,
            None,
            None,
            Some(HINSTANCE(instance.0)),
            None,
        )
        .ok()?;
        Some(Window(hwnd))
    }
}

pub fn destroy(w: Window) {
    // SAFETY: our own window, destroyed on the thread that created it.
    let _ = unsafe { DestroyWindow(w.0) };
}

/// Runs the thread's message loop until WM_QUIT.
pub fn run_loop() {
    let mut msg = MSG::default();
    // SAFETY: the standard loop over this thread's queue with a valid MSG.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            DispatchMessageW(&msg);
        }
    }
}

pub fn quit_loop() {
    // SAFETY: posts WM_QUIT to the current thread.
    unsafe { PostQuitMessage(0) }
}

pub fn set_timer(w: Window, id: usize, ms: u32) {
    // SAFETY: a timer on our own window; re-setting the same id replaces it.
    unsafe { SetTimer(Some(w.0), id, ms, None) };
}

pub fn kill_timer(w: Window, id: usize) {
    // SAFETY: as above; a missing timer just fails.
    let _ = unsafe { KillTimer(Some(w.0), id) };
}

/// WS_EX_TRANSPARENT on or off: clicks pass through the whole window.
pub fn set_click_through(w: Window, on: bool) {
    // SAFETY: reads and writes our own window's extended style.
    unsafe {
        let style = GetWindowLongPtrW(w.0, GWL_EXSTYLE);
        let t = WS_EX_TRANSPARENT.0 as isize;
        let want = if on { style | t } else { style & !t };
        if want != style {
            SetWindowLongPtrW(w.0, GWL_EXSTYLE, want);
        }
    }
}

pub fn ex_style(w: Window) -> WINDOW_EX_STYLE {
    // SAFETY: reads our own window's extended style.
    WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(w.0, GWL_EXSTYLE) } as u32)
}

/// Shows without activating, in the topmost band, just below `after` when given.
pub fn show_below(w: Window, after: Option<Window>) {
    let insert = after.map_or(HWND_TOPMOST, |a| a.0);
    // SAFETY: z-order/show change of our own window; never activates.
    let _ = unsafe {
        SetWindowPos(w.0, Some(insert), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW)
    };
}

pub fn hide(w: Window) {
    // SAFETY: hides our own window.
    let _ = unsafe { ShowWindow(w.0, SW_HIDE) };
}

pub fn is_visible(w: Window) -> bool {
    // SAFETY: query on our own window.
    unsafe { IsWindowVisible(w.0) }.as_bool()
}

/// A 32-bit top-down DIB section the layered window is updated from, reused across frames.
pub struct Surface {
    dc: HDC,
    bmp: HBITMAP,
    old: HGDIOBJ,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Surface {
    fn new(w: i32, h: i32) -> Option<Surface> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: a memory DC and a DIB section owned by the returned Surface, released in Drop.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return None;
            }
            let mut bits: *mut c_void = std::ptr::null_mut();
            let Ok(bmp) = CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) else {
                let _ = DeleteDC(dc);
                return None;
            };
            let old = SelectObject(dc, HGDIOBJ(bmp.0));
            Some(Surface {
                dc,
                bmp,
                old,
                bits: bits.cast(),
                w,
                h,
            })
        }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: undoes `new` in reverse order; the handles are ours and used nowhere else.
        unsafe {
            SelectObject(self.dc, self.old);
            let _ = DeleteObject(HGDIOBJ(self.bmp.0));
            let _ = DeleteDC(self.dc);
        }
    }
}

/// Puts `pm` (premultiplied RGBA) on the layered window at (x, y) physical px with
/// per-pixel alpha. Pixels of alpha 0 then take no clicks.
pub fn present(w: Window, surface: &mut Option<Surface>, x: i32, y: i32, pm: &Pixmap) -> bool {
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    if surface.as_ref().is_none_or(|s| s.w != pw || s.h != ph) {
        *surface = None;
        *surface = Surface::new(pw, ph);
    }
    let Some(s) = surface.as_ref() else { return false };
    let len = (pw * ph * 4) as usize;
    // SAFETY: the DIB section holds exactly w*h 32-bit pixels and lives as long as `s`.
    let dst = unsafe { std::slice::from_raw_parts_mut(s.bits, len) };
    for (d, p) in dst.as_chunks_mut::<4>().0.iter_mut().zip(pm.data().as_chunks::<4>().0) {
        d[0] = p[2];
        d[1] = p[1];
        d[2] = p[0];
        d[3] = p[3];
    }
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    let pos = POINT { x, y };
    let size = SIZE { cx: pw, cy: ph };
    let src = POINT { x: 0, y: 0 };
    // SAFETY: the screen DC is released below; every pointer is to a local that outlives the call.
    unsafe {
        let screen = GetDC(None);
        let ok = UpdateLayeredWindow(
            w.0,
            Some(screen),
            Some(&pos),
            Some(&size),
            Some(s.dc),
            Some(&src),
            COLORREF(0),
            Some(&blend),
            ULW_ALPHA,
        )
        .is_ok();
        ReleaseDC(None, screen);
        ok
    }
}

// ------------------------------------------------------------------- probes

pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut pt = POINT::default();
    // SAFETY: writes into a local POINT. Fails on the secure desktop.
    unsafe { GetCursorPos(&mut pt) }.ok()?;
    Some((pt.x, pt.y))
}

fn monitor_info(m: windows::Win32::Graphics::Gdi::HMONITOR) -> Option<RECT> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: a valid monitor handle and a sized MONITORINFO.
    unsafe { GetMonitorInfoW(m, &mut info) }.as_bool().then_some(info.rcMonitor)
}

/// The monitor nearest (x, y): its bounds and its DPI scale (1.0 when unknown).
pub fn monitor_at(x: i32, y: i32) -> Option<(PxRect, f64)> {
    // SAFETY: plain lookups; outputs are locals.
    let m = unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) };
    let r = monitor_info(m)?;
    let (mut dx, mut dy) = (0u32, 0u32);
    // SAFETY: as above.
    let scale = match unsafe { GetDpiForMonitor(m, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) } {
        Ok(()) if dx > 0 => f64::from(dx) / 96.0,
        _ => 1.0,
    };
    Some((PxRect::new(r.left, r.top, r.right, r.bottom), scale))
}

pub fn key_down(vk: i32) -> bool {
    // SAFETY: a stateless query.
    (unsafe { GetAsyncKeyState(vk) } as u16 & 0x8000) != 0
}

/// The button that drags windows: the right one when the buttons are swapped.
pub fn primary_button_down() -> bool {
    // SAFETY: a stateless query.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    key_down(if swapped { 0x02 } else { 0x01 })
}

pub fn any_button_down() -> bool {
    key_down(0x01) || key_down(0x02)
}

fn gui_info() -> Option<GUITHREADINFO> {
    let mut gti = GUITHREADINFO {
        cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: thread 0 = the foreground thread; writes a sized local.
    unsafe { GetGUIThreadInfo(0, &mut gti) }.ok()?;
    Some(gti)
}

fn class_name(hwnd: HWND) -> String {
    let mut buf = [0u16; 64];
    // SAFETY: writes at most buf.len() chars into the local buffer.
    let n = unsafe { GetClassNameW(hwnd, &mut buf) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

/// A window is being moved or sized on the foreground thread.
pub fn in_move_size() -> bool {
    gui_info().is_some_and(|g| g.flags.contains(GUI_INMOVESIZE))
}

/// An OLE drag (a file from Explorer, a link from a browser) runs on the foreground thread:
/// ole32's tracking window CLIPBRDWNDCLASS holds the mouse capture.
pub fn in_ole_drag() -> bool {
    match gui_info() {
        Some(g) if !g.hwndCapture.is_invalid() => class_name(g.hwndCapture) == "CLIPBRDWNDCLASS",
        _ => false,
    }
}

fn foreground_fills_monitor() -> bool {
    // SAFETY: queries on whatever window is in front; a stale handle just fails.
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() || !IsWindowVisible(hwnd).as_bool() || IsIconic(hwnd).as_bool() {
            return false;
        }
        let name = class_name(hwnd);
        if name == "Progman" || name == "WorkerW" {
            return false;
        }
        let mut r = RECT::default();
        if GetWindowRect(hwnd, &mut r).is_err() {
            return false;
        }
        let Some(m) = monitor_info(MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST)) else {
            return false;
        };
        r.left <= m.left && r.top <= m.top && r.right >= m.right && r.bottom >= m.bottom
    }
}

/// Fullscreen game, presentation, Do Not Disturb; a plain QUNS_BUSY only when the foreground
/// window really covers its monitor (the shell also says busy over an empty desktop).
pub fn is_busy() -> bool {
    // SAFETY: a plain shell query.
    let Ok(state) = (unsafe { SHQueryUserNotificationState() }) else {
        return false;
    };
    if state == QUNS_BUSY {
        return foreground_fills_monitor();
    }
    state == QUNS_RUNNING_D3D_FULL_SCREEN || state == QUNS_PRESENTATION_MODE || state == QUNS_QUIET_TIME
}

/// `hwnd` (a top-level window) is what you are looking at: visible, not minimized, and the
/// foreground window or its root owner (an owned dialog such as Setup counts).
pub fn window_in_front(hwnd: isize) -> bool {
    if hwnd == 0 {
        return false;
    }
    let main = HWND(hwnd as *mut c_void);
    // SAFETY: queries; a stale handle just reads as not visible.
    unsafe {
        if !IsWindowVisible(main).as_bool() || IsIconic(main).as_bool() {
            return false;
        }
        let front = GetAncestor(GetForegroundWindow(), GA_ROOTOWNER);
        !front.is_invalid() && front == main
    }
}

/// Windows' "Show animations in Windows" (client-area animations).
pub fn client_area_animation() -> bool {
    let mut on = BOOL(1);
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL into the local.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some(&mut on as *mut BOOL as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_err() || on.as_bool()
}

pub fn set_cursor(hand: bool) {
    // SAFETY: a shared system cursor; SetCursor only affects our window under the mouse.
    unsafe {
        if let Ok(c) = LoadCursorW(None, if hand { IDC_HAND } else { IDC_ARROW }) {
            SetCursor(Some(c));
        }
    }
}

// ------------------------------------------------------------- hotkeys, clipboard

pub fn register_hotkey(w: Window, id: i32, mods: u32, vk: u32) -> bool {
    // SAFETY: a hotkey on our own window, unregistered by `unregister_hotkey`.
    unsafe { RegisterHotKey(Some(w.0), id, HOT_KEY_MODIFIERS(mods), vk) }.is_ok()
}

pub fn unregister_hotkey(w: Window, id: i32) {
    // SAFETY: as above.
    let _ = unsafe { UnregisterHotKey(Some(w.0), id) };
}

pub fn add_clipboard_listener(w: Window) -> bool {
    // SAFETY: registers our own window; removed by `remove_clipboard_listener`.
    unsafe { AddClipboardFormatListener(w.0) }.is_ok()
}

pub fn remove_clipboard_listener(w: Window) {
    // SAFETY: as above.
    let _ = unsafe { RemoveClipboardFormatListener(w.0) };
}

pub fn clipboard_seq() -> u32 {
    // SAFETY: a stateless query.
    unsafe { GetClipboardSequenceNumber() }
}

/// An image is on the clipboard (any of CF_BITMAP, CF_DIB, CF_DIBV5).
pub fn clipboard_has_image() -> bool {
    // SAFETY: stateless queries; no clipboard ownership needed.
    unsafe {
        [CF_BITMAP, CF_DIB, CF_DIBV5]
            .iter()
            .any(|f| IsClipboardFormatAvailable(u32::from(f.0)).is_ok())
    }
}

/// The clipboard's image as a packed DIB, retrying while another app holds the clipboard
/// (5 × 50 ms). None when there is no image or it stays locked.
pub fn clipboard_dib(owner: Window) -> Option<Vec<u8>> {
    for attempt in 0..=5 {
        // SAFETY: OpenClipboard/CloseClipboard are paired; the global memory is locked only
        // while copied, and its size bounds the copy.
        let got = unsafe {
            if OpenClipboard(Some(owner.0)).is_err() {
                None
            } else {
                let mut out = None;
                for f in [CF_DIB, CF_DIBV5] {
                    if let Ok(h) = GetClipboardData(u32::from(f.0)) {
                        let g = HGLOBAL(h.0);
                        let p = GlobalLock(g) as *const u8;
                        if !p.is_null() {
                            let n = GlobalSize(g);
                            out = Some(std::slice::from_raw_parts(p, n).to_vec());
                            let _ = GlobalUnlock(g);
                            break;
                        }
                    }
                }
                let _ = CloseClipboard();
                Some(out)
            }
        };
        match got {
            Some(dib) => return dib,
            None if attempt < 5 => std::thread::sleep(std::time::Duration::from_millis(50)),
            None => {}
        }
    }
    None
}

// ---------------------------------------------------------------------- drop

#[implement(IDropTarget)]
struct DropTarget {
    path: RefCell<Option<PathBuf>>,
}

fn first_path(data: &IDataObject) -> Option<PathBuf> {
    let fmt = FORMATETC {
        cfFormat: CF_HDROP.0,
        ptd: std::ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    };
    // SAFETY: GetData hands us a medium we release below; DragQueryFileW reads the HDROP in it
    // into a buffer of the length it reported.
    unsafe {
        let mut medium = data.GetData(&fmt).ok()?;
        let hdrop = HDROP(medium.u.hGlobal.0);
        let n = DragQueryFileW(hdrop, u32::MAX, None);
        let mut path = None;
        if n > 0 {
            let len = DragQueryFileW(hdrop, 0, None) as usize;
            let mut buf = vec![0u16; len + 1];
            let got = DragQueryFileW(hdrop, 0, Some(&mut buf)) as usize;
            if got > 0 {
                path = Some(PathBuf::from(String::from_utf16_lossy(&buf[..got])));
            }
        }
        ReleaseStgMedium(&mut medium);
        path
    }
}

fn set_effect(effect: *mut DROPEFFECT, take: bool) {
    if !effect.is_null() {
        // SAFETY: OLE passes a valid in/out DROPEFFECT pointer.
        unsafe { *effect = if take { DROPEFFECT_COPY } else { DROPEFFECT_NONE } };
    }
}

impl IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(&self, data: Ref<IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let path = data.as_ref().and_then(first_path);
        let take = with_handler(|h| h.drag_over(path.as_deref())).unwrap_or(false);
        *self.path.borrow_mut() = path;
        set_effect(effect, take);
        Ok(())
    }

    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let path = self.path.borrow().clone();
        let take = with_handler(|h| h.drag_over(path.as_deref())).unwrap_or(false);
        set_effect(effect, take);
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        *self.path.borrow_mut() = None;
        with_handler(|h| h.drag_leave());
        Ok(())
    }

    fn Drop(&self, data: Ref<IDataObject>, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> windows::core::Result<()> {
        let path = data.as_ref().and_then(first_path);
        *self.path.borrow_mut() = None;
        let take = path.is_some();
        set_effect(effect, take);
        with_handler(|h| h.drop_path(path));
        Ok(())
    }
}

/// Registers the island window as an OLE drop target.
pub fn register_drop(w: Window) -> bool {
    let target: IDropTarget = DropTarget {
        path: RefCell::new(None),
    }
    .into();
    // SAFETY: OLE is initialised on this thread (init_thread); OLE keeps its own reference.
    unsafe { RegisterDragDrop(w.0, &target) }.is_ok()
}

pub fn revoke_drop(w: Window) {
    // SAFETY: undoes register_drop on the same window.
    let _ = unsafe { RevokeDragDrop(w.0) };
}
