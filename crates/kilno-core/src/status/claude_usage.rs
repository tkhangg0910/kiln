//! The usage reply Claude Code saves in its own `.claude.json` (`cachedUsageUtilization`): the
//! same 5-hour and 7-day limits as the status line, plus the per-model weekly ones (Fable on
//! Team premium seats and Max plans) that Claude Code never hands its status line. Kilno only
//! reads this file, and from it only the two keys below: the rest (the account's email and
//! name, MCP server settings with their keys, every project's path) is skipped while parsing,
//! never kept, never logged. No network, no token.
//!
//! The key is Claude Code's own and undocumented: if it moves or changes shape this finds
//! nothing, and the limits come from the status line alone, as before.

use super::model::{AccountLimits, Limit, ModelLimit};
use super::parse::read_limit;
use crate::paths::Env;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::PathBuf;

/// A model's label is the server's; past this it is cut, so a strange one cannot crowd the strip.
pub const MAX_MODEL_NAME: usize = 24;

/// `%CLAUDE_CONFIG_DIR%\.claude.json`, else `%USERPROFILE%\.claude.json` (next to the `.claude`
/// folder, not inside it), where Claude Code keeps its global state.
pub fn path(env: &dyn Env) -> PathBuf {
    match env.var("CLAUDE_CONFIG_DIR") {
        Some(d) => PathBuf::from(d).join(".claude.json"),
        None => crate::paths::home(env).join(".claude.json"),
    }
}

/// The two parts of `.claude.json` read. Every other field is skipped unread.
#[derive(Deserialize)]
struct File {
    #[serde(rename = "oauthAccount")]
    account: Option<Account>,
    #[serde(rename = "cachedUsageUtilization")]
    usage: Option<Cached>,
}

#[derive(Deserialize)]
struct Account {
    #[serde(rename = "accountUuid")]
    uuid: Option<String>,
}

#[derive(Deserialize)]
struct Cached {
    #[serde(rename = "fetchedAtMs")]
    fetched_ms: Option<i64>,
    #[serde(rename = "accountUuid")]
    uuid: Option<String>,
    /// The usage endpoint's body, as Claude Code got it. Loose: one odd field must not lose
    /// the rest.
    utilization: Option<Map<String, Value>>,
}

/// The account's limits from Claude Code's saved usage reply. None when there is none, it is
/// another account's (Claude Code signed in as someone else since), or it says nothing.
pub fn parse(json: &str) -> Option<AccountLimits> {
    read(json).ok().flatten()
}

/// As [`parse`], but a file that is not whole JSON (caught mid-write) is an error, told apart
/// from one that is whole and has nothing to show.
pub fn read(json: &str) -> Result<Option<AccountLimits>, serde_json::Error> {
    let file: File = serde_json::from_str(json.trim_start_matches('\u{feff}'))?;
    Ok(limits(file))
}

fn limits(file: File) -> Option<AccountLimits> {
    let usage = file.usage?;
    // Signed out, or a reply saved for an account that is no longer the signed-in one.
    let signed_in = file.account?.uuid?;
    if usage.uuid.as_deref().is_some_and(|u| !u.eq_ignore_ascii_case(&signed_in)) {
        return None;
    }
    let updated = DateTime::<Utc>::from_timestamp_millis(usage.fetched_ms?)?;
    from_body(&usage.utilization?, updated)
}

/// The usage endpoint's body (saved by Claude Code, or fetched live by [`super::live`]) as of
/// `updated`. None when it says nothing.
pub(crate) fn from_body(body: &Map<String, Value>, updated: DateTime<Utc>) -> Option<AccountLimits> {
    let window = |name: &str| read_limit(body.get(name).and_then(Value::as_object), "utilization").map(clamp);
    let limits = AccountLimits {
        five_hour: window("five_hour"),
        seven_day: window("seven_day"),
        models: models(body.get("limits")),
        updated,
    };
    (limits.five_hour.is_some() || limits.seven_day.is_some() || !limits.models.is_empty()).then_some(limits)
}

/// The `limits[]` rows that are one model's weekly window, in the server's order.
fn models(rows: Option<&Value>) -> Vec<ModelLimit> {
    let Some(rows) = rows.and_then(Value::as_array) else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(Value::as_object)
        .filter(|r| r.get("kind").and_then(Value::as_str) == Some("weekly_scoped"))
        .filter_map(|r| {
            let name = r.get("scope")?.get("model")?.get("display_name")?.as_str()?;
            let name: String = name.trim().chars().filter(|c| !c.is_control()).take(MAX_MODEL_NAME).collect();
            if name.is_empty() {
                return None;
            }
            let limit = read_limit(Some(r), "percent").map(clamp)?;
            Some(ModelLimit { model: name, limit })
        })
        .collect()
}

/// A file anyone could edit is only ever drawn: keep the percentage on the meter's scale.
fn clamp(l: Limit) -> Limit {
    Limit::new(l.percent.clamp(0, 100), l.resets_at)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::MapEnv;

    const ACCOUNT: &str = "bd09f61c-7057-4e86-991c-45af028bf6bb";

    fn file(account: &str, cached_for: &str, limits: &str) -> String {
        format!(
            r#"{{"numStartups":3,
              "oauthAccount":{{"accountUuid":"{account}","emailAddress":"someone@example.com","fullName":"Some One"}},
              "mcpServers":{{"x":{{"env":{{"API_KEY":"secret"}}}}}},
              "cachedUsageUtilization":{{"fetchedAtMs":1794671593140,"accountUuid":"{cached_for}",
                "utilization":{{
                  "five_hour":{{"utilization":7,"resets_at":"2026-11-14T20:10:00.075053+00:00"}},
                  "seven_day":{{"utilization":66,"resets_at":"2026-11-18T08:00:00.075074+00:00"}},
                  "seven_day_opus":null,
                  "limits":{limits}}}}}}}"#
        )
    }

    const ROWS: &str = r#"[
        {"kind":"session","group":"session","percent":7,"resets_at":"2026-11-14T20:10:00+00:00","scope":null},
        {"kind":"weekly_all","group":"weekly","percent":66,"resets_at":"2026-11-18T08:00:00+00:00","scope":null},
        {"kind":"weekly_scoped","group":"weekly","percent":32.6,"resets_at":"2026-11-18T08:00:00+00:00",
         "scope":{"model":{"id":null,"display_name":"Fable"},"surface":null}},
        {"kind":"weekly_scoped","group":"weekly","percent":5,"resets_at":null,
         "scope":{"model":null,"surface":{"display_name":"Cowork"}}}]"#;

    #[test]
    fn reads_the_account_limits_and_the_model_rows() {
        let l = parse(&file(ACCOUNT, ACCOUNT, ROWS)).expect("limits");
        assert_eq!(l.five_hour.as_ref().map(|l| l.percent), Some(7));
        assert_eq!(l.seven_day.as_ref().map(|l| l.percent), Some(66));
        // A surface's row is not a model's.
        assert_eq!(l.models.len(), 1);
        assert_eq!(l.models[0].model, "Fable");
        assert_eq!(l.models[0].limit.percent, 32);
        assert_eq!(l.models[0].limit.resets_at, crate::time::parse_iso("2026-11-18T08:00:00Z"));
        assert_eq!(l.updated.timestamp_millis(), 1794671593140);
    }

    #[test]
    fn another_accounts_reply_is_not_shown() {
        assert!(parse(&file(ACCOUNT, "00000000-0000-0000-0000-000000000000", ROWS)).is_none());
        // Case is not a different account.
        assert!(parse(&file(ACCOUNT, &ACCOUNT.to_uppercase(), ROWS)).is_some());
    }

    #[test]
    fn signed_out_shows_nothing() {
        let json = file(ACCOUNT, ACCOUNT, ROWS).replace(r#""oauthAccount""#, r#""oauthAccountGone""#);
        assert!(parse(&json).is_none());
    }

    #[test]
    fn no_rows_still_gives_the_account_limits() {
        for rows in ["null", "[]", r#""odd""#, "[1,2]"] {
            let l = parse(&file(ACCOUNT, ACCOUNT, rows)).expect(rows);
            assert!(l.models.is_empty(), "{rows}");
            assert_eq!(l.seven_day.map(|l| l.percent), Some(66), "{rows}");
        }
    }

    #[test]
    fn odd_values_stay_on_the_scale() {
        let rows = r#"[{"kind":"weekly_scoped","percent":250,"resets_at":null,
            "scope":{"model":{"display_name":"  A very long model name that goes on and on\n"}}},
            {"kind":"weekly_scoped","percent":-4,"resets_at":null,"scope":{"model":{"display_name":"B"}}},
            {"kind":"weekly_scoped","percent":10,"resets_at":null,"scope":{"model":{"display_name":"   "}}}]"#;
        let l = parse(&file(ACCOUNT, ACCOUNT, rows)).unwrap();
        assert_eq!(l.models.len(), 2);
        assert_eq!(l.models[0].limit.percent, 100);
        assert_eq!(l.models[0].model.chars().count(), MAX_MODEL_NAME);
        assert!(!l.models[0].model.contains('\n'));
        assert_eq!(l.models[1].limit.percent, 0);
    }

    #[test]
    fn broken_or_foreign_files_give_nothing() {
        for json in ["", "{", "[]", "{}", r#"{"cachedUsageUtilization":5}"#, "\u{feff}{}"] {
            assert!(parse(json).is_none(), "{json:?}");
        }
        // A file cut off mid-write.
        let full = file(ACCOUNT, ACCOUNT, ROWS);
        assert!(parse(&full[..full.len() / 2]).is_none());
    }

    #[test]
    fn the_file_sits_next_to_the_claude_folder_or_in_the_config_dir() {
        let home = MapEnv::default().with("USERPROFILE", r"C:\Users\me").with("HOME", r"C:\Users\me");
        assert_eq!(path(&home), PathBuf::from(r"C:\Users\me").join(".claude.json"));
        let cfg = home.with("CLAUDE_CONFIG_DIR", r"D:\cc");
        assert_eq!(path(&cfg), PathBuf::from(r"D:\cc").join(".claude.json"));
    }
}
