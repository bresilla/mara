//! This module provides functionality for showing [`Graph`] graph in [`Ui`].

use std::{collections::HashMap, hash::Hash};

use egui::{
    Align, CornerRadius, Id, LayerId, Layout, Margin, Modifiers, PointerButton, Scene, Sense,
    StrokeKind, Ui, UiBuilder, UiKind, UiStackInfo,
    collapsing_header::paint_default_icon,
    emath::{GuiRounding, TSTransform},
    response::Flags,
};
use mara_core::MaraResponse;
use mara_core::style::{FrameRole, FrameSpec, frame_for};
use mara_core::vocab::{Color32, Pos2, Rect, Stroke, Vec2, pos2, vec2};
use smallvec::SmallVec;

use crate::vendored::{
    Graph, InPin, InPinId, Node, NodeId, NodeUid, OutPin, OutPinId, subgraph::DefId,
    ui::wire::WireId,
};

mod background_pattern;
mod frame_paint;
pub mod lod;
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
    // `PinWireInfo` was never re-exported, which made `NodePin`
    // externally unimplementable: the trait's `draw` takes one and no
    // downstream crate could name the type. PLAN_NODE.md P2.
    pin::{AnyPins, NodePin, PinInfo, PinShape, PinWireInfo},
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

/// Drop shadow painted under each node body, plus the deeper variant
/// used while the node is being dragged.
///
/// The two states are one struct rather than two style fields because
/// they are never meaningful apart: a shadow that does not lift on
/// drag reads as a flat sticker, and a lift with no resting shadow has
/// nothing to lift from. `PLAN_NODE.md` P1.
///
/// Both variants go into the node's underlay slot, which is reserved
/// before the frame and pins are submitted, so the shadow lands under
/// the body rather than over it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ShadowSpec {
    /// Resting offset in points, `[x, y]`. Positive `y` is down.
    pub offset: [i8; 2],
    /// Resting blur radius in points.
    pub blur: u8,
    /// Resting spread in points — grows the shadow rect before blurring.
    pub spread: u8,
    /// Shadow colour, alpha included. Typically near-black at low alpha.
    pub color: Color32,
    /// Offset used while the node is dragged by the primary button.
    pub drag_offset: [i8; 2],
    /// Blur used while the node is dragged.
    pub drag_blur: u8,
}

impl Default for ShadowSpec {
    fn default() -> Self {
        Self {
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(90),
            drag_offset: [0, 10],
            drag_blur: 24,
        }
    }
}

impl ShadowSpec {
    /// The `(offset, blur)` pair for the node's current drag state.
    #[must_use]
    const fn for_state(&self, dragged: bool) -> ([i8; 2], u8) {
        if dragged {
            (self.drag_offset, self.drag_blur)
        } else {
            (self.offset, self.blur)
        }
    }
}

/// Layered selection halo — a core stroke plus three progressively
/// wider, fainter outside strokes.
///
/// A single 1 px border is illegible in a multi-select over a dense
/// graph: at any reasonable zoom it is indistinguishable from the
/// node's own outline. Stacked strokes read as a glow without needing
/// one, and survive being scaled down.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HaloSpec {
    /// Width of the crisp inner stroke, in points.
    pub core_width: f32,
    /// Widths of the three outside strokes, ascending.
    pub widths: [f32; 3],
    /// Alphas of the three outside strokes, descending.
    pub alphas: [f32; 3],
    /// Corner radius of the halo rect.
    pub radius: u8,
    /// Distance from the node edge to the core stroke.
    pub margin: f32,
}

impl Default for HaloSpec {
    fn default() -> Self {
        Self {
            core_width: 2.0,
            widths: [4.0, 7.0, 11.0],
            alphas: [0.18, 0.09, 0.04],
            radius: 10,
            margin: 2.0,
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

    /// Optional drop shadow painted under each node body, deepening
    /// while the node is dragged. Shares the node's underlay slot with
    /// [`GraphStyle::node_halo`], so it is always beneath the body and
    /// the pins. Default `None` (no shadow).
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub node_shadow: Option<ShadowSpec>,

    /// Layered halo painted around selected nodes. `None` keeps the
    /// flat `select_style` rect.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub select_halo: Option<HaloSpec>,

    /// Thickness in points of the accent bar across a node's top edge.
    /// Colour comes from `NodeViewer::node_chrome`. `None` or `0` off.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub header_accent: Option<f32>,

    /// Zoom level-of-detail thresholds.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub lod: Option<lod::LodLadder>,

    /// Camera easing rate in e-folds per second. `None` snaps.
    #[cfg_attr(
        feature = "serde",
        serde(skip_serializing_if = "Option::is_none", default)
    )]
    pub camera_spring: Option<f32>,

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

    /// Pin diameter, in points.
    ///
    /// Reads `mara_core::style::interact_row_h` rather than the
    /// backend's `spacing.interact_size.y` (PLAN.md WS-D1.3). Those are
    /// the same number — the theme installs one from the other — but
    /// going to the source keeps the **touch-density scaling**, which a
    /// constant here would silently drop.
    fn get_pin_size(&self) -> f32 {
        self.pin_size
            .unwrap_or(mara_core::style::interact_row_h() * 0.6)
    }

    /// Fill for a pin the caller did not colour.
    ///
    /// Falls back to Mara's accent rather than reading the backend's
    /// `Style` (PLAN.md WS-D1.3). Not a behaviour change for Mara
    /// consumers: `mara_node_graph_style` sets `pin_fill` explicitly, so
    /// this branch never runs there — and where it does run, egui's
    /// `widgets.active.bg_fill` is the adapted accent anyway, because
    /// that is what Mara's theme installs.
    fn get_pin_fill(&self) -> Color32 {
        self.pin_fill
            .unwrap_or_else(|| mara_core::style::active_accent().into())
    }

    /// Outline for a pin the caller did not stroke. Same reasoning as
    /// [`GraphStyle::get_pin_fill`].
    fn get_pin_stroke(&self) -> Stroke {
        self.pin_stroke.unwrap_or_else(|| {
            let s = mara_core::style::stroke_for(
                mara_core::style::StrokeRole::WidgetBorder,
                mara_core::style::active_accent(),
            );
            Stroke::new(s.width, s.color.into())
        })
    }

    fn get_pin_shape(&self) -> PinShape {
        self.pin_shape.unwrap_or(PinShape::Circle)
    }

    fn get_pin_placement(&self) -> PinPlacement {
        self.pin_placement.unwrap_or_default()
    }

    fn get_wire_width(&self) -> f32 {
        self.wire_width.unwrap_or_else(|| self.get_pin_size() * 0.1)
    }

    fn get_wire_frame_size(&self) -> f32 {
        self.wire_frame_size
            .unwrap_or_else(|| self.get_pin_size() * 3.0)
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

    /// Blank space reserved in a node header for dragging.
    ///
    /// Touch-scaled via `mara_core::style::icon_width`, for the same
    /// reason as [`GraphStyle::get_pin_size`].
    fn get_header_drag_space(&self) -> Vec2 {
        let w = mara_core::style::icon_width();
        self.header_drag_space.unwrap_or_else(|| vec2(w, w))
    }

    fn get_collapsible(&self) -> bool {
        self.collapsible.unwrap_or(true)
    }

    fn get_bg_frame(&self, accent: mara_core::vocab::Color32) -> FrameSpec {
        self.bg_frame
            .unwrap_or_else(|| frame_for(FrameRole::Canvas, accent))
    }

    /// Stroke for the canvas background pattern.
    ///
    /// The backend's `widgets.noninteractive.bg_stroke` is
    /// `(theme().stroke.border_width, widget_border(accent))`; this
    /// reads those two directly (PLAN.md WS-D1.3). Mara's own graph
    /// style sets `bg_pattern_stroke` explicitly, so this branch is only
    /// reached by standalone use of the vendored crate.
    fn get_bg_pattern_stroke(&self) -> Stroke {
        self.bg_pattern_stroke.unwrap_or_else(|| {
            Stroke::new(
                mara_core::style::theme().stroke.border_width,
                mara_core::style::widget_border(mara_core::style::theme_accent()),
            )
        })
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

    /// Outline of the selection marquee.
    ///
    /// Reads `mara_core::style::selection_stroke` rather than the
    /// backend's `visuals.selection` (PLAN.md WS-D1.3); the theme
    /// installs one from the other, so this is the same colour taken at
    /// its source. The half-alpha keeps the original appearance.
    fn get_select_stroke(&self) -> Stroke {
        self.select_stoke.unwrap_or_else(|| {
            let s = mara_core::style::selection_stroke();
            Stroke::new(s.width, s.color.gamma_multiply(0.5))
        })
    }

    /// Fill of the selection marquee. See
    /// [`GraphStyle::get_select_stroke`].
    fn get_select_fill(&self) -> Color32 {
        self.select_fill
            .unwrap_or_else(|| mara_core::style::selection_fill().gamma_multiply(0.3))
    }

    fn get_select_rect_contained(&self) -> bool {
        self.select_rect_contained.unwrap_or(false)
    }

    fn get_select_style(&self) -> SelectionStyle {
        self.select_style.unwrap_or_else(|| SelectionStyle {
            // `window_margin` is `Margin::ZERO` in Mara's theme, and the
            // corner radius is the shape theme's window role.
            margin: Margin::ZERO,
            rounding: mara_core::style::radius_for(mara_core::style::RadiusRole::Popup).into(),
            fill: self.get_select_fill(),
            stroke: self.get_select_stroke(),
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
            node_shadow: None,
            select_halo: None,
            header_accent: None,
            lod: None,
            camera_spring: None,
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
    /// The node the pointer is over, if this one is.
    ///
    /// Collected in the node loop and consumed by the wire loop, which
    /// runs after it — that ordering is what lets hover focus dim wires
    /// in the same frame the hover happens rather than one frame late.
    hovered: Option<NodeId>,
    /// Set on the frame a node's own drag ENDS.
    ///
    /// Distinct from `drag_released`, which reports a *pin* drag
    /// finishing (a wire being dropped). Frame adoption resolves on
    /// this and only this: recomputing membership continuously would
    /// capture any node that merely passes over a group.
    node_drag_stopped: Option<NodeId>,
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

/// Mara's item spacing, in the vendored code's `Vec2`.
///
/// The graph laid out against `ui.spacing().item_spacing`, which is what
/// `mara_core::style::item_spacing` publishes and the theme installs
/// (PLAN.md WS-D1.3). Reading the source keeps the touch-density
/// scaling that a constant here would drop.
fn mara_item_spacing() -> Vec2 {
    mara_core::style::item_spacing().into()
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
    pub fn id(mut self, id: mara_core::vocab::Id) -> Self {
        self.id = Some(Id::from(id));
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

    /// Render [`Graph`] using the given viewer and style.
    ///
    /// Takes a [`MaraUi`] and returns a [`MaraResponse`], so a caller
    /// never names a backend type to host a graph or to ask whether the
    /// canvas was clicked (PLAN.md WS-D1.4). This is what lets a graph
    /// live inside `ViewCtx::offscreen`, whose body is handed a
    /// `MaraUi`.
    ///
    /// The renderer *behind* this signature still works on a raw `Ui` —
    /// 52 call sites, tracked in PLAN.md's mapping table — so it unwraps
    /// once here. `mara_graph` is the one crate exempted from the
    /// sealed-tier ban on that hatch, which is what makes the unwrap
    /// legal rather than a leak.
    ///
    /// Panics on a non-egui backend, as the renderer requires one until
    /// that table is worked through.
    ///
    /// [`MaraUi`]: mara_core::MaraUi
    #[inline]
    pub fn show<T, V>(
        &self,
        graph: &mut Graph<T>,
        viewer: &mut V,
        mara: &mut mara_core::MaraUi<'_>,
    ) -> MaraResponse
    where
        V: NodeViewer<T>,
    {
        let ui = mara.__internal_raw_ui();
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
    let bg_frame_backend = mara_backend_egui::egui_frame_for_style_spec(bg_frame);

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

    // The graph's own state lives in Mara's store now, not egui's
    // context data (PLAN.md WS-D1.4 prerequisite). `EguiCtx` is the
    // bridge while the surrounding surface is still an `egui::Ui`.
    let seam = mara_backend_egui::EguiCtx::new(ui.ctx());
    let mut graph_state = GraphState::load(&seam, graph_id, graph, ui_rect, min_scale, max_scale);
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
        let mut raw = mara_backend_egui::__internal_backend_from_raw(&mut ui);
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
        mara_core::context::MaraCtx::request_repaint(&seam);
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

    // `draw_background` speaks `MaraPainter` since PLAN_NODE.md P2, so
    // the painter is taken through the seal rather than handed over as
    // a backend type. The default body no longer bridges at all.
    let bg_painter = with_mara_ui(&mut ui, |mara| mara.painter());
    viewer.draw_background(
        style.bg_pattern.as_ref(),
        &viewport.into(),
        &style,
        &bg_painter,
        graph,
    );

    let mut node_moved = None;
    let mut node_to_top = None;
    let mut node_drag_stopped: Option<NodeId> = None;
    let mut hovered_node: Option<NodeId> = None;

    // ── Frame groups (PLAN_NODE.md P5) ──
    //
    // Painted here — after the background, before wires and nodes — so
    // a group box sits under everything it contains without needing a
    // reserved slot. Inline rather than slotted because the backend's
    // slot filler maps `Text` to a no-op and the title would silently
    // vanish; see `frame_paint`'s module comment.
    // Clone the context handle so the rect provider does not borrow
    // `ui` — `frame_pass` needs `&mut ui` at the same time, and
    // `egui::Context` is an `Arc` internally so this is a refcount bump.
    let rect_ctx = ui.ctx().clone();
    let node_rect_provider = |node: NodeId| -> mara_core::vocab::Rect {
        node_frame_rect_of(&rect_ctx, graph_id, node, graph, &style)
    };
    // Whether a NODE is being dragged, as opposed to any button being
    // down at all. `pointer.any_down()` is equally true while the
    // canvas is being panned or a rubber-band selection is in flight,
    // and using it made the candidate-group highlight flash during
    // every pan. Matching the dragged widget against the node frame ids
    // costs one hash per node and answers the question actually being
    // asked.
    let dragging_node = ui.ctx().dragged_id().is_some_and(|dragged| {
        graph
            .node_ids()
            .any(|(n, _)| graph_id.with(("graph-node", n)).with("frame") == dragged)
    });
    let frame_outcome = if graph.frames().next().is_some() {
        with_mara_ui(&mut ui, |mara| {
            frame_paint::frame_pass(
                mara,
                graph,
                mara_core::vocab::Id::from(graph_id),
                mara_core::style::active_accent(),
                &node_rect_provider,
                to_global.scaling,
                latest_pos.map(mara_core::vocab::Pos2::from),
                dragging_node,
            )
        })
    } else {
        frame_paint::FramePassOutcome::default()
    };

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

    let wire_frame_size = style.get_wire_frame_size();
    let wire_width = style.get_wire_width();
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

    // The visible region in GRAPH space, plus a margin so a node whose
    // body extends past its cached size does not pop at the edge.
    let viewport_graph: mara_core::vocab::Rect = viewport.into();
    let cull_margin = 400.0;

    let mut nodes_bb = Rect::NOTHING;
    let mut node_rects = Vec::new();

    for node_idx in draw_order {
        if !graph.nodes.contains(node_idx.0) {
            continue;
        }

        // A collapsed frame folds its contents away. Skipped here
        // rather than drawn-and-hidden so a folded group costs nothing
        // to render — which is the point of folding a large one.
        if graph.is_collapsed_away(node_idx) {
            continue;
        }

        // ── Viewport culling (PLAN_NODE.md P11) ──
        //
        // Skipped BEFORE `draw_node`, so an off-screen node costs no
        // viewer calls, no pin construction and no layout — not merely
        // no pixels. egui's tessellator already drops fully-clipped
        // shapes, so culling that only avoided geometry would save
        // almost nothing; the cost this avoids is the work upstream of
        // it. Uses the cached size, which is why it is an estimate: a
        // node whose content grew this frame may be one frame late to
        // appear, and being late by a frame at the screen edge is
        // preferable to laying out five hundred invisible nodes.
        let node_bounds = node_frame_rect_of(&rect_ctx, graph_id, node_idx, graph, &style);
        if node_bounds.is_finite() && !viewport_graph.expand(cull_margin).intersects(node_bounds) {
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
            if let Some(v) = response.node_drag_stopped {
                node_drag_stopped = Some(v);
            }
            if let Some(v) = response.hovered {
                hovered_node = Some(v);
            }

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
    let wire_store = mara_backend_egui::EguiCtx::new(ui.ctx());
    let mut wire_memory =
        mara_core::memory::MaraMemoryCtx::__internal_from_backend_ctx(&wire_store);
    let wire_clip: mara_core::vocab::Rect = ui.clip_rect().into();

    // Draw and interact with wires.
    //
    // Sorted first: the wire set is a `HashSet`, so its iteration order
    // differs run to run and even frame to frame. Painting in that
    // order makes overlapping wires swap z-position every frame — a
    // visible shimmer — and defeats any cache keyed on draw order.
    // Sorting the visible subset once per frame is cheap next to
    // tessellating them.
    let mut ordered_wires: Vec<_> = graph.wires.iter().collect();
    ordered_wires.sort_by_key(|w| (w.out_pin, w.in_pin));

    // ── Hover focus (PLAN_NODE.md P10) ──
    //
    // Hovering a node lights its one-hop neighbourhood and dims
    // everything else. On a three-hundred-gate graph this is the
    // difference between usable and unusable, and it costs one BFS over
    // the wire set plus colour arithmetic at paint time — no extra
    // geometry, no extra passes.
    //
    // `None` means nothing is hovered and everything paints at full
    // strength, which is the common case and must stay free.
    let focus: Option<std::collections::HashSet<NodeId>> = hovered_node.map(|n| {
        let mut set = std::collections::HashSet::new();
        set.insert(n);
        for (o, i) in graph.wires() {
            if o.node == n {
                set.insert(i.node);
            } else if i.node == n {
                set.insert(o.node);
            }
        }
        set
    });
    /// How far an out-of-focus item is pulled toward the background.
    const DIM: f32 = 0.30;

    for wire in ordered_wires {
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

        let mut color = match style.get_wire_color_mode() {
            WireColorMode::Mix => mix_colors(from_r.wire_color, to_r.wire_color),
            WireColorMode::FromSource => from_r.wire_color,
            WireColorMode::FromTarget => to_r.wire_color,
        };

        // A wire is in focus only if BOTH ends are — a wire with one
        // end in the neighbourhood still leads somewhere irrelevant.
        if let Some(f) = &focus
            && !(f.contains(&wire.out_pin.node) && f.contains(&wire.in_pin.node))
        {
            color = color.gamma_multiply(DIM);
        }

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
            style.get_select_fill(),
            style.get_select_stroke(),
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
        // Eased when a spring rate is configured, instant otherwise.
        // Routing every view change through `fly_to` is what makes
        // fit-to-selection, breadcrumb jumps and the subgraph dive cost
        // a target each rather than an animation each.
        if style.camera_spring.is_some() {
            graph_state.fly_to(nodes_bb.into(), ui_rect, min_scale, max_scale);
        } else {
            graph_state.look_at(nodes_bb.into(), ui_rect, min_scale, max_scale);
        }
    }

    // ── Camera spring (PLAN_NODE.md P6) ──
    //
    // Stepped after the frame's interactions so a target set this frame
    // starts moving on the next one, and gated on there being a target
    // at all — an idle canvas must request zero repaints.
    if let (Some(rate), Some(target)) = (style.camera_spring, graph_state.camera_target()) {
        let dt = ui.ctx().input(|i| i.stable_dt).clamp(0.0, 0.25);
        let mut spring = crate::vendored::camera::CameraSpring::at(graph_state.to_global());
        spring.retarget(target);
        if spring.step(dt, rate) {
            graph_state.set_to_global(spring.current());
            ui.ctx().request_repaint();
        } else {
            graph_state.set_to_global(target);
            graph_state.clear_camera_target();
        }
    }

    if modifiers.command && graph_resp.clicked_by(PointerButton::Primary) {
        graph_state.deselect_all_nodes();
    }

    // Wire end position will be overridden when link graph menu is opened.
    let mut wire_end_pos = latest_pos.unwrap_or_else(|| graph_resp.rect.center());

    if drag_released {
        let new_wires = graph_state.take_new_wires();
        if new_wires.is_some() {
            mara_core::context::MaraCtx::request_repaint(&seam);
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

    ui.advance_cursor_after_rect(egui::Rect::from_min_size(
        graph_resp.rect.min,
        egui::Vec2::ZERO,
    ));

    if let Some(node) = node_to_top
        && graph.nodes.contains(node.0)
    {
        graph_state.node_to_top(node);
    }

    if let Some((node, delta)) = node_moved
        && graph.nodes.contains(node.0)
    {
        mara_core::context::MaraCtx::request_repaint(&seam);
        // `drag_targets_node` is the pure model function: dragging a
        // selected node moves the whole selection, anything else moves
        // alone. Keeping the rule in the model rather than here is what
        // makes it testable without a render pass.
        let targets = graph.drag_targets_node(node, graph_state.selected_nodes());
        let d = mara_core::vocab::Vec2::from(delta);
        for target in targets {
            graph.nodes[target.0].pos += d;
        }
        graph.touch();
    }

    // Frame drags land at the SAME deferred site as node drags, for the
    // same reason: the node loop holds `&mut graph` and cannot move
    // anything while iterating.
    if let Some((frame, delta)) = frame_outcome.frame_moved
        && let Some(f) = graph.frame(frame)
    {
        mara_core::context::MaraCtx::request_repaint(&seam);
        let d = mara_core::vocab::Vec2::from(delta);
        match f.move_mode {
            crate::vendored::frames::FrameMove::WithContents => {
                for target in graph.drag_targets_frame(frame) {
                    graph.nodes[target.0].pos += d;
                }
                // A manually-sized frame carries its box along with its
                // contents; an auto-fitting one re-fits and would fight
                // an explicit bounds nudge.
                if let Some(f) = graph.frame_mut(frame)
                    && !f.shrink
                {
                    f.bounds = f.bounds.translate(d);
                }
            }
            crate::vendored::frames::FrameMove::BoxOnly => {
                if let Some(f) = graph.frame_mut(frame)
                    && !f.shrink
                {
                    f.bounds = f.bounds.translate(d);
                }
            }
        }
        graph.touch();
    }

    if let Some(frame) = frame_outcome.toggle_collapsed
        && let Some(f) = graph.frame_mut(frame)
    {
        f.collapsed = !f.collapsed;
        mara_core::context::MaraCtx::request_repaint(&seam);
    }

    if let Some((frame, bounds)) = frame_outcome.frame_resized
        && let Some(f) = graph.frame_mut(frame)
    {
        // Starting a resize turns auto-fit off and keeps the bounds the
        // box had at that moment, so it does not snap back under the
        // cursor mid-drag.
        f.shrink = false;
        f.bounds = bounds;
    }

    // `F` groups the selection. Blender moved every frame operation onto
    // this key in 4.5 precisely because creation-by-menu "got in the
    // way"; a group you can make in one keystroke is a group people
    // actually make.
    // Ctrl/Cmd+G folds the selection into a definition; adding Shift
    // dissolves a selected instance back out. Recorded rather than
    // performed, because both need the document and this function only
    // ever holds one level of it.
    if graph_resp.hovered()
        && modifiers.command
        && ui.input(|i| i.key_pressed(egui::Key::G))
        && !graph_state.selected_nodes().is_empty()
    {
        note_intent(if modifiers.shift {
            StructuralIntent::Expand
        } else {
            StructuralIntent::Collapse
        });
        mara_core::context::MaraCtx::request_repaint(&seam);
    }

    if graph_resp.hovered()
        && ui.input(|i| i.key_pressed(egui::Key::F))
        && !graph_state.selected_nodes().is_empty()
    {
        let members: Vec<NodeId> = graph_state.selected_nodes().to_vec();
        // Nest inside whatever the selection already shares, so grouping
        // a subset of a group produces a child rather than a sibling
        // that overlaps it.
        let common = {
            let first = graph.frame_of(members[0]);
            members
                .iter()
                .all(|n| graph.frame_of(*n) == first)
                .then_some(first)
                .flatten()
        };
        let new_frame = graph.insert_frame(
            "Group",
            mara_core::style::active_accent(),
            mara_core::vocab::Rect::NAN,
        );
        if let Some(parent) = common {
            graph.set_frame_parent(new_frame, Some(parent));
        }
        for node in members {
            graph.set_node_frame(node, Some(new_frame));
        }
        mara_core::context::MaraCtx::request_repaint(&seam);
    }

    // Adoption resolves on drag STOP, never continuously: recomputing
    // membership from geometry every frame is Unreal's model and it
    // captures nodes that merely pass over a box.
    if let Some(node) = node_drag_stopped
        && graph.nodes.contains(node.0)
    {
        let landed = frame_outcome.hovered_frame;
        if landed != graph.frame_of(node) {
            // Dragging fully out releases to the frame's PARENT rather
            // than to the root, so pulling a node out of a nested group
            // leaves it in the surrounding one.
            let target =
                landed.or_else(|| graph.frame_of(node).and_then(|f| graph.frame(f)?.parent));
            graph.set_node_frame(node, target);
        }
    }

    graph_state.store(graph, &seam);

    mara_backend_egui::mara_response_from(&graph_resp)
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
                let mut raw = mara_backend_egui::__internal_backend_from_raw(pin_ui);
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
                visual_pin_rect.into(),
                &mara_backend_egui::__internal_painter_from_egui(pin_ui.painter().clone()),
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
                let mut raw = mara_backend_egui::__internal_backend_from_raw(pin_ui);
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
                visual_pin_rect.into(),
                &mara_backend_egui::__internal_painter_from_egui(pin_ui.painter().clone()),
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

    DrawBodyResponse {
        final_rect: final_rect.into(),
    }
}

//First step for split big function to parts
/// Draw one node. Return Pins info
///
/// # The decoration budget
///
/// Every decorative cue here was added on its own merits and they were
/// never costed together: a selected node that belonged to a group and
/// was a subgraph instance used to paint eleven layers in four unrelated
/// colour systems, and the result read as damage rather than as
/// information. The budget that replaced it:
///
/// - **One resting ring, never two.** [`GraphStyle::node_halo`] is that
///   ring. Group membership is carried by the group box painted behind
///   the node, not by a second ring on the node — a per-node ring in the
///   group's colour competes with the box that already says the same
///   thing, and with the accent that owns the node's edge.
/// - **Selection replaces the resting ring rather than stacking on it.**
///   A selected node drops `node_halo` and shows the selection cue in
///   its place — [`GraphStyle::select_halo`] where one is configured,
///   the flat rect otherwise. Stacking them made selection read as
///   noise; emphasis needs the contrast of a cue appearing where there
///   was none, not a thicker version of a cue already present.
/// - **The deck of cards survives** because it means something no other
///   layer says — "there is a world inside this" — but capped at two
///   cards and only where the body is legible at all.
/// - **The header accent bar means a category, so absent must stay
///   absent.** Falling back to the host accent gave every node in an
///   uncategorised app an identical bar, which is a stripe that
///   distinguishes nothing.
/// - **Badges only at [`DetailTier::Full`].** Below it the label is
///   unreadable and the chip collides with the title.
///
/// [`DetailTier::Full`]: crate::vendored::chrome::DetailTier::Full
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
    let Node { pos, open, .. } = graph.nodes[node.0];
    let size_override = graph.size_override_of(node);

    // Collect pins. Asked by node id rather than by payload, so a
    // wrapper can answer for nodes the app does not own — a subgraph
    // instance's pins come from its definition's interface.
    let inputs_count = viewer.inputs_of(node, graph);
    let outputs_count = viewer.outputs_of(node, graph);

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

    // `NodeState` lives in Mara memory since PLAN_NODE.md P2, so it is
    // reached through the seam rather than the backend context.
    // `EguiCtx` owns a cheap `Arc` clone, so holding it does not borrow
    // `ui`.
    let seam = mara_backend_egui::EguiCtx::new(ui.ctx());
    let mut node_state = NodeState::load(&seam, node_id);

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

    // Detail tier and app-supplied chrome, resolved once and threaded
    // through the rest of the node. PLAN_NODE.md P6.
    let ladder = style.lod.unwrap_or_default();
    let (tier, _tier_alpha) = lod::tier_for(graph_state.to_global().scaling, ladder);
    let chrome = viewer.node_chrome(node, tier, graph);
    let node_accent = chrome
        .accent
        .unwrap_or_else(mara_core::style::active_accent);
    let selected = graph_state.selected_nodes().contains(&node);
    if selected && style.select_halo.is_none() {
        // The flat rect, kept for callers that have not opted into the
        // layered halo. The halo variant is painted into the underlay
        // slot instead, so it lands beneath the body rather than over
        // it.
        let select_style = style.get_select_style();
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
    let pin_size = style.get_pin_size().max(0.0);

    let pin_placement = style.get_pin_placement();

    let header_drag_space = style.get_header_drag_space().max(Vec2::ZERO);

    // Interact with node frame.
    let r = ui.interact(
        node_frame_rect,
        node_id.with("frame"),
        Sense::click_and_drag(),
    );

    // Captured HERE, not at the fill site: `r` is shadowed further
    // down by the node frame's own `InnerResponse`, whose response was
    // allocated by `Frame::show` with no drag sense and is therefore
    // never dragged. Reading the drag state after that point would
    // silently disable the lift.
    let lifted = r.dragged_by(PointerButton::Primary);
    let node_drag_stopped = r.drag_stopped_by(PointerButton::Primary).then_some(node);
    let hovered_self = r.hovered().then_some(node);

    // Double-clicking a node asks to descend into it. Recorded here and
    // acted on by `show_doc`, which is the only caller that knows what
    // a level is. Registered after the canvas response, so this does
    // not collide with the background double-click-to-centre.
    if r.double_clicked_by(PointerButton::Primary) {
        note_entered(node, node_frame_rect.into());
    }

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
        node_state.clear(&seam);
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
        node_state.clear(&seam);
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

    // Reserve one underlay slot in the painter BEFORE the frame +
    // pins are submitted, so everything drawn into it lands beneath
    // the body — the drop shadow under the frame fill, and the halo
    // under the pins where they intersect (with `PinPlacement::Edge`
    // pins straddle the body outline). Filled after `node_frame.show`
    // returns and the final body rect is known.
    //
    // Reserved unconditionally: `fill_paint_slot` takes an `Option`
    // precisely so a caller can reserve first and decide later, and an
    // unfilled slot is inert. Only rect/shadow/mesh geometry may go in
    // here — the backend's slot filler maps text, images and clips to
    // a no-op shape.
    let underlay_slot = with_mara_ui(node_ui, |mara| mara.reserve_paint_slot());

    let r = mara_backend_egui::egui_frame_for_style_spec(node_frame).show(node_ui, |ui| {
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
                    + mara_item_spacing().y
                    - node_state.payload_offset(openness),
            )
            .into(),
            node_rect.max.into(),
        );

        let node_layout =
            viewer.node_layout(style.get_node_layout(), node, &inputs, &outputs, graph);

        let payload_clip_rect = Rect::from_min_max(
            node_rect.min.into(),
            pos2(node_rect.max.x, f32::INFINITY).into(),
        );

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
                    inputs_size.x + outputs_size.x + mara_item_spacing().x,
                    f32::max(inputs_size.y, outputs_size.y),
                );

                let mut pins_rect = inputs_rect.union(outputs_rect);

                // Show body if there's one.
                if viewer.has_body(&graph.nodes.get(node.0).unwrap().value) {
                    let body_rect = Rect::from_min_max(
                        pos2(
                            inputs_rect.right() + mara_item_spacing().x,
                            payload_rect.top(),
                        )
                        .into(),
                        pos2(
                            outputs_rect.left() - mara_item_spacing().x,
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

                    new_pins_size.x += r.final_rect.width() + mara_item_spacing().x;
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

                let mut next_y = inputs_rect.bottom() + mara_item_spacing().y;

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
                    new_pins_size.y += body_rect.height() + mara_item_spacing().y;

                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }

                    pins_rect = pins_rect.union(body_rect);
                    next_y = body_rect.bottom() + mara_item_spacing().y;
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
                new_pins_size.y += outputs_rect.height() + mara_item_spacing().y;

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

                let mut next_y = outputs_rect.bottom() + mara_item_spacing().y;

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
                    new_pins_size.y += body_rect.height() + mara_item_spacing().y;

                    if !graph.nodes.contains(node.0) {
                        // If removed
                        return;
                    }

                    pins_rect = pins_rect.union(body_rect);
                    next_y = body_rect.bottom() + mara_item_spacing().y;
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
                new_pins_size.y += inputs_rect.height() + mara_item_spacing().y;

                pins_rect = pins_rect.union(inputs_rect);

                pins_rect
            }
        };

        if viewer.has_footer(&graph.nodes[node.0].value) {
            let footer_rect = Rect::from_min_max(
                pos2(node_rect.left(), pins_rect.bottom() + mara_item_spacing().y).into(),
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
            new_pins_size.y += footer_size.y + mara_item_spacing().y;

            if !graph.nodes.contains(node.0) {
                // If removed
                return;
            }
        }

        // Render header frame.
        let mut header_rect = Rect::NAN;

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

        mara_backend_egui::egui_frame_for_style_spec(header_frame).show(
            header_ui,
            |ui: &mut Ui| {
                ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
                    if style.get_collapsible() {
                        let (_, r) = ui.allocate_exact_size(
                            egui::Vec2::from(vec2(
                                mara_core::style::icon_width(),
                                mara_core::style::icon_width(),
                            )),
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

                ui.advance_cursor_after_rect(egui::Rect::from_min_max(
                    header_rect.min.into(),
                    pos2(
                        f32::max(header_rect.max.x, node_rect.max.x),
                        header_rect.min.y,
                    )
                    .into(),
                ));
            },
        );

        with_mara_ui(ui, |mara| mara.expand_to_include(header_rect));
        let header_size = header_rect.size();
        node_state.set_header_height(header_size.y);

        // An explicit size wins over the measured one — the only
        // channel an app has to say "this node is 512x384", which a
        // live image or a chart cannot express through drawn content.
        // Applied here rather than at the measurement sites so every
        // path through the node body honours it.
        let measured = vec2(
            f32::max(header_size.x, new_pins_size.x),
            header_size.y
                + header_frame.total_margin().bottomf()
                + mara_item_spacing().y
                + new_pins_size.y,
        );
        // The override is a floor, not a replacement: a node told to be
        // 512 wide must still grow if its own content needs more, or
        // the content would be clipped by the app's own request.
        let final_size = match size_override {
            Some(o) => vec2(measured.x.max(o.x), measured.y.max(o.y)),
            None => measured,
        };
        node_state.set_size(egui::Vec2::from(final_size));
    });

    // Fill the reserved underlay slot now that we know the final
    // body rect — `r.response.rect` is the rect that was used to
    // render the node frame.
    //
    // Painted back to front within the batch: shadow, then halo.
    // `CornerRadius` and `Margin` are spelled out in full below — the
    // unqualified names in this file resolve to the backend's types
    // via the module-level import, not to vocab's.
    {
        let body_rect = r.response.rect;
        let mut underlay: Vec<mara_core::paint::PaintCmd> = Vec::new();

        // Deck of cards: layered rounded rects offset down-right, so a
        // node that contains something reads as a stack rather than a
        // plain block. Painted first, so they sit behind the shadow and
        // the body both.
        if chrome.stacked > 0 && tier.shows_body() {
            for i in (1..=chrome.stacked.min(DECK_CARDS)).rev() {
                let step = 3.0 * i as f32;
                let alpha = 0.35 / i as f32;
                underlay.push(mara_core::paint::PaintCmd::RectStroke {
                    rect: mara_core::vocab::Rect::from(body_rect.translate(egui::vec2(step, step))),
                    corner: node_frame.corner,
                    stroke: mara_core::vocab::Stroke::new(1.0, node_accent.gamma_multiply(alpha)),
                });
            }
        }

        if let Some(shadow) = style.node_shadow {
            let (offset, blur) = shadow.for_state(lifted);
            underlay.push(mara_core::paint::PaintCmd::Shadow {
                rect: body_rect.into(),
                corner: node_frame.corner,
                offset,
                blur,
                spread: shadow.spread,
                color: shadow.color,
            });
        }

        if let Some(halo) = style.node_halo
            && !selected
        {
            underlay.push(mara_core::paint::PaintCmd::RectStroke {
                rect: body_rect.expand(halo.gap).into(),
                corner: mara_core::vocab::CornerRadius::same(halo.radius),
                stroke: mara_core::vocab::Stroke::new(
                    halo.width,
                    mara_core::vocab::Color32::from(halo.color),
                ),
            });
        }

        // Layered selection halo: one crisp stroke plus three wider,
        // fainter ones. Widest first so the faint outer bands sit under
        // the crisp core rather than washing it out.
        if selected && let Some(spec) = style.select_halo {
            let base = mara_core::vocab::Rect::from(body_rect.expand(spec.margin));
            for i in (0..3).rev() {
                underlay.push(mara_core::paint::PaintCmd::RectStroke {
                    rect: base,
                    corner: mara_core::vocab::CornerRadius::same(spec.radius),
                    stroke: mara_core::vocab::Stroke::new(
                        spec.widths[i],
                        node_accent.gamma_multiply(spec.alphas[i]),
                    ),
                });
            }
            underlay.push(mara_core::paint::PaintCmd::RectStroke {
                rect: base,
                corner: mara_core::vocab::CornerRadius::same(spec.radius),
                stroke: mara_core::vocab::Stroke::new(spec.core_width, node_accent),
            });
        }

        if !underlay.is_empty() {
            with_mara_ui(node_ui, |mara| {
                mara.fill_paint_slot(
                    underlay_slot,
                    Some(mara_core::paint::PaintCmd::Group(underlay)),
                );
            });
        }
    }

    // Header accent bar across the node's top edge, above the frame
    // fill because it is submitted after it. Keyed on `chrome.accent`
    // rather than `node_accent`: the bar states a category, and the host
    // accent is what a node has when it has no category.
    let accent_bar = match (style.header_accent, chrome.accent) {
        (Some(t), Some(_)) if t > 0.0 && tier.shows_body() => t,
        _ => 0.0,
    };

    if accent_bar > 0.0
        && let Some(accent) = chrome.accent
    {
        let (bar, bar_corner) = header_accent_bar(
            mara_core::vocab::Rect::from(r.response.rect),
            node_frame.corner,
            accent_bar,
        );
        with_mara_ui(node_ui, |mara| {
            mara.painter().rect_filled(bar, bar_corner, accent);
        });
    }

    // Badges at the header's right edge. The instance count lands here
    // for shared definitions, which is what stops a user being
    // blindsided when editing one chip changes seven others.
    if !chrome.badges.is_empty() && matches!(tier, crate::vendored::chrome::DetailTier::Full) {
        let body = mara_core::vocab::Rect::from(r.response.rect);
        let text_size = mara_core::style::icon_width() * BADGE_TEXT_FACTOR;
        let top = body.min.y + accent_bar + BADGE_INSET;
        let mut right = body.max.x - BADGE_INSET;
        with_mara_ui(node_ui, |mara| {
            let p = mara.painter();
            let chips = fitting_badges(
                &p,
                &chrome.badges,
                text_size,
                body.width() - BADGE_INSET * 2.0,
            );
            for (badge, &(w, h)) in chrome.badges.iter().zip(chips.iter()).rev() {
                let chip = mara_core::vocab::Rect::from_min_max(
                    mara_core::vocab::Pos2::new(right - w, top),
                    mara_core::vocab::Pos2::new(right, top + h),
                );
                let tint = badge.color.unwrap_or(node_accent);
                p.rect_filled(
                    chip,
                    mara_core::vocab::CornerRadius::same(BADGE_CORNER),
                    tint.gamma_multiply(0.22),
                );
                p.text(
                    chip.center(),
                    mara_core::vocab::Align2::CENTER_CENTER,
                    &badge.label,
                    text_size,
                    tint,
                );
                right -= w + BADGE_GAP;
            }
        });
    }

    if !graph.nodes.contains(node.0) {
        ui.ctx().request_repaint();
        node_state.clear(&seam);
        // If removed
        return None;
    }

    let final_rect = r.response.rect;
    with_mara_ui(ui, |mui| {
        viewer.final_node_rect(node, final_rect.into(), mui, graph)
    });

    node_state.store(&seam);
    Some(DrawNodeResponse {
        node_moved,
        node_to_top,
        drag_released,
        pin_hovered,
        final_rect: r.response.rect.into(),
        node_drag_stopped,
        hovered: hovered_self,
    })
}

/// Cards drawn behind a node that contains a subgraph.
///
/// Two is enough to read as a stack; a third adds a stroke and no
/// information, and it is the layer that pushes a selected instance
/// inside a group past what the eye will parse as one object.
const DECK_CARDS: u8 = 2;

/// Badge label size as a fraction of the icon metric.
const BADGE_TEXT_FACTOR: f32 = 0.8;
/// Gap between the node's edge and the badge strip.
const BADGE_INSET: f32 = 5.0;
/// Gap between two adjacent chips.
const BADGE_GAP: f32 = 4.0;
/// Horizontal padding inside a chip, either side of the label.
const BADGE_PAD_X: f32 = 5.0;
/// Vertical padding inside a chip, above and below the label.
const BADGE_PAD_Y: f32 = 2.0;
const BADGE_CORNER: u8 = 4;

/// Chip sizes for the leading badges that fit in `avail` points.
///
/// Sized from a real text measurement rather than a
/// characters-times-width guess. The guess is wrong in both directions
/// — too wide for `1` or `i`, too narrow for `WW` — and a chip narrower
/// than its own label is exactly how badge text ends up painted outside
/// the node box.
///
/// Anything that does not fit is dropped whole rather than clipped: a
/// half-painted chip reads as a rendering fault, and a chip pushed past
/// the node's left edge reads as a stray label belonging to nobody.
/// Trailing badges go first, on the assumption that an app lists the
/// badge it cares about most first.
fn fitting_badges(
    p: &mara_core::MaraPainter,
    badges: &[crate::vendored::chrome::Badge],
    text_size: f32,
    avail: f32,
) -> SmallVec<[(f32, f32); 4]> {
    let mut out: SmallVec<[(f32, f32); 4]> = SmallVec::new();
    let mut used = 0.0;
    for badge in badges {
        let text = p.measure_text(&badge.label, text_size, false);
        let w = text.x + BADGE_PAD_X * 2.0;
        let next = used + w + if out.is_empty() { 0.0 } else { BADGE_GAP };
        if next > avail {
            break;
        }
        used = next;
        out.push((w, text.y + BADGE_PAD_Y * 2.0));
    }
    out
}

/// The header accent bar's rect and corner radius, given the node's
/// body rect and its own corner radius.
///
/// A bar thinner than twice the node's corner radius cannot repeat that
/// radius — the tessellator clamps a corner to half the shape's height
/// — so a naive full-width bar sticks out past the node's rounded top
/// corners. Insetting each end by the radius the bar had to give up
/// keeps it provably inside: at every height down the corner arc the
/// bar's edge is at least as far in as the node's own.
fn header_accent_bar(
    body: mara_core::vocab::Rect,
    corner: mara_core::vocab::CornerRadius,
    thickness: f32,
) -> (mara_core::vocab::Rect, mara_core::vocab::CornerRadius) {
    #![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

    let cap = (thickness * 0.5).clamp(0.0, 255.0);
    let nw = f32::from(corner.nw).min(cap);
    let ne = f32::from(corner.ne).min(cap);
    let rect = mara_core::vocab::Rect::from_min_max(
        mara_core::vocab::Pos2::new(body.min.x + (f32::from(corner.nw) - nw), body.min.y),
        mara_core::vocab::Pos2::new(
            body.max.x - (f32::from(corner.ne) - ne),
            body.min.y + thickness,
        ),
    );
    let radius =
        mara_core::vocab::CornerRadius::from_corners(nw.round() as u8, ne.round() as u8, 0, 0);
    (rect, radius)
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

/// `GraphStyle` must stay `Copy`: `GraphWidget` is `Copy` and
/// `GraphWidget::style` is a `const fn`, so a `String`/`Vec`/`HashMap`
/// field would break both plus every `.style(..)` call site — and the
/// errors would land at the call sites rather than at the field that
/// caused them. This fails at the definition instead.
const _: () = {
    const fn is_copy<T: Copy>() {}
    is_copy::<GraphStyle>();
};

/// A node's outer rect — body plus frame margin — in graph space,
/// **without drawing it**.
///
/// The frame pass needs every member's extent before the node loop
/// runs, so it cannot wait for `draw_node` to report one. This
/// reproduces `draw_node`'s prologue against the same cached
/// `NodeState`, so the two agree to the pixel in the ordinary case.
///
/// It deliberately uses the **style's** node frame rather than calling
/// `NodeViewer::node_frame`. That hook takes `&mut self` and needs the
/// node's pins built, so consulting it here would run every viewer's
/// per-node logic twice per frame. The cost is that a viewer which
/// overrides the frame margin *per node* shifts that node's
/// contribution to its group's bounds by the margin difference — a few
/// points, on a box that is 12 points padded anyway.
fn node_frame_rect_of<T>(
    ctx: &egui::Context,
    graph_id: Id,
    node: NodeId,
    graph: &Graph<T>,
    style: &GraphStyle,
) -> mara_core::vocab::Rect {
    let Some(info) = graph.get_node_info(node) else {
        return mara_core::vocab::Rect::NAN;
    };
    let node_id = graph_id.with(("graph-node", node));
    let openness = ctx.animate_bool(node_id, info.open);
    let seam = mara_backend_egui::EguiCtx::new(ctx);
    let state = NodeState::load(&seam, node_id);
    let rect = state.node_rect(egui::Pos2::from(info.pos).round_ui(), openness);
    let margin = style
        .get_node_frame(mara_core::style::active_accent())
        .total_margin();
    mara_core::vocab::Rect::from(rect).expand_by(margin)
}

/// Run `body` with the sealed surface over a backend `Ui`.
///
/// `NodeViewer` speaks `MaraUi` since WS-D1.4, while this file's render
/// path is still backend-typed. Wrapping here keeps the two changes
/// separable; the helper disappears when the render path ports.
fn with_mara_ui<R>(ui: &mut Ui, body: impl for<'a> FnOnce(&mut mara_core::MaraUi<'a>) -> R) -> R {
    let accent = mara_core::style::active_accent();
    let mut raw = mara_backend_egui::__internal_backend_from_raw(ui);
    let mut mara = mara_core::MaraUi::__internal_over(&mut raw, accent);
    body(&mut mara)
}

// ── Nested documents (PLAN_NODE.md P7) ──────────────────────────────

/// What one `show_doc` pass decided.
pub struct GraphOutcome {
    pub response: MaraResponse,
    /// The level rendered this frame.
    pub path: crate::vendored::nav::NodePath,
    /// One entry per level from the root down. Handed back as data so
    /// the host can paint it as chrome; a canvas-drawn breadcrumb would
    /// pan and zoom with the graph, and would fight Mara's enforced
    /// top bar.
    pub breadcrumb: Vec<crate::vendored::nav::Crumb>,
    /// Selection at the level rendered, as stable ids.
    pub selection: Vec<NodeUid>,
}

/// Where in a document the widget is currently looking.
///
/// UI state, not model state: it lives in Mara memory beside the
/// viewport transform, never in `GraphDoc`, so navigating somewhere
/// does not dirty the document or enter the saved file.
#[derive(Clone, Default)]
struct NavState {
    path: crate::vendored::nav::NodePath,
}

impl NavState {
    fn load(cx: &dyn mara_core::context::MaraCtx, id: Id) -> Self {
        cx.memory()
            .get_temp::<Self>(mara_core::vocab::Id::from(id))
            .unwrap_or_default()
    }

    fn save(self, cx: &dyn mara_core::context::MaraCtx, id: Id) {
        cx.memory().set_temp(mara_core::vocab::Id::from(id), self);
    }
}

/// Wraps the app's viewer so subgraph instances answer for themselves.
///
/// An instance node's pin count comes from its definition's interface,
/// not from the app — the app may not even know the node exists, since
/// collapsing created it. Interception happens on the id-taking
/// widenings, which is precisely why they exist: `inputs(&T)` cannot
/// tell which node it is being asked about.
struct DocViewer<'a, V> {
    inner: &'a mut V,
    ifaces: &'a crate::vendored::subgraph::IfaceTable,
    /// Instances per definition, computed once per frame. Walking the
    /// document per node would be quadratic.
    counts: &'a std::collections::HashMap<DefId, u32>,
}

impl<T, V: NodeViewer<T>> NodeViewer<T> for DocViewer<'_, V> {
    fn title(&mut self, node: &T) -> String {
        self.inner.title(node)
    }

    fn inputs(&mut self, node: &T) -> usize {
        self.inner.inputs(node)
    }

    fn outputs(&mut self, node: &T) -> usize {
        self.inner.outputs(node)
    }

    fn inputs_of(&mut self, node: NodeId, graph: &Graph<T>) -> usize {
        match self.iface_for(node, graph) {
            Some(i) => i.inputs as usize,
            None => self.inner.inputs_of(node, graph),
        }
    }

    fn outputs_of(&mut self, node: NodeId, graph: &Graph<T>) -> usize {
        match self.iface_for(node, graph) {
            Some(i) => i.outputs as usize,
            None => self.inner.outputs_of(node, graph),
        }
    }

    fn title_of(&mut self, node: NodeId, graph: &Graph<T>) -> String {
        match self.iface_for(node, graph) {
            Some(i) => i.name.clone(),
            None => self.inner.title_of(node, graph),
        }
    }

    fn has_body_of(&mut self, node: NodeId, graph: &Graph<T>) -> bool {
        if self.iface_for(node, graph).is_some() {
            return false;
        }
        self.inner.has_body_of(node, graph)
    }

    fn show_input(
        &mut self,
        pin: &InPin,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<T>,
    ) -> impl NodePin + 'static {
        self.inner.show_input(pin, ui, graph)
    }

    fn show_output(
        &mut self,
        pin: &OutPin,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<T>,
    ) -> impl NodePin + 'static {
        self.inner.show_output(pin, ui, graph)
    }

    fn node_chrome(
        &mut self,
        node: NodeId,
        tier: crate::vendored::chrome::DetailTier,
        graph: &Graph<T>,
    ) -> crate::vendored::chrome::NodeChrome {
        let mut chrome = self.inner.node_chrome(node, tier, graph);
        if let Some(iface) = self.iface_for(node, graph) {
            // An instance takes its definition's colour unless the app
            // has an opinion, so every placement of a chip reads alike.
            if chrome.accent.is_none() {
                chrome.accent = iface.color;
            }
            // Two cards behind it: the read is "this contains
            // something", at any zoom and any detail tier.
            if chrome.stacked == 0 {
                chrome.stacked = 2;
            }
            // A count, but only when sharing is actually in play.
            // Editing one chip changes every instance of it, and a user
            // who cannot see that is about to be surprised.
            if let Some(n) = self
                .counts
                .get(&self.def_of(node, graph).unwrap_or(DefId(usize::MAX)))
                && *n > 1
                && chrome.badges.is_empty()
            {
                chrome
                    .badges
                    .push(crate::vendored::chrome::Badge::new(format!("×{n}")));
            }
        }
        chrome
    }

    fn wire_fx(
        &mut self,
        from: &OutPinId,
        to: &InPinId,
        graph: &Graph<T>,
    ) -> crate::vendored::chrome::WireFx {
        self.inner.wire_fx(from, to, graph)
    }

    fn has_graph_menu(&mut self, pos: mara_core::vocab::Pos2, graph: &mut Graph<T>) -> bool {
        self.inner.has_graph_menu(pos, graph)
    }

    fn show_graph_menu(
        &mut self,
        pos: mara_core::vocab::Pos2,
        ui: &mut mara_core::MaraUi<'_>,
        graph: &mut Graph<T>,
    ) {
        self.inner.show_graph_menu(pos, ui, graph);
    }

    fn connect(&mut self, from: &OutPin, to: &InPin, graph: &mut Graph<T>) {
        self.inner.connect(from, to, graph);
    }

    fn disconnect(&mut self, from: &OutPin, to: &InPin, graph: &mut Graph<T>) {
        self.inner.disconnect(from, to, graph);
    }
}

/// Lets the app's viewer serve as the payload source for a structural
/// edit.
///
/// `NodeViewer` cannot be the `NodeFactory` bound directly — it is
/// dyn-incompatible through its RPITIT pin methods, and `collapse` takes
/// `&mut dyn NodeFactory<T>` precisely so the algorithm can be tested
/// with `T = ()`.
struct ViewerFactory<'a, V>(&'a mut V);

impl<T, V: NodeViewer<T>> crate::vendored::subgraph::NodeFactory<T> for ViewerFactory<'_, V> {
    fn instance_node(
        &mut self,
        def: DefId,
        name: &str,
        ports: &crate::vendored::subgraph::Ports,
    ) -> Option<T> {
        self.0.make_instance_node(def, name, ports)
    }

    fn port_node(&mut self, spec: &crate::vendored::subgraph::PortSpec<'_>) -> Option<T> {
        self.0.make_port_node(spec)
    }
}

impl<V> DocViewer<'_, V> {
    fn def_of<T>(&self, node: NodeId, graph: &Graph<T>) -> Option<DefId> {
        graph.instance_def(graph.uid_of(node)?)
    }

    fn iface_for<T>(
        &self,
        node: NodeId,
        graph: &Graph<T>,
    ) -> Option<&crate::vendored::subgraph::Iface> {
        let uid = graph.uid_of(node)?;
        let def = graph.instance_def(uid)?;
        self.ifaces.get(&def)
    }
}

impl GraphWidget {
    /// Render one level of a nested document.
    ///
    /// Which level is UI state, held in Mara memory beside the viewport
    /// transform. Double-clicking an instance descends; `Esc` or
    /// `Backspace` on empty canvas ascends.
    ///
    /// # Why the ids are salted
    ///
    /// `graph_id` keys the sublayer, the whole `GraphState`, every
    /// node's measured size and the wire cache. Entering a child level
    /// without re-keying would inherit the parent's pan, zoom,
    /// selection and draw order — and, because `NodeId` is a per-graph
    /// slab index, the parent's node 0 and the child's node 0 would
    /// share one size cache and one collapse animation. Salting once
    /// with the path re-keys all of them together, and exiting restores
    /// the parent's camera for free.
    pub fn show_doc<T, V>(
        &self,
        doc: &mut crate::vendored::subgraph::GraphDoc<T>,
        viewer: &mut V,
        mara: &mut mara_core::MaraUi<'_>,
    ) -> GraphOutcome
    where
        T: Clone,
        V: NodeViewer<T>,
    {
        use crate::vendored::nav::NodePath;

        let ui = mara.__internal_raw_ui();
        let base_id = self.get_id(ui.id());
        let seam = mara_backend_egui::EguiCtx::new(ui.ctx());

        // A saved path can outlive the instance it points through.
        let mut nav = NavState::load(&seam, base_id);
        nav.path = doc.prune_path(&nav.path);

        let ifaces = doc.iface_snapshot();
        // Instance counts once per frame, not once per node.
        let counts: std::collections::HashMap<DefId, u32> = ifaces
            .keys()
            .map(|d| (*d, doc.instance_count(*d)))
            .collect();
        let breadcrumb = doc.breadcrumb(&nav.path);
        let level_id = base_id.with(("lvl", nav.path.depth(), nav.path.last().map(|u| u.0)));

        let (response, entered, exit_requested, selection) = {
            let Some(level) = doc.level_mut(&nav.path) else {
                // The path pruned to something unrenderable; fall back
                // to the root rather than drawing nothing.
                nav.path = NodePath::root();
                let level = &mut doc.root;
                let mut wrapped = DocViewer {
                    inner: viewer,
                    ifaces: &ifaces,
                    counts: &counts,
                };
                let r = show_graph(
                    base_id,
                    self.style,
                    self.min_size,
                    self.max_size,
                    level,
                    &mut wrapped,
                    ui,
                );
                nav.clone().save(&seam, base_id);
                return GraphOutcome {
                    response: r,
                    path: NodePath::root(),
                    breadcrumb: doc.breadcrumb(&NodePath::root()),
                    selection: Vec::new(),
                };
            };

            let mut wrapped = DocViewer {
                inner: viewer,
                ifaces: &ifaces,
                counts: &counts,
            };
            let r = show_graph(
                level_id,
                self.style,
                self.min_size,
                self.max_size,
                level,
                &mut wrapped,
                ui,
            );

            // Which instance was double-clicked, resolved to a stable
            // id before the borrow ends.
            let entered = ENTERED.with(|e| e.take()).and_then(|(node, rect)| {
                let uid = level.uid_of(node)?;
                level.instance_def(uid).map(|_| (uid, rect))
            });

            let exit = ui
                .ctx()
                .input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Backspace));

            let selection: Vec<NodeUid> = GraphState::selection(&seam, level_id.into())
                .iter()
                .filter_map(|n| level.uid_of(*n))
                .collect();

            (r, entered, exit, selection)
        };

        // Structural edits run here, where the whole document is in
        // hand. `T: Clone` is not required by `show_doc`, so the edits
        // are attempted only when the payload type supports them — see
        // the bound on the helper below.
        if let Some(intent) = INTENT.with(std::cell::Cell::take) {
            apply_intent(doc, &nav.path, &selection, intent, viewer);
        }

        if let Some((uid, instance_rect)) = entered {
            let candidate = nav.path.child(uid);
            if doc.resolve(&candidate).is_ok() {
                nav.path = candidate;

                // ── Portal dive ──
                //
                // Start the child's camera framed on the block that was
                // just opened, then aim it at the interior fitted to the
                // viewport. The spring runs the rest, so entering reads
                // as going *into* something rather than as a page
                // reload. Costs two pure calls and one target, because
                // the spring already exists.
                if self.style.camera_spring.is_some()
                    && let Some(child) = doc.level(&nav.path)
                {
                    let mut interior = mara_core::vocab::Rect::NOTHING;
                    for (id, _) in child.node_ids() {
                        if let Some(info) = child.get_node_info(id) {
                            interior = interior.union(mara_core::vocab::Rect::from_min_size(
                                info.pos,
                                mara_core::vocab::Vec2::new(160.0, 90.0),
                            ));
                        }
                    }
                    if interior.is_finite() {
                        let viewport: mara_core::vocab::Rect = response.rect;
                        let child_id =
                            base_id.with(("lvl", nav.path.depth(), nav.path.last().map(|u| u.0)));
                        let start =
                            crate::vendored::camera::dive_target(instance_rect, interior, 8.0);
                        let end = crate::vendored::camera::settled_target(
                            interior.expand(80.0),
                            viewport,
                            0.1,
                            2.0,
                        );
                        seed_camera(&seam, child_id, start, end);
                    }
                }
            }
        } else if exit_requested && let Some(up) = nav.path.parent() {
            nav.path = up;
        }

        let path = nav.path.clone();
        nav.save(&seam, base_id);

        GraphOutcome {
            response,
            path,
            breadcrumb,
            selection,
        }
    }

    /// Jump straight to a level, for a breadcrumb click or a deep link.
    pub fn set_open_path(
        mara: &mut mara_core::MaraUi<'_>,
        id: mara_core::vocab::Id,
        path: &crate::vendored::nav::NodePath,
    ) {
        let ui = mara.__internal_raw_ui();
        let seam = mara_backend_egui::EguiCtx::new(ui.ctx());
        NavState { path: path.clone() }.save(&seam, Id::from(id));
    }
}

/// Plant a level's starting camera and its destination, so the first
/// frame at that level opens from the block rather than cutting to it.
fn seed_camera(
    cx: &dyn mara_core::context::MaraCtx,
    level_id: Id,
    start: mara_core::transform::Transform,
    end: mara_core::transform::Transform,
) {
    GraphState::seed_view(cx, level_id.into(), start, end);
}

thread_local! {
    /// The instance double-clicked this pass.
    ///
    /// A thread-local rather than a return value because the signal
    /// originates deep inside `draw_node`, which is shared by
    /// `show`—which has no concept of levels—and `show_doc`. Widening
    /// `show_graph`'s return type for a feature only one caller uses
    /// would push the cost onto every other call site. Cleared on read.
    static ENTERED: std::cell::Cell<Option<(NodeId, mara_core::vocab::Rect)>> =
        const { std::cell::Cell::new(None) };
}

/// Carry out a structural gesture against the document.
///
/// Every failure is swallowed deliberately: a user pressing Ctrl+G on a
/// selection that cannot be collapsed — one that spans levels, or whose
/// viewer declines to mint a payload — should get nothing, not a panic
/// and not a half-built definition. `collapse` and `expand` both leave
/// the document untouched when they return `Err`.
fn apply_intent<T, V>(
    doc: &mut crate::vendored::subgraph::GraphDoc<T>,
    path: &crate::vendored::nav::NodePath,
    selection: &[NodeUid],
    intent: StructuralIntent,
    viewer: &mut V,
) where
    T: Clone,
    V: NodeViewer<T>,
{
    if selection.is_empty() {
        return;
    }
    let mut factory = ViewerFactory(viewer);
    match intent {
        StructuralIntent::Collapse => {
            let _ = doc.collapse(
                path,
                selection,
                crate::vendored::subgraph::DefScope::Local,
                "Group",
                &mut factory,
            );
        }
        StructuralIntent::Expand => {
            // Only meaningful for a single instance. Expanding several
            // at once would be ambiguous about where their contents go.
            if let [only] = selection {
                let _ = doc.expand(path, *only);
            }
        }
    }
}

/// What a structural gesture asked for, for `show_doc` to carry out.
///
/// Collapse needs the whole `GraphDoc`; the gesture is detected deep
/// inside `show_graph`, which only ever holds one level. Rather than
/// widen the return type of a function `show` also calls — and which
/// has no concept of levels — the request travels the same way
/// `ENTERED` does.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuralIntent {
    /// Fold the selection into a new definition.
    Collapse,
    /// Dissolve the selected instance back into this level.
    Expand,
}

thread_local! {
    static INTENT: std::cell::Cell<Option<StructuralIntent>> =
        const { std::cell::Cell::new(None) };
}

/// Record a structural gesture, for `show_doc` to act on.
pub(crate) fn note_intent(intent: StructuralIntent) {
    INTENT.with(|i| i.set(Some(intent)));
}

/// Record that `node` was double-clicked, for `show_doc` to act on.
///
/// The rect travels with it because the dive animation needs to know
/// where on screen the block was — by the time the child level renders,
/// that is gone.
pub(crate) fn note_entered(node: NodeId, rect: mara_core::vocab::Rect) {
    ENTERED.with(|e| e.set(Some((node, rect))));
}
