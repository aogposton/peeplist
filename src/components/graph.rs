use dioxus::prelude::*;
use crate::AppState;
use crate::View;
use crate::types::*;
use crate::api::is_self_entity;
use super::entity::compute_distance;

// You're fixed at the canvas center; every entity gets a node whose target
// orbit radius is driven by compute_distance() (see entity.rs) — inverted
// on purpose (higher distance sits nearer the center, lowest sits farthest)
// per explicit user call, not a literal "distance == pixel distance" map.
// The physics
// (d3-force, vendored locally as assets/d3.v7.min.js — see main.rs) is run
// once per data change via document::eval and the settled positions are
// sent back to Rust, rather than continuously animated: this is a first
// pass, not a live-dragging force graph. Pan/zoom on the other hand IS
// live and Dioxus-native (wheel + drag update a transform on a wrapping
// <g>) — no d3.zoom() involved, d3 is scoped to layout only.
//
// The center "You" node already represents the self entity, so it's
// excluded from the orbiting entities — see api::is_self_entity.
const CANVAS_SIZE: f64 = 600.0;
const CENTER: f64 = CANVAS_SIZE / 2.0;
const MIN_RADIUS: f64 = 70.0;
const MAX_RADIUS: f64 = 260.0;
const MIN_ZOOM: f64 = 0.3;
const MAX_ZOOM: f64 = 3.0;

#[derive(serde::Serialize, Clone)]
struct GraphNodeIn {
    id: String,
    target_radius: f64,
}

#[derive(serde::Deserialize, Clone)]
struct GraphNodeOut {
    id: String,
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
            .filter(|e| !is_self_entity(&e.id))
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
            // Distance is normalized against the current entity set's own min/max
            // rather than a fixed cap — compute_distance() grows unbounded with
            // time and drift, so a fixed cap would leave nearly everyone pinned
            // to MAX_RADIUS. This keeps the closest and farthest entity always
            // spanning the full MIN_RADIUS..MAX_RADIUS range, at the cost of
            // radius no longer being an absolute, session-to-session measure.
            let distances: Vec<(String, f64)> = entity_list.iter()
                .map(|e| (e.id.clone(), compute_distance(e, &moment_list, now)))
                .collect();
            let min_distance = distances.iter().map(|(_, d)| *d).fold(f64::INFINITY, f64::min);
            let max_distance = distances.iter().map(|(_, d)| *d).fold(f64::NEG_INFINITY, f64::max);
            let span = max_distance - min_distance;
            let nodes_in: Vec<GraphNodeIn> = distances.iter().map(|(id, distance)| {
                // Inverted on purpose: highest raw distance value renders closest
                // to center, lowest renders farthest. Per-user call, not a
                // "distance == literal pixel distance" mapping.
                let ratio = if span > f64::EPSILON { (distance - min_distance) / span } else { 0.5 };
                let radius = MAX_RADIUS - ratio * (MAX_RADIUS - MIN_RADIUS);
                GraphNodeIn { id: id.clone(), target_radius: radius }
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
        .filter(|e| !is_self_entity(&e.id))
        .cloned()
        .collect();

    let (pan_x, pan_y) = *pan.read();
    let zoom_val = *zoom.read();

    rsx! {
        div {
            class: "h-full flex flex-col",
            div {
                class: "px-4 pt-4",
                h1 { class: "text-2xl font-semibold text-foreground mb-1", "Graph View" }
                p {
                    class: "text-sm text-muted-foreground mb-4",
                    "Everyone, positioned by relationship distance — closer means more distance. Click a node to open that person. Scroll to zoom, drag to pan."
                }
            }
            div {
                class: "flex-1 min-h-0 flex justify-center px-4 pb-4",
                if entity_lookup.is_empty() {
                    div {
                        class: "text-sm text-muted-foreground text-center py-16",
                        "No entities yet — add someone to see them here."
                    }
                } else {
                svg {
                    width: "100%",
                    height: "100%",
                    view_box: "0 0 {CANVAS_SIZE} {CANVAS_SIZE}",
                    preserve_aspect_ratio: "xMidYMid meet",
                    class: if *dragging.read() {
                        "border border-border rounded-lg bg-background cursor-grabbing select-none"
                    } else {
                        "border border-border rounded-lg bg-background cursor-grab select-none"
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
                        for radius in {
                            let mut radii: Vec<i64> = positions.read().iter()
                                .map(|n| {
                                    let dx = n.x - CENTER;
                                    let dy = n.y - CENTER;
                                    (dx * dx + dy * dy).sqrt().round() as i64
                                })
                                .collect();
                            radii.sort_unstable();
                            radii.dedup();
                            radii
                        } {
                            circle {
                                cx: "{CENTER}",
                                cy: "{CENTER}",
                                r: "{radius}",
                                class: "fill-none stroke-border/40 pointer-events-none",
                                stroke_width: "1",
                            }
                        }
                        for node in positions.read().iter() {
                            {
                                let node_id = node.id.clone();
                                let (nx, ny) = (node.x, node.y);
                                let name = entity_lookup.iter().find(|e| e.id == node_id).map(|e| e.name.clone()).unwrap_or_default();
                                rsx! {
                                    g {
                                        key: "{node_id}",
                                        class: "cursor-pointer",
                                        onclick: {
                                            let node_id = node_id.clone();
                                            move |_| {
                                                if let Some(e) = entities.read().iter().find(|e| e.id == node_id).cloned() {
                                                    current_entity.set(Some(e));
                                                    currentView.set(View::Entity);
                                                }
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
}
