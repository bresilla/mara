//! Shared contract for Mara's window-owning runners.
//!
//! Both the desktop runner ([`crate::window`]) and the Android runner
//! ([`crate::android`]) drive the same app trait. The platform-specific
//! event loop, surface lifecycle, and window chrome live in those
//! modules; the app-facing types they have in common live here so a
//! single app implementation runs on either host unchanged.

use egui_winit::egui;

pub use crate::host::MaraHostCtx;
pub use mara_core::{ShellBar, ShellEvent};

/// Surface mode for a Mara-owned runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Egui/Mara owns the shell. Optional scene/viewport widgets are
    /// just UI content inside that shell.
    Egui,
}

/// Window options for a Mara-owned runner.
///
/// `borderless` is honored by the desktop runner; on Android the OS
/// owns the surface fullscreen, so it is ignored there.
#[derive(Debug, Clone)]
pub struct NativeOptions {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub borderless: bool,
    pub surface: Surface,
}

impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            title: "Mara".to_owned(),
            width: 1440.0,
            height: 920.0,
            borderless: true,
            surface: Surface::Egui,
        }
    }
}

/// Creation data passed to a Mara-owned window app.
pub struct CreationContext<'a> {
    pub(crate) egui_ctx: &'a egui::Context,
    pub(crate) render_state: Option<&'a egui_wgpu::RenderState>,
    pub host: MaraHostCtx<'a>,
}

impl CreationContext<'_> {
    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable.
    #[doc(hidden)]
    #[must_use]
    /// Internal first-party accessor — raw egui-wgpu render state.
    /// Sealed apps use `host.gpu()` for the opaque handle instead.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_render_state(&self) -> Option<&egui_wgpu::RenderState> {
        self.render_state
    }

    pub fn __internal_egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }
}

/// App trait for the window-owning modes (desktop and Android).
///
/// The same implementation runs on either host: the runner owns the
/// event loop, surface, and (on desktop) window chrome, and calls these
/// hooks each frame.
pub trait WindowApp: Sized + 'static {
    fn new(ctx: CreationContext<'_>) -> Self;
    fn update(&mut self, ctx: &mut MaraHostCtx<'_>);

    /// Configure the enforced permanent top bar for this frame.
    ///
    /// The runner renders the [`ShellBar`] itself (it is *enforced*,
    /// not opt-in), then calls this so the app can set the view
    /// switcher / active selection. Leave it empty for the default
    /// bar (app-menu + window controls). There is no disable flag —
    /// if nothing renders the bar, `mara_core::enforce` draws a
    /// fallback. The single deliberate escape hatch is calling
    /// `MaraHostCtx::opt_out_shell_bar()` in `update` — a per-frame
    /// decision the runner honors for that frame only.
    fn configure_shell(&mut self, _bar: &mut ShellBar) {}

    /// React to a top-bar interaction the app owns (view switch, menu,
    /// shelf toggle). The runner handles the window actions
    /// (close/maximize) itself, so those never reach here.
    fn on_shell_event(&mut self, _event: ShellEvent, _ctx: &mut MaraHostCtx<'_>) {}
}
