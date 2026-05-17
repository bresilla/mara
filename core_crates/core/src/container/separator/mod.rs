//! Mara-styled separator. Two style variants × two orientations:
//!
//! * [`SeparatorStyle::Line`] — thin hairline rule. Pure visual cue,
//!   no interaction.
//! * [`SeparatorStyle::LineDots`] — short rule on each side of three
//!   centred dots. Same visual rhythm as the line variant; doubles
//!   as a drag handle when painted via [`paint_separator_resize`].
//! * [`SeparatorOrient::Horizontal`] — line runs left↔right; used
//!   between vertically-stacked things (pods inside a container's
//!   body, containers inside a horizontal-strip pane).
//! * [`SeparatorOrient::Vertical`] — line runs top↕bottom; used
//!   between horizontally-stacked things (containers inside a
//!   vertical-strip pane).
//!
//! The strip allocated in the parent ui is always [`separator_strip_h`]
//! pixels thick on the cross axis, regardless of style or
//! orientation, so swapping any combination doesn't shift
//! neighbouring positions. Ink colour comes from
//! [`crate::style::outline_base`] — theme-luma-flipped, so the
//! separator reads as a faint contrasting whisper on both Dark and
//! Light themes.

use std::hash::Hash;

use egui::{Color32, Rect, Response, Sense, Stroke, Ui, vec2};

use crate::style;

/// Alpha applied to the title-divider colour when painting the
/// separator. Theme-driven via
/// [`crate::style::Theme::section_separator_alpha`]: PRO holds
/// the original half-strength 128 (a quiet but readable rule),
/// GAME halves again to 64 so the inter-pod whisper barely
/// registers against the bright accent panel.
fn separator_alpha() -> u8 {
    crate::style::theme().section_separator_alpha
}

/// Cross-axis strip thickness — the rect EVERY separator reserves
/// in the parent ui, every style and orientation. Theme-driven via
/// [`crate::style::Theme::section_separator_strip_h`] so PRO keeps
/// a 2 px hairline strip while GAME pads the rule with breathing
/// room above and below (rule is centred in the strip, so a 14 px
/// strip leaves ~6 px of vertical pad on each side of the line).
/// Same value for interactive and non-interactive variants so
/// swapping doesn't shift neighbours.
pub fn separator_strip_h() -> f32 {
    crate::style::theme().section_separator_strip_h
}
/// Centre-to-centre spacing between the three dots in
/// [`SeparatorStyle::LineDots`].
const DOT_SPACING: f32 = 5.0;
/// Dot radius. Tuned so a three-dot diameter (`2 * DOT_R`) reads
/// proportionate to the 1-px stroke width of the flanking rules.
const DOT_R: f32 = 0.9;
/// Stroke width for the line / flanking rules. Same width every
/// other mara surface uses for its outline so the separator reads
/// as part of the same border family.
const RULE_W: f32 = 1.0;
/// Inset from the edge of the parent ui where the line / flanking
/// rules begin. Effectively zero — the rule runs the full width
/// of the strip the parent allocated, so no breathing gap remains
/// between the rule's ends and whatever sits on either side.
const EDGE_INSET: f32 = 0.0;
/// Gap between the dot cluster and the start of each flanking rule
/// in the `LineDots` variant — keeps the dots and the rules from
/// touching.
const GRIP_HALF_GAP: f32 = 3.0;

/// Visual style for a separator. See module docs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SeparatorStyle {
    /// No separator — neighbours sit flush.
    None,
    /// Plain thin hairline.
    #[default]
    Line,
    /// Hairline + three centred dots + hairline. Doubles as a drag
    /// handle when painted via [`paint_separator_resize`].
    LineDots,
}

/// Which axis the separator's line runs along.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub enum SeparatorOrient {
    /// Line runs left↔right; used between vertically-stacked items.
    #[default]
    Horizontal,
    /// Line runs top↕bottom; used between horizontally-stacked items.
    Vertical,
}

/// Paint a non-interactive separator into the parent `ui`.
///
/// Strip thickness on the cross axis is [`separator_strip_h`];
/// length on the main axis is the parent ui's `available_*`. Colour
/// comes from [`style::outline_base`], which auto-flips per theme
/// luma — white-tinted on Dark, black-tinted on Light.
pub fn paint_separator(ui: &mut Ui, style: SeparatorStyle, orient: SeparatorOrient) {
    if matches!(style, SeparatorStyle::None) {
        return;
    }
    let rect = allocate_strip(ui, orient);
    if !ui.is_rect_visible(rect) {
        return;
    }
    paint_into(ui, rect, style, orient, default_ink());
}

/// Interactive variant: same allocation as [`paint_separator`] but
/// with `Sense::drag` so the user can grab it. Returns the drag
/// `Response` — the caller is expected to read `response.drag_delta()`
/// and apply it to whatever size value owns the neighbour above (or
/// to the left, for vertical orientation).
///
/// On hover or drag, line / dots paint in `accent`; otherwise the
/// same theme-flipped subtle ink as [`paint_separator`]. Cursor
/// flips to `ResizeVertical` for horizontal separators (drag changes
/// vertical extent) and `ResizeHorizontal` for vertical separators.
pub fn paint_separator_resize(
    ui: &mut Ui,
    style: SeparatorStyle,
    orient: SeparatorOrient,
    id_salt: impl Hash,
    accent: Color32,
) -> Response {
    let rect = allocate_strip(ui, orient);
    let id = ui.id().with(("mara_separator_resize", id_salt));
    let cursor = match orient {
        SeparatorOrient::Horizontal => egui::CursorIcon::ResizeVertical,
        SeparatorOrient::Vertical => egui::CursorIcon::ResizeHorizontal,
    };
    let resp = ui.interact(rect, id, Sense::drag()).on_hover_cursor(cursor);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let bright = resp.hovered() || resp.dragged();
    let ink = if bright { accent } else { default_ink() };
    paint_into(ui, rect, style, orient, ink);
    resp
}

/// Allocate the strip rect — the cross axis is `separator_strip_h`,
/// the main axis is `available_width()` / `available_height()`.
/// Reserved with `Sense::hover` so `allocate_exact_size`'s auto-id
/// doesn't claim the interaction id; the explicit `interact` call
/// in [`paint_separator_resize`] owns the drag id under the
/// caller-supplied salt.
fn allocate_strip(ui: &mut Ui, orient: SeparatorOrient) -> Rect {
    let size = match orient {
        SeparatorOrient::Horizontal => vec2(ui.available_width(), separator_strip_h()),
        SeparatorOrient::Vertical => vec2(separator_strip_h(), ui.available_height()),
    };
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    rect
}

/// Theme-flipped ink shared by [`paint_separator`] and the rest
/// state of [`paint_separator_resize`]. Pulls from the active
/// theme's `border_subtle` so inter-pod separators match the
/// hairline divider painted under each container's title — the
/// two horizontal rules in a section read as the same family.
/// Alpha is theme-driven (`separator_alpha`): PRO 128 keeps the
/// rule visible against the dark panel; GAME 64 lets it whisper
/// across the bright accent surface.
fn default_ink() -> Color32 {
    let base = style::theme().border_subtle;
    Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), separator_alpha())
}

fn paint_into(ui: &Ui, rect: Rect, style: SeparatorStyle, orient: SeparatorOrient, ink: Color32) {
    let stroke = Stroke::new(RULE_W, ink);
    match orient {
        SeparatorOrient::Horizontal => paint_horizontal(ui, rect, style, ink, stroke),
        SeparatorOrient::Vertical => paint_vertical(ui, rect, style, ink, stroke),
    }
}

fn paint_horizontal(ui: &Ui, rect: Rect, style: SeparatorStyle, ink: Color32, stroke: Stroke) {
    let mid_y = rect.center().y;
    match style {
        SeparatorStyle::None => {}
        SeparatorStyle::Line => {
            ui.painter().hline(
                (rect.left() + EDGE_INSET)..=(rect.right() - EDGE_INSET),
                mid_y,
                stroke,
            );
        }
        SeparatorStyle::LineDots => {
            let mid_x = rect.center().x;
            for dx in [-DOT_SPACING, 0.0, DOT_SPACING] {
                ui.painter()
                    .circle_filled(egui::pos2(mid_x + dx, mid_y), DOT_R, ink);
            }
            let half = DOT_SPACING + DOT_R + GRIP_HALF_GAP;
            ui.painter()
                .hline((rect.left() + EDGE_INSET)..=(mid_x - half), mid_y, stroke);
            ui.painter()
                .hline((mid_x + half)..=(rect.right() - EDGE_INSET), mid_y, stroke);
        }
    }
}

fn paint_vertical(ui: &Ui, rect: Rect, style: SeparatorStyle, ink: Color32, stroke: Stroke) {
    let mid_x = rect.center().x;
    match style {
        SeparatorStyle::None => {}
        SeparatorStyle::Line => {
            ui.painter().vline(
                mid_x,
                (rect.top() + EDGE_INSET)..=(rect.bottom() - EDGE_INSET),
                stroke,
            );
        }
        SeparatorStyle::LineDots => {
            let mid_y = rect.center().y;
            for dy in [-DOT_SPACING, 0.0, DOT_SPACING] {
                ui.painter()
                    .circle_filled(egui::pos2(mid_x, mid_y + dy), DOT_R, ink);
            }
            let half = DOT_SPACING + DOT_R + GRIP_HALF_GAP;
            ui.painter()
                .vline(mid_x, (rect.top() + EDGE_INSET)..=(mid_y - half), stroke);
            ui.painter()
                .vline(mid_x, (mid_y + half)..=(rect.bottom() - EDGE_INSET), stroke);
        }
    }
}
