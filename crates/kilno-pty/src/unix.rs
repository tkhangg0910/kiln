//! One child process under a Unix pseudo-terminal (`openpty`, through `portable-pty`): the
//! same contract as the ConPTY one in [`super::sys`]. Output is delivered raw on a read thread;
//! the exit (with its code) follows the last output, and nothing is sent once the process was
//! dropped. Dropping kills the child's whole process group.

use super::{clamp_size, PtyConfig, PtyEvent};
use portable_pty::{native_pty_system, Child, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

pub struct PtyProcess {
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    dropped: Arc<AtomicBool>,
    pid: u32,
}

/// `cfg.command_line` as argv: words split on spaces, a single-quoted word kept whole (as
/// `kilno_core::launch::resolve_shell` writes the shell's path), `'\''` read as a quote.
pub fn argv(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    let mut in_word = false;
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '\'' => {
                quoted = !quoted;
                in_word = true;
            }
            '\\' if !quoted => in_word = true,
            c if c.is_whitespace() && !quoted => {
                if in_word {
                    out.push(std::mem::take(&mut word));
                    in_word = false;
                }
            }
            c => {
                word.push(c);
                in_word = true;
            }
        }
    }
    if in_word {
        out.push(word);
    }
    out
}

impl PtyProcess {
    pub(crate) fn spawn(cfg: &PtyConfig, mut sink: Box<dyn FnMut(PtyEvent) + Send>) -> io::Result<Self> {
        let (cols, rows) = clamp_size(cfg.cols, cfg.rows);
        let pair = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(io::Error::other)?;
        let words = argv(&cfg.command_line);
        let Some((program, args)) = words.split_first() else {
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "no command"));
        };
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        if let Some(dir) = cfg.cwd.as_ref().filter(|d| d.is_dir()) {
            cmd.cwd(dir);
        }
        // What the terminal widget understands; the agents colour by these.
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        let mut child = pair.slave.spawn_command(cmd).map_err(io::Error::other)?;
        // The child holds the slave now; ours must go, or the reader never sees EOF.
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(io::Error::other)?;
        let writer = pair.master.take_writer().map_err(io::Error::other)?;
        let dropped = Arc::new(AtomicBool::new(false));

        let gone = Arc::clone(&dropped);
        thread::Builder::new().name("pty read".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                match reader.read(&mut buf) {
                    // EIO is how Linux reports the other end closed.
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if gone.load(Ordering::Acquire) {
                            break;
                        }
                        sink(PtyEvent::Output(buf[..n].to_vec()));
                    }
                }
            }
            let code = child.wait().ok().map(|s| s.exit_code());
            if !gone.load(Ordering::Acquire) {
                sink(PtyEvent::Exited(code));
            }
        })?;

        Ok(Self {
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(killer),
            dropped,
            pid,
        })
    }

    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Write bytes to the child's input. Errors after exit are ignored.
    pub fn write(&self, data: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(data).and_then(|()| w.flush());
        }
    }

    /// Resize the terminal. Sizes under 1 are ignored.
    pub fn resize(&self, cols: u16, rows: u16) {
        if cols < 1 || rows < 1 {
            return;
        }
        if let Ok(m) = self.master.lock() {
            let _ = m.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        }
    }
}

impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.dropped.store(true, Ordering::Release);
        // The shell leads its own session (portable-pty calls setsid), so the agent it started
        // goes with it when its group is hung up; then the shell itself is killed.
        if self.pid > 0 {
            // SAFETY: a plain signal (SIGHUP) to the process group we started; no memory is touched.
            unsafe {
                libc::kill(-(self.pid as libc::pid_t), libc::SIGHUP);
            }
        }
        if let Ok(mut k) = self.killer.lock() {
            let _ = k.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn splits_like_resolve_shell_writes() {
        assert_eq!(argv("'/bin/bash' -l"), ["/bin/bash", "-l"]);
        assert_eq!(argv("'/opt/my shell/zsh' -l"), ["/opt/my shell/zsh", "-l"]);
        assert_eq!(argv(r"'it'\''s' x"), ["it's", "x"]);
        assert!(argv("   ").is_empty());
    }

    #[test]
    fn runs_reads_and_reports_the_exit() {
        let (tx, rx) = mpsc::channel();
        let cfg = PtyConfig { command_line: "/bin/sh -c 'printf hi; exit 3'".into(), cwd: None, cols: 80, rows: 24 };
        let p = PtyProcess::spawn(&cfg, Box::new(move |e| {
            let _ = tx.send(e);
        }))
        .unwrap();
        let mut out = Vec::new();
        let code = loop {
            match rx.recv_timeout(Duration::from_secs(10)).expect("an event") {
                PtyEvent::Output(b) => out.extend(b),
                PtyEvent::Exited(c) => break c,
            }
        };
        assert!(String::from_utf8_lossy(&out).contains("hi"));
        assert_eq!(code, Some(3));
        drop(p);
    }

    #[test]
    fn nothing_after_drop() {
        let (tx, rx) = mpsc::channel();
        let cfg = PtyConfig { command_line: "/bin/sh -c 'sleep 30'".into(), cwd: None, cols: 80, rows: 24 };
        let p = PtyProcess::spawn(&cfg, Box::new(move |e| {
            let _ = tx.send(e);
        }))
        .unwrap();
        p.resize(100, 30);
        p.write(b"x");
        drop(p);
        assert!(!matches!(rx.recv_timeout(Duration::from_secs(3)), Ok(PtyEvent::Exited(_))), "no exit after drop");
    }
}
