use dioxus::prelude::*;
use crate::types::*;
use crate::theme::*;
use crate::AppState;
use crate::ABView;
use crate::View;
use crate::api::{ActiveStorage, is_self_entity};
use lumen_blocks::components::avatar::{Avatar, AvatarFallback};
use lumen_blocks::components::button::{Button, ButtonVariant, ButtonSize};
use lumen_blocks::components::input::Input;
use lumen_blocks::components::label::Label;

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
// Grows by 1 unit every `drift` days since the entity's last *completed*
// task/promise (logging or completing a note doesn't count — see the
// Distance/Drift spec). Falls back to the entity's created_at if it has
// never had a completed task/promise. Never negative.
// Every entity starts BASE_DISTANCE units away and grows further apart at
// `drift` units/day since the entity was created (day 1 = 10, day 2 = 12,
// day 3 = 14 for drift=2 — additive, not a ratio). Distance closes back up
// when something happens: a completed task/promise, a note that carries
// a non-zero gravity (logged-but-never-"completed" events, like someone
// bringing you flowers, still count), or reactions logged on ANY of the
// entity's moments (reactions are a lighter-weight, in-the-moment signal
// that isn't gated on moment type or completion the way gravity is).
// Closing amount is |gravity| (or reaction value) scaled down —
// GRAVITY_DISTANCE_DIVISOR is a first-draft tuning knob, not final.
const BASE_DISTANCE: f64 = 10.0;
const GRAVITY_DISTANCE_DIVISOR: f64 = 20.0;

pub(crate) fn compute_distance(entity: &EntityType, moments: &[MomentType], now: chrono::DateTime<chrono::Utc>) -> f64 {
    let created = chrono::DateTime::parse_from_rfc3339(&entity.created_at)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc));
    let Some(created) = created else { return BASE_DISTANCE; };

    let days_elapsed = ((now - created).num_seconds() as f64 / 86400.0).max(0.0);
    let drift = if entity.drift > 0.0 { entity.drift } else { 2.0 };
    let grown = BASE_DISTANCE + drift * days_elapsed;

    let entity_moments: Vec<&MomentType> = moments.iter().filter(|m| m.entity_id == entity.id).collect();

    let closed_gravity: f64 = entity_moments.iter()
        .filter(|m| {
            let completed_task_or_promise = (m.moment_type_id == 1i64 || m.moment_type_id == 2i64) && m.completed_at.is_some();
            let noteworthy_note = m.moment_type_id == 3i64 && m.gravity.unwrap_or(0) != 0;
            completed_task_or_promise || noteworthy_note
        })
        .map(|m| (m.gravity.unwrap_or(0).unsigned_abs() as f64) / GRAVITY_DISTANCE_DIVISOR)
        .sum();

    let closed_reactions: f64 = entity_moments.iter()
        .flat_map(|m| m.reactions.iter().flatten())
        .map(|r| (r.value.unsigned_abs() as f64) / GRAVITY_DISTANCE_DIVISOR)
        .sum();

    (grown - closed_gravity - closed_reactions).max(0.0)
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
    let drift = if drift > 0.0 { drift } else { 2.0 };
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

fn days_ago_label(dt: Option<chrono::DateTime<chrono::Utc>>, now: chrono::DateTime<chrono::Utc>) -> String {
    match dt {
        None => "No contact logged yet".to_string(),
        Some(dt) => match (now - dt).num_days() {
            0 => "Last contact: today".to_string(),
            1 => "Last contact: 1 day ago".to_string(),
            d => format!("Last contact: {d} days ago"),
        },
    }
}

// A Priority-style ranked list, but for relationships instead of tasks —
// who you've drifted furthest from, at a glance, without a special trip to
// Graph View. Reuses compute_distance (already built for Graph View/Stats)
// rather than adding a new metric.
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
        last_contact: Option<chrono::DateTime<chrono::Utc>>,
        reaction_score: i32,
    }

    let mut rows: Vec<Row> = entities.read().iter()
        .filter(|e| !is_self_entity(e))
        .map(|e| {
            let entity_moments: Vec<&MomentType> = all_moments.iter().filter(|m| m.entity_id == e.id).collect();
            let last_contact = entity_moments.iter()
                .filter_map(|m| chrono::DateTime::parse_from_rfc3339(&m.created_at).ok())
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
                last_contact,
                reaction_score,
            }
        })
        .collect();
    rows.sort_by(|a, b| b.distance.partial_cmp(&a.distance).unwrap_or(std::cmp::Ordering::Equal));

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
                            span { class: "text-xs text-muted-foreground", "{days_ago_label(row.last_contact, now)} · {drift_label(row.entity.drift)}" }
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
    let history_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::History;
    let stats_active = *activity_bar_tgl.read() && *activity_bar_view.read() == ABView::Stats;

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
                        variant: tab_variant(history_active),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            if history_active {
                                activity_bar_tgl.set(false);
                                backdropTgl.set(false);
                            } else {
                                activity_bar_view.set(ABView::History);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        "History"
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
                }
            }
        }
    }
}

#[component]
pub fn ab_history_cmp() -> Element {
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

    let mut entity_moments = moments.read().iter()
        .filter(|m| Some(m.entity_id.clone()) == entity_id)
        .cloned()
        .collect::<Vec<_>>();
    entity_moments.sort_by(|a, b| timeline_key(a).cmp(&timeline_key(b)));

    let kind_label = |t: i64| match t {
        2i64 => "Promise",
        3i64 => "Note",
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
                    "History — {entity_name}"
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
                class: "flex flex-col gap-3 px-4 py-4 overflow-y-auto flex-1 min-h-0",
                if entity_moments.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground text-center py-8",
                        "No history yet for {entity_name}."
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
        let for_entity = all.iter().filter(|m| Some(m.entity_id.clone()) == entity_id);
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
            .map(|e| (e.id.clone(), all_moments.iter().filter(|m| m.entity_id == e.id).count()))
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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1 min-h-0",
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

    let meta = entity.as_ref().and_then(|e| e.metadata.clone()).unwrap_or_default();
    let display_or_not_set = |s: &str| if s.trim().is_empty() { "Not set".to_string() } else { s.to_string() };
    // Self isn't deletable — there's no "unselect yourself" concept in this
    // app's model, and every un-attributed moment defaults to Self, so
    // removing it would just get silently recreated on next capture anyway.
    let is_deletable = entity.as_ref().map(|e| !is_self_entity(e)).unwrap_or(false);

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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1 min-h-0",
                div {
                    class: "rounded-lg border border-border p-4",
                    div {
                        class: "flex flex-col divide-y divide-border",
                        {stat_row("Name", &entity_name)}
                        {stat_row("Type", &type_name)}
                        {stat_row("Known since", &known_since)}
                        if entity.is_some() {
                            {stat_row("Relationship", &display_or_not_set(&meta.relationship))}
                            {stat_row("How you met", &display_or_not_set(&meta.how_met))}
                            {stat_row("Birthday", &display_or_not_set(&meta.birthday))}
                            {stat_row("Location", &display_or_not_set(&meta.location))}
                            {stat_row("Why they matter", &display_or_not_set(&meta.why))}
                            div {
                                class: "flex justify-between items-center text-sm py-1.5",
                                span { class: "text-muted-foreground", "Drift (days/unit)" }
                                input {
                                    r#type: "number",
                                    step: "0.5",
                                    min: "0.1",
                                    class: "w-20 rounded-md border border-input bg-background text-sm text-foreground px-2 py-1 text-right focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                    value: "{drift_value}",
                                    onchange: move |e| {
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
                if is_deletable {
                    div {
                        class: "rounded-lg border border-destructive/40 p-4 flex items-center justify-between gap-3",
                        div {
                            span { class: "text-sm font-medium text-foreground", "Delete this person" }
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
                                let token = auth_token;
                                let vault = active_vault;
                                spawn(async move {
                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                    match storage.delete_entity(entity_id.clone()).await {
                                        Ok(()) => {
                                            entities.write().retain(|e| e.id != entity_id);
                                            moments.write().retain(|m| m.entity_id != entity_id);
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

#[component]
pub fn EntityModalCmp() -> Element {
    let state = use_context::<AppState>();
    let mut entityModalTgl = state.entityModalTgl;
    let mut entities = state.entities;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut entityTypes = use_signal(||vec![]);
    let mut form = use_signal(EntityForm::default);


    let mut onsubmit = move |_| {
        let form_data = form.read().clone();
        let token = auth_token;
        let vault = active_vault;
        form.set(EntityForm::default());
        spawn(async move {
            let metadata = EntityMetadata {
                relationship: form_data.relationship.clone(),
                how_met: form_data.meeting.clone(),
                birthday: form_data.bday.clone(),
                location: form_data.location.clone(),
                why: form_data.why.clone(),
            };
            let new_entity = NewEntityType {
                name: form_data.name.clone(),
                entity_type_id: if form_data.entity_type_sel.is_empty() { None } else { Some(form_data.entity_type_sel.clone()) },
                parent_entity_id: None,
                user_id: None,
                archived_at: None,
                metadata: Some(metadata),
            };

            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.create_entity(new_entity).await {
                Ok(created) => {
                    entities.write().insert(0, created);
                }
                Err(e) => {
                    log::info!("Error creating entity: {}", e);
                }
            }
        });
    };

    use_effect(move || {
        // Read synchronously so the effect actually reruns on vault switch —
        // see the matching comment in views/home.rs's fetch effect.
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.get_entity_types().await {
                Ok(data) => entityTypes.set(data),
                Err(e) => clog!("Error fetching entities: {}",e),
            }
        });
    });
    
    rsx! {
        if *entityModalTgl.read() {
            div {
                class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                onclick: move |_| entityModalTgl.set(false),
                div {
                    class: "w-full max-w-md rounded-lg border border-border bg-card shadow-lg",
                    onclick: move |e| e.stop_propagation(),
                    div {
                        class: "flex items-center justify-between h-14 px-4 border-b border-border",
                        span { class: "text-lg font-semibold text-foreground", "New Entity" }
                        button {
                            class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                            onclick: move |_| entityModalTgl.set(false),
                            "×"
                        }
                    }
                    div {
                        class: "flex flex-col gap-4 px-4 py-4 max-h-[70vh] overflow-y-auto",
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Type" }
                            select {
                                class: "w-full h-10 rounded-md border border-input bg-background text-sm text-foreground px-3 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                value: "{form.read().entity_type_sel}",
                                oninput: move |e| {
                                    form.write().entity_type_sel = e.value();
                                },
                                option { value: "", "Select a type..." }
                                // "Self" is a reserved marker (exactly one
                                // per account, auto-provisioned — see
                                // types.rs's SELF_ENTITY_TYPE_ID), not a
                                // real relationship type to hand-pick when
                                // creating a new person.
                                for entity_type in entityTypes.iter().filter(|t| t.id != crate::types::SELF_ENTITY_TYPE_ID) {
                                    option {
                                        value: "{entity_type.id}",
                                        "{entity_type.name}"
                                    }
                                }
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Name" }
                            Input {
                                full_width: true,
                                value: "{form.read().name}",
                                on_input: move |e: Event<FormData>| form.write().name = e.value(),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Relationship to you" }
                            Input {
                                full_width: true,
                                value: "{form.read().relationship}",
                                on_input: move |e: Event<FormData>| form.write().relationship = e.value(),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "How you met" }
                            Input {
                                full_width: true,
                                value: "{form.read().meeting}",
                                on_input: move |e: Event<FormData>| form.write().meeting = e.value(),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Birthday" }
                            Input {
                                full_width: true,
                                value: "{form.read().bday}",
                                on_input: move |e: Event<FormData>| form.write().bday = e.value(),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Their location" }
                            Input {
                                full_width: true,
                                value: "{form.read().location}",
                                on_input: move |e: Event<FormData>| form.write().location = e.value(),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            Label { "Why they matter" }
                            Input {
                                full_width: true,
                                value: "{form.read().why}",
                                on_input: move |e: Event<FormData>| form.write().why = e.value(),
                            }
                        }
                    }
                    div {
                        class: "px-4 py-3 border-t border-border",
                        Button {
                            variant: ButtonVariant::Primary,
                            full_width: true,
                            on_click: move |e| onsubmit(e),
                            "Create Entity"
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
