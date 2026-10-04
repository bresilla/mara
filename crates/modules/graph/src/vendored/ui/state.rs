use std::collections::HashSet;

use egui::{Id, Pos2, Rect, Vec2, emath::GuiRounding};
use mara_core::context::MaraCtx;
use mara_core::transform::Transform;
use smallvec::{SmallVec, ToSmallVec, smallvec};

use crate::vendored::{Graph, InPinId, NodeId, OutPinId};

pub type RowHeights = SmallVec<[f32; 8]>;

/// Node UI state.
#[derive(Debug)]
pub struct NodeState {
    /// Node size for this frame.
    /// It is updated to fit content.
    size: Vec2,
    header_height: f32,
    input_heights: RowHeights,
    output_heights: RowHeights,

    id: Id,
    dirty: bool,
}

#[derive(Clone, PartialEq)]
struct NodeData {
    size: Vec2,
    header_height: f32,
    input_heights: RowHeights,
    output_heights: RowHeights,
}

impl NodeState {
    /// Per-node measured geometry, from Mara memory.
    ///
    /// Ported off the raw `egui::Context` data store in PLAN_NODE.md
    /// P2, matching what [`GraphState`] already did. The frame pre-pass
    /// that P5 adds calls this from outside the render body, and doing
    /// that through a backend context would have reintroduced an egui
    /// reference in a file that is otherwise nearly free of them.
    pub fn load(cx: &dyn MaraCtx, id: Id) -> Self {
        cx.memory().get_temp::<NodeData>(mara_id(id)).map_or_else(
            || {
                cx.request_discard("NodeState initialization");
                Self::initial(id)
            },
            |data| NodeState {
                size: data.size,
                header_height: data.header_height,
                input_heights: data.input_heights,
                output_heights: data.output_heights,
                id,
                dirty: false,
            },
        )
    }

    pub fn clear(self, cx: &dyn MaraCtx) {
        cx.memory().remove_temp::<NodeData>(mara_id(self.id));
    }

    pub fn store(self, cx: &dyn MaraCtx) {
        if self.dirty {
            cx.memory().set_temp(
                mara_id(self.id),
                NodeData {
                    size: self.size,
                    header_height: self.header_height,
                    input_heights: self.input_heights,
                    output_heights: self.output_heights,
                },
            );
            cx.request_repaint();
        }
    }

    /// Finds node rect at specific position (excluding node frame margin).
    pub fn node_rect(&self, pos: Pos2, openness: f32) -> Rect {
        Rect::from_min_size(
            pos,
            Vec2::new(
                self.size.x,
                f32::max(self.header_height, self.size.y * openness),
            ),
        )
        .round_ui()
    }

    pub fn payload_offset(&self, openness: f32) -> f32 {
        ((self.size.y) * (1.0 - openness)).round_ui()
    }

    pub fn set_size(&mut self, size: Vec2) {
        if self.size != size {
            self.size = size;
            self.dirty = true;
        }
    }

    pub fn header_height(&self) -> f32 {
        self.header_height.round_ui()
    }

    pub fn set_header_height(&mut self, height: f32) {
        #[allow(clippy::float_cmp)]
        if self.header_height != height {
            self.header_height = height;
            self.dirty = true;
        }
    }

    pub const fn input_heights(&self) -> &RowHeights {
        &self.input_heights
    }

    pub const fn output_heights(&self) -> &RowHeights {
        &self.output_heights
    }

    pub fn set_input_heights(&mut self, input_heights: RowHeights) {
        #[allow(clippy::float_cmp)]
        if self.input_heights != input_heights {
            self.input_heights = input_heights;
            self.dirty = true;
        }
    }

    pub fn set_output_heights(&mut self, output_heights: RowHeights) {
        #[allow(clippy::float_cmp)]
        if self.output_heights != output_heights {
            self.output_heights = output_heights;
            self.dirty = true;
        }
    }

    /// First-frame placeholder geometry, replaced as soon as the node
    /// measures itself — [`NodeState::load`] calls `request_discard`
    /// when it lands here, so this size is never what the user sees.
    fn initial(id: Id) -> Self {
        let interact = mara_core::style::interact_size();
        NodeState {
            size: Vec2::new(interact.x, interact.y),
            header_height: interact.y,
            input_heights: SmallVec::new_const(),
            output_heights: SmallVec::new_const(),
            id,
            dirty: true,
        }
    }
}

#[derive(Clone)]
pub enum NewWires {
    In(SmallVec<[InPinId; 4]>),
    Out(SmallVec<[OutPinId; 4]>),
}

#[derive(Clone, Copy)]
struct RectSelect {
    origin: Pos2,
    current: Pos2,
}

pub struct GraphState {
    /// Graph viewport transform to global space.
    to_global: Transform,

    new_wires: Option<NewWires>,

    /// Flag indicating that new wires are owned by the menu now.
    new_wires_menu: bool,

    id: Id,

    /// Flag indicating that the graph state is dirty must be saved.
    dirty: bool,

    /// Active rect selection.
    rect_selection: Option<RectSelect>,

    /// Order of nodes to draw.
    draw_order: Vec<NodeId>,

    /// List of currently selected nodes.
    selected_nodes: SmallVec<[NodeId; 8]>,

    /// Pending camera destination — see [`GraphState::fly_to`].
    camera_target: Option<Transform>,
}

#[derive(Clone, Default)]
struct DrawOrder(Vec<NodeId>);

impl DrawOrder {
    fn save(self, cx: &dyn MaraCtx, id: Id) {
        let mut mem = cx.memory();
        if self.0.is_empty() {
            mem.remove_temp::<Self>(mara_id(id));
        } else {
            mem.set_temp(mara_id(id), self);
        }
    }

    fn load(cx: &dyn MaraCtx, id: Id) -> Self {
        cx.memory()
            .get_temp::<Self>(mara_id(id))
            .unwrap_or_default()
    }
}

#[derive(Clone, Default)]
struct SelectedNodes(SmallVec<[NodeId; 8]>);

impl SelectedNodes {
    fn save(self, cx: &dyn MaraCtx, id: Id) {
        let mut mem = cx.memory();
        if self.0.is_empty() {
            mem.remove_temp::<Self>(mara_id(id));
        } else {
            // The original also wrote through `get_temp_mut_or_default`
            // before inserting; that write was immediately overwritten
            // by the insert, so only the insert is kept.
            mem.set_temp(mara_id(id), self);
        }
    }

    fn load(cx: &dyn MaraCtx, id: Id) -> Self {
        cx.memory()
            .get_temp::<Self>(mara_id(id))
            .unwrap_or_default()
    }
}

#[derive(Clone)]
struct GraphStateData {
    to_global: Transform,
    new_wires: Option<NewWires>,
    new_wires_menu: bool,
    rect_selection: Option<RectSelect>,
    /// Where the camera is heading, when a view change was requested
    /// through the spring rather than applied outright.
    camera_target: Option<Transform>,
}

impl GraphStateData {
    fn save(self, cx: &dyn MaraCtx, id: Id) {
        cx.memory().set_temp(mara_id(id), self);
    }

    fn load(cx: &dyn MaraCtx, id: Id) -> Option<Self> {
        cx.memory().get_temp(mara_id(id))
    }
}

/// egui's `Id` is what this vendored code threads around; the Mara store
/// is keyed by `vocab::Id`. One conversion point rather than a cast at
/// every call.
fn mara_id(id: Id) -> mara_core::vocab::Id {
    id.into()
}

fn prune_selected_nodes<T>(selected_nodes: &mut SmallVec<[NodeId; 8]>, graph: &Graph<T>) -> bool {
    let old_size = selected_nodes.len();
    selected_nodes.retain(|node| graph.nodes.contains(node.0));
    old_size != selected_nodes.len()
}

impl GraphState {
    pub fn load<T>(
        cx: &dyn MaraCtx,
        id: Id,
        graph: &Graph<T>,
        ui_rect: Rect,
        min_scale: f32,
        max_scale: f32,
    ) -> Self {
        let Some(data) = GraphStateData::load(cx, id) else {
            cx.request_discard("Initial placing");
            return Self::initial(id, graph, ui_rect, min_scale, max_scale);
        };

        let mut selected_nodes = SelectedNodes::load(cx, id).0;
        let dirty = prune_selected_nodes(&mut selected_nodes, graph);

        let draw_order = DrawOrder::load(cx, id).0;

        GraphState {
            to_global: data.to_global,
            new_wires: data.new_wires,
            new_wires_menu: data.new_wires_menu,
            id,
            dirty,
            rect_selection: data.rect_selection,
            camera_target: data.camera_target,
            draw_order,
            selected_nodes,
        }
    }

    fn initial<T>(id: Id, graph: &Graph<T>, ui_rect: Rect, min_scale: f32, max_scale: f32) -> Self {
        let mut bb = Rect::NOTHING;

        for (_, node) in &graph.nodes {
            bb.extend_with(Pos2::from(node.pos));
        }

        if bb.is_finite() {
            bb = bb.expand(100.0);
        } else if ui_rect.is_finite() {
            bb = ui_rect;
        } else {
            bb = Rect::from_min_max(Pos2::new(-100.0, -100.0), Pos2::new(100.0, 100.0));
        }

        let scaling2 = ui_rect.size() / bb.size();
        let scaling = scaling2.min_elem().clamp(min_scale, max_scale);

        let to_global = fit_points(bb.center(), ui_rect.center(), scaling);

        GraphState {
            to_global,
            new_wires: None,
            new_wires_menu: false,
            id,
            dirty: true,
            draw_order: Vec::new(),
            rect_selection: None,
            selected_nodes: SmallVec::new(),
            camera_target: None,
        }
    }

    #[inline(always)]
    pub fn store<T>(mut self, graph: &Graph<T>, cx: &dyn MaraCtx) {
        self.dirty |= prune_selected_nodes(&mut self.selected_nodes, graph);

        if self.dirty {
            let data = GraphStateData {
                to_global: self.to_global,
                new_wires: self.new_wires,
                new_wires_menu: self.new_wires_menu,
                rect_selection: self.rect_selection,
                camera_target: self.camera_target,
            };
            data.save(cx, self.id);

            DrawOrder(self.draw_order).save(cx, self.id);
            SelectedNodes(self.selected_nodes).save(cx, self.id);

            cx.request_repaint();
        }
    }

    pub const fn to_global(&self) -> Transform {
        self.to_global
    }

    pub fn set_to_global(&mut self, to_global: Transform) {
        if self.to_global != to_global {
            self.to_global = to_global;
            self.dirty = true;
        }
    }

    /// Add `delta` (in sub-context points) to the saved
    /// translation of the graph with the given `id`,
    /// directly via context data — no live `GraphState` instance
    /// required.
    ///
    /// Used by the outside-in zoom path in `node_view::show` to
    /// keep the scene point under the cursor stationary across a
    /// `zoom` step. The widget reads the saved `to_global` on the
    /// next `GraphState::load`, so writing here BEFORE
    /// `GraphWidget::show` runs makes the new translation take
    /// effect this frame.
    pub fn nudge_saved_translation(
        cx: &dyn MaraCtx,
        id: mara_core::vocab::Id,
        delta: mara_core::vocab::Vec2,
    ) {
        let id = Id::from(id);
        let Some(mut data) = GraphStateData::load(cx, id) else {
            return;
        };
        data.to_global.translation += delta;
        data.save(cx, id);
    }

    pub fn look_at(&mut self, view: Rect, ui_rect: Rect, min_scale: f32, max_scale: f32) {
        let to_global = Self::fit_transform(view, ui_rect, min_scale, max_scale);
        if self.to_global != to_global {
            self.to_global = to_global;
            self.camera_target = None;
            self.dirty = true;
        }
    }

    /// Ease the view to fit `view`, rather than jumping to it.
    ///
    /// The destination is stored and consumed by the renderer's camera
    /// spring. Every "move the view somewhere" feature routes through
    /// here — fit-to-content, fit-to-selection, breadcrumb jumps — so
    /// each is a target rather than a bespoke animation.
    pub fn fly_to(&mut self, view: Rect, ui_rect: Rect, min_scale: f32, max_scale: f32) {
        let target = Self::fit_transform(view, ui_rect, min_scale, max_scale);
        if self.camera_target != Some(target) {
            self.camera_target = Some(target);
            self.dirty = true;
        }
    }

    /// The pending camera destination, if any.
    #[must_use]
    pub const fn camera_target(&self) -> Option<Transform> {
        self.camera_target
    }

    /// Plant a level's camera before it has ever rendered: place it at
    /// `from` and aim it at `to`.
    ///
    /// Used by the subgraph dive, which has to set up the child level's
    /// view from the *parent's* pass — the child has no saved state yet,
    /// and by the time it renders the information about where it was
    /// opened from is gone.
    pub fn seed_view(cx: &dyn MaraCtx, id: mara_core::vocab::Id, from: Transform, to: Transform) {
        let id = Id::from(id);
        let data = GraphStateData {
            to_global: from,
            new_wires: None,
            new_wires_menu: false,
            rect_selection: None,
            camera_target: Some(to),
        };
        data.save(cx, id);
    }

    /// Clear the pending destination — the camera has arrived.
    pub fn clear_camera_target(&mut self) {
        if self.camera_target.is_some() {
            self.camera_target = None;
            self.dirty = true;
        }
    }

    fn fit_transform(view: Rect, ui_rect: Rect, min_scale: f32, max_scale: f32) -> Transform {
        let scaling2 = ui_rect.size() / view.size();
        let scaling = scaling2.min_elem().clamp(min_scale, max_scale);
        fit_points(view.center(), ui_rect.center(), scaling)
    }

    pub fn start_new_wire_in(&mut self, pin: InPinId) {
        self.new_wires = Some(NewWires::In(smallvec![pin]));
        self.new_wires_menu = false;
        self.dirty = true;
    }

    pub fn start_new_wire_out(&mut self, pin: OutPinId) {
        self.new_wires = Some(NewWires::Out(smallvec![pin]));
        self.new_wires_menu = false;
        self.dirty = true;
    }

    pub fn start_new_wires_in(&mut self, pins: &[InPinId]) {
        self.new_wires = Some(NewWires::In(pins.to_smallvec()));
        self.new_wires_menu = false;
        self.dirty = true;
    }

    pub fn start_new_wires_out(&mut self, pins: &[OutPinId]) {
        self.new_wires = Some(NewWires::Out(pins.to_smallvec()));
        self.new_wires_menu = false;
        self.dirty = true;
    }

    pub fn add_new_wire_in(&mut self, pin: InPinId) {
        debug_assert!(!self.new_wires_menu);
        let Some(NewWires::In(pins)) = &mut self.new_wires else {
            unreachable!();
        };

        if !pins.contains(&pin) {
            pins.push(pin);
            self.dirty = true;
        }
    }

    pub fn add_new_wire_out(&mut self, pin: OutPinId) {
        debug_assert!(!self.new_wires_menu);
        let Some(NewWires::Out(pins)) = &mut self.new_wires else {
            unreachable!();
        };

        if !pins.contains(&pin) {
            pins.push(pin);
            self.dirty = true;
        }
    }

    pub fn remove_new_wire_in(&mut self, pin: InPinId) {
        debug_assert!(!self.new_wires_menu);
        let Some(NewWires::In(pins)) = &mut self.new_wires else {
            unreachable!();
        };

        if let Some(idx) = pins.iter().position(|p| *p == pin) {
            pins.swap_remove(idx);
            self.dirty = true;
        }
    }

    pub fn remove_new_wire_out(&mut self, pin: OutPinId) {
        debug_assert!(!self.new_wires_menu);
        let Some(NewWires::Out(pins)) = &mut self.new_wires else {
            unreachable!();
        };

        if let Some(idx) = pins.iter().position(|p| *p == pin) {
            pins.swap_remove(idx);
            self.dirty = true;
        }
    }

    pub const fn has_new_wires(&self) -> bool {
        matches!(
            (self.new_wires.as_ref(), self.new_wires_menu),
            (Some(_), false)
        )
    }

    pub const fn has_new_wires_in(&self) -> bool {
        matches!(
            (&self.new_wires, self.new_wires_menu),
            (Some(NewWires::In(_)), false)
        )
    }

    pub const fn has_new_wires_out(&self) -> bool {
        matches!(
            (&self.new_wires, self.new_wires_menu),
            (Some(NewWires::Out(_)), false)
        )
    }

    pub const fn new_wires(&self) -> Option<&NewWires> {
        match (&self.new_wires, self.new_wires_menu) {
            (Some(new_wires), false) => Some(new_wires),
            _ => None,
        }
    }

    pub const fn take_new_wires(&mut self) -> Option<NewWires> {
        match (&self.new_wires, self.new_wires_menu) {
            (Some(_), false) => {
                self.dirty = true;
                self.new_wires.take()
            }
            _ => None,
        }
    }

    pub(crate) const fn take_new_wires_menu(&mut self) -> Option<NewWires> {
        match (&self.new_wires, self.new_wires_menu) {
            (Some(_), true) => {
                self.dirty = true;
                self.new_wires.take()
            }
            _ => None,
        }
    }

    pub(crate) fn set_new_wires_menu(&mut self, wires: NewWires) {
        debug_assert!(self.new_wires.is_none());
        self.new_wires = Some(wires);
        self.new_wires_menu = true;
    }

    pub(crate) fn update_draw_order<T>(&mut self, graph: &Graph<T>) -> Vec<NodeId> {
        let mut node_ids = graph
            .nodes
            .iter()
            .map(|(id, _)| NodeId(id))
            .collect::<HashSet<_>>();

        self.draw_order.retain(|id| {
            let has = node_ids.remove(id);
            self.dirty |= !has;
            has
        });

        self.dirty |= !node_ids.is_empty();

        for new_id in node_ids {
            self.draw_order.push(new_id);
        }

        self.draw_order.clone()
    }

    pub(crate) fn node_to_top(&mut self, node: NodeId) {
        if let Some(order) = self.draw_order.iter().position(|idx| *idx == node) {
            self.draw_order.remove(order);
            self.draw_order.push(node);
        }
        self.dirty = true;
    }

    pub fn selected_nodes(&self) -> &[NodeId] {
        &self.selected_nodes
    }

    pub fn select_one_node(&mut self, reset: bool, node: NodeId) {
        if reset {
            if self.selected_nodes[..] == [node] {
                return;
            }

            self.deselect_all_nodes();
        } else if let Some(pos) = self.selected_nodes.iter().position(|n| *n == node) {
            if pos == self.selected_nodes.len() - 1 {
                return;
            }
            self.selected_nodes.remove(pos);
        }
        self.selected_nodes.push(node);
        self.dirty = true;
    }

    pub fn select_many_nodes(&mut self, reset: bool, nodes: impl Iterator<Item = NodeId>) {
        if reset {
            self.deselect_all_nodes();
            self.selected_nodes.extend(nodes);
            self.dirty = true;
        } else {
            nodes.for_each(|node| self.select_one_node(false, node));
        }
    }

    pub fn deselect_one_node(&mut self, node: NodeId) {
        if let Some(pos) = self.selected_nodes.iter().position(|n| *n == node) {
            self.selected_nodes.remove(pos);
            self.dirty = true;
        }
    }

    pub fn deselect_many_nodes(&mut self, nodes: impl Iterator<Item = NodeId>) {
        for node in nodes {
            if let Some(pos) = self.selected_nodes.iter().position(|n| *n == node) {
                self.selected_nodes.remove(pos);
                self.dirty = true;
            }
        }
    }

    pub fn deselect_all_nodes(&mut self) {
        self.dirty |= !self.selected_nodes.is_empty();
        self.selected_nodes.clear();
    }

    pub const fn start_rect_selection(&mut self, pos: Pos2) {
        self.dirty |= self.rect_selection.is_none();
        self.rect_selection = Some(RectSelect {
            origin: pos,
            current: pos,
        });
    }

    pub const fn stop_rect_selection(&mut self) {
        self.dirty |= self.rect_selection.is_some();
        self.rect_selection = None;
    }

    pub const fn is_rect_selection(&self) -> bool {
        self.rect_selection.is_some()
    }

    pub const fn update_rect_selection(&mut self, pos: Pos2) {
        if let Some(rect_selection) = &mut self.rect_selection {
            rect_selection.current = pos;
            self.dirty = true;
        }
    }

    pub fn rect_selection(&self) -> Option<Rect> {
        let rect = self.rect_selection?;
        Some(Rect::from_two_pos(rect.origin, rect.current))
    }
}

impl GraphState {
    /// The nodes currently selected in the graph stored under `id`,
    /// readable from outside a render pass.
    ///
    /// Replaces `GraphWidget::get_selected_nodes{,_at}`, which were not
    /// merely egui-typed but **wrong**: they read
    /// `ctx.data(|d| d.get_temp::<SelectedNodes>(graph_id))` — egui's
    /// store, keyed by the raw egui id — while [`SelectedNodes::save`]
    /// writes to Mara memory under `mara_id(id)`. They returned an
    /// empty list for every caller, which is why nothing noticed.
    /// `id` is the id given to [`GraphWidget::id`], and the round trip
    /// through it is deliberate: `vocab::Id -> egui::Id` re-hashes and
    /// is documented in `mara_core` as **not** the inverse of
    /// `egui::Id -> vocab::Id`. The widget converts on the way in, so a
    /// reader must convert identically or it looks under a key nothing
    /// ever wrote — which is a quieter version of the same bug that
    /// made `get_selected_nodes` always return empty.
    #[must_use]
    pub fn selection(cx: &dyn MaraCtx, id: mara_core::vocab::Id) -> SmallVec<[NodeId; 8]> {
        cx.memory()
            .get_temp::<SelectedNodes>(mara_id(Id::from(id)))
            .unwrap_or_default()
            .0
    }
}

/// Transform placing `from` at `to` under uniform `scaling`.
///
/// Replaces two of the three local transform helpers this file used to
/// share with `ui.rs`. Plain arithmetic rather than a round trip through
/// [`Transform::scaled_around`]: the anchored-rescale case genuinely
/// needs that, but `translation = to - scaling * from` does not.
fn fit_points(from: Pos2, to: Pos2, scaling: f32) -> Transform {
    Transform::new(
        mara_core::vocab::Vec2::new(to.x - scaling * from.x, to.y - scaling * from.y),
        scaling,
    )
}
