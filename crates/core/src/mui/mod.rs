//! # `mui` — the sealed Mara UI surface
//!
//! [`MaraUi`] is the only thing Mara hands to consumer drawing code
//! (module inline bodies, view bodies, foldable sections). It holds a
//! backend adapter (`backend::egui::EguiUiBackend`) rather than a raw
//! `egui::Ui`, and routes reads/layout/paint through the
//! [`crate::layout::UiBackend`] contract; only operations not yet
//! promoted to that contract reach the concrete `Ui` through the
//! backend's crate-internal `ui()`/`ui_mut()` seam. It exposes exactly
//! Mara's widget set plus a typed canvas/painter, so an app can
//! compose UI without ever holding a raw `egui::Ui`.
//!
//! Capability rules:
//!
//! * widget methods return [`MaraResponse`] — a plain data snapshot,
//!   not `egui::Response` (whose public `ctx` field would leak the
//!   whole `egui::Context`).
//! * custom drawing goes through [`MaraPainter`], which only speaks
//!   [`crate::vocab`] data types and the theme's fonts.
//! * input is read through [`MaraInput`], a per-frame snapshot.
//! * raw backend handles stay behind crate-internal first-party
//!   adapter hooks, not public app APIs.

use std::{cell::RefCell, ops::RangeInclusive, rc::Rc};

use crate::backend;
use crate::layout::{
    CanvasRectSpec, CanvasSlotSpec, PaintSurfaceSpec, Sense as MaraSense, SpaceSpec,
    StackScopeSpec, UiBackend,
};
use crate::memory::MaraMemoryCtx;
use crate::paint::{PaintCmd, PaintList};
use crate::pod::{Pod, PodResponse};
use crate::vocab;
use crate::widget::TreeBody;
use crate::widget::badge::badge_row_backend;
use crate::widget::button::{ActionButtonResponse, button, button_h, card_action_button};
use crate::widget::chip::{chip_colored_backend, chip_fill};
use crate::widget::color::{color_rgb, color_rgba};
use crate::widget::context_menu::context_menu_mara;
use crate::widget::drag_value::drag_value;
use crate::widget::dropdown::dropdown;
use crate::widget::foldable::section;
use crate::widget::keybinding::keybinding_row_backend;
use crate::widget::label::label_backend;
use crate::widget::progressbar::progressbar_backend;
use crate::widget::readout::readout_backend;
use crate::widget::select::{HybridSelectResponse, hybrid_select_row, select_row};
use crate::widget::slider::slider;
use crate::widget::text_input::text_input;
use crate::widget::toggle::{toggle_backend, toggle_track_only_backend};

// ─── MaraResponse ─────────────────────────────────────────────────

/// Plain-data interaction snapshot returned by every [`MaraUi`]
/// widget method.
///
/// Mara cannot return `egui::Response` from sealed surfaces because
/// its public `ctx` field hands out the entire `egui::Context`.
/// This type copies the flags consumer code actually branches on.
/// Backend-specific follow-ups (e.g. [`MaraUi::context_menu`]) use a
/// backend side-table keyed by this snapshot, not a raw backend
/// response stored inside the public response type.
#[derive(Clone, Debug)]
pub struct MaraResponse {
    pub clicked: bool,
    pub double_clicked: bool,
    pub secondary_clicked: bool,
    pub hovered: bool,
    pub changed: bool,
    pub dragged: bool,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub pointer_button_down: bool,
    pub drag_delta: vocab::Vec2,
    /// Pointer position during click/drag interaction with this
    /// widget, if any.
    pub interact_pointer: Option<vocab::Pos2>,
    /// Pointer position while hovering this widget, if any.
    pub hover_pos: Option<vocab::Pos2>,
    pub rect: vocab::Rect,
    backend_response: vocab::Id,
}

impl From<egui::Response> for MaraResponse {
    fn from(inner: egui::Response) -> Self {
        let backend_response = backend::egui::remember_response(&inner);
        Self {
            clicked: inner.clicked(),
            double_clicked: inner.double_clicked(),
            secondary_clicked: inner.secondary_clicked(),
            hovered: inner.hovered(),
            changed: inner.changed(),
            dragged: inner.dragged(),
            drag_started: inner.drag_started(),
            drag_stopped: inner.drag_stopped(),
            pointer_button_down: inner.is_pointer_button_down_on(),
            drag_delta: inner.drag_delta().into(),
            interact_pointer: inner.interact_pointer_pos().map(Into::into),
            hover_pos: inner.hover_pos().map(Into::into),
            rect: inner.rect.into(),
            backend_response,
        }
    }
}

impl MaraResponse {
    /// Inert response at `rect` — used by non-interactive backends
    /// (the recording backend) and tests.
    pub(crate) fn synthetic(rect: vocab::Rect) -> Self {
        Self {
            clicked: false,
            double_clicked: false,
            secondary_clicked: false,
            hovered: false,
            changed: false,
            dragged: false,
            drag_started: false,
            drag_stopped: false,
            pointer_button_down: false,
            drag_delta: vocab::Vec2::ZERO,
            interact_pointer: None,
            hover_pos: None,
            rect,
            backend_response: vocab::Id::new(("mara_response", "synthetic")),
        }
    }

    #[must_use]
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    #[must_use]
    pub fn double_clicked(&self) -> bool {
        self.double_clicked
    }

    #[must_use]
    pub fn secondary_clicked(&self) -> bool {
        self.secondary_clicked
    }

    #[must_use]
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.changed
    }

    #[must_use]
    pub fn dragged(&self) -> bool {
        self.dragged
    }

    #[must_use]
    pub fn drag_started(&self) -> bool {
        self.drag_started
    }

    #[must_use]
    pub fn drag_stopped(&self) -> bool {
        self.drag_stopped
    }

    #[must_use]
    pub fn pointer_button_down(&self) -> bool {
        self.pointer_button_down
    }

    pub(crate) fn backend_response_id(&self) -> vocab::Id {
        self.backend_response
    }
}

// ─── MaraInput ────────────────────────────────────────────────────

/// Per-frame input snapshot for custom (canvas-style) surfaces.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaraInput {
    /// Latest pointer position, if the pointer is over the window.
    pub pointer: Option<vocab::Pos2>,
    /// Current interaction pointer position for active drag/click gestures.
    pub interact_pointer: Option<vocab::Pos2>,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub any_released: bool,
    pub secondary_down: bool,
    pub secondary_pressed: bool,
    /// Smooth scroll delta this frame.
    pub scroll_delta: vocab::Vec2,
    /// Pointer movement since last frame.
    pub pointer_delta: vocab::Vec2,
    /// Pinch/ctrl-scroll zoom factor this frame (1.0 = none).
    pub zoom_delta: f32,
    pub modifiers_shift: bool,
    pub modifiers_ctrl: bool,
    pub modifiers_alt: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaraKey {
    Escape,
    ArrowDown,
    ArrowUp,
    Enter,
}

// ─── MaraPainter ──────────────────────────────────────────────────

#[derive(Clone)]
enum MaraPainterSink {
    Egui(egui::Painter),
    /// Retained sink for tests today and future non-egui backend flushes.
    #[allow(dead_code)]
    Commands {
        commands: Rc<RefCell<PaintList>>,
        clip: vocab::Rect,
    },
}

impl MaraPainterSink {
    fn clip_rect(&self) -> vocab::Rect {
        match self {
            Self::Egui(painter) => backend::egui::painter_clip_rect(painter),
            Self::Commands { clip, .. } => *clip,
        }
    }
}

/// Typed draw surface for custom content (gauges, plots, overlays).
///
/// Sinks Mara [`PaintCmd`] data into the current backend. The egui
/// backend renders commands immediately today; retained/future
/// backends can collect the same command stream without exposing
/// `egui::Painter` as the semantic drawing model. Every public
/// method speaks [`crate::vocab`] data types only. Text always
/// renders in the theme's font families, so custom drawing cannot
/// drift away from Mara's typography.
pub struct MaraPainter {
    sink: MaraPainterSink,
}

impl MaraPainter {
    pub(crate) fn new(painter: egui::Painter) -> Self {
        Self {
            sink: MaraPainterSink::Egui(painter),
        }
    }

    #[cfg(test)]
    pub(crate) fn recording(clip: impl Into<vocab::Rect>) -> Self {
        Self {
            sink: MaraPainterSink::Commands {
                commands: Rc::new(RefCell::new(PaintList::new())),
                clip: clip.into(),
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn recorded_commands(&self) -> Vec<PaintCmd> {
        match &self.sink {
            MaraPainterSink::Egui(_) => Vec::new(),
            MaraPainterSink::Commands { commands, .. } => commands.borrow().commands().to_vec(),
        }
    }

    /// The rect drawing is clipped to.
    #[must_use]
    pub fn clip_rect(&self) -> vocab::Rect {
        self.sink.clip_rect()
    }

    /// A sub-painter hard-clipped to `rect` (intersected with the
    /// current clip, so clips only ever shrink).
    #[must_use]
    pub fn with_clip(&self, rect: impl Into<vocab::Rect>) -> MaraPainter {
        let rect = rect.into();
        match &self.sink {
            MaraPainterSink::Egui(painter) => {
                MaraPainter::new(backend::egui::painter_with_clip(painter, rect))
            }
            MaraPainterSink::Commands { commands, clip } => MaraPainter {
                sink: MaraPainterSink::Commands {
                    commands: Rc::clone(commands),
                    clip: clip.intersect(rect),
                },
            },
        }
    }

    pub fn line_segment(
        &self,
        a: impl Into<vocab::Pos2>,
        b: impl Into<vocab::Pos2>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Line {
            a: a.into(),
            b: b.into(),
            stroke: stroke.into(),
        });
    }

    /// Open polyline through `points`.
    pub fn polyline(&self, points: Vec<vocab::Pos2>, stroke: impl Into<vocab::Stroke>) {
        self.paint_cmd(PaintCmd::Polyline {
            points,
            stroke: stroke.into(),
        });
    }

    /// Closed path with independent fill and stroke.
    pub fn polygon(
        &self,
        points: Vec<vocab::Pos2>,
        fill: impl Into<vocab::Color32>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Polygon {
            points,
            fill: fill.into(),
            stroke: stroke.into(),
        });
    }

    pub fn rect_filled(
        &self,
        rect: impl Into<vocab::Rect>,
        corner: impl Into<vocab::CornerRadius>,
        fill: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::RectFilled {
            rect: rect.into(),
            corner: corner.into(),
            fill: fill.into(),
        });
    }

    pub fn rect_stroke(
        &self,
        rect: impl Into<vocab::Rect>,
        corner: impl Into<vocab::CornerRadius>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::RectStroke {
            rect: rect.into(),
            corner: corner.into(),
            stroke: stroke.into(),
        });
    }

    pub fn circle_filled(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: f32,
        fill: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::CircleFilled {
            center: center.into(),
            radius,
            fill: fill.into(),
        });
    }

    pub fn circle_stroke(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: f32,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::CircleStroke {
            center: center.into(),
            radius,
            stroke: stroke.into(),
        });
    }

    /// Filled axis-aligned ellipse bounded by `rect`.
    pub fn ellipse_filled(&self, rect: impl Into<vocab::Rect>, fill: impl Into<vocab::Color32>) {
        self.paint_cmd(PaintCmd::Ellipse {
            rect: rect.into(),
            fill: fill.into(),
            stroke: vocab::Stroke::NONE,
        });
    }

    /// Outlined axis-aligned ellipse bounded by `rect`.
    pub fn ellipse_stroke(&self, rect: impl Into<vocab::Rect>, stroke: impl Into<vocab::Stroke>) {
        self.paint_cmd(PaintCmd::Ellipse {
            rect: rect.into(),
            fill: vocab::Color32::TRANSPARENT,
            stroke: stroke.into(),
        });
    }

    /// Open (elliptical) arc — a curved line. Angles in radians, `0` at
    /// the +x axis, increasing clockwise (screen y-down).
    pub fn arc(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: impl Into<vocab::Vec2>,
        start_angle: f32,
        end_angle: f32,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Arc {
            center: center.into(),
            radius: radius.into(),
            start_angle,
            end_angle,
            stroke: stroke.into(),
        });
    }

    /// Filled pie sector from `center` across the arc (angles as in
    /// [`MaraPainter::arc`]).
    pub fn sector(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: impl Into<vocab::Vec2>,
        start_angle: f32,
        end_angle: f32,
        fill: impl Into<vocab::Color32>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Sector {
            center: center.into(),
            radius: radius.into(),
            start_angle,
            end_angle,
            fill: fill.into(),
            stroke: stroke.into(),
        });
    }

    pub fn arrow(
        &self,
        origin: impl Into<vocab::Pos2>,
        vec: impl Into<vocab::Vec2>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Arrow {
            origin: origin.into(),
            vec: vec.into(),
            stroke: stroke.into(),
        });
    }

    /// Text in the theme's proportional family. Returns the painted
    /// rect.
    pub fn text(
        &self,
        pos: impl Into<vocab::Pos2>,
        anchor: impl Into<vocab::Align2>,
        text: impl ToString,
        size: f32,
        color: impl Into<vocab::Color32>,
    ) -> vocab::Rect {
        self.paint_text_cmd(PaintCmd::Text {
            pos: pos.into(),
            anchor: anchor.into(),
            text: text.to_string(),
            size,
            color: color.into(),
            mono: false,
        })
    }

    /// Text in the theme's monospace family. Returns the painted
    /// rect.
    pub fn text_mono(
        &self,
        pos: impl Into<vocab::Pos2>,
        anchor: impl Into<vocab::Align2>,
        text: impl ToString,
        size: f32,
        color: impl Into<vocab::Color32>,
    ) -> vocab::Rect {
        self.paint_text_cmd(PaintCmd::Text {
            pos: pos.into(),
            anchor: anchor.into(),
            text: text.to_string(),
            size,
            color: color.into(),
            mono: true,
        })
    }

    /// Textured quad. `uv` is in 0..=1 texture space; pass
    /// `Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))` (or
    /// [`MaraPainter::full_uv`]) for the whole texture.
    pub fn image(
        &self,
        texture: impl Into<vocab::TextureId>,
        rect: impl Into<vocab::Rect>,
        uv: impl Into<vocab::Rect>,
        tint: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::Image {
            texture: texture.into(),
            rect: rect.into(),
            uv: uv.into(),
            tint: tint.into(),
        });
    }

    /// The full 0..=1 UV rect, for [`MaraPainter::image`].
    #[must_use]
    pub fn full_uv() -> vocab::Rect {
        vocab::Rect::from_min_max(vocab::Pos2::ZERO, vocab::Pos2::new(1.0, 1.0))
    }

    /// Render a Mara paint command through the current backend.
    ///
    /// Today this translates immediately to egui. Later this method
    /// becomes the seam where commands are buffered or sent to a
    /// non-egui renderer.
    pub fn paint_cmd(&self, cmd: PaintCmd) {
        match &self.sink {
            MaraPainterSink::Egui(painter) => backend::egui::render_paint_cmd(painter, cmd),
            MaraPainterSink::Commands { commands, clip } => {
                commands.borrow_mut().push(PaintCmd::Clip {
                    rect: *clip,
                    children: vec![cmd],
                });
            }
        }
    }

    fn paint_text_cmd(&self, cmd: PaintCmd) -> vocab::Rect {
        match &self.sink {
            MaraPainterSink::Egui(painter) => backend::egui::render_text_cmd(painter, cmd),
            MaraPainterSink::Commands { .. } => {
                let rect = match &cmd {
                    PaintCmd::Text { pos, .. }
                    | PaintCmd::TextWithFamily { pos, .. }
                    | PaintCmd::TextRuns { pos, .. } => {
                        vocab::Rect::from_min_size(*pos, vocab::Vec2::ZERO)
                    }
                    _ => vocab::Rect::NOTHING,
                };
                self.paint_cmd(cmd);
                rect
            }
        }
    }
}

// ─── MaraUi ───────────────────────────────────────────────────────

/// The sealed widget surface handed to consumer drawing code.
///
/// Carries an ambient accent colour so widget calls stay terse;
/// override it per-scope with [`MaraUi::set_accent`].
pub struct MaraUi<'a> {
    /// Backend adapter that owns the concrete host `Ui`. `MaraUi`
    /// drives it through the [`crate::layout::UiBackend`] contract;
    /// raw-host access stays behind the adapter's `ui()`/`ui_mut()`
    /// seam.
    pub(crate) backend: backend::egui::EguiUiBackend<'a>,
    accent: vocab::Color32,
}

impl<'a> MaraUi<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui, accent: impl Into<vocab::Color32>) -> Self {
        Self {
            backend: backend::egui::EguiUiBackend::new(ui),
            accent: accent.into(),
        }
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable. First-party Mara module crates
    /// (canvas, image, map, …) use this for backend adapter work
    /// while ordinary app code stays on typed Mara APIs.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_raw_ui(&mut self) -> &mut egui::Ui {
        self.backend.ui_mut()
    }

    /// Internal first-party constructor — NOT part of the public
    /// API and not semver-stable. Used by host plugins that own the
    /// egui pass (e.g. `bevy_mara`) to hand sealed surfaces to app
    /// code.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_from_raw(ui: &'a mut egui::Ui, accent: impl Into<vocab::Color32>) -> Self {
        Self::new(ui, accent)
    }

    // ── ambient state ────────────────────────────────────────────

    #[must_use]
    pub fn accent(&self) -> vocab::Color32 {
        self.accent
    }

    pub fn set_accent(&mut self, accent: impl Into<vocab::Color32>) {
        self.accent = accent.into();
    }

    /// Stable id of the underlying scope, for salting widget ids.
    #[must_use]
    pub fn id(&self) -> vocab::Id {
        self.backend.id()
    }

    #[must_use]
    pub fn available_width(&self) -> f32 {
        self.backend.available_width()
    }

    #[must_use]
    pub fn available_height(&self) -> f32 {
        self.backend.available_height()
    }

    #[must_use]
    pub fn available_rect(&self) -> vocab::Rect {
        self.backend.available_rect()
    }

    /// Per-frame input snapshot for custom interaction logic.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        self.backend.input()
    }

    /// Backend-neutral memory facade for persisted or frame-temp UI
    /// state. Uses Mara IDs and does not expose the raw backend
    /// context.
    #[must_use]
    pub fn memory(&self) -> MaraMemoryCtx<'_> {
        backend::egui::memory_ctx_for_ui(self.backend.ui())
    }

    // ── layout ───────────────────────────────────────────────────

    pub fn space(&mut self, amount: f32) {
        self.backend.add_space(SpaceSpec::vertical(amount));
    }

    /// Theme-coloured thin horizontal rule.
    pub fn separator(&mut self) {
        let w = self.backend.available_rect().width();
        let response = self
            .backend
            .allocate(vocab::Vec2::new(w, 5.0), MaraSense::Hover);
        self.backend.paint(separator_paint_cmd(
            response.rect,
            vocab::Stroke::new(1.0, crate::style::outline_base()),
        ));
    }

    pub fn horizontal<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        backend::egui::show_stack_scope_for_ui(
            self.backend.ui_mut(),
            StackScopeSpec::horizontal(),
            |ui| body(&mut MaraUi::new(ui, accent)),
        )
    }

    pub fn vertical<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        backend::egui::show_stack_scope_for_ui(
            self.backend.ui_mut(),
            StackScopeSpec::vertical(),
            |ui| body(&mut MaraUi::new(ui, accent)),
        )
    }

    // ── text ─────────────────────────────────────────────────────

    /// Body text in the theme's foreground colour.
    pub fn label(&mut self, text: &str) -> MaraResponse {
        let backend = &mut self.backend;
        label_backend(
            backend,
            text,
            crate::style::theme().palette.text_primary.into(),
        )
    }

    pub fn label_colored(&mut self, text: &str, color: impl Into<vocab::Color32>) -> MaraResponse {
        let backend = &mut self.backend;
        label_backend(backend, text, color.into())
    }

    // ── widgets (ambient accent) ─────────────────────────────────

    pub fn button(&mut self, label: &str) -> MaraResponse {
        button(self.backend.ui_mut(), label, self.accent)
    }

    pub fn button_h(&mut self, label: &str, height: f32) -> MaraResponse {
        button_h(self.backend.ui_mut(), label, self.accent, height)
    }

    pub fn card_action_button(
        &mut self,
        glyph: &str,
        name: &str,
        subtitle: &str,
        action_glyph: &str,
        action_tooltip: &str,
    ) -> ActionButtonResponse {
        card_action_button(
            self.backend.ui_mut(),
            glyph,
            name,
            subtitle,
            action_glyph,
            action_tooltip,
            self.accent,
        )
    }

    pub fn toggle(&mut self, label: &str, on: &mut bool) -> MaraResponse {
        let backend = &mut self.backend;
        toggle_backend(
            backend,
            label,
            on,
            self.accent,
            crate::style::theme().widgets.toggle.row_h,
        )
    }

    pub fn toggle_track_only(&mut self, on: &mut bool) -> MaraResponse {
        let toggle = crate::style::theme().widgets.toggle;
        let backend = &mut self.backend;
        toggle_track_only_backend(backend, on, self.accent, toggle.track_w, toggle.row_h)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        &mut self,
        label: &str,
        value: &mut f64,
        range: RangeInclusive<f64>,
        decimals: usize,
        suffix: &str,
    ) -> MaraResponse {
        slider(
            self.backend.ui_mut(),
            label,
            value,
            range,
            decimals,
            suffix,
            self.accent,
        )
    }

    pub fn drag_value(
        &mut self,
        label: &str,
        value: &mut f64,
        speed: f64,
        range: RangeInclusive<f64>,
        decimals: usize,
        suffix: &str,
    ) -> MaraResponse {
        drag_value(
            self.backend.ui_mut(),
            label,
            value,
            speed,
            range,
            decimals,
            suffix,
        )
    }

    pub fn dropdown(
        &mut self,
        id_salt: impl std::hash::Hash,
        selected: &mut usize,
        options: &[&str],
    ) -> MaraResponse {
        dropdown(
            self.backend.ui_mut(),
            id_salt,
            selected,
            options,
            self.accent,
        )
    }

    pub fn select_row(
        &mut self,
        id_salt: impl std::hash::Hash,
        label: &str,
        trailing: Option<&str>,
        selected: bool,
    ) -> MaraResponse {
        select_row(
            self.backend.ui_mut(),
            id_salt,
            label,
            trailing,
            selected,
            self.accent,
        )
    }

    pub fn hybrid_select_row(
        &mut self,
        id_salt: impl std::hash::Hash,
        label: &str,
        trailing: Option<&str>,
        selected: bool,
        radio_on: bool,
    ) -> HybridSelectResponse {
        hybrid_select_row(
            self.backend.ui_mut(),
            id_salt,
            label,
            trailing,
            selected,
            radio_on,
            self.accent,
        )
    }

    pub fn text_input(&mut self, text: &mut String, placeholder: &str) -> MaraResponse {
        text_input(self.backend.ui_mut(), text, placeholder, self.accent)
    }

    pub fn readout(&mut self, label: &str, value: &str) -> MaraResponse {
        let backend = &mut self.backend;
        readout_backend(
            backend,
            label,
            value,
            crate::style::theme().widgets.readout.row_h,
        )
    }

    pub fn readout_h(&mut self, label: &str, value: &str, height: f32) -> MaraResponse {
        let backend = &mut self.backend;
        readout_backend(backend, label, value, height)
    }

    pub fn chip(&mut self, label: &str) -> MaraResponse {
        let backend = &mut self.backend;
        chip_colored_backend(backend, label, chip_fill(self.accent), self.accent)
    }

    pub fn chip_colored(&mut self, label: &str, fill: impl Into<vocab::Color32>) -> MaraResponse {
        let backend = &mut self.backend;
        chip_colored_backend(backend, label, fill.into(), self.accent)
    }

    pub fn badge_row(&mut self, label: &str, badges: &[&str]) -> MaraResponse {
        let backend = &mut self.backend;
        badge_row_backend(backend, label, badges, None, self.accent)
    }

    pub fn badge_row_colored(
        &mut self,
        label: &str,
        badges: &[(&str, Option<vocab::Color32>)],
    ) -> MaraResponse {
        let labels: Vec<&str> = badges.iter().map(|(label, _)| *label).collect();
        let fills: Vec<Option<vocab::Color32>> = badges.iter().map(|(_, fill)| *fill).collect();
        let backend = &mut self.backend;
        badge_row_backend(backend, label, &labels, Some(&fills), self.accent)
    }

    pub fn keybinding_row(&mut self, keys: &str, action: &str) -> MaraResponse {
        let backend = &mut self.backend;
        keybinding_row_backend(
            backend,
            keys,
            action,
            crate::style::theme().widgets.keybinding.row_h,
            self.accent,
        )
    }

    pub fn progressbar(&mut self, label: &str, fraction: f32, text: &str) -> MaraResponse {
        let backend = &mut self.backend;
        progressbar_backend(
            backend,
            label,
            fraction,
            text,
            self.accent,
            crate::style::theme().widgets.progress.row_h,
        )
    }

    pub fn color_rgb(&mut self, label: &str, rgb: &mut [f32; 3]) -> MaraResponse {
        color_rgb(self.backend.ui_mut(), label, rgb, self.accent)
    }

    pub fn color_rgba(&mut self, label: &str, rgba: &mut [f32; 4]) -> MaraResponse {
        color_rgba(self.backend.ui_mut(), label, rgba, self.accent)
    }

    /// Foldable titled section whose body is itself a sealed
    /// [`MaraUi`].
    pub fn section(
        &mut self,
        id_salt: &str,
        title: &str,
        default_open: bool,
        body: impl FnOnce(&mut MaraUi<'_>),
    ) {
        let accent = self.accent;
        section(
            self.backend.ui_mut(),
            id_salt,
            title,
            accent,
            default_open,
            |ui| {
                body(&mut MaraUi::new(ui, accent));
            },
        );
    }

    /// Mara-styled right-click context menu on a previous response.
    pub fn context_menu(&mut self, resp: &MaraResponse, body: impl FnOnce(&mut MaraUi<'_>)) {
        let accent = self.accent;
        backend::egui::with_response_for_ui(self.backend.ui_mut(), resp, |raw| {
            context_menu_mara(raw, accent, |ui| {
                body(&mut MaraUi::new(ui, accent));
            });
        });
    }

    /// Recursive tree built from Mara tree rows. The closure only
    /// sees [`TreeBody`].
    pub fn tree(&mut self, body: impl FnOnce(&mut TreeBody<'_>)) {
        let mut tb = TreeBody::new(self.backend.ui_mut());
        body(&mut tb);
    }

    /// Render a fully-typed [`Pod`] inline.
    pub fn pod(&mut self, pod: Pod) -> PodResponse {
        pod.show(self.backend.ui_mut())
    }

    // ── custom drawing ───────────────────────────────────────────

    /// Allocate a custom-drawing slot of `desired_size`, returning
    /// a clipped [`MaraPainter`] plus the slot's interaction
    /// response. This is the sealed equivalent of egui's
    /// `allocate_painter` — the primitive for gauges, plots, and
    /// other bespoke visuals.
    pub fn canvas(&mut self, desired_size: impl Into<vocab::Vec2>) -> (MaraPainter, MaraResponse) {
        let spec = CanvasSlotSpec::new(desired_size.into(), MaraSense::ClickAndDrag);
        let (painter, response) =
            backend::egui::allocate_canvas_slot_for_ui(self.backend.ui_mut(), spec);
        (MaraPainter::new(painter), response)
    }

    /// Interactive custom-drawing surface over an exact screen-space
    /// rect (e.g. a shelf-aware viewport). Returns a clipped
    /// [`MaraPainter`] plus the rect's click/drag response. Unlike
    /// [`MaraUi::canvas`], this does not advance the layout cursor.
    pub fn canvas_at(&mut self, rect: impl Into<vocab::Rect>) -> (MaraPainter, MaraResponse) {
        let rect = rect.into();
        let spec = CanvasRectSpec::new(
            canvas_at_id(self.backend.id(), rect),
            rect,
            MaraSense::ClickAndDrag,
        );
        let (painter, response) =
            backend::egui::interact_canvas_rect_for_ui(self.backend.ui_mut(), spec);
        (MaraPainter::new(painter), response)
    }

    /// A painter over the remaining available rect, without
    /// allocating it (drawing only, no interaction).
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        MaraPainter::new(backend::egui::painter_for_ui_surface(
            self.backend.ui(),
            PaintSurfaceSpec::remaining_available(),
        ))
    }
}

fn separator_paint_cmd(rect: vocab::Rect, stroke: vocab::Stroke) -> PaintCmd {
    PaintCmd::Line {
        a: rect.left_center(),
        b: rect.right_center(),
        stroke,
    }
}

fn canvas_at_id(scope_id: vocab::Id, rect: vocab::Rect) -> vocab::Id {
    scope_id.with(("mara_canvas_at", rect.min.x as i64, rect.min.y as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> vocab::Rect {
        vocab::Rect::from_min_size(vocab::Pos2::new(x, y), vocab::Vec2::new(w, h))
    }

    #[test]
    fn recording_painter_sinks_commands_behind_current_clip() {
        let clip = rect(0.0, 0.0, 100.0, 100.0);
        let painter = MaraPainter::recording(clip);

        painter.line_segment(
            vocab::Pos2::new(1.0, 2.0),
            vocab::Pos2::new(3.0, 4.0),
            vocab::Stroke::new(2.0, vocab::Color32::WHITE),
        );

        let commands = painter.recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, clip);

        let [PaintCmd::Line { a, b, stroke }] = children.as_slice() else {
            panic!("expected clipped line child command");
        };
        assert_eq!(*a, vocab::Pos2::new(1.0, 2.0));
        assert_eq!(*b, vocab::Pos2::new(3.0, 4.0));
        assert_eq!(*stroke, vocab::Stroke::new(2.0, vocab::Color32::WHITE));
    }

    #[test]
    fn recording_painter_with_clip_only_shrinks_recorded_clip() {
        let root_clip = rect(0.0, 0.0, 100.0, 100.0);
        let sub_clip = rect(10.0, 20.0, 40.0, 30.0);
        let painter = MaraPainter::recording(root_clip);
        let clipped = painter.with_clip(sub_clip);

        clipped.rect_filled(
            rect(0.0, 0.0, 80.0, 80.0),
            vocab::CornerRadius::ZERO,
            vocab::Color32::from_rgb(10, 20, 30),
        );

        assert_eq!(clipped.clip_rect(), sub_clip);

        let commands = painter.recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, root_clip.intersect(sub_clip));

        let [
            PaintCmd::RectFilled {
                rect: painted_rect,
                corner,
                fill,
            },
        ] = children.as_slice()
        else {
            panic!("expected clipped rect fill child command");
        };
        assert_eq!(*painted_rect, rect(0.0, 0.0, 80.0, 80.0));
        assert_eq!(*corner, vocab::CornerRadius::ZERO);
        assert_eq!(*fill, vocab::Color32::from_rgb(10, 20, 30));
    }

    #[test]
    fn recording_painter_text_returns_position_rect_and_records_text() {
        let clip = rect(0.0, 0.0, 100.0, 100.0);
        let painter = MaraPainter::recording(clip);
        let pos = vocab::Pos2::new(5.0, 6.0);

        let painted = painter.text(
            pos,
            vocab::Align2::LEFT_TOP,
            "hello",
            13.0,
            vocab::Color32::GRAY,
        );

        assert_eq!(painted, vocab::Rect::from_min_size(pos, vocab::Vec2::ZERO));

        let commands = painter.recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, clip);

        let [
            PaintCmd::Text {
                pos: recorded_pos,
                anchor,
                text,
                size,
                color,
                mono,
            },
        ] = children.as_slice()
        else {
            panic!("expected clipped text child command");
        };
        assert_eq!(*recorded_pos, pos);
        assert_eq!(*anchor, vocab::Align2::LEFT_TOP);
        assert_eq!(text, "hello");
        assert_eq!(*size, 13.0);
        assert_eq!(*color, vocab::Color32::GRAY);
        assert!(!mono);
    }

    #[test]
    fn separator_lowers_to_backend_neutral_line_command() {
        let rect = rect(10.0, 20.0, 120.0, 5.0);
        let stroke = vocab::Stroke::new(1.0, vocab::Color32::WHITE);
        let PaintCmd::Line { a, b, stroke: s } = separator_paint_cmd(rect, stroke) else {
            panic!("expected separator line command");
        };

        assert_eq!(a, rect.left_center());
        assert_eq!(b, rect.right_center());
        assert_eq!(s, stroke);
    }

    #[test]
    fn canvas_at_id_is_scope_and_position_stable() {
        let scope = vocab::Id::new("scope");
        let rect = rect(10.25, 20.75, 120.0, 80.0);

        assert_eq!(
            canvas_at_id(scope, rect),
            scope.with(("mara_canvas_at", 10_i64, 20_i64))
        );
    }
}
