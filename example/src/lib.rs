//! Root Mara example application.
//!
//! This crate consumes `mara`, `mara_core`, and standalone Mara modules
//! the same way downstream applications should.

pub mod app;
pub mod bevy_content;

pub use app::DemoApp;
