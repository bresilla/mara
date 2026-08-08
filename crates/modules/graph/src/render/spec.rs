//! Every number the node renderer uses, in one place.
//!
//! The old renderer had its metrics scattered across the widget, the
//! style struct, the theme and three call sites, and they drifted:
//! nodes ended up different widths because one path added an inline
//! editor and another did not, header bands did not line up with body
//! edges, and text escaped its box because the truncation budget was
//! computed from a different padding than the one that was painted.
//!
//! Collecting them here is the fix. A reviewer can read the whole
//! visual system in one screen, and a change to `ROW_H` moves every
//! row together instead of moving some of them.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::vocab::{Color32, Vec2};

/// Fixed geometry for a node.
///
/// All lengths are in graph points at scale 1. Nothing here is derived
/// from what the app draws — that is the whole point. A node's size is
/// decided from its *declared* shape (title, pin counts, whether it has
/// a body) before anything is painted, so two nodes with the same shape
/// are always the same size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NodeSpec {
    /// Every node is this wide. Not a minimum — an actual width.
    ///
    /// A node editor reads as a system when its nodes align, and they
    /// only align if they share an edge. Letting content decide width
    /// is what produced the ragged columns this renderer replaces.
    pub width: f32,
    /// Height of every pin row.
    pub row_h: f32,
    /// Height of the header band.
    ///
    /// The same on every node whether or not it has a subtitle. A
    /// header that grew for a second line would give a row of nodes
    /// with mixed subtitles a ragged top edge — the exact effect this
    /// renderer exists to remove.
    pub header_h: f32,
    /// Size of the header icon glyph.
    pub icon_size: f32,
    /// Size of the header's second line.
    pub subtitle_size: f32,
    /// Padding inside the body, left and right.
    pub pad_x: f32,
    /// Gap above the first row and below the last.
    pub pad_y: f32,
    /// Share of the node's width reserved for an unconnected input's
    /// value editor. Fixed rather than content-sized, so growing an
    /// editor never widens the node.
    pub editor_frac: f32,
    /// Corner radius of the body.
    pub corner: u8,
    /// Radius of a pin disc.
    pub pin_r: f32,
    /// Width of the body-coloured ring behind a pin, which is what lets
    /// a pin straddle the node's outline without looking like a gap.
    pub pin_ring: f32,
    /// Title text size.
    pub title_size: f32,
    /// Pin label text size.
    pub label_size: f32,
    /// Border width at rest.
    pub border: f32,
    /// Border width when selected.
    pub border_selected: f32,
}

impl Default for NodeSpec {
    fn default() -> Self {
        Self {
            width: 196.0,
            row_h: 21.0,
            header_h: 38.0,
            icon_size: 14.0,
            subtitle_size: 9.5,
            pad_x: 10.0,
            pad_y: 6.0,
            editor_frac: 0.42,
            corner: 9,
            pin_r: 5.0,
            pin_ring: 2.5,
            title_size: 13.5,
            label_size: 11.5,
            border: 1.0,
            border_selected: 2.0,
        }
    }
}

impl NodeSpec {
    /// The same node metrics at a camera zoom of `z`.
    ///
    /// Zooming scales the whole spec and lays out again, rather than
    /// laying out once and scaling the result. That keeps text crisp at
    /// every zoom — the font is rasterised at the size it is drawn —
    /// and is why this renderer needs none of the offscreen
    /// re-rendering the previous one used to stay sharp.
    #[must_use]
    pub fn scaled(self, z: f32) -> Self {
        Self {
            width: self.width * z,
            row_h: self.row_h * z,
            header_h: self.header_h * z,
            icon_size: self.icon_size * z,
            subtitle_size: self.subtitle_size * z,
            pad_x: self.pad_x * z,
            pad_y: self.pad_y * z,
            editor_frac: self.editor_frac,
            corner: (f32::from(self.corner) * z).round().clamp(0.0, 255.0) as u8,
            pin_r: self.pin_r * z,
            pin_ring: self.pin_ring * z,
            title_size: self.title_size * z,
            label_size: self.label_size * z,
            border: (self.border * z).max(1.0),
            border_selected: (self.border_selected * z).max(1.5),
        }
    }
}

/// Colours the renderer needs that are not per-node.
///
/// Deliberately few. Everything else a node shows — its header tint,
/// its pin colours — comes from the app, because the crate has no way
/// to know what those mean.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphPalette {
    /// The canvas behind everything.
    pub canvas: Color32,
    /// Grid / dot pattern on the canvas.
    pub grid: Color32,
    /// Node body fill. Must read as *raised* against `canvas` — the
    /// single most important relationship in the whole view, and the
    /// one the previous renderer got wrong: body and canvas landed
    /// about eight levels apart and every node read as a hole.
    pub node_fill: Color32,
    /// Node border at rest.
    pub node_border: Color32,
    /// Node border under the pointer.
    pub node_border_hovered: Color32,
    /// Border and halo of a selected node.
    pub selection: Color32,
    /// A wire the app expressed no opinion about.
    pub wire: Color32,
    /// Wash inside the rubber-band selection rectangle.
    pub selection_fill: Color32,
    /// Title text.
    pub title: Color32,
    /// Pin label text.
    pub label: Color32,
    /// The header's second line — dimmer than the title, or the two
    /// lines fight and neither reads first.
    pub subtitle: Color32,
    /// Rule under the header when a node has no colour of its own.
    pub divider: Color32,
    /// Brighter dots on the grid's major intervals, so the canvas has a
    /// sense of scale instead of an even field of noise.
    pub grid_major: Color32,
}

impl GraphPalette {
    /// Derive a palette from a base surface colour and an accent.
    ///
    /// `dark` picks the direction every lift and recession moves in, so
    /// one function serves both themes instead of a light theme getting
    /// a muddy canvas from a hard-coded darkening.
    #[must_use]
    pub fn from_surface(surface: Color32, accent: Color32, dark: bool) -> Self {
        let shift = |c: Color32, amount: f32| -> Color32 {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            let target = if dark { 255.0 } else { 0.0 };
            let mix = |v: u8| (f32::from(v) + (target - f32::from(v)) * amount) as u8;
            Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
        };
        let sink = |c: Color32, amount: f32| -> Color32 {
            let [r, g, b, a] = c.to_srgba_unmultiplied();
            let target = if dark { 0.0 } else { 255.0 };
            let mix = |v: u8| (f32::from(v) + (target - f32::from(v)) * amount) as u8;
            Color32::from_rgba_unmultiplied(mix(r), mix(g), mix(b), a)
        };

        let canvas = sink(surface, 0.35);
        Self {
            canvas,
            grid: shift(canvas, 0.17),
            // The lift that makes a node an object. Opaque on purpose:
            // a translucent body lets the canvas through and no amount
            // of lightening then separates the two.
            node_fill: shift(canvas, 0.19),
            node_border: shift(canvas, 0.30),
            node_border_hovered: shift(canvas, 0.48),
            selection: accent,
            selection_fill: Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36),
            wire: shift(canvas, 0.42),
            title: if dark {
                Color32::from_gray(238)
            } else {
                Color32::from_gray(24)
            },
            divider: shift(canvas, 0.36),
            grid_major: shift(canvas, 0.36),
            subtitle: if dark {
                Color32::from_gray(190)
            } else {
                Color32::from_gray(96)
            },
            label: if dark {
                Color32::from_gray(205)
            } else {
                Color32::from_gray(84)
            },
        }
    }
}

/// The whole visual system for one graph view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GraphSpec {
    pub node: NodeSpec,
    pub palette: GraphPalette,
    /// Spacing of the background dot grid. `None` draws no pattern.
    pub grid_spacing: Option<f32>,
    /// Wire thickness at rest.
    pub wire_width: f32,
    /// How far a wire's control point reaches horizontally, as a
    /// fraction of the horizontal gap between its ends.
    pub wire_slack: f32,
    /// Drop shadow under a node. `None` for none.
    pub shadow: Option<ShadowSpec>,
}

/// A node's drop shadow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShadowSpec {
    pub offset: Vec2,
    pub blur: u8,
    pub color: Color32,
}

impl GraphSpec {
    /// The whole visual system at a camera zoom of `z`.
    ///
    /// Layout and paint are both driven from the *same* scaled spec, so
    /// they cannot disagree about how big a padding is. Handing the
    /// layout a scaled spec and the painter an unscaled one produced
    /// exactly the symptom this rewrite set out to remove: full-size
    /// boxes with unscaled text and pins inside them.
    #[must_use]
    pub fn scaled(self, z: f32) -> Self {
        Self {
            node: self.node.scaled(z),
            grid_spacing: self.grid_spacing.map(|s| s * z),
            wire_width: (self.wire_width * z).max(1.0),
            shadow: self.shadow.map(|s| ShadowSpec {
                offset: s.offset * z,
                blur: (f32::from(s.blur) * z).round().clamp(0.0, 255.0) as u8,
                ..s
            }),
            ..self
        }
    }

    /// The default look, derived from a surface colour and an accent.
    #[must_use]
    pub fn from_surface(surface: Color32, accent: Color32, dark: bool) -> Self {
        Self {
            node: NodeSpec::default(),
            palette: GraphPalette::from_surface(surface, accent, dark),
            grid_spacing: Some(28.0),
            wire_width: 2.0,
            wire_slack: 0.5,
            shadow: Some(ShadowSpec {
                offset: Vec2::new(0.0, 2.0),
                blur: 12,
                color: Color32::from_black_alpha(95),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn luma(c: Color32) -> f32 {
        let [r, g, b, _] = c.to_srgba_unmultiplied();
        0.299 * f32::from(r) + 0.587 * f32::from(g) + 0.114 * f32::from(b)
    }

    /// The relationship the previous renderer got wrong, asserted so it
    /// cannot regress: a node body must be clearly separated from the
    /// canvas it sits on, in both themes.
    #[test]
    fn a_node_reads_as_raised_against_the_canvas() {
        for (surface, dark) in [
            (Color32::from_gray(30), true),
            (Color32::from_gray(240), false),
        ] {
            let p = GraphPalette::from_surface(surface, Color32::from_rgb(90, 150, 220), dark);
            let gap = (luma(p.node_fill) - luma(p.canvas)).abs();
            assert!(
                gap >= 12.0,
                "node/canvas separation is only {gap:.1} levels (dark={dark})"
            );
            if dark {
                assert!(
                    luma(p.node_fill) > luma(p.canvas),
                    "in a dark theme the node must be the lighter of the two"
                );
            } else {
                assert!(luma(p.node_fill) < luma(p.canvas));
            }
        }
    }

    /// The body is opaque. A translucent node lets the canvas through,
    /// and then no amount of lightening separates them.
    #[test]
    fn the_node_body_is_opaque() {
        let p = GraphPalette::from_surface(Color32::from_gray(30), Color32::WHITE, true);
        assert_eq!(p.node_fill.a(), 255);
    }

    /// The border has to sit between the body and the canvas in
    /// lightness, or it reads as a second body edge rather than as an
    /// outline.
    #[test]
    fn the_border_is_lighter_than_the_body_it_outlines() {
        let p = GraphPalette::from_surface(Color32::from_gray(30), Color32::WHITE, true);
        assert!(luma(p.node_border) > luma(p.node_fill));
    }

    #[test]
    fn text_contrasts_with_the_body_in_both_themes() {
        for (surface, dark) in [
            (Color32::from_gray(30), true),
            (Color32::from_gray(240), false),
        ] {
            let p = GraphPalette::from_surface(surface, Color32::WHITE, dark);
            let gap = (luma(p.title) - luma(p.node_fill)).abs();
            assert!(gap > 90.0, "title contrast is only {gap:.1} (dark={dark})");
        }
    }
}
