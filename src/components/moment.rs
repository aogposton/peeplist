use dioxus::prelude::*;
use crate::AppState;
use crate::ABView;
use crate::SortMode;
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
    let mut show_menu = use_signal(|| false);
    let mut menu_coords = use_signal(|| (0.0, 0.0));
    let mut last_moment_right_clicked_id = use_signal(String::new);
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let onConvertTo = move |mType: i64| {
        let id = last_moment_right_clicked_id.read().clone();
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

    const MENU_ITEM_CLASS: &str = "px-2 py-1.5 text-sm rounded-sm hover:bg-muted transition-colors cursor-pointer";

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
                                {
                                    let moment_id = moment.id.clone();
                                    rsx! {
                                        MomentCmp {
                                            moment: moment.clone(),
                                            is_note: true,
                                            oncontextmenu: move |evt: MouseEvent| {
                                                evt.prevent_default();
                                                let coords = evt.client_coordinates();
                                                last_moment_right_clicked_id.set(moment_id.clone());
                                                menu_coords.set((coords.x, coords.y));
                                                show_menu.set(true);
                                            },
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if show_menu() {
            div {
                class: "fixed bg-popover text-popover-foreground border border-border shadow-md rounded-md p-1 z-50 min-w-40",
                style: "top: {menu_coords().1}px; left: {menu_coords().0}px;",
                onclick: move |_| { show_menu.set(false); },
                ul {
                    class: "flex flex-col",
                    li {
                        class: MENU_ITEM_CLASS,
                        onclick: move |_| onConvertTo(1i64),
                        "Convert to Task"
                    }
                    li {
                        class: MENU_ITEM_CLASS,
                        onclick: move |_| onConvertTo(2i64),
                        "Convert to Promise"
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
    let mut show_menu = use_signal(|| false);
    let mut menu_coords = use_signal(|| (0.0, 0.0));
    //
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
                                MomentCmp {
                                    moment: moment.clone(),
                                    oncontextmenu: move |evt: MouseEvent| {
                                        evt.prevent_default();
                                        let coords = evt.client_coordinates();
                                        menu_coords.set((coords.x, coords.y));
                                        show_menu.set(true);
                                    },
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
    let mut show_menu = use_signal(|| false);
    let mut menu_coords = use_signal(|| (0.0, 0.0));
    let mut last_moment_right_clicked_id = use_signal(String::new);
    let mut last_moment_right_clicked_type = use_signal(|| 0);
    let mut dragged_id = use_signal(|| None::<String>);
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut sort_mode = state.sort_mode;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let moments_list = props.moments.clone();

    let mut set_sort_mode = move |mode: SortMode| {
        sort_mode.set(mode);
        // Desktop has no preference persistence yet — see main.rs's startup
        // effect.
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("sort_mode", mode.as_storage_str()).ok();
        }
    };

    let onConvertTo = move |mType:i64| {
        let id = last_moment_right_clicked_id.read().clone();
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
    }
    let is_custom = current_sort_mode == SortMode::Custom;

    const MENU_ITEM_CLASS: &str = "px-2 py-1.5 text-sm rounded-sm hover:bg-muted transition-colors cursor-pointer";
    let sort_btn_class = |active: bool| if active {
        "px-2 py-1 text-xs rounded-md bg-muted text-foreground font-medium cursor-pointer"
    } else {
        "px-2 py-1 text-xs rounded-md text-muted-foreground hover:bg-muted transition-colors cursor-pointer"
    };

    rsx! {
        div {
            class: "mx-4 mb-1 flex items-center gap-1",
            span { class: "text-xs text-muted-foreground mr-1", "Sort:" }
            span {
                class: sort_btn_class(current_sort_mode == SortMode::Default),
                onclick: move |_| set_sort_mode(SortMode::Default),
                "Default"
            }
            span {
                class: sort_btn_class(current_sort_mode == SortMode::DueDate),
                onclick: move |_| set_sort_mode(SortMode::DueDate),
                "Due date"
            }
            span {
                class: sort_btn_class(current_sort_mode == SortMode::Custom),
                onclick: move |_| set_sort_mode(SortMode::Custom),
                "Custom (drag to reorder)"
            }
        }
        div {
            class: "mx-4 mb-3 rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
            for moment in display_list.iter() {
                {
                    let moment = moment.clone();
                    let target_id = moment.id.clone();
                    let list_snapshot = display_list.clone();
                    rsx! {
                        div {
                            key: "{target_id}",
                            draggable: is_custom,
                            class: if is_custom { "cursor-move" } else { "" },
                            ondragstart: {
                                let target_id = target_id.clone();
                                move |_| dragged_id.set(Some(target_id.clone()))
                            },
                            ondragover: move |e| e.prevent_default(),
                            ondrop: {
                                let target_id = target_id.clone();
                                move |e| {
                                    e.prevent_default();
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
                            MomentCmp {
                                moment: moment.clone(),
                                oncontextmenu: move |evt: MouseEvent| {
                                    evt.prevent_default();
                                    let coords = evt.client_coordinates();
                                    last_moment_right_clicked_id.set(moment.id.clone());
                                    last_moment_right_clicked_type.set(moment.moment_type_id);
                                    menu_coords.set((coords.x, coords.y));
                                    show_menu.set(true);
                                },
                            }
                        }
                    }
                }
            }
        }
        if show_menu() {
            if last_moment_right_clicked_type.read().clone() == 1i64 { //task
                div {
                    class: "fixed bg-popover text-popover-foreground border border-border shadow-md rounded-md p-1 z-50 min-w-40",
                    style: "top: {menu_coords().1}px; left: {menu_coords().0}px;",
                    onclick: move |_| { show_menu.set(false); }, // Close menu on click
                    ul {
                        class: "flex flex-col",
                        li {
                            class: MENU_ITEM_CLASS,
                            onclick: move |_| onConvertTo(2i64),
                            "Convert to promise"
                        }
                        li {
                            class: MENU_ITEM_CLASS,
                            onclick: move |_| onConvertTo(3i64),
                            "Convert to Note"
                        }
                    }
                }
            }
            if last_moment_right_clicked_type.read().clone() == 2i64 { //promise
                div {
                    class: "fixed bg-popover text-popover-foreground border border-border shadow-md rounded-md p-1 z-50 min-w-40",
                    style: "top: {menu_coords().1}px; left: {menu_coords().0}px;",
                    onclick: move |_| { show_menu.set(false); }, // Close menu on click
                    ul {
                        class: "flex flex-col",
                        li {
                            class: MENU_ITEM_CLASS,
                            onclick: move |_| onConvertTo(1i64),
                            "Convert to Task"
                        }
                        li {
                            class: MENU_ITEM_CLASS,
                            onclick: move |_| onConvertTo(3i64),
                            "Convert to Note"
                        }
                        li { class: MENU_ITEM_CLASS, "Convert to Promise" }
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

    let due_display = props.moment.due_at.as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            let is_overdue = dt.with_timezone(&chrono::Utc) < chrono::Utc::now() && props.moment.completed_at.is_none();
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
    let is_blocked = props.moment.depends_on.as_ref().is_some_and(|dep_id| {
        moments.read().iter().find(|m| &m.id == dep_id)
            .is_some_and(|dep| dep.moment_type_id != 3 && dep.completed_at.is_none())
    });
    // The list row only ever said "Blocked" with no way to tell what by —
    // the detail panel already names the blocker (see ab_task_cmp's
    // "Blocked by" line), the row itself just never did.
    let blocked_on_title = props.moment.depends_on.as_ref().and_then(|dep_id| {
        moments.read().iter().find(|m| &m.id == dep_id).map(|m| m.title.clone())
    });

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
            oncontextmenu: move |e| props.oncontextmenu.call(e),
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
                // p {
                //     class: "text-slate-400 mt-1",
                //     "{description}"
                // }
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
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut selected_entity = use_signal(|| None::<String>);
    // Description had a signal declared here before but nothing in this
    // component ever rendered a field for it — form_data.description (what
    // actually gets submitted, below) was always empty. Tab now reveals a
    // real field and focuses it, bound directly to form.description.
    let mut description_open = use_signal(|| false);
    let mut description_el = use_signal(|| None::<std::rc::Rc<MountedData>>);

    
    let mut form = use_signal(move || {
        let mut f = MomentForm::default();

        if let Some(entity) = current_entity.read().clone() {
            f.entity_sel = entity.id.clone();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            f.entity_sel = active_vault.read().effective(&auth_token.read()).self_entity_id().to_string();
        }


        f
    });

    use_effect(move || {
        if let Some(entity) = current_entity.read().clone() {
            form.write().entity_sel = entity.id.clone();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            form.write().entity_sel = active_vault.read().effective(&auth_token.read()).self_entity_id().to_string();
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
        let entity_id = parsed.entity_id.clone().unwrap_or(form_data.entity_sel.clone());
        let token = auth_token;
                        let vault = active_vault;

        spawn(async move {
            let new_moment = NewMomentType {
                title: parsed.title.clone(),
                entity_id,
                description: Some(form_data.description.clone()),
                gravity: Some(1),
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
                    // moments here, where the real list is available.
                    if let Some(dep_title) = parsed.depends_on_title.clone() {
                        let dep_id = moments.read().iter()
                            .find(|m| m.entity_id == created_entity_id
                                && m.id != created_id
                                && m.completed_at.is_none()
                                && m.title.to_lowercase() == dep_title.to_lowercase())
                            .map(|m| m.id.clone());
                        if let Some(dep_id) = dep_id {
                            if storage.update_moment_field(created_id.clone(), "depends_on", serde_json::json!(Some(dep_id.clone()))).await.is_ok() {
                                if let Some(m) = moments.write().iter_mut().find(|m| m.id == created_id) {
                                    m.depends_on = Some(dep_id);
                                }
                            }
                        }
                    }
                    if parsed.has_metadata() {
                        let meta = MomentMetadata {
                            tags: parsed.tags_add.clone(),
                            sort_index: None,
                            priority: parsed.priority.clone(),
                            project: parsed.project.clone(),
                            scheduled_at: parsed.scheduled_at.clone(),
                            until_at: parsed.until_at.clone(),
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
                        placeholder: "Title · @name pri:H due:tomorrow +tag ;t;/;p;/;n;".to_string(),
                        entities: entities.read().clone(),
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
                        for entity in entities.iter().filter(|e| !is_self_entity(&e.id)) {
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
    let mut input_el = use_signal(|| None::<std::rc::Rc<MountedData>>);
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
    let dropdown_open = !matches.is_empty() || show_add_new;
    let option_count = matches.len() + if show_add_new { 1 } else { 0 };
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
                    move |e: Event<KeyboardData>| {
                        if dropdown_open {
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
                                    if h < matches.len() {
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

    // Taskwarrior-style single dependency (see MomentType::depends_on).
    // Read the current moment's own depends_on live off the `moments`
    // signal by id, rather than off the `moment` snapshot taken once from
    // current_moment at panel-open time — the depends-on <select>'s own
    // handler below updates `moments` (so the list view and this panel's
    // dependency_candidates/blocking_count already reflect it live) but
    // never updates `current_moment` itself, so `moment.depends_on` goes
    // stale the instant you change the dependency without closing and
    // reopening the panel. That staleness previously meant a freshly-set
    // dependency didn't actually block completion (or show as selected in
    // the dropdown) until the panel was closed and reopened.
    let entity_id = moment.entity_id.clone();
    let dependency_candidates: Vec<MomentType> = moments.read().iter()
        .filter(|m| m.entity_id == entity_id && m.id != id && m.completed_at.is_none())
        .cloned()
        .collect();
    let current_depends_on = moments.read().iter().find(|m| m.id == id).and_then(|m| m.depends_on.clone());
    let blocked_on = current_depends_on.clone().and_then(|dep_id| {
        moments.read().iter().find(|m| m.id == dep_id).cloned()
    });
    // Notes have no completion state (no checkbox is ever rendered for
    // one), so a dependency on a note can never resolve. Depending on a
    // note is still allowed — useful as a reference/context link — it just
    // never actually blocks completion the way a task/promise dependency
    // does. Symmetrically, a note can't "block" anything it's depended on
    // by either, for the same reason.
    let is_blocked = blocked_on.as_ref().is_some_and(|dep| dep.moment_type_id != 3 && dep.completed_at.is_none());
    let blocking_count = if moment.moment_type_id == 3 {
        0
    } else {
        moments.read().iter()
            .filter(|m| m.depends_on == Some(id.clone()) && m.completed_at.is_none())
            .count()
    };

    // Taskwarrior-style attributes, part 2 (priority/project/scheduled/
    // until) — same live-by-id read as current_depends_on above, for the
    // same reason: the moment snapshot taken at panel-open time never
    // updates as fields get edited within the same open session.
    let current_metadata = moments.read().iter().find(|m| m.id == id).and_then(|m| m.metadata.clone()).unwrap_or_default();

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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1 min-h-0",

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
                        Input {
                            input_type: "datetime-local",
                            size: InputSize::Medium,
                            class: Some("min-w-[12rem]".to_string()),
                            value: due_at.clone().unwrap_or_default().chars().take(16).collect::<String>(),
                            on_change: {
                                let id = id.clone();
                                move |e: Event<FormData>| {
                                    let id = id.clone();
                                    let token = auth_token;
                        let vault = active_vault;
                                    spawn(async move {
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "due_at", serde_json::json!(Some(e.value()))).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.due_at = Some(e.value());
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

                input {
                    class: "text-xl font-semibold text-foreground w-full bg-transparent border-none outline-none focus-visible:ring-2 focus-visible:ring-ring rounded-md -mx-1 px-1 py-1",
                    value: "{title}",
                    // Commit on blur, not every keystroke — a local-vault write
                    // rewrites the whole entity file (see api/vault_format.rs),
                    // and this cuts Supabase chatter today too.
                    onchange: {
                        let id = id.clone();
                        move |e| {
                            let id = id.clone();
                            let token = auth_token;
                        let vault = active_vault;
                            spawn(async move {
                                let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                match storage.update_moment_field(id.clone(), "title", serde_json::json!(e.value())).await {
                                    Ok(_) => {
                                        let mut list = moments.write();
                                        if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                            m.title = e.value();
                                        }
                                    }
                                    Err(e) => log::info!("Error updating moment: {}", e),
                                }
                            });
                        }
                    }
                }

                textarea {
                    class: "w-full min-h-32 rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring resize-y",
                    placeholder: "Add a description...",
                    value: "{description.clone().unwrap_or_default()}",
                    onchange: {
                        let id = id.clone();
                        move |e| {
                            let id = id.clone();
                            let token = auth_token;
                        let vault = active_vault;
                            spawn(async move {
                                let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                match storage.update_moment_field(id.clone(), "description", serde_json::json!(e.value())).await {
                                    Ok(_) => {
                                        let mut list = moments.write();
                                        if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                            m.description = Some(e.value());
                                        }
                                    }
                                    Err(e) => log::info!("Error updating moment: {}", e),
                                }
                            });
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
                    class: "rounded-lg border border-border bg-background overflow-hidden",
                    Collapsible {
                        CollapsibleTrigger {
                            class: "text-sm font-medium text-foreground hover:no-underline hover:bg-muted/50",
                            "Advanced"
                        }
                        CollapsibleContent {
                            div {
                                class: "flex flex-col gap-4 p-3 border-t border-border",

                                div {
                                    class: "flex flex-col gap-y-1.5",
                                    Label { size: LabelSize::Small, "Priority" }
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
                                    Label { size: LabelSize::Small, "Project" }
                                    Input {
                                        full_width: true,
                                        placeholder: "e.g. Home.Garden",
                                        value: "{current_metadata.project.clone().unwrap_or_default()}",
                                        on_change: {
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
                                    Label { size: LabelSize::Small, "Scheduled" }
                                    Input {
                                        input_type: "datetime-local",
                                        size: InputSize::Medium,
                                        value: current_metadata.scheduled_at.clone().unwrap_or_default().chars().take(16).collect::<String>(),
                                        on_change: {
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
                                    Label { size: LabelSize::Small, "Until" }
                                    Input {
                                        input_type: "datetime-local",
                                        size: InputSize::Medium,
                                        value: current_metadata.until_at.clone().unwrap_or_default().chars().take(16).collect::<String>(),
                                        on_change: {
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
                                                    "#{tag}"
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
                                        Input {
                                            size: InputSize::Small,
                                            placeholder: "Add tag...",
                                            value: "{tag_input.read()}",
                                            on_input: move |e: Event<FormData>| tag_input.set(e.value()),
                                        }
                                        Button {
                                            variant: ButtonVariant::Secondary,
                                            size: ButtonSize::Small,
                                            on_click: {
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
                                            "Blocked by \"{blocked_on.as_ref().map(|d| d.title.clone()).unwrap_or_default()}\""
                                        }
                                    }
                                    if blocking_count > 0 {
                                        div {
                                            class: "mb-2 text-xs text-muted-foreground",
                                            "Blocking {blocking_count} other open moment(s)"
                                        }
                                    }
                                    select {
                                        class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                        oninput: {
                                            let id = id.clone();
                                            move |e| {
                                                let val = e.value();
                                                let new_dep: Option<String> = if val.is_empty() { None } else { Some(val) };
                                                let id = id.clone();
                                                let token = auth_token;
                                let vault = active_vault;
                                                spawn(async move {
                                                    let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                                    match storage.update_moment_field(id.clone(), "depends_on", serde_json::json!(new_dep)).await {
                                                        Ok(_) => {
                                                            let mut list = moments.write();
                                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                                m.depends_on = new_dep;
                                                            }
                                                        }
                                                        Err(e) => log::info!("Error updating depends_on: {}", e),
                                                    }
                                                });
                                            }
                                        },
                                        option { value: "", selected: current_depends_on.is_none(), "None" }
                                        for candidate in dependency_candidates.iter() {
                                            option {
                                                value: "{candidate.id}",
                                                selected: current_depends_on == Some(candidate.id.clone()),
                                                "{candidate.title}"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                div {
                    class: "rounded-lg border border-border bg-background overflow-hidden",
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
                                Label { size: LabelSize::Small, "Reaction" }
                                gravity_select {
                                    ival: ReactionForm.read().value,
                                    onchange: move |e: i32| {
                                        ReactionForm.write().value = e;
                                    }
                                }
                            }
                            Button {
                                variant: ButtonVariant::Secondary,
                                full_width: true,
                                disabled: is_blocked,
                                on_click: {
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
                Button {
                    variant: ButtonVariant::Destructive,
                    full_width: true,
                    on_click: move |_| {
                        let moment = moment_sig.read().clone();
                        let mut moments = moments.clone();
                        let token = auth_token;
                        let vault = active_vault;
                        spawn(async move {
                            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                            match storage.delete_moment(moment.clone()).await {
                                Ok(()) => {
                                    moments.write().retain(|m| m.id != moment.id);
                                }
                                Err(e) => clog!("Error: {}", e),
                            }
                        });
                        activity_bar_tgl.set(false);
                        backdropTgl.set(false);
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
            .filter(|m| m.depends_on.as_deref() == Some(current.as_str()) && m.completed_at.is_some())
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

// Taskwarrior's "waiting" concept: moments given a future scheduled_at (via
// the scheduled: quick-capture keyword) so they're off your mind until
// that date, with somewhere to go check on all of them at once in the
// meantime. Nothing currently hides a scheduled moment from the normal
// list before its date arrives — this is a review view, not a filter.
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

    let mut scheduled: Vec<(MomentType, chrono::DateTime<chrono::Utc>)> = moments.read().iter()
        .filter(|m| m.completed_at.is_none())
        .filter_map(|m| {
            let s = m.metadata.as_ref()?.scheduled_at.as_ref()?;
            let dt = chrono::DateTime::parse_from_rfc3339(s).ok()?.with_timezone(&chrono::Utc);
            Some((m.clone(), dt))
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
                            if *dt <= now { "Arrived" } else { "{dt.format(\"%b %d\")}" }
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

    let mut due: Vec<MomentType> = moments.read().iter()
        .filter(|m| m.completed_at.is_none() && m.due_at.is_some())
        .cloned()
        .collect();
    due.sort_by(|a, b| a.due_at.cmp(&b.due_at));

    let bucket_of = |m: &MomentType| -> &'static str {
        let Some(due_at) = m.due_at.as_ref() else { return "Later"; };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(due_at) else { return "Later"; };
        let due_date = parsed.with_timezone(&chrono::Utc).date_naive();
        let days = (due_date - today).num_days();
        match days {
            d if d < 0 => "Overdue",
            0 => "Today",
            1..=6 => "This week",
            _ => "Later",
        }
    };

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
                    "Nothing with a due date right now."
                }
            } else {
                for bucket in ["Overdue", "Today", "This week", "Later"] {
                    {
                        let items: Vec<MomentType> = due.iter().filter(|m| bucket_of(m) == bucket).cloned().collect();
                        rsx! {
                            if !items.is_empty() {
                                div {
                                    key: "{bucket}",
                                    div {
                                        class: if bucket == "Overdue" {
                                            "text-xs font-semibold uppercase tracking-wide text-destructive mb-1.5 px-1"
                                        } else {
                                            "text-xs font-semibold uppercase tracking-wide text-muted-foreground mb-1.5 px-1"
                                        },
                                        "{bucket}"
                                    }
                                    div {
                                        class: "rounded-lg border border-border bg-background divide-y divide-border overflow-hidden",
                                        for m in items.iter() {
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
                                                    "{m.due_at.as_deref().unwrap_or(\"\").chars().take(10).collect::<String>()}"
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
    let mut ranked: Vec<(MomentType, crate::urgency::UrgencyBreakdown)> = all.iter()
        .filter(|m| m.moment_type_id != 3i64 && m.completed_at.is_none())
        .map(|m| (m.clone(), crate::urgency::compute_urgency(m, &all, now, &weights)))
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
