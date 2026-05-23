//! Small helpers around egui scroll areas.

use egui::{Id, Vec2};

#[derive(Clone, Copy)]
pub(crate) enum StickyScrollAxis {
    Horizontal,
    Vertical,
}

fn sticky_scroll_active_key() -> Id {
    Id::new("mara_sticky_scroll_active")
}

/// Show a scroll area that keeps receiving wheel deltas briefly after the
/// pointer leaves its viewport.
pub(crate) fn show_sticky_scroll_area<R>(
    ui: &mut egui::Ui,
    axis: StickyScrollAxis,
    scroll_area: egui::ScrollArea,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::containers::scroll_area::ScrollAreaOutput<R> {
    let mut output = scroll_area.show(ui, add_contents);
    let ctx = ui.ctx().clone();
    let now = ctx.input(|i| i.time);
    let pointer = ctx.pointer_latest_pos();
    let pointer_inside = pointer.is_some_and(|pos| output.inner_rect.expand(2.0).contains(pos));
    let scroll_delta = ctx.input(|i| match axis {
        StickyScrollAxis::Horizontal => i.smooth_scroll_delta.x + i.smooth_scroll_delta.y,
        StickyScrollAxis::Vertical => i.smooth_scroll_delta.y,
    });

    if pointer_inside {
        ctx.data_mut(|d| d.insert_persisted(sticky_scroll_active_key(), (output.id, now)));
    }

    let active = ctx.data_mut(|d| d.get_persisted::<(Id, f64)>(sticky_scroll_active_key()));
    let active_here = active.is_some_and(|(id, t)| id == output.id && now - t < 0.75);
    if active_here && !pointer_inside && scroll_delta.abs() > 0.0 {
        let axis_idx = match axis {
            StickyScrollAxis::Horizontal => 0,
            StickyScrollAxis::Vertical => 1,
        };
        let viewport = output.inner_rect.size();
        let max_offset = (output.content_size - viewport).max(Vec2::ZERO)[axis_idx].max(0.0);
        output.state.offset[axis_idx] =
            (output.state.offset[axis_idx] - scroll_delta).clamp(0.0, max_offset);
        output.state.store(&ctx, output.id);
        ctx.data_mut(|d| d.insert_persisted(sticky_scroll_active_key(), (output.id, now)));
        ctx.request_repaint();
    } else if scroll_delta.abs() <= 0.0
        && active.is_some_and(|(id, t)| id == output.id && now - t >= 0.75)
    {
        ctx.data_mut(|d| d.remove::<(Id, f64)>(sticky_scroll_active_key()));
    }

    output
}
