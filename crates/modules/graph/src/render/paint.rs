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
use mara_core::paint::PaintVertex;
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
        // A hovered node lifts. The shadow doing the lifting rather
        // than a colour change is what makes it feel physical.
        let lift = if state.hovered { 1.6 } else { 1.0 };
        p.shadow(
            Rect::from_min_size(l.rect.min + sh.offset * lift, l.rect.size()),
            corner,
            [0, 0],
            (f32::from(sh.blur) * lift) as u8,
            0,
            sh.color,
        );
    }

    // Selection is a halo, not a fatter border. Two rings fading
    // outwards read as the node glowing; a thick outline just reads as
    // a thick outline.
    if state.selected {
        for (i, alpha) in [(1.0_f32, 110_u8), (2.6, 46), (4.6, 20)] {
            let g = l.rect.expand(i * 1.6);
            p.rect_stroke(
                g,
                CornerRadius::same(m.corner.saturating_add((i * 1.6) as u8)),
                Stroke::new(2.0, with_alpha(spec.palette.selection, alpha)),
            );
        }
    }

    p.rect_filled(l.rect, corner, spec.palette.node_fill);

    // The body falls off very slightly toward its foot, under the same
    // light the header gradient implies. Two surfaces lit from two
    // directions is what makes a card look assembled rather than made.
    if l.content.height() > 2.0 {
        let foot = darken(spec.palette.node_fill, 0.16);
        let top = with_alpha(foot, 0);
        let y0 = l.header.max.y;
        let y1 = l.rect.max.y - f32::from(m.corner);
        if y1 > y0 {
            p.mesh(
                vec![
                    PaintVertex {
                        pos: Pos2::new(l.rect.min.x, y0),
                        color: top,
                    },
                    PaintVertex {
                        pos: Pos2::new(l.rect.max.x, y0),
                        color: top,
                    },
                    PaintVertex {
                        pos: Pos2::new(l.rect.max.x, y1),
                        color: foot,
                    },
                    PaintVertex {
                        pos: Pos2::new(l.rect.min.x, y1),
                        color: foot,
                    },
                ],
                vec![0, 1, 2, 0, 2, 3],
            );
        }
    }

    if let Some(tint) = tint {
        paint_header(p, l.header, tint, m.corner, spec);
    }

    // The hairline that separates header from body. Without it the
    // tint bleeds into the body and the node loses its architecture.
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(l.rect.min.x, l.header.max.y - m.border),
            Pos2::new(l.rect.max.x, l.header.max.y),
        ),
        CornerRadius::same(0),
        spec.palette.divider,
    );

    let (border_col, border_w) = if state.selected {
        (spec.palette.selection, m.border_selected)
    } else if state.hovered {
        (spec.palette.node_border_hovered, m.border)
    } else {
        (spec.palette.node_border, m.border)
    };
    p.rect_stroke(l.rect, corner, Stroke::new(border_w, border_col));

    // One light line just inside the top edge. Cheapest possible
    // depth cue and the one that does the most work.
    let inset = f32::from(m.corner) * 0.7;
    p.rect_filled(
        Rect::from_min_max(
            Pos2::new(l.rect.min.x + inset, l.rect.min.y + border_w),
            Pos2::new(l.rect.max.x - inset, l.rect.min.y + border_w + 1.0),
        ),
        CornerRadius::same(0),
        spec.palette.specular,
    );

    // Text below the legibility floor is not small text, it is a grey
    // smear that makes a zoomed-out graph look dirty. Dropping it also
    // makes zooming out cheap.
    if m.title_size < MIN_LEGIBLE_PT {
        return;
    }

    // Icon glyphs go through ordinary text, not a named family: the
    // icon font is merged into the default family, and asking for it by
    // name in a context that never registered it yields `.notdef` —
    // a white box on every node.
    if let Some(icon) = &shape.icon
        && m.icon_size >= MIN_LEGIBLE_PT
        && let Some((glyph, _)) = mara_core::icons::icon_glyph(icon)
    {
        p.text(
            l.icon.center(),
            Align2::CENTER_CENTER,
            glyph,
            m.icon_size,
            with_alpha(spec.palette.title, 225),
        );
    }

    let title_anchor = if shape.subtitle.is_empty() {
        (l.title.center().y, Align2::LEFT_CENTER)
    } else {
        (l.title.max.y, Align2::LEFT_BOTTOM)
    };
    p.text(
        Pos2::new(l.title.min.x, title_anchor.0),
        title_anchor.1,
        fit(p, &shape.title, l.title.width(), m.title_size),
        m.title_size,
        spec.palette.title,
    );
    if !shape.subtitle.is_empty() && m.subtitle_size >= MIN_LEGIBLE_PT {
        p.text(
            Pos2::new(l.subtitle.min.x, l.subtitle.min.y),
            Align2::LEFT_TOP,
            fit(p, &shape.subtitle, l.subtitle.width(), m.subtitle_size),
            m.subtitle_size,
            spec.palette.subtitle,
        );
    }

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

/// Paint the header as a vertical gradient.
///
/// A flat band of colour is the single most dated thing a node can
/// wear. Falling from the tint at the top to a darker, part-transparent
/// version at the divider makes the header read as lit from above and
/// ties it into the body instead of sitting on top of it.
///
/// The rounded band stays flat colour and only the square remainder
/// below it carries the gradient, so the top corners survive.
fn paint_header(p: &MaraPainter, header: Rect, tint: Color32, corner: u8, spec: &GraphSpec) {
    let top = lighten(tint, 0.15);
    let bottom = darken(tint, 0.28);

    // The rounded part is flat colour. Only the square remainder below
    // the corner radius carries the gradient, which is what lets a
    // single seamless quad do the job: stacked strips leave a
    // half-transparent antialiased seam at every boundary, and seven of
    // those across a header look like corduroy.
    p.rect_filled(
        header,
        CornerRadius::from_corners(corner, corner, 0, 0),
        top,
    );

    let y0 = (header.min.y + f32::from(corner)).min(header.max.y);
    if y0 >= header.max.y {
        return;
    }
    let t0 = (y0 - header.min.y) / header.height().max(1.0);
    let c0 = lerp_color(top, bottom, t0);
    let verts = vec![
        PaintVertex {
            pos: Pos2::new(header.min.x, y0),
            color: c0,
        },
        PaintVertex {
            pos: Pos2::new(header.max.x, y0),
            color: c0,
        },
        PaintVertex {
            pos: Pos2::new(header.max.x, header.max.y),
            color: bottom,
        },
        PaintVertex {
            pos: Pos2::new(header.min.x, header.max.y),
            color: bottom,
        },
    ];
    p.mesh(verts, vec![0, 1, 2, 0, 2, 3]);
    let _ = spec;
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
    hot: bool,
    m: &NodeSpec,
    palette: &GraphPalette,
) {
    let r = if hot { m.pin_r * 1.35 } else { m.pin_r };
    // A halo under the pin the pointer is about to grab. Wiring is done
    // by aim, and a target that acknowledges the aim is far easier to
    // hit than one that stays inert until the click lands.
    if hot {
        p.circle_filled(at, r + m.pin_ring * 2.2, with_alpha(color, 60));
    }
    // A collar of body colour, then a dark rim. The collar stops the
    // node's outline cutting the pin in half; the rim keeps the pin
    // legible where it overhangs the canvas.
    p.circle_filled(at, r + m.pin_ring, palette.node_fill);
    p.circle_stroke(
        at,
        r + m.pin_ring * 0.5,
        Stroke::new(1.0, Color32::from_black_alpha(70)),
    );
    if filled {
        p.circle_filled(at, r, color);
        // A small bright cap on the upper-left, the highlight that
        // makes a flat disc read as a bead.
        p.circle_filled(
            Pos2::new(at.x - r * 0.28, at.y - r * 0.28),
            r * 0.34,
            with_alpha(lighten(color, 0.55), 150),
        );
    } else {
        p.circle_filled(at, r, palette.node_fill);
        p.circle_stroke(at, r - 0.5, Stroke::new((m.pin_ring * 0.7).max(1.0), color));
    }
}

/// Paint a wire between two pin anchors.
///
/// Sampled as a cubic whose control points reach horizontally, so a
/// wire leaves a pin sideways and arrives sideways — the shape that
/// reads as a connection rather than as a line that happens to touch.
/// A wire runs from its source pin's colour to its target pin's, which
/// tells you at a glance where a signal came from and what it became.
/// Painted as a run of short segments because a stroke carries one
/// colour; eight is enough that the banding is invisible.
pub fn paint_wire(
    p: &MaraPainter,
    from: Pos2,
    to: Pos2,
    from_color: Color32,
    to_color: Color32,
    width: f32,
    spec: &GraphSpec,
) {
    let pts = wire_points(from, to, spec.wire_slack);

    // A dark pass underneath, slightly wider. The wire then reads as
    // lying on the canvas rather than being painted into it, and stays
    // legible where it crosses a node.
    p.polyline(
        pts.clone(),
        Stroke::new(width + 2.0, Color32::from_black_alpha(60)),
    );

    const SEGS: usize = 8;
    let per = pts.len() / SEGS;
    for s in 0..SEGS {
        let a = s * per;
        let b = if s == SEGS - 1 { pts.len() } else { (s + 1) * per + 1 };
        if b <= a + 1 {
            continue;
        }
        let t = (s as f32 + 0.5) / SEGS as f32;
        p.polyline(
            pts[a..b.min(pts.len())].to_vec(),
            Stroke::new(width, lerp_color(from_color, to_color, t)),
        );
    }
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
    // Every fifth dot is brighter and larger. One even field of dots
    // gives no sense of distance; a major interval turns the same dots
    // into a ruler.
    const MAJOR: i32 = 5;
    let index = |min: f32, o: f32| ((min - o) / step).ceil() as i32;
    let (i0, j0) = (index(area.min.x, origin.x), index(area.min.y, origin.y));

    let mut j = j0;
    let mut y = origin.y + j as f32 * step;
    while y < area.max.y {
        let mut i = i0;
        let mut x = origin.x + i as f32 * step;
        while x < area.max.x {
            let major = i.rem_euclid(MAJOR) == 0 && j.rem_euclid(MAJOR) == 0;
            let (r, col) = if major {
                (1.5, spec.palette.grid_major)
            } else {
                (1.0, spec.palette.grid)
            };
            p.circle_filled(Pos2::new(x, y), r, col);
            i += 1;
            x += step;
        }
        j += 1;
        y += step;
    }
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let [ar, ag, ab, aa] = a.to_srgba_unmultiplied();
    let [br, bg, bb, ba] = b.to_srgba_unmultiplied();
    let mix = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t) as u8;
    Color32::from_rgba_unmultiplied(mix(ar, br), mix(ag, bg), mix(ab, bb), mix(aa, ba))
}

fn lighten(c: Color32, t: f32) -> Color32 {
    lerp_color(c, Color32::from_rgba_unmultiplied(255, 255, 255, c.a()), t)
}

fn darken(c: Color32, t: f32) -> Color32 {
    lerp_color(c, Color32::from_rgba_unmultiplied(0, 0, 0, c.a()), t)
}

fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
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
