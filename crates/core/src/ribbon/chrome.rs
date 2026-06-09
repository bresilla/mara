#[cfg(feature = "bevy")]
use bevy::prelude::*;
use egui;
use std::collections::HashMap;

use super::{
    RibbonSlotItem,
    paint::{EDGE_GAP, SIDE_BTN_GAP, SIDE_BTN_SIZE, paint_ribbon_button, ribbon_button_fg},
    slot_paint::ResolvedSlotRibbon,
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
    pub fn apply_to_rect(self, rect: egui::Rect) -> egui::Rect {
        let gap = ribbon_clearance();
        let mut out = rect;
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
pub fn ribbon_avoiding_rect(ctx: &egui::Context, avoidance: RibbonAvoidance) -> egui::Rect {
    avoidance.apply_to_rect(ctx.content_rect())
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
    pub cursor: Option<egui::Pos2>,
    pub source: Option<(&'static str, RibbonCluster, u32)>,
}

pub(crate) fn chrome_bounds_key() -> egui::Id {
    egui::Id::new("mara_ribbon_chrome_bounds")
}

fn chrome_rect(ctx: &egui::Context) -> egui::Rect {
    ctx.data(|d| d.get_temp::<egui::Rect>(chrome_bounds_key()))
        .unwrap_or_else(|| ctx.content_rect())
}

fn ribbon_rect(ctx: &egui::Context, ribbon: &ResolvedSlotRibbon) -> egui::Rect {
    if ribbon.edge == RibbonEdge::Top {
        ctx.content_rect()
    } else {
        chrome_rect(ctx)
    }
}

fn main_bar_empty_drag_started_id() -> egui::Id {
    egui::Id::new("mara_main_bar_empty_drag_started")
}

#[must_use]
pub fn main_bar_empty_drag_started(ctx: &egui::Context) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(main_bar_empty_drag_started_id())
            .unwrap_or(false)
    })
}

fn effective_cluster(mode: RibbonMode, item: RibbonCluster) -> RibbonCluster {
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
struct SideInsets {
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

fn compute_side_insets(ribbons: &[ResolvedSlotRibbon]) -> SideInsets {
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

fn insets_for_ribbon(
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
            out.top = corner(edge_has_ribbon(ribbons, RibbonEdge::Top));
            out.bottom = corner(edge_has_ribbon(ribbons, RibbonEdge::Bottom));
        }
    }
    out
}

fn strip_rect(ribbon: &ResolvedSlotRibbon, ctx: &egui::Context, insets: SideInsets) -> egui::Rect {
    let screen = ribbon_rect(ctx, ribbon);
    let strip_inset = |inset: f32| {
        if inset > EDGE_GAP {
            inset + EDGE_GAP
        } else {
            EDGE_GAP
        }
    };
    match ribbon.edge {
        RibbonEdge::Left => egui::Rect::from_min_max(
            screen.min + egui::vec2(EDGE_GAP, strip_inset(insets.top)),
            egui::pos2(
                screen.min.x + EDGE_GAP + SIDE_BTN_SIZE,
                screen.max.y - strip_inset(insets.bottom),
            ),
        ),
        RibbonEdge::Right => egui::Rect::from_min_max(
            egui::pos2(
                screen.max.x - EDGE_GAP - SIDE_BTN_SIZE,
                screen.min.y + strip_inset(insets.top),
            ),
            egui::pos2(
                screen.max.x - EDGE_GAP,
                screen.max.y - strip_inset(insets.bottom),
            ),
        ),
        RibbonEdge::Top => egui::Rect::from_min_max(
            screen.min + egui::vec2(strip_inset(insets.left), EDGE_GAP),
            egui::pos2(
                screen.max.x - strip_inset(insets.right),
                screen.min.y + EDGE_GAP + SIDE_BTN_SIZE,
            ),
        ),
        RibbonEdge::Bottom => egui::Rect::from_min_max(
            egui::pos2(
                screen.min.x + strip_inset(insets.left),
                screen.max.y - EDGE_GAP - SIDE_BTN_SIZE,
            ),
            egui::pos2(
                screen.max.x - strip_inset(insets.right),
                screen.max.y - EDGE_GAP,
            ),
        ),
    }
}

fn cluster_region(
    ribbon: &ResolvedSlotRibbon,
    cluster: RibbonCluster,
    ctx: &egui::Context,
    insets: SideInsets,
) -> egui::Rect {
    let strip = strip_rect(ribbon, ctx, insets);
    match ribbon.mode {
        RibbonMode::Centered | RibbonMode::OneSided(_) => strip,
        RibbonMode::TwoSided => {
            if ribbon.edge.is_vertical() {
                let mid = (strip.top() + strip.bottom()) * 0.5;
                match cluster {
                    RibbonCluster::Start | RibbonCluster::Middle => {
                        egui::Rect::from_min_max(strip.min, egui::pos2(strip.max.x, mid))
                    }
                    RibbonCluster::End => {
                        egui::Rect::from_min_max(egui::pos2(strip.min.x, mid), strip.max)
                    }
                }
            } else {
                let mid = (strip.left() + strip.right()) * 0.5;
                match cluster {
                    RibbonCluster::Start | RibbonCluster::Middle => {
                        egui::Rect::from_min_max(strip.min, egui::pos2(mid, strip.max.y))
                    }
                    RibbonCluster::End => {
                        egui::Rect::from_min_max(egui::pos2(mid, strip.min.y), strip.max)
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
                        egui::Rect::from_min_max(strip.min, egui::pos2(strip.max.x, t1))
                    }
                    RibbonCluster::Middle => egui::Rect::from_min_max(
                        egui::pos2(strip.min.x, t1),
                        egui::pos2(strip.max.x, t2),
                    ),
                    RibbonCluster::End => {
                        egui::Rect::from_min_max(egui::pos2(strip.min.x, t2), strip.max)
                    }
                }
            } else {
                let w = strip.width() / 3.0;
                let t1 = strip.min.x + w;
                let t2 = strip.min.x + w * 2.0;
                match cluster {
                    RibbonCluster::Start => {
                        egui::Rect::from_min_max(strip.min, egui::pos2(t1, strip.max.y))
                    }
                    RibbonCluster::Middle => egui::Rect::from_min_max(
                        egui::pos2(t1, strip.min.y),
                        egui::pos2(t2, strip.max.y),
                    ),
                    RibbonCluster::End => {
                        egui::Rect::from_min_max(egui::pos2(t2, strip.min.y), strip.max)
                    }
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ButtonPlacement {
    screen: egui::Rect,
    anchor: egui::Align2,
    offset: egui::Vec2,
}

fn place_button(
    ctx: &egui::Context,
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
                egui::Align2::LEFT_BOTTOM
            } else {
                egui::Align2::LEFT_TOP
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: egui::vec2(x, y),
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
                egui::Align2::RIGHT_BOTTOM
            } else {
                egui::Align2::RIGHT_TOP
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: egui::vec2(x, y),
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
                RibbonCluster::Start => egui::Align2::LEFT_TOP,
                RibbonCluster::Middle => egui::Align2::CENTER_TOP,
                RibbonCluster::End => egui::Align2::RIGHT_TOP,
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: egui::vec2(x, y),
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
                RibbonCluster::Start => egui::Align2::LEFT_BOTTOM,
                RibbonCluster::Middle => egui::Align2::CENTER_BOTTOM,
                RibbonCluster::End => egui::Align2::RIGHT_BOTTOM,
            };
            ButtonPlacement {
                screen,
                anchor,
                offset: egui::vec2(x, y),
            }
        }
    }
}

fn screen_rect(ctx: &egui::Context, placement: ButtonPlacement) -> egui::Rect {
    let _ = ctx;
    let screen = placement.screen;
    let size = egui::vec2(SIDE_BTN_SIZE, SIDE_BTN_SIZE);
    let min = match placement.anchor {
        egui::Align2::LEFT_TOP => egui::pos2(
            screen.min.x + placement.offset.x,
            screen.min.y + placement.offset.y,
        ),
        egui::Align2::LEFT_BOTTOM => egui::pos2(
            screen.min.x + placement.offset.x,
            screen.max.y + placement.offset.y,
        ),
        egui::Align2::RIGHT_TOP => egui::pos2(
            screen.max.x + placement.offset.x,
            screen.min.y + placement.offset.y,
        ),
        egui::Align2::RIGHT_BOTTOM => egui::pos2(
            screen.max.x + placement.offset.x,
            screen.max.y + placement.offset.y,
        ),
        egui::Align2::CENTER_TOP => egui::pos2(
            screen.center().x + placement.offset.x,
            screen.min.y + placement.offset.y,
        ),
        egui::Align2::CENTER_BOTTOM => egui::pos2(
            screen.center().x + placement.offset.x,
            screen.max.y + placement.offset.y,
        ),
        _ => egui::pos2(
            screen.center().x + placement.offset.x,
            screen.center().y + placement.offset.y,
        ),
    };
    egui::Rect::from_min_size(min, size)
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

fn paint_item_glyph(ui: &mut egui::Ui, rect: egui::Rect, item: &RibbonSlotItem, fg: egui::Color32) {
    crate::icons::paint_section_icon(
        ui,
        rect.center(),
        egui::Align2::CENTER_CENTER,
        crate::icons::Icon::from(item.icon),
        18.0,
        fg,
    );
}

pub fn draw_unified_ribbon_chrome(
    ctx: &egui::Context,
    accent: egui::Color32,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
    active: impl Fn(&'static str) -> bool,
) -> Vec<egui::Id> {
    let insets = compute_side_insets(ribbons);
    let chrome = chrome_rect(ctx);
    ctx.data_mut(|d| {
        d.insert_temp(chrome_bounds_key(), chrome);
        d.insert_temp::<[bool; 4]>(
            egui::Id::new("mara_published_ribbon_edges"),
            [
                edge_has_ribbon(ribbons, RibbonEdge::Left),
                edge_has_ribbon(ribbons, RibbonEdge::Right),
                edge_has_ribbon(ribbons, RibbonEdge::Top),
                edge_has_ribbon(ribbons, RibbonEdge::Bottom),
            ],
        );
    });

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
                            let rect = screen_rect(
                                ctx,
                                place_button(
                                    ctx,
                                    ribbon,
                                    cluster_eff,
                                    slot,
                                    count + 1,
                                    insets_for_ribbon(ribbons, ribbon, insets),
                                ),
                            );
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
        let resting = screen_rect(
            ctx,
            place_button(
                ctx,
                ribbon,
                cluster_eff,
                slot_eff,
                total.max(1),
                insets_for_ribbon(ribbons, ribbon, insets),
            ),
        );
        button_rects.push(resting);
        let dragging_this = drag.item == Some(*iid);
        let paint_pos = if dragging_this {
            let center = drag.cursor.unwrap_or_else(|| resting.center());
            egui::pos2(
                center.x - SIDE_BTN_SIZE * 0.5,
                center.y - SIDE_BTN_SIZE * 0.5,
            )
        } else {
            resting.min
        };
        let order = if dragging_this {
            egui::Order::Tooltip
        } else {
            egui::Order::Foreground
        };
        let role = item_role(item, &ribbons[ribbon_idx]);
        let is_active = match role {
            RibbonRole::Panel => open.is_open(rid, iid) || active(iid),
            RibbonRole::Icon => active(iid),
        };
        let area_response = egui::Area::new(egui::Id::new(("mara_ribbon_btn", iid)))
            .order(order)
            .fixed_pos(paint_pos)
            .interactable(true)
            .show(ctx, |ui| {
                let sense = if item.draggable {
                    egui::Sense::click_and_drag()
                } else {
                    egui::Sense::click()
                };
                let (rect, response) =
                    ui.allocate_exact_size(egui::vec2(SIDE_BTN_SIZE, SIDE_BTN_SIZE), sense);
                paint_ribbon_button(
                    ui.painter(),
                    rect,
                    accent,
                    is_active,
                    response.hovered() || dragging_this,
                );
                let glyph = RibbonGlyph::Icon(item.icon);
                let fg = ribbon_button_fg(
                    accent,
                    is_active || dragging_this,
                    response.hovered() || dragging_this,
                    glyph,
                );
                paint_item_glyph(ui, rect, item, fg);
                response.on_hover_text(item.tooltip.clone())
            });
        ctx.move_to_top(area_response.response.layer_id);
        let response = area_response.inner;
        if item.draggable && response.drag_started() {
            drag_started_idx = Some(idx);
        }
        if dragging_this && response.dragged() {
            drag.cursor = ctx.pointer_interact_pos();
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
        let rect = screen_rect(
            ctx,
            place_button(
                ctx,
                ribbon,
                tgt_cluster,
                insert,
                count,
                insets_for_ribbon(ribbons, ribbon, insets),
            ),
        );
        egui::Area::new(egui::Id::new("mara_ribbon_drop_outline"))
            .order(egui::Order::Foreground)
            .fixed_pos(rect.min)
            .interactable(false)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(SIDE_BTN_SIZE, SIDE_BTN_SIZE),
                    egui::Sense::hover(),
                );
                ui.painter().rect(
                    rect,
                    crate::style::radius_for(crate::style::RadiusRole::Section),
                    crate::style::fill_for(crate::style::FillRole::DragGhost, accent),
                    crate::style::stroke_for(crate::style::StrokeRole::DragGhost, accent),
                    egui::StrokeKind::Inside,
                );
            });
    }

    if let Some(idx) = drag_started_idx {
        let (_, _, rid, iid, cluster, slot) = flat[idx];
        drag.item = Some(iid);
        drag.cursor = ctx.pointer_interact_pos();
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
        ctx.input(|i| {
            i.pointer.interact_pos().is_some_and(|pos| {
                i.pointer.button_pressed(egui::PointerButton::Primary)
                    && main_strip.contains(pos)
                    && !button_rects.iter().any(|rect| rect.contains(pos))
            })
        })
    });
    let window_chrome_capabilities = crate::window_chrome::window_chrome_host_capabilities(ctx);
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
        crate::window_chrome::publish_window_chrome_regions(
            ctx,
            drag_regions,
            button_rects.iter().copied(),
        );
    } else {
        crate::window_chrome::clear_window_chrome_regions(ctx);
    }
    if window_chrome_capabilities.native_resize {
        crate::window_chrome::paint_resize_corner_hover(
            ctx,
            accent,
            crate::style::theme().window_chrome,
        );
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            main_bar_empty_drag_started_id(),
            empty_main_bar_drag_started,
        );
    });

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
        clicks.push(item.id);
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
        ctx.data_mut(|data| data.insert_temp(chrome_bounds_key(), rect));
        ctx
    }

    fn test_ctx_with_screen_and_chrome(screen: egui::Rect, chrome: egui::Rect) -> egui::Context {
        let ctx = egui::Context::default();
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        ctx.data_mut(|data| data.insert_temp(chrome_bounds_key(), chrome));
        ctx
    }

    fn ribbon(edge: RibbonEdge) -> ResolvedSlotRibbon {
        ribbon_with_id("test_ribbon", edge)
    }

    fn ribbon_with_id(id: &'static str, edge: RibbonEdge) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: egui::Id::new((id, edge)),
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
            let rect = screen_rect(
                &ctx,
                place_button(&ctx, &ribbon, RibbonCluster::Middle, 0, 1, insets),
            );

            assert_eq!(rect.center().y, chrome.center().y);
        }
    }

    #[test]
    fn vertical_middle_button_group_centers_against_published_chrome_height() {
        let chrome = egui::Rect::from_min_size(egui::pos2(0.0, 96.0), egui::vec2(480.0, 512.0));
        let ctx = test_ctx_with_chrome(chrome);
        let insets = SideInsets::default();
        let ribbon = ribbon(RibbonEdge::Left);

        let first = screen_rect(
            &ctx,
            place_button(&ctx, &ribbon, RibbonCluster::Middle, 0, 3, insets),
        );
        let last = screen_rect(
            &ctx,
            place_button(&ctx, &ribbon, RibbonCluster::Middle, 2, 3, insets),
        );
        let group_center = (first.center().y + last.center().y) * 0.5;

        assert_eq!(group_center, chrome.center().y);
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
