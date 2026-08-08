//! Frame groups — the organisational half of PLAN_NODE.md's grouping
//! work (P4).
//!
//! A frame is a named, coloured box drawn *behind* a set of nodes.
//! Dragging its title bar moves every member. It has no pins, touches
//! no wires, and has no effect on dataflow whatsoever — Blender frames,
//! Unreal comment boxes, ComfyUI groups. Subgraphs, the other half, are
//! a different type entirely and deliberately so: frames never have
//! ports, never enter recursion checks, and must stay click-through so
//! rubber-band selection and canvas panning work over them.
//!
//! Everything here is **pure model**. The two functions that need to
//! know how big a node is take a `&dyn Fn(NodeId) -> Rect` provider
//! rather than a `Ui`, which is what keeps geometry testable with
//! synthetic rects and stops rendering leaking into the model.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::vocab::{Color32, Pos2, Rect, Vec2};
use smallvec::SmallVec;

use super::{Graph, NodeId};

/// Identifies a frame within one [`Graph`].
///
/// A slab key, and therefore **recycled** like [`NodeId`] — always
/// resolved against live frames rather than trusted. Node membership
/// holds one of these, and [`Graph::repair`] prunes any that no longer
/// resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct FrameId(pub usize);

/// What dragging a frame's title bar moves.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum FrameMove {
    /// Move the box and everything inside it. The default, and what
    /// "grouping" means to most people.
    #[default]
    WithContents,
    /// Move only the box, leaving nodes where they are — Unreal's
    /// per-comment toggle. Requires `shrink == false`, since an
    /// auto-fitting frame would immediately snap back.
    BoxOnly,
}

/// What happens to a frame's members when the frame goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum DisposeMode {
    /// Remove the frame, keep the nodes, re-parenting them to the
    /// frame's own parent. Blender's Ungroup, and the safe default:
    /// deleting a box should not delete work.
    #[default]
    Dissolve,
    /// Remove the frame and every node inside it, transitively.
    Purge,
}

/// A named box drawn behind a set of nodes.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Frame {
    pub title: String,
    pub color: Color32,
    /// Enclosing frame, for nesting. `None` at the top level.
    pub parent: Option<FrameId>,
    /// Auto-fit to members every frame.
    ///
    /// Mutually exclusive with manual resize: starting a resize clears
    /// this, keeping the bounds it had at that moment, so the box does
    /// not snap back under the cursor.
    pub shrink: bool,
    /// Authoritative extent when `!shrink`; the last computed fit when
    /// `shrink`.
    pub bounds: Rect,
    pub move_mode: FrameMove,
    /// Render as a pill and hide members from the node loop.
    pub collapsed: bool,
    /// Title text size in points. Blender's range is 8..64.
    pub label_size: f32,
}

/// Title band height as a multiple of the label size: line height plus
/// breathing room above and below, so the text is comfortably clear of
/// both the box's top edge and the topmost member.
///
/// Public because painting has to undo it — the band is clamped to the
/// box height for a frame resized down to the minimum, and the painter
/// derives a label size back out of the clamped band so the text cannot
/// spill out of the box.
pub const TITLE_BAND_RATIO: f32 = 1.6;

impl Frame {
    /// Height of the title band, derived from the label size.
    ///
    /// The band is what [`fit_frame_bounds`] reserves *above* the
    /// topmost member.
    #[must_use]
    pub fn title_band_height(&self) -> f32 {
        self.label_size * TITLE_BAND_RATIO
    }
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            title: String::new(),
            color: Color32::from_gray(128),
            parent: None,
            shrink: true,
            bounds: Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(0.0, 0.0)),
            move_mode: FrameMove::default(),
            collapsed: false,
            label_size: 14.0,
        }
    }
}

/// The title band strip at the top of `bounds`.
///
/// The **only** draggable part of a frame. A hotspot spanning the whole
/// box would be registered after the canvas's own pan/zoom response and
/// would therefore steal panning everywhere a frame exists, as well as
/// breaking rubber-band selection over it.
#[must_use]
pub fn title_band_rect(frame: &Frame, bounds: Rect) -> Rect {
    let h = frame.title_band_height().min(bounds.height());
    Rect::from_min_size(bounds.min, Vec2::new(bounds.width(), h))
}

/// The bounds a `shrink` frame should have, given where its members are.
///
/// `rects` supplies each node's rect in graph space — the renderer's
/// job, since only it knows how big a node drew. Child frames are
/// included, so a parent grows to contain nested groups.
///
/// The title band is reserved by pushing `min.y` **up**, not by
/// unioning member rects alone. Blender shipped the naive version and
/// filed T40094 against it: the topmost node sits under the title text.
#[must_use]
pub fn fit_frame_bounds<T>(
    graph: &Graph<T>,
    frame: FrameId,
    rects: &dyn Fn(NodeId) -> Rect,
    padding: f32,
) -> Option<Rect> {
    let f = graph.frame(frame)?;

    let mut acc: Option<Rect> = None;
    let mut extend = |r: Rect| {
        acc = Some(match acc {
            None => r,
            Some(a) => a.union(r),
        });
    };

    for node in graph.frame_members(frame) {
        extend(rects(node));
    }
    for (child, _) in graph.child_frames(frame) {
        if let Some(b) = fit_frame_bounds(graph, child, rects, padding) {
            extend(b);
        }
    }

    let base = acc?;
    let padded = base.expand(padding);
    Some(Rect::from_min_max(
        Pos2::new(padded.min.x, padded.min.y - f.title_band_height()),
        padded.max,
    ))
}

impl<T> Graph<T> {
    /// Declare a frame. Members are attached separately with
    /// [`Graph::set_node_frame`].
    pub fn insert_frame(
        &mut self,
        title: impl Into<String>,
        color: Color32,
        bounds: Rect,
    ) -> FrameId {
        let id = FrameId(self.ext.frames.insert(Frame {
            title: title.into(),
            color,
            bounds,
            ..Frame::default()
        }));
        self.touch();
        id
    }

    /// Remove a frame, disposing of its members per `mode`.
    ///
    /// Child frames are re-parented or removed to match, so neither
    /// mode can leave a frame pointing at a dead parent.
    pub fn remove_frame(&mut self, frame: FrameId, mode: DisposeMode) {
        if self.frame(frame).is_none() {
            return;
        }
        let parent = self.frame(frame).and_then(|f| f.parent);

        match mode {
            DisposeMode::Dissolve => {
                let members: Vec<NodeId> = self.frame_members(frame).collect();
                for node in members {
                    self.set_node_frame(node, parent);
                }
                let children: Vec<FrameId> = self.child_frames(frame).map(|(c, _)| c).collect();
                for child in children {
                    if let Some(f) = self.ext.frames.get_mut(child.0) {
                        f.parent = parent;
                    }
                }
            }
            DisposeMode::Purge => {
                for node in self.frame_members_deep(frame) {
                    self.remove_node(node);
                }
                let children: Vec<FrameId> = self.child_frames(frame).map(|(c, _)| c).collect();
                for child in children {
                    self.remove_frame(child, DisposeMode::Purge);
                }
            }
        }

        self.ext.frames.try_remove(frame.0);
        self.touch();
    }

    #[must_use]
    pub fn frame(&self, frame: FrameId) -> Option<&Frame> {
        self.ext.frames.get(frame.0)
    }

    /// Mutable access. Bumps the revision, because a caller reached for
    /// this precisely to change something.
    pub fn frame_mut(&mut self, frame: FrameId) -> Option<&mut Frame> {
        let existed = self.ext.frames.contains(frame.0);
        if existed {
            self.ext.revision = self.ext.revision.wrapping_add(1);
        }
        self.ext.frames.get_mut(frame.0)
    }

    pub fn frames(&self) -> impl Iterator<Item = (FrameId, &Frame)> + '_ {
        self.ext.frames.iter().map(|(i, f)| (FrameId(i), f))
    }

    /// Which frame a node belongs to, if any.
    #[must_use]
    pub fn frame_of(&self, node: NodeId) -> Option<FrameId> {
        self.nodes.get(node.0).and_then(|n| n.frame)
    }

    /// Attach a node to a frame, or detach it with `None`.
    ///
    /// A `frame` that does not resolve is treated as `None` rather than
    /// stored — the alternative is a dangling membership that
    /// [`Graph::repair`] would have to clean up later.
    pub fn set_node_frame(&mut self, node: NodeId, frame: Option<FrameId>) {
        let frame = frame.filter(|f| self.ext.frames.contains(f.0));
        if let Some(n) = self.nodes.get_mut(node.0) {
            n.frame = frame;
            self.ext.revision = self.ext.revision.wrapping_add(1);
        }
    }

    /// Nodes directly inside this frame — **not** those inside its
    /// child frames. See [`Graph::frame_members_deep`].
    pub fn frame_members(&self, frame: FrameId) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes
            .iter()
            .filter(move |(_, n)| n.frame == Some(frame))
            .map(|(i, _)| NodeId(i))
    }

    /// Every node inside this frame, transitively through child frames.
    #[must_use]
    pub fn frame_members_deep(&self, frame: FrameId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self.frame_members(frame).collect();
        for (child, _) in self.child_frames(frame) {
            out.extend(self.frame_members_deep(child));
        }
        out
    }

    /// Frames whose parent is `frame`.
    pub fn child_frames(&self, frame: FrameId) -> impl Iterator<Item = (FrameId, &Frame)> + '_ {
        self.frames().filter(move |(_, f)| f.parent == Some(frame))
    }

    /// How deeply nested this frame is. `0` at the top level.
    ///
    /// Walks bounded by the frame count, so a cycle that slipped past
    /// [`Graph::can_parent_frame`] returns a number instead of hanging.
    #[must_use]
    pub fn frame_depth(&self, frame: FrameId) -> u32 {
        let mut depth = 0;
        let mut cur = self.frame(frame).and_then(|f| f.parent);
        let cap = self.ext.frames.len();
        while let Some(p) = cur {
            depth += 1;
            if depth as usize > cap {
                break;
            }
            cur = self.frame(p).and_then(|f| f.parent);
        }
        depth
    }

    /// Whether `parent` may enclose `child` without forming a cycle.
    ///
    /// Rejects self-parenting and any ancestor of `parent` being
    /// `child`. Without this, one drag produces a parent chain that
    /// never terminates, and the first thing to walk it hangs.
    #[must_use]
    pub fn can_parent_frame(&self, child: FrameId, parent: Option<FrameId>) -> bool {
        let Some(parent) = parent else {
            return self.frame(child).is_some();
        };
        if self.frame(child).is_none() || self.frame(parent).is_none() {
            return false;
        }
        if child == parent {
            return false;
        }
        let mut cur = Some(parent);
        let cap = self.ext.frames.len() + 1;
        for _ in 0..cap {
            match cur {
                None => return true,
                Some(f) if f == child => return false,
                Some(f) => cur = self.frame(f).and_then(|x| x.parent),
            }
        }
        false
    }

    /// Re-parent a frame. Returns `false` and changes nothing when the
    /// move would form a cycle.
    pub fn set_frame_parent(&mut self, child: FrameId, parent: Option<FrameId>) -> bool {
        if !self.can_parent_frame(child, parent) {
            return false;
        }
        if let Some(f) = self.ext.frames.get_mut(child.0) {
            f.parent = parent;
            self.ext.revision = self.ext.revision.wrapping_add(1);
            true
        } else {
            false
        }
    }

    /// The nodes a drag starting on `node` should move.
    ///
    /// Dragging a member of the current selection moves the whole
    /// selection; dragging anything else moves only it, and does not
    /// disturb the selection. Pure, so the renderer's deferred-move
    /// site stays a single line.
    #[must_use]
    pub fn drag_targets_node(&self, node: NodeId, selected: &[NodeId]) -> SmallVec<[NodeId; 16]> {
        if selected.contains(&node) {
            selected
                .iter()
                .copied()
                .filter(|n| self.contains(*n))
                .collect()
        } else {
            let mut out = SmallVec::new();
            if self.contains(node) {
                out.push(node);
            }
            out
        }
    }

    /// The nodes a drag on `frame`'s title bar should move —
    /// transitively through child frames, deduplicated.
    #[must_use]
    pub fn drag_targets_frame(&self, frame: FrameId) -> SmallVec<[NodeId; 16]> {
        let mut out: SmallVec<[NodeId; 16]> = SmallVec::new();
        for n in self.frame_members_deep(frame) {
            if !out.contains(&n) {
                out.push(n);
            }
        }
        out
    }

    /// A frame's effective extent: the fitted bounds when `shrink`,
    /// the stored bounds otherwise.
    ///
    /// `None` for a `shrink` frame with nothing in it — an empty group
    /// has no meaningful size, and inventing one puts a stray box on
    /// the canvas.
    #[must_use]
    pub fn frame_bounds(
        &self,
        frame: FrameId,
        rects: &dyn Fn(NodeId) -> Rect,
        padding: f32,
    ) -> Option<Rect> {
        let f = self.frame(frame)?;
        if f.shrink {
            fit_frame_bounds(self, frame, rects, padding)
        } else {
            Some(f.bounds)
        }
    }

    /// The **innermost** frame whose bounds contain `p`.
    ///
    /// Innermost so that dropping a node into a nested group attaches
    /// it to the group it looks like it landed in, not the outermost
    /// box that happens to also contain the point.
    #[must_use]
    pub fn frame_at(
        &self,
        p: Pos2,
        rects: &dyn Fn(NodeId) -> Rect,
        padding: f32,
    ) -> Option<FrameId> {
        let mut best: Option<(FrameId, u32)> = None;
        for (id, _) in self.frames() {
            let Some(b) = self.frame_bounds(id, rects, padding) else {
                continue;
            };
            if !b.contains(p) {
                continue;
            }
            let depth = self.frame_depth(id);
            if best.is_none_or(|(_, d)| depth > d) {
                best = Some((id, depth));
            }
        }
        best.map(|(id, _)| id)
    }

    /// Whether a node is inside a collapsed frame, at any depth.
    ///
    /// A collapsed frame renders as a pill and its contents are skipped
    /// by the node loop. Ancestors count: collapsing an outer group has
    /// to hide what is inside its inner groups too, or folding a group
    /// leaves its nested contents floating with nothing around them.
    #[must_use]
    pub fn is_collapsed_away(&self, node: NodeId) -> bool {
        let mut cur = self.frame_of(node);
        let cap = self.ext.frames.len() + 1;
        for _ in 0..cap {
            match cur {
                None => return false,
                Some(f) => match self.frame(f) {
                    None => return false,
                    Some(fr) if fr.collapsed => return true,
                    Some(fr) => cur = fr.parent,
                },
            }
        }
        false
    }

    /// Drop memberships and parent links that no longer resolve, and
    /// break any parent cycle. Called from [`Graph::repair`].
    pub(crate) fn repair_frames(&mut self) {
        let live: Vec<usize> = self.ext.frames.iter().map(|(i, _)| i).collect();
        let is_live = |f: FrameId| live.contains(&f.0);

        for (_, node) in self.nodes.iter_mut() {
            if let Some(f) = node.frame
                && !is_live(f)
            {
                node.frame = None;
            }
        }

        let ids: Vec<FrameId> = self.frames().map(|(i, _)| i).collect();
        for id in &ids {
            let parent = self.frame(*id).and_then(|f| f.parent);
            let bad = match parent {
                None => false,
                Some(p) => !is_live(p) || !self.can_parent_frame(*id, Some(p)),
            };
            if bad && let Some(f) = self.ext.frames.get_mut(id.0) {
                f.parent = None;
            }
        }
    }
}
