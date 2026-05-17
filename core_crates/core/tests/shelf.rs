use egui::{Id, Rect, pos2};
use mara_core::{
    RibbonEdge, ShelfContainer, ShelfDef, ShelfEdge, ShelfEdgeError, ShelfState, layout_shelves,
    pane::{PaneAnchor, RailZone, TitleSide},
    shelf_insets, style,
};

fn test_tabs() -> Vec<mara_core::container::Tab> {
    vec![mara_core::container::Tab::new("test.tab", "Tab", "box")]
}

#[test]
fn shelf_edge_rejects_top() {
    assert_eq!(
        ShelfEdge::try_from(RibbonEdge::Top),
        Err(ShelfEdgeError::TopShelfForbidden)
    );
    assert_eq!(ShelfEdge::try_from(RibbonEdge::Left), Ok(ShelfEdge::Left));
    assert_eq!(ShelfEdge::try_from(RibbonEdge::Right), Ok(ShelfEdge::Right));
    assert_eq!(
        ShelfEdge::try_from(RibbonEdge::Bottom),
        Ok(ShelfEdge::Bottom)
    );
}

#[test]
fn shelf_edges_choose_expected_container_tab_orientation() {
    assert_eq!(
        ShelfEdge::Left.container_anchor(),
        PaneAnchor::TopRail(RailZone::Middle)
    );
    assert_eq!(
        ShelfEdge::Right.container_anchor(),
        PaneAnchor::TopRail(RailZone::Middle)
    );
    assert_eq!(
        ShelfEdge::Bottom.container_anchor(),
        PaneAnchor::LeftRail(RailZone::Middle)
    );
    assert_eq!(ShelfEdge::Left.container_tab_strip_side(), TitleSide::Left);
    assert_eq!(
        ShelfEdge::Right.container_tab_strip_side(),
        TitleSide::Right
    );
    assert_eq!(ShelfEdge::Bottom.container_tab_strip_side(), TitleSide::Top);
}

#[test]
fn shelves_reserve_viewport_space() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new("left", ShelfEdge::Left, egui::Color32::WHITE).default_size(200.0),
        ShelfDef::new("right", ShelfEdge::Right, egui::Color32::WHITE).default_size(180.0),
        ShelfDef::new("bottom", ShelfEdge::Bottom, egui::Color32::WHITE).default_size(160.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 200.0);
    assert_eq!(layout.right.unwrap().width(), 180.0);
    assert_eq!(layout.bottom.unwrap().height(), 160.0);
    assert_eq!(layout.viewport.min, pos2(200.0, 0.0));
    assert_eq!(layout.viewport.max, pos2(820.0, 640.0));
    assert_eq!(shelf_insets(layout), egui::vec2(380.0, 160.0));
    assert_eq!(layout.available(), available);
}

#[test]
fn shelf_layout_is_canonical_not_declaration_ordered() {
    let theme = *style::theme().shelf();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let mut state = ShelfState::default();
    let shelves = vec![
        ShelfDef::new("bottom", ShelfEdge::Bottom, egui::Color32::WHITE).default_size(160.0),
        ShelfDef::new("right", ShelfEdge::Right, egui::Color32::WHITE).default_size(180.0),
        ShelfDef::new("left", ShelfEdge::Left, egui::Color32::WHITE).default_size(200.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(
        layout.left.unwrap(),
        Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 800.0))
    );
    assert_eq!(
        layout.right.unwrap(),
        Rect::from_min_max(pos2(820.0, 0.0), pos2(1000.0, 800.0))
    );
    assert_eq!(
        layout.bottom.unwrap(),
        Rect::from_min_max(pos2(200.0, 640.0), pos2(820.0, 800.0))
    );
    assert_eq!(layout.viewport.min, pos2(200.0, 0.0));
    assert_eq!(layout.viewport.max, pos2(820.0, 640.0));
}

#[test]
fn shelf_layout_clamps_oversized_shelves_to_available_space() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(300.0, 200.0));
    let shelves = vec![
        ShelfDef::new("left", ShelfEdge::Left, egui::Color32::WHITE).default_size(260.0),
        ShelfDef::new("right", ShelfEdge::Right, egui::Color32::WHITE).default_size(260.0),
        ShelfDef::new("bottom", ShelfEdge::Bottom, egui::Color32::WHITE).default_size(260.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(available.contains(layout.left.unwrap().min));
    assert!(available.contains(layout.left.unwrap().max));
    assert!(available.contains(layout.right.unwrap().min));
    assert!(available.contains(layout.right.unwrap().max));
    assert!(available.contains(layout.bottom.unwrap().min));
    assert!(available.contains(layout.bottom.unwrap().max));
    assert!(layout.viewport.width() >= 0.0);
    assert!(layout.viewport.height() >= 0.0);
}

#[test]
fn shelf_layout_normalizes_invalid_size_bounds() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new("left", ShelfEdge::Left, egui::Color32::WHITE)
            .default_size(f32::NAN)
            .size_bounds(400.0, 200.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 300.0);
    assert_eq!(layout.viewport.min.x, 300.0);
}

#[test]
fn shelf_layout_sanitizes_persisted_invalid_size() {
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("left");
    let mut state = ShelfState::default();
    state.set_edge_size(shelf_id, ShelfEdge::Left, f32::NAN);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves =
        vec![ShelfDef::new(shelf_id, ShelfEdge::Left, egui::Color32::WHITE).default_size(240.0)];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 240.0);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), Some(240.0));
}

#[test]
fn duplicate_shelf_edges_reserve_space_once() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new("left.primary", ShelfEdge::Left, egui::Color32::WHITE).default_size(200.0),
        ShelfDef::new("left.secondary", ShelfEdge::Left, egui::Color32::WHITE).default_size(160.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 200.0);
    assert_eq!(layout.viewport.min.x, 200.0);
    assert_eq!(layout.viewport.max.x, 1000.0);
}

#[test]
fn duplicate_shelf_ids_are_rejected() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new("duplicate", ShelfEdge::Left, egui::Color32::WHITE),
        ShelfDef::new("duplicate", ShelfEdge::Right, egui::Color32::WHITE),
    ];

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = layout_shelves(available, &shelves, &mut state, &theme);
    }));

    assert!(
        result.is_err(),
        "duplicate shelf ids would corrupt size, edge, move, and active-container state"
    );
}

#[test]
fn duplicate_shelf_container_ids_are_rejected() {
    let theme = *style::theme().shelf();
    let mut state = ShelfState::default();
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new("left", ShelfEdge::Left, egui::Color32::WHITE).container(
            ShelfContainer::tabbed(
                Id::new("duplicate-container"),
                "First",
                "settings",
                test_tabs(),
            ),
        ),
        ShelfDef::new("right", ShelfEdge::Right, egui::Color32::WHITE).container(
            ShelfContainer::tabbed(
                Id::new("duplicate-container"),
                "Second",
                "info",
                test_tabs(),
            ),
        ),
    ];

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = layout_shelves(available, &shelves, &mut state, &theme);
    }));

    assert!(
        result.is_err(),
        "duplicate shelf container ids would corrupt edge ownership and move/order state"
    );
}

#[test]
fn shelf_state_persists_size_and_active_container() {
    let shelf_id = Id::new("shelf");
    let container_id = Id::new("container");
    let mut state = ShelfState::default();

    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), None);
    state.set_edge_size(shelf_id, ShelfEdge::Left, 222.0);
    state.set_edge_size(shelf_id, ShelfEdge::Bottom, 144.0);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), Some(222.0));
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Bottom), Some(144.0));
    state.set_edge_size(shelf_id, ShelfEdge::Left, -10.0);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), Some(0.0));
    state.set_edge_size(shelf_id, ShelfEdge::Left, f32::NAN);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), None);

    assert_eq!(state.active_container(shelf_id), None);
    state.set_active_container(shelf_id, container_id);
    assert_eq!(state.active_container(shelf_id), Some(container_id));

    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Left);
    state.set_edge(shelf_id, ShelfEdge::Right);
    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Right);
    state.clear_edge_override(shelf_id);
    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Left);
}

#[test]
fn shelf_layout_uses_state_edge_override() {
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("movable");
    let mut state = ShelfState::default();
    state.set_edge(shelf_id, ShelfEdge::Bottom);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves =
        vec![ShelfDef::new(shelf_id, ShelfEdge::Left, egui::Color32::WHITE).default_size(200.0)];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert!(layout.bottom.is_some());
    assert_eq!(layout.bottom.unwrap().height(), 200.0);
    assert_eq!(layout.viewport.max.y, 600.0);
}

#[test]
fn shelf_layout_creates_edge_for_moved_container() {
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("movable");
    let container_id = Id::new("tools");
    let mut state = ShelfState::default();
    state.set_container_edge(container_id, ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(shelf_id, ShelfEdge::Left, egui::Color32::WHITE)
            .default_size(200.0)
            .container(ShelfContainer::tabbed(
                container_id,
                "Tools",
                "tools",
                test_tabs(),
            )),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert!(layout.right.is_some());
    assert_eq!(layout.right.unwrap().width(), 200.0);
    assert_eq!(layout.viewport.max.x, 800.0);
}

#[test]
fn shelf_layout_clears_edge_when_all_containers_move_out() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source");
    let target_shelf = Id::new("target");
    let container_id = Id::new("tools");
    let mut state = ShelfState::default();
    state.set_container_edge(container_id, ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, egui::Color32::WHITE)
            .default_size(200.0)
            .container(ShelfContainer::tabbed(
                container_id,
                "Tools",
                "tools",
                test_tabs(),
            )),
        ShelfDef::new(target_shelf, ShelfEdge::Right, egui::Color32::WHITE)
            .default_size(260.0)
            .container(ShelfContainer::tabbed(
                Id::new("target-container"),
                "Target",
                "box",
                test_tabs(),
            )),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(
        layout.left.is_none(),
        "source shelf should stop reserving space when every container moved away"
    );
    assert_eq!(layout.right.unwrap().width(), 260.0);
    assert_eq!(layout.viewport.min.x, 0.0);
    assert_eq!(layout.viewport.max.x, 740.0);
}

#[test]
fn shelf_layout_merges_edge_only_container_into_overridden_edge_shelf() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source");
    let target_shelf = Id::new("target");
    let container_id = Id::new("tools");
    let mut state = ShelfState::default();
    state.set_edge(target_shelf, ShelfEdge::Right);
    state.set_container_edge(container_id, ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, egui::Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(
                container_id,
                "Tools",
                "tools",
                test_tabs(),
            )),
        ShelfDef::new(target_shelf, ShelfEdge::Bottom, egui::Color32::WHITE).default_size(260.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert!(
        layout.bottom.is_none(),
        "target shelf was moved to the right edge, so bottom should not reserve space"
    );
    assert_eq!(layout.right.unwrap().width(), 260.0);
    assert_eq!(layout.viewport.max.x, 740.0);
}

#[test]
fn shelf_container_api_is_typed_tabbed_only() {
    let _container = ShelfContainer::tabbed(
        Id::new("tabbed"),
        "Inspector",
        "settings",
        vec![mara_core::container::Tab::new(
            "inspector.main",
            "Main",
            "settings",
        )],
    );
}

#[test]
fn shelf_container_api_rejects_empty_tabbed_containers() {
    let result = std::panic::catch_unwind(|| {
        let _ = ShelfContainer::tabbed(Id::new("empty"), "Empty", "settings", Vec::new());
    });

    assert!(result.is_err());
}
