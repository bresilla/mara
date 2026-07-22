use crate::backend;
use crate::layout::{AreaHost, Layer};
use crate::memory::MaraMemoryCtx;
use crate::mui::{MaraInput, MaraPainter, MaraUi};
use crate::pane::{Pane, PaneBody};
use crate::ribbon::{RibbonAvoidance, ribbon_avoiding_rect};
use crate::shelf::{__internal_show_shelves_egui, ShelfDef, ShelfLayout, ShelfState};
use crate::vocab::{Color32 as MaraColor32, Id as MaraId, Rect as MaraRect};
use crate::workspace::WorkspaceStack;

/// Rendering context for the active L0 view.
///
/// This is a sealed surface: views compose Mara panes, shelves,
/// pods, and the typed painter/body helpers below. The underlying
/// backend context is hidden behind first-party adapter hooks.
pub struct ViewCtx<'a> {
    pub(crate) egui_ctx: &'a egui::Context,
    pub workspace: &'a mut WorkspaceStack,
    pub accent: MaraColor32,
    pub content_avoidance: RibbonAvoidance,
    /// The rect this node renders into — the whole window for the root,
    /// or a cell rect for a child of a parent `Split`/`MultiView`. Every
    /// method (painter, input, panes, body) scopes to this region, so a
    /// node is a self-contained surface (PLAN.md Phase 1 / ADR 0001).
    region: MaraRect,
}

impl<'a> ViewCtx<'a> {
    /// Build a view context from the current egui backend.
    ///
    /// Hidden first-party hook: hosts/app code should use
    /// `MaraHostCtx::view_ctx` instead of passing raw backend context handles
    /// around. Sealed consumers receive a ready-made `ViewCtx` from the app
    /// shell / host facade.
    #[must_use]
    #[doc(hidden)]
    pub fn __internal_new(
        egui_ctx: &'a egui::Context,
        workspace: &'a mut WorkspaceStack,
        accent: impl Into<MaraColor32>,
        content_avoidance: RibbonAvoidance,
    ) -> Self {
        Self {
            region: backend::egui::context_content_rect(egui_ctx),
            egui_ctx,
            workspace,
            accent: accent.into(),
            content_avoidance,
        }
    }

    /// Build a child context scoped to a fixed `rect` (one cell of a
    /// [`MultiView`](crate::MultiView)), with its own `workspace`. Its
    /// `content_rect`/`screen_rect` report that rect, so the hosted view
    /// lays out inside the cell. First-party hook.
    #[must_use]
    #[doc(hidden)]
    pub fn __internal_scoped<'b>(
        &'b self,
        rect: MaraRect,
        workspace: &'b mut WorkspaceStack,
        accent: impl Into<MaraColor32>,
    ) -> ViewCtx<'b> {
        ViewCtx {
            egui_ctx: self.egui_ctx,
            workspace,
            accent: accent.into(),
            content_avoidance: RibbonAvoidance::none(),
            region: rect,
        }
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }

    #[must_use]
    pub fn content_rect(&self) -> MaraRect {
        // The node's region minus any ribbons overlapping it. For the
        // root, `region` is the whole window and the intersection yields
        // the ribbon-avoiding area; for an interior cell, the window
        // ribbons fall outside the cell so the intersection is the cell.
        self.region
            .intersect(ribbon_avoiding_rect(self.egui_ctx, self.content_avoidance))
    }

    /// The node's full rect — for views that paint an edge-to-edge
    /// backdrop behind the ribbons. The whole window for the root, or the
    /// cell rect for a scoped child.
    #[must_use]
    pub fn screen_rect(&self) -> MaraRect {
        self.region
    }

    #[must_use]
    pub fn ribbon_avoiding_rect(&self, avoidance: RibbonAvoidance) -> MaraRect {
        ribbon_avoiding_rect(self.egui_ctx, avoidance)
    }

    /// Per-frame input snapshot for custom view interaction, scoped to
    /// this node's region: pointer positions outside the region read as
    /// absent, so sibling cells only see the pointer over themselves.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        let mut snapshot = backend::egui::input_snapshot(self.egui_ctx);
        if snapshot.pointer.is_some_and(|p| !self.region.contains(p)) {
            snapshot.pointer = None;
        }
        if snapshot
            .interact_pointer
            .is_some_and(|p| !self.region.contains(p))
        {
            snapshot.interact_pointer = None;
        }
        snapshot
    }

    /// Backend-neutral memory facade for view-level UI state.
    #[must_use]
    pub fn memory(&self) -> MaraMemoryCtx<'_> {
        MaraMemoryCtx::new(self.egui_ctx)
    }

    /// Current maximized-widget owner, if a maximizable Mara surface
    /// owns the full host content area this frame.
    #[must_use]
    pub fn fullscreen_owner(&self) -> Option<MaraId> {
        crate::embed::__internal_fullscreen_owner(self.egui_ctx)
    }

    /// `true` when any maximizable Mara surface owns the full host
    /// content area this frame.
    #[must_use]
    pub fn is_any_fullscreen(&self) -> bool {
        self.fullscreen_owner().is_some()
    }

    /// Hide/show the built-in fullscreen restore chip for this
    /// frame. Host shells that provide their own persistent
    /// app/module bar can hide the floating chip and route restore
    /// through their normal chrome.
    pub fn set_fullscreen_minimize_chip_visible(&self, visible: bool) {
        crate::embed::__internal_set_fullscreen_minimize_chip_visible(self.egui_ctx, visible);
    }

    /// Restore the active full-window maximizable widget, if one
    /// exists. Returns `true` when a fullscreen owner was found and
    /// toggled off.
    pub fn restore_fullscreen(&self) -> bool {
        crate::embed::__internal_restore_fullscreen(self.egui_ctx)
    }

    /// Typed painter over this node's region on the background layer —
    /// the view backdrop surface. Clipped to the region and keyed by it,
    /// so sibling cells paint their own backdrops without colliding.
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        MaraPainter::new(backend::egui::context_painter_for_layer(
            self.egui_ctx,
            Layer::Background,
            self.region_layer_id("mara_view_background"),
            self.region,
        ))
    }

    /// Typed painter over this node's region on the foreground layer —
    /// for overlays above panes and shelves.
    #[must_use]
    pub fn overlay_painter(&self) -> MaraPainter {
        MaraPainter::new(backend::egui::context_painter_for_layer(
            self.egui_ctx,
            Layer::Foreground,
            self.region_layer_id("mara_view_overlay"),
            self.region,
        ))
    }

    /// A layer id unique to this node's region, so sibling cells get
    /// distinct paint layers.
    fn region_layer_id(&self, base: &str) -> MaraId {
        MaraId::new((
            base,
            self.region.min.x.to_bits(),
            self.region.min.y.to_bits(),
        ))
    }

    /// Lay a sealed widget surface over the view's content rect
    /// (the area not covered by ribbons).
    pub fn body<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let rect = self.content_rect();
        let id = self.workspace.current().id.with("mara_view_body");
        let accent = self.accent;
        backend::egui::show_area_for_host(
            self.egui_ctx,
            AreaHost::new(id, rect.min, Layer::Background),
            |ui| {
                backend::egui::constrain_ui_to_rect(ui, rect);
                body(&mut MaraUi::new(ui, accent))
            },
        )
        .inner
    }

    /// Show a floating/anchored pane. The closure receives the
    /// typed [`PaneBody`] — containers and pods only.
    pub fn show_pane<'spec>(&self, pane: Pane, body: impl FnOnce(&mut PaneBody<'_, 'spec>)) {
        pane.__internal_show(self.egui_ctx, self.region, body);
    }

    /// Paint all shelves and their typed containers.
    pub fn show_shelves(
        &self,
        layout: ShelfLayout,
        shelves: Vec<ShelfDef<'_>>,
        state: &mut ShelfState,
    ) {
        __internal_show_shelves_egui(self.egui_ctx, layout, shelves, state);
    }

    /// Upload an image as a managed texture. Returns the retained
    /// handle (vocab type — it can update its pixels but cannot
    /// reach the widget tree).
    #[must_use]
    pub fn load_texture(
        &self,
        name: &str,
        image: crate::vocab::ColorImage,
        options: crate::vocab::TextureOptions,
    ) -> crate::vocab::TextureHandle {
        let image: egui::ColorImage = image.into();
        self.egui_ctx
            .load_texture(name, image, options.into())
            .into()
    }

    /// Current responsive size class for this node's region. Views
    /// consult this to reflow on small surfaces (collapse chrome, stack
    /// panes) — a narrow cell reflows like a phone even on a wide window.
    #[must_use]
    pub fn breakpoint(&self) -> crate::style::Breakpoint {
        crate::style::Breakpoint::from_width(self.region.width())
    }

    /// Convenience: phone-class (the most aggressive reflow tier).
    #[must_use]
    pub fn is_compact(&self) -> bool {
        self.breakpoint().is_compact()
    }

    /// Convenience: phone or tablet (not the full desktop shell).
    #[must_use]
    pub fn is_handheld(&self) -> bool {
        self.breakpoint().is_handheld()
    }
}
