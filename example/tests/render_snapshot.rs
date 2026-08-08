//! Look at the demo's *own* graph — the real `DemoViewer` over the
//! real `default_graph()` — rather than a stub.
//!
//! A stub gets the node frame and the header band right and the titles,
//! pin colours and node mix wrong, which is most of what a reviewer is
//! trying to see.
//!
//! Ignored by default — it writes a file and exists to be run by hand:
//! `cargo test -p mara_example --test render_snapshot -- --ignored`

#![cfg(test)]

mod raster;

use mara::extras::graph::render::{GraphSpec, GraphViewState, show_graph};
use mara::ui::mara_core;
use mara_core::vocab::{Color32, Pos2, Rect, Vec2};
use mara_example::app::{DemoViewer, build_graph_lab_doc, default_graph};

#[test]
#[ignore = "writes a PNG; run by hand to look at the result"]
fn snapshot_the_editor_graph() {
    let accent = Color32::from_rgb(120, 160, 220);
    let mut graph = default_graph();
    let mut viewer = DemoViewer::for_graph(1.75);
    let mut state = GraphViewState::default();
    let spec = GraphSpec::from_surface(Color32::from_gray(30), accent, true);

    let path =
        std::env::var("MARA_SNAPSHOT").unwrap_or_else(|_| "/tmp/mara_demo_graph.png".to_string());
    raster::snapshot(&path, accent, |ui| {
        let area = Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(raster::W as f32, raster::H as f32),
        );
        show_graph(ui, area, &mut graph, &mut viewer, &mut state, &spec);
    });
}

/// The Graph Lab document, which is where the grouping features live:
/// frames, a subgraph placed twice, and boundary ports.
#[test]
#[ignore = "writes a PNG; run by hand to look at the result"]
fn snapshot_the_graph_lab() {
    use mara::extras::graph::render::{DocViewState, show_doc};

    let accent = Color32::from_rgb(120, 160, 220);
    let mut doc = build_graph_lab_doc();
    let mut viewer = DemoViewer::for_graph(1.75);
    let mut nav = DocViewState::default();
    let spec = GraphSpec::from_surface(Color32::from_gray(30), accent, true);

    let path =
        std::env::var("MARA_SNAPSHOT").unwrap_or_else(|_| "/tmp/mara_graph_lab.png".to_string());
    raster::snapshot(&path, accent, |ui| {
        let area = Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(raster::W as f32, raster::H as f32),
        );
        show_doc(ui, area, &mut doc, &mut viewer, &mut nav, &spec);
    });
}
