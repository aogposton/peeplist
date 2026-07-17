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
    ab_history_cmp,
    ab_stats_cmp,
    ab_info_cmp,
    views_list_cmp,
    entity_list_cmp,
    tag_list_cmp,
    NotesSectionCmp,
};

use crate::api::{
    get_current_user,
    refresh_access_token,
    VaultKind,
};

use crate::types::{EntityType, MomentType, NewMomentType};
use web_sys::window;
use gloo_timers::future::TimeoutFuture;
use lumen_blocks::components::avatar::{Avatar, AvatarFallback};
use lumen_blocks::components::dropdown::{Dropdown, DropdownContent, DropdownItem, DropdownTrigger, DropdownSeparator};

// Refresh the access token this long before it would otherwise expire via
// inactivity/backend expiry, so a live session never silently dies underneath
// the user. Supabase's default JWT lifetime is 1 hour; 50 minutes leaves margin.
const TOKEN_REFRESH_INTERVAL_MS: u32 = 50 * 60 * 1000;


// #[cfg(target_arch = "wasm32")]
// fn window_size() -> (f64, f64) {
//     use web_sys::window;
//     let w = window().unwrap();
//     let width = w.inner_width().unwrap().as_f64().unwrap();
//     let height = w.inner_height().unwrap().as_f64().unwrap();
//     (width, height)
// }
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
                class:"h-1",
            }
            vault_switcher_cmp { }
            div {
                class: "px-3 mt-1 pt-4 border-t border-border",
                span {
                    class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                    "Views"
                }
                views_list_cmp { }
            }
            div {
                class: "px-3 mt-6 pt-4 border-t border-border",
                span {
                    class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                    "Entities"
                }
                entity_list_cmp { }
            }
            div {
                class: "px-3 mt-6 pt-4 border-t border-border",
                span {
                    class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                    "Tags"
                }
                tag_list_cmp { }
            }
        }
    }
}

// One entry in the vault switcher's dropdown. Deliberately list-shaped (a
// Vec built fresh on every render) rather than hardcoded menu items, even
// though today it only ever holds "Local" plus optionally "Synced" — see
// memory reference_local_first_pivot_plan. Extending to a real open-ended
// multi-vault list later is then just "populate this Vec from a stored
// registry instead," not a UI rewrite.
struct VaultEntry {
    kind: VaultKind,
    label: String,
    // Local is the app's one-and-only local vault right now (no multi-vault
    // support yet), so it's never removable. Synced is removable — "remove"
    // just means log out, there's nothing to delete server-side here.
    removable: bool,
}

#[component]
pub fn vault_switcher_cmp() -> Element {
    let state = use_context::<AppState>();
    let mut auth_token = state.auth_token;
    let mut user_id = state.user_id;
    let mut user_email = state.user_email;
    let mut active_vault = state.active_vault;
    let mut confirming_removal_of = use_signal(|| None::<VaultKind>);

    let entries: Vec<VaultEntry> = {
        let mut v = vec![VaultEntry { kind: VaultKind::Local, label: "Local".to_string(), removable: false }];
        if let Some(email) = user_email.read().clone() {
            v.push(VaultEntry { kind: VaultKind::Synced, label: email, removable: true });
        }
        v
    };
    let has_synced = user_email.read().is_some();

    // See VaultKind::effective's doc comment — the raw signal can say
    // "Synced" even on a first-ever, never-logged-in visit, so the switcher
    // has to normalize it the same way the storage layer does, or it'll
    // claim to be on a vault that isn't even in its own list.
    let current = active_vault.read().effective(&auth_token.read());
    let current_label = entries.iter().find(|e| e.kind == current)
        .map(|e| e.label.clone())
        .unwrap_or_else(|| "Local".to_string());
    let initial = current_label.chars().next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_string());

    let mut select_vault = move |kind: VaultKind| {
        active_vault.set(kind);
        // Desktop has no preference persistence yet (see main.rs's startup
        // effect) — web_sys::window() panics on a native target rather than
        // just returning None, so this has to be compiled out entirely
        // rather than relying on the `if let Some(...)` to fail gracefully.
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("active_vault", kind.as_storage_str()).ok();
        }
    };

    // "Removing" the Synced vault is just logging out — there's one account,
    // so there's nothing else to delete. Falls back to Local if Synced was
    // the active vault, so the switcher never points at a vault that no
    // longer exists.
    let mut remove_synced = move || {
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("auth_token", "").ok();
            storage.set("refresh_token", "").ok();
        }
        auth_token.set(None);
        user_id.set(None);
        user_email.set(None);
        if *active_vault.read() == VaultKind::Synced {
            select_vault(VaultKind::Local);
        }
    };

    rsx! {
        div {
            class: "px-3",
            div {
                class: "w-full sidebar-vault-switcher",
                Dropdown {
                    DropdownTrigger {
                        class: "w-full text-left",
                        div {
                            class: "flex items-center gap-2 rounded-md px-2 py-2 hover:bg-muted transition-colors cursor-pointer w-full",
                            Avatar {
                                class: "h-8 w-8 shrink-0",
                                AvatarFallback { class: "text-sm", "{initial}" }
                            }
                            span {
                                class: "text-sm font-medium text-foreground truncate",
                                "{current_label}"
                            }
                        }
                    }
                    DropdownContent {
                        align: "start",
                        for (idx, entry) in entries.iter().enumerate() {
                            {
                                let kind = entry.kind;
                                let is_current = kind == current;
                                let removable = entry.removable;
                                let label = entry.label.clone();
                                rsx! {
                                    DropdownItem::<String> {
                                        value: label.clone(),
                                        index: idx,
                                        on_select: move |_| select_vault(kind),
                                        div {
                                            class: "flex items-center justify-between gap-2 w-full",
                                            span {
                                                class: "truncate",
                                                if is_current { "✓ " } else { "" }
                                                "{label}"
                                            }
                                            if removable {
                                                if *confirming_removal_of.read() == Some(kind) {
                                                    span {
                                                        class: "text-xs font-medium text-destructive hover:underline px-1 shrink-0",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            remove_synced();
                                                            confirming_removal_of.set(None);
                                                        },
                                                        "Remove"
                                                    }
                                                } else {
                                                    span {
                                                        class: "text-muted-foreground hover:text-foreground px-1 shrink-0",
                                                        title: "Vault settings",
                                                        onclick: move |e| {
                                                            e.stop_propagation();
                                                            confirming_removal_of.set(Some(kind));
                                                        },
                                                        "⋯"
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        if !has_synced {
                            DropdownSeparator {}
                            DropdownItem::<String> {
                                value: "add".to_string(),
                                index: entries.len(),
                                on_select: move |_| {
                                    navigator().push(Route::LoginCMP {});
                                },
                                "+ Add a vault"
                            }
                        }
                    }
                }
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
    let mut auth_token = state.auth_token;
    let mut user_id = state.user_id;
    let mut user_email = state.user_email;
    let mut active_vault = state.active_vault;
    let mut refresh_loop_started = use_signal(|| false);
    let moment = current_moment.read().clone();

    // On every load of the main app, confirm the token we have cached in
    // localStorage is still actually accepted by Supabase. A token can die
    // server-side (expiry, revocation) with no client-side signal, which
    // previously left the app showing an empty "logged in" shell. If the
    // token is dead, log the user out for real (clear storage + state) —
    // but land back on the (now-Local) app, not a dead-end login screen.
    // Login is opt-in now (see the vault switcher's "+ Add a vault"), never
    // something a bad cached token can strand you behind.
    use_effect(move || {
        let Some(token) = auth_token.read().clone() else {
            return;
        };

        spawn(async move {
            match get_current_user(token).await {
                Ok(user) => {
                    user_email.set(Some(user.email));
                }
                Err(e) => {
                clog!("Session check failed, logging out: {}", e);
                #[cfg(not(feature = "desktop"))]
                if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                    storage.set("auth_token", &"").ok();
                    storage.set("refresh_token", &"").ok();
                    storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                }
                auth_token.set(None);
                user_id.set(None);
                user_email.set(None);
                active_vault.set(VaultKind::Local);
                }
            }
        });

        // Keep the session alive proactively so it doesn't reach the point
        // of dying in the first place. Started once per mounted session.
        // Desktop has no persisted refresh_token to act on (see main.rs's
        // startup effect) and web_sys::window() panics on a native target,
        // so there's nothing useful for this loop to do there yet.
        #[cfg(not(feature = "desktop"))]
        if !*refresh_loop_started.read() {
            refresh_loop_started.set(true);
            spawn(async move {
                loop {
                    TimeoutFuture::new(TOKEN_REFRESH_INTERVAL_MS).await;
                    let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) else {
                        break;
                    };
                    let Some(refresh_tok) = storage.get_item("refresh_token").ok().flatten().filter(|s| !s.is_empty()) else {
                        break;
                    };
                    match refresh_access_token(refresh_tok).await {
                        Ok(auth) => {
                            storage.set("auth_token", &auth.access_token).ok();
                            storage.set("refresh_token", &auth.refresh_token).ok();
                            auth_token.set(Some(auth.access_token));
                            user_id.set(Some(auth.user.id));
                        }
                        Err(e) => {
                            clog!("Token refresh failed, logging out: {}", e);
                            storage.set("auth_token", &"").ok();
                            storage.set("refresh_token", &"").ok();
                            storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                            auth_token.set(None);
                            user_id.set(None);
                            user_email.set(None);
                            active_vault.set(VaultKind::Local);
                            break;
                        }
                    }
                }
            });
        }
    });
    let activity_bar_class = if *activity_bar_tgl.read() {
        "openedbtw fixed inset-y-0 right-0 z-[60] w-full xl:w-96 transition-transform duration-300 translate-x-0"
    } else {
        "closedbtw fixed inset-y-0 right-0 z-[60] w-full xl:w-96 transition-transform duration-300 translate-x-full"
    };
 
    //
    let header_title = match current_view.read().clone() {
        Inbox => "".to_string(),
        Entity => "".to_string(),
        Priority => "".to_string(),
        Graph => "".to_string(),
        Distance => "".to_string(),
        Due => "".to_string(),
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


        if *backdropTgl.read() || *sidebarTgl.read() || *momentInputTgl.read() {
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
                class: "xl:hidden fixed h-14 w-14 bottom-6 right-6 z-51 rounded-full shadow-lg flex items-center justify-center text-2xl font-semibold text-white transition-transform duration-200 hover:scale-105 active:scale-95",
                style: "background-color:{HL};",
                onclick: move |_| {
                    let current = *momentInputTgl.read();
                    momentInputTgl.set(!current);
                },
                if *momentInputTgl.read() { "✕" } else { "+" }
            }
            div {
                class: if *momentInputTgl.read() {
                    "xl:hidden fixed inset-x-0 bottom-24 z-50 transition-all duration-200 opacity-100 translate-y-0"
                } else {
                    "xl:hidden fixed inset-x-0 bottom-24 z-50 transition-all duration-200 opacity-0 translate-y-4 pointer-events-none"
                },
                MomentInputCmp { }
            }
            Sidebar { }
            div {
                style: "background-color:{BG};",
                class: "flex h-screen w-full overflow-hidden",
                div {
                    class: "hidden xl:block h-full overflow-y-auto w-64 border-r border-border bg-background transform translate-x-0 transition-transform duration-200",
                    div {
                        class:"h-1",
                    }
                    vault_switcher_cmp { }
                    div {
                        class: "px-3 mt-1 pt-4 border-t border-border",
                        span {
                            class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                            "Views"
                        }
                        views_list_cmp { }
                    }
                    div {
                        class: "px-3 mt-6 pt-4 border-t border-border",
                        span {
                            class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                            "Entities"
                        }
                        entity_list_cmp { }
                    }
                    div {
                        class: "px-3 mt-6 pt-4 border-t border-border",
                        span {
                            class: "block px-3 mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground",
                            "Tags"
                        }
                        tag_list_cmp { }
                    }
                }
                div {
                    class: "xl:w-2/3 w-full overflow-y-auto [&::-webkit-scrollbar]:w-1 [&::-webkit-scrollbar-track]:bg-transparent [&::-webkit-scrollbar-thumb]:bg-black/30",
                    "{header_title}"
                    Outlet::<Route> {}
                }
                div {
                    id:"activity-bar",
                    class: "{activity_bar_class} bg-background border-l border-border shadow-2xl",
                    if current_moment.read().is_some()
                        || *activity_bar_view.read() == ABView::History
                        || *activity_bar_view.read() == ABView::Stats
                        || *activity_bar_view.read() == ABView::Info {
                        match activity_bar_view.read().clone() {
                            ABView::Task => rsx! {
                                ab_task_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::History => rsx! {
                                ab_history_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Stats => rsx! {
                                ab_stats_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                            ABView::Info => rsx! {
                                ab_info_cmp {
                                    key: "{*activity_bar_tgl.read()}"
                                }
                            },
                        }
                    }
                }
            }
        }
    }
}
