//! Model-level characterisation of stable identity and the wire index
//! — PLAN_NODE.md P3.
//!
//! Everything here runs on `Graph<()>` with no rendering, no context
//! and no window. That is deliberate: the two features this phase
//! exists to support (frame membership and subgraph instances) are both
//! *persistent references to nodes*, and their correctness is decided
//! entirely in the model. Testing them through a render pass would
//! prove less and cost more.

use mara_graph::{Graph, InPinId, NodeId, NodeUid, OutPinId, pos2};

fn out(node: NodeId, output: usize) -> OutPinId {
    OutPinId { node, output }
}

fn inp(node: NodeId, input: usize) -> InPinId {
    InPinId { node, input }
}

/// The hazard the whole phase exists for, demonstrated rather than
/// described: `slab` hands a vacated key straight back out, so a
/// `NodeId` held across a delete silently addresses a different node.
/// `NodeUid` must not.
#[test]
fn uid_survives_slab_key_reuse() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let _b = g.insert_node(pos2(10.0, 0.0), ());

    let a_uid = g.uid_of(a).expect("a exists");
    g.remove_node(a);

    let c = g.insert_node(pos2(20.0, 0.0), ());
    let c_uid = g.uid_of(c).expect("c exists");

    assert_eq!(
        c, a,
        "precondition: slab recycled the key, so NodeId collides"
    );
    assert_ne!(c_uid, a_uid, "uid must NOT collide — that is the point");
    assert_eq!(
        g.by_uid(a_uid),
        None,
        "the dead uid must resolve to nothing, not to whatever took the slot"
    );
    assert_eq!(g.by_uid(c_uid), Some(c));
}

#[test]
fn every_node_gets_a_distinct_assigned_uid() {
    let mut g = Graph::<()>::new();
    let ids: Vec<_> = (0..16)
        .map(|i| g.insert_node(pos2(i as f32, 0.0), ()))
        .collect();

    let uids: Vec<NodeUid> = ids.iter().map(|id| g.uid_of(*id).unwrap()).collect();

    assert!(uids.iter().all(|u| u.is_assigned()), "0 is the sentinel");
    let mut sorted = uids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), uids.len(), "uids must be distinct");
}

#[test]
fn repair_is_idempotent() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(10.0, 0.0), ());
    g.connect(out(a, 0), inp(b, 0));

    g.repair();
    let after_first: Vec<_> = g.node_ids().map(|(id, ())| g.uid_of(id).unwrap()).collect();
    let wires_first: Vec<_> = g.wires().collect();

    g.repair();
    let after_second: Vec<_> = g.node_ids().map(|(id, ())| g.uid_of(id).unwrap()).collect();
    let wires_second: Vec<_> = g.wires().collect();

    assert_eq!(after_first, after_second);
    assert_eq!(wires_first.len(), wires_second.len());
}

/// Removing a node must not leave its uid resolving to a live node.
#[test]
fn removing_a_node_drops_its_uid_binding() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let uid = g.uid_of(a).unwrap();

    assert_eq!(g.by_uid(uid), Some(a));
    g.remove_node(a);
    assert_eq!(g.by_uid(uid), None);
    assert!(!g.contains(a));
}

#[test]
fn revision_moves_on_structural_change_and_not_on_a_read() {
    let mut g = Graph::<()>::new();
    let start = g.revision();

    let a = g.insert_node(pos2(0.0, 0.0), ());
    let after_insert = g.revision();
    assert_ne!(after_insert, start, "insert is a structural change");

    let _ = g.len();
    let _ = g.uid_of(a);
    assert_eq!(g.revision(), after_insert, "reads must not bump revision");

    g.remove_node(a);
    assert_ne!(g.revision(), after_insert, "remove is a structural change");
}

// ── The wire adjacency index ────────────────────────────────────────

/// The index must agree with the brute-force scan it replaced, across
/// a shuffled sequence of every mutation that touches it. Written as a
/// deterministic pseudo-random walk rather than a handful of cases,
/// because the failure mode is a *stale* entry after some particular
/// interleaving, which fixed cases miss.
#[test]
fn the_wire_index_agrees_with_brute_force_after_random_mutation() {
    let mut g = Graph::<()>::new();
    let nodes: Vec<_> = (0..8)
        .map(|i| g.insert_node(pos2(i as f32 * 40.0, 0.0), ()))
        .collect();

    // xorshift, so the walk is reproducible without a dev-dependency.
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut rng = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };

    let mut expected: Vec<(OutPinId, InPinId)> = Vec::new();

    for step in 0..400 {
        let a = nodes[(rng() % 8) as usize];
        let b = nodes[(rng() % 8) as usize];
        let oi = (rng() % 3) as usize;
        let ii = (rng() % 3) as usize;
        let (o, i) = (out(a, oi), inp(b, ii));

        match rng() % 4 {
            0 => {
                if g.connect(o, i) {
                    expected.push((o, i));
                }
            }
            1 => {
                if g.disconnect(o, i) {
                    expected.retain(|w| *w != (o, i));
                }
            }
            2 => {
                g.drop_inputs(i);
                expected.retain(|(_, wi)| *wi != i);
            }
            _ => {
                g.drop_outputs(o);
                expected.retain(|(wo, _)| *wo != o);
            }
        }

        // The invariant, checked every step so a failure names the step
        // that broke it rather than the end state.
        for n in &nodes {
            for pin in 0..3 {
                let o = out(*n, pin);
                let mut via_index: Vec<_> = g.out_pin(o).remotes.clone();
                via_index.sort_unstable();
                let mut brute: Vec<_> = expected
                    .iter()
                    .filter(|(wo, _)| *wo == o)
                    .map(|(_, wi)| *wi)
                    .collect();
                brute.sort_unstable();
                assert_eq!(via_index, brute, "out_pin mismatch at step {step}");

                let i = inp(*n, pin);
                let mut via_index: Vec<_> = g.in_pin(i).remotes.clone();
                via_index.sort_unstable();
                let mut brute: Vec<_> = expected
                    .iter()
                    .filter(|(_, wi)| *wi == i)
                    .map(|(wo, _)| *wo)
                    .collect();
                brute.sort_unstable();
                assert_eq!(via_index, brute, "in_pin mismatch at step {step}");
            }
        }
    }
}

/// `remove_node` drops every incident wire. The index must follow, or a
/// later node reusing the slab key inherits ghost connections.
#[test]
fn removing_a_node_clears_it_from_the_wire_index() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(50.0, 0.0), ());
    let c = g.insert_node(pos2(100.0, 0.0), ());

    g.connect(out(a, 0), inp(b, 0));
    g.connect(out(b, 0), inp(c, 0));
    assert_eq!(g.wires().count(), 2);

    g.remove_node(b);

    assert_eq!(g.wires().count(), 0, "both wires touched b");
    assert!(g.out_pin(out(a, 0)).remotes.is_empty());
    assert!(g.in_pin(inp(c, 0)).remotes.is_empty());
}

#[test]
fn wires_of_finds_both_directions() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(50.0, 0.0), ());
    let c = g.insert_node(pos2(100.0, 0.0), ());

    g.connect(out(a, 0), inp(b, 0));
    g.connect(out(b, 0), inp(c, 0));

    assert_eq!(g.wires_of(b).count(), 2, "one incoming, one outgoing");
    assert_eq!(g.wires_of(a).count(), 1);
}

// ── trim_wires_to ───────────────────────────────────────────────────

/// The reconciliation gap that had no owner before P3: a wire to a pin
/// index the viewer no longer reports is silently skipped by the
/// renderer and lives on in the set, reappearing if the count grows
/// back. Shrinking a pin count must *remove* it.
#[test]
fn trim_wires_to_removes_wires_beyond_the_new_pin_count() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(50.0, 0.0), ());

    g.connect(out(a, 0), inp(b, 0));
    g.connect(out(a, 1), inp(b, 1));
    g.connect(out(a, 2), inp(b, 2));
    assert_eq!(g.wires().count(), 3);

    let dropped = g.trim_wires_to(a, 0, 2);

    assert_eq!(dropped, 1, "output pin 2 is gone");
    assert_eq!(g.wires().count(), 2);
    assert!(
        g.wires().all(|(o, _)| o.output < 2),
        "no wire may address a pin the node no longer has"
    );
}

#[test]
fn trim_wires_to_leaves_other_nodes_alone() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(50.0, 0.0), ());
    let c = g.insert_node(pos2(100.0, 0.0), ());

    g.connect(out(a, 5), inp(b, 0));
    g.connect(out(c, 5), inp(b, 1));

    let dropped = g.trim_wires_to(a, 0, 1);

    assert_eq!(dropped, 1);
    assert_eq!(g.wires().count(), 1, "c's wire is untouched");
    assert_eq!(g.wires_of(c).count(), 1);
}

// ── Size override ───────────────────────────────────────────────────

#[test]
fn size_override_round_trips_and_clears() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());

    assert_eq!(g.size_override_of(a), None);
    g.set_size_override(a, Some(mara_graph::vec2(512.0, 384.0)));
    assert_eq!(g.size_override_of(a), Some(mara_graph::vec2(512.0, 384.0)));
    g.set_size_override(a, None);
    assert_eq!(g.size_override_of(a), None);
}
