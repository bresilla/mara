//! Prove that gestures reach the graph.
//!
//! Snapshots check what the renderer draws; nothing checked that a drag
//! ever arrived. It did not: the view registered its interaction and
//! then `canvas_at` registered a second one over the same rect, and the
//! later registration won. Dragging a node and panning the canvas were
//! both dead, and every geometry test still passed.
//!
//! These run by default. They are the regression net for that.

#![cfg(test)]

mod raster;

use mara::extras::graph::render::{Camera, GraphSpec, GraphView, GraphViewState, NodeShape, show_graph};
use mara::extras::graph::{Graph, NodeId};
use mara::ui::mara_core;
use mara_core::vocab::{Color32, Pos2, Rect, Vec2};

struct Node {
    title: &'static str,
    ins: usize,
    outs: usize,
}

struct View;

impl GraphView<Node> for View {
    fn shape(&mut self, id: NodeId, g: &Graph<Node>) -> NodeShape {
        let n = g.get_node(id).expect("node");
        NodeShape {
            title: n.title.to_string(),
            inputs: (0..n.ins).map(|i| format!("in{i}")).collect(),
            outputs: (0..n.outs).map(|i| format!("out{i}")).collect(),
            ..Default::default()
        }
    }
}

fn area() -> Rect {
    Rect::from_min_size(
        Pos2::new(0.0, 0.0),
        Vec2::new(raster::W as f32, raster::H as f32),
    )
}

fn spec() -> GraphSpec {
    GraphSpec::from_surface(
        Color32::from_gray(30),
        Color32::from_rgb(120, 160, 220),
        true,
    )
}

/// A graph with one node whose top-left is at a known screen position,
/// with the camera pinned so screen and graph space differ by a plain
/// offset.
fn one_node() -> (Graph<Node>, NodeId, GraphViewState) {
    let mut g = Graph::new();
    let n = g.insert_node(
        Pos2::new(100.0, 100.0),
        Node {
            title: "Node",
            ins: 1,
            outs: 1,
        },
    );
    let state = GraphViewState::at(Camera {
        pan: Vec2::new(0.0, 0.0),
        zoom: 1.0,
    });
    (g, n, state)
}

/// Press inside a node's header and drag: the node must follow.
#[test]
fn dragging_a_node_moves_it() {
    let (mut g, id, mut state) = one_node();
    let mut view = View;
    let spec = spec();
    let before = g.get_node_info(id).expect("node").pos;

    // Inside the node, clear of any pin: 60pt right and 15pt down from
    // its top-left puts the pointer in the header band.
    let start = egui::pos2(before.x + 60.0, before.y + 15.0);
    let frames = vec![
        raster::pointer_frame(start, false, false),
        raster::pointer_frame(start, true, true),
        raster::pointer_frame(egui::pos2(start.x + 40.0, start.y + 25.0), true, false),
        raster::pointer_frame(egui::pos2(start.x + 80.0, start.y + 50.0), true, false),
    ];
    raster::drive(frames, |ui| {
        show_graph(ui, area(), &mut g, &mut view, &mut state, &spec);
    });

    let after = g.get_node_info(id).expect("node").pos;
    let moved = (after.x - before.x, after.y - before.y);
    assert!(
        moved.0 > 30.0 && moved.1 > 20.0,
        "node did not follow the drag: moved by {moved:?}"
    );
}

/// Press on empty canvas and drag: the camera must follow.
#[test]
fn dragging_the_canvas_pans_the_view() {
    let (mut g, _, mut state) = one_node();
    let mut view = View;
    let spec = spec();
    let before = state.camera.pan;

    // Far from the only node.
    let start = egui::pos2(900.0, 700.0);
    let frames = vec![
        raster::pointer_frame(start, false, false),
        raster::pointer_frame(start, true, true),
        raster::pointer_frame(egui::pos2(start.x - 60.0, start.y + 30.0), true, false),
        raster::pointer_frame(egui::pos2(start.x - 120.0, start.y + 60.0), true, false),
    ];
    raster::drive(frames, |ui| {
        show_graph(ui, area(), &mut g, &mut view, &mut state, &spec);
    });

    let d = (state.camera.pan.x - before.x, state.camera.pan.y - before.y);
    assert!(
        d.0 < -40.0 && d.1 > 20.0,
        "canvas did not pan with the drag: pan moved by {d:?}"
    );
}

/// Panning must not drag a node along with it.
#[test]
fn panning_leaves_the_nodes_where_they_were() {
    let (mut g, id, mut state) = one_node();
    let mut view = View;
    let spec = spec();
    let before = g.get_node_info(id).expect("node").pos;

    let start = egui::pos2(900.0, 700.0);
    let frames = vec![
        raster::pointer_frame(start, false, false),
        raster::pointer_frame(start, true, true),
        raster::pointer_frame(egui::pos2(start.x - 100.0, start.y), true, false),
    ];
    raster::drive(frames, |ui| {
        show_graph(ui, area(), &mut g, &mut view, &mut state, &spec);
    });

    let after = g.get_node_info(id).expect("node").pos;
    assert!((after.x - before.x).abs() < 0.01 && (after.y - before.y).abs() < 0.01);
}

/// The view only takes input inside the rect it was handed.
///
/// It registers ONE interaction over its whole area, so a host that
/// hands it more than the graph's own viewport hands it every control
/// in that strip too — which is how the Graph Lab's graph ended up
/// eating its own shelf's fold buttons.
#[test]
fn the_view_ignores_pointers_outside_its_area() {
    let (mut g, id, mut state) = one_node();
    let mut view = View;
    let spec = spec();
    let before = g.get_node_info(id).expect("node").pos;

    // A viewport that stops well short of the window, as a shelf
    // layout's does.
    let viewport = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(600.0, 400.0));
    let outside = egui::pos2(1200.0, 800.0);
    let frames = vec![
        raster::pointer_frame(outside, false, false),
        raster::pointer_frame(outside, true, true),
        raster::pointer_frame(egui::pos2(outside.x - 150.0, outside.y), true, false),
    ];
    let pan_before = state.camera.pan;
    raster::drive(frames, |ui| {
        show_graph(ui, viewport, &mut g, &mut view, &mut state, &spec);
    });

    assert_eq!(
        state.camera.pan, pan_before,
        "a drag outside the graph's viewport must not pan it"
    );
    let after = g.get_node_info(id).expect("node").pos;
    assert!((after.x - before.x).abs() < 0.01 && (after.y - before.y).abs() < 0.01);
}

/// Clicking a node selects it; clicking empty canvas clears.
#[test]
fn clicking_selects_and_deselects() {
    let (mut g, id, mut state) = one_node();
    let mut view = View;
    let spec = spec();
    let at = egui::pos2(160.0, 115.0);

    raster::drive(
        vec![
            raster::pointer_frame(at, false, false),
            raster::pointer_frame(at, true, true),
            raster::pointer_frame(at, false, true),
        ],
        |ui| {
            show_graph(ui, area(), &mut g, &mut view, &mut state, &spec);
        },
    );
    assert!(
        state.selection.contains(&id),
        "clicking a node did not select it"
    );

    let empty = egui::pos2(900.0, 700.0);
    raster::drive(
        vec![
            raster::pointer_frame(empty, false, false),
            raster::pointer_frame(empty, true, true),
            raster::pointer_frame(empty, false, true),
        ],
        |ui| {
            show_graph(ui, area(), &mut g, &mut view, &mut state, &spec);
        },
    );
    assert!(
        state.selection.is_empty(),
        "clicking empty canvas did not clear the selection"
    );
}
