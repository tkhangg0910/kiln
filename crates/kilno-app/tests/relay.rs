//! The status line relay end to end: the real `kilno.exe`, real pipes (PORTING.md §4.1,
//! STA-001..008, MAIN-001/002).

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The data folder's name: `%LOCALAPPDATA%\Kilno`, `$XDG_DATA_HOME/kilno`.
const APP: &str = if cfg!(windows) { "Kilno" } else { "kilno" };

const ID: &str = "0F8E4C2A-1B3D-4E5F-8A9B-0C1D2E3F4A5B";

fn payload() -> Vec<u8> {
    format!(
        "{{\"session_id\":\"{ID}\",\"model\":{{\"display_name\":\"Opus 5.5 (1M context)\"}},\"cwd\":\"C:\\\\x\",\"note\":\"h\u{e9}llo \u{2192}\"}}\r\n"
    )
    .into_bytes()
}

/// Runs the relay with `args`, the payload on stdin, the data folder (LOCALAPPDATA, or XDG_DATA_HOME off Windows) at `local`.
fn relay(args: &[&str], local: &Path) -> (std::process::Output, Duration) {
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kilno"))
        .args(args)
        .env("LOCALAPPDATA", local)
        .env("XDG_DATA_HOME", local)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start kilno");
    child.stdin.take().expect("stdin").write_all(&payload()).expect("write stdin");
    let out = child.wait_with_output().expect("wait");
    (out, started.elapsed())
}

fn saved(local: &Path) -> std::path::PathBuf {
    local.join(APP).join("status").join(format!("{}.json", ID.to_lowercase()))
}

#[test]
fn pass_echoes_stdin_byte_for_byte_and_saves() {
    let local = tempfile::tempdir().unwrap();
    // The first start of a fresh exe pays for the loader; time the second.
    let _ = relay(&["--statusline", "--pass"], local.path());
    std::fs::remove_dir_all(local.path().join(APP)).unwrap();
    let (out, took) = relay(&["--statusline", "--pass"], local.path());
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, payload(), "stdout is stdin, unchanged");
    assert!(out.stderr.is_empty());
    assert_eq!(std::fs::read(saved(local.path())).unwrap(), payload(), "the raw bytes are saved");
    // No leftover temp file.
    let names: Vec<_> = std::fs::read_dir(local.path().join(APP).join("status")).unwrap().collect();
    assert_eq!(names.len(), 1);
    // Typically a few ms; generous for a loaded test machine and a debug build.
    assert!(took < Duration::from_millis(if cfg!(debug_assertions) { 1000 } else { 300 }), "relay took {took:?}");
    eprintln!("relay --pass took {took:?}");
}

#[test]
fn without_pass_prints_nothing_and_saves() {
    let local = tempfile::tempdir().unwrap();
    let (out, _) = relay(&["--statusline"], local.path());
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty(), "nothing printed without --pass");
    assert_eq!(std::fs::read(saved(local.path())).unwrap(), payload());
}

#[test]
fn pass_only_counts_as_the_second_argument() {
    let local = tempfile::tempdir().unwrap();
    let (out, _) = relay(&["--statusline", "--other", "--pass"], local.path());
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[test]
fn not_json_saves_nothing_and_still_exits_zero() {
    let local = tempfile::tempdir().unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_kilno"))
        .args(["--statusline", "--pass"])
        .env("LOCALAPPDATA", local.path())
        .env("XDG_DATA_HOME", local.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"not json").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(out.stdout, b"not json");
    assert!(!local.path().join(APP).join("status").exists() || std::fs::read_dir(local.path().join(APP).join("status")).unwrap().next().is_none());
}
