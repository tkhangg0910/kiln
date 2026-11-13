//! Reading what Claude Code hands its status line, and the user's own script's usage cache.
//! Pure functions; the watching is in [`super::store`].

use super::model::{AccountLimits, Limit, SessionStatus};
use crate::paths::{claude_dir, temp_dir, Env};
use chrono::{DateTime, Utc};
use serde_json::{Map, Value};
use std::path::PathBuf;

/// One status line payload. None for anything that is not a whole JSON object with a GUID
/// `session_id`, so a half-written or foreign file is ignored rather than shown.
/// `fallback_effort` is only called when the payload has no effort of its own.
pub fn parse(
    json: &str,
    updated: DateTime<Utc>,
    fallback_effort: impl FnOnce() -> String,
) -> Option<SessionStatus> {
    let root = object(json)?;
    let id = str_of(Some(&root), "session_id")?;
    dotnet_guid(id)?;

    let window = obj(Some(&root), "context_window");
    let size = num(window, "context_window_size")
        .filter(|s| *s > 0.0)
        .map_or(200_000, |s| s as i64);

    // The same sum the user's statusline.ps1 shows: what the next request sends.
    let usage = obj(window, "current_usage");
    let used = (num(usage, "input_tokens").unwrap_or(0.0)
        + num(usage, "cache_creation_input_tokens").unwrap_or(0.0)
        + num(usage, "cache_read_input_tokens").unwrap_or(0.0)) as i64;

    let limits = obj(Some(&root), "rate_limits");
    let effort = match str_of(obj(Some(&root), "effort"), "level") {
        Some(e) if !e.is_empty() => e.to_string(),
        _ => fallback_effort(),
    };

    Some(SessionStatus {
        session_id: id.to_string(),
        model: str_of(obj(Some(&root), "model"), "display_name").map(str::to_string),
        cwd: str_of(Some(&root), "cwd").map(str::to_string),
        context_used: used,
        context_size: size,
        context_percent: (used.wrapping_mul(100) / size) as i32,
        effort,
        five_hour: read_limit(obj(limits, "five_hour"), "used_percentage"),
        seven_day: read_limit(obj(limits, "seven_day"), "used_percentage"),
        updated,
    })
}

/// Claude Code 2.1 puts the effort in the payload. Older ones did not, and the user's script
/// reads the variable, then settings.json, then assumes medium.
pub fn effort(payload: Option<&str>, env: Option<&str>, settings: Option<&str>) -> String {
    [payload, env, settings]
        .into_iter()
        .flatten()
        .find(|s| !s.trim().is_empty())
        .unwrap_or("medium")
        .to_string()
}

/// The effort fallback the app uses: `CLAUDE_CODE_EFFORT_LEVEL`, then `effortLevel` in
/// `<claude dir>\settings.json` (only read), else medium.
pub fn default_effort(env: &dyn Env) -> String {
    let from_settings = std::fs::read(claude_dir(env).join("settings.json"))
        .ok()
        .and_then(|b| object(&String::from_utf8_lossy(&b)))
        .and_then(|root| str_of(Some(&root), "effortLevel").map(str::to_string));
    effort(
        None,
        env.var("CLAUDE_CODE_EFFORT_LEVEL").as_deref(),
        from_settings.as_deref(),
    )
}

/// Where the user's statusline.ps1 keeps the limits it fetches for itself when Claude's payload
/// has none: `%TEMP%\claude\statusline-usage-cache.json`. Kiln only reads it.
pub fn usage_cache_path(env: &dyn Env) -> PathBuf {
    temp_dir(env)
        .join("claude")
        .join("statusline-usage-cache.json")
}

/// The script's cache: as `rate_limits`, but "utilization" for the percentage. None when it
/// holds no limits.
pub fn parse_usage_cache(json: &str, updated: DateTime<Utc>) -> Option<AccountLimits> {
    let root = serde_json::from_str::<Value>(strip_bom(json)).ok()?;
    let root = root.as_object();
    let five_hour = read_limit(obj(root, "five_hour"), "utilization");
    let seven_day = read_limit(obj(root, "seven_day"), "utilization");
    if five_hour.is_none() && seven_day.is_none() {
        return None;
    }
    Some(AccountLimits {
        five_hour,
        seven_day,
        updated,
    })
}

/// The limits are the account's, not a session's, so the newest report wins. A session that
/// has not called the API yet has none and must not blank the ones we have. On a tie the first
/// one wins (as .NET's MaxBy).
pub fn newest<'a>(statuses: impl IntoIterator<Item = &'a SessionStatus>) -> Option<AccountLimits> {
    let mut best: Option<&SessionStatus> = None;
    for s in statuses
        .into_iter()
        .filter(|s| s.five_hour.is_some() || s.seven_day.is_some())
    {
        if best.is_none_or(|b| s.updated > b.updated) {
            best = Some(s);
        }
    }
    best.map(|s| AccountLimits {
        five_hour: s.five_hour.clone(),
        seven_day: s.seven_day.clone(),
        updated: s.updated,
    })
}

/// Percent (floored) and reset time. The reset is epoch seconds today; the user's script also
/// takes an ISO time (no offset = UTC), so do we.
fn read_limit(el: Option<&Map<String, Value>>, percent_key: &str) -> Option<Limit> {
    let pct = num(el, percent_key)?;
    let resets_at = match el.and_then(|o| o.get("resets_at")) {
        Some(Value::Number(n)) => n
            .as_i64()
            .filter(|s| *s > 0)
            .and_then(crate::time::from_unix),
        Some(Value::String(s)) => crate::time::parse_iso(s),
        _ => None,
    };
    Some(Limit::new(pct.floor() as i32, resets_at))
}

/// .NET's `Guid.TryParse`: the N, D, B and P forms, surrounding whitespace allowed.
pub(crate) fn dotnet_guid(s: &str) -> Option<uuid::Uuid> {
    let t = s.trim();
    let t = match t.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        Some(inner) => format!("{{{inner}}}"),
        None if t.starts_with("urn:") => return None,
        None => t.to_string(),
    };
    uuid::Uuid::parse_str(&t).ok()
}

fn strip_bom(s: &str) -> &str {
    s.trim_start_matches('\u{feff}')
}

fn object(json: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(strip_bom(json)) {
        Ok(Value::Object(m)) => Some(m),
        _ => None,
    }
}

fn obj<'a>(el: Option<&'a Map<String, Value>>, name: &str) -> Option<&'a Map<String, Value>> {
    el?.get(name)?.as_object()
}

fn str_of<'a>(el: Option<&'a Map<String, Value>>, name: &str) -> Option<&'a str> {
    el?.get(name)?.as_str()
}

fn num(el: Option<&Map<String, Value>>, name: &str) -> Option<f64> {
    el?.get(name)?.as_f64()
}
