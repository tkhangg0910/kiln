//! Opt-in key-echo latency probe for BENCH.md. Off unless `KILN_BENCH_TYPE` names a log file.
//!
//! When on, the first pane runs a bare shell (no agent) and the app types one character into
//! the active pane every 200 ms through the same path a real key takes (`PaneShared::type_keys`).
//! For each character it logs, in milliseconds:
//!   echo  = key written to the PTY → its echo read back (shell + ConPTY, not Kiln's own time)
//!   frame = echo read → end of the UI pass that draws it (Kiln: parse + layout + paint)
//! The GL present follows the pass immediately (vsync off).

use crate::pane::PaneShared;
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

const SAMPLES: usize = 200;
const EVERY: Duration = Duration::from_millis(200);

struct Probe {
    log: PathBuf,
    next: Instant,
    sent: Option<Instant>,
    echoed: Option<Instant>,
    done: usize,
}

fn probe() -> Option<&'static Mutex<Probe>> {
    static P: OnceLock<Option<Mutex<Probe>>> = OnceLock::new();
    P.get_or_init(|| {
        std::env::var_os("KILN_BENCH_TYPE").map(|log| {
            Mutex::new(Probe { log: log.into(), next: Instant::now() + Duration::from_secs(3), sent: None, echoed: None, done: 0 })
        })
    })
    .as_ref()
}

/// The probe is on: panes start a bare shell.
pub fn enabled() -> bool {
    probe().is_some()
}

/// Output arrived on a pane's read thread.
pub fn on_output() {
    if let Some(p) = probe() {
        if let Ok(mut p) = p.lock() {
            if p.sent.is_some() && p.echoed.is_none() {
                p.echoed = Some(Instant::now());
            }
        }
    }
}

/// End of a UI pass. Logs a finished sample, types the next character when due, and returns
/// when the next pass is needed.
pub fn on_frame(active: Option<&PaneShared>) -> Option<Duration> {
    let p = probe()?;
    let mut p = p.lock().ok()?;
    let now = Instant::now();
    if let (Some(sent), Some(echoed)) = (p.sent, p.echoed) {
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&p.log) {
            let _ = writeln!(f, "echo {:.3} frame {:.3}", ms(echoed - sent), ms(now - echoed));
        }
        p.sent = None;
        p.echoed = None;
        p.done += 1;
    }
    if p.done >= SAMPLES {
        return None;
    }
    if p.sent.is_none() && now >= p.next {
        if let Some(pane) = active {
            p.sent = Some(Instant::now());
            p.next = now + EVERY;
            drop(p);
            pane.type_keys(b"x");
            return Some(EVERY);
        }
    }
    Some(p.next.saturating_duration_since(now).max(Duration::from_millis(1)))
}
