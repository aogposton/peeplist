// Taskwarrior-style relative date parsing for due:/scheduled:/until: values
// typed in the quick-capture composer (see quick_capture.rs::parse_date_token,
// the sole call site — the activity bar's own due/scheduled/until editors are
// native <input type="date/time/datetime-local"> elements with no free-text
// grammar of their own).
//
// Pure and side-effect-free: `now` is passed in rather than read internally
// (unlike the old parse_date_token, which called chrono::Utc::now() inline),
// so every relative-date rule here is deterministically unit-testable.
//
// Scope note: this app has no per-user timezone concept anywhere else
// (everything is chrono::Utc-based, see urgency.rs) — "local date" in
// Taskwarrior's own docs is treated here as "UTC date," consistent with the
// rest of the codebase, not a new timezone feature.
//
// Same-cycle convention (2026-08-01 product decision, confirmed with the
// user rather than guessed from Taskwarrior's own ambiguous doc wording):
// a named weekday resolves within the CURRENT week (Mon-Sun containing
// today) even if that day already passed this week; a named month resolves
// within the CURRENT year even if that month already passed this year.
// Typing the exact current day/month means today/this-month, not skipped
// forward a full cycle. The Easter-based holiday cluster follows the same
// rule (current year, no rollover). Nth-day-of-month (`21st`) is the one
// exception — it's explicitly forward-looking ("the next Nth day") per
// Taskwarrior's own unambiguous wording, so it skips to the next month that
// has that day if this month's has already passed.
use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, NaiveDateTime, TimeZone, Timelike, Utc, Weekday};

pub fn parse_taskwarrior_date(input: &str, now: DateTime<Utc>) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_lowercase();

    parse_literal_keyword(&lower, now)
        .or_else(|| parse_relative_offset(&lower, now))
        .or_else(|| parse_weekday_or_month_name(&lower, now))
        .or_else(|| parse_ordinal_day_of_month(&lower, now))
        .or_else(|| parse_easter_cluster(&lower, now.date_naive()))
        .or_else(|| parse_iso8601(trimmed))
}

// --- output formatting -----------------------------------------------------
//
// Bare "%Y-%m-%dT%H:%M" (no seconds/offset) is this app's existing
// due_at/scheduled_at/until_at convention everywhere else (see
// quick_capture.rs's old parse_date_token, moment.rs). Full-precision output
// (seconds, or a real UTC offset) is used only when the input itself carried
// that precision (`eod`'s :59 seconds, or an explicit ISO-8601 input with
// seconds/an offset) — urgency.rs::parse_moment_datetime already accepts
// both shapes, so nothing downstream needs to change.

fn fmt_minute(d: NaiveDate, hour: u32, minute: u32) -> String {
    format!("{}T{:02}:{:02}", d.format("%Y-%m-%d"), hour, minute)
}

fn fmt_second(d: NaiveDate, hour: u32, minute: u32, second: u32) -> String {
    format!("{}T{:02}:{:02}:{:02}", d.format("%Y-%m-%d"), hour, minute, second)
}

fn fmt_minute_from_dt(dt: DateTime<Utc>) -> String {
    fmt_minute(dt.date_naive(), dt.hour(), dt.minute())
}

// --- calendar helpers -------------------------------------------------------

fn last_day_of_month(year: i32, month: u32) -> NaiveDate {
    let first_of_next = if month == 12 {
        NaiveDate::from_ymd_opt(year + 1, 1, 1).unwrap()
    } else {
        NaiveDate::from_ymd_opt(year, month + 1, 1).unwrap()
    };
    first_of_next - Duration::days(1)
}

fn start_of_week_monday(d: NaiveDate) -> NaiveDate {
    d - Duration::days(d.weekday().num_days_from_monday() as i64)
}

fn start_of_quarter(d: NaiveDate) -> NaiveDate {
    let q_start_month = ((d.month() - 1) / 3) * 3 + 1;
    NaiveDate::from_ymd_opt(d.year(), q_start_month, 1).unwrap()
}

fn end_of_quarter(d: NaiveDate) -> NaiveDate {
    let start = start_of_quarter(d);
    last_day_of_month(start.year(), start.month() + 2)
}

// --- 1. literal keywords -----------------------------------------------------

fn parse_literal_keyword(s: &str, now: DateTime<Utc>) -> Option<String> {
    let today = now.date_naive();
    Some(match s {
        "now" => fmt_minute_from_dt(now),
        "today" | "sod" => fmt_minute(today, 0, 0),
        "eod" => fmt_second(today, 23, 59, 59),
        "tomorrow" => fmt_minute(today + Duration::days(1), 0, 0),
        "yesterday" => fmt_minute(today - Duration::days(1), 0, 0),
        "later" | "someday" => fmt_minute(NaiveDate::from_ymd_opt(9999, 12, 30).unwrap(), 0, 0),
        "soy" => fmt_minute(NaiveDate::from_ymd_opt(today.year(), 1, 1).unwrap(), 0, 0),
        "eoy" => fmt_second(NaiveDate::from_ymd_opt(today.year(), 12, 31).unwrap(), 23, 59, 59),
        "som" => fmt_minute(NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap(), 0, 0),
        "eom" => fmt_second(last_day_of_month(today.year(), today.month()), 23, 59, 59),
        "soq" => fmt_minute(start_of_quarter(today), 0, 0),
        "eoq" => fmt_second(end_of_quarter(today), 23, 59, 59),
        "sow" => fmt_minute(start_of_week_monday(today), 0, 0),
        "eow" => fmt_second(start_of_week_monday(today) + Duration::days(6), 23, 59, 59),
        "soww" => fmt_minute(start_of_week_monday(today), 0, 0),
        "eoww" => fmt_second(start_of_week_monday(today) + Duration::days(4), 23, 59, 59),
        _ => return None,
    })
}

// --- 2. relative numeric offsets (Nd, Nw, Nm, Ny, Nmin, Nhour) --------------
//
// Precise offsets from `now` (preserving time-of-day), not snapped to
// midnight — "20min"/"2hour" only make sense as exact-time offsets, and
// Taskwarrior's own durations behave the same way for every unit.

fn parse_relative_offset(s: &str, now: DateTime<Utc>) -> Option<String> {
    let digit_end = s.find(|c: char| !c.is_ascii_digit())?;
    if digit_end == 0 {
        return None;
    }
    let (num_str, suffix) = s.split_at(digit_end);
    let n: i64 = num_str.parse().ok()?;
    let dt = match suffix {
        "min" => now + Duration::minutes(n),
        "hour" => now + Duration::hours(n),
        "d" => now + Duration::days(n),
        "w" => now + Duration::weeks(n),
        "m" => add_months_preserving_time(now, n)?,
        "y" => add_months_preserving_time(now, n.checked_mul(12)?)?,
        _ => return None,
    };
    Some(fmt_minute_from_dt(dt))
}

fn add_months_preserving_time(now: DateTime<Utc>, months: i64) -> Option<DateTime<Utc>> {
    let date = now.date_naive();
    let new_date = if months >= 0 {
        date.checked_add_months(Months::new(months as u32))?
    } else {
        date.checked_sub_months(Months::new(months.unsigned_abs() as u32))?
    };
    Some(Utc.from_utc_datetime(&new_date.and_time(now.time())))
}

// --- 3. weekday / month names ------------------------------------------------

fn weekday_from_str(s: &str) -> Option<Weekday> {
    Some(match s {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" => Weekday::Tue,
        "wednesday" | "wed" => Weekday::Wed,
        "thursday" | "thu" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    })
}

fn month_from_str(s: &str) -> Option<u32> {
    Some(match s {
        "january" | "jan" => 1,
        "february" | "feb" => 2,
        "march" | "mar" => 3,
        "april" | "apr" => 4,
        "may" => 5,
        "june" | "jun" => 6,
        "july" | "jul" => 7,
        "august" | "aug" => 8,
        "september" | "sep" => 9,
        "october" | "oct" => 10,
        "november" | "nov" => 11,
        "december" | "dec" => 12,
        _ => return None,
    })
}

fn parse_weekday_or_month_name(s: &str, now: DateTime<Utc>) -> Option<String> {
    let today = now.date_naive();
    if let Some(wd) = weekday_from_str(s) {
        let monday = start_of_week_monday(today);
        let target = monday + Duration::days(wd.num_days_from_monday() as i64);
        return Some(fmt_minute(target, 0, 0));
    }
    if let Some(month) = month_from_str(s) {
        let target = NaiveDate::from_ymd_opt(today.year(), month, 1).unwrap();
        return Some(fmt_minute(target, 0, 0));
    }
    None
}

// --- 4. Nth day of month (1st, 2nd, 21st, ...) — explicitly forward-looking -

fn parse_ordinal_number(s: &str) -> Option<u32> {
    let digits_end = s.find(|c: char| !c.is_ascii_digit())?;
    if digits_end == 0 {
        return None;
    }
    let (num, suffix) = s.split_at(digits_end);
    let n: u32 = num.parse().ok()?;
    let expected = match n % 100 {
        11..=13 => "th",
        _ => match n % 10 {
            1 => "st",
            2 => "nd",
            3 => "rd",
            _ => "th",
        },
    };
    (suffix == expected).then_some(n)
}

fn parse_ordinal_day_of_month(s: &str, now: DateTime<Utc>) -> Option<String> {
    let today = now.date_naive();
    let day = parse_ordinal_number(s)?;
    if day == 0 || day > 31 {
        return None;
    }
    let mut year = today.year();
    let mut month = today.month();
    // Bounded: every day 1-31 occurs in at least one month within any
    // 12-consecutive-month span, so this always terminates well within 12
    // iterations.
    for _ in 0..24 {
        if let Some(candidate) = NaiveDate::from_ymd_opt(year, month, day) {
            if candidate >= today {
                return Some(fmt_minute(candidate, 0, 0));
            }
        }
        month += 1;
        if month > 12 {
            month = 1;
            year += 1;
        }
    }
    None
}

// --- 5. Easter-based cluster + Swedish midsummer ----------------------------

// Anonymous Gregorian algorithm (Meeus/Jones/Butcher) for the date of Easter
// Sunday in the Gregorian calendar.
fn easter_sunday(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = (h + l - 7 * m + 114) % 31 + 1;
    NaiveDate::from_ymd_opt(year, month as u32, day as u32).unwrap()
}

// Midsummer's Eve: the Friday on or after June 19th. Midsummer's Day: the
// Saturday on or after June 20th (always the day right after Midsummer's
// Eve, since Friday >= June 19 implies Saturday >= June 20).
fn midsommarafton(year: i32) -> NaiveDate {
    let june19 = NaiveDate::from_ymd_opt(year, 6, 19).unwrap();
    let offset = (Weekday::Fri.num_days_from_monday() as i64
        - june19.weekday().num_days_from_monday() as i64)
        .rem_euclid(7);
    june19 + Duration::days(offset)
}

fn parse_easter_cluster(s: &str, today: NaiveDate) -> Option<String> {
    let year = today.year();
    let date = match s {
        "goodfriday" => easter_sunday(year) - Duration::days(2),
        "easter" => easter_sunday(year),
        "eastermonday" => easter_sunday(year) + Duration::days(1),
        "ascension" => easter_sunday(year) + Duration::days(39),
        "pentecost" => easter_sunday(year) + Duration::days(49),
        "midsommarafton" => midsommarafton(year),
        "midsommar" => midsommarafton(year) + Duration::days(1),
        _ => return None,
    };
    Some(fmt_minute(date, 0, 0))
}

// --- 6. ISO-8601 -------------------------------------------------------------

fn parse_iso8601(s: &str) -> Option<String> {
    // Punctuated, with an explicit offset or 'Z' — covers most of the
    // "+hh:mm"/"Z" variants directly.
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc).to_rfc3339());
    }
    // Punctuated, no offset — treated as UTC directly, matching this app's
    // existing bare-datetime convention (see module doc comment above).
    for fmt in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M"] {
        if let Ok(ndt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(fmt_minute(ndt.date(), ndt.hour(), ndt.minute()));
        }
    }
    for fmt in ["%Y-%m-%d", "%Y-%j", "%G-W%V-%u"] {
        if let Ok(nd) = NaiveDate::parse_from_str(s, fmt) {
            return Some(fmt_minute(nd, 0, 0));
        }
    }
    // Unpunctuated "basic" forms (YYYYMMDD, YYYYDDD, YYYYWwwD) — chrono's
    // format-string parser expects the literal separators, so these
    // fixed-width forms are parsed by hand instead.
    parse_iso8601_basic(s)
}

fn weekday_from_iso_num(n: u32) -> Option<Weekday> {
    Some(match n {
        1 => Weekday::Mon,
        2 => Weekday::Tue,
        3 => Weekday::Wed,
        4 => Weekday::Thu,
        5 => Weekday::Fri,
        6 => Weekday::Sat,
        7 => Weekday::Sun,
        _ => return None,
    })
}

fn parse_iso8601_basic(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    if bytes.len() == 8 && bytes.iter().all(u8::is_ascii_digit) {
        let year: i32 = s[0..4].parse().ok()?;
        let month: u32 = s[4..6].parse().ok()?;
        let day: u32 = s[6..8].parse().ok()?;
        let nd = NaiveDate::from_ymd_opt(year, month, day)?;
        return Some(fmt_minute(nd, 0, 0));
    }
    if bytes.len() == 7 && bytes.iter().all(u8::is_ascii_digit) {
        let year: i32 = s[0..4].parse().ok()?;
        let doy: u32 = s[4..7].parse().ok()?;
        let nd = NaiveDate::from_yo_opt(year, doy)?;
        return Some(fmt_minute(nd, 0, 0));
    }
    if bytes.len() == 8 && bytes[4] == b'W' {
        let year: i32 = s[0..4].parse().ok()?;
        let week: u32 = s[5..7].parse().ok()?;
        let weekday_num: u32 = s[7..8].parse().ok()?;
        let wd = weekday_from_iso_num(weekday_num)?;
        let nd = NaiveDate::from_isoywd_opt(year, week, wd)?;
        return Some(fmt_minute(nd, 0, 0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // Wed 2026-08-05 12:30:00 UTC, an arbitrary fixed "now" for every test.
    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 12, 30, 0).unwrap()
    }

    #[test]
    fn today_tomorrow_yesterday() {
        assert_eq!(parse_taskwarrior_date("today", now()), Some("2026-08-05T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("Tomorrow", now()), Some("2026-08-06T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("YESTERDAY", now()), Some("2026-08-04T00:00".to_string()));
    }

    #[test]
    fn now_sod_eod() {
        assert_eq!(parse_taskwarrior_date("now", now()), Some("2026-08-05T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("sod", now()), Some("2026-08-05T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eod", now()), Some("2026-08-05T23:59:59".to_string()));
    }

    #[test]
    fn later_someday() {
        assert_eq!(parse_taskwarrior_date("later", now()), Some("9999-12-30T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("someday", now()), Some("9999-12-30T00:00".to_string()));
    }

    #[test]
    fn start_end_of_year_month_week() {
        assert_eq!(parse_taskwarrior_date("soy", now()), Some("2026-01-01T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eoy", now()), Some("2026-12-31T23:59:59".to_string()));
        assert_eq!(parse_taskwarrior_date("som", now()), Some("2026-08-01T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eom", now()), Some("2026-08-31T23:59:59".to_string()));
        // Wed 2026-08-05 -> week is Mon 2026-08-03 .. Sun 2026-08-09
        assert_eq!(parse_taskwarrior_date("sow", now()), Some("2026-08-03T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eow", now()), Some("2026-08-09T23:59:59".to_string()));
        assert_eq!(parse_taskwarrior_date("soww", now()), Some("2026-08-03T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eoww", now()), Some("2026-08-07T23:59:59".to_string()));
    }

    #[test]
    fn start_end_of_quarter() {
        // Aug is in Q3 (Jul-Sep).
        assert_eq!(parse_taskwarrior_date("soq", now()), Some("2026-07-01T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eoq", now()), Some("2026-09-30T23:59:59".to_string()));
    }

    #[test]
    fn relative_numeric_offsets() {
        assert_eq!(parse_taskwarrior_date("1d", now()), Some("2026-08-06T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("2d", now()), Some("2026-08-07T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("1w", now()), Some("2026-08-12T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("1m", now()), Some("2026-09-05T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("1y", now()), Some("2027-08-05T12:30".to_string()));
        assert_eq!(parse_taskwarrior_date("20min", now()), Some("2026-08-05T12:50".to_string()));
        assert_eq!(parse_taskwarrior_date("2hour", now()), Some("2026-08-05T14:30".to_string()));
    }

    #[test]
    fn month_end_clamping_for_month_offsets() {
        let jan31 = Utc.with_ymd_and_hms(2026, 1, 31, 9, 0, 0).unwrap();
        // Jan 31 + 1 month -> Feb 28 (2026 isn't a leap year), not an error.
        assert_eq!(parse_taskwarrior_date("1m", jan31), Some("2026-02-28T09:00".to_string()));
    }

    #[test]
    fn weekday_names_resolve_within_the_current_week() {
        // now() is Wed 2026-08-05. Current week: Mon 08-03 .. Sun 08-09.
        assert_eq!(parse_taskwarrior_date("monday", now()), Some("2026-08-03T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("Wed", now()), Some("2026-08-05T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("sunday", now()), Some("2026-08-09T00:00".to_string()));
    }

    #[test]
    fn same_day_weekday_means_today() {
        let monday_now = Utc.with_ymd_and_hms(2026, 8, 3, 9, 0, 0).unwrap();
        assert_eq!(parse_taskwarrior_date("monday", monday_now), Some("2026-08-03T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("tuesday", monday_now), Some("2026-08-04T00:00".to_string()));
    }

    #[test]
    fn month_names_resolve_within_the_current_year_even_if_passed() {
        // now() is 2026-08-05. "august" already started; "september" hasn't.
        assert_eq!(parse_taskwarrior_date("august", now()), Some("2026-08-01T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("Sep", now()), Some("2026-09-01T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("january", now()), Some("2026-01-01T00:00".to_string()));
    }

    #[test]
    fn nth_day_of_month_is_forward_looking() {
        // now() is 2026-08-05. The 1st has passed this month -> next month's 1st.
        assert_eq!(parse_taskwarrior_date("1st", now()), Some("2026-09-01T00:00".to_string()));
        // The 21st hasn't happened yet this month.
        assert_eq!(parse_taskwarrior_date("21st", now()), Some("2026-08-21T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("2nd", now()), Some("2026-09-02T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("3rd", now()), Some("2026-09-03T00:00".to_string()));
    }

    #[test]
    fn nth_day_skips_months_that_dont_have_it() {
        // 30th, asked for right after Jan 30 has passed -> skip Feb (no 30th).
        let late_jan = Utc.with_ymd_and_hms(2026, 1, 31, 0, 0, 0).unwrap();
        assert_eq!(parse_taskwarrior_date("30th", late_jan), Some("2026-03-30T00:00".to_string()));
    }

    #[test]
    fn ordinal_suffix_must_match() {
        assert_eq!(parse_taskwarrior_date("1th", now()), None);
        assert_eq!(parse_taskwarrior_date("11th", now()).is_some(), true);
        assert_eq!(parse_taskwarrior_date("11st", now()), None);
    }

    #[test]
    fn easter_cluster_2026() {
        // Easter Sunday 2026 is April 5th (verified via the Gregorian
        // computus algorithm implemented above).
        assert_eq!(parse_taskwarrior_date("easter", now()), Some("2026-04-05T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("goodfriday", now()), Some("2026-04-03T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("eastermonday", now()), Some("2026-04-06T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("ascension", now()), Some("2026-05-14T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("pentecost", now()), Some("2026-05-24T00:00".to_string()));
    }

    #[test]
    fn midsummer_2026() {
        // June 19 2026 is a Friday, so Midsummer's Eve is June 19 itself.
        assert_eq!(parse_taskwarrior_date("midsommarafton", now()), Some("2026-06-19T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("midsommar", now()), Some("2026-06-20T00:00".to_string()));
    }

    #[test]
    fn iso8601_punctuated() {
        assert_eq!(parse_taskwarrior_date("2015-06-15", now()), Some("2015-06-15T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("2015-06-15T12:34", now()), Some("2015-06-15T12:34".to_string()));
        assert_eq!(parse_taskwarrior_date("2015-06-15T12:34:56", now()), Some("2015-06-15T12:34".to_string()));
        let with_offset = parse_taskwarrior_date("2015-06-15T12:34:56+05:00", now()).unwrap();
        assert!(with_offset.starts_with("2015-06-15T07:34:56"));
        let with_z = parse_taskwarrior_date("2015-06-15T12:34:56Z", now()).unwrap();
        assert!(with_z.starts_with("2015-06-15T12:34:56"));
    }

    #[test]
    fn iso8601_ordinal_and_week_dates() {
        // 2015-166 is June 15 2015 (day 166 of a non-leap year).
        assert_eq!(parse_taskwarrior_date("2015-166", now()), Some("2015-06-15T00:00".to_string()));
        // 2015-W24-1 is the Monday of ISO week 24, 2015 (June 8 2015).
        assert_eq!(parse_taskwarrior_date("2015-W24-1", now()), Some("2015-06-08T00:00".to_string()));
    }

    #[test]
    fn iso8601_basic_unpunctuated() {
        assert_eq!(parse_taskwarrior_date("20150615", now()), Some("2015-06-15T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("2015166", now()), Some("2015-06-15T00:00".to_string()));
        assert_eq!(parse_taskwarrior_date("2015W241", now()), Some("2015-06-08T00:00".to_string()));
    }

    #[test]
    fn garbage_input_is_none() {
        assert_eq!(parse_taskwarrior_date("whenever", now()), None);
        assert_eq!(parse_taskwarrior_date("", now()), None);
        assert_eq!(parse_taskwarrior_date("2026-13-40", now()), None);
    }
}
