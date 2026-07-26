//! # mara_graph
//!
//! Standalone node-graph crate for egui. Vendored fork upstream
//! plus a sharp-zoom [`node_view`] helper that renders the graph
//! into a secondary [`egui::Context`] backed by a wgpu texture.
//! See `ACKNOWLEDGEMENTS.md` for upstream attribution.
//!
//! The crate is theme-neutral: it ships [`default_graph_style`] as
//! a sensible egui-default starting point, and lets the caller
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
//!     .min_size(egui::vec2(320.0, 260.0))
//!     .show(&mut state.graph, &mut state.viewer, ui);
//! ```

pub mod node_view;
// Re-export the geometry vocab this crate's public API speaks, so
// consumers (and doctests, which link only this crate) do not need a
// direct `mara_core` dependency just to place a node.
pub use mara_core::vocab::{Pos2, pos2};

mod vendored;

pub use vendored::{
    Graph, InPin, InPinId, Node, NodeId, OutPin, OutPinId,
    ui::{
        AnyPins, BackgroundPattern, Dots, GraphState, GraphStyle, GraphWidget, Grid, Hex, NodeHalo,
        NodeLayout, NodePin, NodeViewer, PinInfo, PinPlacement, PinShape, WireColorMode,
    },
};

pub use node_view::{NodeViewBackend, NodeViewState, show, show_with_anchor};

/// A [`GraphStyle`] with library defaults — no mara theming, just
/// `GraphStyle::new()`. Use this for a vanilla node graph that
/// inherits whatever style the parent `egui::Context` carries.
#[must_use]
pub fn default_graph_style() -> GraphStyle {
    GraphStyle::new()
}
