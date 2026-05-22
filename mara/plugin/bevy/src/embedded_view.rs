//! Legacy re-export of the embedded Bevy viewport module.
//!
//! The actual egui-owned/windowless Bevy viewport now lives in the
//! core module crate `mara_bevy` and is exposed from the unified API
//! as `mara::ui::modules::bevy`. The Bevy plugin crate keeps this
//! re-export temporarily so existing `bevy_mara::*` users do not
//! break while the remaining Bevy-owned-app plugin glue is migrated.

pub use mara_bevy::*;
