//! Backend adapters for Mara runtime vocabulary.
//!
//! These modules are deliberately internal. Public app code speaks
//! Mara data (`vocab`, `PaintCmd`, `MaraResponse`, ...); backend code
//! translates that data into the concrete engine currently in use.
//!
//! `egui` is `#[doc(hidden)] pub` rather than `pub(crate)` because the
//! first-party facade (`mara::extras`) hosts the adapters for the
//! vendored widget crates and must reach the same seam. It is hidden
//! from docs and carries no stability promise — the ratchet counts its
//! callers, and `make check` bans app code from naming it.

#[doc(hidden)]
pub mod egui;
pub mod record;
