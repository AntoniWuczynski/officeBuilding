//! Times for the watch: stamps, waits and the daily window.

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use std::time::Duration;

/// How far back `reviews_per_day` looks.
const DAY: Duration = Duration::from_hours(24);

pub(super) fn timestamp(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub(super) fn delta(wait: Duration) -> TimeDelta {
    TimeDelta::from_std(wait).unwrap_or(TimeDelta::MAX)
}

pub(super) fn later(now: DateTime<Utc>, wait: Duration) -> DateTime<Utc> {
    now.checked_add_signed(delta(wait))
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// How long ago `then` was, or zero for a time in the future.
pub(super) fn since(now: DateTime<Utc>, then: DateTime<Utc>) -> Duration {
    (now - then).to_std().unwrap_or(Duration::ZERO)
}

/// Whether a review stamped `at` falls in the 24 hours before `now`. A stamp that does not
/// parse counts, so the daily cap errs towards fewer reviews.
pub(super) fn within_day(at: &str, now: DateTime<Utc>) -> bool {
    DateTime::parse_from_rfc3339(at).map_or(true, |at| since(now, at.with_timezone(&Utc)) < DAY)
}
