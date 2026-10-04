//! The input harness, tested before anything depends on it —
//! PLAN_NODE.md P3b.
//!
//! A test harness that silently fails to deliver input is worse than no
//! harness: every behavioural test built on it passes vacuously. So the
//! harness proves it can move a pointer, land a drag, and produce a
//! double click *first*, against behaviour the widget already has.

mod harness;

use harness::Harness;
use mara_core::MaraUi;
use mara_graph::{Graph, GraphState, InPin, NodePin, NodeViewer, OutPin, PinInfo, Pos2, pos2};

struct DemoNode {
    title: &'static str,
}

struct Viewer;

impl NodeViewer<DemoNode> for Viewer {
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

fn one_node() -> (Graph<DemoNode>, Pos2) {
    let at = pos2(300.0, 200.0);
    let mut g = Graph::new();
    g.insert_node(at, DemoNode { title: "alpha" });
    (g, at)
}

/// The floor: a frame runs, paints something, and does not panic.
#[test]
fn the_harness_renders_a_frame() {
    let (mut graph, _) = one_node();
    let mut h = Harness::new();
    let summary = h.frame(&mut graph, &mut Viewer);
    assert!(summary.vertex_count > 0, "the harness painted nothing");
}

/// Dragging a node's header must move it. This is the load-bearing
/// proof: it exercises pointer position, button state, per-frame drag
/// deltas and the widget's deferred-move site all at once, and it is
/// the exact mechanism the frame-drag phase reuses.
#[test]
fn dragging_a_node_moves_it() {
    let (mut graph, _) = one_node();
    let id = graph.node_ids().next().unwrap().0;

    let mut h = Harness::new();
    h.frame(&mut graph, &mut Viewer);
    let before = graph.get_node_info(id).unwrap().pos;

    // Aim at the node's header strip, in SCREEN space — the widget fits
    // content to the viewport, so graph coordinates do not land where
    // pointer events are delivered.
    let top_left = h.node_screen_pos(&graph, id);
    let grab = egui::pos2(top_left.x + 30.0, top_left.y + 8.0);
    h.drag(
        grab,
        egui::pos2(grab.x + 120.0, grab.y + 60.0),
        6,
        &mut graph,
        &mut Viewer,
    );

    let after = graph.get_node_info(id).unwrap().pos;
    assert_ne!(
        (after.x, after.y),
        (before.x, before.y),
        "the node did not move — the harness is not delivering drags"
    );
    assert!(
        after.x > before.x && after.y > before.y,
        "moved the wrong way: {before:?} -> {after:?}"
    );
}

/// Shift-click selects, and the selection is readable afterwards
/// through `GraphState::selection`.
///
/// **Ignored: the widget does not register synthesised clicks.** What
/// is established, by elimination:
///
/// * The harness reaches the node — a synthesised *drag* on the same
///   coordinates moves it (`dragging_a_node_moves_it`), and the press
///   frame shows `is_pointer_button_down_on() == true` on the node's
///   own response.
/// * The harness is not at fault — the same press/release sequence
///   against a bare `ui.interact(.., Sense::click_and_drag())` yields
///   `clicked()`, and so does the same sequence against a widget inside
///   a *transformed sublayer*, which is how the graph draws its nodes.
/// * So the release is being lost somewhere inside the graph's own
///   interaction stack, between `ui.interact` on the node frame and
///   egui resolving the click. Not yet identified.
///
/// This blocks PLAN_NODE.md P8's double-click-to-enter from being
/// tested headlessly, so it needs solving before that phase, not after.
/// Left as a failing-but-ignored test rather than deleted, because a
/// deleted test is a question nobody asks again.
#[test]
#[ignore = "synthesised clicks do not reach the graph widget; see doc comment"]
fn shift_click_selects_a_node_and_the_selection_is_readable() {
    let (mut graph, _) = one_node();
    let node = graph.node_ids().next().unwrap().0;
    let mut h = Harness::new();
    h.frame(&mut graph, &mut Viewer);

    let top_left = h.node_screen_pos(&graph, node);
    let on_node = egui::pos2(top_left.x + 30.0, top_left.y + 8.0);
    h.modified_click_at(on_node, egui::Modifiers::SHIFT, &mut graph, &mut Viewer);

    let seam = h.seam();
    let selection = GraphState::selection(&seam, h.graph_id());
    assert!(
        selection.contains(&node),
        "shift-click did not select: selection={selection:?}"
    );
}

/// A plain click must NOT change selection — the widget reserves that
/// for shift and command.
///
/// **Ignored for the same reason as the test above**, and this one is
/// the more instructive of the pair: it *passed* while synthesised
/// clicks were reaching nothing at all. A negative assertion over an
/// input that never arrives is satisfied by construction, which is
/// exactly the vacuous-pass failure a harness is supposed to prevent.
/// Re-enable it together with its positive counterpart, never alone.
#[test]
#[ignore = "would pass vacuously; re-enable with the positive click test"]
fn a_plain_click_leaves_the_selection_alone() {
    let (mut graph, _) = one_node();
    let node = graph.node_ids().next().unwrap().0;
    let mut h = Harness::new();
    h.frame(&mut graph, &mut Viewer);

    let top_left = h.node_screen_pos(&graph, node);
    let on_node = egui::pos2(top_left.x + 30.0, top_left.y + 8.0);
    h.click_at(on_node, &mut graph, &mut Viewer);

    let seam = h.seam();
    assert!(
        GraphState::selection(&seam, h.graph_id()).is_empty(),
        "a plain click must not select"
    );
}

/// Two click sequences in quick succession must leave the model
/// intact. Weaker than it should be — it cannot assert that a *double
/// click* was recognised, because single clicks are not reaching the
/// widget yet (see above). What it does cover is that the harness
/// survives the sequence with its pointer state consistent.
#[test]
fn two_quick_clicks_leave_the_model_intact() {
    let (mut graph, _) = one_node();
    let node = graph.node_ids().next().unwrap().0;
    let mut h = Harness::new();
    h.frame(&mut graph, &mut Viewer);
    let top_left = h.node_screen_pos(&graph, node);
    let on_node = egui::pos2(top_left.x + 30.0, top_left.y + 8.0);

    h.double_click_at(on_node, &mut graph, &mut Viewer);

    // Nothing in the widget consumes a double click yet — P7 adds
    // enter-a-subgraph. What is asserted here is that the harness got
    // through the sequence without losing the pointer or panicking,
    // and that the node survived it.
    assert_eq!(graph.len(), 1);
}

/// Idle frames must not drift the model. A harness that leaked stale
/// events would show up here as a node creeping across the canvas.
#[test]
fn idle_frames_do_not_move_anything() {
    let (mut graph, _) = one_node();
    let id = graph.node_ids().next().unwrap().0;
    let mut h = Harness::new();

    h.frame(&mut graph, &mut Viewer);
    let before = graph.get_node_info(id).unwrap().pos;
    h.idle(5, &mut graph, &mut Viewer);
    let after = graph.get_node_info(id).unwrap().pos;

    assert_eq!((before.x, before.y), (after.x, after.y));
}
