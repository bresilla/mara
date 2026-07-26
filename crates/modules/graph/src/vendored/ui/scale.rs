//! Zoom scaling for graph style types — PLAN.md WS-D1.5.
//!
//! `mara_graph` used to reach for `egui-scale` here. That is a backend
//! dependency, and the only thing it did that this crate could not do
//! itself was scale a backend `Style` — which now lives behind
//! [`mara_core::MaraUi::scale_style`], where backend concerns belong.
//!
//! What is left is scaling this crate's *own* style structs, which is
//! arithmetic over their fields. Doing it here keeps the dependency out
//! of a sealed module and makes the behaviour readable rather than
//! derived.

use super::{BackgroundPattern, GraphStyle, PinPlacement, SelectionStyle, WireStyle};

/// Multiply every length-like field by `scale`.
///
/// Colors, counts, and flags are left alone — only distances,
/// thicknesses, and radii move with zoom.
pub trait Scale {
    fn scale(&mut self, scale: f32);
}

impl Scale for f32 {
    fn scale(&mut self, scale: f32) {
        *self *= scale;
    }
}

impl Scale for egui::Vec2 {
    fn scale(&mut self, scale: f32) {
        *self *= scale;
    }
}

impl Scale for egui::Margin {
    fn scale(&mut self, scale: f32) {
        self.left = (f32::from(self.left) * scale) as i8;
        self.right = (f32::from(self.right) * scale) as i8;
        self.top = (f32::from(self.top) * scale) as i8;
        self.bottom = (f32::from(self.bottom) * scale) as i8;
    }
}

impl Scale for egui::CornerRadius {
    fn scale(&mut self, scale: f32) {
        self.nw = (f32::from(self.nw) * scale) as u8;
        self.ne = (f32::from(self.ne) * scale) as u8;
        self.sw = (f32::from(self.sw) * scale) as u8;
        self.se = (f32::from(self.se) * scale) as u8;
    }
}

impl Scale for egui::Stroke {
    fn scale(&mut self, scale: f32) {
        self.width *= scale;
    }
}

impl Scale for egui::epaint::Shadow {
    fn scale(&mut self, scale: f32) {
        self.offset = [
            (f32::from(self.offset[0]) * scale) as i8,
            (f32::from(self.offset[1]) * scale) as i8,
        ];
        self.blur = (f32::from(self.blur) * scale) as u8;
        self.spread = (f32::from(self.spread) * scale) as u8;
    }
}

impl Scale for mara_core::style::MarginSpec {
    fn scale(&mut self, scale: f32) {
        self.left = (f32::from(self.left) * scale) as i8;
        self.right = (f32::from(self.right) * scale) as i8;
        self.top = (f32::from(self.top) * scale) as i8;
        self.bottom = (f32::from(self.bottom) * scale) as i8;
    }
}

impl Scale for mara_core::vocab::CornerRadius {
    fn scale(&mut self, scale: f32) {
        let scaled = self
            .corners()
            .map(|r| (f32::from(r) * scale).round().clamp(0.0, 255.0) as u8);
        let [nw, ne, se, sw] = scaled;
        *self = mara_core::vocab::CornerRadius::from_corners(nw, ne, sw, se);
    }
}

impl Scale for mara_core::vocab::Stroke {
    fn scale(&mut self, scale: f32) {
        self.width *= scale;
    }
}

impl Scale for mara_core::style::FrameShadowSpec {
    fn scale(&mut self, scale: f32) {
        self.offset = [
            (f32::from(self.offset[0]) * scale) as i8,
            (f32::from(self.offset[1]) * scale) as i8,
        ];
        self.blur = (f32::from(self.blur) * scale) as u8;
        self.spread = (f32::from(self.spread) * scale) as u8;
    }
}

impl Scale for mara_core::style::FrameSpec {
    fn scale(&mut self, scale: f32) {
        self.inner_margin.scale(scale);
        self.outer_margin.scale(scale);
        self.corner.scale(scale);
        self.shadow.scale(scale);
        self.stroke.scale(scale);
    }
}

/// An absent override stays absent — scaling must not materialise a
/// value the caller never set, or the style would stop falling back to
/// its default.
impl<T: Scale> Scale for Option<T> {
    fn scale(&mut self, scale: f32) {
        if let Some(inner) = self {
            inner.scale(scale);
        }
    }
}

impl Scale for WireStyle {
    fn scale(&mut self, scale: f32) {
        match self {
            WireStyle::Line | WireStyle::Bezier3 | WireStyle::Bezier5 => {}
            WireStyle::AxisAligned { corner_radius } => {
                corner_radius.scale(scale);
            }
        }
    }
}

impl Scale for SelectionStyle {
    fn scale(&mut self, scale: f32) {
        self.margin.scale(scale);
        self.rounding.scale(scale);
        self.stroke.scale(scale);
    }
}

impl Scale for PinPlacement {
    fn scale(&mut self, scale: f32) {
        if let PinPlacement::Outside { margin } = self {
            margin.scale(scale);
        }
    }
}

impl Scale for BackgroundPattern {
    fn scale(&mut self, scale: f32) {
        if let BackgroundPattern::Grid(grid) = self {
            grid.spacing = grid.spacing * scale;
        }
    }
}

impl Scale for GraphStyle {
    fn scale(&mut self, scale: f32) {
        self.node_frame.scale(scale);
        self.header_frame.scale(scale);
        self.header_drag_space.scale(scale);
        self.pin_size.scale(scale);
        self.pin_stroke.scale(scale);
        self.pin_placement.scale(scale);
        self.wire_width.scale(scale);
        self.wire_frame_size.scale(scale);
        self.wire_style.scale(scale);
        self.bg_frame.scale(scale);
        self.bg_pattern.scale(scale);
        self.bg_pattern_stroke.scale(scale);
        self.min_scale.scale(scale);
        self.max_scale.scale(scale);
        self.select_stoke.scale(scale);
        self.select_style.scale(scale);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reason `Option` is scaled rather than defaulted: an unset
    /// override must stay unset so the style keeps falling back.
    #[test]
    fn scaling_leaves_absent_overrides_absent() {
        let mut none: Option<f32> = None;
        none.scale(2.0);
        assert_eq!(none, None);

        let mut some = Some(3.0_f32);
        some.scale(2.0);
        assert_eq!(some, Some(6.0));
    }

    #[test]
    fn scaling_a_stroke_moves_width_but_not_color() {
        let mut stroke = egui::Stroke::new(2.0, egui::Color32::RED);
        stroke.scale(2.5);
        assert_eq!(stroke.width, 5.0);
        assert_eq!(stroke.color, egui::Color32::RED);
    }

    #[test]
    fn scaling_a_graph_style_reaches_nested_overrides() {
        let mut style = GraphStyle {
            pin_size: Some(4.0),
            wire_width: Some(2.0),
            ..GraphStyle::default()
        };
        style.scale(3.0);
        assert_eq!(style.pin_size, Some(12.0));
        assert_eq!(style.wire_width, Some(6.0));
    }
}
