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

// Loads Cloudflare Turnstile's script once (guarded so a second run of this
// eval — shouldn't happen, but see the `captcha_started` guard at the call
// site — doesn't inject the script twice), renders the widget into
// `#peeplist-turnstile`, and streams every token its callback produces back
// to Rust — Turnstile can re-invoke the callback more than once (managed
// mode auto-refreshes an expiring token), which is exactly what the
// `while let Ok(token) = eval.recv()` loop at the call site expects.
const TURNSTILE_SCRIPT: &str = r#"
    const siteKey = await dioxus.recv();
    if (!window.__peeplistTurnstileLoaded) {
        window.__peeplistTurnstileLoaded = true;
        await new Promise((resolve) => {
            const script = document.createElement('script');
            script.src = 'https://challenges.cloudflare.com/turnstile/v0/api.js?onload=__peeplistTurnstileOnLoad';
            script.async = true;
            window.__peeplistTurnstileOnLoad = resolve;
            document.head.appendChild(script);
        });
    }
    window.turnstile.render('#peeplist-turnstile', {
        sitekey: siteKey,
        callback: (token) => dioxus.send(token),
    });
"#;

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
    // The email/password Inputs are controlled components (lumen_blocks'
    // Input has a real `value: String` prop, defaulting to "" when not
    // passed) — without binding `value` back to loginform, they were
    // effectively pinned to "", and any re-render that touched the tree
    // (e.g. error.set(None) at the top of submitform, right before the
    // async call) would re-sync the DOM input back down to that pinned ""
    // value, which is what "credentials disappear for a second on submit"
    // actually was. submitting doubles as both the spinner state and what
    // stops that same visible flash from mattering even if it still
    // technically re-renders.
    let mut submitting = use_signal(|| false);
    // No self-service signup existed before this — only an account created
    // by hand directly in Supabase could ever log in (see the local-first
    // pivot plan's Phase 1f, deliberately deferred until now). One form,
    // toggled between the two modes, rather than a separate route/page.
    let mut is_signup = use_signal(|| false);
    let mut needs_confirmation = use_signal(|| false);
    // Password reset (2026-07-29 — closing a real pre-launch gap: there was
    // no recovery path at all, so a forgotten password meant permanent
    // lockout from the Synced vault). Web only — desktop has no persisted
    // session to begin with, and an emailed link would open in the OS
    // browser, not back into the native app, so this wouldn't make sense
    // there yet.
    let mut show_forgot = use_signal(|| false);
    let mut reset_sent = use_signal(|| false);

    // Signup abuse protection (2026-07-29) — a Cloudflare Turnstile widget,
    // loaded and rendered only if TURNSTILE_SITE_KEY is actually configured
    // (see build.rs; empty by default, so this is a no-op until it's set).
    // One widget for the whole form, its token reused across whichever of
    // signup/login/request-reset the user actually submits — matches
    // Supabase's own "Enable CAPTCHA protection" setting, which (once
    // turned on in that project's Auth settings) requires a token on all
    // three of those endpoints, not just signup.
    let mut captcha_token = use_signal(|| None::<String>);
    let mut captcha_started = use_signal(|| false);
    let turnstile_site_key = env!("TURNSTILE_SITE_KEY");
    use_effect(move || {
        if turnstile_site_key.is_empty() || *captcha_started.read() {
            return;
        }
        captcha_started.set(true);
        spawn(async move {
            let mut eval = document::eval(TURNSTILE_SCRIPT);
            if eval.send(turnstile_site_key).is_ok() {
                while let Ok(token) = eval.recv::<String>().await {
                    captcha_token.set(Some(token));
                }
            }
        });
    });

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
        if *submitting.read() {
            return;
        }
        let form = loginform.read().clone();
        let captcha = captcha_token.read().clone();
        error.set(None);
        needs_confirmation.set(false);
        submitting.set(true);
        spawn(async move {
            if *is_signup.read() {
                match signup(form.email, form.password, captcha).await {
                    Ok(SignupOutcome::LoggedIn(auth)) => log_in_session(auth),
                    Ok(SignupOutcome::NeedsConfirmation) => needs_confirmation.set(true),
                    Err(e) => {
                        clog!("{:?}", e);
                        error.set(Some("Couldn't create that account — the email may already be in use, or the password may be too short.".to_string()));
                    }
                }
            } else {
                match login(form.email, form.password, captcha).await {
                    Ok(auth) => log_in_session(auth),
                    Err(e) => {
                        clog!("{:?}", e);
                        error.set(Some("Incorrect email or password.".to_string()));
                    }
                }
            }
            submitting.set(false);
        });
    };

    #[cfg(not(feature = "desktop"))]
    let mut request_reset = move || {
        if *submitting.read() {
            return;
        }
        let email = loginform.read().email.clone();
        if email.trim().is_empty() {
            error.set(Some("Enter your email first.".to_string()));
            return;
        }
        error.set(None);
        submitting.set(true);
        let captcha = captcha_token.read().clone();
        spawn(async move {
            let redirect_to = window()
                .and_then(|w| w.location().origin().ok())
                .map(|origin| format!("{origin}/reset-password"))
                .unwrap_or_default();
            // Same anti-enumeration posture as Supabase's own recover
            // endpoint — always show "check your email" regardless of
            // whether the request itself succeeded, so this can't be used
            // to probe which emails have accounts.
            if let Err(e) = request_password_reset(email, redirect_to, captcha).await {
                clog!("{:?}", e);
            }
            reset_sent.set(true);
            submitting.set(false);
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
                    "Log in to sync this device with your account. Black Server Book works fully offline without one."
                }
                // Beta pricing transparency (2026-07-29 decision: launch with
                // Synced free, but be upfront that it won't stay free
                // forever, rather than silently flipping a paywall on later
                // with no warning) — shown at the exact moment someone's
                // deciding whether to create/add a Synced vault, not just
                // buried in Settings.
                p {
                    class: "text-xs text-muted-foreground rounded-md border border-border bg-muted/30 px-3 py-2 -mt-1",
                    "Synced is free during beta. When we introduce pricing, anyone already using it will get advance notice before anything changes."
                }
                // Turnstile renders into this div once TURNSTILE_SITE_KEY is
                // configured (see the effect above) — a fixed, always-
                // mounted spot regardless of which sub-view (login/signup/
                // forgot-password) is currently showing, since the widget
                // is only ever rendered once per page load, not re-created
                // per branch switch. Collapses to nothing when unconfigured.
                if !turnstile_site_key.is_empty() {
                    div { id: "peeplist-turnstile" }
                }
                if *needs_confirmation.read() {
                    div {
                        class: "text-sm text-foreground rounded-md border border-border p-3",
                        "Check your email to confirm your account, then log in below."
                    }
                }
                if cfg!(not(feature = "desktop")) && *show_forgot.read() {
                    if *reset_sent.read() {
                        div {
                            class: "text-sm text-foreground rounded-md border border-border p-3",
                            "If that email has an account, a reset link is on its way. Check your inbox."
                        }
                        a {
                            class: "text-sm text-center text-muted-foreground hover:text-foreground cursor-pointer",
                            onclick: move |_| {
                                show_forgot.set(false);
                                reset_sent.set(false);
                            },
                            "Back to login"
                        }
                    } else {
                        div {
                            class: "flex flex-col gap-y-1.5",
                            onkeypress: {
                                #[cfg(not(feature = "desktop"))]
                                { move |e: Event<KeyboardData>| if e.key() == Key::Enter { request_reset(); } }
                                #[cfg(feature = "desktop")]
                                { move |_: Event<KeyboardData>| {} }
                            },
                            Label { for_id: Some("reset-email".to_string()), "Email" }
                            Input {
                                id: Some("reset-email".to_string()),
                                name: "email",
                                input_type: "email",
                                full_width: true,
                                value: loginform.read().email.clone(),
                                disabled: *submitting.read(),
                                on_input: move |e: Event<FormData>| loginform.write().email = e.value(),
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
                            disabled: *submitting.read(),
                            loading: *submitting.read(),
                            on_click: {
                                #[cfg(not(feature = "desktop"))]
                                { move |_| request_reset() }
                                #[cfg(feature = "desktop")]
                                { move |_| {} }
                            },
                            "Send reset link"
                        }
                        a {
                            class: "text-sm text-center text-muted-foreground hover:text-foreground cursor-pointer",
                            onclick: move |_| {
                                show_forgot.set(false);
                                error.set(None);
                            },
                            "Back to login"
                        }
                    }
                } else {
                    div {
                        class: "flex flex-col gap-y-1.5",
                        Label { for_id: Some("login-email".to_string()), "Email" }
                        Input {
                            id: Some("login-email".to_string()),
                            name: "email",
                            input_type: "email",
                            full_width: true,
                            value: loginform.read().email.clone(),
                            disabled: *submitting.read(),
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
                            value: loginform.read().password.clone(),
                            disabled: *submitting.read(),
                            variant: if error().is_some() { InputVariant::Error } else { InputVariant::Default },
                            on_input: move |e: Event<FormData>| loginform.write().password = e.value(),
                        }
                    }
                    if cfg!(not(feature = "desktop")) && !*is_signup.read() {
                        a {
                            class: "text-xs text-muted-foreground hover:text-foreground cursor-pointer -mt-3 self-end",
                            onclick: move |_| {
                                show_forgot.set(true);
                                error.set(None);
                            },
                            "Forgot password?"
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
                        disabled: *submitting.read(),
                        loading: *submitting.read(),
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
                    div {
                        class: "flex items-center justify-center gap-3 text-xs text-muted-foreground",
                        // Canonical copy lives on the marketing site
                        // (marketing/privacy|terms/index.html), not as an
                        // in-app route — one place to keep it up to date,
                        // and it needs to sit at the marketing domain
                        // (blackserverbook.com/privacy), not the app's own
                        // subdomain.
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
}

// Landing page for the link in a password-recovery email (see
// api::auth::request_password_reset's `redirect_to`). Supabase appends the
// actual recovery tokens as a URL *fragment* (`#access_token=...&type=
// recovery`), not a query string or path segment, so this has to read
// `window.location.hash` directly rather than anything the router itself
// parses. Web only, same reasoning as LoginCMP's "Forgot password?" link —
// desktop has no persisted session and the email link opens in the OS
// browser anyway, not back into the native app.
#[component]
pub fn ResetPasswordCmp() -> Element {
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut error = use_signal(|| None::<String>);
    let mut success = use_signal(|| false);
    let mut submitting = use_signal(|| false);
    let mut recovery_token = use_signal(|| None::<String>);

    #[cfg(not(feature = "desktop"))]
    use_effect(move || {
        let token = window()
            .and_then(|w| w.location().hash().ok())
            .and_then(|hash| {
                hash.trim_start_matches('#')
                    .split('&')
                    .find_map(|pair| pair.strip_prefix("access_token="))
                    .map(|s| s.to_string())
            });
        recovery_token.set(token);
    });

    let mut submit = move || {
        if *submitting.read() {
            return;
        }
        let Some(token) = recovery_token.read().clone() else {
            error.set(Some("This reset link is invalid or has expired — request a new one from the login page.".to_string()));
            return;
        };
        let pw = new_password.read().clone();
        if pw.len() < 6 {
            error.set(Some("Password needs to be at least 6 characters.".to_string()));
            return;
        }
        if pw != *confirm_password.read() {
            error.set(Some("Passwords don't match.".to_string()));
            return;
        }
        error.set(None);
        submitting.set(true);
        spawn(async move {
            match update_password(token, pw).await {
                Ok(()) => success.set(true),
                Err(e) => {
                    clog!("{:?}", e);
                    error.set(Some("Couldn't reset your password — the link may have expired. Request a new one from the login page.".to_string()));
                }
            }
            submitting.set(false);
        });
    };

    rsx! {
        div {
            class: "flex justify-center items-center w-full min-h-screen overflow-y-auto bg-background py-8",
            div {
                class: "w-full max-w-sm p-8 flex flex-col gap-y-5 rounded-lg border border-border bg-card shadow-sm",
                span { class: "text-2xl font-semibold text-foreground", "Set a new password" }
                if *success.read() {
                    div {
                        class: "text-sm text-foreground rounded-md border border-border p-3",
                        "Password updated."
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        full_width: true,
                        on_click: move |_| { navigator().push(Route::LoginCMP {}); },
                        "Log in"
                    }
                } else {
                    div {
                        class: "flex flex-col gap-y-1.5",
                        Label { for_id: Some("new-password".to_string()), "New password" }
                        Input {
                            id: Some("new-password".to_string()),
                            name: "new-password",
                            input_type: "password",
                            full_width: true,
                            value: new_password.read().clone(),
                            disabled: *submitting.read(),
                            on_input: move |e: Event<FormData>| new_password.set(e.value()),
                        }
                    }
                    div {
                        class: "flex flex-col gap-y-1.5",
                        onkeypress: move |e| {
                            if e.key() == Key::Enter {
                                submit();
                            }
                        },
                        Label { for_id: Some("confirm-password".to_string()), "Confirm new password" }
                        Input {
                            id: Some("confirm-password".to_string()),
                            name: "confirm-password",
                            input_type: "password",
                            full_width: true,
                            value: confirm_password.read().clone(),
                            disabled: *submitting.read(),
                            variant: if error.read().is_some() { InputVariant::Error } else { InputVariant::Default },
                            on_input: move |e: Event<FormData>| confirm_password.set(e.value()),
                        }
                    }
                    if let Some(msg) = error.read().as_ref() {
                        div {
                            class: "text-sm text-destructive",
                            "{msg}"
                        }
                    }
                    Button {
                        variant: ButtonVariant::Primary,
                        full_width: true,
                        disabled: *submitting.read(),
                        loading: *submitting.read(),
                        on_click: move |_| submit(),
                        "Set new password"
                    }
                }
            }
        }
    }
}
