//! A node-graph renderer built as one system.
//!
//! # Why this exists alongside `vendored`
//!
//! The vendored renderer grew from an egui widget, and its geometry is
//! a *consequence* of what its layout engine drew. That is workable
//! until you need two nodes to look alike, at which point it is not
//! fixable in place: width comes from content, row pitch comes from
//! content, and text is truncated against a budget the painter does not
//! share. Several rounds of patching it produced a graph that was
//! reported, accurately, as inconsistent.
//!
//! This module inverts the order. [`layout`] decides every rect from a
//! node's *declared* shape with no painting involved, and [`paint`]
//! draws what it is handed. Two nodes with the same shape are then the
//! same size by construction rather than by coincidence, and every
//! geometric claim is testable without a window.
//!
//! The model is unchanged — [`crate::Graph`], frames and subgraphs are
//! well covered and were never the problem.
//!
//! Everything here is written against `mara_core` only, so this half of
//! the crate carries no backend dependency at all.

pub mod doc;
pub mod layout;
pub mod paint;
pub mod spec;
pub mod view;

pub use doc::{DocResponse, DocViewState, go_up, show_doc};
pub use layout::{NodeLayout, NodeShape, layout_node};
pub use paint::{NodeState, paint_canvas, paint_node, paint_pin, paint_wire, wire_points};
pub use spec::{GraphPalette, GraphSpec, NodeSpec, ShadowSpec};
pub use view::{Camera, GraphResponse, GraphView, GraphViewState, distance_to_wire, show_graph};
