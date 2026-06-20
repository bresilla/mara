//! Host context for Mara views.
//!
//! This is the app/view boundary: every host that drives Mara UI can
//! hand views a `MaraHostCtx` so they can request app-level actions
//! (theme, close, render helpers) without knowing whether the window
//! is owned by Mara, eframe, Bevy, web, or something else.

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

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable.
    #[doc(hidden)]
    pub fn __internal_egui(&self) -> &'a egui::Context {
        self.egui
    }

    pub fn render_state(&self) -> Option<&'a egui_wgpu::RenderState> {
        self.render_state
    }

    pub fn window(&self) -> MaraWindowHost {
        self.window
    }

    /// Current host content rectangle in Mara vocabulary.
    pub fn content_rect(&self) -> mara_core::vocab::Rect {
        self.egui.content_rect().into()
    }

    /// Reserve shelf space for the current host content rectangle.
    ///
    /// Root surfaces should use this host facade instead of reading
    /// raw backend geometry and calling shelf layout helpers directly.
    pub fn layout_shelves(
        &self,
        shelves: &[mara_core::ShelfDef<'_>],
        state: &mut mara_core::ShelfState,
    ) -> mara_core::ShelfLayout {
        let shelf_theme = *mara_core::style::theme().shelf();
        mara_core::layout_shelves(self.content_rect(), shelves, state, &shelf_theme)
    }

    /// Paint shelves through the current host backend.
    pub fn show_shelves(
        &self,
        layout: mara_core::ShelfLayout,
        shelves: Vec<mara_core::ShelfDef<'_>>,
        state: &mut mara_core::ShelfState,
    ) {
        mara_core::shelf::__internal_show_shelves_egui(self.egui, layout, shelves, state);
    }

    /// Show a root-level Mara body in the host content panel.
    ///
    /// The current backend is egui, but the body receives a sealed
    /// [`mara_core::MaraUi`] plus Mara vocabulary geometry.
    pub fn show_root_body<R>(
        &self,
        accent: impl Into<mara_core::vocab::Color32>,
        body: impl FnOnce(&mut mara_core::MaraUi<'_>, mara_core::vocab::Rect) -> R,
    ) -> R {
        let accent = accent.into();
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
            .show(self.egui, |ui| {
                let screen_rect = ui.max_rect().into();
                let mut mui = mara_core::MaraUi::__internal_from_raw(ui, accent);
                body(&mut mui, screen_rect)
            })
            .inner
    }

    /// Draw already-resolved featureful slot ribbons through the
    /// current host backend.
    pub fn draw_slot_ribbons_featureful(
        &self,
        accent: impl Into<mara_core::vocab::Color32>,
        ribbons: &[mara_core::ribbon::ResolvedSlotRibbon],
        open: &mut mara_core::ribbon::RibbonOpen,
        placement: &mut mara_core::ribbon::RibbonPlacement,
        drag: &mut mara_core::ribbon::RibbonDrag,
    ) -> Vec<mara_core::ribbon::RibbonSlotClick> {
        mara_core::ribbon::__internal_draw_slot_ribbons_featureful_egui(
            self.egui, accent, ribbons, open, placement, drag,
        )
    }

    /// Current host input time in seconds.
    pub fn input_time(&self) -> f64 {
        self.egui.input(|i| i.time)
    }

    /// Request another host repaint/frame.
    pub fn request_repaint(&self) {
        self.egui.request_repaint();
    }

    /// Build a sealed Mara view context for a root or module surface.
    ///
    /// App/view code should prefer this over reaching for the raw host
    /// `egui::Context`; the current egui backend stays behind the
    /// host facade.
    pub fn view_ctx<'frame>(
        &'frame self,
        workspace: &'frame mut mara_core::WorkspaceStack,
        accent: impl Into<mara_core::vocab::Color32>,
        ribbon_avoidance: mara_core::RibbonAvoidance,
    ) -> mara_core::ViewCtx<'frame> {
        mara_core::ViewCtx::__internal_new(self.egui, workspace, accent, ribbon_avoidance)
    }

    /// Publish the layout rectangle left after structural shelves reserve
    /// their space. Views that do not draw shelves can publish
    /// `ShelfLayout::full(host.content_rect())` through this facade; real
    /// shelf renderers publish automatically.
    pub fn publish_shelf_layout(&self, layout: mara_core::ShelfLayout) {
        mara_core::shelf::__internal_publish_shelf_layout(self.egui, layout);
    }

    /// Publish a no-shelf layout covering the live host content rect.
    pub fn publish_full_shelf_layout(&self) {
        self.publish_shelf_layout(mara_core::ShelfLayout::full(self.content_rect()));
    }

    /// Show a floating/anchored Mara pane through the sealed host
    /// facade. The closure receives typed [`mara_core::pane::PaneBody`]
    /// content, not a raw backend UI.
    pub fn show_pane<'spec>(
        &self,
        pane: mara_core::pane::Pane,
        body: impl FnOnce(&mut mara_core::pane::PaneBody<'_, 'spec>),
    ) {
        pane.__internal_show(self.egui, body);
    }

    /// Publish the pane ids reachable from the current ribbon set.
    ///
    /// Pane rendering uses this registry to reject loose panes that
    /// have no matching ribbon affordance.
    pub fn publish_ribbon_pane_ids(
        &self,
        ids: impl IntoIterator<Item = impl Into<mara_core::vocab::Id>>,
    ) {
        mara_core::pane::__internal_publish_ribbon_pane_ids(self.egui, ids);
    }

    /// Draw the command palette through the sealed host facade.
    ///
    /// App code supplies Mara command-palette state/items/accent and
    /// receives the picked item id; the current raw egui context stays
    /// behind this host boundary.
    pub fn command_palette(
        &self,
        state: &mut mara_core::CommandPaletteState,
        items: &[mara_core::PaletteItem],
        accent: impl Into<mara_core::vocab::Color32>,
    ) -> Option<&'static str> {
        mara_core::command_palette::__internal_command_palette_egui(self.egui, state, items, accent)
    }

    /// Current maximized-widget owner, if a maximizable Mara surface
    /// owns the full host content area this frame.
    pub fn fullscreen_owner(&self) -> Option<mara_core::vocab::Id> {
        mara_core::embed::__internal_fullscreen_owner(self.egui)
    }

    /// `true` when any maximizable Mara surface owns the full host
    /// content area this frame.
    pub fn is_any_fullscreen(&self) -> bool {
        self.fullscreen_owner().is_some()
    }

    /// Hide/show the built-in fullscreen restore chip for this
    /// frame. Host shells that provide their own persistent
    /// app/module bar can hide the floating chip and route restore
    /// through their normal chrome.
    pub fn set_fullscreen_minimize_chip_visible(&self, visible: bool) {
        mara_core::embed::__internal_set_fullscreen_minimize_chip_visible(self.egui, visible);
    }

    /// Restore the active full-window maximizable widget, if one
    /// exists. Returns `true` when a fullscreen owner was found and
    /// toggled off.
    pub fn restore_fullscreen(&self) -> bool {
        mara_core::embed::__internal_restore_fullscreen(self.egui)
    }

    /// Apply Mara theme/glass/accent for the current frame.
    pub fn apply_theme(
        &self,
        accent: mara_core::style::AccentColor,
        glass: mara_core::style::GlassOpacity,
    ) {
        mara_core::style::set_glass_opacity(glass.0);
        mara_core::style::__internal_apply_theme(self.egui, accent, glass);
        mara_core::window_chrome::__internal_publish_window_chrome_host_capabilities(
            self.egui,
            mara_core::WindowChromeHostCapabilities {
                native_move: self.window.native_move(),
                native_resize: self.window.native_resize(),
                system_maximize: self.window.system_maximize(),
                system_close: self.window.system_close(),
            },
        );
    }

    /// Request the top-level host window to close.
    pub fn request_close(&self) {
        self.egui.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Toggle the top-level host window between maximized and restored.
    /// Reads the current state from the viewport info and sends the
    /// inverse. Hosts that own the window apply it; embedded hosts
    /// ignore the command.
    pub fn request_maximize_toggle(&self) {
        let maximized = self.egui.input(|i| i.viewport().maximized).unwrap_or(false);
        self.egui
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
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

    /// The maximize/restore control mirrors close: only a host that
    /// owns the native window can honor a maximize command.
    pub fn system_maximize(self) -> bool {
        matches!(self, Self::MaraNative)
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
impl<'a> mara_graph::node_view::NodeViewBackend for EframeNodeViewBackend<'a> {
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
