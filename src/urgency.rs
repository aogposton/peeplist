// The single function responsible for turning a moment into a priority
// score, plus the weights that drive it. This exists as its own module
// (not buried in components/moment.rs among UI code) specifically so there
// is one obvious place to point to when explaining or changing *why*
// something ranks the way it does — see compute_urgency() below.
//
// Model: taskwarrior's own urgency formula, which is "coefficient ×
// indicator" summed across independent factors. Each indicator is a plain
// number (usually 0..1, sometimes -1..1) describing *how true* that factor
// is for this moment, entirely independent of how much it should matter.
// The weight (coefficient) is the only thing that says how much it should
// matter — so changing a weight in Settings changes the ranking without
// ever touching this function's logic, and every factor is independently
// tunable (including disabling one entirely by setting its weight to 0).
use crate::types::MomentType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct UrgencyWeights {
    /// Applied to a 0..1 "how close is the due date" ramp — 1.0 once
    /// overdue, sliding down to 0.0 at 14+ days out. No due date at all
    /// contributes nothing.
    pub due: f64,
    /// Flat contribution when `metadata.priority == Some("H")`. Not an
    /// indicator × weight like the others — the weight itself is exactly
    /// what a High-priority task's score gets, since the field is either
    /// set to H or it isn't.
    pub priority_high: f64,
    pub priority_medium: f64,
    pub priority_low: f64,
    /// Applied when `metadata.project` is set to anything non-empty —
    /// taskwarrior's own reasoning (organized/tracked work gets a small
    /// nudge) carried over as-is.
    pub project: f64,
    /// Applied when `metadata.scheduled_at` is set *and has already
    /// passed* — i.e. the task is now actionable, not just eventually
    /// actionable. A future scheduled date contributes nothing (this is a
    /// scoring nudge only; peeplist doesn't yet hide tasks before their
    /// scheduled date — see DESIGN_PROGRESS.md).
    pub scheduled: f64,
    /// Applied to the moment's own -1..1 gravity dial (gravity is stored
    /// -100..100, normalized here).
    pub gravity: f64,
    /// Applied to a 0..1 "how long has this sat around" ramp, capping at
    /// 30 days old.
    pub age: f64,
    /// Applied (as a flat contribution, typically negative — see the
    /// default) when this moment depends on another moment that isn't
    /// completed yet. Blocked work shouldn't usually rank as urgent, since
    /// it can't actually be worked on.
    pub blocked: f64,
    /// Applied (as a flat contribution, typically positive) when finishing
    /// this moment would unblock at least one other open moment.
    pub blocking: f64,
    /// Applied per tag, capped at 3 tags, on the theory that a
    /// more-thoroughly-categorized task was more deliberately captured.
    /// Weak signal by design — small default weight.
    pub tags: f64,
}

impl Default for UrgencyWeights {
    fn default() -> Self {
        Self {
            due: 12.0,
            priority_high: 6.0,
            priority_medium: 3.0,
            priority_low: 1.0,
            project: 1.0,
            scheduled: 5.0,
            gravity: 5.0,
            age: 2.0,
            blocked: -8.0,
            blocking: 8.0,
            tags: 0.5,
        }
    }
}

impl UrgencyWeights {
    pub fn as_storage_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    // Falls back to Default on anything unparseable (empty storage, a
    // shape from a future version with fields this build doesn't know
    // about, corrupted JSON) rather than erroring — same "degrade
    // gracefully" posture as everything else that reads from localStorage
    // in this app.
    pub fn from_storage_string(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }
}

// One line per factor, in the same order as UrgencyWeights above, so a
// reader can hold the two side by side and see exactly which weight
// produced which number.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct UrgencyBreakdown {
    pub due: f64,
    pub priority: f64,
    pub project: f64,
    pub scheduled: f64,
    pub gravity: f64,
    pub age: f64,
    pub blocked: f64,
    pub blocking: f64,
    pub tags: f64,
}

impl UrgencyBreakdown {
    pub fn total(&self) -> f64 {
        self.due + self.priority + self.project + self.scheduled
            + self.gravity + self.age + self.blocked + self.blocking + self.tags
    }

    // Human-readable "why", e.g. "due +8.2 · priority +6.0 · blocking
    // +8.0" — every nonzero factor, largest first, so a hover tooltip (or
    // any future explain-this-ranking UI) can show it directly with no
    // extra bookkeeping. This is the answer to "why is this ranked here."
    pub fn describe(&self) -> String {
        let mut parts: Vec<(&str, f64)> = vec![
            ("due", self.due),
            ("priority", self.priority),
            ("project", self.project),
            ("scheduled", self.scheduled),
            ("gravity", self.gravity),
            ("age", self.age),
            ("blocked", self.blocked),
            ("blocking", self.blocking),
            ("tags", self.tags),
        ];
        parts.retain(|(_, v)| v.abs() > 0.001);
        parts.sort_by(|a, b| b.1.abs().partial_cmp(&a.1.abs()).unwrap_or(std::cmp::Ordering::Equal));
        if parts.is_empty() {
            return "no contributing factors".to_string();
        }
        parts.iter()
            .map(|(name, v)| format!("{name} {v:+.1}"))
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

// Every date-shaped field in this app is written in one of two shapes:
// full RFC3339 (chrono's `.to_rfc3339()`, used for created_at/completed_at)
// or the bare "YYYY-MM-DDTHH:MM" a <input type="datetime-local"> produces
// with no seconds or timezone (used for due_at/scheduled_at/until_at,
// whether set through the Advanced fold or quick-capture — see
// quick_capture.rs). Parsing only the former here was a real, silent bug:
// due-date scoring never actually fired for any due date ever set through
// the real UI, since nothing in the app produces full RFC3339 for that
// field. Treat the bare shape as UTC — good enough for a ranking nudge,
// not claiming timezone precision.
pub fn parse_moment_datetime(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|dt| dt.with_timezone(&Utc))
        .or_else(|| chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M").ok().map(|ndt| ndt.and_utc()))
}

// Taskwarrior's real "wait" semantic: a moment scheduled for the future
// (via scheduled:/wait: — same field, see quick_capture.rs) is hidden from
// every normal view — Inbox, an entity's own list, Priority, Due — until
// that date arrives, then behaves completely normally. The Scheduled view
// is the one place it's visible in the meantime, specifically so there's
// somewhere to go check on everything currently parked. Once the date
// passes, this returns false and the moment reappears everywhere on its
// own — no explicit "un-wait" action needed.
pub fn is_waiting(m: &MomentType, now: DateTime<Utc>) -> bool {
    m.metadata.as_ref()
        .and_then(|meta| meta.scheduled_at.as_deref())
        .and_then(parse_moment_datetime)
        .is_some_and(|scheduled| scheduled > now)
}

/// The single function responsible for computing a moment's priority
/// score. Change a ranking behavior here, and only here — everything else
/// (the Priority view, its settings UI) just calls this and displays the
/// result.
pub fn compute_urgency(
    m: &MomentType,
    all_moments: &[MomentType],
    now: DateTime<Utc>,
    weights: &UrgencyWeights,
) -> UrgencyBreakdown {
    let due_indicator = m.due_at.as_deref()
        .and_then(parse_moment_datetime)
        .map(|dt| {
            let days = (dt - now).num_seconds() as f64 / 86400.0;
            if days <= 0.0 {
                1.0
            } else if days <= 14.0 {
                1.0 - days / 14.0
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    let priority = m.metadata.as_ref().and_then(|meta| meta.priority.as_deref());
    let priority_contribution = match priority {
        Some("H") => weights.priority_high,
        Some("M") => weights.priority_medium,
        Some("L") => weights.priority_low,
        _ => 0.0,
    };

    let project_indicator = m.metadata.as_ref()
        .and_then(|meta| meta.project.as_deref())
        .map(|p| !p.is_empty())
        .unwrap_or(false);

    let scheduled_indicator = m.metadata.as_ref()
        .and_then(|meta| meta.scheduled_at.as_deref())
        .and_then(parse_moment_datetime)
        .map(|dt| dt <= now)
        .unwrap_or(false);

    let gravity_indicator = m.gravity.unwrap_or(0) as f64 / 100.0;

    let age_indicator = DateTime::parse_from_rfc3339(&m.created_at).ok()
        .map(|dt| {
            let days = (now - dt.with_timezone(&Utc)).num_seconds() as f64 / 86400.0;
            (days / 30.0).clamp(0.0, 1.0)
        })
        .unwrap_or(0.0);

    let blocked_indicator = match m.depends_on.as_deref() {
        Some(dep_id) => {
            let dep_done = all_moments.iter()
                .find(|x| x.id == dep_id)
                .map(|x| x.completed_at.is_some())
                .unwrap_or(true);
            !dep_done
        }
        None => false,
    };

    let blocking_indicator = all_moments.iter()
        .any(|x| x.depends_on.as_deref() == Some(m.id.as_str()) && x.completed_at.is_none());

    let tags_indicator = m.metadata.as_ref()
        .map(|meta| meta.tags.len().min(3) as f64)
        .unwrap_or(0.0);

    UrgencyBreakdown {
        due: due_indicator * weights.due,
        priority: priority_contribution,
        project: if project_indicator { weights.project } else { 0.0 },
        scheduled: if scheduled_indicator { weights.scheduled } else { 0.0 },
        gravity: gravity_indicator * weights.gravity,
        age: age_indicator * weights.age,
        blocked: if blocked_indicator { weights.blocked } else { 0.0 },
        blocking: if blocking_indicator { weights.blocking } else { 0.0 },
        tags: tags_indicator * weights.tags,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::MomentMetadata;

    fn base_moment() -> MomentType {
        MomentType {
            id: "1".into(),
            title: "Test".into(),
            description: None,
            gravity: Some(0),
            entity_id: "e1".into(),
            moment_type_id: 1,
            due_at: None,
            completed_at: None,
            deleted_at: None,
            reactions: None,
            created_at: Utc::now().to_rfc3339(),
            depends_on: None,
            metadata: None,
        }
    }

    #[test]
    fn overdue_task_gets_full_due_weight() {
        let mut m = base_moment();
        m.due_at = Some((Utc::now() - chrono::Duration::days(1)).to_rfc3339());
        let b = compute_urgency(&m, &[], Utc::now(), &UrgencyWeights::default());
        assert_eq!(b.due, UrgencyWeights::default().due);
    }

    #[test]
    // Regression test: due_at/scheduled_at are actually written in the bare
    // "YYYY-MM-DDTHH:MM" shape a <input type="datetime-local"> produces
    // (see quick_capture.rs and ab_task_cmp), not full RFC3339 — a version
    // of this function that only parsed RFC3339 would silently score every
    // real due date as "no due date at all."
    fn due_date_in_the_actual_datetime_local_shape_still_scores() {
        let mut m = base_moment();
        m.due_at = Some((Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%dT%H:%M").to_string());
        let b = compute_urgency(&m, &[], Utc::now(), &UrgencyWeights::default());
        assert_eq!(b.due, UrgencyWeights::default().due);
    }

    #[test]
    fn far_future_due_date_contributes_nothing() {
        let mut m = base_moment();
        m.due_at = Some((Utc::now() + chrono::Duration::days(30)).to_rfc3339());
        let b = compute_urgency(&m, &[], Utc::now(), &UrgencyWeights::default());
        assert_eq!(b.due, 0.0);
    }

    #[test]
    fn high_priority_contributes_exactly_its_weight() {
        let mut m = base_moment();
        m.metadata = Some(MomentMetadata { priority: Some("H".to_string()), ..Default::default() });
        let weights = UrgencyWeights::default();
        let b = compute_urgency(&m, &[], Utc::now(), &weights);
        assert_eq!(b.priority, weights.priority_high);
    }

    #[test]
    fn blocked_task_gets_the_blocked_weight() {
        let mut blocker = base_moment();
        blocker.id = "blocker".into();
        let mut m = base_moment();
        m.depends_on = Some("blocker".to_string());
        let weights = UrgencyWeights::default();
        let b = compute_urgency(&m, &[blocker], Utc::now(), &weights);
        assert_eq!(b.blocked, weights.blocked);
    }

    #[test]
    fn completed_blocker_means_no_blocked_penalty() {
        let mut blocker = base_moment();
        blocker.id = "blocker".into();
        blocker.completed_at = Some(Utc::now().to_rfc3339());
        let mut m = base_moment();
        m.depends_on = Some("blocker".to_string());
        let b = compute_urgency(&m, &[blocker], Utc::now(), &UrgencyWeights::default());
        assert_eq!(b.blocked, 0.0);
    }

    #[test]
    fn blocking_open_work_gets_the_blocking_bonus() {
        let mut m = base_moment();
        m.id = "blocker".into();
        let mut dependent = base_moment();
        dependent.id = "dependent".into();
        dependent.depends_on = Some("blocker".to_string());
        let weights = UrgencyWeights::default();
        let b = compute_urgency(&m, &[dependent], Utc::now(), &weights);
        assert_eq!(b.blocking, weights.blocking);
    }

    #[test]
    fn future_scheduled_date_contributes_nothing_yet() {
        let mut m = base_moment();
        m.metadata = Some(MomentMetadata {
            scheduled_at: Some((Utc::now() + chrono::Duration::days(5)).format("%Y-%m-%dT%H:%M").to_string()),
            ..Default::default()
        });
        let b = compute_urgency(&m, &[], Utc::now(), &UrgencyWeights::default());
        assert_eq!(b.scheduled, 0.0);
    }

    #[test]
    fn past_scheduled_date_is_active_and_contributes() {
        let mut m = base_moment();
        m.metadata = Some(MomentMetadata {
            scheduled_at: Some((Utc::now() - chrono::Duration::days(1)).format("%Y-%m-%dT%H:%M").to_string()),
            ..Default::default()
        });
        let weights = UrgencyWeights::default();
        let b = compute_urgency(&m, &[], Utc::now(), &weights);
        assert_eq!(b.scheduled, weights.scheduled);
    }

    #[test]
    fn tags_are_capped_at_three() {
        let mut m = base_moment();
        m.metadata = Some(MomentMetadata {
            tags: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
            ..Default::default()
        });
        let weights = UrgencyWeights::default();
        let b = compute_urgency(&m, &[], Utc::now(), &weights);
        assert_eq!(b.tags, 3.0 * weights.tags);
    }

    #[test]
    fn zero_weight_disables_a_factor_entirely() {
        let mut m = base_moment();
        m.gravity = Some(80);
        let mut weights = UrgencyWeights::default();
        weights.gravity = 0.0;
        let b = compute_urgency(&m, &[], Utc::now(), &weights);
        assert_eq!(b.gravity, 0.0);
    }

    #[test]
    fn describe_lists_only_nonzero_factors_largest_first() {
        let mut m = base_moment();
        m.metadata = Some(MomentMetadata { priority: Some("H".to_string()), ..Default::default() });
        let b = compute_urgency(&m, &[], Utc::now(), &UrgencyWeights::default());
        let d = b.describe();
        assert!(d.contains("priority"));
        assert!(!d.contains("due "));
    }
}
