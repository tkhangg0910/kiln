//! Which limits have just gone past the reserve, so the island says so once per window rather
//! than on every status refresh. A limit's window is told apart by its reset time.

use super::model::AccountLimits;
use super::strip::{limit_meter, past_reserve, reserve_for, Meter};
use crate::settings::KilnSettings;
use chrono::{DateTime, Utc};
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct ReserveWatch {
    warned: HashSet<(bool, Option<DateTime<Utc>>)>,
}

impl ReserveWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// The limits that crossed the reserve since the last call, 5h before 7d. Each (limit,
    /// window) is reported once, ever.
    pub fn crossed(
        &mut self,
        limits: Option<&AccountLimits>,
        settings: &KilnSettings,
        now: DateTime<Utc>,
    ) -> Vec<Meter> {
        let mut crossed = Vec::new();
        let both = [
            (true, limits.and_then(|l| l.five_hour.as_ref())),
            (false, limits.and_then(|l| l.seven_day.as_ref())),
        ];
        for (five_hour, limit) in both {
            let Some(limit) = limit else { continue };
            let reserve_at = reserve_for(settings, five_hour);
            if past_reserve(limit, reserve_at, now)
                && self.warned.insert((five_hour, limit.resets_at))
            {
                crossed.push(limit_meter(five_hour, limit, now, reserve_at));
            }
        }
        crossed
    }
}
