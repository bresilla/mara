//! Pan and zoom — PLAN.md WS-E1.4 (transform half).
//!
//! A [`Transform`] is a uniform scale plus a translation: the whole of
//! what a pannable, zoomable canvas needs. [`PanZoom`] turns pointer
//! input into one.
//!
//! ## Why it exists
//!
//! `mara_graph` puts its nodes inside a transformed layer and drives it
//! with the backend's pan/zoom helper. That is the last capability
//! blocking its renderer port: layout landed with `child_at` and
//! friends, drawing landed with `MaraPainter`, but nothing in the
//! sealed surface could say "this region is panned and zoomed".
//!
//! It is **not** the same thing as [`crate::view::ViewCtx::offscreen`],
//! which rasterises a subtree at an independent scale into a texture.
//! Offscreen keeps text crisp at high zoom; a transform moves and
//! scales a live region. A zoomable editor typically wants both.
//!
//! ## Coordinate spaces
//!
//! A transform maps **content space → screen space**. Its
//! [`inverse`](Transform::inverse) maps back, which is how a pointer
//! position becomes a content-space position for hit testing.

use crate::mui::MaraInput;
use crate::vocab::{Pos2, Rect, Vec2};

/// Uniform scale plus translation, mapping content space to screen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform {
    pub scaling: f32,
    pub translation: Vec2,
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        scaling: 1.0,
        translation: Vec2::ZERO,
    };

    #[must_use]
    pub const fn new(translation: Vec2, scaling: f32) -> Self {
        Self {
            scaling,
            translation,
        }
    }

    /// The reverse mapping — screen space back to content space.
    ///
    /// A zero scale has no inverse; that returns [`Transform::IDENTITY`]
    /// rather than producing infinities, so a degenerate state degrades
    /// to "unzoomed" instead of poisoning every later coordinate.
    #[must_use]
    pub fn inverse(self) -> Self {
        if self.scaling == 0.0 {
            return Self::IDENTITY;
        }
        Self::new(-self.translation / self.scaling, 1.0 / self.scaling)
    }

    #[must_use]
    pub fn mul_pos(self, pos: Pos2) -> Pos2 {
        Pos2::new(
            self.scaling * pos.x + self.translation.x,
            self.scaling * pos.y + self.translation.y,
        )
    }

    #[must_use]
    pub fn mul_vec(self, v: Vec2) -> Vec2 {
        v * self.scaling
    }

    #[must_use]
    pub fn mul_rect(self, rect: Rect) -> Rect {
        Rect::from_min_max(self.mul_pos(rect.min), self.mul_pos(rect.max))
    }

    /// Rescale about a fixed screen point, so the content under
    /// `anchor` stays under it — the behaviour a cursor-anchored zoom
    /// needs.
    #[must_use]
    pub fn scaled_around(self, scaling: f32, anchor: Pos2) -> Self {
        let inverse = self.inverse();
        let content = inverse.mul_pos(anchor);
        Self::new(
            Vec2::new(
                anchor.x - scaling * content.x,
                anchor.y - scaling * content.y,
            ),
            scaling,
        )
    }
}

/// Pan/zoom gesture state.
///
/// Pure logic over [`MaraInput`] — no backend involved — so a surface
/// gets drag-to-pan and scroll-to-zoom without naming one, and the
/// behaviour is testable headlessly.
#[derive(Clone, Copy, Debug)]
pub struct PanZoom {
    transform: Transform,
    min_scale: f32,
    max_scale: f32,
    /// Zoom per unit of scroll. egui's wheel deltas are in points, so
    /// this is deliberately small.
    zoom_speed: f32,
}

impl Default for PanZoom {
    fn default() -> Self {
        Self {
            transform: Transform::IDENTITY,
            min_scale: 0.1,
            max_scale: 10.0,
            zoom_speed: 0.002,
        }
    }
}

impl PanZoom {
    #[must_use]
    pub fn new(min_scale: f32, max_scale: f32) -> Self {
        Self {
            min_scale: min_scale.max(f32::EPSILON),
            max_scale: max_scale.max(min_scale),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn transform(self) -> Transform {
        self.transform
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
        self.clamp();
    }

    /// Apply a frame of input. `dragging` says whether the gesture that
    /// pans is active — the caller decides which button that is, so a
    /// surface can reserve left-drag for selection.
    ///
    /// Returns `true` when the transform changed, so the caller can
    /// request a repaint only when something moved.
    pub fn update(&mut self, input: &MaraInput, dragging: bool) -> bool {
        let before = self.transform;

        if dragging {
            self.transform.translation += input.pointer_delta;
        }

        let scroll = input.scroll_delta.y;
        if scroll.abs() > f32::EPSILON
            && let Some(anchor) = input.pointer
        {
            let factor = (scroll * self.zoom_speed).exp();
            let target = (self.transform.scaling * factor).clamp(self.min_scale, self.max_scale);
            self.transform = self.transform.scaled_around(target, anchor);
        }

        self.clamp();
        self.transform != before
    }

    fn clamp(&mut self) {
        self.transform.scaling = self.transform.scaling.clamp(self.min_scale, self.max_scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(pointer: Option<Pos2>, delta: Vec2, scroll: f32) -> MaraInput {
        MaraInput {
            pointer,
            pointer_delta: delta,
            scroll_delta: Vec2::new(0.0, scroll),
            ..MaraInput::default()
        }
    }

    #[test]
    fn inverse_round_trips_a_position() {
        let t = Transform::new(Vec2::new(30.0, -12.0), 2.5);
        let p = Pos2::new(7.0, 11.0);
        let back = t.inverse().mul_pos(t.mul_pos(p));
        assert!((back.x - p.x).abs() < 1e-3, "{back:?}");
        assert!((back.y - p.y).abs() < 1e-3, "{back:?}");
    }

    /// A zero scale is degenerate. Returning identity keeps every later
    /// coordinate finite instead of spreading NaN/inf through hit tests.
    #[test]
    fn inverse_of_a_degenerate_scale_is_identity() {
        let t = Transform::new(Vec2::new(5.0, 5.0), 0.0);
        assert_eq!(t.inverse(), Transform::IDENTITY);
    }

    /// The whole point of anchored zoom: whatever sits under the cursor
    /// must not move while zooming.
    #[test]
    fn zoom_keeps_the_anchored_content_under_the_cursor() {
        let t = Transform::new(Vec2::new(10.0, 20.0), 1.0);
        let anchor = Pos2::new(120.0, 80.0);
        let content_before = t.inverse().mul_pos(anchor);

        let zoomed = t.scaled_around(3.0, anchor);
        let content_after = zoomed.inverse().mul_pos(anchor);

        assert!((content_after.x - content_before.x).abs() < 1e-3);
        assert!((content_after.y - content_before.y).abs() < 1e-3);
    }

    #[test]
    fn drag_pans_only_while_the_gesture_is_active() {
        let mut pz = PanZoom::default();
        let moved = pz.update(&input(None, Vec2::new(5.0, 7.0), 0.0), false);
        assert!(!moved, "no pan without the gesture");

        assert!(pz.update(&input(None, Vec2::new(5.0, 7.0), 0.0), true));
        assert_eq!(pz.transform().translation, Vec2::new(5.0, 7.0));
    }

    #[test]
    fn zoom_stays_within_its_range() {
        let mut pz = PanZoom::new(0.5, 2.0);
        for _ in 0..200 {
            pz.update(&input(Some(Pos2::ZERO), Vec2::ZERO, 500.0), false);
        }
        assert!((pz.transform().scaling - 2.0).abs() < 1e-4);

        for _ in 0..400 {
            pz.update(&input(Some(Pos2::ZERO), Vec2::ZERO, -500.0), false);
        }
        assert!((pz.transform().scaling - 0.5).abs() < 1e-4);
    }

    #[test]
    fn update_reports_whether_anything_moved() {
        let mut pz = PanZoom::default();
        assert!(!pz.update(&input(None, Vec2::ZERO, 0.0), true));
        assert!(pz.update(&input(None, Vec2::new(1.0, 0.0), 0.0), true));
    }
}
