//! Shared "maximise this widget to full window" wrapper.
//!
//! Graph canvases, code editors, and similar "canvas-shaped"
//! widgets benefit from a one-click lift to full window that
//! leaves their surrounding pane and container untouched. This
//! module provides exactly that, in a widget-agnostic form:
//!
//! ```ignore
//! maximizable(ui, "my_widget", accent, egui::vec2(w, 300.0), |ui| {
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
//!   the full `ctx.content_rect()` with a mara glass frame.
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

use crate::ribbon::{RibbonCluster, RibbonEdge};
use crate::style::{
    RadiusRole, StrokeRole, glass_alpha_window, glass_fill, radius_for, stroke_for,
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
/// upper-right corner, mirroring the legacy maximise chip placement.
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
pub fn maximize_state_key(id_salt: impl std::hash::Hash) -> egui::Id {
    egui::Id::new(("mara_maximize", id_salt))
}

fn pending_restore_fullscreen_key() -> egui::Id {
    egui::Id::new("mara_pending_restore_fullscreen")
}

/// Globally unique id used by the maximizer to track "is some
/// widget currently full-window, and which one?". Reads
/// `Some(owner_state_key)` if a maximizable widget rendered itself
/// in the maximized branch this pass (or last pass — matches the
/// 1-frame freshness window the chip-suppression code uses), else
/// `None`. Hosts use this to gate background rendering: while a
/// fullscreen owner is active they can skip drawing the rest of
/// the egui UI (other panes, ribbons) AND skip the Bevy 3D scene
/// behind, so the fullscreen view truly owns the screen.
pub fn fullscreen_owner(ctx: &egui::Context) -> Option<egui::Id> {
    let global_key = egui::Id::new("mara_maximize_global");
    let pass_nr = ctx.cumulative_pass_nr();
    let stored: Option<(u64, egui::Id)> = ctx.data(|d| d.get_temp(global_key));
    match stored {
        Some((f, id)) if f == pass_nr || f + 1 == pass_nr => Some(id),
        _ => None,
    }
}

/// Convenience predicate over [`fullscreen_owner`] — true when
/// SOME maximizable widget is currently in the full-window state.
pub fn is_any_fullscreen(ctx: &egui::Context) -> bool {
    fullscreen_owner(ctx).is_some()
}

fn suppress_fullscreen_minimize_chip_key() -> egui::Id {
    egui::Id::new("mara_suppress_fullscreen_minimize_chip")
}

/// Hide/show the built-in fullscreen restore chip for this frame.
///
/// Host shells that provide their own persistent app/module bar
/// should call this with `false` before rendering maximizable
/// content, then provide restore through their normal chrome. This
/// keeps fullscreen modules from growing a second floating restore
/// button on top of the persistent bar.
pub fn set_fullscreen_minimize_chip_visible(ctx: &egui::Context, visible: bool) {
    ctx.data_mut(|d| {
        d.insert_temp::<bool>(suppress_fullscreen_minimize_chip_key(), !visible);
    });
}

/// Restore the active full-window maximizable widget, if one exists.
///
/// Returns `true` when a fullscreen owner was found and toggled off.
pub fn restore_fullscreen(ctx: &egui::Context) -> bool {
    let Some(owner) = fullscreen_owner(ctx) else {
        return false;
    };
    ctx.data_mut(|d| {
        d.insert_temp::<egui::Id>(pending_restore_fullscreen_key(), owner);
    });
    true
}

/// Wrap a widget so it gains a maximise / restore toggle.
///
/// Call once per frame with the same `id_salt`. `min_size` is the
/// rect the body renders into while inline; when maximised the
/// body fills `ctx.content_rect()` instead.
pub fn maximizable(
    ui: &mut egui::Ui,
    id_salt: impl Hash + Copy,
    accent: egui::Color32,
    min_size: egui::Vec2,
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
    accent: egui::Color32,
    min_size: egui::Vec2,
    opts: OverlayOpts,
    body: impl FnOnce(&mut egui::Ui),
) {
    // Maximise state keyed purely on the caller's `id_salt` — no
    // `ui.id()` mixed in — so the host can reconstruct the same
    // key from the outside via [`is_maximized`] and route Ctrl+K
    // / context-sensitive logic based on "is THIS widget
    // currently full-window?".
    let max_id = maximize_state_key(id_salt);
    let mut maximized: bool = ui
        .ctx()
        .data(|d| d.get_temp::<bool>(max_id))
        .unwrap_or(false);
    let pending_restore = ui
        .ctx()
        .data(|d| d.get_temp::<egui::Id>(pending_restore_fullscreen_key()))
        == Some(max_id);
    if pending_restore {
        maximized = false;
        ui.ctx().data_mut(|d| {
            d.insert_temp::<bool>(max_id, false);
            d.remove::<egui::Id>(pending_restore_fullscreen_key());
            d.remove::<(u64, egui::Id)>(egui::Id::new("mara_maximize_global"));
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
    let stored_global: Option<(u64, egui::Id)> = ui.ctx().data(|d| d.get_temp(global_key));
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
        let (rect, _) = ui.allocate_exact_size(min_size, egui::Sense::hover());
        if ui.is_rect_visible(rect) {
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                overlay.placeholder_text,
                egui::FontId::proportional(12.0),
                crate::style::on_section_dim(),
            );
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
        let screen = ctx.content_rect();
        let content = opts.content_avoidance.apply_to_rect(screen);
        egui::Area::new(ui.id().with(("mara_maximize_overlay", id_salt)))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(&ctx, |ui| {
                ui.set_min_size(screen.size());
                ui.set_max_size(screen.size());
                let bg_rect = egui::Rect::from_min_size(screen.min, screen.size());
                let bg = crate::style::theme().bg_panel;
                let opaque_bg = egui::Color32::from_rgb(bg.r(), bg.g(), bg.b());
                ui.painter()
                    .rect_filled(bg_rect, egui::CornerRadius::ZERO, opaque_bg);
                ui.allocate_rect(bg_rect, egui::Sense::hover());
            });
        egui::Area::new(ui.id().with(("mara_maximize_overlay_content", id_salt)))
            .order(egui::Order::Foreground)
            .fixed_pos(content.min)
            .show(&ctx, |ui| {
                ui.set_min_size(content.size());
                ui.set_max_size(content.size());
                body(ui);
            });
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
        let desired = egui::vec2(ui.available_width().max(min_size.x), min_size.y);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        body(&mut child);

        // Suppress the chip while another widget is full-window —
        // its overlay covers the screen and our `Order::Tooltip` chip
        // would otherwise paint on top of nothing.
        if !some_other_maximized {
            let chip_pos = egui::pos2(
                rect.max.x - overlay.inline_chip_size - overlay.inline_chip_pad,
                rect.min.y + overlay.inline_chip_pad,
            );
            if max_button_overlay(ui.ctx(), chip_pos, false, accent, id_salt).clicked() {
                toggle = true;
            }
        }
    }

    if toggle {
        ui.ctx()
            .data_mut(|d| d.insert_temp::<bool>(max_id, !maximized));
    }
}

/// The 24 px maximise / restore chip. Lives in its own
/// `Order::Tooltip` Area so it paints (and intercepts clicks)
/// above the wrapped widget's own shapes — Areas at the same
/// `Foreground` order would get shadowed by canvas widgets like
/// the graph graph that register their own foreground sub-layers.
fn max_button_overlay(
    ctx: &egui::Context,
    pos: egui::Pos2,
    maximized: bool,
    accent: egui::Color32,
    id_salt: impl Hash + Copy,
) -> egui::Response {
    let btn = crate::style::theme().overlay.inline_chip_size;
    let area_id = egui::Id::new("mara_maximize_btn").with(id_salt);
    let inner = egui::Area::new(area_id)
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(btn, btn), egui::Sense::click());
            let resp = resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(if maximized { "Restore" } else { "Maximize" });
            if ui.is_rect_visible(rect) {
                paint_ribbon_style_chip(
                    ui.painter(),
                    rect,
                    accent,
                    /* active */ maximized,
                    /* hovered */ resp.hovered(),
                );
                paint_fullscreen_arrows(
                    ui.painter(),
                    rect,
                    accent,
                    /* inward */ maximized,
                    /* hovered */ resp.hovered(),
                );
            }
            resp
        });
    inner.inner
}

/// Compute the upper-left position of the minimize chip given the
/// (edge, cluster) anchor + button size + edge inset. Mirrors the
/// 12-position rail system used by [`crate::ribbon`]:
///
/// * Cluster `Start` ⇒ corner closest to `(left, top)` along the edge.
/// * Cluster `End`   ⇒ opposite corner.
/// * Cluster `Middle`⇒ centred along the edge.
fn compute_chip_pos(
    screen: egui::Rect,
    edge: RibbonEdge,
    cluster: RibbonCluster,
    btn: f32,
    edge_gap: f32,
) -> egui::Pos2 {
    let along_axis_pos = |min: f32, max: f32| -> f32 {
        match cluster {
            RibbonCluster::Start => min + edge_gap,
            RibbonCluster::End => max - edge_gap - btn,
            RibbonCluster::Middle => (min + max) * 0.5 - btn * 0.5,
        }
    };
    match edge {
        RibbonEdge::Top => egui::pos2(
            along_axis_pos(screen.left(), screen.right()),
            screen.top() + edge_gap,
        ),
        RibbonEdge::Bottom => egui::pos2(
            along_axis_pos(screen.left(), screen.right()),
            screen.bottom() - edge_gap - btn,
        ),
        RibbonEdge::Left => egui::pos2(
            screen.left() + edge_gap,
            along_axis_pos(screen.top(), screen.bottom()),
        ),
        RibbonEdge::Right => egui::pos2(
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
    screen: egui::Rect,
    opts: OverlayOpts,
    btn_size: f32,
    edge_gap: f32,
    accent: egui::Color32,
    id_salt: impl Hash + Copy,
) -> bool {
    // Persisted user-chosen anchor (set on drag-release). When
    // empty, fall back to the caller-supplied `opts`.
    let anchor_key = egui::Id::new("mara_maximize_chip_anchor").with(id_salt);
    let stored: Option<(RibbonEdge, RibbonCluster)> = ctx.data(|d| d.get_temp(anchor_key));
    let active_anchor = stored.unwrap_or((opts.minimize_edge, opts.minimize_cluster));
    // While the user is mid-drag, override the chip position with
    // the cursor (so the chip follows the pointer) — keyed by the
    // SAME id so the value clears on release.
    let drag_pos_key = egui::Id::new("mara_maximize_chip_drag_pos").with(id_salt);
    let drag_cursor: Option<egui::Pos2> = ctx.data(|d| d.get_temp(drag_pos_key));
    let chip_pos = if let Some(c) = drag_cursor {
        egui::pos2(c.x - btn_size * 0.5, c.y - btn_size * 0.5)
    } else {
        compute_chip_pos(screen, active_anchor.0, active_anchor.1, btn_size, edge_gap)
    };

    let area_id = egui::Id::new("mara_maximize_minimize").with(id_salt);
    let inner = egui::Area::new(area_id)
        .order(egui::Order::Tooltip)
        .fixed_pos(chip_pos)
        .interactable(true)
        .show(ctx, |ui| {
            let (rect, resp) = ui.allocate_exact_size(
                egui::vec2(btn_size, btn_size),
                egui::Sense::click_and_drag(),
            );
            // Cursor stays the default pointing-hand egui picks for
            // clickable widgets — same as the main-page ribbon
            // buttons. The button is click-first, drag-second; the
            // user shouldn't see a "grab" cursor on hover that
            // suggests "drag-only".
            let resp = resp.on_hover_text("Restore");
            // Drag tracking — write the cursor position to ctx data
            // each frame the chip is being dragged. On release,
            // snap to the nearest anchor and persist it.
            let mut ghost_target: Option<egui::Rect> = None;
            if resp.dragged()
                && let Some(p) = ui.ctx().pointer_interact_pos()
            {
                ui.ctx().data_mut(|d| d.insert_temp(drag_pos_key, p));
                // Compute the live snap target so we can paint
                // a ghost outline showing where the chip WILL
                // land on release.
                let snap = nearest_anchor(screen, p, btn_size, edge_gap);
                let snap_pos = compute_chip_pos(screen, snap.0, snap.1, btn_size, edge_gap);
                ghost_target = Some(egui::Rect::from_min_size(
                    snap_pos,
                    egui::vec2(btn_size, btn_size),
                ));
            }
            if resp.drag_stopped() {
                let cursor = ui
                    .ctx()
                    .pointer_interact_pos()
                    .unwrap_or_else(|| rect.center());
                let snapped = nearest_anchor(screen, cursor, btn_size, edge_gap);
                ui.ctx().data_mut(|d| {
                    d.insert_temp(anchor_key, snapped);
                    d.remove::<egui::Pos2>(drag_pos_key);
                });
            }
            if ui.is_rect_visible(rect) {
                paint_ribbon_style_chip(
                    ui.painter(),
                    rect,
                    accent,
                    /* active */ true,
                    /* hovered */ resp.hovered(),
                );
                let glyph_col = if resp.hovered() {
                    crate::style::contrast_text_for(accent)
                } else {
                    egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220)
                };
                crate::icons::paint_icon(
                    ui.painter(),
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "arrow-minimize",
                    btn_size * crate::style::theme().icons.overlay_icon_scale,
                    glyph_col,
                );
            }
            // Ghost preview at the snap target — a low-alpha
            // accent rect with a dashed-style accent border, painted
            // on its OWN tooltip-layer painter (clip = full screen)
            // so it doesn't get clipped by the chip's tiny area.
            if let Some(g) = ghost_target {
                let ghost_layer = egui::LayerId::new(
                    egui::Order::Tooltip,
                    egui::Id::new(("mara_maximize_chip_ghost", id_salt)),
                );
                let ghost_painter = egui::Painter::new(ui.ctx().clone(), ghost_layer, screen);
                let overlay = crate::style::theme().overlay;
                let ghost_fill = egui::Color32::from_rgba_unmultiplied(
                    accent.r(),
                    accent.g(),
                    accent.b(),
                    overlay.ghost_fill_alpha,
                );
                let ghost_stroke =
                    egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 180);
                ghost_painter.rect(
                    g,
                    radius_for(RadiusRole::Section),
                    ghost_fill,
                    egui::Stroke::new(overlay.ghost_stroke_width, ghost_stroke),
                    egui::StrokeKind::Inside,
                );
            }
            resp
        });
    // Only treat as a "restore" click when the gesture wasn't a drag.
    let resp = inner.inner;
    resp.clicked() && drag_cursor.is_none()
}

/// Find the (edge, cluster) anchor whose chip-centre is closest to
/// `cursor`. Used by the drag-release snap so the chip lands on
/// one of the 12 fixed anchors.
fn nearest_anchor(
    screen: egui::Rect,
    cursor: egui::Pos2,
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
        let centre = egui::pos2(p.x + btn_size * 0.5, p.y + btn_size * 0.5);
        let d = (centre - cursor).length();
        if d < best_d {
            best_d = d;
            best = (e, c);
        }
    }
    best
}

/// Mirror of `ribbon::paint::paint_ribbon_button` (which is
/// `pub(crate)` so we can't call it directly). Same glass tiers
/// and active / hover transitions — keeps the chip in the ribbon
/// button family.
fn paint_ribbon_style_chip(
    painter: &egui::Painter,
    rect: egui::Rect,
    accent: egui::Color32,
    active: bool,
    hovered: bool,
) {
    let bg = if active {
        let blend = |a: u8, b: u8| ((a as f32) * 0.75 + (b as f32) * 0.25).round() as u8;
        let tinted = egui::Color32::from_rgb(
            blend(crate::style::theme().bg_raised.r(), accent.r()),
            blend(crate::style::theme().bg_raised.g(), accent.g()),
            blend(crate::style::theme().bg_raised.b(), accent.b()),
        );
        glass_fill(tinted, accent, glass_alpha_window())
    } else if hovered {
        glass_fill(
            crate::style::theme().bg_raised,
            accent,
            glass_alpha_window(),
        )
    } else {
        glass_fill(crate::style::theme().bg_panel, accent, glass_alpha_window())
    };
    let stroke = if active {
        egui::Stroke::new(crate::style::theme().stroke.border_width, accent)
    } else {
        stroke_for(StrokeRole::WidgetBorder, accent)
    };
    painter.rect(
        rect,
        radius_for(RadiusRole::Section),
        bg,
        stroke,
        egui::StrokeKind::Inside,
    );
}

/// Paint the fullscreen glyph — single diagonal line through the
/// chip's centre, arrowheads at each end. `inward = false` heads
/// point OUT (maximise); `inward = true` heads point IN (restore).
fn paint_fullscreen_arrows(
    painter: &egui::Painter,
    rect: egui::Rect,
    accent: egui::Color32,
    inward: bool,
    hovered: bool,
) {
    let color = if inward {
        crate::style::on_section()
    } else if hovered {
        accent
    } else {
        egui::Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 220)
    };
    let icons = crate::style::theme().icons;
    let stroke_w = icons.overlay_arrow_stroke_w;
    let shrunk = rect.shrink(icons.overlay_arrow_shrink);
    let center = rect.center();
    let ne_corner = egui::pos2(shrunk.max.x, shrunk.min.y);
    let sw_corner = egui::pos2(shrunk.min.x, shrunk.max.y);
    let lerp = |a: egui::Pos2, b: egui::Pos2, t: f32| -> egui::Pos2 {
        egui::pos2(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t)
    };
    let t = icons.overlay_arrow_tip_t;
    let ne_tip = lerp(center, ne_corner, t);
    let sw_tip = lerp(center, sw_corner, t);
    painter.line_segment([sw_tip, ne_tip], egui::Stroke::new(stroke_w, color));
    let (from_ne, from_sw) = if inward {
        (ne_corner, sw_corner)
    } else {
        (center, center)
    };
    paint_arrowhead(painter, from_ne, ne_tip, color);
    paint_arrowhead(painter, from_sw, sw_tip, color);
}

fn paint_arrowhead(
    painter: &egui::Painter,
    from: egui::Pos2,
    tip: egui::Pos2,
    color: egui::Color32,
) {
    let dir = tip - from;
    let len = dir.length().max(1e-3);
    let dir = dir / len;
    let perp = egui::vec2(-dir.y, dir.x);
    let icons = crate::style::theme().icons;
    let head_len = icons.overlay_arrow_head_len;
    let head_half_w = icons.overlay_arrow_head_half_w;
    let back = tip - dir * head_len;
    let p1 = back + perp * head_half_w;
    let p2 = back - perp * head_half_w;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, p1, p2],
        color,
        egui::Stroke::NONE,
    ));
}
