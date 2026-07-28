use crate::layout::{AreaHost, Layer};
use crate::memory::MaraMemoryCtx;
use crate::mui::{MaraInput, MaraPainter, MaraUi};
use crate::pane::{Pane, PaneBody};
use crate::ribbon::{RibbonAvoidance, RibbonSlotClick, RibbonSlotDef, ribbon_avoiding_rect};
use crate::shelf::{__internal_show_shelves_egui, ShelfDef, ShelfLayout, ShelfState};
use crate::vocab::{Color32 as MaraColor32, Id as MaraId, Rect as MaraRect};
use crate::workspace::WorkspaceStack;

/// Rendering context for the active L0 view.
///
/// This is a sealed surface: views compose Mara panes, shelves,
/// pods, and the typed painter/body helpers below. The underlying
/// backend context is hidden behind first-party adapter hooks.
pub struct ViewCtx<'a> {
    /// The frame seam — everything a node does that is not yet waiting
    /// on a backend-typed callee goes through here.
    ///
    /// Owned rather than borrowed: the wrapper holds an `Arc` handle,
    /// so a node can lend a `&dyn MaraCtx` out of itself without the
    /// caller having to keep one alive alongside.
    #[doc(hidden)]
    pub seam: Box<dyn crate::context::MaraCtx + 'a>,
    /// The raw backend handle, kept only for the render entry points
    /// whose callees still take one (panes, shelves, leaf ribbons,
    /// texture upload, offscreen). Goes away with WS-G.
    #[cfg(feature = "backend-egui-conv")]
    #[doc(hidden)]
    pub egui_ctx: &'a egui::Context,
    pub workspace: &'a mut WorkspaceStack,
    pub accent: MaraColor32,
    pub content_avoidance: RibbonAvoidance,
    /// The rect this node renders into — the whole window for the root,
    /// or a cell rect for a child of a parent `Split`/`ViewNode`. Every
    /// method (painter, input, panes, body) scopes to this region, so a
    /// node is a self-contained surface (PLAN.md Phase 1 / ADR 0001).
    #[doc(hidden)]
    pub region: MaraRect,
}

/// Inset `region` by one ribbon rail's clearance on each edge that has a
/// ribbon (`[left, right, top, bottom]`).
fn shrink_region_by_ribbon_edges(region: MaraRect, edges: [bool; 4]) -> MaraRect {
    let c = crate::ribbon::ribbon_clearance();
    let [left, right, top, bottom] = edges;
    let pick = |on: bool| if on { c } else { 0.0 };
    MaraRect::from_min_max(
        crate::vocab::Pos2::new(region.min.x + pick(left), region.min.y + pick(top)),
        crate::vocab::Pos2::new(region.max.x - pick(right), region.max.y - pick(bottom)),
    )
}

impl<'a> ViewCtx<'a> {

    /// Build a child context scoped to a fixed `rect` (one cell of a
    /// [`ViewNode`](crate::ViewNode)), with its own `workspace`. Its
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
            seam: self.seam.boxed_clone(),
            #[cfg(feature = "backend-egui-conv")]
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
    #[cfg(feature = "backend-egui-conv")]
    pub fn __internal_egui_ctx(&self) -> &egui::Context {
        self.egui_ctx
    }

    /// The node's seam context. First-party hook for core code that
    /// needs frame state and would otherwise reach for the raw handle
    /// above just to hand it to something taking `&dyn MaraCtx`.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_seam_ctx(&self) -> &dyn crate::context::MaraCtx {
        self.seam.as_ref()
    }

    #[must_use]
    pub fn content_rect(&self) -> MaraRect {
        // The node's region, minus the ribbons this node drew itself
        // this pass (a leaf via `show_ribbons`): the leaf's rails are
        // children of the leaf, so the body insets from them inside the
        // region — wherever the region is, however small it gets.
        let mut rect = self.region;
        if let Some(edges) = crate::ribbon::slot_paint::view_ribbon_edges(
            self.seam.as_ref(),
            self.workspace.current().id,
        ) {
            rect = shrink_region_by_ribbon_edges(rect, edges);
        }
        // Then window-level rails: for the root, the intersection trims
        // the region under rails that actually exist; for an interior
        // cell the window rails fall outside the cell, so the
        // intersection is the cell itself.
        rect.intersect(ribbon_avoiding_rect(
            self.seam.as_ref(),
            self.content_avoidance,
        ))
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
        ribbon_avoiding_rect(self.seam.as_ref(), avoidance)
    }

    /// Per-frame input snapshot for custom view interaction, scoped to
    /// this node's region: pointer positions outside the region read as
    /// absent, so sibling cells only see the pointer over themselves.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        let mut snapshot = self.seam.input();
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
        self.seam.memory()
    }

    /// Current maximized-widget owner, if a maximizable Mara surface
    /// owns the full host content area this frame.
    #[must_use]
    pub fn fullscreen_owner(&self) -> Option<MaraId> {
        crate::embed::__internal_fullscreen_owner(self.seam.as_ref())
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
        crate::embed::__internal_set_fullscreen_minimize_chip_visible(self.seam.as_ref(), visible);
    }

    /// Restore the active full-window maximizable widget, if one
    /// exists. Returns `true` when a fullscreen owner was found and
    /// toggled off.
    pub fn restore_fullscreen(&self) -> bool {
        crate::embed::__internal_restore_fullscreen(self.seam.as_ref())
    }

    /// Typed painter over this node's region on the background layer —
    /// the view backdrop surface. Clipped to the region and keyed by the
    /// node's stable identity (its workspace id), so sibling cells paint
    /// distinct layers AND the layer keeps its z-slot when the region
    /// moves/resizes. Registered as a real area so later-opened
    /// Background panes stack ABOVE the backdrop, never behind it.
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        self.seam.layer_painter(
            Layer::Background,
            self.node_layer_id("mara_view_background"),
            self.region,
        )
    }

    /// Typed painter over this node's region on the foreground layer —
    /// for overlays above panes and shelves.
    #[must_use]
    pub fn overlay_painter(&self) -> MaraPainter {
        self.seam.layer_painter(
            Layer::Foreground,
            self.node_layer_id("mara_view_overlay"),
            self.region,
        )
    }

    /// A paint-layer id unique and STABLE per node (workspace-keyed, not
    /// region-keyed): sibling cells get distinct layers, and a cell keeps
    /// the same layer — and z-order slot — across moves and resizes.
    fn node_layer_id(&self, base: &str) -> MaraId {
        MaraId::new((base, self.workspace.current().id))
    }

    /// Lay a sealed widget surface over the view's content rect
    /// (the area not covered by ribbons).
    pub fn body<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        self.body_at(
            "mara_view_body",
            self.content_rect(),
            Layer::Background,
            body,
        )
    }

    /// Lay a sealed surface over an arbitrary rect at an explicit layer.
    ///
    /// This is the primitive for content that owns its own pixels —
    /// maps, 3D viewports, embedded renderers — and previously had to
    /// build a backend area by hand to get one. The surface is clipped
    /// and sized to `rect`, and the node's region stays published for
    /// the duration, so panes, ribbons and fullscreen inside `body`
    /// scope to this node rather than the window.
    ///
    /// `salt` distinguishes multiple surfaces owned by the same node —
    /// pass a per-instance value (two viewports in one view must not
    /// share an id). `layer` places the surface relative to chrome:
    /// [`Layer::Background`] for content that chrome draws over,
    /// [`Layer::Foreground`] for overlays.
    pub fn body_at<R>(
        &mut self,
        salt: impl std::hash::Hash,
        rect: impl Into<MaraRect>,
        layer: Layer,
        body: impl FnOnce(&mut MaraUi<'_>) -> R,
    ) -> R {
        let rect = rect.into();
        let region = self.region;
        let id = self.workspace.current().id.with(salt);
        let accent = self.accent;
        let ctx = self.seam.as_ref();
        crate::embed::__internal_with_node_region(ctx, region, || {
            // The seam's area body is `&mut dyn FnMut`, which can
            // neither consume a `FnOnce` nor carry a return value out —
            // so both travel through captures.
            let mut body = Some(body);
            let mut out = None;
            ctx.area(
                AreaHost::new(id, rect.min, layer).accent(accent),
                &mut |mara| {
                    mara.constrain_to(rect);
                    if let Some(body) = body.take() {
                        out = Some(body(mara));
                    }
                },
            );
            out.expect("view body surface did not run")
        })
    }

    /// Render this view node's own left/right/bottom ribbons, anchored to
    /// its region (a leaf owns its ribbons; a narrow cell gets its own
    /// rails). The top edge belongs to the shell bar, not a view, so pass
    /// only left/right/bottom ribbon defs here. Returns the clicks for the
    /// caller to dispatch (PLAN.md Phase 3).
    pub fn show_ribbons(&self, ribbons: &[RibbonSlotDef]) -> Vec<RibbonSlotClick> {
        crate::ribbon::slot_paint::__internal_draw_view_ribbons(
            self.seam.as_ref(),
            self.region,
            self.workspace.current().id,
            self.accent,
            ribbons,
        )
    }

    /// Show a floating/anchored pane. The closure receives the
    /// typed [`PaneBody`] — containers and pods only.
    pub fn show_pane<'spec>(&self, pane: Pane, body: impl FnOnce(&mut PaneBody<'_, 'spec>)) {
        let region = self.region;
        let ctx = self.seam.as_ref();
        crate::embed::__internal_with_node_region(ctx, region, || {
            pane.__internal_show(ctx, region, body);
        });
    }

    /// Paint all shelves and their typed containers.
    pub fn show_shelves(
        &self,
        layout: ShelfLayout,
        shelves: Vec<ShelfDef<'_>>,
        state: &mut ShelfState,
    ) {
        __internal_show_shelves_egui(self.seam.as_ref(), layout, shelves, state);
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
    ) -> Option<crate::vocab::TextureHandle> {
        self.seam.load_texture(name, image, options)
    }

    /// Render a sealed UI body into its own texture at an independent
    /// scale, and get back the texture to paint.
    ///
    /// The body runs in a surface whose rasterisation scale is `scale`,
    /// not the window's — so text stays crisp when the result is
    /// composited at a different zoom. Use it for zoomable editors,
    /// thumbnails, minimaps, or anything that wants "this UI, as an
    /// image".
    ///
    /// `salt` keys the retained surface: pass a stable per-instance
    /// value, because each surface owns a texture, a renderer and its
    /// own font atlas. Reusing a salt reuses that state; a changing
    /// salt leaks a surface per frame.
    ///
    /// Returns `None` when the surface cannot be prepared (degenerate
    /// size, or GPU allocation failure) — paint a fallback rather than
    /// assuming a texture.
    #[cfg(feature = "gpu")]
    pub fn offscreen(
        &mut self,
        salt: impl std::hash::Hash,
        gpu: mara_gpu::MaraRenderState<'_>,
        origin: impl Into<MaraRect>,
        scale: f32,
        mut body: impl FnMut(&mut MaraUi<'_>),
    ) -> Option<crate::vocab::TextureId> {
        let origin = origin.into();
        crate::backend::egui::render_offscreen(
            self.egui_ctx,
            gpu,
            self.workspace.current().id.with(salt),
            origin.size(),
            scale,
            self.accent,
            self.offscreen_input(origin),
            &mut body,
        )
    }

    /// Map this node's input into a surface drawn at `origin`.
    ///
    /// An offscreen surface has no window, so it sees no events unless
    /// they are forwarded — and only the caller knows where the
    /// composited texture ended up. Pointer positions become
    /// surface-local; a pointer outside `origin` reads as absent, so the
    /// surface does not react to clicks that landed elsewhere.
    #[cfg(feature = "gpu")]
    fn offscreen_input(&self, origin: MaraRect) -> crate::backend::egui::OffscreenInput {
        let snapshot = self.input();
        let pointer = snapshot
            .pointer
            .filter(|p| origin.contains(*p))
            .map(|p| crate::vocab::Pos2::new(p.x - origin.min.x, p.y - origin.min.y));
        crate::backend::egui::OffscreenInput {
            pointer,
            primary_down: snapshot.primary_down,
            secondary_down: snapshot.secondary_down,
            middle_down: snapshot.middle_down,
            scroll_delta: snapshot.scroll_delta,
            modifiers_shift: snapshot.modifiers_shift,
            modifiers_ctrl: snapshot.modifiers_ctrl,
            modifiers_alt: snapshot.modifiers_alt,
        }
    }

    /// Device pixels per logical point — the scale factor a view
    /// rendering into its own pixel buffer must size that buffer by.
    #[must_use]
    pub fn pixels_per_point(&self) -> f32 {
        self.seam.pixels_per_point()
    }

    /// Seconds since the host started, for time-based animation and
    /// throttling. Monotonic within a run; not a wall clock.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.seam.now()
    }

    /// Ask the host to schedule another frame.
    pub fn request_repaint(&self) {
        self.seam.request_repaint();
    }

    /// Ask the host to schedule a frame no later than `after` — for
    /// views polling off-thread work (tile fetches, decode queues).
    pub fn request_repaint_after(&self, after: std::time::Duration) {
        self.seam.request_repaint_after(after);
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
