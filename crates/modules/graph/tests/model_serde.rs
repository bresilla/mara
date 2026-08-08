//! The document format, exercised for the first time — PLAN_NODE.md P3.
//!
//! `mara_graph`'s `serde` feature had never compiled (it enabled
//! `egui/serde` but not `mara_core/serde`, and `vocab::{Pos2, Vec2,
//! Rect}` carried no derives at all), so no saved graph has ever
//! existed and there is no backward-compatibility burden. These tests
//! settle the format instead of discovering it later.

#![cfg(feature = "serde")]

use mara_graph::{Graph, InPinId, NodeId, OutPinId, pos2};

fn out(node: NodeId, output: usize) -> OutPinId {
    OutPinId { node, output }
}

fn inp(node: NodeId, input: usize) -> InPinId {
    InPinId { node, input }
}

fn demo() -> Graph<u32> {
    let mut g = Graph::<u32>::new();
    let a = g.insert_node(pos2(10.0, 20.0), 1);
    let b = g.insert_node(pos2(120.0, 60.0), 2);
    let c = g.insert_node_collapsed(pos2(240.0, 20.0), 3);
    g.connect(out(a, 0), inp(b, 0));
    g.connect(out(b, 1), inp(c, 0));
    g.set_size_override(c, Some(mara_graph::vec2(512.0, 384.0)));
    g
}

#[test]
fn a_graph_round_trips_through_json() {
    let before = demo();
    let json = serde_json::to_string(&before).expect("serialise");
    let mut after: Graph<u32> = serde_json::from_str(&json).expect("deserialise");
    after.repair();

    assert_eq!(after.len(), before.len());

    let mut wires_before: Vec<_> = before.wires().collect();
    let mut wires_after: Vec<_> = after.wires().collect();
    wires_before.sort_unstable();
    wires_after.sort_unstable();
    assert_eq!(wires_after, wires_before, "wire set must survive");

    for (id, value) in before.node_ids() {
        assert_eq!(after.get_node(id), Some(value), "payload at {id:?}");
        assert_eq!(
            after.get_node_info(id).map(|n| n.pos),
            before.get_node_info(id).map(|n| n.pos),
            "position at {id:?}"
        );
        assert_eq!(
            after.get_node_info(id).map(|n| n.open),
            before.get_node_info(id).map(|n| n.open),
            "collapsed state at {id:?}"
        );
        assert_eq!(
            after.size_override_of(id),
            before.size_override_of(id),
            "size override at {id:?}"
        );
    }
}

/// The wire adjacency index is `#[serde(skip)]`-equivalent — it is not
/// in the document at all, and `Wires` has a hand-written `Deserialize`
/// where a `skip` attribute would have been inert anyway. If the impl
/// forgets to rebuild it, every pin loses its `remotes` and the graph
/// renders with all wires missing while `wires()` still lists them.
/// That is a silent, total failure, so it gets its own test.
#[test]
fn a_deserialised_graph_has_a_live_wire_index() {
    let before = demo();
    let json = serde_json::to_string(&before).unwrap();
    let after: Graph<u32> = serde_json::from_str(&json).unwrap();

    // Deliberately WITHOUT calling `repair()` — the index has to be
    // correct straight out of deserialisation, because nothing forces
    // a caller to repair.
    let mut any = false;
    for (id, _) in after.node_ids() {
        for pin in 0..3 {
            if !after.out_pin(out(id, pin)).remotes.is_empty() {
                any = true;
            }
            if !after.in_pin(inp(id, pin)).remotes.is_empty() {
                any = true;
            }
        }
    }
    assert!(
        any,
        "no pin reported a remote — the index was not rebuilt on load"
    );

    // And it must agree with the wire set exactly.
    for (o, i) in after.wires() {
        assert!(
            after.out_pin(o).remotes.contains(&i),
            "{o:?} -> {i:?} present in the set but missing from the index"
        );
        assert!(
            after.in_pin(i).remotes.contains(&o),
            "{o:?} -> {i:?} present in the set but missing from the reverse index"
        );
    }
}

/// A document written before uids existed has none. `repair` must mint
/// them rather than leaving every node on the `NodeUid(0)` sentinel,
/// which would make them all alias.
#[test]
fn repair_assigns_uids_to_a_document_that_lacks_them() {
    // `Slab` serialises as a map keyed by slab index, not a sequence.
    let legacy = r#"{
        "nodes": {
            "0": { "value": 7, "pos": { "x": 0.0, "y": 0.0 }, "open": true },
            "1": { "value": 8, "pos": { "x": 50.0, "y": 0.0 }, "open": true }
        },
        "wires": [],
        "ext": { "next_uid": 0 }
    }"#;

    let mut g: Graph<u32> = serde_json::from_str(legacy).expect("legacy document loads");
    assert_eq!(g.len(), 2);

    g.repair();

    let uids: Vec<_> = g.node_ids().map(|(id, _)| g.uid_of(id).unwrap()).collect();
    assert!(
        uids.iter().all(|u| u.is_assigned()),
        "sentinel must be replaced"
    );
    assert_ne!(uids[0], uids[1], "and the replacements must be distinct");
    for (id, _) in g.node_ids() {
        assert_eq!(g.by_uid(g.uid_of(id).unwrap()), Some(id));
    }
}

/// A graph with no grouping state must still load from the minimal
/// `{nodes, wires}` shape — `ext` is `#[serde(default)]` precisely so
/// that adding it did not invalidate the format.
#[test]
fn a_document_without_the_ext_field_loads() {
    let minimal = r#"{
        "nodes": { "0": { "value": 1, "pos": { "x": 3.0, "y": 4.0 }, "open": true } },
        "wires": []
    }"#;

    let mut g: Graph<u32> = serde_json::from_str(minimal).expect("minimal document loads");
    g.repair();
    assert_eq!(g.len(), 1);
    assert_eq!(g.revision(), 0, "revision is session-only and starts at 0");
}
