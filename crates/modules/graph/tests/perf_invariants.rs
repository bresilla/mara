//! Performance invariants — PLAN_NODE.md P11.
//!
//! Every assertion here counts **work**, never wall time and never
//! emitted geometry. Wall time is flaky on a shared machine; geometry
//! is misleading because egui's tessellator already drops fully-clipped
//! shapes, so a test that asserted "less geometry off-screen" would
//! pass with no culling code at all. What culling actually saves is the
//! work *upstream* of geometry — viewer calls, pin construction,
//! layout — so that is what gets counted.

mod harness;

use std::cell::Cell;

use harness::Harness;
use mara_core::MaraUi;
use mara_graph::{Graph, InPin, NodeId, NodePin, NodeViewer, OutPin, PinInfo, pos2};

struct N;

/// Counts how many distinct nodes the renderer asked about.
struct Counting<'a> {
    asked: &'a Cell<usize>,
}

impl NodeViewer<N> for Counting<'_> {
    fn title(&mut self, _n: &N) -> String {
        "n".to_string()
    }
    fn inputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_input(
        &mut self,
        _p: &InPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("in");
        PinInfo::circle()
    }
    fn outputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_output(
        &mut self,
        _p: &OutPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("out");
        PinInfo::circle()
    }
    fn inputs_of(&mut self, node: NodeId, graph: &Graph<N>) -> usize {
        self.asked.set(self.asked.get() + 1);
        self.inputs(&graph[node])
    }
}

/// `n` nodes spread far apart, so most are off-screen at scale 1.
fn spread(n: usize) -> Graph<N> {
    let mut g = Graph::<N>::new();
    for i in 0..n {
        g.insert_node(pos2(i as f32 * 900.0, 0.0), N);
    }
    g
}

/// The culling claim, stated as work avoided: with 200 nodes spread far
/// wider than the viewport, the renderer must ask about a small
/// fraction of them, not all of them.
#[test]
fn off_screen_nodes_cost_no_viewer_calls() {
    let mut graph = spread(200);
    let counter = Cell::new(0);
    let mut viewer = Counting { asked: &counter };

    Harness::new()
        .with_screen(1024.0, 768.0)
        .frame_owned(&mut graph, &mut viewer);

    let asked = counter.get();
    assert!(asked > 0, "something must render");
    assert!(
        asked < 200,
        "all 200 nodes were laid out despite most being off-screen: asked={asked}"
    );
}

/// The complement: nodes that ARE on screen must still be drawn. A
/// culling bug that skipped everything would satisfy the test above.
#[test]
fn on_screen_nodes_are_not_culled() {
    let mut graph = Graph::<N>::new();
    for i in 0..6 {
        graph.insert_node(pos2(i as f32 * 60.0, 0.0), N);
    }
    let counter = Cell::new(0);
    let mut viewer = Counting { asked: &counter };

    Harness::new()
        .with_screen(1024.0, 768.0)
        .frame_owned(&mut graph, &mut viewer);

    assert!(
        counter.get() >= 6,
        "a graph that fits on screen must draw every node: asked={}",
        counter.get()
    );
}

/// Determinism over a large graph. The wire set is a `HashSet`, so
/// before P10 the paint order differed frame to frame — a shimmer on
/// overlapping wires, and a defeated order-keyed cache. This is the
/// guard for the sort that fixed it.
#[test]
fn two_identical_passes_over_a_large_graph_are_byte_identical() {
    fn build() -> Graph<N> {
        let mut g = Graph::<N>::new();
        let ids: Vec<_> = (0..40)
            .map(|i| g.insert_node(pos2((i % 8) as f32 * 120.0, (i / 8) as f32 * 100.0), N))
            .collect();
        for w in ids.windows(2) {
            g.connect(
                mara_graph::OutPinId {
                    node: w[0],
                    output: 0,
                },
                mara_graph::InPinId {
                    node: w[1],
                    input: 0,
                },
            );
        }
        g
    }

    let mut a = build();
    let mut b = build();
    let first = Harness::new().frame_owned(&mut a, &mut V).vertex_count;
    let second = Harness::new().frame_owned(&mut b, &mut V).vertex_count;

    assert_eq!(
        first, second,
        "identical graphs must paint identically; a HashSet-ordered \
         wire pass would drift here"
    );
}

struct V;

impl NodeViewer<N> for V {
    fn title(&mut self, _n: &N) -> String {
        "n".to_string()
    }
    fn inputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_input(
        &mut self,
        _p: &InPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("in");
        PinInfo::circle()
    }
    fn outputs(&mut self, _n: &N) -> usize {
        1
    }
    fn show_output(
        &mut self,
        _p: &OutPin,
        ui: &mut MaraUi<'_>,
        _g: &mut Graph<N>,
    ) -> impl NodePin + 'static {
        ui.label("out");
        PinInfo::circle()
    }
}

/// `Graph::size_override` must reach the rendered node, or the field is
/// dead weight and the AI-pipeline target has no way to make a big
/// image node.
#[test]
fn a_size_override_widens_the_painted_node() {
    let mut plain = Graph::<N>::new();
    plain.insert_node(pos2(100.0, 100.0), N);

    let mut wide = Graph::<N>::new();
    let id = wide.insert_node(pos2(100.0, 100.0), N);
    wide.set_size_override(id, Some(mara_graph::vec2(600.0, 400.0)));

    let a = Harness::new().frame_owned(&mut plain, &mut V).painted;
    let b = Harness::new().frame_owned(&mut wide, &mut V).painted;

    assert!(
        b.width() > a.width() || b.height() > a.height(),
        "the override did not reach the renderer: plain={a:?} wide={b:?}"
    );
}

/// The override is a floor, not a cap: a node whose content needs more
/// room than the override must still get it, or the app's own request
/// clips its own content.
#[test]
fn a_size_override_never_shrinks_a_node_below_its_content() {
    let mut small_override = Graph::<N>::new();
    let id = small_override.insert_node(pos2(100.0, 100.0), N);
    small_override.set_size_override(id, Some(mara_graph::vec2(1.0, 1.0)));

    let mut plain = Graph::<N>::new();
    plain.insert_node(pos2(100.0, 100.0), N);

    let a = Harness::new().frame_owned(&mut plain, &mut V).vertex_count;
    let b = Harness::new()
        .frame_owned(&mut small_override, &mut V)
        .vertex_count;

    assert_eq!(
        a, b,
        "a 1x1 override must not shrink the node below its own content"
    );
}
