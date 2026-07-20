//! # Custom debug inspector — *much* less noisy than egui's
//! built-in `debug_on_hover`
//!
//! egui's stock inspector dumps the full `Ui` chain on hover, which
//! includes every `container_pointer`, every internal panel /
//! horizontal / vertical / clip helper that egui spawns. Useful but
//! drowning in noise for our purposes.
//!
//! This module exposes a minimal *opt-in* alternative: we tag only
//! the rects WE care about (panes, containers, pods, … future things)
//! with WE-write labels, and the inspector paints just one outline +
//! one label at a time.
//!
//! ## Lifecycle
//!
//! 1. The host app toggles the inspector globally:
//!    ```ignore
//!    mara_core::debug::set_enabled(ctx, !mara_core::debug::is_enabled(ctx));
//!    ```
//! 2. Every interesting widget calls [`tag`] with its rect and a
//!    descriptive label, e.g.
//!    ```ignore
//!    mara_core::debug::tag(ui, frame_rect, "Pane[settings]");
//!    mara_core::debug::tag(ui, body_rect, "  Container[Settings 2] > body");
//!    mara_core::debug::tag(ui, pod_rect,  "    SearchPod[…/newui_search_pod]");
//!    ```
//!    Calls are CHEAP when the inspector is off (early return), and
//!    when on, only one survives per frame — the smallest rect that
//!    contains the cursor. Parent tags lose to child tags
//!    automatically.
//! 3. The host app calls [`paint`] once per frame, AFTER all UI has
//!    rendered, to draw the winner. Best place is at the bottom of
//!    your top-level `egui_ctx` callback.
//!
//! ## Display
//!
//! When a tag wins this frame, [`paint`] outlines its rect with a
//! red 2-px stroke and prints the label in the rect's top-left
//! corner. Painting goes through `ctx.debug_painter()` so the
//! overlay sits ABOVE every clip rect — no clipping, no z-fighting.

#![allow(dead_code)]

use egui::{Color32, FontId, Id, Rect, Stroke, StrokeKind, Ui};

const ENABLED_KEY: &str = "mara_debug_inspector_enabled";
const BEST_KEY: &str = "mara_debug_inspector_best";

#[derive(Clone)]
struct Best {
    rect: Rect,
    label: String,
}

impl Default for Best {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            label: String::new(),
        }
    }
}

fn enabled_id() -> Id {
    Id::new(ENABLED_KEY)
}

fn best_id() -> Id {
    Id::new(BEST_KEY)
}

/// `true` when the inspector is on. Cheap — single ctx-data read.
pub fn is_enabled(ctx: &egui::Context) -> bool {
    {
        let memory = crate::memory::MaraMemoryCtx::new(ctx);
        memory.get_temp::<bool>(enabled_id()).unwrap_or(false)
    }
}

/// Toggle the inspector overlay globally for this `ctx`.
pub fn set_enabled(ctx: &egui::Context, on: bool) {
    crate::memory::MaraMemoryCtx::new(ctx).set_temp(enabled_id(), on);
}

/// Register a hover-triggered debug entry. When the inspector is on
/// and the pointer sits inside `rect`, this tag becomes the
/// candidate "best" entry of the frame. If multiple tags overlap
/// the cursor, the one whose rect is CONTAINED by the others (=
/// the deepest, smallest one) wins, mirroring egui's own
/// register-rect logic.
///
/// Cheap when the inspector is off — single ctx-data read for the
/// enabled flag.
pub fn tag(ui: &Ui, rect: Rect, label: impl Into<String>) {
    if !is_enabled(ui.ctx()) {
        return;
    }
    let Some(pointer) = ui.ctx().pointer_hover_pos() else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let label = label.into();
    ui.ctx().data_mut(|d| {
        let prev: Option<Best> = d.get_temp::<Best>(best_id());
        let take = match prev {
            None => true,
            // Always take the SMALLER rect (compared by area). Both
            // candidates already contain the pointer (checked
            // above), so smaller-area = deeper / more specific.
            // Using area instead of `contains_rect` is robust to
            // sibling rects, off-by-pixel mismatches between a
            // container's painted edge and its inner pod's padded
            // edge, etc.
            Some(p) => rect.area() < p.rect.area(),
        };
        if take {
            d.insert_temp(best_id(), Best { rect, label });
        }
    });
}

/// Paint the deepest tag from this frame and clear the slot. Call
/// once at the END of the top-level UI callback. No-op when the
/// inspector is off, or when no tag captured the cursor this frame.
pub fn paint(ctx: &egui::Context) {
    if !is_enabled(ctx) {
        return;
    }
    let mut memory = crate::memory::MaraMemoryCtx::new(ctx);
    let best: Option<Best> = memory.get_temp::<Best>(best_id());
    memory.remove_temp::<Best>(best_id());
    let Some(best) = best else {
        return;
    };
    let p = ctx.debug_painter();
    let outline = Color32::from_rgb(255, 80, 80);
    p.rect_stroke(
        best.rect,
        0.0,
        Stroke::new(2.0, outline),
        StrokeKind::Inside,
    );

    // Label chip — placed OUTSIDE the highlighted rect so it
    // doesn't cover the widget's actual content (text input,
    // title text, etc.). Default position is just above the rect's
    // top edge; if the rect is near the top of the viewport and
    // there's no room above, fall through to just below the rect's
    // bottom edge.
    let font = FontId::monospace(11.0);
    let galley = p.layout_no_wrap(best.label.clone(), font, Color32::WHITE);
    let pad = egui::vec2(5.0, 2.0);
    let chip_size = galley.size() + pad * 2.0;
    let viewport = ctx.content_rect();
    let above_y = best.rect.min.y - chip_size.y - 4.0;
    let below_y = best.rect.max.y + 4.0;
    let chip_top_y = if above_y >= viewport.min.y + 2.0 {
        above_y
    } else {
        below_y
    };
    let chip_origin = egui::pos2(best.rect.min.x, chip_top_y);
    let chip_rect = Rect::from_min_size(chip_origin, chip_size);
    p.rect_filled(
        chip_rect,
        2.0,
        Color32::from_rgba_unmultiplied(0, 0, 0, 220),
    );
    p.rect_stroke(
        chip_rect,
        2.0,
        Stroke::new(1.0, outline),
        StrokeKind::Inside,
    );
    p.galley(chip_origin + pad, galley, Color32::WHITE);
}
