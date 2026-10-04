//! Shared subgraph definitions — PLAN_NODE.md P7.
//!
//! # The one decision this file encodes
//!
//! Subgraphs are **shared by reference**, not copied. Building one
//! full-adder and placing eight of them means eight instances of one
//! definition: fix a bug once and all eight change. That is what every
//! reuse-oriented editor converged on — Blender datablocks, Node-RED
//! subflows, Logisim subcircuits, Turing Complete components — and it
//! is the only model that expresses "build a chip, then use it".
//!
//! Copy-by-value is not a second mechanism. It is [`DefScope::Local`]:
//! a definition hidden from the library and deleted when its last
//! instance is expanded. [`GraphDoc::promote`] flips one bit to share it.
//!
//! The accepted cost, stated plainly: **interior layout is shared too**.
//! All eight adders show their guts in the same arrangement, because
//! positions live in the definition. That is correct and it surprises
//! people, which is why the instance count is meant to be visible on
//! the node and why [`GraphDoc::make_local_copy`] exists.
//!
//! # Ports: order is presentation, id is identity
//!
//! [`PortId`] is stable and never reused; the `Vec` order is what the
//! instance's pin indices follow. Blender had to rebuild its entire
//! socket API in 4.0 because sockets were positional and every script
//! broke when one was inserted. Keeping the two separate is what makes
//! reordering a port a rewrite of wires rather than a corruption of
//! them.
//!
//! This file is checked by `make check` to contain no backend types.

use std::collections::{HashMap, HashSet};

use mara_core::vocab::{Color32, Pos2, Vec2};
use slab::Slab;

use crate::vendored::nav::{Crumb, NodePath};
use crate::vendored::{Graph, InPinId, NodeUid, OutPinId};

/// Identifies a definition within one [`GraphDoc`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct DefId(pub usize);

/// Stable identity for one port of a definition.
///
/// Never reused within a definition, unlike the `Vec` index that
/// determines which pin it maps to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct PortId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum PortDir {
    In,
    Out,
}

/// Whether a definition is part of the reusable library.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum DefScope {
    /// Ad-hoc tidying — hidden from the library, and deleted when its
    /// last instance is expanded. What "just box these twelve messy
    /// nodes" produces.
    #[default]
    Local,
    /// A reusable component, listed in the library and surviving
    /// without instances.
    Shared,
}

/// One port of a definition's interface.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PortDef {
    pub id: PortId,
    pub name: String,
    pub color: Option<Color32>,
}

/// A definition's interface: two ordered lists plus a monotonic id
/// counter.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Ports {
    inputs: Vec<PortDef>,
    outputs: Vec<PortDef>,
    next: u32,
}

impl Ports {
    #[must_use]
    pub fn inputs(&self) -> &[PortDef] {
        &self.inputs
    }

    #[must_use]
    pub fn outputs(&self) -> &[PortDef] {
        &self.outputs
    }

    #[must_use]
    pub fn side(&self, dir: PortDir) -> &[PortDef] {
        match dir {
            PortDir::In => &self.inputs,
            PortDir::Out => &self.outputs,
        }
    }

    fn side_mut(&mut self, dir: PortDir) -> &mut Vec<PortDef> {
        match dir {
            PortDir::In => &mut self.inputs,
            PortDir::Out => &mut self.outputs,
        }
    }

    /// Append a port and return its stable id.
    pub fn push(&mut self, dir: PortDir, name: impl Into<String>) -> PortId {
        self.next += 1;
        let id = PortId(self.next);
        self.side_mut(dir).push(PortDef {
            id,
            name: name.into(),
            color: None,
        });
        id
    }

    /// Which pin index a port currently maps to.
    #[must_use]
    pub fn index_of(&self, dir: PortDir, port: PortId) -> Option<usize> {
        self.side(dir).iter().position(|p| p.id == port)
    }

    /// Which port a pin index currently maps to.
    #[must_use]
    pub fn at(&self, dir: PortDir, index: usize) -> Option<&PortDef> {
        self.side(dir).get(index)
    }

    #[must_use]
    pub fn count(&self, dir: PortDir) -> usize {
        self.side(dir).len()
    }
}

/// A reusable subgraph: an interface plus a body.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GraphDef<T> {
    pub name: String,
    pub color: Option<Color32>,
    pub scope: DefScope,
    pub ports: Ports,
    pub body: Graph<T>,
}

/// What went wrong with a structural edit.
///
/// Every one of these is a case that must not panic: a user dragging an
/// ALU into itself is a mistake to report, not a crash.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum GraphError {
    /// The edit would make a definition contain itself, transitively.
    WouldRecurse,
    /// Nesting deeper than the document's cap.
    DepthLimit(u32),
    /// The app's [`NodeFactory`] declined to mint a payload.
    ViewerDeclined,
    NoSuchLevel,
    NoSuchNode,
    NoSuchDef,
    EmptySelection,
    /// The selection spans more than one level.
    MixedLevels,
}

/// Describes a boundary node the crate needs the app to create.
pub struct PortSpec<'a> {
    pub id: PortId,
    pub dir: PortDir,
    pub name: &'a str,
    pub index: usize,
}

/// Mints the payloads the crate cannot construct itself.
///
/// `Graph<T>` carries no bound on `T`, so the crate has no way to make
/// one — but collapsing a selection has to create an instance node, and
/// a definition body needs one boundary node per port. The app supplies
/// them here.
///
/// Object-safe on purpose: it lets the collapse and instantiate
/// algorithms be tested with `T = ()` and a two-line factory, which is
/// where the wire-rewiring logic actually gets exercised.
/// [`crate::vendored::ui::NodeViewer`] cannot serve as this bound — it
/// is dyn-incompatible via RPITIT.
///
/// Returning `None` aborts the operation with
/// [`GraphError::ViewerDeclined`] and leaves the document untouched.
pub trait NodeFactory<T> {
    fn instance_node(&mut self, def: DefId, name: &str, ports: &Ports) -> Option<T>;
    fn port_node(&mut self, spec: &PortSpec<'_>) -> Option<T>;
}

/// A definition's interface, snapshotted.
///
/// The renderer holds `&mut` one level while needing every definition's
/// pin counts to draw instance nodes — and the level it holds may
/// itself be a definition body. Cloning this much once per frame dodges
/// the borrow entirely. Keep it to these fields: widening it to live
/// interior data is what would make the snapshot expensive.
#[derive(Clone, Debug, PartialEq)]
pub struct Iface {
    pub name: String,
    pub color: Option<Color32>,
    pub inputs: u16,
    pub outputs: u16,
    pub revision: u32,
}

pub type IfaceTable = HashMap<DefId, Iface>;

/// A document: one root graph plus a library of definitions.
///
/// Deliberately a separate type from [`Graph`] rather than making
/// `Graph` recursive. A self-referential `Graph` could not hand out
/// `&mut` one level while reading another's interface, and it would
/// multiply the serde surface for every consumer that never nests
/// anything.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GraphDoc<T> {
    #[cfg_attr(feature = "serde", serde(default = "doc_version"))]
    version: u32,
    pub root: Graph<T>,
    defs: Slab<GraphDef<T>>,
    /// Deepest nesting allowed. A typed error at the cap beats a stack
    /// overflow at render time.
    #[cfg_attr(feature = "serde", serde(default = "default_depth_cap"))]
    depth_cap: u32,
}

#[cfg(feature = "serde")]
fn doc_version() -> u32 {
    1
}

fn default_depth_cap() -> u32 {
    16
}

impl<T> Default for GraphDoc<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> GraphDoc<T> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: 1,
            root: Graph::new(),
            defs: Slab::new(),
            depth_cap: default_depth_cap(),
        }
    }

    /// Wrap an existing graph as a document root.
    #[must_use]
    pub fn from_graph(root: Graph<T>) -> Self {
        Self {
            root,
            ..Self::new()
        }
    }

    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub const fn depth_cap(&self) -> u32 {
        self.depth_cap
    }

    pub fn set_depth_cap(&mut self, cap: u32) {
        self.depth_cap = cap.max(1);
    }

    // ── The definition library ──────────────────────────────────────

    /// Declare an empty definition.
    pub fn insert_def(&mut self, name: impl Into<String>, scope: DefScope) -> DefId {
        DefId(self.defs.insert(GraphDef {
            name: name.into(),
            color: None,
            scope,
            ports: Ports::default(),
            body: Graph::new(),
        }))
    }

    #[must_use]
    pub fn def(&self, d: DefId) -> Option<&GraphDef<T>> {
        self.defs.get(d.0)
    }

    pub fn def_mut(&mut self, d: DefId) -> Option<&mut GraphDef<T>> {
        self.defs.get_mut(d.0)
    }

    pub fn defs(&self) -> impl Iterator<Item = (DefId, &GraphDef<T>)> + '_ {
        self.defs.iter().map(|(i, d)| (DefId(i), d))
    }

    /// Promote a `Local` definition into the shared library.
    pub fn promote(&mut self, d: DefId) {
        if let Some(def) = self.defs.get_mut(d.0) {
            def.scope = DefScope::Shared;
        }
    }

    // ── Addressing levels ───────────────────────────────────────────

    /// Which definition a path lands in. `None` means the root.
    ///
    /// Returns `Err` when the path does not resolve — an instance was
    /// deleted, or the uid never existed.
    pub fn resolve(&self, path: &NodePath) -> Result<Option<DefId>, GraphError> {
        let mut current: Option<DefId> = None;
        for uid in path.iter() {
            let level = self.level_of(current).ok_or(GraphError::NoSuchLevel)?;
            let def = level.instance_def(uid).ok_or(GraphError::NoSuchNode)?;
            if self.defs.get(def.0).is_none() {
                return Err(GraphError::NoSuchDef);
            }
            current = Some(def);
        }
        Ok(current)
    }

    #[must_use]
    pub fn level(&self, path: &NodePath) -> Option<&Graph<T>> {
        self.resolve(path).ok().and_then(|d| self.level_of(d))
    }

    pub fn level_mut(&mut self, path: &NodePath) -> Option<&mut Graph<T>> {
        let d = self.resolve(path).ok()?;
        match d {
            None => Some(&mut self.root),
            Some(d) => self.defs.get_mut(d.0).map(|x| &mut x.body),
        }
    }

    fn level_of(&self, d: Option<DefId>) -> Option<&Graph<T>> {
        match d {
            None => Some(&self.root),
            Some(d) => self.defs.get(d.0).map(|x| &x.body),
        }
    }

    /// The longest prefix of `path` that still resolves.
    ///
    /// A saved path can outlive the instance it points through — the
    /// user deleted it in another session. Truncating beats refusing to
    /// open the document.
    #[must_use]
    pub fn prune_path(&self, path: &NodePath) -> NodePath {
        let mut ok = NodePath::root();
        for uid in path.iter() {
            let candidate = ok.child(uid);
            if self.resolve(&candidate).is_ok() {
                ok = candidate;
            } else {
                break;
            }
        }
        ok
    }

    /// One entry per level from the root down to `path`.
    #[must_use]
    pub fn breadcrumb(&self, path: &NodePath) -> Vec<Crumb> {
        let mut out = vec![Crumb {
            name: "Root".to_string(),
            color: None,
            def: None,
            path: NodePath::root(),
        }];
        let mut here = NodePath::root();
        for uid in path.iter() {
            here = here.child(uid);
            let Ok(Some(d)) = self.resolve(&here) else {
                break;
            };
            let Some(def) = self.def(d) else { break };
            out.push(Crumb {
                name: def.name.clone(),
                color: def.color,
                def: Some(d),
                path: here.clone(),
            });
        }
        out
    }

    /// Every definition's interface, for one frame's rendering.
    #[must_use]
    pub fn iface_snapshot(&self) -> IfaceTable {
        self.defs
            .iter()
            .map(|(i, d)| {
                (
                    DefId(i),
                    Iface {
                        name: d.name.clone(),
                        color: d.color,
                        inputs: d.ports.count(PortDir::In) as u16,
                        outputs: d.ports.count(PortDir::Out) as u16,
                        revision: d.body.revision(),
                    },
                )
            })
            .collect()
    }

    // ── Containment guards ──────────────────────────────────────────

    /// How many instances of `d` exist anywhere in the document.
    ///
    /// Walked rather than cached: a cache would have to be invalidated
    /// on six mutation paths, and keeping one behind a `RefCell` would
    /// make the whole document `!Sync` — which matters, because Mara
    /// state types are held in Bevy resources.
    #[must_use]
    pub fn instance_count(&self, d: DefId) -> u32 {
        let mut n = self.root.instances_of(d);
        for (_, def) in self.defs.iter() {
            n += def.body.instances_of(d);
        }
        n
    }

    /// Whether placing an instance of `d` inside `host` is legal.
    ///
    /// `host = None` is the root, which can hold anything. Otherwise
    /// this refuses if `d` already contains `host` at any depth — the
    /// case that would otherwise recurse forever at render time.
    #[must_use]
    pub fn can_contain(&self, host: Option<DefId>, d: DefId) -> bool {
        let Some(host) = host else {
            return self.defs.contains(d.0);
        };
        if !self.defs.contains(d.0) || !self.defs.contains(host.0) {
            return false;
        }
        if host == d {
            return false;
        }
        !self.def_reaches(d, host)
    }

    /// Whether `from` contains `target` at any depth.
    fn def_reaches(&self, from: DefId, target: DefId) -> bool {
        let mut seen: HashSet<DefId> = HashSet::new();
        let mut stack = vec![from];
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur) {
                continue;
            }
            let Some(def) = self.defs.get(cur.0) else {
                continue;
            };
            for (_, child) in def.body.ext.instances.iter() {
                if *child == target {
                    return true;
                }
                stack.push(*child);
            }
        }
        false
    }

    /// The deepest nesting under `d`.
    #[must_use]
    pub fn depth_of(&self, d: DefId) -> u32 {
        fn walk<T>(doc: &GraphDoc<T>, d: DefId, seen: &mut HashSet<DefId>) -> u32 {
            if !seen.insert(d) {
                // A cycle should be impossible via the guards, but a
                // corrupt document must still terminate.
                return 0;
            }
            let Some(def) = doc.defs.get(d.0) else {
                seen.remove(&d);
                return 0;
            };
            let mut deepest = 0;
            for (_, child) in def.body.ext.instances.iter() {
                deepest = deepest.max(1 + walk(doc, *child, seen));
            }
            seen.remove(&d);
            deepest
        }
        walk(self, d, &mut HashSet::new())
    }

    // ── Placing instances ───────────────────────────────────────────

    /// Place an instance of `def` at `pos` in the level `path` names.
    ///
    /// The app mints the node's payload; a `None` from the factory
    /// leaves the document byte-identical.
    pub fn instantiate(
        &mut self,
        path: &NodePath,
        def: DefId,
        pos: Pos2,
        factory: &mut dyn NodeFactory<T>,
    ) -> Result<NodeUid, GraphError> {
        let host = self.resolve(path)?;
        if !self.can_contain(host, def) {
            return Err(GraphError::WouldRecurse);
        }
        let depth = path.depth() as u32 + 1 + self.depth_of(def);
        if depth > self.depth_cap {
            return Err(GraphError::DepthLimit(self.depth_cap));
        }

        let (name, ports) = {
            let d = self.def(def).ok_or(GraphError::NoSuchDef)?;
            (d.name.clone(), d.ports.clone())
        };
        let payload = factory
            .instance_node(def, &name, &ports)
            .ok_or(GraphError::ViewerDeclined)?;

        let level = self.level_mut(path).ok_or(GraphError::NoSuchLevel)?;
        let node = level.insert_node(pos, payload);
        let uid = level.uid_of(node).expect("just inserted");
        level.ext.instances.insert(uid, def);
        Ok(uid)
    }

    // ── Editing a definition's interface ────────────────────────────

    pub fn rename_port(&mut self, def: DefId, port: PortId, name: impl Into<String>) {
        let Some(d) = self.defs.get_mut(def.0) else {
            return;
        };
        for side in [PortDir::In, PortDir::Out] {
            if let Some(p) = d.ports.side_mut(side).iter_mut().find(|p| p.id == port) {
                p.name = name.into();
                return;
            }
        }
    }

    /// Move a port within its side, rewriting every instance's wires so
    /// they follow the port rather than the index.
    ///
    /// This is where an off-by-one hides. `PortId` is stable and the
    /// `Vec` order is not, so the remap is computed from the *ids*
    /// before and after, never from the positions alone.
    pub fn reorder_port(&mut self, def: DefId, dir: PortDir, from: usize, to: usize) {
        let Some(d) = self.defs.get_mut(def.0) else {
            return;
        };
        let side = d.ports.side_mut(dir);
        if from >= side.len() || to >= side.len() || from == to {
            return;
        }
        let before: Vec<PortId> = side.iter().map(|p| p.id).collect();
        let moved = side.remove(from);
        side.insert(to, moved);
        let after: Vec<PortId> = side.iter().map(|p| p.id).collect();

        // old pin index -> new pin index
        let remap: HashMap<usize, usize> = before
            .iter()
            .enumerate()
            .filter_map(|(old, id)| after.iter().position(|x| x == id).map(|new| (old, new)))
            .collect();
        self.remap_instance_pins(def, dir, &remap);
    }

    /// Remove a port, dropping every wire attached to it across the
    /// whole document and shifting the rest down.
    pub fn remove_port(&mut self, def: DefId, port: PortId) {
        let Some(d) = self.defs.get_mut(def.0) else {
            return;
        };
        for dir in [PortDir::In, PortDir::Out] {
            let side = d.ports.side_mut(dir);
            let Some(idx) = side.iter().position(|p| p.id == port) else {
                continue;
            };
            let before: Vec<PortId> = side.iter().map(|p| p.id).collect();
            side.remove(idx);
            let after: Vec<PortId> = side.iter().map(|p| p.id).collect();
            let remap: HashMap<usize, usize> = before
                .iter()
                .enumerate()
                .filter_map(|(old, id)| after.iter().position(|x| x == id).map(|new| (old, new)))
                .collect();

            // Wires on the removed pin have no entry in `remap` and are
            // dropped by `remap_instance_pins`. Leaving them would let
            // them resurrect if the count later grew back — the exact
            // trap `Graph::trim_wires_to` exists for.
            self.remap_instance_pins(def, dir, &remap);

            // The boundary node inside the body goes too.
            if let Some(d) = self.defs.get_mut(def.0) {
                let doomed: Vec<NodeUid> = d
                    .body
                    .ext
                    .port_nodes
                    .iter()
                    .filter(|(_, p)| **p == port)
                    .map(|(u, _)| *u)
                    .collect();
                for uid in doomed {
                    d.body.ext.port_nodes.remove(&uid);
                    if let Some(id) = d.body.by_uid(uid) {
                        d.body.remove_node(id);
                    }
                }
            }
            return;
        }
    }

    /// Apply a pin-index remap to every instance of `def`, everywhere.
    fn remap_instance_pins(&mut self, def: DefId, dir: PortDir, remap: &HashMap<usize, usize>) {
        Self::remap_in_level(&mut self.root, def, dir, remap);
        for (_, d) in self.defs.iter_mut() {
            Self::remap_in_level(&mut d.body, def, dir, remap);
        }
    }

    fn remap_in_level(
        level: &mut Graph<T>,
        def: DefId,
        dir: PortDir,
        remap: &HashMap<usize, usize>,
    ) {
        let instances: Vec<NodeUid> = level
            .ext
            .instances
            .iter()
            .filter(|(_, d)| **d == def)
            .map(|(u, _)| *u)
            .collect();

        for uid in instances {
            let Some(node) = level.by_uid(uid) else {
                continue;
            };
            let wires: Vec<(OutPinId, InPinId)> = level.wires_of(node).collect();
            for (o, i) in wires {
                let (matches, old) = match dir {
                    PortDir::In if i.node == node => (true, i.input),
                    PortDir::Out if o.node == node => (true, o.output),
                    _ => (false, 0),
                };
                if !matches {
                    continue;
                }
                level.disconnect(o, i);
                if let Some(&new) = remap.get(&old) {
                    let (o2, i2) = match dir {
                        PortDir::In => (o, InPinId { node, input: new }),
                        PortDir::Out => (OutPinId { node, output: new }, i),
                    };
                    level.connect(o2, i2);
                }
                // No entry in `remap` means the port is gone; the wire
                // stays disconnected rather than being re-attached to
                // whatever now occupies that index.
            }
        }
    }

    /// Restore derived invariants across every level.
    pub fn repair(&mut self) {
        self.root.repair();
        let live: HashSet<usize> = self.defs.iter().map(|(i, _)| i).collect();
        for (_, d) in self.defs.iter_mut() {
            d.body.repair();
            d.body.ext.instances.retain(|_, def| live.contains(&def.0));
        }
        self.root
            .ext
            .instances
            .retain(|_, def| live.contains(&def.0));
    }
}

impl<T> Graph<T> {
    /// Which definition this node instantiates, if any.
    #[must_use]
    pub fn instance_def(&self, uid: NodeUid) -> Option<DefId> {
        self.ext.instances.get(&uid).copied()
    }

    /// Which port this node *is*, when it is a definition boundary.
    #[must_use]
    pub fn port_node(&self, uid: NodeUid) -> Option<PortId> {
        self.ext.port_nodes.get(&uid).copied()
    }

    /// Mark a node as an instance of `def`.
    pub fn set_instance(&mut self, uid: NodeUid, def: Option<DefId>) {
        match def {
            Some(d) => self.ext.instances.insert(uid, d),
            None => self.ext.instances.remove(&uid),
        };
        self.touch();
    }

    /// Mark a node as the boundary node for `port`.
    pub fn set_port_node(&mut self, uid: NodeUid, port: Option<PortId>) {
        match port {
            Some(p) => self.ext.port_nodes.insert(uid, p),
            None => self.ext.port_nodes.remove(&uid),
        };
        self.touch();
    }

    /// Instances of `def` at this level.
    #[must_use]
    pub fn instances_of(&self, def: DefId) -> u32 {
        self.ext
            .instances
            .iter()
            .filter(|(uid, d)| **d == def && self.by_uid(**uid).is_some())
            .count() as u32
    }

    /// What ports collapsing `members` would produce. **No mutation.**
    ///
    /// Separated from the collapse itself because this is the part with
    /// the interesting behaviour and it is worth testing on its own:
    /// two external sources feeding one interior pin share a single
    /// input port, while one interior output feeding three external
    /// pins produces a single output port with three wires hanging off
    /// the instance.
    ///
    /// Order is member `pos.y`, then `pos.x`, then pin index — visual
    /// top-to-bottom, and deterministic, so it can be asserted.
    #[must_use]
    pub fn derive_ports(&self, members: &[NodeUid]) -> (Vec<InPinId>, Vec<OutPinId>) {
        let live: Vec<(NodeUid, crate::vendored::NodeId)> = members
            .iter()
            .filter_map(|u| self.by_uid(*u).map(|id| (*u, id)))
            .collect();
        let inside: HashSet<crate::vendored::NodeId> = live.iter().map(|(_, id)| *id).collect();

        let mut ins: Vec<InPinId> = Vec::new();
        let mut outs: Vec<OutPinId> = Vec::new();

        for (o, i) in self.wires() {
            let o_in = inside.contains(&o.node);
            let i_in = inside.contains(&i.node);
            if i_in && !o_in && !ins.contains(&i) {
                ins.push(i);
            }
            if o_in && !i_in && !outs.contains(&o) {
                outs.push(o);
            }
        }

        let key = |node: crate::vendored::NodeId, pin: usize| {
            let p = self
                .get_node_info(node)
                .map_or(Pos2::new(0.0, 0.0), |n| n.pos);
            (ordered(p.y), ordered(p.x), pin)
        };
        ins.sort_by_key(|p| key(p.node, p.input));
        outs.sort_by_key(|p| key(p.node, p.output));
        (ins, outs)
    }
}

/// Float ordering for a sort key. Positions are finite in practice; NaN
/// sorts last rather than panicking the comparator.
fn ordered(v: f32) -> i64 {
    if v.is_nan() {
        i64::MAX
    } else {
        (v * 1000.0) as i64
    }
}

/// Offset applied to boundary nodes so they do not stack on the origin.
pub(crate) const PORT_NODE_SPACING: Vec2 = Vec2::new(0.0, 70.0);

/// Maps a definition body's uids to the fresh ones an expand created.
///
/// Handed to the app after an expand so it can migrate per-instance
/// state. Expanding *destroys* the interior's identity — the nodes move
/// into the host level and get new uids there — so every `NodePath`
/// under the old instance becomes invalid. The crate cannot migrate app
/// state for the app; it can hand over the exact mapping, which is what
/// this is.
pub type UidRemap = HashMap<NodeUid, NodeUid>;

impl<T> GraphDoc<T> {
    /// Fold `members` into a new definition, replacing them with one
    /// instance node.
    ///
    /// The order of operations matters and is not obvious: the boundary
    /// wires must be **snapshotted before any mutation**, because
    /// `Graph::remove_node` drops every wire incident to the node it
    /// removes — and those are precisely the wires that have to be
    /// reconnected to the new instance afterwards.
    ///
    /// A factory that declines leaves the document byte-identical.
    pub fn collapse(
        &mut self,
        path: &NodePath,
        members: &[NodeUid],
        scope: DefScope,
        name: impl Into<String>,
        factory: &mut dyn NodeFactory<T>,
    ) -> Result<(DefId, NodeUid), GraphError>
    where
        T: Clone,
    {
        if members.is_empty() {
            return Err(GraphError::EmptySelection);
        }
        let host = self.resolve(path)?;

        // ── 1. Snapshot everything, mutate nothing ──
        let (ins, outs, member_ids, positions, payloads, interior_wires, crossing) = {
            let level = self.level(path).ok_or(GraphError::NoSuchLevel)?;

            let member_ids: Vec<crate::vendored::NodeId> =
                members.iter().filter_map(|u| level.by_uid(*u)).collect();
            if member_ids.len() != members.len() {
                return Err(GraphError::NoSuchNode);
            }
            let inside: HashSet<crate::vendored::NodeId> = member_ids.iter().copied().collect();

            let (ins, outs) = level.derive_ports(members);

            let positions: Vec<Pos2> = member_ids
                .iter()
                .map(|id| {
                    level
                        .get_node_info(*id)
                        .map_or(Pos2::new(0.0, 0.0), |n| n.pos)
                })
                .collect();
            let payloads: Vec<T> = member_ids.iter().map(|id| level[*id].clone()).collect();

            let mut interior_wires = Vec::new();
            let mut crossing = Vec::new();
            for (o, i) in level.wires() {
                match (inside.contains(&o.node), inside.contains(&i.node)) {
                    (true, true) => interior_wires.push((o, i)),
                    (true, false) | (false, true) => crossing.push((o, i)),
                    (false, false) => {}
                }
            }
            (
                ins,
                outs,
                member_ids,
                positions,
                payloads,
                interior_wires,
                crossing,
            )
        };

        // ── 2. Build the definition ──
        let def = self.insert_def(name, scope);
        {
            let d = self.defs.get_mut(def.0).expect("just inserted");
            for (idx, pin) in ins.iter().enumerate() {
                d.ports.push(PortDir::In, format!("in{idx}"));
                let _ = pin;
            }
            for (idx, pin) in outs.iter().enumerate() {
                d.ports.push(PortDir::Out, format!("out{idx}"));
                let _ = pin;
            }
        }

        // Mint the instance payload before touching the host level, so
        // a refusal costs nothing but the definition — removed below.
        let (def_name, def_ports) = {
            let d = self.def(def).expect("just inserted");
            (d.name.clone(), d.ports.clone())
        };
        let Some(instance_payload) = factory.instance_node(def, &def_name, &def_ports) else {
            self.defs.remove(def.0);
            return Err(GraphError::ViewerDeclined);
        };

        // Boundary node payloads, same discipline.
        let mut boundary_payloads = Vec::new();
        for (idx, port) in def_ports.inputs().iter().enumerate() {
            let spec = PortSpec {
                id: port.id,
                dir: PortDir::In,
                name: &port.name,
                index: idx,
            };
            let Some(p) = factory.port_node(&spec) else {
                self.defs.remove(def.0);
                return Err(GraphError::ViewerDeclined);
            };
            boundary_payloads.push((PortDir::In, port.id, idx, p));
        }
        for (idx, port) in def_ports.outputs().iter().enumerate() {
            let spec = PortSpec {
                id: port.id,
                dir: PortDir::Out,
                name: &port.name,
                index: idx,
            };
            let Some(p) = factory.port_node(&spec) else {
                self.defs.remove(def.0);
                return Err(GraphError::ViewerDeclined);
            };
            boundary_payloads.push((PortDir::Out, port.id, idx, p));
        }

        // ── 3. Populate the body ──
        //
        // Positions normalised to the selection's bounding box origin,
        // so a chip built anywhere on the canvas opens with its guts at
        // a sensible place rather than far off-screen.
        let origin = positions
            .iter()
            .fold(Pos2::new(f32::INFINITY, f32::INFINITY), |acc, p| {
                Pos2::new(acc.x.min(p.x), acc.y.min(p.y))
            });
        let mut old_to_new: HashMap<crate::vendored::NodeId, crate::vendored::NodeId> =
            HashMap::new();
        {
            let body = &mut self.defs.get_mut(def.0).expect("live").body;
            for (i, payload) in payloads.into_iter().enumerate() {
                let p = positions[i];
                let placed =
                    body.insert_node(Pos2::new(p.x - origin.x + 120.0, p.y - origin.y), payload);
                old_to_new.insert(member_ids[i], placed);
            }
            for (o, i) in &interior_wires {
                if let (Some(no), Some(ni)) = (old_to_new.get(&o.node), old_to_new.get(&i.node)) {
                    body.connect(
                        OutPinId {
                            node: *no,
                            output: o.output,
                        },
                        InPinId {
                            node: *ni,
                            input: i.input,
                        },
                    );
                }
            }
        }

        // ── 4. Boundary nodes, one per port ──
        //
        // One node per port rather than one aggregate node per side:
        // each port then has exactly one pin, so all the existing pin
        // layout, hit-testing and wire rendering apply unchanged, and
        // ports become individually positionable.
        let mut port_node_of: HashMap<PortId, crate::vendored::NodeId> = HashMap::new();
        {
            let body = &mut self.defs.get_mut(def.0).expect("live").body;
            for (dir, port, idx, payload) in boundary_payloads {
                let x = if dir == PortDir::In { 0.0 } else { 600.0 };
                let pos = Pos2::new(x, idx as f32 * PORT_NODE_SPACING.y);
                let node = body.insert_node(pos, payload);
                let uid = body.uid_of(node).expect("just inserted");
                body.set_port_node(uid, Some(port));
                port_node_of.insert(port, node);
            }

            // Wire each boundary node to the interior pin it stands for.
            for (idx, pin) in ins.iter().enumerate() {
                let Some(port) = def_ports.at(PortDir::In, idx) else {
                    continue;
                };
                let (Some(bn), Some(target)) =
                    (port_node_of.get(&port.id), old_to_new.get(&pin.node))
                else {
                    continue;
                };
                body.connect(
                    OutPinId {
                        node: *bn,
                        output: 0,
                    },
                    InPinId {
                        node: *target,
                        input: pin.input,
                    },
                );
            }
            for (idx, pin) in outs.iter().enumerate() {
                let Some(port) = def_ports.at(PortDir::Out, idx) else {
                    continue;
                };
                let (Some(bn), Some(source)) =
                    (port_node_of.get(&port.id), old_to_new.get(&pin.node))
                else {
                    continue;
                };
                body.connect(
                    OutPinId {
                        node: *source,
                        output: pin.output,
                    },
                    InPinId {
                        node: *bn,
                        input: 0,
                    },
                );
            }
        }

        // ── 5. Swap the members for the instance ──
        let centre = {
            let sum = positions
                .iter()
                .fold((0.0, 0.0), |a, p| (a.0 + p.x, a.1 + p.y));
            Pos2::new(
                sum.0 / positions.len() as f32,
                sum.1 / positions.len() as f32,
            )
        };

        // The frame the members shared, if they all shared one — the
        // instance inherits it, so collapsing inside a group leaves the
        // result in that group.
        let common_frame = {
            let level = self.level(path).ok_or(GraphError::NoSuchLevel)?;
            let first = level.frame_of(member_ids[0]);
            member_ids
                .iter()
                .all(|id| level.frame_of(*id) == first)
                .then_some(first)
                .flatten()
        };

        let instance_uid = {
            let level = self.level_mut(path).ok_or(GraphError::NoSuchLevel)?;
            for id in &member_ids {
                level.remove_node(*id);
            }
            let node = level.insert_node(centre, instance_payload);
            let uid = level.uid_of(node).expect("just inserted");
            level.set_instance(uid, Some(def));
            if let Some(f) = common_frame {
                level.set_node_frame(node, Some(f));
            }

            // Reconnect the crossing wires to the instance's pins.
            for (o, i) in &crossing {
                let into_selection = member_ids.contains(&i.node);
                if into_selection {
                    if let Some(idx) = ins.iter().position(|p| *p == *i) {
                        level.connect(*o, InPinId { node, input: idx });
                    }
                } else if let Some(idx) = outs.iter().position(|p| *p == *o) {
                    level.connect(OutPinId { node, output: idx }, *i);
                }
            }
            uid
        };

        if !self.can_contain(host, def) {
            // Should be impossible — a brand-new definition cannot
            // contain its host — but a typed error beats a silent
            // cycle if the invariant ever changes.
            return Err(GraphError::WouldRecurse);
        }

        Ok((def, instance_uid))
    }

    /// The inverse of [`collapse`](Self::collapse): dissolve an
    /// instance back into its host level.
    ///
    /// Ships alongside collapse deliberately. ComfyUI shipped collapse
    /// without expand and it became their most-complained-about gap;
    /// retrofitting it is hard because it needs interior re-parenting,
    /// boundary rewiring and uid collision resolution, all of which are
    /// cheap to design now and expensive to bolt on.
    ///
    /// Interior nodes get **fresh** uids: uid uniqueness is per level,
    /// and the definition's uids may already be in use in the host.
    /// The returned [`UidRemap`] is how an app migrates per-instance
    /// state that would otherwise be orphaned.
    pub fn expand(&mut self, path: &NodePath, instance: NodeUid) -> Result<UidRemap, GraphError>
    where
        T: Clone,
    {
        let def = {
            let level = self.level(path).ok_or(GraphError::NoSuchLevel)?;
            level.instance_def(instance).ok_or(GraphError::NoSuchNode)?
        };

        // Snapshot the body and the instance's outside wiring first.
        let (body_nodes, body_wires, port_bindings, anchor, outside, frame) = {
            let d = self.def(def).ok_or(GraphError::NoSuchDef)?;
            let body = &d.body;

            let body_nodes: Vec<(NodeUid, Pos2, T, bool)> = body
                .node_ids()
                .map(|(id, _)| {
                    let uid = body.uid_of(id).expect("live");
                    let info = body.get_node_info(id).expect("live");
                    let is_port = body.port_node(uid).is_some();
                    (uid, info.pos, body[id].clone(), is_port)
                })
                .collect();

            let body_wires: Vec<(NodeUid, usize, NodeUid, usize)> = body
                .wires()
                .filter_map(|(o, i)| {
                    Some((
                        body.uid_of(o.node)?,
                        o.output,
                        body.uid_of(i.node)?,
                        i.input,
                    ))
                })
                .collect();

            // port id -> (boundary uid, pin index on the instance)
            let mut port_bindings: HashMap<NodeUid, (PortDir, usize)> = HashMap::new();
            for (uid, _, _, _) in &body_nodes {
                if let Some(port) = body.port_node(*uid) {
                    for dir in [PortDir::In, PortDir::Out] {
                        if let Some(idx) = d.ports.index_of(dir, port) {
                            port_bindings.insert(*uid, (dir, idx));
                        }
                    }
                }
            }

            let level = self.level(path).ok_or(GraphError::NoSuchLevel)?;
            let node = level.by_uid(instance).ok_or(GraphError::NoSuchNode)?;
            let anchor = level
                .get_node_info(node)
                .map_or(Pos2::new(0.0, 0.0), |n| n.pos);
            let outside: Vec<(OutPinId, InPinId)> = level.wires_of(node).collect();
            let frame = level.frame_of(node);
            (
                body_nodes,
                body_wires,
                port_bindings,
                anchor,
                outside,
                frame,
            )
        };

        // Place the interior into the host level.
        let mut remap: UidRemap = HashMap::new();
        let mut placed: HashMap<NodeUid, crate::vendored::NodeId> = HashMap::new();
        {
            let level = self.level_mut(path).ok_or(GraphError::NoSuchLevel)?;
            for (uid, pos, payload, is_port) in body_nodes {
                if is_port {
                    // Boundary nodes are interface, not content — they
                    // do not survive into the host level.
                    continue;
                }
                let id = level.insert_node(Pos2::new(anchor.x + pos.x, anchor.y + pos.y), payload);
                let new_uid = level.uid_of(id).expect("just inserted");
                remap.insert(uid, new_uid);
                placed.insert(uid, id);
                if let Some(f) = frame {
                    level.set_node_frame(id, Some(f));
                }
            }

            // Interior wires, minus the ones touching boundary nodes.
            for (ou, oi, iu, ii) in &body_wires {
                if let (Some(o), Some(i)) = (placed.get(ou), placed.get(iu)) {
                    level.connect(
                        OutPinId {
                            node: *o,
                            output: *oi,
                        },
                        InPinId {
                            node: *i,
                            input: *ii,
                        },
                    );
                }
            }

            // Reconnect the outside world to whatever the boundary
            // nodes were standing in for.
            let instance_node = level.by_uid(instance).ok_or(GraphError::NoSuchNode)?;
            for (o, i) in &outside {
                if i.node == instance_node {
                    // An external source fed port index `i.input`.
                    if let Some((bu, _)) = port_bindings
                        .iter()
                        .find(|(_, (dir, idx))| *dir == PortDir::In && *idx == i.input)
                    {
                        // The boundary node's own outgoing wires say
                        // which interior pins it fed.
                        for (wo, woi, wi, wii) in &body_wires {
                            if wo == bu
                                && let Some(target) = placed.get(wi)
                            {
                                let _ = woi;
                                level.connect(
                                    *o,
                                    InPinId {
                                        node: *target,
                                        input: *wii,
                                    },
                                );
                            }
                        }
                    }
                } else if o.node == instance_node
                    && let Some((bu, _)) = port_bindings
                        .iter()
                        .find(|(_, (dir, idx))| *dir == PortDir::Out && *idx == o.output)
                {
                    for (wo, woi, wi, _) in &body_wires {
                        if wi == bu
                            && let Some(source) = placed.get(wo)
                        {
                            level.connect(
                                OutPinId {
                                    node: *source,
                                    output: *woi,
                                },
                                *i,
                            );
                        }
                    }
                }
            }

            level.remove_node(instance_node);
        }

        // A `Local` definition exists only for its one instance.
        if self.def(def).map(|d| d.scope) == Some(DefScope::Local) && self.instance_count(def) == 0
        {
            self.defs.remove(def.0);
        }

        Ok(remap)
    }

    /// Give an instance its own private copy of its definition —
    /// Blender's Make Single User.
    ///
    /// The escape hatch for the shared model: when eight adders share a
    /// definition and one of them needs to differ, this is how.
    pub fn make_local_copy(
        &mut self,
        path: &NodePath,
        instance: NodeUid,
    ) -> Result<DefId, GraphError>
    where
        T: Clone,
    {
        let def = {
            let level = self.level(path).ok_or(GraphError::NoSuchLevel)?;
            level.instance_def(instance).ok_or(GraphError::NoSuchNode)?
        };
        let clone = self.def(def).ok_or(GraphError::NoSuchDef)?.clone();
        let copy = DefId(self.defs.insert(GraphDef {
            name: format!("{} copy", clone.name),
            scope: DefScope::Local,
            ..clone
        }));
        let level = self.level_mut(path).ok_or(GraphError::NoSuchLevel)?;
        level.set_instance(instance, Some(copy));
        Ok(copy)
    }
}
