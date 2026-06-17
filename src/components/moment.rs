use dioxus::prelude::*;
use crate::AppState;
use crate::theme::*;
use crate::ui::*;
use crate::types::*;
use crate::api::*;
use lumen_blocks::components::input::Input;
use lumen_blocks::components::button::{Button, ButtonVariant};
use lumen_blocks::components::dropdown::{
    Dropdown, DropdownContent, DropdownItem, DropdownTrigger,
};

#[component]
pub fn CheckboxCmp(props: CheckboxProps) -> Element {
    rsx! {
        div {
            class: "shadow-inner",
            label {
                class: "flex items-center cursor-pointer relative",
                input {
                    r#type: "checkbox",
                    checked: props.checked,
                    class: "peer h-5 w-5 cursor-pointer transition-all appearance-none rounded border border-slate-300 checked:bg-blue-600 checked:border-blue-600",
                    onchange: move |e| props.on_change.call(e.checked()),
                }
                span {
                    class: "absolute text-white opacity-0 peer-checked:opacity-100 inset-shadow-xl top-1/2 left-1/2 transform -translate-x-1/2 -translate-y-1/2",
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
    let mut expanded = use_signal(|| false);
    let mut show_menu = use_signal(|| false);
    let mut menu_coords = use_signal(|| (0.0, 0.0));
    //
    rsx! {
        div {
            class: "w-full px-4", // header toggle
            button {
                class: "flex flex-row items-center gap-2 w-full py-2",
                onclick: move |_| {
                    let current = *expanded.read();
                    expanded.set(!current);
                },
                span {
                    class: "text-black font-medium",
                    "Notes"
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
                    for moment in props.moments.iter() {
                        if  moment.moment_type_id == 3i64 {
                            MomentCmp {
                                moment: moment.clone(),
                                is_note: true,
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
#[component]
pub fn CompletedSectionCmp(props: MomentListProps) -> Element {
    let mut expanded = use_signal(|| false);
    let mut show_menu = use_signal(|| false);
    let mut menu_coords = use_signal(|| (0.0, 0.0));
    //
    rsx! {
        div {
            class: "w-full px-4", // header toggle
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
    let mut last_moment_right_clicked_id = use_signal(|| 0);
    let mut last_moment_right_clicked_type = use_signal(|| 0);
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let auth_token = state.auth_token;

    let moments_list = props.moments.clone();

    let onConvertTo = move |mType:i64| {
        let id = last_moment_right_clicked_id.read().clone();
        let token = auth_token;
        let note_type = mType;
        spawn(async move {
                match update_moment_field(id,"moment_type_id",serde_json::json!(Some(note_type.clone())), token.read().clone().unwrap_or_default()).await {
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


    rsx! {
        div {
            class: "w-full px-4",
            if true {
                for moment in moments_list.clone().into_iter() {
                    if !moment.completed_at.is_some() && moment.moment_type_id != 3i64 {
                        MomentCmp {
                            moment: moment.clone(),
                            oncontextmenu: move |evt: MouseEvent| {
                                evt.prevent_default();
                                let coords = evt.client_coordinates();
                                last_moment_right_clicked_id.set(moment.id);
                                last_moment_right_clicked_type.set(moment.moment_type_id);
                                menu_coords.set((coords.x, coords.y));
                                show_menu.set(true);
                            },
                        }
                    }
                }

            }
            if show_menu() {
                if last_moment_right_clicked_type.read().clone() == 1i64 { //task
                    div {
                        class: "absolute bg-white border shadow-md rounded p-2 z-50",
                        style: "top: {menu_coords().1}px; left: {menu_coords().0}px;",
                        onclick: move |_| { show_menu.set(false); }, // Close menu on click
                        ul {
                            li { 
                                class:"hover:bg-slate-100",
                                onclick: move |_| onConvertTo(2i64),
                                "Convert to promise" 
                            }
                            li { 
                                class:"hover:bg-slate-100",
                                onclick: move |_| onConvertTo(3i64),
                                "Convert to Note" 
                            }
                        }
                    }
                }
                if last_moment_right_clicked_type.read().clone() == 2i64 { //promise
                    div {
                        class: "absolute bg-white border shadow-md rounded p-2 z-50",
                        style: "top: {menu_coords().1}px; left: {menu_coords().0}px;",
                        onclick: move |_| { show_menu.set(false); }, // Close menu on click
                        ul {
                            li { 
                                class:"hover:bg-slate-100",
                                onclick: move |_| onConvertTo(1i64),
                                "Convert to Task" 
                            }
                            li { 
                                class:"hover:bg-slate-100",
                                onclick: move |_| onConvertTo(3i64),
                                "Convert to Note" 
                            }
                            li { class:"hover:bg-slate-100", "Convert to Promise" }
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
    let mut backdropTgl = state.backdropTgl;
    let mut moments = state.moments;  // add this
    let mut current_moment = state.current_moment;
    let mut is_hovering = use_signal(|| false);
    let auth_token = state.auth_token;
    let mut bg = match (is_hovering(), props.moment.moment_type_id == 2i64) {
        (true, true)   => BGpromiseHover,
        (true, false)  => BGhover,
        (false, true)  => BGpromise,
        (false, false) => BG,
    };
    let title = props.moment.title.clone();
    let description = props.moment.description.clone().unwrap_or_default();
    let moment = props.moment.clone();

    let mut is_completed = use_signal(|| props.moment.completed_at.is_some());
    let mut visual_opacity = use_signal(|| if props.moment.completed_at.is_some() { "0.4" } else { "1" });


    let onCheckClicked = move |checked: bool| {
        is_completed.set(checked);
        visual_opacity.set(if checked { "0" } else { "1" });  // fade to nothing
                                                              //
        let mut updated = moment.clone();
        updated.completed_at = if checked { Some(chrono::Utc::now().to_rfc3339()) } else { None };
        let token = auth_token;
        let id = updated.clone().id;
        spawn(async move {
            if checked {
                gloo_timers::future::TimeoutFuture::new(350).await;
            }
            match update_moment_field(id,"completed_at",serde_json::json!(updated.completed_at), token.read().clone().unwrap_or_default()).await {
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
            class: "flex flex-row items-center px-4 py-3 w-full border-b border-gray-100 transition-opacity duration-300",
            style: "background-color:{bg}; opacity:{visual_opacity}; transition: opacity 300ms ease; transition: opacity 300ms ease;",
            onmouseleave: move |_| is_hovering.set(false) ,
            onmouseenter: move |_| is_hovering.set(true),
            onclick: move |_| {
                current_moment.set(Some(props.moment.clone()));
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
                class: "ml-4",
                style: "color: {BaseFont};",
                h2 {
                    class: "items-center font-semibold",
                    "{title}"
                }
                // p {
                //     class: "text-slate-400 mt-1",
                //     "{description}"
                // }
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
    let mut selected_entity = use_signal(|| None::<String>); 

    
    let mut form = use_signal(move || {
        let mut f = MomentForm::default();

        if let Some(entity) = current_entity.read().clone() {
            f.entity_sel = entity.id.to_string();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            f.entity_sel = "0".to_string();
        }
        

        f
    });

    use_effect(move || {
        if let Some(entity) = current_entity.read().clone() {
            form.write().entity_sel = entity.id.to_string();
            selected_entity.set(Some(entity.name));
        }else{
            selected_entity.set(Some("Self".to_string()));
            form.write().entity_sel = "0".to_string();
        }
    });

    let mut submit_moment = move || {
        let form_data = form.read().clone();

        let mut reset_form = MomentForm::default();

        if let Some(entity) = current_entity.read().clone() {
            selected_entity.set(Some(entity.name));
            reset_form.entity_sel = entity.id.to_string();
        }else{
            // selected_entity.set("Self".to_string());
            reset_form.entity_sel = form_data.entity_sel.clone();
        }

        form.set(reset_form);
        let token = auth_token;

        spawn(async move {
            let new_moment = NewMomentType {
                title: form_data.title.clone(),
                entity_id: form_data.entity_sel.parse::<i64>().ok().expect("oops"),
                description: Some(form_data.description.clone()),
                gravity: Some(1),
                moment_type_id: 1,
                deleted_at: None,
            };
            match createMoment(new_moment, token.read().clone().unwrap_or_default()).await {
                Ok(created_moment) => moments.write().insert(0,created_moment),
                Err(e) => clog!("Error: {}", e),
            }
        });
    };
    //
    rsx! {

        div {
            div{
                onkeypress: move |e| {
                    if e.key() == Key::Enter {
                        submit_moment();
                    }
                },
                Input {
                    class: "m-2 px-2 w-[calc(100%-16px)]",
                    name: "task_title",
                    placeholder: "Title",
                    value: "{form.read().title}",
                    on_input: move |e: Event<FormData>| form.write().title = e.value(),
                    icon_right: rsx! {
                        div {
                            class: "mr-4",
                            Dropdown {
                                // disabled: true,
                                DropdownTrigger {
                                    Button {
                                        // variant: ButtonVariant::Outline,
                                        {selected_entity.read().clone().unwrap_or("Self".to_string())}
                                    }
                                }
                                DropdownContent {
                                    for entity in entities.iter() {
                                        DropdownItem::<String> {
                                            value:  "{entity.id}".to_string(),
                                            index: 0,
                                            on_select: {
                                                let name = entity.name.clone();
                                                let id = entity.id.clone();
                                                move |_| {
                                                    *selected_entity.write() = Some(name.clone());
                                                    form.write().entity_sel = id.clone().to_string();
                                                    // if let Some(entity) = current_entity.read().clone() {
                                                    //     selected_entity.set(entity.name);
                                                    // }else{
                                                    //     selected_entity.set("Self".to_string());
                                                    //     f.entity_sel = "0".to_string();
                                                    // }
                                                }
                                            },
                                            "{entity.name}"
                                        }
                                    }
                                }
                            }
                        }
 
                    }
                }

                div {
                    class: "flex items-center",
                    // label { "Link to:" }
                    // Input {
                    //     class: "my-2 mx-4",
                    //     on_input: move |e: Event<FormData>| form.write().entity_sel = e.value(),
                    //     input_type: "text".to_string(),
                    // }
                }

            }
            // input {
            //     name: "task_description",
            //     class: "w-full text-lg bg-slate-100 px-4 py-2 text-slate-400 outline-none",
            //     placeholder: "Description",
            //     value: "{form.read().description}",
            //     oninput: move |e| form.write().description = e.value()
            // }

            button {
                class: "xl:hidden block w-full bg-slate-800 text-white font-semibold rounded-xl py-2",
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
    let auth_token = state.auth_token;

    let moment = current_moment.read().clone().unwrap();
    let moment_sig = use_signal(|| moment.clone());
    let mut reactions = use_signal(|| moment.reactions.clone().unwrap_or_default());
    let id = moment.id;
    let description = moment.description;
    let title = moment.title;
    let gravity = moment.gravity.unwrap_or(0);
    let due_at = moment.due_at;
    let mut ReactionForm = use_signal(ReactionForm::default);

    rsx! {
        div {
            div {
                class: "flex items-center w-full border-b h-12 px-4 gap-x-4",
                div { class:"block xl:hidden", "X" }
                if moment.moment_type_id.clone() != 3i64 {
                    CheckboxCmp {
                        checked: moment.completed_at.clone().is_some(),
                        on_change: move |checked| {
                            let id = id;
                            let token = auth_token;
                            spawn(async move {
                                let completed_at = if checked { Some(chrono::Utc::now().to_rfc3339()) } else { None };
                                match update_moment_field(id, "completed_at", serde_json::json!(completed_at), token.read().clone().unwrap_or_default()).await {
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
                input {
                    r#type: "datetime-local",
                    value: due_at.clone().unwrap_or_default().chars().take(16).collect::<String>(),
                    onchange: move |e| {
                        let id = id;
                        let token = auth_token;
                        spawn(async move {
                            match update_moment_field(id, "due_at", serde_json::json!(Some(e.value())), token.read().clone().unwrap_or_default()).await {
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

            div {
                class: "flex items-center justify-center py-2 border-b w-full",
                "gravity"
                gravity_select {
                    ival: gravity,
                    onchange: move |e: i32| {
                        let id = id;
                        let token = auth_token;
                        spawn(async move {
                            match update_moment_field(id, "gravity", serde_json::json!(Some(e)), token.read().clone().unwrap_or_default()).await {
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

            input {
                class: "mt-4 ml-4 text-2xl text-black w-full focus:border-teal focus:outline-none focus:ring-0",
                value: "{title}",
                oninput: move |e| {
                    let id = id;
                    let token = auth_token;
                    spawn(async move {
                        match update_moment_field(id, "title", serde_json::json!(e.value()), token.read().clone().unwrap_or_default()).await {
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

            textarea {
                class: "px-4 w-full h-50 focus:border-teal focus:outline-none focus:ring-0",
                value: "{description.clone().unwrap_or_default()}",
                oninput: move |e| {
                    let id = id;
                    let token = auth_token;
                    spawn(async move {
                        match update_moment_field(id, "description", serde_json::json!(e.value()), token.read().clone().unwrap_or_default()).await {
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

            div {
                class: "text-2xl m-2",
                "Reactions"
                if reactions.read().is_empty() {
                    div {
                        class: "border m-2 rounded",
                        textarea {
                            class: "px-4 w-full h-50 focus:border-teal focus:outline-none focus:ring-0",
                            value: "{ReactionForm.read().description}",
                            placeholder: "What was the consequence?",
                            oninput: move |e| ReactionForm.write().description = e.value()
                        }
                        div {
                            class: "flex items-center justify-center py-2 border-b w-full",
                            "Reaction"
                            gravity_select {
                                ival: ReactionForm.read().value,
                                onchange: move |e: i32| {
                                    ReactionForm.write().value = e;
                                }
                            }
                        }
                        div {
                            class: "flex",
                            button_cmp {
                                label: rsx!{"Complete with reaction"},
                                class: "border-green-200 flex justify-center items-center",
                                btnclick: move |_| {
                                    let id = id;
                                    let token = auth_token;
                                    spawn(async move {
                                        let completed_at = Some(chrono::Utc::now().to_rfc3339());
                                        let new_reaction = NewReactionType {
                                            moment_id: id,
                                            description: ReactionForm.read().description.clone(),
                                            value: ReactionForm.read().value,
                                        };
                                        let (complete_result, reaction_result) = futures::join!(
                                            update_moment_field(id, "completed_at", serde_json::json!(completed_at), token.read().clone().unwrap_or_default()),
                                            createReaction(new_reaction, token.read().clone().unwrap_or_default())
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
                                    });
                                }
                            }
                        }
                    }
                } else {
                    for reaction in reactions.read().clone().into_iter() {
                        div {
                            class: "w-1/2 flex gap-x-10 items-center text-lg",
                            div {
                                onclick: move |_| {
                                    let reaction_id = reaction.id;
                                    let reaction = reaction.clone();
                                    let token = auth_token;
                                    spawn(async move {
                                        match deleteReaction(reaction, token.read().clone().unwrap_or_default()).await {
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
                                },
                                fa_trash {}
                            }
                            a { class: "text-underline hover:text-blue-100", "{reaction.description}" }
                            span { "{reaction.value:?}" }
                        }
                    }
                }
            }

            button_cmp {
                label: rsx!{"Delete" fa_trash {}},
                class: "border-red-200 flex justify-center items-center",
                btnclick: move |_| {
                    let moment = moment_sig.read().clone();
                    let mut moments = moments.clone();
                    let token = auth_token;
                    spawn(async move {
                        match deleteMoment(moment.clone(), token.read().clone().unwrap_or_default()).await {
                            Ok(()) => {
                                moments.write().retain(|m| m.id != moment.id);
                            }
                            Err(e) => clog!("Error: {}", e),
                        }
                    });
                    activity_bar_tgl.set(false);
                }
            }
        }
    }
}
