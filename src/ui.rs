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
pub fn fa_calendar() -> Element { rsx! { i { class: "fa-solid fa-calendar-days" } } }

#[component]
pub fn fa_clock() -> Element { rsx! { i { class: "fa-solid fa-clock" } } }

#[component]
pub fn fa_gear() -> Element { rsx! { i { class: "fa-solid fa-gear" } } }

#[component]
pub fn fa_repeat() -> Element { rsx! { i { class: "fa-solid fa-repeat" } } }
pub fn fa_calendar_xmark() -> Element { rsx! { i { class: "fa-solid fa-calendar-xmark" } } }

#[component]
pub fn fa_user() -> Element { rsx! { i { class: "fa-solid fa-user" } } }

#[component]
pub fn fa_lock() -> Element { rsx! { i { class: "fa-solid fa-lock" } } }

#[component]
pub fn fa_note_sticky() -> Element { rsx! { i { class: "fa-solid fa-note-sticky" } } }

#[component]
pub fn fa_expand() -> Element { rsx! { i { class: "fa-solid fa-expand" } } }

// ----- Slider
#[derive(Props, Clone, PartialEq)]
pub struct GravitySelectProps {
    pub onchange: EventHandler<i32>,
    #[props(default = 0)]
    pub ival: i32,
}


// UI shows -10..10 (the -100..100 raw range was "gratuitous," per the
// user) but the option `value`s are still the raw *10 numbers underneath —
// the stored gravity field, the Distance/Drift formula's closed_gravity
// term, and every other call site keep working in the original -100..100
// scale completely unchanged. This is deliberately just a display-layer
// fix, not a real rescale, so it doesn't touch existing data or the
// formula at all.
#[component]
pub fn gravity_select(props: GravitySelectProps) -> Element {
    rsx! {
        select {
            oninput: move |e| props.onchange.call(e.value().parse::<i32>().unwrap_or(0)),
            for i in (-100..=100).step_by(10) {
                option {
                    value: "{i}",
                    selected: i == props.ival,
                    "{i / 10}"
                }
            }
        }
    }
}
