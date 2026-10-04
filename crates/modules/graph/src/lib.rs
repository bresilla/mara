//! # mara_graph
//!
//! Standalone node-graph crate. Vendored fork upstream
//! plus a sharp-zoom [`node_view`] helper that renders the graph
//! into a secondary backend context backed by a wgpu texture.
//! See `ACKNOWLEDGEMENTS.md` for upstream attribution.
//!
//! The crate is theme-neutral: it ships [`default_graph_style`] as
//! a sensible default starting point, and lets the caller
//! configure everything else. Mara-tinted styling lives in the
//! `mara_core` crate behind the optional `graph` feature, which
//! depends on this crate and wires the embed / maximise affordance
//! on top.
//!
//! Use it standalone:
//!
//! ```ignore
//! use mara_graph::{Graph, GraphWidget, NodeViewer, default_graph_style};
//!
//! let style = default_graph_style();
//! GraphWidget::new()
//!     .id_salt("my_graph")
//!     .style(style)
//!     .min_size(mara_graph::vec2(320.0, 260.0))
//!     .show(&mut state.graph, &mut state.viewer, ui);
//! ```

pub mod node_view;
// Re-export the geometry vocab this crate's public API speaks, so
// consumers (and doctests, which link only this crate) do not need a
// direct `mara_core` dependency just to place a node.
pub use mara_core::vocab::{Pos2, Vec2, pos2, vec2};

pub mod render;
mod vendored;

pub use vendored::{
    Graph, GraphExt, InPin, InPinId, Node, NodeId, NodeUid, OutPin, OutPinId,
    camera::CameraSpring,
    chrome::{
        ACCENT_PALETTE, Badge, DetailTier, Flow, NodeChrome, NodeShape, Routing, Status, WireFx,
        palette,
    },
    frames::{DisposeMode, Frame, FrameId, FrameMove, fit_frame_bounds, title_band_rect},
    nav::{Crumb, NodePath},
    subgraph::{
        DefId, DefScope, GraphDef, GraphDoc, GraphError, Iface, IfaceTable, NodeFactory, PortDef,
        PortDir, PortId, PortSpec, Ports, UidRemap,
    },
    ui::{
        AnyPins, BackgroundPattern, Dots, GraphOutcome, GraphState, GraphStyle, GraphWidget, Grid,
        HaloSpec, Hex, NodeHalo, NodeLayout, NodeLayoutKind, NodePin, NodeViewer, PinInfo,
        PinPlacement, PinShape, PinWireInfo, SelectionStyle, ShadowSpec, WireColorMode, WireLayer,
        WireStyle,
        lod::{LodLadder, animations_enabled, smoothstep, tier_for},
    },
};

pub use node_view::{NodeViewBackend, NodeViewState, show, show_with_anchor};

/// Everything a consumer needs, in one glob.
///
/// The facade re-exports this wholesale (`pub use
/// mara_graph::prelude::*` in `mara::extras::graph`) rather than
/// maintaining a second hand-written list. Two lists that nothing
/// checks against each other is how `PinWireInfo`, `WireStyle`,
/// `WireLayer`, `SelectionStyle` and `NodeLayoutKind` ended up
/// unreachable from the demo while the build stayed green — a public
/// field whose *type* cannot be named can only ever be left at its
/// default. PLAN_NODE.md P2.
pub mod prelude {
    pub use crate::node_view::{NodeViewBackend, NodeViewState, show, show_with_anchor};
    pub use crate::{
        ACCENT_PALETTE, AnyPins, BackgroundPattern, Badge, CameraSpring, Crumb, DefId, DefScope,
        DetailTier, DisposeMode, Dots, Flow, Frame, FrameId, FrameMove, Graph, GraphDef, GraphDoc,
        GraphError, GraphOutcome, GraphState, GraphStyle, GraphWidget, Grid, HaloSpec, Hex, Iface,
        IfaceTable, InPin, InPinId, LodLadder, Node, NodeChrome, NodeFactory, NodeHalo, NodeId,
        NodeLayout, NodeLayoutKind, NodePath, NodePin, NodeShape, NodeUid, NodeViewer, OutPin,
        OutPinId, PinInfo, PinPlacement, PinShape, PinWireInfo, PortDef, PortDir, PortId, PortSpec,
        Ports, Pos2, Routing, SelectionStyle, ShadowSpec, Status, UidRemap, Vec2, WireColorMode,
        WireFx, WireLayer, WireStyle, default_graph_style, fit_frame_bounds, palette, pos2,
        title_band_rect, vec2,
    };
}

/// A [`GraphStyle`] with library defaults — no mara theming, just
/// `GraphStyle::new()`. Use this for a vanilla node graph that
/// inherits whatever style the parent backend context carries.
#[must_use]
pub fn default_graph_style() -> GraphStyle {
    GraphStyle::new()
}

/// Never called. Names every public `mara_graph` signature that used to
/// be egui-typed, in Mara vocab, so that reintroducing a backend type
/// into one of them fails the build here rather than being noticed when
/// a sealed consumer cannot spell the argument.
///
/// The same technique as `_offscreen_path_is_reachable` in
/// `mara::extras::graph`: a compile-time assertion written as code the
/// compiler must typecheck, rather than a grep that a re-export walks
/// past. PLAN_NODE.md P2.
#[allow(dead_code)]
fn _public_api_is_vocab_typed() {
    fn takes_id(w: GraphWidget, id: mara_core::vocab::Id) -> GraphWidget {
        w.id(id)
    }

    fn nudges(cx: &dyn mara_core::context::MaraCtx, id: mara_core::vocab::Id, d: Vec2) {
        GraphState::nudge_saved_translation(cx, id, d);
    }

    fn reads_selection(
        cx: &dyn mara_core::context::MaraCtx,
        id: mara_core::vocab::Id,
    ) -> impl IntoIterator<Item = NodeId> {
        GraphState::selection(cx, id)
    }

    fn draws_background<T, V: NodeViewer<T>>(
        v: &mut V,
        pattern: Option<&BackgroundPattern>,
        viewport: &mara_core::vocab::Rect,
        style: &GraphStyle,
        painter: &mara_core::mui::MaraPainter,
        graph: &Graph<T>,
    ) {
        v.draw_background(pattern, viewport, style, painter, graph);
    }

    // No calls needed: a nested item is typechecked whether or not it
    // is reached, so declaring these is the assertion.
}
