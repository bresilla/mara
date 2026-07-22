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

/// High-level declaration for one Mara ribbon rail.
///
/// This is the app-facing API for the common "rail buttons open panes,
/// actions return clicks" shell shape. It owns the boring parts that
/// consumers were previously forced to copy from the demo:
/// `ResolvedSlotRibbon` construction, `RibbonScope`, pane-id
/// publication, `RibbonOpen` state, and the required panes-before-ribbons
/// paint order.
pub struct RibbonRail<'a, 'spec> {
    id: &'static str,
    scope: mara_core::RibbonScope,
    edge: mara_core::ribbon::RibbonEdge,
    mode: mara_core::ribbon::RibbonMode,
    accepts: &'static [&'static str],
    default_open: Option<&'static str>,
    panes: Vec<RibbonPane<'a, 'spec>>,
    actions: Vec<RibbonActionButton>,
}

/// One pane button hosted by a [`RibbonRail`].
pub struct RibbonPane<'a, 'spec> {
    id: &'static str,
    title: &'static str,
    icon: &'static str,
    cluster: mara_core::ribbon::RibbonCluster,
    anchor: mara_core::pane::PaneAnchor,
    resize: mara_core::pane::PaneResize,
    body: Box<dyn FnOnce(&mut mara_core::pane::PaneBody<'_, 'spec>) + 'a>,
}

/// One non-pane action button hosted by a [`RibbonRail`].
#[derive(Clone, Copy, Debug)]
pub struct RibbonActionButton {
    id: &'static str,
    icon: &'static str,
    tooltip: &'static str,
    cluster: mara_core::ribbon::RibbonCluster,
    action: mara_core::ribbon::RibbonAction,
}

#[derive(Clone, Default)]
struct HostRibbonRailState {
    initialized: bool,
    open: mara_core::ribbon::RibbonOpen,
    placement: mara_core::ribbon::RibbonPlacement,
    drag: mara_core::ribbon::RibbonDrag,
}

impl<'a, 'spec> RibbonRail<'a, 'spec> {
    #[must_use]
    pub fn view(
        id: &'static str,
        view: impl std::hash::Hash,
        edge: mara_core::ribbon::RibbonEdge,
    ) -> Self {
        Self {
            id,
            scope: mara_core::RibbonScope::View(mara_core::ViewId::new(view)),
            edge,
            mode: mara_core::ribbon::RibbonMode::ThreeSided,
            accepts: &[],
            default_open: None,
            panes: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[must_use]
    pub fn view_left(id: &'static str, view: impl std::hash::Hash) -> Self {
        Self::view(id, view, mara_core::ribbon::RibbonEdge::Left)
    }

    #[must_use]
    pub fn view_right(id: &'static str, view: impl std::hash::Hash) -> Self {
        Self::view(id, view, mara_core::ribbon::RibbonEdge::Right)
    }

    #[must_use]
    pub fn view_bottom(id: &'static str, view: impl std::hash::Hash) -> Self {
        Self::view(id, view, mara_core::ribbon::RibbonEdge::Bottom)
    }

    #[must_use]
    pub fn permanent_top(id: &'static str) -> Self {
        Self {
            id,
            scope: mara_core::RibbonScope::Permanent,
            edge: mara_core::ribbon::RibbonEdge::Top,
            mode: mara_core::ribbon::RibbonMode::ThreeSided,
            accepts: &[],
            default_open: None,
            panes: Vec::new(),
            actions: Vec::new(),
        }
    }

    #[must_use]
    pub fn accepts(mut self, accepts: &'static [&'static str]) -> Self {
        self.accepts = accepts;
        self
    }

    #[must_use]
    pub fn default_open(mut self, pane_id: &'static str) -> Self {
        self.default_open = Some(pane_id);
        self
    }

    #[must_use]
    pub fn pane(
        self,
        id: &'static str,
        icon: &'static str,
        title: &'static str,
        anchor: mara_core::pane::PaneAnchor,
        body: impl FnOnce(&mut mara_core::pane::PaneBody<'_, 'spec>) + 'a,
    ) -> Self {
        self.pane_in(
            ribbon_cluster_for_pane_anchor(anchor),
            id,
            icon,
            title,
            anchor,
            body,
        )
    }

    #[must_use]
    pub fn pane_in(
        mut self,
        cluster: mara_core::ribbon::RibbonCluster,
        id: &'static str,
        icon: &'static str,
        title: &'static str,
        anchor: mara_core::pane::PaneAnchor,
        body: impl FnOnce(&mut mara_core::pane::PaneBody<'_, 'spec>) + 'a,
    ) -> Self {
        self.panes.push(RibbonPane {
            id,
            title,
            icon,
            cluster,
            anchor,
            resize: mara_core::pane::PaneResize::SPAN,
            body: Box::new(body),
        });
        self
    }

    #[must_use]
    pub fn action(
        self,
        id: &'static str,
        icon: &'static str,
        tooltip: &'static str,
        action: mara_core::ribbon::RibbonAction,
    ) -> Self {
        self.action_in(
            mara_core::ribbon::RibbonCluster::End,
            id,
            icon,
            tooltip,
            action,
        )
    }

    #[must_use]
    pub fn action_in(
        mut self,
        cluster: mara_core::ribbon::RibbonCluster,
        id: &'static str,
        icon: &'static str,
        tooltip: &'static str,
        action: mara_core::ribbon::RibbonAction,
    ) -> Self {
        self.actions.push(RibbonActionButton {
            id,
            icon,
            tooltip,
            cluster,
            action,
        });
        self
    }
}

fn ribbon_cluster_for_pane_anchor(
    anchor: mara_core::pane::PaneAnchor,
) -> mara_core::ribbon::RibbonCluster {
    match anchor.zone() {
        mara_core::pane::RailZone::Start => mara_core::ribbon::RibbonCluster::Start,
        mara_core::pane::RailZone::Middle => mara_core::ribbon::RibbonCluster::Middle,
        mara_core::pane::RailZone::End => mara_core::ribbon::RibbonCluster::End,
    }
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

    /// Internal first-party accessor — exposes the raw egui-wgpu render
    /// state, so it is hidden and not semver-stable. Sealed consumers
    /// get GPU wiring through the published context state
    /// (`view_ctx` publishes the target format) instead.
    #[doc(hidden)]
    pub fn __internal_render_state(&self) -> Option<&'a egui_wgpu::RenderState> {
        self.render_state
    }

    /// Opaque GPU handle for GPU-module `show` calls (Bevy viewport,
    /// 3D). Sealed: the app passes it through without ever seeing the
    /// underlying egui-wgpu types (ADR 0002).
    #[must_use]
    pub fn gpu(&self) -> Option<mara_gpu::MaraRenderState<'a>> {
        self.render_state.map(mara_gpu::MaraRenderState::__internal_new)
    }

    /// Render the enforced shell top bar and return its events — the
    /// sealed path for hosts without a runner/plugin doing it for them
    /// (e.g. plain eframe/web shells). Wraps the bar's internal egui
    /// hook so the app never holds the backend context itself.
    pub fn show_shell_bar(
        &self,
        bar: &mut mara_core::ShellBar,
        open: &mut mara_core::RibbonOpen,
        placement: &mut mara_core::RibbonPlacement,
        drag: &mut mara_core::RibbonDrag,
    ) -> Vec<mara_core::ShellEvent> {
        bar.__internal_show_egui(self.egui, open, placement, drag)
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
    /// Shelves are reserved over the FULL content rect (top to bottom) so
    /// the side-shelf background extends up *behind* the enforced glass
    /// top bar — the part of the bar over a shelf shows the shelf
    /// background, the rest shows the view's own background.
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
        mara_core::enforce::__internal_enforce_defaults(self.egui);
        let accent = accent.into();
        // The body gets the FULL content rect (edge to edge, top to
        // bottom) so a root surface can paint full-bleed *behind* the
        // glass top bar — the canvas shows through the bar instead of a
        // flat panel colour sitting under it. The enforced top bar and the
        // shelves render over this body.
        #[allow(deprecated)]
        {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(egui::Color32::TRANSPARENT))
                .show(self.egui, |ui| {
                    let screen_rect = ui.max_rect().into();
                    let mut __raw = mara_core::MaraUi::__internal_backend_from_raw(ui);
                    let mut mui = mara_core::MaraUi::__internal_over(&mut __raw, accent);
                    body(&mut mui, screen_rect)
                })
                .inner
        }
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
        // App ribbon renders must NOT inject system chrome (window
        // controls + shelf toggles): the enforced `ShellBar` owns those.
        // Injecting here too renders them twice AND dispatches a single
        // click twice — for a shelf toggle that means it flips on then off
        // in the same interaction (net no-op), so the shelves never toggle.
        mara_core::ribbon::__internal_draw_slot_ribbons_featureful_no_system_egui(
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
        // Publish the render target format so GPU views (three_d, …) can
        // pull it from the context during `show` and be hosted as plain
        // `ViewNode` leaves, instead of needing a per-frame setter call.
        if let Some(state) = self.render_state {
            self.egui.data_mut(|d| {
                d.insert_temp(egui::Id::new("mara_gpu_target_format"), state.target_format);
            });
        }
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
        pane.__internal_show(self.egui, self.content_rect(), body);
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

    /// Apply the current Mara theme with default host state.
    ///
    /// A bare `mara::window::WindowApp` should look like Mara without
    /// app code manually calling `set_theme`/`apply_theme` every frame.
    /// The active theme itself still comes from
    /// [`mara_core::style::theme`], whose process default is PRO/Dark;
    /// apps can change that global theme and this method will apply it.
    pub fn apply_default_theme(&self) {
        self.apply_theme(
            mara_core::style::AccentColor(mara_core::style::raw_accent()),
            mara_core::style::glass_opacity(),
        );
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

    /// Explicit, deliberate opt-out from the enforced top bar **for
    /// this frame only**.
    ///
    /// The Mara top bar is enforced: if nothing renders a `ShellBar`,
    /// Mara renders a fallback. This is the single escape hatch, and it
    /// is intentionally a *repeated per-frame decision* — call it every
    /// frame the app should run bar-less (e.g. a kiosk view). Stop
    /// calling it and the bar comes back. Host runners honor it too:
    /// they skip their own bar render for an opted-out frame.
    pub fn opt_out_shell_bar(&self) {
        mara_core::enforce::__internal_opt_out_shell(self.egui);
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

impl MaraHostCtx<'_> {
    /// Show a high-level rail declaration.
    ///
    /// This paints open panes first, then the ribbon rail, because the
    /// ribbon must remain above panes. It also publishes the rail's pane
    /// ids before calling `show_pane`, so consumers cannot forget the
    /// registration handshake.
    pub fn show_ribbon_rail<'rail, 'spec>(
        &self,
        rail: RibbonRail<'rail, 'spec>,
        accent: impl Into<mara_core::vocab::Color32>,
    ) -> Vec<mara_core::ribbon::RibbonSlotClick> {
        let accent = accent.into();
        let key = egui::Id::new(("mara_host_ribbon_rail_state", rail.id));
        let mut state = self
            .egui
            .data_mut(|data| data.get_persisted::<HostRibbonRailState>(key))
            .unwrap_or_default();
        if !state.initialized {
            if let Some(pane_id) = rail.default_open {
                state.open.set(rail.id, pane_id);
            }
            state.initialized = true;
        }

        self.publish_ribbon_pane_ids(rail.panes.iter().map(|pane| pane.id));

        let mut resolved = Vec::new();
        for cluster in [
            mara_core::ribbon::RibbonCluster::Start,
            mara_core::ribbon::RibbonCluster::Middle,
            mara_core::ribbon::RibbonCluster::End,
        ] {
            let mut items = Vec::new();
            for pane in rail.panes.iter().filter(|pane| pane.cluster == cluster) {
                items.push(
                    mara_core::ribbon::RibbonSlotItem::featureful(
                        pane.id,
                        pane.icon,
                        pane.title,
                        pane.title,
                        mara_core::ribbon::RibbonAction::Command(mara_core::vocab::Id::new(
                            pane.id,
                        )),
                    )
                    .with_role(mara_core::ribbon::RibbonRole::Panel),
                );
            }
            for action in rail
                .actions
                .iter()
                .filter(|action| action.cluster == cluster)
            {
                items.push(
                    mara_core::ribbon::RibbonSlotItem::featureful(
                        action.id,
                        action.icon,
                        action.tooltip,
                        action.tooltip,
                        action.action,
                    )
                    .with_role(mara_core::ribbon::RibbonRole::Icon),
                );
            }
            if items.is_empty() {
                continue;
            }
            resolved.push(mara_core::ribbon::ResolvedSlotRibbon {
                id: mara_core::vocab::Id::new((rail.id, cluster)),
                chrome_id: Some(rail.id),
                scope: rail.scope,
                edge: rail.edge,
                role: mara_core::ribbon::RibbonRole::Panel,
                mode: rail.mode,
                cluster,
                accepts: rail.accepts,
                items,
            });
        }

        for pane in rail.panes {
            if state.open.is_open(rail.id, pane.id) {
                self.show_pane(
                    mara_core::pane::Pane::new(pane.id, pane.title, pane.anchor, accent)
                        .resize(pane.resize),
                    pane.body,
                );
            }
        }

        let clicks = self.draw_slot_ribbons_featureful(
            accent,
            &resolved,
            &mut state.open,
            &mut state.placement,
            &mut state.drag,
        );
        self.egui.data_mut(|data| data.insert_persisted(key, state));
        clicks
    }
}

#[cfg(test)]
mod tests {
    use super::RibbonRail;
    use mara_core::pane::{PaneAnchor, RailZone};
    use mara_core::ribbon::RibbonCluster;

    #[test]
    fn ribbon_rail_pane_places_button_in_anchor_zone_cluster() {
        let rail = RibbonRail::view_left("rail", "view")
            .pane(
                "start",
                "list",
                "Start",
                PaneAnchor::LeftRail(RailZone::Start),
                |_| {},
            )
            .pane(
                "middle",
                "options",
                "Middle",
                PaneAnchor::LeftRail(RailZone::Middle),
                |_| {},
            )
            .pane(
                "end",
                "save",
                "End",
                PaneAnchor::LeftRail(RailZone::End),
                |_| {},
            );

        assert_eq!(rail.panes[0].cluster, RibbonCluster::Start);
        assert_eq!(rail.panes[1].cluster, RibbonCluster::Middle);
        assert_eq!(rail.panes[2].cluster, RibbonCluster::End);
    }

    #[test]
    fn ribbon_rail_pane_in_keeps_explicit_cluster() {
        let rail = RibbonRail::view_left("rail", "view").pane_in(
            RibbonCluster::End,
            "start",
            "list",
            "Start",
            PaneAnchor::LeftRail(RailZone::Start),
            |_| {},
        );

        assert_eq!(rail.panes[0].cluster, RibbonCluster::End);
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
