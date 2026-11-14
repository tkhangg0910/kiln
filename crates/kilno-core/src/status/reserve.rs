//! Which limits have just gone past the reserve, so the island says so once per window rather
//! than on every status refresh. A limit's window is told apart by its reset time.

use super::model::AccountLimits;
use super::strip::{limit_meter, model_meter, model_reserve, past_reserve, reserve_for, Meter};
use crate::settings::KilnoSettings;
use chrono::{DateTime, Utc};
use std::collections::HashSet;

#[derive(Debug, Default)]
pub struct ReserveWatch {
    /// (the meter's label, the window's reset): "5h", "7d", or a model's name.
    warned: HashSet<(String, Option<DateTime<Utc>>)>,
}

impl ReserveWatch {
    pub fn new() -> Self {
        Self::default()
    }

    /// The limits that crossed the reserve since the last call, 5h, then 7d, then each model's
    /// weekly one. Each (limit, window) is reported once, ever.
    pub fn crossed(
        &mut self,
        limits: Option<&AccountLimits>,
        settings: &KilnoSettings,
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
            if past_reserve(limit, reserve_at, now) {
                let m = limit_meter(five_hour, limit, now, reserve_at);
                if self.warned.insert((m.label.clone(), limit.resets_at)) {
                    crossed.push(m);
                }
            }
        }
        let reserve_at = model_reserve(settings);
        for model in limits.map(|l| l.models.as_slice()).unwrap_or_default() {
            if past_reserve(&model.limit, reserve_at, now) {
                let m = model_meter(model, now, reserve_at);
                if self.warned.insert((m.label.clone(), model.limit.resets_at)) {
                    crossed.push(m);
                }
            }
        }
        crossed
    }
}
