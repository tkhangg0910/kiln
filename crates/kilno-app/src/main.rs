//! `kilno.exe`: every Claude Code and Codex session in one window.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod agent_logo;
mod app;
mod fonts;
mod image_chip;
mod island_host;
mod latency;
mod doc_view;
mod model;
mod pane;
mod pages_open;
mod settings_page;
mod setup_window;
mod status;
mod status_view;
mod theme;
mod updater;
mod win;
#[cfg(unix)]
mod unix;
// The platform layer under one name: Win32 on Windows, xdg-open, ureq and a Unix socket elsewhere.
#[cfg(windows)]
use win as os;
#[cfg(unix)]
use unix as os;

/// Startup timing marks, written to the file named by `KILNO_STARTUP_LOG` (for BENCH.md).
pub fn mark(what: &str) {
    use std::sync::OnceLock;
    static T0: OnceLock<std::time::Instant> = OnceLock::new();
    let t0 = *T0.get_or_init(std::time::Instant::now);
    if let Some(path) = std::env::var_os("KILNO_STARTUP_LOG") {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{:>7.1} ms  {what}", t0.elapsed().as_secs_f64() * 1000.0);
        }
    }
}

/// Appends to `%LOCALAPPDATA%\Kilno\crash.log` (App.LogFatal). Failures are ignored: there is
/// nowhere left to report them.
pub fn log_fatal(what: &str) {
    let _ = kilno_core::crash::append(data_dir(&OsEnv).path(), chrono::Local::now().naive_local(), what);
}

/// A panic anywhere leaves a record of why the window went away, then proceeds as it would
/// have (release builds abort; the hook runs first).
fn install_crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let name = thread.name().unwrap_or("<unnamed>");
        let trace = std::backtrace::Backtrace::force_capture();
        log_fatal(&format!("thread '{name}' {info}\r\n{trace}"));
        default(info);
    }));
}

use kilno_core::paths::{data_dir, OsEnv};
use kilno_core::status::relay;

fn main() {
    // The status line relay runs on every status refresh of every Claude pane: it must be the
    // very first thing, before any window, font or mutex (PORTING.md §4.1).
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_ref().is_some_and(|a| a == relay::ARG) {
        let pass = args.next().is_some_and(|a| a == relay::PASS_ARG);
        let dir = relay::status_dir(&data_dir(&OsEnv));
        let code = relay::run(pass, std::io::stdin().lock(), std::io::stdout().lock(), &dir);
        std::process::exit(code);
    }
    // Claude's hooks (a turn started or ended, a question): just as early, and as quiet.
    if first.as_ref().is_some_and(|a| a == kilno_core::status::hooks::ARG) {
        let event = args.next().map(|a| a.to_string_lossy().into_owned()).unwrap_or_default();
        let dir = kilno_core::status::hooks::turns_dir(&data_dir(&OsEnv));
        std::process::exit(kilno_core::status::hooks::run(&event, std::io::stdin().lock(), &dir));
    }

    mark("main");
    install_crash_log();
    // Children must start as top-level sessions, not as children of a Claude Code session
    // Kilno itself may have been launched from.
    kilno_core::launch::scrub_inherited_session();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    let exe_dir = std::env::current_exe().ok().and_then(|e| e.parent().map(std::path::Path::to_path_buf)).unwrap_or_default();
    let windir = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("WINDIR")).unwrap_or_else(|| r"C:\Windows".into());
    let system_dir = std::path::Path::new(&windir).join("System32");
    let start = kilno_core::cli::parse(&args, &cwd, &exe_dir, &system_dir, |p| std::path::Path::new(p).is_dir());

    // One Kilno per user session: a second launch hands its folder over and leaves.
    #[cfg(windows)]
    if !win::instance::acquire_mutex() {
        if let (Some(text), Some(name)) = (start.handoff(), win::instance::pipe_name()) {
            win::instance::allow_set_foreground_any();
            let _ = win::instance::send(&name, &text, win::instance::CONNECT_TIMEOUT);
        }
        std::process::exit(0);
    }
    #[cfg(unix)]
    if !unix::instance::acquire() {
        if let Some(text) = start.handoff() {
            let _ = unix::instance::send(&text);
        }
        std::process::exit(0);
    }

    let code = app::run(app::Startup { background: start.background, folder: start.folder });
    #[cfg(windows)]
    win::tray::remove();
    // Hard exit: no stray thread (a PTY reader, a watcher) may keep the process alive, and with
    // it the single-instance mutex.
    std::process::exit(code);
}
