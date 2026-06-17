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

#[component]
pub fn peep_list_cmp() -> Element {
    let state = use_context::<AppState>();
    let entities = state.entities;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;
    let mut entityModalTgl = state.entityModalTgl;
    let mut isHovering = use_signal(|| false);


    let opacity = if isHovering() { "0.3" } else { "1.0" };

    rsx! {
        div {
            div { 
                class: "h-10 py-2 w-full flex transition-all justify-center",
                style: "background-color:{HL}; color: white; opacity: {opacity};",
                onclick: move |_| entityModalTgl.set(true), 
                onmouseenter: move |_| isHovering.set(true),
                onmouseout: move |_| isHovering.set(false),
                "Add entity +"
            }

            a {
                class:"hover:bg-blue-100 pointer w-full",
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::Inbox);
                }, 
                "- All"
            }
            br {}
            a {
                class:"hover:bg-blue-100 pointer w-full",
                onclick: move |_| {
                    current_entity.set(None);
                    currentView.set(View::SELF);
                }, 
                "- Self"
            }
            br {}
            for entity in entities.read().clone().into_iter(){
                a {
                    class:"hover:bg-blue-100 pointer w-full",
                    onclick: move |_| {
                        current_entity.set(Some(entity.clone()));
                        currentView.set(View::Entity);
                    }, 
                    "- {entity.name}"
                }
                br {}
            }
        }
    }
}
