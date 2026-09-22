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
/// owns the surface fullscreen, so it is ignored there. `position` is
/// desktop-only too, and best-effort — some window managers ignore an
/// app's requested initial position.
#[derive(Debug, Clone)]
pub struct NativeOptions {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub position: Option<(f32, f32)>,
    pub borderless: bool,
    pub surface: Surface,
}

impl Default for NativeOptions {
    fn default() -> Self {
        Self {
            title: "Mara".to_owned(),
            width: 1440.0,
            height: 920.0,
            position: None,
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
    /// Internal first-party accessor — raw egui-wgpu render state.
    /// Sealed apps use `host.gpu()` for the opaque handle instead.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_render_state(&self) -> Option<&egui_wgpu::RenderState> {
        self.render_state
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable.
    #[doc(hidden)]
    #[must_use]
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

    /// Adjust the shared GPU device limits before device creation.
    /// Requests must retain renderer requirements and fit the adapter limits.
    fn configure_gpu_limits(_supported: &wgpu::Limits, _requested: &mut wgpu::Limits) {}

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

    /// The native window was resized (desktop only; never fires on
    /// Android). Logical size — the same units `NativeOptions::size`
    /// takes, so a value saved here can be handed straight back on the
    /// next launch with no scale-factor conversion.
    fn on_window_resized(&mut self, _width: f32, _height: f32) {}

    /// The native window moved (desktop only; never fires on Android).
    /// Physical pixel position, matching `NativeOptions::position` and
    /// winit's own `WindowEvent::Moved`.
    fn on_window_moved(&mut self, _x: f32, _y: f32) {}
}

pub(crate) fn app_gpu_configuration<A: WindowApp>(
    mut config: egui_wgpu::WgpuConfiguration,
) -> egui_wgpu::WgpuConfiguration {
    if let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup {
        let descriptor = setup.device_descriptor.clone();
        setup.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut device = descriptor(adapter);
            A::configure_gpu_limits(&adapter.limits(), &mut device.required_limits);
            device
        });
    }
    config
}

#[cfg(test)]
mod gpu_configuration_tests {
    use super::*;

    struct ComputeApp;

    impl WindowApp for ComputeApp {
        fn new(_: CreationContext<'_>) -> Self {
            Self
        }
        fn update(&mut self, _: &mut MaraHostCtx<'_>) {}
        fn configure_gpu_limits(supported: &wgpu::Limits, requested: &mut wgpu::Limits) {
            requested.max_bind_groups = requested
                .max_bind_groups
                .max(supported.max_bind_groups.min(6));
        }
    }

    #[test]
    #[ignore = "requires a real GPU"]
    fn app_limits_extend_the_existing_device_descriptor() {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut config = egui_wgpu::WgpuConfiguration::default();
        config.present_mode = wgpu::PresentMode::AutoNoVsync;
        let egui_wgpu::WgpuSetup::CreateNew(setup) = &mut config.wgpu_setup else {
            panic!("expected new device");
        };
        let original = setup.device_descriptor.clone();
        setup.device_descriptor = std::sync::Arc::new(move |adapter| {
            let mut descriptor = original(adapter);
            descriptor.label = Some("preserved renderer descriptor");
            descriptor.memory_hints = wgpu::MemoryHints::MemoryUsage;
            descriptor
        });
        let mut expected = (setup.device_descriptor)(&adapter);
        ComputeApp::configure_gpu_limits(&adapter.limits(), &mut expected.required_limits);
        let config = app_gpu_configuration::<ComputeApp>(config);
        assert_eq!(config.present_mode, wgpu::PresentMode::AutoNoVsync);
        let egui_wgpu::WgpuSetup::CreateNew(setup) = config.wgpu_setup else {
            panic!("expected new device");
        };
        let actual = (setup.device_descriptor)(&adapter);
        assert_eq!(actual.required_limits, expected.required_limits);
        assert_eq!(actual.required_features, expected.required_features);
        assert_eq!(actual.label, expected.label);
        assert!(matches!(actual.memory_hints, wgpu::MemoryHints::MemoryUsage));
        let (device, _) = pollster::block_on(adapter.request_device(&actual)).unwrap();
        assert!(device.limits().max_bind_groups >= adapter.limits().max_bind_groups.min(6));
    }
}
