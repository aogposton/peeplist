// The dioxus prelude contains a ton of common items used in dioxus apps. It's a good idea to import wherever you
// need dioxus
//
use std::fmt;
use dioxus::prelude::*;
use crate::types::*;
use crate::api::VaultKind;
use views::{Logout, LoginCMP, Home};
use layouts::{Navbar};


// web_sys::console::log_1 panics on a native (non-wasm) target — this is
// used for debug logging at dozens of call sites app-wide, so it's fixed
// once here rather than gating every individual clog!() call for desktop.
macro_rules! clog {
    ($($arg:tt)*) => {
        {
            #[cfg(not(feature = "desktop"))]
            web_sys::console::log_1(&format!($($arg)*).into());
            #[cfg(feature = "desktop")]
            println!($($arg)*);
        }
    };
}

mod types;
mod components;
mod views;
mod layouts;
mod api;
mod theme;
mod ui;


#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {


    #[route("/logout")]
    Logout {},

    #[route("/login")]
    LoginCMP {},

    #[layout(Navbar)]
        #[route("/")]
        Home {},

}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const DX_COMPONENTS_THEME_CSS: Asset = asset!("/assets/dx-components-theme.css");
const FA_JS: Asset = asset!("/assets/ae47c6a44d.js");
// Vendored locally (not a CDN reference) — same convention as FA_JS above.
// Used by the Graph View for force-directed layout (see src/components/graph.rs).
const D3_JS: Asset = asset!("/assets/d3.v7.min.js");

#[derive(Clone, PartialEq)]
pub enum View {
    Entity,
    Inbox,
    Priority,
    Graph,

}

#[derive(Clone, PartialEq)]
pub enum ABView {
   Task,
   History,
   Stats,
   Info,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SortMode {
    Default,
    DueDate,
    Custom,
}

impl SortMode {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            SortMode::Default => "default",
            SortMode::DueDate => "due_date",
            SortMode::Custom => "custom",
        }
    }

    pub fn from_storage_str(s: &str) -> SortMode {
        match s {
            "due_date" => SortMode::DueDate,
            "custom" => SortMode::Custom,
            _ => SortMode::Default,
        }
    }
}

impl fmt::Display for View {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            View::Inbox   => write!(f, "Inbox"),
            View::Entity => write!(f, "Entity"),
            View::Priority => write!(f, "Priority"),
            View::Graph => write!(f, "Graph"),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub moments: Signal<Vec<MomentType>>,
    pub entities: Signal<Vec<EntityType>>,
    pub momentInputTgl: Signal<bool>,
    pub entityModalTgl: Signal<bool>,
    pub sidebarTgl: Signal<bool>,
    pub currentView: Signal<View>,
    pub current_entity: Signal<Option<EntityType>>,
    pub current_moment: Signal<Option<MomentType>>,
    pub activity_bar_tgl: Signal<bool>,
    pub activity_bar_view: Signal<ABView>,
    pub auth_token: Signal<Option<String>>,
    pub user_id: Signal<Option<String>>,
    pub user_email: Signal<Option<String>>,
    pub backdropTgl: Signal<bool>,
    pub tag_filter: Signal<Option<String>>,
    pub sort_mode: Signal<SortMode>,
    // Local-first pivot Phase 1b (see memory reference_local_first_pivot_plan).
    // Defaults to Synced, not Local as the plan's eventual design intends —
    // the Local vault is a stub until Phase 1d/1e build the real flat-file
    // backend, so defaulting to it would empty the app for the only
    // currently-functional backend (Supabase). ActiveStorage::for_vault
    // already falls back to Local when logged out, so this matches today's
    // behavior in both the logged-in and logged-out cases.
    pub active_vault: Signal<VaultKind>,
}

fn main() {
    dotenv::from_path("./docker/.env").ok();
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(|| AppState {
        moments: Signal::new(vec![]),
        momentInputTgl: Signal::new(false),
        entities: Signal::new(vec![]),
        entityModalTgl: Signal::new(false),
        activity_bar_tgl: Signal::new(false),
        activity_bar_view: Signal::new(ABView::Task),
        sidebarTgl: Signal::new(false),
        currentView: Signal::new(View::Inbox),
        current_entity: Signal::new(None::<EntityType>),
        current_moment: Signal::new(None::<MomentType>),
        auth_token: Signal::new(None::<String>),
        user_id: Signal::new(None::<String>),
        user_email: Signal::new(None::<String>),
        backdropTgl: Signal::new(false),
        tag_filter: Signal::new(None::<String>),
        sort_mode: Signal::new(SortMode::Default),
        active_vault: Signal::new(VaultKind::Synced),
    });
    let mut state = use_context::<AppState>();
    use_effect(move || {
        // `#[cfg(feature = "web")]` alone doesn't actually exclude this on a
        // desktop build — `default = ["web"]` in Cargo.toml means the web
        // feature stays active unless a build explicitly disables default
        // features, and `dx build --platform desktop` doesn't do that on its
        // own. web_sys::window() panics at runtime on a native (non-wasm)
        // target ("cannot access imported statics on non-wasm targets"), so
        // this has to key off `desktop` being *absent* specifically, not
        // `web` being present. Desktop has no session/preference persistence
        // yet (a known, deliberately deferred gap, not solved here) — it
        // just starts fresh on defaults every launch.
        #[cfg(not(feature = "desktop"))]
        {
            let storage = web_sys::window().unwrap()
                .local_storage().unwrap().unwrap();

            match storage.get_item("auth_token").unwrap() {
                Some(token) => {
                    state.auth_token.set(Some(token));
                }
                None => {
                    // navigator().push(Route::Login {});
                }
            }

            if let Some(mode) = storage.get_item("sort_mode").unwrap() {
                state.sort_mode.set(SortMode::from_storage_str(&mode));
            }

            if let Some(vault) = storage.get_item("active_vault").unwrap() {
                state.active_vault.set(VaultKind::from_storage_str(&vault));
            }
        }
    });

    // The `rsx!` macro lets us define HTML inside of rust. It expands to an Element with all of our HTML inside.
    rsx! {
        // In addition to element and text (which we will see later), rsx can contain other components. In this case,
        // we are using the `document::Link` component to add a link to our favicon and main CSS file into the head of our app.
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Link { rel: "stylesheet", href: DX_COMPONENTS_THEME_CSS }
        document::Script { src: FA_JS }
        document::Script { src: D3_JS }

        meta {
            name:"viewport",
            content:"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no",
        }
        // The router component renders the route enum we defined above. It will handle synchronization of the URL and render
        // the layouts and components for the active route.
        Router::<Route> {}
    }
}
