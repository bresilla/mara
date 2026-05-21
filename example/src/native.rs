use std::num::NonZeroU32;
use std::sync::Arc;

use egui_wgpu::winit::Painter;
use egui_winit::egui::{self, ViewportCommand, ViewportId};
use mara_example::DemoApp;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{ResizeDirection, Window, WindowAttributes, WindowId};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = EventLoop::new()?;
    let mut app = NativeWinitApp::default();
    event_loop.run_app(&mut app)?;
    Ok(())
}

#[derive(Default)]
struct NativeWinitApp {
    window: Option<Arc<Window>>,
    egui_ctx: egui::Context,
    egui_state: Option<egui_winit::State>,
    painter: Option<Painter>,
    demo: Option<DemoApp>,
    last_cursor_pos: Option<egui::Pos2>,
    last_chrome_regions: mara_core::WindowChromeRegions,
}

impl NativeWinitApp {
    fn create_window(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let attrs = WindowAttributes::default()
            .with_title("Mara example")
            .with_inner_size(LogicalSize::new(1440.0, 920.0))
            .with_decorations(false);
        let window = Arc::new(
            event_loop
                .create_window(attrs)
                .expect("failed to create Mara native winit window"),
        );

        let mut painter = pollster::block_on(Painter::new(
            self.egui_ctx.clone(),
            egui_wgpu::WgpuConfiguration::default(),
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
        self.demo = Some(DemoApp::new_winit(render_state.as_ref()));
        self.egui_state = Some(egui_state);
        self.painter = Some(painter);
        self.window = Some(window);
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
        let size = window.inner_size();
        let window_size = egui::vec2(size.width as f32, size.height as f32);
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
        let (Some(window), Some(egui_state), Some(painter), Some(demo)) = (
            self.window.as_ref(),
            self.egui_state.as_mut(),
            self.painter.as_mut(),
            self.demo.as_mut(),
        ) else {
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
        let full_output = self.egui_ctx.run(raw_input, |ctx| {
            demo.update_with_render_state(ctx, &render_state);
        });

        self.last_chrome_regions = mara_core::window_chrome_regions(&self.egui_ctx);

        let egui::FullOutput {
            platform_output,
            textures_delta,
            shapes,
            pixels_per_point,
            viewport_output,
        } = full_output;

        egui_state.handle_platform_output(window, platform_output);

        if viewport_output
            .get(&ViewportId::ROOT)
            .is_some_and(|output| output.commands.contains(&ViewportCommand::Close))
        {
            event_loop.exit();
            return;
        }

        let clipped_primitives = self.egui_ctx.tessellate(shapes, pixels_per_point);
        painter.paint_and_update_textures(
            ViewportId::ROOT,
            pixels_per_point,
            [0.06, 0.08, 0.12, 1.0],
            &clipped_primitives,
            &textures_delta,
            Vec::new(),
        );

        window.request_redraw();
    }
}

impl ApplicationHandler for NativeWinitApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.create_window(event_loop);
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

        match &event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.last_cursor_pos = Some(egui::pos2(position.x as f32, position.y as f32));
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
                window.request_redraw();
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
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
