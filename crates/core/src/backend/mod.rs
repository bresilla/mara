//! Backend adapters for Mara runtime vocabulary.
//!
//! These modules are deliberately internal. Public app code speaks
//! Mara data (`vocab`, `PaintCmd`, `MaraResponse`, ...); backend code
//! translates that data into the concrete engine currently in use.

pub(crate) mod egui;
