//! Collapse and expand — PLAN_NODE.md P8.
//!
//! The headline claim is a round trip: collapsing a selection and then
//! expanding it must leave the graph connected exactly as it was. Every
//! other test here is a way that round trip can silently be wrong.

use mara_core::vocab::Pos2;
use mara_graph::{
    DefId, DefScope, Graph, GraphDoc, GraphError, InPinId, NodeFactory, NodeId, NodePath, OutPinId,
    PortDir, PortSpec, Ports, pos2,
};

#[derive(Clone, Debug, PartialEq)]
struct P(&'static str);

struct Yes;

impl NodeFactory<P> for Yes {
    fn instance_node(&mut self, _d: DefId, _n: &str, _p: &Ports) -> Option<P> {
        Some(P("instance"))
    }
    fn port_node(&mut self, _s: &PortSpec<'_>) -> Option<P> {
        Some(P("port"))
    }
}

struct No;

impl NodeFactory<P> for No {
    fn instance_node(&mut self, _d: DefId, _n: &str, _p: &Ports) -> Option<P> {
        None
    }
    fn port_node(&mut self, _s: &PortSpec<'_>) -> Option<P> {
        None
    }
}

fn out(node: NodeId, output: usize) -> OutPinId {
    OutPinId { node, output }
}

fn inp(node: NodeId, input: usize) -> InPinId {
    InPinId { node, input }
}

fn at(x: f32, y: f32) -> Pos2 {
    pos2(x, y)
}

/// `src -> m1 -> m2 -> dst`, with `m1` and `m2` the selection.
/// One wire crosses in, one crosses out, one is interior.
fn chain() -> (GraphDoc<P>, Vec<mara_graph::NodeUid>) {
    let mut doc = GraphDoc::<P>::new();
    let g = &mut doc.root;
    let src = g.insert_node(at(0.0, 0.0), P("src"));
    let m1 = g.insert_node(at(200.0, 0.0), P("m1"));
    let m2 = g.insert_node(at(400.0, 0.0), P("m2"));
    let dst = g.insert_node(at(600.0, 0.0), P("dst"));

    g.connect(out(src, 0), inp(m1, 0));
    g.connect(out(m1, 0), inp(m2, 0));
    g.connect(out(m2, 0), inp(dst, 0));

    let members = vec![g.uid_of(m1).unwrap(), g.uid_of(m2).unwrap()];
    (doc, members)
}

/// Describe connectivity between surviving *payloads*, so the
/// comparison survives nodes being destroyed and recreated with new
/// ids — which is exactly what collapse and expand do.
fn connectivity(g: &Graph<P>) -> Vec<(String, usize, String, usize)> {
    let mut v: Vec<_> = g
        .wires()
        .map(|(o, i)| {
            (
                g[o.node].0.to_string(),
                o.output,
                g[i.node].0.to_string(),
                i.input,
            )
        })
        .collect();
    v.sort();
    v
}

// ── The round trip ──────────────────────────────────────────────────

/// The headline claim of the whole subgraph feature.
#[test]
fn collapse_then_expand_restores_the_exact_wire_set() {
    let (mut doc, members) = chain();
    let before = connectivity(&doc.root);
    let before_nodes = doc.root.len();

    let (_def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    doc.expand(&NodePath::root(), instance).expect("expand");

    assert_eq!(
        connectivity(&doc.root),
        before,
        "the round trip must restore every wire"
    );
    assert_eq!(doc.root.len(), before_nodes, "and every node");
}

#[test]
fn collapsing_replaces_the_members_with_one_node() {
    let (mut doc, members) = chain();
    let before = doc.root.len();

    doc.collapse(
        &NodePath::root(),
        &members,
        DefScope::Local,
        "chip",
        &mut Yes,
    )
    .expect("collapse");

    assert_eq!(
        doc.root.len(),
        before - members.len() + 1,
        "two members become one instance"
    );
}

/// The instance must carry the boundary wiring — an input from `src`
/// and an output to `dst`. If the snapshot were taken *after*
/// `remove_node`, both would be gone, because removing a node drops
/// every wire incident to it.
#[test]
fn the_instance_keeps_the_boundary_wires() {
    let (mut doc, members) = chain();

    let (_def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    let node = doc.root.by_uid(instance).unwrap();
    let wires: Vec<_> = doc.root.wires_of(node).collect();
    assert_eq!(wires.len(), 2, "one in, one out: {wires:?}");

    let conn = connectivity(&doc.root);
    assert!(
        conn.iter()
            .any(|(a, _, b, _)| a == "src" && b == "instance"),
        "src must feed the instance: {conn:?}"
    );
    assert!(
        conn.iter()
            .any(|(a, _, b, _)| a == "instance" && b == "dst"),
        "the instance must feed dst: {conn:?}"
    );
}

#[test]
fn the_definition_body_holds_the_members_and_their_interior_wire() {
    let (mut doc, members) = chain();

    let (def, _) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    let body = &doc.def(def).unwrap().body;
    let payloads: Vec<&str> = body.node_ids().map(|(_, p)| p.0).collect();
    assert!(payloads.contains(&"m1"));
    assert!(payloads.contains(&"m2"));
    assert!(
        payloads.iter().filter(|p| **p == "port").count() == 2,
        "one boundary node per port: {payloads:?}"
    );

    // The interior wire survives, plus one wire per boundary node.
    assert_eq!(body.wires().count(), 3, "m1->m2, in->m1, m2->out");
}

#[test]
fn the_derived_interface_has_one_port_per_crossing_pin() {
    let (mut doc, members) = chain();
    let (def, _) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    let ports = &doc.def(def).unwrap().ports;
    assert_eq!(ports.count(PortDir::In), 1);
    assert_eq!(ports.count(PortDir::Out), 1);
}

// ── Failure modes ───────────────────────────────────────────────────

#[test]
fn collapsing_nothing_is_an_error_not_an_empty_definition() {
    let (mut doc, _) = chain();
    let defs_before = doc.defs().count();
    assert_eq!(
        doc.collapse(&NodePath::root(), &[], DefScope::Local, "x", &mut Yes),
        Err(GraphError::EmptySelection)
    );
    assert_eq!(doc.defs().count(), defs_before);
}

/// A refusing factory must leave the document byte-identical — no
/// half-built definition, no removed members.
#[test]
fn a_declining_factory_leaves_the_document_untouched() {
    let (mut doc, members) = chain();
    let before_conn = connectivity(&doc.root);
    let before_nodes = doc.root.len();
    let before_defs = doc.defs().count();

    assert_eq!(
        doc.collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut No
        ),
        Err(GraphError::ViewerDeclined)
    );

    assert_eq!(doc.root.len(), before_nodes, "no member was removed");
    assert_eq!(connectivity(&doc.root), before_conn, "no wire was touched");
    assert_eq!(
        doc.defs().count(),
        before_defs,
        "no definition was left behind"
    );
}

#[test]
fn collapsing_a_stale_uid_is_an_error() {
    let (mut doc, members) = chain();
    let dead = mara_graph::NodeUid(9999);
    let mut with_dead = members.clone();
    with_dead.push(dead);

    assert_eq!(
        doc.collapse(
            &NodePath::root(),
            &with_dead,
            DefScope::Local,
            "x",
            &mut Yes
        ),
        Err(GraphError::NoSuchNode)
    );
}

// ── Scope ───────────────────────────────────────────────────────────

/// A `Local` definition exists only for its one instance; a `Shared`
/// one outlives its instances. This is what makes copy-by-value a
/// single bit rather than a second mechanism.
#[test]
fn expanding_a_local_definition_deletes_it() {
    let (mut doc, members) = chain();
    let (def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");
    assert!(doc.def(def).is_some());

    doc.expand(&NodePath::root(), instance).expect("expand");
    assert!(doc.def(def).is_none(), "a Local def with no instances goes");
}

#[test]
fn expanding_one_of_several_shared_instances_keeps_the_definition() {
    let (mut doc, members) = chain();
    let (def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Shared,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    // Place two more.
    for i in 0..2 {
        doc.instantiate(
            &NodePath::root(),
            def,
            at(800.0 + i as f32 * 100.0, 0.0),
            &mut Yes,
        )
        .unwrap();
    }
    assert_eq!(doc.instance_count(def), 3);

    doc.expand(&NodePath::root(), instance).expect("expand");

    assert!(doc.def(def).is_some(), "two instances remain");
    assert_eq!(doc.instance_count(def), 2);
}

/// Expanding hands back a mapping from the definition's uids to the
/// fresh ones in the host level. Without it an app's per-instance
/// state is orphaned with no way to find it.
#[test]
fn expand_returns_a_remap_covering_every_content_node() {
    let (mut doc, members) = chain();
    let (def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Shared,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    let content: Vec<_> = doc
        .def(def)
        .unwrap()
        .body
        .node_ids()
        .filter_map(|(id, _)| {
            let body = &doc.def(def).unwrap().body;
            let uid = body.uid_of(id)?;
            body.port_node(uid).is_none().then_some(uid)
        })
        .collect();

    let remap = doc.expand(&NodePath::root(), instance).expect("expand");

    for uid in &content {
        assert!(
            remap.contains_key(uid),
            "content node {uid:?} missing from the remap"
        );
    }
    assert_eq!(remap.len(), content.len(), "and nothing spurious");
}

/// The remap must point at *live* nodes in the host level, or an app
/// following it lands nowhere.
#[test]
fn every_remapped_uid_resolves_in_the_host_level() {
    let (mut doc, members) = chain();
    let (_, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");
    let remap = doc.expand(&NodePath::root(), instance).expect("expand");

    for new in remap.values() {
        assert!(
            doc.root.by_uid(*new).is_some(),
            "remapped uid {new:?} does not resolve"
        );
    }
}

// ── Make single user ────────────────────────────────────────────────

/// The escape hatch for the shared model: eight adders, one needs to
/// differ. Editing the copy must not touch the original.
#[test]
fn make_local_copy_detaches_one_instance_from_the_shared_definition() {
    let (mut doc, members) = chain();
    let (def, first) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Shared,
            "chip",
            &mut Yes,
        )
        .expect("collapse");
    let second = doc
        .instantiate(&NodePath::root(), def, at(900.0, 0.0), &mut Yes)
        .unwrap();
    assert_eq!(doc.instance_count(def), 2);

    let copy = doc
        .make_local_copy(&NodePath::root(), second)
        .expect("make_local_copy");

    assert_ne!(copy, def);
    assert_eq!(doc.instance_count(def), 1, "the original lost an instance");
    assert_eq!(doc.instance_count(copy), 1);
    assert_eq!(
        doc.root.instance_def(first),
        Some(def),
        "the first is untouched"
    );
    assert_eq!(doc.root.instance_def(second), Some(copy));

    // Editing the copy must not reach the original.
    let before = doc.def(def).unwrap().body.len();
    doc.def_mut(copy)
        .unwrap()
        .body
        .insert_node(at(0.0, 0.0), P("extra"));
    assert_eq!(doc.def(def).unwrap().body.len(), before);
}

// ── Fan-out ─────────────────────────────────────────────────────────

/// One interior output feeding three external consumers is one port
/// with three wires off the instance — the interface describes the
/// boundary, not the traffic across it.
#[test]
fn a_fan_out_becomes_one_port_with_several_wires() {
    let mut doc = GraphDoc::<P>::new();
    let g = &mut doc.root;
    let m = g.insert_node(at(0.0, 0.0), P("m"));
    let a = g.insert_node(at(300.0, 0.0), P("a"));
    let b = g.insert_node(at(300.0, 100.0), P("b"));
    let c = g.insert_node(at(300.0, 200.0), P("c"));
    g.connect(out(m, 0), inp(a, 0));
    g.connect(out(m, 0), inp(b, 0));
    g.connect(out(m, 0), inp(c, 0));
    let members = vec![g.uid_of(m).unwrap()];

    let (def, instance) = doc
        .collapse(
            &NodePath::root(),
            &members,
            DefScope::Local,
            "chip",
            &mut Yes,
        )
        .expect("collapse");

    assert_eq!(doc.def(def).unwrap().ports.count(PortDir::Out), 1);
    let node = doc.root.by_uid(instance).unwrap();
    assert_eq!(
        doc.root.wires_of(node).count(),
        3,
        "one port, three consumers"
    );
}
