//! Persistent docked Shelves.
//!
//! A Shelf is structural chrome: it reserves space on the left,
//! right, or bottom side of the workspace and hosts typed tabbed
//! containers. It is deliberately not a ribbon-opened floating
//! [`crate::pane::Pane`].

use std::collections::{HashMap, HashSet};

use crate::context::MaraCtx;
use egui::{Color32, Id, Pos2, Rect, Vec2, pos2, vec2};

use crate::container::Tab;
use crate::layout::{
    AreaHost, AreaSlotSpec, ChildRegion, CursorIcon, Layer, ScrollRegion, StackAlign,
};
use crate::paint::PaintCmd;
use crate::pane::{self, PaneAnchor, RailZone, TitleSide, active_pane_key};
use crate::ribbon::RibbonEdge;
use crate::style::{self, ShelfTheme};
use crate::vocab::{
    Color32 as MaraColor32, CornerRadius as MaraCornerRadius, Id as MaraId, Pos2 as MaraPos2,
    Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
};

/// Allowed dock edges for persistent Shelves.
///
/// There is intentionally no `Top` variant: top-level chrome is
/// owned by the persistent main bar/ribbon policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShelfEdge {
    Left,
    Right,
    Bottom,
}

impl ShelfEdge {
    #[must_use]
    pub fn is_side(self) -> bool {
        matches!(self, ShelfEdge::Left | ShelfEdge::Right)
    }

    #[must_use]
    pub fn container_anchor(self) -> PaneAnchor {
        match self {
            // Side shelves are docked vertical panes, but their
            // tabbed containers should expose tabs on the side, not
            // across the top.
            ShelfEdge::Left | ShelfEdge::Right => PaneAnchor::TopRail(RailZone::Middle),
            // Bottom shelves should expose each container's tabs
            // along the top edge of the docked pane.
            ShelfEdge::Bottom => PaneAnchor::LeftRail(RailZone::Middle),
        }
    }

    #[must_use]
    pub fn container_tab_strip_side(self) -> TitleSide {
        match self {
            ShelfEdge::Left => TitleSide::Left,
            ShelfEdge::Right => TitleSide::Right,
            ShelfEdge::Bottom => TitleSide::Top,
        }
    }
}

/// Error returned when adapting a generic screen edge into a Shelf
/// edge. `Top` is rejected at the API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfEdgeError {
    TopShelfForbidden,
}

impl TryFrom<RibbonEdge> for ShelfEdge {
    type Error = ShelfEdgeError;

    fn try_from(edge: RibbonEdge) -> Result<Self, Self::Error> {
        match edge {
            RibbonEdge::Left => Ok(ShelfEdge::Left),
            RibbonEdge::Right => Ok(ShelfEdge::Right),
            RibbonEdge::Bottom => Ok(ShelfEdge::Bottom),
            RibbonEdge::Top => Err(ShelfEdgeError::TopShelfForbidden),
        }
    }
}

/// A Shelf-hosted tabbed container. Public constructors only create
/// typed tabbed containers, so consumers cannot smuggle arbitrary
/// egui closures into Shelf content.
pub struct ShelfContainer<'a> {
    spec: crate::pane::ContainerSpec<'a>,
}

impl<'a> ShelfContainer<'a> {
    #[must_use]
    pub fn tabbed(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        tabs: Vec<Tab>,
    ) -> Self {
        Self {
            spec: crate::pane::ContainerSpec::tabbed(id, title, icon, tabs),
        }
    }
}

/// Declarative Shelf definition for one workspace level.
pub struct ShelfDef<'a> {
    pub id: MaraId,
    pub edge: ShelfEdge,
    pub accent: MaraColor32,
    pub containers: Vec<ShelfContainer<'a>>,
    pub default_size: Option<f32>,
    pub min_size: Option<f32>,
    pub max_size: Option<f32>,
    pub movable: bool,
    pub toggle_button: bool,
}

impl<'a> ShelfDef<'a> {
    #[must_use]
    pub fn new(id: impl Into<MaraId>, edge: ShelfEdge, accent: impl Into<MaraColor32>) -> Self {
        Self {
            id: id.into(),
            edge,
            accent: accent.into(),
            containers: Vec::new(),
            default_size: None,
            min_size: None,
            max_size: None,
            movable: false,
            toggle_button: true,
        }
    }

    #[must_use]
    pub fn default_size(mut self, size: f32) -> Self {
        self.default_size = Some(size);
        self
    }

    #[must_use]
    pub fn size_bounds(mut self, min: f32, max: f32) -> Self {
        self.min_size = Some(min);
        self.max_size = Some(max);
        self
    }

    #[must_use]
    pub fn movable(mut self) -> Self {
        self.movable = true;
        self
    }

    #[must_use]
    pub fn with_movable(mut self, movable: bool) -> Self {
        self.movable = movable;
        self
    }

    #[must_use]
    pub fn with_toggle_button(mut self, toggle_button: bool) -> Self {
        self.toggle_button = toggle_button;
        self
    }

    #[must_use]
    pub fn without_toggle_button(self) -> Self {
        self.with_toggle_button(false)
    }

    #[must_use]
    pub fn container(mut self, container: ShelfContainer<'a>) -> Self {
        self.containers.push(container);
        self
    }

    #[must_use]
    pub fn containers(mut self, containers: impl IntoIterator<Item = ShelfContainer<'a>>) -> Self {
        self.containers.extend(containers);
        self
    }

    fn default_extent_for(&self, edge: ShelfEdge, theme: &ShelfTheme) -> f32 {
        let fallback = if edge.is_side() {
            theme.side_default_size
        } else {
            theme.bottom_default_size
        };
        sanitize_extent(self.default_size.unwrap_or(fallback), fallback.max(0.0))
    }

    fn min_extent(&self, theme: &ShelfTheme) -> f32 {
        self.extent_bounds(theme).0
    }

    fn max_extent(&self, theme: &ShelfTheme) -> f32 {
        self.extent_bounds(theme).1
    }

    fn extent_bounds(&self, theme: &ShelfTheme) -> (f32, f32) {
        normalize_extent_bounds(
            self.min_size.unwrap_or(theme.min_size),
            self.max_size.unwrap_or(theme.max_size),
            theme,
        )
    }

    pub(crate) fn egui_id(&self) -> Id {
        self.id.into()
    }
}

fn normalize_extent_bounds(min: f32, max: f32, theme: &ShelfTheme) -> (f32, f32) {
    let fallback_min = theme.min_size.max(0.0);
    let fallback_max = theme.max_size.max(fallback_min);
    let min = sanitize_extent(min, fallback_min);
    let max = sanitize_extent(max, fallback_max);
    if min <= max { (min, max) } else { (max, min) }
}

fn sanitize_extent(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.max(0.0)
    } else {
        fallback
    }
}

mod state;

pub use state::ShelfState;
use state::{
    ShelfContainerLocation, ShelfContainerMoveState, ShelfContainerMoveUpdate, ShelfPaneInfo,
    ShelfResizeStart, detached_shelf_id,
};
/// Output of Shelf layout reservation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShelfLayout {
    pub viewport: MaraRect,
    pub left: Option<MaraRect>,
    pub right: Option<MaraRect>,
    pub bottom: Option<MaraRect>,
}

impl ShelfLayout {
    /// A no-shelf layout: the whole `viewport` is available and no
    /// edge reserves space. This is the correct value to publish
    /// when an app draws ribbons/panes but reserves no shelves —
    /// pass `ctx.content_rect()` so floating chrome tracks the live
    /// window size.
    #[must_use]
    pub fn full(viewport: impl Into<MaraRect>) -> Self {
        Self {
            viewport: viewport.into(),
            left: None,
            right: None,
            bottom: None,
        }
    }

    #[must_use]
    pub fn rect_for(self, edge: ShelfEdge) -> Option<MaraRect> {
        match edge {
            ShelfEdge::Left => self.left,
            ShelfEdge::Right => self.right,
            ShelfEdge::Bottom => self.bottom,
        }
    }

    #[must_use]
    pub fn available(self) -> MaraRect {
        let mut rect = self.viewport;
        for shelf in [self.left, self.right, self.bottom].into_iter().flatten() {
            rect.min.x = rect.min.x.min(shelf.min.x);
            rect.min.y = rect.min.y.min(shelf.min.y);
            rect.max.x = rect.max.x.max(shelf.max.x);
            rect.max.y = rect.max.y.max(shelf.max.y);
        }
        rect
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ShelfPresence {
    pub(crate) left: bool,
    pub(crate) right: bool,
    pub(crate) bottom: bool,
}

impl ShelfPresence {
    fn from_layout(layout: ShelfLayout) -> Self {
        Self {
            left: layout.left.is_some(),
            right: layout.right.is_some(),
            bottom: layout.bottom.is_some(),
        }
    }
}

/// Adapt a shelf set to the current responsive breakpoint.
///
/// On [`Breakpoint::Phone`](crate::style::Breakpoint::Phone) a bottom
/// shelf has nowhere useful to live: it would steal scarce vertical
/// space and sit under the relocated main bar. So this collapses the
/// bottom edge into the right edge — every bottom container moves into
/// the right shelf (or, if there is no right shelf, the bottom shelf is
/// promoted to the right edge) — leaving only left and right side
/// drawers. Combined with the overlay behaviour in [`layout_shelves`],
/// the side shelves act as slide-in panels on phones.
///
/// Above phone-class this is the identity transform, so desktop/tablet
/// keep their declared three-edge layout.
///
/// Call this once and pass the result to both [`layout_shelves`] and
/// the sealed shelf renderer (`ViewCtx::show_shelves` /
/// `MaraHostCtx::show_shelves`) so layout and paint agree on the shelf set.
#[must_use]
pub fn responsive_shelves<'a>(shelves: Vec<ShelfDef<'a>>) -> Vec<ShelfDef<'a>> {
    if style::screen_class() == style::Breakpoint::Phone {
        collapse_bottom_into_right(shelves)
    } else {
        shelves
    }
}

/// Merge every bottom shelf's containers into the right shelf, dropping
/// the now-empty bottom shelves. If no right shelf exists, the first
/// bottom shelf is promoted to the right edge and the rest merge into
/// it. Pure (no breakpoint check) so it is unit-testable directly.
fn collapse_bottom_into_right<'a>(shelves: Vec<ShelfDef<'a>>) -> Vec<ShelfDef<'a>> {
    let has_right = shelves.iter().any(|shelf| shelf.edge == ShelfEdge::Right);
    let mut kept: Vec<ShelfDef<'a>> = Vec::with_capacity(shelves.len());
    let mut overflow: Vec<ShelfContainer<'a>> = Vec::new();
    let mut promoted_right = false;

    for mut shelf in shelves {
        if shelf.edge != ShelfEdge::Bottom {
            kept.push(shelf);
            continue;
        }
        if !has_right && !promoted_right {
            // No right shelf to receive containers — promote this
            // bottom shelf to the right edge so it hosts the merge.
            shelf.edge = ShelfEdge::Right;
            promoted_right = true;
            kept.push(shelf);
        } else {
            overflow.append(&mut shelf.containers);
        }
    }

    if !overflow.is_empty()
        && let Some(right) = kept.iter_mut().find(|shelf| shelf.edge == ShelfEdge::Right)
    {
        right.containers.append(&mut overflow);
    }

    kept
}

/// Reserve structural Shelf space and return the remaining viewport.
pub fn layout_shelves(
    available: impl Into<MaraRect>,
    shelves: &[ShelfDef<'_>],
    state: &mut ShelfState,
    theme: &ShelfTheme,
) -> ShelfLayout {
    assert_unique_shelf_ids(shelves);
    let available = available.into();
    let mut viewport = available;
    let mut left = None;
    let mut right = None;
    let mut bottom = None;

    for entry in shelf_layout_edges(shelves, state) {
        let shelf = &shelves[entry.base_idx];
        let edge = entry.edge;
        let extent = state.extent_for_key(
            entry.shelf_id.with(edge),
            layout_entry_default_extent(state, entry.shelf_id, shelf, edge, theme),
            shelf.extent_bounds(theme),
        );
        match edge {
            ShelfEdge::Left => {
                let extent = extent.min(viewport.width().max(0.0));
                let rect = MaraRect::from_min_max(
                    viewport.min,
                    MaraPos2::new(viewport.min.x + extent, viewport.max.y),
                );
                viewport.min.x = (viewport.min.x + extent).min(viewport.max.x);
                left = Some(rect);
            }
            ShelfEdge::Right => {
                let extent = extent.min(viewport.width().max(0.0));
                let rect = MaraRect::from_min_max(
                    MaraPos2::new(viewport.max.x - extent, viewport.min.y),
                    viewport.max,
                );
                viewport.max.x = (viewport.max.x - extent).max(viewport.min.x);
                right = Some(rect);
            }
            ShelfEdge::Bottom => {
                let extent = extent.min(viewport.height().max(0.0));
                let rect = MaraRect::from_min_max(
                    MaraPos2::new(viewport.min.x, viewport.max.y - extent),
                    viewport.max,
                );
                viewport.max.y = (viewport.max.y - extent).max(viewport.min.y);
                bottom = Some(rect);
            }
        }
    }

    // On phone-class the side shelves behave as slide-in overlays
    // rather than space-reserving docks: the content keeps the full
    // width and the drawer rects (still emitted below) paint on top.
    // `show_shelves` renders them in `Order::Middle` areas, so they
    // already float above the content — we just stop stealing space.
    if style::screen_class() == style::Breakpoint::Phone {
        viewport = available;
    }

    ShelfLayout {
        viewport,
        left,
        right,
        bottom,
    }
}

fn layout_entry_default_extent(
    state: &ShelfState,
    shelf_id: Id,
    shelf: &ShelfDef<'_>,
    edge: ShelfEdge,
    theme: &ShelfTheme,
) -> f32 {
    if let Some(size) = remembered_axis_extent(state, shelf_id, edge) {
        return size;
    }
    if shelf_id == shelf.egui_id() || shelf.edge.is_side() == edge.is_side() {
        shelf.default_extent_for(edge, theme)
    } else if edge.is_side() {
        theme.side_default_size
    } else {
        theme.bottom_default_size
    }
}

fn remembered_axis_extent(state: &ShelfState, shelf_id: Id, edge: ShelfEdge) -> Option<f32> {
    let size = match edge {
        ShelfEdge::Left => state
            .edge_size(shelf_id, ShelfEdge::Left)
            .or_else(|| state.edge_size(shelf_id, ShelfEdge::Right)),
        ShelfEdge::Right => state
            .edge_size(shelf_id, ShelfEdge::Right)
            .or_else(|| state.edge_size(shelf_id, ShelfEdge::Left)),
        ShelfEdge::Bottom => state.edge_size(shelf_id, ShelfEdge::Bottom),
    };
    size.filter(|size| size.is_finite())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ShelfLayoutEntry {
    base_idx: usize,
    shelf_id: Id,
    edge: ShelfEdge,
}

fn shelf_layout_edges(shelves: &[ShelfDef<'_>], state: &ShelfState) -> Vec<ShelfLayoutEntry> {
    let all = shelf_layout_edges_all(shelves, state);
    let toggle_opt_out_edges = toggle_opt_out_edges(shelves, state);
    all.iter()
        .copied()
        .filter(|entry| {
            state.edge_visible(entry.edge) || toggle_opt_out_edges.contains(&entry.edge)
        })
        .collect()
}

fn toggle_opt_out_edges(shelves: &[ShelfDef<'_>], state: &ShelfState) -> HashSet<ShelfEdge> {
    let resolved_edges = resolved_shelf_edges(shelves, state);
    shelves
        .iter()
        .filter(|shelf| !shelf.toggle_button)
        .map(|shelf| {
            resolved_edges
                .get(&shelf.egui_id())
                .copied()
                .unwrap_or(shelf.edge)
        })
        .collect()
}

fn shelf_layout_edges_all(shelves: &[ShelfDef<'_>], state: &ShelfState) -> Vec<ShelfLayoutEntry> {
    let mut out = Vec::new();
    let resolved_edges = resolved_shelf_edges(shelves, state);
    let moved_shelf_owners = declared_moved_shelf_owners(shelves, state);
    let shelf_indices: HashMap<Id, usize> = shelves
        .iter()
        .enumerate()
        .map(|(idx, shelf)| (shelf.egui_id(), idx))
        .collect();
    for (idx, shelf) in shelves.iter().enumerate() {
        let shelf_id = shelf.egui_id();
        let default_edge = resolved_edges.get(&shelf_id).copied().unwrap_or(shelf.edge);
        if shelf.containers.is_empty() {
            if !moved_shelf_owners.contains(&shelf_id) {
                push_unique_edge(&mut out, idx, shelf_id, default_edge);
            }
            continue;
        }
        for container in &shelf.containers {
            let location =
                state.container_location(container.spec.egui_container_id(), default_edge);
            let (shelf_idx, shelf_id) = resolve_target_layout_shelf(
                shelves,
                &shelf_indices,
                &resolved_edges,
                idx,
                shelf_id,
                location,
            );
            push_unique_edge(&mut out, shelf_idx, shelf_id, location.edge);
        }
    }
    out.sort_by_key(|entry| shelf_reservation_order(entry.edge));
    out
}

fn shelf_presence_for(shelves: &[ShelfDef<'_>], state: &ShelfState) -> ShelfPresence {
    let entries = shelf_layout_edges_all(shelves, state);
    let toggle_opt_out_edges = toggle_opt_out_edges(shelves, state);
    let mut presence = ShelfPresence::default();
    for entry in entries
        .iter()
        .copied()
        .filter(|entry| !toggle_opt_out_edges.contains(&entry.edge))
    {
        match entry.edge {
            ShelfEdge::Left => presence.left = true,
            ShelfEdge::Right => presence.right = true,
            ShelfEdge::Bottom => presence.bottom = true,
        }
    }
    presence
}

fn resolve_target_layout_shelf(
    shelves: &[ShelfDef<'_>],
    shelf_indices: &HashMap<Id, usize>,
    resolved_edges: &HashMap<Id, ShelfEdge>,
    source_idx: usize,
    source_shelf: Id,
    location: ShelfContainerLocation,
) -> (usize, Id) {
    if let Some(target_shelf) = location.shelf_id {
        if let Some(idx) = shelf_indices.get(&target_shelf).copied() {
            return (idx, target_shelf);
        }
        if let Some(idx) = shelf_index_for_edge(shelves, resolved_edges, location.edge) {
            return (idx, shelves[idx].egui_id());
        }
        return (source_idx, target_shelf);
    }
    if let Some(idx) = shelf_index_for_edge(shelves, resolved_edges, location.edge) {
        return (idx, shelves[idx].egui_id());
    }
    (source_idx, source_shelf)
}

fn resolved_shelf_edges(shelves: &[ShelfDef<'_>], state: &ShelfState) -> HashMap<Id, ShelfEdge> {
    let mut out = HashMap::with_capacity(shelves.len());
    let mut occupied = HashSet::with_capacity(shelves.len());

    for shelf in shelves {
        let shelf_id = shelf.egui_id();
        if !state.edge_overrides.contains_key(&shelf_id) && occupied.insert(shelf.edge) {
            out.insert(shelf_id, shelf.edge);
        }
    }

    for shelf in shelves {
        let shelf_id = shelf.egui_id();
        if out.contains_key(&shelf_id) {
            continue;
        }
        let desired = state.edge(shelf_id, shelf.edge);
        let edge = if occupied.insert(desired) {
            desired
        } else if occupied.insert(shelf.edge) {
            shelf.edge
        } else {
            desired
        };
        out.insert(shelf_id, edge);
    }

    out
}

fn declared_moved_shelf_owners(shelves: &[ShelfDef<'_>], state: &ShelfState) -> HashSet<Id> {
    shelves
        .iter()
        .flat_map(|shelf| shelf.containers.iter())
        .filter_map(|container| {
            state
                .container_locations
                .get(&container.spec.egui_container_id())
                .and_then(|location| location.shelf_id)
        })
        .collect()
}

fn shelf_index_for_edge(
    shelves: &[ShelfDef<'_>],
    resolved_edges: &HashMap<Id, ShelfEdge>,
    edge: ShelfEdge,
) -> Option<usize> {
    shelves.iter().position(|shelf| {
        resolved_edges
            .get(&shelf.egui_id())
            .copied()
            .unwrap_or(shelf.edge)
            == edge
    })
}

fn shelf_reservation_order(edge: ShelfEdge) -> u8 {
    match edge {
        ShelfEdge::Left => 0,
        ShelfEdge::Right => 1,
        ShelfEdge::Bottom => 2,
    }
}

fn push_unique_edge(
    out: &mut Vec<ShelfLayoutEntry>,
    base_idx: usize,
    shelf_id: Id,
    edge: ShelfEdge,
) {
    if !out.iter().any(|existing| existing.edge == edge) {
        out.push(ShelfLayoutEntry {
            base_idx,
            shelf_id,
            edge,
        });
    }
}

/// Paint all Shelves and their typed tabbed containers through the
/// current egui backend.
///
/// Hidden first-party hook: app/view code should use `ViewCtx` or
/// `MaraHostCtx` instead of passing raw backend context handles around.
#[doc(hidden)]
pub fn __internal_show_shelves_egui<'a>(
    ctx: &egui::Context,
    layout: ShelfLayout,
    shelves: Vec<ShelfDef<'a>>,
    state: &mut ShelfState,
) {
    crate::enforce::__internal_enforce_defaults(ctx);
    assert_unique_shelf_ids(&shelves);
    __internal_publish_shelf_layout(ctx, layout);
    publish_shelf_presence(ctx, shelf_presence_for(&shelves, state));
    clear_published_shelf_pane_infos(ctx);
    let theme = style::theme();
    let shelf_theme = *theme.shelf();
    let available: Rect = layout.available().into();
    let mut shelves = split_shelf_render_groups(shelves, state);
    let mut tab_scope = pane::TabRoutingScope::new();
    for shelf in &mut shelves {
        for container in &mut shelf.containers {
            tab_scope.absorb_spec(&mut container.spec);
        }
    }
    let tab_routing_id = shelf_tab_routing_id();

    for shelf in shelves {
        let Some(rect) = layout.rect_for(shelf.edge) else {
            continue;
        };
        let render_id = shelf_render_id(&shelf);
        let pane_id = shelf_pane_id(&shelf);
        let shelf_id = shelf.egui_id();
        let shelf_edge = shelf.edge;
        let shelf_movable = shelf.movable;

        let area_spec = AreaSlotSpec::new(
            AreaHost::new(
                render_id.with("mara_shelf_area").into(),
                rect.min,
                Layer::Middle,
            ),
            rect.size(),
        );

        crate::backend::egui::show_area_slot(ctx, area_spec, |ui| {
            let shelf_rect = Rect::from_min_size(ui.min_rect().min, rect.size().into());
            let rect_min: Pos2 = rect.min.into();
            let screen_offset = rect_min - shelf_rect.min;
            let move_response = {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                crate::layout::UiBackend::interact(
                    &mut backend,
                    shelf_rect.into(),
                    render_id.with("background_move").into(),
                    crate::layout::Sense::ClickAndDrag,
                )
            };
            let paint_rect = shelf_paint_rect(shelf_edge, shelf_rect);
            paint_shelf_background(ui, paint_rect, shelf.accent.into(), &shelf_theme);
            let resize_response =
                resize_shelf(ui, &shelf, render_id, state, &shelf_theme, shelf_rect);

            let content_rect = shelf_content_rect(shelf_edge, shelf_rect, &shelf_theme);
            if resize_response.drag_started() || resize_response.dragged() {
                state.cancel_drag();
            }

            render_shelf_body(ShelfBodyInput {
                ui,
                content_rect,
                shelf_rect,
                screen_offset,
                layout,
                shelf,
                state,
                tab_routing_id,
                tab_scope: &mut tab_scope,
            });

            let pointer_on_resize = resize_response
                .interact_pointer
                .map(Into::into)
                .is_some_and(|pos| {
                    resize_handle_rect(shelf_edge, shelf_rect, &shelf_theme).contains(pos)
                });
            if shelf_movable
                && !resize_response.drag_started()
                && !resize_response.dragged()
                && !pointer_on_resize
            {
                handle_shelf_move_drag(ShelfMoveDragInput {
                    ctx: ui.ctx(),
                    shelf_id,
                    shelf_edge,
                    pane_id,
                    state,
                    layout,
                    available,
                    shelf_rect,
                    response: &move_response,
                });
            }
        });
    }

    update_container_move_target_from_published(ctx, state);
    finish_container_move_if_released(ctx, state);
    publish_shelf_move_preview_layout(ctx, layout, state, &shelf_theme);
    publish_container_move_preview_layout(ctx, layout, state, &shelf_theme);
    paint_shelf_move_ghost(ctx, layout, state, &shelf_theme);
    paint_container_move_ghost(ctx, layout, state, &shelf_theme);
}

fn top_ribbon_clearance() -> f32 {
    crate::ribbon::EDGE_GAP + crate::ribbon::SIDE_BTN_SIZE + crate::ribbon::SIDE_BTN_GAP
}

fn shelf_paint_rect(edge: ShelfEdge, shelf_rect: Rect) -> Rect {
    let _ = edge;
    shelf_rect
}

fn shelf_content_rect(edge: ShelfEdge, shelf_rect: Rect, theme: &ShelfTheme) -> Rect {
    let mut rect = shelf_rect.shrink(theme.padding);
    if edge.is_side() {
        // Side shelves reserve room for the horizontal main bar. On a
        // phone the bar has moved to the bottom, so reserve there and
        // let the shelf start flush with the top; otherwise reserve at
        // the top where the bar lives.
        let clearance = top_ribbon_clearance();
        if crate::style::screen_class() == crate::style::Breakpoint::Phone {
            rect.max.y = (rect.max.y - clearance).max(rect.min.y);
        } else {
            rect.min.y = (rect.min.y + clearance).min(rect.max.y);
        }
    }
    rect
}

fn assert_unique_shelf_ids(shelves: &[ShelfDef<'_>]) {
    let mut seen_shelves = HashSet::with_capacity(shelves.len());
    assert!(
        shelves.iter().all(|shelf| seen_shelves.insert(shelf.id)),
        "shelves require unique shelf ids"
    );
    let container_count = shelves.iter().map(|shelf| shelf.containers.len()).sum();
    let mut seen_containers = HashSet::with_capacity(container_count);
    assert!(
        shelves
            .iter()
            .flat_map(|shelf| shelf.containers.iter())
            .all(|container| seen_containers.insert(container.spec.egui_container_id())),
        "shelf containers require unique container ids"
    );
}

fn split_shelf_render_groups<'a>(
    shelves: Vec<ShelfDef<'a>>,
    state: &ShelfState,
) -> Vec<ShelfDef<'a>> {
    let mut groups = Vec::new();
    let moved_shelf_owners = declared_moved_shelf_owners(&shelves, state);
    let resolved_edges = resolved_shelf_edges(&shelves, state);
    let bases: Vec<ShelfRenderBase> = shelves
        .iter()
        .map(|shelf| ShelfRenderBase {
            id: shelf.egui_id(),
            edge: resolved_edges
                .get(&shelf.egui_id())
                .copied()
                .unwrap_or(shelf.edge),
            accent: shelf.accent,
            default_size: shelf.default_size,
            min_size: shelf.min_size,
            max_size: shelf.max_size,
            movable: shelf.movable,
            toggle_button: shelf.toggle_button,
        })
        .collect();
    for mut shelf in shelves {
        let shelf_id = shelf.egui_id();
        let default_edge = resolved_edges.get(&shelf_id).copied().unwrap_or(shelf.edge);
        shelf.edge = default_edge;
        if shelf.containers.is_empty() {
            if !moved_shelf_owners.contains(&shelf_id) {
                push_shelf_render_group(&mut groups, shelf, default_edge);
            }
            continue;
        }
        let base = ShelfRenderBase {
            id: shelf_id,
            edge: default_edge,
            accent: shelf.accent,
            default_size: shelf.default_size,
            min_size: shelf.min_size,
            max_size: shelf.max_size,
            movable: shelf.movable,
            toggle_button: shelf.toggle_button,
        };
        for container in shelf.containers {
            let location =
                state.container_location(container.spec.egui_container_id(), default_edge);
            let target_base = resolve_target_render_base(&bases, base, location);
            push_container_render_group(&mut groups, target_base, location.edge, container);
        }
    }
    groups
}

#[derive(Debug, Clone, Copy)]
struct ShelfRenderBase {
    id: Id,
    edge: ShelfEdge,
    accent: MaraColor32,
    default_size: Option<f32>,
    min_size: Option<f32>,
    max_size: Option<f32>,
    movable: bool,
    toggle_button: bool,
}

fn resolve_target_render_base(
    bases: &[ShelfRenderBase],
    source_base: ShelfRenderBase,
    location: ShelfContainerLocation,
) -> ShelfRenderBase {
    if let Some(target_shelf) = location.shelf_id {
        if let Some(base) = bases.iter().find(|base| base.id == target_shelf).copied() {
            return base;
        }
        if let Some(base) = bases
            .iter()
            .find(|base| base.edge == location.edge)
            .copied()
        {
            return base;
        }
        return ShelfRenderBase {
            id: target_shelf,
            edge: location.edge,
            ..source_base
        };
    }
    bases
        .iter()
        .find(|base| base.edge == location.edge)
        .copied()
        .unwrap_or(source_base)
}

fn push_shelf_render_group<'a>(
    groups: &mut Vec<ShelfDef<'a>>,
    mut shelf: ShelfDef<'a>,
    edge: ShelfEdge,
) {
    shelf.edge = edge;
    if groups.iter().any(|group| group.edge == edge) {
        return;
    }
    groups.push(shelf);
}

fn push_container_render_group<'a>(
    groups: &mut Vec<ShelfDef<'a>>,
    base: ShelfRenderBase,
    edge: ShelfEdge,
    container: ShelfContainer<'a>,
) {
    if let Some(group) = groups.iter_mut().find(|group| group.edge == edge) {
        group.containers.push(container);
        return;
    }
    groups.push(ShelfDef {
        id: base.id.into(),
        edge,
        accent: base.accent,
        containers: vec![container],
        default_size: base.default_size,
        min_size: base.min_size,
        max_size: base.max_size,
        movable: base.movable,
        toggle_button: base.toggle_button,
    });
}

fn shelf_render_id(shelf: &ShelfDef<'_>) -> Id {
    shelf_render_key(shelf.egui_id(), shelf.edge)
}

fn shelf_render_key(shelf_id: Id, edge: ShelfEdge) -> Id {
    shelf_id.with(edge)
}

fn shelf_pane_id(shelf: &ShelfDef<'_>) -> Id {
    shelf_render_id(shelf).with("shelf_pane_scope")
}

fn shelf_tab_routing_id() -> Id {
    Id::new("mara_shelf_tab_routing_scope")
}

fn shelf_active_container_key(shelf: &ShelfDef<'_>) -> Id {
    shelf_active_container_key_for(shelf.egui_id(), shelf.edge)
}

fn shelf_active_container_key_for(shelf_id: Id, edge: ShelfEdge) -> Id {
    shelf_render_key(shelf_id, edge).with("active_container")
}

struct ShelfBodyInput<'ui, 'state, 'scope, 'a> {
    ui: &'ui mut egui::Ui,
    content_rect: Rect,
    shelf_rect: Rect,
    screen_offset: Vec2,
    layout: ShelfLayout,
    shelf: ShelfDef<'a>,
    state: &'state mut ShelfState,
    tab_routing_id: Id,
    tab_scope: &'scope mut pane::TabRoutingScope,
}

fn render_shelf_body(input: ShelfBodyInput<'_, '_, '_, '_>) {
    let ShelfBodyInput {
        ui,
        content_rect,
        shelf_rect,
        screen_offset,
        layout,
        shelf,
        state,
        tab_routing_id,
        tab_scope,
    } = input;
    let pane_id = shelf_pane_id(&shelf);
    let shelf_id = shelf.egui_id();
    let anchor = shelf.edge.container_anchor();
    // Container stack axis mirrors `Pane::lay_out_flex`: vertical-
    // strip title sides stack containers horizontally, horizontal-
    // strip title sides stack them vertically. The previous
    // implementation hard-coded `top_down` for every Shelf edge, so
    // the Bottom shelf stacked containers vertically when they
    // should flow horizontally — and the drag ghost-gap allocated
    // along the wrong axis as a result.
    let horizontal_stack = !anchor.title_side().is_horizontal_strip();
    ui.ctx().data_mut(|d| {
        d.insert_temp(active_pane_key(), pane_id);
        d.insert_temp(pane_id.with("mara_pane_open_elapsed"), 99.0_f32);
        d.insert_temp(pane_id.with("mara_pane_section_idx"), 0_u32);
    });
    pane::clear_container_min_widths(ui.ctx(), pane_id);

    // Body viewport — same role as the `ui` `Pane::lay_out_flex`
    // hands to the body closure. All drag plumbing (cache writes,
    // trailing ghost gap, finalize, preview) runs on THIS ui so the
    // recorded rects, the ghost slot, and the cursor/release event
    // all share one coordinate space.
    let child_region = shelf_body_child_region(content_rect, horizontal_stack, shelf.edge);
    let scroll_region = shelf_body_scroll_region(pane_id, content_rect, horizontal_stack);
    crate::backend::egui::show_child_sticky_scroll_region(
        ui,
        child_region,
        scroll_region,
        |viewport| {
            let input = crate::backend::egui::input_snapshot_for_ui(viewport);
            pane::begin_drag_frame(viewport.ctx(), pane_id);
            pane::clear_container_dot_rects(viewport.ctx(), pane_id);
            clear_external_container_gap(viewport.ctx(), pane_id);
            let pre_body_drag = pane::drag_state(viewport.ctx(), pane_id);
            if let (Some(item), Some(pos)) =
                (pre_body_drag.item, input.interact_pointer.map(Into::into))
            {
                pane::set_drag(
                    viewport.ctx(),
                    pane_id,
                    pane::DragState {
                        item: Some(item),
                        cursor: Some(pos),
                    },
                );
            }
            pane::tab_drag::begin_frame(viewport.ctx(), pane_id);

            let screen_shelf_rect = shelf_rect.translate(screen_offset);
            let pointer_cursor = input.interact_pointer.or(input.pointer).map(Into::into);
            let suppress_source_container_gap = should_suppress_source_container_gap(
                pre_body_drag,
                state.container_move,
                pane_id,
                shelf.edge,
                screen_shelf_rect,
                layout,
                pointer_cursor,
            );
            pane::set_ghost_gap_suppressed(viewport.ctx(), pane_id, suppress_source_container_gap);
            let external_container_gap = should_render_external_container_gap(
                pre_body_drag,
                state.container_move,
                shelf.edge,
                screen_shelf_rect,
                pointer_cursor,
            )
            .then_some(())
            .and(state.container_move);
            let saved_target_snapshot = if let Some(drag) = external_container_gap {
                let mut synthetic_snapshot = pane::snapshot(viewport.ctx(), pane_id);
                synthetic_snapshot.retain(|entry| entry.id != drag.container_id);
                let size = container_move_ghost_size_for_edge(
                    viewport.ctx(),
                    drag.container_id,
                    shelf.edge,
                    content_rect,
                );
                synthetic_snapshot.push(pane::RectEntry {
                    id: drag.container_id,
                    rect: Rect::from_min_size(content_rect.min, size),
                    frame: None,
                });
                pane::set_snapshot(viewport.ctx(), pane_id, synthetic_snapshot);
                pane::set_drag(
                    viewport.ctx(),
                    pane_id,
                    pane::DragState {
                        item: Some(drag.container_id),
                        cursor: Some(pointer_cursor.unwrap_or(drag.cursor)),
                    },
                );
                mark_external_container_gap(viewport.ctx(), pane_id);
                Some(pre_body_drag)
            } else {
                None
            };
            if saved_target_snapshot.is_none()
                && let Some(dragged_id) = pre_body_drag.item
            {
                reanchor_source_shelf_snapshot(
                    viewport.ctx(),
                    pane_id,
                    dragged_id,
                    shelf.edge,
                    content_rect,
                );
            }

            let active_key = shelf_active_container_key(&shelf);
            let specs: Vec<_> = shelf
                .containers
                .into_iter()
                .map(|container| container.spec)
                .collect();
            let declared_order: Vec<Id> =
                specs.iter().map(|spec| spec.egui_container_id()).collect();
            let responses = crate::pane::render_containers_with_tab_scope(
                viewport,
                pane_id,
                tab_routing_id,
                anchor,
                shelf.accent.into(),
                specs,
                tab_scope,
                Some(shelf.edge.container_tab_strip_side()),
            );

            let effective_active = resolve_visible_active_container(
                viewport.ctx(),
                pane_id,
                state.active_container_for_group(active_key),
                &declared_order,
                |id| responses.contains_key(&id),
            );
            if let Some(container_id) = effective_active {
                state.set_active_container_for_group(active_key, container_id);
            }
            if let Some(container_id) = effective_active {
                state.set_active_container(shelf_id, container_id);
            } else {
                state.clear_active_container(shelf_id);
                state.clear_active_container_for_group(active_key);
            }

            // ── Trailing ghost gap ──
            //
            // Same logic as `Pane::lay_out_flex`: when the cursor's slot is
            // past the last rendered container, paint the gap inline at the
            // end of the viewport so the trailing drop position is visible.
            let drag_state = pane::drag_state(viewport.ctx(), pane_id);
            if let Some(dragged_id) = drag_state.item
                && !pane::ghost_gap_suppressed(viewport.ctx(), pane_id)
            {
                let snap = pane::target_cache(viewport.ctx(), pane_id);
                let total = pane::current_cache(viewport.ctx(), pane_id).len();
                let cursor = input.interact_pointer.map(Into::into).or(drag_state.cursor);
                if let Some(c) = cursor {
                    let cursor_axis = if horizontal_stack { c.x } else { c.y };
                    let target_idx =
                        pane::compute_target(&snap, dragged_id, cursor_axis, horizontal_stack);
                    if target_idx >= total
                        && let Some(entry) = pane::dragged_entry(&snap, dragged_id)
                    {
                        let entry = source_shelf_gap_entry(
                            viewport.ctx(),
                            dragged_id,
                            shelf.edge,
                            content_rect,
                            entry,
                        );
                        pane::paint_ghost_gap_entry_inline(
                            viewport,
                            entry,
                            shelf.accent.into(),
                            horizontal_stack,
                        );
                    }
                }
            }

            pane::finalize_snapshot(viewport.ctx(), pane_id);
            publish_shelf_pane_info(
                viewport.ctx(),
                ShelfPaneInfo {
                    shelf_id,
                    pane_id,
                    edge: shelf.edge,
                    horizontal_stack,
                    content_rect,
                    screen_rect: shelf_rect.translate(screen_offset),
                    screen_offset,
                    accent: shelf.accent.into(),
                },
            );
            update_container_move_target_slot(
                viewport,
                shelf_id,
                pane_id,
                shelf.edge,
                horizontal_stack,
                content_rect,
                state,
            );

            let external_gap_drag = saved_target_snapshot.is_some();
            if let Some(saved_drag) = saved_target_snapshot {
                if saved_drag.item.is_some() {
                    pane::set_drag(viewport.ctx(), pane_id, saved_drag);
                } else {
                    pane::clear_drag(viewport.ctx(), pane_id);
                }
            }
            if external_gap_drag {
                return;
            }

            if let Some(dragged_id) = drag_state.item {
                let snap = pane::target_cache(viewport.ctx(), pane_id);
                let cursor = input.interact_pointer.map(Into::into).or(drag_state.cursor);
                if let Some(c) = cursor {
                    if let Some(entry) = pane::dragged_entry(&snap, dragged_id) {
                        let screen_shelf_rect = shelf_rect.translate(screen_offset);
                        let target_edge = container_move_target_for_cursor(
                            c,
                            screen_shelf_rect,
                            layout,
                            shelf.edge,
                        );
                        let target_size = target_edge
                            .map(|edge| {
                                container_move_ghost_size_for_edge(
                                    viewport.ctx(),
                                    dragged_id,
                                    edge,
                                    content_rect,
                                )
                            })
                            .unwrap_or_else(|| entry.rect.size());
                        state.update_container_move(ShelfContainerMoveUpdate {
                            container_id: dragged_id,
                            source_shelf: shelf_id,
                            source_pane: pane_id,
                            source_edge: shelf.edge,
                            cursor: c,
                            target_edge,
                            container_size: target_size,
                        });
                        update_container_move_target_from_published(viewport.ctx(), state);
                    }
                    if should_paint_source_container_preview(
                        drag_state,
                        state.container_move,
                        pane_id,
                        shelf.edge,
                    ) {
                        pane::paint_drag_preview(
                            viewport.ctx(),
                            pane_id,
                            &snap,
                            dragged_id,
                            c,
                            shelf.accent.into(),
                        );
                    }
                    crate::backend::egui::set_cursor_icon_for_context(
                        viewport.ctx(),
                        CursorIcon::Grabbing,
                    );
                }

                if input.any_released {
                    if let Some(drag) = state
                        .container_move
                        .filter(|drag| drag.container_id == dragged_id)
                    {
                        if drag.target_edge.is_some() {
                            return;
                        }
                        let screen_shelf_rect = shelf_rect.translate(screen_offset);
                        if should_cancel_no_target_container_release(cursor, screen_shelf_rect) {
                            pane::clear_drag(viewport.ctx(), pane_id);
                            state.clear_container_move();
                            return;
                        }
                    }
                    if let Some(c) = cursor {
                        let cursor_axis = if horizontal_stack { c.x } else { c.y };
                        commit_shelf_container_reorder(
                            viewport.ctx(),
                            pane_id,
                            dragged_id,
                            cursor_axis,
                            horizontal_stack,
                        );
                    }
                    pane::clear_drag(viewport.ctx(), pane_id);
                    state.clear_container_move();
                }
            }

            // ── Tab drag: preview + commit-on-release (Shelf scope) ──
            //
            // `render_containers` runs through the same tab-drag plumbing as
            // a normal Pane, so the drag STARTS work in a Shelf. Without
            // this block the pointer-release event has nowhere to commit /
            // clear, leaving the dragged tab stuck to the cursor.
            if let Some(tab_drag_state) = pane::tab_drag::drag_state(viewport.ctx(), pane_id) {
                let cursor = input.pointer.map(Into::into).or(tab_drag_state.cursor);
                if let Some(c) = cursor {
                    pane::tab_drag::set_drag(
                        viewport.ctx(),
                        pane_id,
                        pane::tab_drag::TabDragState {
                            cursor: Some(c),
                            ..tab_drag_state
                        },
                    );
                    pane::tab_drag::paint_drag_preview(
                        viewport.ctx(),
                        pane_id,
                        MaraVec2::new(28.0, 28.0),
                        c,
                        shelf.accent.into(),
                        "",
                        tab_drag_state.icon,
                    );
                    crate::backend::egui::set_cursor_icon_for_context(
                        viewport.ctx(),
                        CursorIcon::Grabbing,
                    );
                }
                if input.any_released {
                    if let Some(c) = cursor
                        && let Some((tgt_cid, slot)) = pane::tab_drag::find_drop_target_for_drag(
                            viewport.ctx(),
                            pane_id,
                            c,
                            tab_drag_state,
                        )
                    {
                        pane::tab_drag::commit_drop(
                            viewport.ctx(),
                            tab_routing_id,
                            tab_drag_state.tab_id,
                            tab_drag_state.source_container,
                            tgt_cid,
                            slot,
                        );
                    }
                    pane::tab_drag::clear_drag(viewport.ctx(), pane_id);
                }
            }
        },
    );
}

fn shelf_body_child_region(rect: Rect, horizontal_stack: bool, edge: ShelfEdge) -> ChildRegion {
    if horizontal_stack {
        ChildRegion::left_to_right(rect.into(), StackAlign::Min)
    } else if edge.is_side() {
        // Side shelves stack containers vertically. Center them in
        // the shelf span so tabbed containers keep equal left/right
        // breathing room instead of being pinned to the left edge
        // (which makes right-shelf tabs look shoved inward).
        ChildRegion::top_down(rect.into(), StackAlign::Center)
    } else {
        ChildRegion::top_down(rect.into(), StackAlign::Min)
    }
}

fn shelf_body_scroll_region(
    pane_id: Id,
    content_rect: Rect,
    horizontal_stack: bool,
) -> ScrollRegion {
    let id = pane_id.with("mara_shelf_body_scroll").into();
    let spacing = MaraVec2::ZERO;
    if horizontal_stack {
        ScrollRegion::horizontal(id, [false, false], content_rect.width().max(0.0), spacing)
    } else {
        ScrollRegion::vertical(id, [false, false], content_rect.height().max(0.0), spacing)
    }
}

fn resolve_visible_active_container(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    active: Option<Id>,
    declared_order: &[Id],
    is_visible: impl Fn(Id) -> bool,
) -> Option<Id> {
    if let Some(active) = active.filter(|active| is_visible(*active)) {
        return Some(active);
    }
    shelf_display_order(ctx, pane_id, declared_order.iter()).find(|id| is_visible(*id))
}

/// Publish the post-Shelf viewport as the chrome bounds for floating
/// ribbons/panes.
///
/// Internal first-party host hook. App/view code should publish shelf
/// layout through the Mara host facade; the hidden egui shelf renderer still
/// does this automatically for real shelf chrome.
#[doc(hidden)]
pub fn __internal_publish_shelf_layout(ctx: &dyn crate::context::MaraCtx, layout: ShelfLayout) {
    // Record the app/host publish (no-op while the enforcement baseline
    // itself publishes) so `crate::enforce` doesn't stomp a real layout.
    crate::enforce::mark_app_shelf_published(ctx);
    let pass = ctx.pass_nr();
    let mut memory = ctx.memory();
    memory.set_temp(shelf_layout_key(), layout);
    memory.set_temp(shelf_layout_pass_key(), pass);
    memory.set_temp(shelf_presence_key(), ShelfPresence::from_layout(layout));
    memory.set_temp(crate::ribbon::chrome::chrome_bounds_key(), layout.viewport);
}

#[must_use]
#[doc(hidden)]
pub fn __internal_shelf_layout(ctx: &dyn crate::context::MaraCtx) -> Option<ShelfLayout> {
    ctx.memory().get_temp::<ShelfLayout>(shelf_layout_key())
}

/// Whether a shelf layout was already published during the current
/// egui pass. A host's "auto-publish a baseline layout" system uses
/// this to avoid clobbering an explicit facade publication / sealed shelf
/// render call made by app code earlier or later in the same pass — whoever
/// publishes "for real" wins, order-independent.
#[must_use]
#[doc(hidden)]
pub fn __internal_shelf_layout_published_this_pass(ctx: &dyn crate::context::MaraCtx) -> bool {
    let pass = ctx.pass_nr();
    ctx.memory().get_temp::<u64>(shelf_layout_pass_key()) == Some(pass)
}

fn shelf_layout_key() -> egui::Id {
    egui::Id::new("mara.shelf.layout")
}

fn shelf_layout_pass_key() -> egui::Id {
    egui::Id::new("mara.shelf.layout.pass")
}

pub(crate) fn published_shelf_presence(ctx: &dyn crate::context::MaraCtx) -> ShelfPresence {
    ctx.memory()
        .get_temp::<ShelfPresence>(shelf_presence_key())
        .unwrap_or_default()
}

fn publish_shelf_presence(ctx: &dyn crate::context::MaraCtx, presence: ShelfPresence) {
    ctx.memory().set_temp(shelf_presence_key(), presence);
}

fn shelf_presence_key() -> egui::Id {
    egui::Id::new("mara.shelf.presence")
}

fn publish_container_move_preview_layout(
    ctx: &dyn crate::context::MaraCtx,
    layout: ShelfLayout,
    state: &ShelfState,
    theme: &ShelfTheme,
) {
    let Some(drag) = state.container_move else {
        return;
    };
    let Some(preview) = container_move_preview_layout(ctx, layout, drag, theme) else {
        return;
    };
    __internal_publish_shelf_layout(ctx, preview);
}

fn publish_shelf_move_preview_layout(
    ctx: &dyn crate::context::MaraCtx,
    layout: ShelfLayout,
    state: &ShelfState,
    theme: &ShelfTheme,
) {
    let Some(drag) = state.drag else {
        return;
    };
    let Some(preview) = shelf_move_preview_layout(layout, drag, state, theme) else {
        return;
    };
    __internal_publish_shelf_layout(ctx, preview);
}

fn shelf_display_order<'a>(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    containers: impl Iterator<Item = &'a Id>,
) -> impl Iterator<Item = Id> {
    let defaults: Vec<Id> = containers.copied().collect();
    pane::section_order_for(ctx, pane_id, &defaults).into_iter()
}

fn commit_shelf_container_reorder(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    dragged_id: Id,
    cursor_axis: f32,
    horizontal_stack: bool,
) {
    let cache = shelf_target_cache(ctx, pane_id);
    let target_idx = pane::compute_target(&cache, dragged_id, cursor_axis, horizontal_stack);
    let defaults: Vec<Id> = cache.iter().map(|e| e.id).collect();
    let mut order = pane::section_order_for(ctx, pane_id, &defaults);
    order.retain(|cid| *cid != dragged_id);
    let clamped = target_idx.min(order.len());
    order.insert(clamped, dragged_id);
    pane::set_section_order(ctx, pane_id, order);
}

fn paint_shelf_background(ui: &mut egui::Ui, rect: Rect, accent: Color32, theme: &ShelfTheme) {
    let active = style::theme();
    let fill: MaraColor32 = style::glass_fill(active.bg_panel, accent, theme.background_alpha);
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    crate::layout::UiBackend::paint(&mut backend, shelf_background_paint_cmd(rect.into(), fill));
    // Border line in the same style as the pane frame (WidgetBorder
    // stroke), so a docked shelf reads as a framed surface like a pane.
    crate::layout::UiBackend::paint(
        &mut backend,
        PaintCmd::RectStroke {
            rect: rect.into(),
            corner: MaraCornerRadius::ZERO,
            stroke: style::stroke_for(style::StrokeRole::WidgetBorder, accent),
        },
    );
}

fn shelf_background_paint_cmd(rect: MaraRect, fill: MaraColor32) -> PaintCmd {
    PaintCmd::RectFilled {
        rect,
        corner: MaraCornerRadius::ZERO,
        fill,
    }
}

fn resize_shelf(
    ui: &mut egui::Ui,
    shelf: &ShelfDef<'_>,
    render_id: Id,
    state: &mut ShelfState,
    theme: &ShelfTheme,
    rect: Rect,
) -> crate::mui::MaraResponse {
    let handle = resize_handle_rect(shelf.edge, rect, theme);
    let shelf_id = shelf.egui_id();
    let size_key = shelf_id.with(shelf.edge);
    let cursor = shelf_resize_cursor(shelf.edge);
    let resp = {
        let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
        crate::layout::UiBackend::interact(
            &mut backend,
            handle.into(),
            render_id.with("resize").into(),
            crate::layout::Sense::Drag,
        )
    };
    crate::backend::egui::hover_cursor_for_ui_response(ui, &resp, cursor);
    if resp.drag_started() {
        let cur = state
            .edge_size(shelf_id, shelf.edge)
            .unwrap_or_else(|| shelf.default_extent_for(shelf.edge, theme));
        if let Some(pointer) = resp.interact_pointer.map(Into::into) {
            state
                .resize_starts
                .insert(size_key, ShelfResizeStart { size: cur, pointer });
        }
    }
    if let Some(start) = state.resize_starts.get(&size_key).copied() {
        let input = crate::backend::egui::input_snapshot_for_ui(ui);
        let pointer = input
            .interact_pointer
            .map(Into::into)
            .or_else(|| resp.interact_pointer.map(Into::into));
        if input.primary_down {
            let pointer = pointer.unwrap_or(start.pointer);
            let delta = pointer - start.pointer;
            let next = resized_shelf_extent(
                shelf.edge,
                start.size,
                delta,
                shelf.min_extent(theme),
                shelf.max_extent(theme),
            );
            state.set_edge_size(shelf_id, shelf.edge, next);
            crate::backend::egui::request_repaint(ui.ctx());
        } else {
            state.resize_starts.remove(&size_key);
        }
    } else if resp.dragged() {
        let start = state
            .edge_size(shelf_id, shelf.edge)
            .unwrap_or_else(|| shelf.default_extent_for(shelf.edge, theme));
        let next = resized_shelf_extent(
            shelf.edge,
            start,
            resp.drag_delta.into(),
            shelf.min_extent(theme),
            shelf.max_extent(theme),
        );
        state.set_edge_size(shelf_id, shelf.edge, next);
        crate::backend::egui::request_repaint(ui.ctx());
    }
    if resp.drag_stopped() {
        state.resize_starts.remove(&size_key);
    }
    if resp.hovered() || resp.dragged() || state.resize_starts.contains_key(&size_key) {
        crate::backend::egui::set_cursor_icon_for_ui(ui, cursor);
    }
    resp
}

fn resized_shelf_extent(edge: ShelfEdge, start: f32, delta: Vec2, min: f32, max: f32) -> f32 {
    let raw_delta = match edge {
        ShelfEdge::Left => delta.x,
        ShelfEdge::Right => -delta.x,
        ShelfEdge::Bottom => -delta.y,
    };
    (start + raw_delta).clamp(min, max)
}

fn update_container_move_target_slot(
    viewport: &mut egui::Ui,
    shelf_id: Id,
    pane_id: Id,
    shelf_edge: ShelfEdge,
    horizontal_stack: bool,
    content_rect: Rect,
    state: &mut ShelfState,
) {
    let Some(drag) = state.container_move else {
        return;
    };
    if drag.target_edge != Some(shelf_edge) {
        return;
    }
    let snap = shelf_target_cache(viewport.ctx(), pane_id);
    let input = crate::backend::egui::input_snapshot_for_ui(viewport);
    let cursor = input
        .interact_pointer
        .or(input.pointer)
        .map(Into::into)
        .unwrap_or(drag.cursor);
    let cursor_axis = if horizontal_stack { cursor.x } else { cursor.y };
    let target_slot = pane::compute_target(&snap, drag.container_id, cursor_axis, horizontal_stack);
    let target_size = container_move_ghost_size_for_edge(
        viewport.ctx(),
        drag.container_id,
        shelf_edge,
        content_rect,
    );
    state.update_container_move_target_slot(shelf_id, pane_id, target_slot, target_size);
}

fn shelf_pane_info_key(edge: ShelfEdge) -> Id {
    Id::new("mara_shelf_pane_info").with(edge)
}

fn publish_shelf_pane_info(ctx: &dyn crate::context::MaraCtx, info: ShelfPaneInfo) {
    ctx.memory().set_temp(shelf_pane_info_key(info.edge), info);
}

fn shelf_pane_info(ctx: &dyn crate::context::MaraCtx, edge: ShelfEdge) -> Option<ShelfPaneInfo> {
    ctx.memory().get_temp(shelf_pane_info_key(edge))
}

fn clear_published_shelf_pane_infos(ctx: &dyn crate::context::MaraCtx) {
    let mut memory = ctx.memory();
    for edge in [ShelfEdge::Left, ShelfEdge::Right, ShelfEdge::Bottom] {
        memory.remove_temp::<ShelfPaneInfo>(shelf_pane_info_key(edge));
    }
}

fn external_container_gap_key(pane_id: Id) -> Id {
    pane_id.with("mara_shelf_external_container_gap")
}

fn mark_external_container_gap(ctx: &dyn crate::context::MaraCtx, pane_id: Id) {
    ctx.memory().set_temp(external_container_gap_key(pane_id), true);
}

fn clear_external_container_gap(ctx: &dyn crate::context::MaraCtx, pane_id: Id) {
    ctx.memory().remove_temp::<bool>(external_container_gap_key(pane_id));
}

fn external_container_gap_was_painted(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> bool {
    ctx.memory()
        .get_temp::<bool>(external_container_gap_key(pane_id))
        .unwrap_or(false)
}

fn update_container_move_target_from_published(ctx: &dyn crate::context::MaraCtx, state: &mut ShelfState) {
    let Some(drag) = state.container_move else {
        return;
    };
    let Some(target_edge) = drag.target_edge else {
        return;
    };
    let Some(info) = shelf_pane_info(ctx, target_edge) else {
        state.clear_container_move_target_slot();
        return;
    };
    let input = ctx.input();
    let cursor = input
        .interact_pointer
        .or(input.pointer)
        .map(Into::into)
        .unwrap_or(drag.cursor);
    if !info.screen_rect.expand(24.0).contains(cursor) {
        state.clear_container_move_target_slot();
        return;
    }
    let snap = shelf_target_cache(ctx, info.pane_id);
    let cursor_axis = if info.horizontal_stack {
        cursor.x
    } else {
        cursor.y
    };
    let target_slot =
        pane::compute_target(&snap, drag.container_id, cursor_axis, info.horizontal_stack);
    let target_size =
        container_move_ghost_size_for_edge(ctx, drag.container_id, info.edge, info.content_rect);
    state.update_container_move_target_slot(info.shelf_id, info.pane_id, target_slot, target_size);
}

fn should_suppress_source_container_gap(
    source_pane_drag: pane::DragState,
    container_move: Option<ShelfContainerMoveState>,
    pane_id: Id,
    shelf_edge: ShelfEdge,
    screen_shelf_rect: Rect,
    layout: ShelfLayout,
    pointer_cursor: Option<Pos2>,
) -> bool {
    let Some(dragged_id) = source_pane_drag.item else {
        return false;
    };

    if let Some(cursor) = pointer_cursor.or(source_pane_drag.cursor) {
        return container_move_target_for_cursor(cursor, screen_shelf_rect, layout, shelf_edge)
            .is_some();
    }

    container_move.is_some_and(|drag| {
        drag.container_id == dragged_id
            && drag.source_pane == pane_id
            && drag.source_edge == shelf_edge
            && drag.target_edge.is_some_and(|target| target != shelf_edge)
    })
}

fn should_paint_source_container_preview(
    source_pane_drag: pane::DragState,
    container_move: Option<ShelfContainerMoveState>,
    pane_id: Id,
    shelf_edge: ShelfEdge,
) -> bool {
    let Some(dragged_id) = source_pane_drag.item else {
        return false;
    };
    !container_move.is_some_and(|drag| {
        drag.container_id == dragged_id
            && drag.source_pane == pane_id
            && drag.source_edge == shelf_edge
            && drag.target_edge.is_some_and(|target| target != shelf_edge)
    })
}

fn should_render_external_container_gap(
    source_pane_drag: pane::DragState,
    container_move: Option<ShelfContainerMoveState>,
    shelf_edge: ShelfEdge,
    screen_shelf_rect: Rect,
    pointer_cursor: Option<Pos2>,
) -> bool {
    source_pane_drag.item.is_none()
        && container_move.is_some_and(|drag| {
            let cursor = pointer_cursor.unwrap_or(drag.cursor);
            // There is only one render group per edge. If the
            // container is hovering an already-rendered target edge,
            // synthesize the inline gap in that current edge group
            // even when `drag.target_shelf` still contains a stale
            // owner id from a previous frame. The published-pane
            // pass refreshes the real target owner before commit.
            drag.target_edge == Some(shelf_edge)
                && drag.source_edge != shelf_edge
                && screen_shelf_rect.expand(24.0).contains(cursor)
        })
}

fn source_shelf_gap_entry(
    ctx: &dyn crate::context::MaraCtx,
    dragged_id: Id,
    shelf_edge: ShelfEdge,
    content_rect: Rect,
    entry: pane::RectEntry,
) -> pane::RectEntry {
    if content_rect.contains(entry.rect.center()) {
        return entry;
    }

    let size = container_move_ghost_size_for_edge(ctx, dragged_id, shelf_edge, content_rect)
        .min(content_rect.size());
    pane::RectEntry {
        rect: Rect::from_min_size(content_rect.min, size),
        ..entry
    }
}

fn reanchor_source_shelf_snapshot(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: Id,
    dragged_id: Id,
    shelf_edge: ShelfEdge,
    content_rect: Rect,
) {
    let mut snapshot = pane::snapshot(ctx, pane_id);
    let Some(entry) = snapshot.iter_mut().find(|entry| entry.id == dragged_id) else {
        return;
    };
    *entry = source_shelf_gap_entry(ctx, dragged_id, shelf_edge, content_rect, *entry);
    pane::set_snapshot(ctx, pane_id, snapshot);
}

fn finish_container_move_if_released(ctx: &dyn crate::context::MaraCtx, state: &mut ShelfState) {
    if !ctx.input().any_released {
        return;
    }
    let Some(drag) = state.container_move else {
        return;
    };
    commit_container_move(ctx, state, drag);
}

fn commit_container_move(
    ctx: &dyn crate::context::MaraCtx,
    state: &mut ShelfState,
    drag: ShelfContainerMoveState,
) {
    let Some(target) = drag.target_edge else {
        pane::clear_drag(ctx, drag.source_pane);
        state.clear_container_move();
        return;
    };

    let target_shelf = drag.target_shelf.unwrap_or_else(|| {
        state
            .container_locations
            .get(&drag.container_id)
            .and_then(|location| location.shelf_id)
            .unwrap_or_else(|| detached_shelf_id(drag.source_shelf, drag.container_id))
    });
    state.set_container_location(drag.container_id, Some(target_shelf), target);
    let target_group_key = shelf_active_container_key_for(target_shelf, target);
    let source_group_key = shelf_active_container_key_for(drag.source_shelf, drag.source_edge);
    state.set_active_container(target_shelf, drag.container_id);
    state.set_active_container_for_group(target_group_key, drag.container_id);
    if target_shelf != drag.source_shelf
        && state.active_container(drag.source_shelf) == Some(drag.container_id)
    {
        state.active_containers.remove(&drag.source_shelf);
    }
    if target_group_key != source_group_key
        && state.active_container_for_group(source_group_key) == Some(drag.container_id)
    {
        state.active_containers.remove(&source_group_key);
    }
    if let (Some(target_pane), Some(target_slot)) = (drag.target_pane, drag.target_slot) {
        let defaults: Vec<Id> = shelf_target_cache(ctx, target_pane)
            .iter()
            .map(|entry| entry.id)
            .collect();
        let mut order = pane::section_order_for(ctx, target_pane, &defaults);
        order.retain(|cid| *cid != drag.container_id);
        let clamped = target_slot.min(order.len());
        order.insert(clamped, drag.container_id);
        pane::set_section_order(ctx, target_pane, order);
        pane::clear_drag(ctx, target_pane);
    }
    pane::clear_drag(ctx, drag.source_pane);
    state.clear_container_move();
}

fn should_cancel_no_target_container_release(cursor: Option<Pos2>, shelf_rect: Rect) -> bool {
    cursor.is_some_and(|pos| !shelf_rect.expand(24.0).contains(pos))
}

fn container_slot_ghost_rect_in(
    fallback_rect: Option<Rect>,
    snap: &[pane::RectEntry],
    drag: ShelfContainerMoveState,
    slot: usize,
    horizontal_stack: bool,
) -> Option<Rect> {
    let size = drag.container_size;
    let others: Vec<&pane::RectEntry> = snap
        .iter()
        .filter(|entry| entry.id != drag.container_id)
        .collect();
    if let Some(next) = others.get(slot) {
        let pos = pos2(next.rect.left(), next.rect.top());
        return Some(container_slot_ghost_rect_from_pos(
            fallback_rect,
            pos,
            size,
            horizontal_stack,
        ));
    }
    if let Some(last) = others.last() {
        let pos = if horizontal_stack {
            pos2(last.rect.right(), last.rect.top())
        } else {
            pos2(last.rect.left(), last.rect.bottom())
        };
        return Some(container_slot_ghost_rect_from_pos(
            fallback_rect,
            pos,
            size,
            horizontal_stack,
        ));
    }
    fallback_rect.map(|rect| Rect::from_min_size(rect.min, size.min(rect.size())))
}

fn container_slot_ghost_rect_from_pos(
    fallback_rect: Option<Rect>,
    pos: Pos2,
    size: Vec2,
    horizontal_stack: bool,
) -> Rect {
    let Some(bounds) = fallback_rect else {
        return Rect::from_min_size(pos, size);
    };
    let (clamped_size, min) = if horizontal_stack {
        let clamped_size = vec2(size.x, size.y.min(bounds.height()));
        let min = pos2(
            pos.x,
            pos.y.clamp(
                bounds.top(),
                (bounds.bottom() - clamped_size.y).max(bounds.top()),
            ),
        );
        (clamped_size, min)
    } else {
        let clamped_size = vec2(size.x.min(bounds.width()), size.y);
        let min = pos2(
            pos.x.clamp(
                bounds.left(),
                (bounds.right() - clamped_size.x).max(bounds.left()),
            ),
            pos.y,
        );
        (clamped_size, min)
    };
    Rect::from_min_size(min, clamped_size)
}

fn resize_handle_rect(edge: ShelfEdge, rect: Rect, theme: &ShelfTheme) -> Rect {
    let thickness = theme.resize_handle_thickness;
    match edge {
        ShelfEdge::Left => Rect::from_min_size(
            pos2(rect.max.x - thickness, rect.min.y),
            vec2(thickness, rect.height()),
        ),
        ShelfEdge::Right => Rect::from_min_size(rect.min, vec2(thickness, rect.height())),
        ShelfEdge::Bottom => Rect::from_min_size(rect.min, vec2(rect.width(), thickness)),
    }
}

fn shelf_resize_cursor(edge: ShelfEdge) -> CursorIcon {
    match edge {
        ShelfEdge::Left | ShelfEdge::Right => CursorIcon::ResizeHorizontal,
        ShelfEdge::Bottom => CursorIcon::ResizeVertical,
    }
}

struct ShelfMoveDragInput<'a, 'state> {
    ctx: &'a egui::Context,
    shelf_id: Id,
    shelf_edge: ShelfEdge,
    pane_id: Id,
    state: &'state mut ShelfState,
    layout: ShelfLayout,
    available: Rect,
    shelf_rect: Rect,
    response: &'a crate::mui::MaraResponse,
}

fn handle_shelf_move_drag(input: ShelfMoveDragInput<'_, '_>) {
    let ShelfMoveDragInput {
        ctx,
        shelf_id,
        shelf_edge,
        pane_id,
        state,
        layout,
        available,
        shelf_rect,
        response,
    } = input;
    let _ = shelf_rect;
    if response.drag_started()
        && let Some(cursor) = response.interact_pointer.map(Into::into)
        && !pointer_over_shelf_container(ctx, pane_id, cursor)
        && !pane::pointer_over_container_dots(ctx, pane_id, cursor)
    {
        state.begin_drag(shelf_id, shelf_edge, cursor);
    }

    let dragging_this = state.drag.is_some_and(|drag| drag.shelf_id == shelf_id);
    if dragging_this {
        let input = MaraCtx::input(ctx);
        if let Some(cursor) = input
            .interact_pointer
            .map(Into::into)
            .or_else(|| response.interact_pointer.map(Into::into))
        {
            let occupied = occupied_edges_for_layout(layout, Some(shelf_edge));
            let target = shelf_move_target(cursor, available, occupied, shelf_edge);
            state.update_drag(cursor, target);
            crate::backend::egui::set_cursor_icon_for_context(ctx, CursorIcon::Grabbing);
            crate::backend::egui::request_repaint(ctx);
        }
        if input.any_released {
            state.finish_drag();
        }
    }
    if state.drag.is_some() && crate::backend::egui::key_pressed(ctx, crate::mui::MaraKey::Escape) {
        state.cancel_drag();
    }
}

fn pointer_over_shelf_container(ctx: &dyn crate::context::MaraCtx, pane_id: Id, pos: Pos2) -> bool {
    pane::snapshot(ctx, pane_id)
        .iter()
        .any(|entry| entry.frame.unwrap_or(entry.rect).contains(pos))
}

#[derive(Clone, Copy, Default)]
struct ShelfOccupied {
    left: bool,
    right: bool,
    bottom: bool,
}

impl ShelfOccupied {
    fn has(self, edge: ShelfEdge) -> bool {
        match edge {
            ShelfEdge::Left => self.left,
            ShelfEdge::Right => self.right,
            ShelfEdge::Bottom => self.bottom,
        }
    }
}

fn occupied_edges_for_layout(layout: ShelfLayout, exclude: Option<ShelfEdge>) -> ShelfOccupied {
    ShelfOccupied {
        left: layout.left.is_some() && exclude != Some(ShelfEdge::Left),
        right: layout.right.is_some() && exclude != Some(ShelfEdge::Right),
        bottom: layout.bottom.is_some() && exclude != Some(ShelfEdge::Bottom),
    }
}

fn shelf_move_target(
    cursor: Pos2,
    available: Rect,
    occupied: ShelfOccupied,
    source: ShelfEdge,
) -> Option<ShelfEdge> {
    if !available.contains(cursor) {
        return None;
    }
    let distances = [
        (ShelfEdge::Left, (cursor.x - available.left()).abs()),
        (ShelfEdge::Right, (available.right() - cursor.x).abs()),
        (ShelfEdge::Bottom, (available.bottom() - cursor.y).abs()),
    ];
    let edge_band = (available.width().min(available.height()) * 0.28).max(96.0);
    distances
        .into_iter()
        .filter(|(edge, dist)| *edge != source && !occupied.has(*edge) && *dist <= edge_band)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(edge, _)| edge)
}

#[cfg(test)]
fn container_move_target(cursor: Pos2, available: Rect, source: ShelfEdge) -> Option<ShelfEdge> {
    if !available.contains(cursor) {
        return None;
    }
    let distances = [
        (ShelfEdge::Left, (cursor.x - available.left()).abs()),
        (ShelfEdge::Right, (available.right() - cursor.x).abs()),
        (ShelfEdge::Bottom, (available.bottom() - cursor.y).abs()),
    ];
    let edge_band = (available.width().min(available.height()) * 0.28).max(96.0);
    distances
        .into_iter()
        .filter(|(edge, dist)| *edge != source && *dist <= edge_band)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(edge, _)| edge)
}

fn container_move_target_for_cursor(
    cursor: Pos2,
    screen_shelf_rect: Rect,
    layout: ShelfLayout,
    source: ShelfEdge,
) -> Option<ShelfEdge> {
    if screen_shelf_rect.expand(24.0).contains(cursor) {
        return None;
    }

    for edge in [ShelfEdge::Left, ShelfEdge::Right, ShelfEdge::Bottom] {
        if edge == source {
            continue;
        }
        if let Some(rect) = layout.rect_for(edge)
            && rect.expand(24.0).contains(cursor.into())
        {
            return Some(edge);
        }
    }

    container_move_empty_edge_target(cursor, layout, source)
}

fn container_move_empty_edge_target(
    cursor: Pos2,
    layout: ShelfLayout,
    source: ShelfEdge,
) -> Option<ShelfEdge> {
    let available: Rect = layout.available().into();
    if !available.contains(cursor) {
        return None;
    }
    let distances = [
        (ShelfEdge::Left, (cursor.x - available.left()).abs()),
        (ShelfEdge::Right, (available.right() - cursor.x).abs()),
        (ShelfEdge::Bottom, (available.bottom() - cursor.y).abs()),
    ];
    let edge_band = (available.width().min(available.height()) * 0.12).clamp(48.0, 96.0);
    distances
        .into_iter()
        .filter(|(edge, dist)| {
            *edge != source && layout.rect_for(*edge).is_none() && *dist <= edge_band
        })
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(edge, _)| edge)
}

fn paint_shelf_move_ghost(
    ctx: &egui::Context,
    layout: ShelfLayout,
    state: &ShelfState,
    theme: &ShelfTheme,
) {
    let Some(drag) = state.drag else {
        return;
    };
    let Some(target) = drag.target_edge else {
        return;
    };
    let Some(rect) = shelf_move_drop_rect(layout, drag, state, theme) else {
        return;
    };

    crate::backend::egui::show_area_slot(
        ctx,
        AreaSlotSpec::new(
            AreaHost::new(
                MaraId::new("mara_shelf_move_ghost"),
                rect.min.into(),
                Layer::Foreground,
            )
            .non_interactive(),
            rect.size().into(),
        ),
        |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            let response = crate::layout::UiBackend::allocate(
                &mut backend,
                rect.size().into(),
                crate::layout::Sense::Hover,
            );
            let local: Rect = response.rect.into();
            paint_shelf_reservation_ghost(ui, local, target, style::active_accent().into());
        },
    );
}

fn paint_container_move_ghost(
    ctx: &egui::Context,
    layout: ShelfLayout,
    state: &ShelfState,
    theme: &ShelfTheme,
) {
    let Some(drag) = state.container_move else {
        return;
    };
    let Some(target) = drag.target_edge else {
        return;
    };
    if let Some((rect, accent)) = existing_shelf_container_slot_ghost(ctx, target, drag) {
        crate::backend::egui::show_area_slot(
            ctx,
            AreaSlotSpec::new(
                AreaHost::new(
                    MaraId::new("mara_shelf_existing_container_slot_ghost"),
                    rect.min.into(),
                    Layer::Foreground,
                )
                .non_interactive(),
                rect.size().into(),
            ),
            |ui| {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                let response = crate::layout::UiBackend::allocate(
                    &mut backend,
                    rect.size().into(),
                    crate::layout::Sense::Hover,
                );
                let local: Rect = response.rect.into();
                paint_container_slot_ghost(ui, local, accent);
            },
        );
        return;
    }
    if layout.rect_for(target).is_some() {
        return;
    }
    let Some(shelf_rect) = container_drop_rect_for_drag(ctx, layout, drag, target, theme) else {
        return;
    };
    let accent = style::active_accent();
    crate::backend::egui::show_area_slot(
        ctx,
        AreaSlotSpec::new(
            AreaHost::new(
                MaraId::new("mara_shelf_container_move_ghost"),
                shelf_rect.min.into(),
                Layer::Foreground,
            )
            .non_interactive(),
            shelf_rect.size().into(),
        ),
        |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            let response = crate::layout::UiBackend::allocate(
                &mut backend,
                shelf_rect.size().into(),
                crate::layout::Sense::Hover,
            );
            let shelf_local: Rect = response.rect.into();
            paint_shelf_reservation_ghost(ui, shelf_local, target, accent.into());

            let container_rect =
                new_shelf_container_ghost_rect(ctx, drag.container_id, target, shelf_local);
            paint_container_slot_ghost(ui, container_rect, accent.into());
        },
    );
}

fn paint_container_slot_ghost(ui: &mut egui::Ui, rect: Rect, accent: Color32) {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    for cmd in container_slot_ghost_paint_cmds(
        rect.into(),
        accent.into(),
        MaraCornerRadius::same(style::theme().radius_md),
    ) {
        crate::layout::UiBackend::paint(&mut backend, cmd);
    }
}

fn container_slot_ghost_paint_cmds(
    rect: MaraRect,
    accent: MaraColor32,
    corner: MaraCornerRadius,
) -> [PaintCmd; 2] {
    [
        PaintCmd::RectFilled {
            rect,
            corner,
            fill: MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 72),
        },
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke: MaraStroke::new(1.5, accent),
        },
    ]
}

fn paint_shelf_reservation_ghost(ui: &mut egui::Ui, rect: Rect, edge: ShelfEdge, accent: Color32) {
    let fill = style::fill_for(style::FillRole::DragGhost, accent);
    let stroke = style::stroke_for(style::StrokeRole::DragGhost, accent);
    let stroke = MaraStroke::new(stroke.width, stroke.color);
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    for cmd in shelf_reservation_ghost_paint_cmds(edge, rect.into(), fill, stroke) {
        crate::layout::UiBackend::paint(&mut backend, cmd);
    }
}

fn shelf_center_border_segment_mara(edge: ShelfEdge, rect: MaraRect) -> [MaraPos2; 2] {
    match edge {
        ShelfEdge::Left => [
            MaraPos2::new(rect.right(), rect.top()),
            MaraPos2::new(rect.right(), rect.bottom()),
        ],
        ShelfEdge::Right => [
            MaraPos2::new(rect.left(), rect.top()),
            MaraPos2::new(rect.left(), rect.bottom()),
        ],
        ShelfEdge::Bottom => [
            MaraPos2::new(rect.left(), rect.top()),
            MaraPos2::new(rect.right(), rect.top()),
        ],
    }
}

fn shelf_reservation_ghost_paint_cmds(
    edge: ShelfEdge,
    rect: MaraRect,
    fill: MaraColor32,
    stroke: MaraStroke,
) -> [PaintCmd; 2] {
    let [a, b] = shelf_center_border_segment_mara(edge, rect);
    [
        PaintCmd::RectFilled {
            rect,
            corner: MaraCornerRadius::ZERO,
            fill,
        },
        PaintCmd::Line { a, b, stroke },
    ]
}

fn existing_shelf_container_slot_ghost(
    ctx: &dyn crate::context::MaraCtx,
    target: ShelfEdge,
    drag: ShelfContainerMoveState,
) -> Option<(Rect, Color32)> {
    let target_pane = drag.target_pane?;
    let target_slot = drag.target_slot?;
    if external_container_gap_was_painted(ctx, target_pane) {
        return None;
    }
    let info = shelf_pane_info(ctx, target)?;
    if target_pane != info.pane_id {
        return None;
    }
    let snap = shelf_target_cache(ctx, target_pane);
    container_slot_ghost_rect_in(
        Some(info.content_rect),
        &snap,
        drag,
        target_slot,
        info.horizontal_stack,
    )
    .map(|rect| (rect.translate(info.screen_offset), info.accent))
}

fn shelf_target_cache(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> Vec<pane::RectEntry> {
    pane::target_cache(ctx, pane_id)
}

fn new_shelf_container_ghost_rect(
    ctx: &dyn crate::context::MaraCtx,
    container_id: Id,
    target: ShelfEdge,
    shelf_rect: Rect,
) -> Rect {
    let content_rect = shelf_content_rect(target, shelf_rect, style::theme().shelf());
    let container_size =
        container_move_ghost_size_for_edge(ctx, container_id, target, content_rect)
            .min(content_rect.size());
    Rect::from_min_size(content_rect.min, container_size)
}

fn container_move_ghost_size_for_edge(
    ctx: &dyn crate::context::MaraCtx,
    container_id: Id,
    edge: ShelfEdge,
    content_rect: Rect,
) -> Vec2 {
    let anchor = edge.container_anchor();
    let horizontal_stack = !anchor.title_side().is_horizontal_strip();
    let pane_horizontal_strip = anchor.title_side().is_horizontal_strip();
    let flow = crate::container::container_flow(ctx, container_id, pane_horizontal_strip);
    let title = style::theme().container().title_zone_thickness;
    if horizontal_stack {
        vec2(flow + title, content_rect.height())
    } else {
        vec2(content_rect.width(), flow + title)
    }
}

#[cfg(test)]
fn shelf_drop_rect(
    layout: ShelfLayout,
    source: ShelfEdge,
    target: ShelfEdge,
    theme: &ShelfTheme,
) -> Option<Rect> {
    let occupied = occupied_edges_for_layout(layout, Some(source));
    if occupied.has(target) {
        return None;
    }
    Some(drop_rect_for_occupied_edges(
        layout,
        target,
        drop_extent_for(layout, source, target, theme),
        occupied,
    ))
}

fn shelf_move_drop_rect(
    layout: ShelfLayout,
    drag: state::ShelfDragState,
    state: &ShelfState,
    theme: &ShelfTheme,
) -> Option<Rect> {
    let target = drag.target_edge?;
    let occupied = occupied_edges_for_layout(layout, Some(drag.source_edge));
    if occupied.has(target) {
        return None;
    }
    let extent = shelf_move_drop_extent_for(layout, drag, target, state, theme);
    Some(drop_rect_for_occupied_edges(
        layout, target, extent, occupied,
    ))
}

fn shelf_move_drop_extent_for(
    layout: ShelfLayout,
    drag: state::ShelfDragState,
    target: ShelfEdge,
    state: &ShelfState,
    theme: &ShelfTheme,
) -> f32 {
    if drag.source_edge.is_side() == target.is_side() {
        return source_extent_for(layout, drag.source_edge, theme);
    }
    remembered_axis_extent(state, drag.shelf_id, target).unwrap_or_else(|| {
        if target.is_side() {
            theme.side_default_size
        } else {
            theme.bottom_default_size
        }
    })
}

fn shelf_move_preview_layout(
    layout: ShelfLayout,
    drag: state::ShelfDragState,
    state: &ShelfState,
    theme: &ShelfTheme,
) -> Option<ShelfLayout> {
    let target = drag.target_edge?;
    if target == drag.source_edge {
        return None;
    }
    let target_rect = shelf_move_drop_rect(layout, drag, state, theme)?;
    let mut left = layout.left;
    let mut right = layout.right;
    let mut bottom = layout.bottom;

    match drag.source_edge {
        ShelfEdge::Left => left = None,
        ShelfEdge::Right => right = None,
        ShelfEdge::Bottom => bottom = None,
    }
    match target {
        ShelfEdge::Left => left = Some(target_rect.into()),
        ShelfEdge::Right => right = Some(target_rect.into()),
        ShelfEdge::Bottom => bottom = Some(target_rect.into()),
    }

    Some(layout_from_reserved_shelves(
        layout.available(),
        left,
        right,
        bottom,
    ))
}

#[cfg(test)]
fn container_drop_rect(
    layout: ShelfLayout,
    source: ShelfEdge,
    target: ShelfEdge,
    theme: &ShelfTheme,
) -> Option<Rect> {
    layout.rect_for(target).map(Into::into).or_else(|| {
        let occupied = occupied_edges_for_layout(layout, None);
        if occupied.has(target) {
            return None;
        }
        Some(drop_rect_for_occupied_edges(
            layout,
            target,
            drop_extent_for(layout, source, target, theme),
            occupied,
        ))
    })
}

fn container_drop_rect_for_drag(
    ctx: &dyn crate::context::MaraCtx,
    layout: ShelfLayout,
    drag: ShelfContainerMoveState,
    target: ShelfEdge,
    theme: &ShelfTheme,
) -> Option<Rect> {
    if layout.rect_for(target).is_some() {
        return None;
    }
    let source_remains = source_shelf_has_other_containers(ctx, drag, target);
    let occupied = occupied_edges_for_layout(layout, (!source_remains).then_some(drag.source_edge));
    if occupied.has(target) {
        return None;
    }
    Some(drop_rect_for_occupied_edges(
        layout,
        target,
        drop_extent_for(layout, drag.source_edge, target, theme),
        occupied,
    ))
}

fn container_move_preview_layout(
    ctx: &dyn crate::context::MaraCtx,
    layout: ShelfLayout,
    drag: ShelfContainerMoveState,
    theme: &ShelfTheme,
) -> Option<ShelfLayout> {
    let target = drag.target_edge?;
    if layout.rect_for(target).is_some() {
        return None;
    }
    let target_rect = container_drop_rect_for_drag(ctx, layout, drag, target, theme)?;
    let source_remains = source_shelf_has_other_containers(ctx, drag, target);
    let mut left = layout.left;
    let mut right = layout.right;
    let mut bottom = layout.bottom;

    if !source_remains {
        match drag.source_edge {
            ShelfEdge::Left => left = None,
            ShelfEdge::Right => right = None,
            ShelfEdge::Bottom => bottom = None,
        }
    }
    match target {
        ShelfEdge::Left => left = Some(target_rect.into()),
        ShelfEdge::Right => right = Some(target_rect.into()),
        ShelfEdge::Bottom => bottom = Some(target_rect.into()),
    }

    Some(layout_from_reserved_shelves(
        layout.available(),
        left,
        right,
        bottom,
    ))
}

fn layout_from_reserved_shelves(
    available: MaraRect,
    left: Option<MaraRect>,
    right: Option<MaraRect>,
    bottom: Option<MaraRect>,
) -> ShelfLayout {
    let mut viewport = available;
    let mut resolved_left = None;
    let mut resolved_right = None;
    let mut resolved_bottom = None;

    if let Some(rect) = left {
        let extent = rect.width().min(viewport.width().max(0.0));
        let shelf = MaraRect::from_min_max(
            viewport.min,
            MaraPos2::new(
                (viewport.min.x + extent).min(viewport.max.x),
                viewport.max.y,
            ),
        );
        viewport.min.x = (viewport.min.x + extent).min(viewport.max.x);
        resolved_left = Some(shelf);
    }
    if let Some(rect) = right {
        let extent = rect.width().min(viewport.width().max(0.0));
        let shelf = MaraRect::from_min_max(
            MaraPos2::new(
                (viewport.max.x - extent).max(viewport.min.x),
                viewport.min.y,
            ),
            viewport.max,
        );
        viewport.max.x = (viewport.max.x - extent).max(viewport.min.x);
        resolved_right = Some(shelf);
    }
    if let Some(rect) = bottom {
        let extent = rect.height().min(viewport.height().max(0.0));
        let shelf = MaraRect::from_min_max(
            MaraPos2::new(
                viewport.min.x,
                (viewport.max.y - extent).max(viewport.min.y),
            ),
            viewport.max,
        );
        viewport.max.y = (viewport.max.y - extent).max(viewport.min.y);
        resolved_bottom = Some(shelf);
    }

    ShelfLayout {
        viewport,
        left: resolved_left,
        right: resolved_right,
        bottom: resolved_bottom,
    }
}

fn source_shelf_has_other_containers(
    ctx: &dyn crate::context::MaraCtx,
    drag: ShelfContainerMoveState,
    target: ShelfEdge,
) -> bool {
    let cache = shelf_target_cache(ctx, drag.source_pane);
    if cache.is_empty() {
        // On the first drag frame the source shelf target cache can
        // still be cold. For side -> bottom creation, keeping the
        // source side reserved produces a visibly too-small bottom
        // ghost. Prefer the "last container" preview there; existing
        // side-target previews keep the conservative old behavior.
        return !(drag.source_edge.is_side() && target == ShelfEdge::Bottom);
    }
    cache.iter().any(|entry| entry.id != drag.container_id)
}

fn drop_extent_for(
    layout: ShelfLayout,
    source: ShelfEdge,
    target: ShelfEdge,
    theme: &ShelfTheme,
) -> f32 {
    if source.is_side() == target.is_side() {
        source_extent_for(layout, source, theme)
    } else if target.is_side() {
        theme.side_default_size
    } else {
        theme.bottom_default_size
    }
}

fn source_extent_for(layout: ShelfLayout, source: ShelfEdge, theme: &ShelfTheme) -> f32 {
    match source {
        ShelfEdge::Left => layout
            .left
            .map(|rect| rect.width())
            .unwrap_or(theme.side_default_size),
        ShelfEdge::Right => layout
            .right
            .map(|rect| rect.width())
            .unwrap_or(theme.side_default_size),
        ShelfEdge::Bottom => layout
            .bottom
            .map(|rect| rect.height())
            .unwrap_or(theme.bottom_default_size),
    }
}

fn drop_rect_for_occupied_edges(
    layout: ShelfLayout,
    target: ShelfEdge,
    extent: f32,
    occupied: ShelfOccupied,
) -> Rect {
    let available: Rect = layout.available().into();
    let left_w = if occupied.left {
        layout.left.map_or(0.0, |rect| rect.width())
    } else {
        0.0
    };
    let right_w = if occupied.right {
        layout.right.map_or(0.0, |rect| rect.width())
    } else {
        0.0
    };

    match target {
        ShelfEdge::Left => Rect::from_min_max(
            available.min,
            pos2(
                (available.left() + extent).min(available.right()),
                available.bottom(),
            ),
        ),
        ShelfEdge::Right => Rect::from_min_max(
            pos2(
                (available.right() - extent).max(available.left()),
                available.top(),
            ),
            pos2(available.right(), available.bottom()),
        ),
        ShelfEdge::Bottom => Rect::from_min_max(
            pos2(available.left() + left_w, available.bottom() - extent),
            pos2(available.right() - right_w, available.bottom()),
        ),
    }
}

/// Shelf-reserved insets, useful for ribbon/pane placement code.
#[must_use]
pub fn shelf_insets(layout: ShelfLayout) -> MaraVec2 {
    MaraVec2::new(
        layout.left.map_or(0.0, |r| r.width()) + layout.right.map_or(0.0, |r| r.width()),
        layout.bottom.map_or(0.0, |r| r.height()),
    )
}

#[cfg(test)]
mod tests;
