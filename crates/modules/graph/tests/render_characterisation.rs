//! Characterisation of the graph's *rendering*, via a headless frame.
//!
//! PLAN.md WS-D1.3/D1.4 rewrite `vendored/ui.rs` (2 878 lines) and delete
//! `node_view.rs` in favour of `ViewCtx::offscreen`. Neither is safe to
//! attempt against one unit test, and rendering cannot be characterised
//! by asserting on state — so this drives a real egui pass with no
//! window and asserts on the shape of what comes out.
//!
//! The technique is `mara_backend_egui`'s `frame_tests.rs`: build an
//! `egui::Context`, run `begin_pass`/`end_pass` around the widget, and
//! read the tessellated output back. It needs no GPU and no window,
//! which is what makes it usable here — `node_view::show`'s own path
//! cannot be covered this way because it wants a wgpu device.
//!
//! These assert on **invariants a rewrite must preserve**, not on exact
//! geometry: pixel-exact goldens over 2 878 lines of layout would fail
//! on every legitimate change and teach a maintainer to delete them.
//! What is pinned is that nodes paint, that they paint where the model
//! says, that wires appear only for real connections, and that the
//! widget is deterministic across identical passes.

#![allow(deprecated)]

use mara_graph::{Graph, GraphWidget, InPin, NodePin, NodeViewer, OutPin, PinInfo};

use mara_core::MaraUi;
use mara_core::vocab::Pos2;

/// The smallest viewer that renders something: a title and one pin per
/// side. Everything else on `NodeViewer` has a default.
struct MinimalViewer;

/// A node carrying just enough to be identifiable in the output.
struct DemoNode {
    title: &'static str,
}

impl NodeViewer<DemoNode> for MinimalViewer {
    fn title(&mut self, node: &DemoNode) -> String {
        node.title.to_string()
    }

    fn inputs(&mut self, _node: &DemoNode) -> usize {
        1
    }

    fn show_input(
        &mut self,
        _pin: &InPin,
        ui: &mut MaraUi<'_>,
        _graph: &mut Graph<DemoNode>,
    ) -> impl NodePin + 'static {
        ui.label("in");
        PinInfo::circle()
    }

    fn outputs(&mut self, _node: &DemoNode) -> usize {
        1
    }

    fn show_output(
        &mut self,
        _pin: &OutPin,
        ui: &mut MaraUi<'_>,
        _graph: &mut Graph<DemoNode>,
    ) -> impl NodePin + 'static {
        ui.label("out");
        PinInfo::circle()
    }
}

/// One headless pass over `graph`, returning the tessellated shape
/// count and the union of everything painted.
///
/// Two passes are run, not one: egui resolves sizes from the previous
/// frame, so a single pass reports a half-laid-out widget and would
/// make every assertion below depend on first-frame guesswork.
fn render(graph: &mut Graph<DemoNode>) -> RenderSummary {
    render_styled(graph, mara_graph::GraphStyle::new())
}

/// [`render`] with an explicit style, for the background-pattern test.
fn render_styled(graph: &mut Graph<DemoNode>, style: mara_graph::GraphStyle) -> RenderSummary {
    let ctx = egui::Context::default();
    let mut summary = RenderSummary::default();

    for _ in 0..2 {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1024.0, 768.0),
            )),
            ..Default::default()
        });

        egui::CentralPanel::default().show(&ctx, |ui| {
            let _ = GraphWidget::new()
                .style(style.clone())
                .show(graph, &mut MinimalViewer, ui);
        });

        let output = ctx.end_pass();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        summary = RenderSummary::default();
        for prim in &primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                if mesh.indices.is_empty() {
                    continue;
                }
                summary.mesh_count += 1;
                summary.vertex_count += mesh.vertices.len();
                for v in &mesh.vertices {
                    summary.painted = summary.painted.union(egui::Rect::from_min_max(v.pos, v.pos));
                    summary.centroid_sum += v.pos.to_vec2();
                }
            }
        }
    }

    summary
}

/// The same headless frame with no graph in it — the baseline the
/// background-pattern assertion is measured against.
fn render_bare() -> RenderSummary {
    let ctx = egui::Context::default();
    let mut summary = RenderSummary::default();

    for _ in 0..2 {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1024.0, 768.0),
            )),
            ..Default::default()
        });
        egui::CentralPanel::default().show(&ctx, |_ui| {});
        let output = ctx.end_pass();
        let primitives = ctx.tessellate(output.shapes, output.pixels_per_point);

        summary = RenderSummary::default();
        for prim in &primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                if mesh.indices.is_empty() {
                    continue;
                }
                summary.mesh_count += 1;
                summary.vertex_count += mesh.vertices.len();
            }
        }
    }

    summary
}

#[derive(Debug, PartialEq)]
struct RenderSummary {
    mesh_count: usize,
    vertex_count: usize,
    painted: egui::Rect,
    centroid_sum: egui::Vec2,
}

impl Default for RenderSummary {
    fn default() -> Self {
        Self {
            mesh_count: 0,
            vertex_count: 0,
            painted: egui::Rect::NOTHING,
            centroid_sum: egui::Vec2::ZERO,
        }
    }
}

impl RenderSummary {
    fn is_empty(&self) -> bool {
        self.mesh_count == 0
    }

    /// Mean vertex position. The graph paints a full-canvas background
    /// pattern, so the *bounding box* of everything painted is always
    /// the whole viewport and cannot locate a node — the centroid can,
    /// because a node's own geometry pulls the mean toward it.
    fn centroid(&self) -> egui::Vec2 {
        if self.vertex_count == 0 {
            egui::Vec2::ZERO
        } else {
            self.centroid_sum / self.vertex_count as f32
        }
    }
}

/// An empty graph canvas paints more than a bare panel does.
///
/// Compared against a frame containing no graph at all, because
/// `!is_empty()` alone is no evidence — the panel fill satisfies that
/// on its own.
///
/// Note the default style has **`bg_pattern: None`**, so this is *not*
/// covering the background pattern; `the_background_pattern_is_drawn`
/// does that with a pattern actually set. Found by mutation: deleting
/// the pattern changed nothing here, because the fixture never asked
/// for one.
#[test]
fn an_empty_graph_canvas_paints_more_than_a_bare_panel() {
    let mut empty = Graph::<DemoNode>::new();

    let with_graph = render(&mut empty);
    let without_graph = render_bare();

    assert!(!with_graph.is_empty());
    assert!(
        with_graph.vertex_count > without_graph.vertex_count,
        "the graph canvas is geometry: with graph={} bare panel={}",
        with_graph.vertex_count,
        without_graph.vertex_count
    );
}

/// The background pattern reaches the tessellator when one is set.
///
/// `GraphStyle::new()` leaves `bg_pattern` at `None`, so this is the
/// only test that exercises `NodeViewer::draw_background`'s painting
/// path at all. Mutation-verified: forcing the pattern argument to
/// `None` inside the renderer fails this and nothing else.
#[test]
fn the_background_pattern_is_drawn() {
    let mut plain = Graph::<DemoNode>::new();
    let mut patterned = Graph::<DemoNode>::new();

    let without = render_styled(&mut plain, mara_graph::GraphStyle::new()).vertex_count;

    let mut style = mara_graph::GraphStyle::new();
    style.bg_pattern = Some(mara_graph::BackgroundPattern::Grid(
        mara_graph::Grid::default(),
    ));
    let with = render_styled(&mut patterned, style).vertex_count;

    assert!(
        with > without,
        "a grid background is geometry: no pattern={without} grid={with}"
    );
}

#[test]
fn adding_a_node_to_an_empty_graph_adds_geometry() {
    let mut empty = Graph::<DemoNode>::new();
    let mut one = Graph::<DemoNode>::new();
    one.insert_node(Pos2::new(100.0, 100.0), DemoNode { title: "alpha" });

    let empty_out = render(&mut empty);
    let one_out = render(&mut one);

    assert!(
        one_out.vertex_count > empty_out.vertex_count,
        "adding a node must add geometry: empty={} one node={}",
        empty_out.vertex_count,
        one_out.vertex_count
    );
}

#[test]
fn each_added_node_adds_geometry() {
    let mut graph = Graph::<DemoNode>::new();
    graph.insert_node(Pos2::new(60.0, 60.0), DemoNode { title: "alpha" });
    let one = render(&mut graph).vertex_count;

    graph.insert_node(Pos2::new(300.0, 60.0), DemoNode { title: "beta" });
    let two = render(&mut graph).vertex_count;

    graph.insert_node(Pos2::new(540.0, 60.0), DemoNode { title: "gamma" });
    let three = render(&mut graph).vertex_count;

    assert!(
        one < two && two < three,
        "vertex count must grow with node count: {one} < {two} < {three}"
    );
}

/// Relative node positions reach the renderer.
///
/// Asserted through the centroid rather than the bounding box: the
/// graph paints a full-canvas background pattern, so the bounds of
/// everything painted are always the whole viewport. Measured, not
/// assumed — an earlier version of this test compared bounding boxes
/// and could not tell the two layouts apart.
#[test]
fn relative_node_positions_reach_the_renderer() {
    let mut close = Graph::<DemoNode>::new();
    close.insert_node(Pos2::new(0.0, 0.0), DemoNode { title: "alpha" });
    close.insert_node(Pos2::new(20.0, 0.0), DemoNode { title: "beta" });

    let mut spread = Graph::<DemoNode>::new();
    spread.insert_node(Pos2::new(0.0, 0.0), DemoNode { title: "alpha" });
    spread.insert_node(Pos2::new(600.0, 400.0), DemoNode { title: "beta" });

    let close_c = render(&mut close).centroid();
    let spread_c = render(&mut spread).centroid();

    assert!(
        (close_c - spread_c).length() > 1.0,
        "moving one node 600x400 must change what is painted: \
         close={close_c:?} spread={spread_c:?}"
    );
}

/// Geometry outside the viewport is culled rather than tessellated.
///
/// Counter-intuitive and therefore worth pinning: spreading nodes out
/// *reduces* the vertex count, because the parts that leave the
/// viewport stop being emitted. A rewrite that tessellates the whole
/// graph and relies on the GPU to clip would pass every other test
/// here and fail this one — and would scale with graph size instead of
/// with screen size.
#[test]
fn offscreen_geometry_is_culled() {
    let mut onscreen = Graph::<DemoNode>::new();
    onscreen.insert_node(Pos2::new(0.0, 0.0), DemoNode { title: "alpha" });
    onscreen.insert_node(Pos2::new(20.0, 0.0), DemoNode { title: "beta" });

    let mut offscreen = Graph::<DemoNode>::new();
    offscreen.insert_node(Pos2::new(0.0, 0.0), DemoNode { title: "alpha" });
    offscreen.insert_node(Pos2::new(9000.0, 9000.0), DemoNode { title: "beta" });

    let on = render(&mut onscreen).vertex_count;
    let off = render(&mut offscreen).vertex_count;

    assert!(
        off < on,
        "a node pushed far off-viewport must stop contributing geometry: \
         onscreen={on} offscreen={off}"
    );
}

#[test]
fn connecting_two_nodes_paints_a_wire() {
    let mut graph = Graph::<DemoNode>::new();
    let a = graph.insert_node(Pos2::new(60.0, 60.0), DemoNode { title: "alpha" });
    let b = graph.insert_node(Pos2::new(400.0, 60.0), DemoNode { title: "beta" });

    let disconnected = render(&mut graph).vertex_count;

    let connected_ok = graph.connect(
        mara_graph::OutPinId { node: a, output: 0 },
        mara_graph::InPinId { node: b, input: 0 },
    );
    assert!(connected_ok, "the fixture's pins must actually connect");

    let connected = render(&mut graph).vertex_count;

    assert!(
        connected > disconnected,
        "a wire is geometry: disconnected={disconnected} connected={connected}"
    );
}

#[test]
fn rendering_is_deterministic_across_identical_passes() {
    let mut graph = Graph::<DemoNode>::new();
    graph.insert_node(Pos2::new(60.0, 60.0), DemoNode { title: "alpha" });
    graph.insert_node(Pos2::new(400.0, 200.0), DemoNode { title: "beta" });

    let first = render(&mut graph);
    let second = render(&mut graph);

    assert_eq!(
        first.vertex_count, second.vertex_count,
        "same model, same frame size — the output must not drift"
    );
    assert_eq!(first.mesh_count, second.mesh_count);
}

/// Removing a node must remove its geometry, not merely stop updating
/// it. A rewrite that caches per-node meshes and forgets to evict is
/// the failure this catches.
#[test]
fn removing_a_node_removes_its_geometry() {
    let mut graph = Graph::<DemoNode>::new();
    let a = graph.insert_node(Pos2::new(60.0, 60.0), DemoNode { title: "alpha" });
    graph.insert_node(Pos2::new(400.0, 60.0), DemoNode { title: "beta" });

    let both = render(&mut graph).vertex_count;
    graph.remove_node(a);
    let one = render(&mut graph).vertex_count;

    assert!(
        one < both,
        "removing a node must shrink the output: both={both} one={one}"
    );
}
