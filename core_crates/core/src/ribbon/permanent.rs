use egui::Id;

use crate::view::ViewEntry;

use super::{
    RibbonAction, RibbonCluster, RibbonEdge, RibbonOverridePolicy, RibbonScope, RibbonSlot,
    RibbonSlotDef, RibbonSlotId, RibbonSlotItem, RibbonSlotOverride,
};

#[must_use]
pub fn app_menu_slot_id() -> RibbonSlotId {
    RibbonSlotId::new("system.app_menu")
}

#[must_use]
pub fn app_menu_command_id() -> Id {
    Id::new("system.app_menu")
}

#[must_use]
pub fn permanent_app_menu_slot() -> RibbonSlot {
    let menu = RibbonSlotItem::new(
        Id::new("system.app_menu.item"),
        "line-horizontal-3",
        "Menu",
        "Open application menu",
        RibbonAction::Command(app_menu_command_id()),
    );
    RibbonSlot::new(
        app_menu_slot_id(),
        Some(menu),
        RibbonOverridePolicy::LayerOverride,
    )
}

#[must_use]
pub fn left_shelf_slot_id() -> RibbonSlotId {
    RibbonSlotId::new("system.left_shelf")
}

#[must_use]
pub fn left_shelf_command_id() -> Id {
    Id::new("system.left_shelf")
}

#[must_use]
pub fn permanent_left_shelf_slot() -> RibbonSlot {
    let shelf = RibbonSlotItem::new(
        Id::new("system.left_shelf.item"),
        "panel-left",
        "Left shelf",
        "Show left shelf",
        RibbonAction::Command(left_shelf_command_id()),
    );
    RibbonSlot::new(
        left_shelf_slot_id(),
        Some(shelf),
        RibbonOverridePolicy::LayerOverride,
    )
}

#[must_use]
pub fn right_shelf_slot_id() -> RibbonSlotId {
    RibbonSlotId::new("system.right_shelf")
}

#[must_use]
pub fn right_shelf_command_id() -> Id {
    Id::new("system.right_shelf")
}

#[must_use]
pub fn permanent_right_shelf_slot() -> RibbonSlot {
    let shelf = RibbonSlotItem::new(
        Id::new("system.right_shelf.item"),
        "panel-right",
        "Right shelf",
        "Show right shelf",
        RibbonAction::Command(right_shelf_command_id()),
    );
    RibbonSlot::new(
        right_shelf_slot_id(),
        Some(shelf),
        RibbonOverridePolicy::LayerOverride,
    )
}

#[must_use]
pub fn bottom_shelf_slot_id() -> RibbonSlotId {
    RibbonSlotId::new("system.bottom_shelf")
}

#[must_use]
pub fn bottom_shelf_command_id() -> Id {
    Id::new("system.bottom_shelf")
}

#[must_use]
pub fn permanent_bottom_shelf_slot() -> RibbonSlot {
    let shelf = RibbonSlotItem::new(
        Id::new("system.bottom_shelf.item"),
        "panel-bottom",
        "Bottom shelf",
        "Show bottom shelf",
        RibbonAction::Command(bottom_shelf_command_id()),
    );
    RibbonSlot::new(
        bottom_shelf_slot_id(),
        Some(shelf),
        RibbonOverridePolicy::LayerOverride,
    )
}

#[must_use]
pub fn system_close_or_restore_slot_id() -> RibbonSlotId {
    RibbonSlotId::new("system.close_or_restore")
}

#[must_use]
pub fn permanent_system_control_slot() -> RibbonSlot {
    let slot_id = system_close_or_restore_slot_id();
    let close = RibbonSlotItem::new(
        Id::new("system.close_app"),
        crate::style::theme().views.close_icon,
        "Close",
        "Close application",
        RibbonAction::CloseApp,
    );
    RibbonSlot::new(slot_id, Some(close), RibbonOverridePolicy::LayerOverride)
}

#[must_use]
pub fn restore_workspace_slot_override() -> RibbonSlotOverride {
    RibbonSlotOverride::new(
        system_close_or_restore_slot_id(),
        RibbonSlotItem::new(
            Id::new("system.restore_workspace"),
            crate::style::theme().modules.workspace_restore_icon,
            crate::style::theme().modules.workspace_restore_label,
            "Return to parent workspace",
            RibbonAction::PopWorkspace,
        ),
    )
}

#[must_use]
pub fn permanent_view_switcher_ribbon(entries: &[ViewEntry]) -> RibbonSlotDef {
    let slots = entries
        .iter()
        .map(|entry| {
            let slot_id = RibbonSlotId::new(("view.switch", entry.id.0));
            let item = RibbonSlotItem::new(
                Id::new(("view.switch.item", entry.id.0)),
                entry.icon,
                entry.title.clone(),
                format!("Switch to {}", entry.title),
                RibbonAction::SwitchView(entry.id),
            );
            RibbonSlot::new(slot_id, Some(item), RibbonOverridePolicy::Fixed)
        })
        .collect();

    RibbonSlotDef::new(
        Id::new("mara.permanent.view_switcher"),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Start,
        slots,
    )
}
