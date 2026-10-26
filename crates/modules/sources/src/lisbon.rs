//! When the nightly search runs: 21:00 in Lisbon (Europe/Lisbon: UTC in
//! winter, UTC+1 in summer, switching like the rest of the EU on the last
//! Sundays of March and October at 01:00 UTC). No time zone database
//! needed for one zone with one rule.

use chrono::{DateTime, Datelike as _, NaiveDate, TimeDelta, Utc, Weekday};

/// The hour, Lisbon time.
pub const HOUR: u32 = 21;

/// The last Sunday of `month` in `year`.
fn last_sunday(year: i32, month: u32) -> Option<NaiveDate> {
    let next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1)?
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1)?
    };
    let last = next.pred_opt()?;
    let back = last.weekday().num_days_from_sunday();
    last.checked_sub_signed(TimeDelta::days(i64::from(back)))
        .filter(|d| d.weekday() == Weekday::Sun)
}

/// Lisbon's offset from UTC at `at`: one hour in summer time.
pub fn offset(at: DateTime<Utc>) -> TimeDelta {
    let year = at.year();
    let switch = |month| {
        last_sunday(year, month)
            .and_then(|d| d.and_hms_opt(1, 0, 0))
            .map(|t| t.and_utc())
    };
    match (switch(3), switch(10)) {
        (Some(start), Some(end)) if at >= start && at < end => TimeDelta::hours(1),
        _ => TimeDelta::zero(),
    }
}

/// The next 21:00 in Lisbon after `now`.
pub fn next_run(now: DateTime<Utc>) -> DateTime<Utc> {
    let mut day = now.date_naive();
    for _ in 0..3 {
        if let Some(local) = day.and_hms_opt(HOUR, 0, 0) {
            // 21:00 Lisbon is 21:00 UTC minus the offset that evening.
            let guess = local.and_utc();
            let at = guess - offset(guess - TimeDelta::hours(1));
            if at > now {
                return at;
            }
        }
        day = day.succ_opt().unwrap_or(day);
    }
    now + TimeDelta::days(1)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    #[test]
    fn summer_and_winter_evenings() {
        // 26 Oct 2026 is a Monday after the switch (25 Oct): winter, UTC.
        let monday = Utc.with_ymd_and_hms(2026, 10, 26, 12, 0, 0).unwrap();
        assert_eq!(
            next_run(monday),
            Utc.with_ymd_and_hms(2026, 10, 26, 21, 0, 0).unwrap()
        );
        // In July, 21:00 Lisbon is 20:00 UTC.
        let july = Utc.with_ymd_and_hms(2026, 7, 1, 20, 30, 0).unwrap();
        assert_eq!(
            next_run(july),
            Utc.with_ymd_and_hms(2026, 7, 2, 20, 0, 0).unwrap()
        );
        assert_eq!(offset(july), TimeDelta::hours(1));
        assert_eq!(offset(monday), TimeDelta::zero());
    }

    #[test]
    fn the_switch_days_are_the_last_sundays() {
        assert_eq!(last_sunday(2026, 3), NaiveDate::from_ymd_opt(2026, 3, 29));
        assert_eq!(last_sunday(2026, 10), NaiveDate::from_ymd_opt(2026, 10, 25));
        assert_eq!(last_sunday(2027, 12), NaiveDate::from_ymd_opt(2027, 12, 26));
    }
}
