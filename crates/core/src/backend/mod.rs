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

/// The egui adapter, gated on the same switch as the vocab
/// conversions it depends on. Turning `backend-egui-conv` off must
/// remove *both* — a backend without its conversions cannot compile,
/// and conversions without a backend have nothing to convert to.
/// Together they are the WS-G1 split criterion: what is left when this
/// is off is the backend-free crate.
#[cfg(feature = "backend-egui-conv")]
#[doc(hidden)]
pub mod egui;
pub mod record;
