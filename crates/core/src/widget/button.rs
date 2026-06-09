//! Mara-styled button — one widget that covers every shape the kit
//! ships:
//!
//! * **Plain** — single-row, centred label.
//! * **With subtitle** — two-row stack, primary `label` over a dim
//!   `subtitle`. Replaces the old `card_button`.
//! * **With glyph** — leading icon column, label/subtitle to the
//!   right. Pair with `subtitle()` for the UE5-style "Create" cards.
//! * **With animation** — replaces the rest-state hover-fill with one
//!   of thirteen [`FillStyle`] CSS-style transitions.
//!
//! Build via [`Button::new`] and chain options. The shortcut
//! [`button`] / [`button_h`] free functions cover the simple case
//! (`Button::new(label).show(ui, accent)`).
//!
//! Theme behaviour:
//!
//! * Tint fractions (rest / hover / press) come from the active theme
//!   (`button_tint_*`). PRO and GAME look right out of the box.
//! * `theme().button_full_accent_on_press` makes pressed buttons fill
//!   solid accent (GAME); plain panels keep the lerp.
//! * Animation scale is gated by `theme().animations_enabled` and
//!   stretched by `theme().button_anim_scale`.

use crate::style::{
    body_accent, contrast_text_for, glass_alpha_card, pane_fill, section_fill, section_show_frame,
    surface_lift_target, theme, widget_border,
};

/// Default button row height (single row, no subtitle).
pub const BUTTON_ROW_H: f32 = 24.0;
/// Default button row height when a subtitle is present — exactly
/// 2U so a 2-layer button stacks cleanly with two 1U widgets above /
/// below it in the same pod (no off-grid kink).
pub const BUTTON_ROW_H_SUBTITLE: f32 = 2.0 * crate::style::UNIT;
/// Centred label font size.
pub const BUTTON_LABEL_FONT: f32 = 12.0;
/// Subtitle (second-row) font size — small dim caption.
pub const BUTTON_SUBTITLE_FONT: f32 = 10.0;
/// Glyph font size when the button has a leading icon.
pub const BUTTON_GLYPH_FONT: f32 = 14.0;
/// Card-button row height. Kept as an alias of [`BUTTON_ROW_H_SUBTITLE`]
/// for callers (Pod, etc.) that still measure in "card" units.
pub const CARD_BUTTON_ROW_H: f32 = BUTTON_ROW_H_SUBTITLE;
/// Width of the embedded end action in [`ActionButton`].
pub const BUTTON_ACTION_W: f32 = 28.0;
/// Gap between the main button body and embedded end action.
pub const BUTTON_ACTION_GAP: f32 = 6.0;

/// Picks which hover-fill animation to run when [`Button::animation`]
/// is set. At rest the button paints identically to the plain
/// `button` recipe; the animation paints a darker-accent shape that
/// converges toward filling the rect over the theme's animation
/// duration.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum FillStyle {
    /// Solid rect slides in from left edge.
    SlideLeft,
    /// Right-leaning parallelogram slides in from left.
    Parallelogram,
    /// Two parallelograms — one from each side — meet in the centre.
    ParallelogramMeet,
    /// Two opposite-leaning parallelograms forming a bowtie.
    Bowtie,
    /// Two triangle-edged narrow trapezoids meeting from sides.
    BandsMeet,
    /// Four quadrant squares converging diagonally to centre.
    CornerSquares,
    /// Two large triangles growing from opposite corners.
    DiagonalTriangles,
    /// Single circle expanding from centre to fill the button.
    CircleGrow,
    /// Four vertical bars rising from the bottom edge (equalizer).
    Equalizer,
    /// Top + bottom halves slide vertically to meet at the middle.
    HorizontalSlide,
    /// Four horizontal bars top/bottom with a 0.4 s phase delay.
    HorizontalSlideDelayed,
    /// Four vertical bars left/right with a 0.4 s phase delay.
    VerticalSlideDelayed,
    /// Two circles enter from opposite ends and cross through middle.
    CrissCross,
}

/// Builder for the unified mara button. Construct with
/// [`Button::new`], chain optional settings, paint with [`show`].
///
/// [`show`]: Button::show
pub struct Button<'a> {
    label: &'a str,
    subtitle: Option<&'a str>,
    glyph: Option<&'a str>,
    animation: Option<FillStyle>,
    height: Option<f32>,
}

/// Responses from a two-layer button that also has an independent
/// action button embedded at the far end.
#[derive(Debug)]
pub struct ActionButtonResponse {
    /// Click target for the main button body.
    pub body: crate::mui::MaraResponse,
    /// Independent click target for the embedded tail action.
    pub action: crate::mui::MaraResponse,
}

/// Mara card/button with a nested end action, for rows like:
///
/// ```text
/// [ icon  Zone A
///         root · 4 pts                         + ]
/// ```
///
/// The body and the tail action are separate click targets. The
/// tail is visually inside the same button chrome but its rect does
/// not overlap the body, so clicking `+` does not also select the
/// row.
pub struct ActionButton<'a> {
    label: &'a str,
    subtitle: Option<&'a str>,
    glyph: Option<&'a str>,
    action_glyph: &'a str,
    action_tooltip: Option<&'a str>,
    action_armed: bool,
    height: Option<f32>,
}

impl<'a> ActionButton<'a> {
    pub fn new(label: &'a str, action_glyph: &'a str) -> Self {
        Self {
            label,
            subtitle: None,
            glyph: None,
            action_glyph,
            action_tooltip: None,
            action_armed: false,
            height: None,
        }
    }

    /// Add a small dim caption under the primary label.
    pub fn subtitle(mut self, s: &'a str) -> Self {
        self.subtitle = Some(s);
        self
    }

    /// Add a leading icon glyph column. Same icon lookup behaviour
    /// as [`Button::glyph`].
    pub fn glyph(mut self, g: &'a str) -> Self {
        self.glyph = Some(g);
        self
    }

    /// Hover text for the embedded action target.
    pub fn action_tooltip(mut self, text: &'a str) -> Self {
        self.action_tooltip = Some(text);
        self
    }

    /// Paint the embedded action in its active/armed state.
    pub fn action_armed(mut self, armed: bool) -> Self {
        self.action_armed = armed;
        self
    }

    /// Override natural height. Defaults to 1U button height or the
    /// two-layer card height when a subtitle is present.
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// Paint the action button and return independent body/action
    /// responses.
    pub fn show(self, ui: &mut egui::Ui, accent: egui::Color32) -> ActionButtonResponse {
        let th = theme();
        let button = th.widgets.button;
        let height = self.height.unwrap_or(if self.subtitle.is_some() {
            button.subtitle_row_h
        } else {
            button.row_h
        });
        let w = ui.available_width();
        let (rect, base_resp) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::hover());
        let action_size = (height - 2.0 * button.edge_pad.min(6.0))
            .clamp(18.0, BUTTON_ACTION_W)
            .min(rect.width().max(0.0));
        let action_rect = egui::Rect::from_center_size(
            egui::pos2(
                rect.max.x - button.edge_pad - action_size * 0.5,
                rect.center().y,
            ),
            egui::vec2(action_size, action_size),
        );
        let body_rect = egui::Rect::from_min_max(
            rect.min,
            egui::pos2(
                (action_rect.min.x - BUTTON_ACTION_GAP).max(rect.min.x),
                rect.max.y,
            ),
        );
        let body = ui.interact(
            body_rect,
            base_resp.id.with("mara_action_button_body"),
            egui::Sense::click(),
        );
        let mut action = ui.interact(
            action_rect,
            base_resp.id.with("mara_action_button_tail"),
            egui::Sense::click(),
        );
        if let Some(tip) = self.action_tooltip {
            action = action.on_hover_text(tip);
        }
        if !ui.is_rect_visible(rect) {
            return ActionButtonResponse {
                body: body.into(),
                action: action.into(),
            };
        }

        let radius = egui::CornerRadius::same(th.radius_widget);
        let body_acc = body_accent(accent);
        let base = if section_show_frame() {
            section_fill(accent)
        } else {
            pane_fill(accent)
        };
        let target = surface_lift_target(body_acc);
        let active = body.hovered()
            || action.hovered()
            || body.is_pointer_button_down_on()
            || action.is_pointer_button_down_on();
        let hover_t = if th.animations_enabled {
            let dur = 0.25 * th.button_anim_scale.max(0.01);
            ui.ctx().animate_bool_with_time(
                base_resp.id.with("mara_action_button_hover"),
                active,
                dur,
            )
        } else if active {
            1.0
        } else {
            0.0
        };
        let rest_solid = lerp_col(base, target, button.tint_rest);
        let rest_bg = with_alpha(rest_solid, glass_alpha_card());
        let target_bg = if button.full_accent_on_press {
            with_alpha(body_acc, 255)
        } else {
            let press_solid = lerp_col(base, target, button.tint_press);
            with_alpha(press_solid, glass_alpha_card())
        };
        let bg = lerp_col_alpha(rest_bg, target_bg, hover_t);
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, radius, bg);
        painter.rect_stroke(
            rect,
            radius,
            egui::Stroke::new(
                th.border_width,
                lerp_col(widget_border(accent), accent, hover_t),
            ),
            egui::epaint::StrokeKind::Inside,
        );
        paint_click_pulse(
            ui.ctx(),
            &ui.painter_at(rect.expand(10.0)),
            &body,
            rect,
            accent,
            radius,
        );

        let action_hover_t = if th.animations_enabled {
            ui.ctx().animate_bool_with_time(
                action.id.with("mara_action_button_tail_hover"),
                action.hovered() || action.is_pointer_button_down_on() || self.action_armed,
                0.18 * th.button_anim_scale.max(0.01),
            )
        } else if action.hovered() || action.is_pointer_button_down_on() || self.action_armed {
            1.0
        } else {
            0.0
        };
        let action_fill = lerp_col_alpha(
            with_alpha(surface_lift_target(body_acc), glass_alpha_card()),
            with_alpha(accent, if self.action_armed { 96 } else { 74 }),
            action_hover_t,
        );
        let action_radius = egui::CornerRadius::same((action_size * 0.5).round() as u8);
        painter.rect_filled(action_rect, action_radius, action_fill);
        painter.rect_stroke(
            action_rect,
            action_radius,
            egui::Stroke::new(
                th.border_width,
                lerp_col(
                    widget_border(accent),
                    accent,
                    action_hover_t.max(if self.action_armed { 0.8 } else { 0.0 }),
                ),
            ),
            egui::epaint::StrokeKind::Inside,
        );
        paint_click_pulse(
            ui.ctx(),
            &ui.painter_at(action_rect.expand(10.0)),
            &action,
            action_rect,
            accent,
            action_radius,
        );

        let primary = contrast_text_for(bg);
        let secondary = lerp_col(primary, bg, 0.4);
        paint_button_contents(
            ui,
            &painter,
            body_rect,
            self.label,
            self.subtitle,
            self.glyph,
            primary,
            secondary,
            accent,
        );
        paint_glyph_or_icon(
            ui,
            &painter,
            action_rect.center(),
            egui::Align2::CENTER_CENTER,
            self.action_glyph,
            (button.glyph_font + 2.0).max(14.0),
            accent,
        );

        ActionButtonResponse {
            body: body.into(),
            action: action.into(),
        }
    }
}

impl<'a> Button<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            subtitle: None,
            glyph: None,
            animation: None,
            height: None,
        }
    }

    /// Add a small dim caption beneath the primary label. Bumps the
    /// default height to [`BUTTON_ROW_H_SUBTITLE`] (32 px) unless
    /// [`Button::height`] is also set.
    pub fn subtitle(mut self, s: &'a str) -> Self {
        self.subtitle = Some(s);
        self
    }

    /// Add a leading icon glyph column. Painted in `accent` to the
    /// left of the text. Pair with [`Button::subtitle`] for the
    /// "Create" / preset-card layout.
    pub fn glyph(mut self, g: &'a str) -> Self {
        self.glyph = Some(g);
        self
    }

    /// Replace the rest→hover transition with one of thirteen CSS
    /// hover-fill animations. The button still paints `accent`-tinted
    /// glass at rest; the animation overlays a darker-accent shape
    /// that converges to the pressed fill.
    pub fn animation(mut self, a: FillStyle) -> Self {
        self.animation = Some(a);
        self
    }

    /// Override the button's height. Without this, the height
    /// defaults to [`BUTTON_ROW_H`] (no subtitle) or
    /// [`BUTTON_ROW_H_SUBTITLE`] (with subtitle).
    pub fn height(mut self, h: f32) -> Self {
        self.height = Some(h);
        self
    }

    /// Paint the button into `ui` and return its `Response`.
    pub fn show(self, ui: &mut egui::Ui, accent: egui::Color32) -> egui::Response {
        let th = theme();
        let button = th.widgets.button;
        let height = self.height.unwrap_or(if self.subtitle.is_some() {
            button.subtitle_row_h
        } else {
            button.row_h
        });
        let w = ui.available_width();
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, height), egui::Sense::click());
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let radius = egui::CornerRadius::same(th.radius_widget);

        // No press-shrink — buttons keep their footprint on hold so a
        // long press doesn't ripple layout around them. The
        // `press_depress_amount` helper is still defined below for
        // any future widget that wants the effect (the tree's
        // hide/show layering is a more natural fit).
        let painted_rect = rect;

        // Unified rest + hover fills. Used for BOTH the plain and the
        // animated paths so a button with `animation = None` and one
        // with `animation = Some(...)` share the exact same border
        // animation, end-state colour, and timing — only the fill
        // *transition shape* differs (flat fade vs. polygon overlay).
        let body_acc = body_accent(accent);
        let base = if section_show_frame() {
            section_fill(accent)
        } else {
            pane_fill(accent)
        };
        let target = surface_lift_target(body_acc);

        let pressed = resp.is_pointer_button_down_on();
        let active = resp.hovered() || pressed;

        // Smoothed hover/press signal. Drives bg lerp, border lerp,
        // and the FillStyle overlay position. Same id/duration in
        // both paths so the visuals can't drift.
        let hover_t = if th.animations_enabled {
            let dur = 0.25 * th.button_anim_scale.max(0.01);
            ui.ctx()
                .animate_bool_with_time(resp.id.with("mara_button_hover"), active, dur)
        } else if active {
            1.0
        } else {
            0.0
        };

        let rest_solid = lerp_col(base, target, button.tint_rest);
        let rest_bg = with_alpha(rest_solid, glass_alpha_card());
        // Press destination — picks `body_acc` solid in GAME (theme
        // sets `button_full_accent_on_press`), otherwise the standard
        // press-tint glass. Used as the convergence colour by both
        // plain and animated paths so a button looks the same at
        // hover_t = 1 regardless of animation flag.
        let target_bg = if button.full_accent_on_press {
            // Solid accent at full alpha — visually the same colour
            // a long-held GAME button collapses onto.
            with_alpha(body_acc, 255)
        } else {
            let press_solid = lerp_col(base, target, button.tint_press);
            with_alpha(press_solid, glass_alpha_card())
        };

        // Painter for the rect interior. Uses `painted_rect` (the
        // depressed rect) so the bg, border, and FillStyle overlay
        // all shrink with the press animation in lockstep.
        let painter = ui.painter_at(painted_rect);

        // Bg paint:
        //   * Animated → rest fill + FillStyle polygon overlay at
        //     `hover_t` with `target_bg` colour.
        //   * Plain    → single rect lerped from rest_bg → target_bg
        //     by `hover_t`. Same end state as animated, just no shape.
        let centre_bg = if let Some(style) = self.animation {
            painter.rect_filled(painted_rect, radius, rest_bg);
            paint_fill(&painter, painted_rect, hover_t, target_bg, style);
            lerp_col_alpha(rest_bg, target_bg, hover_t)
        } else {
            let bg = lerp_col_alpha(rest_bg, target_bg, hover_t);
            painter.rect_filled(painted_rect, radius, bg);
            bg
        };

        // Border — same lerp recipe in both paths so plain and
        // animated buttons feel like the same widget family.
        let border_col = lerp_col(widget_border(accent), accent, hover_t);
        painter.rect_stroke(
            painted_rect,
            radius,
            egui::Stroke::new(th.border_width, border_col),
            egui::epaint::StrokeKind::Inside,
        );

        // Click pulse — concentric stroke discharge that paints up to
        // 8 px OUTSIDE the painted rect. Use a wider painter so the
        // pulse isn't sliced by `painter`'s clip rect (which equals
        // the depressed button bounds).
        let outer = ui.painter_at(rect.expand(10.0));
        paint_click_pulse(ui.ctx(), &outer, &resp, painted_rect, accent, radius);

        let bg = centre_bg;
        let primary = contrast_text_for(bg);
        let secondary = lerp_col(primary, bg, 0.4);

        // Text/glyph painter follows the depressed rect so the
        // contents track the press shrink in lockstep with bg/border.
        let painter = ui.painter_at(painted_rect);
        paint_button_contents(
            ui,
            &painter,
            painted_rect,
            self.label,
            self.subtitle,
            self.glyph,
            primary,
            secondary,
            accent,
        );
        resp
    }
}

/// Render a plain button at the default [`BUTTON_ROW_H`] height —
/// shortcut for `Button::new(label).show(ui, accent)`.
pub fn button(ui: &mut egui::Ui, label: &str, accent: egui::Color32) -> egui::Response {
    Button::new(label).show(ui, accent)
}

/// Variable-height plain button — shortcut for
/// `Button::new(label).height(height).show(ui, accent)`. Used by
/// resizable pods.
pub fn button_h(
    ui: &mut egui::Ui,
    label: &str,
    accent: egui::Color32,
    height: f32,
) -> egui::Response {
    Button::new(label).height(height).show(ui, accent)
}

/// Compatibility shortcut — paints the old "preset card" layout
/// (glyph + name + subtitle). Same result as
/// `Button::new(name).glyph(glyph).subtitle(subtitle).show(ui, accent)`.
pub fn card_button(
    ui: &mut egui::Ui,
    glyph: &str,
    name: &str,
    subtitle: &str,
    accent: egui::Color32,
) -> egui::Response {
    Button::new(name)
        .glyph(glyph)
        .subtitle(subtitle)
        .show(ui, accent)
}

/// Shortcut for [`ActionButton`] with a leading glyph, primary label,
/// subtitle, and independent tail action.
pub fn card_action_button(
    ui: &mut egui::Ui,
    glyph: &str,
    name: &str,
    subtitle: &str,
    action_glyph: &str,
    action_tooltip: &str,
    accent: egui::Color32,
) -> ActionButtonResponse {
    ActionButton::new(name, action_glyph)
        .glyph(glyph)
        .subtitle(subtitle)
        .action_tooltip(action_tooltip)
        .show(ui, accent)
}

// ─── Colour helpers ────────────────────────────────────────────────

/// Linear interpolation in straight-RGB space (alpha ignored —
/// callers should reapply alpha via [`with_alpha`]).
fn lerp_col(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgb(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
    )
}

/// Lerp two `Color32`s in premultiplied space, INCLUDING alpha — so
/// fading from a glass-alpha rest fill toward an opaque accent fill
/// (GAME's full-press state) yields the correct intermediate alpha
/// during the hover transition. Use for the bg / centre-bg lerps in
/// `Button::show`.
fn lerp_col_alpha(a: egui::Color32, b: egui::Color32, t: f32) -> egui::Color32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    egui::Color32::from_rgba_premultiplied(
        blend(a.r(), b.r()),
        blend(a.g(), b.g()),
        blend(a.b(), b.b()),
        blend(a.a(), b.a()),
    )
}

/// Apply `alpha` to `solid` while preserving its straight-RGB
/// channels. Equivalent to `Color32::from_rgba_unmultiplied(r, g, b,
/// alpha)`, but spelled out so call sites read as "this colour at
/// this alpha".
fn with_alpha(solid: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), alpha)
}

// ─── Press depress + click pulse ───────────────────────────────────

/// Asymmetric press-depress amount in logical points. Animates 0 →
/// `max_px` while the response is held (60 ms), and back over 90 ms
/// on release. Multiplied by `theme.button_anim_scale` so the GAME
/// theme's longer anim envelope stays consistent. Returns 0 when
/// `theme.animations_enabled` is `false`.
///
/// Currently unused by the [`Button`] widget itself — buttons hold
/// their footprint on press so layout doesn't ripple. Kept available
/// for widgets where the "physical depression" feedback fits better
/// (e.g. tree-row collapse / expand rows that already shift indent
/// columns when toggled).
#[allow(dead_code)]
fn press_depress_amount(ctx: &egui::Context, resp_id: egui::Id, pressed: bool, max_px: f32) -> f32 {
    let th = theme();
    if !th.animations_enabled {
        return 0.0;
    }
    let scale = th.button_anim_scale.max(0.01);
    let dur = if pressed { 0.06 * scale } else { 0.09 * scale };
    let t = ctx.animate_bool_with_time(resp_id.with("mara_button_press"), pressed, dur);
    t * max_px
}

/// Concentric click-pulse — on `clicked()`, stash the click time in
/// ctx data; for the next 140 ms paint a stroke-only rect that
/// inflates from `+2 px` to `+8 px` while alpha fades 0.6 → 0.
/// Reads as the button "firing off." `painter` should already be
/// expanded past `rect` by enough margin for the maximum inflate
/// (≈ 10 px) so the pulse isn't sliced by its own clip rect.
fn paint_click_pulse(
    ctx: &egui::Context,
    painter: &egui::Painter,
    resp: &egui::Response,
    rect: egui::Rect,
    accent: egui::Color32,
    radius: egui::CornerRadius,
) {
    let th = theme();
    if !th.animations_enabled {
        return;
    }
    let click_id = resp.id.with("mara_button_click_at");
    if resp.clicked() {
        let now = ctx.input(|i| i.time);
        ctx.data_mut(|d| d.insert_temp(click_id, now));
    }
    let click_at: Option<f64> = ctx.data(|d| d.get_temp(click_id));
    if let Some(t0) = click_at {
        let now = ctx.input(|i| i.time);
        let elapsed = (now - t0) as f32;
        let pulse_dur = 0.14 * th.button_anim_scale.max(0.01);
        if elapsed < pulse_dur {
            let progress = elapsed / pulse_dur;
            let inflate = egui::lerp(2.0..=8.0, progress);
            let alpha = ((1.0 - progress) * 0.6 * 255.0).round().clamp(0.0, 255.0) as u8;
            let pulse =
                egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);
            painter.rect_stroke(
                rect.expand(inflate),
                radius,
                egui::Stroke::new(1.0, pulse),
                egui::epaint::StrokeKind::Outside,
            );
            ctx.request_repaint();
        }
    }
}

fn elided_galley(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: egui::Color32,
    max_w: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap.max_width = max_w;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job.halign = egui::Align::LEFT;
    ui.painter().layout_job(job)
}

fn paint_glyph_or_icon(
    ui: &egui::Ui,
    painter: &egui::Painter,
    pos: egui::Pos2,
    align: egui::Align2,
    glyph: &str,
    font_size: f32,
    color: egui::Color32,
) {
    if crate::icons::icon(glyph).is_some() {
        crate::icons::paint_icon(painter, pos, align, glyph, font_size, color);
    } else {
        painter.text(
            pos,
            align,
            glyph,
            egui::FontId::proportional(font_size),
            color,
        );
    }
    let _ = ui;
}

fn paint_button_contents(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    primary: egui::Color32,
    secondary: egui::Color32,
    accent: egui::Color32,
) {
    let button = theme().widgets.button;
    // ─── Glyph column ───
    let (text_left, text_right) = if let Some(g) = glyph {
        let glyph_pos = egui::pos2(
            rect.min.x + button.edge_pad + button.glyph_w * 0.5,
            rect.center().y,
        );
        paint_glyph_or_icon(
            ui,
            painter,
            glyph_pos,
            egui::Align2::CENTER_CENTER,
            g,
            button.glyph_font,
            accent,
        );
        (
            rect.min.x + button.edge_pad + button.glyph_w + button.glyph_gap,
            rect.max.x - button.edge_pad,
        )
    } else {
        (rect.min.x, rect.max.x)
    };
    let max_text_w = (text_right - text_left).max(0.0);

    // ─── Label / subtitle ───
    let cy = rect.center().y;
    if let Some(sub) = subtitle {
        let label_galley = elided_galley(
            ui,
            label,
            egui::FontId::proportional(button.label_font),
            primary,
            max_text_w,
        );
        let sub_galley = elided_galley(
            ui,
            sub,
            egui::FontId::proportional(button.subtitle_font),
            secondary,
            max_text_w,
        );
        let label_x = if glyph.is_some() {
            text_left
        } else {
            rect.center().x - label_galley.size().x * 0.5
        };
        let sub_x = if glyph.is_some() {
            text_left
        } else {
            rect.center().x - sub_galley.size().x * 0.5
        };
        painter.galley(
            egui::pos2(label_x, cy - 6.0 - label_galley.size().y * 0.5),
            label_galley,
            primary,
        );
        painter.galley(
            egui::pos2(sub_x, cy + 7.0 - sub_galley.size().y * 0.5),
            sub_galley,
            secondary,
        );
    } else {
        let label_centre = if glyph.is_some() {
            egui::pos2((text_left + text_right) * 0.5, cy)
        } else {
            rect.center()
        };
        painter.text(
            label_centre,
            egui::Align2::CENTER_CENTER,
            label,
            egui::FontId::proportional(button.label_font),
            primary,
        );
    }
}

// ─── Animation paint ───────────────────────────────────────────────

fn paint_fill(p: &egui::Painter, rect: egui::Rect, t: f32, color: egui::Color32, style: FillStyle) {
    use FillStyle::*;
    match style {
        SlideLeft => fill_slide_left(p, rect, t, color),
        Parallelogram => fill_parallelogram(p, rect, t, color),
        ParallelogramMeet => fill_parallelogram_meet(p, rect, t, color),
        Bowtie => fill_bowtie(p, rect, t, color),
        BandsMeet => fill_bands_meet(p, rect, t, color),
        CornerSquares => fill_corner_squares(p, rect, t, color),
        DiagonalTriangles => fill_diagonal_triangles(p, rect, t, color),
        CircleGrow => fill_circle_grow(p, rect, t, color),
        Equalizer => fill_equalizer(p, rect, t, color),
        HorizontalSlide => fill_horizontal_slide(p, rect, t, color),
        HorizontalSlideDelayed => fill_horizontal_slide_delayed(p, rect, t, color),
        VerticalSlideDelayed => fill_vertical_slide_delayed(p, rect, t, color),
        CrissCross => fill_criss_cross(p, rect, t, color),
    }
}

const SLANT: f32 = 12.0;

fn fill_slide_left(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let w = rect.width();
    let dx = -w * (1.0 - t);
    let r = rect.translate(egui::vec2(dx, 0.0));
    p.rect_filled(r, egui::CornerRadius::ZERO, c);
}

fn fill_parallelogram(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let total_w = rect.width() + SLANT;
    let dx = -total_w * (1.0 - t);
    let poly = vec![
        egui::pos2(rect.min.x + dx, rect.min.y),
        egui::pos2(rect.min.x + dx + total_w, rect.min.y),
        egui::pos2(rect.min.x + dx + total_w - SLANT, rect.max.y),
        egui::pos2(rect.min.x + dx - SLANT, rect.max.y),
    ];
    p.add(egui::Shape::convex_polygon(poly, c, egui::Stroke::NONE));
}

fn fill_parallelogram_meet(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let half_w = rect.width() * 0.5 + SLANT * 0.5;
    let dx_left = -half_w * (1.0 - t);
    let dx_right = half_w * (1.0 - t);
    let l = vec![
        egui::pos2(rect.min.x + dx_left, rect.min.y),
        egui::pos2(rect.min.x + dx_left + half_w, rect.min.y),
        egui::pos2(rect.min.x + dx_left + half_w - SLANT, rect.max.y),
        egui::pos2(rect.min.x + dx_left - SLANT, rect.max.y),
    ];
    let r = vec![
        egui::pos2(rect.max.x + dx_right - half_w, rect.min.y),
        egui::pos2(rect.max.x + dx_right, rect.min.y),
        egui::pos2(rect.max.x + dx_right + SLANT, rect.max.y),
        egui::pos2(rect.max.x + dx_right - half_w + SLANT, rect.max.y),
    ];
    p.add(egui::Shape::convex_polygon(l, c, egui::Stroke::NONE));
    p.add(egui::Shape::convex_polygon(r, c, egui::Stroke::NONE));
}

fn fill_bowtie(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let total_w = rect.width() * 0.51;
    let dx_l = -total_w * (1.0 - t);
    let dx_r = total_w * (1.0 - t);
    let l = vec![
        egui::pos2(rect.min.x + dx_l, rect.min.y),
        egui::pos2(rect.min.x + dx_l + total_w, rect.min.y),
        egui::pos2(rect.min.x + dx_l + total_w - SLANT, rect.max.y),
        egui::pos2(rect.min.x + dx_l, rect.max.y),
    ];
    let r = vec![
        egui::pos2(rect.max.x + dx_r - total_w + SLANT, rect.min.y),
        egui::pos2(rect.max.x + dx_r, rect.min.y),
        egui::pos2(rect.max.x + dx_r, rect.max.y),
        egui::pos2(rect.max.x + dx_r - total_w, rect.max.y),
    ];
    p.add(egui::Shape::convex_polygon(l, c, egui::Stroke::NONE));
    p.add(egui::Shape::convex_polygon(r, c, egui::Stroke::NONE));
}

fn fill_bands_meet(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let half_h = rect.height() * 0.5;
    let band_w = rect.width() * 0.7;
    let slide = band_w + half_h + 2.0;
    let dx_l = -slide * (1.0 - t);
    let dx_r = slide * (1.0 - t);
    let mid_y = rect.min.y + half_h;
    let l = vec![
        egui::pos2(rect.min.x + dx_l, rect.min.y),
        egui::pos2(rect.min.x + dx_l + band_w, rect.min.y),
        egui::pos2(rect.min.x + dx_l + band_w + half_h, mid_y),
        egui::pos2(rect.min.x + dx_l + band_w, rect.max.y),
        egui::pos2(rect.min.x + dx_l, rect.max.y),
    ];
    let r = vec![
        egui::pos2(rect.max.x + dx_r - band_w, rect.min.y),
        egui::pos2(rect.max.x + dx_r, rect.min.y),
        egui::pos2(rect.max.x + dx_r, rect.max.y),
        egui::pos2(rect.max.x + dx_r - band_w, rect.max.y),
        egui::pos2(rect.max.x + dx_r - band_w - half_h, mid_y),
    ];
    p.add(egui::Shape::convex_polygon(l, c, egui::Stroke::NONE));
    p.add(egui::Shape::convex_polygon(r, c, egui::Stroke::NONE));
}

fn fill_corner_squares(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let qw = rect.width() * 0.5;
    let qh = rect.height() * 0.5;
    let dx = qw * (1.0 - t);
    let dy = qh * (1.0 - t);
    let cx = rect.center().x;
    let cy = rect.center().y;
    let q = |x_min: f32, y_min: f32| {
        egui::Rect::from_min_size(egui::pos2(x_min, y_min), egui::vec2(qw, qh))
    };
    p.rect_filled(
        q(rect.min.x - dx, rect.min.y - dy),
        egui::CornerRadius::ZERO,
        c,
    );
    p.rect_filled(q(cx + dx, rect.min.y - dy), egui::CornerRadius::ZERO, c);
    p.rect_filled(q(rect.min.x - dx, cy + dy), egui::CornerRadius::ZERO, c);
    p.rect_filled(q(cx + dx, cy + dy), egui::CornerRadius::ZERO, c);
}

fn fill_diagonal_triangles(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let bw = rect.width() * 1.05 * t;
    let bh = rect.height() * t;
    let bl = vec![
        egui::pos2(rect.min.x, rect.max.y),
        egui::pos2(rect.min.x + bw, rect.max.y),
        egui::pos2(rect.min.x, rect.max.y - bh),
    ];
    let tr = vec![
        egui::pos2(rect.max.x, rect.min.y),
        egui::pos2(rect.max.x - bw, rect.min.y),
        egui::pos2(rect.max.x, rect.min.y + bh),
    ];
    p.add(egui::Shape::convex_polygon(bl, c, egui::Stroke::NONE));
    p.add(egui::Shape::convex_polygon(tr, c, egui::Stroke::NONE));
}

fn fill_circle_grow(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let r_max = (rect.width().powi(2) + rect.height().powi(2)).sqrt() * 0.5;
    let r = r_max * t;
    p.circle_filled(rect.center(), r, c);
}

fn fill_equalizer(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let bar_w = rect.width() * 0.25;
    let bar_h = rect.height() * t;
    for i in 0..4 {
        let x = rect.min.x + (i as f32) * bar_w;
        let bar =
            egui::Rect::from_min_size(egui::pos2(x, rect.max.y - bar_h), egui::vec2(bar_w, bar_h));
        p.rect_filled(bar, egui::CornerRadius::ZERO, c);
    }
}

fn fill_horizontal_slide(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let h = rect.height();
    let dy = h * (1.0 - t);
    let top = rect.translate(egui::vec2(0.0, -dy));
    let top_clipped = egui::Rect::from_min_max(top.min, egui::pos2(top.max.x, top.max.y - h * 0.5));
    p.rect_filled(top_clipped, egui::CornerRadius::ZERO, c);
    let bot = rect.translate(egui::vec2(0.0, dy));
    let bot_clipped = egui::Rect::from_min_max(egui::pos2(bot.min.x, bot.min.y + h * 0.5), bot.max);
    p.rect_filled(bot_clipped, egui::CornerRadius::ZERO, c);
}

fn fill_horizontal_slide_delayed(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let half_h = rect.height() * 0.5;
    let phase_a = (t * 2.0).clamp(0.0, 1.0);
    let phase_b = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
    let a_h = half_h * phase_a;
    let top_a = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), a_h));
    let bot_a = egui::Rect::from_min_size(
        egui::pos2(rect.min.x, rect.max.y - a_h),
        egui::vec2(rect.width(), a_h),
    );
    p.rect_filled(top_a, egui::CornerRadius::ZERO, c);
    p.rect_filled(bot_a, egui::CornerRadius::ZERO, c);
    if phase_b > 0.0 {
        let b_h = half_h * phase_b;
        let top_b = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + half_h - b_h),
            egui::vec2(rect.width(), b_h),
        );
        let bot_b = egui::Rect::from_min_size(
            egui::pos2(rect.min.x, rect.min.y + half_h),
            egui::vec2(rect.width(), b_h),
        );
        p.rect_filled(top_b, egui::CornerRadius::ZERO, c);
        p.rect_filled(bot_b, egui::CornerRadius::ZERO, c);
    }
}

fn fill_vertical_slide_delayed(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let half_w = rect.width() * 0.5;
    let phase_a = (t * 2.0).clamp(0.0, 1.0);
    let phase_b = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
    let a_w = half_w * phase_a;
    let l_a = egui::Rect::from_min_size(rect.min, egui::vec2(a_w, rect.height()));
    let r_a = egui::Rect::from_min_size(
        egui::pos2(rect.max.x - a_w, rect.min.y),
        egui::vec2(a_w, rect.height()),
    );
    p.rect_filled(l_a, egui::CornerRadius::ZERO, c);
    p.rect_filled(r_a, egui::CornerRadius::ZERO, c);
    if phase_b > 0.0 {
        let b_w = half_w * phase_b;
        let l_b = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + half_w - b_w, rect.min.y),
            egui::vec2(b_w, rect.height()),
        );
        let r_b = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + half_w, rect.min.y),
            egui::vec2(b_w, rect.height()),
        );
        p.rect_filled(l_b, egui::CornerRadius::ZERO, c);
        p.rect_filled(r_b, egui::CornerRadius::ZERO, c);
    }
}

fn fill_criss_cross(p: &egui::Painter, rect: egui::Rect, t: f32, c: egui::Color32) {
    let cx = rect.center().x;
    let cy = rect.center().y;
    let max_r = rect.width() * 0.85;
    let dot_r = 6.0;
    let r;
    let off_x;
    if t < 0.5 {
        let p1 = t * 2.0;
        r = dot_r;
        off_x = egui::lerp(rect.width() * 0.55..=0.0, p1);
    } else {
        let p2 = (t - 0.5) * 2.0;
        r = egui::lerp(dot_r..=max_r, p2);
        off_x = 0.0;
    }
    p.circle_filled(egui::pos2(cx - off_x, cy), r, c);
    p.circle_filled(egui::pos2(cx + off_x, cy), r, c);
}
