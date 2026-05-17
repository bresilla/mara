//! Anchor → screen position for [`super::Pane`].
//!
//! Mirrors `maracore::floating::compute_pane_pos`'s recipe: the
//! caller decides the pane's expected size, and we compute its
//! top-left from anchor + offset + screen rect. The pane is then
//! shown in an `egui::Area` via `fixed_pos(pos)` (NOT
//! `.anchor(...)`), so positioning never lags by a frame on a
//! resize — the new size is reflected on the same frame it's
//! computed.

use egui::{Align, Align2, Pos2, Rect, Vec2, vec2};

use super::RAIL_INSET;
use super::anchor::{PaneAnchor, RailZone};

/// Pick the `Align2` + offset for an anchor. Used by
/// [`compute_pane_pos`] to translate to a top-left screen position
/// given a known content size.
pub(crate) fn anchor_align(anchor: PaneAnchor) -> (Align2, Vec2) {
    let i = RAIL_INSET;
    // Symmetric inset on every side. Each pane sits exactly
    // `RAIL_INSET` pixels in from its anchored screen edges:
    //   Left/Top corners use +i (inset INTO the screen).
    //   Right/Bottom corners use -i (inset OUT from `screen.max`).
    // No per-anchor nudges; if the alignment looks off, the fix
    // belongs in `RAIL_INSET` or in the ribbon's button placement.
    match anchor {
        PaneAnchor::LeftRail(RailZone::Start) => (Align2::LEFT_TOP, vec2(i, i)),
        PaneAnchor::LeftRail(RailZone::Middle) => (Align2::LEFT_CENTER, vec2(i, 0.0)),
        PaneAnchor::LeftRail(RailZone::End) => (Align2::LEFT_BOTTOM, vec2(i, -i)),
        PaneAnchor::RightRail(RailZone::Start) => (Align2::RIGHT_TOP, vec2(-i, i)),
        PaneAnchor::RightRail(RailZone::Middle) => (Align2::RIGHT_CENTER, vec2(-i, 0.0)),
        PaneAnchor::RightRail(RailZone::End) => (Align2::RIGHT_BOTTOM, vec2(-i, -i)),
        PaneAnchor::TopRail(RailZone::Start) => (Align2::LEFT_TOP, vec2(i, i)),
        PaneAnchor::TopRail(RailZone::Middle) => (Align2::CENTER_TOP, vec2(0.0, i)),
        PaneAnchor::TopRail(RailZone::End) => (Align2::RIGHT_TOP, vec2(-i, i)),
        PaneAnchor::BottomRail(RailZone::Start) => (Align2::LEFT_BOTTOM, vec2(i, -i)),
        PaneAnchor::BottomRail(RailZone::Middle) => (Align2::CENTER_BOTTOM, vec2(0.0, -i)),
        PaneAnchor::BottomRail(RailZone::End) => (Align2::RIGHT_BOTTOM, vec2(-i, -i)),
    }
}

/// Compute a pane's top-left position. Mirrors
/// `maracore::floating::compute_pane_pos`. `size` is the expected
/// outer dimensions of the pane; the caller computes this in-frame
/// from the animation `openness`, so the position has zero lag —
/// unlike `egui::Area::anchor()` which uses last frame's
/// `state.size`.
pub(crate) fn compute_pane_pos(align: Align2, offset: Vec2, screen: Rect, size: Vec2) -> Pos2 {
    let x = match align.x() {
        Align::Min => screen.min.x + offset.x,
        Align::Center => screen.center().x - size.x * 0.5 + offset.x,
        Align::Max => screen.max.x - size.x + offset.x,
    };
    let y = match align.y() {
        Align::Min => screen.min.y + offset.y,
        Align::Center => screen.center().y - size.y * 0.5 + offset.y,
        Align::Max => screen.max.y - size.y + offset.y,
    };
    egui::pos2(x, y)
}
