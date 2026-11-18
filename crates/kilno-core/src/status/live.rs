//! Live usage (Settings › Status bar, off unless the user turns it on): the account's limits
//! asked of Anthropic's usage endpoint, the one Claude Code's `/usage` asks. The status line's
//! numbers lag behind it (they come with each reply and can be a point lower), and Claude Code
//! saves its own reply only now and then.
//!
//! Only the sign-in's access token is read from Claude Code's `.credentials.json`: the refresh
//! token and everything else are skipped while parsing. The token is sent to [`URL`] alone,
//! never written, never logged, and dropped after the request. An expired one is not sent:
//! Claude Code renews it, and the next try picks the new one up. The request itself is the
//! app's (it has the HTTP client); this module is the pure part.

use super::claude_usage;
use super::model::AccountLimits;
use crate::paths::Env;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Map, Value};
use std::path::PathBuf;
use std::time::Duration;

/// The usage endpoint. Fixed: the token goes nowhere else.
pub const URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// The beta header the endpoint needs for an OAuth token.
pub const BETA: &str = "oauth-2025-04-20";
/// After a Claude reply, ask again no sooner than this.
pub const AFTER_REPLY: Duration = Duration::from_secs(60);
/// A click on the limits asks again no sooner than this.
pub const AFTER_CLICK: Duration = Duration::from_secs(10);
/// A live reply this recent wins over the status line's numbers; an older one (requests
/// failing since) is just one more report, the newest of each limit winning.
pub const FRESH: Duration = Duration::from_secs(10 * 60);
/// A failed request (offline, signed out, rate-limited) waits this long before the next.
pub const AFTER_FAILURE: Duration = Duration::from_secs(5 * 60);

/// `%CLAUDE_CONFIG_DIR%\.credentials.json`, else `%USERPROFILE%\.claude\.credentials.json`.
pub fn credentials_path(env: &dyn Env) -> PathBuf {
    match env.var("CLAUDE_CONFIG_DIR") {
        Some(d) => PathBuf::from(d).join(".credentials.json"),
        None => crate::paths::home(env)
            .join(".claude")
            .join(".credentials.json"),
    }
}

/// An access token. Its `Debug` never shows it.
pub struct Token(String);

impl Token {
    /// The request's headers, CRLF-separated, for this token.
    pub fn headers(&self) -> String {
        format!("Authorization: Bearer {}\r\nanthropic-beta: {BETA}\r\nAccept: application/json", self.0)
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Token(..)")
    }
}

#[derive(Deserialize)]
struct Credentials {
    #[serde(rename = "claudeAiOauth")]
    oauth: Option<OAuth>,
}

/// The two fields read; the refresh token and the rest are skipped unread.
#[derive(Deserialize)]
struct OAuth {
    #[serde(rename = "accessToken")]
    access: Option<String>,
    #[serde(rename = "expiresAt")]
    expires_ms: Option<i64>,
}

/// The access token in `.credentials.json`, if signed in and not expired at `now`. A token
/// with no expiry is taken; one that is not printable ASCII is not (it goes in a header).
pub fn read_token(json: &str, now: DateTime<Utc>) -> Option<Token> {
    let c: Credentials = serde_json::from_str(json.trim_start_matches('\u{feff}')).ok()?;
    let o = c.oauth?;
    let access = o.access.filter(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_graphic()))?;
    if let Some(ms) = o.expires_ms {
        if DateTime::<Utc>::from_timestamp_millis(ms)? <= now {
            return None;
        }
    }
    Some(Token(access))
}

/// The endpoint's reply, as of `fetched`. None for a body that is not the usage object.
pub fn parse_reply(body: &str, fetched: DateTime<Utc>) -> Option<AccountLimits> {
    let body: Map<String, Value> = serde_json::from_str(body).ok()?;
    claude_usage::from_body(&body, fetched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::MapEnv;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 17, 14, 0, 0).unwrap()
    }

    fn creds(token: &str, expires: i64) -> String {
        format!(
            r#"{{"claudeAiOauth":{{"accessToken":"{token}","refreshToken":"rt-secret","expiresAt":{expires},
              "scopes":["user:inference"],"subscriptionType":"max"}}}}"#
        )
    }

    #[test]
    fn path_follows_claude_config_dir() {
        let e = MapEnv::default().with("USERPROFILE", r"C:\Users\x");
        assert_eq!(credentials_path(&e), PathBuf::from(r"C:\Users\x\.claude\.credentials.json"));
        let e = e.with("CLAUDE_CONFIG_DIR", r"D:\cc");
        assert_eq!(credentials_path(&e), PathBuf::from(r"D:\cc\.credentials.json"));
    }

    #[test]
    fn token_only_while_valid() {
        let later = (now() + chrono::Duration::hours(1)).timestamp_millis();
        let t = read_token(&creds("sk-ant-oat01-abc", later), now()).expect("a valid token");
        assert!(t.headers().starts_with("Authorization: Bearer sk-ant-oat01-abc\r\n"));
        assert!(t.headers().contains("anthropic-beta: oauth-2025-04-20"));
        assert_eq!(format!("{t:?}"), "Token(..)", "never printed");

        let past = (now() - chrono::Duration::minutes(1)).timestamp_millis();
        assert!(read_token(&creds("sk-ant-oat01-abc", past), now()).is_none(), "expired");
        assert!(read_token(&creds("", later), now()).is_none(), "empty");
        assert!(read_token(&creds("a\\r\\nX-Evil: 1", later), now()).is_none(), "no header injection");
        assert!(read_token("{}", now()).is_none(), "signed out");
        assert!(read_token("not json", now()).is_none());
    }

    #[test]
    fn reply() {
        let body = r#"{"five_hour":{"utilization":25.0,"resets_at":"2026-11-17T15:00:00.46+00:00"},
            "seven_day":{"utilization":86.0,"resets_at":"2026-11-18T08:00:00.46+00:00"},
            "limits":[{"kind":"weekly_scoped","percent":3,"resets_at":null,
              "scope":{"model":{"display_name":"Fable"}}}]}"#;
        let l = parse_reply(body, now()).unwrap();
        assert_eq!(l.five_hour.unwrap().percent, 25);
        assert_eq!(l.seven_day.unwrap().percent, 86);
        assert_eq!((l.models[0].model.as_str(), l.models[0].limit.percent), ("Fable", 3));
        assert_eq!(l.updated, now());
        assert!(parse_reply(r#"{"error":{"type":"authentication_error"}}"#, now()).is_none());
        assert!(parse_reply("<html>", now()).is_none());
    }
}
