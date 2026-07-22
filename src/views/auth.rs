// use dioxus_storage::LocalStorage;
use crate::Route;
use dioxus::prelude::*;
use crate::AppState;
use crate::theme::*;
use crate::View::*;
use crate::types::*;
use crate::ui::*;
use crate::api::*;
use web_sys::window;
use lumen_blocks::components::input::{Input, InputVariant};
use lumen_blocks::components::button::{Button, ButtonVariant};
use lumen_blocks::components::label::Label;


pub fn Logout() -> Element {
    let mut state = use_context::<AppState>();

    // Logging out lands you back on the app (now on the Local vault), never
    // on a dead-end login screen — see the vault switcher's "Remove" action
    // in navbar.rs, which is the primary way this gets triggered now.
    let nav = navigator();
    nav.push(Route::Home {});

    // Desktop has no preference persistence yet — see main.rs's startup
    // effect.
    #[cfg(not(feature = "desktop"))]
    {
        if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
            storage.set("auth_token", &"").ok();
            storage.set("refresh_token", &"").ok();
            storage.set("active_vault", VaultKind::Local.as_storage_str()).ok();
        }
    }
    state.auth_token.set(None);
    state.user_id.set(None);
    state.user_email.set(None);
    state.active_vault.set(VaultKind::Local);

    rsx! {

    }
}

#[component]
pub fn LoginCMP() -> Element {
    let mut state = use_context::<AppState>();
    let mut loginform = use_signal(LoginForm::default);
    let mut error = use_signal(|| None::<String>);
    // No self-service signup existed before this — only an account created
    // by hand directly in Supabase could ever log in (see the local-first
    // pivot plan's Phase 1f, deliberately deferred until now). One form,
    // toggled between the two modes, rather than a separate route/page.
    let mut is_signup = use_signal(|| false);
    let mut needs_confirmation = use_signal(|| false);

    use_effect(move || {
        if state.auth_token.read().is_some() {
            navigator().push(Route::Home {});
        }
    });

    let mut log_in_session = move |auth: LoginResponse| {
        // Desktop has no preference persistence yet — see main.rs's
        // startup effect.
        #[cfg(not(feature = "desktop"))]
        {
            if let Some(storage) = window().and_then(|w| w.local_storage().ok().flatten()) {
                storage.set("auth_token", &auth.access_token).ok();
                storage.set("refresh_token", &auth.refresh_token).ok();
                storage.set("active_vault", VaultKind::Synced.as_storage_str()).ok();
            }
        }
        state.auth_token.set(Some(auth.access_token));
        state.user_id.set(Some(auth.user.id));
        state.user_email.set(Some(auth.user.email));
        // The whole point of logging in here is adding the Synced vault
        // (see the vault switcher's "+ Add a vault") — switch straight to
        // it rather than leaving Local selected.
        state.active_vault.set(VaultKind::Synced);
        navigator().push(Route::Home {});
    };

    let mut submitform = move || {
        let form = loginform.read().clone();
        error.set(None);
        needs_confirmation.set(false);
        spawn(async move {
            if *is_signup.read() {
                match signup(form.email, form.password).await {
                    Ok(SignupOutcome::LoggedIn(auth)) => log_in_session(auth),
                    Ok(SignupOutcome::NeedsConfirmation) => needs_confirmation.set(true),
                    Err(e) => {
                        clog!("{:?}", e);
                        error.set(Some("Couldn't create that account — the email may already be in use, or the password may be too short.".to_string()));
                    }
                }
            } else {
                match login(form.email, form.password).await {
                    Ok(auth) => log_in_session(auth),
                    Err(e) => {
                        clog!("{:?}", e);
                        error.set(Some("Incorrect email or password.".to_string()));
                    }
                }
            }
        });
    };


    rsx! {
        div {
            // min-h-screen (not h-screen) + overflow-y-auto: h-screen is a
            // rigid box sized to the *layout* viewport, which on mobile
            // Safari/Chrome doesn't shrink when the on-screen keyboard opens
            // (only the visual viewport does) or reliably matches what's
            // actually visible before any address-bar collapse. A flex-
            // centered card in a rigid h-screen can render below the fold
            // with no way to scroll to it — exactly "the Login button isn't
            // pressable on mobile". min-h-screen lets the box grow past one
            // viewport if it has to, and overflow-y-auto makes sure it can
            // actually be scrolled into view when that happens.
            class: "flex justify-center items-center w-full min-h-screen overflow-y-auto bg-background py-8",
            div {
                class: "w-full max-w-sm p-8 flex flex-col gap-y-5 rounded-lg border border-border bg-card shadow-sm",
                div {
                    class: "flex items-center justify-between",
                    span {
                        class: "text-2xl font-semibold text-foreground",
                        if *is_signup.read() { "Create Synced vault" } else { "Add Synced vault" }
                    }
                    a {
                        class: "text-sm text-muted-foreground hover:text-foreground cursor-pointer",
                        onclick: move |_| { navigator().push(Route::Home {}); },
                        "Cancel"
                    }
                }
                p {
                    class: "text-sm text-muted-foreground -mt-2",
                    "Log in to sync this device with your account. Peeplist works fully offline without one."
                }
                if *needs_confirmation.read() {
                    div {
                        class: "text-sm text-foreground rounded-md border border-border p-3",
                        "Check your email to confirm your account, then log in below."
                    }
                }
                div {
                    class: "flex flex-col gap-y-1.5",
                    Label { for_id: Some("login-email".to_string()), "Email" }
                    Input {
                        id: Some("login-email".to_string()),
                        name: "email",
                        input_type: "email",
                        full_width: true,
                        on_input: move |e: Event<FormData>| loginform.write().email = e.value(),
                    }
                }
                div {
                    class: "flex flex-col gap-y-1.5",
                    onkeypress: move |e| {
                        if e.key() == Key::Enter {
                            submitform();
                        }
                    },
                    Label { for_id: Some("login-password".to_string()), "Password" }
                    Input {
                        id: Some("login-password".to_string()),
                        name: "password",
                        input_type: "password",
                        full_width: true,
                        variant: if error().is_some() { InputVariant::Error } else { InputVariant::Default },
                        on_input: move |e: Event<FormData>| loginform.write().password = e.value(),
                    }
                }
                if let Some(msg) = error() {
                    div {
                        class: "text-sm text-destructive",
                        "{msg}"
                    }
                }
                Button {
                    variant: ButtonVariant::Primary,
                    full_width: true,
                    on_click: move |_| submitform(),
                    if *is_signup.read() { "Create account" } else { "Login" }
                }
                a {
                    class: "text-sm text-center text-muted-foreground hover:text-foreground cursor-pointer",
                    onclick: move |_| {
                        let next = !*is_signup.read();
                        is_signup.set(next);
                        error.set(None);
                        needs_confirmation.set(false);
                    },
                    if *is_signup.read() { "Already have an account? Log in" } else { "Don't have an account? Sign up" }
                }
            }
        }
    }
}
