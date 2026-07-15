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
    GraphViewCmp,
};

use crate::api::ActiveStorage;

use crate::types::MomentType;


#[component]
pub fn Home() -> Element {
    let state = use_context::<AppState>();
    let mut moments = state.moments;
    let mut entities = state.entities;
    let mut sidebarTgl = state.sidebarTgl;
    let current_view = state.currentView;
    let current_entity = state.current_entity;
    let tag_filter = state.tag_filter;
    let auth_token = state.auth_token;
    let active_vault = state.active_vault;

    let has_tag = |m: &MomentType, tag: &str| {
        m.metadata.as_ref().is_some_and(|meta| meta.tags.iter().any(|t| t == tag))
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
        });
    });


    rsx! {
        div {
            class: "w-full h-min-screen pt-2 mb-10",
            style: "background-color:{BG};",
            match current_view.read().clone() {
                Inbox => {
                    let visible: Vec<MomentType> = moments.read().iter()
                        .filter(|m| tag_filter.read().as_ref().map_or(true, |tag| has_tag(m, tag)))
                        .cloned()
                        .collect();
                    rsx! {
                        entity_view_cmp { }
                        div {class:"h-4"}
                        div { class: "hidden xl:block", MomentInputCmp { } }
                        div {class:"h-4"}
                        MomentListCmp { moments: visible.clone() }
                        NotesSectionCmp { moments: visible.clone() }
                        CompletedSectionCmp { moments: visible }
                    }
                },
                Entity => {
                    let visible: Vec<MomentType> = moments.read().iter()
                        .filter(|m| current_entity.read().as_ref().map_or(false, |e| m.entity_id == e.id))
                        .filter(|m| tag_filter.read().as_ref().map_or(true, |tag| has_tag(m, tag)))
                        .cloned()
                        .collect();
                    rsx! {
                        entity_view_cmp { }
                        div {class:"h-4"}
                        div { class: "hidden xl:block", MomentInputCmp { } }
                        div {class:"h-4"}
                        MomentListCmp { moments: visible.clone() }
                        NotesSectionCmp { moments: visible.clone() }
                        CompletedSectionCmp { moments: visible }
                    }
                },
                Priority => rsx! {
                    div {
                        class: "px-4 pt-4 flex items-start justify-between gap-3",
                        div {
                            h1 { class: "text-2xl font-semibold text-foreground mb-1", "Priority" }
                            p { class: "text-sm text-muted-foreground mb-4", "Open tasks and promises across everyone, ranked by urgency." }
                        }
                        UrgencySettingsCmp { }
                    }
                    PriorityViewCmp { }
                },
                Graph => rsx! {
                    GraphViewCmp { }
                }
            }
        }

    }
}
