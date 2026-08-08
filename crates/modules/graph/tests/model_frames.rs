//! Frame groups at the model level — PLAN_NODE.md P4.
//!
//! `T = ()` throughout, zero rendering, synthetic node rects supplied
//! by a closure. That is the point of the design: every decision a
//! frame makes — what a drag moves, what a delete keeps, where the box
//! sits, which group a drop lands in — is arithmetic over the model,
//! and the renderer is left with one line of dispatch per gesture.

use mara_core::vocab::{Color32, Pos2, Rect, Vec2};
use mara_graph::{DisposeMode, Graph, NodeId, fit_frame_bounds, pos2};

/// Every node is a fixed 100×40 box at its own position, so expected
/// bounds are arithmetic a reader can check by hand.
fn rects_of(graph: &Graph<()>) -> impl Fn(NodeId) -> Rect + '_ {
    move |id: NodeId| {
        let p = graph.get_node_info(id).expect("live node").pos;
        Rect::from_min_size(Pos2::new(p.x, p.y), Vec2::new(100.0, 40.0))
    }
}

fn grey() -> Color32 {
    Color32::from_gray(128)
}

fn empty_bounds() -> Rect {
    Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(0.0, 0.0))
}

// ── Membership ──────────────────────────────────────────────────────

#[test]
fn membership_round_trips() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let f = g.insert_frame("group", grey(), empty_bounds());

    assert_eq!(g.frame_of(a), None);
    g.set_node_frame(a, Some(f));
    assert_eq!(g.frame_of(a), Some(f));
    assert_eq!(g.frame_members(f).collect::<Vec<_>>(), vec![a]);

    g.set_node_frame(a, None);
    assert_eq!(g.frame_of(a), None);
    assert_eq!(g.frame_members(f).count(), 0);
}

/// A `FrameId` that does not resolve must not be stored. Storing it
/// would leave a dangling membership for `repair` to find later, and
/// silently place the node in a group that does not exist meanwhile.
#[test]
fn attaching_to_a_dead_frame_is_a_detach_not_a_dangling_reference() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let f = g.insert_frame("group", grey(), empty_bounds());
    g.set_node_frame(a, Some(f));
    g.remove_frame(f, DisposeMode::Dissolve);

    g.set_node_frame(a, Some(f));
    assert_eq!(g.frame_of(a), None, "a dead frame id must not be stored");
}

#[test]
fn removing_a_node_takes_its_membership_with_it() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(200.0, 0.0), ());
    let f = g.insert_frame("group", grey(), empty_bounds());
    g.set_node_frame(a, Some(f));
    g.set_node_frame(b, Some(f));

    g.remove_node(a);

    assert_eq!(g.frame_members(f).collect::<Vec<_>>(), vec![b]);
}

// ── Disposal ────────────────────────────────────────────────────────

/// Deleting a box must not delete work. Members go to the frame's own
/// parent, not to the root, so removing a middle frame from a nest
/// leaves its contents inside the surviving outer group.
#[test]
fn dissolve_reparents_members_to_the_frames_parent() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    assert!(g.set_frame_parent(inner, Some(outer)));
    g.set_node_frame(a, Some(inner));

    g.remove_frame(inner, DisposeMode::Dissolve);

    assert!(g.contains(a), "dissolve must keep the node");
    assert_eq!(g.frame_of(a), Some(outer), "and hand it to the parent");
}

#[test]
fn dissolve_reparents_child_frames_too() {
    let mut g = Graph::<()>::new();
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let mid = g.insert_frame("mid", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    assert!(g.set_frame_parent(mid, Some(outer)));
    assert!(g.set_frame_parent(inner, Some(mid)));

    g.remove_frame(mid, DisposeMode::Dissolve);

    assert_eq!(
        g.frame(inner).unwrap().parent,
        Some(outer),
        "a child must never be left pointing at a removed parent"
    );
}

#[test]
fn purge_removes_members_transitively() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(200.0, 0.0), ());
    let outside = g.insert_node(pos2(600.0, 0.0), ());
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    assert!(g.set_frame_parent(inner, Some(outer)));
    g.set_node_frame(a, Some(outer));
    g.set_node_frame(b, Some(inner));

    g.remove_frame(outer, DisposeMode::Purge);

    assert!(!g.contains(a));
    assert!(!g.contains(b), "purge must reach through child frames");
    assert!(g.contains(outside), "and stop at the boundary");
    assert_eq!(g.frames().count(), 0, "the child frame goes too");
}

// ── Nesting ─────────────────────────────────────────────────────────

/// One bad drag is all it takes to build a parent chain that never
/// terminates; the first thing to walk it hangs. This is the guard.
#[test]
fn can_parent_frame_rejects_self_and_every_ancestor() {
    let mut g = Graph::<()>::new();
    let a = g.insert_frame("a", grey(), empty_bounds());
    let b = g.insert_frame("b", grey(), empty_bounds());
    let c = g.insert_frame("c", grey(), empty_bounds());
    assert!(g.set_frame_parent(b, Some(a)));
    assert!(g.set_frame_parent(c, Some(b)));

    assert!(!g.can_parent_frame(a, Some(a)), "self");
    assert!(!g.can_parent_frame(a, Some(b)), "direct descendant");
    assert!(!g.can_parent_frame(a, Some(c)), "transitive descendant");
    assert!(g.can_parent_frame(a, None), "detaching is always allowed");
}

#[test]
fn a_rejected_reparent_changes_nothing() {
    let mut g = Graph::<()>::new();
    let a = g.insert_frame("a", grey(), empty_bounds());
    let b = g.insert_frame("b", grey(), empty_bounds());
    assert!(g.set_frame_parent(b, Some(a)));

    assert!(!g.set_frame_parent(a, Some(b)), "would cycle");
    assert_eq!(g.frame(a).unwrap().parent, None);
    assert_eq!(g.frame(b).unwrap().parent, Some(a));
}

#[test]
fn frame_depth_counts_ancestors() {
    let mut g = Graph::<()>::new();
    let a = g.insert_frame("a", grey(), empty_bounds());
    let b = g.insert_frame("b", grey(), empty_bounds());
    let c = g.insert_frame("c", grey(), empty_bounds());
    g.set_frame_parent(b, Some(a));
    g.set_frame_parent(c, Some(b));

    assert_eq!(g.frame_depth(a), 0);
    assert_eq!(g.frame_depth(b), 1);
    assert_eq!(g.frame_depth(c), 2);
}

// ── Drag targets ────────────────────────────────────────────────────

#[test]
fn dragging_a_frame_moves_its_members_transitively_and_without_duplicates() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(200.0, 0.0), ());
    let outside = g.insert_node(pos2(600.0, 0.0), ());
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    g.set_frame_parent(inner, Some(outer));
    g.set_node_frame(a, Some(outer));
    g.set_node_frame(b, Some(inner));

    let mut targets = g.drag_targets_frame(outer).to_vec();
    targets.sort_unstable_by_key(|n| n.0);

    assert_eq!(targets, vec![a, b].tap_sorted());
    assert!(!targets.contains(&outside));
}

/// Dragging a node that is part of the selection moves the whole
/// selection; dragging anything else moves only it.
#[test]
fn dragging_a_selected_node_moves_the_selection_and_an_unselected_one_moves_alone() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(200.0, 0.0), ());
    let c = g.insert_node(pos2(400.0, 0.0), ());
    let selected = [a, b];

    let mut with_a = g.drag_targets_node(a, &selected).to_vec();
    with_a.sort_unstable_by_key(|n| n.0);
    assert_eq!(with_a, vec![a, b].tap_sorted());

    assert_eq!(g.drag_targets_node(c, &selected).to_vec(), vec![c]);
}

/// A stale `NodeId` in the selection must not become a move target —
/// slab recycles keys, so it would move an unrelated node.
#[test]
fn drag_targets_skip_dead_nodes() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let b = g.insert_node(pos2(200.0, 0.0), ());
    let selected = [a, b];
    g.remove_node(b);

    assert_eq!(g.drag_targets_node(a, &selected).to_vec(), vec![a]);
}

// ── Geometry ────────────────────────────────────────────────────────

/// The Blender T40094 regression: unioning member rects alone leaves
/// the topmost node sitting under the title text. The fitted bounds
/// must reserve exactly one title band above it.
#[test]
fn fitted_bounds_reserve_one_title_band_above_the_topmost_member() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(100.0, 100.0), ());
    let f = g.insert_frame("group", grey(), empty_bounds());
    g.set_node_frame(a, Some(f));
    g.frame_mut(f).unwrap().label_size = 20.0;
    let band = g.frame(f).unwrap().title_band_height();

    let rects = rects_of(&g);
    let bounds = fit_frame_bounds(&g, f, &rects, 8.0).expect("a member means bounds");

    // member rect is (100,100)..(200,140); padding 8 each way; then the
    // band is carved out above.
    assert!((bounds.min.x - 92.0).abs() < 0.01, "left = 100 - padding");
    assert!(
        (bounds.min.y - (92.0 - band)).abs() < 0.01,
        "top = 100 - padding - band, got {}",
        bounds.min.y
    );
    assert!((bounds.max.x - 208.0).abs() < 0.01);
    assert!((bounds.max.y - 148.0).abs() < 0.01);
    assert!(
        bounds.min.y < 100.0 - band,
        "the band must sit entirely above the node"
    );
}

#[test]
fn fitted_bounds_grow_to_contain_a_nested_child_frame() {
    let mut g = Graph::<()>::new();
    let near = g.insert_node(pos2(0.0, 0.0), ());
    let far = g.insert_node(pos2(500.0, 300.0), ());
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    g.set_frame_parent(inner, Some(outer));
    g.set_node_frame(near, Some(outer));
    g.set_node_frame(far, Some(inner));

    let rects = rects_of(&g);
    let outer_b = fit_frame_bounds(&g, outer, &rects, 8.0).unwrap();
    let inner_b = fit_frame_bounds(&g, inner, &rects, 8.0).unwrap();

    assert!(
        outer_b.contains(inner_b.min) && outer_b.contains(inner_b.max),
        "outer {outer_b:?} must contain inner {inner_b:?}"
    );
}

#[test]
fn an_empty_shrink_frame_has_no_bounds() {
    let mut g = Graph::<()>::new();
    let f = g.insert_frame("empty", grey(), empty_bounds());
    let rects = rects_of(&g);
    assert!(
        fit_frame_bounds(&g, f, &rects, 8.0).is_none(),
        "an empty group has no meaningful size; inventing one paints a stray box"
    );
}

/// A manually-sized frame must ignore where its members are — that is
/// the entire distinction between `shrink` and not.
#[test]
fn a_non_shrink_frame_ignores_member_movement() {
    let mut g = Graph::<()>::new();
    let a = g.insert_node(pos2(0.0, 0.0), ());
    let fixed = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(300.0, 200.0));
    let f = g.insert_frame("fixed", grey(), fixed);
    g.set_node_frame(a, Some(f));
    g.frame_mut(f).unwrap().shrink = false;

    let before = {
        let rects = rects_of(&g);
        g.frame_bounds(f, &rects, 8.0).unwrap()
    };
    g.get_node_info_mut(a).unwrap().pos = pos2(900.0, 900.0);
    let after = {
        let rects = rects_of(&g);
        g.frame_bounds(f, &rects, 8.0).unwrap()
    };

    assert_eq!(before, after);
    assert_eq!(after, fixed);
}

/// Dropping into overlapping groups must attach to the one it looks
/// like it landed in — the innermost.
#[test]
fn frame_at_returns_the_innermost_frame() {
    let mut g = Graph::<()>::new();
    let outer_n = g.insert_node(pos2(0.0, 0.0), ());
    let inner_n = g.insert_node(pos2(100.0, 100.0), ());
    let outer = g.insert_frame("outer", grey(), empty_bounds());
    let inner = g.insert_frame("inner", grey(), empty_bounds());
    g.set_frame_parent(inner, Some(outer));
    g.set_node_frame(outer_n, Some(outer));
    g.set_node_frame(inner_n, Some(inner));

    let rects = rects_of(&g);
    // A point on the inner node is inside both boxes.
    let hit = g.frame_at(Pos2::new(120.0, 120.0), &rects, 8.0);
    assert_eq!(hit, Some(inner), "innermost wins");

    // A point on the outer node only.
    let hit = g.frame_at(Pos2::new(20.0, 20.0), &rects, 8.0);
    assert_eq!(hit, Some(outer));

    // Nowhere near either.
    assert_eq!(g.frame_at(Pos2::new(5000.0, 5000.0), &rects, 8.0), None);
}

// ── Repair ──────────────────────────────────────────────────────────

#[test]
fn repair_breaks_an_artificially_cyclic_parent_chain() {
    let mut g = Graph::<()>::new();
    let a = g.insert_frame("a", grey(), empty_bounds());
    let b = g.insert_frame("b", grey(), empty_bounds());
    g.set_frame_parent(b, Some(a));
    // Force the cycle the guard would refuse, as a corrupt document
    // could.
    g.frame_mut(a).unwrap().parent = Some(b);

    g.repair();

    let a_parent = g.frame(a).unwrap().parent;
    let b_parent = g.frame(b).unwrap().parent;
    assert!(
        a_parent.is_none() || b_parent.is_none(),
        "repair must break the cycle: a={a_parent:?} b={b_parent:?}"
    );
    // And the walkers must terminate.
    let _ = g.frame_depth(a);
    let _ = g.frame_depth(b);
}

trait TapSorted {
    fn tap_sorted(self) -> Self;
}

impl TapSorted for Vec<NodeId> {
    fn tap_sorted(mut self) -> Self {
        self.sort_unstable_by_key(|n| n.0);
        self
    }
}
