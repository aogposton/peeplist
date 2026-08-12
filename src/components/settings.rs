use dioxus::prelude::*;
use crate::AppState;
use crate::Route;
use crate::api::{update_password, ActiveStorage, VaultKind};
use web_sys::window;

// Triggers a real browser download of a string as a file — Blob + a
// throwaway <a download> click, since there's no plain-Rust/web-sys-free
// way to do this and Dioxus has no built-in for it. Takes {filename,
// content} as one JS-side object (rather than two separate dioxus.recv()
// values) since eval.send() here is a single one-shot call, not the
// repeated-token loop pattern views/auth.rs's Turnstile script uses.
#[cfg(not(feature = "desktop"))]
const DOWNLOAD_FILE_SCRIPT: &str = r#"
    const { filename, content } = await dioxus.recv();
    const blob = new Blob([content], { type: "application/yaml" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
"#;

#[cfg(not(feature = "desktop"))]
#[derive(serde::Serialize)]
struct DownloadPayload {
    filename: String,
    content: String,
}

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
    let is_desktop_viewport = state.is_desktop_viewport;
    let sidebar_collapsed = state.sidebar_collapsed;
    // See views/home.rs's heading_top_pad — same fixed-hamburger overlap,
    // same fix.
    let heading_top_pad = if *is_desktop_viewport.read() && !*sidebar_collapsed.read() { "pt-4" } else { "pt-16" };

    let pwa_install_available = state.pwa_install_available;
    let pwa_standalone = state.pwa_standalone;
    let mut install_busy = use_signal(|| false);
    let mut install_declined = use_signal(|| false);
    let trigger_install = move |_| {
        install_busy.set(true);
        spawn(async move {
            // Same script as layouts::navbar's INSTALL_PROMPT_LISTENER_SCRIPT
            // stashes the event for — duplicated locally rather than made
            // pub across modules for one small, self-contained script (same
            // call this codebase already makes for LAST_REFRESHED_AT_KEY).
            let mut eval = document::eval(r#"
                if (window.__bsbInstallPrompt) {
                    window.__bsbInstallPrompt.prompt();
                    const choice = await window.__bsbInstallPrompt.userChoice;
                    window.__bsbInstallPrompt = null;
                    dioxus.send(choice.outcome === 'accepted');
                } else {
                    dioxus.send(false);
                }
            "#);
            if let Ok(accepted) = eval.recv::<bool>().await {
                if !accepted {
                    install_declined.set(true);
                }
            }
            install_busy.set(false);
        });
    };

    // See navbar.rs's vault_switcher_cmp for why this is auth_token, not
    // user_email — the latter only populates after a session-check round
    // trip that can stall or never complete while offline, which used to
    // leave this permanently false despite a real session existing.
    let has_synced = auth_token.read().is_some();

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
            // remove_item, not set("", ...) — an empty string still reads
            // back as Some("") on next launch (main.rs's startup restore),
            // which every auth_token.is_some() check treats as "logged in"
            // with nothing real behind it. See navbar.rs's remove_synced
            // for the live bug this caused (permanently stuck behind
            // Login's already-logged-in redirect, no way back in).
            storage.remove_item("auth_token").ok();
            storage.remove_item("refresh_token").ok();
            // Same key as navbar.rs's LAST_REFRESHED_AT_KEY (the shared
            // refresh-coordination clock) — stale here would just mean a
            // fresh future login's first refresh gets skipped for up to 5
            // minutes, harmless but worth clearing along with the tokens.
            storage.remove_item("auth_last_refreshed_at").ok();
        }
        // Offline-first sync's mirror/queue (see api::synced_mirror/
        // sync_queue) are cached under this account's Synced session —
        // wipe them here too, or a later "+ Add a vault" login (same
        // browser, maybe a different account) would see the previous
        // account's stale cached data before the first real fetch.
        crate::api::synced_mirror::clear();
        crate::api::sync_queue::clear();
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

    // "Delete my account" (2026-08-02, replacing the old two-tier "remove
    // vault" vs "delete data" vs "delete account" spread — see memory: a
    // user explicitly asked for one action that deletes everything,
    // including the login, not a halfway "just the data" option that left
    // an extra step for them to think about). Calls the delete-account
    // Supabase Edge Function — see SupabaseStorage::delete_account's own
    // doc comment for why that has to be a server-side call, not something
    // done with the client's own token.
    let mut confirming_delete_account = use_signal(|| false);
    let mut deleting_account = use_signal(|| false);
    let mut delete_account_error = use_signal(|| None::<String>);
    let delete_my_account = move |_| {
        if *deleting_account.read() {
            return;
        }
        let Some(token) = auth_token.read().clone() else {
            delete_account_error.set(Some("You need to be logged in to the Synced vault to delete its account.".to_string()));
            return;
        };
        deleting_account.set(true);
        delete_account_error.set(None);
        spawn(async move {
            let storage = ActiveStorage::for_vault(VaultKind::Synced, Some(token));
            let result = match &storage {
                ActiveStorage::Supabase(s) => s.delete_account().await,
                ActiveStorage::Local(_) => Ok(()), // can't happen — Synced requested with a token present
            };
            match result {
                Ok(()) => {
                    moments.set(vec![]);
                    entities.set(vec![]);
                    confirming_delete_account.set(false);
                    // Same "land back on Local, not an empty Synced view"
                    // posture as Remove Synced vault above.
                    #[cfg(not(feature = "desktop"))]
                    if let Some(s) = window().and_then(|w| w.local_storage().ok().flatten()) {
                        s.remove_item("auth_token").ok();
                        s.remove_item("refresh_token").ok();
                        s.set("active_vault", VaultKind::Local.as_storage_str()).ok();
                        s.remove_item("auth_last_refreshed_at").ok();
                    }
                    // The account itself no longer exists server-side —
                    // nothing left to ever flush this queue against.
                    crate::api::synced_mirror::clear();
                    crate::api::sync_queue::clear();
                    auth_token.set(None);
                    user_id.set(None);
                    user_email.set(None);
                    active_vault.set(VaultKind::Local);
                }
                Err(e) => {
                    clog!("Error deleting account: {}", e);
                    delete_account_error.set(Some(format!("Couldn't delete your account: {e}")));
                }
            }
            deleting_account.set(false);
        });
    };

    // Full data backup/restore (2026-08-01) — see memory: a real, distressing
    // data-loss incident is what prompted this. Deliberately not gated by
    // has_synced like the sections below — it operates on whichever vault
    // is currently active (Local or Synced), since "I have no way to get my
    // data back out" is exactly as bad for a Local-only user.
    let effective_vault = active_vault.read().effective(&auth_token.read());
    let vault_label = if effective_vault == VaultKind::Synced { "Synced" } else { "Local" };

    let mut backup_busy = use_signal(|| false);
    let mut backup_error = use_signal(|| None::<String>);
    let mut backup_status = use_signal(|| None::<String>);
    let mut restore_busy = use_signal(|| false);
    let mut restore_error = use_signal(|| None::<String>);
    let mut restore_status = use_signal(|| None::<String>);

    let download_backup = move |_| {
        if *backup_busy.read() {
            return;
        }
        backup_busy.set(true);
        backup_error.set(None);
        backup_status.set(None);
        let vault = active_vault.read().effective(&auth_token.read());
        let token = auth_token.read().clone();
        spawn(async move {
            match crate::api::export_backup(vault, token).await {
                Ok(yaml) => {
                    let filename = format!(
                        "black-server-book-backup-{}.yaml",
                        chrono::Utc::now().format("%Y-%m-%d")
                    );
                    #[cfg(not(feature = "desktop"))]
                    {
                        let mut eval = document::eval(DOWNLOAD_FILE_SCRIPT);
                        let _ = eval.send(DownloadPayload { filename, content: yaml });
                    }
                    backup_status.set(Some("Backup downloaded.".to_string()));
                }
                Err(e) => {
                    clog!("Error exporting backup: {}", e);
                    backup_error.set(Some(format!("Couldn't create the backup: {e}")));
                }
            }
            backup_busy.set(false);
        });
    };

    let mut handle_restore_file = move |text: String| {
        if *restore_busy.read() {
            return;
        }
        restore_busy.set(true);
        restore_error.set(None);
        restore_status.set(None);
        let vault = active_vault.read().effective(&auth_token.read());
        let token = auth_token.read().clone();
        spawn(async move {
            match crate::api::import_backup(vault, token.clone(), text).await {
                Ok(summary) => {
                    let untyped_note = if summary.entities_untyped > 0 {
                        format!(
                            " {} imported without a matching type in this vault (set them from the Info panel).",
                            summary.entities_untyped
                        )
                    } else {
                        String::new()
                    };
                    restore_status.set(Some(format!(
                        "Restored {} {}, {} {}, {} {}.{untyped_note}",
                        summary.entities, if summary.entities == 1 { "entity" } else { "entities" },
                        summary.moments, if summary.moments == 1 { "moment" } else { "moments" },
                        summary.reactions, if summary.reactions == 1 { "reaction" } else { "reactions" },
                    )));
                    // Same vault/token as before the restore, so the fetch
                    // effect in views/home.rs (which only reruns when those
                    // change) won't pick this up on its own — refetch here
                    // so the restored data actually shows up without a
                    // manual page reload.
                    let storage = ActiveStorage::for_vault(vault, token);
                    if let Ok(m) = storage.get_moments().await {
                        moments.set(m);
                    }
                    if let Ok(e) = storage.get_entities().await {
                        entities.set(e);
                    }
                }
                Err(e) => {
                    clog!("Error importing backup: {}", e);
                    restore_error.set(Some(format!("Couldn't restore that backup: {e}")));
                }
            }
            restore_busy.set(false);
        });
    };

    rsx! {
        div {
            class: "px-4 {heading_top_pad}",
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
            if !*pwa_standalone.read() {
                div {
                    class: "rounded-lg border border-border bg-background p-4",
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Install app" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Add Black Server Book to your home screen or app dock for quicker access and a full-screen, no-browser-chrome view."
                    }
                    if *pwa_install_available.read() {
                        button {
                            class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-4 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed",
                            disabled: *install_busy.read(),
                            onclick: trigger_install,
                            if *install_busy.read() { "Installing…" } else { "Install Black Server Book" }
                        }
                        if *install_declined.read() {
                            p { class: "text-sm text-muted-foreground mt-2", "No worries — you can install any time from here." }
                        }
                    } else {
                        p {
                            class: "text-xs text-muted-foreground rounded-md border border-border bg-muted/30 px-3 py-2",
                            "Your browser doesn't offer a one-tap install here (common on Safari). On iPhone/iPad: tap the Share icon, then \"Add to Home Screen\". On desktop Chrome/Edge: look for an install icon in the address bar, or the browser menu's \"Install…\" option."
                        }
                    }
                }
            }
            div {
                class: "rounded-lg border border-border bg-background p-4",
                h3 { class: "text-sm font-semibold text-foreground mb-1", "Backup & restore" }
                p {
                    class: "text-sm text-muted-foreground mb-3",
                    "Downloads everything in your {vault_label} vault — entities, moments, reactions, all of it — as one file you can restore from later, on this device or any other."
                }
                div {
                    class: "flex flex-wrap items-center gap-3",
                    button {
                        class: "rounded-md border border-transparent bg-primary text-primary-foreground text-sm px-4 py-1.5 font-medium hover:bg-primary/90 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed",
                        disabled: *backup_busy.read(),
                        onclick: download_backup,
                        if *backup_busy.read() { "Preparing…" } else { "Download backup" }
                    }
                    label {
                        r#for: "restore-backup-input",
                        class: if *restore_busy.read() {
                            "rounded-md border border-border bg-background text-foreground text-sm px-4 py-1.5 font-medium inline-block opacity-50 cursor-not-allowed"
                        } else {
                            "rounded-md border border-border bg-background text-foreground text-sm px-4 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer inline-block"
                        },
                        if *restore_busy.read() { "Restoring…" } else { "Restore from backup file" }
                    }
                    input {
                        id: "restore-backup-input",
                        r#type: "file",
                        accept: "application/yaml,text/yaml,.yaml,.yml",
                        class: "hidden",
                        disabled: *restore_busy.read(),
                        onchange: move |e: Event<FormData>| {
                            let Some(file) = e.files().into_iter().next() else { return };
                            spawn(async move {
                                match file.read_string().await {
                                    Ok(text) => handle_restore_file(text),
                                    Err(err) => {
                                        clog!("Error reading backup file: {:?}", err);
                                        restore_error.set(Some("Couldn't read that file.".to_string()));
                                    }
                                }
                            });
                        },
                    }
                }
                if let Some(msg) = backup_status.read().as_ref() {
                    p { class: "text-sm text-foreground mt-2", "{msg}" }
                }
                if let Some(msg) = backup_error.read().as_ref() {
                    p { class: "text-sm text-destructive mt-2", "{msg}" }
                }
                if let Some(msg) = restore_status.read().as_ref() {
                    p { class: "text-sm text-foreground mt-2", "{msg}" }
                }
                if let Some(msg) = restore_error.read().as_ref() {
                    p { class: "text-sm text-destructive mt-2", "{msg}" }
                }
                p {
                    class: "text-xs text-muted-foreground mt-2",
                    "Restoring adds to what's already here rather than replacing it — importing the same backup twice will duplicate everything in it."
                }
            }
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
                    h3 { class: "text-sm font-semibold text-foreground mb-1", "Delete my account" }
                    p {
                        class: "text-sm text-muted-foreground mb-3",
                        "Permanently deletes everything in your Synced vault — entities, moments, reactions, all of it — and your login itself. This can't be undone; you'd need to sign up again with this email to come back, starting from empty. You'll land back on the Local vault, which is untouched."
                    }
                    if let Some(msg) = delete_account_error.read().as_ref() {
                        p { class: "text-sm text-destructive mb-2", "{msg}" }
                    }
                    if *confirming_delete_account.read() {
                        div {
                            class: "flex items-center gap-2",
                            span { class: "text-sm text-foreground", "Permanently delete your account and everything in it?" }
                            button {
                                class: "rounded-md border border-transparent bg-destructive text-primary-foreground dark:text-foreground text-sm px-3 py-1.5 font-medium hover:bg-destructive/90 transition-colors cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed",
                                disabled: *deleting_account.read(),
                                onclick: delete_my_account,
                                if *deleting_account.read() { "Deleting…" } else { "Confirm" }
                            }
                            button {
                                class: "rounded-md border border-border bg-background text-foreground text-sm px-3 py-1.5 font-medium hover:bg-muted transition-colors cursor-pointer",
                                disabled: *deleting_account.read(),
                                onclick: move |_| confirming_delete_account.set(false),
                                "Cancel"
                            }
                        }
                    } else {
                        button {
                            class: "rounded-md border border-destructive/50 bg-background text-destructive text-sm px-4 py-1.5 font-medium hover:bg-destructive/10 transition-colors cursor-pointer",
                            onclick: move |_| confirming_delete_account.set(true),
                            "Delete my account"
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
