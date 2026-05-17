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

use egui::{Color32, Context, Id, Pos2, Rect, Response, Sense, Ui, vec2};

use crate::container::SeparatorOrient;
use crate::style;

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
/// `Response`. Caller is expected to read `response.drag_delta()`
/// and apply it to the container's persisted flow size.
///
/// On hover or drag the dots paint in `accent`; otherwise in
/// theme-flipped subtle ink (white-tinted on Dark themes,
/// black-tinted on Light). Cursor flips to the matching resize
/// glyph for the orientation.
pub fn paint_container_dots(
    ui: &mut Ui,
    orient: SeparatorOrient,
    id_salt: impl Hash,
    accent: Color32,
) -> Response {
    let rect = allocate_strip(ui, orient);
    // Register the strip's flow-axis size with the active pane so
    // `Pane::show`'s auto-flow accounting includes this handle in
    // the pane's outer height. Without this, the pane would be
    // sized for sum(container_body_flows) + per-container chrome
    // ONLY — the dot-handle strip per container would extend past
    // the pane's painted edge and the visible gaps between
    // containers would compress / clip variably.
    if let Some(pane_id) = ui
        .ctx()
        .data(|d| d.get_temp::<Id>(super::active_pane_key()))
    {
        record_container_dot_rect(ui.ctx(), pane_id, rect);
        // The strip consumes `DOTS_STRIP_H` along the pane's flow
        // axis regardless of orientation: in a horizontal-strip
        // pane (containers stack on Y), the strip is
        // `(w, DOTS_STRIP_H)` so it occupies `DOTS_STRIP_H` on Y
        // (= the flow axis); in a vertical-strip pane the strip
        // is `(DOTS_STRIP_H, h)` so it occupies `DOTS_STRIP_H` on
        // X (= the flow axis). Same value either way.
        super::publish_body_extra_flow(ui.ctx(), pane_id, DOTS_STRIP_H);
    }
    let id = ui.id().with(("mara_pane_container_dots", id_salt));
    let cursor = match orient {
        SeparatorOrient::Horizontal => egui::CursorIcon::ResizeVertical,
        SeparatorOrient::Vertical => egui::CursorIcon::ResizeHorizontal,
    };
    let resp = ui.interact(rect, id, Sense::drag()).on_hover_cursor(cursor);
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let bright = resp.hovered() || resp.dragged();
    let ink = if bright {
        accent
    } else {
        let base = style::outline_base();
        Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), DOTS_ALPHA)
    };
    paint_dots(ui, rect, orient, ink);
    resp
}

fn dot_rects_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_container_dot_rects")
}

pub(crate) fn clear_container_dot_rects(ctx: &Context, pane_id: Id) {
    ctx.data_mut(|d| d.remove::<Vec<Rect>>(dot_rects_key(pane_id)));
}

pub(crate) fn record_container_dot_rect(ctx: &Context, pane_id: Id, rect: Rect) {
    ctx.data_mut(|d| {
        let mut rects: Vec<Rect> = d.get_temp(dot_rects_key(pane_id)).unwrap_or_default();
        rects.push(rect);
        d.insert_temp(dot_rects_key(pane_id), rects);
    });
}

pub(crate) fn pointer_over_container_dots(ctx: &Context, pane_id: Id, pos: Pos2) -> bool {
    ctx.data(|d| {
        d.get_temp::<Vec<Rect>>(dot_rects_key(pane_id))
            .unwrap_or_default()
            .iter()
            .any(|rect| rect.contains(pos))
    })
}

fn allocate_strip(ui: &mut Ui, orient: SeparatorOrient) -> Rect {
    let size = match orient {
        SeparatorOrient::Horizontal => vec2(ui.available_width(), DOTS_STRIP_H),
        SeparatorOrient::Vertical => vec2(DOTS_STRIP_H, ui.available_height()),
    };
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    rect
}

fn paint_dots(ui: &Ui, rect: Rect, orient: SeparatorOrient, ink: Color32) {
    let centre = rect.center();
    match orient {
        SeparatorOrient::Horizontal => {
            // Three dots arranged left-to-right along the strip's
            // long axis, centred on its short-axis midline.
            for dx in [-DOTS_SPACING, 0.0, DOTS_SPACING] {
                ui.painter()
                    .circle_filled(egui::pos2(centre.x + dx, centre.y), DOTS_R, ink);
            }
        }
        SeparatorOrient::Vertical => {
            // Three dots stacked top-to-bottom.
            for dy in [-DOTS_SPACING, 0.0, DOTS_SPACING] {
                ui.painter()
                    .circle_filled(egui::pos2(centre.x, centre.y + dy), DOTS_R, ink);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dot_hit_rects_are_frame_local() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), vec2(30.0, 6.0));

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
        let rect = Rect::from_min_size(egui::pos2(10.0, 20.0), vec2(30.0, 6.0));

        record_container_dot_rect(&ctx, first, rect);

        assert!(pointer_over_container_dots(&ctx, first, rect.center()));
        assert!(!pointer_over_container_dots(&ctx, second, rect.center()));
    }
}
