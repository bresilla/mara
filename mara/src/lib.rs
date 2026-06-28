//! Unified public Mara API.
//!
//! There are only two public use modes:
//!
//! - [`ui`] — UI-only widgets/views/layout/theme/modules. This does
//!   not own a window and is what hosts such as Bevy should consume.
//! - [`window`] — optional native window owner, enabled by the
//!   `window` feature. This adds the borderless window, top-bar drag,
//!   resize, close handling, and the wgpu render state needed by
//!   offscreen Mara widgets.

pub mod host;

pub mod ui;

#[cfg(all(feature = "window", not(target_arch = "wasm32")))]
pub mod window;

pub mod prelude {
    pub use crate::host::{
        MaraHostCtx, MaraWindowHost, RibbonActionButton, RibbonPane, RibbonRail,
    };
    pub use crate::ui::prelude::*;

    #[cfg(all(feature = "window", not(target_arch = "wasm32")))]
    pub use crate::window::{
        AppRunner, CreationContext, NativeOptions, Surface, WindowApp, run, run_native,
    };
}
