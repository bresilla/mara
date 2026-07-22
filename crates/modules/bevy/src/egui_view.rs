//! Mara/egui-hosted Bevy viewport widget.
//!
//! This is the host-facing view wrapper around [`BevyEmbeddedView`]:
//! it owns the egui allocation, interaction forwarding, resize
//! throttling, texture upload, and warmup/fallback painting. Example
//! apps should only hold this state and place it in their content
//! tree; the viewport mechanics live here in the module crate.

use std::time::Duration;

use mara_core::{ViewCtx, vocab::Color32 as MaraColor32};

use crate::{BevyEmbeddedView, BevyViewportInput, BevyViewportWgpuResources};

/// Egui/Mara-hosted Bevy viewport.
///
/// The host still owns the top-level window. This widget reserves an
/// egui region, asks the embedded Bevy bridge to render into an
/// offscreen target, uploads the latest RGBA frame as an egui texture,
/// and forwards pointer/scroll interaction into the Bevy camera.
pub struct MaraBevyViewport {
    /// Per-instance salt for egui area/interact ids, so two viewports
    /// in one context never collide on shared area memory or input.
    instance: u64,
    bevy: BevyEmbeddedView,
    texture: Option<egui::TextureHandle>,
    last_pixels: [u32; 2],
    resize_target_pixels: [u32; 2],
    resize_settle_until: f64,
    last_render_time: f64,
    last_pointer_pos: Option<egui::Pos2>,
    primary_drag_active: bool,
    native_texture: Option<egui::TextureId>,
    native_texture_size: [usize; 2],
}

impl Default for MaraBevyViewport {
    fn default() -> Self {
        Self::new()
    }
}

/// Monotonic per-instance salt so every viewport gets unique egui ids.
fn next_viewport_instance() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl MaraBevyViewport {
    pub fn new() -> Self {
        Self {
            instance: next_viewport_instance(),
            bevy: BevyEmbeddedView::new(),
            texture: None,
            last_pixels: [0, 0],
            resize_target_pixels: [0, 0],
            resize_settle_until: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
            native_texture: None,
            native_texture_size: [0, 0],
        }
    }

    pub fn with_content(
        configure_app: impl Fn(&mut bevy::prelude::App) + Send + Sync + 'static,
    ) -> Self {
        Self {
            instance: next_viewport_instance(),
            bevy: BevyEmbeddedView::with_app_config(configure_app),
            texture: None,
            last_pixels: [0, 0],
            resize_target_pixels: [0, 0],
            resize_settle_until: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
            native_texture: None,
            native_texture_size: [0, 0],
        }
    }

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
            instance: next_viewport_instance(),
            bevy,
            texture: None,
            last_pixels: [0, 0],
            resize_target_pixels: [0, 0],
            resize_settle_until: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
            native_texture: None,
            native_texture_size: [0, 0],
        }
    }

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
            instance: next_viewport_instance(),
            bevy,
            texture: None,
            last_pixels: [0, 0],
            resize_target_pixels: [0, 0],
            resize_settle_until: f64::NEG_INFINITY,
            last_render_time: f64::NEG_INFINITY,
            last_pointer_pos: None,
            primary_drag_active: false,
            native_texture: None,
            native_texture_size: [0, 0],
        }
    }

    /// Pause/resume the embedded Bevy renderer.
    ///
    /// Call this from the host when the Bevy viewport is not the
    /// active Mara view. The last texture is retained, but Bevy's
    /// offscreen app is not ticked and its cameras are marked
    /// inactive until the view is active again.
    pub fn set_active(&mut self, active: bool) {
        self.bevy.set_rendering_enabled(active);
        if !active {
            self.primary_drag_active = false;
            self.last_pointer_pos = None;
        }
    }

    #[must_use]
    pub fn is_active(&self) -> bool {
        self.bevy.rendering_enabled()
    }

    pub fn show(
        &mut self,
        ctx: &mut ViewCtx<'_>,
        render_state: Option<mara_gpu::MaraRenderState<'_>>,
        accent: impl Into<MaraColor32>,
    ) -> Option<MaraColor32> {
        // Unwrap the opaque host handle at the module boundary — sealed
        // callers pass `MaraRenderState` around without seeing egui-wgpu.
        let render_state: Option<&egui_wgpu::RenderState> =
            render_state.map(|state| state.__internal_raw());
        let accent = accent.into();
        self.set_active(true);
        let mut picked_color = None;
        // Render into this node's REGION, not a window-grabbing panel
        // (ADR 0002 / PLAN WS6): a Bevy viewport hosted as a split cell
        // draws and interacts inside its cell rect, so it tiles like
        // any other leaf. Whole-window is just the one-leaf tree.
        let region_rect: egui::Rect = ctx.screen_rect().into();
        {
            egui::Area::new(egui::Id::new(("mara_bevy_viewport_area", self.instance)))
                .order(egui::Order::Background)
                .fixed_pos(region_rect.min)
                .show(ctx.__internal_egui_ctx(), |ui| {
                    ui.set_clip_rect(region_rect);
                    let rect = region_rect;
                    let painter = ui.painter_at(rect);
                    let theme = mara_core::style::theme();
                    painter.rect_filled(rect, 0.0, theme.palette.bg_panel);
                    if rect.width() < 16.0 || rect.height() < 16.0 {
                        if let Some(texture_id) = self.native_texture {
                            paint_texture_id_cover(
                                &painter,
                                texture_id,
                                self.native_texture_size,
                                rect,
                            );
                        } else if let Some(texture) = &self.texture {
                            paint_texture_cover(&painter, texture, rect);
                        }
                        ui.ctx()
                            .request_repaint_after(Duration::from_secs_f64(1.0 / 12.0));
                        return;
                    }

                    if let Some(render_state) = render_state {
                        self.bevy
                            .attach_wgpu_resources(BevyViewportWgpuResources::new(
                                render_state.device.clone(),
                                render_state.queue.clone(),
                                render_state.adapter.clone(),
                            ));
                    }
                    let use_native_gpu_texture = render_state.is_some();

                    let response = ui.interact(
                        rect,
                        egui::Id::new(("mara_embedded_bevy_viewport_interact", self.instance)),
                        egui::Sense::click_and_drag(),
                    );
                    let ppp = ui.ctx().pixels_per_point();
                    let now = ui.ctx().input(|i| i.time);
                    let target_pixels = internal_render_pixels(rect.size(), ppp);
                    if self.resize_target_pixels != target_pixels {
                        self.resize_target_pixels = target_pixels;
                        self.resize_settle_until = now + resize_settle_seconds();
                    }
                    let has_committed_texture =
                        self.native_texture.is_some() || self.texture.is_some();
                    let waiting_for_resize_settle = has_committed_texture
                        && self.last_pixels != [0, 0]
                        && self.last_pixels != target_pixels
                        && now < self.resize_settle_until;
                    let pixels = if waiting_for_resize_settle {
                        self.last_pixels
                    } else {
                        target_pixels
                    };
                    let resize_pending = self.last_pixels != [0, 0] && self.last_pixels != pixels;
                    let render_scale = egui::vec2(
                        pixels[0] as f32 / rect.width().max(1.0),
                        pixels[1] as f32 / rect.height().max(1.0),
                    );

                    // Only the viewport's own egui `Response` may start
                    // Bevy interaction. Do NOT use raw/global pointer
                    // containment as the start condition: floating menus,
                    // panes and container-dot handles can sit above this
                    // rect, so `rect.contains(pointer)` would leak those
                    // UI clicks into the Bevy camera/picker below.
                    let viewport_hovered = response.hovered();
                    let pointer_pos = if viewport_hovered || self.primary_drag_active {
                        ui.ctx().input(|i| i.pointer.interact_pos())
                    } else {
                        response.hover_pos()
                    };
                    let (primary_down, primary_pressed, middle_down, middle_pressed, scroll_delta) =
                        ui.ctx().input(|i| {
                            (
                                i.pointer.primary_down(),
                                i.pointer.button_pressed(egui::PointerButton::Primary),
                                i.pointer.middle_down(),
                                i.pointer.button_pressed(egui::PointerButton::Middle),
                                if viewport_hovered {
                                    i.smooth_scroll_delta.y / 120.0
                                } else {
                                    0.0
                                },
                            )
                        });
                    let viewport_drag_started =
                        viewport_hovered && (primary_pressed || middle_pressed);
                    if viewport_drag_started {
                        self.primary_drag_active = true;
                        self.last_pointer_pos = pointer_pos;
                    }
                    let viewport_drag_down = primary_down || middle_down;
                    let viewport_dragged = viewport_drag_down && self.primary_drag_active;
                    let pointer_delta = if viewport_dragged {
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
                        if !viewport_drag_down {
                            self.primary_drag_active = false;
                            self.last_pointer_pos = None;
                        }
                        [0.0, 0.0]
                    };
                    let middle_dragged = viewport_dragged && middle_down;
                    let primary_dragged = viewport_dragged && primary_down && !middle_dragged;
                    let drag_delta = if primary_dragged {
                        pointer_delta
                    } else {
                        [0.0, 0.0]
                    };
                    let pan_delta = if middle_dragged {
                        pointer_delta
                    } else {
                        [0.0, 0.0]
                    };
                    let primary_clicked = primary_pressed && viewport_hovered;

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
                        pan_delta,
                        scroll_delta,
                        primary_clicked,
                    };

                    let input_active = viewport_dragged
                        || primary_clicked
                        || response.hovered() && viewport_input.scroll_delta.abs() > f32::EPSILON
                        || response.hovered() && ui.ctx().input(|i| i.pointer.any_down());
                    let target_size = [pixels[0] as usize, pixels[1] as usize];
                    let native_texture_needs_committed_frame =
                        self.native_texture.is_some() && self.native_texture_size != target_size;
                    let cpu_texture_needs_committed_frame = self
                        .texture
                        .as_ref()
                        .is_some_and(|texture| texture.size() != target_size);
                    let texture_needs_committed_frame =
                        native_texture_needs_committed_frame || cpu_texture_needs_committed_frame;
                    let has_texture = has_committed_texture;
                    #[cfg(target_arch = "wasm32")]
                    let idle_interval = 1.0 / 12.0;
                    #[cfg(not(target_arch = "wasm32"))]
                    let idle_interval = 1.0 / 24.0;
                    #[cfg(target_arch = "wasm32")]
                    let active_interval = 1.0 / 30.0;
                    #[cfg(not(target_arch = "wasm32"))]
                    let active_interval = 1.0 / 60.0;
                    let target_interval = if input_active
                        || resize_pending
                        || texture_needs_committed_frame
                        || !has_texture
                    {
                        active_interval
                    } else {
                        idle_interval
                    };
                    let elapsed = now - self.last_render_time;
                    let should_render = resize_pending
                        || !has_texture
                        || texture_needs_committed_frame
                        || input_active
                        || elapsed >= target_interval;

                    if should_render {
                        self.last_render_time = now;
                        let dt = ui.ctx().input(|i| i.stable_dt);
                        let render_attempts = 1;
                        for attempt in 0..render_attempts {
                            let input = if attempt == 0 {
                                viewport_input
                            } else {
                                BevyViewportInput {
                                    pointer_pos: viewport_input.pointer_pos,
                                    ..Default::default()
                                }
                            };
                            let dt = if attempt == 0 { dt } else { 0.0 };
                            if use_native_gpu_texture
                                && let Some(render_state) = render_state
                                && let Some(frame) = self
                                    .bevy
                                    .render_texture_with_input(pixels[0], pixels[1], dt, input)
                            {
                                let size = [frame.width as usize, frame.height as usize];
                                if size == target_size {
                                    let texture_id = {
                                        let mut renderer = render_state.renderer.write();
                                        match self.native_texture {
                                            Some(texture_id)
                                                if self.native_texture_size == size =>
                                            {
                                                // The egui texture id already points at the
                                                // Bevy render target. Bevy updates the GPU
                                                // texture contents in-place, so avoid rebuilding
                                                // the egui bind group every frame.
                                                texture_id
                                            }
                                            Some(texture_id) => {
                                                renderer.free_texture(&texture_id);
                                                renderer.register_native_texture(
                                                    &render_state.device,
                                                    &frame.view,
                                                    wgpu::FilterMode::Linear,
                                                )
                                            }
                                            None => renderer.register_native_texture(
                                                &render_state.device,
                                                &frame.view,
                                                wgpu::FilterMode::Linear,
                                            ),
                                        }
                                    };
                                    self.native_texture = Some(texture_id);
                                    self.native_texture_size = size;
                                    self.last_pixels = pixels;
                                    break;
                                }
                            }

                            if !use_native_gpu_texture
                                && let Some(frame) = self
                                    .bevy
                                    .render_frame_with_input(pixels[0], pixels[1], dt, input)
                            {
                                let size = [frame.width as usize, frame.height as usize];
                                let rgba = if size == target_size {
                                    Some(frame.rgba.clone())
                                } else {
                                    None
                                };
                                if let Some(rgba) = rgba {
                                    self.last_pixels = pixels;
                                    let image =
                                        egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
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
                                    break;
                                }
                            }
                        }
                    }

                    if let Some(texture_id) = self.native_texture {
                        paint_texture_id_cover(
                            &painter,
                            texture_id,
                            self.native_texture_size,
                            rect,
                        );
                    } else if let Some(texture) = &self.texture {
                        paint_texture_cover(&painter, texture, rect);
                    } else if self.texture.is_none() {
                        self.paint_warmup(ui, rect, accent);
                    }

                    picked_color = self.bevy.picked_color();
                    let mut next = if should_render {
                        target_interval
                    } else {
                        (target_interval - elapsed).max(active_interval)
                    };
                    if resize_pending || texture_needs_committed_frame {
                        next = next.min(active_interval);
                    }
                    if waiting_for_resize_settle {
                        next = next.min((self.resize_settle_until - now).max(0.0));
                    }
                    ui.ctx()
                        .request_repaint_after(Duration::from_secs_f64(next));
                });
        }
        picked_color
    }

    fn paint_warmup(&self, ui: &egui::Ui, rect: egui::Rect, accent: MaraColor32) {
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
                "embedded Bevy renderer unavailable: no wgpu adapter"
            } else {
                "warming up embedded Bevy renderer…"
            },
            egui::FontId::proportional(13.0),
            mara_core::style::on_panel().into(),
        );
    }
}

fn internal_render_pixels(size: egui::Vec2, pixels_per_point: f32) -> [u32; 2] {
    // Keep the embedded Bevy view close to native DPI. The old cap
    // was intentionally conservative while debugging the web bridge,
    // but it forced high-DPI/browser windows to render low-res and
    // then upscale in egui, making the scene visibly soft.
    #[cfg(not(target_arch = "wasm32"))]
    const MAX_WIDTH: f32 = 2560.0;
    #[cfg(target_arch = "wasm32")]
    const MAX_WIDTH: f32 = 1920.0;
    #[cfg(not(target_arch = "wasm32"))]
    const MAX_HEIGHT: f32 = 1600.0;
    #[cfg(target_arch = "wasm32")]
    const MAX_HEIGHT: f32 = 1200.0;
    #[cfg(not(target_arch = "wasm32"))]
    const MAX_PIXELS: f32 = 3_600_000.0;
    #[cfg(target_arch = "wasm32")]
    const MAX_PIXELS: f32 = 1_600_000.0;

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

fn resize_settle_seconds() -> f64 {
    #[cfg(target_arch = "wasm32")]
    {
        0.10
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        0.07
    }
}

fn paint_texture_cover(painter: &egui::Painter, texture: &egui::TextureHandle, rect: egui::Rect) {
    paint_texture_id_cover(painter, texture.id(), texture.size(), rect);
}

fn paint_texture_id_cover(
    painter: &egui::Painter,
    texture_id: egui::TextureId,
    texture_size: [usize; 2],
    rect: egui::Rect,
) {
    let [texture_width, texture_height] = texture_size;
    if texture_width == 0 || texture_height == 0 || !rect.is_positive() {
        return;
    }

    let texture_aspect = texture_width as f32 / texture_height as f32;
    let rect_aspect = rect.width() / rect.height().max(1.0);
    let uv = if rect_aspect > texture_aspect {
        // The viewport is wider than the old frame. Cover the rect by
        // cropping top/bottom instead of stretching.
        let visible_v = (texture_aspect / rect_aspect).clamp(0.0, 1.0);
        let pad_v = (1.0 - visible_v) * 0.5;
        egui::Rect::from_min_max(egui::pos2(0.0, pad_v), egui::pos2(1.0, 1.0 - pad_v))
    } else {
        // The viewport is taller/narrower than the old frame. Cover
        // the rect by cropping left/right instead of stretching.
        let visible_u = (rect_aspect / texture_aspect).clamp(0.0, 1.0);
        let pad_u = (1.0 - visible_u) * 0.5;
        egui::Rect::from_min_max(egui::pos2(pad_u, 0.0), egui::pos2(1.0 - pad_u, 1.0))
    };

    painter.image(texture_id, rect, uv, egui::Color32::WHITE);
}
