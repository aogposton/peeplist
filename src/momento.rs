// Momentos (2026-08-02) — recurring/important personal moments (birthdays,
// anniversaries, "call Mom every Sunday"), moment_type_id 4. This module is
// the pure RRULE-expansion layer: a momento is a single MomentType "template"
// row (its own `due_at` doubles as the RRULE's DTSTART anchor date, its
// `metadata.recurrence_rule` the RFC 5545 RRULE string) — occurrences are
// never materialized as real rows, every view expands them on the fly here.
//
// Skip and delete-a-single-occurrence are the same action (an explicit
// product decision, not an oversight) — both just add that occurrence's
// date to `momento_excluded_occurrences` (iCal's own EXDATE concept).
// Completing a single occurrence adds it to `momento_completed_occurrences`
// instead. Both ride the existing metadata jsonb blob, no schema change.
use crate::types::MomentMetadata;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use rrule::RRuleSet;

#[derive(Debug, Clone, PartialEq)]
pub struct Occurrence {
    pub date: NaiveDate,
    // Full precision, not just the date — needed for reveal-window math
    // (is_revealed below), which can be as fine-grained as "1 hour before".
    // `date` above is kept as its own field rather than derived on demand
    // since every existing display site already formats straight off it.
    pub datetime: DateTime<Utc>,
    pub completed: bool,
}

// A generous fixed cap, not tied to `count` below — cheap to compute even
// for a daily rule, and simplest way to have enough raw occurrences left
// over after excluded ones are filtered out, without guessing how many
// extra to over-fetch.
const MAX_RAW_OCCURRENCES: u16 = 200;

// Builds the full "DTSTART:...\nRRULE:..." block RRuleSet's FromStr expects
// from this app's own storage shape (a bare due_at string + a bare RRULE
// string, stored separately) and parses it. None on any malformed input —
// every call site already treats a momento with no valid rule as simply
// having no occurrences, not an error to surface.
fn parse_rule_set(due_at: &str, recurrence_rule: &str) -> Option<RRuleSet> {
    let dt_start = crate::urgency::parse_moment_datetime(due_at)?;
    let dtstart_line = dt_start.format("DTSTART:%Y%m%dT%H%M%SZ").to_string();
    let full = format!("{dtstart_line}\nRRULE:{recurrence_rule}");
    full.parse::<RRuleSet>().ok()
}

// The next `count` occurrences on or after `today`, with excluded dates
// dropped and completed ones marked — the one function every momento view
// (the entity's Momentos tab, the global Momentos sidebar view) actually
// needs.
pub fn next_occurrences(due_at: &str, meta: &MomentMetadata, today: NaiveDate, count: usize) -> Vec<Occurrence> {
    let Some(rule) = meta.recurrence_rule.as_deref() else { return Vec::new() };
    let Some(set) = parse_rule_set(due_at, rule) else { return Vec::new() };

    let after = rrule::Tz::UTC.from_utc_datetime(&today.and_hms_opt(0, 0, 0).unwrap());
    let dates = set.after(after).all(MAX_RAW_OCCURRENCES).dates;

    dates.into_iter()
        .map(|dt| dt.with_timezone(&Utc))
        .filter(|dt| !meta.momento_excluded_occurrences.contains(&dt.format("%Y-%m-%d").to_string()))
        .take(count)
        .map(|datetime| {
            let date = datetime.date_naive();
            Occurrence {
                completed: meta.momento_completed_occurrences.contains(&date.format("%Y-%m-%d").to_string()),
                date,
                datetime,
            }
        })
        .collect()
}

// How far in advance a momento surfaces on the cross-entity Momentos
// sidebar view (see MomentMetadata::reveal_lead's own doc comment).
// Approximates "1 month" as 30 days — chrono::Duration has no calendar-
// aware Months variant, and a fuzzy heads-up threshold doesn't need
// exactness the way the RRULE anchor date itself does.
fn reveal_lead_duration(preset: &str) -> Option<Duration> {
    match preset {
        // Zero lead, not "no filtering" — still gated by is_revealed below,
        // just with no advance notice at all: hidden until the occurrence's
        // own due moment arrives, then immediately visible.
        "at_due" => Some(Duration::zero()),
        "1hour" => Some(Duration::hours(1)),
        "1day" => Some(Duration::days(1)),
        "1week" => Some(Duration::weeks(1)),
        "1month" => Some(Duration::days(30)),
        _ => None,
    }
}

// None reveal_lead (the default, "show next on completion" in the add-
// momento form) always reveals — this is just today's occurrence being
// whatever's chronologically next, no artificial hiding on top of that.
// Once the current occurrence's date passes, the next one becomes "next"
// on its own (see next_occurrences' `.after(today)`); completing the
// current occurrence doesn't independently make the following one visible
// any earlier than that. An unrecognized preset string fails open the same
// way (never hides something by accident) rather than an explicit
// always-reveal case being different from an unknown one.
pub fn is_revealed(occurrence_dt: DateTime<Utc>, reveal_lead: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(preset) = reveal_lead else { return true };
    let Some(lead) = reveal_lead_duration(preset) else { return true };
    now >= occurrence_dt - lead
}

// Options for the add-momento form's reveal-timing <select> — (label,
// stored value). The first entry's stored value is "" (empty), which the
// form treats the same as no reveal_lead at all (None) — an explicit,
// visible default rather than an invisible blank-selection default.
pub fn reveal_presets() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Show next on completion (default)", ""),
        ("At due date", "at_due"),
        ("1 hour before", "1hour"),
        ("1 day before", "1day"),
        ("1 week before", "1week"),
        ("1 month before", "1month"),
    ]
}

// Plain-language summary for the presets the add-momento form actually
// offers (Daily/Monthly/Yearly buttons, plus the weekly day-picker's
// FREQ=WEEKLY;BYDAY=<one or more codes> — the raw RRULE field itself isn't
// shown in that form anymore, see components::entity::ab_momentos_cmp, so
// this is the only place most users ever see their rule described).
// Anything else (hand-edited BYMONTHDAY/INTERVAL/etc, from before the field
// was hidden, or an imported backup) falls back to the raw RRULE string
// verbatim.
pub fn describe_rule(recurrence_rule: &str) -> String {
    match recurrence_rule {
        "FREQ=DAILY" => return "Every day".to_string(),
        "FREQ=MONTHLY" => return "Every month".to_string(),
        "FREQ=YEARLY" => return "Every year".to_string(),
        _ => {}
    }
    if let Some(byday) = recurrence_rule.strip_prefix("FREQ=WEEKLY;BYDAY=") {
        let day_name = |code: &str| match code {
            "MO" => Some("Monday"), "TU" => Some("Tuesday"), "WE" => Some("Wednesday"),
            "TH" => Some("Thursday"), "FR" => Some("Friday"), "SA" => Some("Saturday"), "SU" => Some("Sunday"),
            _ => None,
        };
        if let Some(names) = byday.split(',').map(day_name).collect::<Option<Vec<_>>>() {
            if !names.is_empty() {
                return format!("Every {}", names.join(", "));
            }
        }
    }
    recurrence_rule.to_string()
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MomentMetadata;

    fn meta(rule: &str) -> MomentMetadata {
        MomentMetadata { recurrence_rule: Some(rule.to_string()), ..Default::default() }
    }

    #[test]
    fn expands_weekly_occurrences() {
        // 2026-08-03 is a Monday.
        let m = meta("FREQ=WEEKLY;BYDAY=MO");
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let occurrences = next_occurrences("2026-08-03T09:00", &m, today, 3);
        let dates: Vec<String> = occurrences.iter().map(|o| o.date.format("%Y-%m-%d").to_string()).collect();
        assert_eq!(dates, vec!["2026-08-03", "2026-08-10", "2026-08-17"]);
    }

    #[test]
    fn excluded_occurrence_is_dropped_but_series_continues() {
        let mut m = meta("FREQ=WEEKLY;BYDAY=MO");
        m.momento_excluded_occurrences.push("2026-08-10".to_string());
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let occurrences = next_occurrences("2026-08-03T09:00", &m, today, 3);
        let dates: Vec<String> = occurrences.iter().map(|o| o.date.format("%Y-%m-%d").to_string()).collect();
        // 8/10 skipped entirely — not replaced by pulling 8/24 forward early
        // either, since "count" occurrences is a display convenience, not a
        // guarantee every slot is filled from a truncated window; the point
        // here is just confirming 8/10 never appears.
        assert!(!dates.contains(&"2026-08-10".to_string()));
        assert!(dates.contains(&"2026-08-03".to_string()));
        assert!(dates.contains(&"2026-08-17".to_string()));
    }

    #[test]
    fn completed_occurrence_is_marked_but_still_present() {
        let mut m = meta("FREQ=WEEKLY;BYDAY=MO");
        m.momento_completed_occurrences.push("2026-08-03".to_string());
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let occurrences = next_occurrences("2026-08-03T09:00", &m, today, 2);
        assert!(occurrences[0].completed);
        assert_eq!(occurrences[0].date.format("%Y-%m-%d").to_string(), "2026-08-03");
        assert!(!occurrences[1].completed);
    }

    #[test]
    fn yearly_birthday_style() {
        let m = meta("FREQ=YEARLY");
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let occurrences = next_occurrences("2020-05-14T00:00", &m, today, 2);
        let dates: Vec<String> = occurrences.iter().map(|o| o.date.format("%Y-%m-%d").to_string()).collect();
        assert_eq!(dates, vec!["2026-05-14", "2027-05-14"]);
    }

    #[test]
    fn no_recurrence_rule_means_no_occurrences() {
        let m = MomentMetadata::default();
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        assert!(next_occurrences("2026-08-03T09:00", &m, today, 5).is_empty());
    }

    #[test]
    fn invalid_rule_is_none_not_a_panic() {
        let m = meta("NOT-A-VALID-RRULE");
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        assert!(next_occurrences("2026-08-03T09:00", &m, today, 5).is_empty());
    }

    #[test]
    fn describe_known_presets() {
        assert_eq!(describe_rule("FREQ=DAILY"), "Every day");
        assert_eq!(describe_rule("FREQ=WEEKLY;BYDAY=MO"), "Every Monday");
        assert_eq!(describe_rule("FREQ=YEARLY"), "Every year");
    }

    #[test]
    fn describe_multi_day_weekly_rule() {
        assert_eq!(describe_rule("FREQ=WEEKLY;BYDAY=MO,WE,FR"), "Every Monday, Wednesday, Friday");
    }

    #[test]
    fn describe_falls_back_to_raw_string_for_custom_rules() {
        assert_eq!(describe_rule("FREQ=MONTHLY;INTERVAL=3;BYMONTHDAY=15"), "FREQ=MONTHLY;INTERVAL=3;BYMONTHDAY=15");
    }

    #[test]
    fn no_reveal_lead_always_reveals() {
        let occurrence = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(is_revealed(occurrence, None, now));
    }

    #[test]
    fn reveal_lead_hides_until_within_the_window() {
        let occurrence = Utc.with_ymd_and_hms(2026, 8, 10, 9, 0, 0).unwrap();
        // A week before the window opens — not revealed yet.
        let too_early = Utc.with_ymd_and_hms(2026, 8, 1, 9, 0, 0).unwrap();
        assert!(!is_revealed(occurrence, Some("1week"), too_early));
        // Exactly at the window boundary and after — revealed.
        let at_boundary = Utc.with_ymd_and_hms(2026, 8, 3, 9, 0, 0).unwrap();
        assert!(is_revealed(occurrence, Some("1week"), at_boundary));
        let after = Utc.with_ymd_and_hms(2026, 8, 9, 9, 0, 0).unwrap();
        assert!(is_revealed(occurrence, Some("1week"), after));
    }

    #[test]
    fn reveal_lead_hour_granularity() {
        let occurrence = Utc.with_ymd_and_hms(2026, 8, 10, 14, 0, 0).unwrap();
        assert!(!is_revealed(occurrence, Some("1hour"), Utc.with_ymd_and_hms(2026, 8, 10, 12, 0, 0).unwrap()));
        assert!(is_revealed(occurrence, Some("1hour"), Utc.with_ymd_and_hms(2026, 8, 10, 13, 30, 0).unwrap()));
    }

    #[test]
    fn unrecognized_reveal_preset_fails_open() {
        let occurrence = Utc.with_ymd_and_hms(2027, 1, 1, 0, 0, 0).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        assert!(is_revealed(occurrence, Some("not-a-real-preset"), now));
    }

    #[test]
    fn at_due_hides_until_the_exact_due_moment_no_advance_notice() {
        let occurrence = Utc.with_ymd_and_hms(2026, 8, 10, 9, 0, 0).unwrap();
        let a_minute_before = Utc.with_ymd_and_hms(2026, 8, 10, 8, 59, 0).unwrap();
        assert!(!is_revealed(occurrence, Some("at_due"), a_minute_before));
        assert!(is_revealed(occurrence, Some("at_due"), occurrence));
        let after = Utc.with_ymd_and_hms(2026, 8, 10, 9, 0, 1).unwrap();
        assert!(is_revealed(occurrence, Some("at_due"), after));
    }

    #[test]
    fn reveal_presets_lead_with_the_default_show_on_completion_option() {
        let presets = reveal_presets();
        assert_eq!(presets[0], ("Show next on completion (default)", ""));
        assert!(presets.iter().any(|(_, value)| *value == "at_due"));
    }

    #[test]
    fn occurrence_carries_full_datetime_matching_its_date() {
        let m = meta("FREQ=WEEKLY;BYDAY=MO");
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let occurrences = next_occurrences("2026-08-03T09:00", &m, today, 1);
        assert_eq!(occurrences[0].datetime.date_naive(), occurrences[0].date);
        assert_eq!(occurrences[0].datetime.format("%H:%M").to_string(), "09:00");
    }
}
