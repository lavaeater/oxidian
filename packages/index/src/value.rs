//! The typed value model shared by frontmatter, inline fields, and queries.
//!
//! Dataview's expressiveness comes almost entirely from having real types
//! behind `key: value` — `rating > 8` and `due <= date(today) + dur(7 days)`
//! only work if `9` is a number and `2026-09-01` is a date. See
//! `docs/dataview.md` §4.1.
//!
//! Dates are a hand-rolled `Date` rather than a `chrono` dependency: notes only
//! ever carry `YYYY-MM-DD`, and the workspace already parses dates by hand in
//! `app::dates` to stay wasm-light.

use std::cmp::Ordering;
use std::fmt;

use serde::{Deserialize, Serialize};

/// A calendar date. Comparison is chronological, which is what `SORT` needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Parse `YYYY-MM-DD`, rejecting impossible months and days.
    pub fn parse(s: &str) -> Option<Date> {
        let s = s.trim();
        let b = s.as_bytes();
        if s.len() != 10 || b[4] != b'-' || b[7] != b'-' {
            return None;
        }
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[5..7].parse().ok()?;
        let day: u32 = s[8..10].parse().ok()?;
        if !(1..=12).contains(&month) || day == 0 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    /// Days since 1970-01-01 (may be negative), so date arithmetic and
    /// differences are plain integer maths.
    pub fn to_days(self) -> i64 {
        let (y, m) = if self.month <= 2 {
            (self.year - 1, self.month + 12)
        } else {
            (self.year, self.month)
        };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as i64;
        let doy = (153 * (m as i64 - 3) + 2) / 5 + self.day as i64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era as i64 * 146097 + doe - 719468
    }

    pub fn from_days(days: i64) -> Date {
        let z = days + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
        Date { year: (if month <= 2 { y + 1 } else { y }) as i32, month, day }
    }

    pub fn plus_days(self, days: i64) -> Date {
        Date::from_days(self.to_days() + days)
    }
}

impl fmt::Display for Date {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) => 29,
        2 => 28,
        _ => 0,
    }
}

/// A value of a note field, or the result of evaluating an expression.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    Null,
    Str(String),
    Num(f64),
    Bool(bool),
    Date(Date),
    /// A span of days. `date - date` yields one; `dur(7 days)` builds one.
    Duration(i64),
    /// A `[[WikiLink]]` target, kept distinct from a string so links render as
    /// links rather than as their own text.
    Link(String),
    List(Vec<Value>),
}

impl Value {
    /// Infer a value's type from raw YAML/inline text the way a note author
    /// would expect: `9` is a number, `true` a boolean, `2026-09-01` a date,
    /// `[[Note]]` a link, `[a, b]` a list, anything else a string.
    pub fn infer(raw: &str) -> Value {
        let s = raw.trim();
        if s.is_empty() {
            return Value::Null;
        }
        if let Some(inner) = s.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
            && !inner.starts_with('[')
        {
            // `[a, b]` — a flow list. `[[Note]]` is a link, handled below.
            let items: Vec<Value> = inner
                .split(',')
                .map(str::trim)
                .filter(|i| !i.is_empty())
                .map(Value::infer)
                .collect();
            return Value::List(items);
        }
        if let Some(target) = s.strip_prefix("[[").and_then(|s| s.strip_suffix("]]")) {
            let target = target.split('|').next().unwrap_or(target).trim();
            return Value::Link(target.to_string());
        }
        let unquoted = s.trim_matches('"').trim_matches('\'');
        match unquoted {
            "true" | "yes" => return Value::Bool(true),
            "false" | "no" => return Value::Bool(false),
            "null" | "none" | "~" => return Value::Null,
            _ => {}
        }
        if let Some(d) = Date::parse(unquoted) {
            return Value::Date(d);
        }
        if let Ok(n) = unquoted.parse::<f64>() {
            return Value::Num(n);
        }
        Value::Str(unquoted.to_string())
    }

    /// Truthiness, used by `WHERE`. Empty strings, empty lists, zero, and null
    /// are false; everything else is true — matching Dataview.
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Num(n) => *n != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(l) => !l.is_empty(),
            Value::Link(l) => !l.is_empty(),
            Value::Date(_) | Value::Duration(_) => true,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Null => "null",
            Value::Str(_) => "string",
            Value::Num(_) => "number",
            Value::Bool(_) => "boolean",
            Value::Date(_) => "date",
            Value::Duration(_) => "duration",
            Value::Link(_) => "link",
            Value::List(_) => "list",
        }
    }

    /// Plain-text rendering, used for table cells and string coercion.
    pub fn to_display(&self) -> String {
        match self {
            Value::Null => String::new(),
            Value::Str(s) => s.clone(),
            Value::Num(n) => {
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            Value::Bool(b) => b.to_string(),
            Value::Date(d) => d.to_string(),
            Value::Duration(days) => match days.abs() {
                1 => format!("{days} day"),
                _ => format!("{days} days"),
            },
            Value::Link(l) => format!("[[{l}]]"),
            Value::List(items) => {
                items.iter().map(Value::to_display).collect::<Vec<_>>().join(", ")
            }
        }
    }

    /// Ordering for `SORT`. Values of different types never compare equal;
    /// they fall back to type name so a sort is at least stable and grouped.
    /// `Null` always sorts last, which is what "undated last" needs.
    pub fn compare(&self, other: &Value) -> Ordering {
        match (self, other) {
            (Value::Null, Value::Null) => Ordering::Equal,
            (Value::Null, _) => Ordering::Greater,
            (_, Value::Null) => Ordering::Less,
            (Value::Num(a), Value::Num(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (Value::Str(a), Value::Str(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Value::Date(a), Value::Date(b)) => a.cmp(b),
            (Value::Duration(a), Value::Duration(b)) => a.cmp(b),
            (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
            (Value::Link(a), Value::Link(b)) => a.to_lowercase().cmp(&b.to_lowercase()),
            (Value::List(a), Value::List(b)) => a.len().cmp(&b.len()),
            _ => self.type_name().cmp(other.type_name()),
        }
    }

    /// Loose equality across compatible types, so `rating = "9"` and
    /// `rating = 9` both work on a note that wrote `rating: 9`.
    pub fn loose_eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Str(a), Value::Str(b)) => a.eq_ignore_ascii_case(b),
            (Value::Link(a), Value::Link(b)) | (Value::Link(a), Value::Str(b))
            | (Value::Str(a), Value::Link(b)) => a.eq_ignore_ascii_case(b),
            (Value::Str(_), _) | (_, Value::Str(_)) => {
                self.to_display().eq_ignore_ascii_case(&other.to_display())
            }
            _ => self == other,
        }
    }

    /// Does this value contain `needle`? Substring for strings, membership for
    /// lists — `contains(tags, "rpg")` and `contains(title, "Ex")`.
    pub fn contains(&self, needle: &Value) -> bool {
        match self {
            Value::List(items) => items.iter().any(|i| i.loose_eq(needle)),
            Value::Str(s) => s.to_lowercase().contains(&needle.to_display().to_lowercase()),
            Value::Link(l) => l.eq_ignore_ascii_case(&needle.to_display()),
            _ => self.loose_eq(needle),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_round_trip_through_day_numbers() {
        for s in ["1970-01-01", "2026-08-01", "2000-02-29", "1999-12-31"] {
            let d = Date::parse(s).unwrap();
            assert_eq!(Date::from_days(d.to_days()), d, "round trip {s}");
            assert_eq!(d.to_string(), s);
        }
        assert_eq!(Date::parse("1970-01-01").unwrap().to_days(), 0);
    }

    #[test]
    fn date_parsing_rejects_impossible_dates() {
        assert!(Date::parse("2026-13-01").is_none(), "month 13");
        assert!(Date::parse("2026-02-30").is_none(), "february 30th");
        assert!(Date::parse("2026-8-1").is_none(), "must be zero-padded");
        assert!(Date::parse("not a date").is_none());
        // Leap years are real leap years.
        assert!(Date::parse("2024-02-29").is_some());
        assert!(Date::parse("2100-02-29").is_none(), "2100 is not a leap year");
    }

    #[test]
    fn day_arithmetic_crosses_month_and_year_boundaries() {
        let d = Date::parse("2026-08-30").unwrap();
        assert_eq!(d.plus_days(7).to_string(), "2026-09-06");
        assert_eq!(Date::parse("2026-12-31").unwrap().plus_days(1).to_string(), "2027-01-01");
        assert_eq!(Date::parse("2026-01-01").unwrap().plus_days(-1).to_string(), "2025-12-31");
    }

    #[test]
    fn infers_the_type_a_note_author_would_expect() {
        assert_eq!(Value::infer("9"), Value::Num(9.0));
        assert_eq!(Value::infer("9.5"), Value::Num(9.5));
        assert_eq!(Value::infer("true"), Value::Bool(true));
        assert_eq!(Value::infer("2026-09-01"), Value::Date(Date::parse("2026-09-01").unwrap()));
        assert_eq!(Value::infer("[[Deus Ex]]"), Value::Link("Deus Ex".into()));
        assert_eq!(Value::infer("[[Deus Ex|that game]]"), Value::Link("Deus Ex".into()));
        assert_eq!(Value::infer("Deus Ex"), Value::Str("Deus Ex".into()));
        assert_eq!(Value::infer("\"9\""), Value::Num(9.0), "quotes are YAML noise");
        assert_eq!(Value::infer(""), Value::Null);
        assert_eq!(
            Value::infer("[games, rpg]"),
            Value::List(vec![Value::Str("games".into()), Value::Str("rpg".into())])
        );
    }

    #[test]
    fn truthiness_matches_dataview() {
        assert!(!Value::Null.truthy());
        assert!(!Value::Str(String::new()).truthy());
        assert!(!Value::Num(0.0).truthy());
        assert!(!Value::List(vec![]).truthy());
        assert!(Value::Num(9.0).truthy());
        assert!(Value::Str("x".into()).truthy());
    }

    #[test]
    fn null_sorts_last_regardless_of_direction() {
        let mut v = vec![Value::Null, Value::Num(2.0), Value::Num(1.0)];
        v.sort_by(|a, b| a.compare(b));
        assert_eq!(v, vec![Value::Num(1.0), Value::Num(2.0), Value::Null]);
    }

    #[test]
    fn loose_equality_bridges_strings_and_scalars() {
        assert!(Value::Num(9.0).loose_eq(&Value::Str("9".into())));
        assert!(Value::Str("RPG".into()).loose_eq(&Value::Str("rpg".into())));
        assert!(Value::Link("Deus Ex".into()).loose_eq(&Value::Str("deus ex".into())));
        assert!(!Value::Num(9.0).loose_eq(&Value::Num(8.0)));
    }

    #[test]
    fn contains_covers_lists_and_substrings() {
        let tags = Value::List(vec![Value::Str("games".into()), Value::Str("rpg".into())]);
        assert!(tags.contains(&Value::Str("RPG".into())));
        assert!(!tags.contains(&Value::Str("sim".into())));
        assert!(Value::Str("Deus Ex".into()).contains(&Value::Str("us e".into())));
    }

    #[test]
    fn numbers_display_without_trailing_zeros() {
        assert_eq!(Value::Num(9.0).to_display(), "9");
        assert_eq!(Value::Num(9.5).to_display(), "9.5");
        assert_eq!(Value::Duration(1).to_display(), "1 day");
        assert_eq!(Value::Duration(7).to_display(), "7 days");
    }
}
