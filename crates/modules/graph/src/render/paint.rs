//! Draw what [`super::layout`] decided.
//!
//! This module owns no geometry. Every rect it paints was handed to it,
//! which is what stops the painter and the layout disagreeing — the
//! disagreement that let text escape its box in the renderer this
//! replaces.
//!
//! It also draws through a [`MaraPainter`] it is given rather than one
//! it fetches, so a caller can hand it a clipped canvas painter and know
//! nothing will be drawn outside the viewport.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::mui::MaraPainter;
use mara_core::vocab::{Align2, Color32, CornerRadius, Pos2, Rect, Stroke};

use super::layout::{NodeLayout, NodeShape};
use super::spec::{GraphPalette, GraphSpec, NodeSpec};

/// Point size below which text is dropped rather than drawn.
const MIN_LEGIBLE_PT: f32 = 7.0;

/// How a node is being interacted with this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeState {
    pub selected: bool,
    pub hovered: bool,
}

/// Paint one node's body, header and labels.
///
/// `tint` is the app's colour for this node, and it is spent on the
/// header band and nothing else. The crate contributes no colour of its
/// own to a node at rest, so a graph shows exactly as many colours as
/// the app chose — which is why the nodes here read as a set rather
/// than as a pile of unrelated widgets.
pub fn paint_node(
    p: &MaraPainter,
    l: &NodeLayout,
    shape: &NodeShape,
    tint: Option<Color32>,
    state: NodeState,
    spec: &GraphSpec,
) {
    // Metrics come from the layout, colours from the spec. A caller
    // cannot hand the painter geometry the layout did not use.
    let m = l.spec;
    let corner = CornerRadius::same(m.corner);

    if let Some(sh) = spec.shadow {
        p.shadow(
            Rect::from_min_size(l.rect.min + sh.offset, l.rect.size()),
            corner,
            [0, 0],
            sh.blur,
            0,
            sh.color,
        );
    }

    p.rect_filled(l.rect, corner, spec.palette.node_fill);

    if let Some(tint) = tint {
        let c = m.corner;
        p.rect_filled(l.header, CornerRadius::from_corners(c, c, 0, 0), tint);
    }

    // One ring. Selection replaces it rather than adding to it, so a
    // selected node reads as emphasised instead of merely busier.
    let (border_col, border_w) = if state.selected {
        (spec.palette.selection, m.border_selected)
    } else if state.hovered {
        (spec.palette.node_border_hovered, m.border)
    } else {
        (spec.palette.node_border, m.border)
    };
    p.rect_stroke(l.rect, corner, Stroke::new(border_w, border_col));

    // Text below the legibility floor is not small text, it is a grey
    // smear that makes a zoomed-out graph look dirty. Dropping it also
    // makes zooming out cheap.
    if m.title_size < MIN_LEGIBLE_PT {
        return;
    }

    p.text(
        Pos2::new(l.header.min.x + m.pad_x, l.header.center().y),
        Align2::LEFT_CENTER,
        fit(
            p,
            &shape.title,
            l.header.width() - 2.0 * m.pad_x,
            m.title_size,
        ),
        m.title_size,
        spec.palette.title,
    );

    if m.label_size < MIN_LEGIBLE_PT {
        return;
    }

    for (i, r) in l.input_labels.iter().enumerate() {
        p.text(
            Pos2::new(r.min.x, r.center().y),
            Align2::LEFT_CENTER,
            fit(p, &shape.inputs[i], r.width(), m.label_size),
            m.label_size,
            spec.palette.label,
        );
    }
    for (i, r) in l.output_labels.iter().enumerate() {
        p.text(
            Pos2::new(r.max.x, r.center().y),
            Align2::RIGHT_CENTER,
            fit(p, &shape.outputs[i], r.width(), m.label_size),
            m.label_size,
            spec.palette.label,
        );
    }
}

/// Paint one pin disc.
///
/// The ring is the node's own fill, so a pin sitting half over the body
/// edge still reads as a disc rather than as a bite taken out of the
/// outline.
pub fn paint_pin(
    p: &MaraPainter,
    at: Pos2,
    color: Color32,
    filled: bool,
    m: &NodeSpec,
    palette: &GraphPalette,
) {
    let r = m.pin_r;
    p.circle_filled(at, r + m.pin_ring, palette.node_fill);
    if filled {
        p.circle_filled(at, r, color);
    } else {
        p.circle_filled(at, r, palette.node_fill);
        p.circle_stroke(at, r - 0.5, Stroke::new((m.pin_ring * 0.75).max(1.0), color));
    }
}

/// Paint a wire between two pin anchors.
///
/// Sampled as a cubic whose control points reach horizontally, so a
/// wire leaves a pin sideways and arrives sideways — the shape that
/// reads as a connection rather than as a line that happens to touch.
pub fn paint_wire(p: &MaraPainter, from: Pos2, to: Pos2, color: Color32, width: f32, spec: &GraphSpec) {
    let pts = wire_points(from, to, spec.wire_slack);
    p.polyline(pts, Stroke::new(width, color));
}

/// Sample the wire curve between two anchors.
///
/// Public so hit-testing measures distance to the same curve the
/// painter drew, rather than to a straight line that only agrees with
/// it at the ends.
#[must_use]
pub fn wire_points(from: Pos2, to: Pos2, slack: f32) -> Vec<Pos2> {
    let reach = ((to.x - from.x).abs() * slack).clamp(28.0, 180.0);
    let c1 = Pos2::new(from.x + reach, from.y);
    let c2 = Pos2::new(to.x - reach, to.y);

    let steps = 24;
    let mut pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let u = 1.0 - t;
        let x =
            u * u * u * from.x + 3.0 * u * u * t * c1.x + 3.0 * u * t * t * c2.x + t * t * t * to.x;
        let y =
            u * u * u * from.y + 3.0 * u * u * t * c1.y + 3.0 * u * t * t * c2.y + t * t * t * to.y;
        pts.push(Pos2::new(x, y));
    }
    pts
}

/// Smallest on-screen grid spacing that still reads as a grid.
///
/// Below this the dots stop being a spatial reference and become
/// texture — which is what a zoomed-out graph looked like when the
/// spacing simply scaled with the camera.
const GRID_MIN_PX: f32 = 18.0;

/// Paint the canvas and its dot grid.
///
/// `step` is doubled until it clears [`GRID_MIN_PX`], so zooming out
/// drops grid lines a rank at a time — like a ruler — instead of
/// crushing them into noise. Doubling keeps the surviving dots on the
/// same graph coordinates they had before.
pub fn paint_canvas(p: &MaraPainter, area: Rect, origin: Pos2, step: f32, spec: &GraphSpec) {
    p.rect_filled(area, CornerRadius::same(0), spec.palette.canvas);
    if spec.grid_spacing.is_none() || step <= 0.0 {
        return;
    }
    let mut step = step;
    while step < GRID_MIN_PX {
        step *= 2.0;
    }
    let first = |min: f32, o: f32| o + ((min - o) / step).ceil() * step;
    let mut y = first(area.min.y, origin.y);
    while y < area.max.y {
        let mut x = first(area.min.x, origin.x);
        while x < area.max.x {
            p.circle_filled(Pos2::new(x, y), 1.0, spec.palette.grid);
            x += step;
        }
        y += step;
    }
}

/// Truncate `text` to `budget`, measured rather than estimated.
///
/// The renderer this replaces guessed a per-character width and
/// truncated against that, which is why titles ran past their box for
/// some strings and stopped short for others.
fn fit(p: &MaraPainter, text: &str, budget: f32, size: f32) -> String {
    if budget <= 0.0 {
        return String::new();
    }
    if p.measure_text(text, size, false).x <= budget {
        return text.to_string();
    }
    let ellipsis = "…";
    if p.measure_text(ellipsis, size, false).x > budget {
        return String::new();
    }
    let chars: Vec<char> = text.chars().collect();
    let (mut lo, mut hi) = (0usize, chars.len());
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        let candidate: String = chars[..mid].iter().collect::<String>() + ellipsis;
        if p.measure_text(&candidate, size, false).x <= budget {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    chars[..lo].iter().collect::<String>() + ellipsis
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wire must start and end exactly on the pins it joins. If the
    /// sampled curve missed its endpoints, every wire would appear to
    /// float a pixel off its pin.
    #[test]
    fn a_wire_touches_both_of_its_pins() {
        let a = Pos2::new(10.0, 20.0);
        let b = Pos2::new(300.0, 140.0);
        let pts = wire_points(a, b, 0.5);
        assert!((pts[0].x - a.x).abs() < 0.01 && (pts[0].y - a.y).abs() < 0.01);
        let last = pts[pts.len() - 1];
        assert!((last.x - b.x).abs() < 0.01 && (last.y - b.y).abs() < 0.01);
    }

    /// A wire leaves its source rightwards and enters its target from
    /// the left even when the target is behind the source — the case a
    /// straight line renders as an ambiguous diagonal.
    #[test]
    fn a_backwards_wire_still_leaves_and_arrives_sideways() {
        let from = Pos2::new(300.0, 100.0);
        let to = Pos2::new(20.0, 100.0);
        let pts = wire_points(from, to, 0.5);
        assert!(pts[1].x > from.x, "wire must leave the output rightwards");
        assert!(
            pts[pts.len() - 2].x < to.x,
            "wire must arrive at the input from the left"
        );
    }
}
