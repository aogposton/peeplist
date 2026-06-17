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
pub fn Login() -> Element {
    let mut state = use_context::<AppState>();
    let mut loginform = use_signal(LoginForm::default);
    let submitform = move || {
        let form = loginform.read().clone();
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
                Err(e) => clog!("{:?}", e),
            }
        });
    };


    rsx! {
        div {
            class: "flex justify-center items-center w-full h-screen",
            style: "background-color: {BG};",
            div {
                class: "w-100 p-4 flex flex-col gap-y-4",
                style: "background-color: {BG};",
                span {
                    class:"text-2xl flex justify-center",
                    "Login"
                }
                div {
                    class: "flex text-2xl justify-around ",
                    span {class:"text-slate-600 mr-4","Email"}
                    input {
                        r#name: "email",
                        oninput: move |e| loginform.write().email = e.value(),
                        class: "text-black w-full border focus:border-teal focus:outline-none focus:ring-0",
                    }
                }

                div {
                    class: "flex text-2xl justify-around ",
                    span {class:"text-slate-600 mr-4","Password"}
                    input {
                        r#name: "password",
                        r#type: "password",
                        oninput: move |e| loginform.write().password = e.value(),
                        onkeypress: move |e| {
                            if e.key() == Key::Enter {
                                submitform();
                            }
                        },
                        class: "text-black w-full border focus:border-teal focus:outline-none focus:ring-0",
                    }
                }
                div {
                    class: "flex justify-around ",
                    button_cmp {
                        btnclick: move |_| {
                            submitform();
                        },
                        label: rsx! {"submit"},
                        class: "text-black w-full border focus:border-teal focus:outline-none focus:ring-0",
                    }
                }
            }
        }
    }
}
