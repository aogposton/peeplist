use dioxus::prelude::*;
use crate::theme::*;



#[component]
pub fn fa_thumbs_up() -> Element { rsx! { i { class: "fa-solid fa-thumbs-up" } } }

#[component]
pub fn fa_trash() -> Element { rsx! { i { class: "fa-solid fa-trash" } } }

#[component]
pub fn fa_plus() -> Element { rsx! { i { class: "fa-solid fa-plus" } } }

#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    pub label: Element,
    pub btnclick: EventHandler<MouseEvent>,
    #[props(optional)]
    pub class: Option<String>,
}

// ----- Slider
#[derive(Props, Clone, PartialEq)]
pub struct GravitySelectProps {
    pub onchange: EventHandler<i32>,
    #[props(default = 0)]
    pub ival: i32,
}


#[component]
pub fn gravity_select(props: GravitySelectProps) -> Element {
    rsx! {
        select {
            oninput: move |e| props.onchange.call(e.value().parse::<i32>().unwrap_or(0)),
            for i in -100..=100 {
                option {
                    value: "{i}",
                    selected: i == props.ival,
                    "{i}"
                }
            }
        }
    }
}

#[component] pub fn button_cmp(props: ButtonProps) -> Element {
    let mut is_hovering = use_signal(|| false);
    let base_class = "w-full border px-4 py-2 rounded-md font-semibold transition-all cursor-pointer";
    let hover_class = if *is_hovering.read() { "opacity-50" } else { "" };
    let extra_class = props.class.clone().unwrap_or_default();

    rsx! {
        button {
            class: "{BG} {base_class} {hover_class} {extra_class}",
            onmouseenter: move |_| is_hovering.set(true),
            onmouseleave: move |_| is_hovering.set(false),
            onclick: move |e| {
                clog!("btn clicked");
                props.btnclick.call(e);
            },
            {props.label}
        }
    }
}
