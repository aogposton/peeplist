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
mod taskwarrior_date;
mod momento;
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
    // Momentos (2026-08-02) — every entity's recurring/personal moments
    // (birthdays, "call every Sunday") in one cross-entity list, sorted by
    // next-upcoming-occurrence. Complements the per-entity Momentos tab
    // (components::entity::ab_momentos_cmp), same relationship the global
    // Scheduled view already has to a moment's own scheduled_at.
    Momentos,
    // A moment given an until_at (taskwarrior-style deadline, set via the
    // `until:` quick-capture keyword) that's passed without ever being
    // completed — see urgency::is_missed. Same relationship to until_at
    // that Scheduled has to scheduled_at: is_missed hides it from every
    // normal view once it's passed, and this is the one place it's still
    // visible, so there's somewhere to go find it. 2026-08-03: until_at
    // existed as editable metadata since 2026-07-something but had zero
    // actual behavior anywhere in the app until this view's addition
    // finally gave it one.
    Missed,
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
            View::Momentos => Some("momentos"),
            View::Missed => Some("missed"),
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
            "momentos" => Some(View::Momentos),
            "missed" => Some(View::Missed),
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
            View::Momentos => Some("Momentos"),
            View::Missed => Some("Missed"),
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
   // Renamed from History (2026-08-02) — same moment timeline, friendlier
   // label. See components::entity::ab_story_cmp (renamed to match).
   Story,
   Stats,
   Info,
   // Momentos (2026-08-02) — an entity's recurring/personal moments
   // (birthdays, "call every Sunday"). See components::entity::ab_momentos_cmp.
   Momentos,
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
            View::Momentos => write!(f, "Momentos"),
            View::Missed => write!(f, "Missed"),
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub moments: Signal<Vec<MomentType>>,
    pub entities: Signal<Vec<EntityType>>,
    pub momentInputTgl: Signal<bool>,
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
    // Mobile-vs-desktop layout switch (2026-08-01), computed in Rust/JS
    // (window.innerWidth/innerHeight via layouts::Navbar's resize listener)
    // instead of a Tailwind CSS breakpoint. A CSS custom-variant version of
    // this (desktop-w:/desktop-h: in tailwind.css) shipped first and looked
    // right in Chrome's device emulation, but failed silently on a real
    // iPhone SE — Tailwind v4's compiled output relies on native CSS
    // nesting (`.foo { @media (...) { ... } }`), which needs Safari 16.4+;
    // an older/real device just never applied the rule at all, leaving the
    // desktop layout stuck on. Plain JS numbers compared in Rust have no
    // such browser-version dependency. True (desktop) is the default until
    // the first real reading arrives.
    pub is_desktop_viewport: Signal<bool>,
    // Local-timezone offset in minutes, JS Date.getTimezoneOffset()
    // convention (positive = local time is BEHIND UTC, e.g. +240 for EDT) —
    // captured once via layouts::Navbar's mount effect. due_at/scheduled_at/
    // until_at are always stored as real UTC (matches what Supabase's
    // timestamptz column actually does with them — see urgency.rs's
    // parse_moment_datetime doc comment), but every place that *displays* or
    // *edits* one of those fields as a raw date/time was showing/writing the
    // bare UTC digits with no conversion at all — invisible for date-only
    // due dates most of the day, but glaringly wrong (~4 hours, for EDT) the
    // moment a precise time is involved, e.g. quick-capture's `due:2min`
    // (2026-08-01 bug report). Defaults to 0 (UTC) until the first real
    // reading arrives, same posture as is_desktop_viewport above.
    pub local_utc_offset_minutes: Signal<i32>,
    // Auto-folds the docked desktop sidebar (layouts::Navbar) once the
    // window narrows past a comfortable width, distinct from
    // is_desktop_viewport's own mobile-vs-desktop split above — a narrow-
    // but-tall window can still count as "desktop" there (it's an OR on
    // width/height) while still being too cramped for a fixed 256px
    // sidebar. Purely responsive, no manual override: widen the window and
    // it un-folds on its own. Computed alongside is_desktop_viewport in the
    // same VIEWPORT_SCRIPT resize listener.
    pub sidebar_collapsed: Signal<bool>,
    // Whether this device has a touch pointer at all (JS `'ontouchstart' in
    // window || navigator.maxTouchPoints > 0`), captured once via
    // layouts::Navbar's mount effect — distinct from is_desktop_viewport
    // above, which only measures window dimensions. An iPad's viewport is
    // wide enough to land in the "desktop" bucket there (and should: the
    // docked sidebar is genuinely the right call on it), but it's still a
    // touch device, and a couple of things key specifically off "can the
    // user actually hover/right-click, or only tap" rather than window
    // size: the floating quick-add button (a wide *touch* viewport still
    // needs it — there's no hover state to reveal an alternative), and the
    // vault switcher's popup Dropdown (a touch tap on an item inside it
    // closes the menu before the tap registers as a selection — the same
    // library-level touch/pointer bug already routed around for phones,
    // which this device-width check alone never caught for iPad). Defaults
    // to false (assume mouse/desktop) until the first real reading arrives.
    pub is_touch_device: Signal<bool>,
    // Whether the browser has handed us a captured `beforeinstallprompt`
    // event we can still trigger (Settings' install button). Chrome/Edge/
    // Android only — Safari (iOS and macOS) never fires this event at all,
    // so Settings falls back to static "Share > Add to Home Screen"
    // instructions there instead of a button. See layouts::Navbar's mount
    // effect for where this actually gets set.
    pub pwa_install_available: Signal<bool>,
    // Whether the app is currently running as an installed/standalone PWA
    // rather than a normal browser tab — hides the whole "Install" section
    // in Settings when there's nothing left to install. Checked once via
    // `window.matchMedia('(display-mode: standalone)')` (Chrome/Edge) or
    // `navigator.standalone` (the non-standard Safari equivalent).
    pub pwa_standalone: Signal<bool>,
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
// PWA service-worker registration (2026-08-02) — see assets/sw.js's own doc
// comment for the caching strategy. `navigator.serviceWorker` doesn't exist
// on a native webview (desktop), hence the guard here rather than a
// `#[cfg(not(feature = "desktop"))]` — a script eval'd on desktop would
// otherwise throw on `navigator.serviceWorker` being undefined. One-shot,
// fire-and-forget: nothing in Rust needs to know registration happened.
const SW_REGISTER_SCRIPT: &str = r#"
    if ('serviceWorker' in navigator) {
        navigator.serviceWorker.register('/sw.js');
    }
"#;

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
        is_desktop_viewport: Signal::new(true),
        local_utc_offset_minutes: Signal::new(0),
        sidebar_collapsed: Signal::new(false),
        is_touch_device: Signal::new(false),
        pwa_install_available: Signal::new(false),
        pwa_standalone: Signal::new(false),
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
                // Filter out an empty string, not just a missing key —
                // several logout paths write "" to this key rather than
                // removing it outright. Without this filter, a restored
                // Some("") reads as "logged in" to every auth_token.is_some()
                // check in the app (the vault switcher, Login's already-
                // logged-in redirect guard, has_synced), permanently
                // stranding whoever hits it: Login always bounces them back
                // to Home since a session "exists," but there's no real
                // token behind it to actually do anything with.
                if let Ok(Some(token)) = storage.get_item("auth_token") {
                    if !token.is_empty() {
                        state.auth_token.set(Some(token));
                    }
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

    let mut sw_registered = use_signal(|| false);
    use_effect(move || {
        if *sw_registered.read() {
            return;
        }
        sw_registered.set(true);
        spawn(async move {
            document::eval(SW_REGISTER_SCRIPT);
        });
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

        // PWA (2026-08-02) — manifest.json/icon-*.png are plain files copied
        // verbatim by scripts/deploy.sh, not asset!()-wrapped (see that
        // script's own comment for why), so these are literal absolute
        // paths rather than Asset consts like FAVICON above.
        document::Link { rel: "manifest", href: "/manifest.json" }
        document::Link { rel: "apple-touch-icon", href: "/apple-touch-icon.png" }
        // White, not the app's red highlight color — on a PWA this paints
        // the mobile browser topbar, and red there reads like a screen-
        // recording indicator rather than an app color choice.
        meta { name: "theme-color", content: "#ffffff" }

        meta {
            name:"viewport",
            content:"width=device-width, initial-scale=1.0, maximum-scale=1.0, user-scalable=no",
        }
        // The router component renders the route enum we defined above. It will handle synchronization of the URL and render
        // the layouts and components for the active route.
        Router::<Route> {}
    }
}
