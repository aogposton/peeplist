use dioxus::prelude::*;
use crate::AppState;
use crate::ABView;
use crate::SortMode;
use crate::ListDensity;
use crate::UrgencyWeights;
use crate::theme::*;
use crate::ui::*;
use crate::types::*;
use crate::api::*;
use lumen_blocks::components::input::{Input, InputSize};
use lumen_blocks::components::button::{Button, ButtonVariant, ButtonSize};
use lumen_blocks::components::dropdown::{
    Dropdown, DropdownContent, DropdownItem, DropdownTrigger,
};
use lumen_blocks::components::collapsible::{Collapsible, CollapsibleTrigger, CollapsibleContent};
use lumen_blocks::components::label::{Label, LabelSize};
use crate::components::context_menu::{ContextMenu, ContextMenuContent, ContextMenuItem, ContextMenuTrigger};
use web_sys::window;
use crate::quick_capture::{self, TokenKind};

#[component]
pub fn CheckboxCmp(props: CheckboxProps) -> Element {
    rsx! {
        div {
            label {
                class: if props.disabled {
                    "flex items-center relative cursor-not-allowed"
                } else {
                    "flex items-center cursor-pointer relative"
                },
                title: if props.disabled { "Blocked by an incomplete dependency" } else { "" },
                input {
                    r#type: "checkbox",
                    checked: props.checked,
                    disabled: props.disabled,
                    class: if props.disabled {
                        "peer h-5 w-5 appearance-none rounded-md border-2 border-input bg-muted opacity-50 cursor-not-allowed"
                    } else {
                        "peer h-5 w-5 cursor-pointer transition-colors appearance-none rounded-md border-2 border-input bg-background checked:bg-primary checked:border-primary hover:border-primary/50"
                    },
                    onchange: move |e| {
                        if !props.disabled {
                            props.on_change.call(e.checked());
                        }
                    },
                }
                span {
                    class: "absolute text-primary-foreground opacity-0 peer-checked:opacity-100 top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2 pointer-events-none",
                    svg {
                        xmlns: "http://www.w3.org/2000/svg",
                        class: "h-3.5 w-3.5",
                        view_box: "0 0 20 20",
                        fill: "currentColor",
                        stroke: "currentColor",
                        stroke_width: "1",
                        path {
                            fill_rule: "evenodd",
                            d: "M16.707 5.293a1 1 0 010 1.414l-8 8a1 1 0 01-1.414 0l-4-4a1 1 0 011.414-1.414L8 12.586l7.293-7.293a1 1 0 011.414 0z",
                            clip_rule: "evenodd"
                        }
                    }
                }
            }
        }
    }
}


#[component]
pub fn NotesSectionCmp(props: MomentListProps) -> Element {
    // Nothing to show, nothing to render — an empty collapsed shell was
    // just visual noise for the common case of a person/tag with no notes
    // at all.
    if !props.moments.iter().any(|m| m.moment_type_id == 3i64) {
        return rsx! {};
    }
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let onConvertTo = move |id: String, mType: i64| {
        let token = auth_token;
                        let vault = active_vault;
        let note_type = mType;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.update_moment_field(id.clone(), "moment_type_id", serde_json::json!(Some(note_type.clone()))).await {
                Ok(_) => {
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                        m.moment_type_id = note_type.clone();
                    }
                }
                Err(e) => log::info!("Error updating moment: {}", e),
            }
        });
    };

    let onDelete = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let id = m.id.clone();
            match storage.delete_moment(m).await {
                Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                Err(e) => log::info!("Error deleting moment: {}", e),
            }
        });
    };

    let onDuplicate = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let new_moment = NewMomentType {
                title: m.title.clone(),
                description: m.description.clone(),
                gravity: m.gravity,
                entity_id: m.entity_id.clone(),
                moment_type_id: m.moment_type_id,
                deleted_at: None,
            };
            match storage.create_moment(new_moment).await {
                Ok(mut created) => {
                    if let Some(meta) = m.metadata.clone() {
                        let _ = storage.update_moment_field(created.id.clone(), "metadata", serde_json::json!(meta)).await;
                        created.metadata = Some(meta);
                    }
                    moments.write().insert(0, created);
                }
                Err(e) => log::info!("Error duplicating moment: {}", e),
            }
        });
    };

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background overflow-hidden",
            Collapsible {
                CollapsibleTrigger {
                    class: "text-sm font-medium text-foreground hover:no-underline hover:bg-muted/50",
                    "Notes"
                }
                CollapsibleContent {
                    div {
                        class: "flex flex-col divide-y divide-border",
                        for moment in props.moments.iter() {
                            if  moment.moment_type_id == 3i64 {
                                ContextMenu {
                                    key: "{moment.id}",
                                    ContextMenuTrigger {
                                        MomentCmp {
                                            moment: moment.clone(),
                                            is_note: true,
                                        }
                                    }
                                    ContextMenuContent {
                                        ContextMenuItem {
                                            on_select: { let id = moment.id.clone(); move |_| onConvertTo(id.clone(), 1i64) },
                                            "Convert to task"
                                        }
                                        ContextMenuItem {
                                            on_select: { let id = moment.id.clone(); move |_| onConvertTo(id.clone(), 2i64) },
                                            "Convert to promise"
                                        }
                                        ContextMenuItem {
                                            on_select: { let m = moment.clone(); move |_| onDuplicate(m.clone()) },
                                            "Duplicate"
                                        }
                                        ContextMenuItem {
                                            destructive: true,
                                            on_select: { let m = moment.clone(); move |_| onDelete(m.clone()) },
                                            "Delete"
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
pub fn CompletedSectionCmp(props: MomentListProps) -> Element {
    if !props.moments.iter().any(|m| m.completed_at.is_some()) {
        return rsx! {};
    }
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    // This section never actually rendered a menu at all before (it tracked
    // show_menu/menu_coords on right-click but nothing consumed them) — so
    // this is a real gap being filled, not just a dismiss-bug fix like the
    // other two. Delete/Duplicate only, no "convert type" — that's not a
    // meaningful action on something already completed.
    let onDelete = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let id = m.id.clone();
            match storage.delete_moment(m).await {
                Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                Err(e) => log::info!("Error deleting moment: {}", e),
            }
        });
    };

    let onDuplicate = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let new_moment = NewMomentType {
                title: m.title.clone(),
                description: m.description.clone(),
                gravity: m.gravity,
                entity_id: m.entity_id.clone(),
                moment_type_id: m.moment_type_id,
                deleted_at: None,
            };
            match storage.create_moment(new_moment).await {
                Ok(mut created) => {
                    if let Some(meta) = m.metadata.clone() {
                        let _ = storage.update_moment_field(created.id.clone(), "metadata", serde_json::json!(meta)).await;
                        created.metadata = Some(meta);
                    }
                    moments.write().insert(0, created);
                }
                Err(e) => log::info!("Error duplicating moment: {}", e),
            }
        });
    };

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background overflow-hidden",
            Collapsible {
                CollapsibleTrigger {
                    class: "text-sm font-medium text-foreground hover:no-underline hover:bg-muted/50",
                    "Completed"
                }
                CollapsibleContent {
                    div {
                        class: "flex flex-col divide-y divide-border",
                        for moment in props.moments.iter() {
                            if moment.completed_at.is_some() {
                                ContextMenu {
                                    key: "{moment.id}",
                                    ContextMenuTrigger {
                                        MomentCmp {
                                            moment: moment.clone(),
                                        }
                                    }
                                    ContextMenuContent {
                                        ContextMenuItem {
                                            on_select: { let m = moment.clone(); move |_| onDuplicate(m.clone()) },
                                            "Duplicate"
                                        }
                                        ContextMenuItem {
                                            destructive: true,
                                            on_select: { let m = moment.clone(); move |_| onDelete(m.clone()) },
                                            "Delete"
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
pub fn moment_history_cmp() -> Element {
    let mut expanded = use_signal(|| false);
    //
    rsx! {
        div {
            class: "w-full px-4",
            // header toggle
            button {
                class: "flex flex-row items-center gap-2 w-full py-2",
                onclick: move |_| {
                    let current = *expanded.read();
                    expanded.set(!current);
                },
                span {
                    class: "text-black font-medium",
                    "Completed"
                }
                span {
                    class: if *expanded.read() { "transition-transform rotate-180" } else { "transition-transform rotate-0" },
                    "⌄"
                }
            }
            // conditional list
            if *expanded.read() {
                div {
                    class: "flex flex-col gap-3 w-full",
                    // for moment in props.moments.iter() {
                    //     if moment.completed_at.is_some() {
                    //         MomentCmp {
                    //             moment: moment.clone()
                    //         }
                    //     }
                    // }
                }
            }
        }
    }
}

#[component]
pub fn MomentListCmp(props: MomentListProps) -> Element {
    let mut dragged_id = use_signal(|| None::<String>);
    // Which row is currently being dragged over — drives the drop-position
    // preview line (2026-07-28), rendered directly above that row, matching
    // exactly where ondrop below actually inserts the dragged item (it
    // always inserts before the target row, so "line above the target" is
    // never misleading about where the drop will land).
    let mut drag_over_id = use_signal(|| None::<String>);
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut sort_mode = state.sort_mode;
    let mut sort_descending = state.sort_descending;
    let mut list_density = state.list_density;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let entities = state.entities;

    let moments_list = props.moments.clone();

    // Case-insensitive, same convention as the sidebar's own entity
    // ordering — used by the "By entity" sort below to both order the
    // groups and label each one.
    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut set_sort_mode = move |mode: SortMode| {
        // Clicking the mode that's already active flips direction instead
        // of being a no-op; clicking a different mode switches to it and
        // resets to ascending (matches clicking a different table column
        // header — you don't inherit the old column's direction). Custom
        // is the one exception (2026-07-28, user's call) — it's not a
        // sortable dimension with a natural reverse, it's just whatever
        // order you dragged things into, so re-clicking it is a no-op.
        if *sort_mode.read() == mode {
            if mode == SortMode::Custom {
                return;
            }
            let flipped = !*sort_descending.read();
            sort_descending.set(flipped);
            #[cfg(not(feature = "desktop"))]
            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                storage.set("sort_descending", if flipped { "true" } else { "false" }).ok();
            }
            return;
        }
        sort_mode.set(mode);
        sort_descending.set(false);
        // Desktop has no preference persistence yet — see main.rs's startup
        // effect.
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("sort_mode", mode.as_storage_str()).ok();
            storage.set("sort_descending", "false").ok();
        }
    };

    let onConvertTo = move |id: String, mType: i64| {
        let token = auth_token;
                        let vault = active_vault;
        let note_type = mType;
        spawn(async move {
                let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                match storage.update_moment_field(id.clone(),"moment_type_id",serde_json::json!(Some(note_type.clone()))).await {
                Ok(_) => {
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                        m.moment_type_id = note_type.clone();
                    }
                    clog!("helloo?");
                }
                Err(e) => log::info!("Error updating moment: {}", e),
            }
        });
    };

    let onDelete = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let id = m.id.clone();
            match storage.delete_moment(m).await {
                Ok(()) => { moments.write().retain(|mm| mm.id != id); }
                Err(e) => log::info!("Error deleting moment: {}", e),
            }
        });
    };

    // Copies the core content (title, description, gravity, type, entity,
    // tags/project/due date via metadata) into a brand-new moment. Doesn't
    // carry over completion, depends_on, or reactions — a duplicate is a
    // fresh instance of the same content, not a clone of its history/state.
    let onDuplicate = move |m: MomentType| {
        let token = auth_token;
        let vault = active_vault;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            let new_moment = NewMomentType {
                title: m.title.clone(),
                description: m.description.clone(),
                gravity: m.gravity,
                entity_id: m.entity_id.clone(),
                moment_type_id: m.moment_type_id,
                deleted_at: None,
            };
            match storage.create_moment(new_moment).await {
                Ok(mut created) => {
                    if let Some(due) = m.due_at.clone() {
                        let _ = storage.update_moment_field(created.id.clone(), "due_at", serde_json::json!(due)).await;
                        created.due_at = Some(due);
                    }
                    if let Some(meta) = m.metadata.clone() {
                        let _ = storage.update_moment_field(created.id.clone(), "metadata", serde_json::json!(meta)).await;
                        created.metadata = Some(meta);
                    }
                    moments.write().insert(0, created);
                }
                Err(e) => log::info!("Error duplicating moment: {}", e),
            }
        });
    };

    let current_sort_mode = *sort_mode.read();
    let mut display_list: Vec<MomentType> = moments_list.clone().into_iter()
        .filter(|m| !m.completed_at.is_some() && m.moment_type_id != 3i64)
        .collect();
    // Fallback ordering when no sort_index is set yet: parses as a number since
    // ids are still Supabase bigints stringified to decimal text (see
    // types.rs's de_flex_id) — degrades to insertion order once real UUIDs
    // (post local-vault migration) make this unparseable.
    let id_as_f64 = |id: &str| id.parse::<f64>().unwrap_or(0.0);
    match current_sort_mode {
        SortMode::Default => display_list.sort_by(|a, b| id_as_f64(&a.id).partial_cmp(&id_as_f64(&b.id)).unwrap_or(std::cmp::Ordering::Equal)),
        SortMode::DueDate => display_list.sort_by(|a, b| {
            match (&a.due_at, &b.due_at) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.id.cmp(&b.id),
            }
        }),
        SortMode::Custom => display_list.sort_by(|a, b| {
            let ax = a.metadata.as_ref().and_then(|m| m.sort_index).unwrap_or_else(|| id_as_f64(&a.id));
            let bx = b.metadata.as_ref().and_then(|m| m.sort_index).unwrap_or_else(|| id_as_f64(&b.id));
            ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
        }),
        SortMode::ByEntity => display_list.sort_by(|a, b| {
            let na = entity_name(&a.entity_id).to_lowercase();
            let nb = entity_name(&b.entity_id).to_lowercase();
            na.cmp(&nb).then_with(|| id_as_f64(&a.id).partial_cmp(&id_as_f64(&b.id)).unwrap_or(std::cmp::Ordering::Equal))
        }),
    }
    // A plain reverse keeps By entity's groups contiguous (reversing a
    // list sorted by key reverses both group order and order-within-group,
    // never interleaves them), so this works uniformly for every mode
    // without needing per-mode-aware logic. Custom is excluded outright —
    // no reverse toggle exists for it (see set_sort_mode above).
    if *sort_descending.read() && current_sort_mode != SortMode::Custom {
        display_list.reverse();
    }
    let is_custom = current_sort_mode == SortMode::Custom;
    let is_by_entity = current_sort_mode == SortMode::ByEntity;
    let mut last_header_entity_id: Option<String> = None;

    let sort_btn_class = |active: bool| if active {
        "px-2 py-1 text-xs rounded-md bg-muted text-foreground font-medium cursor-pointer"
    } else {
        "px-2 py-1 text-xs rounded-md text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
    };

    // Click the active mode again to flip direction — same "click a
    // table header twice" convention, no separate control. Custom never
    // shows an arrow — it has no direction to flip.
    let dir_arrow = |mode: SortMode| if mode != SortMode::Custom && current_sort_mode == mode {
        if *sort_descending.read() { " ▼" } else { " ▲" }
    } else {
        ""
    };

    let current_density = *list_density.read();
    let mut set_density = move |d: ListDensity| {
        list_density.set(d);
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("list_density", d.as_storage_str()).ok();
        }
    };

    rsx! {
        div {
            class: "mx-4 mb-1 flex items-center justify-between gap-1",
            div {
                class: "flex items-center gap-1",
                span { class: "text-xs text-muted-foreground mr-1", "Sort:" }
                span {
                    class: sort_btn_class(current_sort_mode == SortMode::Default),
                    onclick: move |_| set_sort_mode(SortMode::Default),
                    "Default{dir_arrow(SortMode::Default)}"
                }
                span {
                    class: sort_btn_class(current_sort_mode == SortMode::DueDate),
                    onclick: move |_| set_sort_mode(SortMode::DueDate),
                    "Due date{dir_arrow(SortMode::DueDate)}"
                }
                span {
                    class: sort_btn_class(current_sort_mode == SortMode::Custom),
                    onclick: move |_| set_sort_mode(SortMode::Custom),
                    "Custom (drag to reorder){dir_arrow(SortMode::Custom)}"
                }
                span {
                    class: sort_btn_class(current_sort_mode == SortMode::ByEntity),
                    onclick: move |_| set_sort_mode(SortMode::ByEntity),
                    "By entity{dir_arrow(SortMode::ByEntity)}"
                }
            }
            div {
                class: "flex items-center gap-1",
                span {
                    class: sort_btn_class(current_density == ListDensity::Compact),
                    onclick: move |_| set_density(ListDensity::Compact),
                    "Compact"
                }
                span {
                    class: sort_btn_class(current_density == ListDensity::Full),
                    onclick: move |_| set_density(ListDensity::Full),
                    "Full"
                }
            }
        }
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            for moment in display_list.iter() {
                {
                    let moment = moment.clone();
                    let target_id = moment.id.clone();
                    let list_snapshot = display_list.clone();
                    let show_header = is_by_entity && last_header_entity_id.as_deref() != Some(moment.entity_id.as_str());
                    if show_header {
                        last_header_entity_id = Some(moment.entity_id.clone());
                    }
                    let header_name = if show_header { Some(entity_name(&moment.entity_id)) } else { None };
                    let show_drop_line = is_custom
                        && drag_over_id.read().as_deref() == Some(target_id.as_str())
                        && dragged_id.read().as_deref() != Some(target_id.as_str());
                    rsx! {
                        // Everything for one moment (optional group header, optional
                        // drop-position line, and the row itself) has to render as a
                        // single keyed node here — Dioxus's list diffing keys the
                        // per-iteration output as a unit, so when only the innermost
                        // div carried the key, a completed moment leaving `display_list`
                        // could leave its stale, faded-out DOM node in place while the
                        // next moment's content slid into the wrong slot underneath it.
                        div {
                            key: "{target_id}",
                            if let Some(name) = header_name {
                                div {
                                    class: "px-3 py-1.5 text-xs font-semibold uppercase tracking-wide text-muted-foreground bg-muted/50",
                                    "{name}"
                                }
                            }
                            if show_drop_line {
                                div { class: "h-0.5 mx-3 rounded-full bg-primary" }
                            }
                            div {
                                draggable: is_custom,
                                class: if is_custom { "cursor-move" } else { "" },
                                ondragstart: {
                                    let target_id = target_id.clone();
                                    move |_| dragged_id.set(Some(target_id.clone()))
                                },
                                ondragover: {
                                    let target_id = target_id.clone();
                                    move |e| {
                                        e.prevent_default();
                                        // The actual bug behind "drag and drop
                                        // is laggy": dragover fires continuously
                                        // (mousemove-like frequency) the whole
                                        // time the pointer sits over a row, and
                                        // every signal .set() here was
                                        // triggering a full re-render of the
                                        // entire list (drag_over_id is read by
                                        // every row's show_drop_line check) —
                                        // dozens of full-list re-renders per
                                        // second even while hovering one
                                        // perfectly still row. Only write when
                                        // the hovered row actually changes.
                                        if drag_over_id.read().as_deref() != Some(target_id.as_str()) {
                                            drag_over_id.set(Some(target_id.clone()));
                                        }
                                    }
                                },
                                ondragend: move |_| {
                                    dragged_id.set(None);
                                    drag_over_id.set(None);
                                },
                                ondrop: {
                                    let target_id = target_id.clone();
                                    move |e| {
                                        e.prevent_default();
                                        drag_over_id.set(None);
                                        let Some(from_id) = dragged_id.read().clone() else { return; };
                                        if from_id == target_id { return; }
                                        let mut order = list_snapshot.clone();
                                        let Some(from_pos) = order.iter().position(|m| m.id == from_id) else { return; };
                                        let dragged_item = order.remove(from_pos);
                                        let to_pos = order.iter().position(|m| m.id == target_id).unwrap_or(order.len());
                                        order.insert(to_pos, dragged_item);
                                        let token = auth_token;
                                        let vault = active_vault;
                                        spawn(async move {
                                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                            for (idx, m) in order.iter().enumerate() {
                                                // Reorder only ever touches sort_index — preserve everything
                                                // else already on this moment's metadata (tags, priority, ...)
                                                // rather than constructing a blank one from scratch.
                                                let mut new_meta = m.metadata.clone().unwrap_or_default();
                                                new_meta.sort_index = Some(idx as f64);
                                                if storage.update_moment_field(m.id.clone(), "metadata", serde_json::json!(new_meta)).await.is_ok() {
                                                    let mut list = moments.write();
                                                    if let Some(existing) = list.iter_mut().find(|x| x.id == m.id) {
                                                        existing.metadata = Some(new_meta);
                                                    }
                                                }
                                            }
                                        });
                                    }
                                },
                                ContextMenu {
                                    ContextMenuTrigger {
                                        MomentCmp {
                                            moment: moment.clone(),
                                        }
                                    }
                                    ContextMenuContent {
                                        if moment.moment_type_id == 1i64 {
                                            ContextMenuItem {
                                                on_select: { let id = moment.id.clone(); move |_| onConvertTo(id.clone(), 2i64) },
                                                "Convert to promise"
                                            }
                                        }
                                        if moment.moment_type_id == 2i64 {
                                            ContextMenuItem {
                                                on_select: { let id = moment.id.clone(); move |_| onConvertTo(id.clone(), 1i64) },
                                                "Convert to task"
                                            }
                                        }
                                        ContextMenuItem {
                                            on_select: { let id = moment.id.clone(); move |_| onConvertTo(id.clone(), 3i64) },
                                            "Convert to note"
                                        }
                                        ContextMenuItem {
                                            on_select: { let m = moment.clone(); move |_| onDuplicate(m.clone()) },
                                            "Duplicate"
                                        }
                                        ContextMenuItem {
                                            destructive: true,
                                            on_select: { let m = moment.clone(); move |_| onDelete(m.clone()) },
                                            "Delete"
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
pub fn MomentCmp(props: MomentCmpProps) -> Element {
    let state = use_context::<AppState>();
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut activity_bar_view = state.activity_bar_view;
    let mut backdropTgl = state.backdropTgl;
    let mut moments = state.moments;  // add this
    let mut current_moment = state.current_moment;
    let mut is_hovering = use_signal(|| false);
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut bg = match (is_hovering(), props.moment.moment_type_id == 2i64) {
        (true, true)   => BGpromiseHover,
        (true, false)  => BGhover,
        (false, true)  => BGpromise,
        (false, false) => BG,
    };
    let title = props.moment.title.clone();
    let description = props.moment.description.clone().unwrap_or_default();
    let is_promise = props.moment.moment_type_id == 2i64;
    let accent_border = if is_promise { HL } else { "transparent" };
    let moment = props.moment.clone();

    // "view options" density (2026-07-28) — Full adds this second line
    // (description preview + priority/project/tag pills); Compact (the
    // default) stays exactly what this row always looked like.
    let is_full = *state.list_density.read() == ListDensity::Full;
    let desc_preview = description.lines().next().unwrap_or("").trim().to_string();
    let priority_label = props.moment.metadata.as_ref()
        .and_then(|m| m.priority.as_deref())
        .map(|p| match p { "H" => "High", "M" => "Medium", "L" => "Low", other => other }.to_string());
    let project_label = props.moment.metadata.as_ref().and_then(|m| m.project.clone());
    let tag_labels: Vec<String> = props.moment.metadata.as_ref().map(|m| m.tags.clone()).unwrap_or_default();
    let has_full_line = is_full && (!desc_preview.is_empty() || priority_label.is_some() || project_label.is_some() || !tag_labels.is_empty());

    // Deliberately NOT a separate use_signal seeded once at mount — this
    // component instance persists across re-renders (same list position),
    // and a completion change that arrives via a different path than this
    // row's own checkbox (namely: cascade_uncomplete below, which can
    // un-complete a moment other than the one actually clicked) needs to
    // show up here without the row having been clicked itself. Reading the
    // live prop directly keeps this correct regardless of which code path
    // changed the underlying data.
    let is_completed = || props.moment.completed_at.is_some();
    let mut visual_opacity = use_signal(|| if props.moment.completed_at.is_some() { "0.4" } else { "1" });

    // due_at is stored as bare "YYYY-MM-DDTHH:MM" (no timezone/seconds) —
    // whether set through the Advanced fold's native datetime-local input
    // or quick-capture's due: keyword, nothing in the app ever produces
    // full RFC3339 for this field. Parsing only that format meant this due
    // -date label never actually rendered for any moment, ever — same bug
    // class already found and fixed once in urgency.rs, reused here instead
    // of re-deriving a second copy of the same fix.
    let due_display = props.moment.due_at.as_deref()
        .and_then(crate::urgency::parse_moment_datetime)
        .map(|dt| {
            let is_overdue = dt < chrono::Utc::now() && props.moment.completed_at.is_none();
            (dt.format("%b %d").to_string(), is_overdue)
        });

    // A moment can't be completed while what it depends on isn't — see
    // MomentType::depends_on. Previously this was purely informational
    // (shown in ab_task_cmp's "Depends on" panel, factored into urgency
    // scoring) but never actually enforced at the point of completion.
    // Notes have no completion state at all (see is_note above — no
    // checkbox is ever rendered for one), so a dependency on a note can
    // never resolve. Depending on a note is still allowed (useful as a
    // reference/context link — "this task depends on what's in that note"),
    // it just never actually blocks completion the way depending on an
    // open task or promise does.
    // A moment can depend on more than one thing now (2026-07-29, see
    // MomentType::dependency_ids) — blocked until every one of them is done.
    let unfinished_blockers: Vec<String> = props.moment.dependency_ids().iter().filter_map(|dep_id| {
        moments.read().iter().find(|m| &m.id == dep_id)
            .filter(|dep| dep.moment_type_id != 3 && dep.completed_at.is_none())
            .map(|dep| dep.title.clone())
    }).collect();
    let is_blocked = !unfinished_blockers.is_empty();
    // The list row only ever said "Blocked" with no way to tell what by —
    // the detail panel already names the blocker(s) (see ab_task_cmp's
    // "Blocked by" line), the row itself just never did.
    let blocked_on_title = if unfinished_blockers.is_empty() { None } else { Some(unfinished_blockers.join(", ")) };

    let onCheckClicked = move |checked: bool| {
        if is_blocked {
            return;
        }
        visual_opacity.set(if checked { "0" } else { "1" });  // fade to nothing
                                                              //
        let mut updated = moment.clone();
        updated.completed_at = if checked { Some(chrono::Utc::now().to_rfc3339()) } else { None };
        let token = auth_token;
                        let vault = active_vault;
        let id = updated.clone().id;
        spawn(async move {
            if checked {
                gloo_timers::future::TimeoutFuture::new(350).await;
            }
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.update_moment_field(id,"completed_at",serde_json::json!(updated.completed_at)).await {
                Ok(_) => {
                    {
                        let mut list = moments.write();
                        if let Some(m) = list.iter_mut().find(|m| m.id == updated.id) {
                            m.completed_at = updated.completed_at.clone();
                        }
                    }
                    if updated.completed_at.is_none() {
                        cascade_uncomplete(&storage, moments, updated.id.clone()).await;
                    }
                }
                Err(e) => log::info!("Error updating moment: {}", e),
            }
        });
    };

    rsx! {
        div {
            class: "flex flex-row items-center gap-3 px-4 py-3 w-full transition-colors duration-150",
            style: "background-color:{bg}; opacity:{visual_opacity}; border-left: 3px solid {accent_border}; transition: opacity 300ms ease, background-color 150ms ease;",
            onmouseleave: move |_| is_hovering.set(false) ,
            onmouseenter: move |_| is_hovering.set(true),
            onclick: move |_| {
                current_moment.set(Some(props.moment.clone()));
                activity_bar_view.set(ABView::Task);
                backdropTgl.set(true);
                activity_bar_tgl.set(true);
            },
            div {
                onclick: move |e| e.stop_propagation(),
                if !props.is_note.clone().unwrap_or(false) {
                    CheckboxCmp {
                        checked:is_completed(),
                        on_change: onCheckClicked,
                        disabled: is_blocked,
                    },
                }
            }
            div {
                class: "flex-1 min-w-0",
                h2 {
                    class: "text-sm font-medium truncate",
                    style: "color: {BaseFont};",
                    "{title}"
                }
                if is_promise {
                    span {
                        class: "text-xs font-medium",
                        style: "color: {HL};",
                        "Promise"
                    }
                }
                if is_blocked {
                    span {
                        class: "text-xs font-medium text-destructive",
                        if let Some(t) = &blocked_on_title { "Blocked by \"{t}\"" } else { "Blocked" }
                    }
                }
                if has_full_line {
                    div {
                        class: "flex items-center gap-2 mt-1 flex-wrap",
                        if !desc_preview.is_empty() {
                            span {
                                class: "text-xs text-muted-foreground truncate max-w-full",
                                "{desc_preview}"
                            }
                        }
                        if let Some(p) = &priority_label {
                            span {
                                class: "text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0",
                                "{p}"
                            }
                        }
                        if let Some(proj) = &project_label {
                            span {
                                class: "text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0",
                                "{proj}"
                            }
                        }
                        for tag in tag_labels.iter() {
                            span {
                                class: "text-xs px-1.5 py-0.5 rounded bg-muted text-muted-foreground shrink-0",
                                "+{tag}"
                            }
                        }
                    }
                }
            }
            if let Some((label, is_overdue)) = due_display {
                span {
                    class: if is_overdue {
                        "text-xs font-medium text-destructive shrink-0"
                    } else {
                        "text-xs text-muted-foreground shrink-0"
                    },
                    "{label}"
                }
            }
        }
    }
}


#[component]
pub fn MomentInputCmp() -> Element {
    let state = use_context::<AppState>();
    let mut momentInputTgl = state.momentInputTgl;
    let mut moments = state.moments;
    let mut entities = state.entities;
    let mut title = use_signal(|| String::new());
    let current_entity = state.current_entity;
    let project_filter = state.project_filter;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut selected_entity = use_signal(|| None::<String>);
    // Description had a signal declared here before but nothing in this
    // component ever rendered a field for it — form_data.description (what
    // actually gets submitted, below) was always empty. Tab now reveals a
    // real field and focuses it, bound directly to form.description.
    let mut description_open = use_signal(|| false);
    let mut description_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut title_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let focus_composer = state.focus_composer;
    let mut last_seen_focus_req = use_signal(move || *focus_composer.read());

    // Two MomentInputCmp instances are mounted at once (the mobile popup
    // one here in the layout, and the desktop one in views/home.rs) — only
    // one is ever actually visible at a given viewport width (Tailwind's
    // hidden/xl:block vs xl:hidden), so set_focus on the other is a
    // harmless no-op. Compares against the last-seen counter value instead
    // of reacting unconditionally, since use_effect also runs once on
    // mount and this must not steal focus on first render.
    use_effect(move || {
        let current = *focus_composer.read();
        if current != *last_seen_focus_req.read() {
            last_seen_focus_req.set(current);
            let el = title_el.read().clone();
            spawn(async move {
                if let Some(el) = el {
                    let _ = el.set_focus(true).await;
                }
            });
        }
    });

    let mut form = use_signal(move || {
        let mut f = MomentForm::default();

        if let Some(entity) = current_entity.read().clone() {
            f.entity_sel = entity.id.clone();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            f.entity_sel = active_vault.read().effective(&auth_token.read()).resolve_self_entity_id(&entities.read()).unwrap_or_default();
        }


        f
    });

    use_effect(move || {
        if let Some(entity) = current_entity.read().clone() {
            form.write().entity_sel = entity.id.clone();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            form.write().entity_sel = active_vault.read().effective(&auth_token.read()).resolve_self_entity_id(&entities.read()).unwrap_or_default();
        }
    });

    // Taskwarrior-style quick capture (see quick_capture.rs): the title
    // field itself carries "priority:H", "due:tomorrow", "@Jane", etc, and
    // submit_moment parses those back out instead of taking form.title
    // literally. An @-mention, when present, overrides the entity dropdown.
    let mut submit_moment = move || {
        let raw_title = title.read().clone();
        let entities_snapshot = entities.read().clone();
        let parsed = quick_capture::parse(&raw_title, &entities_snapshot);

        let form_data = form.read().clone();

        let mut reset_form = MomentForm::default();

        if let Some(entity) = current_entity.read().clone() {
            selected_entity.set(Some(entity.name));
            reset_form.entity_sel = entity.id.clone();
        }else{
            // selected_entity.set("Self".to_string());
            reset_form.entity_sel = form_data.entity_sel.clone();
        }

        form.set(reset_form);
        title.set(String::new());
        description_open.set(false);
        // Refocus the composer so the next moment can be typed straight
        // away — most noticeable after the expanded (description-open)
        // bar's "Add Moment" button, which otherwise left focus nowhere.
        {
            let el = title_el.read().clone();
            spawn(async move {
                if let Some(el) = el {
                    let _ = el.set_focus(true).await;
                }
            });
        }
        let entity_id = parsed.entity_id.clone().unwrap_or(form_data.entity_sel.clone());
        // Mirrors entity_id's default above: creating a moment while
        // browsing a specific project should land it in that project
        // unless project:/pro: explicitly says otherwise, same as how
        // browsing an entity already defaults new moments to them.
        let default_project = project_filter.read().clone();
        let token = auth_token;
                        let vault = active_vault;

        spawn(async move {
            let new_moment = NewMomentType {
                title: parsed.title.clone(),
                entity_id,
                description: Some(form_data.description.clone()),
                // 0, not 1 — matches both the "unset" default read elsewhere
                // (live_moment.gravity.unwrap_or(0)) and the new -10..10
                // select's step-of-10 grid (see ui.rs's gravity_select); a
                // raw 1 doesn't land on any option in that grid, so new
                // moments were visually showing gravity as "-10" (the
                // browser's fallback to the first out-of-range option)
                // instead of looking unset.
                gravity: Some(0),
                moment_type_id: parsed.moment_type_id.unwrap_or(1),
                deleted_at: None,
            };
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.create_moment(new_moment).await {
                Ok(created_moment) => {
                    let created_id = created_moment.id.clone();
                    let created_entity_id = created_moment.entity_id.clone();
                    moments.write().insert(0, created_moment);
                    // depends_on_title is a raw typed string, not yet an id
                    // (quick_capture.rs has no moments list to resolve it
                    // against) — match it against the target entity's open
                    // moments here, where the real list is available. Feeds
                    // into metadata.depends_on below rather than the legacy
                    // single-dependency column (see MomentType::dependency_ids).
                    let dep_ids: Vec<String> = parsed.depends_on_title.clone()
                        .and_then(|dep_title| {
                            moments.read().iter()
                                .find(|m| m.entity_id == created_entity_id
                                    && m.id != created_id
                                    && m.completed_at.is_none()
                                    && m.title.to_lowercase() == dep_title.to_lowercase())
                                .map(|m| m.id.clone())
                        })
                        .into_iter()
                        .collect();
                    let effective_project = parsed.project.clone().or_else(|| default_project.clone());
                    if parsed.has_metadata() || effective_project.is_some() || !dep_ids.is_empty() {
                        let meta = MomentMetadata {
                            tags: parsed.tags_add.clone(),
                            sort_index: None,
                            priority: parsed.priority.clone(),
                            project: effective_project,
                            scheduled_at: parsed.scheduled_at.clone(),
                            until_at: parsed.until_at.clone(),
                            depends_on: dep_ids,
                            additional_entity_ids: Vec::new(),
                        };
                        if storage.update_moment_field(created_id.clone(), "metadata", serde_json::json!(meta)).await.is_ok() {
                            if let Some(m) = moments.write().iter_mut().find(|m| m.id == created_id) {
                                m.metadata = Some(meta);
                            }
                        }
                    }
                    if let Some(due) = parsed.due_at.clone() {
                        if storage.update_moment_field(created_id.clone(), "due_at", serde_json::json!(Some(due.clone()))).await.is_ok() {
                            if let Some(m) = moments.write().iter_mut().find(|m| m.id == created_id) {
                                m.due_at = Some(due);
                            }
                        }
                    }
                }
                Err(e) => clog!("Error: {}", e),
            }
        });
    };
    //
    rsx! {

        div {
            class: "mx-4 my-2 rounded-xl border border-border bg-muted/20 shadow-sm p-4",
            div {
                class: "flex items-center gap-2",
                div {
                    class: "flex-1",
                    QuickCaptureInput {
                        value: title.read().clone(),
                        // Not documenting the taskwarrior-style syntax here on
                        // purpose — it read as confusing noise to new users.
                        // Power users find @-mentions/pri:/due:/+tags/;t;/;p;/;n;
                        // on their own (or from docs), same as any other
                        // power-user shortcut isn't advertised in a plain
                        // placeholder.
                        placeholder: "What's on your mind?".to_string(),
                        entities: entities.read().clone(),
                        moments: moments.read().clone(),
                        on_input: move |v: String| {
                            // A completed @mention should be reflected on
                            // the right-hand selector immediately, not just
                            // silently honored at submit time — otherwise
                            // there's no visible confirmation of who the
                            // moment is actually going to before you hit
                            // enter.
                            if let Some(entity_id) = quick_capture::parse(&v, &entities.read()).entity_id {
                                if let Some(entity) = entities.read().iter().find(|e| e.id == entity_id) {
                                    selected_entity.set(Some(entity.name.clone()));
                                    form.write().entity_sel = entity_id.clone();
                                }
                            }
                            title.set(v);
                        },
                        on_submit: move |_| submit_moment(),
                        on_tab: move |_| {
                            description_open.set(true);
                            let el = description_el.read().clone();
                            spawn(async move {
                                if let Some(el) = el {
                                    let _ = el.set_focus(true).await;
                                }
                            });
                        },
                        on_add_entity: move |name: String| {
                            // Bare name only, nothing else — the whole
                            // point is skipping the New Entity form for
                            // quick capture. Details can be filled in later
                            // from the Info panel.
                            let token = auth_token;
                            let vault = active_vault;
                            spawn(async move {
                                let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                let new_entity = NewEntityType {
                                    name,
                                    entity_type_id: None,
                                    parent_entity_id: None,
                                    user_id: None,
                                    archived_at: None,
                                    metadata: None,
                                };
                                match storage.create_entity(new_entity).await {
                                    Ok(created) => {
                                        entities.write().insert(0, created);
                                    }
                                    Err(e) => clog!("Error creating entity: {}", e),
                                }
                            });
                        },
                        input_el: title_el,
                    }
                }
                Dropdown {
                    DropdownTrigger {
                        Button {
                            variant: ButtonVariant::Secondary,
                            size: ButtonSize::Medium,
                            style: "height: 2.5rem;",
                            {selected_entity.read().clone().unwrap_or("Self".to_string())}
                            " ⌄"
                        }
                    }
                    DropdownContent {
                        align: "end",
                        // Self is already the default option (the trigger
                        // label above) — omit it here so it isn't offered
                        // twice.
                        for entity in entities.iter().filter(|e| !is_self_entity(e)) {
                            DropdownItem::<String> {
                                value:  "{entity.id}".to_string(),
                                index: 0,
                                on_select: {
                                    let name = entity.name.clone();
                                    let id = entity.id.clone();
                                    move |_| {
                                        *selected_entity.write() = Some(name.clone());
                                        form.write().entity_sel = id.clone().to_string();
                                    }
                                },
                                "{entity.name}"
                            }
                        }
                    }
                }
            }

            // Always mounted (never conditionally removed) so its
            // MountedData is captured exactly once and stays valid for
            // every later Tab press — visually toggled instead, matching
            // the always-mounted-plus-class-toggle pattern the mobile
            // input popup already uses elsewhere in this file.
            textarea {
                class: if *description_open.read() {
                    "w-full mt-2 min-h-20 rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring resize-y"
                } else {
                    "hidden"
                },
                placeholder: "Description...",
                value: "{form.read().description}",
                onmounted: move |e| description_el.set(Some(e.data())),
                oninput: move |e| form.write().description = e.value(),
            }

            // Desktop-only counterpart to the "Add Moment" button below
            // (that one's xl:hidden — mobile only). Tabbing into the
            // description textarea had nowhere to go from there: Enter in a
            // textarea means newline, not submit, and there was no visible
            // button on desktop to tab to or click — so finishing a moment
            // that had a description meant tabbing/clicking all the way
            // back up to the title field. Only shown once the description
            // is actually open, same visibility condition as the textarea
            // itself.
            if *description_open.read() {
                button {
                    class: "hidden xl:block w-full mt-2 rounded-md py-2 text-sm font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer",
                    style: "background-color:{HL};",
                    onclick: move |_| submit_moment(),
                    "Add Moment",
                }
            }

            button {
                class: "xl:hidden block w-full mt-2 rounded-md py-2 text-sm font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer",
                style: "background-color:{HL};",
                onclick: move |e| submit_moment(),
                "Add Moment",
            }
        }
    }
}


#[derive(Props, Clone, PartialEq)]
pub struct QuickCaptureInputProps {
    pub value: String,
    pub placeholder: String,
    pub entities: Vec<EntityType>,
    // For the depends:/deps: dropdown — matched by title, same live-search
    // treatment as @mention gets for entities, so "no way I'm going to
    // remember whole task names" isn't a real constraint anymore.
    pub moments: Vec<MomentType>,
    pub on_input: EventHandler<String>,
    pub on_submit: EventHandler<()>,
    // Tab, when the @-mention dropdown isn't open (Tab has its existing job
    // there — confirming the highlighted match), jumps to the description
    // field instead of doing the browser's default focus-next-element
    // thing: title, then keep typing a description, without touching the
    // mouse.
    pub on_tab: EventHandler<()>,
    // Fired with a bare name (no id yet) when "+ Add <name>" is chosen from
    // the dropdown — see MomentInputCmp's wiring for the actual create
    // call. Quick-capture already knows how to *reference* a person via
    // @mention; this is the same gesture extended to *creating* one, so
    // adding someone never requires leaving the composer for the full
    // New Entity form.
    pub on_add_entity: EventHandler<String>,
    // Lifted up from this component (rather than a purely internal
    // use_signal) so a parent that just submitted a moment can refocus this
    // input itself — see MomentInputCmp's submit_moment, which needs to
    // call set_focus on this exact element after a submission clears it.
    pub input_el: Signal<Option<std::rc::Rc<MountedData>>>,
}

// The title input for MomentInputCmp, with live taskwarrior-style syntax
// highlighting and an @-mention entity chooser. Built as a colored overlay
// div stacked on top of a real <input> whose own text is transparent (only
// its caret shows) — a plain <input> can't render multi-colored text, and a
// contenteditable div would need its own from-scratch cursor/selection
// handling, so this "invisible input + decorative backdrop" trick is the
// standard lightweight way to fake a syntax-highlighted text field.
//
// The @-mention dropdown is driven purely by trailing_mention_query(), i.e.
// "is the last word of the string a live @fragment" — not real cursor
// position. That's deliberate: quick capture is typed left-to-right at the
// end of the field, and avoiding DOM selection APIs sidesteps the
// web-vs-desktop web_sys split (see clog!/window() gating elsewhere in this
// file) entirely.
fn apply_selected_mention(value: &str, mention_start: usize, matches: &[EntityType], idx: usize, on_input: EventHandler<String>) {
    if let Some(entity) = matches.get(idx) {
        on_input.call(quick_capture::apply_mention(value, mention_start, &entity.name));
    }
}

fn apply_add_new_mention(value: &str, mention_start: usize, name: &str, on_input: EventHandler<String>, on_add_entity: EventHandler<String>) {
    on_input.call(quick_capture::apply_mention(value, mention_start, name));
    on_add_entity.call(name.to_string());
}

#[component]
pub fn QuickCaptureInput(props: QuickCaptureInputProps) -> Element {
    let mut highlighted = use_signal(|| 0usize);
    let value = props.value.clone();
    let on_input = props.on_input;
    let on_submit = props.on_submit;
    let on_tab = props.on_tab;
    let on_add_entity = props.on_add_entity;

    // The real <input>'s text scrolls internally once typing overflows the
    // visible width (native browser behavior) — invisible here since its
    // text is transparent, but the colored overlay below is a totally
    // separate div with no knowledge of that scroll at all, so it just sat
    // frozen while the (invisible) real caret kept advancing off-screen.
    // Mirrors the real input's scrollLeft onto the overlay's text via a
    // transform on every keystroke/click, so the two stay visually locked
    // together instead of drifting apart — which on iOS/Safari is also
    // almost certainly why a cursor-like element looked "unattached" from
    // the input box: the overlay text the user was actually looking at
    // wasn't moving while the real (invisible) input + native caret
    // scrolled correctly underneath it.
    let mut input_el = props.input_el;
    let mut scroll_x = use_signal(|| 0.0f64);
    let sync_scroll = move || {
        spawn(async move {
            let el = input_el.read().clone();
            if let Some(el) = el {
                if let Ok(offset) = el.get_scroll_offset().await {
                    scroll_x.set(offset.x);
                }
            }
        });
    };

    let tokens = quick_capture::tokenize(&value, &props.entities);

    let mention = quick_capture::trailing_mention_query(&value);
    // Mutually exclusive with @mentions — both are "is the trailing word a
    // live fragment of a specific prefix", and a word can only start with
    // one prefix at a time, so only check for a depends: query when there
    // isn't already a mention in progress.
    let depends_query = if mention.is_none() { quick_capture::trailing_depends_query(&value) } else { None };
    let is_depends_mode = depends_query.is_some();
    let dep_matches: Vec<MomentType> = match &depends_query {
        Some(d) => {
            let q_lower = d.query.to_lowercase();
            let mut list: Vec<MomentType> = props.moments.iter()
                .filter(|m| m.completed_at.is_none())
                .filter(|m| m.title.to_lowercase().contains(&q_lower))
                .cloned()
                .collect();
            list.sort_by_key(|m| (!m.title.to_lowercase().starts_with(&q_lower), m.title.to_lowercase()));
            list.truncate(8);
            list
        }
        None => Vec::new(),
    };
    let depends_start = depends_query.map(|d| d.start).unwrap_or(0);

    let mention_query = mention.as_ref().map(|m| m.query.to_string());
    let matches: Vec<EntityType> = match &mention {
        Some(m) => {
            let q_lower = m.query.to_lowercase();
            let mut list: Vec<EntityType> = props.entities.iter()
                .filter(|e| !e.name.is_empty() && e.name.to_lowercase().contains(&q_lower))
                .cloned()
                .collect();
            list.sort_by_key(|e| (!e.name.to_lowercase().starts_with(&q_lower), e.name.to_lowercase()));
            list.truncate(8);
            list
        }
        None => Vec::new(),
    };
    let mention_start = mention.map(|m| m.start).unwrap_or(0);
    // Offer "+ Add <name>" whenever there's a non-empty @query with no
    // exact (case-insensitive) name match — even alongside other fuzzy
    // substring matches, since the typed text might still be a genuinely
    // new, distinct person rather than any of those.
    let show_add_new = mention_query.as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty())
        .is_some_and(|q| !matches.iter().any(|e| e.name.to_lowercase() == q.to_lowercase()));
    let add_new_name = mention_query.as_deref().map(str::trim).unwrap_or("").to_string();
    // is_depends_mode alone (not just !dep_matches.is_empty()) keeps the
    // dropdown open even with zero candidates, so "No matching open tasks"
    // below can actually render instead of just silently doing nothing.
    let dropdown_open = !matches.is_empty() || show_add_new || is_depends_mode;
    let option_count = if is_depends_mode { dep_matches.len() } else { matches.len() + if show_add_new { 1 } else { 0 } };
    if dropdown_open && *highlighted.read() >= option_count {
        highlighted.set(0);
    }

    rsx! {
        div {
            class: "relative flex-1",
            input {
                r#type: "text",
                name: "task_title",
                class: "flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                style: "color: transparent; caret-color: {BaseFont};",
                placeholder: "{props.placeholder}",
                value: "{value}",
                onmounted: move |e| input_el.set(Some(e.data())),
                oninput: move |e| {
                    on_input.call(e.value());
                    sync_scroll();
                },
                onclick: move |_| sync_scroll(),
                onkeyup: move |_| sync_scroll(),
                onkeydown: {
                    let value = value.clone();
                    let matches = matches.clone();
                    let add_new_name = add_new_name.clone();
                    let dep_matches = dep_matches.clone();
                    move |e: Event<KeyboardData>| {
                        if dropdown_open && option_count > 0 {
                            match e.key() {
                                Key::ArrowDown => {
                                    e.prevent_default();
                                    let next = (*highlighted.read() + 1) % option_count;
                                    highlighted.set(next);
                                }
                                Key::ArrowUp => {
                                    e.prevent_default();
                                    let h = *highlighted.read();
                                    highlighted.set(if h == 0 { option_count - 1 } else { h - 1 });
                                }
                                Key::Enter | Key::Tab => {
                                    e.prevent_default();
                                    let h = *highlighted.read();
                                    if is_depends_mode {
                                        if let Some(m) = dep_matches.get(h) {
                                            on_input.call(quick_capture::apply_depends(&value, depends_start, &m.title));
                                        }
                                    } else if h < matches.len() {
                                        apply_selected_mention(&value, mention_start, &matches, h, on_input);
                                    } else if show_add_new {
                                        apply_add_new_mention(&value, mention_start, &add_new_name, on_input, on_add_entity);
                                    }
                                }
                                _ => {}
                            }
                        } else if e.key() == Key::Enter {
                            on_submit.call(());
                        } else if e.key() == Key::Tab {
                            e.prevent_default();
                            on_tab.call(());
                        }
                    }
                },
            }
            div {
                class: "absolute inset-0 flex items-center pointer-events-none px-3 text-sm whitespace-pre overflow-hidden",
                div {
                    style: "transform: translateX(-{scroll_x}px);",
                    for (i, token) in tokens.iter().enumerate() {
                        span {
                            key: "{i}",
                            style: if token.kind.is_recognized() { format!("color: {HL}; font-weight: 600;") } else { format!("color: {BaseFont};") },
                            "{token.text}"
                        }
                    }
                }
            }
            if dropdown_open {
                div {
                    class: "absolute left-0 right-0 top-full mt-1 z-20 rounded-md border border-border bg-background shadow-md overflow-hidden",
                    for (i, entity) in matches.iter().enumerate() {
                        div {
                            key: "{entity.id}",
                            class: if i == *highlighted.read() { "px-3 py-1.5 text-sm cursor-pointer bg-muted" } else { "px-3 py-1.5 text-sm cursor-pointer" },
                            onmousedown: {
                                let value = value.clone();
                                let matches = matches.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    apply_selected_mention(&value, mention_start, &matches, i, on_input);
                                }
                            },
                            "{entity.name}"
                        }
                    }
                    if show_add_new {
                        div {
                            class: if matches.len() == *highlighted.read() { "px-3 py-1.5 text-sm cursor-pointer bg-muted font-medium" } else { "px-3 py-1.5 text-sm cursor-pointer font-medium" },
                            style: "color: {HL};",
                            onmousedown: {
                                let value = value.clone();
                                let add_new_name = add_new_name.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    apply_add_new_mention(&value, mention_start, &add_new_name, on_input, on_add_entity);
                                }
                            },
                            "+ Add \"{add_new_name}\""
                        }
                    }
                    for (i, dep) in dep_matches.iter().enumerate() {
                        div {
                            key: "{dep.id}",
                            class: if i == *highlighted.read() { "px-3 py-1.5 text-sm cursor-pointer bg-muted" } else { "px-3 py-1.5 text-sm cursor-pointer" },
                            onmousedown: {
                                let value = value.clone();
                                let title = dep.title.clone();
                                move |e: Event<MouseData>| {
                                    e.prevent_default();
                                    on_input.call(quick_capture::apply_depends(&value, depends_start, &title));
                                }
                            },
                            "{dep.title}"
                        }
                    }
                    if is_depends_mode && dep_matches.is_empty() {
                        div {
                            class: "px-3 py-1.5 text-sm text-muted-foreground",
                            "No matching open tasks"
                        }
                    }
                }
            }
        }
    }
}


#[component]
pub fn ab_task_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut current_moment = state.current_moment;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let entities = state.entities;
    let mut reassign_error = use_signal(|| None::<String>);
    // Searchable depends-on picker state (replaces a plain <select> that
    // used to be scoped to only the current entity's own moments — see the
    // dependency_candidates comment below). Reset whenever the panel
    // switches to a different moment, same pattern as entity.rs's
    // confirming_delete reset.
    let mut depends_search = use_signal(String::new);
    let mut depends_dropdown_open = use_signal(|| false);
    // Multi-entity moments (2026-07-29) — same searchable chip-add pattern,
    // for entities besides the primary one this moment also fully belongs
    // to (see MomentType::entity_ids).
    let mut additional_search = use_signal(String::new);
    let mut additional_dropdown_open = use_signal(|| false);
    // Full-screen title+description editor (desktop only — mobile already
    // gets a full-width slide-out panel for this) — the compact panel's own
    // title input and ~8-row textarea aren't enough room to actually read
    // or write a real note. Modal overlay, not a route change: quick in/out,
    // same moment, same save handlers as the compact fields below. The
    // signal lives on AppState and the modal itself renders from
    // FullScreenEditorModalCmp at the top level (see its own doc comment
    // for why) — this component only sets the flag.
    let mut full_editor_open = state.full_editor_open;

    // Every call site that opens this panel sets current_moment in the same
    // handler, so this shouldn't be reachable — but that's a convention, not
    // something the type system enforces, so fail to an empty panel instead
    // of panicking if a future change ever breaks it.
    let Some(moment) = current_moment.read().clone() else {
        return rsx! {};
    };
    let moment_sig = use_signal(|| moment.clone());
    let mut reactions = use_signal(|| moment.reactions.clone().unwrap_or_default());
    let id = moment.id.clone();
    // Read live off `moments` by id, not off the one-time `moment` snapshot
    // — same staleness class already fixed for depends_on/metadata below.
    // Any moments.write() while this panel is open (including this panel's
    // own other field edits) re-renders this component, and a field still
    // computed from the stale snapshot snaps back to whatever it was when
    // the panel first opened — gravity_select visibly "resetting to 1"
    // after every edit was this exact bug.
    let live_moment = moments.read().iter().find(|m| m.id == id).cloned().unwrap_or_else(|| moment.clone());
    let description = live_moment.description.clone();
    let title = live_moment.title.clone();
    let gravity = live_moment.gravity.unwrap_or(0);
    let due_at = live_moment.due_at.clone();
    let mut ReactionForm = use_signal(ReactionForm::default);

    let moment_kind = match moment.moment_type_id {
        2i64 => "Promise",
        3i64 => "Note",
        _ => "Task",
    };

    let mut tag_input = use_signal(|| String::new());
    let mut moment_tags = use_signal(|| moment.metadata.clone().unwrap_or_default().tags);

    // Every closure below that touches `id` gets its own clone made right
    // before the closure literal (not just inside the closure body) — `id`
    // is a String (not Copy), and a `move` closure takes full ownership of
    // whatever outer variable it references, so without a dedicated clone
    // per closure only the first one in source order would compile; the
    // rest would find `id` already moved away.
    let id_for_tags = id.clone();
    let mut save_tags = move |new_tags: Vec<String>| {
        let id = id_for_tags.clone();
        let token = auth_token;
                        let vault = active_vault;
        // moment_sig is a snapshot taken once when the panel opened and
        // never updated by any field-patch handler (same staleness class
        // fixed for depends_on above) — read the *current* metadata live by
        // id instead, so saving tags can't clobber a priority/project/
        // scheduled/until edit made earlier in this same panel session.
        let mut new_meta = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();
        new_meta.tags = new_tags.clone();
        moment_tags.set(new_tags);
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await {
                Ok(_) => {
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                        m.metadata = Some(new_meta.clone());
                    }
                }
                Err(e) => log::info!("Error updating tags: {}", e),
            }
        });
    };

    // Taskwarrior-style dependencies (see MomentType::dependency_ids) — a
    // moment can depend on more than one thing now (2026-07-29), unlimited
    // like tags. Read live off the `moments` signal by id rather than off
    // the `moment` snapshot taken once from current_moment at panel-open
    // time — save_deps below updates `moments` but never `current_moment`
    // itself, so a snapshot read would go stale the instant a dependency
    // changes without closing and reopening the panel.
    // Any open task/promise can be a dependency, regardless of which entity
    // it's attributed to — a moment scoped to "only this entity's own
    // moments" (the old behavior) doesn't match how depends_on is actually
    // used; a thing you're waiting on is a thing you're waiting on no
    // matter whose it is.
    let current_dep_ids: Vec<String> = moments.read().iter().find(|m| m.id == id)
        .map(|m| m.dependency_ids())
        .unwrap_or_default();
    let dependency_candidates: Vec<MomentType> = moments.read().iter()
        .filter(|m| m.id != id && m.completed_at.is_none() && !current_dep_ids.contains(&m.id))
        .cloned()
        .collect();

    let id_for_deps = id.clone();
    let mut save_deps = move |new_deps: Vec<String>| {
        let id = id_for_deps.clone();
        let token = auth_token;
        let vault = active_vault;
        // Same "read the live metadata by id, don't clobber another field
        // edited earlier this panel session" pattern as save_tags above.
        let mut new_meta = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();
        new_meta.depends_on = new_deps;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await {
                Ok(_) => {
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                        m.metadata = Some(new_meta.clone());
                    }
                }
                Err(e) => log::info!("Error updating dependencies: {}", e),
            }
        });
    };

    let blocked_on: Vec<MomentType> = current_dep_ids.iter().filter_map(|dep_id| {
        moments.read().iter().find(|m| &m.id == dep_id).cloned()
    }).collect();
    // Notes have no completion state (no checkbox is ever rendered for
    // one), so a dependency on a note can never resolve. Depending on a
    // note is still allowed — useful as a reference/context link — it just
    // never actually blocks completion the way a task/promise dependency
    // does. Symmetrically, a note can't "block" anything it's depended on
    // by either, for the same reason.
    let is_blocked = blocked_on.iter().any(|dep| dep.moment_type_id != 3 && dep.completed_at.is_none());
    let blocking_count = if moment.moment_type_id == 3 {
        0
    } else {
        moments.read().iter()
            .filter(|m| m.dependency_ids().contains(&id) && m.completed_at.is_none())
            .count()
    };

    // Multi-entity moments (2026-07-29) — additional entities this moment
    // also fully belongs to, besides the primary one above (full peers, not
    // lightweight tag-alongs: see MomentType::entity_ids/involves_entity).
    // Same live-by-id-read/save-to-metadata pattern as dependencies above.
    let current_additional_ids: Vec<String> = moments.read().iter().find(|m| m.id == id)
        .map(|m| m.metadata.as_ref().map(|meta| meta.additional_entity_ids.clone()).unwrap_or_default())
        .unwrap_or_default();
    let additional_entity_candidates: Vec<EntityType> = entities.read().iter()
        .filter(|e| !is_self_entity(e) && e.id != live_moment.entity_id && !current_additional_ids.contains(&e.id))
        .cloned()
        .collect();

    let id_for_additional = id.clone();
    let mut save_additional_entities = move |new_ids: Vec<String>| {
        let id = id_for_additional.clone();
        let token = auth_token;
        let vault = active_vault;
        let mut new_meta = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();
        new_meta.additional_entity_ids = new_ids;
        spawn(async move {
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await {
                Ok(_) => {
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                        m.metadata = Some(new_meta.clone());
                    }
                }
                Err(e) => log::info!("Error updating additional entities: {}", e),
            }
        });
    };
    let current_additional_entities: Vec<EntityType> = current_additional_ids.iter().filter_map(|eid| {
        entities.read().iter().find(|e| &e.id == eid).cloned()
    }).collect();

    // Taskwarrior-style attributes, part 2 (priority/project/scheduled/
    // until) — same live-by-id read as current_depends_on above, for the
    // same reason: the moment snapshot taken at panel-open time never
    // updates as fields get edited within the same open session.
    let current_metadata = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();

    let mut advanced_open = use_signal(|| false);

    rsx! {
        div {
            class: "flex flex-col h-full bg-background",

            div {
                class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                span {
                    class: "text-sm font-medium text-muted-foreground",
                    "{moment_kind}"
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
                class: "flex flex-col gap-4 px-4 py-4 pb-[200px] overflow-y-auto flex-1 min-h-0",

                if moment.moment_type_id != 3i64 {
                    div {
                        class: "flex items-center gap-3",
                        // The underlying <input type="checkbox">'s live DOM
                        // `checked` property can decouple from the declared
                        // value once a user has actually clicked it —
                        // switching to a different moment reuses the same
                        // persisted DOM node (this panel doesn't remount
                        // between moments, only between open/close), so the
                        // checkbox kept showing the *previous* moment's
                        // checked state until manually toggled twice. A
                        // `key` prop on a single always-present child isn't
                        // honored by Dioxus's diffing the way it is for
                        // siblings inside a `for` — so this uses the same
                        // single-iteration `for`-with-key pattern already
                        // proven to force a real remount elsewhere in this
                        // file (see MomentListCmp's drag-and-drop rows),
                        // which guarantees a genuinely fresh DOM node (and
                        // thus a fresh, undirtied `checked` property) every
                        // time the moment being shown changes.
                        for _key in [id.clone()] {
                        CheckboxCmp {
                            key: "{_key}",
                            checked: live_moment.completed_at.is_some(),
                            disabled: is_blocked,
                            on_change: {
                                let id = id.clone();
                                move |checked| {
                                    if is_blocked {
                                        return;
                                    }
                                    let id = id.clone();
                                    let token = auth_token;
                        let vault = active_vault;
                                    spawn(async move {
                                        let completed_at = if checked { Some(chrono::Utc::now().to_rfc3339()) } else { None };
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "completed_at", serde_json::json!(completed_at)).await {
                                            Ok(_) => {
                                                {
                                                    let mut list = moments.write();
                                                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                        m.completed_at = completed_at.clone();
                                                    }
                                                }
                                                if completed_at.is_none() {
                                                    cascade_uncomplete(&storage, moments, id.clone()).await;
                                                }
                                            }
                                            Err(e) => clog!("Error updating moment: {}", e),
                                        }
                                    });
                                }
                            }
                        }
                        }
                        // A single `datetime-local` input can't be set from
                        // just a date — the browser treats it as incomplete
                        // (and reports an empty value) until both the date
                        // and time sub-fields are filled in, which is what
                        // "setting a due date without a time doesn't work"
                        // actually was. Split into two real inputs instead;
                        // time is optional and defaults to midnight, date is
                        // still required to have a due_at at all. Combining
                        // reuses whichever half didn't just change from the
                        // moment's current due_at, same bare
                        // "YYYY-MM-DDTHH:MM" storage format as before.
                        input {
                            r#type: "date",
                            class: "rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{due_at.clone().unwrap_or_default().chars().take(10).collect::<String>()}",
                            // oninput, not onchange — onchange only fires on blur, so
                            // closing the activity panel right after picking a date
                            // (without clicking elsewhere first) silently dropped it.
                            oninput: {
                                let id = id.clone();
                                let due_at = due_at.clone();
                                move |e: Event<FormData>| {
                                    let id = id.clone();
                                    let token = auth_token;
                                    let vault = active_vault;
                                    let date = e.value();
                                    let time = due_at.as_deref().and_then(|d| d.get(11..16)).unwrap_or("00:00").to_string();
                                    let new_due = if date.is_empty() { None } else { Some(format!("{date}T{time}")) };
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "due_at", serde_json::json!(new_due)).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.due_at = new_due;
                                                }
                                            }
                                            Err(e) => log::info!("Error updating moment: {}", e),
                                        }
                                    });
                                }
                            }
                        }
                        input {
                            r#type: "time",
                            class: "rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                            value: "{due_at.clone().and_then(|d| d.get(11..16).map(str::to_string)).unwrap_or_default()}",
                            oninput: {
                                let id = id.clone();
                                let due_at = due_at.clone();
                                move |e: Event<FormData>| {
                                    let id = id.clone();
                                    let token = auth_token;
                                    let vault = active_vault;
                                    let date = due_at.as_deref().map(|d| d.chars().take(10).collect::<String>()).unwrap_or_default();
                                    if date.is_empty() {
                                        // No date set yet — a bare time means
                                        // nothing, so there's nothing to save.
                                        return;
                                    }
                                    let time = if e.value().is_empty() { "00:00".to_string() } else { e.value() };
                                    let new_due = Some(format!("{date}T{time}"));
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "due_at", serde_json::json!(new_due)).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.due_at = new_due;
                                                }
                                            }
                                            Err(e) => log::info!("Error updating moment: {}", e),
                                        }
                                    });
                                }
                            }
                        }
                    }
                }

                {
                    rsx! {
                        div {
                            class: "flex items-center gap-1",
                            input {
                                class: "text-xl font-semibold text-foreground w-full bg-transparent border-none outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md -mx-1 px-1 py-1",
                                value: "{title}",
                                // Live, not on blur — closing the activity panel right
                                // after typing (without clicking away first) was silently
                                // discarding the edit.
                                oninput: {
                                    let id = id.clone();
                                    move |e| {
                                        let id = id.clone();
                                        let token = auth_token;
                                        let vault = active_vault;
                                        let val = e.value();
                                        spawn(async move {
                                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                            match storage.update_moment_field(id.clone(), "title", serde_json::json!(val)).await {
                                                Ok(_) => {
                                                    let mut list = moments.write();
                                                    if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                        m.title = val;
                                                    }
                                                }
                                                Err(e) => log::info!("Error updating moment: {}", e),
                                            }
                                        });
                                    }
                                },
                            }
                            button {
                                class: "hidden xl:flex shrink-0 h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer",
                                title: "Expand to full-screen editor",
                                onclick: move |_| full_editor_open.set(true),
                                fa_expand {}
                            }
                        }

                        textarea {
                            class: "w-full min-h-32 rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring resize-y",
                            placeholder: "Add a description...",
                            value: "{description.clone().unwrap_or_default()}",
                            oninput: {
                                let id = id.clone();
                                move |e| {
                                    let id = id.clone();
                                    let token = auth_token;
                                    let vault = active_vault;
                                    let val = e.value();
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "description", serde_json::json!(val)).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.description = Some(val);
                                                }
                                            }
                                            Err(e) => log::info!("Error updating moment: {}", e),
                                        }
                                    });
                                }
                            },
                        }

                    }
                }

                div {
                    class: "flex items-center justify-between rounded-md border border-border px-3 py-2",
                    Label { size: LabelSize::Small, "Gravity" }
                    gravity_select {
                        ival: gravity,
                        onchange: {
                            let id = id.clone();
                            move |e: i32| {
                                let id = id.clone();
                                let token = auth_token;
                        let vault = active_vault;
                                spawn(async move {
                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                    match storage.update_moment_field(id.clone(), "gravity", serde_json::json!(Some(e))).await {
                                        Ok(_) => {
                                            let mut list = moments.write();
                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                m.gravity = Some(e);
                                            }
                                        }
                                        Err(e) => log::info!("Error updating moment: {}", e),
                                    }
                                });
                            }
                        }
                    }
                }

                // Taskwarrior-style attributes, buried here on purpose so the
                // main panel (and the list view, which never shows any of
                // this) stays uncluttered — priority/project/scheduled/until
                // plus the pre-existing tags and depends-on pickers, moved in
                // from their own top-level sections. recur and real
                // enforcement of scheduled/until are explicitly not built yet
                // — see DESIGN_PROGRESS.md.
                div {
                    class: "rounded-lg border border-border bg-background",
                    button {
                        class: "flex w-full items-center justify-between px-3 py-2 text-sm font-medium text-foreground hover:bg-muted/50 transition-colors cursor-pointer",
                        onclick: move |_| {
                            let current = *advanced_open.read();
                            advanced_open.set(!current);
                        },
                        "Advanced"
                        span {
                            class: if *advanced_open.read() { "transition-transform rotate-180" } else { "transition-transform rotate-0" },
                            "⌄"
                        }
                    }
                    if *advanced_open.read() {
                        div {
                                class: "flex flex-col gap-4 p-3 border-t border-border",

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Entity" }
                                    select {
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        oninput: {
                                            let id = id.clone();
                                            move |e| {
                                                reassign_error.set(None);
                                                let id = id.clone();
                                                let new_entity_id = e.value();
                                                let token = auth_token;
                                                let vault = active_vault;
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    match storage.reassign_moment_entity(id.clone(), new_entity_id.clone()).await {
                                                        Ok(()) => {
                                                            if let Some(m) = moments.write().iter_mut().find(|m| m.id == id) {
                                                                m.entity_id = new_entity_id;
                                                            }
                                                        }
                                                        Err(e) => {
                                                            clog!("Error reassigning moment's entity: {}", e);
                                                            reassign_error.set(Some("Couldn't move this to that entity — try again.".to_string()));
                                                        }
                                                    }
                                                });
                                            }
                                        },
                                        for e in entities.read().iter() {
                                            option {
                                                value: "{e.id}",
                                                selected: live_moment.entity_id == e.id,
                                                "{e.name}"
                                            }
                                        }
                                    }
                                    if let Some(msg) = reassign_error.read().as_ref() {
                                        p { class: "text-xs text-destructive mt-1", "{msg}" }
                                    }
                                }

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Also involves" }
                                    // Multi-entity moments (2026-07-29) — every entity
                                    // added here is a full peer of the primary one above:
                                    // this moment counts fully toward their Distance/
                                    // Drift and urgency too, not just a cross-reference.
                                    if !current_additional_entities.is_empty() {
                                        div {
                                            class: "flex flex-wrap gap-1.5 mb-2",
                                            for entity in current_additional_entities.iter() {
                                                span {
                                                    key: "{entity.id}",
                                                    class: "inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs text-foreground",
                                                    "{entity.name}"
                                                    button {
                                                        class: "text-muted-foreground hover:text-destructive cursor-pointer leading-none",
                                                        onclick: {
                                                            let eid = entity.id.clone();
                                                            let mut save_additional_entities = save_additional_entities.clone();
                                                            let current_additional_ids = current_additional_ids.clone();
                                                            move |_| {
                                                                let updated: Vec<String> = current_additional_ids.iter().filter(|d| **d != eid).cloned().collect();
                                                                save_additional_entities(updated);
                                                            }
                                                        },
                                                        "×"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div {
                                        class: "relative",
                                        input {
                                            r#type: "text",
                                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            placeholder: "Search entities to add...",
                                            value: "{additional_search.read()}",
                                            onfocus: move |_| additional_dropdown_open.set(true),
                                            oninput: move |e| {
                                                additional_search.set(e.value());
                                                additional_dropdown_open.set(true);
                                            },
                                        }
                                        if *additional_dropdown_open.read() {
                                            div {
                                                class: "fixed inset-0 z-40",
                                                onclick: move |_| additional_dropdown_open.set(false),
                                            }
                                            div {
                                                class: "absolute z-50 mt-1 w-full max-h-56 overflow-y-auto rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1",
                                                onclick: move |e| e.stop_propagation(),
                                                {
                                                    let query = additional_search.read().to_lowercase();
                                                    let matches: Vec<EntityType> = additional_entity_candidates.iter()
                                                        .filter(|e| query.is_empty() || e.name.to_lowercase().contains(&query))
                                                        .take(8)
                                                        .cloned()
                                                        .collect();
                                                    rsx! {
                                                        if matches.is_empty() {
                                                            div { class: "px-2 py-1.5 text-sm text-muted-foreground", "No matches" }
                                                        }
                                                        for candidate in matches.into_iter() {
                                                            div {
                                                                key: "{candidate.id}",
                                                                class: "flex items-center rounded-sm px-2 py-1.5 text-sm text-foreground cursor-pointer hover:bg-accent transition-colors",
                                                                onclick: {
                                                                    let cand_id = candidate.id.clone();
                                                                    let mut save_additional_entities = save_additional_entities.clone();
                                                                    let current_additional_ids = current_additional_ids.clone();
                                                                    move |_| {
                                                                        additional_search.set(String::new());
                                                                        additional_dropdown_open.set(false);
                                                                        let mut updated = current_additional_ids.clone();
                                                                        if !updated.contains(&cand_id) {
                                                                            updated.push(cand_id.clone());
                                                                            save_additional_entities(updated);
                                                                        }
                                                                    }
                                                                },
                                                                "{candidate.name}"
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Priority" }
                                    select {
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        oninput: {
                                            let id = id.clone();
                                            move |e| {
                                                let id = id.clone();
                                                let token = auth_token;
                                                let vault = active_vault;
                                                let val = e.value();
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    patch_moment_metadata(&storage, moments, id, |m| {
                                                        m.priority = if val.is_empty() { None } else { Some(val) };
                                                    }).await;
                                                });
                                            }
                                        },
                                        option { value: "", selected: current_metadata.priority.is_none(), "None" }
                                        option { value: "H", selected: current_metadata.priority.as_deref() == Some("H"), "High" }
                                        option { value: "M", selected: current_metadata.priority.as_deref() == Some("M"), "Medium" }
                                        option { value: "L", selected: current_metadata.priority.as_deref() == Some("L"), "Low" }
                                    }
                                }

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Project" }
                                    input {
                                        r#type: "text",
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        placeholder: "e.g. Home.Garden",
                                        value: "{current_metadata.project.clone().unwrap_or_default()}",
                                        oninput: {
                                            let id = id.clone();
                                            move |e: Event<FormData>| {
                                                let id = id.clone();
                                                let token = auth_token;
                                                let vault = active_vault;
                                                let val = e.value();
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    patch_moment_metadata(&storage, moments, id, |m| {
                                                        m.project = if val.is_empty() { None } else { Some(val) };
                                                    }).await;
                                                });
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Scheduled" }
                                    input {
                                        r#type: "datetime-local",
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        value: "{current_metadata.scheduled_at.clone().unwrap_or_default().chars().take(16).collect::<String>()}",
                                        oninput: {
                                            let id = id.clone();
                                            move |e: Event<FormData>| {
                                                let id = id.clone();
                                                let token = auth_token;
                                                let vault = active_vault;
                                                let val = e.value();
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    patch_moment_metadata(&storage, moments, id, |m| {
                                                        m.scheduled_at = if val.is_empty() { None } else { Some(val) };
                                                    }).await;
                                                });
                                            }
                                        }
                                    }
                                }

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    label { class: "block mb-1.5 text-xs font-medium text-foreground", "Until" }
                                    input {
                                        r#type: "datetime-local",
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        value: "{current_metadata.until_at.clone().unwrap_or_default().chars().take(16).collect::<String>()}",
                                        oninput: {
                                            let id = id.clone();
                                            move |e: Event<FormData>| {
                                                let id = id.clone();
                                                let token = auth_token;
                                                let vault = active_vault;
                                                let val = e.value();
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    patch_moment_metadata(&storage, moments, id, |m| {
                                                        m.until_at = if val.is_empty() { None } else { Some(val) };
                                                    }).await;
                                                });
                                            }
                                        }
                                    }
                                }

                                div {
                                    div {
                                        class: "text-sm font-medium text-foreground mb-2",
                                        "Tags"
                                    }
                                    if !moment_tags.read().is_empty() {
                                        div {
                                            class: "flex flex-wrap gap-1.5 mb-2",
                                            for tag in moment_tags.read().iter() {
                                                span {
                                                    class: "inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs text-foreground",
                                                    "+{tag}"
                                                    button {
                                                        class: "text-muted-foreground hover:text-destructive cursor-pointer leading-none",
                                                        onclick: {
                                                            let tag = tag.clone();
                                                            let mut save_tags = save_tags.clone();
                                                            move |_| {
                                                                let mut updated = moment_tags.read().clone();
                                                                updated.retain(|t| t != &tag);
                                                                save_tags(updated);
                                                            }
                                                        },
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
                                            class: "rounded-md border border-input bg-background text-xs text-foreground px-2 py-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            placeholder: "Add tag...",
                                            value: "{tag_input.read()}",
                                            oninput: move |e: Event<FormData>| tag_input.set(e.value()),
                                        }
                                        button {
                                            class: "rounded border border-transparent bg-secondary text-secondary-foreground text-xs px-2.5 py-1 font-medium hover:bg-secondary/80 transition-colors cursor-pointer",
                                            onclick: {
                                                let mut save_tags = save_tags.clone();
                                                move |_| {
                                                    let new_tag = tag_input.read().trim().to_string();
                                                    if new_tag.is_empty() {
                                                        return;
                                                    }
                                                    let mut updated = moment_tags.read().clone();
                                                    if !updated.contains(&new_tag) {
                                                        updated.push(new_tag);
                                                        save_tags(updated);
                                                    }
                                                    tag_input.set(String::new());
                                                }
                                            },
                                            "Add"
                                        }
                                    }
                                }

                                div {
                                    div {
                                        class: "text-sm font-medium text-foreground mb-2",
                                        "Depends on"
                                    }
                                    if is_blocked {
                                        div {
                                            class: "flex items-center gap-1.5 mb-2 text-xs text-destructive",
                                            span { class: "h-1.5 w-1.5 rounded-full bg-destructive shrink-0" }
                                            "Blocked by \"{blocked_on.iter().filter(|d| d.moment_type_id != 3 && d.completed_at.is_none()).map(|d| d.title.clone()).collect::<Vec<_>>().join(\"\\\", \\\"\")}\""
                                        }
                                    }
                                    if blocking_count > 0 {
                                        div {
                                            class: "mb-2 text-xs text-muted-foreground",
                                            "Blocking {blocking_count} other open moment(s)"
                                        }
                                    }
                                    // Chips + search-to-add, same shape as the Tags
                                    // section above — no limit on how many dependencies
                                    // a moment can have (2026-07-29), matching tags.
                                    if !blocked_on.is_empty() {
                                        div {
                                            class: "flex flex-wrap gap-1.5 mb-2",
                                            for dep in blocked_on.iter() {
                                                span {
                                                    key: "{dep.id}",
                                                    class: "inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs text-foreground",
                                                    "{dep.title}"
                                                    button {
                                                        class: "text-muted-foreground hover:text-destructive cursor-pointer leading-none",
                                                        onclick: {
                                                            let dep_id = dep.id.clone();
                                                            let mut save_deps = save_deps.clone();
                                                            let current_dep_ids = current_dep_ids.clone();
                                                            move |_| {
                                                                let updated: Vec<String> = current_dep_ids.iter().filter(|d| **d != dep_id).cloned().collect();
                                                                save_deps(updated);
                                                            }
                                                        },
                                                        "×"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    div {
                                        class: "relative",
                                        input {
                                            r#type: "text",
                                            class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                            placeholder: "Search open tasks/promises to add...",
                                            value: "{depends_search.read()}",
                                            onfocus: move |_| depends_dropdown_open.set(true),
                                            oninput: move |e| {
                                                depends_search.set(e.value());
                                                depends_dropdown_open.set(true);
                                            },
                                        }
                                        if *depends_dropdown_open.read() {
                                            // Same invisible-backdrop outside-click-dismiss
                                            // pattern as components/context_menu — a click
                                            // anywhere outside the list closes it.
                                            div {
                                                class: "fixed inset-0 z-40",
                                                onclick: move |_| depends_dropdown_open.set(false),
                                            }
                                            div {
                                                class: "absolute z-50 mt-1 w-full max-h-56 overflow-y-auto rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1",
                                                onclick: move |e| e.stop_propagation(),
                                                {
                                                    let query = depends_search.read().to_lowercase();
                                                    let matches: Vec<MomentType> = dependency_candidates.iter()
                                                        .filter(|m| query.is_empty() || m.title.to_lowercase().contains(&query))
                                                        .take(8)
                                                        .cloned()
                                                        .collect();
                                                    rsx! {
                                                        if matches.is_empty() {
                                                            div { class: "px-2 py-1.5 text-sm text-muted-foreground", "No matches" }
                                                        }
                                                        for candidate in matches.into_iter() {
                                                            div {
                                                                key: "{candidate.id}",
                                                                class: "flex items-center rounded-sm px-2 py-1.5 text-sm text-foreground cursor-pointer hover:bg-accent transition-colors",
                                                                onclick: {
                                                                    let cand_id = candidate.id.clone();
                                                                    let mut save_deps = save_deps.clone();
                                                                    let current_dep_ids = current_dep_ids.clone();
                                                                    move |_| {
                                                                        depends_search.set(String::new());
                                                                        depends_dropdown_open.set(false);
                                                                        let mut updated = current_dep_ids.clone();
                                                                        if !updated.contains(&cand_id) {
                                                                            updated.push(cand_id.clone());
                                                                            save_deps(updated);
                                                                        }
                                                                    }
                                                                },
                                                                "{candidate.title}"
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
                    class: "rounded-lg border border-border bg-background",
                    div {
                        class: "text-sm font-medium text-foreground px-3 py-2 border-b border-border",
                        "Reactions"
                    }
                    if reactions.read().is_empty() {
                        div {
                            class: "flex flex-col gap-3 p-3",
                            textarea {
                                class: "w-full min-h-24 rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring resize-y",
                                value: "{ReactionForm.read().description}",
                                placeholder: "What was the consequence?",
                                oninput: move |e| ReactionForm.write().description = e.value()
                            }
                            div {
                                class: "flex items-center justify-between rounded-md border border-border px-3 py-2",
                                label { class: "block mb-1.5 text-xs font-medium text-foreground", "Reaction" }
                                gravity_select {
                                    ival: ReactionForm.read().value,
                                    onchange: move |e: i32| {
                                        ReactionForm.write().value = e;
                                    }
                                }
                            }
                            button {
                                class: "w-full rounded border border-transparent bg-secondary text-secondary-foreground text-sm px-4 py-1.5 font-medium hover:bg-secondary/80 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: is_blocked,
                                onclick: {
                                    let id = id.clone();
                                    move |_| {
                                        if is_blocked {
                                            return;
                                        }
                                        let id = id.clone();
                                        let token = auth_token;
                        let vault = active_vault;
                                        spawn(async move {
                                            let completed_at = Some(chrono::Utc::now().to_rfc3339());
                                            let new_reaction = NewReactionType {
                                                moment_id: id.clone(),
                                                description: ReactionForm.read().description.clone(),
                                                value: ReactionForm.read().value,
                                            };
                                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                            let (complete_result, reaction_result) = futures::join!(
                                                storage.update_moment_field(id.clone(), "completed_at", serde_json::json!(completed_at)),
                                                storage.create_reaction(new_reaction)
                                            );

                                            let new_r = match reaction_result {
                                                Ok(r) => r,
                                                Err(e) => {
                                                    clog!("Error creating reaction: {}", e);
                                                    return;
                                                }
                                            };
                                            let completed_ok = complete_result.is_ok();
                                            if let Err(e) = complete_result {
                                                clog!("Error marking moment complete: {}", e);
                                                // Reaction was saved even though completion wasn't —
                                                // still reflect the reaction so it isn't silently
                                                // lost, but leave completed_at untouched below.
                                            }

                                            reactions.write().push(new_r.clone());
                                            let mut list = moments.write();
                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                if completed_ok {
                                                    m.completed_at = completed_at;
                                                }
                                                if let Some(r) = &mut m.reactions {
                                                    r.push(new_r);
                                                } else {
                                                    m.reactions = Some(vec![new_r]);
                                                }
                                            }
                                            activity_bar_tgl.set(false);
                                            backdropTgl.set(false);
                                        });
                                    }
                                },
                                "Complete with reaction"
                            }
                        }
                    } else {
                        div {
                            class: "flex flex-col divide-y divide-border",
                            for reaction in reactions.read().clone().into_iter() {
                                div {
                                    class: "flex items-center justify-between gap-3 px-3 py-2 text-sm",
                                    div {
                                        class: "flex flex-col min-w-0",
                                        span { class: "text-foreground truncate", "{reaction.description}" }
                                        span { class: "text-xs text-muted-foreground", "{reaction.value:?}" }
                                    }
                                    button {
                                        class: "h-7 w-7 flex items-center justify-center rounded-md text-muted-foreground hover:bg-destructive/10 hover:text-destructive transition-colors cursor-pointer shrink-0",
                                        onclick: {
                                            let id = id.clone();
                                            let reaction = reaction.clone();
                                            move |_| {
                                                let id = id.clone();
                                                let reaction_id = reaction.id.clone();
                                                let reaction = reaction.clone();
                                                let token = auth_token;
                        let vault = active_vault;
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    match storage.delete_reaction(reaction).await {
                                                        Ok(_) => {
                                                            // update signal — drives the UI
                                                            reactions.write().retain(|r| r.id != reaction_id);
                                                            // keep moments in sync
                                                            let mut list = moments.write();
                                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                                m.reactions.as_mut().map(|v| v.retain(|r| r.id != reaction_id));
                                                            }
                                                        }
                                                        Err(e) => log::info!("Error deleting reaction: {}", e),
                                                    }
                                                });
                                            }
                                        },
                                        fa_trash {}
                                    }
                                }
                            }
                        }
                    }
                }
            }

            div {
                class: "px-4 py-3 border-t border-border shrink-0",
                button {
                    class: "w-full inline-flex items-center justify-center gap-2 rounded border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-4 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer",
                    onclick: {
                        let id = id.clone();
                        move |_| {
                        // Look up live by id rather than moment_sig (a
                        // one-time snapshot from when the panel opened) —
                        // same staleness class fixed elsewhere in this
                        // component; entity_id essentially never changes in
                        // practice, but there's no reason to risk it here
                        // either.
                        let Some(moment) = moments.read().iter().find(|m| m.id == id).cloned() else {
                            return;
                        };
                        let mut moments = moments.clone();
                        let token = auth_token;
                        let vault = active_vault;
                        let mut activity_bar_tgl = activity_bar_tgl;
                        let mut backdropTgl = backdropTgl;
                        spawn(async move {
                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                            match storage.delete_moment(moment.clone()).await {
                                Ok(()) => {
                                    moments.write().retain(|m| m.id != moment.id);
                                    // `ab_task_cmp` is keyed on activity_bar_tgl (see
                                    // navbar.rs), so flipping it remounts/unmounts this
                                    // component — closing the panel here has to come
                                    // *after* the delete resolves and `moments` is
                                    // updated, not before. Closing it synchronously
                                    // right after `spawn()` (the previous bug) tore
                                    // this task down mid-flight the moment the panel
                                    // unmounted, so the delete never actually happened.
                                    activity_bar_tgl.set(false);
                                    backdropTgl.set(false);
                                }
                                Err(e) => clog!("Error deleting moment: {}", e),
                            }
                        });
                        }
                    },
                    fa_trash {}
                    "Delete"
                }
            }
        }
    }
}

// Shared read-modify-write for the metadata jsonb blob (see MomentMetadata —
// tags/priority/project/scheduled_at/until_at all live there, not as real
// DB columns, so patching any one of them means loading the *current* full
// blob, mutating just the one field, and PATCHing "metadata" as a whole).
// Reads live off `moments` by id rather than any snapshot, matching the
// pattern already fixed for depends_on/tags above — an `impl Trait` mutator
// closure only works as a real fn, not a stored closure, so this is a
// module-level fn rather than another per-field-duplicated closure.
async fn patch_moment_metadata(
    storage: &ActiveStorage,
    mut moments: Signal<Vec<MomentType>>,
    id: String,
    mutate: impl FnOnce(&mut MomentMetadata),
) {
    let mut new_meta = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();
    mutate(&mut new_meta);
    if storage.update_moment_field(id.clone(), "metadata", serde_json::json!(new_meta)).await.is_ok() {
        let mut list = moments.write();
        if let Some(m) = list.iter_mut().find(|m| m.id == id) {
            m.metadata = Some(new_meta);
        }
    }
}

// Drag-and-drop dependency authoring in BlockingDagViewCmp (2026-07-29):
// `target_id` becomes blocked by `source_id`. Folds in dependency_ids()
// (not just the raw metadata field) before appending, so a moment still on
// the legacy single-dependency column doesn't silently lose it the first
// time a link is drawn to it — same "read the full current state before
// patching metadata" rule as everywhere else dependencies are written.
async fn add_dependency(storage: &ActiveStorage, mut moments: Signal<Vec<MomentType>>, target_id: String, source_id: String) {
    let Some(current) = moments.read().iter().find(|m| m.id == target_id).cloned() else { return; };
    let mut deps = current.dependency_ids();
    if deps.contains(&source_id) {
        return;
    }
    deps.push(source_id);
    let mut new_meta = current.metadata.clone().unwrap_or_default();
    new_meta.depends_on = deps;
    if storage.update_moment_field(target_id.clone(), "metadata", serde_json::json!(new_meta)).await.is_ok() {
        let mut list = moments.write();
        if let Some(m) = list.iter_mut().find(|m| m.id == target_id) {
            m.metadata = Some(new_meta);
        }
    }
}

// "Completed" has to mean something real: a moment that's blocked by an
// incomplete dependency can't be completed (see CheckboxCmp's `disabled`
// handling above) — so the reverse has to hold too, or completion status
// becomes a lie the instant a finished blocker gets un-finished again.
// Un-completing `root_id` therefore un-completes anything that transitively
// depends on it as well, rather than leaving a completed-but-actually-
// blocked moment sitting there. The user's own framing: don't let the app
// paper over an untangled blocked/blocking chain — force it to be resolved.
async fn cascade_uncomplete(storage: &ActiveStorage, mut moments: Signal<Vec<MomentType>>, root_id: String) {
    let mut queue = vec![root_id];
    let mut visited = std::collections::HashSet::new();
    while let Some(current) = queue.pop() {
        if !visited.insert(current.clone()) {
            continue;
        }
        let dependents: Vec<String> = moments.read().iter()
            .filter(|m| m.dependency_ids().contains(&current) && m.completed_at.is_some())
            .map(|m| m.id.clone())
            .collect();
        for dep_id in dependents {
            if storage.update_moment_field(dep_id.clone(), "completed_at", serde_json::json!(None::<String>)).await.is_ok() {
                let mut list = moments.write();
                if let Some(m) = list.iter_mut().find(|m| m.id == dep_id) {
                    m.completed_at = None;
                }
            }
            queue.push(dep_id);
        }
    }
}

// "On the fly" (2026-07-28) — inspired by a Waffle House order getting
// completed outside the normal flow: jot the task, a full-screen takeover
// says "go do this now," go actually do it, come back and mark it done.
// The moment is created for real the instant it's captured (not held only
// in memory) — getting pulled away mid-errand should never lose it, even
// if "mark done" never happens. Rendered at the top level (navbar.rs),
// same reasoning as FullScreenEditorModalCmp: needs true position:fixed
// full-viewport coverage, not contained by a transformed ancestor.
#[component]
pub fn OnTheFlyCmp() -> Element {
    let state = use_context::<AppState>();
    let mut open = state.on_the_fly_open;
    let mut task = state.on_the_fly_task;
    let mut moments = state.moments;
    let mut entities = state.entities;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut title_input = use_signal(String::new);
    let title_input_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
    let mut was_open = use_signal(|| *open.read());

    // Resets whenever this transitions closed -> open, regardless of what
    // triggered it (this button, or the global "o" keyboard shortcut in
    // layouts::Navbar) — one place for the reset instead of duplicating it
    // at every place that can set on_the_fly_open.
    use_effect(move || {
        let now_open = *open.read();
        if now_open && !*was_open.read() {
            title_input.set(String::new());
            task.set(None);
        }
        was_open.set(now_open);
    });

    rsx! {
        button {
            class: "fixed bottom-6 left-6 z-51 rounded-full shadow-lg h-12 px-4 flex items-center gap-2 text-sm font-semibold text-white transition-transform hover:scale-105 active:scale-95 cursor-pointer",
            style: "background-color:{HL};",
            onclick: move |_| open.set(true),
            fa_bolt {}
            "On the fly"
        }
        if *open.read() {
            div {
                class: "fixed inset-0 bg-black z-100 flex items-center justify-center p-8",
                if let Some(t) = task.read().clone() {
                    // "Go do it" stage — the moment already exists.
                    div {
                        class: "flex flex-col items-center gap-8 text-center max-w-2xl",
                        span { class: "text-sm font-semibold uppercase tracking-widest text-white/50", "Go do this now" }
                        h1 { class: "text-4xl font-bold text-white", "{t.title}" }
                        div {
                            class: "flex items-center gap-3",
                            button {
                                class: "rounded-md border border-transparent bg-white text-black text-base px-6 py-2.5 font-semibold hover:opacity-90 transition-opacity cursor-pointer",
                                onclick: {
                                    let id = t.id.clone();
                                    move |_| {
                                        let id = id.clone();
                                        let token = auth_token;
                                        let vault = active_vault;
                                        spawn(async move {
                                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                            let now = chrono::Utc::now().to_rfc3339();
                                            match storage.update_moment_field(id.clone(), "completed_at", serde_json::json!(Some(now.clone()))).await {
                                                Ok(_) => {
                                                    if let Some(m) = moments.write().iter_mut().find(|m| m.id == id) {
                                                        m.completed_at = Some(now);
                                                    }
                                                }
                                                Err(e) => log::info!("Error completing on-the-fly task: {}", e),
                                            }
                                        });
                                        open.set(false);
                                        task.set(None);
                                    }
                                },
                                "Done"
                            }
                            button {
                                class: "rounded-md border border-white/30 text-white text-base px-6 py-2.5 font-semibold hover:bg-white/10 transition-colors cursor-pointer",
                                title: "Closes this screen — the task stays on your list like normal, nothing is lost.",
                                onclick: move |_| {
                                    open.set(false);
                                    task.set(None);
                                },
                                "Not now"
                            }
                        }
                    }
                } else {
                    // Capture stage — nothing exists yet. Full quick-capture
                    // power here (2026-07-28, upgraded from a plain <input>)
                    // — @mention, pri:/due:/+tags/project:, the same parser
                    // MomentInputCmp uses, so "@admissions office send them
                    // a reply about my hold" actually attaches to that
                    // entity instead of landing as a literal title string.
                    div {
                        class: "flex flex-col items-center gap-3 w-full max-w-xl",
                        h2 { class: "text-2xl font-semibold text-white text-center mb-1", "What do you need to do?" }
                        div {
                            class: "w-full rounded-xl border border-input bg-background shadow-sm p-3",
                            QuickCaptureInput {
                                value: title_input.read().clone(),
                                placeholder: "@mention, pri:H, due:today, +tag all work here...".to_string(),
                                entities: entities.read().clone(),
                                moments: moments.read().clone(),
                                on_input: move |v: String| title_input.set(v),
                                on_submit: move |_| {
                                    let raw_title = title_input.read().clone();
                                    if raw_title.trim().is_empty() { return; }
                                    let entities_snapshot = entities.read().clone();
                                    let parsed = quick_capture::parse(&raw_title, &entities_snapshot);
                                    let token = auth_token;
                                    let vault = active_vault;
                                    let self_id = vault.read().effective(&token.read()).resolve_self_entity_id(&entities.read()).unwrap_or_default();
                                    let entity_id = parsed.entity_id.clone().unwrap_or(self_id);
                                    title_input.set(String::new());
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        let new_moment = NewMomentType {
                                            title: parsed.title.clone(),
                                            entity_id,
                                            description: None,
                                            gravity: Some(0),
                                            moment_type_id: parsed.moment_type_id.unwrap_or(1),
                                            deleted_at: None,
                                        };
                                        match storage.create_moment(new_moment).await {
                                            Ok(created) => {
                                                let created_id = created.id.clone();
                                                moments.write().insert(0, created.clone());
                                                if parsed.has_metadata() {
                                                    let meta = MomentMetadata {
                                                        tags: parsed.tags_add.clone(),
                                                        sort_index: None,
                                                        priority: parsed.priority.clone(),
                                                        project: parsed.project.clone(),
                                                        scheduled_at: parsed.scheduled_at.clone(),
                                                        until_at: parsed.until_at.clone(),
                                                        depends_on: Vec::new(),
                                                        additional_entity_ids: Vec::new(),
                                                    };
                                                    if storage.update_moment_field(created_id.clone(), "metadata", serde_json::json!(meta)).await.is_ok() {
                                                        if let Some(m) = moments.write().iter_mut().find(|m| m.id == created_id) {
                                                            m.metadata = Some(meta);
                                                        }
                                                    }
                                                }
                                                task.set(Some(created));
                                            }
                                            Err(e) => log::info!("Error creating on-the-fly task: {}", e),
                                        }
                                    });
                                },
                                // No description field in this flow — Tab
                                // just does nothing extra here.
                                on_tab: move |_| {},
                                on_add_entity: move |name: String| {
                                    let token = auth_token;
                                    let vault = active_vault;
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        let new_entity = NewEntityType {
                                            name,
                                            entity_type_id: None,
                                            parent_entity_id: None,
                                            user_id: None,
                                            archived_at: None,
                                            metadata: None,
                                        };
                                        match storage.create_entity(new_entity).await {
                                            Ok(created) => { entities.write().insert(0, created); }
                                            Err(e) => log::info!("Error creating entity: {}", e),
                                        }
                                    });
                                },
                                input_el: title_input_el,
                            }
                        }
                        span { class: "text-xs text-white/40", "Press Enter to go do it" }
                        button {
                            class: "rounded-md border border-white/30 text-white text-sm px-5 py-2 font-semibold hover:bg-white/10 transition-colors cursor-pointer mt-2",
                            onclick: move |_| open.set(false),
                            "Cancel"
                        }
                    }
                }
            }
        }
    }
}

// Rendered at the top level (layouts/navbar.rs, sibling to the activity bar
// panel, same as EntityModalCmp) rather than nested inside ab_task_cmp
// where the trigger button lives — the activity bar panel is itself a
// `translate-x-0` sliding drawer, and any position:fixed descendant of a
// transformed ancestor gets contained to that ancestor's box instead of
// the real viewport (confirmed live: the modal rendered correctly-styled
// but pinned inside the 384px activity-bar panel instead of covering the
// screen). Reads current_moment/moments live off AppState, same as
// ab_task_cmp, so it always reflects whichever moment is actually open.
// A deliberately small markdown subset, not full CommonMark — headers,
// bold/italic, inline code, links, and bullet lists, which covers what
// people actually type in a quick note. HTML-escaped first (this goes
// straight into dangerous_inner_html below), then transformed line by
// line so nothing needs real parser-combinator machinery or a new crate
// dependency just for this.
fn render_markdown_lite(src: &str) -> String {
    fn escape_html(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
    // Inline spans: code first (so its contents aren't further mangled by
    // bold/italic/link matching), then links, then bold, then italic.
    fn inline(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.char_indices().peekable();
        let bytes = s.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'`' {
                if let Some(end) = s[i + 1..].find('`') {
                    out.push_str("<code>");
                    out.push_str(&s[i + 1..i + 1 + end]);
                    out.push_str("</code>");
                    i = i + 1 + end + 1;
                    continue;
                }
            }
            if bytes[i] == b'[' {
                if let Some(close) = s[i..].find(']') {
                    if s[i..].as_bytes().get(close + 1) == Some(&b'(') {
                        if let Some(paren_end) = s[i + close + 2..].find(')') {
                            let text = &s[i + 1..i + close];
                            let url = &s[i + close + 2..i + close + 2 + paren_end];
                            out.push_str(&format!("<a href=\"{url}\" target=\"_blank\" rel=\"noopener noreferrer\" class=\"underline\">{text}</a>"));
                            i = i + close + 2 + paren_end + 1;
                            continue;
                        }
                    }
                }
            }
            if bytes[i..].starts_with(b"**") {
                if let Some(end) = s[i + 2..].find("**") {
                    out.push_str("<strong>");
                    out.push_str(&s[i + 2..i + 2 + end]);
                    out.push_str("</strong>");
                    i = i + 2 + end + 2;
                    continue;
                }
            }
            if bytes[i] == b'*' {
                if let Some(end) = s[i + 1..].find('*') {
                    out.push_str("<em>");
                    out.push_str(&s[i + 1..i + 1 + end]);
                    out.push_str("</em>");
                    i = i + 1 + end + 1;
                    continue;
                }
            }
            let ch = s[i..].chars().next().unwrap();
            out.push(ch);
            i += ch.len_utf8();
        }
        let _ = &mut chars;
        out
    }

    let escaped = escape_html(src);
    let mut html = String::new();
    let mut in_list = false;
    for line in escaped.lines() {
        let trimmed = line.trim_start();
        let is_bullet = trimmed.starts_with("- ") || trimmed.starts_with("* ");
        if is_bullet && !in_list {
            html.push_str("<ul class=\"list-disc pl-5\">");
            in_list = true;
        } else if !is_bullet && in_list {
            html.push_str("</ul>");
            in_list = false;
        }
        if is_bullet {
            html.push_str("<li>");
            html.push_str(&inline(&trimmed[2..]));
            html.push_str("</li>");
        } else if let Some(rest) = trimmed.strip_prefix("### ") {
            html.push_str("<h3 class=\"text-base font-semibold mt-2 mb-1\">"); html.push_str(&inline(rest)); html.push_str("</h3>");
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            html.push_str("<h2 class=\"text-lg font-semibold mt-2 mb-1\">"); html.push_str(&inline(rest)); html.push_str("</h2>");
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            html.push_str("<h1 class=\"text-xl font-semibold mt-2 mb-1\">"); html.push_str(&inline(rest)); html.push_str("</h1>");
        } else if trimmed.is_empty() {
            html.push_str("<br>");
        } else {
            html.push_str("<p class=\"mb-1\">"); html.push_str(&inline(trimmed)); html.push_str("</p>");
        }
    }
    if in_list {
        html.push_str("</ul>");
    }
    html
}

#[cfg(test)]
mod markdown_lite_tests {
    use super::render_markdown_lite;

    #[test]
    fn renders_bold_and_italic() {
        let html = render_markdown_lite("**bold** and *italic*");
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>italic</em>"));
    }

    #[test]
    fn renders_headers() {
        assert!(render_markdown_lite("# Big").contains("<h1"));
        assert!(render_markdown_lite("## Medium").contains("<h2"));
        assert!(render_markdown_lite("### Small").contains("<h3"));
    }

    #[test]
    fn renders_inline_code_and_links() {
        let html = render_markdown_lite("`code` and [text](https://example.com)");
        assert!(html.contains("<code>code</code>"));
        assert!(html.contains("href=\"https://example.com\""));
        assert!(html.contains(">text</a>"));
    }

    #[test]
    fn renders_bullet_lists() {
        let html = render_markdown_lite("- one\n- two");
        assert!(html.contains("<ul"));
        assert!(html.contains("<li>one</li>"));
        assert!(html.contains("<li>two</li>"));
    }

    #[test]
    fn escapes_raw_html_to_prevent_injection() {
        let html = render_markdown_lite("<script>alert(1)</script>");
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}

#[component]
pub fn FullScreenEditorModalCmp() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let current_moment = state.current_moment;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut full_editor_open = state.full_editor_open;

    if !*full_editor_open.read() {
        return rsx! {};
    }
    let Some(moment) = current_moment.read().clone() else {
        return rsx! {};
    };
    let id = moment.id.clone();
    let live_moment = moments.read().iter().find(|m| m.id == id).cloned().unwrap_or(moment);
    let title = live_moment.title.clone();
    let description = live_moment.description.clone();
    let moment_kind = match live_moment.moment_type_id {
        2i64 => "Promise",
        3i64 => "Note",
        _ => "Task",
    };

    // Live markdown preview needs to update on every keystroke, not just on
    // blur (that's what "live" means) — so this tracks the textarea
    // separately from `description` (which only reflects the last *saved*
    // value) and resets whenever the modal opens on a different moment,
    // same pattern as ab_task_cmp's depends_search reset.
    let mut live_description = use_signal(|| description.clone().unwrap_or_default());
    use_effect(move || {
        let d = current_moment.read().as_ref().and_then(|m| m.description.clone()).unwrap_or_default();
        live_description.set(d);
    });
    let preview_html = render_markdown_lite(&live_description.read());

    rsx! {
        div {
            class: "hidden xl:flex fixed inset-0 bg-black/40 z-100 items-center justify-center",
            onclick: move |_| full_editor_open.set(false),
            div {
                class: "bg-background w-full h-full flex flex-col",
                onclick: move |e| e.stop_propagation(),
                div {
                    class: "flex items-center justify-between h-14 px-4 border-b border-border shrink-0",
                    span { class: "text-sm font-medium text-muted-foreground", "{moment_kind} · Markdown preview" }
                    button {
                        class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                        onclick: move |_| full_editor_open.set(false),
                        "×"
                    }
                }
                div {
                    class: "flex flex-col gap-3 px-6 py-5 flex-1 min-h-0",
                    input {
                        class: "text-2xl font-semibold text-foreground w-full bg-transparent border-none outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md -mx-1 px-1 py-1 shrink-0",
                        value: "{title}",
                        // oninput, not onchange — onchange only fires on blur, so
                        // hitting the × to close this editor right after typing
                        // (without clicking away first) silently discarded the edit.
                        oninput: {
                            let id = id.clone();
                            move |e| {
                                let id = id.clone();
                                let token = auth_token;
                                let vault = active_vault;
                                let val = e.value();
                                spawn(async move {
                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                    match storage.update_moment_field(id.clone(), "title", serde_json::json!(val)).await {
                                        Ok(_) => {
                                            let mut list = moments.write();
                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                m.title = val;
                                            }
                                        }
                                        Err(e) => log::info!("Error updating moment: {}", e),
                                    }
                                });
                            }
                        },
                    }
                    div {
                        class: "flex gap-4 flex-1 min-h-0",
                        textarea {
                            class: "w-1/2 h-full rounded-md border border-input bg-background text-base text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring resize-none font-mono",
                            placeholder: "Add a description... (markdown: **bold**, *italic*, # headers, `code`, [links](url), - lists)",
                            value: "{live_description.read()}",
                            // Live, not just on blur — both for the preview pane and
                            // so closing the editor right after typing doesn't drop it.
                            oninput: {
                                let id = id.clone();
                                move |e: Event<FormData>| {
                                    let id = id.clone();
                                    let token = auth_token;
                                    let vault = active_vault;
                                    let val = e.value();
                                    live_description.set(val.clone());
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "description", serde_json::json!(val)).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.description = Some(val);
                                                }
                                            }
                                            Err(e) => log::info!("Error updating moment: {}", e),
                                        }
                                    });
                                }
                            },
                        }
                        div {
                            class: "w-1/2 h-full overflow-y-auto rounded-md border border-input bg-background px-4 py-2 text-foreground",
                            dangerous_inner_html: "{preview_html}",
                        }
                    }
                }
            }
        }
    }
}

// Taskwarrior's "waiting" concept: moments given a future scheduled_at (via
// the scheduled:/wait: quick-capture keyword) are hidden from every normal
// view (see is_waiting() in urgency.rs) until that date, with this as the
// one place to go check on everything currently parked. Only ever shows
// what's still incoming — once a date arrives the moment is already back
// in the normal views, so there's nothing left for this one to say about it.
#[component]
pub fn ScheduledViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let entities = state.entities;
    let mut current_moment = state.current_moment;
    let mut activity_bar_view = state.activity_bar_view;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let now = chrono::Utc::now();

    // Only what's still incoming — once a scheduled date arrives the
    // moment is already visible everywhere else again (see is_waiting()),
    // so it doesn't belong in a "what's parked" review list anymore either.
    let mut scheduled: Vec<(MomentType, chrono::DateTime<chrono::Utc>)> = moments.read().iter()
        .filter(|m| m.completed_at.is_none())
        .filter_map(|m| {
            let s = m.metadata.as_ref()?.scheduled_at.as_ref()?;
            let dt = crate::urgency::parse_moment_datetime(s)?;
            (dt > now).then_some((m.clone(), dt))
        })
        .collect();
    scheduled.sort_by_key(|(_, dt)| *dt);

    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            if scheduled.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-8",
                    "Nothing scheduled for later right now."
                }
            } else {
                for (m, dt) in scheduled.iter() {
                    div {
                        key: "{m.id}",
                        class: "flex items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors",
                        onclick: {
                            let m = m.clone();
                            move |_| {
                                current_moment.set(Some(m.clone()));
                                activity_bar_view.set(ABView::Task);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        div {
                            class: "flex flex-col min-w-0",
                            span { class: "text-sm font-medium text-foreground truncate", "{m.title}" }
                            span { class: "text-xs text-muted-foreground", "{entity_name(&m.entity_id)}" }
                        }
                        span {
                            class: "text-xs text-muted-foreground shrink-0",
                            "{dt.format(\"%b %d\")}"
                        }
                    }
                }
            }
        }
    }
}

// One row of the blocking tree, recursive — renders `m`, then recurses
// into everything that depends on it (any open moment whose dependency_ids()
// contains m.id — 2026-07-29: a moment can have more than one dependency
// now, so a moment blocked on two different blockers renders once under
// each of them; a true tree can't represent multiple parents without
// duplicating, which is exactly what BlockingDagViewCmp exists to show
// correctly instead), indented one level further each time. A plain function rather
// than a #[component]: it doesn't need hooks, and recursion through a
// #[component] would need its Props to derive PartialEq on a Vec of
// borrowed data, which is more friction than it's worth here. Signals are
// Copy, so threading them through recursive calls directly (rather than a
// generic closure prop) is simplest.
#[allow(clippy::too_many_arguments)]
fn render_blocking_node(
    m: MomentType,
    all: std::rc::Rc<Vec<MomentType>>,
    entities: std::rc::Rc<Vec<EntityType>>,
    depth: usize,
    mut current_moment: Signal<Option<MomentType>>,
    mut activity_bar_view: Signal<ABView>,
    mut activity_bar_tgl: Signal<bool>,
    mut backdropTgl: Signal<bool>,
) -> Element {
    let entity_name = entities.iter()
        .find(|e| e.id == m.entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());
    let mut children: Vec<MomentType> = all.iter()
        .filter(|c| c.completed_at.is_none() && c.dependency_ids().contains(&m.id))
        .cloned()
        .collect();
    children.sort_by(|a, b| a.title.cmp(&b.title));
    let indent = 16 + depth * 24;

    rsx! {
        div {
            key: "{m.id}",
            style: "padding-left: {indent}px;",
            class: "flex items-center gap-2 pr-4 py-2.5 cursor-pointer hover:bg-muted/50 transition-colors border-b border-border last:border-b-0",
            onclick: {
                let m = m.clone();
                move |_| {
                    current_moment.set(Some(m.clone()));
                    activity_bar_view.set(ABView::Task);
                    backdropTgl.set(true);
                    activity_bar_tgl.set(true);
                }
            },
            if depth > 0 {
                span { class: "text-muted-foreground text-xs shrink-0", "└─" }
            }
            div {
                class: "flex flex-col min-w-0",
                span { class: "text-sm font-medium text-foreground truncate", "{m.title}" }
                span { class: "text-xs text-muted-foreground", "{entity_name}" }
            }
        }
        for child in children.into_iter() {
            {
                let el = render_blocking_node(child, all.clone(), entities.clone(), depth + 1, current_moment, activity_bar_view, activity_bar_tgl, backdropTgl);
                el
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum BlockingMode {
    Tree,
    Graph,
}

#[component]
pub fn BlockingViewCmp() -> Element {
    let mut mode = use_signal(|| BlockingMode::Tree);
    let tab_class = |active: bool| if active {
        "px-3 py-1.5 text-sm font-medium rounded-md bg-muted text-foreground cursor-pointer"
    } else {
        "px-3 py-1.5 text-sm font-medium rounded-md text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
    };
    rsx! {
        div {
            class: "mx-4 mb-3 flex items-center gap-1",
            span { class: tab_class(*mode.read() == BlockingMode::Tree), onclick: move |_| mode.set(BlockingMode::Tree), "Tree" }
            span { class: tab_class(*mode.read() == BlockingMode::Graph), onclick: move |_| mode.set(BlockingMode::Graph), "Graph" }
        }
        if *mode.read() == BlockingMode::Tree {
            BlockingTreeViewCmp {}
        } else {
            BlockingDagViewCmp {}
        }
    }
}

#[component]
fn BlockingTreeViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let entities = state.entities;
    let current_moment = state.current_moment;
    let activity_bar_view = state.activity_bar_view;
    let activity_bar_tgl = state.activity_bar_tgl;
    let backdropTgl = state.backdropTgl;

    // Anyone that's a dependency target of at least one other still-open
    // moment — "what's actually blocking other things," so the user can go
    // free them up. A moment blocking only already-completed things isn't
    // in anyone's way anymore, so it doesn't count.
    let all = std::rc::Rc::new(moments.read().clone());
    let entities_snapshot = std::rc::Rc::new(entities.read().clone());
    let blocking_ids: std::collections::HashSet<String> = all.iter()
        .filter(|m| m.completed_at.is_none())
        .flat_map(|m| m.dependency_ids())
        .collect();
    // Roots only — a blocking moment that's itself blocking-something-
    // else's-blocker (i.e. any of its own dependencies is also in the
    // blocking set) gets skipped here and picked up as a nested child
    // instead (possibly under more than one parent now — see
    // render_blocking_node), so nothing renders twice at the top level.
    let mut roots: Vec<MomentType> = all.iter()
        .filter(|m| blocking_ids.contains(&m.id) && m.completed_at.is_none())
        .filter(|m| !m.dependency_ids().iter().any(|dep_id| blocking_ids.contains(dep_id)))
        .cloned()
        .collect();
    roots.sort_by(|a, b| a.title.cmp(&b.title));

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background overflow-hidden",
            if roots.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-8",
                    "Nothing's blocking anything else right now."
                }
            } else {
                for m in roots.into_iter() {
                    {
                        let el = render_blocking_node(m, all.clone(), entities_snapshot.clone(), 0, current_moment, activity_bar_view, activity_bar_tgl, backdropTgl);
                        el
                    }
                }
            }
        }
    }
}

#[derive(serde::Serialize, Clone)]
struct DagNodeIn {
    id: String,
    connected: bool,
}

#[derive(serde::Serialize, Clone)]
struct DagLinkIn {
    source: String,
    target: String,
}

#[derive(serde::Serialize, Clone)]
struct DagLayoutIn {
    nodes: Vec<DagNodeIn>,
    links: Vec<DagLinkIn>,
}

#[derive(serde::Deserialize, Clone)]
struct DagNodeOut {
    id: String,
    x: f64,
    y: f64,
}

const DAG_CANVAS_W: f64 = 900.0;
const DAG_CANVAS_H: f64 = 560.0;
const DAG_MIN_ZOOM: f64 = 0.3;
const DAG_MAX_ZOOM: f64 = 3.0;

// Every open moment gets a node — most aren't part of any dependency chain
// at all, and per the user's own framing that's fine, even the point:
// "every moment but not every moment is in the flow of the graph." Two
// forceY targets (connected nodes pulled toward the top third, isolated
// ones toward the bottom) is what produces that separation; forceLink only
// exists between nodes that actually have a depends_on edge, so isolated
// nodes never get pulled toward the flow by simulation alone.
const DAG_LAYOUT_SCRIPT: &str = r#"
    const { nodes, links } = await dioxus.recv();
    const width = 900, height = 560;
    nodes.forEach((n) => {
        n.x = width / 2 + (Math.random() - 0.5) * 200;
        n.y = n.connected ? height * 0.32 : height * 0.78;
    });
    const simulation = d3.forceSimulation(nodes)
        .force("link", d3.forceLink(links).id((n) => n.id).distance(70).strength(0.7))
        .force("charge", d3.forceManyBody().strength(-110))
        .force("x", d3.forceX(width / 2).strength(0.02))
        .force("y", d3.forceY((n) => n.connected ? height * 0.32 : height * 0.78).strength(0.3))
        .force("collide", d3.forceCollide(20))
        .stop();
    for (let i = 0; i < 300; i++) {
        simulation.tick();
    }
    dioxus.send(nodes.map((n) => ({ id: n.id, x: n.x, y: n.y })));
"#;

// A real node-link graph, unlike the entity Graph View (components/graph.rs
// — that one's a pure "distance from center" radial layout with no edges
// drawn at all). depends_on is a directed edge; arrows point from what's
// depended on toward what depends on it — same direction as "this has to
// happen before that." Deliberately no titles baked into the nodes
// themselves (user's call — "would be annoying to look at") — hover for
// the title via a native SVG <title> tooltip, click to open in the
// activity bar, same as every other moment list in the app.
#[component]
pub fn BlockingDagViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let mut current_moment = state.current_moment;
    let mut activity_bar_view = state.activity_bar_view;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let mut positions = use_signal(Vec::<DagNodeOut>::new);
    let mut zoom = use_signal(|| 1.0f64);
    let mut pan = use_signal(|| (0.0f64, 0.0f64));
    let mut dragging = use_signal(|| false);
    let mut drag_start = use_signal(|| (0.0f64, 0.0f64));
    let mut pan_start = use_signal(|| (0.0f64, 0.0f64));
    // Drag-and-drop dependency authoring (2026-07-29): drag from the
    // blocker node and drop on the dependent node — same direction as the
    // arrows already drawn below (source = what's depended on). Native
    // HTML5 drag/drop (draggable + ondragstart/ondragover/ondrop), same
    // primitive already used for Custom-sort-mode list reordering
    // elsewhere in this file, not raw mousemove coordinate tracking — a
    // live line following the cursor would need converting screen pixels
    // into this SVG's viewBox space through the pan/zoom transform below,
    // which isn't worth the fragility when the browser's own drag session
    // already suppresses mousemove (so it won't fight with the pan handlers
    // just below) and a highlighted drop-target ring is clear enough
    // feedback without it.
    let mut link_drag_from = use_signal(|| None::<String>);
    let mut link_drag_over = use_signal(|| None::<String>);

    use_effect(move || {
        let open: Vec<MomentType> = moments.read().iter()
            .filter(|m| m.completed_at.is_none())
            .cloned()
            .collect();

        if open.is_empty() {
            positions.set(vec![]);
            return;
        }

        let open_ids: std::collections::HashSet<String> = open.iter().map(|m| m.id.clone()).collect();
        let links: Vec<DagLinkIn> = open.iter()
            .flat_map(|m| m.dependency_ids().into_iter().filter(|d| open_ids.contains(d)).map(|d| DagLinkIn {
                source: d,
                target: m.id.clone(),
            }))
            .collect();
        let connected_ids: std::collections::HashSet<String> = links.iter()
            .flat_map(|l| [l.source.clone(), l.target.clone()])
            .collect();
        let nodes: Vec<DagNodeIn> = open.iter()
            .map(|m| DagNodeIn { id: m.id.clone(), connected: connected_ids.contains(&m.id) })
            .collect();

        spawn(async move {
            let eval = document::eval(DAG_LAYOUT_SCRIPT);
            if eval.send(DagLayoutIn { nodes, links }).is_ok() {
                let mut eval = eval;
                if let Ok(result) = eval.recv::<Vec<DagNodeOut>>().await {
                    positions.set(result);
                }
            }
        });
    });

    let open_lookup: std::collections::HashMap<String, MomentType> = moments.read().iter()
        .filter(|m| m.completed_at.is_none())
        .map(|m| (m.id.clone(), m.clone()))
        .collect();
    let links_for_render: Vec<(String, String)> = open_lookup.values()
        .flat_map(|m| m.dependency_ids().into_iter().filter(|d| open_lookup.contains_key(d)).map(|d| (d, m.id.clone())))
        .collect();

    let (pan_x, pan_y) = *pan.read();
    let zoom_val = *zoom.read();

    rsx! {
        div {
            class: "mx-4 mb-3",
            if open_lookup.is_empty() {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-muted-foreground text-center py-16",
                    "Nothing open right now."
                }
            } else {
                svg {
                    width: "100%",
                    height: "560",
                    view_box: "0 0 {DAG_CANVAS_W} {DAG_CANVAS_H}",
                    preserve_aspect_ratio: "xMidYMid meet",
                    class: if *dragging.read() {
                        "border border-border rounded-lg bg-background cursor-grabbing select-none"
                    } else {
                        "border border-border rounded-lg bg-background cursor-grab select-none"
                    },
                    onwheel: move |e: WheelEvent| {
                        e.prevent_default();
                        let dy = e.data().delta().strip_units().y;
                        let factor = if dy < 0.0 { 1.1 } else { 0.9 };
                        let current = *zoom.read();
                        zoom.set((current * factor).clamp(DAG_MIN_ZOOM, DAG_MAX_ZOOM));
                    },
                    onmousedown: move |e: MouseEvent| {
                        let coords = e.client_coordinates();
                        dragging.set(true);
                        drag_start.set((coords.x, coords.y));
                        pan_start.set(*pan.read());
                    },
                    onmousemove: move |e: MouseEvent| {
                        if *dragging.read() {
                            let coords = e.client_coordinates();
                            let (sx, sy) = *drag_start.read();
                            let (px, py) = *pan_start.read();
                            pan.set((px + (coords.x - sx), py + (coords.y - sy)));
                        }
                    },
                    onmouseup: move |_| {
                        dragging.set(false);
                        let source = link_drag_from.read().clone();
                        let target = link_drag_over.read().clone();
                        link_drag_from.set(None);
                        link_drag_over.set(None);
                        if let (Some(source_id), Some(target_id)) = (source, target) {
                            if source_id != target_id {
                                let token = auth_token;
                                let vault = active_vault;
                                spawn(async move {
                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                    add_dependency(&storage, moments, target_id, source_id).await;
                                });
                            }
                        }
                    },
                    onmouseleave: move |_| {
                        dragging.set(false);
                        link_drag_from.set(None);
                        link_drag_over.set(None);
                    },
                    defs {
                        marker {
                            id: "dag-arrow",
                            view_box: "0 0 10 10",
                            ref_x: "17",
                            ref_y: "5",
                            marker_width: "6",
                            marker_height: "6",
                            orient: "auto-start-reverse",
                            path {
                                d: "M 0 0 L 10 5 L 0 10 z",
                                class: "fill-muted-foreground",
                            }
                        }
                    }
                    g {
                        transform: "translate({pan_x}, {pan_y}) scale({zoom_val})",
                        for (source_id, target_id) in links_for_render.iter() {
                            {
                                let sp = positions.read().iter().find(|n| &n.id == source_id).map(|n| (n.x, n.y));
                                let tp = positions.read().iter().find(|n| &n.id == target_id).map(|n| (n.x, n.y));
                                match (sp, tp) {
                                    (Some((sx, sy)), Some((tx, ty))) => rsx! {
                                        line {
                                            key: "{source_id}-{target_id}",
                                            x1: "{sx}", y1: "{sy}", x2: "{tx}", y2: "{ty}",
                                            class: "stroke-muted-foreground/50",
                                            stroke_width: "1.5",
                                            marker_end: "url(#dag-arrow)",
                                        }
                                    },
                                    _ => rsx! {},
                                }
                            }
                        }
                        for node in positions.read().iter() {
                            {
                                let node_id = node.id.clone();
                                let (nx, ny) = (node.x, node.y);
                                let Some(m) = open_lookup.get(&node_id).cloned() else { return rsx! {}; };
                                let is_connected = links_for_render.iter().any(|(s, t)| s == &node_id || t == &node_id);
                                let is_drag_source = link_drag_from.read().as_deref() == Some(node_id.as_str());
                                let is_drop_target = link_drag_over.read().as_deref() == Some(node_id.as_str());
                                rsx! {
                                    g {
                                        key: "{node_id}",
                                        class: "cursor-pointer",
                                        onclick: {
                                            let m = m.clone();
                                            move |_| {
                                                current_moment.set(Some(m.clone()));
                                                activity_bar_view.set(ABView::Task);
                                                backdropTgl.set(true);
                                                activity_bar_tgl.set(true);
                                            }
                                        },
                                        // Plain mouse events, not HTML5 draggable/dragstart/
                                        // dragover/drop — SVG elements don't reliably support
                                        // that attribute family (dioxus_elements doesn't even
                                        // expose `draggable` on `g`). stop_propagation on
                                        // mousedown keeps this from also triggering the SVG's
                                        // own pan-start just below; the actual drop-target
                                        // detection and commit happen in the SVG's onmouseup.
                                        onmousedown: {
                                            let node_id = node_id.clone();
                                            move |e: Event<MouseData>| {
                                                e.stop_propagation();
                                                link_drag_from.set(Some(node_id.clone()));
                                            }
                                        },
                                        onmouseenter: {
                                            let node_id = node_id.clone();
                                            move |_| {
                                                if link_drag_from.read().is_some()
                                                    && link_drag_over.read().as_deref() != Some(node_id.as_str())
                                                {
                                                    link_drag_over.set(Some(node_id.clone()));
                                                }
                                            }
                                        },
                                        onmouseleave: {
                                            let node_id = node_id.clone();
                                            move |_| {
                                                if link_drag_over.read().as_deref() == Some(node_id.as_str()) {
                                                    link_drag_over.set(None);
                                                }
                                            }
                                        },
                                        if is_drop_target {
                                            circle {
                                                cx: "{nx}", cy: "{ny}", r: "13",
                                                class: "fill-none stroke-primary",
                                                stroke_width: "2",
                                            }
                                        }
                                        circle {
                                            cx: "{nx}",
                                            cy: "{ny}",
                                            r: "7",
                                            class: if is_drag_source {
                                                "fill-primary stroke-background opacity-40"
                                            } else if is_connected {
                                                "fill-primary stroke-background hover:opacity-80 transition-opacity"
                                            } else {
                                                "fill-muted-foreground/40 stroke-background hover:opacity-80 transition-opacity"
                                            },
                                            stroke_width: "1.5",
                                            title { "{m.title} — drag to another node to add a dependency" }
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
pub fn NotesViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let entities = state.entities;
    let mut current_moment = state.current_moment;
    let mut activity_bar_view = state.activity_bar_view;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let mut notes: Vec<MomentType> = moments.read().iter()
        .filter(|m| m.moment_type_id == 3i64)
        .cloned()
        .collect();
    notes.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            if notes.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-8",
                    "No notes yet."
                }
            } else {
                for m in notes.iter() {
                    div {
                        key: "{m.id}",
                        class: "flex items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors",
                        onclick: {
                            let m = m.clone();
                            move |_| {
                                current_moment.set(Some(m.clone()));
                                activity_bar_view.set(ABView::Task);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        div {
                            class: "flex flex-col min-w-0",
                            span { class: "text-sm font-medium text-foreground truncate", "{m.title}" }
                            span { class: "text-xs text-muted-foreground", "{entity_name(&m.entity_id)}" }
                        }
                    }
                }
            }
        }
    }
}

// Moved out of Settings 2026-07-22 — this is a data view (like Due/
// Scheduled), not an account/vault setting, so it belongs in the sidebar's
// "Views" list, not buried in Settings. Same underlying storage calls
// (get_deleted_moments/restore_moment — see api::storage) as when this
// lived in SettingsCmp, just relocated.
#[component]
pub fn RecentlyDeletedViewCmp() -> Element {
    let state = use_context::<AppState>();
    let entities = state.entities;
    let mut moments = state.moments;
    let active_vault = state.active_vault;
    let auth_token = state.auth_token;

    let mut deleted_moments = use_signal(Vec::<MomentType>::new);
    let mut deleted_loading = use_signal(|| true);
    let mut deleted_error = use_signal(|| None::<String>);

    use_effect(move || {
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        deleted_loading.set(true);
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.get_deleted_moments().await {
                Ok(data) => {
                    deleted_moments.set(data);
                    deleted_error.set(None);
                }
                Err(e) => {
                    clog!("Error fetching deleted moments: {}", e);
                    deleted_error.set(Some("Couldn't load recently deleted moments.".to_string()));
                }
            }
            deleted_loading.set(false);
        });
    });

    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let mut restore_error = use_signal(|| None::<String>);
    let mut restore_moment = move |moment: MomentType| {
        restore_error.set(None);
        let vault = *active_vault.read();
        let token = auth_token.read().clone();
        let id = moment.id.clone();
        spawn(async move {
            let storage = ActiveStorage::for_vault(vault, token);
            match storage.restore_moment(id.clone()).await {
                Ok(()) => {
                    deleted_moments.write().retain(|m| m.id != id);
                    let mut restored = moment.clone();
                    restored.deleted_at = None;
                    moments.write().push(restored);
                }
                Err(e) => {
                    clog!("Error restoring moment: {}", e);
                    restore_error.set(Some("Couldn't restore that moment — try again.".to_string()));
                }
            }
        });
    };

    rsx! {
        div {
            class: "mx-4 mb-3 flex flex-col gap-3",
            if let Some(msg) = restore_error.read().as_ref() {
                p { class: "text-sm text-destructive", "{msg}" }
            }
            if *deleted_loading.read() {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-muted-foreground text-center py-8",
                    "Loading…"
                }
            } else if let Some(msg) = deleted_error.read().as_ref() {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-destructive text-center py-8",
                    "{msg}"
                }
            } else if deleted_moments.read().is_empty() {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-muted-foreground text-center py-8",
                    "Nothing in the trash."
                }
            } else {
                div {
                    class: "rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
                    for moment in deleted_moments.read().iter().cloned() {
                        div {
                            key: "{moment.id}",
                            class: "flex items-center justify-between gap-3 px-4 py-3",
                            div {
                                class: "flex flex-col min-w-0",
                                span { class: "text-sm font-medium text-foreground truncate", "{moment.title}" }
                                span { class: "text-xs text-muted-foreground", "{entity_name(&moment.entity_id)}" }
                            }
                            button {
                                class: "text-sm text-primary hover:underline cursor-pointer shrink-0",
                                onclick: move |_| restore_moment(moment.clone()),
                                "Restore"
                            }
                        }
                    }
                }
            }
        }
    }
}

// Distinct from Priority: this answers "what does my week look like" (the
// literal calendar shape of what's due), not "what should I do next" (a
// composite urgency score). Same underlying due_at field, different
// operation — grouping by date instead of ranking by a formula.
#[component]
pub fn DueViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let entities = state.entities;
    let mut current_moment = state.current_moment;
    let mut activity_bar_view = state.activity_bar_view;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;

    let now = chrono::Utc::now();
    let today = now.date_naive();

    // Strictly overdue only, on purpose — the user's own words: "if its not
    // overdue, its not relevant in that view." This used to bucket
    // everything with any due date into Overdue/Today/This week/Later;
    // now anything not already past its due date is excluded outright,
    // not just de-emphasized into a lower bucket.
    let mut due: Vec<MomentType> = moments.read().iter()
        .filter(|m| {
            if m.completed_at.is_some() || crate::urgency::is_waiting(m, now) {
                return false;
            }
            let Some(due_at) = m.due_at.as_ref() else { return false; };
            let Some(parsed) = crate::urgency::parse_moment_datetime(due_at) else { return false; };
            parsed.date_naive() < today
        })
        .cloned()
        .collect();
    due.sort_by(|a, b| a.due_at.cmp(&b.due_at));

    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div {
            class: "mx-4 mb-3 flex flex-col gap-4",
            if due.is_empty() {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-muted-foreground text-center py-8",
                    "Nothing overdue right now."
                }
            } else {
                div {
                    class: "rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
                    for m in due.iter() {
                        div {
                            key: "{m.id}",
                            class: "flex items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors",
                            onclick: {
                                let m = m.clone();
                                move |_| {
                                    current_moment.set(Some(m.clone()));
                                    activity_bar_view.set(ABView::Task);
                                    backdropTgl.set(true);
                                    activity_bar_tgl.set(true);
                                }
                            },
                            div {
                                class: "flex flex-col min-w-0",
                                span { class: "text-sm font-medium text-foreground truncate", "{m.title}" }
                                span { class: "text-xs text-muted-foreground", "{entity_name(&m.entity_id)}" }
                            }
                            span {
                                class: "text-xs text-destructive shrink-0",
                                "{m.due_at.as_deref().unwrap_or(\"\").chars().take(10).collect::<String>()}"
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
pub fn PriorityViewCmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let entities = state.entities;
    let mut current_moment = state.current_moment;
    let mut activity_bar_view = state.activity_bar_view;
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let mut backdropTgl = state.backdropTgl;
    let weights = state.urgency_weights.read().clone();

    let now = chrono::Utc::now();
    let all = moments.read().clone();
    let all_entities = entities.read().clone();
    let mut ranked: Vec<(MomentType, crate::urgency::UrgencyBreakdown)> = all.iter()
        .filter(|m| m.moment_type_id != 3i64 && m.completed_at.is_none() && !crate::urgency::is_waiting(m, now))
        .map(|m| (m.clone(), crate::urgency::compute_urgency(m, &all, &all_entities, now, &weights)))
        .collect();
    ranked.sort_by(|a, b| b.1.total().partial_cmp(&a.1.total()).unwrap_or(std::cmp::Ordering::Equal));

    let entity_name = move |entity_id: &str| entities.read().iter()
        .find(|e| e.id == entity_id)
        .map(|e| e.name.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    rsx! {
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            if ranked.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-8",
                    "Nothing open right now."
                }
            } else {
                for (m, breakdown) in ranked.iter() {
                    div {
                        key: "{m.id}",
                        class: "flex items-center justify-between gap-3 px-4 py-3 cursor-pointer hover:bg-muted/50 transition-colors",
                        onclick: {
                            let m = m.clone();
                            move |_| {
                                current_moment.set(Some(m.clone()));
                                activity_bar_view.set(ABView::Task);
                                backdropTgl.set(true);
                                activity_bar_tgl.set(true);
                            }
                        },
                        div {
                            class: "flex flex-col min-w-0",
                            span { class: "text-sm font-medium text-foreground truncate", "{m.title}" }
                            span { class: "text-xs text-muted-foreground", "{entity_name(&m.entity_id)}" }
                        }
                        span {
                            class: "text-xs font-semibold shrink-0 px-2 py-0.5 rounded-full border border-border text-muted-foreground",
                            title: "{breakdown.describe()}",
                            "{breakdown.total():.1}"
                        }
                    }
                }
            }
        }
    }
}

// One (label, field-getter, field-setter) triple per UrgencyWeights field,
// driving both the settings form below and its persistence — adding a new
// weight later means adding one entry here, not touching the rendering or
// save logic.
fn weight_fields() -> Vec<(&'static str, &'static str, fn(&UrgencyWeights) -> f64, fn(&mut UrgencyWeights, f64))> {
    vec![
        ("Due date", "Ramps up to this value as a due date approaches, maxing out once overdue.", |w| w.due, |w, v| w.due = v),
        ("Priority: High", "Flat bonus when a task's priority is set to High.", |w| w.priority_high, |w, v| w.priority_high = v),
        ("Priority: Medium", "Flat bonus when a task's priority is set to Medium.", |w| w.priority_medium, |w, v| w.priority_medium = v),
        ("Priority: Low", "Flat bonus when a task's priority is set to Low.", |w| w.priority_low, |w, v| w.priority_low = v),
        ("Has a project", "Flat bonus when a task has a project assigned.", |w| w.project, |w, v| w.project = v),
        ("Scheduled (active)", "Flat bonus once a task's scheduled date has arrived.", |w| w.scheduled, |w, v| w.scheduled = v),
        ("Gravity", "Scales the task's own -100..100 importance dial.", |w| w.gravity, |w, v| w.gravity = v),
        ("Age", "Ramps up the longer a task has sat open, capping at 30 days.", |w| w.age, |w, v| w.age = v),
        ("Blocked", "Applied when waiting on an unfinished dependency — usually negative.", |w| w.blocked, |w, v| w.blocked = v),
        ("Blocking", "Applied when finishing this would unblock other open work.", |w| w.blocking, |w, v| w.blocking = v),
        ("Tags", "Applied per tag, capped at 3 tags.", |w| w.tags, |w, v| w.tags = v),
        ("Drift", "Ramps up the more a task's entity has drifted (their Distance), nudging you toward people you've neglected.", |w| w.drift, |w, v| w.drift = v),
    ]
}

// Trigger button + modal for editing the Priority view's ranking weights
// (see src/urgency.rs). Self-contained: owns its own open/closed state, so
// it can be dropped in next to the Priority header with no plumbing.
// Changes apply and persist immediately per field — no separate Save step,
// consistent with how every other per-field edit in this app already works.
#[component]
pub fn UrgencySettingsCmp() -> Element {
    let state = use_context::<AppState>();
    let mut weights = state.urgency_weights;
    let mut open = use_signal(|| false);

    let mut persist = move |w: UrgencyWeights| {
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|win| win.local_storage().ok().flatten()) {
            storage.set("urgency_weights", &w.as_storage_string()).ok();
        }
        weights.set(w);
    };

    rsx! {
        button {
            class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-sm",
            title: "Adjust priority ranking weights",
            onclick: move |_| open.set(true),
            "⚙"
        }
        if *open.read() {
            div {
                class: "fixed inset-0 bg-black/40 z-100 flex items-center justify-center p-4",
                onclick: move |_| open.set(false),
                div {
                    class: "w-full max-w-lg rounded-lg border border-border bg-card shadow-lg",
                    onclick: move |e| e.stop_propagation(),
                    div {
                        class: "flex items-center justify-between h-14 px-4 border-b border-border",
                        span { class: "text-lg font-semibold text-foreground", "Priority ranking weights" }
                        button {
                            class: "h-8 w-8 flex items-center justify-center rounded-md text-muted-foreground hover:bg-muted hover:text-foreground transition-colors cursor-pointer text-lg leading-none",
                            onclick: move |_| open.set(false),
                            "×"
                        }
                    }
                    div {
                        class: "flex flex-col gap-3 px-4 py-4 max-h-[70vh] overflow-y-auto",
                        p {
                            class: "text-xs text-muted-foreground -mt-1 mb-1",
                            "Each row adds (or subtracts, for negative weights) to a task's score when that factor applies. Set a weight to 0 to ignore it entirely."
                        }
                        for (label, help, getter, setter) in weight_fields() {
                            div {
                                key: "{label}",
                                class: "flex items-center justify-between gap-3",
                                div {
                                    class: "flex flex-col min-w-0",
                                    span { class: "text-sm text-foreground", "{label}" }
                                    span { class: "text-xs text-muted-foreground", "{help}" }
                                }
                                input {
                                    r#type: "number",
                                    step: "0.5",
                                    class: "w-20 h-9 shrink-0 rounded-md border border-input bg-background text-sm text-foreground px-2 text-right focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
                                    value: "{getter(&weights.read())}",
                                    oninput: move |e| {
                                        if let Ok(v) = e.value().parse::<f64>() {
                                            let mut w = weights.read().clone();
                                            setter(&mut w, v);
                                            persist(w);
                                        }
                                    },
                                }
                            }
                        }
                    }
                    div {
                        class: "px-4 py-3 border-t border-border",
                        Button {
                            variant: ButtonVariant::Secondary,
                            full_width: true,
                            on_click: move |_| persist(UrgencyWeights::default()),
                            "Reset to defaults"
                        }
                    }
                }
            }
        }
    }
}
