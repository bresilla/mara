//! The context seam — PLAN.md WS-E3.
//!
//! [`MaraCtx`] is to frame-level state what
//! [`UiBackend`](crate::layout::UiBackend) is to drawing and layout: the
//! contract core code uses instead of naming the backend's context type.
//!
//! ## Why it exists
//!
//! Roughly 122 functions in `mara_core` take the backend's context type
//! directly. An earlier plan assumed those could be routed through
//! [`MaraMemoryCtx`](crate::memory::MaraMemoryCtx), but measurement
//! showed memory is only a fraction of what they do — the rest read
//! input, the frame counter, the content rect, request repaints, ask for
//! the display scale, or install fonts. None of that has a sealed home,
//! which is why `crates/core` cannot yet be split into a backend-free
//! crate plus a backend crate (WS-G1).
//!
//! This is that home. It is deliberately **narrow**: only operations
//! that already appear at those call sites, so migrating a function is
//! a signature change rather than a redesign.
//!
//! ## Migration
//!
//! Convert a function taking the backend context to `&dyn MaraCtx`
//! one at a time. The concrete impl lives in `backend/`, so each
//! conversion moves one more file off the coupling ratchet's count.

use crate::memory::MaraMemoryCtx;
use crate::mui::MaraInput;
use crate::vocab::Rect;

/// Frame-level state a surface needs without naming a backend.
pub trait MaraCtx {
    /// Per-frame input snapshot.
    fn input(&self) -> MaraInput;

    /// Monotonic frame counter. Used for pass-stamping — "did this
    /// happen already this frame?" — which is how
    /// [`crate::enforce`] decides whether the app or Mara owns a
    /// default.
    fn pass_nr(&self) -> u64;

    /// The host's content area, excluding any native window chrome.
    fn content_rect(&self) -> Rect;

    /// Device pixels per logical point.
    fn pixels_per_point(&self) -> f32;

    /// Schedule another frame.
    fn request_repaint(&self);

    /// Schedule a frame no later than `after`.
    fn request_repaint_after(&self, after: std::time::Duration);

    /// Seconds since the host started. Frame-level state in the same
    /// category as [`pass_nr`](MaraCtx::pass_nr) — surfaces stamp it to
    /// drive time-based animation without reaching for a clock.
    fn now(&self) -> f64;

    /// Duration of the previous frame, in seconds. Never negative.
    fn dt(&self) -> f32;

    /// Show a floating surface — an overlay, a tooltip, a drag
    /// preview — positioned and layered by `host`.
    ///
    /// The context-level sibling of
    /// [`MaraUi::overlay_at`](crate::MaraUi::overlay_at): a floating
    /// surface belongs to the frame, not to whatever happened to be
    /// drawing when it was requested, so it is requested from here.
    ///
    /// Returns the rect the surface occupied. The default returns
    /// [`Rect::NOTHING`] and runs nothing — a host with no notion of
    /// floating layers has nowhere to put it.
    fn area(&self, host: crate::layout::AreaHost, body: &mut dyn FnMut(&mut crate::MaraUi<'_>)) -> Rect {
        let _ = (host, body);
        Rect::NOTHING
    }

    /// Backend-neutral state store.
    fn memory(&self) -> MaraMemoryCtx<'_>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Pos2, Vec2};

    /// A stand-in host, proving the trait is implementable with no
    /// backend at all and is object-safe — both prerequisites for the
    /// WS-G1 split.
    #[derive(Default)]
    struct FakeCtx {
        pass: u64,
        repaints: std::cell::Cell<u32>,
    }

    impl MaraCtx for FakeCtx {
        fn input(&self) -> MaraInput {
            MaraInput::default()
        }
        fn pass_nr(&self) -> u64 {
            self.pass
        }
        fn content_rect(&self) -> Rect {
            Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))
        }
        fn pixels_per_point(&self) -> f32 {
            2.0
        }
        fn request_repaint(&self) {
            self.repaints.set(self.repaints.get() + 1);
        }
        fn request_repaint_after(&self, _after: std::time::Duration) {
            self.repaints.set(self.repaints.get() + 1);
        }
        fn now(&self) -> f64 {
            42.0
        }
        fn dt(&self) -> f32 {
            1.0 / 60.0
        }
        fn memory(&self) -> MaraMemoryCtx<'_> {
            unimplemented!("this fake covers the frame-state half only")
        }
    }

    #[test]
    fn the_seam_is_implementable_without_a_backend() {
        let ctx = FakeCtx {
            pass: 7,
            ..FakeCtx::default()
        };
        let dynamic: &dyn MaraCtx = &ctx;

        assert_eq!(dynamic.pass_nr(), 7);
        assert_eq!(dynamic.pixels_per_point(), 2.0);
        assert_eq!(dynamic.now(), 42.0);
        assert!(dynamic.dt() > 0.0);
        assert_eq!(dynamic.content_rect().size(), Vec2::new(800.0, 600.0));
        assert!(!dynamic.input().primary_down);

        dynamic.request_repaint();
        dynamic.request_repaint_after(std::time::Duration::from_millis(16));
        assert_eq!(ctx.repaints.get(), 2);
    }
}
