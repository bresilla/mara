//! Tiny native Bevy scene/viewport wrapper used by the root example.
//!
//! The implementation lives in the unified `mara` crate as an
//! embedded Bevy view/module.

pub type ExampleBevyScene = mara::ui::modules::bevy::BevyEmbeddedView;
