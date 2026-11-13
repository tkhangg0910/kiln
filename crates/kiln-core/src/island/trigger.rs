//! Decides whether the island peeks, shows a note, or stays away. Free of Win32 so the tests
//! can drive it with made-up cursors and clocks; the island window only gathers the inputs.
//!
//! Compact is the small pill the island shows by itself while a session works: only the mascot
//! and a status dot, and click-through, so it never stands between you and the tabs under it.
//! Drop is the "Drop here" target a file drag brings out. Pick asks which session a screenshot
//! is for.

use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IslandState {
    #[default]
    Hidden,
    Compact,
    Peek,
    Notify,
    Drop,
    Pick,
}

/// What a session wants you to know. NeedsYou and Error block the session, Done does not.
/// Reserve is the account's: a limit went past the reserve set in Settings. Tip is Kiln's own
/// short hint (the screenshot shortcut), gone after a Done's 4 s.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NoteKind {
    NeedsYou,
    Error,
    Done,
    Reserve,
    Tip,
}

/// Who a note is from. The trigger only compares it: one note per source at a time.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NoteSource {
    /// A session's pane, by the app's pane id.
    Pane(u64),
    /// An account limit, by its meter label ("5h", "7d").
    Limit(String),
    /// Kiln itself ("ShotKey", "Screenshots").
    Kiln(&'static str),
}

/// One notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IslandNote {
    pub source: NoteSource,
    pub name: String,
    pub kind: NoteKind,
}

impl IslandNote {
    pub fn new(source: NoteSource, name: impl Into<String>, kind: NoteKind) -> Self {
        Self {
            source,
            name: name.into(),
            kind,
        }
    }

    /// Blocks its session until dealt with, so it never times out.
    pub fn urgent(&self) -> bool {
        matches!(self.kind, NoteKind::NeedsYou | NoteKind::Error)
    }
}

/// A rectangle in physical pixels. Right and bottom are exclusive, like a Win32 RECT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PxRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl PxRect {
    pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
}

/// Everything the trigger looks at in one poll. Cursor and monitor are physical pixels, as
/// GetCursorPos and GetMonitorInfo report them under PerMonitorV2; `scale` is that monitor's
/// DPI over 96, so the zone can be sized in logical pixels. `busy` is fullscreen, presentation
/// or Do Not Disturb; `kiln_active` is kept apart from it because it also drops a pending Done.
/// `dragging` is an OLE drag in progress (a file, a link): a held button alone is just as
/// likely a tab or a text selection.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct IslandInput {
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub monitor: PxRect,
    pub scale: f64,
    pub button_down: bool,
    pub moving_or_sizing: bool,
    pub busy: bool,
    pub over_island: bool,
    pub now_ms: i64,
    pub kiln_active: bool,
    pub dragging: bool,
}

/// Only the very top rows count. Anything deeper is the browser's tab strip.
pub const EDGE_PX: i32 = 2;
/// Logical px, centred on the monitor. Narrow so passing over a tab does not count.
pub const ZONE_WIDTH: f64 = 240.0;
pub const DWELL_MS: i64 = 600;
pub const LEAVE_MS: i64 = 400;

// A file drag gets a bigger, quicker target than a resting cursor: carrying a file you aim at
// the top of the screen, not at its first two rows, and only an OLE drag counts, so the looser
// zone cannot bring the island out while you browse. Overshooting while aiming is forgiven for
// longer than the cursor's leave.
pub const DRAG_ZONE_WIDTH: f64 = 480.0;
pub const DRAG_ZONE_HEIGHT: f64 = 40.0;
pub const DRAG_DWELL_MS: i64 = 250;
pub const DRAG_LEAVE_MS: i64 = 1500;
pub const DONE_MS: i64 = 4000;
/// At most one self-peek in this long, so a busy afternoon does not keep popping it up.
pub const SELF_PEEK_EVERY_MS: i64 = 180_000;
/// A self-peek leaves after this even while the work runs: it has said its piece.
pub const COMPACT_MS: i64 = 10_000;
/// Resting on the compact pill this long opens the full island; the edge dwell's length.
pub const REST_MS: i64 = 600;
/// The screenshot picker gives up after this with no answer; a cursor on it holds it.
pub const PICK_MS: i64 = 15_000;
/// The login greeting is out this long: one wave, then gone.
pub const GREET_MS: i64 = 2500;
/// A greeting held back by a fullscreen app or Do Not Disturb gives up after this; much later
/// it would only say hello in the middle of something else.
pub const GREET_WITHIN_MS: i64 = 60_000;

/// The cursor is in the top rows, near the monitor's centre.
pub fn in_zone(i: &IslandInput) -> bool {
    let m = i.monitor;
    let centre = (m.left + m.right) as f64 / 2.0;
    i.cursor_y >= m.top
        && i.cursor_y < m.top + EDGE_PX
        && (i.cursor_x as f64 - centre).abs() <= ZONE_WIDTH * i.scale / 2.0
}

/// The larger target a file drag aims at.
pub fn in_drag_zone(i: &IslandInput) -> bool {
    let m = i.monitor;
    let centre = (m.left + m.right) as f64 / 2.0;
    i.cursor_y >= m.top
        && (i.cursor_y as f64) < m.top as f64 + DRAG_ZONE_HEIGHT * i.scale
        && (i.cursor_x as f64 - centre).abs() <= DRAG_ZONE_WIDTH * i.scale / 2.0
}

/// The island's state machine. Feed it every poll with [`IslandTrigger::update`].
#[derive(Debug)]
pub struct IslandTrigger {
    state: IslandState,
    /// The note on screen and its queue number (which tells a re-sent note from the old one).
    note: Option<(IslandNote, u64)>,
    dwell_progress: f64,
    dwell_length_ms: i64,

    dwell_since: Option<i64>,
    away_since: Option<i64>,
    drag_since: Option<i64>,
    /// Set when the island is sent away by hand (Esc, click). Without it a cursor still
    /// resting at the edge would bring it straight back 600 ms later.
    must_leave_first: bool,
    /// The drag going on when the island was sent away is ignored until it ends. A new drag is
    /// deliberate, so it may bring the island straight back.
    drag_end_first: bool,

    /// Every note not yet dealt with, the one showing included. At most one per source.
    notes: Vec<(IslandNote, u64)>,
    seq: u64,
    shown_at: i64,

    /// Sessions working right now, and those the compact pill has already been out for (or
    /// skipped for the rate limit) in this run of work.
    working: HashSet<NoteSource>,
    peeked: HashSet<NoteSource>,
    self_peek_at: Option<i64>,
    compact_since: i64,
    rest_since: Option<i64>,

    pick_since: i64,

    greet_pending: bool,
    greet_since: Option<i64>,
    greeting: bool,
}

impl Default for IslandTrigger {
    fn default() -> Self {
        Self::new()
    }
}

impl IslandTrigger {
    pub fn new() -> Self {
        Self {
            state: IslandState::Hidden,
            note: None,
            dwell_progress: 0.0,
            dwell_length_ms: DWELL_MS,
            dwell_since: None,
            away_since: None,
            drag_since: None,
            must_leave_first: false,
            drag_end_first: false,
            notes: Vec::new(),
            seq: 0,
            shown_at: 0,
            working: HashSet::new(),
            peeked: HashSet::new(),
            self_peek_at: None,
            compact_since: 0,
            rest_since: None,
            pick_since: 0,
            greet_pending: false,
            greet_since: None,
            greeting: false,
        }
    }

    pub fn state(&self) -> IslandState {
        self.state
    }

    /// The note on screen while the state is Notify, otherwise None.
    pub fn note(&self) -> Option<&IslandNote> {
        self.note.as_ref().map(|(n, _)| n)
    }

    /// The queue number of the note on screen: a re-sent note with the same words gets a new
    /// one, so the island can tell it apart and greet it again (the C# compared references).
    pub fn note_seq(&self) -> Option<u64> {
        self.note.as_ref().map(|(_, s)| *s)
    }

    /// 0..1 through the dwell at the edge, for the dwell bar. 0 when not dwelling.
    pub fn dwell_progress(&self) -> f64 {
        self.dwell_progress
    }

    /// How long that dwell is in all: shorter for a file drag, so the bar fills in time.
    pub fn dwell_length_ms(&self) -> i64 {
        self.dwell_length_ms
    }

    /// The compact pill on screen is the login greeting, not a session at work.
    pub fn greeting(&self) -> bool {
        self.state == IslandState::Compact && self.greeting
    }

    /// Queues a note. It shows on the next update unless something is busy. A newer note from
    /// the same source replaces the older one.
    pub fn notify(&mut self, note: IslandNote) {
        self.notes.retain(|(n, _)| n.source != note.source);
        self.seq += 1;
        self.notes.push((note, self.seq));
    }

    /// The session's status moved on (back to work, exited, closed), so whatever it said is
    /// stale: showing it later would send you to a session that no longer needs you.
    pub fn resolve(&mut self, source: &NoteSource) {
        self.notes.retain(|(n, _)| n.source != *source);
    }

    /// A session started or stopped working. A start may bring out the compact pill on a later
    /// update; one held back while Kiln is in front still shows once Kiln goes behind, as long
    /// as the work runs.
    pub fn set_working(&mut self, source: NoteSource, working: bool) {
        if working {
            self.working.insert(source);
        } else {
            self.working.remove(&source);
            self.peeked.remove(&source);
        }
    }

    /// Kiln started at login: say hello once with the compact pill, on a later update.
    pub fn greet(&mut self) {
        self.greet_pending = true;
    }

    /// Asks which session a screenshot is for. You asked for it, so it shows over everything,
    /// also while busy or with Kiln in front; notes wait behind it.
    pub fn pick(&mut self, now_ms: i64) {
        self.hide();
        self.state = IslandState::Pick;
        self.pick_since = now_ms;
    }

    /// The picker was answered or cancelled. Gone, and not back until the cursor leaves the edge.
    pub fn end_pick(&mut self) {
        if self.state != IslandState::Pick {
            return;
        }
        self.hide();
        self.must_leave_first = true;
    }

    pub fn update(&mut self, i: &IslandInput) -> IslandState {
        if self.state == IslandState::Pick {
            if i.over_island {
                self.pick_since = i.now_ms;
            } else if i.now_ms - self.pick_since >= PICK_MS {
                self.hide();
            }
            return self.state;
        }

        let zone = in_zone(i);
        if !zone {
            self.must_leave_first = false;
        }
        if !i.dragging {
            self.drag_end_first = false;
        }
        self.dwell_progress = 0.0;

        if self.greet_pending {
            let since = *self.greet_since.get_or_insert(i.now_ms);
            if i.now_ms - since >= GREET_WITHIN_MS {
                self.greet_pending = false;
            }
        }

        if i.busy || i.kiln_active {
            if i.kiln_active {
                // Kiln in front means you already found it; the hello has nothing left to say.
                self.greet_pending = false;
                // Held, not dropped: a missed permission leaves the session stuck. A Done only
                // reports, and with Kiln in front the sidebar already says it, so that one goes.
                self.notes.retain(|(n, _)| n.urgent());
            }
            // A fullscreen game or a presentation also outranks a peek asked for a moment ago.
            if self.state != IslandState::Hidden {
                self.hide();
            }
            self.dwell_since = None;
            return self.state;
        }

        let drag_zone = in_drag_zone(i);
        if self.state == IslandState::Drop {
            return self.drop_target(i, drag_zone);
        }
        if self.drag_dwell(i, drag_zone) {
            return self.state;
        }

        if self.show_note(i) {
            return self.state;
        }
        if self.state == IslandState::Notify {
            self.hide();
        }

        if self.state == IslandState::Compact {
            return self.compact(i, zone);
        }

        if self.state == IslandState::Peek {
            if zone || i.over_island {
                self.away_since = None;
            } else {
                let away = *self.away_since.get_or_insert(i.now_ms);
                if i.now_ms - away >= LEAVE_MS {
                    self.hide();
                }
            }
            return self.state;
        }

        if self.greet_now(i) || self.self_peek(i) {
            return self.state;
        }

        // A held button means a window is being dragged toward the top to snap or maximize;
        // move/size mode catches the same drag when the button state is not what Windows uses
        // (keyboard move, touch). Either way the dwell restarts from zero afterwards.
        if !zone || self.must_leave_first || i.button_down || i.moving_or_sizing {
            self.dwell_since = None;
            return self.state;
        }

        let since = *self.dwell_since.get_or_insert(i.now_ms);
        let dwelt = i.now_ms - since;
        if dwelt >= DWELL_MS {
            self.state = IslandState::Peek;
            self.dwell_since = None;
            self.away_since = None;
        } else {
            self.dwell_progress = dwelt as f64 / DWELL_MS as f64;
            self.dwell_length_ms = DWELL_MS;
        }
        self.state
    }

    /// A file dragged to the edge and held there for the dwell brings out the drop target, over
    /// whatever else shows; a note waits and comes back after. Move/size mode is a window drag,
    /// and only an OLE drag counts, so a tab dragged along the strip never brings it out.
    fn drag_dwell(&mut self, i: &IslandInput, zone: bool) -> bool {
        if !i.dragging || i.moving_or_sizing || !zone || self.drag_end_first {
            self.drag_since = None;
            return false;
        }
        let since = *self.drag_since.get_or_insert(i.now_ms);
        let dwelt = i.now_ms - since;
        if dwelt < DRAG_DWELL_MS {
            self.dwell_progress = dwelt as f64 / DRAG_DWELL_MS as f64;
            self.dwell_length_ms = DRAG_DWELL_MS;
            return false;
        }
        self.hide();
        self.state = IslandState::Drop;
        true
    }

    /// The target stays while the drag goes on over it or the drag zone. A drag that wanders
    /// off has 1.5 s to come back; one that ended elsewhere has nothing left to aim, so it goes
    /// after the usual 400 ms. A drop on it calls [`release`](Self::release) before the drag ends.
    fn drop_target(&mut self, i: &IslandInput, drag_zone: bool) -> IslandState {
        if i.dragging && (drag_zone || i.over_island) {
            self.away_since = None;
        } else {
            let away = *self.away_since.get_or_insert(i.now_ms);
            let leave = if i.dragging { DRAG_LEAVE_MS } else { LEAVE_MS };
            if i.now_ms - away >= leave {
                self.hide();
            }
        }
        self.state
    }

    /// Something was dropped. Hidden, and not back until the cursor leaves the edge, or a new
    /// drag starts.
    pub fn release(&mut self) {
        self.hide();
        self.must_leave_first = true;
        self.drag_end_first = true;
    }

    /// Brings out the compact pill if a session has started working since the last one and the
    /// rate limit allows. A start the limit blocks is skipped, not held: popping up minutes into
    /// the work would be about nothing in particular.
    fn self_peek(&mut self, i: &IslandInput) -> bool {
        let mut fresh = false;
        for s in &self.working {
            fresh |= self.peeked.insert(s.clone());
        }
        let limited = matches!(self.self_peek_at, Some(at) if i.now_ms - at < SELF_PEEK_EVERY_MS);
        if !fresh || limited {
            return false;
        }
        self.state = IslandState::Compact;
        self.self_peek_at = Some(i.now_ms);
        self.compact_since = i.now_ms;
        self.rest_since = None;
        self.dwell_since = None;
        true
    }

    /// Shows the login greeting if one is waiting. It is not a self-peek: it is about Kiln
    /// being there, not about any work, so a session starting right after still shows.
    fn greet_now(&mut self, i: &IslandInput) -> bool {
        if !self.greet_pending {
            return false;
        }
        self.greet_pending = false;
        self.greeting = true;
        self.state = IslandState::Compact;
        self.compact_since = i.now_ms;
        self.rest_since = None;
        self.dwell_since = None;
        true
    }

    fn compact(&mut self, i: &IslandInput, zone: bool) -> IslandState {
        // A held button is a tab being dragged along the strip, not a cursor resting on the pill.
        if (zone || i.over_island) && !i.button_down && !i.moving_or_sizing {
            let since = *self.rest_since.get_or_insert(i.now_ms);
            if i.now_ms - since >= REST_MS {
                self.state = IslandState::Peek;
                self.greeting = false;
                self.rest_since = None;
                self.away_since = None;
            }
            // Not sent away by the time limit mid-rest: that cursor is about to open it.
            return self.state;
        }
        self.rest_since = None;

        let over = if self.greeting {
            i.now_ms - self.compact_since >= GREET_MS
        } else {
            self.working.is_empty() || i.now_ms - self.compact_since >= COMPACT_MS
        };
        if over {
            self.hide();
        }
        self.state
    }

    /// Shows the most pressing note, if any is left. False when there is none.
    fn show_note(&mut self, i: &IslandInput) -> bool {
        let done = match &self.note {
            Some((n, _)) if !n.urgent() && self.expired(i) => Some(n.source.clone()),
            _ => None,
        };
        if let Some(source) = done {
            self.resolve(&source);
            self.note = None;
        }

        // Urgent before Done, then the newest.
        let Some(best) = self
            .notes
            .iter()
            .max_by_key(|(n, seq)| (n.urgent(), *seq))
            .cloned()
        else {
            return false;
        };

        if self.note.as_ref().map(|(_, seq)| *seq) != Some(best.1) {
            // The 4 s of a Done count from when it is actually on screen, not from when it was
            // queued behind a fullscreen game.
            self.note = Some(best);
            self.shown_at = i.now_ms;
            self.away_since = None;
        }
        self.state = IslandState::Notify;
        self.dwell_since = None;

        if i.over_island {
            self.away_since = None;
        } else {
            self.away_since.get_or_insert(i.now_ms);
        }
        true
    }

    /// A Done has had its 4 s, and the cursor is not reading it: hovering holds it, and it
    /// leaves 400 ms after the cursor does.
    fn expired(&self, i: &IslandInput) -> bool {
        i.now_ms - self.shown_at >= DONE_MS
            && !i.over_island
            && i.now_ms - self.away_since.unwrap_or(i.now_ms) >= LEAVE_MS
    }

    /// Hide now, and stay hidden until the cursor has left the zone once. A note showing is
    /// dealt with; the next pending one, if any, shows on the following update.
    pub fn dismiss(&mut self) {
        if let Some((n, _)) = self.note.take() {
            self.resolve(&n.source);
        }
        self.hide();
        self.must_leave_first = true;
        self.drag_end_first = true;
    }

    fn hide(&mut self) {
        self.state = IslandState::Hidden;
        self.note = None;
        self.greeting = false;
        self.dwell_since = None;
        self.away_since = None;
        self.rest_since = None;
        self.drag_since = None;
    }
}
