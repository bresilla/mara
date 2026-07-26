//! This module provides functionality for showing [`Graph`] graph in [`Ui`].

use std::{collections::HashMap, hash::Hash};

use egui::{
    Align, CornerRadius, Id, LayerId, Layout, Margin, Modifiers, PointerButton,
    Scene, Sense, StrokeKind, Style, Ui, UiBuilder, UiKind, UiStackInfo,
    collapsing_header::paint_default_icon,
    emath::{GuiRounding, TSTransform},
    response::Flags,
};
use mara_core::MaraResponse;
use mara_core::vocab::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use mara_core::style::{FrameRole, FrameSpec, frame_for};
use smallvec::SmallVec;

use crate::vendored::{Graph, InPin, InPinId, Node, NodeId, OutPin, OutPinId, ui::wire::WireId};

mod background_pattern;
mod pin;
mod scale;
mod state;
mod viewer;
mod wire;

use self::scale::Scale;
use self::{
    pin::AnyPin,
    state::{NewWires, NodeState, RowHeights},
    wire::{draw_wire, hit_wire, pick_wire_style},
};

pub use self::{
    background_pattern::{BackgroundPattern, Dots, Grid, Hex},
    pin::{AnyPins, NodePin, PinInfo, PinShape},
    state::GraphState,
    viewer::NodeViewer,
    wire::{WireColorMode, WireLayer, WireStyle},
};

/// Controls how header, pins, body and footer are placed in the node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum NodeLayoutKind {
    /// Input pins, body and output pins are placed horizontally.
    /// With header on top and footer on bottom.
    ///
    /// +---------------------+
    /// |       Header        |
    /// +----+-----------+----+
    /// | In |           | Out|
    /// | In |   Body    | Out|
    /// | In |           | Out|
    /// | In |           |    |
    /// +----+-----------+----+
    /// |       Footer        |
    /// +---------------------+
    ///
    #[default]
    Coil,

    /// All elements are placed in vertical stack.
    /// Header is on top, then input pins, body, output pins and footer.
    ///
    /// +---------------------+
    /// |       Header        |
    /// +---------------------+
    /// | In                  |
    /// | In                  |
    /// | In                  |
    /// | In                  |
    /// +---------------------+
    /// |       Body          |
    /// +---------------------+
    /// |                 Out |
    /// |                 Out |
    /// |                 Out |
    /// +---------------------+
    /// |       Footer        |
    /// +---------------------+
    Sandwich,

    /// All elements are placed in vertical stack.
    /// Header is on top, then output pins, body, input pins and footer.
    ///
    /// +---------------------+
    /// |       Header        |
    /// +---------------------+
    /// |                 Out |
    /// |                 Out |
    /// |                 Out |
    /// +---------------------+
    /// |       Body          |
    /// +---------------------+
    /// | In                  |
    /// | In                  |
    /// | In                  |
    /// | In                  |
    /// +---------------------+
    /// |       Footer        |
    /// +---------------------+
    FlippedSandwich,
    // TODO: Add vertical layouts.
}

/// Controls how node elements are laid out.
///
///
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeLayout {
    /// Controls method of laying out node elements.
    pub kind: NodeLayoutKind,

    /// Controls minimal height of pin rows.
    pub min_pin_row_height: f32,

    /// Controls how pin rows heights are set.
    /// If true, all pin rows will have the same height, matching the largest content.
    /// False by default.
    pub equal_pin_row_heights: bool,
}

impl NodeLayout {
    /// Creates new [`NodeLayout`] with `Coil` kind and flexible pin heights.
    #[must_use]
    #[inline]
    pub const fn coil() -> Self {
        NodeLayout {
            kind: NodeLayoutKind::Coil,
            min_pin_row_height: 0.0,
            equal_pin_row_heights: false,
        }
    }

    /// Creates new [`NodeLayout`] with `Sandwich` kind and flexible pin heights.
    #[must_use]
    #[inline]
    pub const fn sandwich() -> Self {
        NodeLayout {
            kind: NodeLayoutKind::Sandwich,
            min_pin_row_height: 0.0,
            equal_pin_row_heights: false,
        }
    }

    /// Creates new [`NodeLayout`] with `FlippedSandwich` kind and flexible pin heights.
    #[must_use]
    #[inline]
    pub const fn flipped_sandwich() -> Self {
        NodeLayout {
            kind: NodeLayoutKind::FlippedSandwich,
            min_pin_row_height: 0.0,
            equal_pin_row_heights: false,
        }
    }

    /// Returns new [`NodeLayout`] with same `kind` and specified pin heights.
    #[must_use]
    #[inline]
    pub const fn with_equal_pin_rows(self) -> Self {
        NodeLayout {
            kind: self.kind,
            min_pin_row_height: self.min_pin_row_height,
            equal_pin_row_heights: true,
        }
    }

    /// Returns new [`NodeLayout`] with same `kind` and specified minimum pin row height.
    #[must_use]
    #[inline]
    pub const fn with_min_pin_row_height(self, min_pin_row_height: f32) -> Self {
        NodeLayout {
            kind: self.kind,
            min_pin_row_height,
            equal_pin_row_heights: self.equal_pin_row_heights,
        }
    }
}

impl From<NodeLayoutKind> for NodeLayout {
    #[inline]
    fn from(kind: NodeLayoutKind) -> Self {
        NodeLayout {
            kind,
            min_pin_row_height: 0.0,
            equal_pin_row_heights: false,
        }
    }
}

impl Default for NodeLayout {
    #[inline]
    fn default() -> Self {
        NodeLayout::coil()
    }
}

#[derive(Clone, Copy, Debug)]
enum OuterHeights<'a> {
    Flexible { rows: &'a [f32] },
    Matching { max: f32 },
    Tight,
}

#[derive(Clone, Copy, Debug)]
struct Heights<'a> {
    rows: &'a [f32],
    outer: OuterHeights<'a>,
    min_outer: f32,
}

impl Heights<'_> {
    fn get(&self, idx: usize) -> (f32, f32) {
        let inner = match self.rows.get(idx) {
            Some(&value) => value,
            None => 0.0,
        };

        let outer = match &self.outer {
            OuterHeights::Flexible { rows } => match rows.get(idx) {
                Some(&outer) => outer.max(inner),
                None => inner,
            },
            OuterHeights::Matching { max } => max.max(inner),
            OuterHeights::Tight => inner,
        };

        (inner, outer.max(self.min_outer))
    }
}

impl NodeLayout {
    fn input_heights(self, state: &NodeState) -> Heights<'_> {
        let rows = state.input_heights().as_slice();

        let outer = match (self.kind, self.equal_pin_row_heights) {
            (NodeLayoutKind::Coil, false) => OuterHeights::Flexible {
                rows: state.output_heights().as_slice(),
            },
            (_, true) => {
                let mut max_height = 0.0f32;
                for &h in state.input_heights() {
                    max_height = max_height.max(h);
                }
                for &h in state.output_heights() {
                    max_height = max_height.max(h);
                }
                OuterHeights::Matching { max: max_height }
            }
            (_, false) => OuterHeights::Tight,
        };

        Heights {
            rows,
            outer,
            min_outer: self.min_pin_row_height,
        }
    }

    fn output_heights(self, state: &'_ NodeState) -> Heights<'_> {
        let rows = state.output_heights().as_slice();

        let outer = match (self.kind, self.equal_pin_row_heights) {
            (NodeLayoutKind::Coil, false) => OuterHeights::Flexible {
                rows: state.input_heights().as_slice(),
            },
            (_, true) => {
                let mut max_height = 0.0f32;
                for &h in state.input_heights() {
                    max_height = max_height.max(h);
                }
                for &h in state.output_heights() {
                    max_height = max_height.max(h);
                }
                OuterHeights::Matching { max: max_height }
            }
            (_, false) => OuterHeights::Tight,
        };

        Heights {
            rows,
            outer,
            min_outer: self.min_pin_row_height,
        }
    }
}

/// Controls style of node selection rect.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SelectionStyle {
    /// Margin between selection rect and node frame.
    pub margin: Margin,

    /// Rounding of selection rect.
    pub rounding: CornerRadius,

    /// Fill color of selection rect.
    pub fill: Color32,

    /// Stroke of selection rect.
    pub stroke: Stroke,
}

/// Accent halo painted around each node body. Graph reserves a
/// shape slot in the painter buffer BEFORE the body + pins are
/// submitted, then fills that slot with a rounded-rectangle
/// stroke at `body_rect.expand(gap)`. Because the slot is
/// earlier in the buffer than the pin shapes, pins render ON TOP
/// of the halo where they intersect.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NodeHalo {
    /// Stroke colour. Typically the host's accent.
    pub color: Color32,
    /// Distance in points from the body edge to the halo line.
    /// `0` paints the halo on the body edge; positive values push
    /// it outward.
    pub gap: f32,
    /// Stroke width in points.
    pub width: f32,
    /// Corner radius of the halo rect. Should be ≥ body radius
    /// + gap so the halo follows the body's rounded corners.
    pub radius: u8,
}

impl Default for NodeHalo {
    fn default() -> Self {
        Self {
            color: Color32::WHITE,
            gap: 4.0,
            width: 1.5,
            radius: 8,
        }
    }
}

/// Controls how pins are placed in the node.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum PinPlacement {
    /// Pins are placed inside the node frame.
    #[default]
    Inside,

    /// Pins are placed on the edge of the node frame.
    Edge,

    /// Pins are placed outside the node frame.
    Outside {
        /// Margin between node frame and pins.
        margin: f32,
    },
}

/// Style for rendering Graph.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GraphStyle {
    /// Controls how nodes are laid out.
    /// Defaults to [`NodeLayoutKind::Coil`].
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub node_layout: Option<NodeLayout>,

    /// Frame used to draw nodes.
    /// Defaults to [`Frame::window`] constructed from current ui's style.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub node_frame: Option<FrameSpec>,

    /// Frame used to draw node headers.
    /// Defaults to [`node_frame`] without shadow and transparent fill.
    ///
    /// If set, it should not have shadow and fill should be either opaque of fully transparent
    /// unless layering of header fill color with node fill color is desired.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub header_frame: Option<FrameSpec>,

    /// Blank space for dragging node by its header.
    /// Elements in the header are placed after this space.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub header_drag_space: Option<Vec2>,

    /// Whether nodes can be collapsed.
    /// If true, headers will have collapsing button.
    /// When collapsed, node will not show its pins, body and footer.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub collapsible: Option<bool>,

    /// Size of pins.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_size: Option<f32>,

    /// Default fill color for pins.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_fill: Option<Color32>,

    /// Default stroke for pins.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_stroke: Option<Stroke>,

    /// Shape of pins.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_shape: Option<PinShape>,

    /// Placement of pins.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_placement: Option<PinPlacement>,

    /// Width of wires.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_width: Option<f32>,

    /// Size of wire frame which controls curvature of wires.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_frame_size: Option<f32>,

    /// Whether to downscale wire frame when nodes are close.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub downscale_wire_frame: Option<bool>,

    /// Weather to upscale wire frame when nodes are far.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub upscale_wire_frame: Option<bool>,

    /// Controls default style of wires.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_style: Option<WireStyle>,

    /// Layer where wires are rendered.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_layer: Option<WireLayer>,

    /// How the colour of a wire is derived from its endpoint pins.
    /// Defaults to [`WireColorMode::Mix`] (Blender-style gradient
    /// between source and target pin colours). Set to
    /// [`WireColorMode::FromSource`] for the Unreal Blueprints
    /// look — every wire takes the *output* pin's colour
    /// uniformly along its length.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_color_mode: Option<WireColorMode>,

    /// Faux-bloom intensity for wires (`0.0` = none, `1.0` = strong).
    /// Implemented as additional draw passes at increasing widths
    /// and decreasing alpha, painted under the crisp wire — the
    /// stack reads as a soft glow around each wire similar to
    /// post-process bloom in a 3D engine. Default `0.0` (off).
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_glow: Option<f32>,

    /// Faux-bloom intensity for pin glyphs (`0.0` = none,
    /// `1.0` = strong). Same multi-pass approach as
    /// [`GraphStyle::wire_glow`] but applied to pin shapes —
    /// pins shed a soft halo in their type colour. Default `0.0`.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_glow: Option<f32>,

    /// Extra inset (px) applied to [`PinPlacement::Inside`] —
    /// pins are pushed this many additional pixels toward the
    /// node's centre on the input AND output side. Default `0.0`
    /// preserves upstream layout. Useful for editors that want
    /// the pin glyph to sit *inside* the body's content area
    /// rather than flush with the inner margin.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub pin_inset: Option<f32>,

    /// Optional accent halo painted around each node body. Drawn
    /// in the painter buffer BEFORE pin glyphs so pins always
    /// render on top of the halo line — `final_node_rect`
    /// painted halos always end up above pins because they
    /// submit shapes after pins. Default `None` (no halo).
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub node_halo: Option<NodeHalo>,

    /// Frame used to draw background
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub bg_frame: Option<FrameSpec>,

    /// Background pattern.
    /// Defaults to [`BackgroundPattern::Grid`].
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub bg_pattern: Option<BackgroundPattern>,

    /// Stroke for background pattern.
    /// Defaults to `ui.visuals().widgets.noninteractive.bg_stroke`.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub bg_pattern_stroke: Option<Stroke>,

    /// Minimum viewport scale that can be set.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub min_scale: Option<f32>,

    /// Maximum viewport scale that can be set.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub max_scale: Option<f32>,

    /// Enable centering by double click on background
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub centering: Option<bool>,

    /// Stroke for selection.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub select_stoke: Option<Stroke>,

    /// Fill for selection.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub select_fill: Option<Color32>,

    /// Flag to control how rect selection works.
    /// If set to true, only nodes fully contained in selection rect will be selected.
    /// If set to false, nodes intersecting with selection rect will be selected.
    pub select_rect_contained: Option<bool>,

    /// Style for node selection.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub select_style: Option<SelectionStyle>,

    /// Controls whether to show magnified text in crisp mode.
    /// This zooms UI style to max scale and scales down the scene.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub crisp_magnified_text: Option<bool>,

    /// Controls smoothness of wire curves.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub wire_smoothness: Option<f32>,

    #[doc(hidden)]
    #[cfg_attr(feature = "serde", serde(skip_serializing, default))]
    /// Do not access other than with .., here to emulate `#[non_exhaustive(pub)]`
    pub _non_exhaustive: (),
}

impl GraphStyle {
    fn get_node_layout(&self) -> NodeLayout {
        self.node_layout.unwrap_or_default()
    }

    fn get_pin_size(&self, style: &Style) -> f32 {
        self.pin_size.unwrap_or(style.spacing.interact_size.y * 0.6)
    }

    fn get_pin_fill(&self, style: &Style) -> Color32 {
        self.pin_fill
            .unwrap_or_else(|| style.visuals.widgets.active.bg_fill.into())
    }

    fn get_pin_stroke(&self, style: &Style) -> Stroke {
        self.pin_stroke.unwrap_or_else(|| {
            Stroke::new(
                style.visuals.widgets.active.bg_stroke.width,
                style.visuals.widgets.active.bg_stroke.color.into(),
            )
        })
    }

    fn get_pin_shape(&self) -> PinShape {
        self.pin_shape.unwrap_or(PinShape::Circle)
    }

    fn get_pin_placement(&self) -> PinPlacement {
        self.pin_placement.unwrap_or_default()
    }

    fn get_wire_width(&self, style: &Style) -> f32 {
        self.wire_width
            .unwrap_or_else(|| self.get_pin_size(style) * 0.1)
    }

    fn get_wire_frame_size(&self, style: &Style) -> f32 {
        self.wire_frame_size
            .unwrap_or_else(|| self.get_pin_size(style) * 3.0)
    }

    fn get_downscale_wire_frame(&self) -> bool {
        self.downscale_wire_frame.unwrap_or(true)
    }

    fn get_upscale_wire_frame(&self) -> bool {
        self.upscale_wire_frame.unwrap_or(false)
    }

    fn get_wire_style(&self) -> WireStyle {
        self.wire_style.unwrap_or(WireStyle::Bezier5)
    }

    fn get_wire_layer(&self) -> WireLayer {
        self.wire_layer.unwrap_or(WireLayer::BehindNodes)
    }

    fn get_wire_color_mode(&self) -> WireColorMode {
        self.wire_color_mode.unwrap_or(WireColorMode::Mix)
    }

    fn get_wire_glow(&self) -> f32 {
        self.wire_glow.unwrap_or(0.0).clamp(0.0, 1.5)
    }

    #[allow(dead_code)] // mirrors `get_wire_glow`; kept symmetric for future
    // pin-halo render paths that will read it.
    fn get_pin_glow(&self) -> f32 {
        self.pin_glow.unwrap_or(0.0).clamp(0.0, 1.5)
    }

    fn get_pin_inset(&self) -> f32 {
        self.pin_inset.unwrap_or(0.0).max(0.0)
    }

    fn get_header_drag_space(&self, style: &Style) -> Vec2 {
        self.header_drag_space
            .unwrap_or_else(|| vec2(style.spacing.icon_width, style.spacing.icon_width))
    }

    fn get_collapsible(&self) -> bool {
        self.collapsible.unwrap_or(true)
    }

    fn get_bg_frame(&self, accent: mara_core::vocab::Color32) -> FrameSpec {
        self.bg_frame
            .unwrap_or_else(|| frame_for(FrameRole::Canvas, accent))
    }

    fn get_bg_pattern_stroke(&self, style: &Style) -> Stroke {
        self.bg_pattern_stroke
            .unwrap_or_else(|| style.visuals.widgets.noninteractive.bg_stroke.into())
    }

    fn get_min_scale(&self) -> f32 {
        self.min_scale.unwrap_or(0.2)
    }

    fn get_max_scale(&self) -> f32 {
        self.max_scale.unwrap_or(2.0)
    }

    fn get_node_frame(&self, accent: mara_core::vocab::Color32) -> FrameSpec {
        self.node_frame
            .unwrap_or_else(|| frame_for(FrameRole::Window, accent))
    }

    /// The header sits on top of the node body, so it must not cast its
    /// own shadow over it.
    fn get_header_frame(&self, accent: mara_core::vocab::Color32) -> FrameSpec {
        self.header_frame.unwrap_or_else(|| {
            let mut frame = self.get_node_frame(accent);
            frame.shadow = None;
            frame
        })
    }

    fn get_centering(&self) -> bool {
        self.centering.unwrap_or(true)
    }

    fn get_select_stroke(&self, style: &Style) -> Stroke {
        self.select_stoke.unwrap_or_else(|| {
            Stroke::new(
                style.visuals.selection.stroke.width,
                style.visuals.selection.stroke.color.gamma_multiply(0.5).into(),
            )
        })
    }

    fn get_select_fill(&self, style: &Style) -> Color32 {
        self.select_fill
            .unwrap_or_else(|| style.visuals.selection.bg_fill.gamma_multiply(0.3).into())
    }

    fn get_select_rect_contained(&self) -> bool {
        self.select_rect_contained.unwrap_or(false)
    }

    fn get_select_style(&self, style: &Style) -> SelectionStyle {
        self.select_style.unwrap_or_else(|| SelectionStyle {
            margin: style.spacing.window_margin,
            rounding: style.visuals.window_corner_radius,
            fill: self.get_select_fill(style),
            stroke: self.get_select_stroke(style),
        })
    }

    fn get_crisp_magnified_text(&self) -> bool {
        self.crisp_magnified_text.unwrap_or(false)
    }

    fn get_wire_smoothness(&self) -> f32 {
        self.wire_smoothness.unwrap_or(1.0)
    }
}

impl GraphStyle {
    /// Creates new [`GraphStyle`] filled with default values.
    #[must_use]
    pub const fn new() -> Self {
        GraphStyle {
            node_layout: None,
            pin_size: None,
            pin_fill: None,
            pin_stroke: None,
            pin_shape: None,
            pin_placement: None,
            wire_width: None,
            wire_frame_size: None,
            downscale_wire_frame: None,
            upscale_wire_frame: None,
            wire_style: None,
            wire_layer: None,
            wire_color_mode: None,
            wire_glow: None,
            pin_glow: None,
            pin_inset: None,
            node_halo: None,
            header_drag_space: None,
            collapsible: None,

            bg_frame: None,
            bg_pattern: None,
            bg_pattern_stroke: None,

            min_scale: None,
            max_scale: None,
            node_frame: None,
            header_frame: None,
            centering: None,
            select_stoke: None,
            select_fill: None,
            select_rect_contained: None,
            select_style: None,
            crisp_magnified_text: None,
            wire_smoothness: None,

            _non_exhaustive: (),
        }
    }
}

impl Default for GraphStyle {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

struct DrawNodeResponse {
    node_moved: Option<(NodeId, Vec2)>,
    node_to_top: Option<NodeId>,
    drag_released: bool,
    pin_hovered: Option<AnyPin>,
    final_rect: Rect,
}

struct DrawPinsResponse {
    drag_released: bool,
    pin_hovered: Option<AnyPin>,
    final_rect: Rect,
    new_heights: RowHeights,
}

struct DrawBodyResponse {
    final_rect: Rect,
}

struct PinResponse {
    pos: Pos2,
    wire_color: Color32,
    wire_style: WireStyle,
}

/// Widget to display [`Graph`] graph in [`Ui`].
#[derive(Clone, Copy, Debug)]
pub struct GraphWidget {
    id_salt: Id,
    id: Option<Id>,
    style: GraphStyle,
    min_size: Vec2,
    max_size: Vec2,
}

impl Default for GraphWidget {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl GraphWidget {
    /// Returns new [`GraphWidget`] with default parameters.
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        GraphWidget {
            id_salt: Id::new(":graph:"),
            id: None,
            style: GraphStyle::new(),
            min_size: Vec2::ZERO,
            max_size: Vec2::INFINITY,
        }
    }

    /// Assign an explicit and globally unique [`Id`].
    ///
    /// Use this if you want to persist the state of the widget
    /// when it changes position in the widget hierarchy.
    ///
    /// Prefer using [`GraphWidget::id_salt`] otherwise.
    #[inline]
    #[must_use]
    pub const fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    /// Assign a source for the unique [`Id`]
    ///
    /// It must be locally unique for the current [`Ui`] hierarchy position.
    ///
    /// Ignored if [`GraphWidget::id`] was set.
    #[inline]
    #[must_use]
    pub fn id_salt(mut self, id_salt: impl Hash) -> Self {
        self.id_salt = Id::new(id_salt);
        self
    }

    /// Set style parameters for the [`Graph`] widget.
    #[inline]
    #[must_use]
    pub const fn style(mut self, style: GraphStyle) -> Self {
        self.style = style;
        self
    }

    /// Set minimum size of the [`Graph`] widget.
    #[inline]
    #[must_use]
    pub const fn min_size(mut self, min_size: Vec2) -> Self {
        self.min_size = min_size;
        self
    }

    /// Set maximum size of the [`Graph`] widget.
    #[inline]
    #[must_use]
    pub const fn max_size(mut self, max_size: Vec2) -> Self {
        self.max_size = max_size;
        self
    }

    #[inline]
    fn get_id(&self, ui_id: Id) -> Id {
        self.id.unwrap_or_else(|| ui_id.with(self.id_salt))
    }

    /// Render [`Graph`] using given viewer and style into the [`Ui`].
    ///
    /// Returns the graph area's interaction, in Mara vocabulary — a
    /// caller never has to name a backend response type to ask whether
    /// the canvas was clicked or dragged.
    #[inline]
    pub fn show<T, V>(&self, graph: &mut Graph<T>, viewer: &mut V, ui: &mut Ui) -> MaraResponse
    where
        V: NodeViewer<T>,
    {
        let graph_id = self.get_id(ui.id());

        show_graph(
            graph_id,
            self.style,
            self.min_size,
            self.max_size,
            graph,
            viewer,
            ui,
        )
    }
}

#[inline(never)]
fn show_graph<T, V>(
    graph_id: Id,
    mut style: GraphStyle,
    min_size: Vec2,
    max_size: Vec2,
    graph: &mut Graph<T>,
    viewer: &mut V,
    ui: &mut Ui,
) -> MaraResponse
where
    V: NodeViewer<T>,
{
    #![allow(clippy::too_many_lines)]

    let (mut latest_pos, modifiers) = ui.ctx().input(|i| (i.pointer.latest_pos(), i.modifiers));

    let bg_frame = style.get_bg_frame(mara_core::style::active_accent());
    let bg_frame_backend = mara_core::backend::egui::egui_frame_for_style_spec(bg_frame);

    let outer_size_bounds = egui::Vec2::from(
        Vec2::from(ui.available_size_before_wrap())
            .max(min_size)
            .min(max_size),
    );

    let outer_resp = ui.allocate_response(outer_size_bounds, Sense::hover());

    ui.painter().add(bg_frame_backend.paint(outer_resp.rect));

    let mut content_rect = egui::Rect::from(
        mara_core::vocab::Rect::from(outer_resp.rect).shrink_by(bg_frame.total_margin()),
    );

    // Make sure we don't shrink to the negative:
    content_rect.max.x = content_rect.max.x.max(content_rect.min.x);
    content_rect.max.y = content_rect.max.y.max(content_rect.min.y);

    let graph_layer_id = LayerId::new(ui.layer_id().order, graph_id);

    ui.ctx().set_sublayer(ui.layer_id(), graph_layer_id);

    let mut min_scale = style.get_min_scale();
    let mut max_scale = style.get_max_scale();

    let ui_rect = content_rect;

    let mut graph_state =
        GraphState::load(ui.ctx(), graph_id, graph, ui_rect, min_scale, max_scale);
    let mut to_global = graph_state.to_global();

    let clip_rect = ui.clip_rect();

    let mut ui = ui.new_child(
        UiBuilder::new()
            .ui_stack_info(UiStackInfo::new(UiKind::Frame).with_frame(bg_frame_backend))
            .layer_id(graph_layer_id)
            .max_rect(egui::Rect::from(Rect::EVERYTHING))
            .sense(Sense::click_and_drag()),
    );

    if style.get_crisp_magnified_text() {
        style.scale(max_scale);
        let mut raw = mara_core::MaraUi::__internal_backend_from_raw(&mut ui);
        mara_core::MaraUi::__internal_over(&mut raw, mara_core::vocab::Color32::WHITE)
            .scale_style(max_scale);

        min_scale /= max_scale;
        max_scale = 1.0;
    }

    clamp_scale(&mut to_global, min_scale, max_scale, ui_rect.into());

    let mut graph_resp = ui.response();
    // `to_global` is Mara-typed everywhere else; the backend's gesture
    // driver is the one place that still needs its own transform type,
    // so convert across that call and back.
    let mut backend_transform = TSTransform {
        scaling: to_global.scaling,
        translation: to_global.translation.into(),
    };
    Scene::new()
        .zoom_range(min_scale..=max_scale)
        .register_pan_and_zoom(&ui, &mut graph_resp, &mut backend_transform);
    to_global = mara_core::transform::Transform::new(
        backend_transform.translation.into(),
        backend_transform.scaling,
    );

    if graph_resp.changed() {
        ui.ctx().request_repaint();
    }

    // Inform viewer about current transform.
    viewer.current_transform(&mut to_global, graph);

    graph_state.set_to_global(to_global);

    let to_global = to_global;
    let from_global = to_global.inverse();

    // Graph viewport
    let viewport = egui::Rect::from(from_global.mul_rect(ui_rect.into())).round_ui();
    let viewport_clip = egui::Rect::from(from_global.mul_rect(clip_rect.into()));

    ui.set_clip_rect(viewport.intersect(viewport_clip));
    ui.expand_to_include_rect(viewport);

    // Set transform for graph layer.
    with_mara_ui(&mut ui, |mara| mara.set_layer_transform(to_global));

    // Map latest pointer position to graph space.
    latest_pos = latest_pos.map(|pos| egui::Pos2::from(from_global.mul_pos(pos.into())));

    viewer.draw_background(
        style.bg_pattern.as_ref(),
        &viewport.into(),
        &style,
        ui.style(),
        ui.painter(),
        graph,
    );

    let mut node_moved = None;
    let mut node_to_top = None;

    // Process selection rect.
    let mut rect_selection_ended = None;
    if modifiers.shift || graph_state.is_rect_selection() {
        let select_resp = ui.interact(graph_resp.rect, graph_id.with("select"), Sense::drag());

        if select_resp.dragged_by(PointerButton::Primary)
            && let Some(pos) = select_resp.interact_pointer_pos()
        {
            if graph_state.is_rect_selection() {
                graph_state.update_rect_selection(pos);
            } else {
                graph_state.start_rect_selection(pos);
            }
        }

        if select_resp.drag_stopped_by(PointerButton::Primary) {
            if let Some(select_rect) = graph_state.rect_selection() {
                rect_selection_ended = Some(select_rect);
            }
            graph_state.stop_rect_selection();
        }
    }

    let wire_frame_size = style.get_wire_frame_size(ui.style());
    let wire_width = style.get_wire_width(ui.style());
    let wire_threshold = style.get_wire_smoothness();

    let wire_shape_idx = match style.get_wire_layer() {
        WireLayer::BehindNodes => Some(with_mara_ui(&mut ui, |mara| mara.reserve_paint_slot())),
        WireLayer::AboveNodes => None,
    };

    let mut input_info = HashMap::new();
    let mut output_info = HashMap::new();

    let mut pin_hovered = None;

    let draw_order = graph_state.update_draw_order(graph);
    let mut drag_released = false;

    let mut nodes_bb = Rect::NOTHING;
    let mut node_rects = Vec::new();

    for node_idx in draw_order {
        if !graph.nodes.contains(node_idx.0) {
            continue;
        }

        // show_node(node_idx);
        let response = draw_node(
            graph,
            &mut ui,
            node_idx,
            viewer,
            &mut graph_state,
            &style,
            graph_id,
            &mut input_info,
            modifiers,
            &mut output_info,
        );

        if let Some(response) = response {
            if let Some(v) = response.node_to_top {
                node_to_top = Some(v);
            }
            if let Some(v) = response.node_moved {
                node_moved = Some(v);
            }
            if let Some(v) = response.pin_hovered {
                pin_hovered = Some(v);
            }
            drag_released |= response.drag_released;

            nodes_bb = nodes_bb.union(response.final_rect);
            if rect_selection_ended.is_some() {
                node_rects.push((node_idx, response.final_rect));
            }
        }
    }

    let mut hovered_wire = None;
    let mut hovered_wire_disconnect = false;
    let mut wire_shapes: Vec<mara_core::paint::PaintCmd> = Vec::new();
    // The seam while `ui.rs` is still egui-typed (PLAN.md WS-D1.3):
    // `wire.rs` speaks Mara memory + a clip rect, so build them once.
    let mut wire_memory = mara_core::memory::MaraMemoryCtx::__internal_from_backend_ctx(ui.ctx());
    let wire_clip: mara_core::vocab::Rect = ui.clip_rect().into();

    // Draw and interact with wires
    for wire in graph.wires.iter() {
        let Some(from_r) = output_info.get(&wire.out_pin) else {
            continue;
        };
        let Some(to_r) = input_info.get(&wire.in_pin) else {
            continue;
        };

        if !graph_state.has_new_wires() && graph_resp.contains_pointer() && hovered_wire.is_none() {
            // Try to find hovered wire
            // If not dragging new wire
            // And not hovering over item above.

            if let Some(latest_pos) = latest_pos {
                let wire_hit = hit_wire(
                    &mut wire_memory,
                    WireId::Connected {
                        graph_id: graph_id.into(),
                        out_pin: wire.out_pin,
                        in_pin: wire.in_pin,
                    },
                    wire_frame_size,
                    style.get_upscale_wire_frame(),
                    style.get_downscale_wire_frame(),
                    from_r.pos.into(),
                    to_r.pos.into(),
                    latest_pos.into(),
                    wire_width.max(2.0),
                    pick_wire_style(from_r.wire_style, to_r.wire_style),
                );

                if wire_hit {
                    hovered_wire = Some(wire);

                    let wire_r =
                        ui.interact(graph_resp.rect, ui.make_persistent_id(wire), Sense::click());

                    //Remove hovered wire by second click
                    hovered_wire_disconnect |= wire_r.clicked_by(PointerButton::Secondary);
                }
            }
        }

        let color = match style.get_wire_color_mode() {
            WireColorMode::Mix => mix_colors(from_r.wire_color, to_r.wire_color),
            WireColorMode::FromSource => from_r.wire_color,
            WireColorMode::FromTarget => to_r.wire_color,
        };

        let mut draw_width = wire_width;
        if hovered_wire == Some(wire) {
            draw_width *= 1.5;
        }

        // Wire glow — multi-stroke fake bloom with a smooth
        // gaussian-ish falloff. We paint N alpha-reduced layers
        // UNDER the crisp wire, each narrower and brighter than
        // the last. Halved widths vs the earlier two-pass version
        // so the halo stays close to the line and doesn't wash
        // into adjacent wires; the extra layers smooth out the
        // visible "ring" boundaries you got with only 2 passes.
        let glow = style.get_wire_glow();
        if glow > 0.0 {
            // (width_factor, alpha_factor) per layer, outermost first.
            const GLOW_LAYERS: [(f32, f32); 4] =
                [(2.0, 0.08), (1.7, 0.12), (1.4, 0.18), (1.2, 0.25)];
            for (w_mul, a_mul) in GLOW_LAYERS {
                let layer_color = with_alpha_factor(color, a_mul * glow);
                draw_wire(
                    &mut wire_memory,
                    wire_clip,
                    WireId::Connected {
                        graph_id: graph_id.into(),
                        out_pin: wire.out_pin,
                        in_pin: wire.in_pin,
                    },
                    &mut wire_shapes,
                    wire_frame_size,
                    style.get_upscale_wire_frame(),
                    style.get_downscale_wire_frame(),
                    from_r.pos.into(),
                    to_r.pos.into(),
                    mara_core::vocab::Stroke::new(
                        draw_width * w_mul,
                        mara_core::vocab::Color32::from(layer_color),
                    ),
                    wire_threshold,
                    pick_wire_style(from_r.wire_style, to_r.wire_style),
                );
            }
        }

        // Crisp wire on top.
        draw_wire(
            &mut wire_memory,
            wire_clip,
            WireId::Connected {
                graph_id: graph_id.into(),
                out_pin: wire.out_pin,
                in_pin: wire.in_pin,
            },
            &mut wire_shapes,
            wire_frame_size,
            style.get_upscale_wire_frame(),
            style.get_downscale_wire_frame(),
            from_r.pos.into(),
            to_r.pos.into(),
            mara_core::vocab::Stroke::new(draw_width, mara_core::vocab::Color32::from(color)),
            wire_threshold,
            pick_wire_style(from_r.wire_style, to_r.wire_style),
        );
    }

    // Remove hovered wire by second click
    if hovered_wire_disconnect && let Some(wire) = hovered_wire {
        let out_pin = OutPin::new(graph, wire.out_pin);
        let in_pin = InPin::new(graph, wire.in_pin);
        viewer.disconnect(&out_pin, &in_pin, graph);
    }

    if let Some(select_rect) = rect_selection_ended {
        let select_nodes = node_rects.into_iter().filter_map(|(id, rect)| {
            let select = if style.get_select_rect_contained() {
                select_rect.contains_rect(rect.into())
            } else {
                select_rect.intersects(rect.into())
            };

            if select { Some(id) } else { None }
        });

        if modifiers.command {
            graph_state.deselect_many_nodes(select_nodes);
        } else {
            graph_state.select_many_nodes(!modifiers.shift, select_nodes);
        }
    }

    if let Some(select_rect) = graph_state.rect_selection() {
        ui.painter().rect(
            select_rect,
            0.0,
            style.get_select_fill(ui.style()),
            style.get_select_stroke(ui.style()),
            StrokeKind::Inside,
        );
    }

    // If right button is clicked while new wire is being dragged, cancel it.
    // This is to provide way to 'not open' the link graph node menu, but just
    // releasing the new wire to empty space.
    //
    // This uses `button_down` directly, instead of `clicked_by` to improve
    // responsiveness of the cancel action.
    if graph_state.has_new_wires() && ui.input(|x| x.pointer.button_down(PointerButton::Secondary))
    {
        let _ = graph_state.take_new_wires();
        graph_resp.flags.remove(Flags::CLICKED);
    }

    // Do centering unless no nodes are present.
    if style.get_centering() && graph_resp.double_clicked() && nodes_bb.is_finite() {
        let nodes_bb = nodes_bb.expand(100.0);
        graph_state.look_at(nodes_bb.into(), ui_rect, min_scale, max_scale);
    }

    if modifiers.command && graph_resp.clicked_by(PointerButton::Primary) {
        graph_state.deselect_all_nodes();
    }

    // Wire end position will be overridden when link graph menu is opened.
    let mut wire_end_pos = latest_pos.unwrap_or_else(|| graph_resp.rect.center());

    if drag_released {
        let new_wires = graph_state.take_new_wires();
        if new_wires.is_some() {
            ui.ctx().request_repaint();
        }
        match (new_wires, pin_hovered) {
            (Some(NewWires::In(in_pins)), Some(AnyPin::Out(out_pin))) => {
                for in_pin in in_pins {
                    viewer.connect(
                        &OutPin::new(graph, out_pin),
                        &InPin::new(graph, in_pin),
                        graph,
                    );
                }
            }
            (Some(NewWires::Out(out_pins)), Some(AnyPin::In(in_pin))) => {
                for out_pin in out_pins {
                    viewer.connect(
                        &OutPin::new(graph, out_pin),
                        &InPin::new(graph, in_pin),
                        graph,
                    );
                }
            }
            (Some(new_wires), None) if graph_resp.hovered() => {
                let pins = match &new_wires {
                    NewWires::In(x) => AnyPins::In(x),
                    NewWires::Out(x) => AnyPins::Out(x),
                };

                if viewer.has_dropped_wire_menu(pins, graph) {
                    // A wire is dropped without connecting to a pin.
                    // Show context menu for the wire drop.
                    graph_state.set_new_wires_menu(new_wires);

                    // Force open context menu.
                    graph_resp.flags.insert(Flags::LONG_TOUCHED);
                }
            }
            _ => {}
        }
    }

    if let Some(interact_pos) = ui.ctx().input(|i| i.pointer.interact_pos()) {
        if let Some(new_wires) = graph_state.take_new_wires_menu() {
            let pins = match &new_wires {
                NewWires::In(x) => AnyPins::In(x),
                NewWires::Out(x) => AnyPins::Out(x),
            };

            if viewer.has_dropped_wire_menu(pins, graph) {
                graph_resp.context_menu(|ui| {
                    let pins = match &new_wires {
                        NewWires::In(x) => AnyPins::In(x),
                        NewWires::Out(x) => AnyPins::Out(x),
                    };

                    let menu_pos = egui::Pos2::from(from_global.mul_pos(ui.cursor().min.into()));

                    // Override wire end position when the wire-drop context menu is opened.
                    wire_end_pos = menu_pos;

                    // The context menu is opened as *link* graph menu.
                    with_mara_ui(ui, |mui| {
                        viewer.show_dropped_wire_menu(menu_pos.into(), mui, pins, graph)
                    });

                    // Even though menu could be closed in `show_dropped_wire_menu`,
                    // we need to revert the new wires here, because menu state is inaccessible.
                    // Next frame context menu won't be shown and wires will be removed.
                    graph_state.set_new_wires_menu(new_wires);
                });
            }
        } else if viewer.has_graph_menu(interact_pos.into(), graph) {
            graph_resp.context_menu(|ui| {
                let menu_pos = egui::Pos2::from(from_global.mul_pos(ui.cursor().min.into()));

                with_mara_ui(ui, |mui| {
                    viewer.show_graph_menu(menu_pos.into(), mui, graph)
                });
            });
        }
    }

    match graph_state.new_wires() {
        None => {}
        Some(NewWires::In(in_pins)) => {
            for &in_pin in in_pins {
                let from_pos = wire_end_pos;
                let to_r = &input_info[&in_pin];

                draw_wire(
                    &mut wire_memory,
                    wire_clip,
                    WireId::NewInput {
                        graph_id: graph_id.into(),
                        in_pin,
                    },
                    &mut wire_shapes,
                    wire_frame_size,
                    style.get_upscale_wire_frame(),
                    style.get_downscale_wire_frame(),
                    from_pos.into(),
                    to_r.pos.into(),
                    mara_core::vocab::Stroke::new(
                        wire_width,
                        mara_core::vocab::Color32::from(to_r.wire_color),
                    ),
                    wire_threshold,
                    to_r.wire_style,
                );
            }
        }
        Some(NewWires::Out(out_pins)) => {
            for &out_pin in out_pins {
                let from_r = &output_info[&out_pin];
                let to_pos = wire_end_pos;

                draw_wire(
                    &mut wire_memory,
                    wire_clip,
                    WireId::NewOutput {
                        graph_id: graph_id.into(),
                        out_pin,
                    },
                    &mut wire_shapes,
                    wire_frame_size,
                    style.get_upscale_wire_frame(),
                    style.get_downscale_wire_frame(),
                    from_r.pos.into(),
                    to_pos.into(),
                    mara_core::vocab::Stroke::new(
                        wire_width,
                        mara_core::vocab::Color32::from(from_r.wire_color),
                    ),
                    wire_threshold,
                    from_r.wire_style,
                );
            }
        }
    }

    match wire_shape_idx {
        None => {
            let painter = with_mara_ui(&mut ui, |mara| mara.painter());
            for cmd in wire_shapes {
                painter.paint_cmd(cmd);
            }
        }
        Some(slot) => {
            with_mara_ui(&mut ui, |mara| {
                mara.fill_paint_slot(slot, Some(mara_core::paint::PaintCmd::Group(wire_shapes)));
            });
        }
    }

    ui.advance_cursor_after_rect(egui::Rect::from_min_size(graph_resp.rect.min, egui::Vec2::ZERO));

    if let Some(node) = node_to_top
        && graph.nodes.contains(node.0)
    {
        graph_state.node_to_top(node);
    }

    if let Some((node, delta)) = node_moved
        && graph.nodes.contains(node.0)
    {
        ui.ctx().request_repaint();
        if graph_state.selected_nodes().contains(&node) {
            for node in graph_state.selected_nodes() {
                let node = &mut graph.nodes[node.0];
                node.pos += mara_core::vocab::Vec2::from(delta);
            }
        } else {
            let node = &mut graph.nodes[node.0];
            node.pos += mara_core::vocab::Vec2::from(delta);
        }
    }

    graph_state.store(graph, ui.ctx());

    graph_resp.into()
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn draw_inputs<T, V>(
    graph: &mut Graph<T>,
    viewer: &mut V,
    node: NodeId,
    inputs: &[InPin],
    pin_size: f32,
    style: &GraphStyle,
    node_ui: &mut Ui,
    inputs_rect: Rect,
    payload_clip_rect: Rect,
    input_x: f32,
    min_pin_y_top: f32,
    min_pin_y_bottom: f32,
    input_spacing: Option<f32>,
    graph_state: &mut GraphState,
    modifiers: Modifiers,
    input_positions: &mut HashMap<InPinId, PinResponse>,
    heights: Heights,
) -> DrawPinsResponse
where
    V: NodeViewer<T>,
{
    let mut drag_released = false;
    let mut pin_hovered = None;

    // Input pins on the left.
    let mut inputs_ui = node_ui.new_child(
        UiBuilder::new()
            .max_rect(egui::Rect::from(inputs_rect).round_ui())
            .layout(Layout::top_down(Align::Min))
            .id_salt("inputs"),
    );

    let graph_clip_rect = node_ui.clip_rect();
    inputs_ui.shrink_clip_rect(payload_clip_rect.into());

    let pin_layout = Layout::left_to_right(Align::Min);
    let mut new_heights = SmallVec::with_capacity(inputs.len());

    for in_pin in inputs {
        // Show input pin.
        let cursor = inputs_ui.cursor();
        let (height, height_outer) = heights.get(in_pin.id.input);

        let margin = (height_outer - height) / 2.0;
        let outer_rect = cursor.with_max_y(cursor.top() + height_outer);
        let inner_rect = outer_rect.shrink2(egui::Vec2::from(vec2(0.0, margin)));

        let builder = UiBuilder::new().layout(pin_layout).max_rect(inner_rect);

        inputs_ui.scope_builder(builder, |pin_ui| {
            if let Some(input_spacing) = input_spacing {
                let min = pin_ui.next_widget_position();
                pin_ui.advance_cursor_after_rect(egui::Rect::from_min_size(
                    min,
                    egui::Vec2::from(vec2(input_spacing, pin_size)),
                ));
            }

            let y0 = pin_ui.max_rect().min.y;
            let y1 = pin_ui.max_rect().max.y;

            // Show input content
            let node_pin = {
                let accent = mara_core::style::active_accent();
                let mut raw = mara_core::MaraUi::__internal_backend_from_raw(pin_ui);
                let mut mui = mara_core::MaraUi::__internal_over(&mut raw, accent);
                viewer.show_input(in_pin, &mut mui, graph)
            };
            if !graph.nodes.contains(node.0) {
                // If removed
                return;
            }

            let pin_rect = node_pin.pin_rect(
                input_x,
                min_pin_y_top.max(y0),
                min_pin_y_bottom.max(y1),
                pin_size,
            );

            // Interact with pin shape.
            pin_ui.set_clip_rect(graph_clip_rect);

            let r = pin_ui.interact(
                pin_rect.into(),
                pin_ui.next_auto_id(),
                Sense::click_and_drag(),
            );

            pin_ui.skip_ahead_auto_ids(1);

            if r.clicked_by(PointerButton::Secondary) {
                if graph_state.has_new_wires() {
                    graph_state.remove_new_wire_in(in_pin.id);
                } else {
                    viewer.drop_inputs(in_pin, graph);
                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }
                }
            }
            if r.drag_started_by(PointerButton::Primary) {
                if modifiers.command {
                    graph_state.start_new_wires_out(&in_pin.remotes);
                    if !modifiers.shift {
                        graph.drop_inputs(in_pin.id);
                        if !graph.nodes.contains(node.0) {
                            // If removed
                            return;
                        }
                    }
                } else {
                    graph_state.start_new_wire_in(in_pin.id);
                }
            }

            if r.drag_stopped() {
                drag_released = true;
            }

            let mut visual_pin_rect = r.rect;

            if r.contains_pointer() {
                if graph_state.has_new_wires_in() {
                    if modifiers.shift && !modifiers.command {
                        graph_state.add_new_wire_in(in_pin.id);
                    }
                    if !modifiers.shift && modifiers.command {
                        graph_state.remove_new_wire_in(in_pin.id);
                    }
                }
                pin_hovered = Some(AnyPin::In(in_pin.id));
                visual_pin_rect = visual_pin_rect.scale_from_center(1.2);
            }

            let wire_info = node_pin.draw(
                style,
                pin_ui.style(),
                visual_pin_rect.into(),
                &mara_core::MaraPainter::__internal_from_egui(pin_ui.painter().clone()),
            );

            input_positions.insert(
                in_pin.id,
                PinResponse {
                    pos: r.rect.center().into(),
                    wire_color: wire_info.color.into(),
                    wire_style: wire_info.style,
                },
            );

            new_heights.push(with_mara_ui(pin_ui, |mara| mara.occupied_rect()).height());

            pin_ui.expand_to_include_y(outer_rect.bottom());
        });
    }

    let final_rect = with_mara_ui(&mut inputs_ui, |mara| mara.occupied_rect());
    with_mara_ui(node_ui, |mara| {
        mara.expand_to_include(final_rect.intersect(payload_clip_rect))
    });

    DrawPinsResponse {
        drag_released,
        pin_hovered,
        final_rect: final_rect.into(),
        new_heights,
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_lines)]
fn draw_outputs<T, V>(
    graph: &mut Graph<T>,
    viewer: &mut V,
    node: NodeId,
    outputs: &[OutPin],
    pin_size: f32,
    style: &GraphStyle,
    node_ui: &mut Ui,
    outputs_rect: Rect,
    payload_clip_rect: Rect,
    output_x: f32,
    min_pin_y_top: f32,
    min_pin_y_bottom: f32,
    output_spacing: Option<f32>,
    graph_state: &mut GraphState,
    modifiers: Modifiers,
    output_positions: &mut HashMap<OutPinId, PinResponse>,
    heights: Heights,
) -> DrawPinsResponse
where
    V: NodeViewer<T>,
{
    let mut drag_released = false;
    let mut pin_hovered = None;

    let mut outputs_ui = node_ui.new_child(
        UiBuilder::new()
            .max_rect(egui::Rect::from(outputs_rect).round_ui())
            .layout(Layout::top_down(Align::Max))
            .id_salt("outputs"),
    );

    let graph_clip_rect = node_ui.clip_rect();
    outputs_ui.shrink_clip_rect(payload_clip_rect.into());

    let pin_layout = Layout::right_to_left(Align::Min);
    let mut new_heights = SmallVec::with_capacity(outputs.len());

    // Output pins on the right.
    for out_pin in outputs {
        // Show output pin.
        let cursor = outputs_ui.cursor();
        let (height, height_outer) = heights.get(out_pin.id.output);

        let margin = (height_outer - height) / 2.0;
        let outer_rect = cursor.with_max_y(cursor.top() + height_outer);
        let inner_rect = outer_rect.shrink2(egui::Vec2::from(vec2(0.0, margin)));

        let builder = UiBuilder::new().layout(pin_layout).max_rect(inner_rect);

        outputs_ui.scope_builder(builder, |pin_ui| {
            // Allocate space for pin shape.
            if let Some(output_spacing) = output_spacing {
                let min = pin_ui.next_widget_position();
                pin_ui.advance_cursor_after_rect(egui::Rect::from_min_size(
                    min,
                    egui::Vec2::from(vec2(output_spacing, pin_size)),
                ));
            }

            let y0 = pin_ui.max_rect().min.y;
            let y1 = pin_ui.max_rect().max.y;

            // Show output content
            let node_pin = {
                let accent = mara_core::style::active_accent();
                let mut raw = mara_core::MaraUi::__internal_backend_from_raw(pin_ui);
                let mut mui = mara_core::MaraUi::__internal_over(&mut raw, accent);
                viewer.show_output(out_pin, &mut mui, graph)
            };
            if !graph.nodes.contains(node.0) {
                // If removed
                return;
            }

            let pin_rect = node_pin.pin_rect(
                output_x,
                min_pin_y_top.max(y0),
                min_pin_y_bottom.max(y1),
                pin_size,
            );

            pin_ui.set_clip_rect(graph_clip_rect);

            let r = pin_ui.interact(
                pin_rect.into(),
                pin_ui.next_auto_id(),
                Sense::click_and_drag(),
            );

            pin_ui.skip_ahead_auto_ids(1);

            if r.clicked_by(PointerButton::Secondary) {
                if graph_state.has_new_wires() {
                    graph_state.remove_new_wire_out(out_pin.id);
                } else {
                    viewer.drop_outputs(out_pin, graph);
                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }
                }
            }
            if r.drag_started_by(PointerButton::Primary) {
                if modifiers.command {
                    graph_state.start_new_wires_in(&out_pin.remotes);

                    if !modifiers.shift {
                        graph.drop_outputs(out_pin.id);
                        if !graph.nodes.contains(node.0) {
                            // If removed
                            return;
                        }
                    }
                } else {
                    graph_state.start_new_wire_out(out_pin.id);
                }
            }

            if r.drag_stopped() {
                drag_released = true;
            }

            let mut visual_pin_rect = r.rect;

            if r.contains_pointer() {
                if graph_state.has_new_wires_out() {
                    if modifiers.shift && !modifiers.command {
                        graph_state.add_new_wire_out(out_pin.id);
                    }
                    if !modifiers.shift && modifiers.command {
                        graph_state.remove_new_wire_out(out_pin.id);
                    }
                }
                pin_hovered = Some(AnyPin::Out(out_pin.id));
                visual_pin_rect = visual_pin_rect.scale_from_center(1.2);
            }

            let wire_info = node_pin.draw(
                style,
                pin_ui.style(),
                visual_pin_rect.into(),
                &mara_core::MaraPainter::__internal_from_egui(pin_ui.painter().clone()),
            );

            output_positions.insert(
                out_pin.id,
                PinResponse {
                    pos: r.rect.center().into(),
                    wire_color: wire_info.color.into(),
                    wire_style: wire_info.style,
                },
            );

            new_heights.push(with_mara_ui(pin_ui, |mara| mara.occupied_rect()).height());

            pin_ui.expand_to_include_y(outer_rect.bottom());
        });
    }
    let final_rect = with_mara_ui(&mut outputs_ui, |mara| mara.occupied_rect());
    with_mara_ui(node_ui, |mara| {
        mara.expand_to_include(final_rect.intersect(payload_clip_rect))
    });

    DrawPinsResponse {
        drag_released,
        pin_hovered,
        final_rect: final_rect.into(),
        new_heights,
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_body<T, V>(
    graph: &mut Graph<T>,
    viewer: &mut V,
    node: NodeId,
    inputs: &[InPin],
    outputs: &[OutPin],
    ui: &mut Ui,
    body_rect: Rect,
    payload_clip_rect: Rect,
    _graph_state: &GraphState,
) -> DrawBodyResponse
where
    V: NodeViewer<T>,
{
    let mut body_ui = ui.new_child(
        UiBuilder::new()
            .max_rect(egui::Rect::from(body_rect).round_ui())
            .layout(Layout::left_to_right(Align::Min))
            .id_salt("body"),
    );

    body_ui.shrink_clip_rect(payload_clip_rect.into());

    with_mara_ui(&mut body_ui, |mui| {
        viewer.show_body(node, inputs, outputs, mui, graph)
    });

    let final_rect = with_mara_ui(&mut body_ui, |mara| mara.occupied_rect());
    with_mara_ui(ui, |mara| {
        mara.expand_to_include(final_rect.intersect(payload_clip_rect))
    });
    // node_state.set_body_width(body_size.x);

    DrawBodyResponse { final_rect: final_rect.into() }
}

//First step for split big function to parts
/// Draw one node. Return Pins info
#[inline]
#[allow(clippy::too_many_lines)]
#[allow(clippy::too_many_arguments)]
fn draw_node<T, V>(
    graph: &mut Graph<T>,
    ui: &mut Ui,
    node: NodeId,
    viewer: &mut V,
    graph_state: &mut GraphState,
    style: &GraphStyle,
    graph_id: Id,
    input_positions: &mut HashMap<InPinId, PinResponse>,
    modifiers: Modifiers,
    output_positions: &mut HashMap<OutPinId, PinResponse>,
) -> Option<DrawNodeResponse>
where
    V: NodeViewer<T>,
{
    let Node {
        pos,
        open,
        ref value,
    } = graph.nodes[node.0];

    // Collect pins
    let inputs_count = viewer.inputs(value);
    let outputs_count = viewer.outputs(value);

    let inputs = (0..inputs_count)
        .map(|idx| InPin::new(graph, InPinId { node, input: idx }))
        .collect::<Vec<_>>();

    let outputs = (0..outputs_count)
        .map(|idx| OutPin::new(graph, OutPinId { node, output: idx }))
        .collect::<Vec<_>>();

    let node_pos = egui::Pos2::from(pos).round_ui();

    // Generate persistent id for the node.
    let node_id = graph_id.with(("graph-node", node));

    let openness = ui.ctx().animate_bool(node_id, open);

    let mut node_state = NodeState::load(ui.ctx(), node_id, ui.spacing());

    let node_rect = node_state.node_rect(node_pos, openness);

    let mut node_to_top = None;
    let mut node_moved = None;
    let mut drag_released = false;
    let mut pin_hovered = None;

    let node_frame = viewer.node_frame(
        style.get_node_frame(mara_core::style::active_accent()),
        node,
        &inputs,
        &outputs,
        graph,
    );

    let header_frame = viewer.header_frame(
        style.get_header_frame(mara_core::style::active_accent()),
        node,
        &inputs,
        &outputs,
        graph,
    );

    // Rect for node + frame margin.
    let node_frame_rect = egui::Rect::from(
        mara_core::vocab::Rect::from(node_rect).expand_by(node_frame.total_margin()),
    );

    if graph_state.selected_nodes().contains(&node) {
        let select_style = style.get_select_style(ui.style());

        let select_rect = node_frame_rect + select_style.margin;

        ui.painter().rect(
            select_rect,
            select_style.rounding,
            select_style.fill,
            select_style.stroke,
            StrokeKind::Inside,
        );
    }

    // Size of the pin.
    // Side of the square or diameter of the circle.
    let pin_size = style.get_pin_size(ui.style()).max(0.0);

    let pin_placement = style.get_pin_placement();

    let header_drag_space = style.get_header_drag_space(ui.style()).max(Vec2::ZERO);

    // Interact with node frame.
    let r = ui.interact(
        node_frame_rect,
        node_id.with("frame"),
        Sense::click_and_drag(),
    );

    if !modifiers.shift && !modifiers.command && r.dragged_by(PointerButton::Primary) {
        node_moved = Some((node, r.drag_delta().into()));
    }

    if r.clicked_by(PointerButton::Primary) || r.dragged_by(PointerButton::Primary) {
        if modifiers.shift {
            graph_state.select_one_node(modifiers.command, node);
        } else if modifiers.command {
            graph_state.deselect_one_node(node);
        }
    }

    if r.clicked() || r.dragged() {
        node_to_top = Some(node);
    }

    if viewer.has_node_menu(&graph.nodes[node.0].value) {
        r.context_menu(|ui| {
            with_mara_ui(ui, |mui| {
                viewer.show_node_menu(node, &inputs, &outputs, mui, graph)
            });
        });
    }

    if !graph.nodes.contains(node.0) {
        node_state.clear(ui.ctx());
        // If removed
        return None;
    }

    if viewer.has_on_hover_popup(&graph.nodes[node.0].value) {
        r.on_hover_ui_at_pointer(|ui| {
            with_mara_ui(ui, |mui| {
                viewer.show_on_hover_popup(node, &inputs, &outputs, mui, graph)
            });
        });
    }

    if !graph.nodes.contains(node.0) {
        node_state.clear(ui.ctx());
        // If removed
        return None;
    }

    let node_ui = &mut ui.new_child(
        UiBuilder::new()
            .max_rect(node_frame_rect.round_ui())
            .layout(Layout::top_down(Align::Center))
            .id_salt(node_id),
    );

    let mut new_pins_size = Vec2::ZERO;

    // Reserve a slot in the painter for the node halo BEFORE the
    // frame + pins are submitted, so pins render on top of the
    // halo line where they intersect (with `PinPlacement::Edge`
    // pins straddle the body outline). We fill the reserved slot
    // after `node_frame.show` returns and we know the final
    // body rect.
    let halo_slot = style
        .node_halo
        .map(|_| with_mara_ui(node_ui, |mara| mara.reserve_paint_slot()));

    let r = mara_core::backend::egui::egui_frame_for_style_spec(node_frame).show(node_ui, |ui| {
        if viewer.has_node_style(node, &inputs, &outputs, graph) {
            viewer.apply_node_style(ui.style_mut(), node, &inputs, &outputs, graph);
        }

        // Input pins' center side by X axis.
        // `pin_inset` adds an extra inward push for `Inside`
        // placement so the pins sit inside the body's content
        // column rather than flush with the inner margin.
        let pin_inset = style.get_pin_inset();
        let input_x = match pin_placement {
            PinPlacement::Inside => pin_size.mul_add(
                0.5,
                node_frame_rect.left() + node_frame.inner_margin.leftf() + pin_inset,
            ),
            PinPlacement::Edge => node_frame_rect.left(),
            PinPlacement::Outside { margin } => {
                pin_size.mul_add(-0.5, node_frame_rect.left() - margin)
            }
        };

        // Input pins' spacing required.
        let input_spacing = match pin_placement {
            PinPlacement::Inside => Some(pin_size),
            PinPlacement::Edge => Some(
                pin_size
                    .mul_add(0.5, -node_frame.inner_margin.leftf())
                    .max(0.0),
            ),
            PinPlacement::Outside { .. } => None,
        };

        // Output pins' center side by X axis.
        let output_x = match pin_placement {
            PinPlacement::Inside => pin_size.mul_add(
                -0.5,
                node_frame_rect.right() - node_frame.inner_margin.rightf() - pin_inset,
            ),
            PinPlacement::Edge => node_frame_rect.right(),
            PinPlacement::Outside { margin } => {
                pin_size.mul_add(0.5, node_frame_rect.right() + margin)
            }
        };

        // Output pins' spacing required.
        let output_spacing = match pin_placement {
            PinPlacement::Inside => Some(pin_size),
            PinPlacement::Edge => Some(
                pin_size
                    .mul_add(0.5, -node_frame.inner_margin.rightf())
                    .max(0.0),
            ),
            PinPlacement::Outside { .. } => None,
        };

        // Input/output pin block

        if (openness < 1.0 && open) || (openness > 0.0 && !open) {
            ui.ctx().request_repaint();
        }

        // Pins are placed under the header and must not go outside of the header frame.
        let payload_rect = Rect::from_min_max(
            pos2(
                node_rect.min.x,
                node_rect.min.y
                    + node_state.header_height()
                    + header_frame.total_margin().bottomf()
                    + ui.spacing().item_spacing.y
                    - node_state.payload_offset(openness),
            )
            .into(),
            node_rect.max.into(),
        );

        let node_layout =
            viewer.node_layout(style.get_node_layout(), node, &inputs, &outputs, graph);

        let payload_clip_rect =
            Rect::from_min_max(node_rect.min.into(), pos2(node_rect.max.x, f32::INFINITY).into());

        let pins_rect = match node_layout.kind {
            NodeLayoutKind::Coil => {
                // Show input pins.
                let r = draw_inputs(
                    graph,
                    viewer,
                    node,
                    &inputs,
                    pin_size,
                    style,
                    ui,
                    payload_rect,
                    payload_clip_rect,
                    input_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    input_spacing,
                    graph_state,
                    modifiers,
                    input_positions,
                    node_layout.input_heights(&node_state),
                );

                let new_input_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let inputs_rect = r.final_rect;
                let inputs_size = inputs_rect.size();

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                // Show output pins.

                let r = draw_outputs(
                    graph,
                    viewer,
                    node,
                    &outputs,
                    pin_size,
                    style,
                    ui,
                    payload_rect,
                    payload_clip_rect,
                    output_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    output_spacing,
                    graph_state,
                    modifiers,
                    output_positions,
                    node_layout.output_heights(&node_state),
                );

                let new_output_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let outputs_rect = r.final_rect;
                let outputs_size = outputs_rect.size();

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                node_state.set_input_heights(new_input_heights);
                node_state.set_output_heights(new_output_heights);

                new_pins_size = vec2(
                    inputs_size.x + outputs_size.x + ui.spacing().item_spacing.x,
                    f32::max(inputs_size.y, outputs_size.y),
                );

                let mut pins_rect = inputs_rect.union(outputs_rect);

                // Show body if there's one.
                if viewer.has_body(&graph.nodes.get(node.0).unwrap().value) {
                    let body_rect = Rect::from_min_max(
                        pos2(
                            inputs_rect.right() + ui.spacing().item_spacing.x,
                            payload_rect.top(),
                        )
                        .into(),
                        pos2(
                            outputs_rect.left() - ui.spacing().item_spacing.x,
                            payload_rect.bottom(),
                        )
                        .into(),
                    );

                    let r = draw_body(
                        graph,
                        viewer,
                        node,
                        &inputs,
                        &outputs,
                        ui,
                        body_rect,
                        payload_clip_rect,
                        graph_state,
                    );

                    new_pins_size.x += r.final_rect.width() + ui.spacing().item_spacing.x;
                    new_pins_size.y = f32::max(new_pins_size.y, r.final_rect.height());

                    pins_rect = pins_rect.union(body_rect);

                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }
                }

                pins_rect
            }
            NodeLayoutKind::Sandwich => {
                // Show input pins.

                let r = draw_inputs(
                    graph,
                    viewer,
                    node,
                    &inputs,
                    pin_size,
                    style,
                    ui,
                    payload_rect,
                    payload_clip_rect,
                    input_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    input_spacing,
                    graph_state,
                    modifiers,
                    input_positions,
                    node_layout.input_heights(&node_state),
                );

                let new_input_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let inputs_rect = r.final_rect;

                new_pins_size = inputs_rect.size().into();

                let mut next_y = inputs_rect.bottom() + ui.spacing().item_spacing.y;

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                let mut pins_rect = inputs_rect;

                // Show body if there's one.
                if viewer.has_body(&graph.nodes.get(node.0).unwrap().value) {
                    let body_rect = payload_rect.intersect(Rect::everything_below(next_y));

                    let r = draw_body(
                        graph,
                        viewer,
                        node,
                        &inputs,
                        &outputs,
                        ui,
                        body_rect,
                        payload_clip_rect,
                        graph_state,
                    );

                    let body_rect = r.final_rect;

                    new_pins_size.x = f32::max(new_pins_size.x, body_rect.width());
                    new_pins_size.y += body_rect.height() + ui.spacing().item_spacing.y;

                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }

                    pins_rect = pins_rect.union(body_rect);
                    next_y = body_rect.bottom() + ui.spacing().item_spacing.y;
                }

                // Show output pins.

                let outputs_rect = payload_rect.intersect(Rect::everything_below(next_y));

                let r = draw_outputs(
                    graph,
                    viewer,
                    node,
                    &outputs,
                    pin_size,
                    style,
                    ui,
                    outputs_rect,
                    payload_clip_rect,
                    output_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    output_spacing,
                    graph_state,
                    modifiers,
                    output_positions,
                    node_layout.output_heights(&node_state),
                );

                let new_output_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let outputs_rect = r.final_rect;

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                node_state.set_input_heights(new_input_heights);
                node_state.set_output_heights(new_output_heights);

                new_pins_size.x = f32::max(new_pins_size.x, outputs_rect.width());
                new_pins_size.y += outputs_rect.height() + ui.spacing().item_spacing.y;

                pins_rect = pins_rect.union(outputs_rect);

                pins_rect
            }
            NodeLayoutKind::FlippedSandwich => {
                // Show input pins.

                let outputs_rect = payload_rect;
                let r = draw_outputs(
                    graph,
                    viewer,
                    node,
                    &outputs,
                    pin_size,
                    style,
                    ui,
                    outputs_rect,
                    payload_clip_rect,
                    output_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    output_spacing,
                    graph_state,
                    modifiers,
                    output_positions,
                    node_layout.output_heights(&node_state),
                );

                let new_output_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let outputs_rect = r.final_rect;

                new_pins_size = outputs_rect.size().into();

                let mut next_y = outputs_rect.bottom() + ui.spacing().item_spacing.y;

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                let mut pins_rect = outputs_rect;

                // Show body if there's one.
                if viewer.has_body(&graph.nodes.get(node.0).unwrap().value) {
                    let body_rect = payload_rect.intersect(Rect::everything_below(next_y));

                    let r = draw_body(
                        graph,
                        viewer,
                        node,
                        &inputs,
                        &outputs,
                        ui,
                        body_rect,
                        payload_clip_rect,
                        graph_state,
                    );

                    let body_rect = r.final_rect;

                    new_pins_size.x = f32::max(new_pins_size.x, body_rect.width());
                    new_pins_size.y += body_rect.height() + ui.spacing().item_spacing.y;

                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }

                    pins_rect = pins_rect.union(body_rect);
                    next_y = body_rect.bottom() + ui.spacing().item_spacing.y;
                }

                // Show output pins.

                let inputs_rect = payload_rect.intersect(Rect::everything_below(next_y));

                let r = draw_inputs(
                    graph,
                    viewer,
                    node,
                    &inputs,
                    pin_size,
                    style,
                    ui,
                    inputs_rect,
                    payload_clip_rect,
                    input_x,
                    node_rect.min.y,
                    node_rect.min.y + node_state.header_height(),
                    input_spacing,
                    graph_state,
                    modifiers,
                    input_positions,
                    node_layout.input_heights(&node_state),
                );

                let new_input_heights = r.new_heights;

                drag_released |= r.drag_released;

                if r.pin_hovered.is_some() {
                    pin_hovered = r.pin_hovered;
                }

                let inputs_rect = r.final_rect;

                if !graph.nodes.contains(node.0) {
                    // If removed
                    return;
                }

                node_state.set_input_heights(new_input_heights);
                node_state.set_output_heights(new_output_heights);

                new_pins_size.x = f32::max(new_pins_size.x, inputs_rect.width());
                new_pins_size.y += inputs_rect.height() + ui.spacing().item_spacing.y;

                pins_rect = pins_rect.union(inputs_rect);

                pins_rect
            }
        };

        if viewer.has_footer(&graph.nodes[node.0].value) {
            let footer_rect = Rect::from_min_max(
                pos2(
                    node_rect.left(),
                    pins_rect.bottom() + ui.spacing().item_spacing.y,
                )
                .into(),
                pos2(node_rect.right(), node_rect.bottom()).into(),
            );

            let mut footer_ui = ui.new_child(
                UiBuilder::new()
                    .max_rect(egui::Rect::from(footer_rect).round_ui())
                    .layout(Layout::left_to_right(Align::Min))
                    .id_salt("footer"),
            );
            footer_ui.shrink_clip_rect(payload_clip_rect.into());

            with_mara_ui(&mut footer_ui, |mui| {
                viewer.show_footer(node, &inputs, &outputs, mui, graph)
            });

            let final_rect = with_mara_ui(&mut footer_ui, |mara| mara.occupied_rect());
            with_mara_ui(ui, |mara| {
                mara.expand_to_include(final_rect.intersect(payload_clip_rect))
            });
            let footer_size = final_rect.size();

            new_pins_size.x = f32::max(new_pins_size.x, footer_size.x);
            new_pins_size.y += footer_size.y + ui.spacing().item_spacing.y;

            if !graph.nodes.contains(node.0) {
                // If removed
                return;
            }
        }

        // Render header frame.
        let mut header_rect = Rect::NAN;

        let mut header_frame_rect = Rect::NAN; //node_rect + egui::Margin::from(header_frame.total_margin());

        // Show node's header
        //
        // We use `Layout::top_down(Align::Min)` — left-aligned —
        // instead of upstream egui-graph's `Align::Center`. The
        // centred variant horizontally centres any child whose
        // width is less than the header's max width, so any
        // viewer that puts a smaller-than-full-width LTR row in
        // `show_header` (icon + title stacked horizontally, etc.)
        // ends up with its content drifting to the centre of the
        // header rather than being flush left. `Align::Min`
        // anchors the LTR row to the left edge so the icon /
        // title pair starts at the body's inner-margin column,
        // which is what every Blender / Unreal / VSCode-style
        // node editor does.
        let header_ui: &mut Ui = &mut ui.new_child(
            UiBuilder::new()
                .max_rect(egui::Rect::from(
                    mara_core::vocab::Rect::from(node_rect.round_ui())
                        .expand_by(header_frame.total_margin()),
                ))
                .layout(Layout::top_down(Align::Min))
                .id_salt("header"),
        );

        mara_core::backend::egui::egui_frame_for_style_spec(header_frame).show(header_ui, |ui: &mut Ui| {
            ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                if style.get_collapsible() {
                    let (_, r) = ui.allocate_exact_size(
                        egui::Vec2::from(vec2(ui.spacing().icon_width, ui.spacing().icon_width)),
                        Sense::click(),
                    );
                    paint_default_icon(ui, openness, &r);

                    if r.clicked_by(PointerButton::Primary) {
                        // Toggle node's openness.
                        graph.open_node(node, !open);
                    }
                }

                ui.allocate_exact_size(egui::Vec2::from(header_drag_space), Sense::hover());

                with_mara_ui(ui, |mui| {
                    viewer.show_header(node, &inputs, &outputs, mui, graph)
                });

                header_rect = with_mara_ui(ui, |mara| mara.occupied_rect());
            });

            header_frame_rect = 
                header_rect.expand_by(header_frame.total_margin());


            ui.advance_cursor_after_rect(egui::Rect::from_min_max(
                header_rect.min.into(),
                pos2(
                    f32::max(header_rect.max.x, node_rect.max.x),
                    header_rect.min.y,
                )
                .into(),
            ));
        });

        with_mara_ui(ui, |mara| mara.expand_to_include(header_rect));
        let header_size = header_rect.size();
        node_state.set_header_height(header_size.y);

        node_state.set_size(egui::Vec2::from(vec2(
            f32::max(header_size.x, new_pins_size.x),
            header_size.y
                + header_frame.total_margin().bottomf()
                + ui.spacing().item_spacing.y
                + new_pins_size.y,
        )));
    });

    // Fill the reserved halo slot now that we know the final
    // body rect — `r.response.rect` is the rect that was used to
    // render the node frame.
    if let (Some(slot), Some(halo)) = (halo_slot, style.node_halo) {
        let halo_rect = r.response.rect.expand(halo.gap);
        with_mara_ui(node_ui, |mara| {
            mara.fill_paint_slot(
                slot,
                Some(mara_core::paint::PaintCmd::RectStroke {
                    rect: halo_rect.into(),
                    corner: mara_core::vocab::CornerRadius::same(halo.radius),
                    stroke: mara_core::vocab::Stroke::new(halo.width, mara_core::vocab::Color32::from(halo.color)),
                }),
            );
        });
    }

    if !graph.nodes.contains(node.0) {
        ui.ctx().request_repaint();
        node_state.clear(ui.ctx());
        // If removed
        return None;
    }

    let final_rect = r.response.rect;
    with_mara_ui(ui, |mui| {
        viewer.final_node_rect(node, final_rect.into(), mui, graph)
    });

    node_state.store(ui.ctx());
    Some(DrawNodeResponse {
        node_moved,
        node_to_top,
        drag_released,
        pin_hovered,
        final_rect: r.response.rect.into(),
    })
}

const fn mix_colors(a: Color32, b: Color32) -> Color32 {
    #![allow(clippy::cast_possible_truncation)]

    Color32::from_rgba_premultiplied(
        u8::midpoint(a.r(), b.r()),
        u8::midpoint(a.g(), b.g()),
        u8::midpoint(a.b(), b.b()),
        u8::midpoint(a.a(), b.a()),
    )
}

/// Multiply `c`'s alpha by `f` (clamped to `[0, 1]`) for layered
/// glow passes. Returns an UN-premultiplied colour so the egui
/// renderer's standard alpha blend produces the expected halo.
#[inline]
fn with_alpha_factor(c: Color32, f: f32) -> Color32 {
    let a = (c.a() as f32 * f.clamp(0.0, 1.0)).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

// fn mix_colors(mut colors: impl Iterator<Item = Color32>) -> Option<Color32> {
//     let color = colors.next()?;

//     let mut r = color.r() as u32;
//     let mut g = color.g() as u32;
//     let mut b = color.b() as u32;
//     let mut a = color.a() as u32;
//     let mut w = 1;

//     for c in colors {
//         r += c.r() as u32;
//         g += c.g() as u32;
//         b += c.b() as u32;
//         a += c.a() as u32;
//         w += 1;
//     }

//     Some(Color32::from_rgba_premultiplied(
//         (r / w) as u8,
//         (g / w) as u8,
//         (b / w) as u8,
//         (a / w) as u8,
//     ))
// }

// fn mix_sizes(mut sizes: impl Iterator<Item = f32>) -> Option<f32> {
//     let mut size = sizes.next()?;
//     let mut w = 1;

//     for s in sizes {
//         size += s;
//         w += 1;
//     }

//     Some(size / w as f32)
// }

// fn mix_strokes(mut strokes: impl Iterator<Item = Stroke>) -> Option<Stroke> {
//     let stoke = strokes.next()?;

//     let mut width = stoke.width;
//     let mut r = stoke.color.r() as u32;
//     let mut g = stoke.color.g() as u32;
//     let mut b = stoke.color.b() as u32;
//     let mut a = stoke.color.a() as u32;

//     let mut w = 1;

//     for s in strokes {
//         width += s.width;
//         r += s.color.r() as u32;
//         g += s.color.g() as u32;
//         b += s.color.b() as u32;
//         a += s.color.a() as u32;
//         w += 1;
//     }

//     Some(Stroke {
//         width: width / w as f32,
//         color: Color32::from_rgba_premultiplied(
//             (r / w) as u8,
//             (g / w) as u8,
//             (b / w) as u8,
//             (a / w) as u8,
//         ),
//     })
// }

impl<T> Graph<T> {
    /// Render [`Graph`] using given viewer and style into the [`Ui`].
    #[inline]
    pub fn show<V>(&mut self, viewer: &mut V, style: &GraphStyle, id_salt: impl Hash, ui: &mut Ui)
    where
        V: NodeViewer<T>,
    {
        show_graph(
            ui.make_persistent_id(id_salt),
            *style,
            Vec2::ZERO,
            Vec2::INFINITY,
            self,
            viewer,
            ui,
        );
    }
}

/// Clamp the view scale, rescaling about the viewport centre so the
/// content under the middle of the screen stays put.
///
/// The maths lives in [`mara_core::transform::Transform`] now (WS-E1.4) rather than
/// in three local helpers over the backend's transform type; this only
/// converts at the boundary, and that conversion disappears when the
/// rest of this file ports.
#[inline]
fn clamp_scale(
    to_global: &mut mara_core::transform::Transform,
    min_scale: f32,
    max_scale: f32,
    ui_rect: Rect,
) {
    if to_global.scaling >= min_scale && to_global.scaling <= max_scale {
        return;
    }

    let new_scaling = to_global.scaling.clamp(min_scale, max_scale);
    *to_global = to_global.scaled_around(new_scaling, ui_rect.center().into());
}

#[test]
const fn graph_style_is_send_sync() {
    const fn is_send_sync<T: Send + Sync>() {}
    is_send_sync::<GraphStyle>();
}

/// Run `body` with the sealed surface over a backend `Ui`.
///
/// `NodeViewer` speaks `MaraUi` since WS-D1.4, while this file's render
/// path is still backend-typed. Wrapping here keeps the two changes
/// separable; the helper disappears when the render path ports.
fn with_mara_ui<R>(ui: &mut Ui, body: impl for<'a> FnOnce(&mut mara_core::MaraUi<'a>) -> R) -> R {
    let accent = mara_core::style::active_accent();
    let mut raw = mara_core::MaraUi::__internal_backend_from_raw(ui);
    let mut mara = mara_core::MaraUi::__internal_over(&mut raw, accent);
    body(&mut mara)
}
