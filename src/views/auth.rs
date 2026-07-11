// use dioxus_storage::LocalStorage;
use crate::Route;
use dioxus::prelude::*;
use crate::AppState;
use crate::theme::*;
use crate::View::*;
use crate::types::*;
use crate::ui::*;
use crate::api::*;
use web_sys::{window, Storage};
use lumen_blocks::components::input::{Input, InputVariant};
use lumen_blocks::components::button::{Button, ButtonVariant};
use lumen_blocks::components::label::Label;


pub fn Logout() -> Element {
    let mut state = use_context::<AppState>();
    let mut loginform = use_signal(LoginForm::default);

    use_effect(move || {
        spawn(async move {
        });
    });

    let nav = navigator();
    nav.push(Route::Home {});

    let storage = window().unwrap().local_storage().unwrap().unwrap();
    storage.set("auth_token", &"");
    state.auth_token.set(None);
    state.user_id.set(None);

    rsx! {

    }
}

#[component]
pub fn LoginCMP() -> Element {
    let mut state = use_context::<AppState>();
    let mut loginform = use_signal(LoginForm::default);
    let mut error = use_signal(|| None::<String>);

    use_effect(move || {
        if state.auth_token.read().is_some() {
            navigator().push(Route::Home {});
        }
    });

    let mut submitform = move || {
        let form = loginform.read().clone();
        error.set(None);
        spawn(async move {
            match login(form.email, form.password).await {
                Ok(auth) => {
                    let storage = window().unwrap().local_storage().unwrap().unwrap();
                    storage.set("auth_token", &auth.access_token);
                    state.auth_token.set(Some(auth.access_token));
                    state.user_id.set(Some(auth.user.id));
                    let nav = navigator();
                    nav.push(Route::Home {});
                }
                Err(e) => {
                    clog!("{:?}", e);
                    error.set(Some("Incorrect email or password.".to_string()));
                }
            }
        });
    };


    rsx! {
        div {
            class: "flex justify-center items-center w-full h-screen bg-background",
            div {
                class: "w-full max-w-sm p-8 flex flex-col gap-y-5 rounded-lg border border-border bg-card shadow-sm",
                span {
                    class: "text-2xl font-semibold text-center text-foreground",
                    "Login"
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
                    "Login"
                }
            }
        }
    }
}
