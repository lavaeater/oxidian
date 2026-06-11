//! Local-date helpers computed in Rust so they behave identically on web,
//! desktop, and mobile.
//!
//! These used to come from `oxidian.js` (`today()` / `date_vars()`), but the
//! value-returning JS bridge proved unreliable inside the Android WebView
//! (`date_vars` — which returns stringified JSON — came back empty, breaking
//! "Today's note"). Everything is now derived here from a single `YYYY-MM-DD`
//! base date, with the WebView's plain-string `today()` preferred for correct
//! local time and a native clock as the fallback.

use crate::js;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];
// 0 = Sunday
const DAYS: [&str; 7] = [
    "Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday",
];

/// Today's date as `YYYY-MM-DD` (local time where available).
pub async fn today() -> String {
    let js = js::today().await;
    if parse_ymd(&js).is_some() {
        return js;
    }
    let (y, m, d) = native_today();
    format!("{y:04}-{m:02}-{d:02}")
}

/// Date variables as a JSON string, matching the shape parsed by
/// `TemplateVars::from_json` (`year`, `yearShort`, `month`, `monthName`,
/// `date`, `dayName`, `week`). Never returns empty.
pub async fn date_vars_json() -> String {
    let base = today().await;
    let (y, m, d) = parse_ymd(&base).unwrap_or_else(native_today);
    let weekday = weekday_from_sunday(y, m, d);
    let week = iso_week(y, m, d);
    format!(
        "{{\"year\":\"{y:04}\",\"yearShort\":\"{ys:02}\",\"month\":\"{m:02}\",\
         \"monthName\":\"{mn}\",\"date\":\"{d:02}\",\"dayName\":\"{dn}\",\"week\":\"{week:02}\"}}",
        ys = (y % 100).abs(),
        mn = MONTHS[(m - 1) as usize],
        dn = DAYS[weekday as usize],
    )
}

fn parse_ymd(s: &str) -> Option<(i32, u8, u8)> {
    let b = s.as_bytes();
    if s.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i32 = s.get(0..4)?.parse().ok()?;
    let m: u8 = s.get(5..7)?.parse().ok()?;
    let d: u8 = s.get(8..10)?.parse().ok()?;
    if (1..=12).contains(&m) && (1..=31).contains(&d) { Some((y, m, d)) } else { None }
}

fn is_leap(y: i32) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

fn days_in_month(y: i32, m: u8) -> u8 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap(y) { 29 } else { 28 },
        _ => 30,
    }
}

/// Returns `ymd` (YYYY-MM-DD) advanced by `days`, handling month/year rollover.
/// Falls back to the input string if it can't be parsed.
pub fn add_days(ymd: &str, days: u32) -> String {
    let Some((mut y, mut m, mut d)) = parse_ymd(ymd) else {
        return ymd.to_string();
    };
    for _ in 0..days {
        d += 1;
        if d > days_in_month(y, m) {
            d = 1;
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

/// Day of year, 1-based.
fn ordinal(y: i32, m: u8, d: u8) -> i32 {
    let mut doy = d as i32;
    for mm in 1..m {
        doy += days_in_month(y, mm) as i32;
    }
    doy
}

/// Weekday with 0 = Sunday, via Zeller's congruence (Gregorian).
fn weekday_from_sunday(y: i32, m: u8, d: u8) -> u8 {
    let (yy, mm) = if m < 3 { (y - 1, m as i32 + 12) } else { (y, m as i32) };
    let k = yy.rem_euclid(100);
    let j = yy.div_euclid(100);
    // h: 0 = Saturday … 6 = Friday
    let h = (d as i32 + (13 * (mm + 1)) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7);
    ((h + 6) % 7) as u8
}

fn weeks_in_year(y: i32) -> i32 {
    let p = |y: i32| (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)).rem_euclid(7);
    if p(y) == 4 || p(y - 1) == 3 { 53 } else { 52 }
}

/// ISO-8601 week number (1–53).
fn iso_week(y: i32, m: u8, d: u8) -> i32 {
    let doy = ordinal(y, m, d);
    let wd_sun = weekday_from_sunday(y, m, d) as i32; // 0 = Sunday
    let iso_wd = if wd_sun == 0 { 7 } else { wd_sun };  // 1 = Mon … 7 = Sun
    let week = (doy - iso_wd + 10) / 7;
    if week < 1 {
        weeks_in_year(y - 1)
    } else if week > weeks_in_year(y) {
        1
    } else {
        week
    }
}

#[cfg(target_arch = "wasm32")]
fn native_today() -> (i32, u8, u8) {
    let d = js_sys::Date::new_0();
    (d.get_full_year() as i32, d.get_month() as u8 + 1, d.get_date() as u8)
}

#[cfg(not(target_arch = "wasm32"))]
fn native_today() -> (i32, u8, u8) {
    use time::OffsetDateTime;
    // now_local() can fail on multi-threaded platforms (e.g. Android); fall back
    // to UTC so we always return *a* date rather than erroring.
    let dt = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    (dt.year(), u8::from(dt.month()), dt.day())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_known_dates() {
        // 2026-06-04 is a Thursday; 2000-01-01 a Saturday; 2024-02-29 a Thursday.
        assert_eq!(DAYS[weekday_from_sunday(2026, 6, 4) as usize], "Thursday");
        assert_eq!(DAYS[weekday_from_sunday(2000, 1, 1) as usize], "Saturday");
        assert_eq!(DAYS[weekday_from_sunday(2024, 2, 29) as usize], "Thursday");
    }

    #[test]
    fn iso_week_known_dates() {
        assert_eq!(iso_week(2026, 1, 1), 1);
        assert_eq!(iso_week(2026, 6, 4), 23);
        // 2021-01-01 (Friday) is ISO week 53 of 2020.
        assert_eq!(iso_week(2021, 1, 1), 53);
    }

    #[test]
    fn parse_roundtrip() {
        assert_eq!(parse_ymd("2026-06-04"), Some((2026, 6, 4)));
        assert_eq!(parse_ymd("not-a-date"), None);
        assert_eq!(parse_ymd("2026/06/04"), None);
    }
}
