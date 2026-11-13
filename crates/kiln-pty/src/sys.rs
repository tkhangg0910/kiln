//! The Win32 side: pipes, CreatePseudoConsole, CreateProcessW with the pseudoconsole attribute,
//! and the read / exit-wait threads. Every `unsafe` block states why it is sound.

use crate::{clamp_size, PtyConfig, PtyEvent};
use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
use windows::Win32::Storage::FileSystem::{ReadFile, WriteFile};
use windows::Win32::System::Console::{
    ClosePseudoConsole, CreatePseudoConsole, ResizePseudoConsole, COORD, HPCON,
};
use windows::Win32::System::Pipes::CreatePipe;
use windows::Win32::System::Threading::{
    CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
    InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
    WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, INFINITE,
    LPPROC_THREAD_ATTRIBUTE_LIST, PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};

/// A kernel handle owned by exactly one place, closed on drop.
struct Owned(HANDLE);

// SAFETY: a kernel HANDLE is a process-wide value; the kernel objects behind these handles
// (pipe ends, process, thread) may be used and closed from any thread.
unsafe impl Send for Owned {}
// SAFETY: as above; concurrent ReadFile/WriteFile/Wait on distinct handles is allowed.
unsafe impl Sync for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            // SAFETY: the handle came from a successful Win32 call and is owned only by this
            // value, so it is closed exactly once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

/// The pseudoconsole handle; closed exactly once by whoever takes it out of the Mutex.
struct Console(HPCON);
// SAFETY: HPCON is an opaque handle the console API accepts from any thread.
unsafe impl Send for Console {}

struct Shared {
    console: Mutex<Option<Console>>,
    input: Mutex<Option<Owned>>,
    process: Owned,
    _thread: Owned,
    dropped: AtomicBool,
}

impl Shared {
    /// Close the pseudoconsole once. This makes conhost flush and close its end of the output
    /// pipe, so the read thread reaches EOF. May block until the output is drained, so never
    /// call it on the UI thread.
    fn close_console(&self) {
        let taken = self.console.lock().map(|mut c| c.take()).unwrap_or(None);
        if let Some(c) = taken {
            // SAFETY: the HPCON came from CreatePseudoConsole and was just taken out of the
            // Mutex, so no other thread can close or use it again.
            unsafe { ClosePseudoConsole(c.0) };
        }
    }
}

/// A running child under ConPTY. Dropping it kills the child and tears the console down.
pub struct PtyProcess {
    shared: Arc<Shared>,
    pid: u32,
}

fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
    s.encode_wide().chain(std::iter::once(0)).collect()
}

fn pipe() -> io::Result<(Owned, Owned)> {
    let mut read = HANDLE::default();
    let mut write = HANDLE::default();
    // SAFETY: both out-pointers point at live locals; default security, default buffer size,
    // non-inheritable handles, as the C# did.
    unsafe { CreatePipe(&mut read, &mut write, None, 0) }.map_err(io::Error::from)?;
    Ok((Owned(read), Owned(write)))
}

impl PtyProcess {
    pub(crate) fn spawn(cfg: &PtyConfig, mut sink: Box<dyn FnMut(PtyEvent) + Send>) -> io::Result<Self> {
        let (cols, rows) = clamp_size(cfg.cols, cfg.rows);
        // Naming from our side: we write keys into `input_w`, read the screen from `output_r`.
        let (input_r, input_w) = pipe()?;
        let (output_r, output_w) = pipe()?;

        let size = COORD { X: cols as i16, Y: rows as i16 };
        // SAFETY: the two handles are valid pipe ends owned by this function; flags 0 as in C#.
        let hpc = unsafe { CreatePseudoConsole(size, input_r.0, output_w.0, 0) }.map_err(io::Error::from)?;
        let console = Console(hpc);
        // ConPTY duplicated the handles it was given. Ours must go, or the read loop never
        // sees EOF when the child exits.
        drop(input_r);
        drop(output_w);

        let mut list_size = 0usize;
        // SAFETY: size probe with no list; documented to fail with ERROR_INSUFFICIENT_BUFFER
        // and write the needed size into `list_size`.
        let _ = unsafe { InitializeProcThreadAttributeList(None, 1, None, &mut list_size) };
        let mut list_buf = vec![0u8; list_size.max(1)];
        let list = LPPROC_THREAD_ATTRIBUTE_LIST(list_buf.as_mut_ptr() as *mut c_void);
        // SAFETY: `list_buf` is `list_size` bytes and outlives every use of `list` below.
        unsafe { InitializeProcThreadAttributeList(Some(list), 1, None, &mut list_size) }
            .map_err(|e| {
                // SAFETY: the console is still owned only here; close it on this error path.
                unsafe { ClosePseudoConsole(console.0) };
                io::Error::from(e)
            })?;

        let result = (|| -> io::Result<PROCESS_INFORMATION> {
            // SAFETY: for PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE the HPCON itself is lpValue (not a
            // pointer to it) and cbSize is the size of a handle; `list` was initialised above.
            unsafe {
                UpdateProcThreadAttribute(
                    list,
                    0,
                    PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                    Some(console.0 .0 as *const c_void),
                    std::mem::size_of::<HPCON>(),
                    None,
                    None,
                )
            }
            .map_err(io::Error::from)?;

            let mut si = STARTUPINFOEXW::default();
            si.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
            si.lpAttributeList = list;
            // No std handles of ours: without this a child of a process that has a console
            // (tests, a console launch) writes to that console instead of the pseudoconsole.
            si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
            // CreateProcessW may write into the command line, so it gets a private copy.
            let mut cmd = wide(std::ffi::OsStr::new(&cfg.command_line));
            let cwd = cfg.cwd.as_ref().filter(|d| d.is_dir()).map(|d| wide(d.as_os_str()));
            let mut pi = PROCESS_INFORMATION::default();
            // SAFETY: all pointers reference live locals for the duration of the call; the
            // command line buffer is mutable and NUL-terminated; environment NULL inherits ours.
            unsafe {
                CreateProcessW(
                    PCWSTR::null(),
                    Some(PWSTR(cmd.as_mut_ptr())),
                    None,
                    None,
                    false,
                    EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                    None,
                    cwd.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
                    &si.StartupInfo,
                    &mut pi,
                )
            }
            .map_err(io::Error::from)?;
            Ok(pi)
        })();
        // SAFETY: `list` was initialised by InitializeProcThreadAttributeList and is no longer
        // needed once CreateProcessW returned.
        unsafe { DeleteProcThreadAttributeList(list) };
        drop(list_buf);

        let pi = match result {
            Ok(pi) => pi,
            Err(e) => {
                // SAFETY: the console was never handed to anyone else.
                unsafe { ClosePseudoConsole(console.0) };
                return Err(e);
            }
        };

        let shared = Arc::new(Shared {
            console: Mutex::new(Some(console)),
            input: Mutex::new(Some(input_w)),
            process: Owned(pi.hProcess),
            _thread: Owned(pi.hThread),
            dropped: AtomicBool::new(false),
        });

        // The child exiting does not end the output pipe on its own (conhost stays up until the
        // console is closed), so a waiter closes the console when the child is gone.
        let waiter = Arc::clone(&shared);
        thread::Builder::new().name("ConPTY wait".into()).spawn(move || {
            // SAFETY: the process handle stays open as long as `waiter` holds the Arc.
            let r = unsafe { WaitForSingleObject(waiter.process.0, INFINITE) };
            if r == WAIT_OBJECT_0 {
                waiter.close_console();
            }
        })?;

        let reader = Arc::clone(&shared);
        thread::Builder::new().name("ConPTY read".into()).spawn(move || {
            let output = output_r;
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let mut n = 0u32;
                // SAFETY: `output` is our owned read end; `buf` is a live buffer of its length.
                let ok = unsafe { ReadFile(output.0, Some(&mut buf), Some(&mut n), None) };
                if ok.is_err() || n == 0 {
                    break;
                }
                // Keep draining after drop: ClosePseudoConsole waits for its last output to
                // be read, so stopping early would leave it waiting.
                if !reader.dropped.load(Ordering::Acquire) {
                    sink(PtyEvent::Output(buf[..n as usize].to_vec()));
                }
            }
            if !reader.dropped.load(Ordering::Acquire) {
                let mut code = 0u32;
                // SAFETY: the process handle is open while `reader` holds the Arc.
                let code = unsafe { GetExitCodeProcess(reader.process.0, &mut code) }
                    .ok()
                    .map(|_| code);
                sink(PtyEvent::Exited(code));
            }
        })?;

        Ok(Self { shared, pid: pi.dwProcessId })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Write bytes to the child's input, unbuffered. Errors after exit are ignored.
    pub fn write(&self, data: &[u8]) {
        let Ok(guard) = self.shared.input.lock() else { return };
        let Some(h) = guard.as_ref() else { return };
        let mut rest = data;
        while !rest.is_empty() {
            let mut n = 0u32;
            // SAFETY: `h` is our owned write end, held open by the lock guard during the call.
            let ok = unsafe { WriteFile(h.0, Some(rest), Some(&mut n), None) };
            if ok.is_err() || n == 0 {
                return;
            }
            rest = &rest[n as usize..];
        }
    }

    /// Resize the console. Sizes under 1 are ignored. The same size again is allowed on
    /// purpose: it makes ConPTY repaint the whole screen.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols < 1 || rows < 1 {
            return;
        }
        let Ok(guard) = self.shared.console.lock() else { return };
        if let Some(c) = guard.as_ref() {
            let size = COORD { X: cols as i16, Y: rows as i16 };
            // SAFETY: the HPCON is open while it sits in the Mutex we hold.
            let _ = unsafe { ResizePseudoConsole(c.0, size) };
        }
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.shared.dropped.store(true, Ordering::Release);
        // Kill here, so the shell is gone even when Kiln exits before the teardown runs.
        // SAFETY: the process handle is open while `self.shared` is alive.
        unsafe {
            let _ = TerminateProcess(self.shared.process.0, 0);
        }
        let shared = Arc::clone(&self.shared);
        // Off the caller's thread: ClosePseudoConsole can block until conhost has flushed.
        let _ = thread::Builder::new().name("ConPTY close".into()).spawn(move || {
            shared.close_console();
            if let Ok(mut i) = shared.input.lock() {
                i.take();
            }
        });
    }
}
