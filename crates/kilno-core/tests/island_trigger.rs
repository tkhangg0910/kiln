//! Port of checks/island.cs: the island's state machine driven with made-up cursors and clocks.

use kilno_core::island::trigger::{
    IslandInput, IslandNote, IslandState, IslandTrigger, NoteKind, NoteSource, PxRect, GREET_MS,
    GREET_WITHIN_MS,
};
use IslandState::{Compact, Drop, Hidden, Notify, Peek, Pick};

// 1920x1080 at 100%: centre x is 960, the zone runs 840..1080, the edge is rows 0 and 1.
const PRIMARY: PxRect = PxRect::new(0, 0, 1920, 1080);

fn rest() -> IslandInput {
    IslandInput {
        cursor_x: 960,
        cursor_y: 0,
        monitor: PRIMARY,
        scale: 1.0,
        ..Default::default()
    }
}

/// The cursor somewhere in the page, nowhere near the island.
fn desk() -> IslandInput {
    IslandInput {
        cursor_y: 500,
        ..rest()
    }
}

/// Resting on the compact pill: out of the 2 px edge, over its body.
fn on_pill() -> IslandInput {
    IslandInput {
        cursor_y: 30,
        over_island: true,
        ..desk()
    }
}

/// A file being dragged: the button is held and OLE's drag loop has the capture.
fn drag() -> IslandInput {
    IslandInput {
        button_down: true,
        dragging: true,
        ..rest()
    }
}

fn at(i: IslandInput, now_ms: i64) -> IslandInput {
    IslandInput { now_ms, ..i }
}

/// Feeds the same input every 50 ms from `from` to `to` inclusive, like the real poll.
fn hold(t: &mut IslandTrigger, i: IslandInput, from: i64, to: i64) -> IslandState {
    let mut s = t.state();
    let mut ms = from;
    while ms <= to {
        s = t.update(&at(i, ms));
        ms += 50;
    }
    s
}

fn fresh(i: IslandInput, from: i64, to: i64) -> IslandState {
    hold(&mut IslandTrigger::new(), i, from, to)
}

/// A trigger that is already peeking.
fn peeking() -> IslandTrigger {
    let mut t = IslandTrigger::new();
    hold(&mut t, rest(), 0, 600);
    t
}

const A: NoteSource = NoteSource::Pane(1);
const B: NoteSource = NoteSource::Pane(2);

fn needs_a() -> IslandNote {
    IslandNote::new(A, "api-refactor", NoteKind::NeedsYou)
}
fn done_a() -> IslandNote {
    IslandNote::new(A, "api-refactor", NoteKind::Done)
}
fn done_b() -> IslandNote {
    IslandNote::new(B, "docs-pass", NoteKind::Done)
}
fn error_b() -> IslandNote {
    IslandNote::new(B, "docs-pass", NoteKind::Error)
}

// ------------------------------------------------------------------ edge dwell

#[test]
fn peeks_after_a_600_ms_dwell() {
    let mut t = IslandTrigger::new();
    hold(&mut t, rest(), 0, 550);
    assert_eq!(t.update(&at(rest(), 599)), Hidden, "not yet at 599 ms");
    assert_eq!(t.update(&at(rest(), 600)), Peek);
}

#[test]
fn leaving_before_600_ms_cancels_the_dwell() {
    let mut t = IslandTrigger::new();
    hold(&mut t, rest(), 0, 500);
    t.update(&IslandInput {
        cursor_y: 40,
        now_ms: 550,
        ..rest()
    });
    assert_eq!(hold(&mut t, rest(), 600, 1100), Hidden);
    assert_eq!(
        t.update(&at(rest(), 1200)),
        Peek,
        "and the next dwell starts from zero"
    );
}

#[test]
fn left_button_held_never_peeks() {
    assert_eq!(
        fresh(
            IslandInput {
                button_down: true,
                ..rest()
            },
            0,
            2000
        ),
        Hidden
    );
}

#[test]
fn move_size_mode_never_peeks() {
    assert_eq!(
        fresh(
            IslandInput {
                moving_or_sizing: true,
                ..rest()
            },
            0,
            2000
        ),
        Hidden
    );
}

#[test]
fn held_time_does_not_count_toward_the_dwell() {
    let mut t = IslandTrigger::new();
    hold(
        &mut t,
        IslandInput {
            button_down: true,
            ..rest()
        },
        0,
        1000,
    );
    assert_eq!(t.update(&at(rest(), 1050)), Hidden);
}

#[test]
fn busy_never_peeks() {
    assert_eq!(
        fresh(
            IslandInput {
                busy: true,
                ..rest()
            },
            0,
            2000
        ),
        Hidden
    );
}

#[test]
fn busy_hides_an_active_peek() {
    let mut t = peeking();
    assert_eq!(
        t.update(&IslandInput {
            busy: true,
            now_ms: 700,
            ..rest()
        }),
        Hidden
    );
}

#[test]
fn zone_edges_on_the_primary() {
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 1080,
                ..rest()
            },
            0,
            600
        ),
        Peek,
        "right edge"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 1081,
                ..rest()
            },
            0,
            2000
        ),
        Hidden,
        "just outside"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 100,
                ..rest()
            },
            0,
            2000
        ),
        Hidden,
        "far left"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 1,
                ..rest()
            },
            0,
            600
        ),
        Peek,
        "y = 1"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 2,
                ..rest()
            },
            0,
            2000
        ),
        Hidden,
        "y = 2"
    );
}

#[test]
fn hidden_400_ms_after_leaving() {
    let mut t = peeking();
    let away = IslandInput {
        cursor_y: 300,
        ..rest()
    };
    t.update(&at(away, 1000));
    assert_eq!(
        t.update(&at(away, 1399)),
        Peek,
        "still there 399 ms after leaving"
    );
    assert_eq!(t.update(&at(away, 1400)), Hidden);
}

#[test]
fn staying_over_the_island_keeps_it() {
    // Out of the 2 px edge but over the island's 52 px body: it has to stay.
    let mut t = peeking();
    let on_island = IslandInput {
        cursor_y: 30,
        over_island: true,
        ..rest()
    };
    assert_eq!(hold(&mut t, on_island, 650, 5000), Peek);
    t.update(&IslandInput {
        cursor_y: 300,
        now_ms: 5050,
        ..rest()
    });
    t.update(&at(on_island, 5300));
    assert_eq!(
        hold(
            &mut t,
            IslandInput {
                cursor_y: 300,
                ..rest()
            },
            5350,
            5700
        ),
        Peek,
        "coming back before 400 ms resets the leave timer"
    );
}

#[test]
fn dismissed_while_resting_stays_hidden_until_the_cursor_leaves() {
    let mut t = peeking();
    t.dismiss();
    assert_eq!(hold(&mut t, rest(), 700, 3000), Hidden);
    t.update(&IslandInput {
        cursor_y: 300,
        now_ms: 3050,
        ..rest()
    });
    assert_eq!(
        hold(&mut t, rest(), 3100, 3700),
        Peek,
        "re-arms once the cursor has left"
    );
}

#[test]
fn second_monitor_zone_is_its_own_and_scaled() {
    // 2560x1440 at 150%, right of the primary and 200 px higher. Centre x is 3200; the zone is
    // 360 px wide, so 3020..3380; the edge is y -200..-199.
    let s = IslandInput {
        cursor_x: 3200,
        cursor_y: -200,
        monitor: PxRect::new(1920, -200, 4480, 1240),
        scale: 1.5,
        ..Default::default()
    };
    assert_eq!(fresh(s, 0, 600), Peek, "centre peeks");
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 3380,
                ..s
            },
            0,
            600
        ),
        Peek,
        "3380 peeks"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 3381,
                ..s
            },
            0,
            2000
        ),
        Hidden,
        "3381 is outside"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 3020,
                ..s
            },
            0,
            600
        ),
        Peek,
        "3020 peeks"
    );
    assert_eq!(
        fresh(IslandInput { cursor_y: 0, ..s }, 0, 2000),
        Hidden,
        "its own top counts"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: -199,
                ..s
            },
            0,
            600
        ),
        Peek,
        "y = -199 peeks"
    );
}

#[test]
fn px_rect_is_right_and_bottom_exclusive() {
    let r = PxRect::new(0, 0, 10, 10);
    assert!(r.contains(0, 0) && r.contains(9, 9));
    assert!(!r.contains(10, 5) && !r.contains(5, 10) && !r.contains(-1, 0));
}

// ------------------------------------------------------------------ notifications

#[test]
fn a_reserve_note_is_not_urgent() {
    let n = IslandNote::new(
        NoteSource::Limit("5h".into()),
        "5h at 82%",
        NoteKind::Reserve,
    );
    assert!(!n.urgent());
}

#[test]
fn needs_you_stays_until_the_session_moves_on() {
    let mut t = IslandTrigger::new();
    hold(&mut t, desk(), 0, 1000);
    t.notify(needs_a());
    assert_eq!(t.update(&at(desk(), 1050)), Notify, "notify from hidden");
    assert_eq!(t.note(), Some(&needs_a()), "and it shows that note");
    assert_eq!(
        hold(&mut t, desk(), 1100, 61100),
        Notify,
        "needs-you stays for a minute"
    );
    t.resolve(&A);
    assert_eq!(
        t.update(&at(desk(), 61150)),
        Hidden,
        "leaves once the session moves on"
    );
}

#[test]
fn error_stays_until_dismissed_then_goes() {
    let mut t = IslandTrigger::new();
    t.notify(error_b());
    hold(&mut t, desk(), 0, 30000);
    t.dismiss();
    assert_eq!(t.update(&at(desk(), 30050)), Hidden);
    assert_eq!(
        hold(&mut t, desk(), 30100, 32000),
        Hidden,
        "and does not come back"
    );
}

#[test]
fn notify_while_peeking_replaces_the_peek() {
    let mut t = peeking();
    t.notify(done_a());
    assert_eq!(t.update(&at(rest(), 700)), Notify);
}

#[test]
fn done_leaves_at_4_s() {
    // Shown at 1000; the poll lands on 4999 and 5000 to pin the edge.
    let mut t = IslandTrigger::new();
    t.notify(done_a());
    t.update(&at(desk(), 1000));
    hold(&mut t, desk(), 1050, 4950);
    assert_eq!(
        t.update(&at(desk(), 4999)),
        Notify,
        "done still there at 3.999 s"
    );
    assert_eq!(t.update(&at(desk(), 5000)), Hidden);
}

#[test]
fn hovering_holds_a_done_past_4_s() {
    let mut t = IslandTrigger::new();
    let over = IslandInput {
        cursor_y: 30,
        over_island: true,
        ..desk()
    };
    t.notify(done_a());
    t.update(&at(desk(), 0));
    assert_eq!(hold(&mut t, over, 3000, 9000), Notify);
    t.update(&at(desk(), 9050));
    assert_eq!(
        t.update(&at(desk(), 9449)),
        Notify,
        "still there 399 ms after the cursor leaves"
    );
    assert_eq!(
        t.update(&at(desk(), 9450)),
        Hidden,
        "gone 400 ms after the cursor leaves"
    );
}

#[test]
fn busy_holds_a_note_and_shows_it_when_busy_clears() {
    let mut t = IslandTrigger::new();
    let busy = IslandInput {
        busy: true,
        ..desk()
    };
    t.notify(needs_a());
    assert_eq!(hold(&mut t, busy, 0, 5000), Hidden);
    assert_eq!(t.update(&at(desk(), 5050)), Notify);
    assert_eq!(t.note(), Some(&needs_a()), "held note is the one queued");
}

#[test]
fn held_done_shown_for_4_s_from_release() {
    let mut t = IslandTrigger::new();
    t.notify(done_a());
    hold(
        &mut t,
        IslandInput {
            busy: true,
            ..desk()
        },
        0,
        10000,
    );
    t.update(&at(desk(), 10050));
    assert_eq!(t.update(&at(desk(), 14000)), Notify);
    assert_eq!(t.update(&at(desk(), 14050)), Hidden, "then leaves");
}

#[test]
fn busy_hides_a_showing_note() {
    let mut t = IslandTrigger::new();
    t.notify(done_a());
    t.update(&at(desk(), 0));
    assert_eq!(
        t.update(&IslandInput {
            busy: true,
            now_ms: 50,
            ..desk()
        }),
        Hidden
    );
}

#[test]
fn urgent_before_done() {
    let mut t = IslandTrigger::new();
    t.notify(error_b());
    t.notify(done_a());
    hold(
        &mut t,
        IslandInput {
            busy: true,
            ..desk()
        },
        0,
        1000,
    );
    t.update(&at(desk(), 1050));
    assert_eq!(t.note(), Some(&error_b()));
    t.dismiss();
    t.update(&at(desk(), 1100));
    assert_eq!(t.note(), Some(&done_a()), "then the done");
}

#[test]
fn newest_urgent_first() {
    let mut t = IslandTrigger::new();
    t.notify(IslandNote::new(B, "docs-pass", NoteKind::NeedsYou));
    t.notify(needs_a());
    t.update(&at(desk(), 0));
    assert_eq!(t.note(), Some(&needs_a()));
}

#[test]
fn a_newer_note_from_the_same_session_replaces_it() {
    let mut t = IslandTrigger::new();
    t.notify(done_a());
    t.update(&at(desk(), 0));
    t.notify(needs_a());
    t.update(&at(desk(), 50));
    assert_eq!(t.note(), Some(&needs_a()));
    t.notify(done_b());
    t.update(&at(desk(), 100));
    assert_eq!(
        t.note(),
        Some(&needs_a()),
        "a done does not push aside a needs-you"
    );
}

#[test]
fn stale_held_note_dropped() {
    let mut t = IslandTrigger::new();
    t.notify(needs_a());
    hold(
        &mut t,
        IslandInput {
            busy: true,
            ..desk()
        },
        0,
        1000,
    );
    t.resolve(&A);
    assert_eq!(hold(&mut t, desk(), 1050, 2000), Hidden);
}

#[test]
fn kilno_active_holds_needs_you_and_drops_done() {
    let mut t = IslandTrigger::new();
    t.notify(done_b());
    t.notify(needs_a());
    hold(
        &mut t,
        IslandInput {
            kilno_active: true,
            ..desk()
        },
        0,
        1000,
    );
    t.update(&at(desk(), 1050));
    assert_eq!(t.note(), Some(&needs_a()), "kilno active: needs-you held");
    t.dismiss();
    assert_eq!(
        hold(&mut t, desk(), 1100, 2000),
        Hidden,
        "kilno active: done dropped"
    );
}

#[test]
fn kilno_active_hides_a_showing_note() {
    let mut t = IslandTrigger::new();
    t.notify(needs_a());
    t.update(&at(desk(), 0));
    assert_eq!(
        t.update(&IslandInput {
            kilno_active: true,
            now_ms: 50,
            ..desk()
        }),
        Hidden
    );
}

#[test]
fn dismissed_note_at_the_edge_does_not_turn_into_a_peek() {
    let mut t = IslandTrigger::new();
    t.notify(needs_a());
    t.update(&at(rest(), 0));
    t.dismiss();
    assert_eq!(hold(&mut t, rest(), 50, 3000), Hidden);
}

// ------------------------------------------------------------------ dwell bar

#[test]
fn dwell_progress_fills_then_resets_once_peeking() {
    let mut t = IslandTrigger::new();
    t.update(&at(rest(), 0));
    assert_eq!(t.dwell_progress(), 0.0, "at the start");
    t.update(&at(rest(), 300));
    assert_eq!(t.dwell_progress(), 0.5, "halfway at 300 ms");
    t.update(&at(rest(), 450));
    assert_eq!(t.dwell_progress(), 0.75, "0.75 at 450 ms");
    t.update(&at(rest(), 600));
    assert_eq!(t.dwell_progress(), 0.0, "0 once peeking");
}

#[test]
fn dwell_progress_zero_when_not_dwelling() {
    let mut t = IslandTrigger::new();
    hold(&mut t, rest(), 0, 300);
    t.update(&IslandInput {
        cursor_y: 40,
        now_ms: 350,
        ..rest()
    });
    assert_eq!(t.dwell_progress(), 0.0, "the moment the cursor leaves");
    t.update(&IslandInput {
        button_down: true,
        now_ms: 400,
        ..rest()
    });
    assert_eq!(t.dwell_progress(), 0.0, "with a button held");
    t.update(&IslandInput {
        busy: true,
        now_ms: 450,
        ..rest()
    });
    assert_eq!(t.dwell_progress(), 0.0, "while busy");
}

#[test]
fn dwell_progress_zero_away_from_the_edge() {
    let mut t = IslandTrigger::new();
    hold(&mut t, desk(), 0, 1000);
    assert_eq!(t.dwell_progress(), 0.0);
}

// ------------------------------------------------------------------ compact

#[test]
fn compact_when_a_session_starts_working() {
    let mut t = IslandTrigger::new();
    hold(&mut t, desk(), 0, 1000);
    t.set_working(A, true);
    assert_eq!(t.update(&at(desk(), 1050)), Compact);
    assert_eq!(t.note(), None, "compact shows no note");
    assert_eq!(
        hold(&mut t, desk(), 1100, 11000),
        Compact,
        "stays while the work runs"
    );
    assert_eq!(t.update(&at(desk(), 11050)), Hidden, "leaves after 10 s");
    assert_eq!(
        hold(&mut t, desk(), 11100, 400_000),
        Hidden,
        "not back for the same run"
    );
}

#[test]
fn compact_waits_for_kilno_to_go_behind() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    assert_eq!(
        hold(
            &mut t,
            IslandInput {
                kilno_active: true,
                ..desk()
            },
            0,
            5000
        ),
        Hidden
    );
    assert_eq!(t.update(&at(desk(), 5050)), Compact);
}

#[test]
fn compact_not_while_busy() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    assert_eq!(
        hold(
            &mut t,
            IslandInput {
                busy: true,
                ..desk()
            },
            0,
            5000
        ),
        Hidden
    );
}

#[test]
fn compact_stays_while_one_session_still_works() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.set_working(B, true);
    t.update(&at(desk(), 0));
    t.set_working(A, false);
    assert_eq!(t.update(&at(desk(), 50)), Compact);
    t.set_working(B, false);
    assert_eq!(
        t.update(&at(desk(), 100)),
        Hidden,
        "hides when every session is idle"
    );
}

#[test]
fn self_peek_rate_limit() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    t.set_working(A, false);
    t.update(&at(desk(), 1000));
    t.set_working(B, true);
    assert_eq!(
        hold(&mut t, desk(), 2000, 60_000),
        Hidden,
        "a second start within 3 min is skipped"
    );
    t.set_working(B, false);
    t.set_working(A, true);
    assert_eq!(
        t.update(&at(desk(), 179_950)),
        Hidden,
        "still skipped just before 3 min"
    );
    t.set_working(A, false);
    t.update(&at(desk(), 180_000));
    t.set_working(A, true);
    assert_eq!(
        t.update(&at(desk(), 180_050)),
        Compact,
        "a new run 3 min later peeks again"
    );
}

#[test]
fn a_skipped_start_stays_skipped_past_the_limit() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    t.set_working(B, true);
    assert_eq!(hold(&mut t, desk(), 50, 400_000), Hidden);
}

#[test]
fn not_twice_for_the_same_session_while_it_works() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    t.update(&IslandInput {
        kilno_active: true,
        now_ms: 50,
        ..desk()
    });
    assert_eq!(t.state(), Hidden, "Kilno in front hides it");
    assert_eq!(hold(&mut t, desk(), 100, 400_000), Hidden);
    t.set_working(A, false);
    t.update(&at(desk(), 400_050));
    t.set_working(A, true);
    assert_eq!(
        t.update(&at(desk(), 400_100)),
        Compact,
        "its next run counts again"
    );
}

#[test]
fn resting_600_ms_on_the_pill_opens_the_full_island() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    assert_eq!(
        hold(&mut t, on_pill(), 50, 600),
        Compact,
        "550 ms is not yet enough"
    );
    assert_eq!(t.update(&at(on_pill(), 650)), Peek);
    assert_eq!(
        hold(&mut t, on_pill(), 700, 20_000),
        Peek,
        "stays while the cursor is on it"
    );
}

#[test]
fn resting_at_the_edge_above_the_pill_opens_it_too() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    assert_eq!(hold(&mut t, rest(), 50, 650), Peek);
}

#[test]
fn a_drag_across_the_pill_does_not_open_it() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    assert_eq!(
        hold(
            &mut t,
            IslandInput {
                button_down: true,
                ..on_pill()
            },
            50,
            3000
        ),
        Compact
    );
    assert_eq!(
        hold(&mut t, on_pill(), 3050, 3300),
        Compact,
        "passing over briefly"
    );
    hold(&mut t, desk(), 3350, 3500);
    assert_eq!(
        hold(&mut t, on_pill(), 3550, 4100),
        Compact,
        "the rest starts from zero"
    );
}

#[test]
fn a_rest_under_way_holds_it_past_10_s() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    hold(&mut t, on_pill(), 9800, 10_000);
    assert_eq!(t.state(), Compact);
    assert_eq!(
        hold(&mut t, on_pill(), 10_050, 10_400),
        Peek,
        "and then opens it"
    );
}

#[test]
fn a_note_outranks_the_compact_pill() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    t.notify(done_b());
    assert_eq!(t.update(&at(desk(), 50)), Notify);
    assert_eq!(t.note(), Some(&done_b()));
}

#[test]
fn a_pending_note_wins_over_a_fresh_self_peek() {
    let mut t = IslandTrigger::new();
    t.notify(needs_a());
    t.set_working(B, true);
    assert_eq!(t.update(&at(desk(), 0)), Notify);
}

#[test]
fn dismissed_compact_stays_away_while_that_session_works() {
    let mut t = IslandTrigger::new();
    t.set_working(A, true);
    t.update(&at(desk(), 0));
    t.dismiss();
    assert_eq!(t.update(&at(desk(), 50)), Hidden);
    assert_eq!(hold(&mut t, desk(), 100, 400_000), Hidden);
}

// ------------------------------------------------------------------ login greeting

#[test]
fn the_greeting_waves_once() {
    let mut t = IslandTrigger::new();
    t.greet();
    assert_eq!(t.update(&at(desk(), 0)), Compact);
    assert!(t.greeting(), "as the greeting");
    assert_eq!(t.note(), None);
    assert_eq!(
        hold(&mut t, desk(), 50, GREET_MS - 50),
        Compact,
        "stays though nothing works"
    );
    assert_eq!(t.update(&at(desk(), GREET_MS)), Hidden, "then hides");
    assert!(!t.greeting());
    assert_eq!(
        hold(&mut t, desk(), GREET_MS + 50, 400_000),
        Hidden,
        "only once"
    );
}

#[test]
fn greeting_held_while_busy() {
    let mut t = IslandTrigger::new();
    t.greet();
    assert_eq!(
        hold(
            &mut t,
            IslandInput {
                busy: true,
                ..desk()
            },
            0,
            30_000
        ),
        Hidden
    );
    t.update(&at(desk(), 30_050));
    assert!(t.greeting(), "shown once that ends within a minute");
}

#[test]
fn greeting_dropped_if_busy_for_longer_than_a_minute() {
    let mut t = IslandTrigger::new();
    t.greet();
    hold(
        &mut t,
        IslandInput {
            busy: true,
            ..desk()
        },
        0,
        GREET_WITHIN_MS,
    );
    assert_eq!(hold(&mut t, desk(), GREET_WITHIN_MS + 50, 400_000), Hidden);
}

#[test]
fn greeting_dropped_once_kilno_is_in_front() {
    let mut t = IslandTrigger::new();
    t.greet();
    t.update(&IslandInput {
        kilno_active: true,
        now_ms: 0,
        ..desk()
    });
    assert_eq!(hold(&mut t, desk(), 50, 400_000), Hidden);
}

#[test]
fn resting_on_the_greeting_opens_the_full_island() {
    let mut t = IslandTrigger::new();
    t.greet();
    t.update(&at(desk(), 0));
    assert_eq!(hold(&mut t, on_pill(), 50, 650), Peek);
    assert!(!t.greeting(), "which is not the greeting");
}

#[test]
fn the_greeting_does_not_use_up_the_self_peek() {
    let mut t = IslandTrigger::new();
    t.greet();
    hold(&mut t, desk(), 0, GREET_MS);
    t.set_working(A, true);
    t.update(&at(desk(), GREET_MS + 50));
    assert_eq!(t.state(), Compact);
    assert!(!t.greeting());
}

// ------------------------------------------------------------------ drop

#[test]
fn a_file_drag_at_the_edge_shows_the_drop_target_after_250_ms() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 200);
    assert_eq!(t.state(), Hidden, "waits for the drag dwell");
    assert!(
        t.dwell_progress() > 0.5,
        "and fills the dwell bar meanwhile"
    );
    assert_eq!(t.dwell_length_ms(), 250);
    assert_eq!(t.update(&at(drag(), 250)), Drop);
}

#[test]
fn drag_zone_edges() {
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 39,
                ..drag()
            },
            0,
            300
        ),
        Drop,
        "39 px down"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 1160,
                ..drag()
            },
            0,
            300
        ),
        Drop,
        "200 px off centre"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 40,
                ..drag()
            },
            0,
            2000
        ),
        Hidden,
        "40 px down"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_x: 1201,
                ..drag()
            },
            0,
            2000
        ),
        Hidden,
        "241 px off"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 20,
                ..rest()
            },
            0,
            2000
        ),
        Hidden,
        "hover zone stays two rows"
    );
}

#[test]
fn only_an_ole_drag_at_the_edge_shows_it() {
    let window_drag = IslandInput {
        button_down: true,
        moving_or_sizing: true,
        ..rest()
    };
    assert_eq!(
        fresh(window_drag, 0, 2000),
        Hidden,
        "a window drag (move/size mode)"
    );
    assert_eq!(
        fresh(
            IslandInput {
                button_down: true,
                ..rest()
            },
            0,
            2000
        ),
        Hidden,
        "a held button alone"
    );
    assert_eq!(
        fresh(
            IslandInput {
                cursor_y: 300,
                ..drag()
            },
            0,
            2000
        ),
        Hidden,
        "away from the edge"
    );
    assert_eq!(
        fresh(
            IslandInput {
                busy: true,
                ..drag()
            },
            0,
            2000
        ),
        Hidden,
        "busy"
    );
}

#[test]
fn dragging_away_keeps_the_target_1_5_s() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    let on_target = IslandInput {
        cursor_y: 120,
        over_island: true,
        ..drag()
    };
    assert_eq!(
        hold(&mut t, on_target, 300, 3000),
        Drop,
        "moving down onto the target keeps it"
    );
    let away = IslandInput {
        cursor_y: 600,
        ..drag()
    };
    t.update(&at(away, 3050));
    assert_eq!(
        t.update(&at(away, 4549)),
        Drop,
        "dragging away keeps it 1.499 s"
    );
    assert_eq!(t.update(&at(away, 4550)), Hidden);
}

#[test]
fn coming_back_within_the_grace_keeps_it() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    hold(
        &mut t,
        IslandInput {
            cursor_y: 600,
            ..drag()
        },
        300,
        1000,
    );
    let on_target = IslandInput {
        cursor_y: 120,
        over_island: true,
        ..drag()
    };
    assert_eq!(hold(&mut t, on_target, 1050, 3000), Drop);
}

#[test]
fn a_drag_that_ended_elsewhere_hides_it_after_the_leave_time() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    let over = IslandInput {
        cursor_y: 120,
        over_island: true,
        ..rest()
    };
    t.update(&at(over, 300));
    assert_eq!(hold(&mut t, over, 350, 700), Hidden);
}

#[test]
fn a_file_drag_replaces_a_note_which_shows_again_after() {
    let mut t = IslandTrigger::new();
    t.notify(done_b());
    t.update(&at(desk(), 0));
    assert_eq!(hold(&mut t, drag(), 50, 300), Drop);
    hold(&mut t, desk(), 350, 1200);
    assert_eq!(t.state(), Notify);
}

#[test]
fn dropped_at_the_edge_it_stays_away() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    t.release();
    assert_eq!(hold(&mut t, rest(), 300, 3000), Hidden);
}

#[test]
fn after_a_drop_a_new_drag_brings_it_back() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    t.release();
    hold(&mut t, rest(), 300, 500); // the drop ended that drag
    assert_eq!(hold(&mut t, drag(), 550, 900), Drop);
}

#[test]
fn the_dismissed_drag_itself_does_not_bring_it_back() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    t.dismiss(); // Esc, which also cancels the drag
    assert_eq!(hold(&mut t, drag(), 300, 2000), Hidden);
    hold(&mut t, rest(), 2050, 2200); // button up
    assert_eq!(hold(&mut t, drag(), 2250, 2600), Drop, "the next drag does");
}

#[test]
fn a_drag_that_wandered_off_brings_it_back_when_it_returns() {
    let mut t = IslandTrigger::new();
    hold(&mut t, drag(), 0, 250);
    hold(
        &mut t,
        IslandInput {
            cursor_y: 600,
            ..drag()
        },
        300,
        2000,
    );
    assert_eq!(hold(&mut t, drag(), 2050, 2400), Drop);
}

// ------------------------------------------------------------------ pick

#[test]
fn pick_outranks_everything_and_gives_up_after_15_s() {
    let away = IslandInput {
        cursor_y: 500,
        ..rest()
    };
    let mut t = IslandTrigger::new();
    t.pick(0);
    assert_eq!(t.update(&at(away, 50)), Pick, "shows at once");
    assert_eq!(
        t.update(&IslandInput {
            busy: true,
            now_ms: 100,
            ..away
        }),
        Pick,
        "busy"
    );
    assert_eq!(
        t.update(&IslandInput {
            kilno_active: true,
            now_ms: 150,
            ..away
        }),
        Pick,
        "Kilno in front"
    );
    t.notify(IslandNote::new(
        NoteSource::Kilno("pane"),
        "api",
        NoteKind::NeedsYou,
    ));
    assert_eq!(t.update(&at(away, 200)), Pick, "a note waits behind it");
    assert_eq!(t.update(&at(away, 14_950)), Pick, "still there at 14.9 s");
    assert_eq!(
        t.update(&at(away, 15_000)),
        Hidden,
        "gone at 15 s with no answer"
    );
    assert_eq!(
        t.update(&at(away, 15_050)),
        Notify,
        "then the waiting note shows"
    );
}

#[test]
fn pick_held_by_the_cursor_and_ended_by_an_answer() {
    let away = IslandInput {
        cursor_y: 500,
        ..rest()
    };
    let mut t = IslandTrigger::new();
    t.pick(0);
    t.update(&IslandInput {
        over_island: true,
        now_ms: 14_000,
        ..away
    });
    assert_eq!(t.update(&at(away, 28_000)), Pick, "a cursor on it holds it");
    t.end_pick();
    assert_eq!(t.state(), Hidden, "answered, it goes");
    assert_eq!(
        hold(&mut t, rest(), 28_050, 29_000),
        Hidden,
        "no peek straight back at the edge"
    );
}
