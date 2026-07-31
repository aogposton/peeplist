use dioxus::prelude::*;
use crate::AppState;
use crate::Route;
use crate::api::{update_password, ActiveStorage, VaultKind};
use web_sys::window;

// Settings page — account/vault-level controls, not data views. See memory
// project_backlog_review_2026_07_21 / project_ui_backlog_2026_07_21. Data
// export was built here 2026-07-22 then explicitly removed the same day —
// the actual ask was vault management (remove/change-password), not
// export. "Hide/show vaults" from that same ask turned out to be a mix-up
// for the separate hide/show-VIEWS feature, not built here. "Recently
// deleted" also lived here briefly (2026-07-22) before being moved out to
// its own sidebar View (RecentlyDeletedViewCmp, components/moment.rs) —
// it's a data view like Due/Scheduled, not an account/vault setting.
//
// The password/remove/delete-data sections below only apply to the Synced
// vault — Local has no Supabase account behind it, so there's no password
// to change and no server-held data to delete (full on-device Local vault
// wipe is a separate, not-yet-built feature).
#[component]
pub fn SettingsCmp() -> Element {
    let state = use_context::<AppState>();
    let mut auth_token = state.auth_token;
    let mut user_id = state.user_id;
    let mut user_email = state.user_email;
    let mut active_vault = state.active_vault;
    let mut hidden_views = state.hidden_views;
    let mut sidebarTgl = state.sidebarTgl;
    let mut backdropTgl = state.backdropTgl;
    let mut autohide_entities = state.autohide_entities;
    let mut moments = state.moments;
    let mut entities = state.entities;

    let has_synced = user_email.read().is_some();

    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut password_error = use_signal(|| None::<String>);
    let mut password_success = use_signal(|| false);
    let mut password_busy = use_signal(|| false);

    let mut change_password = move |_| {
        password_error.set(None);
        password_success.set(false);
        let pw = new_password.read().clone();
        if pw.len() < 6 {
            password_error.set(Some("Password needs to be at least 6 characters.".to_string()));
            return;
        }
        if pw != *confirm_password.read() {
            password_error.set(Some("Passwords don't match.".to_string()));
            return;
        }
        let Some(token) = auth_token.read().clone() else {
            password_error.set(Some("You need to be logged in to the Synced vault to change its password.".to_string()));
            return;
        };
        password_busy.set(true);
        spawn(async move {
            match update_password(token, pw).await {
                Ok(()) => {
                    password_success.set(true);
                    new_password.set(String::new());
                    confirm_password.set(String::new());
                }
                Err(e) => {
                    clog!("Error updating password: {}", e);
                    password_error.set(Some("Couldn't change the password — double-check you're still logged in and try again.".to_string()));
                }
            }
            password_busy.set(false);
        });
    };

    let mut confirming_remove = use_signal(|| false);
    let remove_synced_vault = move |_| {
        #[cfg(not(feature = "desktop"))]
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("auth_token", "").ok();
            storage.set("refresh_token", "").ok();
        }
        auth_token.set(None);
        user_id.set(None);
        user_email.set(None);
        if *active_vault.read() == VaultKind::Synced {
            active_vault.set(VaultKind::Local);
            #[cfg(not(feature = "desktop"))]
            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
            }
        }
        confirming_remove.set(false);
    };

    // "Delete my account and data" (2026-07-29) — see SupabaseStorage::
    // delete_all_data's own doc comment for exactly what this does and
    // doesn't do (clears data via the existing owner-scoped RLS policies;
    // never touches the actual login/auth.users row, which needs a
    // privileged key this client can't hold).
    let mut confirming_delete_data = use_signal(|| false);
    let mut deleting_data = use_signal(|| false);
    let mut delete_data_error = use_signal(|| None::<String>);
    let delete_my_data = move |_| {
        if *deleting_data.read() {
            return;
        }
        let Some(token) = auth_token.read().clone() else {
            delete_data_error.set(Some("You need to be logged in to the Synced vault to delete its data.".to_string()));
            return;
        };
        deleting_data.set(true);
        delete_data_error.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(VaultKind::Synced, Some(token));
            let result = match &storage {
                ActiveStorage::Supabase(s) => s.delete_all_data().await,
                ActiveStorage::Local(_) => Ok(()), // can't happen — Synced requested with a token present
            };
            match result {
                Ok(()) => {
                    moments.set(vec![]);
                    entities.set(vec![]);
                    confirming_delete_data.set(false);
                    // Same "land back on Local, not an empty Synced view"
                    // posture as Remove Synced vault above.
                    #[cfg(not(feature = "desktop"))]
                    if let Some(s) = window().and_then(|w| w.local_storage().ok().flatten()) {
                        s.set("auth_token", "").ok();
                        s.set("refresh_token", "").ok();
                        s.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                    }
                    auth_token.set(None);
                    user_id.set(None);
                    user_email.set(None);
                    active_vault.set(VaultKind::Local);
                }
                Err(e) => {
                    clog!("Error deleting synced data: {}", e);
                    delete_data_error.set(Some("Couldn't delete everything — some data may remain. Try again.".to_string()));
                }
            }
            deleting_data.set(false);
        });
    };

    rsx! {
        div {
            class: "px-4 pt-4",
            h1 { class: "text-2xl font-semibold text-foreground mb-1", "Settings" }
            p {
                class: "text-sm text-muted-foreground mb-4",
                "Account and vault controls."
            }
            // Beta pricing transparency (2026-07-29) — same notice shown at
            // signup time (see views/auth.rs), repeated here since this is
            // also where an existing Synced user would come looking for
            // "wait, is this free" clarity, not just someone about to sign up.
            p {
                class: "text-xs text-muted-foreground rounded-md border border-border bg-muted/30 px-3 py-2 mb-4",
                "Synced is free during beta. When we introduce pricing, anyone already using it will get advance notice before anything changes."
            }
        }
        div {
            class: "mx-4 mb-3 flex flex-col gap-4",
            div {
                class: "rounded-lg border border-border bg-background p-4 flex items-center justify-between gap-3",
                div {
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Auto-hide inactive entities & projects" }
                    p {
                        class: "text-sm text-muted-foreground",
                        "Entities and projects with nothing currently active (open tasks/promises — notes don't count) drop out of the sidebar's lists. Entities are still in Graph View and All Entities — this only affects the sidebar."
                    }
                }
                label {
                    class: "relative inline-flex items-center cursor-pointer shrink-0",
                    input {
                        r#type: "checkbox",
                        class: "sr-only peer",
                        checked: *autohide_entities.read(),
                        onchange: move |e| {
                            let checked = e.checked();
                            autohide_entities.set(checked);
                            #[cfg(not(feature = "desktop"))]
                            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                                storage.set("autohide_entities", if checked { "true" } else { "false" }).ok();
                            }
                        }
                    }
                    div {
                        class: "w-10 h-6 bg-muted rounded-full peer peer-checked:bg-primary transition-colors relative after:content-[''] after:absolute after:top-0.5 after:left-0.5 after:bg-background after:rounded-full after:h-5 after:w-5 after:transition-transform peer-checked:after:translate-x-4",
                    }
                }
            }
            if !hidden_views.read().is_empty() {
                div {
                    class: "rounded-lg border border-border bg-background p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Hidden views" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Hidden from the sidebar via its 3-dot menu. Bring one back:"
                    }
                    div {
                        class: "flex flex-col divide-y divide-border rounded-md border border-border overflow-hidden",
                        for view in hidden_views.read().iter().copied() {
                            div {
                                key: "{view.sidebar_label().unwrap_or(\"\")}",
                                class: "flex items-center justify-between gap-3 px-3 py-2",
                                span { class: "text-sm text-foreground", "{view.sidebar_label().unwrap_or_default()}" }
                                button {
                                    class: "text-sm text-primary hover:underline cursor-pointer",
                                    onclick: move |_| {
                                        let mut updated = hidden_views.read().clone();
                                        updated.retain(|v| *v != view);
                                        crate::persist_hidden_views(&updated);
                                        hidden_views.set(updated);
                                    },
                                    "Show"
                                }
                            }
                        }
                    }
                }
            }
            if !has_synced {
                div {
                    class: "rounded-lg border border-border bg-background text-sm text-muted-foreground text-center py-8 flex flex-col items-center gap-3",
                    span { "No Synced vault connected — these settings apply once you've added one." }
                    button {
                        class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-4 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer",
                        onclick: move |_| {
                            // Same close-drawer-first fix as the vault switcher's
                            // own "+ Add a vault" — see navbar.rs's comment on why
                            // (mobile sidebar backdrop otherwise blocks the routed
                            // login page).
                            sidebarTgl.set(false);
                            backdropTgl.set(false);
                            navigator().push(Route::LoginCMP {});
                        },
                        "+ Add a vault"
                    }
                }
            } else {
                div {
                    class: "rounded-lg border border-border bg-background p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Change password" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Updates the password on your Synced vault's account."
                    }
                    div {
                        class: "flex flex-col gap-y-3 max-w-sm",
                        div {
                            class: "flex flex-col gap-y-1.5",
                            label { class: "block text-xs font-medium text-foreground", "New password" }
                            input {
                                r#type: "password",
                                class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                value: "{new_password.read()}",
                                oninput: move |e| new_password.set(e.value()),
                            }
                        }
                        div {
                            class: "flex flex-col gap-y-1.5",
                            label { class: "block text-xs font-medium text-foreground", "Confirm new password" }
                            input {
                                r#type: "password",
                                class: "w-full rounded-md border border-input bg-background text-sm text-foreground px-3 py-2 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                                value: "{confirm_password.read()}",
                                oninput: move |e| confirm_password.set(e.value()),
                            }
                        }
                        if let Some(msg) = password_error.read().as_ref() {
                            p { class: "text-sm text-destructive", "{msg}" }
                        }
                        if *password_success.read() {
                            p { class: "text-sm text-foreground", "Password changed." }
                        }
                        button {
                            class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-4 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed self-start",
                            disabled: *password_busy.read(),
                            onclick: change_password,
                            if *password_busy.read() { "Changing…" } else { "Change password" }
                        }
                    }
                }
                div {
                    class: "rounded-lg border border-destructive/30 bg-background p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Remove Synced vault" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Logs this device out of your account. Your data stays in the account — this doesn't delete anything, it just disconnects sync here. You'll land back on the Local vault."
                    }
                    if *confirming_remove.read() {
                        div {
                            class: "flex items-center gap-2",
                            span { class: "text-sm text-foreground", "Remove the Synced vault from this device?" }
                            button {
                                class: "rounded-md border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer",
                                onclick: remove_synced_vault,
                                "Confirm"
                            }
                            button {
                                class: "rounded-md border border-border bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                onclick: move |_| confirming_remove.set(false),
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "rounded-md border border-destructive/50 bg-background text-destructive text-sm px-4 py-1.5 font-medium hover:bg-destructive/10 transition-colors cursor-pointer",
                            onclick: move |_| confirming_remove.set(true),
                            "Remove Synced vault"
                        }
                    }
                }
                div {
                    class: "rounded-lg border border-destructive/30 bg-background p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Delete my account and data" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Permanently deletes everyone and everything in your Synced vault — entities, moments, reactions, all of it. This can't be undone. Your login itself stays active (contact us if you want that gone too); you'll land back on the Local vault, which is untouched."
                    }
                    if let Some(msg) = delete_data_error.read().as_ref() {
                        p { class: "text-sm text-destructive mb-2", "{msg}" }
                    }
                    if *confirming_delete_data.read() {
                        div {
                            class: "flex items-center gap-2",
                            span { class: "text-sm text-foreground", "Permanently delete all Synced data?" }
                            button {
                                class: "rounded-md border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *deleting_data.read(),
                                onclick: delete_my_data,
                                if *deleting_data.read() { "Deleting…" } else { "Confirm" }
                            }
                            button {
                                class: "rounded-md border border-border bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                disabled: *deleting_data.read(),
                                onclick: move |_| confirming_delete_data.set(false),
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "rounded-md border border-destructive/50 bg-background text-destructive text-sm px-4 py-1.5 font-medium hover:bg-destructive/10 transition-colors cursor-pointer",
                            onclick: move |_| confirming_delete_data.set(true),
                            "Delete my account and data"
                        }
                    }
                }
                div {
                    class: "flex items-center gap-3 text-xs text-muted-foreground px-1",
                    // Canonical copy lives on the marketing site, not as an
                    // in-app route — see views/auth.rs's matching links.
                    a {
                        class: "hover:text-foreground cursor-pointer",
                        href: "https://blackserverbook.com/privacy",
                        target: "_blank",
                        "Privacy"
                    }
                    span { "·" }
                    a {
                        class: "hover:text-foreground cursor-pointer",
                        href: "https://blackserverbook.com/terms",
                        target: "_blank",
                        "Terms"
                    }
                }
            }
        }
    }
}
