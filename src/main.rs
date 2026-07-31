// The dioxus prelude contains a ton of common items used in dioxus apps. It's a good idea to import wherever you
// need dioxus
//
use std::fmt;
use dioxus::prelude::*;
use crate::types::*;
use crate::api::VaultKind;
use views::{Logout, LoginCMP, ResetPasswordCmp, Home};
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
mod quick_capture;
mod urgency;

pub use urgency::UrgencyWeights;


#[derive(Debug, Clone, Routable, PartialEq)]
#[rustfmt::skip]
enum Route {


    #[route("/logout")]
    Logout {},

    #[route("/login")]
    LoginCMP {},

    #[route("/reset-password")]
    ResetPasswordCmp {},

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

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum View {
    Entity,
    Inbox,
    Priority,
    // Replaces the old separate Graph/Distance sidebar entries (2026-07-23)
    // — an entity with no currently-active moments is now auto-hidden from
    // the sidebar's own Entities list (see sidebar.rs's entity_list_cmp and
    // AppState::autohide_entities), so "just look at the graph" or "just
    // look at distances" needed one place that always shows literally
    // everyone regardless of that filter. AllEntitiesViewCmp (entity.rs)
    // renders both, toggled locally within the page.
    AllEntities,
    Due,
    Scheduled,
    Blocking,
    Notes,
    Settings,
    RecentlyDeleted,
    // Reuses the same rendering as View::Entity (see views/home.rs's
    // combined `Entity | SelfEntity` match arm) — the only difference is
    // this one's a real sidebar View (hideable, listed in VIEW_ENTRIES)
    // rather than something only reachable by clicking an entity link.
    // Previously the self entity was deliberately excluded from the
    // Entities list entirely; user reversed that 2026-07-22 by asking for
    // this instead.
    SelfEntity,
}

impl View {
    // Only the views that ever appear in the sidebar's "Views" list are
    // hideable/storable here — Entity and Settings aren't in that list
    // (Entity is reached by clicking an entity, Settings via its own
    // link), so they're intentionally absent from both directions.
    pub fn as_storage_str(&self) -> Option<&'static str> {
        match self {
            View::Inbox => Some("inbox"),
            View::Priority => Some("priority"),
            View::Due => Some("due"),
            View::Scheduled => Some("scheduled"),
            View::Blocking => Some("blocking"),
            View::Notes => Some("notes"),
            View::AllEntities => Some("all_entities"),
            View::RecentlyDeleted => Some("recently_deleted"),
            View::SelfEntity => Some("self_entity"),
            View::Entity | View::Settings => None,
        }
    }

    pub fn from_storage_str(s: &str) -> Option<View> {
        match s {
            "inbox" => Some(View::Inbox),
            "priority" => Some(View::Priority),
            "due" => Some(View::Due),
            "scheduled" => Some(View::Scheduled),
            "blocking" => Some(View::Blocking),
            "notes" => Some(View::Notes),
            "all_entities" => Some(View::AllEntities),
            "recently_deleted" => Some(View::RecentlyDeleted),
            "self_entity" => Some(View::SelfEntity),
            _ => None,
        }
    }

    // The sidebar's actual visible label per view — kept separate from
    // Display above (which gives generic names like "Inbox"/"Graph") since
    // the sidebar's real copy diverges ("All", "Graph View"). Used by
    // Settings' hidden-views list so restoring one shows the same label
    // you'd recognize from the sidebar.
    pub fn sidebar_label(&self) -> Option<&'static str> {
        match self {
            View::Inbox => Some("All"),
            View::Priority => Some("Expedite"),
            View::Due => Some("Due"),
            View::Scheduled => Some("Scheduled"),
            View::Blocking => Some("Blocking"),
            View::Notes => Some("Notes"),
            View::AllEntities => Some("All Entities"),
            View::RecentlyDeleted => Some("Recently Deleted"),
            View::SelfEntity => Some("Self"),
            View::Entity | View::Settings => None,
        }
    }
}

// Shared by the sidebar's "Hide" action and Settings' "Show" (restore)
// action — both mutate `hidden_views` and need to persist the result the
// same way, so this is one place instead of two copies of the same
// localStorage-join logic.
pub fn persist_hidden_views(views: &[View]) {
    #[cfg(not(feature = "desktop"))]
    if let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) {
        let joined = views.iter()
            .filter_map(|v| v.as_storage_str())
            .collect::<Vec<_>>()
            .join(",");
        storage.set("hidden_views", &joined).ok();
    }
    #[cfg(feature = "desktop")]
    let _ = views;
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
    ByEntity,
}

impl SortMode {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            SortMode::Default => "default",
            SortMode::DueDate => "due_date",
            SortMode::Custom => "custom",
            SortMode::ByEntity => "by_entity",
        }
    }

    pub fn from_storage_str(s: &str) -> SortMode {
        match s {
            "due_date" => SortMode::DueDate,
            "custom" => SortMode::Custom,
            "by_entity" => SortMode::ByEntity,
            _ => SortMode::Default,
        }
    }
}

// Moment list "view options" density (2026-07-28) — Compact is today's
// existing row (title + inline badges, no description/tags shown) and stays
// the default so nobody's view changes shape on its own; Full adds a second
// line per row (description preview + priority/project/tag pills) for
// anyone who wants more context without opening each moment.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ListDensity {
    Compact,
    Full,
}

impl ListDensity {
    pub fn as_storage_str(&self) -> &'static str {
        match self {
            ListDensity::Compact => "compact",
            ListDensity::Full => "full",
        }
    }

    pub fn from_storage_str(s: &str) -> ListDensity {
        match s {
            "full" => ListDensity::Full,
            _ => ListDensity::Compact,
        }
    }
}

impl fmt::Display for View {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            View::Inbox   => write!(f, "Inbox"),
            View::Entity => write!(f, "Entity"),
            View::Priority => write!(f, "Expedite"),
            View::AllEntities => write!(f, "All Entities"),
            View::Due => write!(f, "Due"),
            View::Scheduled => write!(f, "Scheduled"),
            View::Blocking => write!(f, "Blocking"),
            View::Notes => write!(f, "Notes"),
            View::Settings => write!(f, "Settings"),
            View::RecentlyDeleted => write!(f, "Recently Deleted"),
            View::SelfEntity => write!(f, "Self"),
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
    pub project_filter: Signal<Option<String>>,
    // Not persisted (same as tag_filter above) — resets on reload, unlike
    // sort_mode/active_vault/urgency_weights below.
    pub hide_notes: Signal<bool>,
    pub hide_completed: Signal<bool>,
    pub sort_mode: Signal<SortMode>,
    // Clicking the already-active sort mode's own button again flips this,
    // rather than a separate control — same "click a table header twice"
    // convention. Applies uniformly to whichever mode is active (Default,
    // Due date, Custom, By entity all have a well-defined reverse), so this
    // is one flag, not per-mode state.
    pub sort_descending: Signal<bool>,
    // Moment list "view options" density (Compact/Full) — see ListDensity
    // above. Persisted the same way as sort_mode/sort_descending.
    pub list_density: Signal<ListDensity>,
    // Full-screen title+description editor toggle (2026-07-28, desktop
    // only). Has to live here, not as a local signal inside ab_task_cmp —
    // that panel is nested inside the activity bar's own `translate-x-0`
    // sliding-drawer container (see layouts/navbar.rs), and any
    // position:fixed descendant of a transformed ancestor gets contained
    // to that ancestor's box instead of the real viewport (same class of
    // bug already fixed once for the desktop sidebar). The modal itself
    // has to be rendered as a sibling outside that transform, same
    // approach as EntityModalCmp — which means whatever toggles it needs
    // to be visible from both places.
    pub full_editor_open: Signal<bool>,
    // "On the fly" (2026-07-28) — Waffle House-order-completed-outside-the-
    // flow inspired: jot a task, a full-screen takeover says "go do this
    // now," go actually do it, come back and mark it done, never letting it
    // sit buried in the normal list waiting to be rediscovered. The task is
    // created for real the moment it's captured (not just held in memory)
    // so getting pulled away mid-flow never loses it — see OnTheFlyCmp.
    // on_the_fly_open gates the whole overlay; on_the_fly_task is None
    // while still typing (capture stage) and Some once created (the "go do
    // it" stage).
    pub on_the_fly_open: Signal<bool>,
    pub on_the_fly_task: Signal<Option<MomentType>>,
    // Local-first pivot Phase 1b (see memory reference_local_first_pivot_plan).
    // Defaults to Synced, not Local as the plan's eventual design intends —
    // the Local vault is a stub until Phase 1d/1e build the real flat-file
    // backend, so defaulting to it would empty the app for the only
    // currently-functional backend (Supabase). ActiveStorage::for_vault
    // already falls back to Local when logged out, so this matches today's
    // behavior in both the logged-in and logged-out cases.
    pub active_vault: Signal<VaultKind>,
    // See src/urgency.rs — the weights driving the Priority view's ranking,
    // user-editable via UrgencySettingsCmp. Persisted the same way as
    // sort_mode below (localStorage on web; no persistence yet on desktop).
    pub urgency_weights: Signal<UrgencyWeights>,
    // Sidebar "Views" the user has hidden via the 3-dot menu next to each
    // one — restorable from the Settings page. Persisted the same way as
    // sort_mode/active_vault/urgency_weights above (localStorage on web;
    // no persistence yet on desktop, same known gap as everything else
    // in this list).
    pub hidden_views: Signal<Vec<View>>,
    // Sidebar's own Entities list hides anyone with nothing currently
    // active (see sidebar.rs's entity_list_cmp) — this is the Settings
    // toggle to turn that back off. Defaults on: "out of your face once
    // there's nothing to do for them" is the actual desired behavior, not
    // an opt-in extra (2026-07-23, replaces a separate "archive" concept
    // the user considered and dropped in favor of this).
    pub autohide_entities: Signal<bool>,
    // True until the first moments+entities fetch (see views/home.rs's
    // effect) resolves, or resets to true whenever it re-fetches (vault
    // switch, etc). Everything that reads `moments`/`entities` — Home's own
    // view match, Graph/Stats/History/Info panels — starts as an empty Vec,
    // which used to render as "you have nothing" for a flash before real
    // data arrived. This is the single shared flag they all key a loading
    // skeleton off, instead of each view doing its own separate fetch/flag.
    pub data_loading: Signal<bool>,
    // Bumped by the global "n" keyboard shortcut (see layouts::Navbar) to ask
    // whichever MomentInputCmp instance is actually visible (mobile popup vs
    // desktop composer — exactly one is display:none at any given viewport
    // width) to focus its title field. A counter, not a bool, so two
    // presses in a row without an intervening render still register as two
    // distinct requests.
    pub focus_composer: Signal<u32>,
}

fn main() {
    dotenv::from_path("./docker/.env").ok();
    // Turns a wasm panic's default cryptic "unreachable executed" trap into
    // a real message + stack trace in the browser devtools console. A
    // no-op on desktop (native panics already print a normal message to
    // stderr on their own), so this isn't cfg-gated — just harmless there.
    console_error_panic_hook::set_once();
    dioxus::launch(App);
}

// Minimal crash/error visibility (2026-07-29, see scripts/2026-07-29-
// error-reports.sql and api::report_error) — window.onerror and
// unhandledrejection are the two JS-level events that "something broke and
// nobody but this one browser tab knows" actually flows through, so a
// single global listener here catches both regardless of which view is
// currently mounted. Same document::eval-persistent-listener pattern
// layouts::Navbar's global keyboard shortcuts already use — this works
// identically on desktop (a real webview, real JS) as on web, so it isn't
// cfg-gated either.
const GLOBAL_ERROR_SCRIPT: &str = r#"
    window.addEventListener('error', (e) => {
        dioxus.send({
            message: e.message || 'window.onerror with no message',
            context: e.filename ? (e.filename + ':' + e.lineno + ':' + e.colno) : null,
        });
    });
    window.addEventListener('unhandledrejection', (e) => {
        dioxus.send({
            message: 'Unhandled promise rejection: ' + (e.reason ? String(e.reason) : 'unknown reason'),
            context: null,
        });
    });
"#;

#[derive(serde::Deserialize, Clone)]
struct GlobalErrorEvent {
    message: String,
    context: Option<String>,
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
        project_filter: Signal::new(None::<String>),
        hide_notes: Signal::new(false),
        hide_completed: Signal::new(false),
        sort_mode: Signal::new(SortMode::Default),
        sort_descending: Signal::new(false),
        list_density: Signal::new(ListDensity::Compact),
        full_editor_open: Signal::new(false),
        on_the_fly_open: Signal::new(false),
        on_the_fly_task: Signal::new(None),
        active_vault: Signal::new(VaultKind::Synced),
        urgency_weights: Signal::new(UrgencyWeights::default()),
        hidden_views: Signal::new(vec![]),
        autohide_entities: Signal::new(true),
        data_loading: Signal::new(true),
        focus_composer: Signal::new(0),
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
            // localStorage can legitimately be unavailable (locked-down
            // managed browsers, some privacy modes, storage-partitioned
            // iframe embeds) — that used to be an unconditional unwrap
            // chain here, which panicked the whole app before it ever
            // rendered. The local-first pitch is "just works, no
            // friction" — a hard crash on storage access is the one
            // thing that can't happen, so this now degrades to
            // in-memory defaults (same as the desktop build already
            // does, per the comment above) instead of panicking.
            let storage = web_sys::window()
                .and_then(|w| w.local_storage().ok().flatten());

            if let Some(storage) = storage {
                if let Ok(Some(token)) = storage.get_item("auth_token") {
                    state.auth_token.set(Some(token));
                }

                if let Ok(Some(mode)) = storage.get_item("sort_mode") {
                    state.sort_mode.set(SortMode::from_storage_str(&mode));
                }

                if let Ok(Some(desc)) = storage.get_item("sort_descending") {
                    state.sort_descending.set(desc == "true");
                }

                if let Ok(Some(density)) = storage.get_item("list_density") {
                    state.list_density.set(ListDensity::from_storage_str(&density));
                }

                if let Ok(Some(vault)) = storage.get_item("active_vault") {
                    state.active_vault.set(VaultKind::from_storage_str(&vault));
                }

                if let Ok(Some(weights)) = storage.get_item("urgency_weights") {
                    state.urgency_weights.set(UrgencyWeights::from_storage_string(&weights));
                }

                if let Ok(Some(hidden)) = storage.get_item("hidden_views") {
                    let views: Vec<View> = hidden.split(',')
                        .filter_map(View::from_storage_str)
                        .collect();
                    state.hidden_views.set(views);
                }

                if let Ok(Some(autohide)) = storage.get_item("autohide_entities") {
                    state.autohide_entities.set(autohide != "false");
                }
            } else {
                clog!("localStorage unavailable — starting with in-memory defaults");
            }
        }
    });

    let mut error_listener_started = use_signal(|| false);
    use_effect(move || {
        if *error_listener_started.read() {
            return;
        }
        error_listener_started.set(true);
        spawn(async move {
            let mut eval = document::eval(GLOBAL_ERROR_SCRIPT);
            while let Ok(event) = eval.recv::<GlobalErrorEvent>().await {
                let token = state.auth_token.read().clone();
                #[cfg(not(feature = "desktop"))]
                let user_agent = web_sys::window()
                    .and_then(|w| w.navigator().user_agent().ok());
                #[cfg(feature = "desktop")]
                let user_agent = None;
                spawn(async move {
                    api::report_error(token, event.message, event.context, user_agent).await;
                });
            }
        });
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
