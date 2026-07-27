use crate::context::MaraCtx;
use egui::Context;

use super::{
    RibbonAction, RibbonCluster, RibbonDrag, RibbonEdge, RibbonOpen, RibbonPlacement, RibbonScope,
    RibbonSlotDef, RibbonSlotItem,
    chrome::draw_unified_ribbon_chrome,
    paint::{paint_ribbon_glyph, ribbon_button_fg, ribbon_button_paint_cmds},
    resolve_slot_items,
};
use crate::layout::{Layer, Sense as MaraSense, SlotRibbonLayoutSpec};
use crate::vocab::{Color32 as MaraColor32, Id as MaraId, Rect as MaraRect};

#[cfg(test)]
use crate::vocab::{Pos2 as MaraPos2, Vec2 as MaraVec2};

const LEFT_SHELF_RIBBON_CHROME_ID: &str = "mara.system.left_shelf.ribbon";
const LEFT_SHELF_ITEM_CHROME_ID: &str = "mara.system.left_shelf.item";
const RIGHT_SHELF_RIBBON_CHROME_ID: &str = "mara.system.right_shelf.ribbon";
const RIGHT_SHELF_ITEM_CHROME_ID: &str = "mara.system.right_shelf.item";
const BOTTOM_SHELF_RIBBON_CHROME_ID: &str = "mara.system.bottom_shelf.ribbon";
const BOTTOM_SHELF_ITEM_CHROME_ID: &str = "mara.system.bottom_shelf.item";
const MAXIMIZE_RIBBON_CHROME_ID: &str = "mara.system.maximize.ribbon";
const MAXIMIZE_ITEM_CHROME_ID: &str = "mara.system.maximize.item";
const CLOSE_RIBBON_CHROME_ID: &str = "mara.system.close.ribbon";
const CLOSE_ITEM_CHROME_ID: &str = "mara.system.close.item";

#[derive(Clone, Debug)]
pub struct ResolvedSlotRibbon {
    pub id: MaraId,
    pub chrome_id: Option<&'static str>,
    pub scope: RibbonScope,
    pub edge: RibbonEdge,
    pub role: super::RibbonRole,
    pub mode: super::RibbonMode,
    pub cluster: RibbonCluster,
    pub accepts: &'static [&'static str],
    pub items: Vec<RibbonSlotItem>,
}

#[derive(Clone, Debug)]
pub struct RibbonSlotClick {
    pub ribbon: MaraId,
    pub item: MaraId,
    pub action: RibbonAction,
}

/// Internal egui hook for painting already-resolved slot ribbons as
/// simple icon/button rails.
///
/// Host/app code should route through Mara shell/host facades instead
/// of calling this raw backend helper directly.
#[doc(hidden)]
pub fn __internal_draw_slot_ribbons_egui(
    ctx: &Context,
    accent: impl Into<MaraColor32>,
    ribbons: &[ResolvedSlotRibbon],
) -> Vec<RibbonSlotClick> {
    crate::enforce::__internal_enforce_defaults(ctx);
    let accent: MaraColor32 = accent.into();
    let augmented = shelf_augmented_ribbons(ctx, ribbons, ShelfButtonOrder::Simple);
    let base = augmented.as_deref().unwrap_or(ribbons);
    let responsive = responsive_phone_ribbons(ctx, base);
    let ribbons = responsive.as_deref().unwrap_or(base);
    draw_slot_ribbons_inner(ctx, accent, ribbons)
}

fn draw_slot_ribbons_inner(
    ctx: &Context,
    accent: MaraColor32,
    ribbons: &[ResolvedSlotRibbon],
) -> Vec<RibbonSlotClick> {
    assert_resolved_ribbon_icons(ribbons);
    let mut clicks = Vec::new();
    for ribbon in ribbons {
        draw_one_slot_ribbon(ctx, accent, ribbon, ribbons, &mut clicks);
    }
    clicks
}

/// Internal egui hook for drawing slot ribbons through the featureful chrome path whenever
/// the slot declarations provide static chrome ids.
///
/// This is the convergence point for the single ribbon API and the
/// featureful renderer capabilities: drag/reorder, cross-ribbon
/// placement, panel toggle state, pane anchoring, and fullscreen rail
/// layering are preserved. If any ribbon/item lacks a chrome id, the
/// function falls back to the simple slot painter for that frame.
///
/// Host/app code should route through Mara shell/host facades instead
/// of calling this raw backend helper directly.
#[doc(hidden)]
pub fn __internal_draw_slot_ribbons_featureful_egui(
    ctx: &Context,
    accent: impl Into<MaraColor32>,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
) -> Vec<RibbonSlotClick> {
    crate::enforce::__internal_enforce_defaults(ctx);
    draw_slot_ribbons_featureful_inner(ctx, accent.into(), ribbons, open, placement, drag, true)
}

/// Like [`__internal_draw_slot_ribbons_featureful_egui`] but without
/// injecting system chrome (window controls + shelf toggles). App-level
/// ribbon renders use this so only the enforced shell bar owns those
/// buttons — otherwise every featureful render re-injects them and they
/// appear once per render call (doubled maximize/close + shelf toggles).
pub fn __internal_draw_slot_ribbons_featureful_no_system_egui(
    ctx: &Context,
    accent: impl Into<MaraColor32>,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
) -> Vec<RibbonSlotClick> {
    crate::enforce::__internal_enforce_defaults(ctx);
    draw_slot_ribbons_featureful_inner(ctx, accent.into(), ribbons, open, placement, drag, false)
}

/// Resolve one leaf's [`RibbonSlotDef`] into a drawable
/// [`ResolvedSlotRibbon`]. A leaf owns its ribbons directly, so there are
/// no override layers to apply. Returns `None` when the def resolves to
/// no items (nothing to draw).
pub(crate) fn resolve_leaf_ribbon(def: &RibbonSlotDef) -> Option<ResolvedSlotRibbon> {
    let items: Vec<RibbonSlotItem> = def
        .slots
        .iter()
        .flat_map(|slot| resolve_slot_items(slot, &[]))
        .collect();
    if items.is_empty() {
        return None;
    }
    Some(ResolvedSlotRibbon {
        id: MaraId::new((def.id, def.cluster)),
        chrome_id: def.chrome_id,
        scope: def.scope,
        edge: def.edge,
        role: def.role,
        mode: def.mode,
        cluster: def.cluster,
        accepts: def.accepts,
        items,
    })
}

/// Draw a single view node's own ribbons (left/right/bottom), anchored to
/// its `region` rather than the window: the ribbons are children of the
/// view — when the view moves or shrinks, they move WITH it. Per-view
/// open/placement/drag state is keyed by `salt` (the node's stable
/// identity, e.g. its workspace id), NOT by screen coordinates, so the
/// state also travels with the view across resizes and re-layouts. No
/// system chrome is injected — only the shell bar owns window controls.
/// Returns the clicks the caller dispatches.
pub(crate) fn __internal_draw_view_ribbons(
    ctx: &Context,
    region: MaraRect,
    salt: MaraId,
    accent: MaraColor32,
    defs: &[RibbonSlotDef],
) -> Vec<RibbonSlotClick> {
    let ribbons: Vec<ResolvedSlotRibbon> = defs
        .iter()
        .filter_map(resolve_leaf_ribbon)
        .map(|mut ribbon| {
            // Per-view ribbons exist on left/right/bottom ONLY — the top
            // edge belongs to the shell bar. A leaf declaring Top is
            // remapped to Bottom so it can never stack under the top bar.
            if ribbon.edge == RibbonEdge::Top {
                ribbon.edge = RibbonEdge::Bottom;
            }
            ribbon
        })
        .collect();
    if ribbons.is_empty() {
        return Vec::new();
    }
    let memory = MaraCtx::memory(ctx);
    let mut open: RibbonOpen = memory
        .get_temp(view_ribbon_open_key(salt))
        .unwrap_or_default();
    let mut placement: RibbonPlacement = memory
        .get_temp(view_ribbon_placement_key(salt))
        .unwrap_or_default();
    let mut drag: RibbonDrag = memory
        .get_temp(view_ribbon_drag_key(salt))
        .unwrap_or_default();

    if crate::probe::__internal_enabled(ctx) {
        crate::probe::__internal_record(
            ctx,
            crate::probe::ElementPose::new("view-ribbon-region", region).with_id(salt),
        );
    }

    // ONE anchoring backend for ALL rails: a view's ribbons anchor to
    // its region intersected with the window chrome bounds — the exact
    // same bounds the window-level rail pass lays out against (top-bar
    // strip + shelf reservation, see `chrome::fresh_chrome_bounds`). A
    // full-window leaf (a solo view — the one-leaf tree) therefore
    // aligns its rails IDENTICALLY to a window rail pass, and a cell
    // deeper in the tree clips to its own rect. No per-path special
    // cases. The body/backdrop still uses the full region — only the
    // button anchoring is bounded.
    let chrome_bounds = super::chrome::fresh_chrome_bounds(ctx);
    let mut anchor = region.intersect(chrome_bounds);
    if anchor.width() <= 0.0 || anchor.height() <= 0.0 {
        anchor = region;
    } else if anchor.min.y > chrome_bounds.min.y {
        // Side rails start flush at the anchor's top because the chrome
        // bound already carries breathing room below the top bar. When
        // the anchor's top is an INTERIOR split line instead (a lower
        // cell of a horizontal split), inset it by the standard edge
        // margin — the same EDGE_GAP every rail keeps from the bottom
        // edge — so the first button clears the divider identically.
        anchor.min.y = (anchor.min.y + super::paint::EDGE_GAP).min(anchor.max.y);
    }

    // Publish the anchor region so the shared renderer anchors these
    // ribbons to the node's rect (see `chrome::ribbon_rect`).
    let clicks = crate::embed::__internal_with_node_region(ctx, anchor, || {
        __internal_draw_slot_ribbons_featureful_no_system_egui(
            ctx,
            accent,
            &ribbons,
            &mut open,
            &mut placement,
            &mut drag,
        )
    });

    // Publish which edges this node's ribbons occupy, so the node's
    // `content_rect` can shrink its body away from them (view-local
    // avoidance, keyed by the node's identity so each cell is
    // independent). Stamped with the pass number so a stale entry (an
    // earlier tab, an old layout) can never leak into a later pass.
    let has = |edge: RibbonEdge| ribbons.iter().any(|r| r.edge == edge);
    let edges = [
        has(RibbonEdge::Left),
        has(RibbonEdge::Right),
        has(RibbonEdge::Top),
        has(RibbonEdge::Bottom),
    ];

    let pass_nr = MaraCtx::pass_nr(ctx);
    let mut memory = MaraCtx::memory(ctx);
    memory.set_temp(view_ribbon_open_key(salt), open);
    memory.set_temp(view_ribbon_placement_key(salt), placement);
    memory.set_temp(view_ribbon_drag_key(salt), drag);
    memory.set_temp::<(u64, [bool; 4])>(view_ribbon_edges_key(salt), (pass_nr, edges));
    gc_view_ribbon_state(&mut memory, salt, pass_nr);
    clicks
}

/// Per-view ribbon state keys. Every reader and writer constructs the
/// key through THESE fns — never inline — so they cannot drift apart.
fn view_ribbon_open_key(salt: MaraId) -> crate::vocab::Id {
    crate::vocab::Id::new(("mara_view_ribbon_open", salt))
}

fn view_ribbon_placement_key(salt: MaraId) -> crate::vocab::Id {
    crate::vocab::Id::new(("mara_view_ribbon_placement", salt))
}

fn view_ribbon_drag_key(salt: MaraId) -> crate::vocab::Id {
    crate::vocab::Id::new(("mara_view_ribbon_drag", salt))
}

fn view_ribbon_edges_key(salt: MaraId) -> crate::vocab::Id {
    crate::vocab::Id::new(("mara_view_ribbon_edges", salt))
}

fn view_ribbon_salts_registry_key() -> crate::vocab::Id {
    crate::vocab::Id::new("mara_view_ribbon_salts")
}

/// How many distinct view salts may keep ribbon state before the oldest
/// (by last-drawn pass) is evicted. Open/placement/drag must SURVIVE a
/// tab going inactive (switching back restores the open panel), so the
/// state cannot be pass-stamped away like `edges` — instead the live
/// set is bounded so hosts that mint dynamic salts can't grow memory
/// forever.
const VIEW_RIBBON_STATE_CAP: usize = 256;

/// Bound the per-salt ribbon state set: update `salt`'s last-drawn pass
/// in the registry, and evict the stalest salts' state once the
/// registry exceeds [`VIEW_RIBBON_STATE_CAP`].
fn gc_view_ribbon_state(memory: &mut crate::memory::MaraMemoryCtx<'_>, salt: MaraId, pass_nr: u64) {
    let mut registry: Vec<(MaraId, u64)> = memory
        .get_temp(view_ribbon_salts_registry_key())
        .unwrap_or_default();
    match registry.iter_mut().find(|(id, _)| *id == salt) {
        Some(entry) => entry.1 = pass_nr,
        None => registry.push((salt, pass_nr)),
    }
    while registry.len() > VIEW_RIBBON_STATE_CAP {
        let Some(stalest) = registry
            .iter()
            .enumerate()
            .min_by_key(|(_, (_, pass))| *pass)
            .map(|(idx, _)| idx)
        else {
            break;
        };
        let (dead, _) = registry.swap_remove(stalest);
        memory.remove_temp::<RibbonOpen>(view_ribbon_open_key(dead));
        memory.remove_temp::<RibbonPlacement>(view_ribbon_placement_key(dead));
        memory.remove_temp::<RibbonDrag>(view_ribbon_drag_key(dead));
        memory.remove_temp::<(u64, [bool; 4])>(view_ribbon_edges_key(dead));
    }
    memory.set_temp(view_ribbon_salts_registry_key(), registry);
}

/// Which edges (`[left, right, top, bottom]`) the node identified by
/// `salt` drew its own ribbons on THIS pass, if any. Used by
/// `ViewCtx::content_rect` to inset the body away from the node's own
/// ribbons. Entries from earlier passes are ignored — a view that
/// stopped drawing ribbons (or a tab that went inactive) must not keep
/// shrinking anyone's content rect.
pub(crate) fn view_ribbon_edges(ctx: &dyn crate::context::MaraCtx, salt: MaraId) -> Option<[bool; 4]> {
    let (pass, edges) = ctx.memory()
        .get_temp::<(u64, [bool; 4])>(view_ribbon_edges_key(salt))?;
    (pass == ctx.pass_nr()).then_some(edges)
}

fn draw_slot_ribbons_featureful_inner(
    ctx: &Context,
    accent: MaraColor32,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
    inject_system_chrome: bool,
) -> Vec<RibbonSlotClick> {
    let augment = |order| {
        if inject_system_chrome {
            shelf_augmented_ribbons(ctx, ribbons, order)
        } else {
            None
        }
    };
    let featureful_augmented = augment(ShelfButtonOrder::Featureful);
    let featureful_base = featureful_augmented.as_deref().unwrap_or(ribbons);
    let featureful_responsive = responsive_phone_ribbons(ctx, featureful_base);
    let featureful_ribbons = featureful_responsive.as_deref().unwrap_or(featureful_base);
    if !can_use_featureful_chrome(featureful_ribbons) {
        let simple_augmented = augment(ShelfButtonOrder::Simple);
        let simple_base = simple_augmented.as_deref().unwrap_or(ribbons);
        let simple_responsive = responsive_phone_ribbons(ctx, simple_base);
        return draw_slot_ribbons_inner(
            ctx,
            accent,
            simple_responsive.as_deref().unwrap_or(simple_base),
        );
    }

    let ribbons = featureful_ribbons;
    assert_resolved_ribbon_icons(ribbons);
    let clicks = draw_unified_ribbon_chrome(ctx, accent, ribbons, open, placement, drag, |id| {
        ribbons
            .iter()
            .flat_map(|ribbon| ribbon.items.iter())
            .find(|item| item.chrome_id == Some(id))
            .is_some_and(|item| item.active)
    });
    clicks
        .into_iter()
        .filter_map(|click| {
            ribbons
                .iter()
                .find_map(|ribbon| {
                    ribbon
                        .items
                        .iter()
                        .find(|item| item.id == click)
                        .map(|item| (ribbon.id, item))
                })
                .map(|(ribbon_id, item)| RibbonSlotClick {
                    ribbon: ribbon_id,
                    item: item.id,
                    action: item.action,
                })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShelfButtonOrder {
    /// Simple slot painting lays horizontal end-cluster items left to
    /// right, so the close/restore button stays at the outer edge when
    /// the shelf button is inserted before it.
    Simple,
    /// Featureful chrome lays end-cluster slot zero at the outer edge,
    /// so the close/restore button stays at the outer edge when the
    /// shelf button is inserted after it.
    Featureful,
}

fn shelf_augmented_ribbons(
    ctx: &Context,
    ribbons: &[ResolvedSlotRibbon],
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    let visible_layout = crate::shelf::__internal_shelf_layout(ctx);
    // Phones don't need app window controls (the OS / browser owns
    // close + maximize), so hide both on phone-class. Computed here and
    // passed in so the augmentation stays a pure function of its args.
    let hide_window_controls = crate::style::screen_class() == crate::style::Breakpoint::Phone;
    let maximized = crate::backend::egui::viewport_maximized(ctx);
    augment_shelf_buttons_with_chrome(
        ribbons,
        crate::window_chrome::__internal_window_chrome_host_capabilities(ctx),
        crate::shelf::published_shelf_presence(ctx),
        visible_layout.is_some_and(|layout| layout.left.is_some()),
        visible_layout.is_some_and(|layout| layout.right.is_some()),
        visible_layout.is_some_and(|layout| layout.bottom.is_some()),
        order,
        maximized,
        hide_window_controls,
    )
}

#[cfg(test)]
fn augment_shelf_buttons(
    ribbons: &[ResolvedSlotRibbon],
    presence: crate::shelf::ShelfPresence,
    left_visible: bool,
    right_visible: bool,
    bottom_visible: bool,
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    augment_shelf_buttons_with_chrome(
        ribbons,
        crate::window_chrome::WindowChromeHostCapabilities {
            system_maximize: false,
            system_close: false,
            ..Default::default()
        },
        presence,
        left_visible,
        right_visible,
        bottom_visible,
        order,
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn augment_shelf_buttons_with_chrome(
    ribbons: &[ResolvedSlotRibbon],
    chrome: crate::window_chrome::WindowChromeHostCapabilities,
    presence: crate::shelf::ShelfPresence,
    left_visible: bool,
    right_visible: bool,
    bottom_visible: bool,
    order: ShelfButtonOrder,
    maximized: bool,
    hide_window_controls: bool,
) -> Option<Vec<ResolvedSlotRibbon>> {
    let has_left = presence.left;
    let has_right = presence.right;
    let has_bottom = presence.bottom;
    let mut out = ribbons.to_vec();
    let mut changed = false;
    if chrome.system_maximize && !hide_window_controls && !contains_item(&out, maximize_item_id()) {
        insert_maximize_button(&mut out, maximized);
        changed = true;
    }
    if chrome.system_close && !hide_window_controls && !contains_item(&out, close_item_id()) {
        insert_close_button(&mut out, order);
        changed = true;
    }
    // Shelf toggle buttons attach to an existing permanent top bar
    // (possibly the system-button bar created just above) — they never
    // conjure one out of side rails alone. System window controls are
    // different: a Mara-native window must always expose close/restore,
    // so those may create their own ribbon.
    if !out.iter().any(is_top_permanent_ribbon) {
        return changed.then_some(out);
    }
    if has_left && !contains_item(&out, left_shelf_item_id()) {
        insert_left_shelf_button(&mut out, left_visible);
        changed = true;
    }
    if has_right && !contains_item(&out, right_shelf_item_id()) {
        insert_right_shelf_button(&mut out, right_visible, order);
        changed = true;
    }
    if has_bottom && !contains_item(&out, bottom_shelf_item_id()) {
        insert_bottom_shelf_button(&mut out, bottom_visible, order);
        changed = true;
    }
    changed.then_some(out)
}

fn contains_item(ribbons: &[ResolvedSlotRibbon], item_id: MaraId) -> bool {
    ribbons
        .iter()
        .any(|ribbon| ribbon.items.iter().any(|item| item.id == item_id))
}

fn insert_maximize_button(ribbons: &mut Vec<ResolvedSlotRibbon>, maximized: bool) {
    let item = maximize_item(maximized);
    // Maximize mirrors Close on the opposite corner: Close sits at the
    // trailing (End) edge, so Maximize sits at the leading (Start) edge.
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::Start) {
        ribbon.items.insert(0, item);
    } else {
        ribbons.push(system_button_ribbon(
            MAXIMIZE_RIBBON_CHROME_ID,
            RibbonCluster::Start,
            item,
        ));
    }
}

fn insert_close_button(ribbons: &mut Vec<ResolvedSlotRibbon>, order: ShelfButtonOrder) {
    let item = close_item();
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon.items.len(),
            ShelfButtonOrder::Featureful => 0,
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(system_button_ribbon(
            CLOSE_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn insert_left_shelf_button(ribbons: &mut Vec<ResolvedSlotRibbon>, active: bool) {
    let item = left_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::Start) {
        let insert = ribbon
            .items
            .iter()
            .position(|item| item.id == maximize_item_id())
            .map_or_else(|| (!ribbon.items.is_empty()) as usize, |idx| idx + 1);
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            LEFT_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::Start,
            item,
        ));
    }
}

fn insert_right_shelf_button(
    ribbons: &mut Vec<ResolvedSlotRibbon>,
    active: bool,
    order: ShelfButtonOrder,
) {
    let item = right_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon.items.len().saturating_sub(1),
            ShelfButtonOrder::Featureful => 1.min(ribbon.items.len()),
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            RIGHT_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn insert_bottom_shelf_button(
    ribbons: &mut Vec<ResolvedSlotRibbon>,
    active: bool,
    order: ShelfButtonOrder,
) {
    let item = bottom_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon
                .items
                .iter()
                .position(|item| item.id == right_shelf_item_id())
                .unwrap_or_else(|| ribbon.items.len().saturating_sub(1)),
            ShelfButtonOrder::Featureful => ribbon
                .items
                .iter()
                .position(|item| item.id == right_shelf_item_id())
                .map_or_else(|| 1.min(ribbon.items.len()), |idx| idx + 1),
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            BOTTOM_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn find_top_permanent_cluster_mut(
    ribbons: &mut [ResolvedSlotRibbon],
    cluster: RibbonCluster,
) -> Option<&mut ResolvedSlotRibbon> {
    ribbons
        .iter_mut()
        .find(|ribbon| is_top_permanent_ribbon(ribbon) && ribbon.cluster == cluster)
}

fn is_top_permanent_ribbon(ribbon: &ResolvedSlotRibbon) -> bool {
    ribbon.scope == RibbonScope::Permanent && ribbon.edge == RibbonEdge::Top
}

/// Phone-class ribbon reflow.
///
/// On [`Breakpoint::Phone`](crate::style::Breakpoint::Phone):
/// - the persistent main/top bar drops to the **bottom** (thumb reach)
///   — the single row that lands there;
/// - every other horizontal rail (non-permanent top ribbons and the
///   bottom ribbon) becomes a **vertical side rail**, `End`-cluster to
///   the right, everything else to the left;
/// - existing `Left`/`Right` rails are untouched.
///
/// On phone, side panels (shelves) are slide-in overlays that sit on
/// top of the content rather than pushing it. A side rail painted at
/// the screen edge would then float over the open drawer, so while a
/// side's panel is open we drop that side's rails until it closes.
///
/// Returns `None` above phone-class, so desktop/tablet keep the
/// borrowed slice with no copy.
fn responsive_phone_ribbons(
    ctx: &Context,
    ribbons: &[ResolvedSlotRibbon],
) -> Option<Vec<ResolvedSlotRibbon>> {
    if crate::style::screen_class() != crate::style::Breakpoint::Phone {
        return None;
    }
    let layout = crate::shelf::__internal_shelf_layout(ctx);
    let left_panel_open = layout.is_some_and(|layout| layout.left.is_some());
    let right_panel_open = layout.is_some_and(|layout| layout.right.is_some());
    let remapped = ribbons
        .iter()
        .cloned()
        .map(|mut ribbon| {
            ribbon.edge = responsive_phone_edge(&ribbon);
            ribbon
        })
        .collect();
    Some(hide_side_rails_under_open_panels(
        remapped,
        left_panel_open,
        right_panel_open,
    ))
}

fn hide_side_rails_under_open_panels(
    ribbons: Vec<ResolvedSlotRibbon>,
    left_panel_open: bool,
    right_panel_open: bool,
) -> Vec<ResolvedSlotRibbon> {
    if !left_panel_open && !right_panel_open {
        return ribbons;
    }
    ribbons
        .into_iter()
        .filter(|ribbon| {
            !(left_panel_open && ribbon.edge == RibbonEdge::Left
                || right_panel_open && ribbon.edge == RibbonEdge::Right)
        })
        .collect()
}

fn responsive_phone_edge(ribbon: &ResolvedSlotRibbon) -> RibbonEdge {
    remap_phone_edge(ribbon.edge, ribbon.cluster, ribbon.scope)
}

/// Pure phone edge remap (no breakpoint check — callers gate).
fn remap_phone_edge(edge: RibbonEdge, cluster: RibbonCluster, scope: RibbonScope) -> RibbonEdge {
    let to_side = if cluster == RibbonCluster::End {
        RibbonEdge::Right
    } else {
        RibbonEdge::Left
    };
    match edge {
        RibbonEdge::Top if scope == RibbonScope::Permanent => RibbonEdge::Bottom,
        RibbonEdge::Top | RibbonEdge::Bottom => to_side,
        other => other,
    }
}

/// Remap a ribbon edge for the current breakpoint, matching the phone
/// reflow the ribbon painter applies. Above phone-class returns `edge`
/// unchanged.
///
/// Hosts that position panes anchored to a ribbon button must apply
/// this to the button's declared edge, so the pane opens where the
/// (possibly relocated) button now lives instead of its original edge.
#[must_use]
pub fn phone_remapped_ribbon_edge(
    edge: RibbonEdge,
    cluster: RibbonCluster,
    scope: RibbonScope,
) -> RibbonEdge {
    if crate::style::screen_class() != crate::style::Breakpoint::Phone {
        return edge;
    }
    remap_phone_edge(edge, cluster, scope)
}

fn system_button_ribbon(
    chrome_id: &'static str,
    cluster: RibbonCluster,
    item: RibbonSlotItem,
) -> ResolvedSlotRibbon {
    shelf_button_ribbon(chrome_id, cluster, item)
}

fn shelf_button_ribbon(
    chrome_id: &'static str,
    cluster: RibbonCluster,
    item: RibbonSlotItem,
) -> ResolvedSlotRibbon {
    ResolvedSlotRibbon {
        id: MaraId::new(chrome_id),
        chrome_id: Some(chrome_id),
        scope: RibbonScope::Permanent,
        edge: RibbonEdge::Top,
        role: super::RibbonRole::Icon,
        mode: super::RibbonMode::ThreeSided,
        cluster,
        accepts: &[],
        items: vec![item],
    }
}

fn maximize_item(maximized: bool) -> RibbonSlotItem {
    // Mirrors the close button on the opposite (End) cluster. The glyph
    // reflects the action: "restore" when already maximized, otherwise
    // "maximize".
    let (icon, label, tooltip) = if maximized {
        ("arrow-minimize", "Restore", "Restore window")
    } else {
        ("maximize", "Maximize", "Maximize window")
    };
    let mut item = RibbonSlotItem::featureful(
        MAXIMIZE_ITEM_CHROME_ID,
        icon,
        label,
        tooltip,
        RibbonAction::ToggleMaximize,
    )
    .with_role(super::RibbonRole::Icon);
    item.id = maximize_item_id();
    item
}

fn close_item() -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        CLOSE_ITEM_CHROME_ID,
        crate::style::theme().views.close_icon,
        "Close",
        "Close application",
        RibbonAction::CloseApp,
    )
    .with_role(super::RibbonRole::Icon);
    item.id = close_item_id();
    item
}

fn left_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        LEFT_SHELF_ITEM_CHROME_ID,
        "panel-left",
        "Left shelf",
        "Left shelf",
        RibbonAction::Command(crate::ribbon::left_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = left_shelf_item_id();
    item.active = active;
    item
}

fn right_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        RIGHT_SHELF_ITEM_CHROME_ID,
        "panel-right",
        "Right shelf",
        "Right shelf",
        RibbonAction::Command(crate::ribbon::right_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = right_shelf_item_id();
    item.active = active;
    item
}

fn bottom_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        BOTTOM_SHELF_ITEM_CHROME_ID,
        "panel-bottom",
        "Bottom shelf",
        "Bottom shelf",
        RibbonAction::Command(crate::ribbon::bottom_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = bottom_shelf_item_id();
    item.active = active;
    item
}

fn left_shelf_item_id() -> MaraId {
    MaraId::new("system.left_shelf.item")
}

fn maximize_item_id() -> MaraId {
    MaraId::new("system.maximize.item")
}

fn close_item_id() -> MaraId {
    MaraId::new("system.close_app")
}

fn right_shelf_item_id() -> MaraId {
    MaraId::new("system.right_shelf.item")
}

fn bottom_shelf_item_id() -> MaraId {
    MaraId::new("system.bottom_shelf.item")
}

fn assert_resolved_ribbon_icons(ribbons: &[ResolvedSlotRibbon]) {
    for ribbon in ribbons {
        for item in &ribbon.items {
            assert!(
                crate::icons::is_icon_payload(item.icon),
                "ribbon slot items require an icon that resolves to a bundled font icon or inline SVG"
            );
        }
    }
}

fn can_use_featureful_chrome(ribbons: &[ResolvedSlotRibbon]) -> bool {
    ribbons.iter().all(|ribbon| {
        ribbon.chrome_id.is_some()
            && ribbon.items.iter().all(|item| {
                item.chrome_id.is_some()
                    && item.chrome_tooltip.is_some()
                    && crate::icons::is_icon_payload(item.icon)
            })
    })
}

/// Paint one resolved ribbon's buttons through the SAME placement
/// engine as the featureful renderer — `compute_side_insets` /
/// `insets_for_ribbon` / `place_button` / `screen_rect` in
/// `ribbon::chrome` — so a rail is positioned identically whether it is
/// a window rail, a solo view's rail (the one-leaf tree), or a split
/// cell's rail, and whether or not it carries featureful chrome ids.
/// This frontend only omits the featureful extras (drag/reorder,
/// placement overrides, panel-open state); the geometry is one backend.
fn draw_one_slot_ribbon(
    ctx: &Context,
    accent: MaraColor32,
    ribbon: &ResolvedSlotRibbon,
    all_ribbons: &[ResolvedSlotRibbon],
    clicks: &mut Vec<RibbonSlotClick>,
) {
    use super::chrome::{compute_side_insets, effective_cluster, insets_for_ribbon, place_button};
    use super::paint::{SIDE_BTN_GAP, SIDE_BTN_SIZE};

    if ribbon.items.is_empty() {
        return;
    }

    let insets = insets_for_ribbon(all_ribbons, ribbon, compute_side_insets(all_ribbons));
    let cluster = effective_cluster(ribbon.mode, ribbon.cluster);
    let total = ribbon.items.len() as u32;
    for (idx, item) in ribbon.items.iter().enumerate() {
        let rect = super::chrome::screen_rect(place_button(
            ctx, ribbon, cluster, idx as u32, total, insets,
        ));
        if crate::probe::__internal_enabled(ctx) {
            crate::probe::__internal_record(
                ctx,
                crate::probe::ElementPose::new("slot-ribbon-btn", rect).with_label(format!(
                    "{:?}/{:?} '{}' node-region={}",
                    ribbon.edge,
                    cluster,
                    item.tooltip,
                    crate::embed::current_node_region(ctx).is_some(),
                )),
            );
        }
        let spec = SlotRibbonLayoutSpec::new(
            MaraId::new(("mara_slot_ribbon", ribbon.id, item.id)),
            rect.min,
            true,
            1,
            SIDE_BTN_SIZE,
            SIDE_BTN_GAP,
        );
        crate::backend::egui::show_slot_ribbon_area(ctx, spec, Layer::Foreground, |ui| {
            let Some(rect) = spec.item_screen_rect(0) else {
                return;
            };
            let mut raw = crate::MaraUi::__internal_backend_from_raw(ui);
            let mut mara = crate::MaraUi::__internal_over(&mut raw, accent);
            let response = mara.interact(
                rect,
                MaraId::new(("mara_slot_ribbon_item", ribbon.id, item.id)),
                MaraSense::Click,
            );
            mara.hover_text(&response, &item.tooltip);
            for cmd in ribbon_button_paint_cmds(rect, accent, item.active, response.hovered()) {
                mara.paint(cmd);
            }
            let glyph = glyph_for_item(item);
            paint_ribbon_glyph(
                &mut mara,
                rect,
                glyph,
                ribbon_button_fg(accent, item.active, response.hovered(), glyph),
            );
            if response.clicked() {
                clicks.push(RibbonSlotClick {
                    ribbon: ribbon.id,
                    item: item.id,
                    action: item.action,
                });
            }
        });
    }
}

fn glyph_for_item(item: &RibbonSlotItem) -> super::RibbonGlyph {
    if item.icon.trim_start().starts_with("<svg") || item.icon.trim_start().starts_with("<?xml") {
        super::RibbonGlyph::Svg(item.icon)
    } else {
        super::RibbonGlyph::Icon(item.icon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_leaf_ribbon_none_when_no_items() {
        use crate::ribbon::{
            RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId,
        };
        use crate::vocab::Id;

        // A slot with no default item resolves to nothing → no drawable
        // ribbon (so an empty leaf ribbon set draws nothing).
        let empty_slot = RibbonSlot::new(
            RibbonSlotId::new("empty.slot"),
            None,
            RibbonOverridePolicy::Fixed,
        );
        let def = RibbonSlotDef::new(
            Id::new("empty"),
            RibbonScope::Permanent,
            RibbonEdge::Right,
            RibbonCluster::Middle,
            vec![empty_slot],
        );
        assert!(resolve_leaf_ribbon(&def).is_none());
    }

    /// PROOF: a leaf's ribbon renders INSIDE the node's region — the
    /// area egui actually places must be contained by the cell rect,
    /// nowhere near the window edges.
    #[test]
    #[allow(deprecated)]
    fn view_ribbons_land_inside_the_node_region() {
        use crate::ribbon::{RibbonOverridePolicy, RibbonSlot, RibbonSlotDef, RibbonSlotId};
        use crate::vocab::Id;

        let ctx = egui::Context::default();
        // Cell in the middle-right of a 1600x900 window.
        let region =
            MaraRect::from_min_size(MaraPos2::new(600.0, 100.0), MaraVec2::new(500.0, 600.0));

        let pen = super::super::RibbonSlotItem::new(
            Id::new("pen"),
            "pen",
            "Pen",
            "tip",
            RibbonAction::Command(Id::new("pen.cmd")),
        );
        let def = RibbonSlotDef::new(
            Id::new("test.view.ribbon"),
            RibbonScope::Permanent,
            RibbonEdge::Left,
            RibbonCluster::Middle,
            vec![RibbonSlot::new(
                RibbonSlotId::new("pen.slot"),
                Some(pen),
                RibbonOverridePolicy::Fixed,
            )],
        );

        let mut input = egui::RawInput::default();
        input.screen_rect = Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1600.0, 900.0),
        ));
        let _ = ctx.run(input, |ctx| {
            let _ = __internal_draw_view_ribbons(
                ctx,
                region,
                MaraId::new("test.view.salt"),
                MaraColor32::WHITE,
                std::slice::from_ref(&def),
            );
        });

        let ribbon_id = MaraId::new((def.id, def.cluster));
        let area_id: egui::Id = MaraId::new(("mara_slot_ribbon", ribbon_id, Id::new("pen"))).into();
        let rect = ctx
            .memory(|m| m.area_rect(area_id))
            .expect("leaf ribbon area must exist after the pass");
        let rect: MaraRect = rect.into();
        assert!(
            rect.min.x >= region.min.x
                && rect.min.y >= region.min.y
                && rect.max.x <= region.max.x
                && rect.max.y <= region.max.y,
            "leaf ribbon rendered at {rect:?}, OUTSIDE its region {region:?}"
        );
        // And specifically hugging the region's LEFT edge, not the window's.
        assert!(
            rect.min.x < region.min.x + 40.0,
            "left-edge ribbon should hug the cell's left edge, got {rect:?}"
        );
    }

    fn presence(left: bool, right: bool, bottom: bool) -> crate::shelf::ShelfPresence {
        crate::shelf::ShelfPresence {
            left,
            right,
            bottom,
        }
    }

    fn item(id: &'static str, icon: &'static str, action: RibbonAction) -> RibbonSlotItem {
        RibbonSlotItem::featureful(id, icon, id, id, action)
            .with_role(super::super::RibbonRole::Icon)
    }

    fn top_ribbon(cluster: RibbonCluster, items: Vec<RibbonSlotItem>) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: MaraId::new(("top", cluster)),
            chrome_id: Some("top"),
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: super::super::RibbonRole::Icon,
            mode: super::super::RibbonMode::ThreeSided,
            cluster,
            accepts: &[],
            items,
        }
    }

    fn window_caps(
        system_maximize: bool,
        system_close: bool,
    ) -> crate::window_chrome::WindowChromeHostCapabilities {
        crate::window_chrome::WindowChromeHostCapabilities {
            system_maximize,
            system_close,
            ..Default::default()
        }
    }

    #[test]
    fn window_controls_inject_maximize_and_close_when_shown() {
        let ribbons = vec![top_ribbon(RibbonCluster::Start, Vec::new())];
        let augmented = augment_shelf_buttons_with_chrome(
            &ribbons,
            window_caps(true, true),
            presence(false, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
            false,
            false, // not hidden
        )
        .expect("window controls should be injected");
        assert!(contains_item(&augmented, maximize_item_id()));
        assert!(contains_item(&augmented, close_item_id()));
    }

    #[test]
    fn window_controls_hidden_completely_on_phone() {
        let ribbons = vec![top_ribbon(RibbonCluster::Start, Vec::new())];
        let augmented = augment_shelf_buttons_with_chrome(
            &ribbons,
            window_caps(true, true),
            presence(false, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
            false,
            true, // phone: hide both
        );
        assert!(
            augmented.is_none(),
            "phone-class hides both maximize and close completely"
        );
    }

    #[test]
    fn open_side_panel_hides_only_that_side_rail() {
        let mut left = top_ribbon(RibbonCluster::Start, Vec::new());
        left.edge = RibbonEdge::Left;
        let mut right = top_ribbon(RibbonCluster::Start, Vec::new());
        right.edge = RibbonEdge::Right;
        let mut bottom = top_ribbon(RibbonCluster::Middle, Vec::new());
        bottom.edge = RibbonEdge::Bottom;
        let set = vec![left, right, bottom];

        // Left panel open → only the left rail is dropped.
        let kept = hide_side_rails_under_open_panels(set.clone(), true, false);
        assert!(!kept.iter().any(|r| r.edge == RibbonEdge::Left));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Right));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Bottom));

        // Both panels open → both side rails gone, bottom bar stays.
        let kept = hide_side_rails_under_open_panels(set.clone(), true, true);
        assert!(!kept.iter().any(|r| r.edge.is_vertical()));
        assert!(kept.iter().any(|r| r.edge == RibbonEdge::Bottom));

        // No panel open → nothing removed.
        assert_eq!(
            hide_side_rails_under_open_panels(set.clone(), false, false).len(),
            3
        );
    }

    #[test]
    fn maximize_glyph_reflects_state() {
        assert_eq!(maximize_item(false).icon, "maximize");
        assert_eq!(maximize_item(true).icon, "arrow-minimize");
        assert_eq!(maximize_item(false).action, RibbonAction::ToggleMaximize);
    }

    #[test]
    fn shelf_buttons_are_absent_without_side_shelves() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item("system.maximize.item", "maximize", RibbonAction::Noop)],
        )];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(false, false, false),
                false,
                false,
                false,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "no published side shelves should not alter the top bar"
        );
    }

    #[test]
    fn shelf_buttons_need_an_existing_top_bar() {
        let ribbons = vec![ResolvedSlotRibbon {
            id: MaraId::new("left.rail"),
            chrome_id: Some("left.rail"),
            scope: RibbonScope::View(crate::ViewId::new("test.view")),
            edge: RibbonEdge::Left,
            role: super::super::RibbonRole::Icon,
            mode: super::super::RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items: vec![item("tool", "cube", RibbonAction::Noop)],
        }];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(true, true, true),
                true,
                true,
                true,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "shelf buttons attach to an existing permanent top bar, not to side rails alone"
        );
    }

    #[test]
    fn left_shelf_button_is_inserted_after_maximize_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![
                item("system.maximize.item", "maximize", RibbonAction::Noop),
                item("view.switch.item", "cube", RibbonAction::Noop),
            ],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            true,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("left shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                MaraId::new("system.maximize.item"),
                left_shelf_item_id(),
                MaraId::new("view.switch.item"),
            ]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-left");
        assert!(augmented[0].items[1].active);
    }

    #[test]
    fn hidden_declared_shelf_keeps_inactive_top_bar_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item("system.maximize.item", "maximize", RibbonAction::Noop)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("declared hidden left shelf should keep a button for re-opening");
        assert_eq!(augmented[0].items[1].id, left_shelf_item_id());
        assert!(!augmented[0].items[1].active);
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_featureful_chrome() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![MaraId::new("system.close_app"), right_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-right");
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_simple_painter() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Simple,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![right_shelf_item_id(), MaraId::new("system.close_app")]
        );
        assert_eq!(augmented[0].items[0].icon, "panel-right");
    }

    #[test]
    fn bottom_shelf_button_uses_right_side_of_permanent_bar() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, false, true),
            false,
            false,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("bottom shelf should add a right-side top-bar button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![MaraId::new("system.close_app"), bottom_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-bottom");
    }

    #[test]
    fn bottom_shelf_button_stays_left_of_right_shelf_button_when_both_exist() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, true),
            false,
            true,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("right and bottom shelves should add right-side top-bar buttons");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                MaraId::new("system.close_app"),
                right_shelf_item_id(),
                bottom_shelf_item_id(),
            ]
        );
    }
}
