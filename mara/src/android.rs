//! Mara-owned Android runner.
//!
//! The Android equivalent of [`crate::window`]: a winit + wgpu runner
//! that drives a [`WindowApp`] on a phone/tablet. It shares the app
//! contract with the desktop runner (see [`crate::runner`]) so the same
//! app implementation runs on either host.
//!
//! Two things differ from desktop and both are inherent to Android:
//!
//! 1. **Entry point.** Android has no `main`; the OS calls
//!    `android_main(AndroidApp)`. The `AndroidApp` handle must be
//!    threaded into the winit event loop via
//!    [`with_android_app`](winit::platform::android::EventLoopBuilderExtAndroid::with_android_app).
//!    Apps wire this up with a tiny `cdylib` entry (see the example).
//!
//! 2. **Surface lifecycle.** The GPU surface is owned by the OS and is
//!    created/destroyed as the activity foregrounds/backgrounds. winit
//!    surfaces this as `resumed()` / `suspended()`; we (re)attach the
//!    wgpu surface on resume and detach it on suspend. The egui
//!    context and the app state outlive the surface.
//!
//! There is no window chrome (the OS owns the frame) and no native
//! move/resize/close, so the host advertises
//! [`MaraWindowHost::None`] and the enforced shell drops its window
//! controls.

use std::sync::Arc;
use std::time::Instant;

use egui_wgpu::winit::Painter;
use egui_winit::egui::{self, ViewportId};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::platform::android::EventLoopBuilderExtAndroid;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::host::{MaraHostCtx, MaraWindowHost};
use mara_core::ribbon::{RibbonDrag, RibbonOpen, RibbonPlacement};
use mara_core::{ShellBar, ShellEvent};

// Re-export the shared app contract so app code can be written against
// `mara::android::*` symmetrically with `mara::window::*`. These names are
// also used directly within this module.
pub use crate::runner::{CreationContext, NativeOptions, Surface, WindowApp};
/// Re-exported so apps can spell the `android_main` parameter type
/// without depending on `winit` directly.
pub use winit::platform::android::activity::AndroidApp;

/// Run a [`WindowApp`] as an Android activity.
///
/// Call this from the `#[unsafe(no_mangle)] fn android_main(app:
/// AndroidApp)` entry point of a `cdylib`:
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// fn android_main(app: winit::platform::android::activity::AndroidApp) {
///     mara::android::run_android::<MyApp>(app);
/// }
/// ```
pub fn run_android<A>(android_app: AndroidApp)
where
    A: WindowApp,
{
    if let Err(err) = run_android_inner::<A>(android_app) {
        // There is no terminal on Android; surface the failure to logcat.
        eprintln!("mara android runner exited with error: {err}");
    }
}

fn run_android_inner<A>(android_app: AndroidApp) -> Result<(), Box<dyn std::error::Error>>
where
    A: WindowApp,
{
    let event_loop = EventLoop::<MaraUserEvent>::with_user_event()
        .with_android_app(android_app)
        .build()?;
    let mut app = AndroidWinitApp::<A>::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum MaraUserEvent {
    RequestRepaint(Instant),
}

struct AndroidWinitApp<A: WindowApp> {
    proxy: EventLoopProxy<MaraUserEvent>,
    window: Option<Arc<Window>>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    painter: Option<Painter>,
    app: Option<A>,
    next_repaint: Option<Instant>,
    // Enforced shell top bar — owned by the runner, like the desktop
    // runner. On Android it renders without window controls because the
    // host advertises no window capabilities.
    shell: ShellBar,
    shell_open: RibbonOpen,
    shell_placement: RibbonPlacement,
    shell_drag: RibbonDrag,
}

impl<A: WindowApp> AndroidWinitApp<A> {
    fn new(proxy: EventLoopProxy<MaraUserEvent>) -> Self {
        Self {
            proxy,
            window: None,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            painter: None,
            app: None,
            next_repaint: Some(Instant::now()),
            shell: ShellBar::default(),
            shell_open: RibbonOpen::default(),
            shell_placement: RibbonPlacement::default(),
            shell_drag: RibbonDrag::default(),
        }
    }

    /// (Re)create the window and attach the GPU surface. Called on every
    /// `resumed()`; the painter and the app are created once, on first
    /// resume, and outlive subsequent suspend/resume cycles.
    fn on_resume(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(WindowAttributes::default().with_title("Mara"))
                .expect("failed to create Mara Android window"),
        );

        // The repaint callback is installed once; reinstalling on every
        // resume is harmless but unnecessary, so guard on first painter.
        let first_init = self.painter.is_none();
        if first_init {
            let repaint_proxy = self.proxy.clone();
            self.egui_ctx.set_request_repaint_callback(move |info| {
                let when = Instant::now()
                    .checked_add(info.delay)
                    .unwrap_or_else(Instant::now);
                let _ = repaint_proxy.send_event(MaraUserEvent::RequestRepaint(when));
            });

            // `AutoNoVsync` for the same reason as the desktop runner:
            // present must not block the UI thread (see ARCHITECTURE §4.1).
            let wgpu_config = egui_wgpu::WgpuConfiguration {
                present_mode: wgpu::PresentMode::AutoNoVsync,
                ..egui_wgpu::WgpuConfiguration::default()
            };
            let painter = pollster::block_on(Painter::new(
                self.egui_ctx.clone(),
                wgpu_config,
                false,
                egui_wgpu::RendererOptions::default(),
            ));
            self.painter = Some(painter);
        }

        let painter = self.painter.as_mut().expect("painter initialized above");
        pollster::block_on(painter.set_window(ViewportId::ROOT, Some(window.clone())))
            .expect("failed to attach wgpu surface to Mara Android window");

        if first_init {
            let egui_state = egui_winit::State::new(
                self.egui_ctx.clone(),
                ViewportId::ROOT,
                event_loop,
                Some(window.scale_factor() as f32),
                event_loop.system_theme(),
                painter.max_texture_side(),
            );
            self.egui_state = Some(egui_state);

            let render_state = painter.render_state();
            let host =
                MaraHostCtx::new(&self.egui_ctx, render_state.as_ref(), MaraWindowHost::None);
            self.app = Some(A::new(CreationContext {
                egui_ctx: &self.egui_ctx,
                render_state: render_state.as_ref(),
                host,
            }));
        }

        self.window = Some(window);
    }

    /// Drop the GPU surface when the activity backgrounds. The painter,
    /// egui context, and app are retained for the next resume.
    fn on_suspend(&mut self) {
        if let Some(painter) = self.painter.as_mut() {
            pollster::block_on(painter.set_window(ViewportId::ROOT, None))
                .expect("failed to detach wgpu surface on suspend");
        }
        self.window = None;
    }

    fn schedule_repaint(&mut self, event_loop: &ActiveEventLoop, when: Instant) {
        self.next_repaint = Some(match self.next_repaint {
            Some(current) => current.min(when),
            None => when,
        });
        if when <= Instant::now() {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(when));
        }
    }

    fn pump_scheduled_repaint(&mut self, event_loop: &ActiveEventLoop) {
        let Some(when) = self.next_repaint else {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        };
        if when <= Instant::now() {
            self.next_repaint = None;
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(when));
        }
    }

    fn redraw(&mut self) {
        let (Some(window), Some(egui_state), Some(painter), Some(app)) = (
            self.window.as_ref(),
            self.egui_state.as_mut(),
            self.painter.as_mut(),
            self.app.as_mut(),
        ) else {
            return;
        };

        let mut viewport_info = egui::ViewportInfo::default();
        egui_winit::update_viewport_info(&mut viewport_info, &self.egui_ctx, window, false);

        let mut raw_input = egui_state.take_egui_input(window);
        raw_input.viewports.insert(ViewportId::ROOT, viewport_info);
        painter.handle_screenshots(&mut raw_input.events);

        let Some(render_state) = painter.render_state() else {
            return;
        };

        let shell = &mut self.shell;
        let shell_open = &mut self.shell_open;
        let shell_placement = &mut self.shell_placement;
        let shell_drag = &mut self.shell_drag;

        let full_output = self.egui_ctx.run_ui(raw_input, |ui| {
            let ctx = ui.ctx();
            // Android host: no native window capabilities, so the shell
            // shows no window controls. Mara still owns theme/shell.
            let mut host = MaraHostCtx::new(ctx, Some(&render_state), MaraWindowHost::None);
            host.apply_default_theme();
            host.publish_full_shelf_layout();
            app.update(&mut host);

            // Honor the explicit per-frame shell opt-out, mirroring the
            // desktop runner.
            if mara_core::enforce::__internal_shell_opted_out(ctx) {
                return;
            }
            app.configure_shell(shell);
            for event in shell.__internal_show_egui(ctx, shell_open, shell_placement, shell_drag) {
                // Close/maximize are not meaningful on Android (the OS
                // owns the activity lifecycle); forward everything else.
                match event {
                    ShellEvent::CloseRequested | ShellEvent::MaximizeToggleRequested => {}
                    other => app.on_shell_event(other, &mut host),
                }
            }
        });

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output: _,
        } = full_output;

        egui_state.handle_platform_output(window, platform_output);

        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);
        let clear_color = {
            let bg: egui::Color32 = mara_core::style::theme().palette.bg_window.into();
            egui::Rgba::from(bg).to_array()
        };
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            pixels_per_point,
            clear_color,
            &clipped_primitives,
            &textures_delta,
            Vec::new(),
        );
    }
}

impl<A: WindowApp> ApplicationHandler<MaraUserEvent> for AndroidWinitApp<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.on_resume(event_loop);
        self.schedule_repaint(event_loop, Instant::now());
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.on_suspend();
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: MaraUserEvent) {
        match event {
            MaraUserEvent::RequestRepaint(when) => self.schedule_repaint(event_loop, when),
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        if window.id() != window_id {
            return;
        }

        let mut repaint = None;
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => {
                if let (Some(painter), Some(width), Some(height)) = (
                    self.painter.as_mut(),
                    std::num::NonZeroU32::new(size.width),
                    std::num::NonZeroU32::new(size.height),
                ) {
                    painter.on_window_resized(ViewportId::ROOT, width, height);
                    repaint = Some(Instant::now());
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw();
                return;
            }
            _ => {}
        }

        if let Some(egui_state) = self.egui_state.as_mut() {
            let response = egui_state.on_window_event(window, &event);
            if response.repaint {
                repaint = Some(Instant::now());
            }
        }

        if let Some(when) = repaint {
            self.schedule_repaint(event_loop, when);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.pump_scheduled_repaint(event_loop);
    }
}
