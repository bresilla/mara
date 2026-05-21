//! Embedded Bevy viewport view.
//!
//! The root example is egui-owned. This view reserves a viewport
//! surface inside Mara and, on native builds, asks `bevy_mara` to
//! render a tiny windowless Bevy scene into an offscreen target. The
//! resulting RGBA frame is uploaded as an egui texture and displayed
//! inside the Mara view.

use eframe::egui;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use crate::bevy_scene::ExampleBevyScene;
#[cfg(not(target_arch = "wasm32"))]
use bevy_mara::{BevyViewportInput, BevyViewportWgpuResources};

#[derive(Default)]
pub struct EmbeddedBevyViewport {
    #[cfg(not(target_arch = "wasm32"))]
    bevy: ExampleBevyScene,
    #[cfg(not(target_arch = "wasm32"))]
    texture: Option<egui::TextureHandle>,
    #[cfg(target_arch = "wasm32")]
    ticks: u64,
}

impl EmbeddedBevyViewport {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let bevy = _cc
                .wgpu_render_state
                .as_ref()
                .map(|render_state| {
                    ExampleBevyScene::with_wgpu_resources(BevyViewportWgpuResources::new(
                        render_state.device.clone(),
                        render_state.queue.clone(),
                        render_state.adapter.clone(),
                    ))
                })
                .unwrap_or_default();
            Self {
                bevy,
                texture: None,
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            Self { ticks: 0 }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_render_state(render_state: Option<&egui_wgpu::RenderState>) -> Self {
        let bevy = render_state
            .map(|render_state| {
                ExampleBevyScene::with_wgpu_resources(BevyViewportWgpuResources::new(
                    render_state.device.clone(),
                    render_state.queue.clone(),
                    render_state.adapter.clone(),
                ))
            })
            .unwrap_or_default();
        Self {
            bevy,
            texture: None,
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, accent: egui::Color32) -> Option<egui::Color32> {
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
                    let response = ui.interact(
                        rect,
                        egui::Id::new("mara_embedded_bevy_viewport_interact"),
                        egui::Sense::click_and_drag(),
                    );
                    let ppp = ui.ctx().pixels_per_point();
                    let pixels = [
                        (rect.width() * ppp).round().max(1.0) as u32,
                        (rect.height() * ppp).round().max(1.0) as u32,
                    ];
                    let pointer_pos = response
                        .hover_pos()
                        .or_else(|| response.interact_pointer_pos());
                    let viewport_input = BevyViewportInput {
                        pointer_pos: pointer_pos.map(|pos| {
                            [
                                ((pos.x - rect.left()) * ppp).clamp(0.0, pixels[0] as f32),
                                ((pos.y - rect.top()) * ppp).clamp(0.0, pixels[1] as f32),
                            ]
                        }),
                        drag_delta: if response.dragged() {
                            let delta = ui.ctx().input(|i| i.pointer.delta()) * ppp;
                            [delta.x, delta.y]
                        } else {
                            [0.0, 0.0]
                        },
                        scroll_delta: if response.hovered() {
                            ui.ctx().input(|i| i.raw_scroll_delta.y / 120.0)
                        } else {
                            0.0
                        },
                        primary_clicked: response.clicked(),
                    };

                    let dt = ui.ctx().input(|i| i.stable_dt);
                    if let Some(frame) =
                        self.bevy
                            .render_frame_with_input(pixels[0], pixels[1], dt, viewport_input)
                    {
                        let size = [frame.width as usize, frame.height as usize];
                        let image = egui::ColorImage::from_rgba_unmultiplied(size, &frame.rgba);
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

                    picked_color = self.bevy.picked_swatch_color();
                    ui.ctx().request_repaint_after(Duration::from_millis(16));
                    return;
                }

                #[cfg(target_arch = "wasm32")]
                {
                    let grid = 36.0;
                    let grid_col = egui::Color32::from_rgba_unmultiplied(
                        accent.r(),
                        accent.g(),
                        accent.b(),
                        28,
                    );
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
                        egui::Color32::from_rgba_unmultiplied(
                            accent.r(),
                            accent.g(),
                            accent.b(),
                            90,
                        ),
                        egui::Stroke::new(1.5, accent),
                    ));

                    let status = self.status_text();
                    painter.text(
                        center + egui::vec2(0.0, cube.height() * 0.5 + 28.0),
                        egui::Align2::CENTER_CENTER,
                        status,
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
