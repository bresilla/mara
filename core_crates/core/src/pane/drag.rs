//! Drag-reorder for [`super::Pane`] containers.
//!
//! Direct port of `maracore::floating::SectionDragState`. The
//! pattern is:
//!
//! * **The dragged container `return`s early** in `Normal::show` —
//!   it doesn't allocate a layout slot, so the OTHER containers
//!   visibly collapse upward to fill its place.
//! * **An inline ghost gap** is allocated via `allocate_exact_size`
//!   at the cursor's target slot during the iteration — pushing
//!   subsequent containers DOWN to make room. The gap is painted
//!   with an accent rect so the user sees where the drop will land.
//! * **The persistent order is stable** during the drag — only the
//!   gap moves around as the cursor moves. On release, the
//!   dragged id is spliced into the persisted order at the target
//!   slot.
//! * **A floating preview** of the dragged container's last-known
//!   rect renders at the cursor (paint-only, separate Area).

use egui::{Color32, Context, Id, Pos2, Rect, Sense, Ui, Vec2};

use crate::pane::active_pane_key;
use crate::style;

// ─── State ─────────────────────────────────────────────────────────

/// Per-pane drag bookkeeping. `item` latches the dragged
/// container's id; `cursor` is the latest pointer position used to
/// compute the target slot for the ghost gap.
#[derive(Clone, Copy, Debug, Default)]
pub struct DragState {
    pub item: Option<Id>,
    pub cursor: Option<Pos2>,
}

#[derive(Clone, Copy, Debug)]
pub struct RectEntry {
    pub id: Id,
    pub rect: Rect,
    pub frame: Option<Rect>,
}

// ─── ctx-data accessors ────────────────────────────────────────────

fn drag_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_drag")
}
/// Cache of containers RENDERED THIS FRAME, populated by
/// `Normal::show` as it paints. Cleared at body start.
fn current_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_drag_current")
}
/// Snapshot of the PREVIOUS frame's full cache — including the
/// dragged container's last-known rect (carried forward from
/// before the drag started). Read paths (compute_target, ghost gap
/// sizing, preview) consult this so the dragged dimension stays
/// available even though `Normal::show` skips the dragged item.
fn snapshot_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_drag_snapshot")
}
fn ghost_gap_suppressed_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_drag_ghost_gap_suppressed")
}
fn order_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_section_order")
}
pub fn state(ctx: &Context, pane_id: Id) -> DragState {
    ctx.data(|d| d.get_temp(drag_key(pane_id)))
        .unwrap_or_default()
}

pub fn set_drag(ctx: &Context, pane_id: Id, state: DragState) {
    ctx.data_mut(|d| d.insert_temp(drag_key(pane_id), state));
}

pub fn clear_drag(ctx: &Context, pane_id: Id) {
    ctx.data_mut(|d| d.remove::<DragState>(drag_key(pane_id)));
}

/// Clear the per-frame current cache at body start. Snapshot from
/// the prev frame is preserved so reads still see the dragged
/// container's size.
pub fn begin_frame(ctx: &Context, pane_id: Id) {
    ctx.data_mut(|d| {
        d.remove::<Vec<RectEntry>>(current_key(pane_id));
        d.remove::<bool>(ghost_gap_suppressed_key(pane_id));
    });
}

/// Suppress only the inline layout gap for this pane during the
/// current frame. The dragged item is still lifted out of layout and
/// the drag state stays active.
pub(crate) fn set_ghost_gap_suppressed(ctx: &Context, pane_id: Id, suppressed: bool) {
    ctx.data_mut(|d| {
        if suppressed {
            d.insert_temp(ghost_gap_suppressed_key(pane_id), true);
        } else {
            d.remove::<bool>(ghost_gap_suppressed_key(pane_id));
        }
    });
}

pub(crate) fn ghost_gap_suppressed(ctx: &Context, pane_id: Id) -> bool {
    ctx.data(|d| {
        d.get_temp::<bool>(ghost_gap_suppressed_key(pane_id))
            .unwrap_or(false)
    })
}

pub fn push_rect(ctx: &Context, pane_id: Id, id: Id, rect: Rect) {
    push_rect_with_frame(ctx, pane_id, id, rect, None);
}

pub fn push_rect_with_frame(ctx: &Context, pane_id: Id, id: Id, rect: Rect, frame: Option<Rect>) {
    ctx.data_mut(|d| {
        let mut cache: Vec<RectEntry> = d.get_temp(current_key(pane_id)).unwrap_or_default();
        if let Some(slot) = cache.iter_mut().find(|e| e.id == id) {
            slot.rect = rect;
            slot.frame = frame;
        } else {
            cache.push(RectEntry { id, rect, frame });
        }
        d.insert_temp(current_key(pane_id), cache);
    });
}

pub fn current_cache(ctx: &Context, pane_id: Id) -> Vec<RectEntry> {
    ctx.data(|d| d.get_temp(current_key(pane_id)))
        .unwrap_or_default()
}

pub fn snapshot(ctx: &Context, pane_id: Id) -> Vec<RectEntry> {
    ctx.data(|d| d.get_temp(snapshot_key(pane_id)))
        .unwrap_or_default()
}

/// Return the best geometry basis for in-flight targeting.
///
/// During a drag, the current frame is more authoritative than the
/// previous snapshot because it contains containers that were added or
/// reflowed this frame. But the dragged container itself is skipped by
/// rendering, so its old full rect must still be carried forward for
/// preview/ghost sizing. This merges those two facts without mutating
/// the stored snapshot.
pub fn target_cache(ctx: &Context, pane_id: Id) -> Vec<RectEntry> {
    let mut cache = current_cache(ctx, pane_id);
    if cache.is_empty() {
        return snapshot(ctx, pane_id);
    }
    if let Some(dragged_id) = state(ctx, pane_id).item
        && !cache.iter().any(|entry| entry.id == dragged_id)
        && let Some(entry) = snapshot(ctx, pane_id)
            .iter()
            .find(|entry| entry.id == dragged_id)
            .copied()
    {
        cache.push(entry);
    }
    cache
}

pub(crate) fn set_snapshot(ctx: &Context, pane_id: Id, snapshot: Vec<RectEntry>) {
    ctx.data_mut(|d| d.insert_temp(snapshot_key(pane_id), snapshot));
}

/// Build this frame's snapshot from `current_cache` + the dragged
/// container's previous-frame rect (so its size stays available
/// for ghost gap / preview during the drag).
pub fn finalize_snapshot(ctx: &Context, pane_id: Id) {
    let drag = state(ctx, pane_id);
    let mut cache = current_cache(ctx, pane_id);
    if let Some(dragged_id) = drag.item
        && !cache.iter().any(|e| e.id == dragged_id)
    {
        let prev = snapshot(ctx, pane_id);
        if let Some(entry) = prev.iter().find(|e| e.id == dragged_id).copied() {
            cache.push(entry);
        }
    }
    ctx.data_mut(|d| d.insert_temp(snapshot_key(pane_id), cache));
}

// ─── Order persistence ─────────────────────────────────────────────

/// Read the persisted section order for `pane_id`, merged with
/// `defaults`. Stored ids that are still in `defaults` keep their
/// stored order; new ids in `defaults` (= containers added after
/// the last drag) are appended in their default position.
///
/// **Stable during drag**: the order is NOT visually shuffled while
/// a drag is in flight — the dragged container vanishes from
/// layout and a ghost gap travels with the cursor instead. On
/// release, the persistent order is updated.
pub fn section_order_for(ctx: &Context, pane_id: Id, defaults: &[Id]) -> Vec<Id> {
    let stored: Vec<Id> = ctx
        .data_mut(|d| d.get_persisted(order_key(pane_id)))
        .unwrap_or_default();
    let mut order: Vec<Id> = Vec::with_capacity(defaults.len());
    for id in stored {
        if defaults.contains(&id) && !order.contains(&id) {
            order.push(id);
        }
    }
    for id in defaults {
        if !order.contains(id) {
            order.push(*id);
        }
    }
    order
}

/// Persist a new section order for `pane_id`. Survives across
/// runs (`insert_persisted`).
pub fn set_section_order(ctx: &Context, pane_id: Id, order: Vec<Id>) {
    let mut deduped = Vec::with_capacity(order.len());
    for id in order {
        if !deduped.contains(&id) {
            deduped.push(id);
        }
    }
    ctx.data_mut(|d| d.insert_persisted(order_key(pane_id), deduped));
}

// ─── Convenience for Normal ────────────────────────────────────────

/// Look up the **active pane**'s drag state. Used by `Normal` —
/// which doesn't directly know its parent `Pane`'s id — via the
/// `active_pane_key` pointer that `Pane::show` writes at the top
/// of every frame.
pub fn active_drag(ctx: &Context) -> Option<(Id, DragState)> {
    let pane_id: Id = ctx.data(|d| d.get_temp(active_pane_key()))?;
    let s = state(ctx, pane_id);
    Some((pane_id, s))
}

// ─── Geometry ──────────────────────────────────────────────────────

/// Pick the gap-index where the cursor would drop the dragged
/// container. Walks the snapshot in display order, skipping the
/// dragged entry. Auto-detects whether the layout direction is
/// reversed (`bottom_up` / `right_to_left`) by comparing the first
/// two non-dragged entries' main-axis centres — if entry[1]'s
/// centre is BEFORE entry[0]'s on the stack-axis, the layout is
/// reversed and the cursor-vs-centre comparison is flipped.
///
/// Indices are in the non-dragged-only space (0 = before all
/// others in iteration order, N = after all others).
pub fn compute_target(
    cache: &[RectEntry],
    dragged: Id,
    cursor: f32,
    horizontal_stack: bool,
) -> usize {
    let centre = |e: &RectEntry| -> f32 {
        if horizontal_stack {
            e.rect.center().x
        } else {
            e.rect.center().y
        }
    };
    let others: Vec<&RectEntry> = cache.iter().filter(|e| e.id != dragged).collect();
    let reversed = if others.len() >= 2 {
        centre(others[1]) < centre(others[0])
    } else {
        false
    };
    let mut idx = 0;
    for entry in others {
        let c = centre(entry);
        let before = if reversed { cursor > c } else { cursor < c };
        if before {
            return idx;
        }
        idx += 1;
    }
    idx
}

pub fn dragged_size(snapshot: &[RectEntry], dragged: Id) -> Option<Vec2> {
    snapshot
        .iter()
        .find(|e| e.id == dragged)
        .map(|e| e.rect.size())
}

pub fn dragged_entry(snapshot: &[RectEntry], dragged: Id) -> Option<RectEntry> {
    snapshot.iter().find(|e| e.id == dragged).copied()
}

// ─── Paint helpers ─────────────────────────────────────────────────

/// Allocate a same-sized slot inline in the parent layout and paint
/// a translucent accent rect. Pushes subsequent containers along
/// the stack axis exactly like the dragged container would.
pub fn paint_ghost_gap_inline(
    ui: &mut Ui,
    dragged_size: Vec2,
    accent: Color32,
    _horizontal_stack: bool,
) {
    let (rect, _) = ui.allocate_exact_size(dragged_size, Sense::hover());
    let theme = style::theme();
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(theme.radius_md),
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36),
        egui::Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );
}

/// Allocate a same-main-axis slot but keep the ghost's cross-axis
/// position from the dragged entry's previous real rect. This is
/// important for tabbed containers: their full footprint may be
/// inset by the folder-tab strip, so painting at the raw layout
/// cursor makes the ghost appear shifted left/up compared to where
/// the container will land.
pub fn paint_ghost_gap_entry_inline(
    ui: &mut Ui,
    entry: RectEntry,
    accent: Color32,
    horizontal_stack: bool,
) {
    let size = entry.rect.size();
    let (slot_rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let rect = if horizontal_stack {
        Rect::from_min_size(egui::pos2(slot_rect.left(), entry.rect.top()), size)
    } else {
        Rect::from_min_size(egui::pos2(entry.rect.left(), slot_rect.top()), size)
    };
    let theme = style::theme();
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(theme.radius_md),
        Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36),
        egui::Stroke::new(1.5, accent),
        egui::StrokeKind::Inside,
    );
}

/// Paint the dragged container's preview at the cursor on
/// `Order::Tooltip` so it floats above every other UI element.
pub fn paint_drag_preview(
    ctx: &Context,
    pane_id: Id,
    snapshot: &[RectEntry],
    dragged: Id,
    cursor: Pos2,
    accent: Color32,
) {
    let Some(entry) = snapshot.iter().find(|e| e.id == dragged) else {
        return;
    };
    let size = entry.rect.size();
    let pos = egui::pos2(cursor.x - size.x * 0.5, cursor.y - size.y * 0.5);
    let area_id = pane_id.with("mara_pane_drag_preview");
    egui::Area::new(area_id)
        .order(egui::Order::Tooltip)
        .fixed_pos(pos)
        .interactable(false)
        .show(ctx, |ui| {
            let rect = egui::Rect::from_min_size(pos, size);
            let theme = style::theme();
            ui.painter().rect(
                rect,
                egui::CornerRadius::same(theme.radius_md),
                Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 72),
                egui::Stroke::new(1.5, accent),
                egui::StrokeKind::Inside,
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &'static str, rect: Rect) -> RectEntry {
        RectEntry {
            id: Id::new(id),
            rect,
            frame: None,
        }
    }

    #[test]
    fn compute_target_tracks_vertical_slots_in_visual_order() {
        let dragged = Id::new("dragged");
        let cache = [
            entry(
                "first",
                Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(80.0, 40.0)),
            ),
            entry(
                "dragged",
                Rect::from_min_size(egui::pos2(0.0, 50.0), egui::vec2(80.0, 40.0)),
            ),
            entry(
                "second",
                Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(80.0, 40.0)),
            ),
        ];

        assert_eq!(compute_target(&cache, dragged, 10.0, false), 0);
        assert_eq!(compute_target(&cache, dragged, 70.0, false), 1);
        assert_eq!(compute_target(&cache, dragged, 150.0, false), 2);
    }

    #[test]
    fn compute_target_tracks_reversed_vertical_slots_in_visual_order() {
        let dragged = Id::new("dragged");
        let cache = [
            entry(
                "first",
                Rect::from_min_size(egui::pos2(0.0, 100.0), egui::vec2(80.0, 40.0)),
            ),
            entry(
                "dragged",
                Rect::from_min_size(egui::pos2(0.0, 50.0), egui::vec2(80.0, 40.0)),
            ),
            entry(
                "second",
                Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(80.0, 40.0)),
            ),
        ];

        assert_eq!(compute_target(&cache, dragged, 150.0, false), 0);
        assert_eq!(compute_target(&cache, dragged, 70.0, false), 1);
        assert_eq!(compute_target(&cache, dragged, 10.0, false), 2);
    }

    #[test]
    fn compute_target_tracks_horizontal_slots_in_visual_order() {
        let dragged = Id::new("dragged");
        let cache = [
            entry(
                "first",
                Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(40.0, 80.0)),
            ),
            entry(
                "dragged",
                Rect::from_min_size(egui::pos2(50.0, 0.0), egui::vec2(40.0, 80.0)),
            ),
            entry(
                "second",
                Rect::from_min_size(egui::pos2(100.0, 0.0), egui::vec2(40.0, 80.0)),
            ),
        ];

        assert_eq!(compute_target(&cache, dragged, 10.0, true), 0);
        assert_eq!(compute_target(&cache, dragged, 70.0, true), 1);
        assert_eq!(compute_target(&cache, dragged, 150.0, true), 2);
    }

    #[test]
    fn section_order_repairs_duplicate_and_stale_persisted_ids() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let first = Id::new("first");
        let second = Id::new("second");
        let third = Id::new("third");
        let stale = Id::new("stale");

        set_section_order(&ctx, pane_id, vec![second, first, second, stale, first]);

        assert_eq!(
            section_order_for(&ctx, pane_id, &[first, second, third]),
            vec![second, first, third]
        );
    }

    #[test]
    fn finalize_snapshot_carries_dragged_rect_when_render_skips_it() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let dragged = Id::new("dragged");
        let still_rendered = Id::new("still-rendered");
        let dragged_rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(90.0, 60.0));

        set_snapshot(
            &ctx,
            pane_id,
            vec![RectEntry {
                id: dragged,
                rect: dragged_rect,
                frame: Some(dragged_rect),
            }],
        );
        set_drag(
            &ctx,
            pane_id,
            DragState {
                item: Some(dragged),
                cursor: Some(egui::pos2(30.0, 40.0)),
            },
        );
        begin_frame(&ctx, pane_id);
        push_rect(
            &ctx,
            pane_id,
            still_rendered,
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0)),
        );

        finalize_snapshot(&ctx, pane_id);

        let next = snapshot(&ctx, pane_id);
        assert!(
            next.iter()
                .any(|entry| entry.id == dragged && entry.rect == dragged_rect),
            "drag previews and ghost gaps need the dragged container's last full rect even while rendering skips it"
        );
        assert!(next.iter().any(|entry| entry.id == still_rendered));
    }

    #[test]
    fn target_cache_prefers_live_rects_and_carries_dragged_snapshot() {
        let ctx = Context::default();
        let pane_id = Id::new("pane");
        let dragged = Id::new("dragged");
        let live_new = Id::new("live-new");
        let stale = Id::new("stale");
        let dragged_rect = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(90.0, 60.0));

        set_snapshot(
            &ctx,
            pane_id,
            vec![
                RectEntry {
                    id: dragged,
                    rect: dragged_rect,
                    frame: Some(dragged_rect),
                },
                RectEntry {
                    id: stale,
                    rect: Rect::from_min_size(egui::pos2(100.0, 100.0), egui::vec2(20.0, 20.0)),
                    frame: None,
                },
            ],
        );
        set_drag(
            &ctx,
            pane_id,
            DragState {
                item: Some(dragged),
                cursor: Some(egui::pos2(30.0, 40.0)),
            },
        );
        begin_frame(&ctx, pane_id);
        push_rect(
            &ctx,
            pane_id,
            live_new,
            Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(10.0, 10.0)),
        );

        let target = target_cache(&ctx, pane_id);

        assert!(target.iter().any(|entry| entry.id == live_new));
        assert!(
            target
                .iter()
                .any(|entry| entry.id == dragged && entry.rect == dragged_rect)
        );
        assert!(
            !target.iter().any(|entry| entry.id == stale),
            "live targeting must not resurrect removed containers from stale snapshots"
        );
    }
}
