//! Per-pane state for drag-reordering tabs within a tab strip
//! AND transferring tabs between containers in the same pane.
//!
//! Scope is explicitly per-pane — a tab can only land in a strip
//! that lives inside the same pane its source strip does. Cross-
//! pane transfers are not supported (and the data plumbing
//! doesn't expose them: a tab's pod payload only flows through
//! one pane's `PaneBody::render` call per frame).

use std::collections::{HashMap, HashSet};

use egui::{Color32, Context, Id, Pos2, Rect};

use crate::icons::Icon;

// ─── State types ───────────────────────────────────────────────────

#[derive(Clone, Copy, Debug)]
pub struct TabDragState {
    pub tab_id: Id,
    pub source_container: Id,
    pub cursor: Option<Pos2>,
    pub icon: Option<Icon<'static>>,
}

/// One tab button's painted rect, keyed by `(container_id, tab_id)`.
/// Populated by `paint_folder_tabs` / `paint_top_tabs` each frame so
/// `find_drop_target` can resolve the cursor's slot.
#[derive(Clone, Copy, Debug)]
pub struct TabButtonEntry {
    pub container_id: Id,
    pub tab_id: Id,
    pub rect: Rect,
}

/// One container's painted tab strip rect — the hit zone the
/// dragger has to land in for a drop to count. The `axis_horizontal`
/// flag tells `find_drop_target` whether tabs in this strip are
/// laid out left-to-right (`true`) or top-to-bottom (`false`).
#[derive(Clone, Copy, Debug)]
pub struct TabStripEntry {
    pub container_id: Id,
    pub rect: Rect,
    pub axis_horizontal: bool,
}

// ─── ctx-data keys ─────────────────────────────────────────────────

fn drag_key(pane_id: Id) -> Id {
    pane_id.with("mara_tab_drag")
}
fn strip_cache_key(pane_id: Id) -> Id {
    pane_id.with("mara_tab_strip_cache")
}
fn button_cache_key(pane_id: Id) -> Id {
    pane_id.with("mara_tab_button_cache")
}
/// Per-pane: which container each tab id currently belongs to.
fn owner_key(pane_id: Id) -> Id {
    pane_id.with("mara_tab_owner")
}
fn moved_out_key(container_id: Id) -> Id {
    container_id.with("mara_tab_moved_out")
}
/// Per-pane: per-container ordered tab id list.
fn order_key(pane_id: Id) -> Id {
    pane_id.with("mara_tab_order")
}
fn active_tab_key(container_id: Id) -> Id {
    container_id.with("mara_normal_active_tab")
}
fn active_tab_id_key(container_id: Id) -> Id {
    active_tab_key(container_id).with("tab_id")
}

// ─── Drag state accessors ──────────────────────────────────────────

pub fn drag_state(ctx: &Context, pane_id: Id) -> Option<TabDragState> {
    ctx.data(|d| d.get_temp::<TabDragState>(drag_key(pane_id)))
}

pub fn set_drag(ctx: &Context, pane_id: Id, state: TabDragState) {
    ctx.data_mut(|d| d.insert_temp(drag_key(pane_id), state));
}

pub fn clear_drag(ctx: &Context, pane_id: Id) {
    ctx.data_mut(|d| d.remove::<TabDragState>(drag_key(pane_id)));
}

// ─── Strip + button rect cache ─────────────────────────────────────

/// No-op per-frame hook reserved for future cache lifecycle.
/// `push_button` and `push_strip` already replace any stale entry
/// keyed by `(container_id, tab_id)` / `container_id`, so the
/// caches stay coherent across frames without an explicit clear.
pub fn begin_frame(_ctx: &Context, _pane_id: Id) {}

/// Drop every cached button entry for `container_id` before that
/// container's tab strip paints its current-frame buttons. Without
/// this, a tab that moved out of `container_id` last frame would
/// leave a stale entry in the cache and skew `find_drop_target`'s
/// slot computation by one.
pub fn reset_container_buttons(ctx: &Context, pane_id: Id, container_id: Id) {
    ctx.data_mut(|d| {
        let mut cache: Vec<TabButtonEntry> =
            d.get_temp(button_cache_key(pane_id)).unwrap_or_default();
        cache.retain(|e| e.container_id != container_id);
        d.insert_temp(button_cache_key(pane_id), cache);
    });
}

pub fn push_strip(ctx: &Context, pane_id: Id, entry: TabStripEntry) {
    ctx.data_mut(|d| {
        let mut cache: Vec<TabStripEntry> =
            d.get_temp(strip_cache_key(pane_id)).unwrap_or_default();
        cache.retain(|e| e.container_id != entry.container_id);
        cache.push(entry);
        d.insert_temp(strip_cache_key(pane_id), cache);
    });
}

pub fn push_button(ctx: &Context, pane_id: Id, entry: TabButtonEntry) {
    ctx.data_mut(|d| {
        let mut cache: Vec<TabButtonEntry> =
            d.get_temp(button_cache_key(pane_id)).unwrap_or_default();
        cache.retain(|e| !(e.container_id == entry.container_id && e.tab_id == entry.tab_id));
        cache.push(entry);
        d.insert_temp(button_cache_key(pane_id), cache);
    });
}

pub fn strip_cache(ctx: &Context, pane_id: Id) -> Vec<TabStripEntry> {
    ctx.data(|d| d.get_temp(strip_cache_key(pane_id)))
        .unwrap_or_default()
}

pub fn button_cache(ctx: &Context, pane_id: Id) -> Vec<TabButtonEntry> {
    ctx.data(|d| d.get_temp(button_cache_key(pane_id)))
        .unwrap_or_default()
}

pub(crate) fn retain_containers(
    ctx: &Context,
    pane_id: Id,
    containers: impl IntoIterator<Item = Id>,
) {
    let keep: HashSet<Id> = containers.into_iter().collect();
    ctx.data_mut(|d| {
        let mut strips: Vec<TabStripEntry> =
            d.get_temp(strip_cache_key(pane_id)).unwrap_or_default();
        strips.retain(|entry| keep.contains(&entry.container_id));
        d.insert_temp(strip_cache_key(pane_id), strips);

        let mut buttons: Vec<TabButtonEntry> =
            d.get_temp(button_cache_key(pane_id)).unwrap_or_default();
        buttons.retain(|entry| keep.contains(&entry.container_id));
        d.insert_temp(button_cache_key(pane_id), buttons);
    });
}

// ─── Routing persistence ───────────────────────────────────────────

fn read_owner(ctx: &Context, pane_id: Id) -> HashMap<Id, Id> {
    ctx.data_mut(|d| d.get_persisted(owner_key(pane_id)))
        .unwrap_or_default()
}

fn write_owner(ctx: &Context, pane_id: Id, map: HashMap<Id, Id>) {
    ctx.data_mut(|d| d.insert_persisted(owner_key(pane_id), map));
}

fn read_moved_out(ctx: &Context, container_id: Id) -> HashSet<Id> {
    ctx.data_mut(|d| d.get_persisted(moved_out_key(container_id)))
        .unwrap_or_default()
}

fn write_moved_out(ctx: &Context, container_id: Id, tabs: HashSet<Id>) {
    ctx.data_mut(|d| d.insert_persisted(moved_out_key(container_id), tabs));
}

fn mark_moved_out(ctx: &Context, source_container: Id, target_container: Id, tab_id: Id) {
    if source_container != target_container {
        let mut source_moved = read_moved_out(ctx, source_container);
        source_moved.insert(tab_id);
        write_moved_out(ctx, source_container, source_moved);
    }
    let mut target_moved = read_moved_out(ctx, target_container);
    if target_moved.remove(&tab_id) {
        write_moved_out(ctx, target_container, target_moved);
    }
}

fn read_order(ctx: &Context, pane_id: Id) -> HashMap<Id, Vec<Id>> {
    ctx.data_mut(|d| d.get_persisted(order_key(pane_id)))
        .unwrap_or_default()
}

fn dedupe_ids(ids: impl IntoIterator<Item = Id>) -> Vec<Id> {
    let mut out = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

fn write_order(ctx: &Context, pane_id: Id, map: HashMap<Id, Vec<Id>>) {
    let map: HashMap<Id, Vec<Id>> = map
        .into_iter()
        .map(|(container_id, ids)| (container_id, dedupe_ids(ids)))
        .collect();
    ctx.data_mut(|d| d.insert_persisted(order_key(pane_id), map));
}

/// For one container: the ordered tab ids belonging to it, derived
/// from persisted owner map + persisted per-container order, falling
/// back to declared `default_tabs` for any tab whose owner isn't yet
/// persisted. Tabs the persisted owner map assigns AWAY from this
/// container are filtered out; tabs the persisted owner map assigns
/// TO this container from elsewhere are pulled in.
pub fn route(
    ctx: &Context,
    pane_id: Id,
    container_id: Id,
    default_tabs_here: &[Id],
    all_tabs_in_pane: &[(Id, Id)], // (tab_id, declared_container)
) -> Vec<Id> {
    let owner = read_owner(ctx, pane_id);
    let order = read_order(ctx, pane_id);
    let declared_containers: HashSet<Id> = all_tabs_in_pane
        .iter()
        .map(|(_, declared)| *declared)
        .collect();
    let moved_out = read_moved_out(ctx, container_id);

    // Tabs currently owned by this container = persisted owner ==
    // container_id, OR declared in this container AND not persisted
    // elsewhere. Stale owner ids are ignored so removed containers
    // cannot make declared tabs disappear forever.
    let owned: Vec<Id> = all_tabs_in_pane
        .iter()
        .filter_map(|(tid, declared)| {
            if *declared == container_id && moved_out.contains(tid) {
                return None;
            }
            let actual_owner = owner
                .get(tid)
                .copied()
                .filter(|owner_id| declared_containers.contains(owner_id))
                .unwrap_or(*declared);
            (actual_owner == container_id).then_some(*tid)
        })
        .collect();

    // Order: persisted order, filtered to owned + appended with any
    // owned tabs missing from the persisted list (newcomers).
    let persisted = order.get(&container_id).cloned().unwrap_or_default();
    let mut out: Vec<Id> = Vec::with_capacity(owned.len());
    for id in persisted {
        if owned.contains(&id) && !out.contains(&id) {
            out.push(id);
        }
    }
    // Append declared-but-unpersisted tabs in their declared order.
    for tid in default_tabs_here {
        if owned.contains(tid) && !out.contains(tid) {
            out.push(*tid);
        }
    }
    // Append any owned tabs that weren't in the declared list either
    // (= transferred in from another container) in tab-id discovery
    // order.
    for tid in &owned {
        if !out.contains(tid) {
            out.push(*tid);
        }
    }
    out
}

fn live_strip_tab_order(
    ctx: &Context,
    pane_id: Id,
    container_id: Id,
    drag: TabDragState,
) -> Vec<Id> {
    let axis_horizontal = strip_cache(ctx, pane_id)
        .into_iter()
        .find(|strip| strip.container_id == container_id)
        .map(|strip| strip.axis_horizontal)
        .unwrap_or(true);

    let mut buttons: Vec<TabButtonEntry> = button_cache(ctx, pane_id)
        .into_iter()
        .filter(|button| button.container_id == container_id)
        .filter(|button| {
            !(button.container_id == drag.source_container && button.tab_id == drag.tab_id)
        })
        .collect();

    buttons.sort_by(|a, b| {
        let ax = if axis_horizontal {
            a.rect.center().x
        } else {
            a.rect.center().y
        };
        let bx = if axis_horizontal {
            b.rect.center().x
        } else {
            b.rect.center().y
        };
        ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
    });

    buttons.into_iter().map(|button| button.tab_id).collect()
}

/// Commit a drop: move `tab_id` (currently at `source_container`) to
/// `target_container` at slot `target_slot` (0 = first). Updates
/// both the owner map and the per-container order.
pub fn commit_drop(
    ctx: &Context,
    pane_id: Id,
    tab_id: Id,
    source_container: Id,
    target_container: Id,
    target_slot: usize,
) {
    let drag = TabDragState {
        tab_id,
        source_container,
        cursor: None,
        icon: None,
    };
    let live_target_order = live_strip_tab_order(ctx, pane_id, target_container, drag);

    let mut owner = read_owner(ctx, pane_id);
    owner.insert(tab_id, target_container);
    write_owner(ctx, pane_id, owner);
    mark_moved_out(ctx, source_container, target_container, tab_id);

    let mut order = read_order(ctx, pane_id);
    // Remove from source.
    if let Some(src) = order.get_mut(&source_container) {
        src.retain(|id| *id != tab_id);
    }
    // Insert into target at the slot computed from the live rendered strip.
    // If this container has never had persisted order before, seeding from
    // the button cache preserves declared/visual tab order instead of treating
    // the target as empty and forcing the moved tab to the front.
    let mut target_order = if live_target_order.is_empty() {
        order.remove(&target_container).unwrap_or_default()
    } else {
        let mut seeded = live_target_order;
        if let Some(persisted) = order.remove(&target_container) {
            for id in persisted {
                if id != tab_id && !seeded.contains(&id) {
                    seeded.push(id);
                }
            }
        }
        seeded
    };
    target_order.retain(|id| *id != tab_id);
    let slot = target_slot.min(target_order.len());
    target_order.insert(slot, tab_id);
    order.insert(target_container, target_order);
    write_order(ctx, pane_id, order);
    ctx.data_mut(|d| d.insert_persisted(active_tab_key(target_container), slot));
    ctx.data_mut(|d| d.insert_persisted(active_tab_id_key(target_container), tab_id));
}

// ─── Drop target detection ─────────────────────────────────────────

/// Given the cursor, locate the (container, insertion-slot) that
/// would receive the drop. Returns `None` if the cursor isn't over
/// any registered tab strip in this pane.
#[cfg(test)]
fn find_drop_target(ctx: &Context, pane_id: Id, cursor: Pos2) -> Option<(Id, usize)> {
    find_drop_target_filtered(ctx, pane_id, cursor, None)
}

pub fn find_drop_target_for_drag(
    ctx: &Context,
    pane_id: Id,
    cursor: Pos2,
    drag: TabDragState,
) -> Option<(Id, usize)> {
    find_drop_target_filtered(ctx, pane_id, cursor, Some(drag))
}

fn find_drop_target_filtered(
    ctx: &Context,
    pane_id: Id,
    cursor: Pos2,
    drag: Option<TabDragState>,
) -> Option<(Id, usize)> {
    let strips = strip_cache(ctx, pane_id);
    let target_strip = strips.iter().find(|s| s.rect.contains(cursor)).copied()?;
    let buttons = button_cache(ctx, pane_id);
    let mut tabs_in_strip: Vec<TabButtonEntry> = buttons
        .into_iter()
        .filter(|b| {
            b.container_id == target_strip.container_id
                && !drag.is_some_and(|drag| {
                    b.container_id == drag.source_container && b.tab_id == drag.tab_id
                })
        })
        .collect();
    // Sort by axis position so slot indexing is stable.
    tabs_in_strip.sort_by(|a, b| {
        let ax = if target_strip.axis_horizontal {
            a.rect.center().x
        } else {
            a.rect.center().y
        };
        let bx = if target_strip.axis_horizontal {
            b.rect.center().x
        } else {
            b.rect.center().y
        };
        ax.partial_cmp(&bx).unwrap_or(std::cmp::Ordering::Equal)
    });
    let cursor_axis = if target_strip.axis_horizontal {
        cursor.x
    } else {
        cursor.y
    };
    let mut slot = 0usize;
    for entry in &tabs_in_strip {
        let c = if target_strip.axis_horizontal {
            entry.rect.center().x
        } else {
            entry.rect.center().y
        };
        if cursor_axis < c {
            return Some((target_strip.container_id, slot));
        }
        slot += 1;
    }
    Some((target_strip.container_id, slot))
}

// ─── Paint helpers ─────────────────────────────────────────────────

/// Paint the dragged tab's preview at the cursor on
/// `Order::Tooltip` — floats above every pane / container layer.
pub fn paint_drag_preview(
    ctx: &Context,
    pane_id: Id,
    button_size: egui::Vec2,
    cursor: Pos2,
    accent: Color32,
    label: &str,
    icon: Option<Icon<'static>>,
) {
    let pos = egui::pos2(
        cursor.x - button_size.x * 0.5,
        cursor.y - button_size.y * 0.5,
    );
    let area_id = pane_id.with("mara_tab_drag_preview");
    egui::Area::new(area_id)
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .interactable(false)
        .show(ctx, |ui| {
            let rect = Rect::from_min_size(pos, button_size);
            let theme = crate::style::theme();
            ui.painter().rect(
                rect,
                egui::CornerRadius::same(theme.radius_compact),
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 72),
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Inside,
            );
            // Glyph + label, centred. Best-effort; icon may be empty.
            if let Some(icon) = icon {
                let icon_size = button_size.y * 0.55;
                crate::icons::paint_section_icon(
                    ui,
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    icon,
                    icon_size,
                    crate::style::on_panel(),
                );
            }
            let _ = label;
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, pos2, vec2};

    #[test]
    fn drag_target_ignores_source_tab_button_for_same_strip_slots() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container_id = Id::new("container");
        let first = Id::new("first");
        let dragged = Id::new("dragged");
        let third = Id::new("third");
        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [first, dragged, third].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        assert_eq!(
            find_drop_target(&ctx, pane_id, pos2(190.0, 16.0)),
            Some((container_id, 2)),
            "the raw cache still contains the dragged tab from the previous frame"
        );
        assert_eq!(
            find_drop_target_for_drag(
                &ctx,
                pane_id,
                pos2(190.0, 16.0),
                TabDragState {
                    tab_id: dragged,
                    source_container: container_id,
                    cursor: Some(pos2(190.0, 16.0)),
                    icon: None,
                },
            ),
            Some((container_id, 1)),
            "same-strip target slots must be computed as if the dragged tab has already been lifted"
        );
    }

    #[test]
    fn drag_target_ignores_source_tab_button_for_vertical_same_strip_slots() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container_id = Id::new("container");
        let first = Id::new("first");
        let dragged = Id::new("dragged");
        let third = Id::new("third");
        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 320.0)),
                axis_horizontal: false,
            },
        );
        for (idx, tab_id) in [first, dragged, third].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id,
                    tab_id,
                    rect: Rect::from_min_size(pos2(0.0, (idx as f32) * 100.0), vec2(40.0, 80.0)),
                },
            );
        }

        assert_eq!(
            find_drop_target_for_drag(
                &ctx,
                pane_id,
                pos2(20.0, 190.0),
                TabDragState {
                    tab_id: dragged,
                    source_container: container_id,
                    cursor: Some(pos2(20.0, 190.0)),
                    icon: None,
                },
            ),
            Some((container_id, 1)),
            "side-tab strips must also compute slots as if the dragged tab has already been lifted"
        );
    }

    #[test]
    fn drag_target_keeps_foreign_strip_buttons() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let source_container = Id::new("source");
        let target_container = Id::new("target");
        let dragged = Id::new("dragged");
        let target_first = Id::new("target-first");
        let target_second = Id::new("target-second");
        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: target_container,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [target_first, target_second].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: target_container,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        assert_eq!(
            find_drop_target_for_drag(
                &ctx,
                pane_id,
                pos2(190.0, 16.0),
                TabDragState {
                    tab_id: dragged,
                    source_container,
                    cursor: Some(pos2(190.0, 16.0)),
                    icon: None,
                },
            ),
            Some((target_container, 2)),
            "only the lifted source tab should be filtered; target-strip tabs still define slots"
        );
    }

    #[test]
    fn retain_containers_prunes_stale_tab_drop_targets() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let current = Id::new("current");
        let stale = Id::new("removed");
        for (container_id, x) in [(current, 0.0), (stale, 200.0)] {
            push_strip(
                &ctx,
                pane_id,
                TabStripEntry {
                    container_id,
                    rect: Rect::from_min_size(pos2(x, 0.0), vec2(120.0, 32.0)),
                    axis_horizontal: true,
                },
            );
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id,
                    tab_id: container_id.with("tab"),
                    rect: Rect::from_min_size(pos2(x, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        retain_containers(&ctx, pane_id, [current]);

        assert_eq!(strip_cache(&ctx, pane_id).len(), 1);
        assert_eq!(button_cache(&ctx, pane_id).len(), 1);
        assert_eq!(
            find_drop_target(&ctx, pane_id, pos2(240.0, 16.0)),
            None,
            "removed containers must not remain invisible tab drop targets"
        );
        assert_eq!(
            find_drop_target(&ctx, pane_id, pos2(40.0, 16.0)),
            Some((current, 1))
        );
    }

    #[test]
    fn commit_drop_seeds_empty_target_order_from_live_tabs() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let source = Id::new("source");
        let target = Id::new("target");
        let dragged = Id::new("dragged");
        let target_first = Id::new("target-first");
        let target_second = Id::new("target-second");

        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: target,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [target_first, target_second].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: target,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        commit_drop(&ctx, pane_id, dragged, source, target, 1);

        assert_eq!(
            route(
                &ctx,
                pane_id,
                target,
                &[target_first, target_second],
                &[
                    (dragged, source),
                    (target_first, target),
                    (target_second, target),
                ],
            ),
            vec![target_first, dragged, target_second],
            "dropping into an unpersisted target container should preserve the live target tabs around the insertion slot"
        );
    }

    #[test]
    fn route_repairs_duplicate_and_stale_persisted_tab_order() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container = Id::new("container");
        let first = Id::new("first");
        let second = Id::new("second");
        let third = Id::new("third");
        let stale = Id::new("stale");

        let mut order = HashMap::new();
        order.insert(container, vec![second, first, second, stale, first]);
        write_order(&ctx, pane_id, order);

        assert_eq!(
            route(
                &ctx,
                pane_id,
                container,
                &[first, second, third],
                &[(first, container), (second, container), (third, container)],
            ),
            vec![second, first, third],
            "persisted tab order must not duplicate buttons or resurrect stale tabs after drag/drop"
        );
    }

    #[test]
    fn route_ignores_stale_persisted_owner_containers() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container = Id::new("container");
        let removed_container = Id::new("removed-container");
        let tab = Id::new("tab");

        let mut owner = HashMap::new();
        owner.insert(tab, removed_container);
        write_owner(&ctx, pane_id, owner);

        assert_eq!(
            route(&ctx, pane_id, container, &[tab], &[(tab, container)]),
            vec![tab],
            "a stale persisted owner must fall back to the tab's current declared container instead of hiding the tab"
        );
    }

    #[test]
    fn moved_out_tabs_do_not_reappear_when_source_container_changes_pane() {
        let ctx = Context::default();
        let first_pane = Id::new("first-pane");
        let second_pane = Id::new("second-pane");
        let source = Id::new("source-container");
        let target = Id::new("target-container");
        let moved_tab = Id::new("moved-tab");
        let remaining_tab = Id::new("remaining-tab");

        commit_drop(&ctx, first_pane, moved_tab, source, target, 0);

        assert_eq!(
            route(
                &ctx,
                second_pane,
                source,
                &[moved_tab, remaining_tab],
                &[(moved_tab, source), (remaining_tab, source)],
            ),
            vec![remaining_tab],
            "when a container moves to another shelf/pane, tabs previously moved out of it must not come back from its declared defaults"
        );
    }

    #[test]
    fn commit_drop_uses_vertical_live_target_order() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let source = Id::new("source");
        let target = Id::new("target");
        let dragged = Id::new("dragged");
        let top = Id::new("top");
        let bottom = Id::new("bottom");

        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: target,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(40.0, 260.0)),
                axis_horizontal: false,
            },
        );
        for (idx, tab_id) in [top, bottom].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: target,
                    tab_id,
                    rect: Rect::from_min_size(pos2(0.0, (idx as f32) * 80.0), vec2(40.0, 40.0)),
                },
            );
        }

        commit_drop(&ctx, pane_id, dragged, source, target, 1);

        assert_eq!(
            route(
                &ctx,
                pane_id,
                target,
                &[top, bottom],
                &[(dragged, source), (top, target), (bottom, target)],
            ),
            vec![top, dragged, bottom],
            "side-tab shelves use vertical strip order when seeding the target drop order"
        );
    }

    #[test]
    fn commit_drop_same_container_reorders_from_live_tabs_without_duplication() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container = Id::new("container");
        let first = Id::new("first");
        let dragged = Id::new("dragged");
        let last = Id::new("last");

        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: container,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [first, dragged, last].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: container,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        commit_drop(&ctx, pane_id, dragged, container, container, 2);

        assert_eq!(
            route(
                &ctx,
                pane_id,
                container,
                &[first, dragged, last],
                &[(first, container), (dragged, container), (last, container)],
            ),
            vec![first, last, dragged],
            "same-strip commits should use the live order with the dragged tab lifted, then insert it once at the target slot"
        );
    }

    #[test]
    fn commit_drop_selects_dropped_tab_in_target_container() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let source = Id::new("source");
        let target = Id::new("target");
        let dragged = Id::new("dragged");
        let target_first = Id::new("target-first");
        let target_second = Id::new("target-second");

        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: target,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [target_first, target_second].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: target,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        commit_drop(&ctx, pane_id, dragged, source, target, 1);

        let active = ctx.data_mut(|d| d.get_persisted::<usize>(active_tab_key(target)));
        let active_id = ctx.data_mut(|d| d.get_persisted::<Id>(active_tab_id_key(target)));
        assert_eq!(
            active,
            Some(1),
            "after dropping a tab, the receiving container should select the moved tab instead of leaving a different tab visible"
        );
        assert_eq!(
            active_id,
            Some(dragged),
            "selection is persisted by tab id as well as index so later reorder/stale-index cleanup keeps the dropped tab selected"
        );
    }

    #[test]
    fn commit_drop_same_container_selects_reordered_tab() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let container = Id::new("container");
        let first = Id::new("first");
        let dragged = Id::new("dragged");
        let last = Id::new("last");

        push_strip(
            &ctx,
            pane_id,
            TabStripEntry {
                container_id: container,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(360.0, 32.0)),
                axis_horizontal: true,
            },
        );
        for (idx, tab_id) in [first, dragged, last].into_iter().enumerate() {
            push_button(
                &ctx,
                pane_id,
                TabButtonEntry {
                    container_id: container,
                    tab_id,
                    rect: Rect::from_min_size(pos2((idx as f32) * 100.0, 0.0), vec2(80.0, 32.0)),
                },
            );
        }

        commit_drop(&ctx, pane_id, dragged, container, container, 2);

        let active = ctx.data_mut(|d| d.get_persisted::<usize>(active_tab_key(container)));
        let active_id = ctx.data_mut(|d| d.get_persisted::<Id>(active_tab_id_key(container)));
        assert_eq!(
            active,
            Some(2),
            "same-container tab reorder should keep the dragged tab selected at its new slot"
        );
        assert_eq!(active_id, Some(dragged));
    }
}
