//! Mara-themed wrappers around the standalone host-widget crates.
//!
//! The vendored node-graph and code-editor implementations live in
//! their own crates ([`mara_graph`], [`mara_code`]) so they can
//! be used independently of `mara_core`. The wrappers in this
//! module bundle each crate's widget with the [`embed`](crate::embed)
//! maximise / restore mechanism and the active mara theme — call
//! them when you want "the graph in a mara-styled pod, with the
//! fullscreen affordance" rather than the bare standalone widget.
//!
//! Feature gates:
//!
//! * `graph` (default-on) — enables [`graph`] and pulls in
//!   [`mara_graph`].
//! * `code` (default-on) — enables [`code`] and pulls in
//!   [`mara_code`].
//!
#[cfg(feature = "code")]
pub mod code;
#[cfg(feature = "graph")]
pub mod graph;
