//! Native colour picker — PLAN.md WS-E1.3.
//!
//! Draws hue / saturation / value (and optionally alpha) as gradient
//! bars built from [`PaintCmd::Mesh`], with a draggable handle each.
//! Every pixel comes from the paint IR, so this works on any backend —
//! unlike the egui colour picker it replaces, which was one of the
//! seven tracked `ui_escapes`.
//!
//! HSV rather than a wheel: a wheel needs either a per-pixel shader or
//! a generated texture, both of which would pull capability the sealed
//! surface does not have. Bars give the same control from primitives
//! that already exist, and read more precisely for keyboard-free
//! dragging.

use crate::layout::{ColorPickerAlpha, Sense, UiBackend};
use crate::paint::{PaintCmd, PaintVertex};
use crate::style::theme;
use crate::vocab;

const BAR_H: f32 = 14.0;
const BAR_GAP: f32 = 6.0;
/// Horizontal gradient resolution. Hue needs the most steps; 48 keeps
/// the banding invisible at any realistic bar width without making the
/// vertex count interesting.
const STEPS: usize = 48;

/// Show the picker and report whether `color` changed.
pub(crate) fn color_picker_backend(
    backend: &mut dyn UiBackend,
    color: &mut vocab::Color32,
    alpha: ColorPickerAlpha,
) -> bool {
    let (mut h, mut s, mut v) = rgb_to_hsv(color.r(), color.g(), color.b());
    let mut a = f32::from(color.a()) / 255.0;
    let show_alpha = alpha == ColorPickerAlpha::OnlyBlend;

    let mut changed = false;
    changed |= bar(backend, &mut h, |t| hsv_to_color(t, 1.0, 1.0, 1.0));
    changed |= bar(backend, &mut s, |t| hsv_to_color(h, t, v.max(0.05), 1.0));
    changed |= bar(backend, &mut v, |t| hsv_to_color(h, s, t, 1.0));
    if show_alpha {
        changed |= bar(backend, &mut a, |t| hsv_to_color(h, s, v, t));
    }

    if changed {
        let rgb = hsv_to_color(h, s, v, 1.0);
        *color = vocab::Color32::from_rgba_unmultiplied(
            rgb.r(),
            rgb.g(),
            rgb.b(),
            if show_alpha {
                (a * 255.0).round().clamp(0.0, 255.0) as u8
            } else {
                255
            },
        );
    }
    changed
}

/// One gradient bar with a handle. `value` is in `0.0..=1.0`;
/// `sample` gives the colour at a normalised position.
fn bar(
    backend: &mut dyn UiBackend,
    value: &mut f32,
    sample: impl Fn(f32) -> vocab::Color32,
) -> bool {
    let width = backend.available_width().max(32.0);
    let response = backend.allocate(vocab::Vec2::new(width, BAR_H + BAR_GAP), Sense::Drag);
    let rect = vocab::Rect::from_min_max(
        vocab::Pos2::new(response.rect.min.x, response.rect.min.y),
        vocab::Pos2::new(response.rect.max.x, response.rect.min.y + BAR_H),
    );

    let mut vertices = Vec::with_capacity((STEPS + 1) * 2);
    let mut indices = Vec::with_capacity(STEPS * 6);
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let x = rect.min.x + rect.width() * t;
        let color = sample(t);
        vertices.push(PaintVertex {
            pos: vocab::Pos2::new(x, rect.min.y),
            color,
        });
        vertices.push(PaintVertex {
            pos: vocab::Pos2::new(x, rect.max.y),
            color,
        });
    }
    for step in 0..STEPS as u32 {
        let i = step * 2;
        indices.extend_from_slice(&[i, i + 1, i + 3, i, i + 3, i + 2]);
    }
    backend.paint(PaintCmd::Mesh { vertices, indices });

    let palette = theme().palette;
    backend.paint(PaintCmd::RectStroke {
        rect,
        corner: vocab::CornerRadius::same(2),
        stroke: vocab::Stroke::new(1.0, palette.border_subtle),
    });

    // Drag anywhere on the bar sets the value — matching how every
    // slider behaves, rather than requiring the handle to be grabbed.
    let mut changed = false;
    if response.dragged()
        && let Some(pointer) = response.interact_pointer
    {
        let t = ((pointer.x - rect.min.x) / rect.width().max(1.0)).clamp(0.0, 1.0);
        if (t - *value).abs() > f32::EPSILON {
            *value = t;
            changed = true;
        }
    }

    let handle_x = rect.min.x + rect.width() * value.clamp(0.0, 1.0);
    backend.paint(PaintCmd::RectFilled {
        rect: vocab::Rect::from_min_max(
            vocab::Pos2::new(handle_x - 1.5, rect.min.y - 2.0),
            vocab::Pos2::new(handle_x + 1.5, rect.max.y + 2.0),
        ),
        corner: vocab::CornerRadius::same(1),
        fill: palette.text_primary,
    });
    changed
}

/// sRGB bytes to HSV, each component normalised to `0.0..=1.0`.
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let (r, g, b) = (
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let hue = if delta <= f32::EPSILON {
        0.0
    } else if max == r {
        ((g - b) / delta).rem_euclid(6.0) / 6.0
    } else if max == g {
        ((b - r) / delta + 2.0) / 6.0
    } else {
        ((r - g) / delta + 4.0) / 6.0
    };
    let saturation = if max <= f32::EPSILON {
        0.0
    } else {
        delta / max
    };
    (hue, saturation, max)
}

/// HSV (each `0.0..=1.0`) plus alpha to an sRGB colour.
fn hsv_to_color(h: f32, s: f32, v: f32, a: f32) -> vocab::Color32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let byte = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    vocab::Color32::from_rgba_unmultiplied(
        byte(r),
        byte(g),
        byte(b),
        (a * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_primary_colours() {
        for (r, g, b) in [
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 0),
            (18, 200, 130),
            (0, 0, 0),
            (255, 255, 255),
        ] {
            let (h, s, v) = rgb_to_hsv(r, g, b);
            let back = hsv_to_color(h, s, v, 1.0);
            assert_eq!(
                (back.r(), back.g(), back.b()),
                (r, g, b),
                "round trip lost ({r},{g},{b}) via hsv ({h},{s},{v})"
            );
        }
    }

    #[test]
    fn grey_has_no_hue_and_no_saturation() {
        let (h, s, v) = rgb_to_hsv(128, 128, 128);
        assert_eq!(h, 0.0);
        assert_eq!(s, 0.0);
        assert!((v - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn hue_wraps_rather_than_clamping() {
        assert_eq!(
            hsv_to_color(0.0, 1.0, 1.0, 1.0),
            hsv_to_color(1.0, 1.0, 1.0, 1.0)
        );
        assert_eq!(
            hsv_to_color(0.25, 1.0, 1.0, 1.0),
            hsv_to_color(1.25, 1.0, 1.0, 1.0)
        );
    }
}
