//! # egui_mara — plain-egui facade for the mara UI kit.
//!
//! Mirrors `bevy_mara`, minus the Bevy bits. Re-exports every
//! public item from [`mara_core`] verbatim and adds a single
//! convenience helper ([`apply_theme_now`]) so `eframe` apps can
//! one-line the per-frame theme refresh that `bevy_mara`'s
//! `ThemePlugin` does automatically.
//!
//! ```ignore
//! use eframe::egui;
//! use egui_mara::prelude::*;
//!
//! struct App {
//!     accent: AccentColor,
//!     glass:  GlassOpacity,
//! }
//!
//! impl eframe::App for App {
//!     fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
//!         apply_theme_now(ctx, self.accent, self.glass);
//!         egui::CentralPanel::default().show(ctx, |ui| {
//!             let mut on = false;
//!             toggle(ui, "power", &mut on, self.accent.0);
//!         });
//!     }
//! }
//! ```
//!
//! Plain-egui hosts don't get the Bevy-side input firewall
//! (`bevy_mara::EguiInputAbsorbPlugin`) — they don't need it,
//! since `eframe` doesn't have a 3D scene competing for the same
//! pointer events.

pub use mara_core::*;

/// Per-frame theme refresh — wraps [`mara_core::style::set_glass_opacity`]
/// and [`mara_core::style::apply_theme`] so eframe `update` methods
/// can stay one-liners. Idempotent; safe to call every frame.
pub fn apply_theme_now(
    ctx: &egui::Context,
    accent: mara_core::style::AccentColor,
    glass: mara_core::style::GlassOpacity,
) {
    mara_core::style::set_glass_opacity(glass.0);
    mara_core::style::apply_theme(ctx, accent, glass);
    mara_core::publish_window_chrome_host_capabilities(
        ctx,
        mara_core::WindowChromeHostCapabilities {
            native_move: true,
            native_resize: true,
        },
    );
}

/// Per-frame eframe host bridge for Mara's borderless-window chrome.
///
/// The ribbon renderer in `mara_core` publishes the top-bar drag
/// region and interactive exclusions. This bridge maps those
/// host-neutral regions onto eframe viewport commands, so native
/// eframe apps get the same automatic move/resize behavior as the
/// Bevy facade.
pub struct EframeWindowChrome {
    state: mara_core::WindowChromeState,
    move_started_until_release: bool,
    move_command_enabled: bool,
}

impl Default for EframeWindowChrome {
    fn default() -> Self {
        Self {
            state: mara_core::WindowChromeState::default(),
            move_started_until_release: false,
            move_command_enabled: true,
        }
    }
}

impl EframeWindowChrome {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn without_move_command() -> Self {
        Self {
            move_command_enabled: false,
            ..Self::default()
        }
    }

    pub fn update(&mut self, ctx: &egui::Context) {
        // eframe's native drag/resize commands are one-shot viewport
        // commands. Never keep a persistent app-side claim here:
        // Wayland/eframe can keep stale pointer interaction data
        // around while the compositor owns a native move, which makes
        // the whole app look like a stuck drag handle.
        self.state.clear_claim();

        mara_core::publish_window_chrome_host_capabilities(
            ctx,
            mara_core::WindowChromeHostCapabilities {
                // The root Makefile forces the native eframe example
                // onto X11/XWayland. eframe's Wayland StartDrag path
                // can wedge while the compositor owns the drag, but
                // X11 keeps the persistent bar movable.
                native_move: true,
                native_resize: true,
            },
        );

        let regions = mara_core::window_chrome_regions(ctx);
        let metrics = mara_core::style::theme().window_chrome;
        let (hover_resize, pressed_resize, pressed_move, primary_down, primary_released) = ctx
            .input(|input| {
                let window_size = input
                    .viewport()
                    .inner_rect
                    .map(|rect| rect.size())
                    .unwrap_or_else(|| input.content_rect().size());
                let hit_at = |pos| {
                    mara_core::hit_test_window_chrome_regions(&regions, pos, window_size, metrics)
                };

                let hover_resize = input.pointer.hover_pos().and_then(|pos| match hit_at(pos) {
                    Some(mara_core::WindowChromeHit::Resize(direction)) => Some(direction),
                    _ => None,
                });
                let pressed_pos = if input.pointer.button_pressed(egui::PointerButton::Primary) {
                    input
                        .pointer
                        .interact_pos()
                        .or_else(|| input.pointer.hover_pos())
                } else {
                    None
                };
                let pressed_hit = pressed_pos.and_then(hit_at);
                let pressed_resize = match pressed_hit {
                    Some(mara_core::WindowChromeHit::Resize(direction)) => Some(direction),
                    _ => None,
                };
                let pressed_move = pressed_hit == Some(mara_core::WindowChromeHit::Move);
                (
                    hover_resize,
                    pressed_resize,
                    pressed_move,
                    input.pointer.primary_down(),
                    input.pointer.button_released(egui::PointerButton::Primary),
                )
            });

        if primary_released || !primary_down {
            self.move_started_until_release = false;
        }

        if let Some(direction) = hover_resize {
            ctx.set_cursor_icon(resize_cursor(direction));
        }

        if let Some(direction) = pressed_resize {
            mara_core::claim_window_chrome_input(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(egui_resize_direction(
                direction,
            )));
            return;
        }

        if self.move_command_enabled && pressed_move && !self.move_started_until_release {
            self.move_started_until_release = true;
            mara_core::claim_window_chrome_input(ctx);
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
    }
}

pub fn close_native_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
}

fn resize_cursor(direction: mara_core::WindowResizeDirection) -> egui::CursorIcon {
    match direction {
        mara_core::WindowResizeDirection::North | mara_core::WindowResizeDirection::South => {
            egui::CursorIcon::ResizeVertical
        }
        mara_core::WindowResizeDirection::East | mara_core::WindowResizeDirection::West => {
            egui::CursorIcon::ResizeHorizontal
        }
        mara_core::WindowResizeDirection::NorthEast
        | mara_core::WindowResizeDirection::SouthWest => egui::CursorIcon::ResizeNeSw,
        mara_core::WindowResizeDirection::NorthWest
        | mara_core::WindowResizeDirection::SouthEast => egui::CursorIcon::ResizeNwSe,
    }
}

fn egui_resize_direction(direction: mara_core::WindowResizeDirection) -> egui::ResizeDirection {
    match direction {
        mara_core::WindowResizeDirection::North => egui::ResizeDirection::North,
        mara_core::WindowResizeDirection::South => egui::ResizeDirection::South,
        mara_core::WindowResizeDirection::East => egui::ResizeDirection::East,
        mara_core::WindowResizeDirection::West => egui::ResizeDirection::West,
        mara_core::WindowResizeDirection::NorthEast => egui::ResizeDirection::NorthEast,
        mara_core::WindowResizeDirection::SouthEast => egui::ResizeDirection::SouthEast,
        mara_core::WindowResizeDirection::NorthWest => egui::ResizeDirection::NorthWest,
        mara_core::WindowResizeDirection::SouthWest => egui::ResizeDirection::SouthWest,
    }
}

/// Glob-import. Mirrors `bevy_mara::prelude` — apps that flip
/// between Bevy and eframe hosts get the same module surface
/// from `<facade>::prelude::*` and only the `main` differs.
pub mod prelude {
    pub use super::EframeNodeViewBackend;
    pub use super::EframeWindowChrome;
    pub use super::apply_theme_now;
    pub use super::close_native_window;
    pub use mara_core::*;
}

/// `NodeViewBackend` impl backed by eframe's `egui_wgpu::RenderState`.
///
/// Borrow it from your `eframe::App::update` via
/// `frame.wgpu_render_state()` — it provides the wgpu device + queue
/// and lets us register native textures with the SAME egui-wgpu
/// renderer eframe uses to paint the rest of the UI, so the
/// secondary context's offscreen render target can be drawn as
/// `egui::Image` without copy-back.
///
/// ```ignore
/// fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
///     let render_state = frame.wgpu_render_state().expect("wgpu");
///     let mut backend = egui_mara::EframeNodeViewBackend::new(render_state);
///     egui::CentralPanel::default().show(ctx, |ui| {
///         mara_core::extras::graph::mara_node_graph(
///             ui, &mut self.node_view_state, &mut backend,
///             &mut self.graph, &mut self.viewer,
///             accent, ui.available_size(),
///         );
///     });
/// }
/// ```
pub struct EframeNodeViewBackend<'a> {
    render_state: &'a egui_wgpu::RenderState,
}

impl<'a> EframeNodeViewBackend<'a> {
    pub fn new(render_state: &'a egui_wgpu::RenderState) -> Self {
        Self { render_state }
    }
}

impl<'a> mara_core::extras::node_view::NodeViewBackend for EframeNodeViewBackend<'a> {
    fn wgpu(&self) -> (wgpu::Device, wgpu::Queue) {
        // `wgpu::Device` / `Queue` are cheap to clone in wgpu 27 —
        // internally Arc-counted handles to the same backend
        // resources.
        (
            self.render_state.device.clone(),
            self.render_state.queue.clone(),
        )
    }

    fn target_format(&self) -> wgpu::TextureFormat {
        self.render_state.target_format
    }

    fn register_native(
        &mut self,
        _texture: &wgpu::Texture,
        view: &wgpu::TextureView,
        _size_pixels: [u32; 2],
        filter: wgpu::FilterMode,
    ) -> egui::TextureId {
        // eframe wraps the renderer in `Arc<RwLock<...>>`; take a
        // write-guard for the mutating call. The lock is short-
        // lived (just the registration call), so no contention
        // with the parent UI's render pass which runs in the
        // `eframe::Frame::end_pass` flow much later. The eframe
        // path doesn't need `texture` or `size_pixels` because
        // egui-wgpu's `register_native_texture` uses the view
        // directly — those args exist for backends like Bevy that
        // can't sample arbitrary `wgpu::TextureView`s and need to
        // mirror into their own asset system.
        let mut renderer = self.render_state.renderer.write();
        renderer.register_native_texture(&self.render_state.device, view, filter)
    }

    fn unregister_native(&mut self, id: egui::TextureId) {
        let mut renderer = self.render_state.renderer.write();
        renderer.free_texture(&id);
    }
}
