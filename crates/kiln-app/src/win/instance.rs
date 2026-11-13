//! Single instance (System/SingleInstance.cs): typing `kiln` in a second folder adds a session
//! to the window you already have. A named mutex says who is first; a per-user named pipe hands
//! the second launch's folder over. Names are the C# build's, so the two never run side by side.
//!
//! Protocol: the client connects (2 s), writes the folder as UTF-8 and closes; no reply. An
//! empty message means "just show the window". A UTF-8 BOM (what .NET's StreamWriter wrote) is
//! stripped on read.

use std::io;
use std::time::{Duration, Instant};

use windows::core::{HSTRING, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, ERROR_BROKEN_PIPE, ERROR_FILE_NOT_FOUND,
    ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, GetLastError, HANDLE, HLOCAL,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SE_KERNEL_OBJECT,
};
use windows::Win32::Security::{
    EqualSid, GetTokenInformation, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, ReadFile, WriteFile, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_GENERIC_WRITE, FILE_SHARE_NONE, OPEN_EXISTING,
    PIPE_ACCESS_INBOUND, SECURITY_IDENTIFICATION, SECURITY_SQOS_PRESENT,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, WaitNamedPipeW, PIPE_READMODE_BYTE,
    PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

/// A dev build runs beside the installed Kiln instead of handing its folder over.
pub const MUTEX_NAME: &str =
    if cfg!(debug_assertions) { r"Local\Kiln.SingleInstance.Dev" } else { r"Local\Kiln.SingleInstance" };
const PIPE_BASE: &str = if cfg!(debug_assertions) { "Kiln.Handoff.Dev" } else { "Kiln.Handoff" };

/// A handoff larger than this is not a folder path; the rest is dropped.
const MAX_MESSAGE: usize = 64 * 1024;
/// How long the second launch tries to reach the first.
pub const CONNECT_TIMEOUT: Duration = Duration::from_millis(2000);

/// Owns a kernel handle and closes it.
struct Owned(HANDLE);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: the handle was opened by us and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Frees what the security APIs allocate with LocalAlloc.
struct Local(*mut core::ffi::c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // SAFETY: the pointer came from an API documented to allocate with LocalAlloc.
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0))) };
    }
}

/// The current user's token user (SID) bytes, kept alive in a buffer.
struct UserSid(Vec<u64>);
impl UserSid {
    fn current() -> Option<Self> {
        let mut token = HANDLE::default();
        // SAFETY: the pseudo handle needs no closing; `token` receives a handle we close below.
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.ok()?;
        let token = Owned(token);
        let mut len = 0u32;
        // SAFETY: a size query with no buffer; fails with ERROR_INSUFFICIENT_BUFFER and sets len.
        let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut len) };
        if len == 0 {
            return None;
        }
        // u64 storage keeps the TOKEN_USER aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: the buffer holds `len` bytes, as asked for.
        unsafe { GetTokenInformation(token.0, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len) }.ok()?;
        Some(Self(buf))
    }

    fn psid(&self) -> PSID {
        // SAFETY: the buffer starts with a TOKEN_USER written by GetTokenInformation; its SID
        // pointer points into the same buffer, which lives as long as `self`.
        unsafe { (*(self.0.as_ptr() as *const TOKEN_USER)).User.Sid }
    }

    fn string(&self) -> Option<String> {
        let mut s = PWSTR::null();
        // SAFETY: a valid SID; the string is allocated by the call and freed by `Local`.
        unsafe { ConvertSidToStringSidW(self.psid(), &mut s) }.ok()?;
        let _free = Local(s.0.cast());
        // SAFETY: a NUL-terminated wide string from the call above.
        unsafe { s.to_string() }.ok()
    }
}

/// `S-1-5-21-…` of the user Kiln runs as.
pub fn user_sid() -> Option<String> {
    UserSid::current()?.string()
}

/// `\\.\pipe\Kiln.Handoff.<SID>` (Dev: `Kiln.Handoff.Dev.<SID>`). Pipe names are
/// machine-wide, so the SID keeps another account from taking the name or receiving folders.
pub fn pipe_name() -> Option<String> {
    Some(format!(r"\\.\pipe\{PIPE_BASE}.{}", user_sid()?))
}

/// Takes the single-instance mutex. True: this process is the first and keeps it (the handle
/// lives until the process exits). False: another Kiln has it.
pub fn acquire_mutex() -> bool {
    // SAFETY: plain object creation; the name is a valid wide string for the call.
    let made = unsafe { CreateMutexW(None, true, &HSTRING::from(MUTEX_NAME)) };
    // SAFETY: read right after the call that set it.
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    match made {
        // Never closed: the mutex is ours for the life of the process (closed by its exit).
        Ok(_) => !existed,
        // No mutex at all (should not happen): run rather than refuse to start.
        Err(_) => true,
    }
}

/// This launch came from your click or Enter, so it may bring a window forward; pass that right
/// on, or the running Kiln's window would only flash in the taskbar.
pub fn allow_set_foreground_any() {
    // SAFETY: no pointers; ASFW_ANY is (DWORD)-1.
    let _ = unsafe { AllowSetForegroundWindow(u32::MAX) };
}

/// Only the current user may open the pipe; owner the current user (so the client can check it).
fn current_user_only(sid: &str) -> Option<Local> {
    let sddl = HSTRING::from(format!("O:{sid}D:P(A;;GA;;;{sid})"));
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: valid SDDL string; the descriptor is LocalAlloc'd and freed by `Local`.
    unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(&sddl, 1, &mut sd, None) }.ok()?;
    Some(Local(sd.0))
}

/// Serves the handoff pipe on its own thread: each message (BOM stripped, trimmed) goes to
/// `received`. A client vanishing keeps it serving; the name held by someone else ends it.
pub fn serve(name: String, received: impl Fn(String) + Send + 'static) -> io::Result<()> {
    let sid = UserSid::current().ok_or_else(io::Error::last_os_error)?;
    let sid_str = sid.string().ok_or_else(io::Error::last_os_error)?;
    std::thread::Builder::new().name("kiln-handoff".into()).spawn(move || {
        let Some(sd) = current_user_only(&sid_str) else { return };
        let wide = HSTRING::from(name.as_str());
        loop {
            let sa = SECURITY_ATTRIBUTES {
                nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: sd.0,
                bInheritHandle: false.into(),
            };
            // SAFETY: valid name and security attributes for the call; the handle is owned below.
            let h = unsafe {
                CreateNamedPipeW(
                    &wide,
                    PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE,
                    PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                    1,
                    0,
                    MAX_MESSAGE as u32,
                    0,
                    Some(&sa),
                )
            };
            if h.is_invalid() {
                // SAFETY: read right after the failing call.
                let err = unsafe { GetLastError() };
                if err == ERROR_ACCESS_DENIED {
                    return; // someone else holds the name: serving it is impossible
                }
                std::thread::sleep(Duration::from_millis(200));
                continue;
            }
            let pipe = Owned(h);
            // SAFETY: a valid pipe handle; blocking connect.
            let connected = match unsafe { ConnectNamedPipe(pipe.0, None) } {
                Ok(()) => true,
                // A client that connected and already closed (an empty handoff, a short
                // write) before we got here: what it wrote is still there to read.
                Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult() || e.code() == ERROR_NO_DATA.to_hresult(),
            };
            if !connected {
                continue;
            }
            let bytes = read_to_end(&pipe);
            // SAFETY: a connected pipe handle we own.
            let _ = unsafe { DisconnectNamedPipe(pipe.0) };
            drop(pipe);
            if let Some(bytes) = bytes {
                received(kiln_core::cli::decode_handoff(&bytes));
            }
        }
    })?;
    Ok(())
}

/// Reads until the client closes its end. None when the read failed for another reason.
fn read_to_end(pipe: &Owned) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let mut n = 0u32;
        // SAFETY: the buffer is valid for its length; synchronous read on our pipe.
        match unsafe { ReadFile(pipe.0, Some(&mut buf), Some(&mut n), None) } {
            Ok(()) if n == 0 => return Some(out),
            Ok(()) => {
                if out.len() < MAX_MESSAGE {
                    out.extend_from_slice(&buf[..n as usize]);
                }
            }
            Err(e) if e.code() == ERROR_BROKEN_PIPE.to_hresult() => return Some(out),
            Err(_) => return None,
        }
    }
}

/// Hands `text` to the running instance: connect (retrying until `timeout`, the server may be
/// between two clients or just starting), check the pipe is the current user's, write, close.
pub fn send(name: &str, text: &str, timeout: Duration) -> io::Result<()> {
    let wide = HSTRING::from(name);
    let deadline = Instant::now() + timeout;
    let pipe = loop {
        // SAFETY: valid name; identification-level QoS so a fake server can't impersonate us.
        let h = unsafe {
            CreateFileW(
                &wide,
                FILE_GENERIC_WRITE.0,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match h {
            Ok(h) => break Owned(h),
            Err(e) => {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    return Err(io::Error::new(io::ErrorKind::TimedOut, e));
                }
                if e.code() == ERROR_PIPE_BUSY.to_hresult() {
                    // SAFETY: valid name; waits at most `left`.
                    let _ = unsafe { WaitNamedPipeW(&wide, left.as_millis() as u32) };
                } else if e.code() == ERROR_FILE_NOT_FOUND.to_hresult() {
                    std::thread::sleep(Duration::from_millis(50).min(left));
                } else {
                    return Err(io::Error::from_raw_os_error(e.code().0 & 0xFFFF));
                }
            }
        }
    };
    if !owned_by_current_user(&pipe) {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    let mut rest = text.as_bytes();
    while !rest.is_empty() {
        let mut n = 0u32;
        // SAFETY: the slice is valid for its length; synchronous write.
        unsafe { WriteFile(pipe.0, Some(rest), Some(&mut n), None) }.map_err(io::Error::other)?;
        rest = &rest[n as usize..];
    }
    Ok(())
}

/// .NET's CurrentUserOnly check on the client: the pipe's owner is us.
fn owned_by_current_user(pipe: &Owned) -> bool {
    let Some(me) = UserSid::current() else { return false };
    let mut owner = PSID::default();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: valid handle; `owner` points into `sd`, which is freed by `Local` after the compare.
    let err = unsafe {
        GetSecurityInfo(pipe.0, SE_KERNEL_OBJECT, OWNER_SECURITY_INFORMATION, Some(&mut owner), None, None, None, Some(&mut sd))
    };
    if err.is_err() {
        return false;
    }
    let _free = Local(sd.0);
    // SAFETY: both SIDs are valid for the call.
    unsafe { EqualSid(owner, me.psid()) }.is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn test_pipe(tag: &str) -> String {
        format!(r"\\.\pipe\Kiln.Handoff.Test.{}.{tag}.{}", std::process::id(), user_sid().unwrap())
    }

    #[test]
    fn names_carry_the_user_sid() {
        let sid = user_sid().expect("a SID");
        assert!(sid.starts_with("S-1-"), "{sid}");
        let name = pipe_name().unwrap();
        assert!(name.starts_with(r"\\.\pipe\Kiln.Handoff."), "{name}");
        assert!(name.ends_with(&sid));
        if cfg!(debug_assertions) {
            assert_eq!(MUTEX_NAME, r"Local\Kiln.SingleInstance.Dev");
            assert!(name.contains(".Dev."));
        }
    }

    #[test]
    fn folders_and_empty_messages_reach_the_server() {
        let name = test_pipe("msgs");
        let (tx, rx) = channel();
        serve(name.clone(), move |m| {
            let _ = tx.send(m);
        })
        .unwrap();
        let wait = Duration::from_secs(5);
        send(&name, r"C:\Users\me\project ", CONNECT_TIMEOUT).unwrap();
        assert_eq!(rx.recv_timeout(wait).unwrap(), r"C:\Users\me\project");
        send(&name, "", CONNECT_TIMEOUT).unwrap();
        assert_eq!(rx.recv_timeout(wait).unwrap(), "");
        // What the C# client wrote: a BOM first.
        send(&name, "\u{FEFF}D:\\Đồ án", CONNECT_TIMEOUT).unwrap();
        assert_eq!(rx.recv_timeout(wait).unwrap(), "D:\\Đồ án");
    }

    #[test]
    fn nobody_listening_times_out() {
        let name = test_pipe("none");
        let t = Instant::now();
        let r = send(&name, "x", Duration::from_millis(300));
        assert!(r.is_err());
        assert!(t.elapsed() >= Duration::from_millis(250));
    }

    #[test]
    fn a_second_server_cannot_take_the_name() {
        let name = test_pipe("dup");
        let (tx, rx) = channel();
        serve(name.clone(), move |m| {
            let _ = tx.send(("first", m));
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let (tx2, rx2) = channel();
        serve(name.clone(), move |m| {
            let _ = tx2.send(("second", m));
        })
        .unwrap();
        send(&name, "a", CONNECT_TIMEOUT).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), ("first", "a".to_string()));
        assert!(rx2.recv_timeout(Duration::from_millis(300)).is_err());
    }
}
