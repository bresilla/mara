//!
//! # Node graph
//!
//! Provides a node-graph container.
//!
//!

// `non_ascii_idents` was here in the upstream graph crate but is
// ignored at module level — moved to a vendored sub-module, the
// only place to opt into it is the workspace root, which is out
// of scope. Drop it from the list to silence the lint warning.
// Upstream graph shipped strict `#![deny(missing_docs, …)]` at the
// module root. Vendored here, those denies break our build whenever
// a contributor lands a `pub` item without a doc comment. Downgraded
// to warnings so the crate stays compiling.
#![allow(missing_docs)]
#![deny(unsafe_code)]
#![allow(clippy::range_plus_one, clippy::inline_always, clippy::use_self)]

pub mod camera;
pub mod chrome;
pub mod frames;
pub mod nav;
pub mod subgraph;
pub mod ui;

use std::ops::{Index, IndexMut};

use frames::{Frame, FrameId};
use mara_core::vocab::{Pos2, Vec2};
use slab::Slab;
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet};

impl<T> Default for Graph<T> {
    fn default() -> Self {
        Graph::new()
    }
}

/// Node identifier.
///
/// This is newtype wrapper around [`usize`] that implements
/// necessary traits, but omits arithmetic operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct NodeId(pub usize);

/// Stable identity for a node, unique within one [`Graph`] level.
///
/// [`NodeId`] is a raw `slab` key and slab **recycles vacated keys** —
/// delete node 7, insert anything, and the new node *is* node 7. That
/// is fine for a value used within one frame and fatal for anything
/// persisted: frame membership, subgraph instance bindings and saved
/// documents would all silently rebind to whatever took the slot.
///
/// The rule this type exists to enforce: **anything persisted uses
/// `NodeUid`; anything per-frame uses `NodeId`.**
///
/// Uniqueness is per-level rather than per-document, because every map
/// keyed by one lives on a single [`Graph`]. `NodeUid(0)` is the
/// unassigned sentinel, repaired by [`Graph::repair`] — that is what
/// lets a document serialised before uids existed load without them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct NodeUid(pub u64);

impl NodeUid {
    /// The sentinel meaning "no uid assigned yet".
    pub const UNASSIGNED: Self = Self(0);

    /// Whether this uid has been assigned.
    #[must_use]
    pub const fn is_assigned(self) -> bool {
        self.0 != 0
    }
}

/// Node of the graph.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct Node<T> {
    /// Node generic value.
    pub value: T,

    /// Position of the top-left corner of the node.
    /// This does not include frame margin.
    pub pos: Pos2,

    /// Flag indicating that the node is open - not collapsed.
    pub open: bool,

    /// Stable identity — private, read via [`Graph::uid_of`].
    ///
    /// Private because `get_node_info_mut` hands out `&mut Node<T>`: a
    /// public field would let a caller assign a duplicate or a foreign
    /// uid and silently corrupt every map keyed by one.
    #[cfg_attr(feature = "serde", serde(default))]
    uid: NodeUid,

    /// The frame group this node belongs to, if any — private, read via
    /// [`Graph::frame_of`] and written via [`Graph::set_node_frame`].
    ///
    /// Membership lives on the node rather than as a member list on the
    /// frame: re-parenting is then a single write, a removed node
    /// cannot resurrect into a group, and there is no second collection
    /// to keep in step with the slab.
    #[cfg_attr(feature = "serde", serde(default))]
    frame: Option<FrameId>,

    /// Explicit size in points, overriding the measured content size.
    ///
    /// The only channel an app has to say "this node is 512×384" — node
    /// size is otherwise derived entirely from what the viewer draws,
    /// which a live image or a chart cannot express. Read via
    /// [`Graph::size_override_of`].
    #[cfg_attr(feature = "serde", serde(default))]
    size_override: Option<Vec2>,
}

/// Output pin identifier.
/// Cosists of node id and pin index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OutPinId {
    /// Node id.
    pub node: NodeId,

    /// Output pin index.
    pub output: usize,
}

/// Input pin identifier. Cosists of node id and pin index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct InPinId {
    /// Node id.
    pub node: NodeId,

    /// Input pin index.
    pub input: usize,
}

/// Connection between two nodes.
///
/// Nodes may support multiple connections to the same input or output.
/// But duplicate connections between same input and the same output are not allowed.
/// Attempt to insert existing connection will be ignored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
struct Wire {
    out_pin: OutPinId,
    in_pin: InPinId,
}

/// The wire set, plus the two adjacency indexes that keep pin
/// resolution off the critical path.
///
/// Before PLAN_NODE.md P3 `wired_inputs`/`wired_outputs` were full
/// linear scans of `wires`, and `InPin::new`/`OutPin::new` call them —
/// while the renderer builds an `InPin` and an `OutPin` for *every pin
/// of every node every frame*. At 2 000 nodes × 4 pins × 3 000 wires
/// that is ~24M iterations and ~8 000 allocations per frame before
/// anything is painted. The indexes turn each of those scans into a
/// hash lookup.
///
/// Both are derived state: they are rebuilt from `wires`, never
/// serialised, and [`Wires::reindex`] restores them.
#[derive(Clone, Debug)]
struct Wires {
    wires: HashSet<Wire>,
    /// Output pins feeding each input pin.
    in_of: HashMap<InPinId, SmallVec<[OutPinId; 2]>>,
    /// Input pins fed by each output pin.
    out_of: HashMap<OutPinId, SmallVec<[InPinId; 2]>>,
}

#[cfg(feature = "serde")]
impl serde::Serialize for Wires {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;

        let mut seq = serializer.serialize_seq(Some(self.wires.len()))?;
        for wire in &self.wires {
            seq.serialize_element(&wire)?;
        }
        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Wires {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = HashSet<Wire>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a sequence of wires")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut wires = HashSet::new();
                while let Some(wire) = seq.next_element()? {
                    wires.insert(wire);
                }
                Ok(wires)
            }
        }

        let wires = deserializer.deserialize_seq(Visitor)?;
        // The adjacency indexes are derived, and this impl is written
        // by hand — `#[serde(skip)]` on the fields would be inert here,
        // so they are rebuilt explicitly. Skipping this step does not
        // merely lose the speedup: `wired_inputs`/`wired_outputs` would
        // answer *empty* for every pin, so a loaded graph would render
        // with all of its wires missing.
        let mut this = Wires {
            wires,
            in_of: HashMap::new(),
            out_of: HashMap::new(),
        };
        this.reindex();
        Ok(this)
    }
}

impl Wires {
    fn new() -> Self {
        Wires {
            wires: HashSet::new(),
            in_of: HashMap::new(),
            out_of: HashMap::new(),
        }
    }

    /// Rebuild both adjacency indexes from `wires`. Idempotent.
    fn reindex(&mut self) {
        self.in_of.clear();
        self.out_of.clear();
        for wire in &self.wires {
            self.in_of
                .entry(wire.in_pin)
                .or_default()
                .push(wire.out_pin);
            self.out_of
                .entry(wire.out_pin)
                .or_default()
                .push(wire.in_pin);
        }
    }

    fn index_insert(&mut self, wire: Wire) {
        self.in_of
            .entry(wire.in_pin)
            .or_default()
            .push(wire.out_pin);
        self.out_of
            .entry(wire.out_pin)
            .or_default()
            .push(wire.in_pin);
    }

    fn index_remove(&mut self, wire: &Wire) {
        if let Some(v) = self.in_of.get_mut(&wire.in_pin) {
            v.retain(|o| *o != wire.out_pin);
            if v.is_empty() {
                self.in_of.remove(&wire.in_pin);
            }
        }
        if let Some(v) = self.out_of.get_mut(&wire.out_pin) {
            v.retain(|i| *i != wire.in_pin);
            if v.is_empty() {
                self.out_of.remove(&wire.out_pin);
            }
        }
    }

    fn insert(&mut self, wire: Wire) -> bool {
        if self.wires.insert(wire) {
            self.index_insert(wire);
            true
        } else {
            false
        }
    }

    fn remove(&mut self, wire: &Wire) -> bool {
        if self.wires.remove(wire) {
            self.index_remove(wire);
            true
        } else {
            false
        }
    }

    /// Drop every wire matching `keep_out`, maintaining both indexes.
    ///
    /// One helper for the three bulk removals so the index bookkeeping
    /// cannot drift between them.
    fn drop_matching(&mut self, doomed: impl Fn(&Wire) -> bool) -> usize {
        let removed: Vec<Wire> = self.wires.iter().copied().filter(|w| doomed(w)).collect();
        for wire in &removed {
            self.wires.remove(wire);
            self.index_remove(wire);
        }
        removed.len()
    }

    fn drop_node(&mut self, node: NodeId) -> usize {
        self.drop_matching(|wire| wire.out_pin.node == node || wire.in_pin.node == node)
    }

    fn drop_inputs(&mut self, pin: InPinId) -> usize {
        self.drop_matching(|wire| wire.in_pin == pin)
    }

    fn drop_outputs(&mut self, pin: OutPinId) -> usize {
        self.drop_matching(|wire| wire.out_pin == pin)
    }

    fn wired_inputs(&self, out_pin: OutPinId) -> impl Iterator<Item = InPinId> + '_ {
        self.out_of.get(&out_pin).into_iter().flatten().copied()
    }

    fn wired_outputs(&self, in_pin: InPinId) -> impl Iterator<Item = OutPinId> + '_ {
        self.in_of.get(&in_pin).into_iter().flatten().copied()
    }

    fn iter(&self) -> impl Iterator<Item = Wire> + '_ {
        self.wires.iter().copied()
    }
}

/// Graph is generic node-graph container.
///
/// It holds graph state - positioned nodes and wires between their pins.
/// It can be rendered using [`Graph::show`].
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Graph<T> {
    // #[cfg_attr(feature = "serde", serde(with = "serde_nodes"))]
    nodes: Slab<Node<T>>,
    wires: Wires,
    /// Everything added after the original vendored model, in one
    /// field.
    ///
    /// One field rather than several so the serde shape stays close to
    /// `{nodes, wires}` — a document written before this existed loads
    /// as a graph with no grouping — and so [`Graph::repair`] has a
    /// single place to sweep.
    #[cfg_attr(feature = "serde", serde(default))]
    ext: GraphExt,
}

/// Model state added by PLAN_NODE.md: stable identity, and the maps
/// that grouping and subgraphs key off it.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GraphExt {
    /// Next uid to hand out. Monotonic; never reused, unlike the slab
    /// keys it exists to compensate for.
    next_uid: u64,
    /// Frame groups declared at this level. Membership is on the node,
    /// not here.
    #[cfg_attr(feature = "serde", serde(default))]
    frames: Slab<Frame>,
    /// Nodes at this level that are subgraph instances.
    #[cfg_attr(feature = "serde", serde(default))]
    instances: HashMap<NodeUid, subgraph::DefId>,
    /// Only non-empty inside a definition body: which node *is* which
    /// boundary port.
    #[cfg_attr(feature = "serde", serde(default))]
    port_nodes: HashMap<NodeUid, subgraph::PortId>,
    /// Bumped on any structural or positional mutation, so a consumer
    /// caching something derived from this graph — a fitted frame
    /// bounds, an interior thumbnail — can tell whether to recompute.
    ///
    /// Session-only: it is not serialised and resets to 0 on load, so a
    /// cache that persists a revision alongside its artefact must force
    /// a miss after loading rather than comparing.
    #[cfg_attr(feature = "serde", serde(skip))]
    revision: u32,
    /// Reverse index for [`Graph::by_uid`]. Derived, rebuilt by
    /// [`Graph::repair`].
    #[cfg_attr(feature = "serde", serde(skip))]
    by_uid: HashMap<NodeUid, NodeId>,
}

impl<T> Graph<T> {
    /// Create a new empty Graph.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mara_graph::Graph;
    /// let graph = Graph::<()>::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Graph {
            nodes: Slab::new(),
            wires: Wires::new(),
            ext: GraphExt::default(),
        }
    }

    /// Adds a node to the Graph.
    /// Returns the index of the node.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mara_graph::Graph;
    /// let mut graph = Graph::<()>::new();
    /// graph.insert_node(mara_graph::pos2(0.0, 0.0), ());
    /// ```
    pub fn insert_node(&mut self, pos: Pos2, node: T) -> NodeId {
        self.insert_node_with(pos, node, true)
    }

    /// Adds a node to the Graph in collapsed state.
    /// Returns the index of the node.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mara_graph::Graph;
    /// let mut graph = Graph::<()>::new();
    /// graph.insert_node_collapsed(mara_graph::pos2(0.0, 0.0), ());
    /// ```
    pub fn insert_node_collapsed(&mut self, pos: Pos2, node: T) -> NodeId {
        self.insert_node_with(pos, node, false)
    }

    /// The one insertion path, so uid allocation and the reverse index
    /// cannot be added to one entry point and forgotten on the other.
    fn insert_node_with(&mut self, pos: Pos2, node: T, open: bool) -> NodeId {
        let uid = self.alloc_uid();
        let idx = self.nodes.insert(Node {
            value: node,
            pos,
            open,
            uid,
            frame: None,
            size_override: None,
        });
        let id = NodeId(idx);
        self.ext.by_uid.insert(uid, id);
        self.ext.revision = self.ext.revision.wrapping_add(1);
        id
    }

    fn alloc_uid(&mut self) -> NodeUid {
        // Skip 0: it is the "unassigned" sentinel that lets a document
        // written before uids existed be repaired on load.
        self.ext.next_uid = self.ext.next_uid.max(1);
        let uid = NodeUid(self.ext.next_uid);
        self.ext.next_uid += 1;
        uid
    }

    /// Opens or collapses a node.
    ///
    /// # Panics
    ///
    /// Panics if the node does not exist.
    #[track_caller]
    pub fn open_node(&mut self, node: NodeId, open: bool) {
        self.nodes[node.0].open = open;
    }

    /// Removes a node from the Graph.
    /// Returns the node if it was removed.
    ///
    /// # Panics
    ///
    /// Panics if the node does not exist.
    ///
    /// # Examples
    ///
    /// ```
    /// # use mara_graph::Graph;
    /// let mut graph = Graph::<()>::new();
    /// let node = graph.insert_node(mara_graph::pos2(0.0, 0.0), ());
    /// graph.remove_node(node);
    /// ```
    #[track_caller]
    pub fn remove_node(&mut self, idx: NodeId) -> T {
        let node = self.nodes.remove(idx.0);
        // Drop every map entry keyed by this uid before the slab key
        // can be handed to a different node. Leaving `by_uid` would
        // make a dead uid resolve to a live, unrelated node; leaving
        // `instances` would let a `NodePath` through a deleted instance
        // keep resolving, so navigating to a level that no longer
        // exists would silently succeed.
        self.ext.by_uid.remove(&node.uid);
        self.ext.instances.remove(&node.uid);
        self.ext.port_nodes.remove(&node.uid);
        self.wires.drop_node(idx);
        self.ext.revision = self.ext.revision.wrapping_add(1);
        node.value
    }

    /// Connects two nodes.
    /// Returns true if the connection was successful.
    /// Returns false if the connection already exists.
    ///
    /// # Panics
    ///
    /// Panics if either node does not exist.
    #[track_caller]
    pub fn connect(&mut self, from: OutPinId, to: InPinId) -> bool {
        assert!(self.nodes.contains(from.node.0));
        assert!(self.nodes.contains(to.node.0));

        let wire = Wire {
            out_pin: from,
            in_pin: to,
        };
        self.wires.insert(wire)
    }

    /// Disconnects two nodes.
    /// Returns true if the connection was removed.
    ///
    /// # Panics
    ///
    /// Panics if either node does not exist.
    #[track_caller]
    pub fn disconnect(&mut self, from: OutPinId, to: InPinId) -> bool {
        assert!(self.nodes.contains(from.node.0));
        assert!(self.nodes.contains(to.node.0));

        let wire = Wire {
            out_pin: from,
            in_pin: to,
        };

        self.wires.remove(&wire)
    }

    /// Removes all connections to the node's pin.
    ///
    /// Returns number of removed connections.
    ///
    /// # Panics
    ///
    /// Panics if the node does not exist.
    #[track_caller]
    pub fn drop_inputs(&mut self, pin: InPinId) -> usize {
        assert!(self.nodes.contains(pin.node.0));
        self.wires.drop_inputs(pin)
    }

    /// Removes all connections from the node's pin.
    /// Returns number of removed connections.
    ///
    /// # Panics
    ///
    /// Panics if the node does not exist.
    #[track_caller]
    pub fn drop_outputs(&mut self, pin: OutPinId) -> usize {
        assert!(self.nodes.contains(pin.node.0));
        self.wires.drop_outputs(pin)
    }

    /// Returns reference to the node.
    #[must_use]
    pub fn get_node(&self, idx: NodeId) -> Option<&T> {
        self.nodes.get(idx.0).map(|node| &node.value)
    }

    /// Returns mutable reference to the node.
    pub fn get_node_mut(&mut self, idx: NodeId) -> Option<&mut T> {
        match self.nodes.get_mut(idx.0) {
            Some(node) => Some(&mut node.value),
            None => None,
        }
    }

    /// Returns reference to the node data.
    #[must_use]
    pub fn get_node_info(&self, idx: NodeId) -> Option<&Node<T>> {
        self.nodes.get(idx.0)
    }

    /// Returns mutable reference to the node data.
    pub fn get_node_info_mut(&mut self, idx: NodeId) -> Option<&mut Node<T>> {
        self.nodes.get_mut(idx.0)
    }

    /// Iterates over shared references to each node.
    pub fn nodes(&self) -> NodesIter<'_, T> {
        NodesIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node.
    pub fn nodes_mut(&mut self) -> NodesIterMut<'_, T> {
        NodesIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over shared references to each node and its position.
    pub fn nodes_pos(&self) -> NodesPosIter<'_, T> {
        NodesPosIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node and its position.
    pub fn nodes_pos_mut(&mut self) -> NodesPosIterMut<'_, T> {
        NodesPosIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over shared references to each node and its identifier.
    pub fn node_ids(&self) -> NodesIdsIter<'_, T> {
        NodesIdsIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node and its identifier.
    pub fn nodes_ids_mut(&mut self) -> NodesIdsIterMut<'_, T> {
        NodesIdsIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over shared references to each node, its position and its identifier.
    pub fn nodes_pos_ids(&self) -> NodesPosIdsIter<'_, T> {
        NodesPosIdsIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node, its position and its identifier.
    pub fn nodes_pos_ids_mut(&mut self) -> NodesPosIdsIterMut<'_, T> {
        NodesPosIdsIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over shared references to each node data.
    pub fn nodes_info(&self) -> NodeInfoIter<'_, T> {
        NodeInfoIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node data.
    pub fn nodes_info_mut(&mut self) -> NodeInfoIterMut<'_, T> {
        NodeInfoIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over shared references to each node id and data.
    pub fn nodes_ids_data(&self) -> NodeIdsDataIter<'_, T> {
        NodeIdsDataIter {
            nodes: self.nodes.iter(),
        }
    }

    /// Iterates over mutable references to each node id and data.
    pub fn nodes_ids_data_mut(&mut self) -> NodeIdsDataIterMut<'_, T> {
        NodeIdsDataIterMut {
            nodes: self.nodes.iter_mut(),
        }
    }

    /// Iterates over wires.
    pub fn wires(&self) -> impl Iterator<Item = (OutPinId, InPinId)> + '_ {
        self.wires.iter().map(|wire| (wire.out_pin, wire.in_pin))
    }

    /// Returns input pin of the node.
    #[must_use]
    pub fn in_pin(&self, pin: InPinId) -> InPin {
        InPin::new(self, pin)
    }

    /// Returns output pin of the node.
    #[must_use]
    pub fn out_pin(&self, pin: OutPinId) -> OutPin {
        OutPin::new(self, pin)
    }

    // ── Stable identity (PLAN_NODE.md P3) ───────────────────────────

    /// This node's stable identity, for anything that outlives a frame.
    #[must_use]
    pub fn uid_of(&self, id: NodeId) -> Option<NodeUid> {
        self.nodes.get(id.0).map(|n| n.uid)
    }

    /// Resolve a stable identity back to this frame's slab key.
    ///
    /// `None` once the node is gone — which is the whole point: a
    /// `NodeId` held across a delete would resolve to whatever reused
    /// the slot instead.
    #[must_use]
    pub fn by_uid(&self, uid: NodeUid) -> Option<NodeId> {
        self.ext.by_uid.get(&uid).copied()
    }

    /// Counter bumped on every structural or positional mutation.
    ///
    /// Session-only — see [`GraphExt`].
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.ext.revision
    }

    /// Note that this graph changed in a way `revision` should reflect.
    ///
    /// Needed because node positions are mutated through
    /// `nodes_pos_mut` and friends, which hand out `&mut Pos2` and
    /// cannot observe the write.
    pub fn touch(&mut self) {
        self.ext.revision = self.ext.revision.wrapping_add(1);
    }

    /// Whether a node with this id exists.
    #[must_use]
    pub fn contains(&self, id: NodeId) -> bool {
        self.nodes.contains(id.0)
    }

    /// Number of nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the graph has no nodes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Explicit size override for a node, if the app set one.
    #[must_use]
    pub fn size_override_of(&self, id: NodeId) -> Option<Vec2> {
        self.nodes.get(id.0).and_then(|n| n.size_override)
    }

    /// Set or clear a node's explicit size, in points.
    pub fn set_size_override(&mut self, id: NodeId, size: Option<Vec2>) {
        if let Some(node) = self.nodes.get_mut(id.0) {
            node.size_override = size;
            self.ext.revision = self.ext.revision.wrapping_add(1);
        }
    }

    /// Every wire touching `node`, in either direction.
    pub fn wires_of(&self, node: NodeId) -> impl Iterator<Item = (OutPinId, InPinId)> + '_ {
        self.wires
            .iter()
            .filter(move |w| w.out_pin.node == node || w.in_pin.node == node)
            .map(|w| (w.out_pin, w.in_pin))
    }

    /// Drop wires addressing pins at or beyond the given counts, and
    /// return how many went.
    ///
    /// Nothing else in the crate reconciles a wire's positional pin
    /// index against the viewer's pin count: the renderer silently
    /// skips a wire whose endpoint has no drawn pin, so a wire to a
    /// removed pin lingers invisibly in the set forever and *reappears*
    /// if the count later grows back. Any operation that shrinks a
    /// node's pin count must call this.
    pub fn trim_wires_to(&mut self, node: NodeId, inputs: usize, outputs: usize) -> usize {
        let dropped = self.wires.drop_matching(|w| {
            (w.in_pin.node == node && w.in_pin.input >= inputs)
                || (w.out_pin.node == node && w.out_pin.output >= outputs)
        });
        if dropped > 0 {
            self.ext.revision = self.ext.revision.wrapping_add(1);
        }
        dropped
    }

    /// Restore every derived invariant. Idempotent.
    ///
    /// Assigns uids to nodes that have none (a document written before
    /// uids existed), rebuilds the uid reverse index and both wire
    /// adjacency indexes, and drops wires whose endpoints no longer
    /// exist. Call after deserialising, or after mutating the model
    /// through a path that bypasses the accessors.
    pub fn repair(&mut self) {
        let mut max_uid = 0_u64;
        let mut seen: HashSet<NodeUid> = HashSet::new();
        let mut needs_uid: Vec<usize> = Vec::new();

        for (idx, node) in self.nodes.iter() {
            if node.uid.is_assigned() && seen.insert(node.uid) {
                max_uid = max_uid.max(node.uid.0);
            } else {
                // Unassigned, or a duplicate — a hand-edited or merged
                // document can produce either, and both break `by_uid`.
                needs_uid.push(idx);
            }
        }

        self.ext.next_uid = self.ext.next_uid.max(max_uid + 1);
        for idx in needs_uid {
            let uid = self.alloc_uid();
            self.nodes[idx].uid = uid;
        }

        self.ext.by_uid = self
            .nodes
            .iter()
            .map(|(idx, node)| (node.uid, NodeId(idx)))
            .collect();

        let live: HashSet<NodeId> = self.nodes.iter().map(|(idx, _)| NodeId(idx)).collect();
        self.wires
            .drop_matching(|w| !live.contains(&w.out_pin.node) || !live.contains(&w.in_pin.node));
        self.wires.reindex();
        self.repair_frames();

        // Instance and port bindings are keyed by uid, so a document
        // that lost nodes without going through `remove_node` — a
        // hand-edited file, a merge — can carry entries for uids that
        // no longer exist.
        let uids: HashSet<NodeUid> = self.nodes.iter().map(|(_, n)| n.uid).collect();
        self.ext.instances.retain(|uid, _| uids.contains(uid));
        self.ext.port_nodes.retain(|uid, _| uids.contains(uid));
    }

    /// Whether the derived indexes look stale — an O(1) check the
    /// renderer can afford every frame before deciding to
    /// [`repair`](Self::repair).
    #[must_use]
    pub fn needs_repair(&self) -> bool {
        self.ext.by_uid.len() != self.nodes.len()
    }
}

impl<T> Index<NodeId> for Graph<T> {
    type Output = T;

    #[inline]
    #[track_caller]
    fn index(&self, idx: NodeId) -> &Self::Output {
        &self.nodes[idx.0].value
    }
}

impl<T> IndexMut<NodeId> for Graph<T> {
    #[inline]
    #[track_caller]
    fn index_mut(&mut self, idx: NodeId) -> &mut Self::Output {
        &mut self.nodes[idx.0].value
    }
}

/// Iterator over shared references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesIter<'a, T> {
    type Item = &'a T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<&'a T> {
        let (_, node) = self.nodes.next()?;
        Some(&node.value)
    }

    fn nth(&mut self, n: usize) -> Option<&'a T> {
        let (_, node) = self.nodes.nth(n)?;
        Some(&node.value)
    }
}

/// Iterator over mutable references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesIterMut<'a, T> {
    type Item = &'a mut T;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<&'a mut T> {
        let (_, node) = self.nodes.next()?;
        Some(&mut node.value)
    }

    fn nth(&mut self, n: usize) -> Option<&'a mut T> {
        let (_, node) = self.nodes.nth(n)?;
        Some(&mut node.value)
    }
}

/// Iterator over shared references to nodes and their positions.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesPosIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesPosIter<'a, T> {
    type Item = (Pos2, &'a T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(Pos2, &'a T)> {
        let (_, node) = self.nodes.next()?;
        Some((node.pos, &node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(Pos2, &'a T)> {
        let (_, node) = self.nodes.nth(n)?;
        Some((node.pos, &node.value))
    }
}

/// Iterator over mutable references to nodes and their positions.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesPosIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesPosIterMut<'a, T> {
    type Item = (Pos2, &'a mut T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(Pos2, &'a mut T)> {
        let (_, node) = self.nodes.next()?;
        Some((node.pos, &mut node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(Pos2, &'a mut T)> {
        let (_, node) = self.nodes.nth(n)?;
        Some((node.pos, &mut node.value))
    }
}

/// Iterator over shared references to nodes and their identifiers.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesIdsIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesIdsIter<'a, T> {
    type Item = (NodeId, &'a T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, &'a T)> {
        let (idx, node) = self.nodes.next()?;
        Some((NodeId(idx), &node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, &'a T)> {
        let (idx, node) = self.nodes.nth(n)?;
        Some((NodeId(idx), &node.value))
    }
}

/// Iterator over mutable references to nodes and their identifiers.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesIdsIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesIdsIterMut<'a, T> {
    type Item = (NodeId, &'a mut T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, &'a mut T)> {
        let (idx, node) = self.nodes.next()?;
        Some((NodeId(idx), &mut node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, &'a mut T)> {
        let (idx, node) = self.nodes.nth(n)?;
        Some((NodeId(idx), &mut node.value))
    }
}

/// Iterator over shared references to nodes, their positions and their identifiers.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesPosIdsIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesPosIdsIter<'a, T> {
    type Item = (NodeId, Pos2, &'a T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, Pos2, &'a T)> {
        let (idx, node) = self.nodes.next()?;
        Some((NodeId(idx), node.pos, &node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, Pos2, &'a T)> {
        let (idx, node) = self.nodes.nth(n)?;
        Some((NodeId(idx), node.pos, &node.value))
    }
}

/// Iterator over mutable references to nodes, their positions and their identifiers.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodesPosIdsIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodesPosIdsIterMut<'a, T> {
    type Item = (NodeId, Pos2, &'a mut T);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, Pos2, &'a mut T)> {
        let (idx, node) = self.nodes.next()?;
        Some((NodeId(idx), node.pos, &mut node.value))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, Pos2, &'a mut T)> {
        let (idx, node) = self.nodes.nth(n)?;
        Some((NodeId(idx), node.pos, &mut node.value))
    }
}

/// Iterator over shared references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodeInfoIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodeInfoIter<'a, T> {
    type Item = &'a Node<T>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<&'a Node<T>> {
        let (_, node) = self.nodes.next()?;
        Some(node)
    }

    fn nth(&mut self, n: usize) -> Option<&'a Node<T>> {
        let (_, node) = self.nodes.nth(n)?;
        Some(node)
    }
}

/// Iterator over mutable references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodeInfoIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodeInfoIterMut<'a, T> {
    type Item = &'a mut Node<T>;

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<&'a mut Node<T>> {
        let (_, node) = self.nodes.next()?;
        Some(node)
    }

    fn nth(&mut self, n: usize) -> Option<&'a mut Node<T>> {
        let (_, node) = self.nodes.nth(n)?;
        Some(node)
    }
}

/// Iterator over shared references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodeIdsDataIter<'a, T> {
    nodes: slab::Iter<'a, Node<T>>,
}

impl<'a, T> Iterator for NodeIdsDataIter<'a, T> {
    type Item = (NodeId, &'a Node<T>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, &'a Node<T>)> {
        let (id, node) = self.nodes.next()?;
        Some((NodeId(id), node))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, &'a Node<T>)> {
        let (id, node) = self.nodes.nth(n)?;
        Some((NodeId(id), node))
    }
}

/// Iterator over mutable references to nodes.
#[must_use = "iterator adaptors are lazy and do nothing unless consumed"]
pub struct NodeIdsDataIterMut<'a, T> {
    nodes: slab::IterMut<'a, Node<T>>,
}

impl<'a, T> Iterator for NodeIdsDataIterMut<'a, T> {
    type Item = (NodeId, &'a mut Node<T>);

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.nodes.size_hint()
    }

    fn next(&mut self) -> Option<(NodeId, &'a mut Node<T>)> {
        let (id, node) = self.nodes.next()?;
        Some((NodeId(id), node))
    }

    fn nth(&mut self, n: usize) -> Option<(NodeId, &'a mut Node<T>)> {
        let (id, node) = self.nodes.nth(n)?;
        Some((NodeId(id), node))
    }
}

/// Node and its output pin.
#[derive(Clone, Debug)]
pub struct OutPin {
    /// Output pin identifier.
    pub id: OutPinId,

    /// List of input pins connected to this output pin.
    pub remotes: Vec<InPinId>,
}

/// Node and its output pin.
#[derive(Clone, Debug)]
pub struct InPin {
    /// Input pin identifier.
    pub id: InPinId,

    /// List of output pins connected to this input pin.
    pub remotes: Vec<OutPinId>,
}

impl OutPin {
    fn new<T>(graph: &Graph<T>, pin: OutPinId) -> Self {
        OutPin {
            id: pin,
            remotes: graph.wires.wired_inputs(pin).collect(),
        }
    }
}

impl InPin {
    fn new<T>(graph: &Graph<T>, pin: InPinId) -> Self {
        InPin {
            id: pin,
            remotes: graph.wires.wired_outputs(pin).collect(),
        }
    }
}
