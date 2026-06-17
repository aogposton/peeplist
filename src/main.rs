// The dioxus prelude contains a ton of common items used in dioxus apps. It's a good idea to import wherever you
// need dioxus
//
use std::fmt;
use dioxus::prelude::*;
use crate::types::*;
use views::{Logout, Login, Home};
use layouts::{Navbar};


macro_rules! clog {
    ($($arg:tt)*) => {
        web_sys::console::log_1(&format!($($arg)*).into())
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
    Login {},

    #[layout(Navbar)]
        #[route("/")]
        Home {},

}

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const FA_JS: Asset = asset!("/assets/ae47c6a44d.js");

#[derive(Clone, PartialEq)]
pub enum View {
    Entity,
    Inbox,
    SELF

}

#[derive(Clone, PartialEq)]
pub enum ABView {
   Task,
}

impl fmt::Display for View {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            View::Inbox   => write!(f, "Inbox"),
            View::Entity => write!(f, "Entity"),
            View::SELF => write!(f, "Self"),
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
    pub backdropTgl: Signal<bool>,
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
        backdropTgl: Signal::new(false),
    });
    let mut state = use_context::<AppState>();
    use_effect(move || {
        #[cfg(feature = "web")]
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
        }
    });

    // The `rsx!` macro lets us define HTML inside of rust. It expands to an Element with all of our HTML inside.
    rsx! {
        // In addition to element and text (which we will see later), rsx can contain other components. In this case,
        // we are using the `document::Link` component to add a link to our favicon and main CSS file into the head of our app.
        document::Link { rel: "icon", href: FAVICON }
        document::Link { rel: "stylesheet", href: MAIN_CSS }
        document::Link { rel: "stylesheet", href: TAILWIND_CSS }
        document::Script { src: FA_JS }

        meta {
            name:"viewport",
            content:"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no",
        }
        // The router component renders the route enum we defined above. It will handle synchronization of the URL and render
        // the layouts and components for the active route.
        Router::<Route> {}
    }
}
