//! Host context for Mara views.
//!
//! This is the app/view boundary: every host that drives Mara UI can
//! hand views a `MaraHostCtx` so they can request app-level actions
//! (theme, close, render helpers) without knowing whether the window
//! is owned by Mara, eframe, Bevy, web, or something else.

use crate::ui::egui;

/// Per-frame host context passed from the app shell to Mara views.
#[derive(Clone, Copy)]
pub struct MaraHostCtx<'a> {
    egui: &'a egui::Context,
    render_state: Option<&'a egui_wgpu::RenderState>,
    window: MaraWindowHost,
}

impl<'a> MaraHostCtx<'a> {
    pub fn new(
        egui: &'a egui::Context,
        render_state: Option<&'a egui_wgpu::RenderState>,
        window: MaraWindowHost,
    ) -> Self {
        Self {
            egui,
            render_state,
            window,
        }
    }

    pub fn ui_only(
        egui: &'a egui::Context,
        render_state: Option<&'a egui_wgpu::RenderState>,
    ) -> Self {
        Self::new(egui, render_state, MaraWindowHost::ExternalEgui)
    }

    pub fn mara_window(
        egui: &'a egui::Context,
        render_state: Option<&'a egui_wgpu::RenderState>,
    ) -> Self {
        Self::new(egui, render_state, MaraWindowHost::MaraNative)
    }

    pub fn egui(&self) -> &'a egui::Context {
        self.egui
    }

    pub fn render_state(&self) -> Option<&'a egui_wgpu::RenderState> {
        self.render_state
    }

    pub fn window(&self) -> MaraWindowHost {
        self.window
    }

    /// Apply Mara theme/glass/accent for the current frame.
    pub fn apply_theme(
        &self,
        accent: mara_core::style::AccentColor,
        glass: mara_core::style::GlassOpacity,
    ) {
        mara_core::style::set_glass_opacity(glass.0);
        mara_core::style::apply_theme(self.egui, accent, glass);
        mara_core::publish_window_chrome_host_capabilities(
            self.egui,
            mara_core::WindowChromeHostCapabilities {
                native_move: self.window.native_move(),
                native_resize: self.window.native_resize(),
                system_menu: self.window.system_menu(),
                system_close: self.window.system_close(),
            },
        );
    }

    /// Request the top-level host window to close.
    pub fn request_close(&self) {
        self.egui.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Build the node-view render backend for this frame, if the host
    /// has a native egui-wgpu render state available.
    #[cfg(feature = "graph")]
    pub fn node_view_backend(&self) -> Option<EframeNodeViewBackend<'a>> {
        self.render_state.map(EframeNodeViewBackend::new)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaraWindowHost {
    None,
    ExternalEgui,
    MaraNative,
}

impl MaraWindowHost {
    pub fn native_move(self) -> bool {
        matches!(self, Self::ExternalEgui | Self::MaraNative)
    }

    pub fn native_resize(self) -> bool {
        matches!(self, Self::ExternalEgui | Self::MaraNative)
    }

    pub fn system_menu(self) -> bool {
        true
    }

    pub fn system_close(self) -> bool {
        matches!(self, Self::MaraNative)
    }
}

/// `NodeViewBackend` impl backed by an egui-wgpu render state.
#[cfg(feature = "graph")]
pub struct EframeNodeViewBackend<'a> {
    render_state: &'a egui_wgpu::RenderState,
}

#[cfg(feature = "graph")]
impl<'a> EframeNodeViewBackend<'a> {
    pub fn new(render_state: &'a egui_wgpu::RenderState) -> Self {
        Self { render_state }
    }
}

#[cfg(feature = "graph")]
impl<'a> mara_core::extras::node_view::NodeViewBackend for EframeNodeViewBackend<'a> {
    fn wgpu(&self) -> (wgpu::Device, wgpu::Queue) {
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
        let mut renderer = self.render_state.renderer.write();
        renderer.register_native_texture(&self.render_state.device, view, filter)
    }

    fn unregister_native(&mut self, id: egui::TextureId) {
        let mut renderer = self.render_state.renderer.write();
        renderer.free_texture(&id);
    }
}
