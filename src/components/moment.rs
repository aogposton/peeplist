use dioxus::prelude::*;
use crate::AppState;
use crate::ABView;
use crate::SortMode;
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

#[component]
pub fn CheckboxCmp(props: CheckboxProps) -> Element {
    rsx! {
        div {
            label {
                class: "flex items-center cursor-pointer relative",
                input {
                    r#type: "checkbox",
                    checked: props.checked,
                    class: "peer h-5 w-5 cursor-pointer transition-colors appearance-none rounded-md border-2 border-input bg-background checked:bg-primary checked:border-primary hover:border-primary/50",
                    onchange: move |e| props.on_change.call(e.checked()),
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
                                            let tags = m.metadata.as_ref().map(|md| md.tags.clone()).unwrap_or_default();
                                            let new_meta = MomentMetadata { tags, sort_index: Some(idx as f64) };
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

    let mut is_completed = use_signal(|| props.moment.completed_at.is_some());
    let mut visual_opacity = use_signal(|| if props.moment.completed_at.is_some() { "0.4" } else { "1" });

    let due_display = props.moment.due_at.as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            let is_overdue = dt.with_timezone(&chrono::Utc) < chrono::Utc::now() && props.moment.completed_at.is_none();
            (dt.format("%b %d").to_string(), is_overdue)
        });


    let onCheckClicked = move |checked: bool| {
        is_completed.set(checked);
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
                    let mut list = moments.write();
                    if let Some(m) = list.iter_mut().find(|m| m.id == updated.id) {
                        m.completed_at = updated.completed_at;
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
    let entities = state.entities;
    let mut title = use_signal(|| String::new());
    let mut description = use_signal(|| String::new());
    let current_entity = state.current_entity;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;
    let mut selected_entity = use_signal(|| None::<String>); 

    
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

    let mut submit_moment = move || {
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
        let token = auth_token;
                        let vault = active_vault;

        spawn(async move {
            let new_moment = NewMomentType {
                title: form_data.title.clone(),
                entity_id: form_data.entity_sel.clone(),
                description: Some(form_data.description.clone()),
                gravity: Some(1),
                moment_type_id: 1,
                deleted_at: None,
            };
            let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
            match storage.create_moment(new_moment).await {
                Ok(created_moment) => moments.write().insert(0,created_moment),
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
                onkeypress: move |e| {
                    if e.key() == Key::Enter {
                        submit_moment();
                    }
                },
                div {
                    class: "flex-1",
                    Input {
                        full_width: true,
                        name: "task_title",
                        placeholder: "Title",
                        value: "{form.read().title}",
                        on_input: move |e: Event<FormData>| form.write().title = e.value(),
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

            button {
                class: "xl:hidden block w-full mt-2 rounded-md py-2 text-sm font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer",
                style: "background-color:{HL};",
                onclick: move |e| submit_moment(),
                "Add Moment",
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

    let moment = current_moment.read().clone().unwrap();
    let moment_sig = use_signal(|| moment.clone());
    let mut reactions = use_signal(|| moment.reactions.clone().unwrap_or_default());
    let id = moment.id.clone();
    let description = moment.description;
    let title = moment.title;
    let gravity = moment.gravity.unwrap_or(0);
    let due_at = moment.due_at;
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
        let sort_index = moment_sig.read().metadata.as_ref().and_then(|m| m.sort_index);
        let new_meta = MomentMetadata { tags: new_tags.clone(), sort_index };
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
    let entity_id = moment.entity_id.clone();
    let dependency_candidates: Vec<MomentType> = moments.read().iter()
        .filter(|m| m.entity_id == entity_id && m.id != id && m.completed_at.is_none())
        .cloned()
        .collect();
    let blocked_on = moment.depends_on.clone().and_then(|dep_id| {
        moments.read().iter().find(|m| m.id == dep_id).cloned()
    });
    let is_blocked = blocked_on.as_ref().is_some_and(|dep| dep.completed_at.is_none());
    let blocking_count = moments.read().iter()
        .filter(|m| m.depends_on == Some(id.clone()) && m.completed_at.is_none())
        .count();

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
                class: "flex flex-col gap-4 px-4 py-4 overflow-y-auto flex-1",

                if moment.moment_type_id != 3i64 {
                    div {
                        class: "flex items-center gap-3",
                        CheckboxCmp {
                            checked: moment.completed_at.clone().is_some(),
                            on_change: {
                                let id = id.clone();
                                move |checked| {
                                    let id = id.clone();
                                    let token = auth_token;
                        let vault = active_vault;
                                    spawn(async move {
                                        let completed_at = if checked { Some(chrono::Utc::now().to_rfc3339()) } else { None };
                                        let storage = ActiveStorage::for_vault(*vault.read(), token.read().clone());
                                        match storage.update_moment_field(id.clone(), "completed_at", serde_json::json!(completed_at)).await {
                                            Ok(_) => {
                                                let mut list = moments.write();
                                                if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                    m.completed_at = completed_at;
                                                }
                                            }
                                            Err(e) => clog!("Error updating moment: {}", e),
                                        }
                                    });
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

                div {
                    class: "rounded-lg border border-border bg-background p-3",
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
                    class: "rounded-lg border border-border bg-background p-3",
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
                        option { value: "", selected: moment.depends_on.is_none(), "None" }
                        for candidate in dependency_candidates.iter() {
                            option {
                                value: "{candidate.id}",
                                selected: moment.depends_on == Some(candidate.id.clone()),
                                "{candidate.title}"
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
                                on_click: {
                                    let id = id.clone();
                                    move |_| {
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
                                            let new_r = reaction_result.expect("hello?");
                                            // update the signal so UI reacts
                                            reactions.write().push(new_r.clone());
                                            let mut list = moments.write();
                                            if let Some(m) = list.iter_mut().find(|m| m.id == id) {
                                                m.completed_at = completed_at;
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

// Taskwarrior-esque urgency score, highest = most worth doing next.
// due: ramps up to +12 as the due date approaches, maxes out once overdue.
// gravity: the user's own -100..100 importance dial, scaled to -5..+5.
// age: tiny nudge for things that have been sitting around, caps at +2 after 30 days.
// blocked: a moment waiting on an unfinished dependency is heavily deprioritized (-8).
// blocking: a moment that's itself blocking other open work gets a bonus (+8),
// since finishing it unblocks something else.
fn compute_urgency(m: &MomentType, all_moments: &[MomentType], now: chrono::DateTime<chrono::Utc>) -> f64 {
    let due_score = m.due_at.as_deref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| {
            let days = (dt.with_timezone(&chrono::Utc) - now).num_seconds() as f64 / 86400.0;
            if days <= 0.0 {
                12.0
            } else if days <= 14.0 {
                12.0 * (1.0 - days / 14.0)
            } else {
                0.0
            }
        })
        .unwrap_or(0.0);

    let gravity_score = (m.gravity.unwrap_or(0) as f64 / 100.0) * 5.0;

    let age_score = chrono::DateTime::parse_from_rfc3339(&m.created_at).ok()
        .map(|dt| {
            let days = (now - dt.with_timezone(&chrono::Utc)).num_seconds() as f64 / 86400.0;
            (days / 30.0).clamp(0.0, 1.0) * 2.0
        })
        .unwrap_or(0.0);

    let blocked_penalty = match m.depends_on.as_deref() {
        Some(dep_id) => {
            let dep_done = all_moments.iter()
                .find(|x| x.id == dep_id)
                .map(|x| x.completed_at.is_some())
                .unwrap_or(true);
            if dep_done { 0.0 } else { -8.0 }
        }
        None => 0.0,
    };

    let blocking_bonus = if all_moments.iter().any(|x| x.depends_on.as_deref() == Some(m.id.as_str()) && x.completed_at.is_none()) {
        8.0
    } else {
        0.0
    };

    due_score + gravity_score + age_score + blocked_penalty + blocking_bonus
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

    let now = chrono::Utc::now();
    let all = moments.read().clone();
    let mut ranked: Vec<(MomentType, f64)> = all.iter()
        .filter(|m| m.moment_type_id != 3i64 && m.completed_at.is_none())
        .map(|m| (m.clone(), compute_urgency(m, &all, now)))
        .collect();
    ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

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
                for (m, score) in ranked.iter() {
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
                            "{score:.1}"
                        }
                    }
                }
            }
        }
    }
}
