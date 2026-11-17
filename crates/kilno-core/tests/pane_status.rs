//! Port of checks/scan.cs and checks/prompt.cs (pane status), plus the unit tests PORTING.md
//! Appendix E CHECKS implies, and the stateful tracker / one-shot deadline.

use chrono::{DateTime, TimeDelta, TimeZone, Utc};
use kilno_core::pane_status::*;
use kilno_core::sessions::TurnState;

// ------------------------------------------------------------------ scan.cs

fn feed(chunks: &[&str]) -> Option<PaneStatus> {
    let mut carry = Vec::new();
    let mut last = None;
    for c in chunks {
        if let Some(s) = scan(c.as_bytes(), &mut carry, false) {
            last = Some(s);
        }
    }
    last
}

fn feed_codex(text: &str, codex: bool) -> Option<PaneStatus> {
    scan(text.as_bytes(), &mut Vec::new(), codex)
}

#[test]
fn plain_output_is_nothing() {
    assert_eq!(feed(&["\x1b[32mrunning tests\x1b[0m\r\n"]), None);
}

#[test]
fn api_error_in_one_chunk() {
    assert_eq!(
        feed(&["API Error: 529 Overloaded."]),
        Some(PaneStatus::Error)
    );
}

#[test]
fn permission_prompt_in_one_chunk() {
    assert_eq!(
        feed(&["Do you want to proceed?"]),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn api_error_split_across_two_reads() {
    assert_eq!(feed(&["...API Er", "ror: 529"]), Some(PaneStatus::Error));
}

#[test]
fn prompt_split_across_two_reads() {
    assert_eq!(
        feed(&["Do you wa", "nt to proceed?"]),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn prompt_split_byte_by_byte() {
    let bytes: Vec<String> = "Do you want to".chars().map(|c| c.to_string()).collect();
    let refs: Vec<&str> = bytes.iter().map(String::as_str).collect();
    assert_eq!(feed(&refs), Some(PaneStatus::NeedsInput));
}

#[test]
fn no_false_positive_across_a_gap() {
    assert_eq!(
        feed(&["Do you wa", "XXXXXXXXXXXXXXXXXXXX", "nt to proceed?"]),
        None
    );
}

#[test]
fn utf8_box_drawing_is_not_a_marker() {
    assert_eq!(feed(&["╭──────────╮ ✳ thinking"]), None);
}

#[test]
fn error_wins_over_prompt_in_one_chunk() {
    assert_eq!(
        feed(&["Do you want to proceed? API Error: boom"]),
        Some(PaneStatus::Error)
    );
}

#[test]
fn prompt_with_conpty_cursor_moves_for_spaces() {
    assert_eq!(
        feed(&["\x1b[23;2HDo\x1b[1Cyou\x1b[1Cwant\x1b[1Cto\x1b[1Cproceed?\x1b[24;2H>"]),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn a_cursor_move_split_across_two_reads() {
    assert_eq!(
        feed(&["Do\x1b[1Cyou\x1b[", "1Cwant\x1b[1Cto"]),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn other_cursor_moves_are_not_spaces() {
    assert_eq!(feed(&["Do\x1b[5Cyou\x1b[1Cwant\x1b[1Cto"]), None);
}

#[test]
fn codex_run_a_command() {
    assert_eq!(
        feed_codex("Would you like to run the following command?", true),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn codex_make_edits() {
    assert_eq!(
        feed_codex("Would you like to make the following edits?", true),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn codex_network_access() {
    assert_eq!(
        feed_codex(
            "Do you want to approve network access to \"pypi.org\"?",
            true
        ),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn claude_would_you_like_to_is_just_text() {
    assert_eq!(feed_codex("Would you like to see the diff?", false), None);
}

fn asked_scan() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap()
}

#[test]
fn turn_started_working() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::Done,
            TurnState::Working,
            a,
            kilno_core::time::min(),
            true
        ),
        Some(PaneStatus::Working)
    );
}

#[test]
fn turn_complete_done() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::Working,
            TurnState::Done,
            a,
            kilno_core::time::min(),
            true
        ),
        Some(PaneStatus::Done)
    );
}

#[test]
fn turn_aborted_done_nothing_runs() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::Working,
            TurnState::Stopped,
            a,
            kilno_core::time::min(),
            true
        ),
        Some(PaneStatus::Done)
    );
}

#[test]
fn a_question_stays_for_the_tool_call_written_before_it() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::NeedsInput,
            TurnState::Working,
            a - TimeDelta::seconds(1),
            a,
            true
        ),
        None
    );
}

#[test]
fn typed_then_a_line_after_the_question_answered() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::NeedsInput,
            TurnState::Working,
            a + TimeDelta::seconds(3),
            a,
            true
        ),
        Some(PaneStatus::Working)
    );
}

#[test]
fn a_closed_shell_stays_closed() {
    let a = asked_scan();
    assert_eq!(
        from_turn(
            PaneStatus::Exited,
            TurnState::Working,
            a,
            kilno_core::time::min(),
            true
        ),
        None
    );
}

// ---------------------------------------------------------------- prompt.cs

type Events = Vec<(i64, String)>;

fn dialog(from: i64) -> Events {
    vec![
        (from, "\x1b[2K\x1b[1A\x1b[2K".into()),
        (
            from + 40,
            "╭──────────────────────────╮\r\n│ Bash command             │\r\n│   git push --dry-run     │\r\n".into(),
        ),
        (from + 80, "│ Do\x1b[1Cyou\x1b[1Cwant\x1b[1Cto\x1b[1Cproceed?  │\r\n".into()),
        (from + 120, "│ ❯ 1. Yes                 │\r\n│   2. No, and tell Claude │\r\n".into()),
        (from + 160, "╰──────────────────────────╯\r\n".into()),
    ]
}

fn burst(from: i64, chunks: i64) -> Events {
    (0..chunks)
        .map(|i| (from + i * 60, "\x1b[2K✳ Running…\r".to_string()))
        .collect()
}

fn cat(parts: &[Events]) -> Events {
    parts.concat()
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap()
}

fn ms(at: i64) -> DateTime<Utc> {
    t0() + TimeDelta::milliseconds(at)
}

/// Events in time order, keys after output at the same ms only as the C# stable sort puts them
/// (output listed first).
fn ordered(output: &Events, keys: &Events) -> Vec<(i64, String, bool)> {
    let mut ev: Vec<(i64, String, bool)> = output
        .iter()
        .map(|(a, t)| (*a, t.clone(), false))
        .chain(keys.iter().map(|(a, t)| (*a, t.clone(), true)))
        .collect();
    ev.sort_by_key(|e| e.0);
    ev
}

/// The C# simulator, on the pure functions: status starts Working, last = −∞, never resized.
fn run(output: Events, until: i64, keys: Events) -> PaneStatus {
    let mut status = PaneStatus::Working;
    let mut answered = false;
    let mut key_at: Option<DateTime<Utc>> = None;
    let mut carry = Vec::new();
    let mut recent: std::collections::VecDeque<i64> = Default::default();
    let mut last = i64::from(i32::MIN / 2);
    let ev = ordered(&output, &keys);
    let mut i = 0;
    let mut tick = 400;
    while tick <= until {
        while i < ev.len() && ev[i].0 <= tick {
            let (at, text, key) = &ev[i];
            i += 1;
            if *key {
                key_at = Some(ms(*at));
                if answers(text.as_bytes()) {
                    answered = true;
                }
                continue;
            }
            last = *at;
            if counts_as_work(ms(*at), None, key_at) {
                recent.push_back(*at);
                while recent.len() > RECENT_CAP {
                    recent.pop_front();
                }
            }
            if let Some(s) = scan(text.as_bytes(), &mut carry, false) {
                status = s;
                answered = false;
            }
        }
        let b = recent.iter().filter(|&&t| tick - t < 900).count();
        if let Some(n) = from_timer(status, b, (tick - last) as f64, answered) {
            status = n;
        }
        tick += 400;
    }
    status
}

/// The same scenario through [`PaneStatusTracker`], ticked every 400 ms.
fn run_tracker(output: Events, until: i64, keys: Events) -> PaneStatus {
    let mut tr = PaneStatusTracker::new(false, t0());
    let ev = ordered(&output, &keys);
    let mut i = 0;
    let mut tick = 400;
    while tick <= until {
        while i < ev.len() && ev[i].0 <= tick {
            let (at, text, key) = &ev[i];
            i += 1;
            if *key {
                tr.on_key(text.as_bytes(), ms(*at));
            } else {
                tr.on_output(text.as_bytes(), ms(*at));
            }
        }
        tr.on_tick(ms(tick));
        tick += 400;
    }
    tr.status()
}

fn both(output: Events, until: i64, keys: Events) -> PaneStatus {
    let a = run(output.clone(), until, keys.clone());
    let b = run_tracker(output, until, keys);
    assert_eq!(a, b, "tracker disagrees with the simulator");
    a
}

fn k(at: i64, s: &str) -> Events {
    vec![(at, s.to_string())]
}

#[test]
fn a_dialog_drawn_in_several_reads_stays_needs_you() {
    assert_eq!(
        both(cat(&[burst(0, 10), dialog(1000)]), 5000, vec![]),
        PaneStatus::NeedsInput
    );
}

#[test]
fn an_api_error_drawn_in_several_reads_stays_an_error() {
    let out: Events = vec![
        (0, "\x1b[2K".into()),
        (40, "  ⎿  API Error: 529 Overloaded\r\n".into()),
        (80, "\x1b[2K> \r\n".into()),
        (120, "│ >  │\r\n".into()),
    ];
    assert_eq!(both(out, 5000, vec![]), PaneStatus::Error);
}

#[test]
fn a_plain_turn_still_ends_done() {
    assert_eq!(both(burst(0, 10), 5000, vec![]), PaneStatus::Done);
}

#[test]
fn an_arrow_key_and_its_redraw_keep_it() {
    let out = cat(&[burst(0, 10), dialog(1000), burst(5100, 4)]);
    assert_eq!(both(out, 9000, k(5000, "\x1b[B")), PaneStatus::NeedsInput);
}

#[test]
fn a_focus_report_keeps_it() {
    let out = cat(&[burst(0, 10), dialog(1000), burst(5100, 4)]);
    assert_eq!(both(out, 9000, k(5000, "\x1b[I")), PaneStatus::NeedsInput);
}

#[test]
fn answered_working_while_it_runs() {
    let out = cat(&[burst(0, 10), dialog(1000), burst(5100, 10)]);
    assert_eq!(both(out, 5600, k(5000, "\r")), PaneStatus::Working);
}

#[test]
fn answered_done_when_the_turn_ends() {
    let out = cat(&[burst(0, 10), dialog(1000), burst(5100, 10)]);
    assert_eq!(both(out, 9000, k(5000, "\r")), PaneStatus::Done);
}

#[test]
fn answered_no_done_though_little_is_drawn_after() {
    let out = cat(&[
        burst(0, 10),
        dialog(1000),
        vec![(5100, "  ⎿  Interrupted\r\n".into())],
    ]);
    assert_eq!(both(out, 9000, k(5000, "\x1b")), PaneStatus::Done);
}

#[test]
fn a_second_question_after_the_answer_needs_you_again() {
    let out = cat(&[burst(0, 10), dialog(1000), burst(5100, 10), dialog(6000)]);
    assert_eq!(both(out, 12000, k(5000, "1")), PaneStatus::NeedsInput);
}

fn typed() -> (Events, Events) {
    let keys: Events = (0..30).map(|i| (6000 + i * 120, "a".to_string())).collect();
    let echo: Events = keys
        .iter()
        .map(|(a, _)| (a + 30, "\x1b[2Ka".to_string()))
        .collect();
    (keys, echo)
}

#[test]
fn typing_a_prompt_not_working_while_you_type_so_no_done_after() {
    let (keys, echo) = typed();
    assert_eq!(
        both(cat(&[burst(0, 10), echo]), 8000, keys),
        PaneStatus::Done
    );
}

#[test]
fn sending_it_working_once_the_reply_streams() {
    let (keys, echo) = typed();
    let keys = cat(&[keys, k(10200, "\r")]);
    assert_eq!(
        both(cat(&[burst(0, 10), echo, burst(10500, 10)]), 11000, keys),
        PaneStatus::Working
    );
}

fn asked_prompt() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 16, 12, 0, 0).unwrap()
}

#[test]
fn codex_a_line_written_while_the_approval_is_open_keeps_it() {
    let a = asked_prompt();
    assert_eq!(
        from_turn(
            PaneStatus::NeedsInput,
            TurnState::Working,
            a + TimeDelta::seconds(3),
            a,
            false
        ),
        None
    );
}

#[test]
fn codex_answered_the_next_line_is_working() {
    let a = asked_prompt();
    assert_eq!(
        from_turn(
            PaneStatus::NeedsInput,
            TurnState::Working,
            a + TimeDelta::seconds(3),
            a,
            true
        ),
        Some(PaneStatus::Working)
    );
}

#[test]
fn codex_answered_a_finished_turn_is_done() {
    let a = asked_prompt();
    assert_eq!(
        from_turn(
            PaneStatus::NeedsInput,
            TurnState::Done,
            a + TimeDelta::seconds(9),
            a,
            true
        ),
        Some(PaneStatus::Done)
    );
}

// ------------------------------------------------------- implied unit tests

#[test]
fn answers_rules() {
    assert!(!answers(b"\x1b[A"), "arrow keys only move the choice");
    assert!(!answers(b"\x1b[I"), "focus report");
    assert!(answers(b"\x1b"), "bare ESC answers no");
    assert!(answers(b"\r"));
    assert!(answers(b"1"));
    assert!(answers(b"\x1b["), "two bytes is not a sequence");
}

#[test]
fn counts_as_work_thresholds() {
    let t = t0();
    assert!(counts_as_work(t, None, None));
    assert!(
        !counts_as_work(t, Some(t - TimeDelta::milliseconds(500)), None),
        "500 ms after a resize is not work"
    );
    assert!(counts_as_work(
        t,
        Some(t - TimeDelta::milliseconds(501)),
        None
    ));
    assert!(
        !counts_as_work(t, None, Some(t - TimeDelta::milliseconds(250))),
        "250 ms after a key is not work"
    );
    assert!(counts_as_work(
        t,
        None,
        Some(t - TimeDelta::milliseconds(251))
    ));
    assert!(!counts_as_work(
        t,
        Some(t - TimeDelta::seconds(5)),
        Some(t - TimeDelta::milliseconds(100))
    ));
}

#[test]
fn scan_carry_joins_do_you_w_ant_to() {
    let mut carry = Vec::new();
    assert_eq!(scan(b"Do you w", &mut carry, false), None);
    assert_eq!(
        scan(b"ant to", &mut carry, false),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn scan_carry_is_sixteen_bytes() {
    let mut carry = Vec::new();
    scan(b"0123456789abcdefghijklmnop", &mut carry, false);
    assert_eq!(carry, b"abcdefghijklmnop");
    let mut short = Vec::new();
    scan(b"abc", &mut short, false);
    assert_eq!(short, b"abc");
    assert_eq!(CARRY_LEN, "Would you like to".len() - 1);
}

#[test]
fn scan_esc_1c_split_across_reads() {
    let mut carry = Vec::new();
    assert_eq!(scan(b"Do\x1b", &mut carry, false), None);
    assert_eq!(
        scan(b"[1Cyou\x1b[1Cwant\x1b[1Cto", &mut carry, false),
        Some(PaneStatus::NeedsInput)
    );
}

#[test]
fn scan_codex_marker_only_in_codex_panes() {
    assert_eq!(
        scan(b"Would you like to", &mut Vec::new(), true),
        Some(PaneStatus::NeedsInput)
    );
    assert_eq!(scan(b"Would you like to", &mut Vec::new(), false), None);
}

#[test]
fn from_timer_rules() {
    use PaneStatus::*;
    assert_eq!(
        from_timer(NeedsInput, 10, 5000.0, false),
        None,
        "a question stays until answered"
    );
    assert_eq!(
        from_timer(Error, 10, 5000.0, false),
        None,
        "an error stays until answered"
    );
    assert_eq!(from_timer(NeedsInput, 3, 0.0, true), Some(Working));
    assert_eq!(from_timer(Error, 0, 1501.0, true), Some(Done));
    assert_eq!(
        from_timer(Done, 3, 0.0, false),
        Some(Working),
        "burst of 3 is work"
    );
    assert_eq!(
        from_timer(Done, 2, 0.0, false),
        None,
        "2 chunks is a redraw"
    );
    assert_eq!(
        from_timer(Working, 0, 1500.0, false),
        None,
        "1500 ms is not yet quiet"
    );
    assert_eq!(from_timer(Working, 0, 1500.5, false), Some(Done));
    assert_eq!(from_timer(Starting, 0, 2000.0, false), Some(Done));
}

#[test]
fn tick_skips_exited() {
    let mut tr = PaneStatusTracker::new(false, t0());
    assert_eq!(tr.on_exit(), Some(PaneStatus::Exited));
    assert_eq!(tr.on_tick(ms(10_000)), TickOutcome::default());
    assert_eq!(tr.status(), PaneStatus::Exited);
    assert_eq!(tr.next_deadline(ms(10_000)), None);
    assert_eq!(
        tr.on_turn(TurnState::Working, ms(10_000)),
        None,
        "exited never changes"
    );
}

// --------------------------------------------------------------- tracker

#[test]
fn tracker_recent_queue_keeps_twelve() {
    let mut tr = PaneStatusTracker::new(false, t0());
    for i in 0..20 {
        tr.on_output(b"x", ms(i));
    }
    assert_eq!(tr.on_tick(ms(400)).status, Some(PaneStatus::Working));
}

#[test]
fn tracker_resize_repaint_is_not_work() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_resize(ms(0));
    for i in 0..5 {
        tr.on_output(b"repaint", ms(100 + i * 50));
    }
    assert_eq!(
        tr.on_tick(ms(400)).status,
        None,
        "Starting stays: repaint after a resize"
    );
    assert_eq!(
        tr.on_tick(ms(2400)).status,
        Some(PaneStatus::Done),
        "quiet still counts"
    );
}

#[test]
fn tracker_scan_records_question_for_codex_turns() {
    let mut tr = PaneStatusTracker::new(true, t0());
    tr.on_turn(TurnState::Working, ms(0));
    assert_eq!(tr.status(), PaneStatus::Working);
    assert_eq!(
        tr.on_output(b"Would you like to run the following command?", ms(1000)),
        Some(PaneStatus::NeedsInput)
    );
    assert_eq!(
        tr.on_turn(TurnState::Working, ms(3000)),
        None,
        "not answered"
    );
    // A key answers: Codex writes nothing while the command runs, so it is working at once.
    assert_eq!(tr.on_key(b"y", ms(4000)), Some(PaneStatus::Working));
    assert_eq!(
        tr.on_turn(TurnState::Done, ms(9000)),
        Some(PaneStatus::Done)
    );
}

#[test]
fn tracker_arrow_key_does_not_answer_a_codex_question() {
    let mut tr = PaneStatusTracker::new(true, t0());
    tr.on_turn(TurnState::Working, ms(0));
    tr.on_output(b"Would you like to", ms(1000));
    assert_eq!(tr.on_key(b"\x1b[B", ms(2000)), None);
    assert_eq!(tr.status(), PaneStatus::NeedsInput);
}

#[test]
fn tracker_question_before_line_stays_even_answered() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"Do you want to proceed?", ms(5000));
    tr.on_key(b"\r", ms(5100));
    // A transcript line from before the question does not clear it.
    assert_eq!(tr.on_turn(TurnState::Working, ms(4000)), None);
    assert_eq!(
        tr.on_turn(TurnState::Working, ms(6000)),
        Some(PaneStatus::Working)
    );
}

#[test]
fn tracker_turn_known_ignores_timer_status() {
    let mut tr = PaneStatusTracker::new(true, t0());
    tr.on_turn(TurnState::Working, ms(0));
    for i in 0..5 {
        tr.on_output(b"x", ms(100 + i * 10));
    }
    assert_eq!(
        tr.on_tick(ms(5000)).status,
        None,
        "quiet does not end a Codex turn"
    );
    assert_eq!(tr.status(), PaneStatus::Working);
}

#[test]
fn tracker_only_reports_changes() {
    let mut tr = PaneStatusTracker::new(false, t0());
    assert_eq!(
        tr.on_output(b"API Error: x", ms(0)),
        Some(PaneStatus::Error)
    );
    assert_eq!(tr.on_output(b"API Error: y", ms(10)), None);
}

// ----------------------------------------------------------------- prefill

#[test]
fn prefill_ready_needs_output_after_the_command() {
    assert!(!prefill_ready(ms(1000), None), "no command typed yet");
    assert!(!prefill_ready(ms(1000), Some(ms(1000))));
    assert!(prefill_ready(ms(1001), Some(ms(1000))));
}

#[test]
fn prefill_typed_once_at_first_done() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.set_prefill(Some("explain this".into()));
    tr.on_output(b"PS C:\\> ", ms(100));
    tr.on_command_typed(ms(400));
    // Quiet, but the only output is older than the command: the shell's prompt.
    let out = tr.on_tick(ms(2000));
    assert_eq!(out.status, Some(PaneStatus::Done));
    assert_eq!(out.prefill, None);
    for i in 0..5 {
        tr.on_output(b"claude starting", ms(2100 + i * 60));
    }
    assert_eq!(tr.on_tick(ms(2400)).status, Some(PaneStatus::Working));
    let out = tr.on_tick(ms(4400));
    assert_eq!(out.status, Some(PaneStatus::Done));
    assert_eq!(out.prefill.as_deref(), Some("explain this"));
    assert_eq!(tr.prefill(), None);
    assert_eq!(tr.on_tick(ms(4800)).prefill, None, "typed once");
}

#[test]
fn prefill_typed_while_already_done() {
    // FromTimer returns Done again on every quiet tick, which still tries the prefill.
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_command_typed(ms(0));
    tr.on_output(b"x", ms(100));
    assert_eq!(tr.on_tick(ms(2000)).status, Some(PaneStatus::Done));
    tr.set_prefill(Some("p".into()));
    let out = tr.on_tick(ms(2400));
    assert_eq!(out.status, None);
    assert_eq!(out.prefill.as_deref(), Some("p"));
}

#[test]
fn prefill_codex_after_quiet_with_turn_known() {
    let mut tr = PaneStatusTracker::new(true, t0());
    tr.set_prefill(Some("p".into()));
    tr.on_command_typed(ms(0));
    tr.on_turn(TurnState::Done, ms(0));
    tr.on_output(b"x", ms(100));
    assert_eq!(
        tr.on_tick(ms(1600)).prefill,
        None,
        "1500 ms is not yet quiet"
    );
    assert_eq!(tr.on_tick(ms(1601)).prefill.as_deref(), Some("p"));
}

#[test]
fn prefill_not_typed_on_a_question() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.set_prefill(Some("p".into()));
    tr.on_command_typed(ms(0));
    tr.on_output(
        b"Do you trust the files in this folder? Do you want to proceed",
        ms(100),
    );
    assert_eq!(
        tr.on_tick(ms(5000)),
        TickOutcome::default(),
        "a trust prompt is not the input box"
    );
}

// ---------------------------------------------------------------- deadline

#[test]
fn deadline_quiet_pane_arms_nothing() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"x", ms(0));
    tr.on_tick(ms(2000));
    assert_eq!(tr.status(), PaneStatus::Done);
    assert_eq!(tr.next_deadline(ms(2000)), None);
}

#[test]
fn deadline_after_output_is_the_quiet_time() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"x", ms(100));
    assert_eq!(tr.next_deadline(ms(100)), Some(ms(1601)));
    // Ticking at the deadline ends the turn, as a 400 ms grid tick after it would.
    assert_eq!(tr.on_tick(ms(1601)).status, Some(PaneStatus::Done));
}

#[test]
fn deadline_burst_is_due_now() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"x", ms(0));
    tr.on_output(b"x", ms(60));
    assert_eq!(
        tr.next_deadline(ms(60)),
        Some(ms(1561)),
        "two chunks: no burst yet"
    );
    tr.on_output(b"x", ms(120));
    assert_eq!(tr.next_deadline(ms(120)), Some(ms(120)));
    assert_eq!(tr.on_tick(ms(120)).status, Some(PaneStatus::Working));
    assert_eq!(tr.next_deadline(ms(120)), Some(ms(1621)));
}

#[test]
fn deadline_stale_burst_does_not_spin() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"x", ms(3000));
    tr.on_output(b"x", ms(3010));
    tr.on_output(b"x", ms(3020));
    // The app ticked late: the burst has gone stale, so nothing is due before the quiet time.
    assert_eq!(tr.next_deadline(ms(4500)), Some(ms(4521)));
}

#[test]
fn deadline_question_waits_for_a_key() {
    let mut tr = PaneStatusTracker::new(false, t0());
    tr.on_output(b"Do you want to proceed?", ms(0));
    assert_eq!(tr.next_deadline(ms(0)), None);
    tr.on_key(b"\x1b[B", ms(500));
    assert_eq!(
        tr.next_deadline(ms(500)),
        None,
        "an arrow key does not answer"
    );
    tr.on_key(b"\r", ms(600));
    assert_eq!(tr.next_deadline(ms(600)), Some(ms(1501)));
}

#[test]
fn deadline_driven_one_shot_matches_simulation_outcomes() {
    // Drive every prompt.cs scenario with one-shot ticks at next_deadline (and on each event):
    // the final status agrees with the 400 ms grid.
    let cases: Vec<(Events, i64, Events)> = vec![
        (cat(&[burst(0, 10), dialog(1000)]), 5000, vec![]),
        (burst(0, 10), 5000, vec![]),
        (
            cat(&[burst(0, 10), dialog(1000), burst(5100, 4)]),
            9000,
            k(5000, "\x1b[B"),
        ),
        (
            cat(&[burst(0, 10), dialog(1000), burst(5100, 10)]),
            9000,
            k(5000, "\r"),
        ),
        (
            cat(&[
                burst(0, 10),
                dialog(1000),
                vec![(5100, "  ⎿  Interrupted\r\n".into())],
            ]),
            9000,
            k(5000, "\x1b"),
        ),
        (
            cat(&[burst(0, 10), dialog(1000), burst(5100, 10), dialog(6000)]),
            12000,
            k(5000, "1"),
        ),
    ];
    for (output, until, keys) in cases {
        let want = run(output.clone(), until, keys.clone());
        let mut tr = PaneStatusTracker::new(false, t0());
        let mut deadline: Option<DateTime<Utc>> = None;
        for (at, text, key) in ordered(&output, &keys) {
            while let Some(d) = deadline.filter(|d| *d <= ms(at)) {
                tr.on_tick(d);
                deadline = tr.next_deadline(d);
            }
            if key {
                tr.on_key(text.as_bytes(), ms(at));
            } else {
                tr.on_output(text.as_bytes(), ms(at));
            }
            deadline = tr.next_deadline(ms(at));
        }
        while let Some(d) = deadline.filter(|d| *d <= ms(until)) {
            tr.on_tick(d);
            deadline = tr.next_deadline(d);
        }
        assert_eq!(tr.status(), want);
    }
}

// --------------------------------------------------------------- Claude hooks

/// Once a hook has reported, an idle Claude's redraw burst no longer fakes a turn and a Done.
#[test]
fn hooks_decide_working_and_done() {
    let mut t = PaneStatusTracker::new(false, ms(0));
    assert_eq!(t.on_hook(Hook::Working, ms(1000)), Some(PaneStatus::Working));
    assert_eq!(t.on_hook(Hook::Done, ms(5000)), Some(PaneStatus::Done));

    // A redraw burst while idle: three chunks within 900 ms, then quiet.
    for i in 0..4 {
        t.on_output(b"\x1b[2K redraw", ms(10_000 + i * 100));
    }
    assert_eq!(t.on_tick(ms(10_400)).status, None, "no Working from a burst");
    assert_eq!(t.on_tick(ms(13_000)).status, None, "and so no second Done");
    assert_eq!(t.status(), PaneStatus::Done);
}

#[test]
fn hook_ask_waits_for_an_answer() {
    let mut t = PaneStatusTracker::new(false, ms(0));
    t.on_hook(Hook::Working, ms(0));
    assert_eq!(t.on_hook(Hook::Ask, ms(2000)), Some(PaneStatus::NeedsInput));
    assert_eq!(t.on_tick(ms(9000)).status, None, "a question stands while quiet");
    assert_eq!(t.on_key(b"1", ms(10_000)), Some(PaneStatus::Working), "answered: back to work");
}

#[test]
fn a_turn_stopped_without_a_hook_ends_after_a_long_quiet() {
    let mut t = PaneStatusTracker::new(false, ms(0));
    t.on_hook(Hook::Working, ms(0));
    t.on_output(b"spinner", ms(1000));
    assert_eq!(t.on_tick(ms(3000)).status, None, "a short quiet is not the end");
    assert_eq!(t.next_deadline(ms(3000)), Some(ms(1000 + HOOK_QUIET_MS + 1)));
    assert_eq!(t.on_tick(ms(1000 + HOOK_QUIET_MS + 1)).status, Some(PaneStatus::Done), "Esc: no Stop came");
}

#[test]
fn without_hooks_the_stream_still_decides() {
    let mut t = PaneStatusTracker::new(false, ms(0));
    for i in 0..4 {
        t.on_output(b"work", ms(1000 + i * 100));
    }
    assert_eq!(t.on_tick(ms(1400)).status, Some(PaneStatus::Working));
    assert_eq!(t.on_tick(ms(3500)).status, Some(PaneStatus::Done));
}
