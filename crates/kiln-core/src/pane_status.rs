//! A pane's status light, inferred from the terminal output and (Codex) the transcript.
//! Port of TerminalPane.Scan/Answers/CountsAsWork/FromTimer/FromTurn (PORTING.md §4.5).
//!
//! Working vs waiting-for-you comes from the shape of the output stream alone. "Any output means
//! working" is wrong: an idle Claude Code still emits the odd redraw. So the two transitions use
//! different tests: going quiet ends the turn, and claiming work needs *sustained* output, which
//! a lone redraw cannot fake. Between the two the previous state stands. Silence cannot tell
//! "finished" from "waiting on you", so questions and errors come from markers in the bytes.

use crate::sessions::TurnState;
use chrono::{DateTime, TimeDelta, Utc};
use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneStatus {
    Starting,
    Working,
    /// The agent asked something and is blocked until you answer.
    NeedsInput,
    /// Quiet with nothing outstanding: the turn is over.
    Done,
    /// The agent printed an API failure.
    Error,
    /// The pane's shell ended.
    Exited,
}

// Two ASCII literals, not a parser: every API failure Claude Code prints begins "API Error:",
// and every permission prompt asks "Do you want to". ASCII bytes never occur inside a multi-byte
// UTF-8 sequence, so the raw stream is scanned without decoding it.
const ERROR_MARK: &[u8] = b"API Error:";
const ASK_MARK: &[u8] = b"Do you want to";
/// Codex asks before a command, an edit or new permissions with "Would you like to". Codex panes
/// only: Claude's own replies say it too often.
const CODEX_ASK_MARK: &[u8] = b"Would you like to";
/// ConPTY sends each space between words as a cursor move.
const CURSOR_RIGHT: &[u8] = b"\x1b[1C";

/// Bytes kept from one chunk for the next: (longest marker − 1) = 16.
pub const CARRY_LEN: usize = 16;

/// Ticks of the C# idle timer; also the period an app may still drive [`PaneStatusTracker::on_tick`] at.
pub const TICK: TimeDelta = TimeDelta::milliseconds(400);
/// Chunks counted as work within this window make a burst.
pub const BURST_WINDOW_MS: i64 = 900;
/// A burst of at least this many chunks means work.
pub const BURST_MIN: usize = 3;
/// Quiet for longer than this ends the turn.
pub const QUIET_MS: i64 = 1500;
/// Chunks remembered for the burst count.
pub const RECENT_CAP: usize = 12;

fn find(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// `ESC[1C` back to spaces so the markers match (#27).
fn spaced(buf: Vec<u8>) -> Vec<u8> {
    if !find(&buf, CURSOR_RIGHT) {
        return buf;
    }
    let mut out = Vec::with_capacity(buf.len());
    let mut i = 0;
    while i < buf.len() {
        if buf[i..].starts_with(CURSOR_RIGHT) {
            out.push(b' ');
            i += CURSOR_RIGHT.len();
        } else {
            out.push(buf[i]);
            i += 1;
        }
    }
    out
}

/// The status a chunk implies, or None if it says nothing new. `carry` holds the tail of the
/// previous chunk (after `ESC[1C` → space) so a marker straddling two pipe reads still matches.
/// An error wins over a question in the same chunk.
pub fn scan(chunk: &[u8], carry: &mut Vec<u8>, codex: bool) -> Option<PaneStatus> {
    let mut joined = std::mem::take(carry);
    joined.extend_from_slice(chunk);
    let buf = spaced(joined);
    *carry = buf[buf.len().saturating_sub(CARRY_LEN)..].to_vec();

    if find(&buf, ERROR_MARK) {
        Some(PaneStatus::Error)
    } else if find(&buf, ASK_MARK) || (codex && find(&buf, CODEX_ASK_MARK)) {
        Some(PaneStatus::NeedsInput)
    } else {
        None
    }
}

/// Whether keys typed in the pane can answer a question. Arrow keys only move the choice, and
/// focus and mouse reports come with no key at all: all of those start `ESC [` (and are longer
/// than two bytes). A bare ESC answers ("no").
pub fn answers(keys: &[u8]) -> bool {
    !(keys.len() > 2 && keys[0] == 0x1b && keys[1] == b'[')
}

/// Whether output at `at` counts toward a burst. A resize repaints the whole screen and each key
/// is echoed; counted, either reads as sustained output and raises a Done for a turn that never
/// happened. Both still count as output for going quiet. `None` = never resized / no key yet.
pub fn counts_as_work(
    at: DateTime<Utc>,
    resized_at: Option<DateTime<Utc>>,
    key_at: Option<DateTime<Utc>>,
) -> bool {
    let after =
        |t: Option<DateTime<Utc>>, ms: i64| t.is_none_or(|t| at - t > TimeDelta::milliseconds(ms));
    after(resized_at, 500) && after(key_at, 250)
}

/// The idle timer's call: `burst` = chunks counted as work in the last 900 ms, `quiet_ms` = ms
/// since the last output. None leaves the status as it is.
pub fn from_timer(
    current: PaneStatus,
    burst: usize,
    quiet_ms: f64,
    answered: bool,
) -> Option<PaneStatus> {
    // A question or an error stays until you type in the pane (#27): its own drawing reads as a
    // burst, and the silence after it as done.
    if matches!(current, PaneStatus::NeedsInput | PaneStatus::Error) && !answered {
        return None;
    }
    if burst >= BURST_MIN {
        return Some(PaneStatus::Working);
    }
    (quiet_ms > QUIET_MS as f64).then_some(PaneStatus::Done)
}

/// A Codex pane's status from its transcript; `at` is its last line's time. A question on screen
/// is not in the transcript, and a turn waiting on it still writes lines, so it stands until you
/// type and a line newer than the question follows. A stopped turn reads as done.
pub fn from_turn(
    current: PaneStatus,
    turn: TurnState,
    at: DateTime<Utc>,
    asked_at: DateTime<Utc>,
    answered: bool,
) -> Option<PaneStatus> {
    if current == PaneStatus::Exited {
        return None;
    }
    if current == PaneStatus::NeedsInput && (!answered || at <= asked_at) {
        return None;
    }
    Some(if turn == TurnState::Working {
        PaneStatus::Working
    } else {
        PaneStatus::Done
    })
}

/// Whether the prefill may be typed: the agent has drawn its input box once output newer than
/// the typed command has gone quiet. Output older than the command is the shell's prompt.
pub fn prefill_ready(last_output: DateTime<Utc>, command_typed_at: Option<DateTime<Utc>>) -> bool {
    command_typed_at.is_some_and(|t| last_output > t)
}

/// What a tick decided.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickOutcome {
    /// The new status, only when it changed.
    pub status: Option<PaneStatus>,
    /// Text to write to the pty once, without Enter (the pane's prefill).
    pub prefill: Option<String>,
}

/// One pane's status state: the scan carry, the recent work chunks, and the times keys, resizes
/// and questions happened. Feed it every pty read, key, resize and (Codex) transcript turn.
///
/// The C# pane ticked every 400 ms; driven at 400 ms, [`Self::on_tick`] decides exactly as it
/// did. To idle without a periodic timer (PORTING.md D13), arm a one-shot timer at
/// [`Self::next_deadline`] after every call instead.
#[derive(Clone, Debug)]
pub struct PaneStatusTracker {
    codex: bool,
    status: PaneStatus,
    carry: Vec<u8>,
    recent: VecDeque<DateTime<Utc>>,
    last_output: DateTime<Utc>,
    key_at: Option<DateTime<Utc>>,
    resized_at: Option<DateTime<Utc>>,
    /// A key typed since the last question or error appeared, so it may be answered.
    answered: bool,
    /// When a question last appeared (compared with transcript times, so UTC; D11).
    asked_at: DateTime<Utc>,
    /// A Codex transcript has reported a turn, so working/done come from it from here on.
    turn_known: bool,
    command_typed_at: Option<DateTime<Utc>>,
    prefill: Option<String>,
}

impl PaneStatusTracker {
    /// A pane starting now; `codex` turns on Codex's own question marker.
    pub fn new(codex: bool, now: DateTime<Utc>) -> Self {
        Self {
            codex,
            status: PaneStatus::Starting,
            carry: Vec::new(),
            recent: VecDeque::with_capacity(RECENT_CAP + 1),
            last_output: now,
            key_at: None,
            resized_at: None,
            answered: false,
            asked_at: crate::time::min(),
            turn_known: false,
            command_typed_at: None,
            prefill: None,
        }
    }

    pub fn status(&self) -> PaneStatus {
        self.status
    }

    pub fn last_output(&self) -> DateTime<Utc> {
        self.last_output
    }

    /// Text to type into the agent's input once, unsent, at the first quiet after the command.
    pub fn set_prefill(&mut self, text: Option<String>) {
        self.prefill = text;
    }

    pub fn prefill(&self) -> Option<&str> {
        self.prefill.as_deref()
    }

    /// The agent command was typed into the shell.
    pub fn on_command_typed(&mut self, now: DateTime<Utc>) {
        self.command_typed_at = Some(now);
    }

    fn set(&mut self, s: PaneStatus) -> Option<PaneStatus> {
        (self.status != s).then(|| {
            self.status = s;
            s
        })
    }

    /// A pty read. Returns the new status when a marker changed it.
    pub fn on_output(&mut self, bytes: &[u8], now: DateTime<Utc>) -> Option<PaneStatus> {
        self.last_output = now;
        if counts_as_work(now, self.resized_at, self.key_at) {
            self.recent.push_back(now);
            while self.recent.len() > RECENT_CAP {
                self.recent.pop_front();
            }
        }
        let seen = scan(bytes, &mut self.carry, self.codex)?;
        if seen == PaneStatus::NeedsInput {
            self.asked_at = now;
        }
        self.answered = false;
        self.set(seen)
    }

    /// Keys sent to the pty. Returns the new status when it changed.
    pub fn on_key(&mut self, bytes: &[u8], now: DateTime<Utc>) -> Option<PaneStatus> {
        self.key_at = Some(now);
        if !answers(bytes) {
            return None;
        }
        self.answered = true;
        // Codex writes nothing while an approved command runs, so its transcript would leave the
        // question up; the turn goes on either way, and the transcript corrects this from here.
        if self.turn_known && self.status == PaneStatus::NeedsInput {
            return self.set(PaneStatus::Working);
        }
        None
    }

    /// The pty was resized (the app skips same-size and minimized resizes).
    pub fn on_resize(&mut self, now: DateTime<Utc>) {
        self.resized_at = Some(now);
    }

    /// A Codex transcript's turn and its last line's time.
    pub fn on_turn(&mut self, turn: TurnState, at: DateTime<Utc>) -> Option<PaneStatus> {
        self.turn_known = true;
        let next = from_turn(self.status, turn, at, self.asked_at, self.answered)?;
        self.set(next)
    }

    /// The shell ended.
    pub fn on_exit(&mut self) -> Option<PaneStatus> {
        self.set(PaneStatus::Exited)
    }

    fn quiet_ms(&self, now: DateTime<Utc>) -> f64 {
        (now - self.last_output)
            .num_microseconds()
            .map_or(f64::MAX, |us| us as f64 / 1000.0)
    }

    fn take_prefill(&mut self) -> Option<String> {
        if prefill_ready(self.last_output, self.command_typed_at) {
            self.prefill.take()
        } else {
            None
        }
    }

    /// The idle timer.
    pub fn on_tick(&mut self, now: DateTime<Utc>) -> TickOutcome {
        let mut out = TickOutcome::default();
        if self.status == PaneStatus::Exited {
            return out;
        }
        let quiet = self.quiet_ms(now);
        if self.turn_known {
            if quiet > QUIET_MS as f64 {
                out.prefill = self.take_prefill();
            }
            return out;
        }
        let burst = self.burst_at(now);
        if let Some(next) = from_timer(self.status, burst, quiet, self.answered) {
            out.status = self.set(next);
            if next == PaneStatus::Done {
                out.prefill = self.take_prefill();
            }
        }
        out
    }

    fn burst_at(&self, now: DateTime<Utc>) -> usize {
        self.recent
            .iter()
            .filter(|&&t| now - t < TimeDelta::milliseconds(BURST_WINDOW_MS))
            .count()
    }

    /// The earliest time a tick could change anything, or None while nothing can happen without
    /// new input (a quiet pane arms no timer). `now` itself means "tick now". Call after every
    /// `on_*` and tick. A hint for a one-shot timer: it may decide up to one 400 ms tick sooner
    /// than the C# grid did.
    pub fn next_deadline(&self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if self.status == PaneStatus::Exited {
            return None;
        }
        let quiet_at = self.last_output + TimeDelta::milliseconds(QUIET_MS + 1);
        let prefill_due =
            self.prefill.is_some() && prefill_ready(self.last_output, self.command_typed_at);
        if self.turn_known {
            return prefill_due.then_some(quiet_at);
        }
        if matches!(self.status, PaneStatus::NeedsInput | PaneStatus::Error) && !self.answered {
            return None;
        }
        if self.status != PaneStatus::Working && self.burst_at(now) >= BURST_MIN {
            return Some(now);
        }
        (self.status != PaneStatus::Done || prefill_due).then_some(quiet_at)
    }
}
