//! RAM and CPU for the sidebar footer (Settings › Status bar): Kilno alone, or Kilno and each
//! session's process tree (the shell, the agent, what the agent runs). A thread samples every
//! 2 s while the window shows and the option is on, and sleeps otherwise. Only memory, CPU and
//! the parent of each process are read: no command lines, paths or environments.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::Duration;
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

const EVERY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Part {
    pub label: String,
    /// Bytes (the working set on Windows, the resident set elsewhere).
    pub mem: u64,
    /// Percent of the whole machine, as Task Manager shows it.
    pub cpu: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Usage {
    pub total: Part,
    /// Kilno first, then each session.
    pub parts: Vec<Part>,
}

/// What the thread should measure.
#[derive(Clone, Default, PartialEq)]
struct Want {
    on: bool,
    /// The sessions' shells, by name; empty counts Kilno alone.
    sessions: Vec<(String, u32)>,
}

pub struct Sampler {
    want: Arc<Mutex<Want>>,
    out: Arc<Mutex<Option<Usage>>>,
    thread: Thread,
}

impl Sampler {
    pub fn start(ctx: egui::Context) -> Self {
        let want = Arc::new(Mutex::new(Want::default()));
        let out = Arc::new(Mutex::new(None));
        let (w, o) = (want.clone(), out.clone());
        let handle = thread::Builder::new()
            .name("kilno-usage".into())
            .spawn(move || run(&w, &o, &ctx))
            .expect("start the usage thread");
        Self { want, out, thread: handle.thread().clone() }
    }

    /// Called every frame; wakes the thread only when it was off and is now on.
    pub fn set(&self, on: bool, sessions: Vec<(String, u32)>) {
        let next = Want { on, sessions };
        let mut w = self.want.lock().unwrap_or_else(|e| e.into_inner());
        if *w != next {
            let wake = next.on && !w.on;
            *w = next;
            if wake {
                self.thread.unpark();
            }
        }
    }

    pub fn get(&self) -> Option<Usage> {
        self.out.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

fn run(want: &Mutex<Want>, out: &Mutex<Option<Usage>>, ctx: &egui::Context) {
    let me = sysinfo::get_current_pid().ok();
    let cores = thread::available_parallelism().map_or(1, |n| n.get()) as f32;
    // A system sysinfo cannot read: the line never shows.
    let Ok(mut sys) = System::new() else { return };
    let kind = ProcessRefreshKind::nothing().with_memory().with_cpu();
    loop {
        let w = want.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (Some(me), true) = (me, w.on) else {
            *out.lock().unwrap_or_else(|e| e.into_inner()) = None;
            thread::park();
            continue;
        };
        // Kilno alone needs only its own process; the sessions need the list, for the trees.
        let which = if w.sessions.is_empty() { ProcessesToUpdate::Some(&[me]) } else { ProcessesToUpdate::All };
        sys.refresh_processes_specifics(which, true, kind);
        let procs: HashMap<u32, Proc> = sys
            .processes()
            .iter()
            .map(|(pid, p)| (pid.as_u32(), Proc { parent: p.parent().map(Pid::as_u32), mem: p.memory(), cpu: p.cpu_usage() / cores }))
            .collect();
        let usage = measure(&procs, me.as_u32(), &w.sessions);
        let mut o = out.lock().unwrap_or_else(|e| e.into_inner());
        // Repaint only when what the footer prints changes.
        if o.as_ref().map(|u| line(&u.total)) != Some(line(&usage.total)) {
            ctx.request_repaint();
        }
        *o = Some(usage);
        drop(o);
        thread::park_timeout(EVERY);
    }
}

struct Proc {
    parent: Option<u32>,
    mem: u64,
    cpu: f32,
}

fn measure(procs: &HashMap<u32, Proc>, me: u32, sessions: &[(String, u32)]) -> Usage {
    let own = procs.get(&me).map_or((0, 0.0), |p| (p.mem, p.cpu));
    let mut parts = vec![Part { label: "Kilno".into(), mem: own.0, cpu: own.1 }];
    if !sessions.is_empty() {
        let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
        for (pid, p) in procs {
            if let Some(parent) = p.parent.filter(|q| q != pid) {
                children.entry(parent).or_default().push(*pid);
            }
        }
        for (label, root) in sessions {
            let (mut mem, mut cpu, mut stack) = (0, 0.0, vec![*root]);
            while let Some(pid) = stack.pop() {
                if let Some(p) = procs.get(&pid) {
                    mem += p.mem;
                    cpu += p.cpu;
                }
                stack.extend(children.get(&pid).into_iter().flatten().filter(|c| **c != me));
            }
            parts.push(Part { label: label.clone(), mem, cpu });
        }
    }
    let total = Part {
        label: String::new(),
        mem: parts.iter().map(|p| p.mem).sum(),
        cpu: parts.iter().map(|p| p.cpu).sum(),
    };
    Usage { total, parts }
}

/// "182 MB · 3%", as the footer prints it.
pub fn line(p: &Part) -> String {
    format!("{} · {:.0}%", mb(p.mem), p.cpu)
}

fn mb(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{mb:.0} MB") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(parent: Option<u32>, mb: u64, cpu: f32) -> Proc {
        Proc { parent, mem: mb << 20, cpu }
    }

    #[test]
    fn kilno_alone_or_with_each_sessions_tree() {
        // 1 Kilno; 10 a session's shell, 11 its agent, 12 what the agent runs; 20 another
        // session; 30 a stranger; 13 claims to be its own parent.
        let procs: HashMap<u32, Proc> = [
            (1, p(None, 50, 1.0)),
            (10, p(Some(1), 20, 0.0)),
            (11, p(Some(10), 300, 4.0)),
            (12, p(Some(11), 30, 1.0)),
            (13, p(Some(13), 1, 0.0)),
            (20, p(Some(1), 10, 0.5)),
            (30, p(None, 999, 50.0)),
        ]
        .into();
        let alone = measure(&procs, 1, &[]);
        assert_eq!((alone.total.mem >> 20, alone.parts.len()), (50, 1));

        let all = measure(&procs, 1, &[("fix login".into(), 10), ("new".into(), 20), ("gone".into(), 99)]);
        let mbs: Vec<_> = all.parts.iter().map(|p| (p.label.as_str(), p.mem >> 20)).collect();
        assert_eq!(mbs, [("Kilno", 50), ("fix login", 350), ("new", 10), ("gone", 0)]);
        assert_eq!(all.total.mem >> 20, 410, "the stranger is not counted");
        assert!((all.total.cpu - 6.5).abs() < 1e-4);
    }

    /// What one sample costs on this machine: `cargo test --release -p kilno-app sample_cost -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn sample_cost() {
        let mut sys = System::new().unwrap();
        let kind = ProcessRefreshKind::nothing().with_memory().with_cpu();
        let me = sysinfo::get_current_pid().unwrap();
        for (name, which) in [("kilno", ProcessesToUpdate::Some(&[me])), ("all", ProcessesToUpdate::All)] {
            sys.refresh_processes_specifics(which, true, kind);
            let t = std::time::Instant::now();
            for _ in 0..20 {
                sys.refresh_processes_specifics(which, true, kind);
            }
            eprintln!("{name}: {:?} per sample, {} processes", t.elapsed() / 20, sys.processes().len());
        }
    }

    #[test]
    fn the_line_reads_mb_then_gb() {
        assert_eq!(line(&Part { label: String::new(), mem: 182 << 20, cpu: 3.4 }), "182 MB · 3%");
        assert_eq!(line(&Part { label: String::new(), mem: 1536 << 20, cpu: 12.6 }), "1.5 GB · 13%");
    }
}
