//! The notification-area (system tray) icon: Kilno keeps running with its window closed, and
//! the icon shows that it does, next to the island. A click brings the window back; the right
//! click menu has Show Kilno and Quit Kilno.
//!
//! The icon has its own thread and a hidden window of its own (not message-only: those never
//! get Explorer's TaskbarCreated broadcast, which is the cue to add the icon again after
//! Explorer restarts). [`remove`] takes it away before the process exits, so no dead icon
//! lingers until the mouse passes over it.

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::OnceLock;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW, GetCursorPos,
    GetMessageW, GetSystemMetrics, LoadImageW, PostMessageW, RegisterClassW, RegisterWindowMessageW,
    SetForegroundWindow, TrackPopupMenu, TranslateMessage, HICON, IMAGE_ICON, LR_DEFAULTCOLOR, MF_SEPARATOR, MF_STRING,
    MSG, SM_CXSMICON, TPM_BOTTOMALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WINDOW_EX_STYLE, WM_APP, WM_LBUTTONUP,
    WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_TOOLWINDOW, WS_POPUP,
};

/// What the icon asks the app for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCmd {
    Show,
    Quit,
}

/// Explorer's messages about the icon arrive as this.
const CALLBACK: u32 = WM_APP + 1;
const ICON_ID: u32 = 1;
const MENU_SHOW: usize = 1;
const MENU_QUIT: usize = 2;
const TIP: &str = "Kilno";

static ON_CMD: OnceLock<Box<dyn Fn(TrayCmd) + Send + Sync>> = OnceLock::new();
/// The icon's window, 0 until it exists.
static HWND_TRAY: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Adds the icon, on a thread of its own, and calls `on_cmd` (on that thread) for each click or
/// menu choice. Once per process; a second call does nothing.
pub fn start(on_cmd: impl Fn(TrayCmd) + Send + Sync + 'static) {
    if ON_CMD.set(Box::new(on_cmd)).is_err() {
        return;
    }
    let _ = std::thread::Builder::new().name("tray".into()).spawn(run);
}

/// Takes the icon away (on quit, sign-out, update). Safe from any thread, and when there is none.
pub fn remove() {
    let hwnd = HWND_TRAY.swap(0, Ordering::SeqCst);
    if hwnd != 0 {
        let nid = data(HWND(hwnd as *mut _));
        // SAFETY: a fully initialised NOTIFYICONDATAW naming our own window and icon id.
        let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
    }
}

fn run() {
    // SAFETY: the null module name is this exe.
    let Ok(module) = (unsafe { GetModuleHandleW(PCWSTR::null()) }) else { return };
    let class = w!("Kilno.Tray");
    let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module.into(), lpszClassName: class, ..Default::default() };
    // SAFETY: the class struct and its static name outlive the call.
    if unsafe { RegisterClassW(&wc) } == 0 {
        return;
    }
    // SAFETY: a static string.
    TASKBAR_CREATED.store(unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) }, Ordering::SeqCst);
    // Never shown: a tool window, so it has no taskbar button even if something showed it.
    // SAFETY: the class was registered above on this thread's module; no parent, menu or data.
    let hwnd = unsafe {
        CreateWindowExW(WS_EX_TOOLWINDOW | WINDOW_EX_STYLE(0), class, w!("Kilno"), WS_POPUP, 0, 0, 0, 0, None, None, Some(module.into()), None)
    };
    let Ok(hwnd) = hwnd else { return };
    HWND_TRAY.store(hwnd.0 as isize, Ordering::SeqCst);
    add(hwnd);
    let mut msg = MSG::default();
    // SAFETY: a plain message loop for this thread's window; `msg` is a live local.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

/// The icon's description: our window, our id, and (for add) the icon, tip and callback.
fn data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    }
}

fn add(hwnd: HWND) {
    let mut nid = data(hwnd);
    nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    nid.uCallbackMessage = CALLBACK;
    nid.hIcon = small_icon();
    for (dst, src) in nid.szTip.iter_mut().zip(TIP.encode_utf16()) {
        *dst = src;
    }
    // SAFETY: a fully initialised NOTIFYICONDATAW for our own window.
    let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

/// The exe's icon (resource 1, from build.rs) at the small-icon size.
fn small_icon() -> HICON {
    // SAFETY: the null module name is this exe; no pointers otherwise.
    unsafe {
        let Ok(module) = GetModuleHandleW(PCWSTR::null()) else { return HICON::default() };
        let size = GetSystemMetrics(SM_CXSMICON);
        // MAKEINTRESOURCE(1) as the name; the handle is kept for the life of the process.
        LoadImageW(Some(module.into()), PCWSTR(std::ptr::without_provenance(1)), IMAGE_ICON, size, size, LR_DEFAULTCOLOR)
            .map(|h| HICON(h.0))
            .unwrap_or_default()
    }
}

fn send(cmd: TrayCmd) {
    if let Some(f) = ON_CMD.get() {
        f(cmd);
    }
}

/// The right-click menu at the cursor; what was picked, if anything.
fn menu(hwnd: HWND) -> Option<TrayCmd> {
    // SAFETY: a menu we create, use and destroy here, owned by our own window on its thread.
    unsafe {
        let menu = CreatePopupMenu().ok()?;
        let _ = AppendMenuW(menu, MF_STRING, MENU_SHOW, w!("Show Kilno"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit Kilno"));
        let mut at = POINT::default();
        let _ = GetCursorPos(&mut at);
        // Without this the menu does not close when the user clicks elsewhere (KB135788).
        let _ = SetForegroundWindow(hwnd);
        let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, at.x, at.y, None, hwnd, None);
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        match picked.0 as usize {
            MENU_SHOW => Some(TrayCmd::Show),
            MENU_QUIT => Some(TrayCmd::Quit),
            _ => None,
        }
    }
}

extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == CALLBACK {
        // Legacy (version 0) callbacks: the mouse message is lParam.
        match lparam.0 as u32 {
            WM_LBUTTONUP => send(TrayCmd::Show),
            WM_RBUTTONUP => {
                if let Some(cmd) = menu(hwnd) {
                    send(cmd);
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }
    let restarted = TASKBAR_CREATED.load(Ordering::SeqCst);
    if restarted != 0 && msg == restarted && HWND_TRAY.load(Ordering::SeqCst) != 0 {
        // Explorer started again with an empty notification area.
        add(hwnd);
        return LRESULT(0);
    }
    // SAFETY: every other message goes to the default procedure unchanged.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}
