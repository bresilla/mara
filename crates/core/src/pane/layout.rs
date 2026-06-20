//! Anchor → screen position for [`super::Pane`].
//!
//! Mirrors the established floating-pane positioning recipe: the
//! caller decides the pane's expected size, and we compute its
//! top-left from anchor + offset + screen rect. The pane is then
//! shown in the current egui backend via `fixed_pos(pos)` (NOT
//! `.anchor(...)`), so positioning never lags by a frame on a
//! resize — the new size is reflected on the same frame it's
//! computed.

use super::RAIL_INSET;
use super::anchor::{PaneAnchor, RailZone};
use crate::vocab::{Align2 as MaraAlign2, Pos2 as MaraPos2, Rect as MaraRect, Vec2 as MaraVec2};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AxisAlign {
    Min,
    Center,
    Max,
}

/// Backend-neutral placement result for a floating pane.
///
/// The pane renderer can still hand this to the egui backend today,
/// but the anchoring decision itself is Mara-owned: callers provide
/// screen bounds and an outer pane size in Mara vocabulary and get a
/// Mara position/rect back. This keeps the zero-lag anchor math out
/// of egui's `Area` state model.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PanePlacement {
    pub pos: MaraPos2,
    pub size: MaraVec2,
    pub rect: MaraRect,
}

impl PanePlacement {
    #[must_use]
    pub(crate) fn new(
        align: MaraAlign2,
        offset: MaraVec2,
        screen: MaraRect,
        size: MaraVec2,
    ) -> Self {
        let pos = compute_pane_pos(align, offset, screen, size);
        let rect = MaraRect::from_min_size(pos, size);
        Self { pos, size, rect }
    }
}

/// Pick the `Align2` + offset for an anchor. Used by
/// [`compute_pane_pos`] to translate to a top-left screen position
/// given a known content size.
pub(crate) fn anchor_align(anchor: PaneAnchor) -> (MaraAlign2, MaraVec2) {
    let i = RAIL_INSET;
    // Symmetric inset on every side. Each pane sits exactly
    // `RAIL_INSET` pixels in from its anchored screen edges:
    //   Left/Top corners use +i (inset INTO the screen).
    //   Right/Bottom corners use -i (inset OUT from `screen.max`).
    // No per-anchor nudges; if the alignment looks off, the fix
    // belongs in `RAIL_INSET` or in the ribbon's button placement.
    match anchor {
        PaneAnchor::LeftRail(RailZone::Start) => (MaraAlign2::LEFT_TOP, MaraVec2::new(i, i)),
        PaneAnchor::LeftRail(RailZone::Middle) => (MaraAlign2::LEFT_CENTER, MaraVec2::new(i, 0.0)),
        PaneAnchor::LeftRail(RailZone::End) => (MaraAlign2::LEFT_BOTTOM, MaraVec2::new(i, -i)),
        PaneAnchor::RightRail(RailZone::Start) => (MaraAlign2::RIGHT_TOP, MaraVec2::new(-i, i)),
        PaneAnchor::RightRail(RailZone::Middle) => {
            (MaraAlign2::RIGHT_CENTER, MaraVec2::new(-i, 0.0))
        }
        PaneAnchor::RightRail(RailZone::End) => (MaraAlign2::RIGHT_BOTTOM, MaraVec2::new(-i, -i)),
        PaneAnchor::TopRail(RailZone::Start) => (MaraAlign2::LEFT_TOP, MaraVec2::new(i, i)),
        PaneAnchor::TopRail(RailZone::Middle) => (MaraAlign2::CENTER_TOP, MaraVec2::new(0.0, i)),
        PaneAnchor::TopRail(RailZone::End) => (MaraAlign2::RIGHT_TOP, MaraVec2::new(-i, i)),
        PaneAnchor::BottomRail(RailZone::Start) => (MaraAlign2::LEFT_BOTTOM, MaraVec2::new(i, -i)),
        PaneAnchor::BottomRail(RailZone::Middle) => {
            (MaraAlign2::CENTER_BOTTOM, MaraVec2::new(0.0, -i))
        }
        PaneAnchor::BottomRail(RailZone::End) => (MaraAlign2::RIGHT_BOTTOM, MaraVec2::new(-i, -i)),
    }
}

pub(crate) fn anchor_span_align(anchor: PaneAnchor, horizontal_strip: bool) -> AxisAlign {
    let (align, _) = anchor_align(anchor);
    if horizontal_strip {
        align_x(align)
    } else {
        align_y(align)
    }
}

/// Convert the pane's span/flow axes into a backend-neutral outer
/// size. Horizontal title strips flow vertically; vertical title
/// strips flow horizontally.
#[must_use]
pub(crate) const fn pane_outer_size(
    horizontal_strip: bool,
    span_outer: f32,
    pane_flow: f32,
) -> MaraVec2 {
    if horizontal_strip {
        MaraVec2::new(span_outer, pane_flow)
    } else {
        MaraVec2::new(pane_flow, span_outer)
    }
}

/// Compute a pane's top-left position. `size` is the expected
/// outer dimensions of the pane; the caller computes this in-frame
/// from the animation `openness`, so the position has zero lag —
/// unlike `egui::Area::anchor()` which uses last frame's
/// `state.size`.
pub(crate) fn compute_pane_pos(
    align: MaraAlign2,
    offset: MaraVec2,
    screen: MaraRect,
    size: MaraVec2,
) -> MaraPos2 {
    let x = match align_x(align) {
        AxisAlign::Min => screen.min.x + offset.x,
        AxisAlign::Center => screen.center().x - size.x * 0.5 + offset.x,
        AxisAlign::Max => screen.max.x - size.x + offset.x,
    };
    let y = match align_y(align) {
        AxisAlign::Min => screen.min.y + offset.y,
        AxisAlign::Center => screen.center().y - size.y * 0.5 + offset.y,
        AxisAlign::Max => screen.max.y - size.y + offset.y,
    };
    MaraPos2::new(x, y)
}

fn align_x(align: MaraAlign2) -> AxisAlign {
    if matches!(
        align,
        MaraAlign2::LEFT_TOP | MaraAlign2::LEFT_CENTER | MaraAlign2::LEFT_BOTTOM
    ) {
        AxisAlign::Min
    } else if matches!(
        align,
        MaraAlign2::RIGHT_TOP | MaraAlign2::RIGHT_CENTER | MaraAlign2::RIGHT_BOTTOM
    ) {
        AxisAlign::Max
    } else {
        AxisAlign::Center
    }
}

fn align_y(align: MaraAlign2) -> AxisAlign {
    if matches!(
        align,
        MaraAlign2::LEFT_TOP | MaraAlign2::CENTER_TOP | MaraAlign2::RIGHT_TOP
    ) {
        AxisAlign::Min
    } else if matches!(
        align,
        MaraAlign2::LEFT_BOTTOM | MaraAlign2::CENTER_BOTTOM | MaraAlign2::RIGHT_BOTTOM
    ) {
        AxisAlign::Max
    } else {
        AxisAlign::Center
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pane::{PaneAnchor, RailZone};

    #[test]
    fn pane_outer_size_maps_span_and_flow_without_backend_types() {
        assert_eq!(
            pane_outer_size(true, 320.0, 96.0),
            MaraVec2::new(320.0, 96.0)
        );
        assert_eq!(
            pane_outer_size(false, 320.0, 96.0),
            MaraVec2::new(96.0, 320.0)
        );
    }

    #[test]
    fn pane_placement_keeps_right_bottom_anchor_pinned_to_current_size() {
        let screen = MaraRect::from_min_size(MaraPos2::ZERO, MaraVec2::new(800.0, 600.0));
        let (align, offset) = anchor_align(PaneAnchor::RightRail(RailZone::End));
        let size = MaraVec2::new(200.0, 100.0);

        let placement = PanePlacement::new(align, offset, screen, size);

        assert_eq!(placement.size, size);
        assert_eq!(
            placement.pos,
            MaraPos2::new(
                800.0 - super::super::RAIL_INSET - 200.0,
                600.0 - super::super::RAIL_INSET - 100.0
            )
        );
        assert_eq!(placement.rect, MaraRect::from_min_size(placement.pos, size));
    }
}
