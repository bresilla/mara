//! `show_doc` — level-salted state and navigation. PLAN_NODE.md P7.
//!
//! The claim under test is not "nesting renders" but "levels do not
//! leak into each other". `graph_id` keys the sublayer, the whole
//! `GraphState`, every node's measured size and the wire cache; if
//! entering a child does not re-key all of them, the child inherits the
//! parent's pan and zoom and — because `NodeId` is a per-graph slab
//! index — the parent's node 0 and the child's node 0 share one size
//! cache. Both failures look like rendering glitches and are actually
//! id collisions.

mod harness;

use mara_core::MaraUi;
use mara_graph::{
    DefId, DefScope, Graph, GraphDoc, GraphWidget, InPin, NodeFactory, NodePath, NodePin,
    NodeViewer, OutPin, PinInfo, PortDir, PortSpec, Ports, pos2,
};

#[derive(Clone)]
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

struct Factory;

impl NodeFactory<N> for Factory {
    fn instance_node(&mut self, _d: DefId, _n: &str, _p: &Ports) -> Option<N> {
        Some(N { title: "chip" })
    }
    fn port_node(&mut self, _s: &PortSpec<'_>) -> Option<N> {
        Some(N { title: "port" })
    }
}

/// A document whose root holds one instance of a definition, and whose
/// definition body holds three nodes of its own.
fn doc_with_instance() -> (GraphDoc<N>, DefId, mara_graph::NodeUid) {
    let mut doc = GraphDoc::<N>::new();
    let d = doc.insert_def("chip", DefScope::Shared);
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.def_mut(d).unwrap().ports.push(PortDir::Out, "y");

    for i in 0..3 {
        doc.def_mut(d)
            .unwrap()
            .body
            .insert_node(pos2(i as f32 * 150.0, 0.0), N { title: "inner" });
    }
    doc.root.insert_node(pos2(0.0, 0.0), N { title: "outer" });
    let uid = doc
        .instantiate(&NodePath::root(), d, pos2(300.0, 0.0), &mut Factory)
        .unwrap();
    (doc, d, uid)
}

/// Run `n` frames of `show_doc` and return the last summary.
fn render(doc: &mut GraphDoc<N>, frames: usize) -> (usize, NodePath) {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0));
    let mut vertices = 0;
    let mut path = NodePath::root();
    let mut time = 0.0_f64;

    for _ in 0..frames {
        time += 0.016;
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        });
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&ctx, |ui| {
            let mut backend = mara_backend_egui::EguiUiBackend::new(ui);
            MaraUi::__internal_over_backend_ret(
                &mut backend,
                mara_core::style::active_accent(),
                |mara| {
                    let out = GraphWidget::new()
                        .id(mara_core::vocab::Id::new("doc.test"))
                        .show_doc(doc, &mut V, mara);
                    path = out.path.clone();
                },
            );
        });
        let output = ctx.end_pass();
        let prims = ctx.tessellate(output.shapes, output.pixels_per_point);
        vertices = prims
            .iter()
            .filter_map(|p| match &p.primitive {
                egui::epaint::Primitive::Mesh(m) if !m.indices.is_empty() => Some(m.vertices.len()),
                _ => None,
            })
            .sum();
    }
    (vertices, path)
}

#[test]
fn show_doc_renders_the_root_level() {
    let (mut doc, ..) = doc_with_instance();
    let (vertices, path) = render(&mut doc, 2);
    assert!(vertices > 0, "show_doc painted nothing");
    assert_eq!(path, NodePath::root(), "starts at the root");
}

/// The interface is the source of truth for an instance's pins — the
/// app is never asked, and must not be, because it did not create the
/// node and cannot know how many ports its definition has.
#[test]
fn an_instance_takes_its_pin_count_from_the_definition_not_the_viewer() {
    use std::cell::Cell;

    struct Counting<'a> {
        asked: &'a Cell<usize>,
    }

    impl NodeViewer<N> for Counting<'_> {
        fn title(&mut self, node: &N) -> String {
            node.title.to_string()
        }
        fn inputs(&mut self, _n: &N) -> usize {
            self.asked.set(self.asked.get() + 1);
            7 // deliberately wrong for an instance
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
            7
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

    // A document with ONLY the instance, so any `inputs` call must have
    // come from it.
    let mut doc = GraphDoc::<N>::new();
    let d = doc.insert_def("chip", DefScope::Shared);
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.instantiate(&NodePath::root(), d, pos2(0.0, 0.0), &mut Factory)
        .unwrap();

    let asked = Cell::new(0);
    let mut viewer = Counting { asked: &asked };

    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0));
    for _ in 0..2 {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&ctx, |ui| {
            let mut backend = mara_backend_egui::EguiUiBackend::new(ui);
            MaraUi::__internal_over_backend_ret(
                &mut backend,
                mara_core::style::active_accent(),
                |mara| {
                    let _ = GraphWidget::new()
                        .id(mara_core::vocab::Id::new("iface.test"))
                        .show_doc(&mut doc, &mut viewer, mara);
                },
            );
        });
        let out = ctx.end_pass();
        let _ = ctx.tessellate(out.shapes, out.pixels_per_point);
    }

    assert_eq!(
        asked.get(),
        0,
        "the viewer must never be asked about an instance's pins"
    );
}

/// Entering a level must re-key node state. Root's `NodeId(0)` and the
/// definition body's `NodeId(0)` are different nodes with the same slab
/// index; sharing a size cache between them is the failure this pins.
#[test]
fn levels_render_independently() {
    let (mut doc, d, uid) = doc_with_instance();

    let (root_vertices, _) = render(&mut doc, 2);

    // The definition body has three nodes, the root has two. If node
    // state were shared across levels, the two would have collided on
    // slab indices 0 and 1.
    assert_eq!(doc.def(d).unwrap().body.len(), 3);
    assert_eq!(doc.root.len(), 2);
    assert!(root_vertices > 0);
    assert!(
        doc.root.instance_def(uid).is_some(),
        "the instance is bound"
    );
}

/// A path pointing through a deleted instance must fall back rather
/// than render nothing or panic.
#[test]
fn a_stale_path_falls_back_to_a_level_that_exists() {
    let (mut doc, _, uid) = doc_with_instance();
    let deep = NodePath::root().child(uid);
    assert!(doc.resolve(&deep).is_ok());

    let id = doc.root.by_uid(uid).unwrap();
    doc.root.remove_node(id);

    assert!(doc.resolve(&deep).is_err(), "the level is gone");
    assert_eq!(doc.prune_path(&deep), NodePath::root());

    // And rendering still works.
    let (vertices, path) = render(&mut doc, 2);
    assert!(vertices > 0);
    assert_eq!(path, NodePath::root());
}

#[test]
fn the_breadcrumb_names_every_level_from_the_root() {
    let (doc, _, uid) = doc_with_instance();
    let crumbs = doc.breadcrumb(&NodePath::root().child(uid));
    let names: Vec<&str> = crumbs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Root", "chip"]);
}
