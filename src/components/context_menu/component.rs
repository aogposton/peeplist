use dioxus::prelude::*;

// A minimal right-click menu, replacing lumen_blocks' ContextMenu wrapper.
// That component pins an old dioxus-primitives revision that predates
// upstream's own outside-click-dismiss fix (confirmed by diffing the
// pinned rev against a newer checkout — the pinned ContextMenuContent has
// no use_outside_dismiss call at all), so the menu only ever closed by
// picking an item or refreshing the page. Bumping the shared lumen_blocks
// dependency to chase that one fix risked regressing every other component
// built on it elsewhere in the app, so this is a small, fully
// self-contained replacement instead — same backdrop-click-to-dismiss
// pattern already used by the delete-confirmation modals in sidebar.rs.

#[derive(Clone, Copy)]
struct MenuCtx {
    open: Signal<bool>,
    position: Signal<(f64, f64)>,
}

#[component]
pub fn ContextMenu(children: Element) -> Element {
    let ctx = MenuCtx {
        open: use_signal(|| false),
        position: use_signal(|| (0.0, 0.0)),
    };
    use_context_provider(|| ctx);
    rsx! {
        div { class: "relative", {children} }
    }
}

#[component]
pub fn ContextMenuTrigger(children: Element) -> Element {
    let mut ctx: MenuCtx = use_context();
    rsx! {
        div {
            oncontextmenu: move |e: Event<MouseData>| {
                e.prevent_default();
                let p = e.data().client_coordinates();
                ctx.position.set((p.x, p.y));
                ctx.open.set(true);
            },
            {children}
        }
    }
}

// value/index kept for source compatibility with the previous lumen_blocks-
// backed call sites (which set them for the library's own keyboard roving-
// focus bookkeeping) — this minimal version doesn't need either, there's
// no keyboard navigation implemented here.
#[component]
pub fn ContextMenuContent(#[props(default)] align: String, children: Element) -> Element {
    let ctx: MenuCtx = use_context();
    if !*ctx.open.read() {
        return rsx! {};
    }
    let (x, y) = *ctx.position.read();
    let _ = align;
    let mut open = ctx.open;
    rsx! {
        // Full-viewport, invisible — a click anywhere outside the menu
        // itself lands here first and closes it. The menu content sits on
        // top (higher z-index) and stops the click from reaching this
        // backdrop, same as the entity-delete confirmation popup.
        div {
            class: "fixed inset-0 z-40",
            onclick: move |_| open.set(false),
            oncontextmenu: move |e: Event<MouseData>| { e.prevent_default(); open.set(false); },
        }
        div {
            class: "fixed z-50 min-w-[10rem] rounded-md border border-border bg-popover text-popover-foreground shadow-lg p-1",
            style: "left: {x}px; top: {y}px;",
            onclick: move |e| e.stop_propagation(),
            {children}
        }
    }
}

#[component]
pub fn ContextMenuItem(
    #[props(default)] value: String,
    #[props(default)] index: usize,
    #[props(default)] destructive: bool,
    on_select: Option<EventHandler<String>>,
    children: Element,
) -> Element {
    let mut ctx: MenuCtx = use_context();
    let _ = index;
    let classes = if destructive {
        "flex w-full items-center rounded-sm px-2 py-1.5 text-sm text-destructive cursor-pointer select-none hover:bg-accent transition-colors"
    } else {
        "flex w-full items-center rounded-sm px-2 py-1.5 text-sm text-foreground cursor-pointer select-none hover:bg-accent transition-colors"
    };
    rsx! {
        div {
            class: classes,
            onclick: move |_| {
                if let Some(handler) = &on_select {
                    handler.call(value.clone());
                }
                ctx.open.set(false);
            },
            {children}
        }
    }
}
