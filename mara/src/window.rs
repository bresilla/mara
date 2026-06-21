//! Mara-owned native window mode.
//!
//! This mode is for apps that want Mara to create and own the
//! borderless native window. It wires Mara's host-neutral chrome
//! regions to winit native drag/resize commands. If another host owns
//! the window, use [`crate::ui`] instead.

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_wgpu::winit::Painter;
use egui_winit::egui::{self, ViewportCommand, ViewportId};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::{ResizeDirection, Window, WindowAttributes, WindowId};

pub use crate::host::{MaraHostCtx, MaraWindowHost};
pub use mara_core::{ShellBar, ShellEvent, ShellView};

use mara_core::ribbon::{RibbonDrag, RibbonOpen, RibbonPlacement};

/// Surface mode for the Mara-owned runner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Surface {
    /// Egui/Mara owns the shell. Optional scene/viewport widgets are
    /// just UI content inside that shell.
    Egui,
}

/// Native window options for [`AppRunner`].
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
    pub render_state: Option<&'a egui_wgpu::RenderState>,
    pub host: MaraHostCtx<'a>,
}

impl CreationContext<'_> {
    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }
}

/// App trait for the window-owning mode.
pub trait WindowApp: Sized + 'static {
    fn new(ctx: CreationContext<'_>) -> Self;
    fn update(&mut self, ctx: &mut MaraHostCtx<'_>);

    /// Configure the enforced permanent top bar for this frame.
    ///
    /// The runner renders the [`ShellBar`] itself (it is *enforced*,
    /// not opt-in), then calls this so the app can set the view
    /// switcher / active selection. Leave it empty for the default
    /// bar (app-menu + window controls); set `bar.enabled = false` to
    /// opt out entirely.
    fn configure_shell(&mut self, _bar: &mut ShellBar) {}

    /// React to a top-bar interaction the app owns (view switch, menu,
    /// shelf toggle). The runner handles the window actions
    /// (close/maximize) itself, so those never reach here.
    fn on_shell_event(&mut self, _event: ShellEvent, _ctx: &mut MaraHostCtx<'_>) {}
}

/// Builder for the Mara-owned native window.
#[derive(Debug, Clone, Default)]
pub struct AppRunner {
    options: NativeOptions,
}

impl AppRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn title(mut self, title: impl Into<String>) -> Self {
        self.options.title = title.into();
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.options.width = width;
        self.options.height = height;
        self
    }

    pub fn borderless(mut self, borderless: bool) -> Self {
        self.options.borderless = borderless;
        self
    }

    pub fn surface(mut self, surface: Surface) -> Self {
        self.options.surface = surface;
        self
    }

    pub fn options(mut self, options: NativeOptions) -> Self {
        self.options = options;
        self
    }

    pub fn run<A>(self) -> Result<(), Box<dyn std::error::Error>>
    where
        A: WindowApp,
    {
        run_native::<A>(self.options)
    }
}

pub fn run<A>() -> Result<(), Box<dyn std::error::Error>>
where
    A: WindowApp,
{
    AppRunner::new().run::<A>()
}

pub fn run_native<A>(options: NativeOptions) -> Result<(), Box<dyn std::error::Error>>
where
    A: WindowApp,
{
    let event_loop = EventLoop::<MaraUserEvent>::with_user_event().build()?;
    let mut app = NativeWinitApp::<A>::new(options, event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum MaraUserEvent {
    RequestRepaint(Instant),
}

struct NativeWinitApp<A: WindowApp> {
    options: NativeOptions,
    proxy: EventLoopProxy<MaraUserEvent>,
    window: Option<Arc<Window>>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    painter: Option<Painter>,
    app: Option<A>,
    last_cursor_pos: Option<egui::Pos2>,
    last_chrome_regions: mara_core::WindowChromeRegions,
    next_repaint: Option<Instant>,
    // Enforced shell top bar — owned by the runner, not the app, so
    // every `mara::window` app has it (the app only configures it).
    shell: ShellBar,
    shell_open: RibbonOpen,
    shell_placement: RibbonPlacement,
    shell_drag: RibbonDrag,
}

impl<A: WindowApp> NativeWinitApp<A> {
    fn new(options: NativeOptions, proxy: EventLoopProxy<MaraUserEvent>) -> Self {
        Self {
            options,
            proxy,
            window: None,
            egui_ctx: egui::Context::default(),
            egui_state: None,
            painter: None,
            app: None,
            last_cursor_pos: None,
            last_chrome_regions: mara_core::WindowChromeRegions::default(),
            next_repaint: Some(Instant::now()),
            shell: ShellBar::default(),
            shell_open: RibbonOpen::default(),
            shell_placement: RibbonPlacement::default(),
            shell_drag: RibbonDrag::default(),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title(self.options.title.clone())
            .with_inner_size(LogicalSize::new(self.options.width, self.options.height))
            .with_decorations(!self.options.borderless);
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create Mara native window"),
        );

        let repaint_proxy = self.proxy.clone();
        self.egui_ctx.set_request_repaint_callback(move |info| {
            let when = Instant::now()
                .checked_add(info.delay)
                .unwrap_or_else(Instant::now);
            let _ = repaint_proxy.send_event(MaraUserEvent::RequestRepaint(when));
        });

        // `paint_and_update_textures` acquires + presents the surface on
        // the UI thread; with the default `AutoVsync` (Fifo) present mode
        // that call BLOCKS until the next vsync, stalling the whole UI
        // (input handling, pane open/animation) behind the GPU. Use a
        // non-blocking present mode so the UI thread never waits on the
        // swapchain.
        let wgpu_config = egui_wgpu::WgpuConfiguration {
            present_mode: wgpu::PresentMode::AutoNoVsync,
            ..egui_wgpu::WgpuConfiguration::default()
        };
        let mut painter = pollster::block_on(Painter::new(
            self.egui_ctx.clone(),
            wgpu_config,
            false,
            egui_wgpu::RendererOptions::default(),
        ));
        pollster::block_on(painter.set_window(ViewportId::ROOT, Some(window.clone())))
            .expect("failed to attach wgpu surface to Mara native window");

        let egui_state = egui_winit::State::new(
            self.egui_ctx.clone(),
            ViewportId::ROOT,
            event_loop,
            Some(window.scale_factor() as f32),
            event_loop.system_theme(),
            painter.max_texture_side(),
        );

        let render_state = painter.render_state();
        let host = MaraHostCtx::mara_window(&self.egui_ctx, render_state.as_ref());
        self.app = Some(A::new(CreationContext {
            egui_ctx: &self.egui_ctx,
            render_state: render_state.as_ref(),
            host,
        }));
        self.egui_state = Some(egui_state);
        self.painter = Some(painter);
        self.window = Some(window);
    }

    fn schedule_repaint(&mut self, event_loop: &ActiveEventLoop, when: Instant) {
        self.next_repaint = Some(match self.next_repaint {
            Some(current) => current.min(when),
            None => when,
        });
        let now = Instant::now();
        if when <= now {
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

        let now = Instant::now();
        if when <= now {
            self.next_repaint = None;
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
            }
            event_loop.set_control_flow(ControlFlow::Wait);
        } else {
            event_loop.set_control_flow(ControlFlow::WaitUntil(when));
        }
    }

    fn handle_native_chrome_press(&self, button: MouseButton, state: ElementState) -> bool {
        if button != MouseButton::Left || state != ElementState::Pressed {
            return false;
        }
        let Some(window) = self.window.as_ref() else {
            return false;
        };
        let Some(pos) = self.last_cursor_pos else {
            return false;
        };
        // `last_cursor_pos` and the winit window size are in PHYSICAL
        // pixels, but the published chrome regions (drag + exclusion rects)
        // are in egui LOGICAL POINTS. The two only coincide at
        // pixels_per_point == 1.0; Ctrl+/Ctrl- changes egui's zoom factor
        // (hence `pixels_per_point`), so without this conversion the
        // exclusion rects drift out from under the cursor as you zoom and
        // a button press is mis-read as a window drag. Convert to points.
        let ppp = self.egui_ctx.pixels_per_point().max(f32::EPSILON);
        let pos = mara_core::vocab::pos2(pos.x / ppp, pos.y / ppp);
        let size = window.inner_size();
        let window_size = mara_core::vocab::vec2(size.width as f32 / ppp, size.height as f32 / ppp);
        let Some(hit) = mara_core::hit_test_window_chrome_regions(
            &self.last_chrome_regions,
            pos,
            window_size,
            mara_core::style::theme().window_chrome,
        ) else {
            return false;
        };

        match hit {
            mara_core::WindowChromeHit::Move => {
                if let Err(err) = window.drag_window() {
                    eprintln!("Mara native window drag failed: {err}");
                    return false;
                }
            }
            mara_core::WindowChromeHit::Resize(direction) => {
                if let Err(err) = window.drag_resize_window(winit_resize_direction(direction)) {
                    eprintln!("Mara native window resize failed: {err}");
                    return false;
                }
            }
        }
        true
    }

    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let (
            Some(window),
            Some(egui_state),
            Some(painter),
            Some(app),
            shell,
            shell_open,
            shell_placement,
            shell_drag,
        ) = (
            self.window.as_ref(),
            self.egui_state.as_mut(),
            self.painter.as_mut(),
            self.app.as_mut(),
            &mut self.shell,
            &mut self.shell_open,
            &mut self.shell_placement,
            &mut self.shell_drag,
        )
        else {
            return;
        };

        let mut viewport_info = egui::ViewportInfo::default();
        egui_winit::update_viewport_info(&mut viewport_info, &self.egui_ctx, window, false);

        let mut raw_input = egui_state.take_egui_input(window);
        raw_input
            .viewports
            .insert(ViewportId::ROOT, viewport_info.clone());
        painter.handle_screenshots(&mut raw_input.events);

        let Some(render_state) = painter.render_state() else {
            return;
        };

        // Layout pose probe: when `MARA_SHOW_POSE` is set, capture this
        // frame's element rects/state and dump them to the terminal
        // after the pass. Throttled so it doesn't flood the console.
        let probe_this_frame = std::env::var_os("MARA_SHOW_POSE").is_some()
            && self.egui_ctx.cumulative_pass_nr().is_multiple_of(120);
        if probe_this_frame {
            mara_core::probe::__internal_set_enabled(&self.egui_ctx, true);
        }

        // Frame timing is opt-in (MARA_FRAME_TIME). Only SLOW frames are
        // printed (see threshold below) so interaction-triggered spikes
        // stand out instead of being buried in steady-state noise.
        let frame_timing = std::env::var_os("MARA_FRAME_TIME").is_some();
        let frame_t0 = std::time::Instant::now();

        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            let mut host = MaraHostCtx::mara_window(ctx, Some(&render_state));
            app.update(&mut host);

            // Enforced permanent top bar — rendered by the runner, not
            // the app. The app only configures it (views/active) and
            // reacts to app-level events; the runner owns the window
            // actions. Drawn after the app body so it reads the live
            // theme/capabilities/shelf layout the app published.
            app.configure_shell(shell);
            for event in shell.show(ctx, shell_open, shell_placement, shell_drag) {
                match event {
                    ShellEvent::CloseRequested => {
                        ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                    ShellEvent::MaximizeToggleRequested => {
                        let maximized = ctx.input(|i| i.viewport().maximized).unwrap_or(false);
                        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
                    }
                    other => app.on_shell_event(other, &mut host),
                }
            }
        });

        if probe_this_frame {
            let poses = mara_core::probe::__internal_drain(&self.egui_ctx);
            eprintln!("{}", mara_core::probe::format(&poses));
            mara_core::probe::__internal_set_enabled(&self.egui_ctx, false);
        }

        self.last_chrome_regions =
            mara_core::window_chrome::__internal_window_chrome_regions(&self.egui_ctx);

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = full_output;

        egui_state.handle_platform_output(window, platform_output);

        if let Some(output) = viewport_output.get(&ViewportId::ROOT) {
            if output.commands.contains(&ViewportCommand::Close) {
                event_loop.exit();
                return;
            }
            // Honor the maximize/restore window control: apply the last
            // Maximized command of the frame to the winit window.
            if let Some(maximized) = output.commands.iter().rev().find_map(|cmd| match cmd {
                ViewportCommand::Maximized(value) => Some(*value),
                _ => None,
            }) {
                window.set_maximized(maximized);
            }
        }

        let run_ms = frame_t0.elapsed().as_secs_f32() * 1000.0;
        let tess_t0 = std::time::Instant::now();
        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);
        let tess_ms = tess_t0.elapsed().as_secs_f32() * 1000.0;
        let tex_set = textures_delta.set.len();
        let tex_free = textures_delta.free.len();
        let paint_t0 = std::time::Instant::now();
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            pixels_per_point,
            [0.06, 0.08, 0.12, 1.0],
            &clipped_primitives,
            &textures_delta,
            Vec::new(),
        );
        if frame_timing {
            let paint_ms = paint_t0.elapsed().as_secs_f32() * 1000.0;
            let total = run_ms + tess_ms + paint_ms;
            // Only report laggy frames so interaction spikes are obvious.
            if total > 10.0 {
                eprintln!(
                    "MARA_FRAME[SLOW]: total={total:.1}ms | ui_run={run_ms:.1} tessellate={tess_ms:.1} gpu_paint={paint_ms:.1} | shapes={} tex_uploads={tex_set} tex_frees={tex_free}",
                    clipped_primitives.len()
                );
            }
        }
    }
}

impl<A: WindowApp> ApplicationHandler<MaraUserEvent> for NativeWinitApp<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop);
        self.schedule_repaint(event_loop, Instant::now());
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

        let mut repaint_after_event = None;
        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_pos = Some(egui::pos2(position.x as f32, position.y as f32));
                repaint_after_event = Some(Instant::now());
            }
            WindowEvent::MouseInput { button, state, .. } => {
                if self.handle_native_chrome_press(*button, *state) {
                    return;
                }
            }
            WindowEvent::Resized(size) => {
                if let (Some(painter), Some(width), Some(height)) = (
                    self.painter.as_mut(),
                    NonZeroU32::new(size.width),
                    NonZeroU32::new(size.height),
                ) {
                    painter.on_window_resized(ViewportId::ROOT, width, height);
                    repaint_after_event = Some(Instant::now() + Duration::from_millis(16));
                }
            }
            WindowEvent::RedrawRequested => {
                self.redraw(event_loop);
                return;
            }
            _ => {}
        }

        if let Some(egui_state) = self.egui_state.as_mut() {
            let response = egui_state.on_window_event(window, &event);
            if response.repaint {
                repaint_after_event = Some(Instant::now());
            }
        }

        if let Some(when) = repaint_after_event {
            self.schedule_repaint(event_loop, when);
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.pump_scheduled_repaint(event_loop);
    }
}

fn winit_resize_direction(direction: mara_core::WindowResizeDirection) -> ResizeDirection {
    match direction {
        mara_core::WindowResizeDirection::North => ResizeDirection::North,
        mara_core::WindowResizeDirection::NorthEast => ResizeDirection::NorthEast,
        mara_core::WindowResizeDirection::East => ResizeDirection::East,
        mara_core::WindowResizeDirection::SouthEast => ResizeDirection::SouthEast,
        mara_core::WindowResizeDirection::South => ResizeDirection::South,
        mara_core::WindowResizeDirection::SouthWest => ResizeDirection::SouthWest,
        mara_core::WindowResizeDirection::West => ResizeDirection::West,
        mara_core::WindowResizeDirection::NorthWest => ResizeDirection::NorthWest,
    }
}
