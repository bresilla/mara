//! Backend-neutral scroll-offset model.
//!
//! This is the first slice of a Mara-owned scroll subsystem. Today
//! scroll hosts (shelf bodies, command-palette result list, resizable
//! pods) use egui's `ScrollArea`, which owns the offset, clamping, and
//! scroll-into-view behaviour. To make scrolling backend-agnostic,
//! Mara needs to own the offset state and the clamp/scroll-by/
//! scroll-into-view math as plain data + pure operations.
//!
//! This module provides exactly the engine-independent core:
//!
//! * [`ScrollState`] — a scroll offset persisted through
//!   [`MaraMemory`], with `scroll_by`, `clamp`, and `scroll_to_visible`
//!   computed against content vs. viewport sizes.
//! * [`max_offset`] — the maximum legal offset for a content/viewport
//!   pair.
//!
//! Offsets are non-negative distances scrolled from the top-left, in
//! the same units as the rects. The egui backend will feed content /
//! viewport sizes and apply the resulting offset; a future backend
//! reuses this verbatim.

use crate::memory::MaraMemory;
use crate::vocab::{Id, Rect, Vec2};

/// Maximum legal scroll offset for `content` shown through `viewport`
/// (zero on an axis where the content fits).
#[must_use]
pub fn max_offset(content: Vec2, viewport: Vec2) -> Vec2 {
    Vec2::new(
        (content.x - viewport.x).max(0.0),
        (content.y - viewport.y).max(0.0),
    )
}

fn clamp_axis(value: f32, max: f32) -> f32 {
    value.clamp(0.0, max)
}

/// Scroll offset for a scroll host, in content-space distance from the
/// top-left.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollState {
    pub offset: Vec2,
}

fn scroll_key(id: Id) -> Id {
    id.with("mara_scroll_offset")
}

impl ScrollState {
    #[must_use]
    pub const fn new(offset: Vec2) -> Self {
        Self { offset }
    }

    /// Read the persisted offset for `id` (default: top-left).
    #[must_use]
    pub fn load(memory: &impl MaraMemory, id: Id) -> Self {
        Self {
            offset: memory
                .get_temp::<Vec2>(scroll_key(id))
                .unwrap_or(Vec2::ZERO),
        }
    }

    /// Persist this offset for `id`.
    pub fn store(self, memory: &mut impl MaraMemory, id: Id) {
        memory.set_temp(scroll_key(id), self.offset);
    }

    /// Clamp the offset into the legal range for `content`/`viewport`.
    pub fn clamp(&mut self, content: Vec2, viewport: Vec2) {
        let max = max_offset(content, viewport);
        self.offset = Vec2::new(
            clamp_axis(self.offset.x, max.x),
            clamp_axis(self.offset.y, max.y),
        );
    }

    /// Apply a scroll delta (e.g. wheel) and clamp. Positive `delta`
    /// scrolls content up/left into view (offset increases).
    pub fn scroll_by(&mut self, delta: Vec2, content: Vec2, viewport: Vec2) {
        self.offset = Vec2::new(self.offset.x + delta.x, self.offset.y + delta.y);
        self.clamp(content, viewport);
    }

    /// Adjust the offset minimally so `target` (a rect in content
    /// space) is fully visible within a `viewport`-sized window, then
    /// clamp. If the target is larger than the viewport on an axis, its
    /// leading edge is aligned.
    pub fn scroll_to_visible(&mut self, target: Rect, content: Vec2, viewport: Vec2) {
        self.offset.y = adjust_axis(self.offset.y, target.min.y, target.max.y, viewport.y);
        self.offset.x = adjust_axis(self.offset.x, target.min.x, target.max.x, viewport.x);
        self.clamp(content, viewport);
    }
}

/// One-axis scroll-into-view: returns the new offset so `[lo, hi]` sits
/// inside `[offset, offset + viewport]`, moving as little as possible.
fn adjust_axis(offset: f32, lo: f32, hi: f32, viewport: f32) -> f32 {
    if lo < offset {
        lo
    } else if hi > offset + viewport {
        hi - viewport
    } else {
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::Pos2;
    use std::any::Any;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockMemory {
        temp: HashMap<Id, Box<dyn Any + Send + Sync>>,
    }
    impl MaraMemory for MockMemory {
        fn get_persisted<T: Clone + Send + Sync + 'static>(&self, _id: Id) -> Option<T> {
            None
        }
        fn set_persisted<T: Clone + Send + Sync + 'static>(&mut self, _id: Id, _value: T) {}
        fn get_temp<T: Clone + Send + Sync + 'static>(&self, id: Id) -> Option<T> {
            self.temp
                .get(&id)
                .and_then(|v| v.downcast_ref::<T>())
                .cloned()
        }
        fn set_temp<T: Clone + Send + Sync + 'static>(&mut self, id: Id, value: T) {
            self.temp.insert(id, Box::new(value));
        }
    }

    #[test]
    fn max_offset_is_zero_when_content_fits() {
        assert_eq!(
            max_offset(Vec2::new(50.0, 50.0), Vec2::new(100.0, 100.0)),
            Vec2::ZERO
        );
        assert_eq!(
            max_offset(Vec2::new(300.0, 80.0), Vec2::new(100.0, 100.0)),
            Vec2::new(200.0, 0.0)
        );
    }

    #[test]
    fn scroll_by_clamps_to_range() {
        let content = Vec2::new(100.0, 500.0);
        let viewport = Vec2::new(100.0, 100.0);
        let mut s = ScrollState::default();
        s.scroll_by(Vec2::new(0.0, 50.0), content, viewport);
        assert_eq!(s.offset, Vec2::new(0.0, 50.0));
        s.scroll_by(Vec2::new(0.0, 1000.0), content, viewport);
        assert_eq!(s.offset, Vec2::new(0.0, 400.0)); // max = 500-100
        s.scroll_by(Vec2::new(0.0, -1000.0), content, viewport);
        assert_eq!(s.offset, Vec2::new(0.0, 0.0));
    }

    #[test]
    fn clamp_pulls_stale_offset_into_range() {
        let mut s = ScrollState::new(Vec2::new(0.0, 999.0));
        s.clamp(Vec2::new(100.0, 200.0), Vec2::new(100.0, 100.0));
        assert_eq!(s.offset, Vec2::new(0.0, 100.0));
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn scroll_to_visible_scrolls_down_to_reveal_below() {
        let content = Vec2::new(100.0, 1000.0);
        let viewport = Vec2::new(100.0, 100.0);
        let mut s = ScrollState::default();
        // target at y 300..340, below the current 0..100 window.
        s.scroll_to_visible(rect(0.0, 300.0, 100.0, 40.0), content, viewport);
        assert_eq!(s.offset.y, 240.0); // hi(340) - viewport(100)
    }

    #[test]
    fn scroll_to_visible_scrolls_up_to_reveal_above() {
        let content = Vec2::new(100.0, 1000.0);
        let viewport = Vec2::new(100.0, 100.0);
        let mut s = ScrollState::new(Vec2::new(0.0, 500.0));
        s.scroll_to_visible(rect(0.0, 420.0, 100.0, 30.0), content, viewport);
        assert_eq!(s.offset.y, 420.0); // lo above window → align to lo
    }

    #[test]
    fn scroll_to_visible_noop_when_already_visible() {
        let content = Vec2::new(100.0, 1000.0);
        let viewport = Vec2::new(100.0, 100.0);
        let mut s = ScrollState::new(Vec2::new(0.0, 200.0));
        s.scroll_to_visible(rect(0.0, 220.0, 100.0, 30.0), content, viewport);
        assert_eq!(s.offset.y, 200.0);
    }

    #[test]
    fn offset_persists_through_memory() {
        let mut memory = MockMemory::default();
        let id = Id::new("list");
        assert_eq!(ScrollState::load(&memory, id).offset, Vec2::ZERO);
        ScrollState::new(Vec2::new(0.0, 42.0)).store(&mut memory, id);
        assert_eq!(ScrollState::load(&memory, id).offset, Vec2::new(0.0, 42.0));
    }
}
