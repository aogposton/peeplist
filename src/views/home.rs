use dioxus::prelude::*;
use crate::AppState;
use crate::theme::*;
use crate::View::*;
use crate::components::{
    MomentCmp,
    MomentListCmp,
    MomentInputCmp,
    NotesSectionCmp,
    CompletedSectionCmp,
    entity_view_cmp,
    PriorityViewCmp,
    UrgencySettingsCmp,
    AllEntitiesViewCmp,
    DueViewCmp,
    ScheduledViewCmp,
    BlockingViewCmp,
    NotesViewCmp,
    SettingsCmp,
    RecentlyDeletedViewCmp,
    MomentosViewCmp,
    MissedViewCmp,
};

use crate::api::ActiveStorage;

use crate::types::MomentType;


#[component]
pub fn Home() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut entities = state.entities;
    let mut sidebarTgl = state.sidebarTgl;
    let mut data_loading = state.data_loading;
    let current_view = state.currentView;
    let current_entity = state.current_entity;
    let tag_filter = state.tag_filter;
    let project_filter = state.project_filter;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut hide_notes = state.hide_notes;
    let mut hide_completed = state.hide_completed;
    let is_desktop_viewport = state.is_desktop_viewport;
    let sidebar_collapsed = state.sidebar_collapsed;
    // The fixed hamburger button (layouts::Navbar) sits top-left, roughly
    // 4px-52px from the top — shown whenever the docked sidebar isn't
    // (mobile, or a narrow/collapsed desktop window; same condition the
    // hamburger button itself uses). Inbox/Entity/Self already clear it
    // naturally (entity_view_cmp's profile header has its own top space),
    // but every other view's plain `h1`-under-`pt-4` heading sat right in
    // its bounding box on a touch-sized viewport — 2026-08-03 bug report.
    let heading_top_pad = if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "pt-4" } else { "pt-16" };

    let has_tag = |m: &MomentType, tag: &str| {
        m.metadata.as_ref().is_some_and(|meta| meta.tags.iter().any(|t| t == tag))
    };
    let has_project = |m: &MomentType, project: &str| {
        m.metadata.as_ref().and_then(|meta| meta.project.as_deref()) == Some(project)
    };
    // The one place reveal_lead actually hides anything (see
    // MomentMetadata::reveal_lead's doc comment) — the general moment
    // list, not the dedicated Momentos tab/sidebar (those always show
    // everything; see MomentosViewCmp/ab_momentos_cmp). Non-momentos pass
    // through unaffected.
    let momento_visible = |m: &MomentType, now: chrono::DateTime<chrono::Utc>| {
        if m.moment_type_id != 4i64 {
            return true;
        }
        let meta = m.metadata.clone().unwrap_or_default();
        let today = now.date_naive();
        match crate::momento::next_occurrences(m.due_at.as_deref().unwrap_or_default(), &meta, today, 1).into_iter().next() {
            Some(occ) => crate::momento::is_revealed(occ.datetime, meta.reveal_lead.as_deref(), now),
            None => true,
        }
    };

    use_effect(move || {
        // Read both synchronously so Dioxus tracks them as effect
        // dependencies — reading them only inside spawn()'s async block (as
        // this used to) means the effect never reruns on its own, since
        // that read happens on a separate scheduled task, not during this
        // closure's tracked execution. That's why switching vaults used to
        // need a manual page refresh to actually take effect.
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        data_loading.set(true);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.get_moments().await {
                Ok(data) => moments.set(data),
                Err(e) => log::info!("Error fetching moments: {}", e),
            }

            match storage.get_entities().await {
                Ok(data) => entities.set(data),
                Err(e) => log::info!("Error fetching entities: {}", e),
            }

            data_loading.set(false);
        });
    });


    rsx! {
        div {
            class: "w-full h-min-screen pt-2 mb-10",
            style: "background-color:{BG};",
            if *data_loading.read() {
                loading_skeleton_cmp {}
            } else {
            match current_view.read().clone() {
                Inbox => {
                    let now = chrono::Utc::now();
                    let visible: Vec<MomentType> = moments.read().iter()
                        .filter(|m| !crate::urgency::is_waiting(m, now))
                        .filter(|m| !crate::urgency::is_missed(m, now))
                        .filter(|m| momento_visible(m, now))
                        .filter(|m| tag_filter.read().as_ref().map_or(true, |tag| has_tag(m, tag)))
                        .filter(|m| project_filter.read().as_ref().map_or(true, |p| has_project(m, p)))
                        .cloned()
                        .collect();
                    rsx! {
                        entity_view_cmp { }
                        div {class:"h-4"}
                        div { class: if *is_desktop_viewport.read() { "block" } else { "hidden" }, MomentInputCmp { } }
                        div {class:"h-4"}
                        MomentListCmp { moments: visible.clone() }
                        div {
                            class: "flex items-center gap-3 px-5 mb-1",
                            a {
                                class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer",
                                onclick: move |_| { let v = *hide_notes.read(); hide_notes.set(!v); },
                                if *hide_notes.read() { "Show notes" } else { "Hide notes" }
                            }
                            a {
                                class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer",
                                onclick: move |_| { let v = *hide_completed.read(); hide_completed.set(!v); },
                                if *hide_completed.read() { "Show completed" } else { "Hide completed" }
                            }
                        }
                        if !*hide_notes.read() { NotesSectionCmp { moments: visible.clone() } }
                        if !*hide_completed.read() { CompletedSectionCmp { moments: visible } }
                    }
                },
                // SelfEntity is a sidebar View (hideable, listed like Due/
                // Priority) whose click handler (sidebar.rs's
                // views_list_cmp) sets current_entity to the self entity
                // before landing here — same rendering as clicking any
                // other entity, just reached from the Views list instead
                // of the Entities list.
                Entity | SelfEntity => {
                    let now = chrono::Utc::now();
                    let visible: Vec<MomentType> = moments.read().iter()
                        .filter(|m| current_entity.read().as_ref().map_or(false, |e| m.involves_entity(&e.id)))
                        .filter(|m| !crate::urgency::is_waiting(m, now))
                        .filter(|m| !crate::urgency::is_missed(m, now))
                        .filter(|m| momento_visible(m, now))
                        .filter(|m| tag_filter.read().as_ref().map_or(true, |tag| has_tag(m, tag)))
                        .filter(|m| project_filter.read().as_ref().map_or(true, |p| has_project(m, p)))
                        .cloned()
                        .collect();
                    rsx! {
                        entity_view_cmp { }
                        div {class:"h-4"}
                        div { class: if *is_desktop_viewport.read() { "block" } else { "hidden" }, MomentInputCmp { } }
                        div {class:"h-4"}
                        MomentListCmp { moments: visible.clone() }
                        div {
                            class: "flex items-center gap-3 px-5 mb-1",
                            a {
                                class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer",
                                onclick: move |_| { let v = *hide_notes.read(); hide_notes.set(!v); },
                                if *hide_notes.read() { "Show notes" } else { "Hide notes" }
                            }
                            a {
                                class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer",
                                onclick: move |_| { let v = *hide_completed.read(); hide_completed.set(!v); },
                                if *hide_completed.read() { "Show completed" } else { "Hide completed" }
                            }
                        }
                        if !*hide_notes.read() { NotesSectionCmp { moments: visible.clone() } }
                        if !*hide_completed.read() { CompletedSectionCmp { moments: visible } }
                    }
                },
                Priority => rsx! {
                    div {
                        class: "px-4 {heading_top_pad} flex items-start justify-between gap-3",
                        div {
                            h1 { class: "text-2xl font-semibold text-foreground mb-1", "Expedite" }
                            p { class: "text-sm text-muted-foreground mb-4", "Open tasks and promises across everyone, ranked by urgency." }
                        }
                        UrgencySettingsCmp { }
                    }
                    PriorityViewCmp { }
                },
                AllEntities => rsx! {
                    AllEntitiesViewCmp { }
                },
                Due => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Due" }
                        p { class: "text-sm text-muted-foreground mb-4", "Only what's overdue — nothing due today or later shows up here." }
                    }
                    DueViewCmp { }
                },
                Scheduled => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Scheduled" }
                        p { class: "text-sm text-muted-foreground mb-4", "Waiting to come back into view — check on these before they arrive." }
                    }
                    ScheduledViewCmp { }
                },
                Blocking => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Blocking" }
                        p { class: "text-sm text-muted-foreground mb-4", "Still open, and standing between other things and done." }
                    }
                    BlockingViewCmp { }
                },
                Notes => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Notes" }
                        p { class: "text-sm text-muted-foreground mb-4", "Every note, in one place, newest first." }
                    }
                    NotesViewCmp { }
                },
                Settings => rsx! {
                    SettingsCmp { }
                },
                RecentlyDeleted => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Recently Deleted" }
                        p { class: "text-sm text-muted-foreground mb-4", "Deleted moments in this vault. Restore one to bring it back to its entity." }
                    }
                    RecentlyDeletedViewCmp { }
                },
                Momentos => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Momentos" }
                        p { class: "text-sm text-muted-foreground mb-4", "Every recurring/personal moment, across everyone, next-upcoming first." }
                    }
                    MomentosViewCmp { }
                },
                Missed => rsx! {
                    div {
                        class: "px-4 {heading_top_pad}",
                        h1 { class: "text-2xl font-semibold text-foreground mb-1", "Missed" }
                        p { class: "text-sm text-muted-foreground mb-4", "Had a deadline, and it passed without getting done." }
                    }
                    MissedViewCmp { }
                }
            }
            }
        }

    }
}

// Generic on purpose — the thing loading is moments/entities, which every
// view (Inbox, Priority, Graph, Stats, etc.) is ultimately rendering a
// projection of, not view-specific content. A handful of pulsing rows
// reads as "data incoming" regardless of which view you land on first.
#[component]
fn loading_skeleton_cmp() -> Element {
    rsx! {
        div {
            class: "px-4 pt-4 flex flex-col gap-3 animate-pulse",
            div { class: "h-6 w-32 rounded bg-muted" }
            div { class: "h-10 w-full rounded-lg bg-muted mt-2" }
            for _ in 0..5 {
                div { class: "h-14 w-full rounded-lg bg-muted" }
            }
        }
    }
}
