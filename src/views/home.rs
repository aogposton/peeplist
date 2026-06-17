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
};

use crate::api::{
    createEntity,
    createMoment,
    getEntities,
    getMoments,
};

use crate::types::{EntityType, MomentType, NewMomentType};


#[component]
pub fn Home() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut entities = state.entities;
    let mut sidebarTgl = state.sidebarTgl;
    let current_view = state.currentView;
    let current_entity = state.current_entity;
    let auth_token = state.auth_token;
    let filtered_moments = moments.read().clone() .into_iter() .filter(|m| m.moment_type_id == 3).collect::<Vec<_>>();
    let entity_moments = match current_entity.read().clone() {
        Some(entity) => moments.read().iter()
            .filter(|m| m.entity_id == entity.id)
            .cloned()
            .collect::<Vec<_>>(),
        None => vec![],
    };

    use_effect(move || {
        let token = auth_token;
        spawn(async move {
            match getMoments(token.read().clone().unwrap_or_default()).await {
                Ok(data) => moments.set(data),
                Err(e) => log::info!("Error fetching moments: {}", e),
            }

            match getEntities(token.read().clone().unwrap_or_default()).await {
                Ok(data) => entities.set(data),
                Err(e) => log::info!("Error fetching entities: {}", e),
            }
        });
    });


    rsx! {
        div {
            class: "w-full h-min-screen pt-2 mb-10",
            style: "background-color:{BG};",
            match current_view.read().clone() {
                Inbox => rsx! {
                    entity_view_cmp { }
                    div {class:"h-4"}
                    MomentInputCmp { }
                    div {class:"h-4"}
                    MomentListCmp { moments: moments.read().clone() }
                    NotesSectionCmp { moments: moments.read().clone() }
                    CompletedSectionCmp { moments: moments.read().clone() }
                },
                Entity | SELF => rsx! {
                    entity_view_cmp { }
                    div {class:"h-4"}
                    MomentInputCmp { }
                    div {class:"h-4"}
                    MomentListCmp { 
                        moments: moments.read().iter()
                            .filter(|m| current_entity.read().as_ref().map_or(false, |e| m.entity_id == e.id))
                            .cloned()
                            .collect::<Vec<_>>()
                    }
                    NotesSectionCmp { 
                        moments: moments.read().iter()
                            .filter(|m| current_entity.read().as_ref().map_or(false, |e| m.entity_id == e.id))
                            .cloned()
                            .collect::<Vec<_>>()
                    }
                    CompletedSectionCmp { 
                        moments: moments.read().iter()
                            .filter(|m| current_entity.read().as_ref().map_or(false, |e| m.entity_id == e.id))
                            .cloned()
                            .collect::<Vec<_>>()
                    }
                }
            }
        }

    }
}
