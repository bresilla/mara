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
}

/// Glob-import. Mirrors `bevy_mara::prelude` — apps that flip
/// between Bevy and eframe hosts get the same module surface
/// from `<facade>::prelude::*` and only the `main` differs.
pub mod prelude {
    pub use super::EframeNodeViewBackend;
    pub use super::apply_theme_now;
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
