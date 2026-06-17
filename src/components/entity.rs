use dioxus::prelude::*;
use crate::types::*;
use crate::theme::*;
use crate::AppState;
use crate::api::{
    createEntity,
    getEntities,
    getEntityTypes,
};
use lumen_blocks::components::collapsible::{
    Collapsible, CollapsibleContent, CollapsibleTrigger,
};

#[component]
pub fn entity_view_cmp() -> Element {
    let state = use_context::<AppState>();
    let current_entity = state.current_entity;
    let mut is_graphs_open = use_signal(|| false);
    let mut is_stats_open = use_signal(|| false);
    let mut is_info_open = use_signal(|| false);
    let x = if let Some(entity) = current_entity() {
        rsx! {
            div {
                h1 {
                    class: "text-2xl w-full flex justify-center",
                    "{entity.name}"
                }
                div {
                    class: "flex justify-center gap-x-4",
                    a {
                        onclick: move  |_| {
                            let tgl = *is_stats_open.read();
                            is_info_open.set(!tgl);
                        },
                        "info"

                    }
                    a {
                        onclick: move  |_| {
                            let tgl = *is_stats_open.read();
                            is_stats_open.set(!tgl);
                        },
                        "History"

                    }
                    a {
                        onclick: move  |_| {
                            let tgl = *is_stats_open.read();
                            is_stats_open.set(!tgl);
                        },
                        "stats"

                    }
                    a {
                        onclick: move  |_| {
                            let tgl = *is_graphs_open.read();
                            is_graphs_open.set(!tgl);
                        },
                        "graphs"

                    }
                }
                if *is_stats_open.read(){
                    div{
                        class: "flex justify-around",
                        div {
                            class: "m-4 w-100",
                            div {
                                class: "border-b",
                            "Relationship Details:" 
                            }
                            div {
                                class: "flex justify-between",
                                span {"promise kept:"}
                                span {"10"}
                            }
                            div {
                                class: "flex justify-between",
                                span {"promise pending:"}
                                span {"10"}
                            }
                            div {
                                class: "flex justify-between",
                                span {"tasks with reactions:"}
                                span {"10"}
                            }
                            div {
                                class: "flex justify-between",
                                span {"Distance:"}
                                span {"10"}
                            }
                            div {
                                class: "flex justify-between",
                                span {"Drift:"}
                                span {"10"}
                            }
                        }
                        div {
                            class: "m-4 w-100",
                            div {
                                class: "border-b",
                            "Surperlatives" 
                            }
                            div {
                                class: "flex justify-between",
                                span {"Entity Ranking"}
                                span {"#2 out of 30"}
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! { 
            div {
                h1 {
                    class: "text-2xl w-full flex justify-center",
                    "All"
                }
            }
        }
    }; x
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
            let new_entity = NewEntityType {
                name: form_data.name.clone(),
                entity_type_id: None,
                parent_entity_id: None,
                user_id: None,
                archived_at: None,
            };
    
            match createEntity(new_entity, token.read().clone().unwrap_or_default()).await {
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
            match getEntityTypes(token.read().clone().unwrap_or_default()).await {
                Ok(data) => entityTypes.set(data),
                Err(e) => clog!("Error fetching entities: {}",e),
            }
        });
    });
    
    rsx! {
        if *entityModalTgl.read() {
            div {
                class: "modal z-100",
                onclick: move |_| entityModalTgl.set(false),
                div {
                    class: "modal-content",
                    onclick: move |e| e.stop_propagation(),
                    span { onclick: move |_| entityModalTgl.set(false), class: "close", "×" }
                    h1 {class:"text-3xl","New Entity"}
                    hr {}
                    div {
                        class: "flex items-center",
                        label { "Type" }
                        select {
                            class: "border my-2 mx-4",
                            value: "{form.read().entity_type_sel}",
                            oninput: move |e| {
                                form.write().entity_type_sel = e.value();
                            },
                            for entity_type in entityTypes.iter() {
                                option {
                                    value: "{entity_type.id}",
                                    "{entity_type.name}"
                                }
                            }
                        }
                    }
                    div {
                        class: "flex items-center",
                        label { "Name" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().name}",
                            oninput: move |e| {
                                form.write().name = e.value();
                            },
                        }
                    }
                    div {
                        class: "flex items-center",
                        label { "Relationship to you" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().relationship}",
                            oninput: move |e| {
                                form.write().relationship = e.value();
                            },
                        }
                    }
    
                    div {
                        class: "flex items-center",
                        label { "How you met" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().meeting}",
                            oninput: move |e| {
                                form.write().meeting = e.value();
                            },
                        }
                    }
    
                    div {
                        class: "flex items-center",
                        label { "Birthday" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().bday}",
                            oninput: move |e| {
                                form.write().bday = e.value();
                            },
                        }
                    }
    
                    div {
                        class: "flex items-center",
                        label { "Their location" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().location}",
                            oninput: move |e| {
                                form.write().location = e.value();
                            },
                        }
                    }
    
                    div {
                        class: "flex items-center",
                        label { "Why they matter" }
                        input {
                            class: "border my-2 mx-4",
                            value: "{form.read().why}",
                            oninput: move |e| {
                                form.write().why = e.value();
                            },
                        }
                    }
                    a {
                        onclick: move |e| onsubmit(e),
                        class: "text-white my-2 px-10 py-2",
                        style: "background-color:{HL};",
                        "submit"
                    }
                }
            }
        }
    }
}
