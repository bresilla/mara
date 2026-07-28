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

/// Pointer and keyboard state to feed into an offscreen surface, in the
/// surface's OWN coordinate space.
///
/// Without this an offscreen surface is inert: it has no window, so it
/// receives no events unless the host forwards them. The caller maps
/// window coordinates into surface-local ones — it is the only party
/// that knows where the composited texture was drawn.
#[cfg(feature = "gpu")]
#[derive(Clone, Copy, Debug, Default)]
pub struct OffscreenInput {
    /// Pointer position in surface-local points, or `None` when the
    /// pointer is elsewhere.
    pub pointer: Option<crate::vocab::Pos2>,
    pub primary_down: bool,
    pub secondary_down: bool,
    pub middle_down: bool,
    pub scroll_delta: crate::vocab::Vec2,
    /// Pointer movement since the previous frame, in surface-local
    /// points.
    ///
    /// Distinct from `pointer`: a surface that steers a camera reads the
    /// *delta*, and reconstructing it by differencing positions across
    /// frames loses the first frame of every drag.
    pub pointer_delta: crate::vocab::Vec2,
    /// Active touch, if the host is forwarding one.
    ///
    /// Separate from `pointer` because a touch is not a mouse: it has no
    /// hover state, and a host that synthesises a pointer from it loses
    /// multi-touch gestures. `None` on a mouse-driven host.
    pub touch: Option<OffscreenTouch>,
    pub modifiers_shift: bool,
    pub modifiers_ctrl: bool,
    pub modifiers_alt: bool,
}

/// How far content shifts, in surface-local points, when an offscreen
/// surface's scale changes about a cursor.
///
/// Add the result to the embedded content's pan to keep the point under
/// the cursor stationary across a zoom step.
///
/// `cursor` and `origin` are in the *parent's* coordinates; the scales
/// are the surface's before and after. Returns zero for a degenerate
/// scale rather than an infinity — a zoom of nothing moves nothing.
///
/// This exists so the caller of [`ViewCtx::offscreen`] can anchor a zoom
/// without a callback from inside the render (PLAN.md WS-D1.4). The
/// caller owns the scale it passes in, so it already knows both ends of
/// the step; nothing inside the surface knows more than it does.
///
/// [`ViewCtx::offscreen`]: crate::ViewCtx::offscreen
#[must_use]
pub fn offscreen_zoom_anchor_delta(
    cursor: crate::vocab::Pos2,
    origin: crate::vocab::Pos2,
    old_scale: f32,
    new_scale: f32,
) -> crate::vocab::Vec2 {
    if old_scale.abs() < f32::EPSILON || new_scale.abs() < f32::EPSILON {
        return crate::vocab::Vec2::ZERO;
    }
    let offset = crate::vocab::Vec2::new(cursor.x - origin.x, cursor.y - origin.y);
    crate::vocab::Vec2::new(
        offset.x / new_scale - offset.x / old_scale,
        offset.y / new_scale - offset.y / old_scale,
    )
}

/// One touch point forwarded into an offscreen surface.
#[cfg(feature = "gpu")]
#[derive(Clone, Copy, Debug)]
pub struct OffscreenTouch {
    /// Position in surface-local points.
    pub pos: crate::vocab::Pos2,
    /// Whether the finger is currently down.
    pub down: bool,
    /// Distinguishes simultaneous touches within one gesture.
    pub id: u64,
}

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

    /// Discard this pass's output and run it again before presenting.
    ///
    /// Distinct from [`request_repaint`](MaraCtx::request_repaint),
    /// which schedules a *future* frame: this one says the pass just
    /// computed is not fit to show. An immediate-mode surface needs it
    /// on the frame it first learns a size — laying out with a guess and
    /// presenting it is a visible flash.
    ///
    /// `reason` is for the host's debug output only.
    ///
    /// The default does nothing. A host that cannot re-run a pass
    /// presents the first one, which is the pre-existing behaviour
    /// rather than a regression.
    fn request_discard(&self, reason: &str) {
        let _ = reason;
    }

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

    /// Take any of `keys` that were pressed this frame, consuming them
    /// so nothing downstream sees them.
    ///
    /// A modal surface — a command palette — has to swallow Escape and
    /// the arrows before the app's own shortcuts run, which is a
    /// frame-level claim rather than a surface-level one.
    ///
    /// The default consumes nothing.
    fn consume_keys(&self, keys: &[crate::mui::MaraKey]) -> Vec<crate::mui::MaraKey> {
        let _ = keys;
        Vec::new()
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

    /// Upload an image as a managed texture, returning the retained
    /// handle.
    ///
    /// A texture belongs to the frame's host, not to whichever surface
    /// happened to be drawing — a view uploads once and paints the
    /// handle for as long as it lives.
    ///
    /// `None` when the host has no texture store.
    fn load_texture(
        &self,
        name: &str,
        image: crate::vocab::ColorImage,
        options: crate::vocab::TextureOptions,
    ) -> Option<crate::vocab::TextureHandle> {
        let _ = (name, image, options);
        None
    }

    /// Render a UI body into its own texture at an independent
    /// rasterisation scale, and return the texture to paint.
    ///
    /// Backs [`ViewCtx::offscreen`](crate::ViewCtx::offscreen). It sits
    /// on the context rather than on a surface because an offscreen
    /// pass needs the *host* — a device to allocate on and a texture
    /// store to register the result in — not whichever surface happened
    /// to be drawing.
    ///
    /// `None` when the surface cannot be prepared: a degenerate size, a
    /// failed GPU allocation, or a host with no offscreen support at
    /// all, which is what the default returns. Callers paint a fallback
    /// rather than assume a texture.
    #[cfg(feature = "gpu")]
    fn render_offscreen(
        &self,
        gpu: mara_gpu::MaraRenderState<'_>,
        id: crate::vocab::Id,
        size_points: crate::vocab::Vec2,
        scale: f32,
        accent: crate::vocab::Color32,
        input: OffscreenInput,
        body: &mut dyn FnMut(&mut crate::MaraUi<'_>),
    ) -> Option<crate::vocab::TextureId> {
        let _ = (gpu, id, size_points, scale, accent, input, body);
        None
    }

    /// Backend-neutral state store.
    fn memory(&self) -> MaraMemoryCtx<'_>;

    /// Clone behind the box.
    ///
    /// A view node owns its context so it can lend one out, and a
    /// scoped child needs its own copy — but a trait object cannot
    /// derive `Clone`.
    fn boxed_clone(&self) -> Box<dyn MaraCtx + '_>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Pos2, Vec2};

    /// The zoom-anchor arithmetic, pinned against the formula
    /// `mara_graph::node_view` computes inside its render:
    /// `offset/z_new - offset/z_old`. Reproducing it at the call site is
    /// what lets `ViewCtx::offscreen` stay callback-free.
    mod zoom_anchor {
        use super::*;

        #[test]
        fn no_scale_change_moves_nothing() {
            let d = offscreen_zoom_anchor_delta(
                Pos2::new(120.0, 80.0),
                Pos2::new(20.0, 10.0),
                2.0,
                2.0,
            );
            assert_eq!(d, Vec2::ZERO);
        }

        #[test]
        fn a_cursor_at_the_origin_never_moves() {
            let at_origin = Pos2::new(20.0, 10.0);
            let d = offscreen_zoom_anchor_delta(at_origin, at_origin, 1.0, 4.0);
            assert_eq!(d, Vec2::ZERO, "the anchor point is the fixed point");
        }

        #[test]
        fn it_matches_the_offset_over_scale_difference() {
            let cursor = Pos2::new(120.0, 80.0);
            let origin = Pos2::new(20.0, 10.0);
            let (old, new) = (1.0_f32, 2.0_f32);
            let off = Vec2::new(cursor.x - origin.x, cursor.y - origin.y);
            assert_eq!(
                offscreen_zoom_anchor_delta(cursor, origin, old, new),
                Vec2::new(off.x / new - off.x / old, off.y / new - off.y / old)
            );
        }

        /// Zooming in pulls content back toward the origin; zooming out
        /// pushes it away. Opposite signs, and reversing the step
        /// reverses the delta.
        #[test]
        fn zoom_in_and_out_are_opposite_and_symmetric() {
            let cursor = Pos2::new(120.0, 80.0);
            let origin = Pos2::new(20.0, 10.0);
            let in_ = offscreen_zoom_anchor_delta(cursor, origin, 1.0, 2.0);
            let out = offscreen_zoom_anchor_delta(cursor, origin, 2.0, 1.0);
            assert!(in_.x < 0.0 && out.x > 0.0, "in={in_:?} out={out:?}");
            assert_eq!(in_.x, -out.x);
            assert_eq!(in_.y, -out.y);
        }

        /// A degenerate scale yields zero, not an infinity that would
        /// poison the pan it is added to.
        #[test]
        fn a_degenerate_scale_yields_zero_not_infinity() {
            let c = Pos2::new(120.0, 80.0);
            let o = Pos2::new(20.0, 10.0);
            assert_eq!(offscreen_zoom_anchor_delta(c, o, 0.0, 2.0), Vec2::ZERO);
            assert_eq!(offscreen_zoom_anchor_delta(c, o, 2.0, 0.0), Vec2::ZERO);
        }
    }

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
        fn boxed_clone(&self) -> Box<dyn MaraCtx + '_> {
            Box::new(Self {
                pass: self.pass,
                repaints: std::cell::Cell::new(self.repaints.get()),
            })
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
