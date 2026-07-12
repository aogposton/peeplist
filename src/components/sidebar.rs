use dioxus::prelude::*;
use crate::theme::*;
use crate::ui::*;
use crate::View;
use crate::ABView;
use crate::Route;
use crate::AppState;

use crate::components::{
    ab_task_cmp,
};

const NAV_LINK_CLASS: &str = "block rounded-md px-3 py-2 text-sm font-medium text-foreground hover:bg-muted transition-colors cursor-pointer";

#[component]
pub fn peep_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let entities = state.entities;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut entityModalTgl = state.entityModalTgl;

    rsx! {
        div {
            class: "flex flex-col gap-y-1",
            button {
                class: "w-full rounded-md py-2 mb-3 text-sm font-semibold text-white transition-opacity hover:opacity-90 cursor-pointer",
                style: "background-color:{HL};",
                onclick: move |_| entityModalTgl.set(true),
                "+ Add entity"
            }

            a {
                class: NAV_LINK_CLASS,
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::Inbox);
                },
                "All"
            }
            a {
                class: NAV_LINK_CLASS,
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::SELF);
                },
                "Self"
            }
            a {
                class: NAV_LINK_CLASS,
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::Priority);
                },
                "Priority"
            }
            for entity in entities.read().clone().into_iter(){
                a {
                    class: NAV_LINK_CLASS,
                    onclick: move |_| {
                        current_entity.set(Some(entity.clone()));
                        currentView.set(View::Entity);
                    },
                    "{entity.name}"
                }
            }
        }
    }
}

#[component]
pub fn tag_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let moments = state.moments;
    let mut tag_filter = state.tag_filter;

    let mut tags: Vec<String> = moments.read().iter()
        .filter_map(|m| m.metadata.as_ref())
        .flat_map(|meta| meta.tags.clone())
        .collect();
    tags.sort();
    tags.dedup();

    rsx! {
        div {
            class: "flex flex-col gap-y-1",
            if tags.is_empty() {
                span {
                    class: "px-3 text-xs text-muted-foreground",
                    "No tags yet"
                }
            } else {
                for tag in tags.iter() {
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
                                tag_filter.set(if is_active { None } else { Some(tag.clone()) });
                            }
                        },
                        "#{tag}"
                    }
                }
            }
        }
    }
}
