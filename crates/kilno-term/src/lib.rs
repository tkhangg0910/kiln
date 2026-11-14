//! A terminal for one pane: VT parsing (alacritty_terminal), the colour theme, key/mouse
//! encoding, and an egui widget that draws it.
//!
//! Bytes are fed on the PTY's read thread ([`Terminal::feed`]), so a flood of output (`cat` of
//! a 10 MB file) is parsed off the UI thread; the UI only locks the grid while it draws one
//! frame. Synchronized updates (DEC 2026, Claude Code's full-screen redraws) are honoured by
//! the parser, so a redraw is never shown half done.

pub mod input;
pub mod palette;
pub mod tags;
pub mod widget;

use alacritty_terminal::event::{Event, EventListener, WindowSize};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Config, Term, TermMode};
use alacritty_terminal::vte::ansi::{Processor, StdSyncHandler};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub use alacritty_terminal;

/// Lines kept above the screen (the C# xterm used 5000; 20000 cost ~29 MB a pane).
pub const SCROLLBACK: usize = 5000;

/// Where replies the terminal itself must send (device status, colour queries) go: the PTY.
pub type Writer = Arc<dyn Fn(&[u8]) + Send + Sync>;

/// A host-side message in the terminal (terminal.html "notice"): on a new line, dim grey
/// (SGR 38;5;244), then a line break, as xterm's `writeln`.
pub fn notice(text: &str) -> Vec<u8> {
    format!("\r\n\x1b[38;5;244m{text}\x1b[0m\r\n").into_bytes()
}

/// Grid size in cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GridSize {
    pub cols: u16,
    pub rows: u16,
}

impl Dimensions for GridSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }
    fn screen_lines(&self) -> usize {
        self.rows as usize
    }
    fn columns(&self) -> usize {
        self.cols as usize
    }
}

/// Receives the parser's requests. Lives inside the Term, so it must be cheap and non-blocking.
#[derive(Clone)]
pub struct Listener {
    writer: Writer,
    shared: Arc<Mutex<ListenerState>>,
}

#[derive(Default)]
struct ListenerState {
    cell_px: (u16, u16),
    size: Option<GridSize>,
    /// OSC 52 clipboard writes, taken by the UI.
    clipboard: Vec<String>,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(s) => (self.writer)(s.as_bytes()),
            Event::ColorRequest(i, fmt) => {
                let rgb = palette::default_rgb(i);
                (self.writer)(fmt(rgb).as_bytes());
            }
            Event::TextAreaSizeRequest(fmt) => {
                let st = self.shared.lock().ok();
                if let Some((size, cell)) = st.and_then(|s| s.size.map(|z| (z, s.cell_px))) {
                    let ws = WindowSize {
                        num_lines: size.rows,
                        num_cols: size.cols,
                        cell_width: cell.0,
                        cell_height: cell.1,
                    };
                    (self.writer)(fmt(ws).as_bytes());
                }
            }
            Event::ClipboardStore(_, text) => {
                if let Ok(mut s) = self.shared.lock() {
                    s.clipboard.push(text);
                }
            }
            _ => {}
        }
    }
}

/// One pane's terminal state, shared between the PTY read thread and the UI.
pub struct Terminal {
    term: Arc<FairMutex<Term<Listener>>>,
    parser: Mutex<Processor<StdSyncHandler>>,
    listener: Listener,
    /// Bumped on every change; the UI compares it to know whether to repaint.
    generation: AtomicU64,
}

impl Terminal {
    pub fn new(size: GridSize, writer: Writer) -> Self {
        let listener = Listener {
            writer,
            shared: Arc::new(Mutex::new(ListenerState { size: Some(size), ..Default::default() })),
        };
        let config = Config { scrolling_history: SCROLLBACK, ..Config::default() };
        let term = Term::new(config, &size, listener.clone());
        Self {
            term: Arc::new(FairMutex::new(term)),
            parser: Mutex::new(Processor::new()),
            listener,
            generation: AtomicU64::new(0),
        }
    }

    /// Parse bytes from the PTY. Call from the read thread.
    pub fn feed(&self, bytes: &[u8]) {
        let Ok(mut parser) = self.parser.lock() else { return };
        let mut term = self.term.lock();
        parser.advance(&mut *term, bytes);
        drop(term);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// When a synchronized update is open, the instant it must be flushed even if the app
    /// never ends it. The owner calls [`Terminal::flush_sync`] at that time.
    pub fn sync_deadline(&self) -> Option<Instant> {
        self.parser.lock().ok().and_then(|p| p.sync_timeout().sync_timeout())
    }

    /// End an expired synchronized update so its content shows.
    pub fn flush_sync(&self) {
        let Ok(mut parser) = self.parser.lock() else { return };
        if parser.sync_timeout().sync_timeout().is_some_and(|t| t <= Instant::now()) {
            let mut term = self.term.lock();
            parser.stop_sync(&mut *term);
            drop(term);
            self.generation.fetch_add(1, Ordering::Release);
        }
    }

    /// Changes since start; differs from the last value seen when a repaint is due.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    pub(crate) fn touch(&self) {
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Lock the grid (for drawing or selection).
    pub fn lock(&self) -> impl std::ops::DerefMut<Target = Term<Listener>> + '_ {
        self.term.lock()
    }

    /// Resize the grid. The caller resizes the PTY to the same size.
    pub fn resize(&self, size: GridSize, cell_px: (u16, u16)) {
        if let Ok(mut s) = self.listener.shared.lock() {
            s.size = Some(size);
            s.cell_px = cell_px;
        }
        self.term.lock().resize(size);
        self.touch();
    }

    pub fn size(&self) -> GridSize {
        let t = self.term.lock();
        GridSize { cols: t.columns() as u16, rows: t.screen_lines() as u16 }
    }

    pub fn mode(&self) -> TermMode {
        *self.term.lock().mode()
    }

    /// Scroll the view through history.
    pub fn scroll(&self, scroll: Scroll) {
        self.term.lock().scroll_display(scroll);
        self.touch();
    }

    /// Text the app asked to put on the clipboard (OSC 52), oldest first.
    pub fn take_clipboard_requests(&self) -> Vec<String> {
        self.listener.shared.lock().map(|mut s| std::mem::take(&mut s.clipboard)).unwrap_or_default()
    }

    /// The selected text, if any.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().selection_to_string().filter(|s| !s.is_empty())
    }

    pub fn clear_selection(&self) {
        self.term.lock().selection = None;
        self.touch();
    }

    /// Report focus to the app (only when it asked for focus events, mode 1004).
    pub fn focus_report(&self, focused: bool) -> Option<&'static [u8]> {
        let mut t = self.term.lock();
        t.is_focused = focused;
        drop(t);
        self.touch();
        self.mode().contains(TermMode::FOCUS_IN_OUT).then_some(if focused { b"\x1b[I" } else { b"\x1b[O" })
    }

    /// Write bytes the terminal sends on the user's behalf (keys, pastes).
    pub fn writer(&self) -> &Writer {
        &self.listener.writer
    }

    /// The text of each visible screen row (not scrollback), wrapped rows separate, trimmed.
    pub fn screen_lines_text(&self) -> Vec<String> {
        let t = self.term.lock();
        let grid = t.grid();
        let cols = grid.columns();
        let mut out = Vec::with_capacity(grid.screen_lines());
        for line in 0..grid.screen_lines() as i32 {
            let row = &grid[alacritty_terminal::index::Line(line)];
            let mut s = String::with_capacity(cols);
            for c in 0..cols {
                let cell = &row[alacritty_terminal::index::Column(c)];
                if cell.flags.contains(alacritty_terminal::term::cell::Flags::WIDE_CHAR_SPACER) {
                    continue;
                }
                s.push(cell.c);
                if let Some(z) = cell.zerowidth() {
                    s.extend(z);
                }
            }
            out.push(s.trim().to_string());
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term() -> (Terminal, Arc<Mutex<Vec<u8>>>) {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let s2 = sent.clone();
        let t = Terminal::new(
            GridSize { cols: 20, rows: 4 },
            Arc::new(move |b: &[u8]| s2.lock().unwrap().extend_from_slice(b)),
        );
        (t, sent)
    }

    #[test]
    fn feeds_text_and_vietnamese() {
        let (t, _) = term();
        t.feed("xin ch\u{e0}o \u{1ebf}\r\n".as_bytes());
        assert_eq!(t.screen_lines_text()[0], "xin ch\u{e0}o \u{1ebf}");
        assert!(t.generation() > 0);
    }

    #[test]
    fn notices_are_dim_lines_of_their_own() {
        assert_eq!(notice("[shell exited]"), b"\r\n\x1b[38;5;244m[shell exited]\x1b[0m\r\n".to_vec());
        let (t, _) = term();
        t.feed(b"PS C:\\x> ");
        t.feed(&notice("no shell"));
        assert_eq!(t.screen_lines_text()[1], "no shell");
    }

    #[test]
    fn split_utf8_across_reads() {
        let (t, _) = term();
        let b = "\u{1ebf}".as_bytes();
        t.feed(&b[..1]);
        t.feed(&b[1..]);
        assert_eq!(t.screen_lines_text()[0], "\u{1ebf}");
    }

    #[test]
    fn answers_device_status() {
        let (t, sent) = term();
        t.feed(b"\x1b[6n");
        assert_eq!(&*sent.lock().unwrap(), b"\x1b[1;1R");
    }

    #[test]
    fn wide_chars_take_two_cells() {
        let (t, _) = term();
        t.feed("\u{4e2d}a".as_bytes());
        let g = t.lock();
        let row = &g.grid()[alacritty_terminal::index::Line(0)];
        assert_eq!(row[alacritty_terminal::index::Column(2)].c, 'a');
    }

    #[test]
    fn resize_changes_size() {
        let (t, _) = term();
        t.resize(GridSize { cols: 30, rows: 6 }, (8, 16));
        assert_eq!(t.size(), GridSize { cols: 30, rows: 6 });
    }

    // Release only: a debug build parses ~50x slower, which says nothing about the product.
    #[cfg(not(debug_assertions))]
    #[test]
    fn big_output_parses_quickly() {
        let (t, _) = term();
        let chunk = "0123456789abcdef".repeat(1024); // 16 KiB like one PTY read
        let start = Instant::now();
        for _ in 0..640 {
            t.feed(chunk.as_bytes()); // 10 MiB
        }
        assert!(start.elapsed().as_secs_f32() < 2.0, "{:?}", start.elapsed());
    }
}
