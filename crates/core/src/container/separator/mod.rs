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

use crate::{
    layout::{CursorIcon, Sense, UiBackend},
    mui::MaraResponse,
    paint::PaintCmd,
    style,
    vocab::{Color32 as MaraColor32, Id, Pos2, Rect as MaraRect, Stroke, Vec2},
};

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
pub(crate) fn paint_separator(
    ui: &mut crate::MaraUi<'_>,
    style: SeparatorStyle,
    orient: SeparatorOrient,
) {
    if matches!(style, SeparatorStyle::None) {
        return;
    }
    paint_separator_backend(ui.backend_mut(), style, orient);
}

/// Interactive variant: same allocation as [`paint_separator`] but
/// with `Sense::drag` so the user can grab it. Returns the drag
/// `MaraResponse` — the caller is expected to read `response.drag_delta`
/// and apply it to whatever size value owns the neighbour above (or
/// to the left, for vertical orientation).
///
/// On hover or drag, line / dots paint in `accent`; otherwise the
/// same theme-flipped subtle ink as [`paint_separator`]. Cursor
/// flips to `ResizeVertical` for horizontal separators (drag changes
/// vertical extent) and `ResizeHorizontal` for vertical separators.
pub(crate) fn paint_separator_resize(
    ui: &mut crate::MaraUi<'_>,
    style: SeparatorStyle,
    orient: SeparatorOrient,
    id_salt: impl Hash,
    accent: impl Into<MaraColor32>,
) -> MaraResponse {
    let id = ui.id().with(("mara_separator_resize", id_salt));
    let cursor = match orient {
        SeparatorOrient::Horizontal => CursorIcon::ResizeVertical,
        SeparatorOrient::Vertical => CursorIcon::ResizeHorizontal,
    };
    let resp = paint_separator_resize_backend(ui.backend_mut(), style, orient, id, accent.into());
    ui.hover_cursor(&resp, cursor);
    resp
}

/// Backend-neutral non-interactive separator renderer.
pub fn paint_separator_backend(
    backend: &mut dyn UiBackend,
    style: SeparatorStyle,
    orient: SeparatorOrient,
) -> MaraResponse {
    let resp = allocate_strip_backend(backend, orient);
    paint_into_backend(backend, resp.rect, style, orient, default_ink());
    resp
}

/// Backend-neutral interactive separator renderer.
pub fn paint_separator_resize_backend(
    backend: &mut dyn UiBackend,
    style: SeparatorStyle,
    orient: SeparatorOrient,
    id: Id,
    accent: MaraColor32,
) -> MaraResponse {
    let strip = allocate_strip_backend(backend, orient);
    let resp = backend.interact(strip.rect, id, Sense::Drag);
    let bright = resp.hovered() || resp.dragged();
    let ink = if bright { accent } else { default_ink() };
    paint_into_backend(backend, strip.rect, style, orient, ink);
    resp
}

fn allocate_strip_backend(backend: &mut dyn UiBackend, orient: SeparatorOrient) -> MaraResponse {
    let available = backend.available_rect();
    let size = match orient {
        SeparatorOrient::Horizontal => Vec2::new(available.width().max(0.0), separator_strip_h()),
        SeparatorOrient::Vertical => Vec2::new(separator_strip_h(), available.height().max(0.0)),
    };
    backend.allocate(size, Sense::Hover)
}

/// Theme-flipped ink shared by [`paint_separator`] and the rest
/// state of [`paint_separator_resize`]. Pulls from the active
/// theme's `border_subtle` so inter-pod separators match the
/// hairline divider painted under each container's title — the
/// two horizontal rules in a section read as the same family.
/// Alpha is theme-driven (`separator_alpha`): PRO 128 keeps the
/// rule visible against the dark panel; GAME 64 lets it whisper
/// across the bright accent surface.
fn default_ink() -> MaraColor32 {
    let base = style::theme().border_subtle;
    MaraColor32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), separator_alpha())
}

fn paint_into_backend(
    backend: &mut dyn UiBackend,
    rect: MaraRect,
    style: SeparatorStyle,
    orient: SeparatorOrient,
    ink: MaraColor32,
) {
    let stroke = Stroke::new(RULE_W, ink);
    match orient {
        SeparatorOrient::Horizontal => paint_horizontal_backend(backend, rect, style, ink, stroke),
        SeparatorOrient::Vertical => paint_vertical_backend(backend, rect, style, ink, stroke),
    }
}

fn paint_horizontal_backend(
    backend: &mut dyn UiBackend,
    rect: MaraRect,
    style: SeparatorStyle,
    ink: MaraColor32,
    stroke: Stroke,
) {
    let mid_y = rect.center().y;
    match style {
        SeparatorStyle::None => {}
        SeparatorStyle::Line => {
            backend.paint(PaintCmd::Line {
                a: Pos2::new(rect.left() + EDGE_INSET, mid_y),
                b: Pos2::new(rect.right() - EDGE_INSET, mid_y),
                stroke,
            });
        }
        SeparatorStyle::LineDots => {
            let mid_x = rect.center().x;
            for dx in [-DOT_SPACING, 0.0, DOT_SPACING] {
                backend.paint(PaintCmd::CircleFilled {
                    center: Pos2::new(mid_x + dx, mid_y),
                    radius: DOT_R,
                    fill: ink,
                });
            }
            let half = DOT_SPACING + DOT_R + GRIP_HALF_GAP;
            backend.paint(PaintCmd::Line {
                a: Pos2::new(rect.left() + EDGE_INSET, mid_y),
                b: Pos2::new(mid_x - half, mid_y),
                stroke,
            });
            backend.paint(PaintCmd::Line {
                a: Pos2::new(mid_x + half, mid_y),
                b: Pos2::new(rect.right() - EDGE_INSET, mid_y),
                stroke,
            });
        }
    }
}

fn paint_vertical_backend(
    backend: &mut dyn UiBackend,
    rect: MaraRect,
    style: SeparatorStyle,
    ink: MaraColor32,
    stroke: Stroke,
) {
    let mid_x = rect.center().x;
    match style {
        SeparatorStyle::None => {}
        SeparatorStyle::Line => {
            backend.paint(PaintCmd::Line {
                a: Pos2::new(mid_x, rect.top() + EDGE_INSET),
                b: Pos2::new(mid_x, rect.bottom() - EDGE_INSET),
                stroke,
            });
        }
        SeparatorStyle::LineDots => {
            let mid_y = rect.center().y;
            for dy in [-DOT_SPACING, 0.0, DOT_SPACING] {
                backend.paint(PaintCmd::CircleFilled {
                    center: Pos2::new(mid_x, mid_y + dy),
                    radius: DOT_R,
                    fill: ink,
                });
            }
            let half = DOT_SPACING + DOT_R + GRIP_HALF_GAP;
            backend.paint(PaintCmd::Line {
                a: Pos2::new(mid_x, rect.top() + EDGE_INSET),
                b: Pos2::new(mid_x, mid_y - half),
                stroke,
            });
            backend.paint(PaintCmd::Line {
                a: Pos2::new(mid_x, mid_y + half),
                b: Pos2::new(mid_x, rect.bottom() - EDGE_INSET),
                stroke,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::backend::record::RecordingBackend;

    #[test]
    fn separator_backend_emits_horizontal_line() {
        let mut backend = RecordingBackend {
            available: MaraRect::from_min_size(Pos2::ZERO, Vec2::new(160.0, 32.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = paint_separator_backend(
            &mut backend,
            SeparatorStyle::Line,
            SeparatorOrient::Horizontal,
        );

        assert_eq!(response.rect.width(), 160.0);
        let [PaintCmd::Line { a, b, .. }] = backend.paints.as_slice() else {
            panic!("line separator should emit one line command");
        };
        assert_eq!(a.y, b.y);
    }

    #[test]
    fn separator_resize_backend_emits_dots_and_flanking_rules() {
        let mut backend = RecordingBackend {
            available: MaraRect::from_min_size(Pos2::ZERO, Vec2::new(160.0, 32.0)),
            paints: Vec::new(),
            ..Default::default()
        };

        let response = paint_separator_resize_backend(
            &mut backend,
            SeparatorStyle::LineDots,
            SeparatorOrient::Vertical,
            Id::new("resize"),
            MaraColor32::WHITE,
        );

        assert_eq!(response.rect.height(), 32.0);
        assert_eq!(backend.paints.len(), 5);
        assert!(
            backend
                .paints
                .iter()
                .take(3)
                .all(|cmd| matches!(cmd, PaintCmd::CircleFilled { .. }))
        );
        assert!(
            backend
                .paints
                .iter()
                .skip(3)
                .all(|cmd| matches!(cmd, PaintCmd::Line { .. }))
        );
    }
}
