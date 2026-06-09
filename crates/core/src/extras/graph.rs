//! Node-graph integration — thin glue around [`egui_graph`] so graph
//! widgets inherit the mara palette and border language without
//! every consumer having to hand-tune a `GraphStyle`.
//!
//! Two pieces of surface:
//!
//! * [`mara_node_graph_style`] — builds a [`GraphStyle`] configured with
//!   mara's `BG_*` / `widget_border` / accent colours, the same
//!   corner radius as [`section`](crate::widget::foldable::section),
//!   and a pin/wire width that matches the border stroke. Pass the
//!   returned style straight into
//!   [`GraphWidget::style`](egui_graph::ui::GraphWidget::style).
//! * `pub use egui_graph` re-export — callers don't need a second
//!   direct dep. `use bevy_mara::graph::{Graph, NodeViewer,
//!   GraphWidget, NodeId, InPin, OutPin, ...};` lands the full
//!   upstream surface.
//!
//! Drop the whole thing into any section body:
//!
//! ```ignore
//! section(ui, "graph", "Graph", accent, true, |ui| {
//!     GraphWidget::new()
//!         .id_salt("my_graph")
//!         .style(mara_node_graph_style(accent))
//!         .min_size(egui::vec2(320.0, 260.0))
//!         .show(&mut state.graph, &mut state.viewer, ui);
//! });
//! ```

use egui;

pub use mara_graph::{
    AnyPins, BackgroundPattern, Dots, Graph, GraphState, GraphStyle, GraphWidget, Grid, Hex, InPin,
    InPinId, NodeHalo, NodeId, NodeLayout, NodePin, NodeViewBackend, NodeViewState, NodeViewer,
    OutPin, OutPinId, PinInfo, PinPlacement, PinShape, WireColorMode,
};

// `mara_node_graph` / `mara_node_graph_with_opts` route through
// `crate::embed::maximizable_with_opts` for the fullscreen chip
// + overlay swap. `OverlayOpts` is re-exported so callers pick up
// the chip-placement type from the same module.
pub use crate::embed::OverlayOpts;
use crate::style::{
    FrameRole, GraphCanvasPattern, RadiusRole, StrokeRole, frame_for, glass_alpha_window,
    glass_fill, radius_for, stroke_for,
};

/// Build a [`GraphStyle`] that inherits the mara palette + border
/// language. Call per-frame with the current accent so the graph
/// re-tints when the user swaps accent colour (the same way every
/// other mara surface does).
///
/// What the returned style pins down:
///
/// * **Node frame** — `BG_2_RAISED` glass fill + `widget_border`
///   stroke + `crate::style::theme().radius_md` corner, matching
///   [`section`](crate::widget::foldable::section) so nodes look
///   like first-class mara surfaces.
/// * **Background** — `BG_1_PANEL` glass fill behind everything,
///   the same colour a floating window uses, so the graph canvas
///   sits cleanly in an editor panel.
/// * **Pins / wires** — `widget_border(accent)` + stroke width 1 px,
///   identical to every other widget's edge.
///
/// Everything else stays at the library default so scroll / zoom /
/// selection interactions remain familiar to upstream users.
pub fn mara_node_graph_style(accent: egui::Color32) -> GraphStyle {
    // ── Blender-style geometry ──
    // Blender (4.x) measures all node geometry off `widget_unit = 20 px`:
    //   * NODE_DY (header height, row height) = widget_unit = 20 px
    //   * BASIS_RAD (corner radius)           = 0.2 × widget_unit = 4 px
    //   * NODE_MARGIN_X (header text indent)  = 1.2 × widget_unit = 24 px
    //   * NODE_DYS (half-row, gutter)         = widget_unit / 2  = 10 px
    //   * NODE_SOCKSIZE (pin radius)          = 0.25 × widget_unit = 5 px
    // We mirror those constants so the node geometry feels
    // proportionally identical, with mara's glass-fill background.
    // Horizontal padding shared by body AND header so the header
    // band lines up with the body edges (graph sizes each frame as
    // content + 2 × inner_margin, so any divergence here makes the
    // header poke out like a hat).
    let graph = crate::style::theme().graph;

    // Body uses the mara section recipe — same fill, border and
    // corner radius every foldable section / container in the
    // kit uses, so a node and a section sit at the same visual
    // tier instead of looking like a separate widget family.
    //
    //   * `section_fill(accent)` resolves through the active
    //     theme's `section_fill_mode` (dark in PRO, accent-tinted
    //     in GAME); `glass_fill` then layers the user's chosen
    //     glass tint on top.
    //   * `widget_border(accent)` is the same edge stroke a
    //     button / dropdown / search input renders.
    //   * `theme().radius_md` matches the container corner radius
    //     (PRO 6 px, GAME 0 px square).
    let node_frame = frame_for(FrameRole::Section, accent)
        .inner_margin(egui::Margin::symmetric(graph.node_pad_x, graph.node_pad_y));
    let body_radius = crate::style::theme().shape.radius_md;

    // Header — TRANSPARENT here. The category-coloured band is
    // painted PER-NODE inside `NodeViewer::show_header` (see the
    // demo's `show_header` impl) by reading the node's category +
    // smearing a Unreal-style left-anchored gradient across the
    // header rect. That keeps `mara_node_graph_style` host-agnostic
    // (no fixed colour palette baked in) and lets each app's
    // viewer decide which colour to spill.
    let header_frame = egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(egui::CornerRadius {
            nw: body_radius,
            ne: body_radius,
            sw: 0,
            se: 0,
        })
        .inner_margin(egui::Margin {
            left: graph.node_pad_x,
            right: graph.node_pad_x,
            top: graph.node_pad_y,
            bottom: graph.node_pad_y,
        });

    // Background mirrors the code-editor recipe — `pane_fill(accent)`
    // routes through the theme so GAME's accent-tinted dark and
    // PRO's neutral `bg_panel` both flow in here automatically. The
    // node graph and the code editor now visually share the same
    // canvas surface.
    let canvas_base = crate::style::pane_fill(accent);
    let bg_fill = glass_fill(canvas_base, accent, glass_alpha_window());

    // Grid stroke — `contrast_text_for(canvas_base)` at low alpha so
    // the pattern is automatically lighter than the bg on a dark
    // canvas, darker on a light one. Alpha 28 keeps it firmly in
    // the "there but quiet" tier: visible enough to read as a grid,
    // not loud enough to compete with the nodes.
    let grid_base = crate::style::contrast_text_for(canvas_base);
    let grid_stroke = egui::Stroke::new(
        1.0,
        egui::Color32::from_rgba_unmultiplied(
            grid_base.r(),
            grid_base.g(),
            grid_base.b(),
            graph.grid_alpha,
        ),
    );

    let bg_pattern = match graph.canvas_pattern {
        GraphCanvasPattern::Dots { spacing, radius } => {
            BackgroundPattern::Dots(Dots::new(egui::vec2(spacing, spacing), radius))
        }
        GraphCanvasPattern::Hex { radius } => BackgroundPattern::Hex(Hex::new(radius)),
    };

    GraphStyle {
        node_frame: Some(node_frame),
        header_frame: Some(header_frame),
        bg_frame: Some(
            egui::Frame::new()
                .fill(bg_fill)
                .stroke(stroke_for(StrokeRole::WidgetBorder, accent))
                .corner_radius(radius_for(RadiusRole::Pane))
                .inner_margin(egui::Margin::same(graph.bg_inner_margin)),
        ),
        // Canvas pattern is theme-driven:
        //   PRO  → Blender-style dot grid (30-px pitch, 1-px radius)
        //          — large enough to read as a grid when zoomed out,
        //          quiet enough to disappear behind nodes.
        //   GAME → pointy-top hex tessellation (24-px circumradius)
        //          — sci-fi HUD motif (Halo waypoint, Stellaris).
        bg_pattern: Some(bg_pattern),
        bg_pattern_stroke: Some(grid_stroke),
        // Pin defaults — overridden per-node-type by the demo's
        // `PinType::pin()` builder. Blender uses a 1-px black
        // outline on every socket; mirrored here.
        pin_fill: Some(crate::style::on_section()),
        pin_stroke: Some(egui::Stroke::new(
            graph.pin_stroke_width,
            egui::Color32::from_black_alpha(graph.pin_stroke_alpha),
        )),
        // Wires — Blender uses 2.5 px width with a 1-px dark
        // outline pass underneath; egui-graph draws a single
        // stroke, so we settle on 2.0 px (UE Blueprints' default
        // 1.5 px felt too thin against the dot grid).
        wire_width: Some(graph.wire_width),
        wire_style: None,
        // Unreal-Blueprints wire colour rule — the wire takes the
        // OUTPUT (source) pin's colour uniformly along its length,
        // not a gradient between source and target. The "wire shows
        // the type that's flowing out" idiom — much easier to
        // read than Blender's interpolated mix when you're
        // following a wire from its origin.
        wire_color_mode: Some(WireColorMode::FromSource),
        // Faux-bloom — wires and pins shed a soft halo in their
        // type colour. Driven by `theme().graph_wire_glow` /
        // `graph_pin_glow` so PRO stays "vibrant but tasteful"
        // (~0.6 / 0.5) while GAME ramps to a full neon halo
        // (~1.0 / 0.85). Layered alpha-reduced strokes under
        // the crisp wire give a "post-process bloom" feel
        // without an actual GPU pass.
        wire_glow: Some(graph.wire_glow),
        pin_glow: Some(graph.pin_glow),
        // Pin glyph centre sits ON the body's border line — the
        // pin bisects the outline, half inside / half outside.
        // Reads as "above" / sitting on the border the way
        // Blender + Unreal node editors do, with the wire
        // arriving at the body's edge rather than past it.
        pin_placement: Some(PinPlacement::Edge),
        pin_inset: None,
        // Accent halo close to the body, painted UNDER pin
        // glyphs (graph reserves the painter slot before pins
        // submit, so pins always render on top of the halo
        // line). 3 px gap, 1.5 px stroke.
        node_halo: Some(NodeHalo {
            color: accent,
            gap: graph.node_halo_gap,
            width: graph.node_halo_width,
            // Halo follows the body's rounded corners — body
            // radius + a bit of slack for the outset.
            radius: body_radius.saturating_add(graph.node_halo_radius_outset),
        }),
        downscale_wire_frame: Some(true),
        upscale_wire_frame: Some(true),
        // ── Outside-in zoom ──
        // Lock graph's internal `TSTransform.scaling` to 1.0 so it
        // never stretches the rasterised glyphs (a bitmap atlas
        // scaled past 1.0 is the source of the bilinear blur on
        // zoom). Zoom is instead driven from the outside in
        // `node_view::show`, which grows the secondary egui
        // context's `pixels_per_point` AND shrinks its
        // `screen_rect` proportionally — the atlas re-rasterises
        // glyphs at the new pixel resolution AND the layout area
        // shrinks so nodes appear bigger. End result: text stays
        // sharp at any zoom level.
        // Graph's pan (TSTransform.translation) is still managed
        // internally by drag-pan inside the widget; only the zoom
        // axis is hijacked.
        min_scale: Some(1.0),
        max_scale: Some(1.0),
        // No collapse arrow on the header. Folding is rarely
        // needed and the right-click context menu already handles
        // it; freeing the right edge keeps the title bar clean.
        collapsible: Some(false),
        // Zero the leading drag-space padding — by default graph
        // allocates an `icon_width × icon_width` (~16×16 px) hover
        // strip before `show_header`. That pushes our icon away
        // from the left edge of the header band and looks broken
        // next to a per-category coloured fill.
        header_drag_space: Some(egui::vec2(0.0, 0.0)),
        ..GraphStyle::new()
    }
}

/// Render the graph widget with a built-in **maximise / restore**
/// toggle in its top-left corner.
///
/// The maximise state is scoped to THIS graph — clicking the icon
/// lifts only the graph into a full-window overlay, leaving the
/// floating panel and any outer container the caller placed it in
/// completely untouched. Click again to restore.
///
/// When maximised the caller-supplied `min_size` still allocates
/// in-place so the section / panel layout doesn't collapse while
/// the graph is "gone" to the overlay — the hole is filled with a
/// small "(maximised)" caption.
///
/// Render an `egui-graph` node graph through mara's sharp-zoom
/// pipeline: a SECONDARY `egui::Context` with `pixels_per_point`
/// compensated for zoom, painted into a wgpu texture by the
/// [`NodeViewBackend`] and composited back into the parent UI. The
/// graph stays sharp at any zoom level (text + shape edges
/// rasterise at the zoomed size, never up-scaled) and stays
/// host-agnostic — the backend trait has impls for Bevy
/// (`bevy_mara::node_view_backend::BevyNodeViewBackend`) and the
/// unified `mara` window host (`mara::host::EframeNodeViewBackend`).
///
/// `state` carries the per-graph camera (`pan`, `zoom`) plus the
/// secondary egui context and wgpu texture across frames; pass the
/// SAME `NodeViewState` each frame for the same graph instance.
///
/// Use this in place of [`GraphWidget::new().show`] whenever you
/// want the mara styling + the fullscreen affordance. The
/// fullscreen chip lands top-right by default;
/// [`mara_node_graph_with_opts`] takes an [`OverlayOpts`] for custom
/// chip placement.
pub fn mara_node_graph<T, V: NodeViewer<T>>(
    ui: &mut egui::Ui,
    state: &mut NodeViewState,
    backend: &mut dyn NodeViewBackend,
    graph: &mut Graph<T>,
    viewer: &mut V,
    accent: egui::Color32,
    desired_size: egui::Vec2,
) {
    mara_node_graph_with_opts(
        ui,
        state,
        backend,
        graph,
        viewer,
        accent,
        desired_size,
        OverlayOpts::default(),
    )
}

/// Like [`mara_node_graph`] but accepts [`OverlayOpts`] so the caller
/// The maximise-state key the node-graph wrapper registers with
/// [`crate::embed`]. Compare against
/// [`crate::embed::fullscreen_owner`] to detect "is the graph the
/// one currently in fullscreen?" — useful when the host wants to
/// paint graph-specific chrome (toolbar / category sidebar /
/// status line) on top of the maximised canvas using its normal
/// ribbon assembly.
#[must_use]
pub fn graph_fullscreen_key() -> egui::Id {
    crate::embed::maximize_state_key(egui::Id::new("mara_node_graph_widget"))
}

/// `true` while the node-graph widget is currently in its
/// fullscreen overlay. Shorthand for
/// `fullscreen_owner(ctx) == Some(graph_fullscreen_key())`.
#[must_use]
pub fn is_graph_fullscreen(ctx: &egui::Context) -> bool {
    crate::embed::fullscreen_owner(ctx) == Some(graph_fullscreen_key())
}

/// picks where the fullscreen / minimize chip lands on the overlay
/// (which edge + which cluster along that edge).
#[allow(clippy::too_many_arguments)]
pub fn mara_node_graph_with_opts<T, V: NodeViewer<T>>(
    ui: &mut egui::Ui,
    state: &mut NodeViewState,
    backend: &mut dyn NodeViewBackend,
    graph: &mut Graph<T>,
    viewer: &mut V,
    accent: egui::Color32,
    desired_size: egui::Vec2,
    fs_opts: OverlayOpts,
) {
    let id_for_graph_base = egui::Id::new("mara_node_graph_widget");
    // Auto-recentre bookkeeping. The `version` is folded into the
    // GraphWidget's id below; bumping it invalidates egui-graph's
    // saved transform so `GraphState::initial` runs again and
    // refits the bb to the live viewport. We bump on first paint
    // (no `last_sz` yet) and whenever the viewport size drifts
    // more than `RESIZE_THRESHOLD` from the last fit — pane drags,
    // maximise / restore, and fullscreen toggles all easily cross
    // that threshold; per-pixel render jitter does not. We also
    // keep bumping for `SETTLE_FRAMES` extra frames after a
    // natural trigger so the eventual layout (often resolved a
    // frame or two AFTER the size change) gets fit instead of the
    // mid-resolve rect.
    const RESIZE_THRESHOLD: f32 = 8.0;
    const SETTLE_FRAMES: u32 = 2;
    let version_id = ui.id().with(("mara_node_graph_version", id_for_graph_base));
    let last_sz_id = ui.id().with(("mara_node_graph_last_sz", id_for_graph_base));
    let settle_id = ui.id().with(("mara_node_graph_settle", id_for_graph_base));

    // `maximizable_with_opts` paints the maximize chip and, when
    // active, swaps to a fullscreen body — its body callback gets
    // a `&mut Ui` whose `available_size()` is either the inline
    // pod rect or the full window. Use that as the sharp-zoom
    // target size so the secondary egui context renders at the
    // exact pixel dimensions of whichever surface owns the pane
    // this frame.
    crate::embed::maximizable_with_opts(
        ui,
        id_for_graph_base,
        accent,
        desired_size,
        fs_opts,
        |inner_ui| {
            let size = inner_ui.available_size();
            // Sub-context theme bridge — `mara_graph::show_with_anchor`
            // is theme-neutral, so we install fonts + apply the active
            // mara theme onto the secondary context here. First-frame
            // install is one-shot; theme apply runs each frame so a
            // mid-session theme swap re-tints the sub-context too.
            if state.take_first_frame() {
                crate::style::install_fonts(
                    state.ctx(),
                    crate::style::font_weight(),
                    crate::style::title_weight(),
                );
            }
            crate::style::apply_theme_to(
                state.ctx(),
                crate::style::AccentColor(crate::style::active_accent()),
                crate::style::glass_opacity(),
            );

            let parent_ctx = inner_ui.ctx().clone();
            let mut version: u32 = parent_ctx.data(|d| d.get_temp(version_id)).unwrap_or(0);
            let last_sz: Option<egui::Vec2> =
                parent_ctx.data(|d| d.get_temp::<egui::Vec2>(last_sz_id));
            let settle_left: u32 = parent_ctx
                .data(|d| d.get_temp::<u32>(settle_id))
                .unwrap_or(0);
            let size_usable = size.x >= 10.0 && size.y >= 10.0;
            let natural_bump = size_usable
                && match last_sz {
                    None => true,
                    Some(prev) => {
                        let dx = (size.x - prev.x).abs();
                        let dy = (size.y - prev.y).abs();
                        dx > RESIZE_THRESHOLD || dy > RESIZE_THRESHOLD
                    }
                };
            let settle_bump = size_usable && settle_left > 0;
            let should_bump = natural_bump || settle_bump;
            if should_bump {
                version = version.wrapping_add(1);
            }
            let new_settle = if natural_bump {
                SETTLE_FRAMES
            } else {
                settle_left.saturating_sub(1)
            };
            if size_usable {
                parent_ctx.data_mut(|d| {
                    d.insert_temp::<u32>(version_id, version);
                    d.insert_temp::<egui::Vec2>(last_sz_id, size);
                    d.insert_temp::<u32>(settle_id, new_settle);
                });
                if should_bump || new_settle > 0 {
                    parent_ctx.request_repaint();
                }
            }
            // On a real resize (maximise / restore / pane drag),
            // reset our outside-in `state.zoom` to 1.0 so the
            // re-fit pass below renders at native scale and the
            // whole graph fits the new viewport. Without this the
            // user's previous zoom level (e.g. zoomed-in 3× while
            // maximised) carries over to the smaller inline rect
            // and the graph stays cropped.
            if natural_bump {
                state.set_zoom(1.0);
            }
            // Versioned graph id — bumping version forces a fresh
            // fit because the saved GraphStateData lookup misses.
            let id_for_graph = id_for_graph_base.with(version);

            mara_graph::show_with_anchor(
                inner_ui,
                state,
                backend,
                size,
                // Cursor-anchor the wheel zoom by nudging graph's
                // saved `TSTransform.translation` by the same
                // sub-points delta `node_view::show_with_anchor`
                // computes — applied here BEFORE the graph widget
                // runs in the body callback below, so graph's
                // first `GraphState::load` of this frame picks up
                // the updated translation and the scene point
                // under the cursor stays under the cursor.
                |sub_ctx, delta| {
                    mara_graph::GraphState::nudge_saved_translation(sub_ctx, id_for_graph, delta);
                },
                |sub_ui| {
                    GraphWidget::new()
                        .id(id_for_graph)
                        .style(mara_node_graph_style(accent))
                        .min_size(size)
                        .show(graph, viewer, sub_ui);
                },
            );
        },
    );
}

// ─── Typed PaneBody constructor ─────────────────────────────────────
//
// Adds `PaneBody::add_node_graph(...)` so pane bodies can host a
// node graph as a first-class container without reaching into
// `ContainerSpec::raw_internal` directly. The graph closure needs
// to borrow `&mut NodeViewState` / `&mut Graph` / `&mut Viewer` /
// `&mut dyn NodeViewBackend` (all non-`'static`), which is why this
// goes through the crate-internal `raw_internal` escape — but the
// closure is fully owned by mara_core, so external callers cannot
// smuggle arbitrary egui code through it.

impl<'ui, 'spec> crate::pane::PaneBody<'ui, 'spec> {
    /// Append a mara-themed node-graph container to the pane.
    /// The graph borrows `state` / `graph` / `viewer` / `backend`
    /// for the duration of THIS call (they don't have to outlive
    /// the pane closure).
    ///
    /// **Reorder trade-off:** the graph paints inline at this
    /// point in the pane body closure, *not* through the
    /// deferred-pending pipeline that drives drag-reorder for
    /// `add_normal` / `add_tabbed`. The graph's position relative
    /// to other containers in the pane is therefore fixed by
    /// declaration order — drag-reorder works between any
    /// `add_normal`/`add_tabbed` containers, but not against the
    /// graph. This is the only way to support the non-`'static`
    /// borrows the graph needs.
    #[allow(clippy::too_many_arguments)]
    pub fn add_node_graph<T, V>(
        &mut self,
        id: impl Into<egui::Id>,
        title: impl Into<String>,
        icon: &'static str,
        state: &'spec mut NodeViewState,
        graph: &'spec mut Graph<T>,
        viewer: &'spec mut V,
        backend: &'spec mut dyn NodeViewBackend,
    ) -> &mut Self
    where
        V: NodeViewer<T>,
        T: 'spec,
    {
        // Enqueue as a `ContainerSpec::raw_internal` so the graph
        // participates in the same drag-reorder flow as `add_normal`
        // / `add_tabbed` (snapshot push, inline ghost gap, section
        // order persistence). The non-`'static` borrows are tied to
        // `'spec` — the lifetime of the `PaneBody`, bounded by the
        // surrounding `Pane::show` call — so the host MUST keep
        // `state` / `graph` / `viewer` / `backend` alive at least
        // that long. The Bevy demo does this by lifting them above
        // the `Pane::show` call (see `editor_pane`'s call site).
        self.add(crate::pane::ContainerSpec::raw_internal(
            id,
            title,
            icon,
            move |inner_ui| {
                let avail = inner_ui.available_size_before_wrap();
                let accent = crate::style::active_accent();
                mara_node_graph(inner_ui, state, backend, graph, viewer, accent, avail);
            },
        ))
    }
}

// ─── View + Module bridge ──────────────────────────────────────────
//
// The sharp offscreen node-view path above remains available for host
// demos that can provide a backend. This retained surface uses the
// plain egui graph widget so it can implement PLAN.md's View + Module
// traits without storing host-specific renderer state.

/// A retained node-graph surface that can be routed as a top-level
/// [`crate::MaraView`] or embedded as a [`crate::MaraModule`].
#[derive(Clone, Debug)]
pub struct GraphSurface<T, V> {
    id: egui::Id,
    title: String,
    graph: Graph<T>,
    viewer: V,
    units: usize,
}

impl<T, V> GraphSurface<T, V> {
    #[must_use]
    pub fn new(
        id: impl std::hash::Hash,
        title: impl Into<String>,
        graph: Graph<T>,
        viewer: V,
    ) -> Self {
        Self {
            id: egui::Id::new(id),
            title: title.into(),
            graph,
            viewer,
            units: 14,
        }
    }

    #[must_use]
    pub fn graph(&self) -> &Graph<T> {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut Graph<T> {
        &mut self.graph
    }

    #[must_use]
    pub fn viewer(&self) -> &V {
        &self.viewer
    }

    pub fn viewer_mut(&mut self) -> &mut V {
        &mut self.viewer
    }

    #[must_use]
    pub fn with_units(mut self, units: usize) -> Self {
        self.units = units.max(1);
        self
    }
}

impl<T, V> GraphSurface<T, V>
where
    V: NodeViewer<T>,
{
    fn show_graph(&mut self, ui: &mut egui::Ui) {
        let size = ui.available_size_before_wrap();
        GraphWidget::new()
            .id(self.id)
            .style(mara_node_graph_style(crate::style::active_accent()))
            .min_size(size)
            .show(&mut self.graph, &mut self.viewer, ui);
    }

    fn toolbar(&self, scope: crate::RibbonScope) -> crate::RibbonSlotDef {
        let add_node = crate::RibbonSlotItem::new(
            egui::Id::new(("graph.add_node", self.id)),
            "add",
            "Add Node",
            "Add a graph node",
            crate::RibbonAction::Command(egui::Id::new(("graph.add_node.command", self.id))),
        );
        crate::RibbonSlotDef::new(
            egui::Id::new(("graph.ribbon", self.id)),
            scope,
            crate::RibbonEdge::Top,
            crate::RibbonCluster::Middle,
            vec![crate::RibbonSlot::new(
                crate::RibbonSlotId::new(("graph.add_node.slot", self.id)),
                Some(add_node),
                crate::RibbonOverridePolicy::Fixed,
            )],
        )
    }
}

impl<T, V> crate::MaraView for GraphSurface<T, V>
where
    V: NodeViewer<T>,
{
    fn id(&self) -> crate::ViewId {
        crate::ViewId(self.id)
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "node_tree"
    }

    fn ribbons(&mut self) -> Vec<crate::RibbonSlotDef> {
        vec![self.toolbar(crate::RibbonScope::View(crate::ViewId(self.id)))]
    }

    fn show(&mut self, ctx: &mut crate::ViewCtx<'_>) {
        egui::CentralPanel::default().show(ctx.egui_ctx, |ui| {
            self.show_graph(ui);
        });
    }
}

impl<T, V> crate::MaraModule for GraphSurface<T, V>
where
    V: NodeViewer<T>,
{
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&self) -> &str {
        &self.title
    }

    fn icon(&self) -> &'static str {
        "node_tree"
    }

    fn inline(
        &mut self,
        ui: &mut egui::Ui,
        ctx: crate::ModuleInlineCtx<'_>,
    ) -> crate::ModuleResponse {
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Graph: {}", self.title));
            });
            self.show_graph(ui);
            if ctx.can_enter_workspace()
                && ui
                    .button(crate::style::theme().modules.inline_workspace_button_label)
                    .clicked()
            {
                crate::ModuleResponse::enter_workspace()
            } else {
                crate::ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut crate::WorkspaceCtx<'_>) {
        ws.add_bar(
            crate::WorkspaceBar::new(
                egui::Id::new(("graph.workspace.bar", self.id)),
                crate::WorkspaceBarEdge::Top,
                crate::WorkspaceBarCluster::Middle,
            )
            .with_item(crate::WorkspaceBarItem::command(
                egui::Id::new(("graph.workspace.add_node", self.id)),
                "Add Node",
                Some("add"),
            )),
        );
        ws.add_ribbon(self.toolbar(crate::RibbonScope::WorkspaceLevel(ws.level.id)));
    }
}

#[cfg(test)]
mod view_module_bridge_tests {
    use super::*;

    #[derive(Clone, Debug)]
    struct TestNode(&'static str);

    #[derive(Clone, Debug, Default)]
    struct TestViewer;

    impl NodeViewer<TestNode> for TestViewer {
        fn title(&mut self, node: &TestNode) -> String {
            node.0.to_owned()
        }

        fn inputs(&mut self, _node: &TestNode) -> usize {
            0
        }

        fn show_input(
            &mut self,
            _pin: &InPin,
            _ui: &mut egui::Ui,
            _graph: &mut Graph<TestNode>,
        ) -> impl NodePin + 'static {
            PinInfo::default()
        }

        fn outputs(&mut self, _node: &TestNode) -> usize {
            0
        }

        fn show_output(
            &mut self,
            _pin: &OutPin,
            _ui: &mut egui::Ui,
            _graph: &mut Graph<TestNode>,
        ) -> impl NodePin + 'static {
            PinInfo::default()
        }
    }

    fn assert_view<T: crate::MaraView>(_value: &T) {}
    fn assert_module<T: crate::MaraModule>(_value: &T) {}

    #[test]
    fn graph_surface_is_both_view_and_module() {
        let mut graph = Graph::new();
        graph.insert_node(egui::pos2(0.0, 0.0), TestNode("Node"));
        let surface = GraphSurface::new("graph-surface", "Graph", graph, TestViewer);
        assert_view(&surface);
        assert_module(&surface);
        assert_eq!(crate::MaraView::title(&surface), "Graph");
        assert_eq!(crate::MaraModule::icon(&surface), "node_tree");
    }
}
