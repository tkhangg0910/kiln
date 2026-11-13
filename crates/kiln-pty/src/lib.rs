//! One child process under a Windows pseudoconsole (ConPTY).
//!
//! Output arrives as raw bytes on a dedicated read thread, never decoded here: a read can split
//! a UTF-8 sequence or an escape sequence in half, and the terminal parser reassembles them.
//! All `unsafe` lives in [`sys`]; this module only orchestrates threads and lifetimes.
//!
//! Teardown follows the C# original (PORTING.md §4.6): kill the child first, then close the
//! pseudoconsole off the caller's thread (before Windows 11 24H2 `ClosePseudoConsole` blocks
//! until its output is drained), while the read thread keeps draining to EOF.

#[cfg(windows)]
mod sys;

use std::io;
use std::path::PathBuf;

/// What to start, and the first size of its console.
#[derive(Clone, Debug)]
pub struct PtyConfig {
    /// Full command line, as CreateProcessW takes it.
    pub command_line: String,
    /// Working directory; ignored (Kiln's own is inherited) when it does not exist.
    pub cwd: Option<PathBuf>,
    pub cols: u16,
    pub rows: u16,
}

/// What the child does, delivered on background threads.
#[derive(Debug)]
pub enum PtyEvent {
    /// Raw bytes the child wrote to its console.
    Output(Vec<u8>),
    /// The child exited and its console is drained. Not sent once the process was dropped.
    Exited(Option<u32>),
}

#[cfg(windows)]
pub use sys::PtyProcess;

/// Spawn `cfg` under a new pseudoconsole. `sink` receives output and the exit, on a background
/// thread, in order.
#[cfg(windows)]
pub fn spawn(cfg: &PtyConfig, sink: impl FnMut(PtyEvent) + Send + 'static) -> io::Result<PtyProcess> {
    sys::PtyProcess::spawn(cfg, Box::new(sink))
}

#[cfg(not(windows))]
pub fn spawn(_cfg: &PtyConfig, _sink: impl FnMut(PtyEvent) + Send + 'static) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "ConPTY needs Windows"))
}

/// Size clamped the way the C# did: anything under 1 becomes 80 x 24.
pub fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (if cols < 1 { 80 } else { cols }, if rows < 1 { 24 } else { rows })
}
