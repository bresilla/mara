//! The camera, the input state machine, and the frame.
//!
//! # Why the state lives in the caller
//!
//! The renderer this replaces kept its camera and its selection in the
//! backend's keyed memory store. Two bugs followed directly from that,
//! and both were shipped: the selection was written under one key and
//! read under another, so it was permanently empty; and two graphs on
//! screen at once shared a key, so panning one panned the other.
//!
//! Here the state is a plain [`GraphViewState`] the app owns. There is
//! no key, so there is nothing to get wrong, and two graphs differ
//! exactly when the app holds two of them.
//!
//! The whole view also takes a single backend interaction id — one
//! `canvas_at` over the viewport — and does its own hit-testing against
//! the rects [`super::layout`] produced. Per-node ids were the other
//! source of collisions, and none are needed: the layout already knows
//! where everything is.
//!
//! This file is checked by `make check` to contain no backend types.

use std::collections::BTreeSet;

use mara_core::MaraUi;
use mara_core::mui::MaraKey;
use mara_core::layout::{ChildRegion, CursorIcon, Sense, StackAlign};
use mara_core::vocab::{Color32, CornerRadius, PointerButton, Pos2, Rect, Stroke, Vec2};

use super::layout::{NodeLayout, NodeShape, layout_node};
use super::paint::{NodeState, paint_canvas, paint_node, paint_pin, paint_wire, wire_points};
use super::group::{band_at, move_frame, paint_frame, place_frames};
use super::spec::GraphSpec;
use crate::{FrameId, Graph, InPinId, NodeId, OutPinId};

/// Where the graph is being looked at from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// Screen position of the graph's origin.
    pub pan: Vec2,
    /// Screen points per graph point.
    pub zoom: f32,
}

impl Default for Camera {
    fn default() -> Self {
        Self {
            pan: Vec2::new(0.0, 0.0),
            zoom: 1.0,
        }
    }
}

impl Camera {
    /// Smallest and largest zoom the camera will take.
    pub const ZOOM_RANGE: (f32, f32) = (0.2, 3.0);

    /// Below this, node titles fall under the renderer's legibility
    /// floor and stop being drawn.
    pub const LEGIBLE_ZOOM: f32 = 0.75;

    #[must_use]
    pub fn to_screen(self, g: Pos2) -> Pos2 {
        Pos2::new(g.x * self.zoom + self.pan.x, g.y * self.zoom + self.pan.y)
    }

    #[must_use]
    pub fn to_graph(self, s: Pos2) -> Pos2 {
        Pos2::new((s.x - self.pan.x) / self.zoom, (s.y - self.pan.y) / self.zoom)
    }

    /// Zoom by `factor` while holding `anchor` (screen space) still.
    ///
    /// Zooming about the pointer rather than about the viewport centre
    /// is what makes a wheel feel like it is moving the paper instead
    /// of moving the camera somewhere else.
    pub fn zoom_about(&mut self, factor: f32, anchor: Pos2) {
        let (lo, hi) = Self::ZOOM_RANGE;
        let next = (self.zoom * factor).clamp(lo, hi);
        let k = next / self.zoom;
        self.pan = Vec2::new(
            anchor.x - (anchor.x - self.pan.x) * k,
            anchor.y - (anchor.y - self.pan.y) * k,
        );
        self.zoom = next;
    }

    /// Frame `bounds` (graph space) inside `area` (screen space).
    ///
    /// Never magnifies past 1:1. Blowing a two-node graph up to fill
    /// the viewport reads as broken rather than as helpful, so framing
    /// only ever zooms out.
    pub fn fit(&mut self, bounds: Rect, area: Rect, margin: f32) {
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        let (lo, _) = Self::ZOOM_RANGE;
        let sx = (area.width() - 2.0 * margin) / bounds.width();
        let sy = (area.height() - 2.0 * margin) / bounds.height();
        self.zoom = sx.min(sy).clamp(lo, 1.0);
        self.centre_on(bounds.center(), area);
    }

    /// How a graph is framed the first time it is shown.
    ///
    /// Fits, but refuses to go below the zoom at which node titles stop
    /// being drawn. Opening a large graph zoomed out far enough to see
    /// all of it means opening it on a field of unreadable slabs; a
    /// legible view of part of it is more use than an illegible view of
    /// the whole.
    pub fn frame_initial(&mut self, bounds: Rect, area: Rect, margin: f32) {
        self.fit(bounds, area, margin);
        if self.zoom < Self::LEGIBLE_ZOOM {
            self.zoom = Self::LEGIBLE_ZOOM;
            self.centre_on(bounds.center(), area);
        }
    }

    /// Put `at` (graph space) at the centre of `area`.
    pub fn centre_on(&mut self, at: Pos2, area: Rect) {
        let target = area.center();
        self.pan = Vec2::new(target.x - at.x * self.zoom, target.y - at.y * self.zoom);
    }
}

/// What the pointer is in the middle of doing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Gesture {
    /// Moving the selected nodes; the anchor is in graph space.
    MoveNodes { last: Pos2 },
    /// Rubber-band selecting from a screen-space anchor.
    BoxSelect { from: Pos2, additive: bool },
    /// Pulling a wire out of an output, looking for an input.
    WireFromOutput(OutPinId),
    /// Pulling a wire out of an input, looking for an output.
    WireFromInput(InPinId),
    /// Dragging the canvas itself.
    Pan,
    /// Moving a frame and everything inside it; anchor in graph space.
    MoveFrame { frame: FrameId, last: Pos2 },
}

/// Everything the view remembers between frames.
///
/// Owned by the app, deliberately — see the module docs.
#[derive(Clone, Debug, Default)]
pub struct GraphViewState {
    pub camera: Camera,
    pub selection: BTreeSet<NodeId>,
    gesture: Option<Gesture>,
    /// Set once, the first time the view is shown, so a fresh graph is
    /// framed instead of starting off-screen at the origin.
    framed: bool,
}

impl GraphViewState {
    /// A view already pointed at `camera`, which suppresses the
    /// one-time framing a default view does on its first paint.
    #[must_use]
    pub fn at(camera: Camera) -> Self {
        Self {
            camera,
            framed: true,
            ..Default::default()
        }
    }

    /// Ask for the graph to be framed on the next paint.
    pub fn request_fit(&mut self) {
        self.framed = false;
    }

    /// Is a wire currently being pulled?
    #[must_use]
    pub fn is_wiring(&self) -> bool {
        matches!(
            self.gesture,
            Some(Gesture::WireFromOutput(_) | Gesture::WireFromInput(_))
        )
    }
}

/// What the app tells the renderer about its nodes.
///
/// Every method is asked *before* anything is painted, and the answers
/// fully determine the geometry. An app cannot make one node wider than
/// another by drawing more into it, which is the point.
pub trait GraphView<T> {
    /// Title and pin labels. Called once per visible node per frame.
    ///
    /// The whole graph is passed, not just the payload, because a node
    /// that stands for something else — a subgraph instance, a boundary
    /// port — cannot name itself from its payload alone.
    fn shape(&mut self, id: NodeId, graph: &Graph<T>) -> NodeShape;

    /// The node's colour, spent on its header band. `None` leaves the
    /// node uncoloured.
    fn tint(&mut self, id: NodeId, graph: &Graph<T>) -> Option<Color32> {
        let _ = (id, graph);
        None
    }

    /// Colour of one input pin and of the wires arriving at it.
    fn input_color(&mut self, pin: InPinId, graph: &Graph<T>) -> Option<Color32> {
        let _ = (pin, graph);
        None
    }

    /// Colour of one output pin and of the wires leaving it.
    fn output_color(&mut self, pin: OutPinId, graph: &Graph<T>) -> Option<Color32> {
        let _ = (pin, graph);
        None
    }

    /// May these two pins be joined? Refusing here is what stops an
    /// incompatible wire being made, and greys the target while the
    /// wire is being dragged.
    fn accepts(&mut self, from: OutPinId, to: InPinId, graph: &Graph<T>) -> bool {
        let _ = (from, to, graph);
        true
    }

    /// Draw into the body area a node reserved via [`NodeShape::body_h`].
    ///
    /// The graph is passed mutably because a node body is usually where
    /// a value lives — a number to drag, a colour to pick, an operator
    /// to choose — and an editor that cannot write back is decoration.
    fn body(&mut self, id: NodeId, rect: Rect, ui: &mut MaraUi<'_>, graph: &mut Graph<T>) {
        let _ = (id, rect, ui, graph);
    }

    /// Draw the value editor for an unconnected input, in the free half
    /// of its pin row.
    ///
    /// A disconnected input with no default to set is the commonest
    /// dead end in a node editor: the node wants a number and there is
    /// nowhere to type one.
    fn input_editor(
        &mut self,
        pin: InPinId,
        rect: Rect,
        ui: &mut MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        let _ = (pin, rect, ui, graph);
    }
}

/// What happened in the graph this frame.
#[derive(Clone, Debug, Default)]
pub struct GraphResponse {
    /// Wires made this frame, already applied to the graph.
    pub connected: Vec<(OutPinId, InPinId)>,
    /// Wires removed this frame, already applied to the graph.
    pub disconnected: Vec<(OutPinId, InPinId)>,
    /// A node was double-clicked — the app's cue to enter a subgraph.
    pub double_clicked: Option<NodeId>,
    /// The canvas was right-clicked, in graph space.
    pub context_menu_at: Option<Pos2>,
    /// A node was right-clicked.
    pub node_context_menu: Option<NodeId>,
    /// Whether the pointer is over the viewport.
    pub hovered: bool,
    /// A node or a frame was dragged this frame.
    pub moved: bool,
}

/// One laid-out node, kept for the frame so hit-testing and painting
/// agree by construction.
struct Placed {
    id: NodeId,
    layout: NodeLayout,
    shape: NodeShape,
    tint: Option<Color32>,
}

/// Draw and drive a graph inside `area`.
pub fn show_graph<T, V: GraphView<T>>(
    ui: &mut MaraUi<'_>,
    area: Rect,
    graph: &mut Graph<T>,
    view: &mut V,
    state: &mut GraphViewState,
    spec: &GraphSpec,
) -> GraphResponse {
    let base_spec = *spec;
    let response = ui.interact(area, canvas_id(ui, area), Sense::ClickAndDrag);
    let mut out = GraphResponse {
        hovered: response.hovered,
        ..Default::default()
    };

    if !state.framed {
        state.framed = true;
        if let Some(b) = graph_bounds(graph, view, spec) {
            state.camera.frame_initial(b, area, 48.0);
        }
    }

    let input = ui.input();
    let pointer = input.pointer.filter(|p| area.contains(*p));

    // ── camera ──────────────────────────────────────────────────
    if let Some(p) = pointer {
        if input.zoom_delta != 1.0 {
            state.camera.zoom_about(input.zoom_delta, p);
        } else if input.scroll_delta.y != 0.0 && !input.modifiers_shift {
            state
                .camera
                .zoom_about((input.scroll_delta.y * 0.0015).exp(), p);
        }
    }
    if input.modifiers_shift {
        state.camera.pan += input.scroll_delta;
    }

    // ── layout ──────────────────────────────────────────────────
    // One spec for the whole frame. Layout and paint both read it, so
    // there is no way for them to disagree about a size.
    let zoomed = spec.scaled(state.camera.zoom);
    let spec = &zoomed;
    let node_spec = spec.node;
    let sited: Vec<(NodeId, Pos2)> = graph.nodes_pos_ids().map(|(id, pos, _)| (id, pos)).collect();
    let mut placed: Vec<Placed> = sited
        .into_iter()
        .map(|(id, pos)| {
            let shape = view.shape(id, graph);
            let tint = view.tint(id, graph);
            let layout = layout_node(state.camera.to_screen(pos), &shape, &node_spec);
            Placed {
                id,
                layout,
                shape,
                tint,
            }
        })
        .collect();
    // Selected nodes paint last, and are therefore hit-tested first.
    placed.sort_by_key(|p| u8::from(state.selection.contains(&p.id)));

    // Frames are laid out from the same node rects the nodes use, so an
    // auto-fitting box never disagrees with what it encloses.
    let shapes: Vec<(NodeId, Rect)> = placed
        .iter()
        .map(|pl| {
            let g_min = state.camera.to_graph(pl.layout.rect.min);
            let g_max = state.camera.to_graph(pl.layout.rect.max);
            (pl.id, Rect::from_two_pos(g_min, g_max))
        })
        .collect();
    let node_rect = move |id: NodeId| {
        shapes
            .iter()
            .find(|(n, _)| *n == id)
            .map_or_else(|| Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(0.0, 0.0)), |(_, r)| *r)
    };
    let frames = place_frames(graph, state.camera, &base_spec, &node_rect);

    let hit = pointer.and_then(|p| hit_test(&placed, p, &node_spec));
    let band_hit = pointer.and_then(|p| band_at(&frames, p));

    // ── gestures ────────────────────────────────────────────────
    let ctx_pointer = response.interact_pointer.or(pointer);
    if response.drag_started() {
        // Middle-drag always pans, whatever is under it. Every node
        // editor works this way, and it is the only pan gesture that
        // does not have to compete with selection.
        state.gesture = if response.dragged_by(PointerButton::Middle)
            || response.dragged_by(PointerButton::Secondary)
        {
            Some(Gesture::Pan)
        } else {
            ctx_pointer.and_then(|p| {
                // A frame's title band wins over everything, because it
                // is the only handle the box has.
                if hit.is_none()
                    && let Some(frame) = band_at(&frames, p)
                {
                    return Some(Gesture::MoveFrame {
                        frame,
                        last: state.camera.to_graph(p),
                    });
                }
                begin_gesture(
                    p,
                    &placed,
                    graph,
                    state,
                    &node_spec,
                    input.modifiers_shift,
                )
            })
        };
    }

    if response.dragged() {
        if let (Some(g), Some(p)) = (state.gesture, ctx_pointer) {
            match g {
                Gesture::Pan => state.camera.pan += input.pointer_delta,
                Gesture::MoveNodes { last } => {
                    let now = state.camera.to_graph(p);
                    let delta = Vec2::new(now.x - last.x, now.y - last.y);
                    for id in &state.selection {
                        if let Some(info) = graph.get_node_info_mut(*id) {
                            info.pos += delta;
                        }
                    }
                    state.gesture = Some(Gesture::MoveNodes { last: now });
                    out.moved = true;
                    // The layout above is now stale by one frame's drag;
                    // shifting it keeps the node under the pointer.
                    for pl in &mut placed {
                        if state.selection.contains(&pl.id) {
                            shift_layout(&mut pl.layout, input.pointer_delta);
                        }
                    }
                }
                Gesture::MoveFrame { frame, last } => {
                    let now = state.camera.to_graph(p);
                    move_frame(graph, frame, Vec2::new(now.x - last.x, now.y - last.y));
                    state.gesture = Some(Gesture::MoveFrame { frame, last: now });
                    out.moved = true;
                }
                _ => {}
            }
        }
    }

    if response.drag_stopped() {
        if let (Some(g), Some(p)) = (state.gesture.take(), ctx_pointer) {
            finish_gesture(g, p, &placed, graph, view, state, &node_spec, &mut out);
        }
    }

    if response.clicked() {
        match hit {
            Some((id, _)) => {
                if input.modifiers_shift || input.modifiers_command {
                    if !state.selection.insert(id) {
                        state.selection.remove(&id);
                    }
                } else {
                    state.selection.clear();
                    state.selection.insert(id);
                }
            }
            None => state.selection.clear(),
        }
    }
    if response.double_clicked() {
        out.double_clicked = hit.map(|(id, _)| id);
    }
    if response.secondary_clicked() {
        match hit {
            Some((id, _)) => out.node_context_menu = Some(id),
            None => out.context_menu_at = ctx_pointer.map(|p| state.camera.to_graph(p)),
        }
    }

    if hit.is_some() {
        ui.set_cursor_icon(CursorIcon::PointingHand);
    }

    // ── keyboard ────────────────────────────────────────────────
    if response.hovered || !state.selection.is_empty() {
        if input.key_pressed(MaraKey::Delete) || input.key_pressed(MaraKey::Backspace) {
            for id in std::mem::take(&mut state.selection) {
                if graph.contains(id) {
                    for (from, to) in graph.wires_of(id).collect::<Vec<_>>() {
                        out.disconnected.push((from, to));
                    }
                    graph.remove_node(id);
                }
            }
        }
        if input.modifiers_command && input.key_pressed(MaraKey::A) {
            state.selection = graph.node_ids().map(|(id, _)| id).collect();
        }
        if input.key_pressed(MaraKey::Escape) {
            state.selection.clear();
        }
        // Frame everything, the universal "I am lost" key in every
        // node editor and 3D viewport.
        if input.key_pressed(MaraKey::F) && !input.modifiers_command {
            if let Some(b) = graph_bounds(graph, view, &base_spec) {
                state.camera.frame_initial(b, area, 48.0);
            }
        }
    }

    // ── paint ───────────────────────────────────────────────────
    let (p, _) = ui.canvas_at(area);
    let origin = state.camera.to_screen(Pos2::new(0.0, 0.0));
    paint_canvas(&p, area, origin, spec.grid_spacing.unwrap_or(0.0), spec);

    let anchors = |placed: &Vec<Placed>, out_pin: OutPinId, in_pin: InPinId| {
        let a = placed
            .iter()
            .find(|pl| pl.id == out_pin.node)
            .and_then(|pl| pl.layout.outputs.get(out_pin.output).copied());
        let b = placed
            .iter()
            .find(|pl| pl.id == in_pin.node)
            .and_then(|pl| pl.layout.inputs.get(in_pin.input).copied());
        a.zip(b)
    };

    // Frames sit under the wires, which is what makes a group read as
    // the surface the graph is drawn on rather than as a pane over it.
    for pf in &frames {
        if let Some(f) = graph.frame(pf.id) {
            paint_frame(&p, pf, f, band_hit == Some(pf.id), state.camera.zoom, spec);
        }
    }

    let wire_w = spec.wire_width;
    for (from, to) in graph.wires() {
        if let Some((a, b)) = anchors(&placed, from, to) {
            let col = view
                .output_color(from, graph)
                .or_else(|| view.input_color(to, graph))
                .unwrap_or(spec.palette.wire);
            paint_wire(&p, a, b, col, wire_w, spec);
        }
    }

    for pl in &placed {
        let st = NodeState {
            selected: state.selection.contains(&pl.id),
            hovered: hit.is_some_and(|(id, _)| id == pl.id),
        };
        paint_node(&p, &pl.layout, &pl.shape, pl.tint, st, spec);

        for (i, at) in pl.layout.inputs.iter().enumerate() {
            let pin = InPinId {
                node: pl.id,
                input: i,
            };
            let col = view.input_color(pin, graph).unwrap_or(spec.palette.wire);
            paint_pin(
                &p,
                *at,
                col,
                !graph.in_pin(pin).remotes.is_empty(),
                &pl.layout.spec,
                &spec.palette,
            );
        }
        for (i, at) in pl.layout.outputs.iter().enumerate() {
            let pin = OutPinId {
                node: pl.id,
                output: i,
            };
            let col = view.output_color(pin, graph).unwrap_or(spec.palette.wire);
            paint_pin(
                &p,
                *at,
                col,
                !graph.out_pin(pin).remotes.is_empty(),
                &pl.layout.spec,
                &spec.palette,
            );
        }
    }

    // ── gesture overlay ─────────────────────────────────────────
    if let (Some(g), Some(cursor)) = (state.gesture, ctx_pointer) {
        match g {
            Gesture::WireFromOutput(pin) => {
                if let Some(a) = pin_anchor_out(&placed, pin) {
                    paint_wire(&p, a, cursor, spec.palette.selection, wire_w, spec);
                }
            }
            Gesture::WireFromInput(pin) => {
                if let Some(b) = pin_anchor_in(&placed, pin) {
                    paint_wire(&p, cursor, b, spec.palette.selection, wire_w, spec);
                }
            }
            Gesture::BoxSelect { from, .. } => {
                let r = Rect::from_two_pos(from, cursor);
                p.rect_filled(r, CornerRadius::same(2), spec.palette.selection_fill);
                p.rect_stroke(
                    r,
                    CornerRadius::same(2),
                    Stroke::new(1.0, spec.palette.selection),
                );
            }
            _ => {}
        }
    }

    // Bodies and inline editors draw last and through the ui, because
    // they are real widgets: they take input, and they must land on top
    // of the node they belong to.
    for pl in &placed {
        for (i, r) in pl.layout.input_editors.iter().enumerate() {
            let pin = InPinId {
                node: pl.id,
                input: i,
            };
            if r.width() > 8.0 && graph.in_pin(pin).remotes.is_empty() {
                let r = *r;
                ui.clipped(r, |ui| {
                    ui.in_region(ChildRegion::top_down(r, StackAlign::Min), &mut |inner| {
                        view.input_editor(pin, r, inner, graph);
                    });
                });
            }
        }
        // Clipped to the rect the layout assigned. An app's body widget
        // has no idea how big the node is, and an unclipped one paints
        // its plot straight out through the node's edge and across the
        // canvas — which is exactly what happened.
        if pl.layout.body.height() > 0.0 {
            let body = pl.layout.body;
            ui.clipped(body, |ui| {
                ui.in_region(ChildRegion::top_down(body, StackAlign::Min), &mut |inner| {
                    view.body(pl.id, body, inner, graph);
                });
            });
        }
    }

    out
}

/// The graph-space bounds of every node, for framing.
fn graph_bounds<T, V: GraphView<T>>(
    graph: &Graph<T>,
    view: &mut V,
    spec: &GraphSpec,
) -> Option<Rect> {
    let mut bounds: Option<Rect> = None;
    let sited: Vec<(NodeId, Pos2)> = graph.nodes_pos_ids().map(|(id, pos, _)| (id, pos)).collect();
    for (id, pos) in sited {
        let shape = view.shape(id, graph);
        let r = layout_node(pos, &shape, &spec.node).rect;
        bounds = Some(match bounds {
            Some(b) => b.union(r),
            None => r,
        });
    }
    bounds
}

fn shift_layout(l: &mut NodeLayout, d: Vec2) {
    let mv = |r: &mut Rect| *r = Rect::from_min_size(r.min + d, r.size());
    mv(&mut l.rect);
    mv(&mut l.header);
    mv(&mut l.content);
    mv(&mut l.body);
    for r in l.input_labels.iter_mut().chain(l.output_labels.iter_mut()) {
        mv(r);
    }
    for pt in l.inputs.iter_mut().chain(l.outputs.iter_mut()) {
        *pt += d;
    }
}

/// Which node is under `p`, and whether the hit was on one of its pins.
fn hit_test(
    placed: &[Placed],
    p: Pos2,
    spec: &super::spec::NodeSpec,
) -> Option<(NodeId, Option<PinHit>)> {
    let grab = (spec.pin_r + spec.pin_ring + 3.0).max(6.0);
    for pl in placed.iter().rev() {
        for (i, at) in pl.layout.inputs.iter().enumerate() {
            if near(*at, p, grab) {
                return Some((pl.id, Some(PinHit::In(i))));
            }
        }
        for (i, at) in pl.layout.outputs.iter().enumerate() {
            if near(*at, p, grab) {
                return Some((pl.id, Some(PinHit::Out(i))));
            }
        }
        if pl.layout.rect.contains(p) {
            return Some((pl.id, None));
        }
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PinHit {
    In(usize),
    Out(usize),
}

fn near(a: Pos2, b: Pos2, r: f32) -> bool {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy <= r * r
}

fn pin_anchor_out(placed: &[Placed], pin: OutPinId) -> Option<Pos2> {
    placed
        .iter()
        .find(|pl| pl.id == pin.node)
        .and_then(|pl| pl.layout.outputs.get(pin.output).copied())
}

fn pin_anchor_in(placed: &[Placed], pin: InPinId) -> Option<Pos2> {
    placed
        .iter()
        .find(|pl| pl.id == pin.node)
        .and_then(|pl| pl.layout.inputs.get(pin.input).copied())
}

fn begin_gesture<T>(
    p: Pos2,
    placed: &[Placed],
    graph: &mut Graph<T>,
    state: &mut GraphViewState,
    spec: &super::spec::NodeSpec,
    additive: bool,
) -> Option<Gesture> {
    match hit_test(placed, p, spec) {
        Some((id, Some(PinHit::Out(i)))) => Some(Gesture::WireFromOutput(OutPinId {
            node: id,
            output: i,
        })),
        Some((id, Some(PinHit::In(i)))) => {
            // Dragging a connected input picks the existing wire up by
            // its far end, the gesture every node editor has taught
            // people to expect. The wire is detached now, so letting go
            // over empty canvas deletes it.
            let pin = InPinId { node: id, input: i };
            match graph.in_pin(pin).remotes.first().copied() {
                Some(src) => {
                    graph.disconnect(src, pin);
                    Some(Gesture::WireFromOutput(src))
                }
                None => Some(Gesture::WireFromInput(pin)),
            }
        }
        Some((id, None)) => {
            if additive {
                state.selection.insert(id);
            } else if !state.selection.contains(&id) {
                state.selection.clear();
                state.selection.insert(id);
            }
            Some(Gesture::MoveNodes {
                last: state.camera.to_graph(p),
            })
        }
        // Empty canvas. Dragging it moves the canvas — the gesture
        // everyone reaches for first, and the one this renderer shipped
        // without. Rubber-band selection moves to Shift, where it does
        // not have to compete.
        None => {
            if additive {
                Some(Gesture::BoxSelect {
                    from: p,
                    additive: true,
                })
            } else {
                Some(Gesture::Pan)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn finish_gesture<T, V: GraphView<T>>(
    g: Gesture,
    p: Pos2,
    placed: &[Placed],
    graph: &mut Graph<T>,
    view: &mut V,
    state: &mut GraphViewState,
    spec: &super::spec::NodeSpec,
    out: &mut GraphResponse,
) {
    match g {
        Gesture::WireFromOutput(from) => {
            if let Some((id, Some(PinHit::In(i)))) = hit_test(placed, p, spec) {
                let to = InPinId { node: id, input: i };
                if view.accepts(from, to, graph) && graph.connect(from, to) {
                    out.connected.push((from, to));
                }
            }
        }
        Gesture::WireFromInput(to) => {
            if let Some((id, Some(PinHit::Out(i)))) = hit_test(placed, p, spec) {
                let from = OutPinId {
                    node: id,
                    output: i,
                };
                if view.accepts(from, to, graph) && graph.connect(from, to) {
                    out.connected.push((from, to));
                }
            }
        }
        Gesture::BoxSelect { from, additive } => {
            let r = Rect::from_two_pos(from, p);
            if !additive {
                state.selection.clear();
            }
            for pl in placed {
                if r.intersects(pl.layout.rect) {
                    state.selection.insert(pl.id);
                }
            }
        }
        Gesture::MoveNodes { .. } | Gesture::MoveFrame { .. } | Gesture::Pan => {}
    }
}

/// A stable id for the viewport, derived from the enclosing ui.
fn canvas_id(ui: &MaraUi<'_>, area: Rect) -> mara_core::vocab::Id {
    let _ = area;
    ui.id().with("mara_graph_view")
}

/// Distance from `p` to the wire between two anchors.
///
/// Used by callers that want click-to-cut; measured against the same
/// sampled curve the painter draws.
#[must_use]
pub fn distance_to_wire(from: Pos2, to: Pos2, slack: f32, p: Pos2) -> f32 {
    wire_points(from, to, slack)
        .windows(2)
        .map(|s| point_segment_distance(p, s[0], s[1]))
        .fold(f32::INFINITY, f32::min)
}

fn point_segment_distance(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let (abx, aby) = (b.x - a.x, b.y - a.y);
    let len2 = abx * abx + aby * aby;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.x - a.x) * abx + (p.y - a.y) * aby) / len2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (a.x + abx * t - p.x, a.y + aby * t - p.y);
    (dx * dx + dy * dy).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Screen and graph space must be exact inverses, or a node drifts
    /// out from under the pointer a little on every drag frame.
    #[test]
    fn the_camera_round_trips() {
        let c = Camera {
            pan: Vec2::new(31.0, -17.0),
            zoom: 1.75,
        };
        let g = Pos2::new(120.0, -40.0);
        let back = c.to_graph(c.to_screen(g));
        assert!((back.x - g.x).abs() < 0.001 && (back.y - g.y).abs() < 0.001);
    }

    /// Zooming holds the anchor point still. Without this the graph
    /// slides away from the cursor as you scroll.
    #[test]
    fn zooming_holds_the_point_under_the_cursor() {
        let mut c = Camera::default();
        let anchor = Pos2::new(400.0, 250.0);
        let before = c.to_graph(anchor);
        c.zoom_about(1.4, anchor);
        c.zoom_about(1.4, anchor);
        let after = c.to_graph(anchor);
        assert!((before.x - after.x).abs() < 0.01);
        assert!((before.y - after.y).abs() < 0.01);
    }

    /// Zoom stays inside its range however hard it is pushed, so a
    /// runaway wheel event cannot leave the view unrecoverable.
    #[test]
    fn zoom_is_bounded() {
        let (lo, hi) = Camera::ZOOM_RANGE;
        let mut c = Camera::default();
        for _ in 0..50 {
            c.zoom_about(2.0, Pos2::new(0.0, 0.0));
        }
        assert!((c.zoom - hi).abs() < 0.001);
        for _ in 0..80 {
            c.zoom_about(0.5, Pos2::new(0.0, 0.0));
        }
        assert!((c.zoom - lo).abs() < 0.001);
    }

    /// Framing puts the content's centre at the viewport's centre and
    /// leaves it inside the margin.
    #[test]
    fn fitting_centres_the_content() {
        let mut c = Camera::default();
        let bounds = Rect::from_min_size(Pos2::new(-200.0, 50.0), Vec2::new(400.0, 200.0));
        let area = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));
        c.fit(bounds, area, 40.0);
        let centre = c.to_screen(bounds.center());
        assert!((centre.x - area.center().x).abs() < 0.01);
        assert!((centre.y - area.center().y).abs() < 0.01);
        assert!(c.to_screen(bounds.min).x >= area.min.x + 39.0);
    }

    /// Framing a small graph must not magnify it. A two-node graph
    /// blown up to fill the viewport reads as broken.
    #[test]
    fn fitting_never_zooms_past_one_to_one() {
        let mut c = Camera::default();
        let tiny = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(40.0, 30.0));
        let area = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1600.0, 1000.0));
        c.fit(tiny, area, 40.0);
        assert!((c.zoom - 1.0).abs() < 0.001, "zoom went to {}", c.zoom);
    }

    /// Opening a sprawling graph must not drop the user onto a field of
    /// unreadable slabs. Initial framing stops at the legibility floor
    /// and centres instead of fitting everything.
    #[test]
    fn initial_framing_stops_at_the_legibility_floor() {
        let mut c = Camera::default();
        let sprawl = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(4000.0, 9000.0));
        let area = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(1600.0, 1000.0));
        c.frame_initial(sprawl, area, 48.0);
        assert!((c.zoom - Camera::LEGIBLE_ZOOM).abs() < 0.001);
        let centre = c.to_screen(sprawl.center());
        assert!((centre.x - area.center().x).abs() < 0.01);
        assert!((centre.y - area.center().y).abs() < 0.01);
    }

    /// A click within the grab radius of a pin must resolve to that
    /// pin, not to the node body underneath it.
    #[test]
    fn a_pin_wins_the_hit_test_against_its_own_body() {
        let spec = super::super::spec::NodeSpec::default();
        let shape = NodeShape {
            title: "n".into(),
            inputs: vec!["a".into()],
            outputs: vec!["b".into()],
            body_h: 0.0,
        };
        let layout = layout_node(Pos2::new(0.0, 0.0), &shape, &spec);
        let pin = layout.inputs[0];
        let placed = vec![Placed {
            id: NodeId(0),
            layout,
            shape,
            tint: None,
        }];

        assert_eq!(
            hit_test(&placed, Pos2::new(pin.x + 2.0, pin.y), &spec),
            Some((NodeId(0), Some(PinHit::In(0))))
        );
        assert_eq!(
            hit_test(&placed, Pos2::new(pin.x + 60.0, pin.y), &spec),
            Some((NodeId(0), None))
        );
    }

    /// Distance to a wire is measured against the drawn curve, so a
    /// click on the visible bow of a backwards wire registers even
    /// though it is far from the straight line between its ends.
    #[test]
    fn wire_distance_follows_the_drawn_curve() {
        let from = Pos2::new(0.0, 0.0);
        let to = Pos2::new(200.0, 0.0);
        let on_curve = wire_points(from, to, 0.5)[12];
        assert!(distance_to_wire(from, to, 0.5, on_curve) < 0.5);
        assert!(distance_to_wire(from, to, 0.5, Pos2::new(100.0, 90.0)) > 40.0);
    }
}
