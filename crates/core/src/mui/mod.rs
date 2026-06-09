//! # `mui` — the sealed Mara UI surface
//!
//! [`MaraUi`] is the only thing Mara hands to consumer drawing code
//! (module inline bodies, view bodies, foldable sections). It wraps
//! an `egui::Ui` behind a private field and exposes exactly Mara's
//! widget set plus a typed canvas/painter, so an app can compose UI
//! without ever holding a raw `egui::Ui`.
//!
//! Capability rules:
//!
//! * widget methods return [`MaraResponse`] — a plain data snapshot,
//!   not `egui::Response` (whose public `ctx` field would leak the
//!   whole `egui::Context`).
//! * custom drawing goes through [`MaraPainter`], which only speaks
//!   [`crate::vocab`] data types and the theme's fonts.
//! * input is read through [`MaraInput`], a per-frame snapshot.
//! * the raw `egui::Ui` is reachable only behind the `raw-egui`
//!   feature (`MaraUi::raw_ui_mut`) — the explicit, greppable
//!   escape hatch used by first-party module crates and hosts.

use std::ops::RangeInclusive;

use egui::{Align2, Color32, CornerRadius, FontId, Id, Pos2, Rect, Sense, Stroke, Vec2};

use crate::pod::{Pod, PodResponse};
use crate::style::theme;
use crate::widget::{
    ActionButtonResponse, HybridSelectResponse, TreeBody, badge_row, badge_row_colored, button,
    button_h, card_action_button, card_button, chip, chip_colored, color_rgb, color_rgba,
    context_menu_mara, drag_value, dropdown, hybrid_select_row, keybinding_row, progressbar,
    readout, readout_h, section, select_row, slider, text_input, toggle, toggle_track_only,
};

// ─── MaraResponse ─────────────────────────────────────────────────

/// Plain-data interaction snapshot returned by every [`MaraUi`]
/// widget method.
///
/// Mara cannot return `egui::Response` from sealed surfaces because
/// its public `ctx` field hands out the entire `egui::Context`.
/// This type copies the flags consumer code actually branches on
/// and privately retains the original response so Mara-typed
/// follow-ups (e.g. [`MaraUi::context_menu`]) still work.
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
    pub drag_delta: Vec2,
    /// Pointer position during click/drag interaction with this
    /// widget, if any.
    pub interact_pointer: Option<Pos2>,
    /// Pointer position while hovering this widget, if any.
    pub hover_pos: Option<Pos2>,
    pub rect: Rect,
    pub(crate) inner: egui::Response,
}

impl From<egui::Response> for MaraResponse {
    fn from(inner: egui::Response) -> Self {
        Self {
            clicked: inner.clicked(),
            double_clicked: inner.double_clicked(),
            secondary_clicked: inner.secondary_clicked(),
            hovered: inner.hovered(),
            changed: inner.changed(),
            dragged: inner.dragged(),
            drag_started: inner.drag_started(),
            drag_stopped: inner.drag_stopped(),
            drag_delta: inner.drag_delta(),
            interact_pointer: inner.interact_pointer_pos(),
            hover_pos: inner.hover_pos(),
            rect: inner.rect,
            inner,
        }
    }
}

impl MaraResponse {
    /// The raw `egui::Response`. Raw-egui escape hatch.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn raw(&self) -> &egui::Response {
        &self.inner
    }
}

// ─── MaraInput ────────────────────────────────────────────────────

/// Per-frame input snapshot for custom (canvas-style) surfaces.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaraInput {
    /// Latest pointer position, if the pointer is over the window.
    pub pointer: Option<Pos2>,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub secondary_down: bool,
    pub secondary_pressed: bool,
    /// Smooth scroll delta this frame.
    pub scroll_delta: Vec2,
    /// Pointer movement since last frame.
    pub pointer_delta: Vec2,
    /// Pinch/ctrl-scroll zoom factor this frame (1.0 = none).
    pub zoom_delta: f32,
    pub modifiers_shift: bool,
    pub modifiers_ctrl: bool,
    pub modifiers_alt: bool,
}

impl MaraInput {
    pub(crate) fn snapshot(ctx: &egui::Context) -> Self {
        ctx.input(|i| Self {
            pointer: i.pointer.latest_pos(),
            primary_down: i.pointer.primary_down(),
            primary_pressed: i.pointer.primary_pressed(),
            primary_released: i.pointer.primary_released(),
            secondary_down: i.pointer.secondary_down(),
            secondary_pressed: i.pointer.secondary_pressed(),
            scroll_delta: i.smooth_scroll_delta,
            pointer_delta: i.pointer.delta(),
            zoom_delta: i.zoom_delta(),
            modifiers_shift: i.modifiers.shift,
            modifiers_ctrl: i.modifiers.ctrl,
            modifiers_alt: i.modifiers.alt,
        })
    }
}

// ─── MaraPainter ──────────────────────────────────────────────────

/// Typed draw surface for custom content (gauges, plots, overlays).
///
/// Wraps an `egui::Painter` behind a private field; every method
/// speaks [`crate::vocab`] data types only. Text always renders in
/// the theme's font families, so custom drawing cannot drift away
/// from Mara's typography.
pub struct MaraPainter {
    painter: egui::Painter,
}

impl MaraPainter {
    pub(crate) fn new(painter: egui::Painter) -> Self {
        Self { painter }
    }

    /// The rect drawing is clipped to.
    #[must_use]
    pub fn clip_rect(&self) -> Rect {
        self.painter.clip_rect()
    }

    /// A sub-painter hard-clipped to `rect` (intersected with the
    /// current clip, so clips only ever shrink).
    #[must_use]
    pub fn with_clip(&self, rect: Rect) -> MaraPainter {
        MaraPainter::new(
            self.painter
                .with_clip_rect(rect.intersect(self.painter.clip_rect())),
        )
    }

    pub fn line_segment(&self, a: Pos2, b: Pos2, stroke: Stroke) {
        self.painter.line_segment([a, b], stroke);
    }

    /// Open polyline through `points`.
    pub fn polyline(&self, points: Vec<Pos2>, stroke: Stroke) {
        self.painter.line(points, stroke);
    }

    /// Closed path with independent fill and stroke.
    pub fn polygon(&self, points: Vec<Pos2>, fill: Color32, stroke: Stroke) {
        self.painter.add(egui::epaint::PathShape {
            points,
            closed: true,
            fill,
            stroke: stroke.into(),
        });
    }

    pub fn rect_filled(&self, rect: Rect, corner: impl Into<CornerRadius>, fill: Color32) {
        self.painter.rect_filled(rect, corner, fill);
    }

    pub fn rect_stroke(&self, rect: Rect, corner: impl Into<CornerRadius>, stroke: Stroke) {
        self.painter
            .rect_stroke(rect, corner, stroke, egui::StrokeKind::Inside);
    }

    pub fn circle_filled(&self, center: Pos2, radius: f32, fill: Color32) {
        self.painter.circle_filled(center, radius, fill);
    }

    pub fn circle_stroke(&self, center: Pos2, radius: f32, stroke: Stroke) {
        self.painter.circle_stroke(center, radius, stroke);
    }

    pub fn arrow(&self, origin: Pos2, vec: Vec2, stroke: Stroke) {
        self.painter.arrow(origin, vec, stroke);
    }

    /// Text in the theme's proportional family. Returns the painted
    /// rect.
    pub fn text(
        &self,
        pos: Pos2,
        anchor: Align2,
        text: impl ToString,
        size: f32,
        color: Color32,
    ) -> Rect {
        self.painter
            .text(pos, anchor, text, FontId::proportional(size), color)
    }

    /// Text in the theme's monospace family. Returns the painted
    /// rect.
    pub fn text_mono(
        &self,
        pos: Pos2,
        anchor: Align2,
        text: impl ToString,
        size: f32,
        color: Color32,
    ) -> Rect {
        self.painter
            .text(pos, anchor, text, FontId::monospace(size), color)
    }

    /// Textured quad. `uv` is in 0..=1 texture space; pass
    /// `Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))` (or
    /// [`MaraPainter::full_uv`]) for the whole texture.
    pub fn image(&self, texture: egui::TextureId, rect: Rect, uv: Rect, tint: Color32) {
        self.painter.image(texture, rect, uv, tint);
    }

    /// The full 0..=1 UV rect, for [`MaraPainter::image`].
    #[must_use]
    pub fn full_uv() -> Rect {
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0))
    }

    /// The raw `egui::Painter`. Raw-egui escape hatch.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn raw(&self) -> &egui::Painter {
        &self.painter
    }
}

// ─── MaraUi ───────────────────────────────────────────────────────

/// The sealed widget surface handed to consumer drawing code.
///
/// Carries an ambient accent colour so widget calls stay terse;
/// override it per-scope with [`MaraUi::set_accent`].
pub struct MaraUi<'a> {
    pub(crate) ui: &'a mut egui::Ui,
    accent: Color32,
}

impl<'a> MaraUi<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui, accent: Color32) -> Self {
        Self { ui, accent }
    }

    /// Wrap a raw `egui::Ui`. Raw-egui escape hatch for hosts that
    /// own their own egui pass and want to embed sealed Mara
    /// content inside it.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn from_raw(ui: &'a mut egui::Ui, accent: Color32) -> Self {
        Self::new(ui, accent)
    }

    /// The raw `egui::Ui`. Raw-egui escape hatch.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn raw_ui_mut(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable. First-party Mara module crates
    /// (canvas, image, map, …) use this instead of enabling
    /// `raw-egui`, because a dependency enabling that feature would
    /// unify it ON for the entire consumer graph and unseal
    /// `mara_core::egui` for everyone.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_raw_ui(&mut self) -> &mut egui::Ui {
        self.ui
    }

    /// Internal first-party constructor — NOT part of the public
    /// API and not semver-stable. Used by host plugins that own the
    /// egui pass (e.g. `bevy_mara`) to hand sealed surfaces to app
    /// code.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_from_raw(ui: &'a mut egui::Ui, accent: Color32) -> Self {
        Self::new(ui, accent)
    }

    // ── ambient state ────────────────────────────────────────────

    #[must_use]
    pub fn accent(&self) -> Color32 {
        self.accent
    }

    pub fn set_accent(&mut self, accent: Color32) {
        self.accent = accent;
    }

    /// Stable id of the underlying scope, for salting widget ids.
    #[must_use]
    pub fn id(&self) -> Id {
        self.ui.id()
    }

    #[must_use]
    pub fn available_width(&self) -> f32 {
        self.ui.available_width()
    }

    #[must_use]
    pub fn available_height(&self) -> f32 {
        self.ui.available_height()
    }

    #[must_use]
    pub fn available_rect(&self) -> Rect {
        self.ui.available_rect_before_wrap()
    }

    /// Per-frame input snapshot for custom interaction logic.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        MaraInput::snapshot(self.ui.ctx())
    }

    // ── layout ───────────────────────────────────────────────────

    pub fn space(&mut self, amount: f32) {
        self.ui.add_space(amount);
    }

    /// Theme-coloured thin horizontal rule.
    pub fn separator(&mut self) {
        let w = self.ui.available_width();
        let (rect, _) = self
            .ui
            .allocate_exact_size(Vec2::new(w, 5.0), Sense::hover());
        self.ui.painter().line_segment(
            [rect.left_center(), rect.right_center()],
            Stroke::new(1.0, crate::style::outline_base()),
        );
    }

    pub fn horizontal<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        self.ui
            .horizontal(|ui| body(&mut MaraUi::new(ui, accent)))
            .inner
    }

    pub fn vertical<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        self.ui
            .vertical(|ui| body(&mut MaraUi::new(ui, accent)))
            .inner
    }

    // ── text ─────────────────────────────────────────────────────

    /// Body text in the theme's foreground colour.
    pub fn label(&mut self, text: &str) -> MaraResponse {
        self.label_colored(text, theme().palette.text_primary)
    }

    pub fn label_colored(&mut self, text: &str, color: Color32) -> MaraResponse {
        self.ui.label(egui::RichText::new(text).color(color)).into()
    }

    // ── widgets (ambient accent) ─────────────────────────────────

    pub fn button(&mut self, label: &str) -> MaraResponse {
        button(self.ui, label, self.accent).into()
    }

    pub fn button_h(&mut self, label: &str, height: f32) -> MaraResponse {
        button_h(self.ui, label, self.accent, height).into()
    }

    pub fn card_button(&mut self, glyph: &str, name: &str, subtitle: &str) -> MaraResponse {
        card_button(self.ui, glyph, name, subtitle, self.accent).into()
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
            self.ui,
            glyph,
            name,
            subtitle,
            action_glyph,
            action_tooltip,
            self.accent,
        )
    }

    pub fn toggle(&mut self, label: &str, on: &mut bool) -> MaraResponse {
        toggle(self.ui, label, on, self.accent).into()
    }

    pub fn toggle_track_only(&mut self, on: &mut bool) -> MaraResponse {
        toggle_track_only(self.ui, on, self.accent).into()
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
        slider(self.ui, label, value, range, decimals, suffix, self.accent).into()
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
        drag_value(self.ui, label, value, speed, range, decimals, suffix).into()
    }

    pub fn dropdown(
        &mut self,
        id_salt: impl std::hash::Hash,
        selected: &mut usize,
        options: &[&str],
    ) -> MaraResponse {
        dropdown(self.ui, id_salt, selected, options, self.accent).into()
    }

    pub fn select_row(
        &mut self,
        id_salt: impl std::hash::Hash,
        label: &str,
        trailing: Option<&str>,
        selected: bool,
    ) -> MaraResponse {
        select_row(self.ui, id_salt, label, trailing, selected, self.accent).into()
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
            self.ui,
            id_salt,
            label,
            trailing,
            selected,
            radio_on,
            self.accent,
        )
    }

    pub fn text_input(&mut self, text: &mut String, placeholder: &str) -> MaraResponse {
        text_input(self.ui, text, placeholder, self.accent).into()
    }

    pub fn readout(&mut self, label: &str, value: &str) -> MaraResponse {
        readout(self.ui, label, value).into()
    }

    pub fn readout_h(&mut self, label: &str, value: &str, height: f32) -> MaraResponse {
        readout_h(self.ui, label, value, height).into()
    }

    pub fn chip(&mut self, label: &str) -> MaraResponse {
        chip(self.ui, label, self.accent).into()
    }

    pub fn chip_colored(&mut self, label: &str, fill: Color32) -> MaraResponse {
        chip_colored(self.ui, label, fill, self.accent).into()
    }

    pub fn badge_row(&mut self, label: &str, badges: &[&str]) -> MaraResponse {
        badge_row(self.ui, label, badges, self.accent).into()
    }

    pub fn badge_row_colored(
        &mut self,
        label: &str,
        badges: &[(&str, Option<Color32>)],
    ) -> MaraResponse {
        badge_row_colored(self.ui, label, badges, self.accent).into()
    }

    pub fn keybinding_row(&mut self, keys: &str, action: &str) -> MaraResponse {
        keybinding_row(self.ui, keys, action).into()
    }

    pub fn progressbar(&mut self, label: &str, fraction: f32, text: &str) -> MaraResponse {
        progressbar(self.ui, label, fraction, text, self.accent).into()
    }

    pub fn color_rgb(&mut self, label: &str, rgb: &mut [f32; 3]) -> MaraResponse {
        color_rgb(self.ui, label, rgb, self.accent).into()
    }

    pub fn color_rgba(&mut self, label: &str, rgba: &mut [f32; 4]) -> MaraResponse {
        color_rgba(self.ui, label, rgba, self.accent).into()
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
        section(self.ui, id_salt, title, accent, default_open, |ui| {
            body(&mut MaraUi::new(ui, accent));
        });
    }

    /// Mara-styled right-click context menu on a previous response.
    pub fn context_menu(&mut self, resp: &MaraResponse, body: impl FnOnce(&mut MaraUi<'_>)) {
        let accent = self.accent;
        context_menu_mara(&resp.inner, accent, |ui| {
            body(&mut MaraUi::new(ui, accent));
        });
    }

    /// Recursive tree built from Mara tree rows. The closure only
    /// sees [`TreeBody`].
    pub fn tree(&mut self, body: impl FnOnce(&mut TreeBody<'_>)) {
        let mut tb = TreeBody::new(self.ui);
        body(&mut tb);
    }

    /// Render a fully-typed [`Pod`] inline.
    pub fn pod(&mut self, pod: Pod) -> PodResponse {
        pod.show(self.ui)
    }

    // ── custom drawing ───────────────────────────────────────────

    /// Allocate a custom-drawing slot of `desired_size`, returning
    /// a clipped [`MaraPainter`] plus the slot's interaction
    /// response. This is the sealed equivalent of egui's
    /// `allocate_painter` — the primitive for gauges, plots, and
    /// other bespoke visuals.
    pub fn canvas(&mut self, desired_size: Vec2) -> (MaraPainter, MaraResponse) {
        let (response, painter) = self
            .ui
            .allocate_painter(desired_size, Sense::click_and_drag());
        let clipped = painter.with_clip_rect(response.rect.intersect(self.ui.clip_rect()));
        (MaraPainter::new(clipped), response.into())
    }

    /// Interactive custom-drawing surface over an exact screen-space
    /// rect (e.g. a shelf-aware viewport). Returns a clipped
    /// [`MaraPainter`] plus the rect's click/drag response. Unlike
    /// [`MaraUi::canvas`], this does not advance the layout cursor.
    pub fn canvas_at(&mut self, rect: Rect) -> (MaraPainter, MaraResponse) {
        let response = self.ui.interact(
            rect,
            self.ui
                .id()
                .with(("mara_canvas_at", rect.min.x as i64, rect.min.y as i64)),
            Sense::click_and_drag(),
        );
        let painter = self
            .ui
            .painter()
            .with_clip_rect(rect.intersect(self.ui.clip_rect()));
        (MaraPainter::new(painter), response.into())
    }

    /// A painter over the remaining available rect, without
    /// allocating it (drawing only, no interaction).
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        let rect = self.ui.available_rect_before_wrap();
        MaraPainter::new(
            self.ui
                .painter()
                .with_clip_rect(rect.intersect(self.ui.clip_rect())),
        )
    }
}
