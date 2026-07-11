use crate::Route;
use crate::theme::*;
use dioxus::prelude::*;
use crate::AppState;
use crate::ui::*;
use crate::View::*;
use crate::ABView;
// use dioxus_sdk_window::size::{get_window_size, use_window_size};
// use dioxus_sdk::utils::window::use_window_size;

use crate::components::{
    MomentCmp,
    MomentListCmp,
    MomentInputCmp,
    EntityModalCmp,
    ab_task_cmp,
    peep_list_cmp,
    NotesSectionCmp,
};

use crate::api::{
    createEntity,
    createMoment,
};

use crate::types::{EntityType, MomentType, NewMomentType};


// #[cfg(target_arch = "wasm32")]
// fn window_size() -> (f64, f64) {
//     use web_sys::window;
//     let w = window().unwrap();
//     let width = w.inner_width().unwrap().as_f64().unwrap();
//     let height = w.inner_height().unwrap().as_f64().unwrap();
//     (width, height)
// }
const NAV_LINK_CLASS: &str = "block rounded-md px-3 py-2 text-sm font-medium text-foreground hover:bg-muted transition-colors cursor-pointer";
const NAV_LINK_DESTRUCTIVE_CLASS: &str = "block rounded-md px-3 py-2 text-sm font-medium text-destructive hover:bg-destructive/10 transition-colors cursor-pointer";

#[component]
pub fn Sidebar() -> Element {
    let state = use_context::<AppState>();
    let mut sidebarTgl = state.sidebarTgl;
    rsx! {
        div {
            class: if *sidebarTgl.read() {
                "fixed top-0 left-0 h-full overflow-y-auto w-64 shadow-xl z-40 transform translate-x-0 transition-transform duration-200 bg-background border-r border-border"
            } else {
                "fixed top-0 left-0 h-full overflow-y-auto w-64 shadow-xl z-40 transform -translate-x-full transition-transform duration-200 bg-background border-r border-border"
            },
            div {
                class:"h-10",
            }
            profile_cmp { }
            div {
                class: "px-3 mt-8",
                peep_list_cmp { }
            }
        }
    }
}

#[component]
pub fn profile_cmp() -> Element {
    let state = use_context::<AppState>();
    rsx! {
        div {
            class: "flex flex-col gap-y-1 px-3",
            if state.auth_token.read().is_none() {
                a {
                    class: NAV_LINK_CLASS,
                    onclick: move |_| {
                        let nav = navigator();
                        nav.push(Route::LoginCMP {});
                    },
                    "Login"
                }
            }
            a {
                class: NAV_LINK_CLASS,
                onclick: move |_| {
                    let nav = navigator();
                    nav.push(Route::LoginCMP {});
                },
                "Profile"
            }
            a {
                class: NAV_LINK_CLASS,
                onclick: move |_| {
                    let nav = navigator();
                    nav.push(Route::LoginCMP {});
                },
                "Crisis View"
            }
            a {
                class: NAV_LINK_DESTRUCTIVE_CLASS,
                onclick: move |_| {
                    let nav = navigator();
                    nav.push(Route::Logout {});
                },
                "Logout"
            }
        }
    }
}

#[component]
pub fn Navbar() -> Element {
    let state = use_context::<AppState>();
    let mut activity_bar_tgl = state.activity_bar_tgl;
    let activity_bar_view = state.activity_bar_view;
    let mut momentInputTgl = state.momentInputTgl;
    let mut backdropTgl = state.backdropTgl;
    let mut sidebarTgl = state.sidebarTgl;
    let current_moment = state.current_moment;
    let current_view = state.currentView;
    let current_entity = state.current_entity;
    let moment = current_moment.read().clone();
    let activity_bar_class = if *activity_bar_tgl.read() {
        "openedbtw fixed inset-y-0 right-0 z-50 w-2/3 transition-transform duration-300 translate-x-0"
    } else {
        "closedbtw fixed inset-y-0 right-0 z-50 w-1/3 transition-transform duration-300 translate-x-full"
    };
 
    //
    let header_title = match current_view.read().clone() {
        Inbox => "".to_string(),
        SELF => "".to_string(),
        Entity => "".to_string()
    };

    rsx! {

        button {
            class: "xl:hidden fixed flex z-51 left-4 top-1 text-2xl",
            onclick: move |_| {
                let tgl = *sidebarTgl.read();
                sidebarTgl.set(!tgl);
                clog!("clicked hamburger");
            },
            if *sidebarTgl.read() { "" } else { "☰" }
        }


        if *backdropTgl.read() || *sidebarTgl.read() {
            div {
                id: "backdrop",
                class: "xl:hidden fixed inset-0 bg-black/20 z-30",
                onclick: move |_| {
                    clog!("clicked");
                    momentInputTgl.set(false);
                    sidebarTgl.set(false);
                    backdropTgl.set(false);
                    activity_bar_tgl.set(false);
                }
            }
        }

        div {
            style: "background-color:{BG};",
            EntityModalCmp { }
            button {
                id: "add-moment-button",
                class: "fixed h-20 w-20 bottom-4 right-4 z-10 rounded-lg m-8 shadow-lg",
                style: "background-color:{HL};",
                onclick: move |_| {
                    let current = *momentInputTgl.read();
                    momentInputTgl.set(!current);
                },
                if *momentInputTgl.read() { "✕" } else { "+" }
            }
            Sidebar { }
            div {
                style: "background-color:{BG};",
                class: "flex h-screen w-full overflow-hidden",
                div {
                    class: "hidden xl:block h-full overflow-y-auto w-64 border-r border-border bg-background transform translate-x-0 transition-transform duration-200",
                    div {
                        class:"h-10",
                    }
                    profile_cmp { }
                    div {
                        class: "px-3 mt-8",
                        peep_list_cmp { }
                    }
                }
                div {
                    class: "xl:w-2/3 w-full overflow-y-auto [&::-webkit-scrollbar]:w-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:bg-black/30",
                    "{header_title}"
                    Outlet::<Route> {}
                }
                div {
                    id:"activity-bar",
                    class: "{activity_bar_class}",
                    style: "background-color:{BG}; ",
                    if let Some(_) = current_moment.read().clone() {
                        match activity_bar_view.read().clone() {
                            ABView::Task => rsx! {
                                ab_task_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            }
                        }
                    }
                }

                if *momentInputTgl.read() { MomentInputCmp { } } 
            }
        }
    }
}
