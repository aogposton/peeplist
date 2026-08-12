use dioxus::prelude::*;
use chrono::Datelike;
use crate::types::*;
use crate::theme::*;
use crate::AppState;
use crate::ABView;
use crate::View;
use crate::api::{ActiveStorage, is_self_entity};
use crate::components::{GraphViewCmp, CheckboxCmp};
use lumen_blocks::components::avatar::{Avatar, AvatarFallback};
use lumen_blocks::components::button::{Button, ButtonVariant, ButtonSize};

fn stat_row(label: &str, value: &str) -> Element {
    rsx! {
        div {
            class: "flex justify-between items-center text-sm py-1.5",
            span { class: "text-muted-foreground", "{label}" }
            span { class: "text-foreground font-medium", "{value}" }
        }
    }
}


// Distance: arbitrary units measuring how far a relationship has drifted.
// Every entity starts BASE_DISTANCE units away. From there it grows at
// `drift` units/day — but the clock resets to zero every time you actually
// touch this entity (log a moment about them, or complete one), rather than
// counting up from whenever the entity was first added. Anchoring growth to
// created_at (the pre-2026-08-07 behavior) was the bug behind a real report:
// someone met with yesterday looked farther away than someone with a single
// note added today, because the frequently-contacted entity kept aging from
// its original add date regardless of the contact, while the brand new one
// started fresh at day zero. Anchoring to "last touched" instead means
// staying in regular contact keeps you near BASE_DISTANCE indefinitely, and
// only actual silence lets the daily drift add up.
//
// Distance closes back up (moves toward the center) from two kinds of
// effect, both additive on top of the base+growth number above:
//   - Flat baselines, unconditional on gravity: every logged moment (note or
//     task alike — logging something you learned about someone counts, not
//     just completing a task) nudges closer by MOMENT_BASELINE. A completed
//     task/promise gets a second, larger COMPLETION_BASELINE on top, since
//     following through is a stronger signal than having just logged intent.
//   - Signed gravity/reaction values (2026-08-07 — previously `.abs()`'d, so
//     a bad interaction closed distance exactly like a good one). Positive
//     pulls closer, negative pushes farther away, same divisor either way.
// GRAVITY_DISTANCE_DIVISOR/MOMENT_BASELINE/COMPLETION_BASELINE are first-
// draft tuning knobs, not final. Never negative overall (floored at 0).
const BASE_DISTANCE: f64 = 10.0;
const DEFAULT_DRIFT: f64 = 1.0;
const GRAVITY_DISTANCE_DIVISOR: f64 = 20.0;
const MOMENT_BASELINE: f64 = 3.0;
const COMPLETION_BASELINE: f64 = 4.0;

pub(crate) fn compute_distance(entity: &EntityType, moments: &[MomentType], now: chrono::DateTime<chrono::Utc>) -> f64 {
    let created = chrono::DateTime::parse_from_rfc3339(&entity.created_at)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let Some(created) = created else { return BASE_DISTANCE; };

    // involves_entity, not a plain entity_id equality check — a multi-entity
    // moment (2026-07-29) counts fully toward every entity it's attached to,
    // including closing their Distance same as a moment solely theirs would.
    let entity_moments: Vec<&MomentType> = moments.iter().filter(|m| m.involves_entity(&entity.id)).collect();

    // Most recent time you engaged with this entity at all — logging a
    // moment or completing one both count. Falls back to the entity's own
    // created_at when there's no moment yet, so a brand new zero-moment
    // entity still grows from the day it was added (this is also what keeps
    // backdated_created_at_for_distance's individuation math correct: with
    // no moments, this reduces to exactly the old created_at-anchored
    // formula it was built to solve).
    let last_touch = entity_moments.iter()
        .flat_map(|m| [Some(m.created_at.as_str()), m.completed_at.as_deref()])
        .flatten()
        .filter_map(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .max()
        .unwrap_or(created);

    let days_since_touch = ((now - last_touch).num_seconds() as f64 / 86400.0).max(0.0);
    let drift = if entity.drift > 0.0 { entity.drift } else { DEFAULT_DRIFT };
    let grown = BASE_DISTANCE + drift * days_since_touch;

    let moment_baseline = entity_moments.len() as f64 * MOMENT_BASELINE;
    let completion_baseline = entity_moments.iter()
        .filter(|m| (m.moment_type_id == 1i64 || m.moment_type_id == 2i64) && m.completed_at.is_some())
        .count() as f64 * COMPLETION_BASELINE;

    let signed_gravity: f64 = entity_moments.iter()
        .map(|m| m.gravity.unwrap_or(0) as f64 / GRAVITY_DISTANCE_DIVISOR)
        .sum();
    let signed_reactions: f64 = entity_moments.iter()
        .flat_map(|m| m.reactions.iter().flatten())
        .map(|r| r.value as f64 / GRAVITY_DISTANCE_DIVISOR)
        .sum();

    (grown - moment_baseline - completion_baseline - signed_gravity - signed_reactions).max(0.0)
}

// Individuation (splitting a person out of a group entity, see memory
// project_entity_individuation) — per explicit user decision 2026-07-22:
// the new individual entity "gets the exact same drift/distance as the
// group has. They start AT the full entity's distance then drift towards
// and away after." A brand-new entity has zero moments of its own (no
// automatic moment transfer — that's a separate, not-yet-designed piece of
// individuation), so its distance is just BASE_DISTANCE + drift *
// days_elapsed with nothing closed. Backdating created_at is what
// reproduces `target_distance` under that formula — same drift rate, same
// starting point, drifting independently from there on. If target_distance
// is already below BASE_DISTANCE (a heavily-engaged group, closed gravity
// pulling it down further than a zero-moment entity ever could on its own),
// this floors at days_elapsed = 0 — the closest honest approximation
// without inventing moment history that doesn't exist.
pub(crate) fn backdated_created_at_for_distance(target_distance: f64, drift: f64, now: chrono::DateTime<chrono::Utc>) -> String {
    let drift = if drift > 0.0 { drift } else { DEFAULT_DRIFT };
    let days_elapsed = ((target_distance - BASE_DISTANCE) / drift).max(0.0);
    let seconds = (days_elapsed * 86400.0) as i64;
    (now - chrono::Duration::seconds(seconds)).to_rfc3339()
}

// entity.drift is a rate (days of Distance growth per day since the
// relationship was added — see compute_distance above), not a contact-
// interval expectation the way "recur every 7 days" would be. Labeling its
// magnitude in words is honest; claiming a specific "expected every ~N
// days" cadence from it wouldn't be, since that number isn't actually
// derived from contact frequency. Buckets are first-draft, same as
// urgency's coefficients.
fn drift_label(drift: f64) -> &'static str {
    match drift {
        d if d < 1.0 => "Very attentive",
        d if d < 3.0 => "Steady",
        d if d < 6.0 => "Drifts quickly",
        _ => "Drifts fast",
    }
}

// Was "Last contact" — wrong claim. Creating a moment isn't proof you
// actually interacted with the entity it's attached to, only a *completed*
// one is even a reasonable proxy for that. Renamed and recomputed off
// completed_at instead of created_at (see DistanceViewCmp's rows below).
fn days_ago_label(dt: Option<chrono::DateTime<chrono::Utc>>, now: chrono::DateTime<chrono::Utc>) -> String {
    match dt {
        None => "No completed moments yet".to_string(),
        Some(dt) => match (now - dt).num_days() {
            0 => "Last completed moment: today".to_string(),
            1 => "Last completed moment: 1 day ago".to_string(),
            d => format!("Last completed moment: {d} days ago"),
        },
    }
}

// A ranked list of everyone you're tracking, closest first — reuses
// compute_distance (already built for Graph View/Stats)
// rather than adding a new metric.

#[derive(Clone, Copy, PartialEq)]
enum AllEntitiesMode {
    Distance,
    Graph,
}

// Replaces the old separate Graph View / Distance sidebar entries
// (2026-07-23) — see View::AllEntities's doc comment in main.rs for why:
// the sidebar's own Entities list now auto-hides anyone with nothing
// currently active, so this is the one place guaranteed to always show
// literally everyone regardless of that filter, with a plain local toggle
// between the two ways of looking at "everyone" instead of two separate
// sidebar links.
#[component]
pub fn AllEntitiesViewCmp() -> Element {
    let state = use_context::<AppState>();
    let is_desktop_viewport = state.is_desktop_viewport;
    let sidebar_collapsed = state.sidebar_collapsed;
    // See views/home.rs's heading_top_pad — same fixed-hamburger overlap,
    // same fix; this view builds its own heading instead of going through
    // Home's match arms.
    let heading_top_pad = if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "pt-4" } else { "pt-16" };
    let mut mode = use_signal(|| AllEntitiesMode::Distance);
    let tab_class = |active: bool| if active {
        "px-3 py-1.5 text-sm font-medium rounded-md bg-muted text-foreground cursor-pointer"
    } else {
        "px-3 py-1.5 text-sm font-medium rounded-md text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
    };
    rsx! {
        div {
            class: "px-4 {heading_top_pad}",
            div {
                class: "flex items-start justify-between gap-3 mb-4",
                div {
                    h1 { class: "text-2xl font-semibold text-foreground mb-1", "All Entities" }
                    p { class: "text-sm text-muted-foreground", "Everyone you're tracking, regardless of what's currently active for them." }
                }
                div {
                    class: "flex items-center gap-1 shrink-0",
                    span { class: tab_class(*mode.read() == AllEntitiesMode::Distance), onclick: move |_| mode.set(AllEntitiesMode::Distance), "Distance" }
                    span { class: tab_class(*mode.read() == AllEntitiesMode::Graph), onclick: move |_| mode.set(AllEntitiesMode::Graph), "Graph" }
                }
            }
        }
        if *mode.read() == AllEntitiesMode::Distance {
            DistanceViewCmp { }
        } else {
            GraphViewCmp { }
        }
    }
}

#[component]
pub fn DistanceViewCmp() -> Element {
    let state = use_context::<AppState>();
    let entities = state.entities;
    let moments = state.moments;
    let mut current_entity = state.current_entity;
    let mut current_view = state.currentView;

    let now = chrono::Utc::now();
    let all_moments = moments.read().clone();

    struct Row {
        entity: EntityType,
        distance: f64,
        last_completed: Option<chrono::DateTime<chrono::Utc>>,
        reaction_score: i32,
    }

    let mut rows: Vec<Row> = entities.read().iter()
        .filter(|e| !is_self_entity(e))
        .map(|e| {
            let entity_moments: Vec<&MomentType> = all_moments.iter().filter(|m| m.involves_entity(&e.id)).collect();
            let last_completed = entity_moments.iter()
                .filter_map(|m| m.completed_at.as_deref())
                .filter_map(|dt| chrono::DateTime::parse_from_rfc3339(dt).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .max();
            let reaction_score: i32 = entity_moments.iter()
                .filter_map(|m| m.reactions.as_ref())
                .flat_map(|rs| rs.iter())
                .map(|r| r.value)
                .sum();
            Row {
                distance: compute_distance(e, &all_moments, now),
                entity: e.clone(),
                last_completed,
                reaction_score,
            }
        })
        .collect();
    // Closest first (ascending) — 2026-07-23, was descending (farthest-
    // drifted first, Priority-view style). Matches the same closest-first
    // direction as the Graph View's own distance mapping.
    rows.sort_by(|a, b| a.distance.partial_cmp(&b.distance).unwrap_or(std::cmp::Ordering::Equal));

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            if rows.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-8",
                    "No one to show yet — add a person to start tracking."
                }
            } else {
                for row in rows.iter() {
                    div {
                        key: "{row.entity.id}",
                        class: "flex items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors",
                        onclick: {
                            let entity = row.entity.clone();
                            move |_| {
                                current_entity.set(Some(entity.clone()));
                                current_view.set(View::Entity);
                            }
                        },
                        div {
                            class: "flex flex-col min-w-0",
                            span { class: "text-sm font-medium text-foreground truncate", "{row.entity.name}" }
                            span { class: "text-xs text-muted-foreground", "{days_ago_label(row.last_completed, now)} · {drift_label(row.entity.drift)}" }
                        }
                        div {
                            class: "flex items-center gap-3 shrink-0",
                            span {
                                class: "text-xs text-muted-foreground",
                                title: "Total reaction value logged for this person",
                                "reactions: {row.reaction_score}"
                            }
                            span {
                                class: "text-xs font-semibold px-2 py-0.5 rounded-full border border-border text-muted-foreground",
                                title: "Distance",
                                "{row.distance:.1}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn entity_view_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut current_entity = state.current_entity;
    let entities = state.entities;
    let tag_filter = state.tag_filter;
    let project_filter = state.project_filter;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut activity_bar_view = state.activity_bar_view;
    let mut backdropTgl = state.backdropTgl;

    let entity = current_entity();
    // Individuation provenance (see components::sidebar::entity_list_cmp's
    // "Individuate" action, and EntityType::parent_entity_id) — resolved by
    // name here rather than shown as a raw id, and only shown at all if the
    // parent hasn't itself been deleted since.
    let parent = entity.as_ref()
        .and_then(|e| e.parent_entity_id.as_deref())
        .and_then(|parent_id| entities.read().iter().find(|e| e.id == parent_id).cloned());
    // A tag or project is a real list, not just a filter — its name belongs
    // at the top the same way an entity's does, instead of a generic "All"
    // that hides which list you're actually looking at.
    let name = entity.as_ref().map(|e| e.name.clone())
        .or_else(|| tag_filter.read().clone())
        .or_else(|| project_filter.read().clone())
        .unwrap_or_else(|| "All".to_string());
    let initial = name.chars().next().unwrap_or('?').to_uppercase().to_string();

    let tab_variant = |open: bool| if open { ButtonVariant::Secondary } else { ButtonVariant::Ghost };
    let info_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::Info;
    let story_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::Story;
    let stats_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::Stats;
    let momentos_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::Momentos;

    rsx! {
        div {
            class: "flex flex-col items-center gap-3 px-6 pt-6 pb-4 border-b border-border",
            Avatar {
                class: "h-14 w-14",
                AvatarFallback { class: "text-lg", "{initial}" }
            }
            h1 {
                class: "text-2xl font-semibold text-foreground",
                "{name}"
            }
            if let Some(parent) = parent.clone() {
                a {
                    class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer -mt-2",
                    onclick: move |_| current_entity.set(Some(parent.clone())),
                    "Split from {parent.name}"
                }
            }
            if entity.is_some() {
                div {
                    class: "flex gap-1.5",
                    Button {
                        variant: tab_variant(info_active),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            if info_active {
                                activity_bar_tgl.set(false);
                                backdropTgl.set(false);
                            } else {
                                activity_bar_view.set(ABView::Info);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        "Info"
                    }
                    Button {
                        variant: tab_variant(story_active),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            if story_active {
                                activity_bar_tgl.set(false);
                                backdropTgl.set(false);
                            } else {
                                activity_bar_view.set(ABView::Story);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        "Story"
                    }
                    Button {
                        variant: tab_variant(stats_active),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            if stats_active {
                                activity_bar_tgl.set(false);
                                backdropTgl.set(false);
                            } else {
                                activity_bar_view.set(ABView::Stats);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        "Stats"
                    }
                    Button {
                        variant: tab_variant(momentos_active),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            if momentos_active {
                                activity_bar_tgl.set(false);
                                backdropTgl.set(false);
                            } else {
                                activity_bar_view.set(ABView::Momentos);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        "Momentos"
                    }
                }
            }
        }
    }
}

#[component]
pub fn ab_story_cmp() -> Element {
    let state = use_context::<AppState>();
    let current_entity = state.current_entity;
    let moments = state.moments;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let entity = current_entity.read().clone();
    let entity_name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
    let entity_id = entity.as_ref().map(|e| e.id.clone());

    // Chronological "story" order: a moment's place in the timeline is when it
    // happened — its completion — not when it was created. Notes never complete,
    // so they fall back to when they were entered; the same fallback covers
    // still-open tasks/promises so they show up where they were introduced
    // rather than being dropped from the timeline.
    let timeline_key = |m: &MomentType| m.completed_at.clone().unwrap_or_else(|| m.created_at.clone());

    // involves_entity, not a plain entity_id equality check — a moment
    // co-attributed to two entities (e.g. "@Zerrick and @Breanna... started a
    // book club") was only ever showing up in the primary entity's Story,
    // never the additional one's (2026-08-07 bug report).
    let mut entity_moments = moments.read().iter()
        .filter(|m| entity_id.as_deref().is_some_and(|id| m.involves_entity(id)))
        .cloned()
        .collect::<Vec<_>>();
    entity_moments.sort_by(|a, b| timeline_key(a).cmp(&timeline_key(b)));

    let kind_label = |t: i64| match t {
        2i64 => "Promise",
        3i64 => "Note",
        4i64 => "Momento",
        5i64 => "Info",
        _ => "Task",
    };

    let fmt_ts = |s: &str| -> String { s.chars().take(16).collect() };

    rsx! {
        div {
            class: "flex flex-col h-full bg-background",
            div {
                class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                span {
                    class: "text-sm font-medium text-muted-foreground",
                    "Story — {entity_name}"
                }
                button {
                    class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                    onclick: move |_| {
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
                    },
                    "×"
                }
            }

            div {
                class: "flex flex-col gap-3 px-4 py-4 pb-[200px] overflow-y-auto flex-1 min-h-0",
                if entity_moments.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground text-center py-8",
                        "No story yet for {entity_name}."
                    }
                } else {
                    for m in entity_moments.iter() {
                        div {
                            class: "rounded-lg border border-border p-3",
                            div {
                                class: "flex items-start justify-between gap-2",
                                span {
                                    class: "text-sm font-medium text-foreground",
                                    "{m.title}"
                                }
                                span {
                                    class: "text-xs shrink-0 px-2 py-0.5 rounded-full border border-border text-muted-foreground",
                                    "{kind_label(m.moment_type_id)}"
                                }
                            }
                            if let Some(desc) = m.description.clone() {
                                if !desc.is_empty() {
                                    div {
                                        class: "text-xs text-muted-foreground mt-1 whitespace-pre-wrap",
                                        "{desc}"
                                    }
                                }
                            }
                            div {
                                class: "flex items-center gap-2 mt-1.5 text-xs text-muted-foreground",
                                if let Some(completed) = m.completed_at.clone() {
                                    span { "Completed {fmt_ts(&completed)}" }
                                } else if m.moment_type_id == 3i64 {
                                    span { "Added {fmt_ts(&m.created_at)}" }
                                } else if let Some(due) = m.due_at.clone() {
                                    span { "Due {fmt_ts(&due)}" }
                                } else {
                                    span { "Added {fmt_ts(&m.created_at)}" }
                                }
                                if m.gravity.unwrap_or(0) != 0 {
                                    span {
                                        class: "px-1.5 py-0.5 rounded border border-border",
                                        "Gravity {m.gravity.unwrap_or(0)}"
                                    }
                                }
                            }
                            if let Some(reactions) = m.reactions.clone() {
                                if !reactions.is_empty() {
                                    div {
                                        class: "mt-2 pt-2 border-t border-border flex flex-col gap-1.5",
                                        for r in reactions.iter() {
                                            div {
                                                class: "ml-4 pl-3 border-l-2 border-border flex items-center justify-between gap-2 text-xs",
                                                span { class: "text-foreground", "{r.description}" }
                                                span { class: "text-muted-foreground shrink-0", "{r.value}" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Momentos (2026-08-02) — an entity's recurring/personal moments (birthdays,
// anniversaries, "call every Sunday"). Each momento is one MomentType
// "template" row (moment_type_id 4): its own `due_at` is the RRULE's anchor
// date, `metadata.recurrence_rule` the RRULE string — see src/momento.rs for
// the expansion logic every list below goes through, and MomentMetadata's
// own doc comment (types.rs) for why occurrences are never real rows.
#[component]
pub fn ab_momentos_cmp() -> Element {
    let state = use_context::<AppState>();
    let current_entity = state.current_entity;
    let mut moments = state.moments;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let entity = current_entity.read().clone();
    let entity_name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
    let entity_id = entity.as_ref().map(|e| e.id.clone());
    let today = chrono::Utc::now().date_naive();

    let mut momentos: Vec<MomentType> = moments.read().iter()
        .filter(|m| entity_id.as_deref().is_some_and(|id| m.involves_entity(id)) && m.moment_type_id == 4i64)
        .cloned()
        .collect();
    momentos.sort_by(|a, b| a.title.cmp(&b.title));

    let mut new_title = use_signal(String::new);
    // The RRULE's DTSTART anchor — the first occurrence, and (for Weekly/
    // Monthly/Yearly) which day-of-week/day-of-month/day-of-year the
    // pattern repeats on. Labeled "Start date" in the form now — it was
    // unclear before whether this was a start or stop date.
    let mut new_start = use_signal(String::new);
    // Optional RRULE UNTIL — when set, the series stops producing
    // occurrences after this date. Deliberately a separate concept from a
    // regular moment's `until_at` (components::moment's Missed-view field,
    // urgency::is_missed) despite the similar name: this is baked directly
    // into the momento's own recurrence_rule string as UNTIL=..., not
    // metadata.until_at.
    let mut new_stop_date = use_signal(String::new);
    let mut new_rule = use_signal(String::new);
    let mut new_reveal = use_signal(String::new);
    let mut create_error = use_signal(|| None::<String>);
    // Which repeat frequency is currently selected — drives both button
    // highlighting and whether the weekly day-picker shows at all (only
    // when Weekly is the active choice, not unconditionally). None until
    // something's picked; new_rule stays empty until then too.
    let mut frequency_mode = use_signal(|| None::<&'static str>);
    let mut weekly_days = use_signal(Vec::<chrono::Weekday>::new);

    fn day_code(day: chrono::Weekday) -> &'static str {
        use chrono::Weekday::*;
        match day {
            Mon => "MO", Tue => "TU", Wed => "WE", Thu => "TH", Fri => "FR", Sat => "SA", Sun => "SU",
        }
    }
    fn day_label(day: chrono::Weekday) -> &'static str {
        use chrono::Weekday::*;
        match day {
            Mon => "Mon", Tue => "Tue", Wed => "Wed", Thu => "Thu", Fri => "Fri", Sat => "Sat", Sun => "Sun",
        }
    }
    const WEEK_DAYS: [chrono::Weekday; 7] = {
        use chrono::Weekday::*;
        [Mon, Tue, Wed, Thu, Fri, Sat, Sun]
    };
    const FREQUENCY_BUTTONS: [&str; 4] = ["Daily", "Weekly", "Monthly", "Yearly"];

    fn base_rule_for(mode: &str, byday: &str) -> Option<String> {
        match mode {
            "Daily" => Some("FREQ=DAILY".to_string()),
            "Weekly" => if byday.is_empty() { None } else { Some(format!("FREQ=WEEKLY;BYDAY={byday}")) },
            "Monthly" => Some("FREQ=MONTHLY".to_string()),
            "Yearly" => Some("FREQ=YEARLY".to_string()),
            _ => None,
        }
    }

    // Recomputes new_rule from whatever's currently selected — called
    // after every change (frequency button, day toggle, stop date) so
    // new_rule always reflects the full current state rather than each
    // handler patching it ad hoc.
    let mut rebuild_rule = move || {
        let Some(mode) = *frequency_mode.read() else {
            new_rule.set(String::new());
            return;
        };
        let byday = weekly_days.read().iter().copied().map(day_code).collect::<Vec<_>>().join(",");
        let Some(base) = base_rule_for(mode, &byday) else {
            new_rule.set(String::new());
            return;
        };
        let stop = new_stop_date.read().clone();
        if stop.is_empty() {
            new_rule.set(base);
        } else {
            // Basic (no-dash/colon) form matching DTSTART's own formatting
            // (see momento::parse_rule_set) — end of that calendar day, so
            // the stop date's own occurrence still counts as included.
            let compact = stop.replace('-', "");
            new_rule.set(format!("{base};UNTIL={compact}T235959Z"));
        }
    };

    let mut toggle_weekly_day = move |day: chrono::Weekday| {
        let mut days = weekly_days.read().clone();
        if days.contains(&day) {
            days.retain(|d| *d != day);
        } else {
            days.push(day);
        }
        // Stable Mon->Sun order regardless of click order, so the RRULE's
        // BYDAY list is deterministic rather than reflecting click history.
        days.sort_by_key(|d| d.num_days_from_monday());
        weekly_days.set(days);
        rebuild_rule();
    };

    let create_momento = move |_| {
        let Some(eid) = entity_id.clone() else { return };
        let title = new_title.read().clone();
        let start = new_start.read().clone();
        let rule = new_rule.read().clone();
        let reveal = new_reveal.read().clone();
        if title.trim().is_empty() || start.is_empty() || rule.trim().is_empty() {
            create_error.set(Some("Title, start date, and a repeat frequency are all required.".to_string()));
            return;
        }
        create_error.set(None);
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let new_moment = NewMomentType {
                title: title.clone(),
                description: None,
                gravity: None,
                entity_id: eid,
                moment_type_id: 4,
                deleted_at: None,
            };
            match storage.create_moment(new_moment).await {
                Ok(mut created) => {
                    let due_at = format!("{start}T09:00");
                    let reveal_lead = if reveal.is_empty() { None } else { Some(reveal.clone()) };
                    let meta = MomentMetadata { recurrence_rule: Some(rule.clone()), reveal_lead, ..Default::default() };
                    let _ = storage.update_moment_field(created.id.clone(), "due_at", serde_json::json!(due_at)).await;
                    let _ = storage.update_moment_field(created.id.clone(), "metadata", serde_json::json!(meta)).await;
                    created.due_at = Some(due_at);
                    created.metadata = Some(meta);
                    moments.write().insert(0, created);
                    new_title.set(String::new());
                    new_start.set(String::new());
                    new_stop_date.set(String::new());
                    new_rule.set(String::new());
                    new_reveal.set(String::new());
                    frequency_mode.set(None);
                    weekly_days.set(Vec::new());
                }
                Err(e) => {
                    clog!("Error creating momento: {}", e);
                    create_error.set(Some(format!("Couldn't create that: {e}")));
                }
            }
        });
    };

    let toggle_completed = move |momento_id: String, date: String| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            crate::components::patch_moment_metadata(&storage, moments, momento_id, move |m| {
                if m.momento_completed_occurrences.contains(&date) {
                    m.momento_completed_occurrences.retain(|d| d != &date);
                } else {
                    m.momento_completed_occurrences.push(date);
                }
            }).await;
        });
    };

    // Deleting a single occurrence and "skipping" it are the same action
    // (2026-08-02 product decision) — both just exclude that one date from
    // the pattern going forward. The series itself continues; only this one
    // date drops out permanently.
    let delete_occurrence = move |momento_id: String, date: String| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            crate::components::patch_moment_metadata(&storage, moments, momento_id, move |m| {
                if !m.momento_excluded_occurrences.contains(&date) {
                    m.momento_excluded_occurrences.push(date);
                }
            }).await;
        });
    };

    // Deletes the whole momento template — every past and future occurrence,
    // not just one date. Distinct from delete_occurrence above.
    let delete_all_iterations = move |momento: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        let momento_id = momento.id.clone();
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.delete_moment(momento).await {
                Ok(()) => moments.write().retain(|m| m.id != momento_id),
                Err(e) => clog!("Error deleting momento: {}", e),
            }
        });
    };

    rsx! {
        div {
            class: "flex flex-col h-full bg-background",
            div {
                class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                span { class: "text-sm font-medium text-muted-foreground", "Momentos — {entity_name}" }
                button {
                    class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                    onclick: move |_| {
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
                    },
                    "×"
                }
            }
            div {
                class: "flex flex-col gap-3 px-4 py-4 pb-[200px] overflow-y-auto flex-1 min-h-0",
                if momentos.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground text-center py-4",
                        "No momentos yet for {entity_name}."
                    }
                } else {
                    for m in momentos.iter() {
                        {
                            let meta = m.metadata.clone().unwrap_or_default();
                            let occurrences = meta.recurrence_rule.as_deref()
                                .map(|_| crate::momento::next_occurrences(m.due_at.as_deref().unwrap_or_default(), &meta, today, 3))
                                .unwrap_or_default();
                            let rule_label = meta.recurrence_rule.as_deref().map(crate::momento::describe_rule).unwrap_or_default();
                            rsx! {
                                div {
                                    key: "{m.id}",
                                    class: "rounded-lg border border-border p-3",
                                    div {
                                        class: "flex items-start justify-between gap-2 mb-2",
                                        span { class: "text-sm font-medium text-foreground", "{m.title}" }
                                        div {
                                            class: "flex items-center gap-2 shrink-0",
                                            span {
                                                class: "text-xs px-2 py-0.5 rounded-full border border-border text-muted-foreground",
                                                "{rule_label}"
                                            }
                                            button {
                                                class: "text-xs text-destructive hover:underline cursor-pointer",
                                                onclick: {
                                                    let momento = m.clone();
                                                    move |_| delete_all_iterations(momento.clone())
                                                },
                                                "Delete all"
                                            }
                                        }
                                    }
                                    div {
                                        class: "flex flex-col gap-1.5",
                                        for occ in occurrences.iter() {
                                            {
                                                let date_str = occ.date.format("%Y-%m-%d").to_string();
                                                let momento_id = m.id.clone();
                                                let momento_id2 = m.id.clone();
                                                let date_str2 = date_str.clone();
                                                let date_str3 = date_str.clone();
                                                let completed = occ.completed;
                                                rsx! {
                                                    div {
                                                        key: "{date_str}",
                                                        class: "flex items-center justify-between gap-2 text-xs",
                                                        div {
                                                            class: "flex items-center gap-2",
                                                            CheckboxCmp {
                                                                checked: completed,
                                                                on_change: move |checked: bool| {
                                                                    let _ = checked;
                                                                    toggle_completed(momento_id.clone(), date_str2.clone())
                                                                },
                                                                disabled: false,
                                                            }
                                                            span {
                                                                class: if completed { "text-muted-foreground line-through" } else { "text-foreground" },
                                                                "{occ.date.format(\"%b %d, %Y\")}"
                                                            }
                                                        }
                                                        button {
                                                            class: "text-destructive hover:underline cursor-pointer shrink-0",
                                                            onclick: move |_| delete_occurrence(momento_id2.clone(), date_str3.clone()),
                                                            "Delete this"
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "rounded-lg border border-border p-3 flex flex-col gap-2 mt-2",
                    span { class: "text-sm font-medium text-foreground", "Add a momento" }
                    input {
                        r#type: "text",
                        placeholder: "Title (e.g. \"Call Mom\")",
                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                        value: "{new_title.read()}",
                        oninput: move |e| new_title.set(e.value()),
                    }
                    div {
                        class: "flex flex-col gap-1",
                        label { class: "text-xs text-muted-foreground", "Start date — the first occurrence" }
                        input {
                            r#type: "date",
                            lang: "en-US",
                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{new_start.read()}",
                            oninput: move |e| new_start.set(e.value()),
                        }
                    }
                    div {
                        class: "flex flex-col gap-1",
                        label { class: "text-xs text-muted-foreground", "Stop date (optional) — series ends after this date" }
                        input {
                            r#type: "date",
                            lang: "en-US",
                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{new_stop_date.read()}",
                            oninput: move |e| {
                                new_stop_date.set(e.value());
                                rebuild_rule();
                            },
                        }
                    }
                    div {
                        class: "flex flex-col gap-1",
                        label { class: "text-xs text-muted-foreground", "Repeat" }
                        div {
                            class: "flex flex-wrap gap-1.5",
                            for label in FREQUENCY_BUTTONS {
                                button {
                                    key: "{label}",
                                    class: if *frequency_mode.read() == Some(label) {
                                        "rounded-md border border-transparent bg-primary text-primary-foreground text-xs px-2 py-1 cursor-pointer"
                                    } else {
                                        "rounded-md border border-border bg-background text-xs px-2 py-1 hover:bg-muted transition-colors cursor-pointer"
                                    },
                                    onclick: move |_| {
                                        frequency_mode.set(Some(label));
                                        rebuild_rule();
                                    },
                                    "{label}"
                                }
                            }
                        }
                    }
                    if *frequency_mode.read() == Some("Weekly") {
                        div {
                            class: "flex flex-col gap-1",
                            label { class: "text-xs text-muted-foreground", "On these days" }
                            div {
                                class: "flex flex-wrap gap-1",
                                for day in WEEK_DAYS {
                                    button {
                                        key: "{day}",
                                        class: if weekly_days.read().contains(&day) {
                                            "rounded-md border border-transparent bg-primary text-primary-foreground text-xs px-2 py-1 cursor-pointer"
                                        } else {
                                            "rounded-md border border-border bg-background text-xs px-2 py-1 hover:bg-muted transition-colors cursor-pointer"
                                        },
                                        onclick: move |_| toggle_weekly_day(day),
                                        "{day_label(day)}"
                                    }
                                }
                            }
                        }
                    }
                    div {
                        class: "flex flex-col gap-1",
                        label { class: "text-xs text-muted-foreground", "Show in the moment list" }
                        select {
                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{new_reveal.read()}",
                            onchange: move |e| new_reveal.set(e.value()),
                            for (label, value) in crate::momento::reveal_presets() {
                                option { key: "{value}", value: "{value}", "{label}" }
                            }
                        }
                    }
                    if let Some(msg) = create_error.read().as_ref() {
                        p { class: "text-xs text-destructive", "{msg}" }
                    }
                    button {
                        class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-3 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer self-start",
                        onclick: create_momento,
                        "Add momento"
                    }
                }
            }
        }
    }
}

#[component]
pub fn ab_stats_cmp() -> Element {
    let state = use_context::<AppState>();
    let current_entity = state.current_entity;
    let moments = state.moments;
    let entities = state.entities;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let entity = current_entity.read().clone();
    let entity_name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
    let entity_id = entity.as_ref().map(|e| e.id.clone());

    // Promises kept/pending and reaction coverage, scoped to this entity's moments.
    let (promises_kept, promises_pending, tasks_with_reactions) = {
        let all = moments.read();
        let for_entity = all.iter().filter(|m| entity_id.as_deref().is_some_and(|id| m.involves_entity(id)));
        for_entity.fold((0usize, 0usize, 0usize), |(kept, pending, reacted), m| {
            let kept = kept + (m.moment_type_id == 2i64 && m.completed_at.is_some()) as usize;
            let pending = pending + (m.moment_type_id == 2i64 && m.completed_at.is_none()) as usize;
            let reacted = reacted + m.reactions.as_ref().is_some_and(|r| !r.is_empty()) as usize;
            (kept, pending, reacted)
        })
    };

    let distance_label = entity.as_ref().map(|e| {
        let d = compute_distance(e, &moments.read(), chrono::Utc::now());
        format!("{d:.1} (drift {:.0}d/unit)", e.drift)
    }).unwrap_or_else(|| "—".to_string());

    // Ranking: entities ordered by total moments logged, most active first.
    // Excludes the self entity (api::is_self_entity) — "your relationship
    // with yourself, ranked" isn't meaningful, and every moment posted with
    // no entity selected is attributed to it.
    let (entity_rank, total_entities) = {
        let all_moments = moments.read();
        let all_entities = entities.read();
        let mut counts: Vec<(String, usize)> = all_entities.iter()
            .filter(|e| !is_self_entity(e))
            .map(|e| (e.id.clone(), all_moments.iter().filter(|m| m.involves_entity(&e.id)).count()))
            .collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        let rank = entity_id.and_then(|id| counts.iter().position(|(eid, _)| *eid == id)).map(|pos| pos + 1);
        (rank, counts.len())
    };
    let ranking_label = match entity_rank {
        Some(rank) => format!("#{rank} out of {total_entities}"),
        None => "—".to_string(),
    };

    rsx! {
        div {
            class: "flex flex-col h-full bg-background",
            div {
                class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                span { class: "text-sm font-medium text-muted-foreground", "Stats — {entity_name}" }
                button {
                    class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                    onclick: move |_| {
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
                    },
                    "×"
                }
            }
            div {
                class: "flex flex-col gap-4 px-4 py-4 pb-[200px] overflow-y-auto flex-1 min-h-0",
                div {
                    class: "rounded-lg border border-border p-4",
                    h3 {
                        class: "text-sm font-semibold text-foreground mb-2 pb-2 border-b border-border",
                        "Relationship Details"
                    }
                    div {
                        class: "flex flex-col divide-y divide-border",
                        {stat_row("Promises kept", &promises_kept.to_string())}
                        {stat_row("Promises pending", &promises_pending.to_string())}
                        {stat_row("Tasks with reactions", &tasks_with_reactions.to_string())}
                        {stat_row("Distance", &distance_label)}
                    }
                }
                div {
                    class: "rounded-lg border border-border p-4",
                    h3 {
                        class: "text-sm font-semibold text-foreground mb-2 pb-2 border-b border-border",
                        "Superlatives"
                    }
                    div {
                        class: "flex flex-col divide-y divide-border",
                        {stat_row("Engagement ranking", &ranking_label)}
                    }
                }
            }
        }
    }
}

#[component]
pub fn ab_info_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut current_entity = state.current_entity;
    let mut entities = state.entities;
    let mut moments = state.moments;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;
    let mut entity_types = use_signal(|| vec![]);
    let mut confirming_delete = use_signal(|| false);
    let mut info_input = use_signal(String::new);

    use_effect(move || {
        // Reset the confirm step whenever a different entity's Info panel
        // is shown, so a stale "click again to confirm" doesn't carry over
        // and let a mis-click delete the wrong person.
        let _ = current_entity.read().as_ref().map(|e| e.id.clone());
        confirming_delete.set(false);
    });

    use_effect(move || {
        // Read synchronously so the effect actually reruns on vault switch —
        // see the matching comment in views/home.rs's fetch effect.
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.get_entity_types().await {
                Ok(data) => entity_types.set(data),
                Err(e) => clog!("Error fetching entity types: {}", e),
            }
        });
    });

    let entity = current_entity.read().clone();
    let entity_name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
    let type_name = entity.as_ref()
        .and_then(|e| e.entity_type_id.clone())
        .and_then(|type_id| entity_types.read().iter().find(|t| t.id == type_id).map(|t| t.name.clone()))
        .unwrap_or_else(|| "Not set".to_string());
    let known_since = entity.as_ref()
        .map(|e| e.created_at.chars().take(10).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());
    let drift_value = entity.as_ref().map(|e| e.drift).unwrap_or(2.0);

    // Self isn't deletable — there's no "unselect yourself" concept in this
    // app's model, and every un-attributed moment defaults to Self, so
    // removing it would just get silently recreated on next capture anyway.
    let is_deletable = entity.as_ref().map(|e| !is_self_entity(e)).unwrap_or(false);

    // "Delete this person" used to be hardcoded — now names the entity's
    // actual type (a pet's delete button should say "pet", not "person").
    // "Not set" (type_name's own fallback) reads badly as "Delete this Not
    // set", so this falls back to the generic "entity" instead.
    let delete_kind = if type_name == "Not set" { "entity".to_string() } else { type_name.to_lowercase() };

    // Free-form facts about this entity (2026-08-03) — replaces the old
    // fixed Relationship/How you met/Birthday/Location/Why they matter
    // fields with an open-ended list instead, per explicit user request:
    // "they can put it in themselves if it matters to them, no need for
    // the clutter." An Info item is just a Note (moment_type_id 5) hidden
    // from the normal moment flow — see MomentType::moment_type_id's doc
    // comment — filtered to this entity and shown newest-first.
    let info_items: Vec<MomentType> = entity.as_ref()
        .map(|e| {
            let mut items: Vec<MomentType> = moments.read().iter()
                .filter(|m| m.involves_entity(&e.id) && m.moment_type_id == 5i64)
                .cloned()
                .collect();
            items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            items
        })
        .unwrap_or_default();

    let mut submit_info = move || {
        let Some(entity_id) = current_entity.read().as_ref().map(|e| e.id.clone()) else { return; };
        let title = info_input.read().trim().to_string();
        if title.is_empty() {
            return;
        }
        info_input.set(String::new());
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let new_moment = NewMomentType {
                title,
                description: None,
                gravity: None,
                entity_id,
                moment_type_id: 5,
                deleted_at: None,
            };
            match storage.create_moment(new_moment).await {
                Ok(created) => moments.write().push(created),
                Err(e) => clog!("Error adding info: {}", e),
            }
        });
    };

    let delete_info_item = move |id: String| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let Some(m) = moments.read().iter().find(|m| m.id == id).cloned() else { return; };
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.delete_moment(m).await {
                Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                Err(e) => clog!("Error deleting info: {}", e),
            }
        });
    };

    rsx! {
        div {
            class: "flex flex-col h-full bg-background",
            div {
                class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                span { class: "text-sm font-medium text-muted-foreground", "Info — {entity_name}" }
                button {
                    class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                    onclick: move |_| {
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
                    },
                    "×"
                }
            }
            div {
                class: "flex flex-col gap-4 px-4 py-4 pb-[200px] overflow-y-auto flex-1 min-h-0",
                div {
                    class: "rounded-lg border border-border p-4",
                    div {
                        class: "flex flex-col divide-y divide-border",
                        {stat_row("Name", &entity_name)}
                        // Editable once an entity exists to edit (2026-08-01)
                        // — Type used to only be settable in the now-removed
                        // "Add Entity" modal, at creation time, with no way
                        // back in afterward. Entities are now always created
                        // bare (via @mention in the composer), so this is
                        // the only place Type can ever be set. Falls back to
                        // the plain read-only row for the "All" pseudo-view
                        // (entity is None there — nothing to edit).
                        if let Some(e) = entity.as_ref() {
                            div {
                                class: "flex justify-between items-center gap-3 text-sm py-1.5",
                                span { class: "text-muted-foreground shrink-0", "Type" }
                                select {
                                    class: "rounded-md border border-transparent hover:border-input focus:border-input bg-transparent text-right text-sm text-foreground px-2 py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    value: "{e.entity_type_id.clone().unwrap_or_default()}",
                                    oninput: {
                                        let entity_id = e.id.clone();
                                        move |ev: Event<FormData>| {
                                            let new_val = ev.value();
                                            let entity_id = entity_id.clone();
                                            let token = auth_token;
                                            let vault = active_vault;
                                            spawn(async move {
                                                let new_type = if new_val.is_empty() { None } else { Some(new_val.clone()) };
                                                let value = match &new_type {
                                                    Some(v) => serde_json::json!(v),
                                                    None => serde_json::Value::Null,
                                                };
                                                let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                match storage.update_entity_field(entity_id.clone(), "entity_type_id", value).await {
                                                    Ok(_) => {
                                                        let mut list = entities.write();
                                                        if let Some(ent) = list.iter_mut().find(|x| x.id == entity_id) {
                                                            ent.entity_type_id = new_type.clone();
                                                        }
                                                        if let Some(cur) = current_entity.write().as_mut() {
                                                            if cur.id == entity_id {
                                                                cur.entity_type_id = new_type;
                                                            }
                                                        }
                                                    }
                                                    Err(err) => log::info!("Error updating entity type: {}", err),
                                                }
                                            });
                                        }
                                    },
                                    option { value: "", "Not set" }
                                    // "Self" is a reserved marker, not a
                                    // real relationship type to hand-pick —
                                    // same exclusion the old modal used.
                                    for entity_type in entity_types.read().iter().filter(|t| t.id != crate::types::SELF_ENTITY_TYPE_ID) {
                                        option {
                                            value: "{entity_type.id}",
                                            "{entity_type.name}"
                                        }
                                    }
                                }
                            }
                        } else {
                            {stat_row("Type", &type_name)}
                        }
                        {stat_row("Known since", &known_since)}
                        if entity.is_some() {
                            div {
                                class: "flex justify-between items-center text-sm py-1.5",
                                span { class: "text-muted-foreground", "Drift (days/unit)" }
                                input {
                                    r#type: "number",
                                    step: "0.5",
                                    min: "0.1",
                                    class: "w-20 rounded-md border border-input bg-background text-sm text-foreground px-2 py-1 text-right focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    value: "{drift_value}",
                                    oninput: move |e| {
                                        let Some(entity_id) = current_entity.read().as_ref().map(|e| e.id.clone()) else { return; };
                                        let Ok(new_drift) = e.value().parse::<f64>() else { return; };
                                        let token = auth_token;
                                        let vault = active_vault;
                                        spawn(async move {
                                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                            match storage.update_entity_field(entity_id.clone(), "drift", serde_json::json!(new_drift)).await {
                                                Ok(_) => {
                                                    let mut list = entities.write();
                                                    if let Some(ent) = list.iter_mut().find(|x| x.id == entity_id) {
                                                        ent.drift = new_drift;
                                                    }
                                                    if let Some(cur) = current_entity.write().as_mut() {
                                                        if cur.id == entity_id {
                                                            cur.drift = new_drift;
                                                        }
                                                    }
                                                }
                                                Err(err) => log::info!("Error updating drift: {}", err),
                                            }
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
                if entity.is_some() {
                    div {
                        class: "rounded-lg border border-border p-4 flex flex-col gap-3",
                        span { class: "text-sm font-semibold text-foreground", "Info" }
                        if info_items.is_empty() {
                            div {
                                class: "text-sm text-muted-foreground text-center py-4",
                                "Nothing added yet."
                            }
                        } else {
                            div {
                                class: "flex flex-col divide-y divide-border",
                                for item in info_items.iter() {
                                    div {
                                        key: "{item.id}",
                                        class: "flex items-start justify-between gap-2 py-2",
                                        span { class: "text-sm text-foreground whitespace-pre-wrap", "{item.title}" }
                                        button {
                                            class: "text-xs text-muted-foreground hover:text-destructive shrink-0 cursor-pointer",
                                            title: "Delete",
                                            onclick: { let id = item.id.clone(); move |_| delete_info_item(id.clone()) },
                                            "×"
                                        }
                                    }
                                }
                            }
                        }
                        div {
                            class: "flex items-center gap-2",
                            input {
                                r#type: "text",
                                class: "flex-1 min-w-0 rounded-md border border-input bg-background text-sm text-foreground px-3 py-1.5 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                placeholder: "Add something worth remembering…",
                                value: "{info_input}",
                                oninput: move |e| info_input.set(e.value()),
                                onkeydown: move |e| {
                                    if e.key() == Key::Enter {
                                        submit_info();
                                    }
                                }
                            }
                            button {
                                class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-3 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer shrink-0",
                                onclick: move |_| submit_info(),
                                "Add"
                            }
                        }
                    }
                }
                if is_deletable {
                    div {
                        class: "rounded-lg border border-destructive/40 p-4 flex items-center justify-between gap-3",
                        div {
                            span { class: "text-sm font-medium text-foreground", "Delete this {delete_kind}" }
                            p {
                                class: "text-xs text-muted-foreground mt-0.5",
                                "Removes them from your vault. Their history goes to trash, not erased outright."
                            }
                        }
                        Button {
                            variant: ButtonVariant::Destructive,
                            on_click: move |_| {
                                if !*confirming_delete.read() {
                                    confirming_delete.set(true);
                                    return;
                                }
                                let Some(entity_id) = current_entity.read().as_ref().map(|e| e.id.clone()) else { return; };
                                let vault = *active_vault.read();
                                let token = auth_token.read().clone();
                                // Same fix as the sidebar's right-click delete
                                // (2026-07-23): Supabase's FK on moments.entity_id
                                // blocks deleting the entity while anything still
                                // references it — soft-deleting a moment doesn't
                                // clear that reference, only reassigning does. So
                                // every moment gets moved to Self first,
                                // unconditionally, then soft-deleted there (so
                                // "goes to trash, not erased outright" above stays
                                // true), before the entity itself is deleted.
                                let Some(self_id) = vault.effective(&token).resolve_self_entity_id(&entities.read()) else {
                                    clog!("Error deleting entity: couldn't resolve Self entity");
                                    return;
                                };
                                let to_move: Vec<MomentType> = moments.read().iter()
                                    .filter(|m| m.entity_id == entity_id)
                                    .cloned()
                                    .collect();
                                confirming_delete.set(false);
                                spawn(async move {
                                    let storage = ActiveStorage::for_vault(vault, token.clone());
                                    for m in &to_move {
                                        let mid = m.id.clone();
                                        if let Err(e) = storage.reassign_moment_entity(mid.clone(), self_id.clone()).await {
                                            clog!("Error reassigning moment before entity delete: {}", e);
                                            continue;
                                        }
                                        if let Some(mm) = moments.write().iter_mut().find(|mm| mm.id == mid) {
                                            mm.entity_id = self_id.clone();
                                        }
                                        match storage.delete_moment({ let mut m = m.clone(); m.entity_id = self_id.clone(); m }).await {
                                            Ok(()) => moments.write().retain(|mm| mm.id != mid),
                                            Err(e) => clog!("Error deleting moment during entity delete: {}", e),
                                        }
                                    }
                                    match storage.delete_entity(entity_id.clone()).await {
                                        Ok(()) => {
                                            entities.write().retain(|e| e.id != entity_id);
                                            current_entity.set(None);
                                            activity_bar_tgl.set(false);
                                            backdropTgl.set(false);
                                        }
                                        Err(e) => clog!("Error deleting entity: {}", e),
                                    }
                                });
                            },
                            if *confirming_delete.read() { "Click again to confirm" } else { "Delete" }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod individuation_tests {
    use super::*;

    fn entity_with(created_at: &str, drift: f64) -> EntityType {
        EntityType {
            id: "group".into(),
            name: "The Smiths".into(),
            entity_type_id: None,
            parent_entity_id: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            drift,
            metadata: None,
        }
    }

    // The whole point of backdated_created_at_for_distance: a brand-new,
    // zero-moment entity created with the returned timestamp should
    // immediately compute back to (approximately) the same distance the
    // original entity had at the moment of the split — same drift rate,
    // same starting point.
    #[test]
    fn backdated_entity_reproduces_the_original_distance() {
        let now = chrono::Utc::now();
        // An entity that's been drifting for 30 days at 2.0/day: distance =
        // 10 (BASE_DISTANCE) + 2*30 = 70.
        let original = entity_with(&(now - chrono::Duration::days(30)).to_rfc3339(), 2.0);
        let original_distance = compute_distance(&original, &[], now);
        assert!((original_distance - 70.0).abs() < 0.01, "expected ~70.0, got {original_distance}");

        let backdated = backdated_created_at_for_distance(original_distance, original.drift, now);
        let new_entity = entity_with(&backdated, original.drift);
        let new_distance = compute_distance(&new_entity, &[], now);

        assert!(
            (new_distance - original_distance).abs() < 0.01,
            "individuated entity's distance ({new_distance}) should match the original's ({original_distance}) at the moment of the split"
        );
    }

    // A heavily-engaged group (lots of closed gravity pulling distance
    // below what a zero-moment entity could ever reach) can't be perfectly
    // reproduced — floors at days_elapsed = 0, i.e. BASE_DISTANCE, the
    // closest honest approximation.
    #[test]
    fn target_distance_below_base_floors_at_zero_days_elapsed() {
        let now = chrono::Utc::now();
        let backdated = backdated_created_at_for_distance(3.0, 2.0, now);
        let new_entity = entity_with(&backdated, 2.0);
        let new_distance = compute_distance(&new_entity, &[], now);
        assert!((new_distance - BASE_DISTANCE).abs() < 0.01, "expected the BASE_DISTANCE floor, got {new_distance}");
    }

    // Same drift rate carries forward — a fast-drifting group's split-off
    // individual should keep drifting at the same rate, not reset to the
    // 2.0/day default.
    #[test]
    fn drift_rate_carries_over_unchanged() {
        let now = chrono::Utc::now();
        let original = entity_with(&(now - chrono::Duration::days(10)).to_rfc3339(), 5.0);
        let original_distance = compute_distance(&original, &[], now);
        let backdated = backdated_created_at_for_distance(original_distance, original.drift, now);

        let one_day_later = now + chrono::Duration::days(1);
        let new_entity = entity_with(&backdated, original.drift);
        let distance_one_day_later = compute_distance(&new_entity, &[], one_day_later);

        assert!(
            (distance_one_day_later - (original_distance + 5.0)).abs() < 0.01,
            "expected distance to grow by exactly the 5.0/day drift rate, got a delta of {}",
            distance_one_day_later - original_distance
        );
    }
}

#[cfg(test)]
mod distance_formula_tests {
    use super::*;

    fn entity_with(created_at: &str, drift: f64) -> EntityType {
        EntityType {
            id: "e1".into(),
            name: "Test Person".into(),
            entity_type_id: None,
            parent_entity_id: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            drift,
            metadata: None,
        }
    }

    fn moment_with(moment_type_id: i64, created_at: &str, completed_at: Option<&str>, gravity: Option<i32>) -> MomentType {
        MomentType {
            id: "m1".into(),
            title: "Test moment".into(),
            description: None,
            gravity,
            entity_id: "e1".into(),
            moment_type_id,
            due_at: None,
            completed_at: completed_at.map(|s| s.to_string()),
            deleted_at: None,
            reactions: None,
            created_at: created_at.to_string(),
            updated_at: created_at.to_string(),
            depends_on: None,
            metadata: None,
        }
    }

    // Positive gravity should now pull an entity closer (lower distance)
    // than the same entity with no gravity at all; negative should push it
    // farther. Pre-2026-08-07 both directions closed distance identically
    // because the raw value was `.abs()`'d away.
    #[test]
    fn signed_gravity_moves_distance_in_opposite_directions() {
        let now = chrono::Utc::now();
        let created = (now - chrono::Duration::days(5)).to_rfc3339();
        let entity = entity_with(&created, 1.0);

        let neutral = compute_distance(&entity, &[moment_with(1, &created, Some(&created), Some(0))], now);
        let positive = compute_distance(&entity, &[moment_with(1, &created, Some(&created), Some(60))], now);
        let negative = compute_distance(&entity, &[moment_with(1, &created, Some(&created), Some(-60))], now);

        assert!(positive < neutral, "positive gravity ({positive}) should be closer than neutral ({neutral})");
        assert!(negative > neutral, "negative gravity ({negative}) should be farther than neutral ({neutral})");
    }

    // Just logging a note (no gravity, never completed) should still nudge
    // distance closer than an entity with no moments at all — per explicit
    // user request: "if I learn something new about someone, baseline, i
    // did get slightly closer to them."
    #[test]
    fn logging_a_plain_note_closes_distance() {
        let now = chrono::Utc::now();
        let created = now.to_rfc3339();
        let entity = entity_with(&created, 1.0);

        let no_moments = compute_distance(&entity, &[], now);
        let with_note = compute_distance(&entity, &[moment_with(3, &created, None, None)], now);

        assert!(with_note < no_moments, "logging a note ({with_note}) should be closer than no moments ({no_moments})");
    }

    // Completing a task should close distance by more than just logging one
    // (and leaving it open) — the completion baseline stacks on top of the
    // flat per-moment baseline every logged moment gets.
    #[test]
    fn completing_a_task_closes_more_than_leaving_it_open() {
        let now = chrono::Utc::now();
        let created = now.to_rfc3339();
        let entity = entity_with(&created, 1.0);

        let open = compute_distance(&entity, &[moment_with(1, &created, None, Some(0))], now);
        let completed = compute_distance(&entity, &[moment_with(1, &created, Some(&created), Some(0))], now);

        assert!(completed < open, "a completed task ({completed}) should close distance more than an open one ({open})");
    }

    // The bug report this rebuild fixes: an old entity you still actively
    // talk to should stay close, not keep aging from whenever it was first
    // added. Growth now resets from the most recent touch, not created_at.
    #[test]
    fn recent_contact_beats_pure_age() {
        let now = chrono::Utc::now();
        let old_created = (now - chrono::Duration::days(60)).to_rfc3339();
        let yesterday = (now - chrono::Duration::days(1)).to_rfc3339();

        // Old entity, but talked to (completed a task) yesterday.
        let long_known_active = entity_with(&old_created, 1.0);
        let active_distance = compute_distance(
            &long_known_active,
            &[moment_with(1, &yesterday, Some(&yesterday), Some(0))],
            now,
        );

        // Brand new entity, added today, with a single note.
        let today = now.to_rfc3339();
        let brand_new = entity_with(&today, 1.0);
        let new_distance = compute_distance(&brand_new, &[moment_with(3, &today, None, None)], now);

        assert!(
            active_distance < new_distance,
            "an old entity actively talked to yesterday ({active_distance}) should be closer than a brand-new entity with one note ({new_distance})"
        );
    }
}
