//! # Ribbons — edge-anchored slot button strips.
//!
//! The public API has one ribbon declaration family:
//! [`RibbonSlotDef`] / [`RibbonSlot`] / [`RibbonSlotItem`]. It carries
//! permanent/view/workspace scope semantics and the featureful chrome
//! behavior: panel buttons, drag/reorder placement, cross-ribbon drops,
//! fullscreen rails, and pane anchoring.
//! * [`paint_ribbon_button`] / [`paint_ribbon_glyph`] / etc. —
//!   surfaced from [`paint`] for downstream callers that want to
//!   paint their own button-shaped widgets in the same style.
//! * [`EDGE_GAP`] / [`SIDE_BTN_SIZE`] / [`SIDE_BTN_GAP`] — layout
//!   constants every consumer (incl. [`crate::pane`]) reads to
//!   align with the rail strip.

pub mod action;
#[allow(dead_code)]
pub(crate) mod chrome;
pub mod dispatch;
mod paint;
pub mod permanent;
pub mod resolve;
pub mod slot;
pub mod slot_paint;

// Layout constants — re-exported so `pane::layout` and other
// modules can compute insets without duplicating values.
pub use paint::{EDGE_GAP, SIDE_BTN_GAP, SIDE_BTN_SIZE};

pub use action::RibbonAction;
pub use chrome::{
    RibbonAvoidance, RibbonCluster, RibbonDrag, RibbonEdge, RibbonGlyph, RibbonMode, RibbonOpen,
    RibbonPlacement, RibbonRole, RibbonWidth, main_bar_empty_drag_started, ribbon_avoiding_rect,
    ribbon_clearance,
};
pub use dispatch::{RibbonActionError, RibbonActionResult, dispatch_ribbon_action};
pub use permanent::{
    app_menu_command_id, app_menu_slot_id, bottom_shelf_command_id, bottom_shelf_slot_id,
    left_shelf_command_id, left_shelf_slot_id, permanent_app_menu_slot,
    permanent_bottom_shelf_slot, permanent_left_shelf_slot, permanent_right_shelf_slot,
    permanent_system_control_slot, permanent_view_switcher_ribbon, restore_workspace_slot_override,
    right_shelf_command_id, right_shelf_slot_id, system_close_or_restore_slot_id,
};
pub use resolve::{resolve_slot_item, resolve_slot_items};
pub use slot::{
    RibbonOverrideLayer, RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef,
    RibbonSlotId, RibbonSlotItem, RibbonSlotOverride,
};
pub use slot_paint::{
    ResolvedSlotRibbon, RibbonSlotClick, draw_slot_ribbons, draw_slot_ribbons_featureful,
    phone_remapped_ribbon_edge,
};
