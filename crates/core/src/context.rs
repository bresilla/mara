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
    fn area(
        &self,
        host: crate::layout::AreaHost,
        body: &mut dyn FnMut(&mut crate::MaraUi<'_>),
    ) -> Rect {
        let _ = (host, body);
        Rect::NOTHING
    }

    /// [`area`](MaraCtx::area) with a minimum size — for a floating
    /// surface whose extent is known up front (a divider strip, a
    /// fixed-size popup) rather than derived from its content.
    fn area_slot(
        &self,
        spec: crate::layout::AreaSlotSpec,
        body: &mut dyn FnMut(&mut crate::MaraUi<'_>),
    ) -> Rect {
        let _ = (spec, body);
        Rect::NOTHING
    }

    /// A painter over a registered floating layer, clipped to `clip`.
    ///
    /// [`area`](MaraCtx::area) lends its surface to a closure and takes
    /// it back at the end; a view backdrop instead needs a painter it
    /// can **keep** and hand to drawing code that knows nothing about
    /// surfaces. Registering the layer under `id` also fixes its z-slot,
    /// so the backdrop keeps its depth when the region moves or resizes
    /// and panes opened later stack above it rather than behind it.
    ///
    /// The default records commands instead of rasterising: a host with
    /// no layer stack still gets a painter that behaves correctly, it
    /// just paints nowhere.
    fn layer_painter(
        &self,
        layer: crate::layout::Layer,
        id: crate::vocab::Id,
        clip: Rect,
    ) -> crate::MaraPainter {
        let _ = (layer, id);
        crate::MaraPainter::__internal_recording(clip)
    }

    /// Set the pointer cursor for the rest of this frame.
    ///
    /// The frame-level sibling of
    /// [`MaraUi::set_cursor_icon`](crate::MaraUi::set_cursor_icon). A
    /// surface can only speak for the pointer while it is over that
    /// surface; a drag in progress carries the pointer *off* the
    /// surface that started it, and the grab cursor has to survive
    /// that. So the cursor for a live drag is set here.
    ///
    /// The default does nothing — a host with no pointer has no cursor.
    fn set_cursor_icon(&self, cursor: crate::layout::CursorIcon) {
        let _ = cursor;
    }

    /// Whether the layout probe is capturing this frame.
    ///
    /// Recording a pose costs a string format at every call site, so
    /// callers gate on this first.
    fn probe_enabled(&self) -> bool {
        false
    }

    /// Record one labeled layout pose for the probe.
    ///
    /// The probe is how first-party tooling reads back where things
    /// actually landed. It lives on the context because a pose belongs
    /// to the frame, not to whichever surface happened to notice it.
    ///
    /// Both default to inert: a host with no probe records nothing.
    fn probe_record(&self, pose: crate::probe::ElementPose) {
        let _ = pose;
    }

    /// Enable (with a fresh log) or disable pose recording.
    fn probe_set_enabled(&self, on: bool) {
        let _ = on;
    }

    /// Drain and return the poses recorded this frame.
    fn probe_drain(&self) -> Vec<crate::probe::ElementPose> {
        Vec::new()
    }

    /// The host window's full rect, including any native chrome.
    ///
    /// [`content_rect`](MaraCtx::content_rect) is what a view lays out
    /// into; this is the window itself. They differ exactly when Mara
    /// draws its own title bar, which is when the window chrome needs
    /// to know where the real edges are.
    ///
    /// Defaults to `content_rect` — with no chrome, they are the same
    /// rect.
    fn window_rect(&self) -> Rect {
        self.content_rect()
    }

    /// Whether the host window is maximized.
    ///
    /// Window chrome draws a different glyph for maximize and restore,
    /// so it has to ask. Frame-level rather than per-surface: there is
    /// one window, and every surface that asks means the same one.
    ///
    /// Defaults to `false` — a host with no window is not maximized.
    fn viewport_maximized(&self) -> bool {
        false
    }

    /// Apply Mara's enforced per-pass defaults to this host.
    ///
    /// Every Mara surface entry point calls this before it draws, which
    /// is what makes the defaults *enforced* rather than opt-in: an app
    /// that never asks still gets the theme, the image-loader chain a
    /// sealed module's `Svg` command needs, and the shell bar. Cheap
    /// after the first call of a pass — the rest are stamp reads.
    ///
    /// The default does nothing. A host with no widget tree has no
    /// visuals to install and no bar to fall back to, so there is
    /// nothing to enforce.
    fn enforce_defaults(&self) {}

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
