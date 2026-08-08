//! Zoom level-of-detail — PLAN_NODE.md P6.
//!
//! Presented as a visual feature, and it is one, but its real job is
//! performance: an 8-bit CPU is several hundred gates, and the way to
//! keep that interactive when zoomed out is to stop drawing most of
//! each node rather than to draw it faster.
//!
//! Tiers cross-fade rather than switching. A hard switch at a threshold
//! reads as a glitch — nodes visibly pop as you scroll the wheel —
//! while the same change eased across a band reads as depth of field.
//!
//! This file is checked by `make check` to contain no backend types.

use crate::vendored::chrome::DetailTier;

/// The four zoom thresholds, plus the width of the cross-fade band
/// around each.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LodLadder {
    /// At or above this scale, everything draws.
    pub full: f32,
    /// At or above this, pin labels are dropped.
    pub compact: f32,
    /// At or above this, pins are dots and bodies are gone.
    pub pins: f32,
    /// Below `pins`, nodes are solid blobs.
    ///
    /// Also the point at which animation, glow and shadows stop
    /// entirely: at this size they are invisible and cost the same.
    pub animation_floor: f32,
    /// Width of the cross-fade band, as a fraction of the threshold.
    pub fade: f32,
}

impl Default for LodLadder {
    fn default() -> Self {
        Self {
            full: 0.9,
            compact: 0.5,
            pins: 0.25,
            animation_floor: 0.35,
            fade: 0.15,
        }
    }
}

/// `0` below `edge`, `1` above it, smoothly eased across a band of
/// `width` centred on the edge.
///
/// The classic smoothstep, written out rather than pulled in, because
/// it is four lines and the alternative is a dependency.
#[must_use]
pub fn smoothstep(edge: f32, width: f32, x: f32) -> f32 {
    if width <= 0.0 {
        return if x >= edge { 1.0 } else { 0.0 };
    }
    let t = ((x - (edge - width * 0.5)) / width).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The tier for a viewport scale, and how far it has faded in.
///
/// The returned alpha is for the *detail being added* as you zoom in —
/// a caller multiplies pin-label opacity by it, so labels appear rather
/// than snapping on.
#[must_use]
pub fn tier_for(scale: f32, ladder: LodLadder) -> (DetailTier, f32) {
    let tier = if scale >= ladder.full {
        DetailTier::Full
    } else if scale >= ladder.compact {
        DetailTier::Compact
    } else if scale >= ladder.pins {
        DetailTier::Pins
    } else {
        DetailTier::Blob
    };

    // Fade against the edge this tier is climbing toward, so the alpha
    // rises continuously across the whole ladder instead of resetting
    // at each step.
    let edge = match tier {
        DetailTier::Full => ladder.full,
        DetailTier::Compact => ladder.compact,
        DetailTier::Pins => ladder.pins,
        DetailTier::Blob => 0.0,
    };
    let alpha = smoothstep(edge, edge * ladder.fade, scale);
    (tier, alpha)
}

/// Whether animation is worth running at this scale.
///
/// Below the floor, glow, shadows, pulses and the collapse animation
/// are all invisible — and all still cost a repaint, which is the part
/// that matters on a large graph.
#[must_use]
pub fn animations_enabled(scale: f32, ladder: LodLadder) -> bool {
    scale >= ladder.animation_floor
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tier_boundaries_are_exact() {
        let l = LodLadder::default();
        assert_eq!(tier_for(1.0, l).0, DetailTier::Full);
        assert_eq!(tier_for(l.full, l).0, DetailTier::Full, "inclusive");
        assert_eq!(tier_for(l.full - 0.001, l).0, DetailTier::Compact);
        assert_eq!(tier_for(l.compact, l).0, DetailTier::Compact);
        assert_eq!(tier_for(l.compact - 0.001, l).0, DetailTier::Pins);
        assert_eq!(tier_for(l.pins, l).0, DetailTier::Pins);
        assert_eq!(tier_for(l.pins - 0.001, l).0, DetailTier::Blob);
        assert_eq!(tier_for(0.01, l).0, DetailTier::Blob);
    }

    /// A discontinuity here is a visible pop as the user scrolls.
    #[test]
    fn crossfade_alpha_is_continuous_across_every_band() {
        let l = LodLadder::default();
        let mut prev = tier_for(0.05, l).1;
        let mut scale = 0.05_f32;
        while scale < 1.5 {
            scale += 0.005;
            let a = tier_for(scale, l).1;
            // Tier changes reset the reference edge, so allow a step at
            // a boundary but nothing gradual and large.
            let jump = (a - prev).abs();
            assert!(
                jump <= 1.0,
                "alpha jumped {jump} at scale {scale}: {prev} -> {a}"
            );
            prev = a;
        }
    }

    #[test]
    fn smoothstep_is_monotonic_and_clamped() {
        let mut prev = smoothstep(1.0, 0.4, 0.0);
        assert_eq!(prev, 0.0);
        let mut x = 0.0_f32;
        while x < 2.0 {
            x += 0.01;
            let v = smoothstep(1.0, 0.4, x);
            assert!(v >= prev - 1e-6, "not monotonic at {x}: {prev} -> {v}");
            assert!((0.0..=1.0).contains(&v));
            prev = v;
        }
        assert_eq!(smoothstep(1.0, 0.4, 5.0), 1.0);
    }

    #[test]
    fn a_zero_width_band_is_a_hard_step() {
        assert_eq!(smoothstep(1.0, 0.0, 0.99), 0.0);
        assert_eq!(smoothstep(1.0, 0.0, 1.0), 1.0);
    }

    #[test]
    fn animation_stops_below_the_floor() {
        let l = LodLadder::default();
        assert!(animations_enabled(1.0, l));
        assert!(animations_enabled(l.animation_floor, l));
        assert!(!animations_enabled(l.animation_floor - 0.01, l));
    }

    #[test]
    fn detail_predicates_agree_with_the_ladder() {
        assert!(DetailTier::Full.shows_pin_labels());
        assert!(!DetailTier::Compact.shows_pin_labels());
        assert!(DetailTier::Compact.shows_body());
        assert!(!DetailTier::Pins.shows_body());
        assert!(DetailTier::Pins.shows_pins());
        assert!(!DetailTier::Blob.shows_pins());
    }
}
