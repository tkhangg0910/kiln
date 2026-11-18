//! Turns what a session's status line was handed and its folder's git state into the pane
//! header strip's and the sidebar footer's text. Which theme draws it is the view's business.

use super::model::{AccountLimits, Limit, ModelLimit, SessionStatus};
use crate::git::GitStatus;
use crate::settings::KilnoSettings;
use chrono::{DateTime, Utc};
use regex::Regex;
use std::sync::OnceLock;

/// How full a meter is: green under 50, yellow to 69, orange to 89, red from 90.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Band {
    Green,
    Yellow,
    Orange,
    Red,
}

/// A percentage in the strip or the footer: its label, the numbers beside it (if any) and its
/// tooltip. `stale` is a limit past its reset that Claude has not reported on since; `reserve`
/// is one at or past the threshold you set, which reads red whatever its band. `reset` is a
/// limit's time to its reset alone ("1h20m"), for the themes that word it their own way.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Meter {
    pub label: String,
    pub detail: Option<String>,
    pub percent: i32,
    pub tip: String,
    pub stale: bool,
    pub reserve: bool,
    pub reset: Option<String>,
}

impl Meter {
    pub fn band(&self) -> Band {
        if self.reserve {
            Band::Red
        } else {
            band_for(self.percent)
        }
    }
}

/// What the pane header strip shows for one session, in order. A None part is not shown.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Strip {
    pub model: Option<String>,
    pub git: Option<GitStatus>,
    pub context: Option<Meter>,
    pub effort: Option<String>,
    pub five_hour: Option<Meter>,
    pub seven_day: Option<Meter>,
    /// The per-model weekly limits (Fable), after the 7-day.
    pub models: Vec<Meter>,
}

/// The header strip, or None when there is nothing to show: no status yet, the header switched
/// off, or every field unticked. An empty strip, never zeros.
pub fn header(
    status: Option<&SessionStatus>,
    git: Option<&GitStatus>,
    limits: Option<&AccountLimits>,
    settings: &KilnoSettings,
    now: DateTime<Utc>,
) -> Option<Strip> {
    let status = status.filter(|_| settings.status_header)?;
    // With the footer off the account's limits ride in the header instead.
    let limits_here = !settings.status_footer;
    let limit = |five_hour: bool, on: bool, l: Option<&Limit>| {
        l.filter(|_| limits_here && on)
            .map(|l| limit_meter(five_hour, l, now, reserve_for(settings, five_hour)))
    };
    let strip = Strip {
        model: status
            .model
            .as_deref()
            .filter(|_| settings.status_model)
            .map(model_name),
        git: git.filter(|_| settings.status_branch).cloned(),
        // A window of unknown size (OpenCode does not store it) shows no meter.
        context: (settings.status_context && status.context_size > 0).then(|| context_meter(status)),
        // Codex leaves effort out when it runs on its default, and then so does the strip.
        effort: Some(status.effort.clone()).filter(|e| settings.status_effort && !e.is_empty()),
        five_hour: limit(
            true,
            settings.status_five_hour,
            limits.and_then(|l| l.five_hour.as_ref()),
        ),
        seven_day: limit(
            false,
            settings.status_seven_day,
            limits.and_then(|l| l.seven_day.as_ref()),
        ),
        models: if limits_here { model_meters(limits, settings, now) } else { Vec::new() },
    };
    (strip != Strip::default()).then_some(strip)
}

fn context_meter(s: &SessionStatus) -> Meter {
    let (used, size) = (tokens(s.context_used), tokens(s.context_size));
    Meter {
        label: "ctx".into(),
        detail: Some(format!("{used}/{size}")),
        percent: s.context_percent,
        tip: format!(
            "Context: {used} of {size} tokens used ({}%)",
            s.context_percent
        ),
        stale: false,
        reserve: false,
        reset: None,
    }
}

/// The sidebar footer's limits, 5h then 7d: none while it is off or nothing has reported them.
pub fn footer(
    limits: Option<&AccountLimits>,
    settings: &KilnoSettings,
    now: DateTime<Utc>,
) -> Vec<Meter> {
    let mut meters = Vec::new();
    let Some(limits) = limits.filter(|_| settings.status_footer) else {
        return meters;
    };
    if let Some(h5) = limits
        .five_hour
        .as_ref()
        .filter(|_| settings.status_five_hour)
    {
        meters.push(limit_meter(true, h5, now, reserve_for(settings, true)));
    }
    if let Some(d7) = limits
        .seven_day
        .as_ref()
        .filter(|_| settings.status_seven_day)
    {
        meters.push(limit_meter(false, d7, now, reserve_for(settings, false)));
    }
    meters.extend(model_meters(Some(limits), settings, now));
    meters
}

/// One limit's meter. A `reserve_at` of 0 is no reserve on this limit.
pub fn limit_meter(five_hour: bool, limit: &Limit, now: DateTime<Utc>, reserve_at: i32) -> Meter {
    let (name, label) = if five_hour {
        ("5-hour", "5h")
    } else {
        ("7-day", "7d")
    };
    meter(name, label, limit, now, reserve_at)
}

/// One model's weekly limit: labelled with the model's name ("Fable 32%"), told apart from
/// the account's 7-day in the tooltip.
pub fn model_meter(m: &ModelLimit, now: DateTime<Utc>, reserve_at: i32) -> Meter {
    meter(&format!("{} weekly", m.model), &m.model, &m.limit, now, reserve_at)
}

/// The per-model weekly meters the settings ask for, in the server's order.
fn model_meters(limits: Option<&AccountLimits>, settings: &KilnoSettings, now: DateTime<Utc>) -> Vec<Meter> {
    let Some(limits) = limits.filter(|_| settings.status_model_weekly) else {
        return Vec::new();
    };
    let reserve_at = model_reserve(settings);
    limits.models.iter().map(|m| model_meter(m, now, reserve_at)).collect()
}

/// The reserve for the per-model weekly limits: 0 when they are not watched.
pub fn model_reserve(s: &KilnoSettings) -> i32 {
    if s.reserve_model_weekly {
        s.reserve_at
    } else {
        0
    }
}

fn meter(name: &str, label: &str, limit: &Limit, now: DateTime<Utc>, reserve_at: i32) -> Meter {
    // Limits only change when Claude refreshes its status line, so past the reset the number is
    // the old window's: the new one starts empty, until the next report says otherwise.
    if is_stale(limit, now) {
        return Meter {
            label: label.into(),
            detail: None,
            percent: 0,
            tip: format!(
                "{name} limit: reset since the last report (which said {}%); a new window starts at 0%",
                limit.percent
            ),
            stale: true,
            reserve: false,
            reset: None,
        };
    }
    let reset = limit.resets_at.map(|at| reset_in(at, now));
    let past = past_reserve(limit, reserve_at, now);
    let mut tip = format!("{name} limit: {}% used", limit.percent);
    if let Some(r) = &reset {
        tip.push_str(&format!(", resets in {r}"));
    }
    if past {
        tip.push_str(&format!(". Past your {reserve_at}% reserve"));
    }
    Meter {
        label: label.into(),
        detail: reset.as_ref().map(|r| format!("\u{21BB} {r}")),
        percent: limit.percent,
        tip,
        stale: false,
        reserve: past,
        reset,
    }
}

/// The threshold watching this limit, or 0 when the reserve is off or not on it.
pub fn reserve_for(s: &KilnoSettings, five_hour: bool) -> i32 {
    let on = if five_hour {
        s.reserve_five_hour
    } else {
        s.reserve_seven_day
    };
    if on {
        s.reserve_at
    } else {
        0
    }
}

/// A stale limit is the old window's, so it is past nothing.
pub fn past_reserve(limit: &Limit, reserve_at: i32, now: DateTime<Utc>) -> bool {
    reserve_at > 0 && limit.percent >= reserve_at && !is_stale(limit, now)
}

/// Past its reset time; a limit with no reset time is never stale.
fn is_stale(limit: &Limit, now: DateTime<Utc>) -> bool {
    limit.resets_at.is_some_and(|at| at <= now)
}

pub fn band_for(percent: i32) -> Band {
    match percent {
        p if p >= 90 => Band::Red,
        p if p >= 70 => Band::Orange,
        p if p >= 50 => Band::Yellow,
        _ => Band::Green,
    }
}

/// Effort as a step of five, low to max, for the themes that draw it as rising bars. -1 for
/// one not on the scale.
pub fn effort_step(effort: &str) -> i32 {
    match effort {
        "low" => 0,
        "medium" | "med" => 1,
        "high" => 2,
        "xhigh" => 3,
        "max" => 4,
        _ => -1,
    }
}

/// As the user's script writes token counts (340k, 1M, 1.5M), with the design's capital M.
/// Midpoints round as .NET does: to even for the millions (Math.Round), away from zero for the
/// thousands ("F0").
pub fn tokens(n: i64) -> String {
    if n >= 1_000_000 {
        let m = (n as f64 / 1_000_000.0 * 10.0).round_ties_even() / 10.0;
        return if (m - m.round()).abs() < 0.05 {
            format!("{}M", m.round() as i64)
        } else {
            format!("{m:.1}M")
        };
    }
    if n >= 1000 {
        format!("{}k", (n as f64 / 1000.0).round() as i64)
    } else {
        n.to_string()
    }
}

/// "Opus 5.5 (1M context)" reads "Opus 5.5 1M", as in the user's script.
pub fn model_name(display_name: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re =
        RE.get_or_init(|| Regex::new(r"\s*\((\d+\.?\d*[kKmM])\s+context\)").expect("valid regex"));
    re.replace_all(display_name, " ${1}").trim().to_string()
}

/// Time to a reset in the user's script's format: 4d10h, 1h20m, 20m, or now.
pub fn reset_in(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let left = at - now;
    if left <= chrono::Duration::zero() {
        return "now".into();
    }
    let (days, hours, minutes) = (
        left.num_days(),
        left.num_hours() % 24,
        left.num_minutes() % 60,
    );
    if days > 0 {
        format!("{days}d{hours}h")
    } else if hours > 0 {
        format!("{hours}h{minutes:02}m")
    } else {
        format!("{minutes}m")
    }
}
