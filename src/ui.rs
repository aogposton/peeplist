use dioxus::prelude::*;
use crate::theme::*;



#[component]
pub fn fa_thumbs_up() -> Element { rsx! { i { class: "fa-solid fa-thumbs-up" } } }

#[component]
pub fn fa_trash() -> Element { rsx! { i { class: "fa-solid fa-trash" } } }

#[component]
pub fn fa_plus() -> Element { rsx! { i { class: "fa-solid fa-plus" } } }

#[component]
pub fn fa_bolt() -> Element { rsx! { i { class: "fa-solid fa-bolt" } } }

#[component]
pub fn fa_inbox() -> Element { rsx! { i { class: "fa-solid fa-inbox" } } }

#[component]
pub fn fa_circle_nodes() -> Element { rsx! { i { class: "fa-solid fa-circle-nodes" } } }

#[component]
pub fn fa_compass() -> Element { rsx! { i { class: "fa-solid fa-compass" } } }

#[component]
pub fn fa_calendar() -> Element { rsx! { i { class: "fa-solid fa-calendar-days" } } }

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
