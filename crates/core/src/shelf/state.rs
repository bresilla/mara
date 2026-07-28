use std::collections::{HashMap, HashSet};

use crate::vocab::{Color32 as MaraColor32, Id};
use crate::vocab::{Pos2, Rect, Vec2};

use super::{ShelfEdge, sanitize_extent, shelf_active_container_key_for};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShelfDragState {
    pub(super) shelf_id: Id,
    pub(super) source_edge: ShelfEdge,
    pub(super) cursor: Pos2,
    pub(super) target_edge: Option<ShelfEdge>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShelfResizeStart {
    pub(super) size: f32,
    pub(super) pointer: Pos2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShelfContainerMoveState {
    pub(super) container_id: Id,
    pub(super) source_shelf: Id,
    pub(super) source_pane: Id,
    pub(super) source_edge: ShelfEdge,
    pub(super) cursor: Pos2,
    pub(super) target_edge: Option<ShelfEdge>,
    pub(super) target_shelf: Option<Id>,
    pub(super) target_pane: Option<Id>,
    pub(super) target_slot: Option<usize>,
    pub(super) container_size: Vec2,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ShelfContainerMoveUpdate {
    pub(super) container_id: Id,
    pub(super) source_shelf: Id,
    pub(super) source_pane: Id,
    pub(super) source_edge: ShelfEdge,
    pub(super) cursor: Pos2,
    pub(super) target_edge: Option<ShelfEdge>,
    pub(super) container_size: Vec2,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ShelfPaneInfo {
    pub(super) shelf_id: Id,
    pub(super) pane_id: Id,
    pub(super) edge: ShelfEdge,
    pub(super) horizontal_stack: bool,
    pub(super) content_rect: Rect,
    pub(super) screen_rect: Rect,
    pub(super) screen_offset: Vec2,
    pub(super) accent: MaraColor32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ShelfContainerLocation {
    pub(super) shelf_id: Option<Id>,
    pub(super) edge: ShelfEdge,
}

pub(super) fn detached_shelf_id(source_shelf: Id, container_id: Id) -> Id {
    source_shelf.with("mara_detached_shelf").with(container_id)
}

/// Persistent Shelf UI state: user sizes and per-Shelf active group.
#[derive(Debug, Default)]
pub struct ShelfState {
    pub(super) sizes: HashMap<Id, f32>,
    pub(super) resize_starts: HashMap<Id, ShelfResizeStart>,
    pub(super) edge_overrides: HashMap<Id, ShelfEdge>,
    pub(super) container_locations: HashMap<Id, ShelfContainerLocation>,
    pub(super) active_containers: HashMap<Id, Id>,
    pub(super) hidden_edges: HashSet<ShelfEdge>,
    pub(super) drag: Option<ShelfDragState>,
    pub(super) container_move: Option<ShelfContainerMoveState>,
}

impl ShelfState {
    fn size(&self, key: Id) -> Option<f32> {
        self.sizes.get(&key).copied()
    }

    fn set_size(&mut self, key: Id, size: f32) {
        if size.is_finite() {
            self.sizes.insert(key, size.max(0.0));
        } else {
            self.sizes.remove(&key);
        }
    }

    /// Read the user's persisted size for a shelf on a concrete edge.
    ///
    /// Shelf sizes are intentionally edge-scoped: a shelf moved from
    /// left to bottom needs a different dimension axis, and moving it
    /// back should restore the side width instead of reusing the
    /// bottom height.
    #[must_use]
    pub fn edge_size(&self, shelf_id: Id, edge: ShelfEdge) -> Option<f32> {
        self.size(shelf_id.with(edge))
    }

    /// Persist a user size for a shelf on a concrete edge.
    pub fn set_edge_size(&mut self, shelf_id: Id, edge: ShelfEdge, size: f32) {
        self.set_size(shelf_id.with(edge), size);
    }

    #[must_use]
    pub fn edge(&self, shelf_id: Id, default: ShelfEdge) -> ShelfEdge {
        self.edge_overrides
            .get(&shelf_id)
            .copied()
            .unwrap_or(default)
    }

    pub fn set_edge(&mut self, shelf_id: Id, edge: ShelfEdge) {
        self.edge_overrides.insert(shelf_id, edge);
    }

    pub fn clear_edge_override(&mut self, shelf_id: Id) {
        self.edge_overrides.remove(&shelf_id);
    }

    #[must_use]
    pub fn container_edge(&self, container_id: Id, default: ShelfEdge) -> ShelfEdge {
        self.container_locations
            .get(&container_id)
            .map(|location| location.edge)
            .unwrap_or(default)
    }

    pub fn set_container_edge(&mut self, container_id: Id, edge: ShelfEdge) {
        self.container_locations.insert(
            container_id,
            ShelfContainerLocation {
                shelf_id: None,
                edge,
            },
        );
    }

    pub fn clear_container_edge_override(&mut self, container_id: Id) {
        self.container_locations.remove(&container_id);
    }

    #[must_use]
    pub fn edge_visible(&self, edge: ShelfEdge) -> bool {
        !self.hidden_edges.contains(&edge)
    }

    pub fn set_edge_visible(&mut self, edge: ShelfEdge, visible: bool) {
        if visible {
            self.hidden_edges.remove(&edge);
        } else {
            self.hidden_edges.insert(edge);
        }
    }

    pub fn toggle_edge_visible(&mut self, edge: ShelfEdge) {
        let visible = !self.edge_visible(edge);
        self.set_edge_visible(edge, visible);
    }

    pub(super) fn container_location(
        &self,
        container_id: Id,
        default_edge: ShelfEdge,
    ) -> ShelfContainerLocation {
        self.container_locations
            .get(&container_id)
            .copied()
            .unwrap_or(ShelfContainerLocation {
                shelf_id: None,
                edge: default_edge,
            })
    }

    pub(super) fn set_container_location(
        &mut self,
        container_id: Id,
        shelf_id: Option<Id>,
        edge: ShelfEdge,
    ) {
        self.container_locations
            .insert(container_id, ShelfContainerLocation { shelf_id, edge });
    }

    #[must_use]
    pub fn active_container(&self, shelf_id: Id) -> Option<Id> {
        self.active_containers.get(&shelf_id).copied()
    }

    pub fn set_active_container(&mut self, shelf_id: Id, container_id: Id) {
        self.active_containers.insert(shelf_id, container_id);
    }

    pub(super) fn clear_active_container(&mut self, shelf_id: Id) {
        self.active_containers.remove(&shelf_id);
    }

    #[doc(hidden)]
    pub fn active_container_for_group(&self, group_id: Id) -> Option<Id> {
        self.active_containers.get(&group_id).copied()
    }

    #[doc(hidden)]
    pub fn set_active_container_for_group(&mut self, group_id: Id, container_id: Id) {
        self.active_containers.insert(group_id, container_id);
    }

    pub(super) fn clear_active_container_for_group(&mut self, group_id: Id) {
        self.active_containers.remove(&group_id);
    }

    pub(super) fn extent_for_key(&mut self, size_key: Id, default: f32, bounds: (f32, f32)) -> f32 {
        let (min, max) = bounds;
        let default = sanitize_extent(default, min).clamp(min, max);
        let value = self.sizes.entry(size_key).or_insert(default);
        *value = sanitize_extent(*value, default).clamp(min, max);
        *value
    }

    pub(super) fn begin_drag(&mut self, shelf_id: Id, source_edge: ShelfEdge, cursor: Pos2) {
        self.drag = Some(ShelfDragState {
            shelf_id,
            source_edge,
            cursor,
            target_edge: None,
        });
    }

    pub(super) fn update_drag(&mut self, cursor: Pos2, target_edge: Option<ShelfEdge>) {
        if let Some(drag) = &mut self.drag {
            drag.cursor = cursor;
            drag.target_edge = target_edge;
        }
    }

    pub(super) fn finish_drag(&mut self) {
        if let Some(drag) = self.drag.take()
            && let Some(target) = drag
                .target_edge
                .filter(|target| *target != drag.source_edge)
        {
            let source_group_key = shelf_active_container_key_for(drag.shelf_id, drag.source_edge);
            let target_group_key = shelf_active_container_key_for(drag.shelf_id, target);
            if let Some(active) = self.active_containers.remove(&source_group_key) {
                self.active_containers.insert(target_group_key, active);
            }
            if let Some(size) = self
                .sizes
                .get(&drag.shelf_id.with(drag.source_edge))
                .copied()
                .filter(|_| drag.source_edge.is_side() == target.is_side())
            {
                self.sizes.insert(drag.shelf_id.with(target), size);
            }
            self.resize_starts
                .remove(&drag.shelf_id.with(drag.source_edge));
            self.resize_starts.remove(&drag.shelf_id.with(target));
            for location in self.container_locations.values_mut().filter(|location| {
                location.shelf_id == Some(drag.shelf_id) && location.edge == drag.source_edge
            }) {
                location.edge = target;
            }
            self.set_edge(drag.shelf_id, target);
        }
    }

    pub(super) fn cancel_drag(&mut self) {
        self.drag = None;
    }

    pub(super) fn update_container_move(&mut self, update: ShelfContainerMoveUpdate) {
        let previous_slot = self
            .container_move
            .filter(|drag| {
                drag.container_id == update.container_id && drag.target_edge == update.target_edge
            })
            .and_then(|drag| {
                drag.target_pane
                    .zip(drag.target_slot)
                    .zip(drag.target_shelf)
            })
            .map(|((pane, slot), shelf)| (pane, slot, shelf));
        self.container_move = Some(ShelfContainerMoveState {
            container_id: update.container_id,
            source_shelf: update.source_shelf,
            source_pane: update.source_pane,
            source_edge: update.source_edge,
            cursor: update.cursor,
            target_edge: update.target_edge,
            target_shelf: previous_slot.map(|(_, _, shelf)| shelf),
            target_pane: previous_slot.map(|(pane, _, _)| pane),
            target_slot: previous_slot.map(|(_, slot, _)| slot),
            container_size: update.container_size,
        });
    }

    pub(super) fn update_container_move_target_slot(
        &mut self,
        target_shelf: Id,
        target_pane: Id,
        target_slot: usize,
        target_size: Vec2,
    ) {
        if let Some(drag) = &mut self.container_move {
            drag.target_shelf = Some(target_shelf);
            drag.target_pane = Some(target_pane);
            drag.target_slot = Some(target_slot);
            drag.container_size = target_size;
        }
    }

    pub(super) fn clear_container_move_target_slot(&mut self) {
        if let Some(drag) = &mut self.container_move {
            drag.target_shelf = None;
            drag.target_pane = None;
            drag.target_slot = None;
        }
    }

    pub(super) fn clear_container_move(&mut self) {
        self.container_move = None;
    }
}
