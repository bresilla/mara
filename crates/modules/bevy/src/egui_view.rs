//! Mara/egui-hosted Bevy viewport widget.
//!
//! This is the host-facing view wrapper around [`BevyEmbeddedView`]:
//! it owns the egui allocation, interaction forwarding, resize
//! throttling, texture upload, and warmup/fallback painting. Example
//! apps should only hold this state and place it in their content
//! tree; the viewport mechanics live here in the module crate.

use std::time::Duration;

use egui::{self, TextureHandle};

use crate::{BevyEmbeddedView, BevyViewportInput, BevyViewportWgpuResources};

/// Egui/Mara-hosted Bevy viewport.
///
/// The host still owns the top-level window. This widget reserves an
/// egui region, asks the embedded Bevy bridge to render into an
/// offscreen target, uploads the latest RGBA frame as an egui texture,
/// and forwards pointer/scroll interaction into the Bevy camera.
#[derive(Default)]
pub struct MaraBevyViewport {
    #[cfg(not(target_arch = "wasm32"))]
    bevy: BevyEmbeddedView,
    #[cfg(not(target_arch = "wasm32"))]
    texture: Option<TextureHandle>,
    #[cfg(not(target_arch = "wasm32"))]
    last_pixels: [u32; 2],
    #[cfg(not(target_arch = "wasm32"))]
    target_pixels: [u32; 2],
    #[cfg(not(target_arch = "wasm32"))]
    target_pixels_since: f64,
    #[cfg(not(target_arch = "wasm32"))]
    last_render_time: f64,
    #[cfg(not(target_arch = "wasm32"))]
    last_pointer_pos: Option<egui::Pos2>,
    #[cfg(not(target_arch = "wasm32"))]
    primary_drag_active: bool,
    #[cfg(target_arch = "wasm32")]
    ticks: u64,
}

/// Backwards-friendly name for apps that think of this as the
/// embedded Bevy viewport.
pub type EmbeddedBevyViewport = MaraBevyViewport;

impl MaraBevyViewport {
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                bevy: BevyEmbeddedView::new(),
                texture: None,
                last_pixels: [0, 0],
                target_pixels: [0, 0],
                target_pixels_since: f64::NEG_INFINITY,
                last_render_time: f64::NEG_INFINITY,
                last_pointer_pos: None,
                primary_drag_active: false,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self { ticks: 0 }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_content(
        configure_app: impl Fn(&mut bevy::prelude::App) + Send + Sync + 'static,
    ) -> Self {
        Self {
            bevy: BevyEmbeddedView::with_app_config(configure_app),
            texture: None,
            last_pixels: [0, 0],
            target_pixels: [0, 0],
            target_pixels_since: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_render_state(render_state: Option<&egui_wgpu::RenderState>) -> Self {
        let bevy = render_state
            .map(|render_state| {
                BevyEmbeddedView::with_wgpu_resources(BevyViewportWgpuResources::new(
                    render_state.device.clone(),
                    render_state.queue.clone(),
                    render_state.adapter.clone(),
                ))
            })
            .unwrap_or_default();
        Self {
            bevy,
            texture: None,
            last_pixels: [0, 0],
            target_pixels: [0, 0],
            target_pixels_since: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_render_state_and_content(
        render_state: Option<&egui_wgpu::RenderState>,
        configure_app: impl Fn(&mut bevy::prelude::App) + Send + Sync + 'static,
    ) -> Self {
        let bevy = if let Some(render_state) = render_state {
            BevyEmbeddedView::with_wgpu_resources_and_app_config(
                BevyViewportWgpuResources::new(
                    render_state.device.clone(),
                    render_state.queue.clone(),
                    render_state.adapter.clone(),
                ),
                configure_app,
            )
        } else {
            BevyEmbeddedView::with_app_config(configure_app)
        };
        Self {
            bevy,
            texture: None,
            last_pixels: [0, 0],
            target_pixels: [0, 0],
            target_pixels_since: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
        }
    }
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        #[cfg_attr(target_arch = "wasm32", allow(unused_variables))] render_state: Option<
            &egui_wgpu::RenderState,
        >,
        accent: egui::Color32,
    ) -> Option<egui::Color32> {
        #[cfg(target_arch = "wasm32")]
        {
            self.ticks = self.ticks.saturating_add(1);
        }

        let mut picked_color = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
            .show(ctx, |ui| {
                let rect = ui.max_rect();
                let painter = ui.painter_at(rect);
                let theme = mara_core::style::theme();
                painter.rect_filled(rect, 0.0, theme.palette.bg_panel);

                #[cfg(not(target_arch = "wasm32"))]
                {
                    if let Some(render_state) = render_state {
                        self.bevy
                            .attach_wgpu_resources(BevyViewportWgpuResources::new(
                                render_state.device.clone(),
                                render_state.queue.clone(),
                                render_state.adapter.clone(),
                            ));
                    }

                    let response = ui.interact(
                        rect,
                        egui::Id::new("mara_embedded_bevy_viewport_interact"),
                        egui::Sense::click_and_drag(),
                    );
                    let ppp = ui.ctx().pixels_per_point();
                    let now = ui.ctx().input(|i| i.time);
                    let desired_pixels = internal_render_pixels(rect.size(), ppp);
                    if self.target_pixels != desired_pixels {
                        self.target_pixels = desired_pixels;
                        self.target_pixels_since = now;
                    }

                    const RESIZE_SETTLE_SECONDS: f64 = 0.12;
                    let has_committed_size = self.last_pixels != [0, 0];
                    let resize_pending =
                        has_committed_size && self.target_pixels != self.last_pixels;
                    let resize_settled = now - self.target_pixels_since >= RESIZE_SETTLE_SECONDS;
                    let commit_resize = !has_committed_size || resize_pending && resize_settled;
                    let pixels = if commit_resize {
                        self.target_pixels
                    } else {
                        self.last_pixels
                    };
                    let render_scale = egui::vec2(
                        pixels[0] as f32 / rect.width().max(1.0),
                        pixels[1] as f32 / rect.height().max(1.0),
                    );

                    let pointer_pos = response
                        .hover_pos()
                        .or_else(|| response.interact_pointer_pos())
                        .or_else(|| ui.ctx().input(|i| i.pointer.interact_pos()));
                    let pointer_inside = pointer_pos.is_some_and(|pos| rect.contains(pos));
                    let (primary_down, primary_pressed, scroll_delta) = ui.ctx().input(|i| {
                        (
                            i.pointer.primary_down(),
                            i.pointer.button_pressed(egui::PointerButton::Primary),
                            if response.hovered() {
                                (i.smooth_scroll_delta.y + i.raw_scroll_delta.y) / 120.0
                            } else {
                                0.0
                            },
                        )
                    });
                    let primary_dragged =
                        primary_down && (self.primary_drag_active || pointer_inside);
                    let drag_delta = if primary_dragged {
                        self.primary_drag_active = true;
                        if let Some(pos) = pointer_pos {
                            let delta = self
                                .last_pointer_pos
                                .map(|last| pos - last)
                                .unwrap_or_default()
                                * render_scale;
                            self.last_pointer_pos = Some(pos);
                            [delta.x, delta.y]
                        } else {
                            [0.0, 0.0]
                        }
                    } else {
                        if !primary_down {
                            self.primary_drag_active = false;
                            self.last_pointer_pos = None;
                        }
                        [0.0, 0.0]
                    };
                    let primary_clicked = primary_pressed && pointer_inside;

                    let viewport_input = BevyViewportInput {
                        pointer_pos: pointer_pos.map(|pos| {
                            [
                                ((pos.x - rect.left()) * render_scale.x)
                                    .clamp(0.0, pixels[0] as f32),
                                ((pos.y - rect.top()) * render_scale.y)
                                    .clamp(0.0, pixels[1] as f32),
                            ]
                        }),
                        drag_delta,
                        scroll_delta,
                        primary_clicked,
                    };

                    let input_active = primary_dragged
                        || primary_clicked
                        || response.hovered() && viewport_input.scroll_delta.abs() > f32::EPSILON
                        || response.hovered() && ui.ctx().input(|i| i.pointer.any_down());
                    let texture_needs_committed_frame =
                        self.texture.as_ref().is_some_and(|texture| {
                            texture.size() != [pixels[0] as usize, pixels[1] as usize]
                        });
                    let idle_interval = 1.0 / 24.0;
                    let active_interval = 1.0 / 60.0;
                    let target_interval = if input_active
                        || commit_resize
                        || texture_needs_committed_frame
                        || self.texture.is_none()
                    {
                        active_interval
                    } else {
                        idle_interval
                    };
                    let elapsed = now - self.last_render_time;
                    let should_render = commit_resize
                        || self.texture.is_none()
                        || texture_needs_committed_frame
                        || input_active
                        || elapsed >= target_interval;

                    if should_render {
                        self.last_pixels = pixels;
                        self.last_render_time = now;
                        let dt = ui.ctx().input(|i| i.stable_dt);
                        if let Some(frame) = self.bevy.render_frame_with_input(
                            pixels[0],
                            pixels[1],
                            dt,
                            viewport_input,
                        ) {
                            let size = [frame.width as usize, frame.height as usize];
                            if size == [pixels[0] as usize, pixels[1] as usize] {
                                let image =
                                    egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
                                match &mut self.texture {
                                    Some(texture) if texture.size() == size => {
                                        texture.set(image, egui::TextureOptions::LINEAR);
                                    }
                                    _ => {
                                        self.texture = Some(ui.ctx().load_texture(
                                            "mara_embedded_bevy_viewport",
                                            image,
                                            egui::TextureOptions::LINEAR,
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    if let Some(texture) = &self.texture {
                        painter.image(
                            texture.id(),
                            rect,
                            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    } else {
                        self.paint_warmup(ui, rect, accent);
                    }

                    picked_color = self.bevy.picked_color();
                    let mut next = if should_render {
                        target_interval
                    } else {
                        (target_interval - elapsed).max(active_interval)
                    };
                    if resize_pending && !resize_settled {
                        next = next.min(
                            (RESIZE_SETTLE_SECONDS - (now - self.target_pixels_since)).max(0.0),
                        );
                    }
                    ui.ctx()
                        .request_repaint_after(Duration::from_secs_f64(next));
                    return;
                }

                #[cfg(target_arch = "wasm32")]
                self.paint_web_placeholder(ui, rect, accent);
            });
        picked_color
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn paint_warmup(&self, ui: &egui::Ui, rect: egui::Rect, accent: egui::Color32) {
        let painter = ui.painter_at(rect);
        let grid = 36.0;
        let grid_col =
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 28);
        let mut x = rect.left();
        while x < rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_col),
            );
            x += grid;
        }
        let mut y = rect.top();
        while y < rect.bottom() {
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_col),
            );
            y += grid;
        }
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            if self.bevy.renderer_failed() {
                "embedded Bevy renderer unavailable: no native wgpu adapter"
            } else {
                "warming up embedded Bevy renderer…"
            },
            egui::FontId::proportional(13.0),
            mara_core::style::on_panel(),
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn paint_web_placeholder(&self, ui: &egui::Ui, rect: egui::Rect, accent: egui::Color32) {
        let painter = ui.painter_at(rect);
        let grid = 36.0;
        let grid_col =
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 28);
        let mut x = rect.left();
        while x < rect.right() {
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0, grid_col),
            );
            x += grid;
        }
        let mut y = rect.top();
        while y < rect.bottom() {
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(1.0, grid_col),
            );
            y += grid;
        }

        let center = rect.center();
        let pulse = self.phase().sin() * 0.5 + 0.5;
        let yaw = self.rotation_angle();
        let cube = egui::Rect::from_center_size(
            center,
            egui::vec2(150.0 + 26.0 * pulse, 110.0 + 18.0 * pulse),
        );
        let skew = yaw.sin() * 44.0;
        let body = vec![
            egui::pos2(cube.left() + skew, cube.top()),
            egui::pos2(cube.right() + skew, cube.top() + 22.0),
            egui::pos2(cube.right() - skew, cube.bottom()),
            egui::pos2(cube.left() - skew, cube.bottom() - 22.0),
        ];
        painter.add(egui::Shape::convex_polygon(
            body,
            egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 90),
            egui::Stroke::new(1.5, accent),
        ));

        painter.text(
            center + egui::vec2(0.0, cube.height() * 0.5 + 28.0),
            egui::Align2::CENTER_CENTER,
            self.status_text(),
            egui::FontId::proportional(13.0),
            mara_core::style::on_panel(),
        );
        painter.text(
            center + egui::vec2(0.0, cube.height() * 0.5 + 50.0),
            egui::Align2::CENTER_CENTER,
            "egui/eframe owns this window; Bevy is embedded as viewport state.",
            egui::FontId::proportional(12.0),
            mara_core::style::on_panel_dim(),
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn phase(&self) -> f32 {
        self.ticks as f32 / 60.0
    }

    #[cfg(target_arch = "wasm32")]
    fn rotation_angle(&self) -> f32 {
        self.phase() * 0.75
    }

    #[cfg(target_arch = "wasm32")]
    fn status_text(&self) -> String {
        format!(
            "Web placeholder for embedded Bevy viewport · frame {}",
            self.ticks
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn internal_render_pixels(size: egui::Vec2, pixels_per_point: f32) -> [u32; 2] {
    const MAX_WIDTH: f32 = 1600.0;
    const MAX_HEIGHT: f32 = 1000.0;
    const MAX_PIXELS: f32 = 1_200_000.0;

    let mut width = (size.x * pixels_per_point).round().max(1.0);
    let mut height = (size.y * pixels_per_point).round().max(1.0);

    let scale = (MAX_WIDTH / width)
        .min(MAX_HEIGHT / height)
        .min((MAX_PIXELS / (width * height)).sqrt())
        .min(1.0);
    width = (width * scale).round().max(1.0);
    height = (height * scale).round().max(1.0);

    [width as u32, height as u32]
}
