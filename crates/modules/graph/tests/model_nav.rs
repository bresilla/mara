//! `GraphDoc`, ports and level navigation — PLAN_NODE.md P7.
//!
//! `T = ()` with a two-line factory, no rendering anywhere. The
//! algorithms this exercises — recursion guards, port remapping,
//! path pruning — are the ones a rendering test could not reach and
//! the ones where a mistake corrupts a document rather than making it
//! look wrong.

use mara_core::vocab::Pos2;
use mara_graph::{
    DefId, DefScope, GraphDoc, GraphError, InPinId, NodeFactory, NodePath, OutPinId, PortDir,
    PortSpec, Ports, pos2,
};

/// Accepts every request. `T = ()`, so a payload costs nothing.
struct Yes;

impl NodeFactory<()> for Yes {
    fn instance_node(&mut self, _d: DefId, _n: &str, _p: &Ports) -> Option<()> {
        Some(())
    }
    fn port_node(&mut self, _s: &PortSpec<'_>) -> Option<()> {
        Some(())
    }
}

/// Refuses everything — the app declining to mint a payload.
struct No;

impl NodeFactory<()> for No {
    fn instance_node(&mut self, _d: DefId, _n: &str, _p: &Ports) -> Option<()> {
        None
    }
    fn port_node(&mut self, _s: &PortSpec<'_>) -> Option<()> {
        None
    }
}

fn at(x: f32, y: f32) -> Pos2 {
    pos2(x, y)
}

// ── Addressing ──────────────────────────────────────────────────────

#[test]
fn the_root_path_resolves_to_no_definition() {
    let doc = GraphDoc::<()>::new();
    assert_eq!(doc.resolve(&NodePath::root()), Ok(None));
    assert!(doc.level(&NodePath::root()).is_some());
}

#[test]
fn level_addresses_the_right_graph_at_depth_three() {
    let mut doc = GraphDoc::<()>::new();
    let a = doc.insert_def("a", DefScope::Shared);
    let b = doc.insert_def("b", DefScope::Shared);
    let c = doc.insert_def("c", DefScope::Shared);

    // root -> a -> b -> c
    let ia = doc
        .instantiate(&NodePath::root(), a, at(0.0, 0.0), &mut Yes)
        .unwrap();
    let pa = NodePath::root().child(ia);
    let ib = doc.instantiate(&pa, b, at(0.0, 0.0), &mut Yes).unwrap();
    let pb = pa.child(ib);
    let ic = doc.instantiate(&pb, c, at(0.0, 0.0), &mut Yes).unwrap();
    let pc = pb.child(ic);

    assert_eq!(doc.resolve(&pa), Ok(Some(a)));
    assert_eq!(doc.resolve(&pb), Ok(Some(b)));
    assert_eq!(doc.resolve(&pc), Ok(Some(c)));

    // Prove it is the *right* graph by writing into it.
    doc.level_mut(&pc).unwrap().insert_node(at(5.0, 5.0), ());
    assert_eq!(doc.def(c).unwrap().body.len(), 1);
    assert_eq!(
        doc.def(b).unwrap().body.len(),
        1,
        "b holds only its instance of c"
    );
    assert_eq!(doc.root.len(), 1);
}

#[test]
fn breadcrumb_returns_the_full_chain_with_the_root_first() {
    let mut doc = GraphDoc::<()>::new();
    let alu = doc.insert_def("ALU", DefScope::Shared);
    let adder = doc.insert_def("Adder", DefScope::Shared);
    let i1 = doc
        .instantiate(&NodePath::root(), alu, at(0.0, 0.0), &mut Yes)
        .unwrap();
    let p1 = NodePath::root().child(i1);
    let i2 = doc.instantiate(&p1, adder, at(0.0, 0.0), &mut Yes).unwrap();
    let p2 = p1.child(i2);

    let crumbs = doc.breadcrumb(&p2);
    let names: Vec<&str> = crumbs.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["Root", "ALU", "Adder"]);
    assert_eq!(crumbs[0].path, NodePath::root());
    assert_eq!(crumbs[2].path, p2);
}

/// A saved path can outlive the instance it points through. Truncating
/// beats refusing to open the document.
#[test]
fn a_path_whose_instance_is_gone_prunes_to_the_surviving_prefix() {
    let mut doc = GraphDoc::<()>::new();
    let a = doc.insert_def("a", DefScope::Shared);
    let b = doc.insert_def("b", DefScope::Shared);
    let ia = doc
        .instantiate(&NodePath::root(), a, at(0.0, 0.0), &mut Yes)
        .unwrap();
    let pa = NodePath::root().child(ia);
    let ib = doc.instantiate(&pa, b, at(0.0, 0.0), &mut Yes).unwrap();
    let deep = pa.child(ib);

    assert_eq!(doc.prune_path(&deep), deep, "intact path survives whole");

    // Delete the inner instance.
    let inner = doc.def(a).unwrap().body.by_uid(ib).unwrap();
    doc.def_mut(a).unwrap().body.remove_node(inner);

    assert_eq!(doc.prune_path(&deep), pa, "prunes to the surviving prefix");
    assert!(doc.resolve(&deep).is_err());
}

// ── Recursion guards ────────────────────────────────────────────────

/// The first user who drags an ALU into itself must get an error, not a
/// stack overflow at render time.
#[test]
fn a_definition_cannot_contain_itself() {
    let mut doc = GraphDoc::<()>::new();
    let a = doc.insert_def("a", DefScope::Shared);
    let ia = doc
        .instantiate(&NodePath::root(), a, at(0.0, 0.0), &mut Yes)
        .unwrap();
    let inside_a = NodePath::root().child(ia);

    assert!(!doc.can_contain(Some(a), a));
    assert_eq!(
        doc.instantiate(&inside_a, a, at(0.0, 0.0), &mut Yes),
        Err(GraphError::WouldRecurse)
    );
}

#[test]
fn transitive_self_containment_is_rejected() {
    let mut doc = GraphDoc::<()>::new();
    let outer = doc.insert_def("outer", DefScope::Shared);
    let inner = doc.insert_def("inner", DefScope::Shared);

    // outer contains inner.
    let io = doc
        .instantiate(&NodePath::root(), outer, at(0.0, 0.0), &mut Yes)
        .unwrap();
    let in_outer = NodePath::root().child(io);
    doc.instantiate(&in_outer, inner, at(0.0, 0.0), &mut Yes)
        .unwrap();

    // So inner must not be allowed to contain outer.
    assert!(!doc.can_contain(Some(inner), outer));
}

#[test]
fn nesting_past_the_cap_returns_an_error_rather_than_recursing() {
    let mut doc = GraphDoc::<()>::new();
    doc.set_depth_cap(3);

    let defs: Vec<DefId> = (0..6)
        .map(|i| doc.insert_def(format!("d{i}"), DefScope::Shared))
        .collect();

    let mut path = NodePath::root();
    let mut placed = 0;
    let mut last_err = None;
    for d in &defs {
        match doc.instantiate(&path, *d, at(0.0, 0.0), &mut Yes) {
            Ok(uid) => {
                path = path.child(uid);
                placed += 1;
            }
            Err(e) => {
                last_err = Some(e);
                break;
            }
        }
    }

    assert!(placed <= 3, "the cap must bind: placed {placed}");
    assert!(
        matches!(last_err, Some(GraphError::DepthLimit(3))),
        "expected DepthLimit, got {last_err:?}"
    );
}

/// A declining factory must leave the document exactly as it was —
/// no orphan node, no dangling instance entry.
#[test]
fn a_declining_factory_leaves_the_document_untouched() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);

    let before_root = doc.root.len();
    let before_count = doc.instance_count(d);

    assert_eq!(
        doc.instantiate(&NodePath::root(), d, at(0.0, 0.0), &mut No),
        Err(GraphError::ViewerDeclined)
    );

    assert_eq!(doc.root.len(), before_root, "no node was left behind");
    assert_eq!(doc.instance_count(d), before_count);
}

// ── Instance counting ───────────────────────────────────────────────

/// The 8-bit-CPU case: one definition, many instances. The count has to
/// be right or the "×8" badge lies about the sharing.
#[test]
fn instance_count_spans_the_whole_document() {
    let mut doc = GraphDoc::<()>::new();
    let adder = doc.insert_def("adder", DefScope::Shared);
    let alu = doc.insert_def("alu", DefScope::Shared);

    // Four adders at the root.
    for i in 0..4 {
        doc.instantiate(
            &NodePath::root(),
            adder,
            at(i as f32 * 100.0, 0.0),
            &mut Yes,
        )
        .unwrap();
    }
    // One ALU at the root, holding three more adders.
    let ia = doc
        .instantiate(&NodePath::root(), alu, at(500.0, 0.0), &mut Yes)
        .unwrap();
    let in_alu = NodePath::root().child(ia);
    for i in 0..3 {
        doc.instantiate(&in_alu, adder, at(i as f32 * 100.0, 0.0), &mut Yes)
            .unwrap();
    }

    assert_eq!(doc.instance_count(adder), 7, "4 at root + 3 inside the ALU");
    assert_eq!(doc.instance_count(alu), 1);
}

#[test]
fn removing_an_instance_lowers_the_count() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    let uid = doc
        .instantiate(&NodePath::root(), d, at(0.0, 0.0), &mut Yes)
        .unwrap();
    assert_eq!(doc.instance_count(d), 1);

    let id = doc.root.by_uid(uid).unwrap();
    doc.root.remove_node(id);
    assert_eq!(doc.instance_count(d), 0);
}

// ── Port derivation ─────────────────────────────────────────────────

fn out(node: mara_graph::NodeId, output: usize) -> OutPinId {
    OutPinId { node, output }
}

fn inp(node: mara_graph::NodeId, input: usize) -> InPinId {
    InPinId { node, input }
}

/// Two external sources feeding **one** interior pin must share a
/// single input port — the interface describes the boundary, not the
/// wires crossing it.
#[test]
fn two_external_sources_into_one_interior_pin_produce_one_input_port() {
    let mut g = mara_graph::Graph::<()>::new();
    let src_a = g.insert_node(at(0.0, 0.0), ());
    let src_b = g.insert_node(at(0.0, 100.0), ());
    let member = g.insert_node(at(300.0, 50.0), ());

    g.connect(out(src_a, 0), inp(member, 0));
    g.connect(out(src_b, 0), inp(member, 0));

    let uid = g.uid_of(member).unwrap();
    let (ins, outs) = g.derive_ports(&[uid]);

    assert_eq!(ins.len(), 1, "one interior pin means one port");
    assert_eq!(ins[0], inp(member, 0));
    assert!(outs.is_empty());
}

/// One interior output feeding three external pins is still one output
/// port — with three wires hanging off the instance afterwards.
#[test]
fn one_interior_output_feeding_three_consumers_produces_one_output_port() {
    let mut g = mara_graph::Graph::<()>::new();
    let member = g.insert_node(at(0.0, 0.0), ());
    let a = g.insert_node(at(300.0, 0.0), ());
    let b = g.insert_node(at(300.0, 100.0), ());
    let c = g.insert_node(at(300.0, 200.0), ());

    g.connect(out(member, 0), inp(a, 0));
    g.connect(out(member, 0), inp(b, 0));
    g.connect(out(member, 0), inp(c, 0));

    let uid = g.uid_of(member).unwrap();
    let (ins, outs) = g.derive_ports(&[uid]);

    assert!(ins.is_empty());
    assert_eq!(outs.len(), 1);
    assert_eq!(outs[0], out(member, 0));
}

/// Wires wholly inside the selection are interior and must not become
/// ports; wires wholly outside are irrelevant.
#[test]
fn interior_and_external_wires_produce_no_ports() {
    let mut g = mara_graph::Graph::<()>::new();
    let m1 = g.insert_node(at(0.0, 0.0), ());
    let m2 = g.insert_node(at(150.0, 0.0), ());
    let e1 = g.insert_node(at(600.0, 0.0), ());
    let e2 = g.insert_node(at(750.0, 0.0), ());

    g.connect(out(m1, 0), inp(m2, 0)); // interior
    g.connect(out(e1, 0), inp(e2, 0)); // external

    let members = [g.uid_of(m1).unwrap(), g.uid_of(m2).unwrap()];
    let (ins, outs) = g.derive_ports(&members);

    assert!(ins.is_empty(), "an interior wire is not a boundary");
    assert!(outs.is_empty(), "an external wire is not a boundary");
}

/// Order is member `pos.y`, then `pos.x`, then pin index — visual
/// top-to-bottom. It must not depend on slab order, or the same
/// selection would derive its ports differently after unrelated edits.
#[test]
fn port_order_follows_position_not_slab_order() {
    let mut g = mara_graph::Graph::<()>::new();
    // Inserted bottom-first, so slab order disagrees with visual order.
    let low = g.insert_node(at(0.0, 500.0), ());
    let high = g.insert_node(at(0.0, 100.0), ());
    let mid = g.insert_node(at(0.0, 300.0), ());
    let src = g.insert_node(at(-300.0, 0.0), ());

    g.connect(out(src, 0), inp(low, 0));
    g.connect(out(src, 1), inp(high, 0));
    g.connect(out(src, 2), inp(mid, 0));

    let members = [
        g.uid_of(low).unwrap(),
        g.uid_of(high).unwrap(),
        g.uid_of(mid).unwrap(),
    ];
    let (ins, _) = g.derive_ports(&members);

    assert_eq!(ins.len(), 3);
    assert_eq!(ins[0].node, high, "topmost first");
    assert_eq!(ins[1].node, mid);
    assert_eq!(ins[2].node, low);
}

#[test]
fn port_derivation_is_repeatable() {
    let mut g = mara_graph::Graph::<()>::new();
    let src = g.insert_node(at(-200.0, 0.0), ());
    let members: Vec<_> = (0..6)
        .map(|i| {
            let n = g.insert_node(at(0.0, i as f32 * 50.0), ());
            g.connect(out(src, i), inp(n, 0));
            g.uid_of(n).unwrap()
        })
        .collect();

    let first = g.derive_ports(&members);
    for _ in 0..5 {
        assert_eq!(g.derive_ports(&members), first, "derivation must not drift");
    }
}

// ── Editing the interface ───────────────────────────────────────────

/// Reordering a port must move every instance's wires with it. This is
/// the operation where an off-by-one hides, so it gets an explicit
/// before/after on a real instance.
#[test]
fn reordering_a_port_moves_every_instance_wire() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "b");
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "c");

    let uid = doc
        .instantiate(&NodePath::root(), d, at(200.0, 0.0), &mut Yes)
        .unwrap();
    let inst = doc.root.by_uid(uid).unwrap();
    let feeder = doc.root.insert_node(at(0.0, 0.0), ());

    // Wire the feeder into port index 0 only.
    doc.root.connect(out(feeder, 0), inp(inst, 0));
    assert_eq!(doc.root.wires().count(), 1);

    // Move port 0 to the end: the wire must follow to index 2.
    doc.reorder_port(d, PortDir::In, 0, 2);

    let wires: Vec<_> = doc.root.wires().collect();
    assert_eq!(wires.len(), 1, "the wire must survive, not be dropped");
    assert_eq!(
        wires[0].1.input, 2,
        "and must follow the port to its new index"
    );

    // And the interface order really did change.
    let ports = &doc.def(d).unwrap().ports;
    assert_eq!(ports.at(PortDir::In, 2).unwrap().name, "a");
}

#[test]
fn reordering_leaves_unrelated_wires_alone() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "b");

    let uid = doc
        .instantiate(&NodePath::root(), d, at(200.0, 0.0), &mut Yes)
        .unwrap();
    let inst = doc.root.by_uid(uid).unwrap();
    let x = doc.root.insert_node(at(0.0, 0.0), ());
    let y = doc.root.insert_node(at(0.0, 200.0), ());

    doc.root.connect(out(x, 0), inp(inst, 0));
    doc.root.connect(out(x, 1), inp(y, 0)); // nothing to do with the instance

    doc.reorder_port(d, PortDir::In, 0, 1);

    let unrelated = doc
        .root
        .wires()
        .filter(|(o, i)| o.node == x && i.node == y)
        .count();
    assert_eq!(unrelated, 1, "an unrelated wire must be untouched");
}

/// Removing a port must DROP its wires, not leave them addressing an
/// index that now belongs to a different port. An orphaned wire is
/// invisible and resurrects if the count later grows back.
#[test]
fn removing_a_port_drops_its_wires_and_shifts_the_rest_down() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    let pa = doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "b");

    let uid = doc
        .instantiate(&NodePath::root(), d, at(200.0, 0.0), &mut Yes)
        .unwrap();
    let inst = doc.root.by_uid(uid).unwrap();
    let f = doc.root.insert_node(at(0.0, 0.0), ());

    doc.root.connect(out(f, 0), inp(inst, 0)); // -> port a
    doc.root.connect(out(f, 1), inp(inst, 1)); // -> port b
    assert_eq!(doc.root.wires().count(), 2);

    doc.remove_port(d, pa);

    let wires: Vec<_> = doc.root.wires().collect();
    assert_eq!(wires.len(), 1, "the wire on the removed port must be gone");
    assert_eq!(wires[0].1.input, 0, "b shifted down into index 0");
    assert_eq!(doc.def(d).unwrap().ports.count(PortDir::In), 1);
}

#[test]
fn renaming_a_port_touches_no_wire() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    let p = doc.def_mut(d).unwrap().ports.push(PortDir::In, "old");
    let uid = doc
        .instantiate(&NodePath::root(), d, at(200.0, 0.0), &mut Yes)
        .unwrap();
    let inst = doc.root.by_uid(uid).unwrap();
    let f = doc.root.insert_node(at(0.0, 0.0), ());
    doc.root.connect(out(f, 0), inp(inst, 0));

    let before: Vec<_> = doc.root.wires().collect();
    doc.rename_port(d, p, "new");
    let after: Vec<_> = doc.root.wires().collect();

    assert_eq!(before, after);
    assert_eq!(
        doc.def(d).unwrap().ports.at(PortDir::In, 0).unwrap().name,
        "new"
    );
}

/// `PortId` must be stable across reordering — that is the whole reason
/// order and identity are separate.
#[test]
fn port_ids_survive_reordering() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("d", DefScope::Shared);
    let a = doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    let b = doc.def_mut(d).unwrap().ports.push(PortDir::In, "b");

    doc.reorder_port(d, PortDir::In, 0, 1);

    let ports = &doc.def(d).unwrap().ports;
    assert_eq!(ports.index_of(PortDir::In, a), Some(1));
    assert_eq!(ports.index_of(PortDir::In, b), Some(0));
}

// ── Snapshot ────────────────────────────────────────────────────────

#[test]
fn the_iface_snapshot_reports_each_definitions_pin_counts() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("chip", DefScope::Shared);
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "a");
    doc.def_mut(d).unwrap().ports.push(PortDir::In, "b");
    doc.def_mut(d).unwrap().ports.push(PortDir::Out, "y");

    let snap = doc.iface_snapshot();
    let iface = snap.get(&d).expect("definition is in the snapshot");
    assert_eq!(iface.inputs, 2);
    assert_eq!(iface.outputs, 1);
    assert_eq!(iface.name, "chip");
}

#[test]
fn promote_moves_a_local_definition_into_the_library() {
    let mut doc = GraphDoc::<()>::new();
    let d = doc.insert_def("tidy", DefScope::Local);
    assert_eq!(doc.def(d).unwrap().scope, DefScope::Local);
    doc.promote(d);
    assert_eq!(doc.def(d).unwrap().scope, DefScope::Shared);
}
