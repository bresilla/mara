use egui::Color32;

use crate::mui::{MaraInput, MaraPainter, MaraUi};
use crate::pane::{Pane, PaneBody};
use crate::ribbon::{RibbonAvoidance, ribbon_avoiding_rect};
use crate::shelf::{ShelfDef, ShelfLayout, ShelfState, show_shelves};
use crate::workspace::WorkspaceStack;

/// Rendering context for the active L0 view.
///
/// This is a sealed surface: views compose Mara panes, shelves,
/// pods, and the typed painter/body helpers below. The underlying
/// `egui::Context` is only reachable behind the `raw-egui` feature.
pub struct ViewCtx<'a> {
    pub(crate) egui_ctx: &'a egui::Context,
    pub workspace: &'a mut WorkspaceStack,
    pub accent: Color32,
    pub content_avoidance: RibbonAvoidance,
}

impl<'a> ViewCtx<'a> {
    /// Build a view context. Hosts own the egui pass, so calling
    /// this requires already holding an `egui::Context` — sealed
    /// consumers receive a ready-made `ViewCtx` from the app shell
    /// instead.
    #[must_use]
    pub fn new(
        egui_ctx: &'a egui::Context,
        workspace: &'a mut WorkspaceStack,
        accent: Color32,
        content_avoidance: RibbonAvoidance,
    ) -> Self {
        Self {
            egui_ctx,
            workspace,
            accent,
            content_avoidance,
        }
    }

    /// The raw `egui::Context`. Raw-egui escape hatch.
    #[cfg(feature = "raw-egui")]
    #[must_use]
    pub fn egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable. See `MaraUi::__internal_raw_ui` for
    /// why first-party crates use this instead of `raw-egui`.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }

    #[must_use]
    pub fn content_rect(&self) -> egui::Rect {
        ribbon_avoiding_rect(self.egui_ctx, self.content_avoidance)
    }

    /// The full window/screen rect — for views that paint an
    /// edge-to-edge backdrop behind the ribbons.
    #[must_use]
    pub fn screen_rect(&self) -> egui::Rect {
        self.egui_ctx.content_rect()
    }

    #[must_use]
    pub fn ribbon_avoiding_rect(&self, avoidance: RibbonAvoidance) -> egui::Rect {
        ribbon_avoiding_rect(self.egui_ctx, avoidance)
    }

    /// Per-frame input snapshot for custom view interaction.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        MaraInput::snapshot(self.egui_ctx)
    }

    /// Typed painter over the full screen on the background layer —
    /// the view backdrop surface.
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        let rect = self.egui_ctx.content_rect();
        MaraPainter::new(egui::Painter::new(
            self.egui_ctx.clone(),
            egui::LayerId::background(),
            rect,
        ))
    }

    /// Typed painter over the full screen on the foreground layer —
    /// for overlays above panes and shelves.
    #[must_use]
    pub fn overlay_painter(&self) -> MaraPainter {
        let rect = self.egui_ctx.content_rect();
        MaraPainter::new(egui::Painter::new(
            self.egui_ctx.clone(),
            egui::LayerId::new(egui::Order::Foreground, egui::Id::new("mara_view_overlay")),
            rect,
        ))
    }

    /// Lay a sealed widget surface over the view's content rect
    /// (the area not covered by ribbons).
    pub fn body<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let rect = self.content_rect();
        let id = self.workspace.current().id.with("mara_view_body");
        let accent = self.accent;
        egui::Area::new(id)
            .order(egui::Order::Background)
            .fixed_pos(rect.min)
            .show(self.egui_ctx, |ui| {
                ui.set_clip_rect(rect);
                ui.set_min_size(rect.size());
                ui.set_max_size(rect.size());
                body(&mut MaraUi::new(ui, accent))
            })
            .inner
    }

    /// Show a floating/anchored pane. The closure receives the
    /// typed [`PaneBody`] — containers and pods only.
    pub fn show_pane<'spec>(&self, pane: Pane, body: impl FnOnce(&mut PaneBody<'_, 'spec>)) {
        pane.show(self.egui_ctx, body);
    }

    /// Paint all shelves and their typed containers.
    pub fn show_shelves(
        &self,
        layout: ShelfLayout,
        shelves: Vec<ShelfDef<'_>>,
        state: &mut ShelfState,
    ) {
        show_shelves(self.egui_ctx, layout, shelves, state);
    }

    /// Upload an image as a managed texture. Returns the retained
    /// handle (vocab type — it can update its pixels but cannot
    /// reach the widget tree).
    #[must_use]
    pub fn load_texture(
        &self,
        name: &str,
        image: egui::ColorImage,
        options: egui::TextureOptions,
    ) -> egui::TextureHandle {
        self.egui_ctx.load_texture(name, image, options)
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
