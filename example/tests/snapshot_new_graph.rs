//! Look at the rebuilt node renderer.
//!
//! Ignored by default — it writes a file and exists to be run by hand:
//! `cargo test -p mara_example --test snapshot_new_graph -- --ignored`

#![cfg(test)]

mod raster;

use mara::extras::graph::render::{
    Camera, GraphSpec, GraphView, GraphViewState, NodeShape, show_graph,
};
use mara::extras::graph::{Graph, InPinId, NodeId, OutPinId};
use mara::ui::mara_core;
use mara_core::vocab::{Color32, Pos2, Rect, Vec2};

/// A node in the sample graph: enough variety to expose the failures
/// the previous renderer had — long titles, lopsided pin counts, a node
/// with a reserved body, and several colour families at once.
struct Demo {
    title: &'static str,
    inputs: &'static [&'static str],
    outputs: &'static [&'static str],
    tint: Color32,
    body_h: f32,
}

const SKY: Color32 = Color32::from_rgb(58, 104, 158);
const MOSS: Color32 = Color32::from_rgb(66, 124, 88);
const RUST: Color32 = Color32::from_rgb(150, 88, 54);
const PLUM: Color32 = Color32::from_rgb(108, 74, 140);

fn sample() -> Graph<Demo> {
    let mut g = Graph::new();
    let load = g.insert_node(
        Pos2::new(0.0, 0.0),
        Demo {
            title: "Load Dataset",
            inputs: &[],
            outputs: &["images", "labels"],
            tint: SKY,
            body_h: 0.0,
        },
    );
    let augment = g.insert_node(
        Pos2::new(260.0, -40.0),
        Demo {
            title: "Augment",
            inputs: &["images"],
            outputs: &["out"],
            tint: MOSS,
            body_h: 0.0,
        },
    );
    let infer = g.insert_node(
        Pos2::new(520.0, 20.0),
        Demo {
            title: "Segmentation Model (very long name)",
            inputs: &["image", "weights", "mask"],
            outputs: &["logits"],
            tint: PLUM,
            body_h: 0.0,
        },
    );
    let preview = g.insert_node(
        Pos2::new(800.0, 100.0),
        Demo {
            title: "Preview",
            inputs: &["image"],
            outputs: &[],
            tint: RUST,
            body_h: 90.0,
        },
    );
    let weights = g.insert_node(
        Pos2::new(260.0, 190.0),
        Demo {
            title: "Weights",
            inputs: &[],
            outputs: &["w"],
            tint: SKY,
            body_h: 0.0,
        },
    );

    g.connect(
        OutPinId {
            node: load,
            output: 0,
        },
        InPinId {
            node: augment,
            input: 0,
        },
    );
    g.connect(
        OutPinId {
            node: augment,
            output: 0,
        },
        InPinId {
            node: infer,
            input: 0,
        },
    );
    g.connect(
        OutPinId {
            node: weights,
            output: 0,
        },
        InPinId {
            node: infer,
            input: 1,
        },
    );
    g.connect(
        OutPinId {
            node: infer,
            output: 0,
        },
        InPinId {
            node: preview,
            input: 0,
        },
    );
    g
}

struct View;

impl GraphView<Demo> for View {
    fn shape(&mut self, id: NodeId, g: &Graph<Demo>) -> NodeShape {
        let v = g.get_node(id).expect("node exists");
        NodeShape {
            title: v.title.to_string(),
            inputs: v.inputs.iter().map(|s| (*s).to_string()).collect(),
            outputs: v.outputs.iter().map(|s| (*s).to_string()).collect(),
            body_h: v.body_h,
        }
    }

    fn tint(&mut self, id: NodeId, g: &Graph<Demo>) -> Option<Color32> {
        g.get_node(id).map(|v| v.tint)
    }

    fn output_color(&mut self, pin: OutPinId, g: &Graph<Demo>) -> Option<Color32> {
        g.get_node(pin.node).map(|n| n.tint)
    }

    fn body(&mut self, _id: NodeId, _rect: Rect, _ui: &mut mara_core::MaraUi<'_>) {}
}

#[test]
#[ignore = "writes a PNG; run by hand to look at the result"]
fn snapshot_the_rebuilt_graph() {
    let accent = Color32::from_rgb(120, 160, 220);
    let mut graph = sample();
    let mut view = View;
    let zoom: f32 = std::env::var("MARA_SNAP_ZOOM")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0);
    let mut state = if zoom > 0.0 {
        GraphViewState::at(Camera {
            pan: Vec2::new(120.0, 260.0),
            zoom,
        })
    } else {
        GraphViewState::default()
    };
    state.selection.insert(NodeId(2));
    let spec = GraphSpec::from_surface(Color32::from_gray(30), accent, true);

    let path = std::env::var("MARA_SNAPSHOT")
        .unwrap_or_else(|_| "/tmp/mara_new_graph.png".to_string());
    raster::snapshot(&path, accent, |ui| {
        let area = Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(raster::W as f32, raster::H as f32),
        );
        show_graph(ui, area, &mut graph, &mut view, &mut state, &spec);
    });
}
