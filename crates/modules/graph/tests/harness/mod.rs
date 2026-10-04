//! Headless input driver for the graph widget — PLAN_NODE.md P3b.
//!
//! # Why this exists
//!
//! `render_characterisation.rs` builds a `RawInput` carrying only
//! `screen_rect`. That is enough to assert on what gets *painted*, and
//! nothing else: with no events, no pointer and no time base, the
//! widget never sees a click, a drag or a key. Every behaviour the
//! frame and subgraph phases add is an interaction, and the app may not
//! be launched to check them by hand — so without this, those phases
//! would ship on the strength of the compiler agreeing that the code
//! typechecks.
//!
//! # What it does
//!
//! Drives a real `egui::Context` over a scripted sequence of frames,
//! synthesising the `RawInput.events` a windowing system would deliver:
//! pointer motion, button press/release with correct `time` deltas so
//! egui's double-click detector fires, and key presses with modifiers.
//!
//! Two details that are easy to get wrong and expensive to debug:
//!
//! * **egui resolves sizes from the previous frame.** A single pass
//!   reports a half-laid-out widget, so every interaction needs at
//!   least one warm-up frame before the frame that acts. [`Harness`]
//!   runs the warm-up automatically.
//! * **`time` must advance.** Double-click detection is a time
//!   threshold, not an event count. Frames advance a fixed 16 ms unless
//!   a step asks otherwise, and [`Harness::double_click`] deliberately
//!   places its two presses inside that window.

#![allow(dead_code)]
// `CentralPanel::show` is deprecated in favour of `show_inside`, but the
// existing render-characterisation fixture uses it and the two must
// drive identical passes to stay comparable.
#![allow(deprecated)]

use mara_core::MaraUi;
use mara_graph::{Graph, GraphStyle, GraphWidget, NodeViewer};

/// Seconds advanced per synthesised frame. Comfortably under egui's
/// double-click threshold, so two clicks in consecutive frames read as
/// a double click and two clicks several frames apart do not.
const FRAME_DT: f64 = 0.016;

/// A scripted headless session over one graph widget.
pub struct Harness {
    ctx: egui::Context,
    time: f64,
    screen: egui::Rect,
    pending: Vec<egui::Event>,
    pointer: Option<egui::Pos2>,
    /// Modifiers held for the whole frame.
    ///
    /// Separate from the `modifiers` carried on a `PointerButton`
    /// event, and the one that actually matters: egui's `InputState`
    /// tracks modifiers from `RawInput.modifiers`, not from event
    /// payloads, and the widget reads them from there once per frame.
    /// Setting them only on the event delivers a click with no shift.
    held: egui::Modifiers,
    style: GraphStyle,
    /// Explicit widget id, so a test can read the graph's saved state
    /// back out of Mara memory without reconstructing the salt the
    /// widget would otherwise derive from its host `Ui`.
    id: mara_core::vocab::Id,
    /// Set once the first frame has run, so callers cannot accidentally
    /// assert against first-frame layout guesswork.
    warmed: bool,
}

impl Default for Harness {
    fn default() -> Self {
        Self::new()
    }
}

impl Harness {
    #[must_use]
    pub fn new() -> Self {
        Self {
            ctx: egui::Context::default(),
            time: 0.0,
            screen: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1024.0, 768.0)),
            pending: Vec::new(),
            pointer: None,
            held: egui::Modifiers::default(),
            style: GraphStyle::new(),
            id: mara_core::vocab::Id::new("mara_graph.harness"),
            warmed: false,
        }
    }

    /// The id the widget renders under — pass this to
    /// `GraphState::selection` and friends.
    #[must_use]
    pub fn graph_id(&self) -> mara_core::vocab::Id {
        self.id
    }

    #[must_use]
    pub fn with_style(mut self, style: GraphStyle) -> Self {
        self.style = style;
        self
    }

    #[must_use]
    pub fn with_screen(mut self, w: f32, h: f32) -> Self {
        self.screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h));
        self
    }

    /// The context, for reading back stored state after a frame.
    #[must_use]
    pub fn ctx(&self) -> &egui::Context {
        &self.ctx
    }

    /// A Mara context over this harness's egui context, for reading the
    /// graph's saved state out of Mara memory.
    #[must_use]
    pub fn seam(&self) -> mara_backend_egui::EguiCtx {
        mara_backend_egui::EguiCtx::new(&self.ctx)
    }

    /// Map a point in **graph space** to the screen point that lands on
    /// it.
    ///
    /// Necessary because node positions are graph coordinates while
    /// synthesised pointer events are screen coordinates, and the two
    /// are not the same: the widget fits the content to the viewport on
    /// its first frame, so a node at graph `(300, 200)` is generally
    /// nowhere near screen `(300, 200)`.
    ///
    /// Must be called *after* at least one frame, so the viewport
    /// transform has been saved. The `ui_rect` passed to `load` is only
    /// consulted when there is no saved state, which is exactly the
    /// case this rules out.
    pub fn graph_to_screen<T>(&self, graph: &Graph<T>, p: mara_core::vocab::Pos2) -> egui::Pos2 {
        assert!(
            self.warmed,
            "graph_to_screen before the first frame: the viewport transform is not saved yet"
        );
        let seam = self.seam();
        let state = mara_graph::GraphState::load(
            &seam,
            // Same conversion the widget performs on `GraphWidget::id`
            // — the vocab/egui id conversions are not inverses, so any
            // other route reads a key nothing wrote.
            egui::Id::from(self.id),
            graph,
            self.screen,
            0.0,
            f32::INFINITY,
        );
        let mapped = state.to_global().mul_pos(p);
        egui::pos2(mapped.x, mapped.y)
    }

    /// Where a node currently is on screen — its top-left corner.
    pub fn node_screen_pos<T>(&self, graph: &Graph<T>, node: mara_graph::NodeId) -> egui::Pos2 {
        let pos = graph
            .get_node_info(node)
            .expect("node must exist to be located")
            .pos;
        self.graph_to_screen(graph, pos)
    }

    // ── Queuing input ───────────────────────────────────────────────

    /// Move the pointer to `pos`, in screen points.
    pub fn move_to(&mut self, pos: egui::Pos2) -> &mut Self {
        self.pointer = Some(pos);
        self.pending.push(egui::Event::PointerMoved(pos));
        self
    }

    /// Press the primary button wherever the pointer currently is.
    pub fn press(&mut self) -> &mut Self {
        self.button(egui::PointerButton::Primary, true)
    }

    /// Release the primary button.
    pub fn release(&mut self) -> &mut Self {
        self.button(egui::PointerButton::Primary, false)
    }

    pub fn button(&mut self, button: egui::PointerButton, pressed: bool) -> &mut Self {
        let pos = self.pointer.unwrap_or(egui::Pos2::ZERO);
        // Re-assert the position immediately before the button event.
        // A real windowing system delivers motion continuously, so the
        // pointer is always "fresh" when a button event arrives; a
        // harness that sends a position only when it changes leaves
        // egui believing the pointer left between the press frame and
        // the release frame, and the pair never resolves into a click.
        // Attached to the button event rather than to every frame,
        // which is what makes hover stable instead of flickering.
        self.pending.push(egui::Event::PointerMoved(pos));
        self.pending.push(egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: self.held,
        });
        self
    }

    /// Hold `modifiers` for every subsequent frame until
    /// [`Harness::unhold`].
    pub fn hold(&mut self, modifiers: egui::Modifiers) -> &mut Self {
        self.held = modifiers;
        self
    }

    /// Stop holding modifiers.
    pub fn unhold(&mut self) -> &mut Self {
        self.held = egui::Modifiers::default();
        self
    }

    /// Press the primary button with `modifiers` held. Selection in
    /// this widget is modifier-driven — a plain click deliberately does
    /// not change it — so a test that wants a node selected needs this
    /// rather than [`Harness::press`].
    pub fn press_with(&mut self, modifiers: egui::Modifiers) -> &mut Self {
        self.held = modifiers;
        self.press()
    }

    /// A full click at `pos` with `modifiers` held throughout.
    ///
    /// Held across both the press frame and the release frame because
    /// the widget resolves selection on `clicked_by`, which fires on
    /// release — reading the modifiers *that* frame, not the press one.
    pub fn modified_click_at<T, V: NodeViewer<T>>(
        &mut self,
        pos: egui::Pos2,
        modifiers: egui::Modifiers,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) {
        self.hold(modifiers);
        self.move_to(pos);
        self.frame(graph, viewer);
        self.press();
        self.frame(graph, viewer);
        self.release();
        self.frame(graph, viewer);
        self.unhold();
    }

    pub fn key(&mut self, key: egui::Key, modifiers: egui::Modifiers) -> &mut Self {
        self.pending.push(egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        });
        self
    }

    // ── Composite gestures ──────────────────────────────────────────

    /// A press-and-release at `pos`, delivered across two frames.
    ///
    /// Split across frames rather than queued into one because egui
    /// resolves a click from *state transitions between* frames; both
    /// events in a single frame can collapse into no click at all.
    pub fn click_at<T, V: NodeViewer<T>>(
        &mut self,
        pos: egui::Pos2,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) {
        // The move gets its own frame before the press. egui resolves a
        // widget's interaction from the pointer position it *had* when
        // the widget was registered, so a move and a press delivered in
        // the same frame can land on nothing at all.
        self.move_to(pos);
        self.frame(graph, viewer);
        self.press();
        self.release();
        self.frame(graph, viewer);
    }

    /// Two clicks close enough together in time to read as a double
    /// click. The whole reason `time` advances by a fixed small step.
    pub fn double_click_at<T, V: NodeViewer<T>>(
        &mut self,
        pos: egui::Pos2,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) {
        self.click_at(pos, graph, viewer);
        self.click_at(pos, graph, viewer);
    }

    /// Press at `from`, move to `to` in `steps` increments, release.
    ///
    /// Stepped rather than teleported because a drag is reported as a
    /// per-frame *delta*: a single jump produces one enormous delta,
    /// which is exactly the case real code never sees and therefore the
    /// case a test should not pin.
    pub fn drag<T, V: NodeViewer<T>>(
        &mut self,
        from: egui::Pos2,
        to: egui::Pos2,
        steps: usize,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) {
        let steps = steps.max(1);
        self.move_to(from);
        self.frame(graph, viewer);
        self.press();
        self.frame(graph, viewer);

        let delta = (to - from) / steps as f32;
        for i in 1..=steps {
            self.move_to(from + delta * i as f32);
            self.frame(graph, viewer);
        }

        self.release();
        self.frame(graph, viewer);
    }

    // ── Running frames ──────────────────────────────────────────────

    /// Run one frame with the queued input, rendering `graph`.
    ///
    /// Returns the tessellated summary so a caller can assert on what
    /// was painted as well as on model state.
    pub fn frame<T, V: NodeViewer<T>>(
        &mut self,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) -> FrameSummary {
        if !self.warmed {
            self.warmed = true;
            // A warm-up pass with no events, so the widget has real
            // sizes before the first interaction lands.
            let saved = std::mem::take(&mut self.pending);
            self.run_once(graph, viewer);
            self.pending = saved;
        }
        self.run_once(graph, viewer)
    }

    /// Run one warm-up frame plus one measured frame, consuming the
    /// harness. For one-shot "what does this graph paint" assertions,
    /// where keeping the session around would only invite reading
    /// first-frame layout.
    pub fn frame_owned<T, V: NodeViewer<T>>(
        mut self,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) -> FrameSummary {
        self.frame(graph, viewer);
        self.frame(graph, viewer)
    }

    /// Run `n` frames with no input — for settling animations.
    pub fn idle<T, V: NodeViewer<T>>(&mut self, n: usize, graph: &mut Graph<T>, viewer: &mut V) {
        for _ in 0..n {
            self.pending.clear();
            self.frame(graph, viewer);
        }
    }

    fn run_once<T, V: NodeViewer<T>>(
        &mut self,
        graph: &mut Graph<T>,
        viewer: &mut V,
    ) -> FrameSummary {
        let events = std::mem::take(&mut self.pending);

        self.time += FRAME_DT;

        self.ctx.begin_pass(egui::RawInput {
            screen_rect: Some(self.screen),
            time: Some(self.time),
            modifiers: self.held,
            events,
            ..Default::default()
        });

        let style = self.style;
        let id = self.id;
        egui::CentralPanel::default().show(&self.ctx, |ui| {
            let mut backend = mara_backend_egui::EguiUiBackend::new(ui);
            MaraUi::__internal_over_backend_ret(
                &mut backend,
                mara_core::style::active_accent(),
                |mara| {
                    let _ = GraphWidget::new()
                        .id(id)
                        .style(style)
                        .show(graph, viewer, mara);
                },
            );
        });

        let output = self.ctx.end_pass();
        let primitives = self.ctx.tessellate(output.shapes, output.pixels_per_point);

        let mut summary = FrameSummary::default();
        for prim in &primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &prim.primitive {
                if mesh.indices.is_empty() {
                    continue;
                }
                summary.vertex_count += mesh.vertices.len();
                for v in &mesh.vertices {
                    summary.painted = summary
                        .painted
                        .union(egui::Rect::from_min_max(v.pos, v.pos));
                }
            }
        }
        summary
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FrameSummary {
    pub vertex_count: usize,
    pub painted: egui::Rect,
}

impl Default for FrameSummary {
    fn default() -> Self {
        Self {
            vertex_count: 0,
            painted: egui::Rect::NOTHING,
        }
    }
}
