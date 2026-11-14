//! Measures a Kilno build for BENCH.md: launch → first visible window, private working set of
//! the kilno process (and, separately, of helper processes such as WebView2), and CPU at idle.
//!
//! kilno-bench <exe> [--cwd DIR] [--runs N] [--settle SECS] [--idle SECS] [--helpers NAME]
//! The child inherits this process's environment, so point CLAUDE_CONFIG_DIR, CODEX_HOME and
//! LOCALAPPDATA at temp folders before running it.

use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use std::time::{Duration, Instant};
use windows::core::{BOOL, PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX2};
use windows::Win32::System::Threading::{
    CreateProcessW, GetProcessTimes, OpenProcess, CREATE_NEW_PROCESS_GROUP, PROCESS_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, STARTUPINFOW,
};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowThreadProcessId, IsWindowVisible};

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

struct Find {
    pid: u32,
    found: bool,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lp: LPARAM) -> BOOL {
    // SAFETY: lp is the &mut Find passed to EnumWindows below, alive for the whole enumeration.
    let f = unsafe { &mut *(lp.0 as *mut Find) };
    let mut pid = 0u32;
    // SAFETY: hwnd comes from EnumWindows; pid is a live local.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    // SAFETY: hwnd is a valid top-level window handle during the callback.
    if pid == f.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        f.found = true;
        return BOOL(0);
    }
    BOOL(1)
}

fn has_visible_window(pid: u32) -> bool {
    let mut f = Find { pid, found: false };
    // SAFETY: the callback only reads/writes `f`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(enum_cb), LPARAM(&mut f as *mut Find as isize)) };
    f.found
}

fn private_ws(pid: u32) -> Option<usize> {
    // SAFETY: plain query access; the handle is closed below.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut c = PROCESS_MEMORY_COUNTERS_EX2 { cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32, ..Default::default() };
    // SAFETY: `c` is sized by cb; the EX2 struct starts with PROCESS_MEMORY_COUNTERS.
    let ok = unsafe { GetProcessMemoryInfo(h, &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) }.is_ok();
    // SAFETY: h was opened above.
    unsafe { let _ = CloseHandle(h); }
    ok.then_some(c.PrivateWorkingSetSize)
}

fn cpu_time(pid: u32) -> Option<Duration> {
    // SAFETY: plain query access; closed below.
    let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let (mut a, mut b, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    // SAFETY: four live out-params.
    let ok = unsafe { GetProcessTimes(h, &mut a, &mut b, &mut k, &mut u) }.is_ok();
    // SAFETY: h was opened above.
    unsafe { let _ = CloseHandle(h); }
    let t = |f: FILETIME| ((f.dwHighDateTime as u64) << 32 | f.dwLowDateTime as u64) * 100;
    ok.then(|| Duration::from_nanos(t(k) + t(u)))
}

/// (pid, parent, exe name) of every process.
fn processes() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    // SAFETY: snapshot handle is closed below; entries are read into a live, sized struct.
    unsafe {
        let Ok(snap) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(snap, &mut e).is_ok();
        while ok {
            let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(0);
            out.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..len])));
            ok = Process32NextW(snap, &mut e).is_ok();
        }
        let _ = CloseHandle(snap);
    }
    out
}

fn descendants(root: u32) -> Vec<(u32, String)> {
    let all = processes();
    let mut found = vec![root];
    let mut out = Vec::new();
    let mut i = 0;
    while i < found.len() {
        let p = found[i];
        for (pid, parent, name) in &all {
            if *parent == p && !found.contains(pid) {
                found.push(*pid);
                out.push((*pid, name.clone()));
            }
        }
        i += 1;
    }
    out
}

fn spawn(exe: &str, cwd: Option<&str>) -> (u32, HANDLE) {
    let mut cmd = wide(&format!("\"{exe}\""));
    let cwd_w = cwd.map(wide);
    let si = STARTUPINFOW { cb: std::mem::size_of::<STARTUPINFOW>() as u32, ..Default::default() };
    let mut pi = PROCESS_INFORMATION::default();
    // SAFETY: live buffers for the call; handles in `pi` are returned to the caller.
    unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_NEW_PROCESS_GROUP,
            None,
            cwd_w.as_ref().map_or(PCWSTR::null(), |w| PCWSTR(w.as_ptr())),
            &si,
            &mut pi,
        )
    }
    .expect("CreateProcessW");
    // SAFETY: the thread handle is not needed.
    unsafe { let _ = CloseHandle(pi.hThread); }
    (pi.dwProcessId, pi.hProcess)
}

fn kill_tree(pid: u32) {
    let _ = std::process::Command::new("taskkill").args(["/T", "/F", "/PID", &pid.to_string()]).output();
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let exe = args.first().expect("usage: kilno-bench <exe> [--cwd DIR] [--runs N] [--settle S] [--idle S] [--helpers NAME]").clone();
    let opt = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned();
    let runs: usize = opt("--runs").and_then(|v| v.parse().ok()).unwrap_or(5);
    let settle: u64 = opt("--settle").and_then(|v| v.parse().ok()).unwrap_or(10);
    let idle: u64 = opt("--idle").and_then(|v| v.parse().ok()).unwrap_or(10);
    let helpers = opt("--helpers");
    let cwd = opt("--cwd");
    let _ = OsString::new();

    let mut starts = Vec::new();
    for run in 0..runs {
        let t = Instant::now();
        let (pid, h) = spawn(&exe, cwd.as_deref());
        while !has_visible_window(pid) && t.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(1));
        }
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        starts.push(ms);
        println!("run {run}: window visible after {ms:.0} ms");
        let last = run + 1 == runs;
        if last {
            std::thread::sleep(Duration::from_secs(settle));
            let mb = |b: usize| b as f64 / 1048576.0;
            println!("private working set after {settle} s: {:.1} MB", mb(private_ws(pid).unwrap_or(0)));
            if let Some(name) = &helpers {
                let kids: Vec<_> = descendants(pid).into_iter().filter(|(_, n)| n.eq_ignore_ascii_case(name)).collect();
                let sum: usize = kids.iter().filter_map(|(p, _)| private_ws(*p)).sum();
                println!("{} x {name}: private working set {:.1} MB", kids.len(), mb(sum));
            }
            let c0 = cpu_time(pid).unwrap_or_default();
            std::thread::sleep(Duration::from_secs(idle));
            let c1 = cpu_time(pid).unwrap_or_default();
            println!("CPU over {idle} s idle: {:.0} ms ({:.3} % of one core)", (c1 - c0).as_secs_f64() * 1000.0, (c1 - c0).as_secs_f64() / idle as f64 * 100.0);
        }
        kill_tree(pid);
        // SAFETY: handle from CreateProcessW, closed once.
        unsafe { let _ = CloseHandle(h); }
        std::thread::sleep(Duration::from_millis(800));
    }
    let mut s = starts.clone();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    println!("startup: first {:.0} ms, median {:.0} ms, min {:.0} ms", starts[0], s[s.len() / 2], s[0]);
}
