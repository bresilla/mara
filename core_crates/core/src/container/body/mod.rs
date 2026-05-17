//! Shared **body** layout helper for [`super::Normal`] and (later)
//! `super::tabbed`.
//!
//! Inside [`Body::paint`]:
//!
//! 1. Snapshot the parent ui's available rect and pre-allocate that
//!    exact size via `allocate_ui_with_layout` — gives a fixed-size
//!    slot for the inner [`egui::ScrollArea`] to fill.
//! 2. Clamp the inner ui's CROSS axis to `span_inner` so widgets
//!    that call `ui.available_width()` / `available_height()` see a
//!    stable value across the layout's measurement passes.
//! 3. Wrap the user's body closure in a *vertical* `ScrollArea`.
//!    The body always forces `top_down` so pods stack vertically
//!    regardless of which side the container's title is on, so the
//!    scroll axis is always vertical too — there's no case where
//!    a horizontal scrollbar would help reach hidden pod content.

use egui::Ui;

#[derive(Copy, Clone, Debug)]
pub struct Body {
    /// `true` when the parent container's title strip runs along
    /// the X axis (Top/Bottom title) — body's span axis is X.
    /// `false` when the strip runs along Y (Left/Right title) —
    /// body's span axis is Y.
    pub horizontal_strip: bool,
    /// Pane's locked span-axis size in pixels. Width when
    /// `horizontal_strip`, height otherwise.
    pub span_inner: f32,
    /// Optional cap on the body's flow-axis size — used by
    /// vertical-strip containers to keep total width within the
    /// caller's `CONTAINER_MAX_WIDTH`.
    pub max_flow: Option<f32>,
}

impl Body {
    pub fn new(horizontal_strip: bool, span_inner: f32) -> Self {
        Self {
            horizontal_strip,
            span_inner,
            max_flow: None,
        }
    }

    /// Cap the body's flow axis (the dim perpendicular to the
    /// title strip).
    pub fn max_flow(mut self, max: f32) -> Self {
        self.max_flow = Some(max);
        self
    }

    /// Pre-allocate a fixed-size rect for the body slot, clamp the
    /// cross axis, and wrap `body` in a vertical [`egui::ScrollArea`].
    ///
    /// Two non-obvious settings make scrolling work for the small
    /// body slots a stack of pods produces:
    ///
    /// * `allocate_ui_with_layout(slot_size, …)` instead of letting
    ///   the ScrollArea derive its own size from
    ///   `available_rect_before_wrap`. The latter inside a Frame
    ///   whose content_ui's `max_rect` extends past the
    ///   openness-clipped visible area returns an inflated value,
    ///   and `max_offset = content_size - viewport_size` ends up
    ///   wrong.
    /// * `min_scrolled_height(0.0)` to disable ScrollArea's default
    ///   `min_scrolled_size = 64`. With the default in place, when
    ///   the actual slot is smaller than 64 the ScrollArea inflates
    ///   `inner_size` to 64; the visible viewport stays clipped to
    ///   the real slot but the scroll max_offset is computed against
    ///   the inflated 64, so the user can scroll content past the
    ///   bottom of the visible area and the last pod ends up
    ///   half-cut below.
    ///
    /// Cross-axis clamping: for horizontal-strip containers we
    /// `set_max_width(span_inner)` (the container's locked width);
    /// for vertical-strip we `set_max_width(span_inner)` too, since
    /// `span_inner` IS the cross axis from the body's perspective
    /// regardless — the body's main axis is always Y because we
    /// force `top_down`. (The legacy `max_flow` cap is kept for
    /// callers that need to bound the body's perpendicular extent.)
    pub fn paint<R>(&self, ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> (R, f32) {
        let slot_size = ui.available_rect_before_wrap().size();
        let span_inner = self.span_inner;
        let horizontal_strip = self.horizontal_strip;
        let max_flow = self.max_flow;
        ui.allocate_ui_with_layout(
            slot_size,
            egui::Layout::top_down(egui::Align::Min),
            move |ui| {
                if horizontal_strip {
                    ui.set_max_width(span_inner);
                } else {
                    ui.set_max_height(span_inner);
                    if let Some(m) = max_flow {
                        ui.set_max_width(m);
                    }
                }
                // `ScrollBarVisibility::AlwaysHidden` — the
                // container's auto-fit path now grows to the body's
                // intrinsic content height (no `CONTAINER_AUTOFIT_CAP`
                // applied), so content always fits and a visible
                // scrollbar would be redundant. Worse, allowing the
                // bar to auto-toggle creates a 1-frame oscillation
                // when widgets expand on demand (color picker,
                // dropdown popup, …): bar appears → reserves width →
                // content reflows → measured intrinsic changes →
                // container grows → bar disappears → repeat. Hiding
                // the bar breaks the loop. Past `CONTAINER_MAX_FLOW`
                // (= 1200 px) content gets clipped instead of
                // scrolled — that's the intended escape hatch (the
                // user can drag the container's resize handle for
                // more space, or the caller can split the section).
                // Wrap the user body so we can append the
                // theme's body-end inner pad (see
                // `Theme::section_body_inner_end_pad`) AFTER the
                // last pod renders. Inside the ScrollArea so the
                // trailing space contributes to the scroll's
                // measured `content_size` and the container
                // auto-fits to include it.
                let end_pad = crate::style::theme().section_body_inner_end_pad;
                let scroll = egui::ScrollArea::vertical()
                    .id_salt("mara_body_scroll_v")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        let r = body(ui);
                        if end_pad > 0.0 {
                            ui.add_space(end_pad);
                        }
                        r
                    });
                // ScrollArea reports its inner content's natural
                // size. The caller persists this so the container
                // can auto-fit on the next frame (see
                // `crate::container::record_container_intrinsic`).
                (scroll.inner, scroll.content_size.y)
            },
        )
        .inner
    }
}
