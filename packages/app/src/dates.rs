//! Local-date helpers computed in Rust so they behave identically on web,
//! desktop, and mobile.
//!
//! These used to come from `oxidian.js` (`today()` / `date_vars()`), but the
//! value-returning JS bridge proved unreliable inside the Android `WebView`
//! (`date_vars` — which returns stringified JSON — came back empty, breaking
//! "Today's note"). Everything is now derived here from a single `YYYY-MM-DD`
//! base date, with the `WebView`'s plain-string `today()` preferred for correct
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

/// Substitutes `{{today}}` / `{{tomorrow}}` in a snippet with real dates.
/// Shared by the slash menu and the task-metadata menu, whose built-in
/// commands carry the same placeholders (see `views::slash` / `views::task_menu`).
pub async fn fill_placeholders(text: &str) -> String {
    if !text.contains("{{today}}") && !text.contains("{{tomorrow}}") {
        return text.to_string();
    }
    let today = today().await;
    let tomorrow = add_days(&today, 1);
    text.replace("{{today}}", &today).replace("{{tomorrow}}", &tomorrow)
}

/// Date variables as a JSON string, matching the shape parsed by
/// `TemplateVars::from_json` (`year`, `yearShort`, `month`, `monthName`,
/// `date`, `dayName`, `week`, `weekYear`). Never returns empty.
pub async fn date_vars_json() -> String {
    let base = today().await;
    date_vars_json_for(&base)
}

/// The same variables, for an arbitrary `YYYY-MM-DD` — what the weekly and
/// monthly logs need, since those are written for a period that is often not
/// the one containing today (you plan next week *this* week).
///
/// An unparseable date falls back to the system clock rather than returning
/// empty, because every caller uses these to build a path.
pub fn date_vars_json_for(ymd: &str) -> String {
    let (y, m, d) = parse_ymd(ymd).unwrap_or_else(native_today);
    let weekday = weekday_from_sunday(y, m, d);
    let (week, week_year) = iso_week_and_year(y, m, d);
    format!(
        "{{\"year\":\"{y:04}\",\"yearShort\":\"{ys:02}\",\"month\":\"{m:02}\",\
         \"monthName\":\"{mn}\",\"date\":\"{d:02}\",\"dayName\":\"{dn}\",\
         \"week\":\"{week:02}\",\"weekYear\":\"{week_year:04}\"}}",
        ys = (y % 100).abs(),
        mn = MONTHS[(m - 1) as usize],
        dn = DAYS[weekday as usize],
    )
}

// ── Periods ──────────────────────────────────────────────────────────────────

/// The three Bullet Journal logging scales. See `docs/bujo-roadmap.md` §4.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Period {
    Day,
    Week,
    Month,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::Day => "Day",
            Period::Week => "Week",
            Period::Month => "Month",
        }
    }

    /// The next scale up, for "zoom out" navigation. A month has nowhere to go.
    pub fn wider(self) -> Option<Self> {
        match self {
            Period::Day => Some(Period::Week),
            Period::Week => Some(Period::Month),
            Period::Month => None,
        }
    }
}

/// A stable, sortable name for the period containing `ymd`:
/// `2026-08-13`, `2026-W33`, `2026-08`.
///
/// The week key uses the **ISO week-year**, not the calendar year: 2021-01-01
/// belongs to week 53 of 2020, and calling it `2021-W53` would file it under a
/// week that does not exist.
pub fn period_key(period: Period, ymd: &str) -> String {
    let Some((y, m, d)) = parse_ymd(ymd) else {
        return ymd.to_string();
    };
    match period {
        Period::Day => format!("{y:04}-{m:02}-{d:02}"),
        Period::Week => {
            let (week, week_year) = iso_week_and_year(y, m, d);
            format!("{week_year:04}-W{week:02}")
        }
        Period::Month => format!("{y:04}-{m:02}"),
    }
}

/// The first date of the period containing `ymd` — Monday for a week, the 1st
/// for a month. Weeks start on Monday because the key is an ISO week.
pub fn period_start(period: Period, ymd: &str) -> String {
    let Some((y, m, d)) = parse_ymd(ymd) else {
        return ymd.to_string();
    };
    match period {
        Period::Day => ymd.to_string(),
        Period::Week => {
            let sun = i32::from(weekday_from_sunday(y, m, d));
            let iso_wd = if sun == 0 { 7 } else { sun }; // 1 = Mon … 7 = Sun
            shift_days(ymd, -(iso_wd - 1))
        }
        Period::Month => format!("{y:04}-{m:02}-01"),
    }
}

/// The last date of the period containing `ymd`.
pub fn period_end(period: Period, ymd: &str) -> String {
    match period {
        Period::Day => ymd.to_string(),
        Period::Week => shift_days(&period_start(Period::Week, ymd), 6),
        Period::Month => {
            let Some((y, m, _)) = parse_ymd(ymd) else {
                return ymd.to_string();
            };
            format!("{y:04}-{m:02}-{:02}", days_in_month(y, m))
        }
    }
}

/// A date inside the period `delta` steps away — the previous or next day,
/// week, or month. Returns the *start* of that period, so repeated stepping
/// can't drift (stepping months from the 31st must not skip February).
pub fn shift_period(period: Period, ymd: &str, delta: i32) -> String {
    let start = period_start(period, ymd);
    match period {
        Period::Day => shift_days(&start, delta),
        Period::Week => shift_days(&start, delta * 7),
        Period::Month => {
            let Some((y, m, _)) = parse_ymd(&start) else {
                return start;
            };
            let total = y * 12 + i32::from(m - 1) + delta;
            let (ny, nm) = (total.div_euclid(12), total.rem_euclid(12) + 1);
            format!("{ny:04}-{nm:02}-01")
        }
    }
}

/// `ymd` moved by `delta` days, forwards or backwards.
pub fn shift_days(ymd: &str, delta: i32) -> String {
    let Some((mut y, mut m, mut d)) = parse_ymd(ymd) else {
        return ymd.to_string();
    };
    for _ in 0..delta.abs() {
        if delta > 0 {
            d += 1;
            if d > days_in_month(y, m) {
                d = 1;
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
            }
        } else {
            d -= 1;
            if d == 0 {
                m = if m == 1 {
                    y -= 1;
                    12
                } else {
                    m - 1
                };
                d = days_in_month(y, m);
            }
        }
    }
    format!("{y:04}-{m:02}-{d:02}")
}

#[allow(clippy::many_single_char_names)]
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
    let mut doy = i32::from(d);
    for mm in 1..m {
        doy += i32::from(days_in_month(y, mm));
    }
    doy
}

/// Weekday with 0 = Sunday, via Zeller's congruence (Gregorian).
#[allow(clippy::many_single_char_names)]
fn weekday_from_sunday(y: i32, m: u8, d: u8) -> u8 {
    let (yy, mm) = if m < 3 { (y - 1, i32::from(m) + 12) } else { (y, i32::from(m)) };
    let k = yy.rem_euclid(100);
    let j = yy.div_euclid(100);
    // h: 0 = Saturday … 6 = Friday
    let h = (i32::from(d) + (13 * (mm + 1)) / 5 + k + k / 4 + j / 4 + 5 * j).rem_euclid(7);
    // `rem_euclid(7)` above guarantees h is in 0..7, so (h + 6) % 7 is too.
    #[allow(clippy::cast_sign_loss)]
    let result = ((h + 6) % 7) as u8;
    result
}

fn weeks_in_year(y: i32) -> i32 {
    let p = |y: i32| (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400)).rem_euclid(7);
    if p(y) == 4 || p(y - 1) == 3 { 53 } else { 52 }
}

/// ISO-8601 week number (1–53).
/// ISO week number *and* the week-year it belongs to. These disagree at the
/// turn of the year — 2021-01-01 is week 53 of 2020 — and a weekly log filed
/// under the calendar year would land in a week that doesn't exist.
fn iso_week_and_year(y: i32, m: u8, d: u8) -> (i32, i32) {
    let doy = ordinal(y, m, d);
    let wd_sun = i32::from(weekday_from_sunday(y, m, d)); // 0 = Sunday
    let iso_wd = if wd_sun == 0 { 7 } else { wd_sun };  // 1 = Mon … 7 = Sun
    let week = (doy - iso_wd + 10) / 7;
    if week < 1 {
        (weeks_in_year(y - 1), y - 1)
    } else if week > weeks_in_year(y) {
        (1, y + 1)
    } else {
        (week, y)
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
        assert_eq!(iso_week_and_year(2026, 1, 1), (1, 2026));
        assert_eq!(iso_week_and_year(2026, 6, 4), (23, 2026));
        // 2021-01-01 (Friday) is ISO week 53 of *2020* — the week-year and the
        // calendar year disagree, which is what `period_key` has to respect.
        assert_eq!(iso_week_and_year(2021, 1, 1), (53, 2020));
    }

    #[test]
    fn parse_roundtrip() {
        assert_eq!(parse_ymd("2026-06-04"), Some((2026, 6, 4)));
        assert_eq!(parse_ymd("not-a-date"), None);
        assert_eq!(parse_ymd("2026/06/04"), None);
    }

    #[test]
    fn add_days_within_month() {
        assert_eq!(add_days("2026-06-04", 0), "2026-06-04");
        assert_eq!(add_days("2026-06-04", 3), "2026-06-07");
    }

    #[test]
    fn add_days_rolls_over_month_and_year() {
        assert_eq!(add_days("2026-06-30", 1), "2026-07-01");
        assert_eq!(add_days("2026-12-31", 1), "2027-01-01");
    }

    #[test]
    fn add_days_handles_leap_february() {
        // 2024 is a leap year: Feb has 29 days.
        assert_eq!(add_days("2024-02-28", 1), "2024-02-29");
        assert_eq!(add_days("2024-02-28", 2), "2024-03-01");
        // 2026 is not: Feb 28 -> Mar 1.
        assert_eq!(add_days("2026-02-28", 1), "2026-03-01");
    }

    #[test]
    fn add_days_spanning_many_months() {
        // 100 days after 2026-01-01 -> 2026-04-11.
        assert_eq!(add_days("2026-01-01", 100), "2026-04-11");
    }

    #[test]
    fn add_days_passes_through_invalid_input() {
        assert_eq!(add_days("not-a-date", 5), "not-a-date");
    }

    // ── Periods (docs/bujo-roadmap.md §4) ────────────────────────────────────

    #[test]
    fn shift_days_goes_backwards_over_month_and_year_boundaries() {
        assert_eq!(shift_days("2026-08-13", -3), "2026-08-10");
        assert_eq!(shift_days("2026-03-01", -1), "2026-02-28");
        assert_eq!(shift_days("2024-03-01", -1), "2024-02-29", "leap year");
        assert_eq!(shift_days("2026-01-01", -1), "2025-12-31");
        assert_eq!(shift_days("2026-08-13", 0), "2026-08-13");
    }

    #[test]
    fn period_keys_are_stable_and_sortable() {
        // 2026-08-13 is a Thursday in ISO week 33.
        assert_eq!(period_key(Period::Day, "2026-08-13"), "2026-08-13");
        assert_eq!(period_key(Period::Week, "2026-08-13"), "2026-W33");
        assert_eq!(period_key(Period::Month, "2026-08-13"), "2026-08");
    }

    #[test]
    fn a_week_key_uses_the_iso_week_year_not_the_calendar_year() {
        // Filing this under 2021 would put it in a week 53 that doesn't exist.
        assert_eq!(period_key(Period::Week, "2021-01-01"), "2020-W53");
    }

    #[test]
    fn every_day_of_a_week_shares_one_key_and_one_start() {
        // Monday 2026-08-10 through Sunday 2026-08-16.
        for day in 10..=16 {
            let d = format!("2026-08-{day:02}");
            assert_eq!(period_key(Period::Week, &d), "2026-W33", "{d}");
            assert_eq!(period_start(Period::Week, &d), "2026-08-10", "{d}");
            assert_eq!(period_end(Period::Week, &d), "2026-08-16", "{d}");
        }
        // The next day starts a new week.
        assert_eq!(period_key(Period::Week, "2026-08-17"), "2026-W34");
    }

    #[test]
    fn period_start_and_end_bound_the_month() {
        assert_eq!(period_start(Period::Month, "2026-08-13"), "2026-08-01");
        assert_eq!(period_end(Period::Month, "2026-08-13"), "2026-08-31");
        assert_eq!(period_end(Period::Month, "2026-02-05"), "2026-02-28");
        assert_eq!(period_end(Period::Month, "2024-02-05"), "2024-02-29");
    }

    #[test]
    fn stepping_months_from_a_long_month_does_not_skip_short_ones() {
        // The reason `shift_period` normalises to the period start first:
        // naively adding a month to the 31st would land on a date February
        // doesn't have, and skip it entirely.
        let jan = "2026-01-31";
        let feb = shift_period(Period::Month, jan, 1);
        assert_eq!(feb, "2026-02-01");
        assert_eq!(period_key(Period::Month, &feb), "2026-02");
        assert_eq!(shift_period(Period::Month, &feb, 1), "2026-03-01");
    }

    #[test]
    fn stepping_wraps_the_year_in_both_directions() {
        assert_eq!(shift_period(Period::Month, "2026-12-05", 1), "2027-01-01");
        assert_eq!(shift_period(Period::Month, "2026-01-05", -1), "2025-12-01");
        assert_eq!(shift_period(Period::Day, "2026-01-01", -1), "2025-12-31");
        // A week step lands on the neighbouring Monday.
        assert_eq!(shift_period(Period::Week, "2026-08-13", 1), "2026-08-17");
        assert_eq!(shift_period(Period::Week, "2026-08-13", -1), "2026-08-03");
    }

    #[test]
    fn stepping_forward_then_back_returns_to_the_same_period() {
        for period in [Period::Day, Period::Week, Period::Month] {
            let start = period_key(period, "2026-08-13");
            let there = shift_period(period, "2026-08-13", 1);
            let back = shift_period(period, &there, -1);
            assert_eq!(period_key(period, &back), start, "{period:?}");
        }
    }

    #[test]
    fn date_vars_carry_the_week_year_for_weekly_paths() {
        let json = date_vars_json_for("2021-01-01");
        assert!(json.contains("\"week\":\"53\""), "{json}");
        assert!(json.contains("\"weekYear\":\"2020\""), "{json}");
        // The calendar year is still there for everything else.
        assert!(json.contains("\"year\":\"2021\""), "{json}");
    }
}
