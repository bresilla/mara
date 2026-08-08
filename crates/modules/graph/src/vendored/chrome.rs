//! Per-node and per-wire appearance descriptors — PLAN_NODE.md P6.
//!
//! # The point of this file
//!
//! The crate must serve a logic simulator, an n8n-style automation
//! editor and an ML pipeline without learning what any of them mean. So
//! it supplies *machinery* — accent colours, status rings, badges,
//! gradient wires, flow pulses — and the app supplies meaning through
//! two returned structs. The logic sim maps hi/lo to `WireFx::color_a`
//! and `color_b` plus a `Flow::Pulse` on each value change; n8n maps run
//! state to `NodeChrome::status`; the ML pipeline maps a preview image
//! to `NodeChrome::thumbnail`. One mechanism, three domains, and no
//! domain vocabulary anywhere in here.
//!
//! Nothing in this file defines bus widths, signal levels, mute/bypass
//! or clocking. If a type here starts to, it has drifted.
//!
//! Every enum is `#[non_exhaustive]` from the first commit, because the
//! next variant is otherwise a breaking change.
//!
//! This file is checked by `make check` to contain no backend types.

use mara_core::vocab::{Color32, TextureId};
use smallvec::SmallVec;

/// How much of a node to draw, chosen from the viewport scale.
///
/// Reaches the viewer so an app-drawn body degrades in step with the
/// crate's own chrome — a node that keeps rendering a full chart at
/// `Blob` tier defeats the whole ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum DetailTier {
    /// Everything.
    Full,
    /// Pin labels dropped.
    Compact,
    /// Pins are dots, title truncated, no body, no animation.
    Pins,
    /// A solid rounded rect and nothing else.
    Blob,
}

impl DetailTier {
    /// Whether pin labels are worth drawing at this tier.
    #[must_use]
    pub const fn shows_pin_labels(self) -> bool {
        matches!(self, Self::Full)
    }

    /// Whether the node body is worth drawing at this tier.
    #[must_use]
    pub const fn shows_body(self) -> bool {
        matches!(self, Self::Full | Self::Compact)
    }

    /// Whether individual pins are worth drawing at this tier.
    #[must_use]
    pub const fn shows_pins(self) -> bool {
        !matches!(self, Self::Blob)
    }
}

/// What a node is doing, as far as the app is concerned.
///
/// Deliberately generic: `Running` covers an n8n node executing, a
/// shader compiling and a model inferring. The crate renders it; it
/// never interprets it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Status {
    Ok,
    Error,
    Running { progress: f32 },
    Waiting,
    Disabled,
}

impl Status {
    /// The colour a `Blob`-tier node paints in.
    ///
    /// Status rather than category, so that an error stays findable in
    /// a graph zoomed out far enough that every node is a rectangle.
    /// That is the case where finding it matters most.
    #[must_use]
    pub fn blob_color(self, fallback: Color32) -> Color32 {
        match self {
            Self::Error => Color32::from_rgba_premultiplied(200, 60, 50, 255),
            Self::Running { .. } => Color32::from_rgba_premultiplied(70, 140, 210, 255),
            Self::Waiting => Color32::from_rgba_premultiplied(190, 150, 60, 255),
            Self::Disabled => Color32::from_gray(90),
            Self::Ok => fallback,
        }
    }
}

/// A short marker drawn on a node's header.
#[derive(Clone, Debug, PartialEq)]
pub struct Badge {
    pub label: String,
    pub color: Option<Color32>,
}

impl Badge {
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            color: None,
        }
    }
}

/// The overall silhouette of a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum NodeShape {
    /// Header, body, frame — the ordinary node.
    #[default]
    Card,
    /// A bare dot with no header and no frame.
    ///
    /// Reroute and pass-through nodes are the primary long-wire
    /// readability tool in every logic simulator and in Blender, and
    /// they cannot be expressed as a `Card` — wrapping a 1-in/1-out
    /// reroute in a full frame plus header plus collapse chevron is not
    /// a reroute node, it is a small node.
    Dot,
}

/// Everything the crate needs to know to dress one node.
#[derive(Clone, Debug, Default)]
pub struct NodeChrome {
    /// Header accent. `None` leaves the node on the host's accent.
    pub accent: Option<Color32>,
    pub status: Option<Status>,
    pub badges: SmallVec<[Badge; 2]>,
    /// A preview image drawn in the node body — the ML pipeline's
    /// display node, a video frame, a chart already rasterised.
    pub thumbnail: Option<TextureId>,
    pub shape: NodeShape,
    /// How many cards to stack behind this node — "there is more
    /// inside this than you can see".
    ///
    /// The cheapest possible signal that a node contains a world, and
    /// the one that survives every zoom level and every LOD tier. Set
    /// automatically for subgraph instances; an app can set it for
    /// anything it considers a container.
    pub stacked: u8,
    /// `0.0` fully dimmed, `1.0` fully lit. Driven by hover focus; an
    /// app can also drive it directly to grey out irrelevant nodes.
    pub emphasis: f32,
}

impl NodeChrome {
    /// A chrome with the node lit and nothing else set.
    #[must_use]
    pub fn lit() -> Self {
        Self {
            emphasis: 1.0,
            ..Self::default()
        }
    }
}

/// How a wire is routed between its two pins.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Routing {
    /// Smooth curve. The default, and what most node editors use.
    #[default]
    Bezier,
    /// Right angles with rounded corners. What a schematic wants, and
    /// what makes a dense logic graph readable.
    Orthogonal,
    /// 45-degree diagonals.
    Subway,
}

/// Animation carried along a wire.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Flow {
    /// A continuous procession of dots.
    Continuous { speed: f32, dots: u8 },
    /// A single travelling pulse emitted at time `t0`.
    ///
    /// Preferred over `Continuous` for simulation: you watch causality
    /// propagate rather than watching every wire shimmer, and an idle
    /// graph costs zero repaints because there is nothing in flight.
    Pulse { t0: f64 },
}

/// Everything the crate needs to know to dress one wire.
#[derive(Clone, Debug, Default)]
pub struct WireFx {
    /// Colour at the source pin. `None` falls back to the pin's own.
    pub color_a: Option<Color32>,
    /// Colour at the target pin, interpolated along the wire.
    pub color_b: Option<Color32>,
    pub width: Option<f32>,
    /// Draw as two parallel strokes — the schematic convention for a
    /// bus. The crate knows nothing about bit widths; an app that has
    /// them sets this and `width`.
    pub double_line: bool,
    pub routing: Option<Routing>,
    pub flow: Option<Flow>,
    pub label: Option<String>,
    pub emphasis: f32,
}

impl WireFx {
    #[must_use]
    pub fn lit() -> Self {
        Self {
            emphasis: 1.0,
            ..Self::default()
        }
    }
}

/// A fixed eight-colour palette for node accents.
///
/// Constant lightness and chroma (L≈0.65, C≈0.14 in OKLCH) at hues
/// 20/55/95/145/190/250/300/340, so no swatch shouts louder than
/// another and a graph coloured from them reads as one design.
///
/// Fixed rather than a free picker on purpose: user-picked colours make
/// shared graphs look chaotic, and every large node-editor ecosystem
/// that shipped a picker converged on preset swatches anyway.
pub const ACCENT_PALETTE: [Color32; 8] = [
    Color32::from_rgba_premultiplied(219, 118, 108, 255),
    Color32::from_rgba_premultiplied(200, 138, 79, 255),
    Color32::from_rgba_premultiplied(163, 158, 71, 255),
    Color32::from_rgba_premultiplied(106, 172, 108, 255),
    Color32::from_rgba_premultiplied(78, 170, 160, 255),
    Color32::from_rgba_premultiplied(110, 158, 214, 255),
    Color32::from_rgba_premultiplied(174, 138, 208, 255),
    Color32::from_rgba_premultiplied(212, 122, 174, 255),
];

/// Pick a palette entry by index, wrapping.
///
/// For apps that colour by category and want stable, non-clashing
/// colours without choosing them.
#[must_use]
pub const fn palette(index: usize) -> Color32 {
    ACCENT_PALETTE[index % ACCENT_PALETTE.len()]
}
