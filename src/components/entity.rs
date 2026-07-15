use dioxus::prelude::*;
use crate::types::*;
use crate::theme::*;
use crate::AppState;
use crate::ABView;
use crate::api::{
    createEntity,
    getEntities,
    getEntityTypes,
    update_entity_field,
};
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
// when something happens: a completed task/promise, or a note that carries
// a non-zero gravity (logged-but-never-"completed" events, like someone
// bringing you flowers, still count). Closing amount is |gravity| scaled
// down — GRAVITY_DISTANCE_DIVISOR is a first-draft tuning knob, not final.
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

    let closed: f64 = moments.iter()
        .filter(|m| m.entity_id == entity.id)
        .filter(|m| {
            let completed_task_or_promise = (m.moment_type_id == 1i64 || m.moment_type_id == 2i64) && m.completed_at.is_some();
            let noteworthy_note = m.moment_type_id == 3i64 && m.gravity.unwrap_or(0) != 0;
            completed_task_or_promise || noteworthy_note
        })
        .map(|m| (m.gravity.unwrap_or(0).unsigned_abs() as f64) / GRAVITY_DISTANCE_DIVISOR)
        .sum();

    (grown - closed).max(0.0)
}

#[component]
pub fn entity_view_cmp() -> Element {
    let state = use_context::<AppState>();
    let current_entity = state.current_entity;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut activity_bar_view = state.activity_bar_view;
    let mut backdropTgl = state.backdropTgl;
    let mut is_graphs_open = use_signal(|| false);

    let entity = current_entity();
    let name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
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
                    Button {
                        variant: tab_variant(*is_graphs_open.read()),
                        size: ButtonSize::Small,
                        on_click: move |_| {
                            let tgl = *is_graphs_open.read();
                            is_graphs_open.set(!tgl);
                        },
                        "Graphs"
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
    let entity_id = entity.as_ref().map(|e| e.id);

    // Chronological "story" order: a moment's place in the timeline is when it
    // happened — its completion — not when it was created. Notes never complete,
    // so they fall back to when they were entered; the same fallback covers
    // still-open tasks/promises so they show up where they were introduced
    // rather than being dropped from the timeline.
    let timeline_key = |m: &MomentType| m.completed_at.clone().unwrap_or_else(|| m.created_at.clone());

    let mut entity_moments = moments.read().iter()
        .filter(|m| Some(m.entity_id) == entity_id)
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
                class: "flex flex-col gap-3 px-4 py-4 overflow-y-auto flex-1",
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
    let entity_id = entity.as_ref().map(|e| e.id);

    // Promises kept/pending and reaction coverage, scoped to this entity's moments.
    let (promises_kept, promises_pending, tasks_with_reactions) = {
        let all = moments.read();
        let for_entity = all.iter().filter(|m| Some(m.entity_id) == entity_id);
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
    let (entity_rank, total_entities) = {
        let all_moments = moments.read();
        let all_entities = entities.read();
        let mut counts: Vec<(i64, usize)> = all_entities.iter()
            .map(|e| (e.id, all_moments.iter().filter(|m| m.entity_id == e.id).count()))
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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1",
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
    let auth_token = state.auth_token;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;
    let mut entity_types = use_signal(|| vec![]);

    use_effect(move || {
        let token = auth_token;
        spawn(async move {
            let token_val = token.read().clone().unwrap_or_default();
            match getEntityTypes(token_val).await {
                Ok(data) => entity_types.set(data),
                Err(e) => clog!("Error fetching entity types: {}", e),
            }
        });
    });

    let entity = current_entity.read().clone();
    let entity_name = entity.as_ref().map(|e| e.name.clone()).unwrap_or_else(|| "All".to_string());
    let type_name = entity.as_ref()
        .and_then(|e| e.entity_type_id)
        .and_then(|type_id| entity_types.read().iter().find(|t| t.id == type_id).map(|t| t.name.clone()))
        .unwrap_or_else(|| "Not set".to_string());
    let known_since = entity.as_ref()
        .map(|e| e.created_at.chars().take(10).collect::<String>())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown".to_string());
    let drift_value = entity.as_ref().map(|e| e.drift).unwrap_or(2.0);

    let meta = entity.as_ref().and_then(|e| e.metadata.clone()).unwrap_or_default();
    let display_or_not_set = |s: &str| if s.trim().is_empty() { "Not set".to_string() } else { s.to_string() };

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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1",
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
                                        let Some(entity_id) = current_entity.read().as_ref().map(|e| e.id) else { return; };
                                        let Ok(new_drift) = e.value().parse::<f64>() else { return; };
                                        let token = auth_token;
                                        spawn(async move {
                                            let token_val = token.read().clone().unwrap_or_default();
                                            match update_entity_field(entity_id, "drift", serde_json::json!(new_drift), token_val).await {
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
    let mut entityTypes = use_signal(||vec![]);
    let mut form = use_signal(EntityForm::default);
    
    
    let mut onsubmit = move |_| {
        let form_data = form.read().clone();
        let token = auth_token;
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
                entity_type_id: form_data.entity_type_sel.parse::<i64>().ok(),
                parent_entity_id: None,
                user_id: None,
                archived_at: None,
                metadata: Some(metadata),
            };
    
            let token_val = token.read().clone().unwrap_or_default();
            match createEntity(new_entity, token_val).await {
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
        let token = auth_token;
        spawn(async move {
            let token_val = token.read().clone().unwrap_or_default();
            match getEntityTypes(token_val).await {
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
                                for entity_type in entityTypes.iter() {
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
