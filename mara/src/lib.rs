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

pub mod extras;
pub mod host;

pub mod ui;

// Shared app-facing contract for the window-owning runners (desktop and
// Android). Both `window` and `android` re-export the relevant types, so
// this stays internal.
#[cfg(all(
    any(feature = "window", feature = "android"),
    not(target_arch = "wasm32")
))]
mod runner;

#[cfg(all(feature = "window", not(target_arch = "wasm32")))]
pub mod window;

// `android` is the OS-driven sibling of `window`: same `WindowApp`
// contract, Android event loop + surface lifecycle. Only compiled when
// actually targeting Android.
#[cfg(all(feature = "android", target_os = "android"))]
pub mod android;

pub mod prelude {
    pub use crate::host::{
        MaraHostCtx, MaraWindowHost, RibbonActionButton, RibbonPane, RibbonRail,
    };
    pub use crate::ui::prelude::*;

    #[cfg(all(feature = "window", not(target_arch = "wasm32")))]
    pub use crate::window::{
        AppRunner, CreationContext, NativeOptions, Surface, WindowApp, run, run_native,
    };

    // On Android the app contract comes through the `android` module;
    // expose the same vocabulary so app code is host-agnostic.
    #[cfg(all(feature = "android", target_os = "android"))]
    pub use crate::android::run_android;
    #[cfg(all(feature = "android", target_os = "android", not(feature = "window")))]
    pub use crate::runner::{CreationContext, NativeOptions, Surface, WindowApp};
}
