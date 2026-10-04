use egui::Style;
use mara_core::MaraPainter;
use mara_core::vocab::{Color32, Rect, Stroke, Vec2, pos2, vec2};

use crate::vendored::{InPinId, OutPinId};

use super::{GraphStyle, WireStyle};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AnyPin {
    Out(OutPinId),
    In(InPinId),
}

/// In the current context, these are the I/O pins of the 'source' node that the newly
/// created node's I/O pins will connect to.
#[derive(Debug)]
pub enum AnyPins<'a> {
    /// Output pins.
    Out(&'a [OutPinId]),
    /// Input pins
    In(&'a [InPinId]),
}

/// Contains information about a pin's wire.
/// Used to draw the wire.
/// When two pins are connected, the wire is drawn between them,
/// using merged `PinWireInfo` from both pins.
pub struct PinWireInfo {
    /// Desired color of the wire.
    pub color: Color32,

    /// Desired style of the wire.
    /// Zoomed with current scale.
    pub style: WireStyle,
}

/// Uses `Painter` to draw a pin.
pub trait NodePin {
    /// Calculates pin Rect from the given parameters.
    fn pin_rect(&self, x: f32, y0: f32, y1: f32, size: f32) -> Rect {
        // Center vertically by default.
        let y = (y0 + y1) * 0.5;
        let pin_pos = pos2(x, y);
        Rect::from_center_size(pin_pos, vec2(size, size))
    }

    /// Draws the pin.
    ///
    /// `rect` is the interaction rectangle of the pin.
    /// Pin should fit in it.
    /// `painter` is used to add pin's shapes to the UI.
    ///
    /// Returns the color
    #[must_use]
    fn draw(
        self,
        graph_style: &GraphStyle,
        style: &Style,
        rect: Rect,
        painter: &MaraPainter,
    ) -> PinWireInfo;
}

/// Shape of a pin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PinShape {
    /// Circle shape.
    #[default]
    Circle,

    /// Triangle shape.
    Triangle,

    /// Square shape.
    Square,

    /// Star shape.
    Star,
}

/// Information about a pin returned by `NodeViewer::show_input` and `NodeViewer::show_output`.
///
/// All fields are optional.
/// If a field is `None`, the default value is used derived from the graph style.
#[derive(Default)]
pub struct PinInfo {
    /// Shape of the pin.
    pub shape: Option<PinShape>,

    /// Fill color of the pin.
    pub fill: Option<Color32>,

    /// Outline stroke of the pin.
    pub stroke: Option<Stroke>,

    /// Color of the wire connected to the pin.
    /// If `None`, the pin's fill color is used.
    pub wire_color: Option<Color32>,

    /// Style of the wire connected to the pin.
    pub wire_style: Option<WireStyle>,

    /// Custom vertical position of a pin
    pub position: Option<f32>,
}

impl PinInfo {
    /// Sets the shape of the pin.
    #[must_use]
    pub const fn with_shape(mut self, shape: PinShape) -> Self {
        self.shape = Some(shape);
        self
    }

    /// Sets the fill color of the pin.
    #[must_use]
    pub const fn with_fill(mut self, fill: Color32) -> Self {
        self.fill = Some(fill);
        self
    }

    /// Sets the outline stroke of the pin.
    #[must_use]
    pub const fn with_stroke(mut self, stroke: Stroke) -> Self {
        self.stroke = Some(stroke);
        self
    }

    /// Sets the style of the wire connected to the pin.
    #[must_use]
    pub const fn with_wire_style(mut self, wire_style: WireStyle) -> Self {
        self.wire_style = Some(wire_style);
        self
    }

    /// Sets the color of the wire connected to the pin.
    #[must_use]
    pub const fn with_wire_color(mut self, wire_color: Color32) -> Self {
        self.wire_color = Some(wire_color);
        self
    }

    /// Creates a circle pin.
    #[must_use]
    pub fn circle() -> Self {
        PinInfo {
            shape: Some(PinShape::Circle),
            ..Default::default()
        }
    }

    /// Creates a triangle pin.
    #[must_use]
    pub fn triangle() -> Self {
        PinInfo {
            shape: Some(PinShape::Triangle),
            ..Default::default()
        }
    }

    /// Creates a square pin.
    #[must_use]
    pub fn square() -> Self {
        PinInfo {
            shape: Some(PinShape::Square),
            ..Default::default()
        }
    }

    /// Creates a star pin.
    #[must_use]
    pub fn star() -> Self {
        PinInfo {
            shape: Some(PinShape::Star),
            ..Default::default()
        }
    }

    /// Returns the shape of the pin.
    #[must_use]
    pub fn get_shape(&self, graph_style: &GraphStyle) -> PinShape {
        self.shape.unwrap_or_else(|| graph_style.get_pin_shape())
    }

    /// Returns fill color of the pin.
    #[must_use]
    pub fn get_fill(&self, graph_style: &GraphStyle, style: &Style) -> Color32 {
        self.fill
            .unwrap_or_else(|| graph_style.get_pin_fill(style).into())
    }

    /// Returns outline stroke of the pin.
    #[must_use]
    pub fn get_stroke(&self, graph_style: &GraphStyle, style: &Style) -> Stroke {
        self.stroke.unwrap_or_else(|| {
            let s = graph_style.get_pin_stroke(style);
            Stroke::new(s.width, Color32::from(s.color))
        })
    }

    /// Draws the pin and returns color.
    ///
    /// Wires are drawn with returned color by default.
    #[must_use]
    pub fn draw(
        &self,
        graph_style: &GraphStyle,
        style: &Style,
        rect: Rect,
        painter: &MaraPainter,
    ) -> PinWireInfo {
        let shape = self.get_shape(graph_style);
        let fill = self.get_fill(graph_style, style);
        let stroke = self.get_stroke(graph_style, style);

        // Pin glow — 4-layer fake bloom under the crisp pin.
        // Each layer is a wider, alpha-reduced copy of the same
        // shape; the alpha of the four layers accumulates to a
        // smooth halo with no visible ring boundaries. Sizes
        // halved vs the earlier two-pass version so the halo
        // hugs the pin instead of bleeding into the row label.
        let glow = graph_style.pin_glow.unwrap_or(0.0).clamp(0.0, 1.5);
        if glow > 0.0 {
            // (expand_factor_of_width, alpha_factor) per layer,
            // outermost first.
            const GLOW_LAYERS: [(f32, f32); 4] =
                [(0.60, 0.08), (0.45, 0.13), (0.30, 0.20), (0.15, 0.28)];
            let no_stroke = Stroke::NONE;
            for (e_mul, a_mul) in GLOW_LAYERS {
                let a = (fill.a() as f32 * a_mul * glow).round().clamp(0.0, 220.0) as u8;
                let c = Color32::from_rgba_unmultiplied(fill.r(), fill.g(), fill.b(), a);
                let r = rect.expand(rect.width() * e_mul);
                draw_pin(painter, shape, c, no_stroke, r);
            }
        }

        draw_pin(painter, shape, fill, stroke, rect);

        PinWireInfo {
            color: self.wire_color.unwrap_or(fill),
            style: self
                .wire_style
                .unwrap_or_else(|| graph_style.get_wire_style()),
        }
    }
}

impl NodePin for PinInfo {
    fn draw(
        self,
        graph_style: &GraphStyle,
        style: &Style,
        rect: Rect,
        painter: &MaraPainter,
    ) -> PinWireInfo {
        Self::draw(&self, graph_style, style, rect, painter)
    }
}

pub fn draw_pin(painter: &MaraPainter, shape: PinShape, fill: Color32, stroke: Stroke, rect: Rect) {
    let center = rect.center();
    let size = f32::min(rect.width(), rect.height());

    match shape {
        PinShape::Circle => {
            painter.circle_filled(center, size / 2.0, fill);
            if stroke.width > 0.0 {
                painter.circle_stroke(center, size / 2.0, stroke);
            }
        }
        PinShape::Triangle => {
            const A: Vec2 = vec2(-0.649_519, 0.4875);
            const B: Vec2 = vec2(0.649_519, 0.4875);
            const C: Vec2 = vec2(0.0, -0.6375);

            let points = vec![center + A * size, center + B * size, center + C * size];

            painter.polygon(points, fill, stroke);
        }
        PinShape::Square => {
            let points = vec![
                center + vec2(-0.5, -0.5) * size,
                center + vec2(0.5, -0.5) * size,
                center + vec2(0.5, 0.5) * size,
                center + vec2(-0.5, 0.5) * size,
            ];

            painter.polygon(points, fill, stroke);
        }

        PinShape::Star => {
            let points = vec![
                center + size * 0.700_000 * vec2(0.0, -1.0),
                center + size * 0.267_376 * vec2(-0.587_785, -0.809_017),
                center + size * 0.700_000 * vec2(-0.951_057, -0.309_017),
                center + size * 0.267_376 * vec2(-0.951_057, 0.309_017),
                center + size * 0.700_000 * vec2(-0.587_785, 0.809_017),
                center + size * 0.267_376 * vec2(0.0, 1.0),
                center + size * 0.700_000 * vec2(0.587_785, 0.809_017),
                center + size * 0.267_376 * vec2(0.951_057, 0.309_017),
                center + size * 0.700_000 * vec2(0.951_057, -0.309_017),
                center + size * 0.267_376 * vec2(0.587_785, -0.809_017),
            ];

            painter.polygon(points, fill, stroke);
        }
    }
}
