//! What a session last reported: model, context, effort and the account's limits.

use chrono::{DateTime, Utc};

/// One rate limit: percent used and when it resets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Limit {
    pub percent: i32,
    pub resets_at: Option<DateTime<Utc>>,
}

impl Limit {
    pub fn new(percent: i32, resets_at: Option<DateTime<Utc>>) -> Self {
        Self { percent, resets_at }
    }
}

/// The account's 5-hour and 7-day limits, and when they were last reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountLimits {
    pub five_hour: Option<Limit>,
    pub seven_day: Option<Limit>,
    pub updated: DateTime<Utc>,
}

/// What one session last told its status line (Claude) or its transcript (Codex).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionStatus {
    pub session_id: String,
    pub model: Option<String>,
    pub cwd: Option<String>,
    pub context_used: i64,
    pub context_size: i64,
    pub context_percent: i32,
    pub effort: String,
    pub five_hour: Option<Limit>,
    pub seven_day: Option<Limit>,
    pub updated: DateTime<Utc>,
}
