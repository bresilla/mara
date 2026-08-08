//! Critically-damped camera motion — PLAN_NODE.md P6.
//!
//! Presented as a visual nicety and treated here as a prerequisite,
//! because every later "move the view somewhere" feature is ten lines
//! once this exists and a bespoke animation without it: fit-to-content,
//! fit-to-selection, breadcrumb jumps, minimap pans, and the portal
//! dive into a subgraph.
//!
//! Two decisions that make it feel right rather than merely smooth:
//!
//! * **Zoom interpolates in log space.** Linear interpolation from 1×
//!   to 8× spends most of its time in the high magnifications and
//!   arrives with a lurch; in log2 space each equal time step multiplies
//!   the scale by an equal factor, which is what "zooming steadily"
//!   means to an eye.
//! * **Exponential approach, not a fixed-duration tween.** A new target
//!   mid-flight is absorbed rather than restarting a curve, so a user
//!   spinning the wheel gets one continuous motion instead of a stutter
//!   per notch.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::transform::Transform;
use mara_core::vocab::{Rect, Vec2};

/// How fast the camera converges, in e-folds per second.
///
/// ~18 puts it within a pixel of the target in about 200 ms, which is
/// long enough to read as motion and short enough not to feel sluggish.
pub const DEFAULT_RATE: f32 = 18.0;

/// Below this distance, and this much scale error, the camera is
/// considered arrived — otherwise it asks for repaints forever chasing
/// a target it can never quite reach in floating point.
const SETTLED_TRANSLATION: f32 = 0.05;
const SETTLED_LOG_ZOOM: f32 = 0.0005;

/// A camera easing toward a target transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CameraSpring {
    translation: Vec2,
    /// Scale carried as log2, so interpolation is geometric.
    log_zoom: f32,
    target_translation: Vec2,
    target_log_zoom: f32,
}

impl CameraSpring {
    /// Re-exported so a caller configuring `GraphStyle::camera_spring`
    /// does not have to invent a rate.
    pub const DEFAULT_RATE_HINT: f32 = DEFAULT_RATE;

    /// A camera already at `t`, with nothing pending.
    #[must_use]
    pub fn at(t: Transform) -> Self {
        let log_zoom = t.scaling.max(f32::MIN_POSITIVE).log2();
        Self {
            translation: t.translation,
            log_zoom,
            target_translation: t.translation,
            target_log_zoom: log_zoom,
        }
    }

    /// Aim at `t` without moving. The next [`step`](Self::step) begins
    /// the approach.
    pub fn retarget(&mut self, t: Transform) {
        self.target_translation = t.translation;
        self.target_log_zoom = t.scaling.max(f32::MIN_POSITIVE).log2();
    }

    /// Jump to `t` immediately, cancelling any motion. For a caller
    /// that is restoring a saved view rather than navigating to one.
    pub fn snap(&mut self, t: Transform) {
        *self = Self::at(t);
    }

    /// The camera's current transform.
    #[must_use]
    pub fn current(&self) -> Transform {
        Transform {
            translation: self.translation,
            scaling: self.log_zoom.exp2(),
        }
    }

    /// Whether the camera has arrived.
    ///
    /// A caller gates `request_repaint` on `!settled()`, which is what
    /// keeps an idle canvas at zero frames.
    #[must_use]
    pub fn settled(&self) -> bool {
        let d = self.target_translation - self.translation;
        d.length() <= SETTLED_TRANSLATION
            && (self.target_log_zoom - self.log_zoom).abs() <= SETTLED_LOG_ZOOM
    }

    /// Advance by `dt` seconds. Returns `true` while still moving.
    ///
    /// The approach factor is `1 - exp(-dt * rate)`, which is
    /// frame-rate independent: halving `dt` and doubling the number of
    /// steps lands in the same place, so the motion does not change
    /// character between a 60 Hz and a 144 Hz display.
    pub fn step(&mut self, dt: f32, rate: f32) -> bool {
        if self.settled() {
            // Snap the last sub-epsilon gap, or the camera sits
            // fractionally off forever.
            self.translation = self.target_translation;
            self.log_zoom = self.target_log_zoom;
            return false;
        }
        let dt = dt.clamp(0.0, 0.25);
        let alpha = (1.0 - (-dt * rate.max(0.0)).exp()).clamp(0.0, 1.0);
        self.translation += (self.target_translation - self.translation) * alpha;
        self.log_zoom += (self.target_log_zoom - self.log_zoom) * alpha;
        true
    }
}

/// The transform that maps `interior` onto the screen area `instance`
/// currently occupies, letting a dive *start* from the instance node
/// and open outward into its contents.
///
/// Pure, so the feel of the animation can be tested without rendering
/// anything: the claim is that entering a subgraph begins exactly where
/// the block was, which is what makes it read as going *into* something
/// rather than as a page reload.
///
/// `max_scale` bounds the result so a one-node interior does not arrive
/// at 40× magnification.
#[must_use]
pub fn dive_target(instance: Rect, interior: Rect, max_scale: f32) -> Transform {
    let iw = interior.width().max(f32::MIN_POSITIVE);
    let ih = interior.height().max(f32::MIN_POSITIVE);
    let scaling = (instance.width() / iw)
        .min(instance.height() / ih)
        .clamp(f32::MIN_POSITIVE, max_scale);

    // Place the interior's centre at the instance's centre.
    let c = interior.center();
    let to = instance.center();
    Transform {
        translation: Vec2::new(to.x - c.x * scaling, to.y - c.y * scaling),
        scaling,
    }
}

/// Where the camera should settle once inside — the interior fitted to
/// the viewport. The dive runs from [`dive_target`] to this.
#[must_use]
pub fn settled_target(interior: Rect, viewport: Rect, min_scale: f32, max_scale: f32) -> Transform {
    let iw = interior.width().max(f32::MIN_POSITIVE);
    let ih = interior.height().max(f32::MIN_POSITIVE);
    let scaling = (viewport.width() / iw)
        .min(viewport.height() / ih)
        .clamp(min_scale, max_scale);
    let c = interior.center();
    let to = viewport.center();
    Transform {
        translation: Vec2::new(to.x - c.x * scaling, to.y - c.y * scaling),
        scaling,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use mara_core::vocab::Pos2;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    /// A dive must begin exactly where the block is — that is what
    /// makes it read as entering something. Assert the mapped interior
    /// lands centred on the instance and fits inside it.
    #[test]
    fn a_dive_starts_centred_on_the_instance() {
        let instance = rect(300.0, 200.0, 120.0, 80.0);
        let interior = rect(0.0, 0.0, 600.0, 400.0);
        let viewport = rect(0.0, 0.0, 1024.0, 768.0);

        let _ = viewport;
        let t = dive_target(instance, interior, 8.0);

        let mapped_centre = Pos2::new(
            interior.center().x * t.scaling + t.translation.x,
            interior.center().y * t.scaling + t.translation.y,
        );
        assert!((mapped_centre.x - instance.center().x).abs() < 0.01);
        assert!((mapped_centre.y - instance.center().y).abs() < 0.01);

        let mapped_w = interior.width() * t.scaling;
        let mapped_h = interior.height() * t.scaling;
        assert!(mapped_w <= instance.width() + 0.01, "must fit the block");
        assert!(mapped_h <= instance.height() + 0.01);
    }

    /// The settled view fills the viewport, so the dive is a genuine
    /// zoom rather than a jump between two arbitrary framings.
    #[test]
    fn the_settled_target_fits_the_interior_to_the_viewport() {
        let interior = rect(0.0, 0.0, 600.0, 400.0);
        let viewport = rect(0.0, 0.0, 1200.0, 800.0);

        let t = settled_target(interior, viewport, 0.1, 8.0);

        assert!(
            (t.scaling - 2.0).abs() < 0.01,
            "600x400 into 1200x800 is 2x"
        );
        let mapped_centre = Pos2::new(
            interior.center().x * t.scaling + t.translation.x,
            interior.center().y * t.scaling + t.translation.y,
        );
        assert!((mapped_centre.x - viewport.center().x).abs() < 0.01);
        assert!((mapped_centre.y - viewport.center().y).abs() < 0.01);
    }

    /// A tiny interior must not arrive at absurd magnification.
    #[test]
    fn the_settled_scale_is_clamped() {
        let tiny = rect(0.0, 0.0, 4.0, 4.0);
        let viewport = rect(0.0, 0.0, 1024.0, 768.0);
        let t = settled_target(tiny, viewport, 0.1, 4.0);
        assert!(t.scaling <= 4.0);
    }

    /// A degenerate interior must produce a usable transform rather
    /// than an infinity — an empty definition is a normal thing to
    /// enter, not an error.
    #[test]
    fn an_empty_interior_does_not_produce_a_degenerate_transform() {
        let empty = rect(10.0, 10.0, 0.0, 0.0);
        let viewport = rect(0.0, 0.0, 1024.0, 768.0);
        let t = settled_target(empty, viewport, 0.1, 4.0);
        assert!(t.scaling.is_finite() && t.scaling > 0.0);
        assert!(t.translation.x.is_finite() && t.translation.y.is_finite());
    }

    fn t(x: f32, y: f32, s: f32) -> Transform {
        Transform {
            translation: Vec2::new(x, y),
            scaling: s,
        }
    }

    #[test]
    fn a_fresh_camera_is_settled_and_reports_its_transform() {
        let c = CameraSpring::at(t(10.0, 20.0, 2.0));
        assert!(c.settled());
        let cur = c.current();
        assert!((cur.scaling - 2.0).abs() < 1e-4);
        assert_eq!(cur.translation, Vec2::new(10.0, 20.0));
    }

    /// The property that separates a spring from a tween: it must
    /// approach without ever passing the target. An overshoot on a
    /// camera reads as the view bouncing.
    #[test]
    fn it_converges_monotonically_and_never_overshoots() {
        let mut c = CameraSpring::at(t(0.0, 0.0, 1.0));
        c.retarget(t(100.0, 0.0, 4.0));

        let mut prev_x = 0.0_f32;
        let mut prev_s = 1.0_f32;
        for _ in 0..600 {
            c.step(1.0 / 60.0, DEFAULT_RATE);
            let cur = c.current();
            assert!(cur.translation.x >= prev_x - 1e-4, "translation reversed");
            assert!(cur.translation.x <= 100.0 + 1e-3, "overshot translation");
            assert!(cur.scaling >= prev_s - 1e-4, "scale reversed");
            assert!(cur.scaling <= 4.0 + 1e-3, "overshot scale");
            prev_x = cur.translation.x;
            prev_s = cur.scaling;
        }
        assert!(c.settled(), "600 frames must be enough to arrive");
    }

    /// The log-space proof, stated as an equivalence rather than as a
    /// property of the ratios — an exponential approach decays, so
    /// successive ratios are *not* constant and asserting they are just
    /// re-tests the easing curve.
    ///
    /// What is actually claimed: zoom is eased in log2 space. So a
    /// camera going 1× → 16× must have its `log2(scale)` follow exactly
    /// the same curve as a camera easing its translation 0 → 4. If zoom
    /// were eased linearly, `scale` itself would follow that curve
    /// instead and this fails by a wide margin.
    #[test]
    fn zoom_is_eased_in_log_space() {
        let mut zoom = CameraSpring::at(t(0.0, 0.0, 1.0));
        zoom.retarget(t(0.0, 0.0, 16.0)); // log2: 0 -> 4

        let mut pan = CameraSpring::at(t(0.0, 0.0, 1.0));
        pan.retarget(t(4.0, 0.0, 1.0)); // linear: 0 -> 4

        // Compared only while BOTH are still in flight. The two axes
        // settle on different epsilons — translation snaps within
        // 0.05 points, log-zoom within 0.0005 — so once either has
        // arrived they legitimately stop tracking each other.
        let mut compared = 0;
        for step in 0..60 {
            let zoom_moving = zoom.step(1.0 / 60.0, DEFAULT_RATE);
            let pan_moving = pan.step(1.0 / 60.0, DEFAULT_RATE);
            if !zoom_moving || !pan_moving {
                break;
            }

            let log_scale = zoom.current().scaling.log2();
            let translated = pan.current().translation.x;
            assert!(
                (log_scale - translated).abs() < 1e-3,
                "step {step}: log2(scale)={log_scale} should track translation={translated}"
            );
            compared += 1;
        }
        assert!(
            compared >= 10,
            "only {compared} steps compared — the test proved almost nothing"
        );
    }

    /// The same claim from the other side: the *raw* scale must not
    /// track the linear curve, or the easing is linear after all.
    #[test]
    fn raw_zoom_does_not_progress_linearly() {
        let mut zoom = CameraSpring::at(t(0.0, 0.0, 1.0));
        zoom.retarget(t(0.0, 0.0, 16.0));

        let mut pan = CameraSpring::at(t(0.0, 0.0, 1.0));
        pan.retarget(t(15.0, 0.0, 1.0)); // 1 -> 16 linearly

        let mut max_divergence = 0.0_f32;
        for _ in 0..30 {
            zoom.step(1.0 / 60.0, DEFAULT_RATE);
            pan.step(1.0 / 60.0, DEFAULT_RATE);
            let linear = 1.0 + pan.current().translation.x;
            max_divergence = max_divergence.max((zoom.current().scaling - linear).abs());
        }
        assert!(
            max_divergence > 1.0,
            "raw scale tracked the linear curve too closely ({max_divergence}) — \
             zoom is not being eased in log space"
        );
    }

    #[test]
    fn stepping_a_settled_camera_reports_no_motion() {
        let mut c = CameraSpring::at(t(5.0, 5.0, 1.0));
        assert!(!c.step(1.0 / 60.0, DEFAULT_RATE));
    }

    /// Frame-rate independence: the same elapsed time must produce the
    /// same result regardless of how it is subdivided, or the camera
    /// feels different on a 144 Hz display.
    #[test]
    fn the_approach_is_frame_rate_independent() {
        let mut coarse = CameraSpring::at(t(0.0, 0.0, 1.0));
        coarse.retarget(t(100.0, 0.0, 1.0));
        for _ in 0..10 {
            coarse.step(1.0 / 60.0, DEFAULT_RATE);
        }

        let mut fine = CameraSpring::at(t(0.0, 0.0, 1.0));
        fine.retarget(t(100.0, 0.0, 1.0));
        for _ in 0..20 {
            fine.step(1.0 / 120.0, DEFAULT_RATE);
        }

        let a = coarse.current().translation.x;
        let b = fine.current().translation.x;
        assert!(
            (a - b).abs() < 0.5,
            "{a} vs {b} — subdivision changed the path"
        );
    }

    #[test]
    fn retargeting_mid_flight_absorbs_rather_than_restarting() {
        let mut c = CameraSpring::at(t(0.0, 0.0, 1.0));
        c.retarget(t(100.0, 0.0, 1.0));
        for _ in 0..5 {
            c.step(1.0 / 60.0, DEFAULT_RATE);
        }
        let mid = c.current().translation.x;
        assert!(mid > 0.0 && mid < 100.0);

        c.retarget(t(200.0, 0.0, 1.0));
        c.step(1.0 / 60.0, DEFAULT_RATE);
        let after = c.current().translation.x;
        assert!(
            after > mid,
            "a new target must continue from where the camera is, not from zero"
        );
    }

    #[test]
    fn snap_cancels_motion() {
        let mut c = CameraSpring::at(t(0.0, 0.0, 1.0));
        c.retarget(t(100.0, 0.0, 4.0));
        c.snap(t(7.0, 8.0, 2.0));
        assert!(c.settled());
        assert_eq!(c.current().translation, Vec2::new(7.0, 8.0));
    }
}
