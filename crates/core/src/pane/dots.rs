//! Three-dot drag handle painted between (and after) every
//! container inside a [`super::Pane`]. The "container resizer".
//!
//! Visually distinct from
//! [`crate::container::paint_separator_resize`] (the inter-pod
//! separator inside a container's body):
//!
//! * **No flanking rules** — just three dots, larger and more
//!   visible. The handle reads as a clear pane-level affordance
//!   instead of "the line that happens to have dots on it".
//! * **Bigger dots and wider hit-rect** for a comfortable drag
//!   target — pane resize is a rare deliberate gesture, the
//!   handle should be easy to grab.
//! * **Painted AFTER every container** (including the last) — the
//!   last container's handle drags its bottom edge (= the pane's
//!   bottom edge), so the pane grows with it.
//!
//! Orientation matches the container stack direction within the
//! pane: horizontal-strip panes (Top / Bottom rail middle, corner
//! Left/Right zones) stack containers along Y → handle runs
//! horizontally; vertical-strip panes stack along X → handle runs
//! vertically.

use std::hash::Hash;

use crate::vocab::Id;

use crate::container::SeparatorOrient;
use crate::{
    layout::{CursorIcon, Sense},
    mui::MaraResponse,
    paint::PaintCmd,
    style,
    vocab::{Color32 as MaraColor32, Pos2 as MaraPos2, Rect as MaraRect, Vec2 as MaraVec2},
};

/// Cross-axis hit-rect thickness for the dot handle. Bigger than
/// the inter-pod [`crate::container::separator::separator_strip_h`] so the
/// pane-level affordance is easy to grab and reads as more
/// pronounced visually.
const DOTS_STRIP_H: f32 = 6.0;
/// Centre-to-centre spacing between the three dots.
const DOTS_SPACING: f32 = 7.0;
/// Dot radius. Larger than the inter-pod
/// [`crate::container::separator`] dots (~0.9) so the pane-level
/// resize affordance reads as more substantial.
const DOTS_R: f32 = 1.7;
/// Alpha applied to [`style::outline_base`] in the rest state.
/// Higher than the inter-pod separator's `90` so the handle stays
/// visible as a proper affordance, not a whisper.
const DOTS_ALPHA: u8 = 160;

/// Paint a three-dot resize handle into `ui` and return its drag
/// `MaraResponse`. Caller is expected to read `response.drag_delta()`
/// and apply it to the container's persisted flow size.
///
/// On hover or drag the dots paint in `accent`; otherwise in
/// theme-flipped subtle ink (white-tinted on Dark themes,
/// black-tinted on Light). Cursor flips to the matching resize
/// glyph for the orientation.
pub(crate) fn paint_container_dots(
    ui: &mut crate::MaraUi<'_>,
    orient: SeparatorOrient,
    id_salt: impl Hash,
    accent: impl Into<MaraColor32>,
) -> MaraResponse {
    let accent = accent.into();
    let rect = allocate_strip(ui, orient);
    let ctx = ui.ctx();
    // Register the strip's flow-axis size with the active pane so
    // pane auto-flow accounting includes this handle in
    // the pane's outer height. Without this, the pane would be
    // sized for sum(container_body_flows) + per-container chrome
    // ONLY — the dot-handle strip per container would extend past
    // the pane's painted edge and the visible gaps between
    // containers would compress / clip variably.
    if let Some(pane_id) = ctx.memory().get_temp::<Id>(super::active_pane_key()) {
        record_container_dot_rect(ctx, pane_id, rect);
        // The strip consumes `DOTS_STRIP_H` along the pane's flow
        // axis regardless of orientation: in a horizontal-strip
        // pane (containers stack on Y), the strip is
        // `(w, DOTS_STRIP_H)` so it occupies `DOTS_STRIP_H` on Y
        // (= the flow axis); in a vertical-strip pane the strip
        // is `(DOTS_STRIP_H, h)` so it occupies `DOTS_STRIP_H` on
        // X (= the flow axis). Same value either way.
        super::publish_body_extra_flow(ctx, pane_id, DOTS_STRIP_H);
    }
    let id = ui.id().with(("mara_pane_container_dots", id_salt));
    let cursor = match orient {
        SeparatorOrient::Horizontal => CursorIcon::ResizeVertical,
        SeparatorOrient::Vertical => CursorIcon::ResizeHorizontal,
    };
    let resp = ui.interact(rect, id, Sense::Drag);
    ui.hover_cursor(&resp, cursor);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let bright = resp.hovered() || resp.dragged();
    let ink = if bright {
        accent
    } else {
        let base = style::outline_base();
        MaraColor32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), DOTS_ALPHA)
    };
    paint_dots(ui, rect, orient, ink);
    resp
}

fn dot_rects_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_container_dot_rects")
}

pub(crate) fn clear_container_dot_rects(ctx: &dyn crate::context::MaraCtx, pane_id: Id) {
    ctx.memory()
        .remove_temp::<Vec<MaraRect>>(dot_rects_key(pane_id));
}

pub(crate) fn record_container_dot_rect(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    rect: impl Into<MaraRect>,
) {
    let rect = rect.into();
    let mut memory = ctx.memory();
    let mut rects: Vec<MaraRect> = memory.get_temp(dot_rects_key(pane_id)).unwrap_or_default();
    rects.push(rect);
    memory.set_temp(dot_rects_key(pane_id), rects);
}

pub(crate) fn pointer_over_container_dots(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    pos: impl Into<MaraPos2>,
) -> bool {
    let pos = pos.into();
    {
        let memory = ctx.memory();
        memory
            .get_temp::<Vec<MaraRect>>(dot_rects_key(pane_id))
            .unwrap_or_default()
            .iter()
            .any(|rect| rect.contains(pos))
    }
}

fn allocate_strip(ui: &mut crate::MaraUi<'_>, orient: SeparatorOrient) -> MaraRect {
    let available = ui.available_rect();
    let size = match orient {
        SeparatorOrient::Horizontal => MaraVec2::new(available.width(), DOTS_STRIP_H),
        SeparatorOrient::Vertical => MaraVec2::new(DOTS_STRIP_H, available.height()),
    };
    ui.allocate(size, Sense::Hover).rect
}

fn pane_dot_paint_cmds(rect: MaraRect, orient: SeparatorOrient, ink: MaraColor32) -> [PaintCmd; 3] {
    let centre = rect.center();
    match orient {
        SeparatorOrient::Horizontal => {
            [-DOTS_SPACING, 0.0, DOTS_SPACING].map(|dx| PaintCmd::CircleFilled {
                center: MaraPos2::new(centre.x + dx, centre.y),
                radius: DOTS_R,
                fill: ink,
            })
        }
        SeparatorOrient::Vertical => {
            [-DOTS_SPACING, 0.0, DOTS_SPACING].map(|dy| PaintCmd::CircleFilled {
                center: MaraPos2::new(centre.x, centre.y + dy),
                radius: DOTS_R,
                fill: ink,
            })
        }
    }
}

fn paint_dots(
    ui: &mut crate::MaraUi<'_>,
    rect: MaraRect,
    orient: SeparatorOrient,
    ink: MaraColor32,
) {
    for cmd in pane_dot_paint_cmds(rect, orient, ink) {
        ui.paint(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_hit_rects_are_frame_local() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        let rect = MaraRect::from_min_size(MaraPos2::new(10.0, 20.0), MaraVec2::new(30.0, 6.0));

        record_container_dot_rect(&ctx, pane_id, rect);
        assert!(pointer_over_container_dots(&ctx, pane_id, rect.center()));

        clear_container_dot_rects(&ctx, pane_id);
        assert!(!pointer_over_container_dots(&ctx, pane_id, rect.center()));
    }

    #[test]
    fn dot_hit_rects_are_scoped_per_pane() {
        let ctx = egui::Context::default();
        let first = Id::new("first-pane");
        let second = Id::new("second-pane");
        let rect = MaraRect::from_min_size(MaraPos2::new(10.0, 20.0), MaraVec2::new(30.0, 6.0));

        record_container_dot_rect(&ctx, first, rect);

        assert!(pointer_over_container_dots(&ctx, first, rect.center()));
        assert!(!pointer_over_container_dots(&ctx, second, rect.center()));
    }

    #[test]
    fn pane_dot_paint_cmds_are_mara_circle_commands() {
        let rect = MaraRect::from_min_size(
            MaraPos2::new(10.0, 20.0),
            crate::vocab::Vec2::new(30.0, 6.0),
        );

        let cmds = pane_dot_paint_cmds(rect, SeparatorOrient::Horizontal, MaraColor32::WHITE);

        assert_eq!(cmds.len(), 3);
        assert!(matches!(cmds[0], PaintCmd::CircleFilled { .. }));
        assert!(matches!(cmds[1], PaintCmd::CircleFilled { .. }));
        assert!(matches!(cmds[2], PaintCmd::CircleFilled { .. }));
        if let PaintCmd::CircleFilled {
            center,
            radius,
            fill,
        } = cmds[1]
        {
            assert_eq!(center, rect.center());
            assert_eq!(radius, DOTS_R);
            assert_eq!(fill, MaraColor32::WHITE);
        }
    }
}
