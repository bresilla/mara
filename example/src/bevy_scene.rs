//! Tiny native Bevy scene/viewport wrapper used by the root example.
//!
//! The implementation lives in `bevy_mara` so external apps can
//! consume the same eframe-owned Bevy viewport bridge. This module is
//! only the example-facing name from `PLAN.md`.

pub type ExampleBevyScene = bevy_mara::BevyEmbeddedView;
