//! Shared "maximise this widget to full window" wrapper.
//!
//! Graph canvases, code editors, and similar "canvas-shaped"
//! widgets benefit from a one-click lift to full window that
//! leaves their surrounding pane and container untouched. This
//! module provides exactly that, in a widget-agnostic form:
//!
//! ```ignore
//! maximizable(ui, "my_widget", accent, mara::ui::vocab::Vec2::new(w, 300.0), |ui| {
//!     // Render your widget into this inner `ui` — it's either
//!     // the inline rect the caller wanted, or a full-window
//!     // overlay depending on the maximise state.
//! });
//! ```
//!
//! The wrapper:
//!
//! * Stores a `bool` per `id_salt` in `egui::Context::data`.
//! * Not maximised: allocates a rect of `min_size` in the current
//!   `Ui` and renders the body inside a child `Ui` pinned to
//!   that rect.
//! * Maximised: paints a placeholder in the current `Ui` so the
//!   surrounding layout keeps its footprint, then renders the
//!   body inside an `egui::Area` at the highest order covering
//!   the full host content rect with a mara glass frame.
//! * Paints a 24 px chip in the top-left of whichever rect holds
//!   the body — ribbon-button styling (accent fill on active,
//!   accent border), glyph = two diagonal arrows joined by a line
//!   (outward for maximise, inward for restore).
//!
//! The chip's Area runs at `Order::Tooltip` so it always paints
//! and intercepts clicks above the wrapped widget's shapes, even
//! when the widget (like `egui-graph`) draws interactive content
//! across its entire rect.

use std::hash::Hash;

use egui;

use crate::layout::{
    AreaHost, ChildRegion, CursorIcon, Layer, Sense as MaraSense, StackAlign, UiBackend,
};
use crate::paint::PaintCmd;
use crate::ribbon::{RibbonCluster, RibbonEdge};
use crate::style::{
    RadiusRole, StrokeRole, glass_alpha_window, glass_fill, radius_for, stroke_for,
};
use crate::vocab::{
    Align2 as MaraAlign2, Color32 as MaraColor32, CornerRadius as MaraCornerRadius, Id as MaraId,
    Pos2 as MaraPos2, Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
};

/// Configuration for the fullscreen overlay's chrome — currently
/// just where the minimize button sits, but expandable as more
/// overlay tools land. Each `(edge, cluster)` corresponds to one
/// of the 12 anchor points around the screen, matching the rail
/// system used by [`crate::ribbon`]:
///
/// * `edge`: which screen edge the button hugs.
/// * `cluster`: where along that edge — `Start` = near the
///   "title-side" corner, `Middle` = centred, `End` = far corner.
///
/// Default: `(RibbonEdge::Right, RibbonCluster::Start)` —
/// upper-right corner.
#[derive(Copy, Clone, Debug)]
pub struct OverlayOpts {
    pub minimize_edge: RibbonEdge,
    pub minimize_cluster: RibbonCluster,
    pub content_avoidance: crate::RibbonAvoidance,
}

impl Default for OverlayOpts {
    fn default() -> Self {
        Self {
            minimize_edge: RibbonEdge::Right,
            minimize_cluster: RibbonCluster::Start,
            content_avoidance: crate::RibbonAvoidance::none(),
        }
    }
}

impl OverlayOpts {
    /// Build with the minimize button anchored to the given edge +
    /// cluster on the fullscreen overlay.
    pub fn minimize_at(edge: RibbonEdge, cluster: RibbonCluster) -> Self {
        Self {
            minimize_edge: edge,
            minimize_cluster: cluster,
            ..Self::default()
        }
    }

    /// Keep the fullscreen overlay/background full-window, but lay
    /// out the body UI away from selected ribbon rails.
    #[must_use]
    pub fn avoid_ribbons(mut self, avoidance: crate::RibbonAvoidance) -> Self {
        self.content_avoidance = avoidance;
        self
    }
}

/// The egui data key that [`maximizable`] uses to store the
/// maximise-flag for a given `id_salt`. Exposed so callers can do
/// context-sensitive routing without poking inside the widget.
pub fn maximize_state_key(id_salt: impl std::hash::Hash) -> MaraId {
    MaraId::new(("mara_maximize", id_salt))
}

fn pending_restore_fullscreen_key() -> egui::Id {
    egui::Id::new("mara_pending_restore_fullscreen")
}

fn current_node_region_key() -> egui::Id {
    egui::Id::new("mara_current_node_region")
}

/// The rect the current view node renders into, if a scoped node
/// (`ViewCtx`) is rendering. The fullscreen overlay paints here instead
/// of the whole window, and per-view ribbons anchor here, so a leaf's
/// chrome stays inside its cell. `None` (outside any node render) means
/// whole-window.
pub(crate) fn current_node_region(ctx: &egui::Context) -> Option<MaraRect> {
    ctx.data(|d| d.get_temp::<MaraRect>(current_node_region_key()))
}

/// Publish `region` as the current node region for the duration of
/// `body`, restoring the previous value afterward so nested nodes see
/// their own region and unwind to the parent's. First-party hook used by
/// `ViewCtx` around its render entry points.
#[doc(hidden)]
pub fn __internal_with_node_region<R>(
    ctx: &egui::Context,
    region: MaraRect,
    body: impl FnOnce() -> R,
) -> R {
    let key = current_node_region_key();
    let prev = ctx.data(|d| d.get_temp::<MaraRect>(key));
    ctx.data_mut(|d| d.insert_temp(key, region));
    let out = body();
    ctx.data_mut(|d| match prev {
        Some(rect) => {
            d.insert_temp(key, rect);
        }
        None => {
            d.remove::<MaraRect>(key);
        }
    });
    out
}

/// Internal fullscreen-owner read for first-party host adapters.
///
/// Public app code should reach this through a sealed host/view
/// context method, not by receiving a raw `egui::Context`.
#[doc(hidden)]
pub fn __internal_fullscreen_owner(ctx: &egui::Context) -> Option<MaraId> {
    let global_key = egui::Id::new("mara_maximize_global");
    let pass_nr = ctx.cumulative_pass_nr();
    let stored: Option<(u64, MaraId)> = ctx.data(|d| d.get_temp(global_key));
    match stored {
        Some((f, id)) if f == pass_nr || f + 1 == pass_nr => Some(id),
        _ => None,
    }
}

/// Internal fullscreen-active predicate for first-party host adapters.
#[doc(hidden)]
pub fn __internal_is_any_fullscreen(ctx: &egui::Context) -> bool {
    __internal_fullscreen_owner(ctx).is_some()
}

fn suppress_fullscreen_minimize_chip_key() -> egui::Id {
    egui::Id::new("mara_suppress_fullscreen_minimize_chip")
}

/// Internal fullscreen restore-chip visibility setter for
/// first-party host adapters.
#[doc(hidden)]
pub fn __internal_set_fullscreen_minimize_chip_visible(ctx: &egui::Context, visible: bool) {
    ctx.data_mut(|d| {
        d.insert_temp::<bool>(suppress_fullscreen_minimize_chip_key(), !visible);
    });
}

/// Internal fullscreen restore request for first-party host adapters.
///
/// Returns `true` when a fullscreen owner was found and toggled off.
#[doc(hidden)]
pub fn __internal_restore_fullscreen(ctx: &egui::Context) -> bool {
    let Some(owner) = __internal_fullscreen_owner(ctx) else {
        return false;
    };
    ctx.data_mut(|d| {
        d.insert_temp::<MaraId>(pending_restore_fullscreen_key(), owner);
    });
    true
}

/// Wrap a widget so it gains a maximise / restore toggle.
///
/// Call once per frame with the same `id_salt`. `min_size` is the
/// rect the body renders into while inline; when maximised the
/// body fills the host content rect instead.
pub fn maximizable(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    accent: impl Into<MaraColor32>,
    min_size: impl Into<MaraVec2>,
    body: impl FnOnce(&mut egui::Ui),
) {
    maximizable_with_opts(ui, id_salt, accent, min_size, OverlayOpts::default(), body)
}

/// Same as [`maximizable`] but accepts [`OverlayOpts`] to control
/// where the minimize button lands on the fullscreen overlay. Use
/// this when you want a non-default position — e.g. minimize on
/// the bottom-left corner instead of the top-right.
pub fn maximizable_with_opts(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    accent: impl Into<MaraColor32>,
    min_size: impl Into<MaraVec2>,
    opts: OverlayOpts,
    body: impl FnOnce(&mut egui::Ui),
) {
    let accent = accent.into();
    let min_size = min_size.into();
    // Maximise state keyed purely on the caller's `id_salt` — no
    // `ui.id()` mixed in — so the host can reconstruct the same
    // key from the outside via [`is_maximized`] and route Ctrl+K
    // / context-sensitive logic based on "is THIS widget
    // currently full-window?".
    let max_id = maximize_state_key(id_salt);
    let max_key: egui::Id = max_id.into();
    let mut maximized: bool = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(max_key))
        .unwrap_or(false);
    let pending_restore = ui
        .ctx()
        .data(|d| d.get_temp::<MaraId>(pending_restore_fullscreen_key()))
        == Some(max_id);
    if pending_restore {
        maximized = false;
        ui.ctx().data_mut(|d| {
            d.insert_temp::<bool>(max_key, false);
            d.remove::<MaraId>(pending_restore_fullscreen_key());
            d.remove::<(u64, MaraId)>(egui::Id::new("mara_maximize_global"));
        });
    }
    let mut toggle = false;

    // Global "is any maximizable widget currently full-window?"
    // tracker. Stored as `(pass_nr, owner_id)` so stale values
    // (widget toggled off and never re-rendered) are automatically
    // ignored on the next pass. Every other `maximizable` call
    // checks this to suppress its own button when SOMEONE ELSE is
    // full-window — otherwise `Order::Tooltip` button areas from
    // inline-in-a-pane widgets would still paint on top of the
    // overlay.
    let global_key = egui::Id::new("mara_maximize_global");
    let pass_nr = ui.ctx().cumulative_pass_nr();
    let stored_global: Option<(u64, MaraId)> = ui.ctx().data(|d| d.get_temp(global_key));
    let some_other_maximized = match stored_global {
        Some((f, id)) => (f == pass_nr || f + 1 == pass_nr) && id != max_id,
        None => false,
    };
    if maximized {
        ui.ctx()
            .data_mut(|d| d.insert_temp(global_key, (pass_nr, max_id)));
    }

    let overlay = crate::style::theme().overlay;

    if maximized {
        // Placeholder in the caller's layout so the surrounding
        // section / pane keep their footprint while the widget is
        // detached into the overlay.
        let rect = {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            backend.allocate(min_size, MaraSense::Hover).rect
        };
        if ui.is_rect_visible(rect.into()) {
            paint_maximize_placeholder(ui, rect, overlay.placeholder_text);
        }

        // Full-window overlay at `Order::Foreground` — paints
        // ABOVE `Order::Middle`, which is where the GAME-theme
        // floating section icons (`z::CONTAINER_FLOATING_ICON`)
        // sit. Without this layering the big icons would bleed
        // through onto the maximised node-graph / code-editor
        // canvas. For the host's own fullscreen ribbons /
        // tooltips to remain visible on top, the host must paint
        // them AFTER the pane loop returns (egui paints
        // later-registered Areas at the same Order on top of
        // earlier ones). Frame has NO corner radius / stroke /
        // inner margin so the overlay covers edge-to-edge.
        let ctx = ui.ctx().clone();
        // Fullscreen within the current view node's region (a cell), or
        // the whole window when no node is scoping (the root / host).
        let screen = current_node_region(&ctx)
            .unwrap_or_else(|| crate::backend::egui::context_content_rect(&ctx));
        let content = opts.content_avoidance.apply_to_rect(screen);
        crate::backend::egui::show_area_for_host(
            &ctx,
            AreaHost::new(
                ui.id().with(("mara_maximize_overlay", id_salt)).into(),
                screen.min,
                Layer::Foreground,
            ),
            |ui| {
                crate::backend::egui::constrain_ui_to_rect(ui, screen);
                let bg = crate::style::theme().bg_panel;
                let opaque_bg = MaraColor32::from_rgb(bg.r(), bg.g(), bg.b());
                paint_cmd(ui, maximize_overlay_background_cmd(screen, opaque_bg));
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                let _ = backend.allocate(screen.size(), MaraSense::Hover);
            },
        );
        crate::backend::egui::show_area_for_host(
            &ctx,
            AreaHost::new(
                ui.id()
                    .with(("mara_maximize_overlay_content", id_salt))
                    .into(),
                content.min,
                Layer::Foreground,
            ),
            |ui| {
                crate::backend::egui::constrain_ui_to_rect(ui, content);
                body(ui);
            },
        );
        // Minimize button — a draggable ribbon-styled chip. The
        // initial position comes from `opts`; the user can grab the
        // chip and drag it to ANY of the 12 edge/cluster anchor
        // points, and that choice persists in ctx data across
        // frames. Painted in its OWN `Order::Tooltip` Area so it
        // sits on top of the `Foreground` overlay above.
        let suppress_minimize_chip: bool = ctx
            .data(|d| d.get_temp(suppress_fullscreen_minimize_chip_key()))
            .unwrap_or(false);
        if !suppress_minimize_chip
            && fullscreen_minimize_button(
                &ctx,
                screen,
                opts,
                overlay.fullscreen_button_size,
                overlay.fullscreen_edge_gap,
                accent,
                id_salt,
            )
        {
            toggle = true;
        }
    } else {
        // Inline — allocate a rect of `min_size` and render the body
        // into a child `Ui` pinned to it. The maximise chip floats
        // in the body's TOP-RIGHT corner (same place as when
        // maximised), so the affordance is consistent regardless of
        // mode and lives *inside* the widget's canvas — section
        // headers no longer reserve any actions slot.
        let desired = MaraVec2::new(ui.available_width().max(min_size.x), min_size.y);
        let rect = {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            backend.allocate(desired, MaraSense::Hover).rect
        };
        let mut child = crate::backend::egui::child_ui_for_region(
            ui,
            ChildRegion::top_down(rect, StackAlign::Min),
        );
        body(&mut child);

        // Suppress the chip while another widget is full-window —
        // its overlay covers the screen and our `Order::Tooltip` chip
        // would otherwise paint on top of nothing.
        if !some_other_maximized {
            let chip_pos = inline_chip_pos(rect, overlay.inline_chip_size, overlay.inline_chip_pad);
            if max_button_overlay(ui.ctx(), chip_pos, false, accent, id_salt).clicked() {
                toggle = true;
            }
        }
    }

    if toggle {
        ui.ctx()
            .data_mut(|d| d.insert_temp::<bool>(max_key, !maximized));
    }
}

/// The 24 px maximise / restore chip. Lives in its own
/// `Order::Tooltip` Area so it paints (and intercepts clicks)
/// above the wrapped widget's own shapes — Areas at the same
/// `Foreground` order would get shadowed by canvas widgets like
/// the graph graph that register their own foreground sub-layers.
fn max_button_overlay(
    ctx: &egui::Context,
    pos: MaraPos2,
    maximized: bool,
    accent: impl Into<MaraColor32>,
    id_salt: impl Hash + Copy,
) -> crate::mui::MaraResponse {
    let accent = accent.into();
    let btn = crate::style::theme().overlay.inline_chip_size;
    let area_id = MaraId::new("mara_maximize_btn").with(id_salt);
    let inner = crate::backend::egui::show_area_for_host(
        ctx,
        AreaHost::new(area_id, pos, Layer::Overlay),
        |ui| {
            let resp = {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                backend.allocate(MaraVec2::new(btn, btn), MaraSense::Click)
            };
            crate::backend::egui::hover_cursor_for_ui_response(ui, &resp, CursorIcon::PointingHand);
            crate::backend::egui::hover_text_for_ui_response(
                ui,
                &resp,
                if maximized { "Restore" } else { "Maximize" },
            );
            let rect = resp.rect;
            if ui.is_rect_visible(rect.into()) {
                let hovered = resp.hovered();
                paint_ribbon_style_chip(
                    ui, rect, accent, /* active */ maximized, /* hovered */ hovered,
                );
                paint_fullscreen_arrows(
                    ui, rect, accent, /* inward */ maximized, /* hovered */ hovered,
                );
            }
            resp
        },
    );
    inner.inner
}

fn inline_chip_pos(body_rect: MaraRect, chip_size: f32, pad: f32) -> MaraPos2 {
    MaraPos2::new(body_rect.max.x - chip_size - pad, body_rect.min.y + pad)
}

/// Submit a single Mara paint command through the egui backend.
fn paint_cmd(ui: &mut egui::Ui, cmd: PaintCmd) {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    crate::layout::UiBackend::paint(&mut backend, cmd);
}

fn paint_maximize_placeholder(ui: &mut egui::Ui, rect: MaraRect, text: &str) {
    paint_cmd(
        ui,
        maximize_placeholder_text_cmd(rect, text, crate::style::on_section_dim()),
    );
}

fn maximize_placeholder_text_cmd(rect: MaraRect, text: &str, color: MaraColor32) -> PaintCmd {
    PaintCmd::Text {
        pos: rect.center(),
        anchor: MaraAlign2::CENTER_CENTER,
        text: text.to_owned(),
        size: 12.0,
        color,
        mono: false,
    }
}

fn maximize_overlay_background_cmd(rect: MaraRect, fill: MaraColor32) -> PaintCmd {
    PaintCmd::RectFilled {
        rect,
        corner: MaraCornerRadius::ZERO,
        fill,
    }
}

fn maximize_chip_ghost_paint_cmds(
    rect: MaraRect,
    accent: MaraColor32,
    corner: MaraCornerRadius,
    fill_alpha: u8,
    stroke_width: f32,
) -> [PaintCmd; 2] {
    [
        PaintCmd::RectFilled {
            rect,
            corner,
            fill: MaraColor32::from_rgba_unmultiplied(
                accent.r(),
                accent.g(),
                accent.b(),
                fill_alpha,
            ),
        },
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke: MaraStroke::new(
                stroke_width,
                MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 180),
            ),
        },
    ]
}

fn minimize_chip_icon_paint_cmd(
    rect: MaraRect,
    btn_size: f32,
    color: MaraColor32,
) -> Option<PaintCmd> {
    crate::icons::icon_paint_cmd(
        crate::icons::Icon::Name("arrow-minimize"),
        rect.center(),
        MaraAlign2::CENTER_CENTER,
        btn_size * crate::style::theme().icons.overlay_icon_scale,
        color,
    )
}

/// Compute the upper-left position of the minimize chip given the
/// (edge, cluster) anchor + button size + edge inset. Mirrors the
/// 12-position rail system used by [`crate::ribbon`]:
///
/// * Cluster `Start` ⇒ corner closest to `(left, top)` along the edge.
/// * Cluster `End`   ⇒ opposite corner.
/// * Cluster `Middle`⇒ centred along the edge.
fn compute_chip_pos(
    screen: MaraRect,
    edge: RibbonEdge,
    cluster: RibbonCluster,
    btn: f32,
    edge_gap: f32,
) -> MaraPos2 {
    let along_axis_pos = |min: f32, max: f32| -> f32 {
        match cluster {
            RibbonCluster::Start => min + edge_gap,
            RibbonCluster::End => max - edge_gap - btn,
            RibbonCluster::Middle => (min + max) * 0.5 - btn * 0.5,
        }
    };
    match edge {
        RibbonEdge::Top => MaraPos2::new(
            along_axis_pos(screen.left(), screen.right()),
            screen.top() + edge_gap,
        ),
        RibbonEdge::Bottom => MaraPos2::new(
            along_axis_pos(screen.left(), screen.right()),
            screen.bottom() - edge_gap - btn,
        ),
        RibbonEdge::Left => MaraPos2::new(
            screen.left() + edge_gap,
            along_axis_pos(screen.top(), screen.bottom()),
        ),
        RibbonEdge::Right => MaraPos2::new(
            screen.right() - edge_gap - btn,
            along_axis_pos(screen.top(), screen.bottom()),
        ),
    }
}

/// Paint the fullscreen overlay's minimize button.
///
/// The chip starts at the `(edge, cluster)` baked into `opts`. The
/// user can grab and drag it to ANY of the 12 anchor points; on
/// release the chip snaps to whichever anchor was nearest. The
/// chosen anchor persists in ctx data, so the next time this
/// widget enters fullscreen the chip reappears where the user left
/// it. Returns `true` on a regular click (= restore).
fn fullscreen_minimize_button(
    ctx: &egui::Context,
    screen: MaraRect,
    opts: OverlayOpts,
    btn_size: f32,
    edge_gap: f32,
    accent: impl Into<MaraColor32>,
    id_salt: impl Hash + Copy,
) -> bool {
    let accent = accent.into();
    let accent_egui = crate::backend::egui::color32_for_backend(accent);
    // Persisted user-chosen anchor (set on drag-release). When
    // empty, fall back to the caller-supplied `opts`.
    let anchor_key = egui::Id::new("mara_maximize_chip_anchor").with(id_salt);
    let stored: Option<(RibbonEdge, RibbonCluster)> = ctx.data(|d| d.get_temp(anchor_key));
    let active_anchor = stored.unwrap_or((opts.minimize_edge, opts.minimize_cluster));
    // While the user is mid-drag, override the chip position with
    // the cursor (so the chip follows the pointer) — keyed by the
    // SAME id so the value clears on release.
    let drag_pos_key = egui::Id::new("mara_maximize_chip_drag_pos").with(id_salt);
    let drag_cursor: Option<MaraPos2> = ctx.data(|d| d.get_temp(drag_pos_key));
    let chip_pos: MaraPos2 = if let Some(c) = drag_cursor {
        MaraPos2::new(c.x - btn_size * 0.5, c.y - btn_size * 0.5)
    } else {
        compute_chip_pos(screen, active_anchor.0, active_anchor.1, btn_size, edge_gap)
    };

    let area_id = MaraId::new("mara_maximize_minimize").with(id_salt);
    let inner = crate::backend::egui::show_area_for_host(
        ctx,
        AreaHost::new(area_id, chip_pos, Layer::Overlay),
        |ui| {
            let resp = {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                backend.allocate(MaraVec2::new(btn_size, btn_size), MaraSense::ClickAndDrag)
            };
            // Cursor stays the default pointing-hand egui picks for
            // clickable widgets — same as the main-page ribbon
            // buttons. The button is click-first, drag-second; the
            // user shouldn't see a "grab" cursor on hover that
            // suggests "drag-only".
            crate::backend::egui::hover_text_for_ui_response(ui, &resp, "Restore");
            let rect = resp.rect;
            // Drag tracking — write the cursor position to ctx data
            // each frame the chip is being dragged. On release,
            // snap to the nearest anchor and persist it.
            let mut ghost_target: Option<MaraRect> = None;
            if resp.dragged()
                && let Some(p) = crate::backend::egui::pointer_interact_pos(ui.ctx())
            {
                ui.ctx().data_mut(|d| d.insert_temp(drag_pos_key, p));
                // Compute the live snap target so we can paint
                // a ghost outline showing where the chip WILL
                // land on release.
                let snap = nearest_anchor(screen, p, btn_size, edge_gap);
                let snap_pos = compute_chip_pos(screen, snap.0, snap.1, btn_size, edge_gap);
                ghost_target = Some(MaraRect::from_min_size(
                    snap_pos,
                    MaraVec2::new(btn_size, btn_size),
                ));
            }
            if resp.drag_stopped() {
                let cursor = crate::backend::egui::pointer_interact_pos(ui.ctx())
                    .unwrap_or_else(|| rect.center());
                let snapped = nearest_anchor(screen, cursor, btn_size, edge_gap);
                ui.ctx().data_mut(|d| {
                    d.insert_temp(anchor_key, snapped);
                    d.remove::<MaraPos2>(drag_pos_key);
                });
            }
            if ui.is_rect_visible(rect.into()) {
                paint_ribbon_style_chip(
                    ui,
                    rect,
                    accent,
                    /* active */ true,
                    /* hovered */ resp.hovered(),
                );
                let glyph_col = if resp.hovered() {
                    crate::style::contrast_text_for(accent_egui).into()
                } else {
                    egui::Color32::from_rgba_unmultiplied(
                        accent_egui.r(),
                        accent_egui.g(),
                        accent_egui.b(),
                        220,
                    )
                };
                if crate::icons::icon_fonts_ready()
                    && let Some(cmd) =
                        minimize_chip_icon_paint_cmd(rect, btn_size, glyph_col.into())
                {
                    paint_cmd(ui, cmd);
                }
            }
            // Ghost preview at the snap target — a low-alpha
            // accent rect with a dashed-style accent border, painted
            // on its OWN tooltip-layer painter (clip = full screen)
            // so it doesn't get clipped by the chip's tiny area.
            if let Some(g) = ghost_target {
                let ghost_painter = crate::backend::egui::context_painter_for_layer(
                    ui.ctx(),
                    Layer::Overlay,
                    MaraId::new(("mara_maximize_chip_ghost", id_salt)),
                    screen,
                );
                let overlay = crate::style::theme().overlay;
                for cmd in maximize_chip_ghost_paint_cmds(
                    g,
                    accent,
                    radius_for(RadiusRole::Section),
                    overlay.ghost_fill_alpha,
                    overlay.ghost_stroke_width,
                ) {
                    crate::backend::egui::render_paint_cmd(&ghost_painter, cmd);
                }
            }
            resp
        },
    );
    // Only treat as a "restore" click when the gesture wasn't a drag.
    let resp = inner.inner;
    resp.clicked() && drag_cursor.is_none()
}

/// Find the (edge, cluster) anchor whose chip-centre is closest to
/// `cursor`. Used by the drag-release snap so the chip lands on
/// one of the 12 fixed anchors.
fn nearest_anchor(
    screen: MaraRect,
    cursor: MaraPos2,
    btn_size: f32,
    edge_gap: f32,
) -> (RibbonEdge, RibbonCluster) {
    const CANDIDATES: &[(RibbonEdge, RibbonCluster)] = &[
        (RibbonEdge::Top, RibbonCluster::Start),
        (RibbonEdge::Top, RibbonCluster::Middle),
        (RibbonEdge::Top, RibbonCluster::End),
        (RibbonEdge::Bottom, RibbonCluster::Start),
        (RibbonEdge::Bottom, RibbonCluster::Middle),
        (RibbonEdge::Bottom, RibbonCluster::End),
        (RibbonEdge::Left, RibbonCluster::Start),
        (RibbonEdge::Left, RibbonCluster::Middle),
        (RibbonEdge::Left, RibbonCluster::End),
        (RibbonEdge::Right, RibbonCluster::Start),
        (RibbonEdge::Right, RibbonCluster::Middle),
        (RibbonEdge::Right, RibbonCluster::End),
    ];
    let mut best = CANDIDATES[0];
    let mut best_d = f32::INFINITY;
    for &(e, c) in CANDIDATES {
        let p = compute_chip_pos(screen, e, c, btn_size, edge_gap);
        let centre = MaraPos2::new(p.x + btn_size * 0.5, p.y + btn_size * 0.5);
        let d = centre.distance(cursor);
        if d < best_d {
            best_d = d;
            best = (e, c);
        }
    }
    best
}

/// Mirror of `ribbon::paint::ribbon_button_paint_cmds`. Same glass
/// tiers and active / hover transitions — keeps the chip in the
/// ribbon button family.
fn paint_ribbon_style_chip(
    ui: &mut egui::Ui,
    rect: MaraRect,
    accent: impl Into<MaraColor32>,
    active: bool,
    hovered: bool,
) {
    for cmd in ribbon_style_chip_paint_cmds(rect, accent.into(), active, hovered) {
        paint_cmd(ui, cmd);
    }
}

fn ribbon_style_chip_paint_cmds(
    rect: MaraRect,
    accent: MaraColor32,
    active: bool,
    hovered: bool,
) -> [PaintCmd; 2] {
    let accent_egui = crate::backend::egui::color32_for_backend(accent);
    let bg = if active {
        let blend = |a: u8, b: u8| ((a as f32) * 0.75 + (b as f32) * 0.25).round() as u8;
        let tinted = egui::Color32::from_rgb(
            blend(crate::style::theme().bg_raised.r(), accent_egui.r()),
            blend(crate::style::theme().bg_raised.g(), accent_egui.g()),
            blend(crate::style::theme().bg_raised.b(), accent_egui.b()),
        );
        glass_fill(tinted, accent_egui, glass_alpha_window())
    } else if hovered {
        glass_fill(
            crate::style::theme().bg_raised,
            accent_egui,
            glass_alpha_window(),
        )
    } else {
        glass_fill(
            crate::style::theme().bg_panel,
            accent_egui,
            glass_alpha_window(),
        )
    };
    let stroke = if active {
        MaraStroke::new(crate::style::theme().stroke.border_width, accent)
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent_egui)
    };
    let corner: MaraCornerRadius = radius_for(RadiusRole::Section);
    [
        PaintCmd::RectFilled {
            rect,
            corner,
            fill: bg,
        },
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke,
        },
    ]
}

/// Paint the fullscreen glyph — single diagonal line through the
/// chip's centre, arrowheads at each end. `inward = false` heads
/// point OUT (maximise); `inward = true` heads point IN (restore).
fn paint_fullscreen_arrows(
    ui: &mut egui::Ui,
    rect: MaraRect,
    accent: impl Into<MaraColor32>,
    inward: bool,
    hovered: bool,
) {
    for cmd in fullscreen_arrow_paint_cmds(rect, accent.into(), inward, hovered) {
        paint_cmd(ui, cmd);
    }
}

fn fullscreen_arrow_paint_cmds(
    rect: MaraRect,
    accent: MaraColor32,
    inward: bool,
    hovered: bool,
) -> Vec<PaintCmd> {
    let color = fullscreen_arrow_color(accent, inward, hovered);
    let icons = crate::style::theme().icons;
    let stroke_w = icons.overlay_arrow_stroke_w;
    let shrunk = rect.shrink(icons.overlay_arrow_shrink);
    let center = rect.center();
    let ne_corner = MaraPos2::new(shrunk.max.x, shrunk.min.y);
    let sw_corner = MaraPos2::new(shrunk.min.x, shrunk.max.y);
    let t = icons.overlay_arrow_tip_t;
    let ne_tip = lerp_pos(center, ne_corner, t);
    let sw_tip = lerp_pos(center, sw_corner, t);
    let (from_ne, from_sw) = if inward {
        (ne_corner, sw_corner)
    } else {
        (center, center)
    };
    vec![
        PaintCmd::Line {
            a: sw_tip,
            b: ne_tip,
            stroke: MaraStroke::new(stroke_w, color),
        },
        arrowhead_paint_cmd(from_ne, ne_tip, color),
        arrowhead_paint_cmd(from_sw, sw_tip, color),
    ]
}

fn fullscreen_arrow_color(accent: MaraColor32, inward: bool, hovered: bool) -> MaraColor32 {
    if inward {
        crate::style::on_section()
    } else if hovered {
        accent
    } else {
        MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220)
    }
}

fn lerp_pos(a: MaraPos2, b: MaraPos2, t: f32) -> MaraPos2 {
    MaraPos2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
}

fn arrowhead_paint_cmd(from: MaraPos2, tip: MaraPos2, color: MaraColor32) -> PaintCmd {
    let dx = tip.x - from.x;
    let dy = tip.y - from.y;
    let len = (dx.mul_add(dx, dy * dy)).sqrt().max(1e-3);
    let dir_x = dx / len;
    let dir_y = dy / len;
    let perp_x = -dir_y;
    let perp_y = dir_x;
    let icons = crate::style::theme().icons;
    let head_len = icons.overlay_arrow_head_len;
    let head_half_w = icons.overlay_arrow_head_half_w;
    let back = MaraPos2::new(tip.x - dir_x * head_len, tip.y - dir_y * head_len);
    let p1 = MaraPos2::new(back.x + perp_x * head_half_w, back.y + perp_y * head_half_w);
    let p2 = MaraPos2::new(back.x - perp_x * head_half_w, back.y - perp_y * head_half_w);
    PaintCmd::Polygon {
        points: vec![tip, p1, p2],
        fill: color,
        stroke: MaraStroke::NONE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Color32, Pos2, Rect, Vec2};

    #[test]
    fn maximize_placeholder_lowers_to_mara_text_command() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 40.0));
        let color = Color32::WHITE;

        let cmd = maximize_placeholder_text_cmd(rect, "Fullscreen", color);

        assert!(matches!(
            cmd,
            PaintCmd::Text {
                pos,
                anchor: MaraAlign2::CENTER_CENTER,
                text,
                size,
                color: got_color,
                mono: false,
            } if pos == rect.center()
                && text == "Fullscreen"
                && size == 12.0
                && got_color == color
        ));
    }

    #[test]
    fn maximize_overlay_background_lowers_to_mara_rect_command() {
        let rect = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(100.0, 80.0));
        let fill = Color32::from_rgb(10, 20, 30);

        let cmd = maximize_overlay_background_cmd(rect, fill);

        assert!(matches!(
            cmd,
            PaintCmd::RectFilled {
                rect: got_rect,
                corner: MaraCornerRadius::ZERO,
                fill: got_fill,
            } if got_rect == rect && got_fill == fill
        ));
    }

    #[test]
    fn maximize_state_key_uses_mara_id_vocabulary() {
        let key: MaraId = maximize_state_key("widget");

        assert_eq!(key, MaraId::new(("mara_maximize", "widget")));
    }

    #[test]
    fn node_region_nests_and_restores() {
        let ctx = egui::Context::default();
        let outer = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(100.0, 100.0));
        let inner = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(20.0, 20.0));

        assert!(current_node_region(&ctx).is_none());
        __internal_with_node_region(&ctx, outer, || {
            assert_eq!(current_node_region(&ctx), Some(outer));
            // A nested node (a Split cell inside a cell) sees its own
            // region and restores the parent's on the way out — this is
            // what keeps a fullscreen overlay scoped to the right cell.
            __internal_with_node_region(&ctx, inner, || {
                assert_eq!(current_node_region(&ctx), Some(inner));
            });
            assert_eq!(current_node_region(&ctx), Some(outer));
        });
        assert!(current_node_region(&ctx).is_none());
    }

    #[test]
    fn maximize_chip_ghost_lowers_to_mara_fill_and_stroke_commands() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(24.0, 24.0));
        let accent = Color32::from_rgb(60, 90, 120);
        let corner = MaraCornerRadius::same(4);

        let cmds = maximize_chip_ghost_paint_cmds(rect, accent, corner, 44, 1.5);

        assert!(matches!(
            cmds[0],
            PaintCmd::RectFilled {
                rect: got_rect,
                corner: got_corner,
                fill,
            } if got_rect == rect
                && got_corner == corner
                && fill == Color32::from_rgba_unmultiplied(60, 90, 120, 44)
        ));
        assert!(matches!(
            cmds[1],
            PaintCmd::RectStroke {
                rect: got_rect,
                corner: got_corner,
                stroke,
            } if got_rect == rect
                && got_corner == corner
                && stroke == MaraStroke::new(1.5, Color32::from_rgba_unmultiplied(60, 90, 120, 180))
        ));
    }

    #[test]
    fn minimize_chip_icon_lowers_to_mara_named_icon_command() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(24.0, 24.0));
        let cmd = minimize_chip_icon_paint_cmd(rect, 24.0, Color32::WHITE)
            .expect("arrow-minimize icon should be bundled");

        let PaintCmd::TextWithFamily {
            pos,
            anchor,
            text,
            family,
            ..
        } = cmd
        else {
            panic!("minimize chip icon should lower to a named-font text command");
        };
        assert_eq!(pos, rect.center());
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        assert_eq!(text.chars().count(), 1);
        assert!(matches!(family, crate::paint::TextFamily::Named(_)));
    }

    #[test]
    fn fullscreen_chip_anchor_geometry_uses_mara_rects() {
        let screen = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));

        assert_eq!(
            compute_chip_pos(screen, RibbonEdge::Right, RibbonCluster::Start, 24.0, 8.0),
            Pos2::new(768.0, 8.0)
        );
        assert_eq!(
            compute_chip_pos(screen, RibbonEdge::Bottom, RibbonCluster::Middle, 24.0, 8.0),
            Pos2::new(388.0, 568.0)
        );
    }

    #[test]
    fn inline_chip_position_uses_mara_rects() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 80.0));

        assert_eq!(inline_chip_pos(rect, 24.0, 8.0), Pos2::new(178.0, 28.0));
    }

    #[test]
    fn nearest_fullscreen_chip_anchor_uses_mara_positions() {
        let screen = Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(800.0, 600.0));

        assert_eq!(
            nearest_anchor(screen, Pos2::new(790.0, 300.0), 24.0, 8.0),
            (RibbonEdge::Right, RibbonCluster::Middle)
        );
        assert_eq!(
            nearest_anchor(screen, Pos2::new(400.0, 590.0), 24.0, 8.0),
            (RibbonEdge::Bottom, RibbonCluster::Middle)
        );
    }

    #[test]
    fn ribbon_style_chip_lowers_to_mara_fill_and_stroke_commands() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(24.0, 24.0));
        let accent = Color32::from_rgb(60, 90, 120);

        let cmds = ribbon_style_chip_paint_cmds(rect, accent, true, false);

        assert!(matches!(
            cmds[0],
            PaintCmd::RectFilled {
                rect: got_rect,
                ..
            } if got_rect == rect
        ));
        assert!(matches!(
            cmds[1],
            PaintCmd::RectStroke {
                rect: got_rect,
                stroke,
                ..
            } if got_rect == rect && stroke.color == accent
        ));
    }

    #[test]
    fn fullscreen_arrow_glyph_lowers_to_mara_line_and_polygon_commands() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(24.0, 24.0));
        let accent = Color32::from_rgb(60, 90, 120);

        let cmds = fullscreen_arrow_paint_cmds(rect, accent, false, true);

        assert_eq!(cmds.len(), 3);
        assert!(matches!(
            cmds[0],
            PaintCmd::Line {
                stroke,
                ..
            } if stroke.color == accent
        ));
        assert!(matches!(
            &cmds[1],
            PaintCmd::Polygon {
                points,
                fill,
                stroke,
            } if points.len() == 3 && *fill == accent && *stroke == MaraStroke::NONE
        ));
        assert!(matches!(
            &cmds[2],
            PaintCmd::Polygon {
                points,
                fill,
                stroke,
            } if points.len() == 3 && *fill == accent && *stroke == MaraStroke::NONE
        ));
    }
}
