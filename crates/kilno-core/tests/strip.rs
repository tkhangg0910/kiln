//! checks/strip.cs: the header strip and footer text.

use chrono::{DateTime, Duration, TimeZone, Utc};
use kilno_core::git::GitStatus;
use kilno_core::settings::KilnoSettings;
use kilno_core::status::reserve::ReserveWatch;
use kilno_core::status::strip::{
    band_for, effort_step, footer, header, limit_meter, model_name, reset_in, tokens, Band, Meter,
};
use kilno_core::status::{AccountLimits, Limit, SessionStatus};

fn now() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 3, 12, 0, 0).unwrap()
}

fn all() -> KilnoSettings {
    KilnoSettings::default()
}

/// The design's sample session: Opus 5.5 1M, 340k of 1M, xhigh; 5h at 62%, 7d at 14%.
fn status() -> SessionStatus {
    SessionStatus {
        session_id: "3f136615-e5ba-45b2-9766-6cc4d88ef5d9".into(),
        model: Some("Opus 5.5 (1M context)".into()),
        cwd: Some(r"C:\work".into()),
        context_used: 340_012,
        context_size: 1_000_000,
        context_percent: 34,
        effort: "xhigh".into(),
        five_hour: Some(Limit::new(
            62,
            Some(now() + Duration::hours(1) + Duration::minutes(20)),
        )),
        seven_day: Some(Limit::new(
            14,
            Some(now() + Duration::days(4) + Duration::hours(10)),
        )),
        updated: now(),
    }
}

fn git() -> GitStatus {
    GitStatus {
        branch: "feature/upgrade-kilno-v1".into(),
        added: 12,
        deleted: 3,
    }
}

fn limits() -> AccountLimits {
    let s = status();
    AccountLimits {
        five_hour: s.five_hour,
        seven_day: s.seven_day,
        updated: now(),
    }
}

fn labels(m: &[Meter]) -> String {
    m.iter()
        .map(|m| m.label.as_str())
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn bands() {
    for (p, b) in [
        (0, Band::Green),
        (49, Band::Green),
        (50, Band::Yellow),
        (69, Band::Yellow),
        (70, Band::Orange),
        (89, Band::Orange),
        (90, Band::Red),
        (104, Band::Red),
    ] {
        assert_eq!(band_for(p), b, "band {p}");
    }
}

#[test]
fn token_counts() {
    assert_eq!(tokens(999), "999", "tokens under a thousand");
    assert_eq!(tokens(340_012), "340k", "tokens in k");
    assert_eq!(tokens(200_000), "200k", "tokens 200k");
    assert_eq!(tokens(1_000_000), "1M", "tokens a round million");
    assert_eq!(tokens(1_500_000), "1.5M", "tokens a million and a half");
}

#[test]
fn model_names() {
    assert_eq!(
        model_name("Opus 5.5 (1M context)"),
        "Opus 5.5 1M",
        "model with a context size"
    );
    assert_eq!(
        model_name("Sonnet 5.5 (200k context)"),
        "Sonnet 5.5 200k",
        "model with a k size"
    );
    assert_eq!(model_name("Haiku 4.5"), "Haiku 4.5", "model without one");
}

#[test]
fn reset_countdowns() {
    let n = now();
    assert_eq!(
        reset_in(n + Duration::hours(1) + Duration::minutes(20), n),
        "1h20m",
        "reset in hours"
    );
    assert_eq!(
        reset_in(n + Duration::hours(3) + Duration::minutes(5), n),
        "3h05m",
        "reset in hours pads minutes"
    );
    assert_eq!(
        reset_in(n + Duration::days(4) + Duration::hours(10), n),
        "4d10h",
        "reset in days"
    );
    assert_eq!(
        reset_in(n + Duration::minutes(20) + Duration::seconds(30), n),
        "20m",
        "reset in minutes"
    );
    assert_eq!(reset_in(n - Duration::minutes(1), n), "now", "reset passed");
}

#[test]
fn header_strip() {
    let s = header(
        Some(&status()),
        Some(&git()),
        Some(&limits()),
        &all(),
        now(),
    )
    .unwrap();
    assert_eq!(s.model.as_deref(), Some("Opus 5.5 1M"), "model");
    assert_eq!(s.git, Some(git()), "branch");
    let c = s.context.unwrap();
    assert_eq!(c.label, "ctx", "context label");
    assert_eq!(c.detail.as_deref(), Some("340k/1M"), "context numbers");
    assert_eq!(c.percent, 34, "context percent");
    assert_eq!(c.band(), Band::Green, "context band");
    assert_eq!(
        c.tip, "Context: 340k of 1M tokens used (34%)",
        "context tooltip"
    );
    assert_eq!(s.effort.as_deref(), Some("xhigh"), "effort");
    assert_eq!(
        (s.five_hour, s.seven_day),
        (None, None),
        "limits stay in the footer while it is on"
    );
}

#[test]
fn header_with_footer_off() {
    let no_footer = KilnoSettings {
        status_footer: false,
        ..all()
    };
    let s = header(
        Some(&status()),
        Some(&git()),
        Some(&limits()),
        &no_footer,
        now(),
    )
    .unwrap();
    let h5 = s.five_hour.unwrap();
    assert_eq!(h5.label, "5h", "5h label");
    assert_eq!(h5.detail.as_deref(), Some("\u{21BB} 1h20m"), "5h reset");
    assert_eq!(h5.band(), Band::Yellow, "5h band");
    assert_eq!(
        h5.tip, "5-hour limit: 62% used, resets in 1h20m",
        "5h tooltip"
    );
    assert_eq!(
        s.seven_day.unwrap().tip,
        "7-day limit: 14% used, resets in 4d10h",
        "7d tooltip"
    );
    let no_7d = KilnoSettings {
        status_seven_day: false,
        ..no_footer.clone()
    };
    assert_eq!(
        header(
            Some(&status()),
            Some(&git()),
            Some(&limits()),
            &no_7d,
            now()
        )
        .unwrap()
        .seven_day,
        None,
        "a limit unticked stays out"
    );
    assert_eq!(
        header(Some(&status()), Some(&git()), None, &no_footer, now())
            .unwrap()
            .five_hour,
        None,
        "no limits reported yet shows none"
    );
    assert_eq!(
        limit_meter(true, &Limit::new(5, None), now(), 0).detail,
        None,
        "a limit without a reset time has no countdown"
    );
}

#[test]
fn bare_reset() {
    assert_eq!(
        footer(Some(&limits()), &all(), now())[0].reset.as_deref(),
        Some("1h20m"),
        "a limit's reset alone"
    );
    let s = header(
        Some(&status()),
        Some(&git()),
        Some(&limits()),
        &all(),
        now(),
    )
    .unwrap();
    assert_eq!(s.context.unwrap().reset, None, "context has no reset");
}

#[test]
fn effort_steps() {
    assert_eq!(effort_step("low"), 0, "effort low");
    assert_eq!(effort_step("medium"), 1, "effort medium");
    assert_eq!(effort_step("med"), 1, "effort med");
    assert_eq!(effort_step("xhigh"), 3, "effort xhigh");
    assert_eq!(effort_step("max"), 4, "effort max");
    assert_eq!(effort_step("turbo"), -1, "effort unknown shows no steps");
}

#[test]
fn sidebar_footer() {
    let l = limits();
    let f = footer(Some(&l), &all(), now());
    assert_eq!(
        f.iter()
            .map(|m| m.label.as_str())
            .collect::<Vec<_>>()
            .join(" "),
        "5h 7d",
        "footer shows 5h then 7d"
    );
    assert_eq!(f[0].detail.as_deref(), Some("↻ 1h20m"), "footer 5h reset");
    assert_eq!(f[1].band(), Band::Green, "footer 7d band");
    assert!(!f[0].stale, "a current limit is not stale");
    assert_eq!(
        footer(
            Some(&l),
            &KilnoSettings {
                status_footer: false,
                ..all()
            },
            now()
        )
        .len(),
        0,
        "footer off shows none there"
    );
    assert_eq!(
        footer(None, &all(), now()).len(),
        0,
        "no limits reported yet shows no footer"
    );
    assert_eq!(
        labels(&footer(
            Some(&l),
            &KilnoSettings {
                status_five_hour: false,
                ..all()
            },
            now()
        )),
        "7d",
        "an unticked limit stays out of the footer"
    );
    assert_eq!(
        footer(
            Some(&l),
            &KilnoSettings {
                status_header: false,
                ..all()
            },
            now()
        )
        .len(),
        2,
        "the header off leaves the footer as it is"
    );

    // Past its reset time the number belongs to the old window until Claude reports again.
    let stale = footer(Some(&l), &all(), now() + Duration::hours(2));
    assert!(stale[0].stale, "a limit past its reset is stale");
    assert_eq!(
        stale[0].tip, "5-hour limit: reset since the last report, which said 62%",
        "a stale limit says so"
    );
    assert_eq!(stale[0].detail, None, "a stale limit has no countdown");
    assert!(!stale[1].stale, "the 7d limit is still current then");
    assert_eq!(
        footer(Some(&l), &all(), now() + Duration::minutes(30))[0]
            .detail
            .as_deref(),
        Some("↻ 50m"),
        "the countdown ticks without new data"
    );
}

#[test]
fn reserve() {
    let n = now();
    let at80 = KilnoSettings {
        reserve_at: 80,
        ..all()
    };
    let near = AccountLimits {
        five_hour: Some(Limit::new(82, Some(n + Duration::hours(1)))),
        seven_day: Some(Limit::new(14, Some(n + Duration::days(4)))),
        updated: n,
    };
    let f = footer(Some(&near), &at80, n);
    assert_eq!(f[0].band(), Band::Red, "past the reserve reads red");
    assert_eq!(f[1].band(), Band::Green, "under the reserve keeps its band");
    assert_eq!(
        f[0].tip, "5-hour limit: 82% used, resets in 1h00m. Past your 80% reserve",
        "the tooltip says why it is red"
    );
    assert_eq!(
        footer(Some(&near), &all(), n)[0].band(),
        Band::Orange,
        "reserve off keeps the band"
    );
    let not_5h = KilnoSettings {
        reserve_five_hour: false,
        ..at80.clone()
    };
    assert_eq!(
        footer(Some(&near), &not_5h, n)[0].band(),
        Band::Orange,
        "a limit the reserve does not watch keeps its band"
    );
    let header_only = KilnoSettings {
        status_footer: false,
        ..at80.clone()
    };
    assert_eq!(
        header(Some(&status()), Some(&git()), Some(&near), &header_only, n)
            .unwrap()
            .five_hour
            .unwrap()
            .band(),
        Band::Red,
        "the reserve shows in the header too"
    );
    assert_eq!(
        footer(Some(&near), &at80, n + Duration::hours(2))[0].band(),
        Band::Orange,
        "a stale limit is not past the reserve"
    );

    let mut watch = ReserveWatch::new();
    assert_eq!(
        labels(&watch.crossed(Some(&near), &at80, n)),
        "5h",
        "crossing warns"
    );
    assert_eq!(
        watch.crossed(Some(&near), &at80, n).len(),
        0,
        "once per window"
    );
    assert_eq!(
        watch.crossed(None, &at80, n).len(),
        0,
        "nothing reported warns nothing"
    );
    let next = AccountLimits {
        five_hour: Some(Limit::new(85, Some(n + Duration::hours(6)))),
        ..near.clone()
    };
    assert_eq!(
        labels(&watch.crossed(Some(&next), &at80, n + Duration::minutes(90))),
        "5h",
        "a new window warns again"
    );
    let both = AccountLimits {
        five_hour: Some(Limit::new(91, Some(n + Duration::hours(1)))),
        seven_day: Some(Limit::new(90, Some(n + Duration::days(4)))),
        updated: n,
    };
    let at90 = KilnoSettings {
        reserve_at: 90,
        ..all()
    };
    assert_eq!(
        labels(&ReserveWatch::new().crossed(Some(&both), &at90, n)),
        "5h,7d",
        "both can cross at once"
    );
    assert_eq!(
        ReserveWatch::new().crossed(Some(&both), &all(), n).len(),
        0,
        "reserve off warns nothing"
    );
    assert_eq!(
        ReserveWatch::new()
            .crossed(Some(&near), &at80, n + Duration::hours(2))
            .len(),
        0,
        "stale limits warn nothing"
    );
}

#[test]
fn near_limit_context_orange() {
    let s = SessionStatus {
        context_used: 810_000,
        context_percent: 81,
        ..status()
    };
    assert_eq!(
        header(Some(&s), Some(&git()), Some(&limits()), &all(), now())
            .unwrap()
            .context
            .unwrap()
            .band(),
        Band::Orange
    );
}

#[test]
fn nothing_to_show() {
    let (st, g, l) = (status(), git(), limits());
    assert_eq!(
        header(None, Some(&g), Some(&l), &all(), now()),
        None,
        "no status yet is no strip"
    );
    assert_eq!(
        header(
            Some(&st),
            Some(&g),
            Some(&l),
            &KilnoSettings {
                status_header: false,
                ..all()
            },
            now()
        ),
        None,
        "header off is no strip"
    );
    let none = KilnoSettings {
        status_model: false,
        status_branch: false,
        status_context: false,
        status_effort: false,
        ..all()
    };
    assert_eq!(
        header(Some(&st), Some(&g), Some(&l), &none, now()),
        None,
        "every field off is no strip"
    );

    let some = KilnoSettings {
        status_model: false,
        status_effort: false,
        ..all()
    };
    let s = header(Some(&st), None, Some(&l), &some, now()).unwrap();
    assert_eq!(
        (s.model, s.effort),
        (None, None),
        "an unticked field is left out"
    );
    assert_eq!(s.git, None, "not a repo is no branch");
    assert_eq!(
        s.context.map(|c| c.percent),
        Some(34),
        "the rest still shows"
    );

    let no_model = SessionStatus {
        model: None,
        ..status()
    };
    assert_eq!(
        header(Some(&no_model), Some(&g), Some(&l), &all(), now())
            .unwrap()
            .model,
        None,
        "a session without a model name shows none"
    );
}
