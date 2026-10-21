//! Small helpers around egui scroll areas.

use egui::Vec2;
use mara_core::vocab::Id;

#[derive(Clone, Copy)]
pub(crate) enum StickyScrollAxis {
    Horizontal,
    Vertical,
}

impl StickyScrollAxis {
    fn index(self) -> usize {
        match self {
            Self::Horizontal => 0,
            Self::Vertical => 1,
        }
    }
}

fn sticky_scroll_active_key() -> Id {
    Id::new("mara_sticky_scroll_active")
}

fn sticky_scroll_pending_key() -> Id {
    Id::new("mara_sticky_scroll_pending")
}

/// A sticky-scroll target waiting to be applied to the area with `key`.
#[derive(Clone, Copy)]
struct PendingOffset {
    key: Id,
    axis: usize,
    offset: f32,
}

/// Show a scroll area that keeps receiving wheel deltas briefly after the
/// pointer leaves its viewport.
///
/// `key` is the caller's stable Mara id for this area. It must match the
/// `id_salt` the caller gave `scroll_area`, because it is how a deferred
/// offset finds its way back to the right area.
///
/// # Why the offset is applied one frame late
///
/// Sticky scrolling has to move a scroll area's offset from the wheel
/// delta that arrives *after* the area has already been shown. egui 0.37
/// made `scroll_area::State::offset` private with no setter, so the old
/// direct write (`state.offset[axis] = ...; state.store(..)`) no longer
/// compiles. Two egui-supplied routes exist, and neither is used here:
///
/// - `Ui::scroll_with_delta` stores the delta in pass-global state. The
///   next `ScrollArea` to finish in the pass takes it, not necessarily
///   this one. Calling it after `show` could move a sibling area.
/// - Builder offsets (`ScrollArea::scroll_offset`,
///   `vertical_scroll_offset`, `horizontal_scroll_offset`) are applied
///   when an area begins, so they are scoped to one area but only
///   take effect in the next frame.
///
/// So the target is computed after `show`, stored, and applied through the
/// builder on the next frame. At 60fps the extra frame is not visible,
/// and the repaint request below ensures that frame runs.
///
/// The builder only sets one axis, so the other axis keeps its position.
///
/// # When to change this
///
/// - If egui adds an id-scoped setter or a per-area scroll delta, drop the
///   pending state and apply the delta in the same frame.
/// - Saved scroll positions are keyed by the scroll area's id. This
///   helper does not change that id, so existing positions are kept.
/// - If sticky scrolling appears to lag or to stop, check that
///   `PendingOffset` is still applied at the top of this function.
pub(crate) fn show_sticky_scroll_area<R>(
    ui: &mut egui::Ui,
    key: Id,
    axis: StickyScrollAxis,
    scroll_area: egui::ScrollArea,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::containers::scroll_area::ScrollAreaOutput<R> {
    let ctx = ui.ctx().clone();
    // Seam view of the same context for Mara-side state; an `Arc` bump.
    let seam = crate::EguiCtx::new(&ctx);

    // Apply last frame's sticky target. It is only consumed when it belongs
    // to this area, so another area's pending offset is left in place.
    let pending = mara_core::memory::MaraMemoryCtx::new(&seam)
        .get_temp::<PendingOffset>(sticky_scroll_pending_key());
    let scroll_area = match pending {
        Some(p) if p.key == key => {
            mara_core::memory::MaraMemoryCtx::new(&seam)
                .remove_temp::<PendingOffset>(sticky_scroll_pending_key());
            match p.axis {
                0 => scroll_area.horizontal_scroll_offset(p.offset),
                _ => scroll_area.vertical_scroll_offset(p.offset),
            }
        }
        _ => scroll_area,
    };

    let output = scroll_area.show(ui, add_contents);
    let now = ctx.input(|i| i.time);
    let pointer = ctx.pointer_latest_pos();
    let pointer_inside = pointer.is_some_and(|pos| output.inner_rect.expand(2.0).contains(pos));
    let scroll_delta = ctx.input(|i| match axis {
        StickyScrollAxis::Horizontal => i.smooth_scroll_delta.x + i.smooth_scroll_delta.y,
        StickyScrollAxis::Vertical => i.smooth_scroll_delta.y,
    });

    if pointer_inside {
        mara_core::memory::MaraMemoryCtx::new(&seam)
            .set_persisted(sticky_scroll_active_key(), (output.id, now));
    }

    let active = mara_core::memory::MaraMemoryCtx::new(&seam)
        .get_persisted::<(egui::Id, f64)>(sticky_scroll_active_key());
    let active_here = active.is_some_and(|(id, t)| id == output.id && now - t < 0.75);
    if active_here && !pointer_inside && scroll_delta.abs() > 0.0 {
        let axis_idx = axis.index();
        let viewport = output.inner_rect.size();
        let max_offset = (output.content_size - viewport).max(Vec2::ZERO)[axis_idx].max(0.0);
        let current = output.state.clamped_offset()[axis_idx];
        let offset = (current - scroll_delta).clamp(0.0, max_offset);
        mara_core::memory::MaraMemoryCtx::new(&seam).set_temp(
            sticky_scroll_pending_key(),
            PendingOffset {
                key,
                axis: axis_idx,
                offset,
            },
        );
        mara_core::memory::MaraMemoryCtx::new(&seam)
            .set_persisted(sticky_scroll_active_key(), (output.id, now));
        ctx.request_repaint();
    }

    output
}

