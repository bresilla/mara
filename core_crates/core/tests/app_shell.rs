use mara_core::{
    AppShellError, MaraView, RibbonAction, RibbonCluster, RibbonEdge, RibbonOverrideLayer,
    RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    RibbonSlotOverride, ViewCtx, ViewId, ViewRouter, WindowControlsPolicy,
    dispatch_app_shell_action, permanent_system_control_slot, resolve_app_shell_ribbons,
    resolve_app_shell_ribbons_with_workspace_chrome,
    resolve_app_shell_ribbons_with_workspace_layers,
};

struct ShellView {
    id: ViewId,
    title: &'static str,
    override_slot: Option<RibbonSlotOverride>,
}

impl ShellView {
    fn new(id: &'static str, title: &'static str) -> Self {
        Self {
            id: ViewId::new(id),
            title,
            override_slot: None,
        }
    }

    fn with_override(mut self, override_slot: RibbonSlotOverride) -> Self {
        self.override_slot = Some(override_slot);
        self
    }
}

impl MaraView for ShellView {
    fn id(&self) -> ViewId {
        self.id
    }

    fn title(&self) -> &str {
        self.title
    }

    fn icon(&self) -> &'static str {
        "view"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        let item = RibbonSlotItem::new(
            egui::Id::new(("view.local.item", self.id.0)),
            "eye",
            "View Tool",
            "View-local tool",
            RibbonAction::Command(egui::Id::new("view-tool")),
        );
        vec![RibbonSlotDef::new(
            egui::Id::new(("view.local.ribbon", self.id.0)),
            RibbonScope::View(self.id),
            RibbonEdge::Left,
            RibbonCluster::Start,
            vec![RibbonSlot::new(
                RibbonSlotId::new(("view.local.slot", self.id.0)),
                Some(item),
                RibbonOverridePolicy::Fixed,
            )],
        )]
    }

    fn ribbon_overrides(&mut self) -> RibbonOverrideLayer {
        RibbonOverrideLayer::new(self.override_slot.clone().into_iter().collect())
    }

    fn show(&mut self, _ctx: &mut ViewCtx<'_>) {}
}

fn permanent_main_with_system_control() -> Vec<RibbonSlotDef> {
    vec![RibbonSlotDef::new(
        egui::Id::new("main.bar"),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Middle,
        vec![permanent_system_control_slot()],
    )]
}

fn empty_permanent_ribbon(id: &'static str) -> RibbonSlotDef {
    RibbonSlotDef::new(
        egui::Id::new(id),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Middle,
        Vec::new(),
    )
}

fn permanent_ribbon_on_edge(id: &'static str, edge: RibbonEdge) -> RibbonSlotDef {
    RibbonSlotDef::new(
        egui::Id::new(id),
        RibbonScope::Permanent,
        edge,
        RibbonCluster::Middle,
        Vec::new(),
    )
}

#[test]
fn app_shell_chrome_rejects_duplicate_permanent_slot_ids_when_merging() {
    let slot_id = RibbonSlotId::new("shared.slot");
    let first = RibbonSlotDef::new(
        egui::Id::new("main.bar"),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Middle,
        vec![RibbonSlot::new(
            slot_id,
            Some(RibbonSlotItem::new(
                egui::Id::new("first"),
                "settings",
                "First",
                "First",
                RibbonAction::Noop,
            )),
            RibbonOverridePolicy::Fixed,
        )],
    );
    let second = RibbonSlotDef::new(
        egui::Id::new("extra.bar"),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Middle,
        vec![RibbonSlot::new(
            slot_id,
            Some(RibbonSlotItem::new(
                egui::Id::new("second"),
                "info",
                "Second",
                "Second",
                RibbonAction::Noop,
            )),
            RibbonOverridePolicy::Fixed,
        )],
    );

    let result = std::panic::catch_unwind(|| {
        let _ = mara_core::AppShellChrome::new(first).with_permanent_ribbon(second);
    });

    assert!(result.is_err());
}

#[test]
fn app_shell_chrome_rejects_non_permanent_merged_ribbon() {
    let result = std::panic::catch_unwind(|| {
        let _ = mara_core::AppShellChrome::new(empty_permanent_ribbon("main.bar"))
            .with_permanent_ribbon(RibbonSlotDef::new(
                egui::Id::new("view.local.bar"),
                RibbonScope::View(ViewId::new("canvas")),
                RibbonEdge::Top,
                RibbonCluster::Middle,
                Vec::new(),
            ));
    });

    assert!(result.is_err());
}

#[test]
fn app_shell_resolves_permanent_and_active_view_ribbons() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let permanent = permanent_main_with_system_control();

    let resolved = resolve_app_shell_ribbons(&mut router, &permanent).unwrap();
    assert_eq!(resolved.ribbons.len(), 2);
    assert_eq!(resolved.ribbons[0].items[0].action, RibbonAction::CloseApp);
    assert_eq!(
        resolved.ribbons[1].scope,
        RibbonScope::View(ViewId::new("bevy"))
    );
    assert_eq!(resolved.ribbons[1].items[0].icon, "eye");
}

#[test]
fn app_shell_rejects_more_than_one_permanent_ribbon() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let permanent = vec![
        empty_permanent_ribbon("main.bar"),
        empty_permanent_ribbon("second.permanent.bar"),
    ];

    let error = resolve_app_shell_ribbons(&mut router, &permanent).unwrap_err();
    assert!(matches!(
        error,
        AppShellError::MultiplePermanentRibbons { count: 2 }
    ));
}

#[test]
fn app_shell_rejects_missing_permanent_ribbon() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));

    let error = resolve_app_shell_ribbons(&mut router, &[]).unwrap_err();
    assert!(matches!(error, AppShellError::MissingPermanentRibbon));
}

#[test]
fn app_shell_rejects_non_top_permanent_ribbons() {
    for (name, edge) in [
        ("left.main.bar", RibbonEdge::Left),
        ("right.main.bar", RibbonEdge::Right),
        ("bottom.main.bar", RibbonEdge::Bottom),
    ] {
        let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
        let permanent = vec![permanent_ribbon_on_edge(name, edge)];

        let error = resolve_app_shell_ribbons(&mut router, &permanent).unwrap_err();
        assert!(matches!(
            error,
            AppShellError::PermanentRibbonNotTop { id, edge: actual_edge }
                if id == egui::Id::new(name) && actual_edge == edge
        ));
    }
}

#[test]
fn app_shell_rejects_permanent_ribbon_that_accepts_icon_drops() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let permanent = vec![empty_permanent_ribbon("main.drop.target.bar").accepts(&["*"])];

    let error = resolve_app_shell_ribbons(&mut router, &permanent).unwrap_err();
    assert!(matches!(
        error,
        AppShellError::PermanentRibbonMustBeFixed { id }
            if id == egui::Id::new("main.drop.target.bar")
    ));
}

#[test]
fn app_shell_rejects_active_view_ribbons_with_wrong_scope() {
    struct WrongScopedView {
        id: ViewId,
        ribbon: RibbonSlotDef,
    }

    impl MaraView for WrongScopedView {
        fn id(&self) -> ViewId {
            self.id
        }

        fn title(&self) -> &str {
            "Wrong Scoped"
        }

        fn icon(&self) -> &'static str {
            "view"
        }

        fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
            vec![self.ribbon.clone()]
        }

        fn show(&mut self, _ctx: &mut ViewCtx<'_>) {}
    }

    let bad_ribbon_id = egui::Id::new("bad.view.ribbon.scope");
    let mut router = ViewRouter::new(WrongScopedView {
        id: ViewId::new("bevy"),
        ribbon: RibbonSlotDef::new(
            bad_ribbon_id,
            RibbonScope::View(ViewId::new("canvas")),
            RibbonEdge::Left,
            RibbonCluster::Start,
            Vec::new(),
        ),
    });

    let error = resolve_app_shell_ribbons(&mut router, &permanent_main_with_system_control())
        .expect_err("active views may only emit ribbons scoped to themselves");
    assert!(matches!(
        error,
        AppShellError::ViewRibbonWrongScope { id } if id == bad_ribbon_id
    ));
}

#[test]
fn app_shell_rejects_workspace_ribbons_with_wrong_scope() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("canvas-module"));
    let bad_ribbon_id = egui::Id::new("bad.workspace.ribbon.scope");
    let workspace_ribbon = RibbonSlotDef::new(
        bad_ribbon_id,
        RibbonScope::WorkspaceLevel(egui::Id::new("other-workspace")),
        RibbonEdge::Right,
        RibbonCluster::Middle,
        Vec::new(),
    );

    let error = resolve_app_shell_ribbons_with_workspace_chrome(
        &mut router,
        &permanent_main_with_system_control(),
        &[workspace_ribbon],
        &[],
    )
    .expect_err("workspace ribbons must belong to the active workspace level");
    assert!(matches!(
        error,
        AppShellError::WorkspaceRibbonWrongScope { id } if id == bad_ribbon_id
    ));
}

#[test]
fn app_shell_rejects_direct_invalid_ribbon_defs() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let mut permanent = permanent_main_with_system_control();
    permanent[0].chrome_id = Some(" ");

    let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = resolve_app_shell_ribbons(&mut router, &permanent);
    }));

    assert!(rejected.is_err());
}

#[test]
fn l1_workspace_overrides_permanent_close_slot_with_restore() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("graph"));
    let permanent = permanent_main_with_system_control();

    let resolved = resolve_app_shell_ribbons(&mut router, &permanent).unwrap();
    let system = &resolved.ribbons[0].items[0];
    assert_eq!(system.action, RibbonAction::PopWorkspace);
    assert_eq!(system.icon, "arrow-minimize");
}

#[test]
fn deepest_workspace_override_beats_active_view_override_in_app_shell() {
    let override_slot = RibbonSlotOverride::new(
        mara_core::system_close_or_restore_slot_id(),
        RibbonSlotItem::new(
            egui::Id::new("view.close.override"),
            "settings",
            "Settings",
            "Settings",
            RibbonAction::Command(egui::Id::new("settings")),
        ),
    );
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy").with_override(override_slot));
    let permanent = permanent_main_with_system_control();

    let l0 = resolve_app_shell_ribbons(&mut router, &permanent).unwrap();
    assert_eq!(l0.ribbons[0].items[0].icon, "settings");

    router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("graph"));
    let l1 = resolve_app_shell_ribbons(&mut router, &permanent).unwrap();
    assert_eq!(l1.ribbons[0].items[0].action, RibbonAction::PopWorkspace);
}

#[test]
fn app_shell_dispatch_routes_actions() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let graph = router.register(ShellView::new("graph", "Graph"));

    dispatch_app_shell_action(&mut router, RibbonAction::SwitchView(graph)).unwrap();
    assert_eq!(router.active(), Ok(graph));

    dispatch_app_shell_action(
        &mut router,
        RibbonAction::PushModuleWorkspace(egui::Id::new("image")),
    )
    .unwrap();
    assert_eq!(router.active_workspace().unwrap().depth(), 1);

    dispatch_app_shell_action(&mut router, RibbonAction::PopWorkspace).unwrap();
    assert_eq!(router.active_workspace().unwrap().depth(), 0);
}

#[test]
fn workspace_supplied_layers_can_override_active_l1_slots() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("graph"));
    let permanent = permanent_main_with_system_control();
    let workspace_layer = RibbonOverrideLayer::new(vec![RibbonSlotOverride::new(
        mara_core::system_close_or_restore_slot_id(),
        RibbonSlotItem::new(
            egui::Id::new("module.custom.restore"),
            "arrow-minimize",
            "Back",
            "Back from module",
            RibbonAction::PopWorkspace,
        ),
    )]);

    let resolved = resolve_app_shell_ribbons_with_workspace_layers(
        &mut router,
        &permanent,
        &[workspace_layer],
    )
    .unwrap();
    assert_eq!(resolved.ribbons[0].items[0].icon, "arrow-minimize");
}

#[test]
fn workspace_local_ribbons_participate_in_shell_resolution() {
    use mara_core::{
        RibbonAction, RibbonCluster, RibbonEdge, RibbonOverridePolicy, RibbonScope, RibbonSlot,
        RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    };

    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let level = router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("canvas-module"));
    let workspace_ribbon = RibbonSlotDef::new(
        egui::Id::new("workspace.ribbon"),
        RibbonScope::WorkspaceLevel(level.id),
        RibbonEdge::Top,
        RibbonCluster::Middle,
        vec![RibbonSlot::new(
            RibbonSlotId::new("workspace.tool"),
            Some(RibbonSlotItem::new(
                egui::Id::new("workspace.pen"),
                "pen",
                "Pen",
                "Use pen",
                RibbonAction::Command(egui::Id::new("workspace.pen.command")),
            )),
            RibbonOverridePolicy::Fixed,
        )],
    );

    let resolved = mara_core::resolve_app_shell_ribbons_with_workspace_chrome(
        &mut router,
        &permanent_main_with_system_control(),
        &[workspace_ribbon],
        &[],
    )
    .unwrap();

    let workspace = resolved
        .ribbons
        .iter()
        .find(|ribbon| matches!(ribbon.scope, RibbonScope::WorkspaceLevel(id) if id == level.id))
        .unwrap();
    assert_eq!(workspace.items[0].icon, "pen");
}

#[test]
fn app_shell_calls_workspace_renderer_for_l1() {
    use mara_core::{
        RibbonAction, RibbonCluster, RibbonEdge, RibbonOverridePolicy, RibbonScope, RibbonSlot,
        RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    };

    let egui_ctx = egui::Context::default();
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let level = router
        .active_workspace_mut()
        .unwrap()
        .push_module(egui::Id::new("graph-module"));
    let mut called = false;

    let (resolved, _) = mara_core::show_app_shell_with_workspace_renderer(
        &egui_ctx,
        &mut router,
        &permanent_main_with_system_control(),
        egui::Color32::WHITE,
        |_ctx, ws| {
            called = true;
            ws.add_ribbon(RibbonSlotDef::new(
                egui::Id::new("workspace.rendered.ribbon"),
                RibbonScope::WorkspaceLevel(level.id),
                RibbonEdge::Top,
                RibbonCluster::Middle,
                vec![RibbonSlot::new(
                    RibbonSlotId::new("workspace.rendered.tool"),
                    Some(RibbonSlotItem::new(
                        egui::Id::new("workspace.rendered.item"),
                        "flowchart",
                        "Graph",
                        "Graph tool",
                        RibbonAction::Command(egui::Id::new("workspace.rendered.command")),
                    )),
                    RibbonOverridePolicy::Fixed,
                )],
            ));
        },
    )
    .unwrap();

    assert!(called);
    assert!(resolved.ribbons.iter().any(
        |ribbon| matches!(ribbon.scope, RibbonScope::WorkspaceLevel(id) if id == level.id)
            && ribbon.items[0].icon == "flowchart"
    ));
}

fn main_bar_with_slots(slots: Vec<RibbonSlot>) -> mara_core::AppShellChrome {
    mara_core::AppShellChrome::new(RibbonSlotDef::new(
        egui::Id::new("main.bar"),
        RibbonScope::Permanent,
        RibbonEdge::Top,
        RibbonCluster::Middle,
        slots,
    ))
}

#[test]
fn app_shell_chrome_enforces_persistent_main_bar() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let canvas = router.register(ShellView::new("canvas", "Canvas"));
    let slot = RibbonSlot::new(
        RibbonSlotId::new("view.bevy"),
        Some(RibbonSlotItem::new(
            egui::Id::new("view.bevy.item"),
            "cube",
            "Bevy",
            "Bevy view",
            RibbonAction::SwitchView(ViewId::new("bevy")),
        )),
        RibbonOverridePolicy::Fixed,
    );
    let chrome = main_bar_with_slots(vec![slot]);

    let initial = mara_core::resolve_app_shell_chrome(&mut router, &chrome).unwrap();
    assert_eq!(initial.ribbons[0].scope, RibbonScope::Permanent);
    assert_eq!(initial.ribbons[0].items[0].icon, "cube");

    router.set_active(canvas).unwrap();
    let switched = mara_core::resolve_app_shell_chrome(&mut router, &chrome).unwrap();
    assert_eq!(switched.ribbons[0].scope, RibbonScope::Permanent);
    assert_eq!(switched.ribbons[0].items[0].icon, "cube");
}

#[test]
fn app_shell_chrome_rejects_non_permanent_main_bar() {
    let result = std::panic::catch_unwind(|| {
        let _ = mara_core::AppShellChrome::new(RibbonSlotDef::new(
            egui::Id::new("main.view.scoped.bar"),
            RibbonScope::View(ViewId::new("canvas")),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            Vec::new(),
        ));
    });

    assert!(result.is_err());
}

#[test]
fn persistent_main_bar_slot_requires_explicit_hide_override() {
    let slot_id = RibbonSlotId::new("global.about");
    let slot = RibbonSlot::new(
        slot_id,
        Some(RibbonSlotItem::new(
            egui::Id::new("global.about.item"),
            "info",
            "About",
            "About",
            RibbonAction::Command(egui::Id::new("about")),
        )),
        RibbonOverridePolicy::LayerOverride,
    );
    let mut router = ViewRouter::new(
        ShellView::new("canvas", "Canvas").with_override(RibbonSlotOverride::hidden(slot_id)),
    );
    let chrome = main_bar_with_slots(vec![slot]);

    let resolved = mara_core::resolve_app_shell_chrome(&mut router, &chrome).unwrap();
    assert!(
        resolved.ribbons[0]
            .items
            .iter()
            .all(|item| item.id != egui::Id::new("global.about.item")),
        "the view override should hide only the requested inherited slot"
    );
    assert!(
        resolved.ribbons[0]
            .items
            .iter()
            .any(|item| item.action == RibbonAction::CloseApp),
        "mandatory window controls should remain unless the chrome opts out"
    );
}

#[test]
fn app_shell_chrome_includes_mandatory_close_controls_by_default() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let chrome = main_bar_with_slots(vec![]);

    assert_eq!(
        chrome.window_controls_policy(),
        WindowControlsPolicy::Enabled
    );
    let resolved = mara_core::resolve_app_shell_chrome(&mut router, &chrome).unwrap();

    assert_eq!(
        resolved
            .ribbons
            .iter()
            .filter(|ribbon| matches!(ribbon.scope, RibbonScope::Permanent))
            .count(),
        1
    );
    let system = &resolved.ribbons[0];
    assert_eq!(system.scope, RibbonScope::Permanent);
    assert_eq!(system.edge, RibbonEdge::Top);
    assert!(
        system
            .items
            .iter()
            .any(|item| item.action == RibbonAction::CloseApp)
    );
}

#[test]
fn app_shell_chrome_rejects_non_top_main_bars() {
    for edge in [RibbonEdge::Left, RibbonEdge::Right, RibbonEdge::Bottom] {
        let result = std::panic::catch_unwind(|| {
            let _ = mara_core::AppShellChrome::new(RibbonSlotDef::new(
                egui::Id::new(("main.non.top.bar", format!("{edge:?}"))),
                RibbonScope::Permanent,
                edge,
                RibbonCluster::Middle,
                Vec::new(),
            ));
        });

        assert!(result.is_err());
    }
}

#[test]
fn app_shell_chrome_rejects_main_bar_that_accepts_icon_drops() {
    let result = std::panic::catch_unwind(|| {
        let _ = mara_core::AppShellChrome::new(
            RibbonSlotDef::new(
                egui::Id::new("main.drop.target.bar"),
                RibbonScope::Permanent,
                RibbonEdge::Top,
                RibbonCluster::Middle,
                Vec::new(),
            )
            .accepts(&["*"]),
        );
    });

    assert!(result.is_err());
}

#[test]
fn app_shell_chrome_rejects_non_top_merged_permanent_ribbons() {
    for edge in [RibbonEdge::Left, RibbonEdge::Right, RibbonEdge::Bottom] {
        let result = std::panic::catch_unwind(|| {
            let _ = main_bar_with_slots(vec![]).with_permanent_ribbon(RibbonSlotDef::new(
                egui::Id::new(("non.top.extra.bar", format!("{edge:?}"))),
                RibbonScope::Permanent,
                edge,
                RibbonCluster::Middle,
                Vec::new(),
            ));
        });

        assert!(result.is_err());
    }
}

#[test]
fn app_shell_chrome_can_opt_out_of_window_controls_for_games() {
    let mut router = ViewRouter::new(ShellView::new("bevy", "Bevy"));
    let chrome = main_bar_with_slots(vec![]).without_window_controls();

    assert_eq!(
        chrome.window_controls_policy(),
        WindowControlsPolicy::Hidden
    );
    let resolved = mara_core::resolve_app_shell_chrome(&mut router, &chrome).unwrap();

    let permanent = resolved
        .ribbons
        .iter()
        .find(|ribbon| matches!(ribbon.scope, RibbonScope::Permanent))
        .unwrap();
    assert!(permanent.items.is_empty());
}
