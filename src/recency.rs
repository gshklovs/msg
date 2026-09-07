//! Human labels for "when did we last talk".
//!
//! Three shapes of the same fact, because the five picker themes want
//! different amounts of it:
//!
//! | | now | 5 min | 1 h | 3 days | this year | older |
//! |---|---|---|---|---|---|---|
//! | [`short`] | `now` | `5m` | `1h` | `Tue` | `Aug 30` | `2024` |
//! | [`long`] | `Texted just now` | `Texted 5 minutes ago` | `Texted an hour ago` | `Tuesday` | `August 30` | `August 30, 2024` |
//! | [`trailing`] | `just now` | `5 minutes ago` | `an hour ago` | `Tuesday` | `August 30` | `August 30, 2024` |

use chrono::{DateTime, Datelike, Local, TimeZone};

const MINUTE: i64 = 60;
const HOUR: i64 = 60 * MINUTE;
const DAY: i64 = 24 * HOUR;
const WEEK: i64 = 7 * DAY;

/// Seconds between `then` and `now`, never negative. A zero timestamp means
/// "no message ever" and yields `None`.
fn age(ts: i64, now: i64) -> Option<i64> {
    if ts <= 0 {
        return None;
    }
    Some((now - ts).max(0))
}

fn local(ts: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(ts, 0).single()
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

const WEEKDAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// The tight form that fits in a right-hand column: `2m`, `1h`, `Tue`, `Aug 30`, `2024`.
pub fn short(ts: i64, now: i64) -> String {
    let Some(age) = age(ts, now) else {
        return String::new();
    };
    let Some(then) = local(ts) else {
        return String::new();
    };
    if age < 45 {
        "now".into()
    } else if age < HOUR {
        format!("{}m", (age / MINUTE).max(1))
    } else if age < DAY {
        format!("{}h", age / HOUR)
    } else if age < WEEK {
        WEEKDAYS[then.weekday().num_days_from_monday() as usize][..3].into()
    } else if then.year() == local(now).map(|n| n.year()).unwrap_or(then.year()) {
        format!("{} {}", &MONTHS[then.month0() as usize][..3], then.day())
    } else {
        then.year().to_string()
    }
}

/// The sentence form used as a row subtitle: `Texted 5 minutes ago`, `Tuesday`.
pub fn long(ts: i64, now: i64) -> String {
    match trailing(ts, now) {
        s if s.is_empty() => String::new(),
        // A weekday or a date is a label, not something you "texted".
        s if s.ends_with("ago") || s == "just now" => format!("Texted {s}"),
        s => s,
    }
}

/// [`long`] without the `Texted ` prefix, for use after another clause:
/// `Group of 3, 5 minutes ago`.
pub fn trailing(ts: i64, now: i64) -> String {
    let Some(age) = age(ts, now) else {
        return String::new();
    };
    let Some(then) = local(ts) else {
        return String::new();
    };
    if age < 45 {
        "just now".into()
    } else if age < 2 * MINUTE {
        "a minute ago".into()
    } else if age < HOUR {
        format!("{} minutes ago", age / MINUTE)
    } else if age < 2 * HOUR {
        "an hour ago".into()
    } else if age < DAY {
        format!("{} hours ago", age / HOUR)
    } else if age < WEEK {
        WEEKDAYS[then.weekday().num_days_from_monday() as usize].into()
    } else {
        let month = MONTHS[then.month0() as usize];
        let this_year = local(now).map(|n| n.year()) == Some(then.year());
        if this_year {
            format!("{month} {}", then.day())
        } else {
            format!("{month} {}, {}", then.day(), then.year())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A fixed instant so the relative branches are exact; the calendar
    // branches are checked by shape, since they follow the machine's zone.
    const NOW: i64 = 1_788_640_000;

    #[test]
    fn a_missing_timestamp_has_no_label() {
        assert_eq!(short(0, NOW), "");
        assert_eq!(long(0, NOW), "");
        assert_eq!(trailing(0, NOW), "");
    }

    #[test]
    fn minutes_and_hours_read_naturally() {
        assert_eq!(short(NOW - 10, NOW), "now");
        assert_eq!(long(NOW - 10, NOW), "Texted just now");
        assert_eq!(short(NOW - 70, NOW), "1m");
        assert_eq!(long(NOW - 70, NOW), "Texted a minute ago");
        assert_eq!(short(NOW - 2 * 60, NOW), "2m");
        assert_eq!(long(NOW - 2 * 60, NOW), "Texted 2 minutes ago");
        assert_eq!(short(NOW - 3600, NOW), "1h");
        assert_eq!(long(NOW - 3600, NOW), "Texted an hour ago");
        assert_eq!(short(NOW - 5 * 3600, NOW), "5h");
        assert_eq!(long(NOW - 5 * 3600, NOW), "Texted 5 hours ago");
    }

    #[test]
    fn a_future_timestamp_clamps_to_now() {
        assert_eq!(short(NOW + 10_000, NOW), "now");
    }

    #[test]
    fn this_week_is_a_weekday_name() {
        let s = short(NOW - 3 * DAY, NOW);
        let l = long(NOW - 3 * DAY, NOW);
        assert_eq!(s.len(), 3, "{s} should be a three-letter weekday");
        assert!(WEEKDAYS.iter().any(|w| *w == l), "{l} should be a weekday");
        assert!(l.starts_with(&s), "{s} should abbreviate {l}");
    }

    #[test]
    fn older_than_a_week_is_a_date() {
        let s = short(NOW - 30 * DAY, NOW);
        let l = long(NOW - 30 * DAY, NOW);
        let (month, day) = l.split_once(' ').expect("month and day");
        assert!(MONTHS.contains(&month), "{month} should be a month name");
        assert!(day.trim_end_matches(',').parse::<u32>().is_ok() || day.contains(','));
        assert!(s.starts_with(&month[..3]), "{s} should abbreviate {l}");
    }

    #[test]
    fn a_previous_year_shortens_to_the_year_alone() {
        let s = short(NOW - 800 * DAY, NOW);
        assert_eq!(s.len(), 4, "{s} should be a bare year");
        assert!(s.parse::<i32>().is_ok());
        assert!(trailing(NOW - 800 * DAY, NOW).contains(&s));
    }
}
