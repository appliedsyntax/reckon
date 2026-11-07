//! Calendar arithmetic (proleptic Gregorian, no dependencies).

use std::time::{SystemTime, UNIX_EPOCH};

pub const MONTHS: [&str; 12] = [
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
pub const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

pub fn unix_now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Days since 1970-01-01 for a civil date (Howard Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = m as i64;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Civil date for a day count since 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// 0 = Sunday.
pub fn weekday(days: i64) -> usize {
    (days + 4).rem_euclid(7) as usize
}

pub fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: u32) -> u32 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

pub fn month_from_name(word: &str) -> Option<u32> {
    let w = word.to_lowercase();
    if w.len() < 3 {
        return None;
    }
    if w == "sept" {
        return Some(9);
    }
    MONTHS
        .iter()
        .position(|m| {
            let m = m.to_lowercase();
            m == w || (w.len() == 3 && m.starts_with(&w))
        })
        .map(|i| i as u32 + 1)
}

/// Splits local seconds into (days, seconds-of-day).
pub fn split(local: f64) -> (i64, f64) {
    let days = (local / 86400.0).floor() as i64;
    (days, local - days as f64 * 86400.0)
}

/// Adds whole calendar months to a local timestamp, clamping the day
/// (Jan 31 + 1 month = Feb 28/29).
pub fn add_months(local: f64, months: i64) -> f64 {
    let (days, sod) = split(local);
    let (y, m, d) = civil_from_days(days);
    let total = y * 12 + (m as i64 - 1) + months;
    let ny = total.div_euclid(12);
    let nm = (total.rem_euclid(12) + 1) as u32;
    let nd = d.min(days_in_month(ny, nm));
    days_from_civil(ny, nm, nd) as f64 * 86400.0 + sod
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        for z in [-800_000i64, -1, 0, 1, 19_000, 20_399, 2_000_000] {
            let (y, m, d) = civil_from_days(z);
            assert_eq!(days_from_civil(y, m, d), z);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2000, 3, 1), 11017);
        assert_eq!(weekday(days_from_civil(2026, 11, 7)), 6);
    }
    #[test]
    fn months() {
        let jan31 = days_from_civil(2024, 1, 31) as f64 * 86400.0;
        let (y, m, d) = civil_from_days(split(add_months(jan31, 1)).0);
        assert_eq!((y, m, d), (2024, 2, 29));
        assert_eq!(month_from_name("Sep"), Some(9));
        assert_eq!(month_from_name("march"), Some(3));
        assert_eq!(month_from_name("ma"), None);
    }
}
