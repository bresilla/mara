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
            egui_ctx,
            workspace,
            accent: accent.into(),
            content_avoidance,
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
        ribbon_avoiding_rect(self.egui_ctx, self.content_avoidance)
    }

    /// The full window/screen rect — for views that paint an
    /// edge-to-edge backdrop behind the ribbons.
    #[must_use]
    pub fn screen_rect(&self) -> MaraRect {
        backend::egui::context_content_rect(self.egui_ctx)
    }

    #[must_use]
    pub fn ribbon_avoiding_rect(&self, avoidance: RibbonAvoidance) -> MaraRect {
        ribbon_avoiding_rect(self.egui_ctx, avoidance)
    }

    /// Per-frame input snapshot for custom view interaction.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        backend::egui::input_snapshot(self.egui_ctx)
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

    /// Typed painter over the full screen on the background layer —
    /// the view backdrop surface.
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        let rect = backend::egui::context_content_rect(self.egui_ctx);
        MaraPainter::new(backend::egui::context_painter_for_layer(
            self.egui_ctx,
            Layer::Background,
            MaraId::new("mara_view_background"),
            rect,
        ))
    }

    /// Typed painter over the full screen on the foreground layer —
    /// for overlays above panes and shelves.
    #[must_use]
    pub fn overlay_painter(&self) -> MaraPainter {
        let rect = backend::egui::context_content_rect(self.egui_ctx);
        MaraPainter::new(backend::egui::context_painter_for_layer(
            self.egui_ctx,
            Layer::Foreground,
            MaraId::new("mara_view_overlay"),
            rect,
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
        pane.__internal_show(self.egui_ctx, body);
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

    /// Current responsive size class for this frame. Views consult
    /// this to reflow on small screens (collapse chrome, stack panes).
    #[must_use]
    pub fn breakpoint(&self) -> crate::style::Breakpoint {
        crate::style::screen_class()
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
