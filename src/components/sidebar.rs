use dioxus::prelude::*;
use crate::theme::*;
use crate::ui::*;
use crate::View;
use crate::AppState;
use crate::api::{is_self_entity, ActiveStorage};
use crate::types::{EntityType, MomentType, NewEntityType};
use lumen_blocks::components::dropdown::{Dropdown, DropdownContent, DropdownItem, DropdownTrigger};
use lumen_blocks::components::context_menu::{ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger};

const NAV_LINK_CLASS: &str = "block rounded-md px-3 py-2 text-sm font-medium text-foreground hover:bg-muted transition-colors cursor-pointer";
const NAV_LINK_ICON_CLASS: &str = "flex items-center gap-2 rounded-md px-3 py-2 text-sm font-medium text-foreground hover:bg-muted transition-colors cursor-pointer";

// (View, icon, label) for every hideable sidebar view — see View::
// as_storage_str/from_storage_str (main.rs) for why Entity/Settings aren't
// here. Data-driven so the hide/show 3-dot menu doesn't need repeating six
// times.
const VIEW_ENTRIES: &[(View, fn() -> Element, &str)] = &[
    (View::Inbox, fa_inbox, "All"),
    (View::SelfEntity, fa_user, "Self"),
    (View::Priority, fa_bolt, "Priority"),
    (View::Due, fa_calendar, "Due"),
    (View::Scheduled, fa_clock, "Scheduled"),
    (View::Distance, fa_compass, "Distance"),
    (View::Graph, fa_circle_nodes, "Graph View"),
    (View::RecentlyDeleted, fa_trash, "Recently Deleted"),
];

#[component]
pub fn views_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut tag_filter = state.tag_filter;
    let mut project_filter = state.project_filter;
    let mut hidden_views = state.hidden_views;
    let entities = state.entities;

    rsx! {
        div {
            class: "flex flex-col gap-y-1",
            for (view, icon, label) in VIEW_ENTRIES.iter().copied() {
                if !hidden_views.read().contains(&view) {
                    div {
                        key: "{label}",
                        class: "group flex items-center rounded-md hover:bg-muted transition-colors",
                        a {
                            class: "flex-1 flex items-center gap-2 px-3 py-2 text-sm font-medium text-foreground cursor-pointer min-w-0",
                            onclick: move |_| {
                                // SelfEntity reuses View::Entity's own
                                // rendering (see views/home.rs's combined
                                // match arm), which reads current_entity —
                                // so this is the one view in this list that
                                // needs it set instead of cleared.
                                if view == View::SelfEntity {
                                    let self_entity = entities.read().iter().find(|e| is_self_entity(e)).cloned();
                                    current_entity.set(self_entity);
                                } else {
                                    current_entity.set(None);
                                }
                                tag_filter.set(None);
                                project_filter.set(None);
                                currentView.set(view);
                            },
                            {icon()}
                            span { "{label}" }
                        }
                        Dropdown {
                            DropdownTrigger {
                                class: "shrink-0 pr-2 text-muted-foreground hover:text-foreground opacity-0 group-hover:opacity-100 transition-opacity cursor-pointer",
                                "⋯"
                            }
                            DropdownContent {
                                // "start" (the default) anchors the menu's
                                // *left* edge to the trigger and grows
                                // rightward — fine for vault_switcher_cmp's
                                // full-width trigger near the sidebar's left
                                // edge, but this trigger sits right at the
                                // sidebar's right edge, so a rightward-
                                // growing menu overshoots the 256px sidebar
                                // width and gets clipped by its
                                // overflow-y-auto (looked like a z-index bug —
                                // it isn't one, the menu is genuinely clipped,
                                // not just painted behind). "end" anchors the
                                // menu's right edge to the trigger instead,
                                // growing leftward, which stays inside the
                                // sidebar's bounds.
                                align: "end",
                                DropdownItem::<String> {
                                    value: "hide".to_string(),
                                    index: 0,
                                    on_select: move |_| {
                                        let mut updated = hidden_views.read().clone();
                                        if !updated.contains(&view) {
                                            updated.push(view);
                                        }
                                        crate::persist_hidden_views(&updated);
                                        hidden_views.set(updated);
                                    },
                                    "Hide"
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
pub fn entity_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut entities = state.entities;
    let mut moments = state.moments;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut entityModalTgl = state.entityModalTgl;
    let mut tag_filter = state.tag_filter;
    let mut project_filter = state.project_filter;
    let mut expanded = use_signal(|| true);
    let active_vault = state.active_vault;
    let auth_token = state.auth_token;

    // Same delete-vs-reassign choice as tag/project deletion (see
    // tag_list_cmp/project_list_cmp) — user asked for entity delete to work
    // "just like deleting projects and tags... just delete the moments or
    // move them to self" 2026-07-22.
    //
    // Both paths reassign every moment to Self *first*, always, even the
    // "delete" one — not just an implementation convenience. Supabase
    // enforces moments.entity_id as a real foreign key; a soft-deleted
    // moment (deleted_at set, per api::moment::deleteMoment) still holds
    // that reference, so Postgres would keep rejecting the entity's own
    // deletion with the exact silent-then-reappears failure this was
    // fixed for earlier today, even with every one of its moments already
    // "deleted." Reassigning to Self first unblocks the entity delete
    // unconditionally; the "delete" path then soft-deletes those
    // (now Self's) moments afterward, so they land in Recently Deleted —
    // still recoverable, same as any other deleted moment.
    let mut delete_error = use_signal(|| None::<String>);
    let mut confirming_delete_entity = use_signal(|| None::<EntityType>);

    let entity_moments = move |entity_id: &str| moments.read().iter()
        .filter(|m| m.entity_id == entity_id)
        .cloned()
        .collect::<Vec<_>>();

    let mut finish_entity_delete = move |entity: EntityType, vault: crate::api::VaultKind, token: Option<String>| {
        let was_viewing_this = current_entity.read().as_ref() == Some(&entity);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.delete_entity(entity.id.clone()).await {
                Ok(()) => {
                    entities.write().retain(|e| e.id != entity.id);
                    if was_viewing_this {
                        current_entity.set(None);
                        currentView.set(View::Inbox);
                    }
                }
                Err(e) => {
                    clog!("Error deleting entity: {}", e);
                    delete_error.set(Some("Couldn't delete that entity — try again.".to_string()));
                }
            }
        });
    };

    let mut delete_entity_and_moments = move |entity: EntityType| {
        delete_error.set(None);
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let self_id = match vault.effective(&token).resolve_self_entity_id(&entities.read()) {
            Some(id) => id,
            None => { delete_error.set(Some("Couldn't find your Self entity — try reloading.".to_string())); return; }
        };
        let to_move = entity_moments(&entity.id);
        confirming_delete_entity.set(None);
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
            finish_entity_delete(entity, vault, token);
        });
    };

    let mut move_moments_and_delete_entity = move |entity: EntityType| {
        delete_error.set(None);
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let self_id = match vault.effective(&token).resolve_self_entity_id(&entities.read()) {
            Some(id) => id,
            None => { delete_error.set(Some("Couldn't find your Self entity — try reloading.".to_string())); return; }
        };
        let to_move = entity_moments(&entity.id);
        confirming_delete_entity.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token.clone());
            for m in &to_move {
                let mid = m.id.clone();
                match storage.reassign_moment_entity(mid.clone(), self_id.clone()).await {
                    Ok(()) => {
                        if let Some(mm) = moments.write().iter_mut().find(|mm| mm.id == mid) {
                            mm.entity_id = self_id.clone();
                        }
                    }
                    Err(e) => clog!("Error reassigning moment before entity delete: {}", e),
                }
            }
            finish_entity_delete(entity, vault, token);
        });
    };

    // Split a person out of a group entity — see memory
    // project_entity_individuation and entity::backdated_created_at_for_distance's
    // doc comment for the distance/drift carryover rule (user decision,
    // 2026-07-22). Deliberately minimal v1: no moments transfer to the new
    // entity automatically (picking which ones is a separate, undesigned
    // problem) and there's no formal "group vs individual" entity type —
    // any entity can be individuated from, same as any entity can have
    // children via the already-existing-but-previously-unused
    // parent_entity_id.
    let mut confirming_individuate = use_signal(|| None::<EntityType>);
    let mut individuate_name = use_signal(String::new);
    let mut individuate_error = use_signal(|| None::<String>);

    let mut individuate = move |original: EntityType| {
        let name = individuate_name.read().trim().to_string();
        if name.is_empty() {
            individuate_error.set(Some("Give the new person a name.".to_string()));
            return;
        }
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let now = chrono::Utc::now();
        let target_distance = crate::components::compute_distance(&original, &moments.read(), now);
        let drift = if original.drift > 0.0 { original.drift } else { 2.0 };
        let backdated_created_at = crate::components::backdated_created_at_for_distance(target_distance, drift, now);
        let new_entity = NewEntityType {
            name,
            entity_type_id: original.entity_type_id.clone(),
            parent_entity_id: Some(original.id.clone()),
            user_id: None,
            archived_at: None,
            metadata: None,
        };
        confirming_individuate.set(None);
        individuate_name.set(String::new());
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            let created = match storage.create_entity(new_entity).await {
                Ok(e) => e,
                Err(e) => { clog!("Error creating individuated entity: {}", e); return; }
            };
            let id = created.id.clone();
            // Best-effort — the entity already exists correctly named and
            // linked even if backdating drift/created_at fails; worst case
            // it just starts at BASE_DISTANCE instead of the group's
            // current distance.
            if let Err(e) = storage.update_entity_field(id.clone(), "drift", serde_json::json!(drift)).await {
                clog!("Error setting individuated entity's drift: {}", e);
            }
            if let Err(e) = storage.update_entity_field(id.clone(), "created_at", serde_json::json!(backdated_created_at)).await {
                clog!("Error backdating individuated entity: {}", e);
            }
            let mut final_entity = created;
            final_entity.drift = drift;
            final_entity.created_at = backdated_created_at;
            entities.write().insert(0, final_entity);
        });
    };

    // The self entity (see api::is_self_entity) is deliberately excluded
    // from this list — it's now its own sidebar View instead (see
    // views_list_cmp's VIEW_ENTRIES / View::SelfEntity), reachable and
    // hideable the same way Due/Priority/etc are, rather than mixed in
    // among real entities here.
    let mut visible_entities: Vec<_> = entities.read().iter()
        .filter(|e| !is_self_entity(e))
        .cloned()
        .collect();
    // Case-insensitive — a plain sort() puts every uppercase name before
    // every lowercase one (ASCII 'A'-'Z' sort below 'a'-'z'), which doesn't
    // read as alphabetical once names are a mix of cases.
    visible_entities.sort_by_key(|e| e.name.to_lowercase());

    rsx! {
        div {
            class: "px-3 mt-6 pt-4 border-t border-border flex flex-col gap-y-1",
            div {
                class: "flex items-center justify-between mb-1",
                span {
                    class: "flex items-center gap-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground cursor-pointer select-none",
                    onclick: move |_| { let v = *expanded.read(); expanded.set(!v); },
                    span { class: "text-[10px]", if *expanded.read() { "▾" } else { "▸" } }
                    "Entities"
                }
                button {
                    class: "h-6 w-6 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer",
                    title: "Add entity",
                    onclick: move |_| entityModalTgl.set(true),
                    fa_plus {}
                }
            }
            if let Some(msg) = delete_error.read().as_ref() {
                div {
                    class: "text-xs text-destructive px-3 py-1.5 rounded-md bg-destructive/10",
                    "{msg}"
                }
            }
            if *expanded.read() {
            if visible_entities.is_empty() {
                span {
                    class: "px-3 text-xs text-muted-foreground",
                    "No entities yet"
                }
            } else {
                for entity in visible_entities.into_iter(){
                    ContextMenu {
                        key: "{entity.id}",
                        ContextMenuTrigger {
                            a {
                                class: NAV_LINK_CLASS,
                                onclick: {
                                    let entity = entity.clone();
                                    move |_| {
                                        // Symmetric with the tag-click reset below: an
                                        // entity is its own distinct browsing mode too,
                                        // and a tag filter left active from earlier
                                        // silently compounded with whatever entity got
                                        // clicked next — "click a person, then click a
                                        // tag that's already active" did nothing
                                        // visible because the tag click's own reset
                                        // only fires when *activating* a tag, and this
                                        // was the one place a tag could stay active
                                        // without the user having touched tags at all.
                                        current_entity.set(Some(entity.clone()));
                                        tag_filter.set(None);
                                        project_filter.set(None);
                                        currentView.set(View::Entity);
                                    }
                                },
                                "{entity.name}"
                            }
                        }
                        ContextMenuContent {
                            ContextMenuItem {
                                value: "individuate".to_string(),
                                index: 0,
                                on_select: {
                                    let entity = entity.clone();
                                    move |_| {
                                        individuate_error.set(None);
                                        confirming_individuate.set(Some(entity.clone()));
                                    }
                                },
                                "Individuate"
                            }
                            ContextMenuItem {
                                value: "delete".to_string(),
                                index: 1,
                                destructive: true,
                                on_select: {
                                    let entity = entity.clone();
                                    move |_| confirming_delete_entity.set(Some(entity.clone()))
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
            }
        }
        if let Some(original) = confirming_individuate.read().clone() {
            div {
                class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                onclick: move |_| confirming_individuate.set(None),
                div {
                    class: "bg-background rounded-lg border border-border shadow-lg w-full max-w-sm p-4 flex flex-col gap-3",
                    onclick: move |e| e.stop_propagation(),
                    h3 { class: "text-sm font-semibold text-foreground", "Individuate from \"{original.name}\"" }
                    p {
                        class: "text-sm text-muted-foreground",
                        "Creates a new person nested under \"{original.name}\", starting at its current Distance/Drift. No moments move over automatically."
                    }
                    div {
                        class: "flex flex-col gap-y-1.5",
                        label { class: "block text-xs font-medium text-foreground", "New person's name" }
                        input {
                            r#type: "text",
                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{individuate_name.read()}",
                            oninput: move |e| individuate_name.set(e.value()),
                        }
                    }
                    if let Some(msg) = individuate_error.read().as_ref() {
                        p { class: "text-sm text-destructive", "{msg}" }
                    }
                    div {
                        class: "flex flex-col gap-2",
                        button {
                            class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-3 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer",
                            onclick: move |_| individuate(original.clone()),
                            "Create"
                        }
                        button {
                            class: "text-sm text-muted-foreground hover:text-foreground cursor-pointer",
                            onclick: move |_| confirming_individuate.set(None),
                            "Cancel"
                        }
                    }
                }
            }
        }
        if let Some(entity) = confirming_delete_entity.read().clone() {
            {
                let count = entity_moments(&entity.id).len();
                rsx! {
                    div {
                        class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                        onclick: move |_| confirming_delete_entity.set(None),
                        div {
                            class: "bg-background rounded-lg border border-border shadow-lg w-full max-w-sm p-4 flex flex-col gap-3",
                            onclick: move |e| e.stop_propagation(),
                            h3 { class: "text-sm font-semibold text-foreground", "Delete \"{entity.name}\"?" }
                            p {
                                class: "text-sm text-muted-foreground",
                                "{count} moment(s) belong to this person. Delete them too, or move them to Self and keep them?"
                            }
                            div {
                                class: "flex flex-col gap-2",
                                button {
                                    class: "rounded-md border border-transparent bg-destructive text-destructive-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer",
                                    onclick: {
                                        let entity = entity.clone();
                                        move |_| delete_entity_and_moments(entity.clone())
                                    },
                                    "Delete {count} moment(s)"
                                }
                                button {
                                    class: "rounded-md border border-input bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                    onclick: {
                                        let entity = entity.clone();
                                        move |_| move_moments_and_delete_entity(entity.clone())
                                    },
                                    "Move to Self"
                                }
                                button {
                                    class: "text-sm text-muted-foreground hover:text-foreground cursor-pointer",
                                    onclick: move |_| confirming_delete_entity.set(None),
                                    "Cancel"
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
pub fn tag_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut tag_filter = state.tag_filter;
    let mut project_filter = state.project_filter;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut expanded = use_signal(|| true);
    let active_vault = state.active_vault;
    let auth_token = state.auth_token;

    let mut tags: Vec<String> = moments.read().iter()
        .filter_map(|m| m.metadata.as_ref())
        .flat_map(|meta| meta.tags.clone())
        .collect();
    // Case-insensitive — see visible_entities' matching comment above.
    tags.sort_by_key(|t| t.to_lowercase());
    tags.dedup();

    // Set when the user picks "Delete" from a tag's right-click menu — the
    // tag name they're about to act on, until they pick one of the two
    // resolutions below (or cancel). Nothing happens on right-click alone;
    // this just opens the confirmation with the choice the user asked for
    // explicitly: delete every moment carrying the tag, or just unlink the
    // tag from them and leave the moments themselves alone.
    let mut confirming_delete = use_signal(|| None::<String>);

    let mut delete_tag_moments = move |tag: String| {
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let matching: Vec<MomentType> = moments.read().iter()
            .filter(|m| m.metadata.as_ref().is_some_and(|meta| meta.tags.contains(&tag)))
            .cloned()
            .collect();
        confirming_delete.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            for m in matching {
                let id = m.id.clone();
                match storage.delete_moment(m).await {
                    Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                    Err(e) => clog!("Error deleting moment while deleting tag: {}", e),
                }
            }
        });
    };

    let mut unlink_tag = move |tag: String| {
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let matching: Vec<MomentType> = moments.read().iter()
            .filter(|m| m.metadata.as_ref().is_some_and(|meta| meta.tags.contains(&tag)))
            .cloned()
            .collect();
        confirming_delete.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            for m in matching {
                let id = m.id.clone();
                let mut new_meta = m.metadata.clone().unwrap_or_default();
                new_meta.tags.retain(|t| t != &tag);
                match storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await {
                    Ok(()) => {
                        if let Some(mm) = moments.write().iter_mut().find(|mm| mm.id == id) {
                            mm.metadata = Some(new_meta.clone());
                        }
                    }
                    Err(e) => clog!("Error unlinking tag from moment: {}", e),
                }
            }
        });
    };

    rsx! {
        div {
            class: "px-3 mt-6 pt-4 border-t border-border flex flex-col gap-y-1",
            span {
                class: "flex items-center gap-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-1 cursor-pointer select-none",
                onclick: move |_| { let v = *expanded.read(); expanded.set(!v); },
                span { class: "text-[10px]", if *expanded.read() { "▾" } else { "▸" } }
                "Tags"
            }
            if *expanded.read() {
            if tags.is_empty() {
                span {
                    class: "px-3 text-xs text-muted-foreground",
                    "No tags yet"
                }
            } else {
                for tag in tags.iter() {
                    ContextMenu {
                        key: "{tag}",
                        ContextMenuTrigger {
                            a {
                                class: if tag_filter.read().as_deref() == Some(tag.as_str()) {
                                    "block rounded-md px-3 py-2 text-sm font-medium bg-muted text-foreground cursor-pointer"
                                } else {
                                    NAV_LINK_CLASS
                                },
                                onclick: {
                                    let tag = tag.clone();
                                    move |_| {
                                        let is_active = tag_filter.read().as_deref() == Some(tag.as_str());
                                        if is_active {
                                            tag_filter.set(None);
                                        } else {
                                            // A tag is a cross-cutting list, not a
                                            // filter scoped to whatever entity
                                            // happens to be selected — "bugs" should
                                            // mean every bug for every person, not
                                            // just this one's. Drop entity scope so
                                            // the tag actually behaves like a list.
                                            tag_filter.set(Some(tag.clone()));
                                            project_filter.set(None);
                                            current_entity.set(None);
                                            currentView.set(View::Inbox);
                                        }
                                    }
                                },
                                "{tag}"
                            }
                        }
                        ContextMenuContent {
                            ContextMenuItem {
                                value: "delete".to_string(),
                                index: 0,
                                destructive: true,
                                on_select: {
                                    let tag = tag.clone();
                                    move |_| confirming_delete.set(Some(tag.clone()))
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
            }
        }
        if let Some(tag) = confirming_delete.read().clone() {
            {
                let count = moments.read().iter()
                    .filter(|m| m.metadata.as_ref().is_some_and(|meta| meta.tags.contains(&tag)))
                    .count();
                rsx! {
                    div {
                        class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                        onclick: move |_| confirming_delete.set(None),
                        div {
                            class: "bg-background rounded-lg border border-border shadow-lg w-full max-w-sm p-4 flex flex-col gap-3",
                            onclick: move |e| e.stop_propagation(),
                            h3 { class: "text-sm font-semibold text-foreground", "Delete tag \"{tag}\"" }
                            p {
                                class: "text-sm text-muted-foreground",
                                "{count} moment(s) carry this tag. Delete them too, or just remove the tag and keep the moments?"
                            }
                            div {
                                class: "flex flex-col gap-2",
                                button {
                                    class: "rounded-md border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer",
                                    onclick: {
                                        let tag = tag.clone();
                                        move |_| delete_tag_moments(tag.clone())
                                    },
                                    "Delete {count} moment(s)"
                                }
                                button {
                                    class: "rounded-md border border-border bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                    onclick: {
                                        let tag = tag.clone();
                                        move |_| unlink_tag(tag.clone())
                                    },
                                    "Just remove the tag"
                                }
                                button {
                                    class: "text-sm text-muted-foreground hover:text-foreground cursor-pointer",
                                    onclick: move |_| confirming_delete.set(None),
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// Same cross-cutting-list treatment as tags, for the same reason: "which
// projects exist" isn't something you're meant to hold in your head, it's
// something you click through — same as not remembering every entity name
// off the top of your head.
#[component]
pub fn project_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut tag_filter = state.tag_filter;
    let mut project_filter = state.project_filter;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut expanded = use_signal(|| true);
    let active_vault = state.active_vault;
    let auth_token = state.auth_token;

    let mut projects: Vec<String> = moments.read().iter()
        .filter_map(|m| m.metadata.as_ref())
        .filter_map(|meta| meta.project.clone())
        .filter(|p| !p.is_empty())
        .collect();
    // Case-insensitive — see visible_entities' matching comment above.
    projects.sort_by_key(|p| p.to_lowercase());
    projects.dedup();

    // Same delete-vs-unlink choice as tag_list_cmp above, mirrored for
    // metadata.project (a single Option<String>, not a Vec like tags).
    let mut confirming_delete = use_signal(|| None::<String>);

    let mut delete_project_moments = move |project: String| {
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let matching: Vec<MomentType> = moments.read().iter()
            .filter(|m| m.metadata.as_ref().and_then(|meta| meta.project.as_deref()) == Some(project.as_str()))
            .cloned()
            .collect();
        confirming_delete.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            for m in matching {
                let id = m.id.clone();
                match storage.delete_moment(m).await {
                    Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                    Err(e) => clog!("Error deleting moment while deleting project: {}", e),
                }
            }
        });
    };

    let mut unlink_project = move |project: String| {
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let matching: Vec<MomentType> = moments.read().iter()
            .filter(|m| m.metadata.as_ref().and_then(|meta| meta.project.as_deref()) == Some(project.as_str()))
            .cloned()
            .collect();
        confirming_delete.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            for m in matching {
                let id = m.id.clone();
                let mut new_meta = m.metadata.clone().unwrap_or_default();
                new_meta.project = None;
                match storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await {
                    Ok(()) => {
                        if let Some(mm) = moments.write().iter_mut().find(|mm| mm.id == id) {
                            mm.metadata = Some(new_meta.clone());
                        }
                    }
                    Err(e) => clog!("Error unlinking project from moment: {}", e),
                }
            }
        });
    };

    rsx! {
        div {
            class: "px-3 mt-6 pt-4 border-t border-border flex flex-col gap-y-1",
            span {
                class: "flex items-center gap-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-1 cursor-pointer select-none",
                onclick: move |_| { let v = *expanded.read(); expanded.set(!v); },
                span { class: "text-[10px]", if *expanded.read() { "▾" } else { "▸" } }
                "Projects"
            }
            if *expanded.read() {
            if projects.is_empty() {
                span {
                    class: "px-3 text-xs text-muted-foreground",
                    "No projects yet"
                }
            } else {
                for project in projects.iter() {
                    ContextMenu {
                        key: "{project}",
                        ContextMenuTrigger {
                            a {
                                class: if project_filter.read().as_deref() == Some(project.as_str()) {
                                    "block rounded-md px-3 py-2 text-sm font-medium bg-muted text-foreground cursor-pointer"
                                } else {
                                    NAV_LINK_CLASS
                                },
                                onclick: {
                                    let project = project.clone();
                                    move |_| {
                                        let is_active = project_filter.read().as_deref() == Some(project.as_str());
                                        if is_active {
                                            project_filter.set(None);
                                        } else {
                                            project_filter.set(Some(project.clone()));
                                            tag_filter.set(None);
                                            current_entity.set(None);
                                            currentView.set(View::Inbox);
                                        }
                                    }
                                },
                                "{project}"
                            }
                        }
                        ContextMenuContent {
                            ContextMenuItem {
                                value: "delete".to_string(),
                                index: 0,
                                destructive: true,
                                on_select: {
                                    let project = project.clone();
                                    move |_| confirming_delete.set(Some(project.clone()))
                                },
                                "Delete"
                            }
                        }
                    }
                }
            }
            }
        }
        if let Some(project) = confirming_delete.read().clone() {
            {
                let count = moments.read().iter()
                    .filter(|m| m.metadata.as_ref().and_then(|meta| meta.project.as_deref()) == Some(project.as_str()))
                    .count();
                rsx! {
                    div {
                        class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                        onclick: move |_| confirming_delete.set(None),
                        div {
                            class: "bg-background rounded-lg border border-border shadow-lg w-full max-w-sm p-4 flex flex-col gap-3",
                            onclick: move |e| e.stop_propagation(),
                            h3 { class: "text-sm font-semibold text-foreground", "Delete project \"{project}\"" }
                            p {
                                class: "text-sm text-muted-foreground",
                                "{count} moment(s) are in this project. Delete them too, or just remove the project and keep the moments?"
                            }
                            div {
                                class: "flex flex-col gap-2",
                                button {
                                    class: "rounded-md border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer",
                                    onclick: {
                                        let project = project.clone();
                                        move |_| delete_project_moments(project.clone())
                                    },
                                    "Delete {count} moment(s)"
                                }
                                button {
                                    class: "rounded-md border border-border bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                    onclick: {
                                        let project = project.clone();
                                        move |_| unlink_project(project.clone())
                                    },
                                    "Just remove the project"
                                }
                                button {
                                    class: "text-sm text-muted-foreground hover:text-foreground cursor-pointer",
                                    onclick: move |_| confirming_delete.set(None),
                                    "Cancel"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
