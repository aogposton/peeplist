use dioxus::prelude::*;
use crate::AppState;
use crate::View;
use crate::types::*;
use super::entity::compute_distance;

// You're fixed at the canvas center; every entity gets a node whose target
// orbit radius is driven by compute_distance() (see entity.rs) — closer
// relationships (lower distance) sit nearer the center. The physics
// (d3-force, vendored locally as assets/d3.v7.min.js — see main.rs) is run
// once per data change via document::eval and the settled positions are
// sent back to Rust, rather than continuously animated: this is a first
// pass, not a live-dragging force graph. Pan/zoom on the other hand IS
// live and Dioxus-native (wheel + drag update a transform on a wrapping
// <g>) — no d3.zoom() involved, d3 is scoped to layout only.
//
// Entity id 0 is reserved to always mean "yourself" (see memory
// project_self_entity_convention) — the center "You" node already
// represents that, so it's excluded from the orbiting entities.
const CANVAS_SIZE: f64 = 600.0;
const CENTER: f64 = CANVAS_SIZE / 2.0;
const MIN_RADIUS: f64 = 70.0;
const MAX_RADIUS: f64 = 260.0;
const DISTANCE_CAP: f64 = 8.0;
const MIN_ZOOM: f64 = 0.3;
const MAX_ZOOM: f64 = 3.0;
const SELF_ENTITY_ID: i64 = 0;

#[derive(serde::Serialize, Clone)]
struct GraphNodeIn {
    id: i64,
    target_radius: f64,
}

#[derive(serde::Deserialize, Clone)]
struct GraphNodeOut {
    id: i64,
    x: f64,
    y: f64,
}

const LAYOUT_SCRIPT: &str = r#"
    const nodes = await dioxus.recv();
    const width = 600, height = 600, centerX = width / 2, centerY = height / 2;
    nodes.forEach((n) => {
        const angle = Math.random() * 2 * Math.PI;
        n.x = centerX + Math.cos(angle) * n.target_radius;
        n.y = centerY + Math.sin(angle) * n.target_radius;
    });
    const simulation = d3.forceSimulation(nodes)
        .force("radial", d3.forceRadial((n) => n.target_radius, centerX, centerY).strength(0.9))
        .force("charge", d3.forceManyBody().strength(-40))
        .force("collide", d3.forceCollide(30))
        .stop();
    for (let i = 0; i < 300; i++) {
        simulation.tick();
    }
    dioxus.send(nodes.map((n) => ({ id: n.id, x: n.x, y: n.y })));
"#;

#[component]
pub fn GraphViewCmp() -> Element {
    let state = use_context::<AppState>();
    let entities = state.entities;
    let moments = state.moments;
    let mut current_entity = state.current_entity;
    let mut currentView = state.currentView;

    let mut positions = use_signal(Vec::<GraphNodeOut>::new);
    let mut computing = use_signal(|| false);

    let mut zoom = use_signal(|| 1.0f64);
    let mut pan = use_signal(|| (0.0f64, 0.0f64));
    let mut dragging = use_signal(|| false);
    let mut drag_start = use_signal(|| (0.0f64, 0.0f64));
    let mut pan_start = use_signal(|| (0.0f64, 0.0f64));

    use_effect(move || {
        let entity_list: Vec<EntityType> = entities.read().iter()
            .filter(|e| e.id != SELF_ENTITY_ID)
            .cloned()
            .collect();
        let moment_list = moments.read().clone();

        if entity_list.is_empty() {
            positions.set(vec![]);
            return;
        }

        computing.set(true);
        spawn(async move {
            let now = chrono::Utc::now();
            let nodes_in: Vec<GraphNodeIn> = entity_list.iter().map(|e| {
                let distance = compute_distance(e, &moment_list, now);
                let radius = MIN_RADIUS + distance.min(DISTANCE_CAP) / DISTANCE_CAP * (MAX_RADIUS - MIN_RADIUS);
                GraphNodeIn { id: e.id, target_radius: radius }
            }).collect();

            let eval = document::eval(LAYOUT_SCRIPT);
            if eval.send(nodes_in).is_ok() {
                let mut eval = eval;
                if let Ok(result) = eval.recv::<Vec<GraphNodeOut>>().await {
                    positions.set(result);
                }
            }
            computing.set(false);
        });
    });

    let entity_lookup: Vec<EntityType> = entities.read().iter()
        .filter(|e| e.id != SELF_ENTITY_ID)
        .cloned()
        .collect();

    let (pan_x, pan_y) = *pan.read();
    let zoom_val = *zoom.read();

    rsx! {
        div {
            class: "px-4 pt-4",
            h1 { class: "text-2xl font-semibold text-foreground mb-1", "Graph View" }
            p {
                class: "text-sm text-muted-foreground mb-4",
                "Everyone, positioned by relationship distance — closer means less distance. Click a node to open that person. Scroll to zoom, drag to pan."
            }
        }
        div {
            class: "flex justify-center px-4 pb-8",
            if entity_lookup.is_empty() {
                div {
                    class: "text-sm text-muted-foreground text-center py-16",
                    "No entities yet — add someone to see them here."
                }
            } else {
                svg {
                    width: "{CANVAS_SIZE}",
                    height: "{CANVAS_SIZE}",
                    view_box: "0 0 {CANVAS_SIZE} {CANVAS_SIZE}",
                    class: if *dragging.read() {
                        "border border-border rounded-lg bg-background max-w-full cursor-grabbing select-none"
                    } else {
                        "border border-border rounded-lg bg-background max-w-full cursor-grab select-none"
                    },
                    onwheel: move |e: WheelEvent| {
                        e.prevent_default();
                        let dy = e.data().delta().strip_units().y;
                        let factor = if dy < 0.0 { 1.1 } else { 0.9 };
                        let current = *zoom.read();
                        zoom.set((current * factor).clamp(MIN_ZOOM, MAX_ZOOM));
                    },
                    onmousedown: move |e: MouseEvent| {
                        let coords = e.client_coordinates();
                        dragging.set(true);
                        drag_start.set((coords.x, coords.y));
                        pan_start.set(*pan.read());
                    },
                    onmousemove: move |e: MouseEvent| {
                        if *dragging.read() {
                            let coords = e.client_coordinates();
                            let (sx, sy) = *drag_start.read();
                            let (px, py) = *pan_start.read();
                            pan.set((px + (coords.x - sx), py + (coords.y - sy)));
                        }
                    },
                    onmouseup: move |_| dragging.set(false),
                    onmouseleave: move |_| dragging.set(false),
                    g {
                        transform: "translate({pan_x}, {pan_y}) scale({zoom_val})",
                        circle {
                            cx: "{CENTER}",
                            cy: "{CENTER}",
                            r: "24",
                            class: "fill-primary",
                        }
                        text {
                            x: "{CENTER}",
                            y: "{CENTER + 5.0}",
                            text_anchor: "middle",
                            class: "text-xs fill-primary-foreground font-semibold pointer-events-none",
                            "You"
                        }
                        for node in positions.read().iter() {
                            {
                                let node_id = node.id;
                                let (nx, ny) = (node.x, node.y);
                                let name = entity_lookup.iter().find(|e| e.id == node_id).map(|e| e.name.clone()).unwrap_or_default();
                                rsx! {
                                    g {
                                        key: "{node_id}",
                                        class: "cursor-pointer",
                                        onclick: move |_| {
                                            if let Some(e) = entities.read().iter().find(|e| e.id == node_id).cloned() {
                                                current_entity.set(Some(e));
                                                currentView.set(View::Entity);
                                            }
                                        },
                                        circle {
                                            cx: "{nx}",
                                            cy: "{ny}",
                                            r: "18",
                                            class: "fill-muted stroke-border hover:fill-accent transition-colors",
                                            stroke_width: "1.5",
                                        }
                                        text {
                                            x: "{nx}",
                                            y: "{ny + 32.0}",
                                            text_anchor: "middle",
                                            class: "text-xs fill-foreground pointer-events-none",
                                            "{name}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
