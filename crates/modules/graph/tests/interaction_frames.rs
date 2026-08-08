//! Frame rendering and dragging, driven headlessly — PLAN_NODE.md P5.
//!
//! These are the assertions the model tests cannot make: that the frame
//! pass actually paints, that the title band is the only draggable part
//! (so canvas panning survives), and that a frame drag moves its
//! members. Everything here goes through the P3b input harness.
//!
//! Note the harness's known limitation: synthesised *clicks* do not
//! reach the graph widget, so nothing below depends on one. Drags do,
//! which is what frames need.

mod harness;

use harness::Harness;
use mara_core::MaraUi;
use mara_core::vocab::{Color32, Pos2, Rect, Vec2};
use mara_graph::{
    Graph, InPin, NodeId, NodePin, NodeViewer, OutPin, PinInfo, fit_frame_bounds, pos2,
};

struct N {
    title: &'static str,
}

struct V;

impl NodeViewer<N> for V {
    fn title(&mut self, node: &N) -> String {
        node.title.to_string()
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

fn grey() -> Color32 {
    Color32::from_gray(140)
}

fn zero_rect() -> Rect {
    Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(0.0, 0.0))
}

/// Two nodes inside one auto-fitting frame, plus one outside it.
fn framed_graph() -> (Graph<N>, NodeId, NodeId, NodeId, mara_graph::FrameId) {
    let mut g = Graph::<N>::new();
    let a = g.insert_node(pos2(100.0, 100.0), N { title: "a" });
    let b = g.insert_node(pos2(100.0, 220.0), N { title: "b" });
    let outside = g.insert_node(pos2(600.0, 100.0), N { title: "out" });
    let f = g.insert_frame("group", grey(), zero_rect());
    g.set_node_frame(a, Some(f));
    g.set_node_frame(b, Some(f));
    (g, a, b, outside, f)
}

/// The frame pass must contribute geometry, and it must land *behind*
/// the nodes it contains — a group box painted over its members would
/// hide them.
#[test]
fn a_frame_adds_geometry() {
    let (mut with_frame, ..) = framed_graph();

    let mut without_frame = Graph::<N>::new();
    without_frame.insert_node(pos2(100.0, 100.0), N { title: "a" });
    without_frame.insert_node(pos2(100.0, 220.0), N { title: "b" });
    without_frame.insert_node(pos2(600.0, 100.0), N { title: "out" });

    let framed = Harness::new()
        .frame_owned(&mut with_frame, &mut V)
        .vertex_count;
    let bare = Harness::new()
        .frame_owned(&mut without_frame, &mut V)
        .vertex_count;

    assert!(
        framed > bare,
        "the frame pass painted nothing: framed={framed} bare={bare}"
    );
}

/// An auto-fitting frame with no members has no meaningful extent.
/// Painting one anyway leaves a stray box on the canvas.
#[test]
fn an_empty_frame_paints_nothing() {
    let mut plain = Graph::<N>::new();
    plain.insert_node(pos2(100.0, 100.0), N { title: "a" });

    let mut with_empty = Graph::<N>::new();
    with_empty.insert_node(pos2(100.0, 100.0), N { title: "a" });
    with_empty.insert_frame("empty", grey(), zero_rect());

    let a = Harness::new().frame_owned(&mut plain, &mut V).vertex_count;
    let b = Harness::new()
        .frame_owned(&mut with_empty, &mut V)
        .vertex_count;

    assert_eq!(a, b, "an empty auto-fitting frame must paint nothing");
}

/// Dragging the title band moves every member by the delta, and moves
/// nothing outside the frame.
#[test]
fn title_band_drag_moves_members_and_only_members() {
    let (mut g, a, b, outside, f) = framed_graph();
    let mut h = Harness::new();
    h.frame(&mut g, &mut V);

    let before_a = g.get_node_info(a).unwrap().pos;
    let before_b = g.get_node_info(b).unwrap().pos;
    let before_out = g.get_node_info(outside).unwrap().pos;

    // Aim at the title band, which sits above the topmost member.
    let from = {
        let rects = rects_for(&g);
        let bounds = fit_frame_bounds(&g, f, &rects, 12.0).expect("members give bounds");
        let band_mid = Pos2::new(bounds.min.x + 40.0, bounds.min.y + 6.0);
        h.graph_to_screen(&g, band_mid)
    };

    h.drag(
        from,
        egui::pos2(from.x + 90.0, from.y + 50.0),
        5,
        &mut g,
        &mut V,
    );

    let after_a = g.get_node_info(a).unwrap().pos;
    let after_b = g.get_node_info(b).unwrap().pos;
    let after_out = g.get_node_info(outside).unwrap().pos;

    let da = (after_a.x - before_a.x, after_a.y - before_a.y);
    let db = (after_b.x - before_b.x, after_b.y - before_b.y);

    assert!(
        da.0 > 1.0 && da.1 > 1.0,
        "member a did not follow the frame: {before_a:?} -> {after_a:?}"
    );
    assert!(
        (da.0 - db.0).abs() < 0.01 && (da.1 - db.1).abs() < 0.01,
        "members must move by the SAME delta: a={da:?} b={db:?}"
    );
    assert_eq!(
        (before_out.x, before_out.y),
        (after_out.x, after_out.y),
        "a node outside the frame must not move"
    );
}

/// The load-bearing negative: the frame BODY is click-through, so
/// dragging it pans the canvas instead of moving the group. A hotspot
/// over the whole box would steal panning everywhere a frame exists.
#[test]
fn dragging_the_frame_body_pans_the_canvas() {
    let (mut g, a, _, _, f) = framed_graph();
    let mut h = Harness::new();
    h.frame(&mut g, &mut V);

    let before_a = g.get_node_info(a).unwrap().pos;
    let before_origin = h.graph_to_screen(&g, pos2(0.0, 0.0));

    // A point inside the frame but below the title band and clear of
    // both member nodes — the gap between them.
    let from = {
        let rects = rects_for(&g);
        let bounds = fit_frame_bounds(&g, f, &rects, 12.0).unwrap();
        let body = Pos2::new(bounds.max.x - 6.0, bounds.center().y);
        h.graph_to_screen(&g, body)
    };

    h.drag(from, egui::pos2(from.x + 70.0, from.y), 5, &mut g, &mut V);

    let after_a = g.get_node_info(a).unwrap().pos;
    let after_origin = h.graph_to_screen(&g, pos2(0.0, 0.0));

    assert_eq!(
        (before_a.x, before_a.y),
        (after_a.x, after_a.y),
        "dragging the body must NOT move members"
    );
    assert!(
        (after_origin.x - before_origin.x).abs() > 1.0,
        "dragging the body must pan the canvas: origin {before_origin:?} -> {after_origin:?}"
    );
}

/// A collapsed frame hides its members, so the canvas paints less.
#[test]
fn collapsing_a_frame_removes_its_members_geometry() {
    let (mut g, ..) = framed_graph();
    let open = Harness::new().frame_owned(&mut g, &mut V).vertex_count;

    let f = g.frames().next().unwrap().0;
    g.frame_mut(f).unwrap().collapsed = true;
    let closed = Harness::new().frame_owned(&mut g, &mut V).vertex_count;

    assert!(
        closed < open,
        "a collapsed frame must hide its members: open={open} closed={closed}"
    );
}

/// Two identical passes must produce identical output. Frames add a
/// sort and a recursive bounds walk; either could introduce order
/// dependence that shows up as flicker rather than as a failure.
#[test]
fn framed_rendering_is_deterministic() {
    let (mut g, ..) = framed_graph();
    let first = Harness::new().frame_owned(&mut g, &mut V).vertex_count;
    let second = Harness::new().frame_owned(&mut g, &mut V).vertex_count;
    assert_eq!(first, second);
}

fn rects_for(graph: &Graph<N>) -> impl Fn(NodeId) -> Rect + '_ {
    // The test only needs bounds that are close enough to aim at; the
    // renderer's own provider uses measured node state, which a test
    // outside a pass cannot see.
    move |id: NodeId| {
        let p = graph.get_node_info(id).expect("live").pos;
        Rect::from_min_size(Pos2::new(p.x, p.y), Vec2::new(90.0, 60.0))
    }
}

// ── PLAN_NODE.md P6 — chrome, LOD, camera ──────────────────────────

/// The LOD ladder's real job is performance, so the assertion is about
/// *work avoided*, not about how it looks: a graph zoomed out far
/// enough must stop asking the viewer about node bodies.
///
/// Counting viewer calls rather than emitted geometry on purpose —
/// egui's tessellator already drops fully-clipped shapes, so a
/// geometry-based assertion would pass with no LOD code at all.
#[test]
fn the_blob_tier_stops_asking_the_viewer_for_bodies() {
    use std::cell::Cell;

    struct Counting<'a> {
        body_queries: &'a Cell<usize>,
    }

    impl NodeViewer<N> for Counting<'_> {
        fn title(&mut self, node: &N) -> String {
            node.title.to_string()
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
        fn node_chrome(
            &mut self,
            _node: mara_graph::NodeId,
            tier: mara_graph::DetailTier,
            _g: &Graph<N>,
        ) -> mara_graph::NodeChrome {
            if tier.shows_body() {
                self.body_queries.set(self.body_queries.get() + 1);
            }
            mara_graph::NodeChrome::lit()
        }
    }

    fn run(scale: f32) -> usize {
        let mut g = Graph::<N>::new();
        for i in 0..6 {
            g.insert_node(pos2(i as f32 * 200.0, 0.0), N { title: "n" });
        }
        let counter = Cell::new(0);
        let mut viewer = Counting {
            body_queries: &counter,
        };
        let mut style = mara_graph::GraphStyle::new();
        style.lod = Some(mara_graph::LodLadder::default());
        // Lock the viewport scale so the ladder sees the value we mean.
        style.min_scale = Some(scale);
        style.max_scale = Some(scale);
        Harness::new()
            .with_style(style)
            .frame_owned(&mut g, &mut viewer);
        counter.get()
    }

    let zoomed_in = run(1.0);
    let zoomed_out = run(0.1);

    assert!(
        zoomed_in > 0,
        "at full zoom the viewer must be asked about bodies"
    );
    assert_eq!(
        zoomed_out, 0,
        "at blob tier the viewer must not be asked about bodies at all"
    );
}
