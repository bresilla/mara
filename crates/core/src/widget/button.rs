//! Mara-styled button — one widget that covers every shape the kit
//! ships:
//!
//! * **Plain** — single-row, centred label.
//! * **With subtitle** — two-row stack, primary `label` over a dim
//!   `subtitle` for card-style buttons.
//! * **With glyph** — leading icon column, label/subtitle to the
//!   right. Pair with `subtitle()` for the UE5-style "Create" cards.
//! * **With animation** — replaces the rest-state hover-fill with one
//!   of thirteen [`FillStyle`] CSS-style transitions.
//!
//! Build via [`Button::new`] and chain options. The shortcut
//! [`MaraUi::button`](crate::mui::MaraUi::button) method covers the
//! simple case (`Button::new(label).show(mui)`).
//!
//! Theme behaviour:
//!
//! * Tint fractions (rest / hover / press) come from the active theme
//!   (`button_tint_*`). PRO and GAME look right out of the box.
//! * `theme().button_full_accent_on_press` makes pressed buttons fill
//!   solid accent (GAME); plain panels keep the lerp.
//! * Animation scale is gated by `theme().animations_enabled` and
//!   stretched by `theme().button_anim_scale`.

use crate::memory::MaraAnim;
use crate::style::{
    body_accent, contrast_text_for, glass_alpha_card, pane_fill, section_fill, section_show_frame,
    surface_lift_target, theme, widget_border,
};
use crate::{
    layout::{Sense, UiBackend},
    memory::{MaraMemory, MaraMemoryCtx},
    mui::MaraResponse,
    paint::PaintCmd,
    vocab::{
        Align2, Color32 as MaraColor32, CornerRadius as MaraCornerRadius, Id as MaraId,
        Pos2 as MaraPos2, Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
    },
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

    /// Paint the action button on a sealed Mara UI and return
    /// independent body/action responses.
    pub fn show(self, ui: &mut crate::mui::MaraUi<'_>) -> ActionButtonResponse {
        let accent = ui.accent();
        self.show_backend(&mut ui.backend, accent)
    }

    /// Backend-neutral renderer — paints the action button and, if a
    /// tooltip was set, shows it through the backend's hover contract.
    pub(crate) fn show_backend<B: crate::layout::UiBackend>(
        self,
        backend: &mut B,
        accent: impl Into<MaraColor32>,
    ) -> ActionButtonResponse {
        let accent = accent.into();
        let tooltip = self.action_tooltip;
        let response = action_button_backend(
            backend,
            self.label,
            self.subtitle,
            self.glyph,
            self.action_glyph,
            self.action_armed,
            accent,
            self.height,
        );
        if let Some(tip) = tooltip {
            backend.hover_text(&response.action, tip);
        }
        response
    }

    /// egui-backend adapter retained for the pod render path, which
    /// still holds a raw `egui::Ui`.
    pub(crate) fn show_egui(
        self,
        ui: &mut egui::Ui,
        accent: impl Into<MaraColor32>,
    ) -> ActionButtonResponse {
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
        self.show_backend(&mut backend, accent)
    }
}

#[allow(clippy::too_many_arguments)]
pub fn action_button_backend(
    backend: &mut impl UiBackend,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    action_glyph: &str,
    action_armed: bool,
    accent: MaraColor32,
    height: Option<f32>,
) -> ActionButtonResponse {
    let th = theme();
    let button = th.widgets.button;
    let height = height.unwrap_or(if subtitle.is_some() {
        button.subtitle_row_h
    } else {
        button.row_h
    });
    let avail_w = backend.available_rect().width().max(0.0);
    let base_resp = backend.allocate(MaraVec2::new(avail_w, height), Sense::Hover);
    let rect = base_resp.rect;
    let action_size = (height - 2.0 * button.edge_pad.min(6.0))
        .clamp(18.0, BUTTON_ACTION_W)
        .min(rect.width().max(0.0));
    let action_center = MaraPos2::new(
        rect.max.x - button.edge_pad - action_size * 0.5,
        rect.center().y,
    );
    let action_rect = MaraRect::from_min_size(
        MaraPos2::new(
            action_center.x - action_size * 0.5,
            action_center.y - action_size * 0.5,
        ),
        MaraVec2::new(action_size, action_size),
    );
    let body_rect = MaraRect::from_min_max(
        rect.min,
        MaraPos2::new(
            (action_rect.min.x - BUTTON_ACTION_GAP).max(rect.min.x),
            rect.max.y,
        ),
    );
    let id = MaraId::new((
        "mara_action_button",
        label,
        action_glyph,
        rect.min.x.to_bits(),
        rect.min.y.to_bits(),
        rect.max.x.to_bits(),
        rect.max.y.to_bits(),
    ));
    let body = backend.interact(body_rect, id.with("body"), Sense::Click);
    let action = backend.interact(action_rect, id.with("tail"), Sense::Click);

    let body_acc = body_accent(accent);
    let base = if section_show_frame() {
        section_fill(accent)
    } else {
        pane_fill(accent)
    };
    let target = surface_lift_target(body_acc);
    let active = body.hovered || action.hovered;
    let rest_solid = lerp_col(base, target, button.tint_rest);
    let rest_bg = with_alpha(rest_solid, glass_alpha_card());
    let target_bg = if button.full_accent_on_press {
        with_alpha(body_acc, 255)
    } else {
        let press_solid = lerp_col(base, target, button.tint_press);
        with_alpha(press_solid, glass_alpha_card())
    };
    let bg = if active { target_bg } else { rest_bg };
    let border_col = if active {
        accent
    } else {
        widget_border(accent)
    };

    backend.paint(PaintCmd::RectFilled {
        rect,
        corner: MaraCornerRadius::same(th.radius_widget),
        fill: bg,
    });
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: MaraCornerRadius::same(th.radius_widget),
        stroke: MaraStroke::new(th.border_width, border_col),
    });

    let action_hover = action.hovered || action_armed;
    let action_fill = if action_hover {
        with_alpha(accent, if action_armed { 96 } else { 74 })
    } else {
        with_alpha(surface_lift_target(body_acc), glass_alpha_card())
    };
    let action_border = lerp_col(
        widget_border(accent),
        accent,
        if action_hover { 1.0 } else { 0.0 },
    );
    let action_radius = MaraCornerRadius::same((action_size * 0.5).round() as u8);
    backend.paint(PaintCmd::RectFilled {
        rect: action_rect,
        corner: action_radius,
        fill: action_fill,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: action_rect,
        corner: action_radius,
        stroke: MaraStroke::new(th.border_width, action_border),
    });

    let primary = contrast_text_for(bg);
    paint_button_contents_backend(
        backend,
        body_rect,
        label,
        subtitle,
        glyph,
        primary,
        lerp_col(primary, bg, 0.4),
        accent,
    );
    backend.paint(PaintCmd::Text {
        pos: action_rect.center(),
        anchor: Align2::CENTER_CENTER,
        text: action_glyph.to_owned(),
        size: (button.glyph_font + 2.0).max(14.0),
        color: accent,
        mono: false,
    });

    ActionButtonResponse { body, action }
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

    /// Paint the button into a sealed Mara UI and return its Mara
    /// interaction snapshot.
    pub fn show(self, ui: &mut crate::mui::MaraUi<'_>) -> MaraResponse {
        let accent = ui.accent();
        self.show_egui(ui.egui_ui(), accent)
    }

    /// Current egui-backend adapter used by first-party internals.
    pub(crate) fn show_egui(
        self,
        ui: &mut egui::Ui,
        accent: impl Into<MaraColor32>,
    ) -> MaraResponse {
        let accent = accent.into();
        let th = theme();
        let button = th.widgets.button;
        let height = self.height.unwrap_or(if self.subtitle.is_some() {
            button.subtitle_row_h
        } else {
            button.row_h
        });
        if self.animation.is_none() {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            return button_content_backend(
                &mut backend,
                self.label,
                self.subtitle,
                self.glyph,
                accent,
                height,
            );
        }
        let resp = {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            button_allocate_backend(&mut backend, height, Sense::Click)
        };

        // No press-shrink — buttons keep their footprint on hold so a
        // long press doesn't ripple layout around them. The
        // `press_depress_amount` helper is still defined below for
        // any future widget that wants the effect (the tree's
        // hide/show layering is a more natural fit).
        let painted_rect = resp.rect;

        let pressed = resp.pointer_button_down();
        let active = resp.hovered() || pressed;

        // Smoothed hover/press signal. Drives bg lerp, border lerp,
        // and the FillStyle overlay position. Same id/duration in
        // both paths so the visuals can't drift.
        let hover_t = if th.animations_enabled {
            let dur = 0.25 * th.button_anim_scale.max(0.01);
            crate::backend::egui::memory_ctx_for_ui(ui).animate_bool(
                resp.backend_response_id().with("mara_button_hover"),
                active,
                dur,
            )
        } else if active {
            1.0
        } else {
            0.0
        };
        let ctx = crate::backend::egui::context_for_ui(ui);

        {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            paint_animated_button_backend(
                &mut backend,
                painted_rect,
                self.label,
                self.subtitle,
                self.glyph,
                accent,
                self.animation
                    .expect("animated branch requires animation style"),
                hover_t,
            );
            paint_click_pulse(&ctx, &mut backend, &resp, painted_rect, accent);
        }
        resp
    }
}

/// Backend-neutral plain button renderer.
pub fn button_backend(
    backend: &mut impl UiBackend,
    label: &str,
    accent: MaraColor32,
    height: f32,
) -> MaraResponse {
    button_content_backend(backend, label, None, None, accent, height)
}

fn button_allocate_backend(
    backend: &mut impl UiBackend,
    height: f32,
    sense: Sense,
) -> MaraResponse {
    let avail_w = backend.available_rect().width().max(0.0);
    backend.allocate(MaraVec2::new(avail_w, height), sense)
}

/// Backend-neutral button renderer for plain and card-shaped button content.
pub fn button_content_backend(
    backend: &mut impl UiBackend,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    accent: MaraColor32,
    height: f32,
) -> MaraResponse {
    let th = theme();
    let button = th.widgets.button;
    let resp = button_allocate_backend(backend, height, Sense::Click);

    let body_acc = body_accent(accent);
    let base = if section_show_frame() {
        section_fill(accent)
    } else {
        pane_fill(accent)
    };
    let target = surface_lift_target(body_acc);
    let rest_solid = lerp_col(base, target, button.tint_rest);
    let rest_bg = with_alpha(rest_solid, glass_alpha_card());
    let target_bg = if button.full_accent_on_press {
        with_alpha(body_acc, 255)
    } else {
        let press_solid = lerp_col(base, target, button.tint_press);
        with_alpha(press_solid, glass_alpha_card())
    };
    let bg = if resp.hovered { target_bg } else { rest_bg };
    let border_col = if resp.hovered {
        accent
    } else {
        widget_border(accent)
    };
    let primary = contrast_text_for(bg);

    backend.paint(PaintCmd::RectFilled {
        rect: resp.rect,
        corner: MaraCornerRadius::same(th.radius_widget),
        fill: bg,
    });
    backend.paint(PaintCmd::RectStroke {
        rect: resp.rect,
        corner: MaraCornerRadius::same(th.radius_widget),
        stroke: MaraStroke::new(th.border_width, border_col),
    });
    paint_button_contents_backend(
        backend,
        resp.rect,
        label,
        subtitle,
        glyph,
        primary,
        lerp_col(primary, bg, 0.4),
        accent,
    );
    resp
}

#[allow(clippy::too_many_arguments)]
fn paint_animated_button_backend(
    backend: &mut impl UiBackend,
    rect: MaraRect,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    accent: MaraColor32,
    style: FillStyle,
    hover_t: f32,
) {
    let th = theme();
    let button = th.widgets.button;
    let radius = MaraCornerRadius::same(th.radius_widget);
    let body_acc = body_accent(accent);
    let base = if section_show_frame() {
        section_fill(accent)
    } else {
        pane_fill(accent)
    };
    let target = surface_lift_target(body_acc);
    let rest_solid = lerp_col(base, target, button.tint_rest);
    let rest_bg = with_alpha(rest_solid, glass_alpha_card());
    let target_bg = if button.full_accent_on_press {
        with_alpha(body_acc, 255)
    } else {
        let press_solid = lerp_col(base, target, button.tint_press);
        with_alpha(press_solid, glass_alpha_card())
    };

    backend.paint(PaintCmd::RectFilled {
        rect,
        corner: radius,
        fill: rest_bg,
    });
    if let Some(commands) = fill_paint_cmds(rect, hover_t, target_bg, style) {
        for cmd in commands {
            backend.paint(cmd);
        }
    }
    let centre_bg = lerp_col_alpha(rest_bg, target_bg, hover_t);
    let border_col = lerp_col(widget_border(accent), accent, hover_t);
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: radius,
        stroke: MaraStroke::new(th.border_width, border_col),
    });

    let primary = contrast_text_for(centre_bg);
    paint_button_contents_backend(
        backend,
        rect,
        label,
        subtitle,
        glyph,
        primary,
        lerp_col(primary, centre_bg, 0.4),
        accent,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_button_contents_backend(
    backend: &mut impl UiBackend,
    rect: MaraRect,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    primary: MaraColor32,
    secondary: MaraColor32,
    accent: MaraColor32,
) {
    for cmd in button_content_paint_cmds(rect, label, subtitle, glyph, primary, secondary, accent) {
        backend.paint(cmd);
    }
}

fn button_content_paint_cmds(
    rect: MaraRect,
    label: &str,
    subtitle: Option<&str>,
    glyph: Option<&str>,
    primary: MaraColor32,
    secondary: MaraColor32,
    accent: MaraColor32,
) -> Vec<PaintCmd> {
    let button = theme().widgets.button;
    let mut commands = Vec::with_capacity(if subtitle.is_some() { 3 } else { 2 });
    let (text_left, text_right) = if let Some(g) = glyph {
        if let Some(cmd) = glyph_or_icon_paint_cmd(
            MaraPos2::new(
                rect.min.x + button.edge_pad + button.glyph_w * 0.5,
                rect.center().y,
            ),
            Align2::CENTER_CENTER,
            g,
            button.glyph_font,
            accent,
        ) {
            commands.push(cmd);
        }
        (
            rect.min.x + button.edge_pad + button.glyph_w + button.glyph_gap,
            rect.max.x - button.edge_pad,
        )
    } else {
        (rect.min.x, rect.max.x)
    };
    let text_clip = MaraRect::from_min_max(
        MaraPos2::new(text_left, rect.min.y),
        MaraPos2::new(text_right.max(text_left), rect.max.y),
    );
    let cy = rect.center().y;
    if let Some(sub) = subtitle {
        let label_pos = if glyph.is_some() {
            MaraPos2::new(text_left, cy - 6.0)
        } else {
            MaraPos2::new(rect.center().x, cy - 6.0)
        };
        let anchor = if glyph.is_some() {
            Align2::LEFT_CENTER
        } else {
            Align2::CENTER_CENTER
        };
        commands.push(PaintCmd::Clip {
            rect: text_clip,
            children: vec![
                PaintCmd::Text {
                    pos: label_pos,
                    anchor,
                    text: label.to_owned(),
                    size: button.label_font,
                    color: primary,
                    mono: false,
                },
                PaintCmd::Text {
                    pos: if glyph.is_some() {
                        MaraPos2::new(text_left, cy + 7.0)
                    } else {
                        MaraPos2::new(rect.center().x, cy + 7.0)
                    },
                    anchor,
                    text: sub.to_owned(),
                    size: button.subtitle_font,
                    color: secondary,
                    mono: false,
                },
            ],
        });
    } else {
        commands.push(PaintCmd::Clip {
            rect: text_clip,
            children: vec![PaintCmd::Text {
                pos: if glyph.is_some() {
                    MaraPos2::new((text_left + text_right) * 0.5, cy)
                } else {
                    rect.center()
                },
                anchor: Align2::CENTER_CENTER,
                text: label.to_owned(),
                size: button.label_font,
                color: primary,
                mono: false,
            }],
        });
    }
    commands
}

fn glyph_or_icon_paint_cmd(
    pos: MaraPos2,
    anchor: Align2,
    glyph: &str,
    size: f32,
    color: MaraColor32,
) -> Option<PaintCmd> {
    if crate::icons::icon(glyph).is_some() {
        crate::icons::icon_paint_cmd(crate::icons::Icon::Name(glyph), pos, anchor, size, color)
    } else {
        Some(PaintCmd::Text {
            pos,
            anchor,
            text: glyph.to_owned(),
            size,
            color,
            mono: false,
        })
    }
}

/// Shortcut for [`ActionButton`] with a leading glyph, primary label,
/// subtitle, and independent tail action.
pub(crate) fn card_action_button(
    backend: &mut impl crate::layout::UiBackend,
    glyph: &str,
    name: &str,
    subtitle: &str,
    action_glyph: &str,
    action_tooltip: &str,
    accent: impl Into<MaraColor32>,
) -> ActionButtonResponse {
    ActionButton::new(name, action_glyph)
        .glyph(glyph)
        .subtitle(subtitle)
        .action_tooltip(action_tooltip)
        .show_backend(backend, accent)
}

// ─── Colour helpers ────────────────────────────────────────────────

/// Linear interpolation in straight-RGB space (alpha ignored —
/// callers should reapply alpha via [`with_alpha`]).
fn lerp_col(a: MaraColor32, b: MaraColor32, t: f32) -> MaraColor32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    MaraColor32::from_rgb(
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
fn lerp_col_alpha(a: MaraColor32, b: MaraColor32, t: f32) -> MaraColor32 {
    let t = t.clamp(0.0, 1.0);
    let blend = |x: u8, y: u8| ((x as f32) * (1.0 - t) + (y as f32) * t).round() as u8;
    MaraColor32::from_rgba_premultiplied(
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
fn with_alpha(solid: MaraColor32, alpha: u8) -> MaraColor32 {
    MaraColor32::from_rgba_unmultiplied(solid.r(), solid.g(), solid.b(), alpha)
}

// ─── Press depress + click pulse ───────────────────────────────────

/// Concentric click-pulse — on `clicked()`, stash the click time in
/// Mara memory; for the next 140 ms paint a stroke-only rect that
/// inflates from `+2 px` to `+8 px` while alpha fades 0.6 → 0.
/// Reads as the button "firing off." `painter` should already be
/// expanded past `rect` by enough margin for the maximum inflate
/// (≈ 10 px) so the pulse isn't sliced by its own clip rect.
fn paint_click_pulse(
    ctx: &egui::Context,
    backend: &mut impl UiBackend,
    resp: &MaraResponse,
    rect: MaraRect,
    accent: MaraColor32,
) {
    let th = theme();
    if !th.animations_enabled {
        return;
    }
    let click_id = button_click_pulse_memory_id(resp.backend_response_id());
    if resp.clicked {
        let now = crate::backend::egui::input_time(ctx);
        let mut memory = MaraMemoryCtx::new(ctx);
        record_button_click_pulse(&mut memory, click_id, true, now);
    }
    let click_at = {
        let memory = MaraMemoryCtx::new(ctx);
        button_click_pulse_started_at(&memory, click_id)
    };
    if let Some(t0) = click_at {
        let now = crate::backend::egui::input_time(ctx);
        let elapsed = (now - t0) as f32;
        if let Some(cmd) = button_click_pulse_paint_cmd(
            rect,
            elapsed,
            0.14 * th.button_anim_scale.max(0.01),
            accent,
            MaraCornerRadius::same(th.radius_widget),
        ) {
            backend.paint(cmd);
            crate::backend::egui::request_repaint(ctx);
        }
    }
}

fn button_click_pulse_memory_id(response_id: MaraId) -> MaraId {
    response_id.with("mara_button_click_at")
}

fn record_button_click_pulse(memory: &mut impl MaraMemory, id: MaraId, clicked: bool, now: f64) {
    if clicked {
        memory.set_temp(id, now);
    }
}

fn button_click_pulse_started_at(memory: &impl MaraMemory, id: MaraId) -> Option<f64> {
    memory.get_temp::<f64>(id)
}

fn button_click_pulse_paint_cmd(
    rect: MaraRect,
    elapsed: f32,
    duration: f32,
    accent: MaraColor32,
    radius: MaraCornerRadius,
) -> Option<PaintCmd> {
    if duration <= 0.0 || elapsed < 0.0 || elapsed >= duration {
        return None;
    }
    let progress = elapsed / duration;
    let inflate = lerp_f32(2.0, 8.0, progress);
    let alpha = ((1.0 - progress) * 0.6 * 255.0).round().clamp(0.0, 255.0) as u8;
    let pulse = MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha);
    Some(PaintCmd::RectStrokeOutside {
        rect: rect.expand(inflate),
        corner: radius,
        stroke: MaraStroke::new(1.0, pulse),
    })
}

fn lerp_f32(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

// ─── Animation paint ───────────────────────────────────────────────

const SLANT: f32 = 12.0;

/// Paint-command lowering for animated fill styles.
fn fill_paint_cmds(
    rect: MaraRect,
    t: f32,
    c: MaraColor32,
    style: FillStyle,
) -> Option<Vec<PaintCmd>> {
    use FillStyle::*;
    let poly = |points: Vec<MaraPos2>| PaintCmd::Polygon {
        points,
        fill: c,
        stroke: MaraStroke::NONE,
    };
    let filled = |rect: MaraRect| PaintCmd::RectFilled {
        rect,
        corner: 0.0.into(),
        fill: c,
    };
    match style {
        SlideLeft => {
            let w = rect.width();
            let dx = -w * (1.0 - t);
            Some(vec![filled(rect.translate(MaraVec2::new(dx, 0.0)))])
        }
        Parallelogram => {
            let total_w = rect.width() + SLANT;
            let dx = -total_w * (1.0 - t);
            Some(vec![poly(vec![
                MaraPos2::new(rect.min.x + dx, rect.min.y),
                MaraPos2::new(rect.min.x + dx + total_w, rect.min.y),
                MaraPos2::new(rect.min.x + dx + total_w - SLANT, rect.max.y),
                MaraPos2::new(rect.min.x + dx - SLANT, rect.max.y),
            ])])
        }
        ParallelogramMeet => {
            let half_w = rect.width() * 0.5 + SLANT * 0.5;
            let dx_left = -half_w * (1.0 - t);
            let dx_right = half_w * (1.0 - t);
            Some(vec![
                poly(vec![
                    MaraPos2::new(rect.min.x + dx_left, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_left + half_w, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_left + half_w - SLANT, rect.max.y),
                    MaraPos2::new(rect.min.x + dx_left - SLANT, rect.max.y),
                ]),
                poly(vec![
                    MaraPos2::new(rect.max.x + dx_right - half_w, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_right, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_right + SLANT, rect.max.y),
                    MaraPos2::new(rect.max.x + dx_right - half_w + SLANT, rect.max.y),
                ]),
            ])
        }
        Bowtie => {
            let total_w = rect.width() * 0.51;
            let dx_l = -total_w * (1.0 - t);
            let dx_r = total_w * (1.0 - t);
            Some(vec![
                poly(vec![
                    MaraPos2::new(rect.min.x + dx_l, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_l + total_w, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_l + total_w - SLANT, rect.max.y),
                    MaraPos2::new(rect.min.x + dx_l, rect.max.y),
                ]),
                poly(vec![
                    MaraPos2::new(rect.max.x + dx_r - total_w + SLANT, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_r, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_r, rect.max.y),
                    MaraPos2::new(rect.max.x + dx_r - total_w, rect.max.y),
                ]),
            ])
        }
        BandsMeet => {
            let half_h = rect.height() * 0.5;
            let band_w = rect.width() * 0.7;
            let slide = band_w + half_h + 2.0;
            let dx_l = -slide * (1.0 - t);
            let dx_r = slide * (1.0 - t);
            let mid_y = rect.min.y + half_h;
            Some(vec![
                poly(vec![
                    MaraPos2::new(rect.min.x + dx_l, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_l + band_w, rect.min.y),
                    MaraPos2::new(rect.min.x + dx_l + band_w + half_h, mid_y),
                    MaraPos2::new(rect.min.x + dx_l + band_w, rect.max.y),
                    MaraPos2::new(rect.min.x + dx_l, rect.max.y),
                ]),
                poly(vec![
                    MaraPos2::new(rect.max.x + dx_r - band_w, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_r, rect.min.y),
                    MaraPos2::new(rect.max.x + dx_r, rect.max.y),
                    MaraPos2::new(rect.max.x + dx_r - band_w, rect.max.y),
                    MaraPos2::new(rect.max.x + dx_r - band_w - half_h, mid_y),
                ]),
            ])
        }
        CornerSquares => {
            let qw = rect.width() * 0.5;
            let qh = rect.height() * 0.5;
            let dx = qw * (1.0 - t);
            let dy = qh * (1.0 - t);
            let cx = rect.center().x;
            let cy = rect.center().y;
            let q = |x_min: f32, y_min: f32| {
                MaraRect::from_min_size(MaraPos2::new(x_min, y_min), MaraVec2::new(qw, qh))
            };
            Some(vec![
                filled(q(rect.min.x - dx, rect.min.y - dy)),
                filled(q(cx + dx, rect.min.y - dy)),
                filled(q(rect.min.x - dx, cy + dy)),
                filled(q(cx + dx, cy + dy)),
            ])
        }
        DiagonalTriangles => {
            let bw = rect.width() * 1.05 * t;
            let bh = rect.height() * t;
            Some(vec![
                poly(vec![
                    MaraPos2::new(rect.min.x, rect.max.y),
                    MaraPos2::new(rect.min.x + bw, rect.max.y),
                    MaraPos2::new(rect.min.x, rect.max.y - bh),
                ]),
                poly(vec![
                    MaraPos2::new(rect.max.x, rect.min.y),
                    MaraPos2::new(rect.max.x - bw, rect.min.y),
                    MaraPos2::new(rect.max.x, rect.min.y + bh),
                ]),
            ])
        }
        CircleGrow => {
            let r_max = (rect.width().powi(2) + rect.height().powi(2)).sqrt() * 0.5;
            Some(vec![PaintCmd::CircleFilled {
                center: rect.center(),
                radius: r_max * t,
                fill: c,
            }])
        }
        Equalizer => {
            let bar_w = rect.width() * 0.25;
            let bar_h = rect.height() * t;
            Some(
                (0..4)
                    .map(|i| {
                        let x = rect.min.x + (i as f32) * bar_w;
                        filled(MaraRect::from_min_size(
                            MaraPos2::new(x, rect.max.y - bar_h),
                            MaraVec2::new(bar_w, bar_h),
                        ))
                    })
                    .collect(),
            )
        }
        HorizontalSlide => {
            let h = rect.height();
            let dy = h * (1.0 - t);
            let top = rect.translate(MaraVec2::new(0.0, -dy));
            let bot = rect.translate(MaraVec2::new(0.0, dy));
            Some(vec![
                filled(MaraRect::from_min_max(
                    top.min,
                    MaraPos2::new(top.max.x, top.max.y - h * 0.5),
                )),
                filled(MaraRect::from_min_max(
                    MaraPos2::new(bot.min.x, bot.min.y + h * 0.5),
                    bot.max,
                )),
            ])
        }
        HorizontalSlideDelayed => {
            let half_h = rect.height() * 0.5;
            let phase_a = (t * 2.0).clamp(0.0, 1.0);
            let phase_b = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
            let a_h = half_h * phase_a;
            let mut out = vec![
                filled(MaraRect::from_min_size(
                    rect.min,
                    MaraVec2::new(rect.width(), a_h),
                )),
                filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.min.x, rect.max.y - a_h),
                    MaraVec2::new(rect.width(), a_h),
                )),
            ];
            if phase_b > 0.0 {
                let b_h = half_h * phase_b;
                out.push(filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.min.x, rect.min.y + half_h - b_h),
                    MaraVec2::new(rect.width(), b_h),
                )));
                out.push(filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.min.x, rect.min.y + half_h),
                    MaraVec2::new(rect.width(), b_h),
                )));
            }
            Some(out)
        }
        VerticalSlideDelayed => {
            let half_w = rect.width() * 0.5;
            let phase_a = (t * 2.0).clamp(0.0, 1.0);
            let phase_b = ((t - 0.5) * 2.0).clamp(0.0, 1.0);
            let a_w = half_w * phase_a;
            let mut out = vec![
                filled(MaraRect::from_min_size(
                    rect.min,
                    MaraVec2::new(a_w, rect.height()),
                )),
                filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.max.x - a_w, rect.min.y),
                    MaraVec2::new(a_w, rect.height()),
                )),
            ];
            if phase_b > 0.0 {
                let b_w = half_w * phase_b;
                out.push(filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.min.x + half_w - b_w, rect.min.y),
                    MaraVec2::new(b_w, rect.height()),
                )));
                out.push(filled(MaraRect::from_min_size(
                    MaraPos2::new(rect.min.x + half_w, rect.min.y),
                    MaraVec2::new(b_w, rect.height()),
                )));
            }
            Some(out)
        }
        CrissCross => {
            let cx = rect.center().x;
            let cy = rect.center().y;
            let max_r = rect.width() * 0.85;
            let dot_r = 6.0;
            let (r, off_x) = if t < 0.5 {
                let p1 = t * 2.0;
                (dot_r, lerp_f32(rect.width() * 0.55, 0.0, p1))
            } else {
                let p2 = (t - 0.5) * 2.0;
                (lerp_f32(dot_r, max_r, p2), 0.0)
            };
            Some(vec![
                PaintCmd::CircleFilled {
                    center: MaraPos2::new(cx - off_x, cy),
                    radius: r,
                    fill: c,
                },
                PaintCmd::CircleFilled {
                    center: MaraPos2::new(cx + off_x, cy),
                    radius: r,
                    fill: c,
                },
            ])
        }
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;

    use crate::vocab::{Id, Pos2, Rect, Vec2};

    use crate::backend::record::{RecordingBackend, RecordingMemory};

    #[test]
    fn button_backend_emits_fill_stroke_and_label_commands() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, BUTTON_ROW_H)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = button_backend(&mut backend, "Apply", MaraColor32::WHITE, BUTTON_ROW_H);

        assert_eq!(response.rect.width(), 240.0);
        assert_eq!(backend.paints.len(), 3);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Clip { children, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("button should emit fill, stroke and label text commands");
        };
        let [PaintCmd::Text { text, .. }] = children.as_slice() else {
            panic!("button label should be clipped text");
        };
        assert_eq!(text, "Apply");
    }

    #[test]
    fn button_content_backend_emits_card_glyph_label_and_subtitle() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(240.0, CARD_BUTTON_ROW_H)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = button_content_backend(
            &mut backend,
            "Zone A",
            Some("root · 4 pts"),
            Some("+"),
            MaraColor32::WHITE,
            CARD_BUTTON_ROW_H,
        );

        assert_eq!(response.rect.width(), 240.0);
        assert_eq!(backend.paints.len(), 4);
        let [
            PaintCmd::RectFilled { .. },
            PaintCmd::RectStroke { .. },
            PaintCmd::Text { text: glyph, .. },
            PaintCmd::Clip { children, .. },
        ] = backend.paints.as_slice()
        else {
            panic!("card button should emit chrome, glyph, label and subtitle commands");
        };
        let [
            PaintCmd::Text { text: label, .. },
            PaintCmd::Text { text: subtitle, .. },
        ] = children.as_slice()
        else {
            panic!("card button label and subtitle should be clipped text commands");
        };
        assert_eq!(glyph, "+");
        assert_eq!(label, "Zone A");
        assert_eq!(subtitle, "root · 4 pts");
    }

    #[test]
    fn button_glyph_backend_lowers_named_icons_to_mara_text_family() {
        let cmd = glyph_or_icon_paint_cmd(
            Pos2::new(5.0, 6.0),
            Align2::CENTER_CENTER,
            "search",
            BUTTON_GLYPH_FONT,
            MaraColor32::WHITE,
        )
        .expect("search icon should lower");

        let PaintCmd::TextWithFamily { text, family, .. } = cmd else {
            panic!("named button glyphs should lower to named-font text commands");
        };
        assert_eq!(text.chars().count(), 1);
        assert!(matches!(family, crate::paint::TextFamily::Named(_)));
    }

    #[test]
    fn action_button_backend_emits_body_tail_and_action_text() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(260.0, CARD_BUTTON_ROW_H)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = action_button_backend(
            &mut backend,
            "Zone A",
            Some("root · 4 pts"),
            Some("⊕"),
            "+",
            true,
            MaraColor32::WHITE,
            None,
        );

        assert!(response.body.rect.width() < 260.0);
        assert!(response.action.rect.width() > 0.0);
        assert_eq!(backend.paints.len(), 7);
        let PaintCmd::Text {
            text: action_text, ..
        } = backend.paints.last().expect("action glyph command")
        else {
            panic!("action button should finish by painting the action glyph");
        };
        assert_eq!(action_text, "+");
    }

    #[test]
    fn animated_fill_backend_cmds_cover_all_styles_without_egui() {
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(120.0, 24.0));

        for style in [
            FillStyle::SlideLeft,
            FillStyle::Parallelogram,
            FillStyle::ParallelogramMeet,
            FillStyle::Bowtie,
            FillStyle::BandsMeet,
            FillStyle::CornerSquares,
            FillStyle::DiagonalTriangles,
            FillStyle::CircleGrow,
            FillStyle::Equalizer,
            FillStyle::HorizontalSlide,
            FillStyle::HorizontalSlideDelayed,
            FillStyle::VerticalSlideDelayed,
            FillStyle::CrissCross,
        ] {
            let commands = fill_paint_cmds(rect, 0.75, MaraColor32::WHITE, style)
                .expect("every fill style should lower to paint commands");
            assert!(!commands.is_empty(), "{style:?} should emit commands");
        }

        let slide = fill_paint_cmds(rect, 0.5, MaraColor32::WHITE, FillStyle::SlideLeft)
            .expect("slide-left should lower to paint commands");
        assert!(matches!(slide.as_slice(), [PaintCmd::RectFilled { .. }]));

        let polygon = fill_paint_cmds(rect, 0.5, MaraColor32::WHITE, FillStyle::ParallelogramMeet)
            .expect("parallelogram-meet should lower to polygon commands");
        assert!(
            polygon
                .iter()
                .all(|cmd| matches!(cmd, PaintCmd::Polygon { .. }))
        );

        let equalizer = fill_paint_cmds(rect, 0.5, MaraColor32::WHITE, FillStyle::Equalizer)
            .expect("equalizer should lower to paint commands");
        assert_eq!(equalizer.len(), 4);
        assert!(
            equalizer
                .iter()
                .all(|cmd| matches!(cmd, PaintCmd::RectFilled { .. }))
        );

        let circle = fill_paint_cmds(rect, 0.5, MaraColor32::WHITE, FillStyle::CircleGrow)
            .expect("circle-grow should lower to paint commands");
        assert!(matches!(circle.as_slice(), [PaintCmd::CircleFilled { .. }]));
    }

    #[test]
    fn animated_button_backend_emits_chrome_fill_overlay_and_content() {
        let mut backend = RecordingBackend {
            available: Rect::from_min_size(Pos2::ZERO, Vec2::new(180.0, CARD_BUTTON_ROW_H)),
            paints: Vec::new(),
            ..Default::default()
        };
        let response = button_allocate_backend(&mut backend, CARD_BUTTON_ROW_H, Sense::Click);

        paint_animated_button_backend(
            &mut backend,
            response.rect,
            "Launch",
            Some("animated"),
            Some("+"),
            MaraColor32::WHITE,
            FillStyle::CircleGrow,
            0.5,
        );

        assert_eq!(response.rect.width(), 180.0);
        assert!(
            backend
                .paints
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::CircleFilled { .. })),
            "animated fill should lower through Mara paint commands"
        );
        assert!(
            backend
                .paints
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::RectStroke { .. })),
            "animated button border should be a Mara rect stroke"
        );
        assert!(
            backend
                .paints
                .iter()
                .any(|cmd| matches!(cmd, PaintCmd::Clip { .. })),
            "animated button content should use the shared Mara content commands"
        );
    }

    #[test]
    fn click_pulse_timestamp_uses_mara_memory() {
        let mut memory = RecordingMemory::default();
        let id = button_click_pulse_memory_id(Id::new("apply-button"));

        assert_eq!(button_click_pulse_started_at(&memory, id), None);

        record_button_click_pulse(&mut memory, id, true, 12.5);

        assert_eq!(button_click_pulse_started_at(&memory, id), Some(12.5));

        record_button_click_pulse(&mut memory, id, false, 99.0);

        assert_eq!(button_click_pulse_started_at(&memory, id), Some(12.5));
    }

    #[test]
    fn click_pulse_paint_lowers_to_outside_rect_stroke_command() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 24.0));
        let cmd = button_click_pulse_paint_cmd(
            rect,
            0.07,
            0.14,
            MaraColor32::from_rgb(100, 120, 140),
            MaraCornerRadius::same(4),
        )
        .expect("pulse should paint inside its duration");

        let PaintCmd::RectStrokeOutside {
            rect: pulse_rect,
            stroke,
            ..
        } = cmd
        else {
            panic!("click pulse should lower to an outside rect stroke");
        };
        assert_eq!(pulse_rect, rect.expand(5.0));
        assert_eq!(stroke.width, 1.0);
        assert_eq!(
            stroke.color,
            MaraColor32::from_rgba_unmultiplied(100, 120, 140, 77)
        );

        assert!(
            button_click_pulse_paint_cmd(
                rect,
                0.14,
                0.14,
                MaraColor32::WHITE,
                MaraCornerRadius::same(4),
            )
            .is_none()
        );
    }
}
