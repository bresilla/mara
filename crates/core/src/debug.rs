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

use crate::memory::MaraMemory;
use crate::vocab::Id;
use egui::Rect;

const ENABLED_KEY: &str = "mara_debug_inspector_enabled";
const BEST_KEY: &str = "mara_debug_inspector_best";

#[derive(Clone)]
#[doc(hidden)]
pub struct Best {
    pub rect: Rect,
    pub label: String,
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

#[doc(hidden)]
pub fn best_id() -> Id {
    Id::new(BEST_KEY)
}

/// `true` when the inspector is on. Cheap — single ctx-data read.
pub fn is_enabled(ctx: &dyn crate::context::MaraCtx) -> bool {
    {
        let memory = ctx.memory();
        memory.get_temp::<bool>(enabled_id()).unwrap_or(false)
    }
}

/// Toggle the inspector overlay globally for this `ctx`.
pub fn set_enabled(ctx: &dyn crate::context::MaraCtx, on: bool) {
    ctx.memory().set_temp(enabled_id(), on);
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
pub fn tag(ctx: &dyn crate::context::MaraCtx, rect: Rect, label: impl Into<String>) {
    if !is_enabled(ctx) {
        return;
    }
    let Some(pointer) = ctx.input().pointer.map(Into::into) else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let label = label.into();
    {
        let mut memory = ctx.memory();
        let prev: Option<Best> = memory.get_temp::<Best>(best_id());
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
            memory.set_temp(best_id(), Best { rect, label });
        }
    }
}

/// Backend-neutral [`tag`] — reads the inspector flag + pointer and
/// stores the best hovered rect entirely through the [`UiBackend`]
/// contract, so widgets rendered on any backend can register debug
/// tags. `rect` is a Mara rect (converted to the backend rect for
/// storage). Used by the pod render path.
pub fn tag_backend(
    backend: &mut dyn crate::layout::UiBackend,
    rect: crate::vocab::Rect,
    label: impl Into<String>,
) {
    let enabled = backend
        .memory()
        .get_temp::<bool>(enabled_id().into())
        .unwrap_or(false);
    if !enabled {
        return;
    }
    let Some(pointer) = backend.input().pointer else {
        return;
    };
    if !rect.contains(pointer) {
        return;
    }
    let label = label.into();
    let egui_rect: Rect = rect.into();
    let mut memory = backend.memory();
    let prev: Option<Best> = memory.get_temp::<Best>(best_id().into());
    let take = match prev {
        None => true,
        Some(p) => egui_rect.area() < p.rect.area(),
    };
    if take {
        memory.set_temp(
            best_id().into(),
            Best {
                rect: egui_rect,
                label,
            },
        );
    }
}
