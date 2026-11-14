//! Runs real children under ConPTY.
#![cfg(windows)]

use kilno_pty::{spawn, PtyConfig, PtyEvent};
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn collect(rx: &mpsc::Receiver<PtyEvent>, until: impl Fn(&str, bool) -> bool, secs: u64) -> (String, bool) {
    let mut out = Vec::new();
    let mut exited = false;
    let end = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < end {
        match rx.recv_timeout(Duration::from_millis(100)) {
            Ok(PtyEvent::Output(b)) => out.extend_from_slice(&b),
            Ok(PtyEvent::Exited(_)) => exited = true,
            Err(_) => {}
        }
        if until(&String::from_utf8_lossy(&out), exited) {
            break;
        }
    }
    (String::from_utf8_lossy(&out).into_owned(), exited)
}

#[test]
fn echoes_and_exits() {
    let (tx, rx) = mpsc::channel();
    let cfg = PtyConfig {
        command_line: "cmd.exe /c echo kilno-pty-ok".into(),
        cwd: None,
        cols: 80,
        rows: 24,
    };
    let _p = spawn(&cfg, move |e| {
        let _ = tx.send(e);
    })
    .expect("spawn");
    let (out, exited) = collect(&rx, |_, x| x, 15);
    assert!(out.contains("kilno-pty-ok"), "output: {out:?}");
    assert!(exited, "exit event");
}

#[test]
fn input_reaches_the_child_and_resize_works() {
    let (tx, rx) = mpsc::channel();
    let cfg = PtyConfig {
        command_line: "cmd.exe /q /k".into(),
        cwd: Some(std::env::temp_dir()),
        cols: 100,
        rows: 30,
    };
    let p = spawn(&cfg, move |e| {
        let _ = tx.send(e);
    })
    .expect("spawn");
    p.resize(120, 40);
    p.resize(0, 10); // ignored
    p.write("echo typed-\u{1ebf}\r".as_bytes()); // a Vietnamese letter survives the round trip
    let (out, _) = collect(&rx, |s, _| s.contains("typed-\u{1ebf}"), 15);
    assert!(out.contains("typed-\u{1ebf}"), "output: {out:?}");
    p.write(b"exit\r");
    let (_, exited) = collect(&rx, |_, x| x, 15);
    assert!(exited);
}

#[test]
fn drop_kills_without_blocking_and_sends_no_exit() {
    let (tx, rx) = mpsc::channel();
    let cfg = PtyConfig {
        command_line: "cmd.exe /q /k".into(),
        cwd: None,
        cols: 80,
        rows: 24,
    };
    let p = spawn(&cfg, move |e| {
        let _ = tx.send(e);
    })
    .expect("spawn");
    let pid = p.pid();
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    drop(p);
    assert!(t.elapsed() < Duration::from_millis(200), "drop must not block");
    let (_, exited) = collect(&rx, |_, x| x, 3);
    assert!(!exited, "no Exited after drop");
    let alive = std::process::Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
        .unwrap_or(false);
    assert!(!alive, "child killed");
}

#[test]
fn bad_command_is_an_error() {
    let cfg = PtyConfig {
        command_line: "no-such-program-kilno.exe".into(),
        cwd: None,
        cols: 80,
        rows: 24,
    };
    assert!(spawn(&cfg, |_| {}).is_err());
}
