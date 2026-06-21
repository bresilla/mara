use mara_core::{
    RibbonAction, RibbonActionError, RibbonActionResult, RibbonOverrideLayer, RibbonOverridePolicy,
    RibbonSlot, RibbonSlotId, RibbonSlotItem, RibbonSlotOverride, ViewId, ViewRouter,
    dispatch_ribbon_action, permanent_system_control_slot, permanent_view_switcher_ribbon,
    resolve_slot_item, resolve_slot_items, restore_workspace_slot_override, vocab::Id as MaraId,
};

mod support {
    use mara_core::{MaraView, ViewCtx, ViewId};

    pub struct MockView {
        id: ViewId,
        title: &'static str,
        icon: &'static str,
    }

    impl MockView {
        pub fn new(id: &'static str, title: &'static str, icon: &'static str) -> Self {
            Self {
                id: ViewId::new(id),
                title,
                icon,
            }
        }
    }

    impl MaraView for MockView {
        fn id(&self) -> ViewId {
            self.id
        }

        fn title(&self) -> &str {
            self.title
        }

        fn icon(&self) -> &'static str {
            self.icon
        }

        fn show(&mut self, _ctx: &mut ViewCtx<'_>) {}
    }
}

fn item(id: &'static str, icon: &'static str) -> RibbonSlotItem {
    RibbonSlotItem::new(
        MaraId::new(id),
        icon,
        id,
        id,
        RibbonAction::Command(MaraId::new(id)),
    )
}

#[test]
fn ribbon_slot_items_require_non_empty_icons() {
    let result = std::panic::catch_unwind(|| {
        let _ = RibbonSlotItem::new(
            egui::Id::new("missing-icon"),
            "  ",
            "Missing icon",
            "Missing icon",
            RibbonAction::Noop,
        );
    });

    assert!(result.is_err());
}

#[test]
fn ribbon_slot_items_require_resolvable_font_or_svg_icons() {
    let missing_font_icon = std::panic::catch_unwind(|| {
        let _ = RibbonSlotItem::new(
            egui::Id::new("missing-font-icon"),
            "not_a_real_fluent_icon",
            "Missing icon",
            "Missing icon",
            RibbonAction::Noop,
        );
    });
    assert!(missing_font_icon.is_err());

    let svg_icon = RibbonSlotItem::new(
        egui::Id::new("svg-icon"),
        r#"<svg viewBox="0 0 16 16"></svg>"#,
        "SVG",
        "SVG",
        RibbonAction::Noop,
    );
    assert!(svg_icon.icon.starts_with("<svg"));
}

#[test]
fn ribbon_slot_items_require_label_and_tooltip() {
    let missing_label = std::panic::catch_unwind(|| {
        let _ = RibbonSlotItem::new(
            egui::Id::new("missing-label"),
            "info",
            " ",
            "Tooltip",
            RibbonAction::Noop,
        );
    });
    let missing_tooltip = std::panic::catch_unwind(|| {
        let _ = RibbonSlotItem::new(
            egui::Id::new("missing-tooltip"),
            "info",
            "Info",
            " ",
            RibbonAction::Noop,
        );
    });

    assert!(missing_label.is_err());
    assert!(missing_tooltip.is_err());
}

#[test]
fn slot_resolution_rejects_direct_invalid_slot_items() {
    let slot = RibbonSlot {
        id: RibbonSlotId::new("direct.invalid.item"),
        default_item: Some(RibbonSlotItem {
            id: MaraId::new("direct.invalid.item"),
            chrome_id: None,
            chrome_tooltip: None,
            icon: " ",
            label: "Invalid".to_owned(),
            tooltip: "Invalid".to_owned(),
            action: RibbonAction::Noop,
            active: false,
            draggable: false,
            role: None,
            child_ribbon: None,
        }),
        override_policy: RibbonOverridePolicy::Fixed,
    };

    let rejected = std::panic::catch_unwind(|| {
        let _ = resolve_slot_item(&slot, &[]);
    });

    assert!(rejected.is_err());
}

#[test]
fn featureful_ribbon_items_require_chrome_metadata() {
    let blank_chrome_id = std::panic::catch_unwind(|| {
        let _ = item("blank-chrome-id", "info").with_chrome_id(" ");
    });
    let blank_chrome_tooltip = std::panic::catch_unwind(|| {
        let _ = item("blank-chrome-tooltip", "info").with_chrome_tooltip(" ");
    });
    let blank_child_ribbon = std::panic::catch_unwind(|| {
        let _ = item("blank-child-ribbon", "info").with_child_ribbon(" ");
    });

    assert!(blank_chrome_id.is_err());
    assert!(blank_chrome_tooltip.is_err());
    assert!(blank_child_ribbon.is_err());
}

#[test]
fn featureful_ribbon_defs_require_chrome_metadata() {
    let blank_chrome_id = std::panic::catch_unwind(|| {
        let _ = mara_core::RibbonSlotDef::new(
            egui::Id::new("blank-chrome-ribbon"),
            mara_core::RibbonScope::Permanent,
            mara_core::RibbonEdge::Top,
            mara_core::RibbonCluster::Start,
            Vec::new(),
        )
        .with_chrome_id(" ");
    });
    let blank_accept_tag = std::panic::catch_unwind(|| {
        let _ = mara_core::RibbonSlotDef::new(
            egui::Id::new("blank-accept-ribbon"),
            mara_core::RibbonScope::Permanent,
            mara_core::RibbonEdge::Top,
            mara_core::RibbonCluster::Start,
            Vec::new(),
        )
        .accepts(&["panel", " "]);
    });

    assert!(blank_chrome_id.is_err());
    assert!(blank_accept_tag.is_err());
}

#[test]
fn ribbon_definitions_reject_duplicate_slot_ids() {
    let slot_id = RibbonSlotId::new("duplicate");
    let result = std::panic::catch_unwind(|| {
        let _ = mara_core::RibbonSlotDef::new(
            egui::Id::new("broken-ribbon"),
            mara_core::RibbonScope::Permanent,
            mara_core::RibbonEdge::Top,
            mara_core::RibbonCluster::Start,
            vec![
                RibbonSlot::new(
                    slot_id,
                    Some(item("first", "settings")),
                    RibbonOverridePolicy::Fixed,
                ),
                RibbonSlot::new(
                    slot_id,
                    Some(item("second", "info")),
                    RibbonOverridePolicy::Fixed,
                ),
            ],
        );
    });

    assert!(result.is_err());
}

#[test]
fn ribbon_override_layers_reject_duplicate_slot_ids() {
    let slot_id = RibbonSlotId::new("duplicate.override");
    let duplicate_layer = std::panic::catch_unwind(|| {
        let _ = RibbonOverrideLayer::new(vec![
            RibbonSlotOverride::new(slot_id, item("first-override", "settings")),
            RibbonSlotOverride::hidden(slot_id),
        ]);
    });
    let duplicate_builder = std::panic::catch_unwind(|| {
        let _ = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
            slot_id,
            item("builder-override", "info"),
        )])
        .with_hidden_slot(slot_id);
    });

    assert!(duplicate_layer.is_err());
    assert!(duplicate_builder.is_err());
}

#[test]
fn slot_resolution_rejects_direct_invalid_override_layers() {
    let slot_id = RibbonSlotId::new("direct.invalid.layer");
    let slot = RibbonSlot::new(
        slot_id,
        Some(item("default", "settings")),
        RibbonOverridePolicy::LayerOverride,
    );
    let layer = RibbonOverrideLayer {
        overrides: vec![RibbonSlotOverride {
            slot: slot_id,
            item: Some(RibbonSlotItem {
                id: MaraId::new("direct.invalid.override.item"),
                chrome_id: None,
                chrome_tooltip: None,
                icon: "info",
                label: String::new(),
                tooltip: "Invalid override".to_owned(),
                action: RibbonAction::Noop,
                active: false,
                draggable: false,
                role: None,
                child_ribbon: None,
            }),
        }],
    };

    let rejected = std::panic::catch_unwind(|| {
        let _ = resolve_slot_item(&slot, &[layer]);
    });

    assert!(rejected.is_err());
}

#[test]
fn fixed_slot_ignores_layer_overrides() {
    let slot_id = RibbonSlotId::new("system.close_or_restore");
    let slot = RibbonSlot::new(
        slot_id,
        Some(item("close", "dismiss")),
        RibbonOverridePolicy::Fixed,
    );
    let layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        slot_id,
        item("restore", "arrow-minimize"),
    )]);

    assert_eq!(resolve_slot_item(&slot, &[layer]).unwrap().icon, "dismiss");
}

#[test]
fn layer_override_replaces_default() {
    let slot_id = RibbonSlotId::new("system.close_or_restore");
    let slot = RibbonSlot::new(
        slot_id,
        Some(item("close", "dismiss")),
        RibbonOverridePolicy::LayerOverride,
    );
    let layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        slot_id,
        item("restore", "arrow-minimize"),
    )]);

    assert_eq!(
        resolve_slot_item(&slot, &[layer]).unwrap().icon,
        "arrow-minimize"
    );
}

#[test]
fn deeper_workspace_override_beats_view_override() {
    let slot_id = RibbonSlotId::new("system.close_or_restore");
    let slot = RibbonSlot::new(
        slot_id,
        Some(item("close", "dismiss")),
        RibbonOverridePolicy::LayerOverride,
    );
    let view_layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        slot_id,
        RibbonSlotItem::new(
            egui::Id::new("view-settings"),
            "settings",
            "settings",
            "settings",
            RibbonAction::SwitchView(ViewId::new("settings")),
        ),
    )]);
    let l1_layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        slot_id,
        RibbonSlotItem::new(
            egui::Id::new("restore"),
            "arrow-minimize",
            "restore",
            "restore",
            RibbonAction::PopWorkspace,
        ),
    )]);

    let resolved = resolve_slot_item(&slot, &[view_layer, l1_layer]).unwrap();
    assert_eq!(resolved.icon, "arrow-minimize");
    assert_eq!(resolved.action, RibbonAction::PopWorkspace);
}

#[test]
fn fallback_returns_permanent_default_when_no_override_exists() {
    let slot = RibbonSlot::new(
        RibbonSlotId::new("global.status"),
        Some(item("status", "circle")),
        RibbonOverridePolicy::LayerOverride,
    );

    assert_eq!(resolve_slot_item(&slot, &[]).unwrap().icon, "circle");
}

#[test]
fn append_policy_keeps_default_and_adds_layer_items() {
    let slot_id = RibbonSlotId::new("global.tools");
    let slot = RibbonSlot::new(
        slot_id,
        Some(item("base", "home")),
        RibbonOverridePolicy::LayerAppend,
    );
    let view_layer =
        RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(slot_id, item("view", "eye"))]);
    let l1_layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        slot_id,
        item("l1", "paint-brush"),
    )]);

    let icons: Vec<&'static str> = resolve_slot_items(&slot, &[view_layer, l1_layer])
        .into_iter()
        .map(|item| item.icon)
        .collect();
    assert_eq!(icons, vec!["home", "eye", "paint-brush"]);
}

#[test]
fn permanent_view_switcher_generates_switch_view_items() {
    let mut router = ViewRouter::new(support::MockView::new("bevy", "Bevy", "cube"));
    router.register(support::MockView::new("graph", "Graph", "flowchart"));

    let ribbon = permanent_view_switcher_ribbon(router.entries());
    assert_eq!(ribbon.slots.len(), 2);
    let graph_item = ribbon.slots[1].default_item.as_ref().unwrap();
    assert_eq!(graph_item.icon, "flowchart");
    assert_eq!(
        graph_item.action,
        RibbonAction::SwitchView(ViewId::new("graph"))
    );
}

#[test]
fn permanent_system_control_slot_can_resolve_to_restore_override() {
    let slot = permanent_system_control_slot();
    assert_eq!(
        resolve_slot_item(&slot, &[]).unwrap().action,
        RibbonAction::CloseApp
    );

    let layer = RibbonOverrideLayer::new(vec![restore_workspace_slot_override()]);
    assert_eq!(
        resolve_slot_item(&slot, &[layer]).unwrap().action,
        RibbonAction::PopWorkspace
    );
}

#[test]
fn dispatch_switch_view_and_workspace_actions() {
    let mut router = ViewRouter::new(support::MockView::new("bevy", "Bevy", "cube"));
    let graph = router.register(support::MockView::new("graph", "Graph", "flowchart"));

    assert_eq!(
        dispatch_ribbon_action(RibbonAction::SwitchView(graph), &mut router),
        Ok(RibbonActionResult::SwitchedView(graph))
    );
    assert_eq!(router.active(), Ok(graph));

    let module_id = MaraId::new("image-module");
    assert_eq!(
        dispatch_ribbon_action(RibbonAction::PushModuleWorkspace(module_id), &mut router),
        Ok(RibbonActionResult::PushedModuleWorkspace(module_id))
    );
    assert_eq!(router.active_workspace().unwrap().depth(), 1);

    assert_eq!(
        dispatch_ribbon_action(RibbonAction::CloseApp, &mut router),
        Err(RibbonActionError::AppWindowControlsDenied)
    );

    assert_eq!(
        dispatch_ribbon_action(RibbonAction::PopWorkspace, &mut router),
        Ok(RibbonActionResult::PoppedWorkspace)
    );
    assert_eq!(router.active_workspace().unwrap().depth(), 0);
}
