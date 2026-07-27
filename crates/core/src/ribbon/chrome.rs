#[cfg(feature = "bevy")]
use bevy::prelude::*;
use egui;

use crate::context::MaraCtx;
use std::collections::HashMap;

use super::{
    RibbonSlotItem,
    paint::{EDGE_GAP, SIDE_BTN_GAP, SIDE_BTN_SIZE, ribbon_button_fg, ribbon_button_paint_cmds},
    slot_paint::ResolvedSlotRibbon,
};
use crate::{
    layout::{Layer, Sense as MaraSense, SlotRibbonLayoutSpec, UiBackend},
    paint::PaintCmd,
    vocab::{
        Align2 as MaraAlign2, Color32 as MaraColor32, Id as MaraId, Pos2 as MaraPos2,
        Rect as MaraRect, Vec2 as MaraVec2,
    },
};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RibbonEdge {
    Left,
    Right,
    Top,
    Bottom,
}

impl RibbonEdge {
    #[must_use]
    pub fn is_vertical(self) -> bool {
        matches!(self, RibbonEdge::Left | RibbonEdge::Right)
    }
}

/// Ribbon rails that a content body should avoid.
///
/// This only describes the *body/content* rect. Fullscreen module
/// backgrounds still paint edge-to-edge; only the inner content is
/// moved out from under the selected ribbon buttons.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RibbonAvoidance {
    pub left: bool,
    pub right: bool,
    pub top: bool,
    pub bottom: bool,
}

impl RibbonAvoidance {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            left: false,
            right: false,
            top: false,
            bottom: false,
        }
    }

    #[must_use]
    pub const fn all() -> Self {
        Self {
            left: true,
            right: true,
            top: true,
            bottom: true,
        }
    }

    #[must_use]
    pub const fn sides(left: bool, right: bool, top: bool, bottom: bool) -> Self {
        Self {
            left,
            right,
            top,
            bottom,
        }
    }

    #[must_use]
    pub const fn edge(edge: RibbonEdge) -> Self {
        match edge {
            RibbonEdge::Left => Self::sides(true, false, false, false),
            RibbonEdge::Right => Self::sides(false, true, false, false),
            RibbonEdge::Top => Self::sides(false, false, true, false),
            RibbonEdge::Bottom => Self::sides(false, false, false, true),
        }
    }

    #[must_use]
    pub const fn top() -> Self {
        Self::edge(RibbonEdge::Top)
    }

    #[must_use]
    pub const fn with_edge(mut self, edge: RibbonEdge) -> Self {
        match edge {
            RibbonEdge::Left => self.left = true,
            RibbonEdge::Right => self.right = true,
            RibbonEdge::Top => self.top = true,
            RibbonEdge::Bottom => self.bottom = true,
        }
        self
    }

    #[must_use]
    pub fn apply_to_rect(self, rect: impl Into<MaraRect>) -> MaraRect {
        let gap = ribbon_clearance();
        let mut out = rect.into();
        if self.left {
            out.min.x = (out.min.x + gap).min(out.max.x);
        }
        if self.right {
            out.max.x = (out.max.x - gap).max(out.min.x);
        }
        if self.top {
            out.min.y = (out.min.y + gap).min(out.max.y);
        }
        if self.bottom {
            out.max.y = (out.max.y - gap).max(out.min.y);
        }
        out
    }
}

#[must_use]
pub const fn ribbon_clearance() -> f32 {
    EDGE_GAP + SIDE_BTN_SIZE + SIDE_BTN_GAP
}

#[must_use]
pub(crate) fn ribbon_avoiding_rect(ctx: &dyn crate::context::MaraCtx, avoidance: RibbonAvoidance) -> MaraRect {
    // Every edge is gated on a window rail actually existing there this
    // frame: avoidance reserves clearance for real rails only. Per-view
    // ribbons render inside their own leaf's region (PLAN Phase 3), so a
    // tab whose views carry their own rails gets the full tab region —
    // no phantom gutters on edges with no window rail. Before the first
    // publish the edges default to all-present (conservative).
    let [has_left, has_right, has_top, has_bottom] = crate::pane::published_ribbon_edges(ctx);
    let effective = RibbonAvoidance {
        left: avoidance.left && has_left,
        right: avoidance.right && has_right,
        top: avoidance.top && has_top,
        bottom: avoidance.bottom && has_bottom,
    };
    effective.apply_to_rect(MaraCtx::content_rect(ctx))
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RibbonCluster {
    Start,
    Middle,
    End,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RibbonMode {
    Centered,
    OneSided(RibbonCluster),
    TwoSided,
    ThreeSided,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RibbonRole {
    Panel,
    Icon,
}

#[derive(Clone, Copy, Debug)]
pub enum RibbonGlyph {
    Text(&'static str),
    Icon(&'static str),
    Svg(&'static str),
}

impl From<&'static str> for RibbonGlyph {
    fn from(s: &'static str) -> Self {
        let trimmed = s.trim_start();
        if trimmed.starts_with("<svg") || trimmed.starts_with("<?xml") {
            RibbonGlyph::Svg(s)
        } else {
            RibbonGlyph::Text(s)
        }
    }
}

#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Default, Debug, Clone)]
pub struct RibbonOpen {
    pub per_ribbon: HashMap<&'static str, &'static str>,
}

impl RibbonOpen {
    pub fn get(&self, ribbon: &'static str) -> Option<&'static str> {
        assert_chrome_id(ribbon, "ribbon open state requires a non-empty ribbon id");
        self.per_ribbon.get(ribbon).copied()
    }

    pub fn is_open(&self, ribbon: &'static str, item: &'static str) -> bool {
        assert_chrome_id(ribbon, "ribbon open state requires a non-empty ribbon id");
        assert_chrome_id(item, "ribbon open state requires a non-empty item id");
        self.per_ribbon.get(ribbon).copied() == Some(item)
    }

    pub fn toggle(&mut self, ribbon: &'static str, item: &'static str) {
        assert_chrome_id(ribbon, "ribbon open state requires a non-empty ribbon id");
        assert_chrome_id(item, "ribbon open state requires a non-empty item id");
        if self.is_open(ribbon, item) {
            self.per_ribbon.remove(ribbon);
        } else {
            self.per_ribbon.insert(ribbon, item);
        }
    }

    pub fn close_all(&mut self) {
        self.per_ribbon.clear();
    }

    pub fn set(&mut self, ribbon: &'static str, item: &'static str) {
        assert_chrome_id(ribbon, "ribbon open state requires a non-empty ribbon id");
        assert_chrome_id(item, "ribbon open state requires a non-empty item id");
        self.per_ribbon.insert(ribbon, item);
    }
}

#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Default, Debug, Clone)]
pub struct RibbonWidth {
    pub per_cluster: HashMap<(&'static str, RibbonCluster), f32>,
}

impl RibbonWidth {
    pub fn get(&self, ribbon: &'static str, cluster: RibbonCluster) -> Option<f32> {
        assert_chrome_id(ribbon, "ribbon width state requires a non-empty ribbon id");
        self.per_cluster
            .get(&(ribbon, cluster))
            .copied()
            .filter(|width| width.is_finite())
            .map(|width| width.max(0.0))
    }

    pub fn set(&mut self, ribbon: &'static str, cluster: RibbonCluster, width: f32) {
        assert_chrome_id(ribbon, "ribbon width state requires a non-empty ribbon id");
        if width.is_finite() {
            self.per_cluster.insert((ribbon, cluster), width.max(0.0));
        } else {
            self.per_cluster.remove(&(ribbon, cluster));
        }
    }
}

#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Default, Debug, Clone)]
pub struct RibbonPlacement {
    pub overrides: HashMap<&'static str, (&'static str, RibbonCluster, u32)>,
}

impl RibbonPlacement {
    pub fn set(
        &mut self,
        item_id: &'static str,
        ribbon: &'static str,
        cluster: RibbonCluster,
        slot: u32,
    ) {
        assert_chrome_id(item_id, "ribbon placement requires a non-empty item id");
        assert_chrome_id(ribbon, "ribbon placement requires a non-empty ribbon id");
        self.overrides.insert(item_id, (ribbon, cluster, slot));
    }

    pub fn resolve_parts(
        &self,
        item_id: &'static str,
        ribbon: &'static str,
        cluster: RibbonCluster,
        slot: u32,
    ) -> (&'static str, RibbonCluster, u32) {
        assert_chrome_id(item_id, "ribbon placement requires a non-empty item id");
        assert_chrome_id(ribbon, "ribbon placement requires a non-empty ribbon id");
        // System chrome (window controls + shelf toggles, prefix
        // `mara.system.`) is injected fresh every frame at fixed positions
        // and is never user-reorderable, so it always ignores placement
        // overrides. Without this, a stray drag on any other ribbon button
        // stamps a persistent override onto maximize/close that mis-places
        // (and effectively hides) them on every subsequent view.
        if item_id.starts_with("mara.system.") {
            return (ribbon, cluster, slot);
        }
        self.overrides
            .get(item_id)
            .copied()
            .filter(|(target_ribbon, _, _)| !target_ribbon.trim().is_empty())
            .unwrap_or((ribbon, cluster, slot))
    }
}

fn assert_chrome_id(id: &'static str, message: &str) {
    assert!(!id.trim().is_empty(), "{message}");
}

#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Default, Debug, Clone)]
pub struct RibbonDrag {
    pub item: Option<&'static str>,
    pub cursor: Option<MaraPos2>,
    pub source: Option<(&'static str, RibbonCluster, u32)>,
}

pub(crate) fn chrome_bounds_key() -> crate::vocab::Id {
    crate::vocab::Id::new("mara_ribbon_chrome_bounds")
}

fn chrome_rect(ctx: &dyn crate::context::MaraCtx) -> MaraRect {
    ctx.memory()
        .get_temp::<MaraRect>(chrome_bounds_key())
        .unwrap_or_else(|| MaraCtx::content_rect(ctx))
}

/// The chrome bounds (= viewport that floating side ribbons / panes
/// lay out against) derived **fresh** from the authoritative source
/// every pass: the published shelf layout's viewport, or the live
/// window `content_rect()` when no shelves are reserved.
///
/// The unified renderer must use this instead of reading back
/// [`chrome_bounds_key`] — it *writes* that key, so reading it would
/// re-consume the previous pass's value and freeze the bounds at the
/// first frame's window size. That self-perpetuation was the root of
/// the "side rail / panes stop tracking window resize" bug, and it
/// bit hosts that never did anything wrong (a single stale write
/// stuck forever). See [`crate::shelf::__internal_publish_shelf_layout`].
pub(crate) fn fresh_chrome_bounds(ctx: &dyn crate::context::MaraCtx) -> MaraRect {
    let rect = crate::shelf::__internal_shelf_layout(ctx)
        .map(|layout| layout.viewport)
        .unwrap_or_else(|| MaraCtx::content_rect(ctx));
    // The enforced top bar is full-width and owns the top strip. Reserve
    // that strip in the chrome bounds so CONTENT positioned inside it
    // (panes, side rails) renders BELOW the bar and is never hidden under
    // it. This is the single source of top-bar clearance for in-chrome
    // content. Backgrounds (view bodies, shelf fills) use the raw content
    // rect and still extend behind the glass bar.
    let [_, _, has_top, _] = crate::pane::published_ribbon_edges(ctx);
    if has_top {
        let clearance = ribbon_clearance();
        return MaraRect::from_min_max(
            crate::vocab::Pos2::new(rect.min.x, (rect.min.y + clearance).min(rect.max.y)),
            rect.max,
        );
    }
    rect
}

fn ribbon_rect(ctx: &dyn crate::context::MaraCtx, ribbon: &ResolvedSlotRibbon) -> MaraRect {
    // A ribbon rendered inside a view node belongs to that node: it
    // anchors to the node's region on ANY edge (including top), so a
    // leaf's own ribbons stay inside the leaf, not on the window.
    if let Some(region) = crate::embed::current_node_region(ctx) {
        return region;
    }
    // Outside any node render (the shell bar / the app-level ribbon
    // pass): the permanent top bar spans the whole window; every other
    // rail uses the post-shelf chrome bounds.
    if ribbon.edge == RibbonEdge::Top {
        MaraCtx::content_rect(ctx)
    } else {
        chrome_rect(ctx)
    }
}

fn main_bar_empty_drag_started_id() -> crate::vocab::Id {
    crate::vocab::Id::new("mara_main_bar_empty_drag_started")
}

#[must_use]
pub(crate) fn main_bar_empty_drag_started(ctx: &dyn crate::context::MaraCtx) -> bool {
    {
        let memory = ctx.memory();
        memory
            .get_temp::<bool>(main_bar_empty_drag_started_id())
            .unwrap_or(false)
    }
}

pub(crate) fn effective_cluster(mode: RibbonMode, item: RibbonCluster) -> RibbonCluster {
    match mode {
        RibbonMode::Centered => RibbonCluster::Middle,
        RibbonMode::OneSided(end) => end,
        RibbonMode::TwoSided => match item {
            RibbonCluster::Middle => RibbonCluster::Start,
            other => other,
        },
        RibbonMode::ThreeSided => item,
    }
}

fn clusters_for_mode(mode: RibbonMode) -> &'static [RibbonCluster] {
    match mode {
        RibbonMode::Centered => &[RibbonCluster::Middle],
        RibbonMode::OneSided(RibbonCluster::Start) => &[RibbonCluster::Start],
        RibbonMode::OneSided(RibbonCluster::Middle) => &[RibbonCluster::Middle],
        RibbonMode::OneSided(RibbonCluster::End) => &[RibbonCluster::End],
        RibbonMode::TwoSided => &[RibbonCluster::Start, RibbonCluster::End],
        RibbonMode::ThreeSided => &[
            RibbonCluster::Start,
            RibbonCluster::Middle,
            RibbonCluster::End,
        ],
    }
}

#[derive(Clone, Copy, Default)]
pub(crate) struct SideInsets {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

fn ribbon_id(ribbon: &ResolvedSlotRibbon) -> Option<&'static str> {
    ribbon.chrome_id
}

fn item_id(item: &RibbonSlotItem) -> Option<&'static str> {
    item.chrome_id
}

pub(crate) fn compute_side_insets(ribbons: &[ResolvedSlotRibbon]) -> SideInsets {
    // Keep the exact old assembly spacing: when a perpendicular
    // rail exists, reserve its edge gap + button + one inter-button
    // gap. Do not add a second edge gap here, or side ribbons drift
    // away from the persistent top bar.
    let with_rail = EDGE_GAP + SIDE_BTN_SIZE + SIDE_BTN_GAP;
    let inset = |present: bool| if present { with_rail } else { EDGE_GAP };
    SideInsets {
        left: inset(ribbons.iter().any(|r| r.edge == RibbonEdge::Left)),
        right: inset(ribbons.iter().any(|r| r.edge == RibbonEdge::Right)),
        top: inset(ribbons.iter().any(|r| r.edge == RibbonEdge::Top)),
        bottom: inset(ribbons.iter().any(|r| r.edge == RibbonEdge::Bottom)),
    }
}

fn edge_has_ribbon(ribbons: &[ResolvedSlotRibbon], edge: RibbonEdge) -> bool {
    ribbons.iter().any(|ribbon| ribbon.edge == edge)
}

/// Phone-only: a small screen can't show both a left and a right side
/// pane at once. When a side pane is opened, close every open pane on
/// the opposite side. `ribbons` carries the (already phone-remapped)
/// edges; `open` is keyed by ribbon chrome id.
fn enforce_single_open_side(
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    opened_rid: &'static str,
) {
    if crate::style::screen_class() != crate::style::Breakpoint::Phone {
        return;
    }
    close_opposite_side_panes(ribbons, open, opened_rid);
}

/// Pure side-exclusivity: close every open pane on the side opposite
/// the just-opened pane. No breakpoint check — the caller gates.
fn close_opposite_side_panes(
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    opened_rid: &'static str,
) {
    let Some(opened_edge) = ribbons
        .iter()
        .find(|ribbon| ribbon_id(ribbon) == Some(opened_rid))
        .map(|ribbon| ribbon.edge)
    else {
        return;
    };
    let opposite = match opened_edge {
        RibbonEdge::Left => RibbonEdge::Right,
        RibbonEdge::Right => RibbonEdge::Left,
        // Only side panes participate in left/right exclusivity.
        RibbonEdge::Top | RibbonEdge::Bottom => return,
    };
    let to_close: Vec<&'static str> = ribbons
        .iter()
        .filter(|ribbon| ribbon.edge == opposite)
        .filter_map(ribbon_id)
        .collect();
    for rid in to_close {
        open.per_ribbon.remove(rid);
    }
}

pub(crate) fn insets_for_ribbon(
    ribbons: &[ResolvedSlotRibbon],
    ribbon: &ResolvedSlotRibbon,
    base: SideInsets,
) -> SideInsets {
    let mut out = base;

    // Horizontal bars (top AND bottom) span the full width and own
    // their corners. Side rails sit *between* them, insetting their
    // top/bottom ends for whichever horizontal bars are present. So a
    // bottom bar runs corner-to-corner and the side rails stop short
    // above it — mirroring how the top bar has always behaved.
    let with_rail = EDGE_GAP + SIDE_BTN_SIZE + SIDE_BTN_GAP;
    let corner = |present: bool| if present { with_rail } else { EDGE_GAP };

    match ribbon.edge {
        RibbonEdge::Top | RibbonEdge::Bottom => {
            out.left = EDGE_GAP;
            out.right = EDGE_GAP;
        }
        RibbonEdge::Left | RibbonEdge::Right => {
            // `fresh_chrome_bounds` already reserves the top bar strip AND
            // exactly one `SIDE_BTN_GAP` of clearance below it (the rail
            // clearance is EDGE_GAP + SIDE_BTN_SIZE + SIDE_BTN_GAP, of which
            // the top bar occupies EDGE_GAP + SIDE_BTN_SIZE). So the rail's
            // first icon needs NO extra top inset — it then sits one normal
            // inter-icon gap below the top bar, matching the spacing between
            // the rail icons themselves. The bottom bar is NOT reserved in
            // the chrome rect, so the rail still self-insets there.
            out.top = 0.0;
            out.bottom = corner(edge_has_ribbon(ribbons, RibbonEdge::Bottom));
        }
    }
    out
}

fn strip_rect(ribbon: &ResolvedSlotRibbon, ctx: &dyn crate::context::MaraCtx, insets: SideInsets) -> MaraRect {
    let screen = ribbon_rect(ctx, ribbon);
    let strip_inset = |inset: f32| {
        if inset > EDGE_GAP {
            inset + EDGE_GAP
        } else {
            EDGE_GAP
        }
    };
    match ribbon.edge {
        RibbonEdge::Left => MaraRect::from_min_max(
            MaraPos2::new(
                screen.min.x + EDGE_GAP,
                screen.min.y + strip_inset(insets.top),
            ),
            MaraPos2::new(
                screen.min.x + EDGE_GAP + SIDE_BTN_SIZE,
                screen.max.y - strip_inset(insets.bottom),
            ),
        ),
        RibbonEdge::Right => MaraRect::from_min_max(
            MaraPos2::new(
                screen.max.x - EDGE_GAP - SIDE_BTN_SIZE,
                screen.min.y + strip_inset(insets.top),
            ),
            MaraPos2::new(
                screen.max.x - EDGE_GAP,
                screen.max.y - strip_inset(insets.bottom),
            ),
        ),
        RibbonEdge::Top => MaraRect::from_min_max(
            MaraPos2::new(
                screen.min.x + strip_inset(insets.left),
                screen.min.y + EDGE_GAP,
            ),
            MaraPos2::new(
                screen.max.x - strip_inset(insets.right),
                screen.min.y + EDGE_GAP + SIDE_BTN_SIZE,
            ),
        ),
        RibbonEdge::Bottom => MaraRect::from_min_max(
            MaraPos2::new(
                screen.min.x + strip_inset(insets.left),
                screen.max.y - EDGE_GAP - SIDE_BTN_SIZE,
            ),
            MaraPos2::new(
                screen.max.x - strip_inset(insets.right),
                screen.max.y - EDGE_GAP,
            ),
        ),
    }
}

fn cluster_region(
    ribbon: &ResolvedSlotRibbon,
    cluster: RibbonCluster,
    ctx: &dyn crate::context::MaraCtx,
    insets: SideInsets,
) -> MaraRect {
    let strip = strip_rect(ribbon, ctx, insets);
    match ribbon.mode {
        RibbonMode::Centered | RibbonMode::OneSided(_) => strip,
        RibbonMode::TwoSided => {
            if ribbon.edge.is_vertical() {
                let mid = (strip.top() + strip.bottom()) * 0.5;
                match cluster {
                    RibbonCluster::Start | RibbonCluster::Middle => {
                        MaraRect::from_min_max(strip.min, MaraPos2::new(strip.max.x, mid))
                    }
                    RibbonCluster::End => {
                        MaraRect::from_min_max(MaraPos2::new(strip.min.x, mid), strip.max)
                    }
                }
            } else {
                let mid = (strip.left() + strip.right()) * 0.5;
                match cluster {
                    RibbonCluster::Start | RibbonCluster::Middle => {
                        MaraRect::from_min_max(strip.min, MaraPos2::new(mid, strip.max.y))
                    }
                    RibbonCluster::End => {
                        MaraRect::from_min_max(MaraPos2::new(mid, strip.min.y), strip.max)
                    }
                }
            }
        }
        RibbonMode::ThreeSided => {
            if ribbon.edge.is_vertical() {
                let h = strip.height() / 3.0;
                let t1 = strip.min.y + h;
                let t2 = strip.min.y + h * 2.0;
                match cluster {
                    RibbonCluster::Start => {
                        MaraRect::from_min_max(strip.min, MaraPos2::new(strip.max.x, t1))
                    }
                    RibbonCluster::Middle => MaraRect::from_min_max(
                        MaraPos2::new(strip.min.x, t1),
                        MaraPos2::new(strip.max.x, t2),
                    ),
                    RibbonCluster::End => {
                        MaraRect::from_min_max(MaraPos2::new(strip.min.x, t2), strip.max)
                    }
                }
            } else {
                let w = strip.width() / 3.0;
                let t1 = strip.min.x + w;
                let t2 = strip.min.x + w * 2.0;
                match cluster {
                    RibbonCluster::Start => {
                        MaraRect::from_min_max(strip.min, MaraPos2::new(t1, strip.max.y))
                    }
                    RibbonCluster::Middle => MaraRect::from_min_max(
                        MaraPos2::new(t1, strip.min.y),
                        MaraPos2::new(t2, strip.max.y),
                    ),
                    RibbonCluster::End => {
                        MaraRect::from_min_max(MaraPos2::new(t2, strip.min.y), strip.max)
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ButtonPlacement {
    screen: MaraRect,
    anchor: MaraAlign2,
    offset: MaraVec2,
}

pub(crate) fn place_button(
    ctx: &dyn crate::context::MaraCtx,
    ribbon: &ResolvedSlotRibbon,
    cluster: RibbonCluster,
    slot: u32,
    total: u32,
    insets: SideInsets,
) -> ButtonPlacement {
    let screen = ribbon_rect(ctx, ribbon);
    let step = SIDE_BTN_SIZE + SIDE_BTN_GAP;
    let len = total as f32 * SIDE_BTN_SIZE + total.saturating_sub(1) as f32 * SIDE_BTN_GAP;
    match ribbon.edge {
        RibbonEdge::Left => {
            let x = EDGE_GAP;
            let y = match cluster {
                RibbonCluster::Start => insets.top + slot as f32 * step,
                RibbonCluster::Middle => {
                    let screen_h = screen.height();
                    (screen_h - len) * 0.5 + slot as f32 * step
                }
                RibbonCluster::End => -SIDE_BTN_SIZE - insets.bottom - slot as f32 * step,
            };
            let anchor = if cluster == RibbonCluster::End {
                MaraAlign2::LEFT_BOTTOM
            } else {
                MaraAlign2::LEFT_TOP
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: MaraVec2::new(x, y),
            }
        }
        RibbonEdge::Right => {
            let x = -EDGE_GAP - SIDE_BTN_SIZE;
            let y = match cluster {
                RibbonCluster::Start => insets.top + slot as f32 * step,
                RibbonCluster::Middle => {
                    let screen_h = screen.height();
                    (screen_h - len) * 0.5 + slot as f32 * step
                }
                RibbonCluster::End => -SIDE_BTN_SIZE - insets.bottom - slot as f32 * step,
            };
            let anchor = if cluster == RibbonCluster::End {
                MaraAlign2::RIGHT_BOTTOM
            } else {
                MaraAlign2::RIGHT_TOP
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: MaraVec2::new(x, y),
            }
        }
        RibbonEdge::Top => {
            let y = EDGE_GAP;
            let x = match cluster {
                RibbonCluster::Start => insets.left + slot as f32 * step,
                RibbonCluster::Middle => slot as f32 * step - len * 0.5,
                RibbonCluster::End => -SIDE_BTN_SIZE - insets.right - slot as f32 * step,
            };
            let anchor = match cluster {
                RibbonCluster::Start => MaraAlign2::LEFT_TOP,
                RibbonCluster::Middle => MaraAlign2::CENTER_TOP,
                RibbonCluster::End => MaraAlign2::RIGHT_TOP,
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: MaraVec2::new(x, y),
            }
        }
        RibbonEdge::Bottom => {
            let y = -EDGE_GAP - SIDE_BTN_SIZE;
            let x = match cluster {
                RibbonCluster::Start => insets.left + slot as f32 * step,
                RibbonCluster::Middle => slot as f32 * step - len * 0.5,
                RibbonCluster::End => -SIDE_BTN_SIZE - insets.right - slot as f32 * step,
            };
            let anchor = match cluster {
                RibbonCluster::Start => MaraAlign2::LEFT_BOTTOM,
                RibbonCluster::Middle => MaraAlign2::CENTER_BOTTOM,
                RibbonCluster::End => MaraAlign2::RIGHT_BOTTOM,
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: MaraVec2::new(x, y),
            }
        }
    }
}

pub(crate) fn screen_rect(placement: ButtonPlacement) -> MaraRect {
    let screen = placement.screen;
    let size = MaraVec2::new(SIDE_BTN_SIZE, SIDE_BTN_SIZE);
    let anchor = placement.anchor;
    let offset = placement.offset;
    let min = if anchor == MaraAlign2::LEFT_TOP {
        crate::vocab::Pos2::new(screen.min.x + offset.x, screen.min.y + offset.y)
    } else if anchor == MaraAlign2::LEFT_BOTTOM {
        crate::vocab::Pos2::new(screen.min.x + offset.x, screen.max.y + offset.y)
    } else if anchor == MaraAlign2::RIGHT_TOP {
        crate::vocab::Pos2::new(screen.max.x + offset.x, screen.min.y + offset.y)
    } else if anchor == MaraAlign2::RIGHT_BOTTOM {
        crate::vocab::Pos2::new(screen.max.x + offset.x, screen.max.y + offset.y)
    } else if anchor == MaraAlign2::CENTER_TOP {
        crate::vocab::Pos2::new(screen.center().x + offset.x, screen.min.y + offset.y)
    } else if anchor == MaraAlign2::CENTER_BOTTOM {
        crate::vocab::Pos2::new(screen.center().x + offset.x, screen.max.y + offset.y)
    } else {
        crate::vocab::Pos2::new(screen.center().x + offset.x, screen.center().y + offset.y)
    };
    MaraRect::from_min_size(min, size)
}

fn accepts_drop(
    source_item: &RibbonSlotItem,
    source: &ResolvedSlotRibbon,
    target: &ResolvedSlotRibbon,
) -> bool {
    let Some(src) = ribbon_id(source) else {
        return false;
    };
    source_item.draggable
        && (target.accepts.contains(&"*")
            || target.accepts.contains(&src)
            || ribbon_id(target) == Some(src))
}

fn item_role(item: &RibbonSlotItem, ribbon: &ResolvedSlotRibbon) -> RibbonRole {
    item.role.unwrap_or(ribbon.role)
}

fn paint_item_glyph(ui: &mut egui::Ui, rect: MaraRect, item: &RibbonSlotItem, fg: MaraColor32) {
    let icon = crate::icons::Icon::from(item.icon);
    if matches!(icon, crate::icons::Icon::Name(_)) && !crate::icons::icon_fonts_ready() {
        return;
    }
    if let Some(cmd) = crate::icons::icon_paint_cmd(
        icon,
        rect.center(),
        crate::vocab::Align2::CENTER_CENTER,
        18.0,
        fg,
    ) {
        crate::backend::egui::render_paint_cmd_ui(ui, cmd);
    }
}

pub fn draw_unified_ribbon_chrome(
    ctx: &egui::Context,
    accent: MaraColor32,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
    active: impl Fn(&'static str) -> bool,
) -> Vec<egui::Id> {
    let insets = compute_side_insets(ribbons);
    // Window-pass only: publish the chrome bounds and which WINDOW edges
    // carry rails. A node-scoped render (a leaf drawing its own ribbons
    // inside its region) must not stomp the window's published state —
    // its edges belong to its region and are recorded per-view by
    // `slot_paint::__internal_draw_view_ribbons`. An EMPTY draw publishes
    // nothing either: "this call drew no rails" must not overwrite what
    // the shell bar (or the app rail pass) published this frame — draw
    // order between the two must not decide whether the top edge exists.
    // Writes go through the backend-neutral memory facade.
    if !ribbons.is_empty() && crate::embed::current_node_region(ctx).is_none() {
        let chrome = fresh_chrome_bounds(ctx);
        let mut memory = MaraCtx::memory(ctx);
        memory.set_temp(chrome_bounds_key(), chrome);
        memory.set_temp::<[bool; 4]>(
            egui::Id::new("mara_published_ribbon_edges"),
            [
                edge_has_ribbon(ribbons, RibbonEdge::Left),
                edge_has_ribbon(ribbons, RibbonEdge::Right),
                edge_has_ribbon(ribbons, RibbonEdge::Top),
                edge_has_ribbon(ribbons, RibbonEdge::Bottom),
            ],
        );
    }

    let mut flat = Vec::new();
    for (r_idx, ribbon) in ribbons.iter().enumerate() {
        let Some(rid) = ribbon_id(ribbon) else {
            continue;
        };
        for (i_idx, item) in ribbon.items.iter().enumerate() {
            let Some(iid) = item_id(item) else {
                continue;
            };
            flat.push((r_idx, i_idx, rid, iid, ribbon.cluster, i_idx as u32));
        }
    }

    let resolved: Vec<(&'static str, RibbonCluster, u32)> = flat
        .iter()
        .map(|(_, _, rid, iid, cluster, slot)| placement.resolve_parts(iid, rid, *cluster, *slot))
        .collect();

    let mut target: Option<(&'static str, RibbonCluster, u32)> = None;
    if let (Some(dragged_id), Some(cursor), Some(_source)) = (drag.item, drag.cursor, drag.source) {
        let source = flat
            .iter()
            .position(|(_, _, _, iid, _, _)| *iid == dragged_id)
            .and_then(|idx| {
                let current_rid = resolved[idx].0;
                let source_ribbon = ribbons
                    .iter()
                    .find(|ribbon| ribbon_id(ribbon) == Some(current_rid))?;
                let (base_r_idx, item_idx, _, _, _, _) = flat[idx];
                Some((&ribbons[base_r_idx].items[item_idx], source_ribbon))
            });
        if let Some((source_item, src_ribbon)) = source {
            'hit: for ribbon in ribbons {
                if !accepts_drop(source_item, src_ribbon, ribbon) {
                    continue;
                }
                let Some(rid) = ribbon_id(ribbon) else {
                    continue;
                };
                for &cluster in clusters_for_mode(ribbon.mode) {
                    if cluster_region(
                        ribbon,
                        cluster,
                        ctx,
                        insets_for_ribbon(ribbons, ribbon, insets),
                    )
                    .contains(cursor)
                    {
                        let cluster_eff = effective_cluster(ribbon.mode, cluster);
                        let count = resolved
                            .iter()
                            .zip(flat.iter())
                            .filter(|((rrid, c, _), (_, _, _, iid, _, _))| {
                                *rrid == rid
                                    && effective_cluster(ribbon.mode, *c) == cluster_eff
                                    && *iid != dragged_id
                            })
                            .count() as u32;
                        let axis_cursor = if ribbon.edge.is_vertical() {
                            cursor.y
                        } else {
                            cursor.x
                        };
                        let mut best_slot = 0;
                        let mut best_dist = f32::INFINITY;
                        for slot in 0..=count {
                            let rect = screen_rect(place_button(
                                ctx,
                                ribbon,
                                cluster_eff,
                                slot,
                                count + 1,
                                insets_for_ribbon(ribbons, ribbon, insets),
                            ));
                            let axis = if ribbon.edge.is_vertical() {
                                rect.center().y
                            } else {
                                rect.center().x
                            };
                            let dist = (axis - axis_cursor).abs();
                            if dist < best_dist {
                                best_dist = dist;
                                best_slot = slot;
                            }
                        }
                        target = Some((rid, cluster_eff, best_slot));
                        break 'hit;
                    }
                }
            }
        }
    }

    let effective = |item_idx: usize| -> (&'static str, RibbonCluster, u32, u32) {
        let (rid, cluster_raw, slot) = resolved[item_idx];
        let Some(ribbon) = ribbons.iter().find(|ribbon| ribbon_id(ribbon) == Some(rid)) else {
            return (rid, cluster_raw, slot, 1);
        };
        let kind = (rid, effective_cluster(ribbon.mode, cluster_raw));
        let raw_total = |target_kind: (&'static str, RibbonCluster)| -> u32 {
            resolved
                .iter()
                .filter(|(rrid, c, _)| {
                    if *rrid != target_kind.0 {
                        return false;
                    }
                    let Some(ribbon) = ribbons
                        .iter()
                        .find(|ribbon| ribbon_id(ribbon) == Some(*rrid))
                    else {
                        return false;
                    };
                    effective_cluster(ribbon.mode, *c) == target_kind.1
                })
                .count() as u32
        };

        let Some((src_rid, src_cluster_raw, src_slot)) = drag.source else {
            return (kind.0, kind.1, slot, raw_total(kind));
        };
        let Some(src_ribbon) = ribbons
            .iter()
            .find(|ribbon| ribbon_id(ribbon) == Some(src_rid))
        else {
            return (kind.0, kind.1, slot, raw_total(kind));
        };
        let src_kind = (src_rid, effective_cluster(src_ribbon.mode, src_cluster_raw));

        let mut out_slot = slot;
        let mut total_delta: i32 = 0;
        if let Some((tgt_rid, tgt_cluster_eff, insert)) = target {
            let tgt_kind = (tgt_rid, tgt_cluster_eff);
            if kind == src_kind && kind == tgt_kind {
                if src_slot < insert && slot > src_slot && slot <= insert {
                    out_slot = slot - 1;
                } else if src_slot > insert && slot >= insert && slot < src_slot {
                    out_slot = slot + 1;
                }
            } else {
                if kind == src_kind && slot > src_slot {
                    out_slot = slot - 1;
                }
                if kind == tgt_kind && slot >= insert {
                    out_slot = slot + 1;
                }
                if kind == src_kind {
                    total_delta = -1;
                } else if kind == tgt_kind {
                    total_delta = 1;
                }
            }
        } else {
            if kind == src_kind && slot > src_slot {
                out_slot = slot - 1;
            }
            if kind == src_kind {
                total_delta = -1;
            }
        }

        let total = (raw_total(kind) as i32 + total_delta).max(1) as u32;
        (kind.0, kind.1, out_slot, total)
    };

    let mut click_flags = vec![false; flat.len()];
    let mut drag_started_idx = None;
    let mut drag_stopped = false;
    let mut button_rects = Vec::with_capacity(flat.len());

    for (idx, (base_r_idx, i_idx, _base_rid, iid, _base_cluster, _base_slot)) in
        flat.iter().enumerate()
    {
        let (rid, cluster, slot, total) = effective(idx);
        let Some((ribbon_idx, ribbon)) = ribbons
            .iter()
            .enumerate()
            .find(|(_, ribbon)| ribbon_id(ribbon) == Some(rid))
        else {
            continue;
        };
        let item = &ribbons[*base_r_idx].items[*i_idx];
        let cluster_eff = effective_cluster(ribbon.mode, cluster);
        let slot_eff = slot.min(total.saturating_sub(1));
        let resting = screen_rect(place_button(
            ctx,
            ribbon,
            cluster_eff,
            slot_eff,
            total.max(1),
            insets_for_ribbon(ribbons, ribbon, insets),
        ));
        button_rects.push(resting);
        let dragging_this = drag.item == Some(*iid);
        let paint_pos = if dragging_this {
            let center = drag.cursor.unwrap_or_else(|| resting.center());
            MaraPos2::new(
                center.x - SIDE_BTN_SIZE * 0.5,
                center.y - SIDE_BTN_SIZE * 0.5,
            )
        } else {
            resting.min
        };
        // System chrome (window controls + shelf toggles, prefix
        // `mara.system.`) renders on the top-most Overlay layer so it can
        // never be covered by Foreground content (view bodies, panes,
        // shelf chrome). This keeps maximize/close persistently visible on
        // the top bar across every view.
        let is_system_chrome = item
            .chrome_id
            .is_some_and(|id| id.starts_with("mara.system."));
        let layer = if dragging_this || is_system_chrome {
            Layer::Overlay
        } else {
            Layer::Foreground
        };
        let role = item_role(item, &ribbons[ribbon_idx]);
        let is_active = match role {
            RibbonRole::Panel => open.is_open(rid, iid) || active(iid),
            RibbonRole::Icon => active(iid),
        };
        let button_spec = SlotRibbonLayoutSpec::new(
            MaraId::new(("mara_ribbon_btn", iid)),
            paint_pos,
            true,
            1,
            SIDE_BTN_SIZE,
            SIDE_BTN_GAP,
        );
        let area_response =
            crate::backend::egui::show_slot_ribbon_area(ctx, button_spec, layer, |ui| {
                let sense = if item.draggable {
                    MaraSense::ClickAndDrag
                } else {
                    MaraSense::Click
                };
                let rect = button_spec
                    .item_screen_rect(0)
                    .expect("single ribbon button spec must have an item rect");
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                let response =
                    backend.interact(rect, MaraId::new(("mara_ribbon_btn_hit", iid)), sense);
                if crate::probe::__internal_enabled(ui.ctx()) {
                    crate::probe::__internal_record(
                        ui.ctx(),
                        crate::probe::ElementPose::new("ribbon-btn", rect).with_label(format!(
                            "{:?}/{:?} '{}'",
                            ribbon.edge, cluster_eff, item.tooltip
                        )),
                    );
                }
                for cmd in ribbon_button_paint_cmds(
                    rect,
                    accent,
                    is_active,
                    response.hovered() || dragging_this,
                ) {
                    crate::backend::egui::render_paint_cmd_ui(ui, cmd);
                }
                let glyph = RibbonGlyph::Icon(item.icon);
                let fg = ribbon_button_fg(
                    accent,
                    is_active || dragging_this,
                    response.hovered() || dragging_this,
                    glyph,
                );
                paint_item_glyph(ui, rect, item, fg);
                crate::backend::egui::hover_text_for_ui_response(ui, &response, &item.tooltip);
                response
            });
        crate::backend::egui::move_area_response_to_top(ctx, &area_response.response);
        let response = area_response.inner;
        if item.draggable && response.drag_started() {
            drag_started_idx = Some(idx);
        }
        if dragging_this && response.dragged() {
            drag.cursor = crate::backend::egui::pointer_interact_pos(ctx);
        }
        if dragging_this && response.drag_stopped() {
            drag_stopped = true;
        }
        if response.clicked() && drag.item.is_none() && !dragging_this {
            click_flags[idx] = true;
        }
    }

    if let (Some((tgt_rid, tgt_cluster, insert)), Some(_dragged)) = (target, drag.item)
        && let Some(ribbon) = ribbons
            .iter()
            .find(|ribbon| ribbon_id(ribbon) == Some(tgt_rid))
    {
        let count = resolved
            .iter()
            .zip(flat.iter())
            .filter(|((rid, c, _), (_, _, _, iid, _, _))| {
                *rid == tgt_rid
                    && effective_cluster(ribbon.mode, *c) == tgt_cluster
                    && drag.item != Some(*iid)
            })
            .count() as u32
            + 1;
        let rect = screen_rect(place_button(
            ctx,
            ribbon,
            tgt_cluster,
            insert,
            count,
            insets_for_ribbon(ribbons, ribbon, insets),
        ));
        let outline_spec = SlotRibbonLayoutSpec::new(
            MaraId::new("mara_ribbon_drop_outline"),
            rect.min,
            true,
            1,
            SIDE_BTN_SIZE,
            SIDE_BTN_GAP,
        );
        crate::context::MaraCtx::area_slot(
            ctx,
            outline_spec.area_slot(Layer::Foreground, false),
            // Named `mara`, not `ui`: this is the sealed surface. The
            // guard in `make check` bans the backend `Ui`'s interact
            // call in this file, and the name keeps the two distinct.
            &mut |mara| {
                let rect = outline_spec
                    .item_screen_rect(0)
                    .expect("single ribbon drop-outline spec must have an item rect");
                let _response = mara.interact(
                    rect,
                    MaraId::new("mara_ribbon_drop_outline_hit"),
                    MaraSense::Hover,
                );
                let corner = crate::style::radius_for(crate::style::RadiusRole::Section);
                mara.paint(PaintCmd::RectFilled {
                    rect,
                    corner,
                    fill: crate::style::fill_for(crate::style::FillRole::DragGhost, accent),
                });
                mara.paint(PaintCmd::RectStroke {
                    rect,
                    corner,
                    stroke: crate::style::stroke_for(crate::style::StrokeRole::DragGhost, accent),
                });
            },
        );
    }

    if let Some(idx) = drag_started_idx {
        let (_, _, rid, iid, cluster, slot) = flat[idx];
        drag.item = Some(iid);
        drag.cursor = crate::backend::egui::pointer_interact_pos(ctx);
        drag.source = Some(placement.resolve_parts(iid, rid, cluster, slot));
    }

    if drag_stopped {
        if let (Some(dragged_id), Some((tgt_rid, tgt_cluster, insert))) = (drag.item, target)
            && let Some(source) = drag.source
        {
            resolve_drop(
                placement,
                ribbons,
                &flat,
                dragged_id,
                source,
                tgt_rid,
                tgt_cluster,
                insert,
            );
        }
        drag.item = None;
        drag.cursor = None;
        drag.source = None;
    }

    let empty_main_bar_drag_started = ribbons.first().is_some_and(|main| {
        let main_strip = strip_rect(main, ctx, insets_for_ribbon(ribbons, main, insets));
        crate::backend::egui::primary_pointer_pressed_interact_pos(ctx).is_some_and(|pos| {
            main_strip.contains(pos) && !button_rects.iter().any(|rect| rect.contains(pos))
        })
    });
    let window_chrome_capabilities =
        crate::window_chrome::__internal_window_chrome_host_capabilities(ctx);
    if window_chrome_capabilities.native_move || window_chrome_capabilities.native_resize {
        let drag_regions = if window_chrome_capabilities.native_move {
            ribbons
                .first()
                .map(|main| strip_rect(main, ctx, insets_for_ribbon(ribbons, main, insets)))
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Pad each button's exclusion rect by the inter-button gap so the
        // whole button cluster (including the slivers *between* buttons and
        // the bar's top/bottom margin) is a contiguous no-drag zone.
        // Without this, a press that lands in a 4px gap or near a button
        // edge is hit-tested as the top-bar drag region and starts a window
        // move instead of clicking the button.
        crate::window_chrome::__internal_publish_window_chrome_regions(
            ctx,
            drag_regions,
            button_rects.iter().map(|rect| rect.expand(SIDE_BTN_GAP)),
        );
    } else {
        crate::window_chrome::clear_window_chrome_regions(ctx);
    }
    if window_chrome_capabilities.native_resize {
        crate::window_chrome::__internal_paint_resize_corner_hover(
            ctx,
            accent,
            crate::style::theme().window_chrome,
        );
    }
    {
        let mut memory = MaraCtx::memory(ctx);
        memory.set_temp(
            main_bar_empty_drag_started_id(),
            empty_main_bar_drag_started,
        );
    };

    let mut clicks = Vec::new();
    for (idx, fired) in click_flags.iter().copied().enumerate() {
        if !fired {
            continue;
        }
        let (base_r_idx, i_idx, _, iid, _, _) = flat[idx];
        let (rid, _, _) = resolved[idx];
        let item = &ribbons[base_r_idx].items[i_idx];
        let role = item_role(item, &ribbons[base_r_idx]);
        if role == RibbonRole::Panel {
            open.toggle(rid, iid);
            // Phone: opening a side pane closes the opposite side's.
            if open.is_open(rid, iid) {
                enforce_single_open_side(ribbons, open, rid);
            }
        }
        clicks.push(item.id.into());
    }
    clicks
}

#[allow(clippy::too_many_arguments)]
fn resolve_drop(
    placement: &mut RibbonPlacement,
    ribbons: &[ResolvedSlotRibbon],
    flat: &[(usize, usize, &'static str, &'static str, RibbonCluster, u32)],
    dragged_id: &'static str,
    source: (&'static str, RibbonCluster, u32),
    tgt_rid: &'static str,
    tgt_cluster_eff: RibbonCluster,
    insert: u32,
) {
    let (src_rid, src_cluster_raw, src_slot) = source;
    let Some(src_ribbon) = ribbons
        .iter()
        .find(|ribbon| ribbon_id(ribbon) == Some(src_rid))
    else {
        return;
    };
    let src_cluster_eff = effective_cluster(src_ribbon.mode, src_cluster_raw);

    let now: Vec<(&'static str, (&'static str, RibbonCluster, u32))> = flat
        .iter()
        .map(|(_, _, base_rid, iid, base_cluster, base_slot)| {
            (
                *iid,
                placement.resolve_parts(iid, base_rid, *base_cluster, *base_slot),
            )
        })
        .collect();
    let cross_cluster = (src_rid, src_cluster_eff) != (tgt_rid, tgt_cluster_eff);

    for (id, (rid, c_raw, slot)) in &now {
        if *id == dragged_id {
            continue;
        }
        let Some(ribbon) = ribbons
            .iter()
            .find(|ribbon| ribbon_id(ribbon) == Some(*rid))
        else {
            continue;
        };
        let c_eff = effective_cluster(ribbon.mode, *c_raw);
        let mut new_slot = *slot;
        if cross_cluster {
            if *rid == src_rid && c_eff == src_cluster_eff && *slot > src_slot {
                new_slot -= 1;
            }
            if *rid == tgt_rid && c_eff == tgt_cluster_eff && *slot >= insert {
                new_slot += 1;
            }
        } else if src_slot < insert && *slot > src_slot && *slot <= insert {
            new_slot -= 1;
        } else if src_slot > insert && *slot >= insert && *slot < src_slot {
            new_slot += 1;
        }
        placement.overrides.insert(*id, (*rid, *c_raw, new_slot));
    }

    placement
        .overrides
        .insert(dragged_id, (tgt_rid, tgt_cluster_eff, insert));

    for ribbon in ribbons {
        let Some(rid) = ribbon_id(ribbon) else {
            continue;
        };
        for &cluster in clusters_for_mode(ribbon.mode) {
            let c_eff = effective_cluster(ribbon.mode, cluster);
            let mut occ: Vec<(&'static str, u32)> = flat
                .iter()
                .filter_map(|(_, _, base_rid, iid, base_cluster, base_slot)| {
                    let (r, c, s) =
                        placement.resolve_parts(iid, base_rid, *base_cluster, *base_slot);
                    if r != rid {
                        return None;
                    }
                    let ribbon = ribbons.iter().find(|ribbon| ribbon_id(ribbon) == Some(r))?;
                    if effective_cluster(ribbon.mode, c) != c_eff {
                        return None;
                    }
                    Some((*iid, s))
                })
                .collect();
            occ.sort_by_key(|(_, s)| *s);
            for (n, (id, _)) in occ.into_iter().enumerate() {
                let Some((_, _, base_rid, _, base_cluster, base_slot)) =
                    flat.iter().find(|(_, _, _, iid, _, _)| *iid == id)
                else {
                    continue;
                };
                let (r, c_raw, _) =
                    placement.resolve_parts(id, base_rid, *base_cluster, *base_slot);
                placement.overrides.insert(id, (r, c_raw, n as u32));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ribbon::{RibbonAction, RibbonScope};

    fn test_ctx_with_chrome(rect: egui::Rect) -> egui::Context {
        let ctx = egui::Context::default();
        crate::memory::MaraMemoryCtx::new(&ctx).set_temp(chrome_bounds_key(), MaraRect::from(rect));
        ctx
    }

    fn test_ctx_with_screen_and_chrome(screen: egui::Rect, chrome: egui::Rect) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        crate::memory::MaraMemoryCtx::new(&ctx)
            .set_temp(chrome_bounds_key(), MaraRect::from(chrome));
        ctx
    }

    fn ribbon(edge: RibbonEdge) -> ResolvedSlotRibbon {
        ribbon_with_id("test_ribbon", edge)
    }

    fn ribbon_with_id(id: &'static str, edge: RibbonEdge) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: crate::vocab::Id::new((id, edge)),
            chrome_id: Some(id),
            scope: RibbonScope::Permanent,
            edge,
            role: RibbonRole::Icon,
            mode: RibbonMode::ThreeSided,
            cluster: RibbonCluster::Middle,
            accepts: &["*"],
            items: vec![
                RibbonSlotItem::featureful("test_item", "info", "Test", "Test", RibbonAction::Noop)
                    .draggable(true),
            ],
        }
    }

    #[test]
    fn bottom_bar_spans_full_width_side_rails_inset() {
        // The bottom bar runs corner-to-corner; the side rails stop
        // short above it. Holds in BOTH declaration orders, so a
        // relocated main bar dropped to the bottom still spans fully.
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let ctx = test_ctx_with_chrome(chrome);
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let bottom = ribbon_with_id("bottom", RibbonEdge::Bottom);

        for order in [
            vec![left.clone(), bottom.clone()],
            vec![bottom.clone(), left.clone()],
        ] {
            let base = compute_side_insets(&order);
            let left_ribbon = order.iter().find(|r| r.edge == RibbonEdge::Left).unwrap();
            let bottom_ribbon = order.iter().find(|r| r.edge == RibbonEdge::Bottom).unwrap();
            let left_strip = strip_rect(
                left_ribbon,
                &ctx,
                insets_for_ribbon(&order, left_ribbon, base),
            );
            let bottom_strip = strip_rect(
                bottom_ribbon,
                &ctx,
                insets_for_ribbon(&order, bottom_ribbon, base),
            );
            // Bottom bar reaches the left edge (owns the corner).
            assert_eq!(bottom_strip.left(), chrome.left() + EDGE_GAP);
            // Side rail stops above the bottom bar.
            assert!(left_strip.bottom() < bottom_strip.top());
        }
    }

    #[test]
    fn opening_a_side_pane_closes_the_opposite_side() {
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let right = ribbon_with_id("right", RibbonEdge::Right);
        let ribbons = vec![left, right];
        let mut open = RibbonOpen::default();
        open.set("left", "left_pane");
        open.set("right", "right_pane");

        // Just opened the left pane → the right side must close.
        close_opposite_side_panes(&ribbons, &mut open, "left");
        assert!(open.is_open("left", "left_pane"));
        assert!(open.get("right").is_none());

        // Now open the right pane → the left side closes.
        open.set("right", "right_pane");
        close_opposite_side_panes(&ribbons, &mut open, "right");
        assert!(open.is_open("right", "right_pane"));
        assert!(open.get("left").is_none());
    }

    #[test]
    fn fresh_chrome_bounds_track_window_resize_without_explicit_publish() {
        // No shelf layout published. The bounds must follow the live
        // window each pass — regression for the self-perpetuating
        // chrome_bounds_key that froze side ribbons at frame 1.
        let ctx = egui::Context::default();

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 480.0),
            )),
            ..Default::default()
        });
        // Chrome bounds reserve the (assumed-present) top bar strip, so the
        // content area starts one rail clearance below the window top.
        let cr = ctx.content_rect();
        assert_eq!(
            fresh_chrome_bounds(&ctx),
            MaraRect::from(egui::Rect::from_min_max(
                egui::pos2(cr.min.x, cr.min.y + ribbon_clearance()),
                cr.max,
            ))
        );
        // Simulate the renderer writing the key (what froze it before).
        let first = fresh_chrome_bounds(&ctx);
        crate::memory::MaraMemoryCtx::new(&ctx).set_temp(chrome_bounds_key(), first);
        let _ = ctx.end_pass();

        // Window grows on the next pass.
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(1200.0, 700.0),
            )),
            ..Default::default()
        });
        let second = fresh_chrome_bounds(&ctx);
        let _ = ctx.end_pass();

        assert_eq!(
            second,
            MaraRect::from(egui::Rect::from_min_max(
                egui::pos2(0.0, ribbon_clearance()),
                egui::pos2(1200.0, 700.0)
            )),
            "chrome bounds must follow the resized window, not the stale write"
        );
        assert_ne!(second, first, "bounds must not freeze at the first pass");
    }

    #[test]
    fn fresh_chrome_bounds_prefer_published_shelf_viewport() {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(800.0, 480.0),
            )),
            ..Default::default()
        });
        let reserved = egui::Rect::from_min_max(egui::pos2(60.0, 40.0), egui::pos2(740.0, 480.0));
        crate::shelf::__internal_publish_shelf_layout(
            &ctx,
            crate::shelf::ShelfLayout::full(reserved),
        );
        // The published shelf viewport is preferred, then the top-bar strip
        // is reserved on top of it.
        assert_eq!(
            fresh_chrome_bounds(&ctx),
            MaraRect::from(egui::Rect::from_min_max(
                egui::pos2(60.0, 40.0 + ribbon_clearance()),
                egui::pos2(740.0, 480.0)
            ))
        );
        let _ = ctx.end_pass();
    }

    #[test]
    fn top_ribbon_uses_full_window_even_when_chrome_bounds_are_reserved() {
        let screen = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let chrome = egui::Rect::from_min_max(egui::pos2(220.0, 0.0), egui::pos2(620.0, 480.0));
        let ctx = test_ctx_with_screen_and_chrome(screen, chrome);
        let top = ribbon_with_id("top", RibbonEdge::Top);
        let left = ribbon_with_id("left", RibbonEdge::Left);
        let ribbons = vec![top, left];
        let base = compute_side_insets(&ribbons);

        let top_strip = strip_rect(
            &ribbons[0],
            &ctx,
            insets_for_ribbon(&ribbons, &ribbons[0], base),
        );
        assert_eq!(top_strip.left(), screen.left() + EDGE_GAP);
        assert_eq!(top_strip.right(), screen.right() - EDGE_GAP);

        let left_strip = strip_rect(
            &ribbons[1],
            &ctx,
            insets_for_ribbon(&ribbons, &ribbons[1], base),
        );
        assert_eq!(left_strip.left(), chrome.left() + EDGE_GAP);
    }

    #[test]
    fn vertical_middle_buttons_center_against_published_chrome_height() {
        let chrome = egui::Rect::from_min_size(egui::pos2(24.0, 40.0), egui::vec2(320.0, 384.0));
        let ctx = test_ctx_with_chrome(chrome);
        let insets = SideInsets::default();

        for edge in [RibbonEdge::Left, RibbonEdge::Right] {
            let ribbon = ribbon(edge);
            let rect = screen_rect(place_button(
                &ctx,
                &ribbon,
                RibbonCluster::Middle,
                0,
                1,
                insets,
            ));

            assert_eq!(rect.center().y, chrome.center().y);
        }
    }

    #[test]
    fn vertical_middle_button_group_centers_against_published_chrome_height() {
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 96.0), egui::vec2(480.0, 512.0));
        let ctx = test_ctx_with_chrome(chrome);
        let insets = SideInsets::default();
        let ribbon = ribbon(RibbonEdge::Left);

        let first = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::Middle,
            0,
            3,
            insets,
        ));
        let last = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::Middle,
            2,
            3,
            insets,
        ));
        let group_center = (first.center().y + last.center().y) * 0.5;

        assert_eq!(group_center, chrome.center().y);
    }

    #[test]
    fn featureful_button_placement_uses_mara_geometry() {
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(800.0, 480.0));
        let ctx = test_ctx_with_chrome(chrome);
        let ribbon = ribbon(RibbonEdge::Bottom);

        let rect: MaraRect = screen_rect(place_button(
            &ctx,
            &ribbon,
            RibbonCluster::End,
            0,
            1,
            SideInsets::default(),
        ));

        assert_eq!(rect.bottom(), chrome.bottom() - EDGE_GAP);
        assert_eq!(rect.right(), chrome.right());
    }

    #[test]
    fn ribbon_open_rejects_blank_chrome_ids() {
        let mut open = RibbonOpen::default();

        let blank_ribbon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open.set(" ", "item");
        }));
        let blank_item = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            open.toggle("ribbon", " ");
        }));

        assert!(blank_ribbon.is_err());
        assert!(blank_item.is_err());
    }

    #[test]
    fn ribbon_width_sanitizes_invalid_values() {
        let mut widths = RibbonWidth::default();

        widths.set("ribbon", RibbonCluster::Start, -12.0);
        assert_eq!(widths.get("ribbon", RibbonCluster::Start), Some(0.0));

        widths.set("ribbon", RibbonCluster::Start, f32::NAN);
        assert_eq!(widths.get("ribbon", RibbonCluster::Start), None);

        widths
            .per_cluster
            .insert(("ribbon", RibbonCluster::Middle), f32::NEG_INFINITY);
        assert_eq!(widths.get("ribbon", RibbonCluster::Middle), None);

        let blank_ribbon = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            widths.set(" ", RibbonCluster::End, 10.0);
        }));
        assert!(blank_ribbon.is_err());
    }

    #[test]
    fn ribbon_placement_rejects_blank_ids_and_ignores_invalid_direct_targets() {
        let mut placement = RibbonPlacement::default();
        placement.set("item", "target", RibbonCluster::End, 3);
        assert_eq!(
            placement.resolve_parts("item", "source", RibbonCluster::Start, 0),
            ("target", RibbonCluster::End, 3)
        );

        placement
            .overrides
            .insert("bad-target", (" ", RibbonCluster::End, 9));
        assert_eq!(
            placement.resolve_parts("bad-target", "source", RibbonCluster::Start, 0),
            ("source", RibbonCluster::Start, 0)
        );

        let blank_item = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            placement.set(" ", "target", RibbonCluster::Middle, 0);
        }));
        let blank_fallback = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = placement.resolve_parts("item", " ", RibbonCluster::Middle, 0);
        }));

        assert!(blank_item.is_err());
        assert!(blank_fallback.is_err());
    }
}
