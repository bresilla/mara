use super::*;
use crate::layout::ScrollAxis;

fn test_tabs() -> Vec<Tab> {
    vec![Tab::new("test.tab", "Tab", "box")]
}

fn shelf_with_container(id: &'static str, edge: ShelfEdge) -> ShelfDef<'static> {
    ShelfDef::new(Id::new(id), edge, Color32::WHITE).container(ShelfContainer::tabbed(
        Id::new(("container", id)),
        id,
        "box",
        test_tabs(),
    ))
}

fn test_shelf_layout(
    viewport: Rect,
    left: Option<Rect>,
    right: Option<Rect>,
    bottom: Option<Rect>,
) -> ShelfLayout {
    ShelfLayout {
        viewport: viewport.into(),
        left: left.map(Into::into),
        right: right.map(Into::into),
        bottom: bottom.map(Into::into),
    }
}

#[test]
fn shelf_body_child_region_uses_mara_stack_policy() {
    let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 140.0));

    let bottom_row = shelf_body_child_region(rect, true, ShelfEdge::Bottom);
    assert_eq!(bottom_row.rect, rect.into());
    assert_eq!(
        bottom_row.direction,
        crate::layout::StackDirection::LeftToRight
    );
    assert_eq!(bottom_row.align, StackAlign::Min);

    let side_column = shelf_body_child_region(rect, false, ShelfEdge::Left);
    assert_eq!(
        side_column.direction,
        crate::layout::StackDirection::TopDown
    );
    assert_eq!(side_column.align, StackAlign::Center);

    let bottom_column = shelf_body_child_region(rect, false, ShelfEdge::Bottom);
    assert_eq!(
        bottom_column.direction,
        crate::layout::StackDirection::TopDown
    );
    assert_eq!(bottom_column.align, StackAlign::Min);
}

#[test]
fn shelf_body_scroll_region_uses_axis_and_extent_from_layout() {
    let pane_id = Id::new("shelf-pane");
    let rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(300.0, 140.0));

    let horizontal = shelf_body_scroll_region(pane_id, rect, true);
    assert_eq!(horizontal.axis, ScrollAxis::Horizontal);
    assert_eq!(horizontal.auto_shrink, [false, false]);
    assert_eq!(horizontal.max_extent, 300.0);
    assert_eq!(horizontal.item_spacing, MaraVec2::ZERO);

    let vertical = shelf_body_scroll_region(pane_id, rect, false);
    assert_eq!(vertical.axis, ScrollAxis::Vertical);
    assert_eq!(vertical.auto_shrink, [false, false]);
    assert_eq!(vertical.max_extent, 140.0);
    assert_eq!(vertical.item_spacing, MaraVec2::ZERO);
}

#[test]
fn collapse_bottom_merges_into_existing_right_shelf() {
    let shelves = vec![
        shelf_with_container("left", ShelfEdge::Left),
        shelf_with_container("right", ShelfEdge::Right),
        shelf_with_container("bottom", ShelfEdge::Bottom),
    ];

    let collapsed = collapse_bottom_into_right(shelves);

    // Only left + right remain; no bottom edge.
    assert_eq!(collapsed.len(), 2);
    assert!(collapsed.iter().all(|s| s.edge != ShelfEdge::Bottom));
    let right = collapsed
        .iter()
        .find(|s| s.edge == ShelfEdge::Right)
        .expect("right shelf survives");
    // Right shelf now owns its own container plus the bottom's.
    assert_eq!(right.containers.len(), 2);
}

#[test]
fn collapse_bottom_promotes_when_no_right_shelf() {
    let shelves = vec![
        shelf_with_container("left", ShelfEdge::Left),
        shelf_with_container("bottom", ShelfEdge::Bottom),
    ];

    let collapsed = collapse_bottom_into_right(shelves);

    assert_eq!(collapsed.len(), 2);
    assert!(collapsed.iter().all(|s| s.edge != ShelfEdge::Bottom));
    let right = collapsed
        .iter()
        .find(|s| s.edge == ShelfEdge::Right)
        .expect("bottom shelf promoted to right");
    assert_eq!(right.containers.len(), 1);
}

#[test]
fn collapse_bottom_merges_multiple_bottoms() {
    let shelves = vec![
        shelf_with_container("bottom_a", ShelfEdge::Bottom),
        shelf_with_container("bottom_b", ShelfEdge::Bottom),
    ];

    let collapsed = collapse_bottom_into_right(shelves);

    // First bottom promoted to right, second merged in.
    assert_eq!(collapsed.len(), 1);
    assert_eq!(collapsed[0].edge, ShelfEdge::Right);
    assert_eq!(collapsed[0].containers.len(), 2);
}

#[test]
fn collapse_bottom_is_noop_without_bottom() {
    let shelves = vec![
        shelf_with_container("left", ShelfEdge::Left),
        shelf_with_container("right", ShelfEdge::Right),
    ];

    let collapsed = collapse_bottom_into_right(shelves);
    assert_eq!(collapsed.len(), 2);
}

#[test]
fn container_move_preserves_existing_shelf_slot_while_drag_continues() {
    let mut state = ShelfState::default();
    let container_id = Id::new("dragged");
    let pane_id = Id::new("target-pane");

    state.update_container_move(ShelfContainerMoveUpdate {
        container_id,
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(10.0, 10.0),
        target_edge: Some(ShelfEdge::Right),
        container_size: vec2(120.0, 240.0),
    });
    state.update_container_move_target_slot(
        Id::new("target-shelf"),
        pane_id,
        2,
        vec2(120.0, 240.0),
    );

    state.update_container_move(ShelfContainerMoveUpdate {
        container_id,
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(20.0, 20.0),
        target_edge: Some(ShelfEdge::Right),
        container_size: vec2(120.0, 240.0),
    });

    let drag = state
        .container_move
        .expect("container drag should continue");
    assert_eq!(drag.target_pane, Some(pane_id));
    assert_eq!(drag.target_slot, Some(2));
}

#[test]
fn container_move_target_slot_adopts_target_shelf_size() {
    let mut state = ShelfState::default();
    let container_id = Id::new("dragged");
    let pane_id = Id::new("target-pane");

    state.update_container_move(ShelfContainerMoveUpdate {
        container_id,
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(10.0, 10.0),
        target_edge: Some(ShelfEdge::Bottom),
        container_size: vec2(120.0, 240.0),
    });
    state.update_container_move_target_slot(Id::new("target-shelf"), pane_id, 1, vec2(360.0, 96.0));

    let drag = state
        .container_move
        .expect("container drag should be tracked");
    assert_eq!(drag.target_slot, Some(1));
    assert_eq!(drag.container_size, vec2(360.0, 96.0));
}

#[test]
fn external_container_gap_flag_is_frame_local() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("target-pane");

    mark_external_container_gap(&ctx, pane_id);
    assert!(external_container_gap_was_painted(&ctx, pane_id));

    clear_external_container_gap(&ctx, pane_id);
    assert!(!external_container_gap_was_painted(&ctx, pane_id));
}

#[test]
fn published_shelf_pane_info_is_cleared_before_shelf_render() {
    let ctx = egui::Context::default();
    let info = ShelfPaneInfo {
        shelf_id: Id::new("stale-shelf"),
        pane_id: Id::new("stale-pane"),
        edge: ShelfEdge::Left,
        horizontal_stack: false,
        content_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)),
        screen_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 100.0)),
        screen_offset: Vec2::ZERO,
        accent: Color32::WHITE,
    };

    publish_shelf_pane_info(&ctx, info);
    assert_eq!(shelf_pane_info(&ctx, ShelfEdge::Left), Some(info));

    clear_published_shelf_pane_infos(&ctx);
    assert_eq!(shelf_pane_info(&ctx, ShelfEdge::Left), None);
}

#[test]
fn publish_shelf_layout_sets_chrome_bounds_to_reserved_viewport() {
    let ctx = egui::Context::default();
    let viewport = Rect::from_min_max(pos2(200.0, 0.0), pos2(900.0, 640.0));
    let layout = test_shelf_layout(
        viewport,
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 640.0))),
        None,
        Some(Rect::from_min_max(pos2(200.0, 640.0), pos2(900.0, 800.0))),
    );

    __internal_publish_shelf_layout(&ctx, layout);

    let chrome = ctx
        .data(|d| d.get_temp::<MaraRect>(egui::Id::from(crate::ribbon::chrome::chrome_bounds_key())))
        .expect("shelf layout should publish ribbon chrome bounds");
    assert_eq!(chrome, viewport.into());
    assert_eq!(__internal_shelf_layout(&ctx), Some(layout));
    assert_eq!(
        published_shelf_presence(&ctx),
        ShelfPresence {
            left: true,
            right: false,
            bottom: true
        }
    );
}

#[test]
fn show_shelves_publishes_hidden_shelf_presence_for_top_bar_buttons() {
    let ctx = egui::Context::default();
    let shelf_id = Id::new("hidden-left");
    let mut state = ShelfState::default();
    state.set_edge_visible(ShelfEdge::Left, false);
    let shelves =
        vec![
            ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(Id::new("container"), "Tools", "box", test_tabs()),
            ),
        ];
    let layout = layout_shelves(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0)),
        &shelves,
        &mut state,
        style::theme().shelf(),
    );
    assert!(layout.left.is_none());

    __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);

    assert_eq!(
        published_shelf_presence(&ctx),
        ShelfPresence {
            left: true,
            right: false,
            bottom: false
        },
        "a hidden declared shelf should keep its top-bar toggle available"
    );
}

#[test]
fn show_shelves_respects_shelf_toggle_button_opt_out() {
    let shelf_id = Id::new("fixed-left");
    let mut state = ShelfState::default();
    let shelves = vec![
        ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE)
            .without_toggle_button()
            .container(ShelfContainer::tabbed(
                Id::new("container"),
                "Tools",
                "box",
                test_tabs(),
            )),
    ];
    let layout = layout_shelves(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0)),
        &shelves,
        &mut state,
        style::theme().shelf(),
    );
    assert!(layout.left.is_some());

    assert_eq!(
        shelf_presence_for(&shelves, &state),
        ShelfPresence::default(),
        "a shelf that opts out must not publish a hide/show top-bar button"
    );
}

#[test]
fn side_shelf_content_is_lowered_but_shelf_rect_keeps_full_height() {
    let shelf_rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 600.0));
    let theme = *style::theme().shelf();

    let paint = shelf_paint_rect(ShelfEdge::Left, shelf_rect);
    let content = shelf_content_rect(ShelfEdge::Left, shelf_rect, &theme);

    assert_eq!(shelf_rect.top(), 0.0, "layout keeps the shelf full-height");
    assert_eq!(
        paint, shelf_rect,
        "background paints full-height so the top ribbon strip matches the shelf body"
    );
    assert!(
        content.top() > shelf_rect.top() + theme.padding,
        "containers/content start below the top ribbon"
    );
    assert_eq!(paint.bottom(), shelf_rect.bottom());
    assert_eq!(content.bottom(), shelf_rect.bottom() - theme.padding);
}

#[test]
fn bottom_shelf_content_is_not_lowered_for_top_ribbon() {
    let shelf_rect = Rect::from_min_max(pos2(0.0, 420.0), pos2(800.0, 600.0));
    let theme = *style::theme().shelf();

    assert_eq!(shelf_paint_rect(ShelfEdge::Bottom, shelf_rect), shelf_rect);
    assert_eq!(
        shelf_content_rect(ShelfEdge::Bottom, shelf_rect, &theme),
        shelf_rect.shrink(theme.padding)
    );
}

#[test]
fn show_shelves_sets_public_active_container_for_default_visible_container() {
    let ctx = egui::Context::default();
    let shelf_id = Id::new("active-shelf");
    let container_id = Id::new("visible-container");
    let theme = *style::theme().shelf();
    let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
    let shelves = vec![
        ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE)
            .default_size(220.0)
            .container(ShelfContainer::tabbed(
                container_id,
                "Visible",
                "box",
                test_tabs(),
            )),
    ];
    let mut state = ShelfState::default();
    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(available),
        ..Default::default()
    });
    __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
    let _ = ctx.end_pass();

    assert_eq!(
        state.active_container(shelf_id),
        Some(container_id),
        "the public shelf state should mirror the visible default active container"
    );
}

#[test]
fn show_shelves_repairs_stale_public_active_container_from_rendered_group() {
    let ctx = egui::Context::default();
    let shelf_id = Id::new("active-shelf");
    let visible_container = Id::new("visible-container");
    let stale_container = Id::new("removed-container");
    let edge = ShelfEdge::Left;
    let theme = *style::theme().shelf();
    let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
    let shelves = vec![
        ShelfDef::new(shelf_id, edge, Color32::WHITE)
            .default_size(220.0)
            .container(ShelfContainer::tabbed(
                visible_container,
                "Visible",
                "box",
                test_tabs(),
            )),
    ];
    let mut state = ShelfState::default();
    state.set_active_container(shelf_id, stale_container);
    state.set_active_container_for_group(
        shelf_active_container_key_for(shelf_id, edge),
        visible_container,
    );
    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(available),
        ..Default::default()
    });
    __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
    let _ = ctx.end_pass();

    assert_eq!(
        state.active_container(shelf_id),
        Some(visible_container),
        "public shelf active state should be repaired from the visible rendered group"
    );
}

#[test]
fn show_shelves_clears_active_container_when_no_container_is_visible() {
    let ctx = egui::Context::default();
    let shelf_id = Id::new("empty-shelf");
    let stale_container = Id::new("removed-container");
    let edge = ShelfEdge::Left;
    let theme = *style::theme().shelf();
    let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(640.0, 480.0));
    let shelves = vec![ShelfDef::new(shelf_id, edge, Color32::WHITE).default_size(220.0)];
    let mut state = ShelfState::default();
    state.set_active_container(shelf_id, stale_container);
    state.set_active_container_for_group(
        shelf_active_container_key_for(shelf_id, edge),
        stale_container,
    );
    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    ctx.begin_pass(egui::RawInput {
        screen_rect: Some(available),
        ..Default::default()
    });
    __internal_show_shelves_egui(&ctx, layout, shelves, &mut state);
    let _ = ctx.end_pass();

    assert_eq!(
        state.active_container(shelf_id),
        None,
        "empty shelves must not keep stale public active-container state"
    );
    assert_eq!(
        state.active_container_for_group(shelf_active_container_key_for(shelf_id, edge)),
        None,
        "empty rendered shelf groups must not keep stale active-container state"
    );
}

#[test]
fn commit_container_move_inserts_into_target_pane_order() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let source_pane = Id::new("source-pane");
    let target_shelf = Id::new("target-shelf");
    let source_shelf = Id::new("source-shelf");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let mut state = ShelfState::default();
    state.set_active_container(source_shelf, dragged);
    state.set_active_container_for_group(
        shelf_active_container_key_for(source_shelf, ShelfEdge::Left),
        dragged,
    );
    pane::set_drag(
        &ctx,
        target_pane,
        pane::DragState {
            item: Some(dragged),
            cursor: Some(pos2(0.0, 0.0)),
        },
    );
    pane::set_drag(
        &ctx,
        source_pane,
        pane::DragState {
            item: Some(dragged),
            cursor: Some(pos2(0.0, 0.0)),
        },
    );
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
                frame: None,
            },
        ],
    );

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf,
            source_pane,
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(target_shelf),
            target_pane: Some(target_pane),
            target_slot: Some(1),
            container_size: vec2(100.0, 80.0),
        },
    );

    assert_eq!(
        state.container_edge(dragged, ShelfEdge::Left),
        ShelfEdge::Right
    );
    assert_eq!(
        state.container_location(dragged, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(target_shelf),
            edge: ShelfEdge::Right,
        },
        "committing into an existing shelf should adopt that shelf owner, not only the edge"
    );
    assert_eq!(
        state.active_container(target_shelf),
        Some(dragged),
        "the receiving shelf should select the container that was just moved into it"
    );
    assert_eq!(
        state.active_container_for_group(shelf_active_container_key_for(
            target_shelf,
            ShelfEdge::Right
        )),
        Some(dragged),
        "the receiving rendered shelf group should select the moved container immediately"
    );
    assert_eq!(
        state.active_container(source_shelf),
        None,
        "the source shelf must not keep a moved-away container as its public active container"
    );
    assert_eq!(
        state.active_container_for_group(shelf_active_container_key_for(
            source_shelf,
            ShelfEdge::Left
        )),
        None,
        "the source rendered shelf group must not keep a moved-away container selected"
    );
    assert_eq!(
        pane::section_order_for(&ctx, target_pane, &[first, dragged, second]),
        vec![first, dragged, second]
    );
    assert!(
        pane::drag_state(&ctx, target_pane).item.is_none(),
        "committing into a target shelf should clear target-pane drag state so the ghost cannot stick"
    );
    assert!(
        pane::drag_state(&ctx, source_pane).item.is_none(),
        "committing into a target shelf should also clear source-pane drag state"
    );
}

#[test]
fn commit_container_move_inserts_into_bottom_target_order() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("bottom-target-pane");
    let target_shelf = Id::new("bottom-target-shelf");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    let mut state = ShelfState::default();
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(120.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(140.0, 0.0), vec2(120.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: third,
                rect: Rect::from_min_size(pos2(280.0, 0.0), vec2(120.0, 80.0)),
                frame: None,
            },
        ],
    );

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Bottom),
            target_shelf: Some(target_shelf),
            target_pane: Some(target_pane),
            target_slot: Some(2),
            container_size: vec2(120.0, 80.0),
        },
    );

    assert_eq!(
        state.container_location(dragged, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(target_shelf),
            edge: ShelfEdge::Bottom,
        }
    );
    assert_eq!(
        pane::section_order_for(&ctx, target_pane, &[first, second, dragged, third]),
        vec![first, second, dragged, third]
    );
}

#[test]
fn commit_container_move_clamps_oversized_target_slot_to_end() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let mut state = ShelfState::default();
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
                frame: None,
            },
        ],
    );

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(usize::MAX),
            container_size: vec2(100.0, 80.0),
        },
    );

    assert_eq!(
        pane::section_order_for(&ctx, target_pane, &[first, second, dragged]),
        vec![first, second, dragged],
        "a stale/oversized slot should clamp to the end instead of corrupting order"
    );
}

#[test]
fn commit_container_move_deduplicates_existing_target_order_entry() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let mut state = ShelfState::default();
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: dragged,
                rect: Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(0.0, 180.0), vec2(100.0, 80.0)),
                frame: None,
            },
        ],
    );
    pane::set_section_order(&ctx, target_pane, vec![first, dragged, second]);

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(0),
            container_size: vec2(100.0, 80.0),
        },
    );

    assert_eq!(
        pane::section_order_for(&ctx, target_pane, &[dragged, first, second]),
        vec![dragged, first, second],
        "moving an already-known container should reposition it without duplicating the id"
    );
}

#[test]
fn commit_container_move_uses_live_target_cache_for_trailing_slot() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    let mut state = ShelfState::default();
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
                frame: None,
            },
        ],
    );
    pane::push_rect(
        &ctx,
        target_pane,
        first,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
    );
    pane::push_rect(
        &ctx,
        target_pane,
        second,
        Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
    );
    pane::push_rect(
        &ctx,
        target_pane,
        third,
        Rect::from_min_size(pos2(0.0, 180.0), vec2(100.0, 80.0)),
    );

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(3),
            container_size: vec2(100.0, 80.0),
        },
    );

    assert_eq!(
        pane::section_order_for(&ctx, target_pane, &[first, second, third, dragged]),
        vec![first, second, third, dragged],
        "committing after the last live-rendered container must not fall back to a stale shorter snapshot"
    );
}

#[test]
fn same_shelf_reorder_uses_live_cache_for_trailing_slot() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    pane::set_snapshot(
        &ctx,
        pane_id,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
                frame: None,
            },
            pane::RectEntry {
                id: dragged,
                rect: Rect::from_min_size(pos2(0.0, 180.0), vec2(100.0, 80.0)),
                frame: None,
            },
        ],
    );
    pane::set_section_order(&ctx, pane_id, vec![first, second, dragged]);
    pane::push_rect(
        &ctx,
        pane_id,
        first,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 80.0)),
    );
    pane::push_rect(
        &ctx,
        pane_id,
        second,
        Rect::from_min_size(pos2(0.0, 90.0), vec2(100.0, 80.0)),
    );
    pane::push_rect(
        &ctx,
        pane_id,
        third,
        Rect::from_min_size(pos2(0.0, 180.0), vec2(100.0, 80.0)),
    );

    commit_shelf_container_reorder(&ctx, pane_id, dragged, 260.0, false);

    assert_eq!(
        pane::section_order_for(&ctx, pane_id, &[first, second, third, dragged]),
        vec![first, second, third, dragged],
        "same-shelf reorder commit must not drop live-rendered containers that were absent from the stale snapshot"
    );
}

#[test]
fn commit_adopted_container_to_new_edge_keeps_current_shelf_owner() {
    let ctx = egui::Context::default();
    let adopted_shelf = Id::new("adopted-shelf");
    let original_shelf = Id::new("original-shelf");
    let dragged = Id::new("dragged");
    let mut state = ShelfState::default();
    state.set_container_location(dragged, Some(adopted_shelf), ShelfEdge::Right);

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: adopted_shelf,
            source_pane: Id::new("adopted-pane"),
            source_edge: ShelfEdge::Right,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Bottom),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(120.0, 80.0),
        },
    );

    assert_eq!(
        state.container_location(dragged, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(adopted_shelf),
            edge: ShelfEdge::Bottom,
        },
        "moving an already-adopted container to a new edge should keep the shelf it was dragged from"
    );

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(original_shelf, ShelfEdge::Left, Color32::WHITE)
                .container(ShelfContainer::tabbed(dragged, "Moved", "box", test_tabs())),
            ShelfDef::new(adopted_shelf, ShelfEdge::Right, Color32::WHITE),
        ],
        &state,
    );

    assert!(groups.iter().any(|group| {
        group.id == adopted_shelf
            && group.edge == ShelfEdge::Bottom
            && group
                .containers
                .iter()
                .any(|container| container.spec.container_id() == dragged)
    }));
}

#[test]
fn missing_published_target_clears_stale_container_move_slot() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("old-target-pane");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: Id::new("dragged"),
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("old-target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(2),
            container_size: vec2(100.0, 80.0),
        }),
        ..Default::default()
    };

    clear_published_shelf_pane_infos(&ctx);
    update_container_move_target_from_published(&ctx, &mut state);

    let drag = state
        .container_move
        .expect("container move should remain active");
    assert_eq!(drag.target_edge, Some(ShelfEdge::Right));
    assert_eq!(drag.target_shelf, None);
    assert_eq!(drag.target_pane, None);
    assert_eq!(drag.target_slot, None);
}

#[test]
fn published_target_clears_slot_when_cursor_left_target_shelf_rect() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("old-target-pane");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: Id::new("dragged"),
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Right,
            cursor: pos2(300.0, 200.0),
            target_edge: Some(ShelfEdge::Left),
            target_shelf: Some(Id::new("old-target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(2),
            container_size: vec2(100.0, 80.0),
        }),
        ..Default::default()
    };
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: Id::new("left-shelf"),
            pane_id: target_pane,
            edge: ShelfEdge::Left,
            horizontal_stack: false,
            content_rect: Rect::from_min_size(pos2(8.0, 8.0), vec2(120.0, 400.0)),
            screen_rect: Rect::from_min_size(pos2(0.0, 0.0), vec2(140.0, 420.0)),
            screen_offset: Vec2::ZERO,
            accent: Color32::WHITE,
        },
    );

    update_container_move_target_from_published(&ctx, &mut state);

    let drag = state
        .container_move
        .expect("container move should remain active");
    assert_eq!(drag.target_edge, Some(ShelfEdge::Left));
    assert_eq!(
        drag.target_pane, None,
        "stale target shelf slots must be cleared when the cursor is in the canvas, not over the shelf"
    );
    assert_eq!(drag.target_slot, None);
}

#[test]
fn published_existing_shelf_target_tracks_middle_container_slot() {
    let ctx = egui::Context::default();
    let target_shelf = Id::new("target-shelf");
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(120.0, 150.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(80.0, 120.0),
        }),
        ..Default::default()
    };
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 100.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(100.0, 140.0), vec2(80.0, 100.0)),
                frame: None,
            },
            pane::RectEntry {
                id: third,
                rect: Rect::from_min_size(pos2(100.0, 260.0), vec2(80.0, 100.0)),
                frame: None,
            },
        ],
    );
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: target_shelf,
            pane_id: target_pane,
            edge: ShelfEdge::Right,
            horizontal_stack: false,
            content_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 400.0)),
            screen_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 400.0)),
            screen_offset: Vec2::ZERO,
            accent: Color32::WHITE,
        },
    );

    update_container_move_target_from_published(&ctx, &mut state);

    let drag = state
        .container_move
        .expect("container move should keep tracking target shelf");
    assert_eq!(drag.target_shelf, Some(target_shelf));
    assert_eq!(drag.target_pane, Some(target_pane));
    assert_eq!(
        drag.target_slot,
        Some(1),
        "cursor between first and second target containers should place the ghost in the middle"
    );
}

#[test]
fn published_existing_shelf_target_prefers_live_rendered_container_positions() {
    let ctx = egui::Context::default();
    let target_shelf = Id::new("target-shelf");
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Right,
            cursor: pos2(120.0, 250.0),
            target_edge: Some(ShelfEdge::Left),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(80.0, 120.0),
        }),
        ..Default::default()
    };
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 100.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(100.0, 140.0), vec2(80.0, 100.0)),
                frame: None,
            },
        ],
    );
    pane::push_rect(
        &ctx,
        target_pane,
        first,
        Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 100.0)),
    );
    pane::push_rect(
        &ctx,
        target_pane,
        second,
        Rect::from_min_size(pos2(100.0, 140.0), vec2(80.0, 100.0)),
    );
    pane::push_rect(
        &ctx,
        target_pane,
        third,
        Rect::from_min_size(pos2(100.0, 260.0), vec2(80.0, 100.0)),
    );
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: target_shelf,
            pane_id: target_pane,
            edge: ShelfEdge::Left,
            horizontal_stack: false,
            content_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 400.0)),
            screen_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 400.0)),
            screen_offset: Vec2::ZERO,
            accent: Color32::WHITE,
        },
    );

    update_container_move_target_from_published(&ctx, &mut state);

    let drag = state
        .container_move
        .expect("container move should keep tracking the live target shelf");
    assert_eq!(
        drag.target_slot,
        Some(2),
        "existing-shelf drops must use this frame's live rendered positions, not a stale two-container snapshot"
    );
    let (rect, _) = existing_shelf_container_slot_ghost(&ctx, ShelfEdge::Left, drag)
        .expect("existing shelf slot ghost should use the live target slot");
    assert_eq!(
        rect.min,
        pos2(96.0, 260.0),
        "the foreground ghost should keep the target slot's main-axis position while filling the shelf cross-axis"
    );
}

#[test]
fn existing_shelf_slot_ghost_translates_local_shelf_rects_to_screen_space() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let target = Id::new("target");
    let screen_offset = vec2(480.0, 360.0);
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![pane::RectEntry {
            id: target,
            rect: Rect::from_min_size(pos2(96.0, 140.0), vec2(80.0, 100.0)),
            frame: None,
        }],
    );
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: Id::new("target-shelf"),
            pane_id: target_pane,
            edge: ShelfEdge::Right,
            horizontal_stack: false,
            content_rect: Rect::from_min_size(pos2(80.0, 16.0), vec2(120.0, 260.0)),
            screen_rect: Rect::from_min_size(pos2(560.0, 376.0), vec2(120.0, 260.0)),
            screen_offset,
            accent: Color32::WHITE,
        },
    );

    let (rect, _) = existing_shelf_container_slot_ghost(
        &ctx,
        ShelfEdge::Right,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(0),
            container_size: vec2(80.0, 100.0),
        },
    )
    .expect("existing shelf slot ghost should be computed");

    assert_eq!(
        rect.min,
        pos2(576.0, 500.0),
        "foreground ghost areas are positioned in screen space, so local shelf geometry must be translated by the shelf area's screen offset"
    );
}

#[test]
fn container_move_target_stays_in_source_shelf_when_cursor_is_inside_screen_shelf_rect() {
    let available = Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0));
    let layout = test_shelf_layout(available, None, None, None);
    let local_shelf = Rect::from_min_size(pos2(0.0, 0.0), vec2(200.0, 600.0));
    let screen_shelf = local_shelf.translate(vec2(500.0, 0.0));
    let cursor = pos2(690.0, 120.0);

    assert_eq!(
        container_move_target(cursor, available, ShelfEdge::Left),
        Some(ShelfEdge::Right),
        "without source-shelf containment, this cursor is close enough to the right edge to start an external move"
    );
    assert_eq!(
        container_move_target_for_cursor(cursor, screen_shelf, layout, ShelfEdge::Left),
        None,
        "starting/holding a container drag inside its current shelf must keep the ghost in that shelf even when the shelf UI uses local coordinates"
    );
}

#[test]
fn container_move_target_does_not_snap_to_existing_left_shelf_from_canvas_band() {
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(120.0, 0.0), pos2(800.0, 600.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(120.0, 600.0))),
        None,
        None,
    );
    let source_shelf = Rect::from_min_max(pos2(680.0, 0.0), pos2(800.0, 600.0));
    let cursor = pos2(165.0, 300.0);

    assert_eq!(
        container_move_target(cursor, layout.available().into(), ShelfEdge::Right),
        Some(ShelfEdge::Left),
        "the old broad edge-band logic snapped to the existing left shelf even from the canvas"
    );
    assert_eq!(
        container_move_target_for_cursor(cursor, source_shelf, layout, ShelfEdge::Right),
        None,
        "existing shelves should only be targeted when the cursor is actually over that shelf, not merely near the window edge"
    );
}

#[test]
fn shelf_target_cache_prefers_live_rects_but_keeps_dragged_geometry() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let live = Id::new("live");
    let stale = Id::new("stale");
    let dragged_rect = Rect::from_min_size(pos2(10.0, 20.0), vec2(90.0, 60.0));
    pane::set_snapshot(
        &ctx,
        pane_id,
        vec![
            pane::RectEntry {
                id: dragged,
                rect: dragged_rect,
                frame: Some(dragged_rect),
            },
            pane::RectEntry {
                id: stale,
                rect: Rect::from_min_size(pos2(100.0, 100.0), vec2(20.0, 20.0)),
                frame: None,
            },
        ],
    );
    pane::set_drag(
        &ctx,
        pane_id,
        pane::DragState {
            item: Some(dragged),
            cursor: Some(pos2(30.0, 40.0)),
        },
    );
    pane::begin_drag_frame(&ctx, pane_id);
    pane::push_rect(
        &ctx,
        pane_id,
        live,
        Rect::from_min_size(pos2(0.0, 0.0), vec2(10.0, 10.0)),
    );

    let cache = shelf_target_cache(&ctx, pane_id);

    assert!(cache.iter().any(|entry| entry.id == live));
    assert!(
        cache
            .iter()
            .any(|entry| entry.id == dragged && entry.rect == dragged_rect),
        "shelf drag previews still need the dragged container's carried full geometry"
    );
    assert!(
        !cache.iter().any(|entry| entry.id == stale),
        "shelf targeting must not resurrect removed containers from stale snapshots"
    );
}

#[test]
fn existing_shelf_slot_foreground_ghost_is_suppressed_when_inline_gap_was_marked() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("target-pane");
    let dragged = Id::new("dragged");
    let accent = Color32::from_rgb(10, 140, 220);
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: Id::new("first"),
                rect: Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 100.0)),
                frame: None,
            },
            pane::RectEntry {
                id: Id::new("second"),
                rect: Rect::from_min_size(pos2(100.0, 140.0), vec2(80.0, 100.0)),
                frame: None,
            },
        ],
    );
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: Id::new("target-shelf"),
            pane_id: target_pane,
            edge: ShelfEdge::Right,
            horizontal_stack: false,
            content_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 260.0)),
            screen_rect: Rect::from_min_size(pos2(96.0, 16.0), vec2(120.0, 260.0)),
            screen_offset: Vec2::ZERO,
            accent,
        },
    );
    mark_external_container_gap(&ctx, target_pane);

    let ghost = existing_shelf_container_slot_ghost(
        &ctx,
        ShelfEdge::Right,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(120.0, 150.0),
            target_edge: Some(ShelfEdge::Right),
            target_shelf: Some(Id::new("target-shelf")),
            target_pane: Some(target_pane),
            target_slot: Some(1),
            container_size: vec2(80.0, 100.0),
        },
    );

    assert!(
        ghost.is_none(),
        "when the target shelf already painted the inline layout gap, do not paint a second foreground destination ghost"
    );
}

#[test]
fn published_bottom_shelf_target_tracks_horizontal_middle_slot() {
    let ctx = egui::Context::default();
    let target_pane = Id::new("bottom-target-pane");
    let dragged = Id::new("dragged");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(300.0, 520.0),
            target_edge: Some(ShelfEdge::Bottom),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(160.0, 120.0),
        }),
        ..Default::default()
    };
    pane::set_snapshot(
        &ctx,
        target_pane,
        vec![
            pane::RectEntry {
                id: first,
                rect: Rect::from_min_size(pos2(24.0, 500.0), vec2(160.0, 120.0)),
                frame: None,
            },
            pane::RectEntry {
                id: second,
                rect: Rect::from_min_size(pos2(204.0, 500.0), vec2(160.0, 120.0)),
                frame: None,
            },
            pane::RectEntry {
                id: third,
                rect: Rect::from_min_size(pos2(384.0, 500.0), vec2(160.0, 120.0)),
                frame: None,
            },
        ],
    );
    publish_shelf_pane_info(
        &ctx,
        ShelfPaneInfo {
            shelf_id: Id::new("bottom-shelf"),
            pane_id: target_pane,
            edge: ShelfEdge::Bottom,
            horizontal_stack: true,
            content_rect: Rect::from_min_size(pos2(16.0, 492.0), vec2(720.0, 144.0)),
            screen_rect: Rect::from_min_size(pos2(16.0, 492.0), vec2(720.0, 144.0)),
            screen_offset: Vec2::ZERO,
            accent: Color32::WHITE,
        },
    );

    update_container_move_target_from_published(&ctx, &mut state);

    let drag = state
        .container_move
        .expect("bottom shelf container move should stay active");
    assert_eq!(
        drag.target_slot,
        Some(2),
        "bottom shelf containers flow horizontally, so x-position should choose the middle slot"
    );
    let (rect, _) = existing_shelf_container_slot_ghost(&ctx, ShelfEdge::Bottom, drag)
        .expect("bottom shelf slot ghost should be computed");
    assert_eq!(rect.min, pos2(384.0, 492.0));
    assert_eq!(rect.height(), 144.0);
}

#[test]
fn external_container_gap_ignores_stale_target_shelf_owner() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(0.0, 0.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: Some(Id::new("stale-target-owner")),
        target_pane: Some(Id::new("stale-target-pane")),
        target_slot: Some(4),
        container_size: vec2(100.0, 80.0),
    };

    assert!(should_render_external_container_gap(
        pane::DragState::default(),
        Some(drag),
        ShelfEdge::Right,
        Rect::from_min_size(pos2(-10.0, -10.0), vec2(40.0, 40.0)),
        None,
    ));
    assert!(!should_render_external_container_gap(
        pane::DragState::default(),
        Some(drag),
        ShelfEdge::Right,
        Rect::from_min_size(pos2(100.0, 100.0), vec2(40.0, 40.0)),
        None,
    ));
}

#[test]
fn external_container_gap_uses_current_pointer_not_stale_drag_cursor() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Right,
        cursor: pos2(40.0, 100.0),
        target_edge: Some(ShelfEdge::Left),
        target_shelf: Some(Id::new("left-shelf")),
        target_pane: Some(Id::new("left-pane")),
        target_slot: Some(0),
        container_size: vec2(100.0, 80.0),
    };
    let left_shelf_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(140.0, 600.0));

    assert!(
        should_render_external_container_gap(
            pane::DragState::default(),
            Some(drag),
            ShelfEdge::Left,
            left_shelf_rect,
            None,
        ),
        "without a live pointer, the helper falls back to the stored drag cursor"
    );
    assert!(
        !should_render_external_container_gap(
            pane::DragState::default(),
            Some(drag),
            ShelfEdge::Left,
            left_shelf_rect,
            Some(pos2(260.0, 300.0)),
        ),
        "a current pointer in the canvas must suppress a stale left-shelf gap"
    );
}

#[test]
fn source_container_gap_is_suppressed_during_cross_shelf_drag() {
    let dragged = Id::new("dragged");
    let source_pane = Id::new("right-pane");
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(140.0, 0.0), pos2(680.0, 520.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(140.0, 520.0))),
        Some(Rect::from_min_max(pos2(680.0, 0.0), pos2(800.0, 520.0))),
        None,
    );
    let right_shelf_rect = layout.right.expect("right shelf");

    assert!(should_suppress_source_container_gap(
        pane::DragState {
            item: Some(dragged),
            cursor: Some(pos2(720.0, 200.0)),
        },
        None,
        source_pane,
        ShelfEdge::Right,
        right_shelf_rect.into(),
        layout,
        Some(pos2(60.0, 200.0)),
    ));
    assert!(
        !should_suppress_source_container_gap(
            pane::DragState {
                item: Some(dragged),
                cursor: Some(pos2(720.0, 200.0)),
            },
            None,
            source_pane,
            ShelfEdge::Right,
            right_shelf_rect.into(),
            layout,
            Some(pos2(720.0, 200.0)),
        ),
        "normal same-shelf reorder still keeps the inline gap"
    );
}

#[test]
fn source_container_preview_is_suppressed_during_cross_shelf_drag() {
    let dragged = Id::new("dragged");
    let source_pane = Id::new("right-pane");
    let source_shelf = Id::new("right-shelf");
    let drag_state = pane::DragState {
        item: Some(dragged),
        cursor: Some(pos2(720.0, 200.0)),
    };

    assert!(!should_paint_source_container_preview(
        drag_state,
        Some(ShelfContainerMoveState {
            container_id: dragged,
            source_shelf,
            source_pane,
            source_edge: ShelfEdge::Right,
            cursor: pos2(60.0, 200.0),
            target_edge: Some(ShelfEdge::Left),
            target_shelf: Some(Id::new("left-shelf")),
            target_pane: Some(Id::new("left-pane")),
            target_slot: Some(0),
            container_size: vec2(100.0, 80.0),
        }),
        source_pane,
        ShelfEdge::Right,
    ));
    assert!(
        should_paint_source_container_preview(
            drag_state,
            Some(ShelfContainerMoveState {
                container_id: dragged,
                source_shelf,
                source_pane,
                source_edge: ShelfEdge::Right,
                cursor: pos2(720.0, 200.0),
                target_edge: None,
                target_shelf: None,
                target_pane: None,
                target_slot: None,
                container_size: vec2(100.0, 80.0),
            }),
            source_pane,
            ShelfEdge::Right,
        ),
        "same-source dragging/reordering can still paint the normal held-container preview"
    );
}

#[test]
fn source_shelf_gap_entry_reanchors_stale_right_shelf_rect() {
    let ctx = egui::Context::default();
    let dragged = Id::new("dragged");
    let right_content = Rect::from_min_size(pos2(1660.0, 80.0), vec2(280.0, 820.0));
    let stale_left_rect = pane::RectEntry {
        id: dragged,
        rect: Rect::from_min_size(pos2(16.0, 280.0), vec2(280.0, 160.0)),
        frame: None,
    };

    let fixed = source_shelf_gap_entry(
        &ctx,
        dragged,
        ShelfEdge::Right,
        right_content,
        stale_left_rect,
    );

    assert_eq!(fixed.rect.min, right_content.min);
    assert_eq!(fixed.rect.width(), right_content.width());
    assert!(right_content.contains(fixed.rect.center()));
}

#[test]
fn source_shelf_gap_entry_reanchors_stale_bottom_shelf_rect() {
    let ctx = egui::Context::default();
    let dragged = Id::new("dragged");
    let bottom_content = Rect::from_min_size(pos2(280.0, 910.0), vec2(1320.0, 220.0));
    let stale_left_rect = pane::RectEntry {
        id: dragged,
        rect: Rect::from_min_size(pos2(16.0, 280.0), vec2(280.0, 160.0)),
        frame: None,
    };

    let fixed = source_shelf_gap_entry(
        &ctx,
        dragged,
        ShelfEdge::Bottom,
        bottom_content,
        stale_left_rect,
    );

    assert_eq!(fixed.rect.min, bottom_content.min);
    assert_eq!(fixed.rect.height(), bottom_content.height());
    assert!(bottom_content.contains(fixed.rect.center()));
}

#[test]
fn source_shelf_snapshot_reanchors_dragged_entry_without_moving_siblings() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("right-shelf-pane");
    let dragged = Id::new("dragged");
    let sibling = Id::new("sibling");
    let right_content = Rect::from_min_size(pos2(1660.0, 80.0), vec2(280.0, 820.0));
    pane::set_snapshot(
        &ctx,
        pane_id,
        vec![
            pane::RectEntry {
                id: dragged,
                rect: Rect::from_min_size(pos2(16.0, 280.0), vec2(280.0, 160.0)),
                frame: None,
            },
            pane::RectEntry {
                id: sibling,
                rect: Rect::from_min_size(pos2(1660.0, 260.0), vec2(280.0, 160.0)),
                frame: None,
            },
        ],
    );

    reanchor_source_shelf_snapshot(&ctx, pane_id, dragged, ShelfEdge::Right, right_content);

    let snapshot = pane::snapshot(&ctx, pane_id);
    let dragged_entry = snapshot
        .iter()
        .find(|entry| entry.id == dragged)
        .expect("dragged entry should still be present");
    let sibling_entry = snapshot
        .iter()
        .find(|entry| entry.id == sibling)
        .expect("sibling entry should still be present");
    assert_eq!(dragged_entry.rect.min, right_content.min);
    assert!(right_content.contains(dragged_entry.rect.center()));
    assert_eq!(sibling_entry.rect.min, pos2(1660.0, 260.0));
}

#[test]
fn external_container_gap_does_not_render_in_source_pane_or_source_edge() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(0.0, 0.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(100.0, 80.0),
    };

    assert!(!should_render_external_container_gap(
        pane::DragState {
            item: Some(Id::new("dragged")),
            cursor: Some(pos2(0.0, 0.0)),
        },
        Some(drag),
        ShelfEdge::Right,
        Rect::from_min_size(pos2(-10.0, -10.0), vec2(40.0, 40.0)),
        None,
    ));
    assert!(!should_render_external_container_gap(
        pane::DragState::default(),
        Some(drag),
        ShelfEdge::Left,
        Rect::from_min_size(pos2(-10.0, -10.0), vec2(40.0, 40.0)),
        None,
    ));
}

#[test]
fn commit_container_move_to_new_shelf_creates_detached_shelf_owner() {
    let ctx = egui::Context::default();
    let dragged = Id::new("dragged");
    let source_shelf = Id::new("source-shelf");
    let detached_shelf = detached_shelf_id(source_shelf, dragged);
    let mut state = ShelfState::default();

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: dragged,
            source_shelf,
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Bottom),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(100.0, 80.0),
        },
    );

    assert_eq!(
        state.container_location(dragged, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(detached_shelf),
            edge: ShelfEdge::Bottom,
        }
    );
}

#[test]
fn commit_container_move_without_target_clears_stale_drag_state() {
    let ctx = egui::Context::default();
    let dragged = Id::new("dragged");
    let source_pane = Id::new("source-pane");
    let mut state = ShelfState {
        container_move: Some(ShelfContainerMoveState {
            container_id: dragged,
            source_shelf: Id::new("source-shelf"),
            source_pane,
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: None,
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(100.0, 80.0),
        }),
        ..Default::default()
    };
    pane::set_drag(
        &ctx,
        source_pane,
        pane::DragState {
            item: Some(dragged),
            cursor: Some(pos2(0.0, 0.0)),
        },
    );

    let drag = state
        .container_move
        .expect("stale container move should be present");
    commit_container_move(&ctx, &mut state, drag);

    assert!(state.container_move.is_none());
    assert!(pane::drag_state(&ctx, source_pane).item.is_none());
    assert_eq!(
        state.container_edge(dragged, ShelfEdge::Left),
        ShelfEdge::Left
    );
}

#[test]
fn no_target_container_release_cancels_only_when_outside_source_shelf() {
    let shelf_rect = Rect::from_min_size(pos2(100.0, 100.0), vec2(240.0, 400.0));

    assert!(!should_cancel_no_target_container_release(
        Some(pos2(120.0, 120.0)),
        shelf_rect
    ));
    assert!(!should_cancel_no_target_container_release(
        Some(pos2(90.0, 120.0)),
        shelf_rect
    ));
    assert!(should_cancel_no_target_container_release(
        Some(pos2(10.0, 120.0)),
        shelf_rect
    ));
}

#[test]
fn container_move_target_allows_existing_shelf_edges_but_rejects_source_edge() {
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));

    assert_eq!(
        container_move_target(pos2(990.0, 400.0), available, ShelfEdge::Left),
        Some(ShelfEdge::Right),
        "containers may target an occupied/existing shelf edge for insertion"
    );
    assert_eq!(
        container_move_target(pos2(10.0, 400.0), available, ShelfEdge::Left),
        None,
        "moving out of a shelf should not create a cross-shelf move back into the source edge"
    );
}

#[test]
fn container_move_target_prefers_nearest_valid_edge() {
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));

    assert_eq!(
        container_move_target(pos2(930.0, 770.0), available, ShelfEdge::Left),
        Some(ShelfEdge::Bottom),
        "near a corner, the closest edge should own the target ghost"
    );
}

#[test]
fn layout_repairs_corrupted_persisted_shelf_size() {
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("left-shelf");
    let mut state = ShelfState::default();
    state.sizes.insert(shelf_id.with(ShelfEdge::Left), f32::NAN);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves =
        vec![ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE).default_size(240.0)];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 240.0);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Left), Some(240.0));
}

#[test]
fn container_slot_ghost_uses_existing_shelf_insertion_slot() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(0.0, 0.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(80.0, 120.0),
    };
    let snap = [
        pane::RectEntry {
            id: Id::new("first"),
            rect: Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 120.0)),
            frame: None,
        },
        pane::RectEntry {
            id: Id::new("second"),
            rect: Rect::from_min_size(pos2(100.0, 160.0), vec2(80.0, 120.0)),
            frame: None,
        },
    ];

    let before_first = container_slot_ghost_rect_in(None, &snap, drag, 0, false)
        .expect("slot before first should have a ghost");
    let middle = container_slot_ghost_rect_in(None, &snap, drag, 1, false)
        .expect("slot between containers should have a ghost");
    let after_last = container_slot_ghost_rect_in(None, &snap, drag, 2, false)
        .expect("slot after last should have a ghost");

    assert_eq!(before_first.min, pos2(100.0, 20.0));
    assert_eq!(middle.min, pos2(100.0, 160.0));
    assert_eq!(after_last.min, pos2(100.0, 280.0));
}

#[test]
fn container_slot_ghost_uses_horizontal_insertion_slot_for_bottom_shelf() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(0.0, 0.0),
        target_edge: Some(ShelfEdge::Bottom),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(180.0, 96.0),
    };
    let snap = [
        pane::RectEntry {
            id: Id::new("first"),
            rect: Rect::from_min_size(pos2(30.0, 400.0), vec2(180.0, 96.0)),
            frame: None,
        },
        pane::RectEntry {
            id: Id::new("second"),
            rect: Rect::from_min_size(pos2(230.0, 400.0), vec2(180.0, 96.0)),
            frame: None,
        },
    ];

    let before_first = container_slot_ghost_rect_in(None, &snap, drag, 0, true)
        .expect("slot before first should have a horizontal ghost");
    let middle = container_slot_ghost_rect_in(None, &snap, drag, 1, true)
        .expect("slot between containers should have a horizontal ghost");
    let after_last = container_slot_ghost_rect_in(None, &snap, drag, 2, true)
        .expect("slot after last should have a horizontal ghost");

    assert_eq!(before_first.min, pos2(30.0, 400.0));
    assert_eq!(middle.min, pos2(230.0, 400.0));
    assert_eq!(after_last.min, pos2(410.0, 400.0));
}

#[test]
fn existing_shelf_container_ghost_preserves_slot_main_axis() {
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(0.0, 0.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(80.0, 120.0),
    };
    let content_rect = Rect::from_min_size(pos2(100.0, 20.0), vec2(100.0, 260.0));
    let snap = [
        pane::RectEntry {
            id: Id::new("first"),
            rect: Rect::from_min_size(pos2(100.0, 20.0), vec2(80.0, 120.0)),
            frame: None,
        },
        pane::RectEntry {
            id: Id::new("second"),
            rect: Rect::from_min_size(pos2(100.0, 160.0), vec2(80.0, 120.0)),
            frame: None,
        },
    ];

    let after_last = container_slot_ghost_rect_in(Some(content_rect), &snap, drag, 2, false)
        .expect("slot after last should keep the actual insertion position");

    assert_eq!(after_last.min, pos2(100.0, 280.0));
    assert!(content_rect.contains(after_last.min));
    assert_eq!(
        after_last.height(),
        120.0,
        "slot ghosts keep the dragged container's main-axis size instead of snapping upward"
    );
}

#[test]
fn new_shelf_container_ghost_uses_target_shelf_content_rect() {
    let ctx = egui::Context::default();
    let container_id = Id::new("dragged");
    let shelf_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 180.0));
    let content_rect = shelf_rect.shrink(style::theme().shelf().padding);

    let ghost = new_shelf_container_ghost_rect(&ctx, container_id, ShelfEdge::Bottom, shelf_rect);

    assert_eq!(ghost.min, content_rect.min);
    assert_eq!(ghost.height(), content_rect.height());
    assert!(ghost.width() < content_rect.width());
    assert!(content_rect.contains(ghost.max));
}

#[test]
fn new_side_shelf_container_ghost_uses_target_shelf_width() {
    let ctx = egui::Context::default();
    let container_id = Id::new("dragged");
    let shelf_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 700.0));
    let content_rect = shelf_content_rect(ShelfEdge::Right, shelf_rect, style::theme().shelf());

    let ghost = new_shelf_container_ghost_rect(&ctx, container_id, ShelfEdge::Right, shelf_rect);

    assert_eq!(ghost.min, content_rect.min);
    assert_eq!(ghost.width(), content_rect.width());
    assert!(ghost.height() < content_rect.height());
    assert!(content_rect.contains(ghost.max));
}

#[test]
fn container_move_preview_layout_reserves_new_side_shelf_for_ribbons() {
    let ctx = egui::Context::default();
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(220.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(220.0, 800.0))),
        None,
        None,
    );
    let drag = ShelfContainerMoveState {
        container_id: Id::new("dragged"),
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(990.0, 400.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(120.0, 260.0),
    };

    let preview = container_move_preview_layout(&ctx, layout, drag, &theme)
        .expect("new side shelf target should publish preview layout");
    let right = preview
        .right
        .expect("preview should reserve the target side shelf");

    assert_eq!(right.top(), 0.0);
    assert_eq!(right.bottom(), 800.0);
    assert_eq!(preview.viewport.right(), right.left());
    assert_eq!(preview.left, layout.left);
}

#[test]
fn shelf_reservation_ghost_border_only_faces_viewport_center() {
    let rect = crate::vocab::Rect::from_min_max(
        crate::vocab::Pos2::new(10.0, 20.0),
        crate::vocab::Pos2::new(110.0, 220.0),
    );

    assert_eq!(
        shelf_center_border_segment_mara(ShelfEdge::Left, rect),
        [
            crate::vocab::Pos2::new(110.0, 20.0),
            crate::vocab::Pos2::new(110.0, 220.0)
        ]
    );
    assert_eq!(
        shelf_center_border_segment_mara(ShelfEdge::Right, rect),
        [
            crate::vocab::Pos2::new(10.0, 20.0),
            crate::vocab::Pos2::new(10.0, 220.0)
        ]
    );
    assert_eq!(
        shelf_center_border_segment_mara(ShelfEdge::Bottom, rect),
        [
            crate::vocab::Pos2::new(10.0, 20.0),
            crate::vocab::Pos2::new(110.0, 20.0)
        ]
    );
}

#[test]
fn shelf_reservation_ghost_lowers_to_mara_fill_and_border_commands() {
    let rect = crate::vocab::Rect::from_min_size(
        crate::vocab::Pos2::new(10.0, 20.0),
        crate::vocab::Vec2::new(100.0, 40.0),
    );
    let fill = crate::vocab::Color32::from_black_alpha(80);
    let stroke = crate::vocab::Stroke::new(1.5, crate::vocab::Color32::WHITE);

    let cmds = shelf_reservation_ghost_paint_cmds(ShelfEdge::Bottom, rect, fill, stroke);

    assert!(matches!(
        cmds[0],
        crate::paint::PaintCmd::RectFilled {
            rect: got_rect,
            fill: got_fill,
            ..
        } if got_rect == rect && got_fill == fill
    ));
    assert!(matches!(
        cmds[1],
        crate::paint::PaintCmd::Line {
            a,
            b,
            stroke: got_stroke,
        } if a == crate::vocab::Pos2::new(10.0, 20.0)
            && b == crate::vocab::Pos2::new(110.0, 20.0)
            && got_stroke == stroke
    ));
}

#[test]
fn container_slot_ghost_lowers_to_mara_fill_and_stroke_commands() {
    let rect = crate::vocab::Rect::from_min_size(
        crate::vocab::Pos2::new(10.0, 20.0),
        crate::vocab::Vec2::new(100.0, 40.0),
    );
    let accent = crate::vocab::Color32::from_rgb(30, 40, 50);
    let corner = crate::vocab::CornerRadius::same(4);

    let cmds = container_slot_ghost_paint_cmds(rect, accent, corner);

    assert!(matches!(
        cmds[0],
        crate::paint::PaintCmd::RectFilled {
            rect: got_rect,
            corner: got_corner,
            fill,
        } if got_rect == rect
            && got_corner == corner
            && fill == crate::vocab::Color32::from_rgba_unmultiplied(30, 40, 50, 72)
    ));
    assert!(matches!(
        cmds[1],
        crate::paint::PaintCmd::RectStroke {
            rect: got_rect,
            corner: got_corner,
            stroke,
        } if got_rect == rect
            && got_corner == corner
            && stroke == crate::vocab::Stroke::new(1.5, accent)
    ));
}

#[test]
fn shelf_move_ghost_to_bottom_respects_occupied_side_shelf() {
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(780.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        Some(Rect::from_min_max(pos2(780.0, 0.0), pos2(1000.0, 800.0))),
        None,
    );

    let ghost = shelf_drop_rect(layout, ShelfEdge::Left, ShelfEdge::Bottom, &theme)
        .expect("bottom is the only free target edge");

    assert_eq!(ghost.left(), 0.0);
    assert_eq!(ghost.right(), 780.0);
    assert_eq!(ghost.bottom(), 800.0);
    assert_eq!(
        ghost.height(),
        theme.bottom_default_size,
        "cross-axis shelf moves should preview the target bottom height, not the source side width"
    );
}

#[test]
fn shelf_move_ghost_to_side_keeps_full_height_when_bottom_is_occupied() {
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 620.0)),
        None,
        None,
        Some(Rect::from_min_max(pos2(0.0, 620.0), pos2(1000.0, 800.0))),
    );

    let ghost = shelf_drop_rect(layout, ShelfEdge::Left, ShelfEdge::Right, &theme)
        .expect("right edge is free");

    assert_eq!(
        ghost,
        Rect::from_min_max(pos2(700.0, 0.0), pos2(1000.0, 800.0)),
        "side shelves reserve before bottom shelves, so side drop ghosts must show full height"
    );
}

#[test]
fn shelf_move_preview_layout_moves_chrome_bounds_before_drop() {
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    let drag = state::ShelfDragState {
        shelf_id: Id::new("source-shelf"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(990.0, 400.0),
        target_edge: Some(ShelfEdge::Right),
    };
    let state = ShelfState::default();

    let preview = shelf_move_preview_layout(layout, drag, &state, &theme)
        .expect("shelf move ghost should publish a target-edge preview layout");

    assert_eq!(preview.left, None);
    let right = preview
        .right
        .expect("target right shelf should be reserved before drop");
    assert_eq!(right.height(), 800.0);
    assert_eq!(right.width(), 240.0);
    assert_eq!(
        preview.viewport.right(),
        right.left(),
        "ribbon chrome bounds should use the preview viewport while the ghost is shown"
    );
}

#[test]
fn shelf_move_preview_layout_uses_target_axis_default_for_cross_axis_move() {
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    let drag = state::ShelfDragState {
        shelf_id: Id::new("source-shelf"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(500.0, 790.0),
        target_edge: Some(ShelfEdge::Bottom),
    };
    let state = ShelfState::default();

    let preview = shelf_move_preview_layout(layout, drag, &state, &theme)
        .expect("bottom target should produce a preview layout");
    let bottom = preview
        .bottom
        .expect("preview should reserve the target bottom shelf");

    assert_eq!(preview.left, None);
    assert_eq!(bottom.height(), theme.bottom_default_size);
    assert_eq!(preview.viewport.bottom(), bottom.top());
}

#[test]
fn shelf_move_preview_layout_remembers_bottom_size_for_side_to_bottom() {
    let mut state = ShelfState::default();
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("source-shelf");
    state.set_edge_size(shelf_id, ShelfEdge::Bottom, 188.0);
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    let drag = state::ShelfDragState {
        shelf_id,
        source_edge: ShelfEdge::Left,
        cursor: pos2(500.0, 790.0),
        target_edge: Some(ShelfEdge::Bottom),
    };

    let preview = shelf_move_preview_layout(layout, drag, &state, &theme)
        .expect("bottom target should produce a preview layout");
    let bottom = preview.bottom.expect("bottom should be reserved");

    assert_eq!(bottom.height(), 188.0);
    assert_eq!(preview.left, None);
}

#[test]
fn shelf_move_preview_layout_remembers_side_size_for_bottom_to_side() {
    let mut state = ShelfState::default();
    let theme = *style::theme().shelf();
    let shelf_id = Id::new("source-shelf");
    state.set_edge_size(shelf_id, ShelfEdge::Left, 260.0);
    state.set_edge_size(shelf_id, ShelfEdge::Bottom, 188.0);
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 612.0)),
        None,
        None,
        Some(Rect::from_min_max(pos2(0.0, 612.0), pos2(1000.0, 800.0))),
    );
    let drag = state::ShelfDragState {
        shelf_id,
        source_edge: ShelfEdge::Bottom,
        cursor: pos2(990.0, 400.0),
        target_edge: Some(ShelfEdge::Right),
    };

    let preview = shelf_move_preview_layout(layout, drag, &state, &theme)
        .expect("right target should produce a preview layout");
    let right = preview.right.expect("right should be reserved");

    assert_eq!(right.width(), 260.0);
    assert_eq!(preview.bottom, None);
}

#[test]
fn container_new_bottom_shelf_ghost_respects_source_side_shelf() {
    let theme = *style::theme().shelf();
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );

    let ghost = container_drop_rect(layout, ShelfEdge::Left, ShelfEdge::Bottom, &theme)
        .expect("bottom edge is free");

    assert_eq!(
        ghost,
        Rect::from_min_max(
            pos2(240.0, 800.0 - theme.bottom_default_size),
            pos2(1000.0, 800.0)
        ),
        "moving one container out of a side shelf leaves that source shelf in place, but the new bottom shelf ghost uses the target bottom height"
    );
}

#[test]
fn container_drag_bottom_shelf_ghost_releases_empty_source_shelf() {
    let ctx = egui::Context::default();
    let theme = *style::theme().shelf();
    let source_pane = Id::new("source-pane");
    let dragged = Id::new("dragged");
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    pane::set_snapshot(
        &ctx,
        source_pane,
        vec![pane::RectEntry {
            id: dragged,
            rect: Rect::from_min_size(pos2(16.0, 16.0), vec2(200.0, 160.0)),
            frame: None,
        }],
    );
    let drag = ShelfContainerMoveState {
        container_id: dragged,
        source_shelf: Id::new("source-shelf"),
        source_pane,
        source_edge: ShelfEdge::Left,
        cursor: pos2(500.0, 790.0),
        target_edge: Some(ShelfEdge::Bottom),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(200.0, 160.0),
    };

    let ghost = container_drop_rect_for_drag(&ctx, layout, drag, ShelfEdge::Bottom, &theme)
        .expect("bottom edge should be available");

    assert_eq!(
        ghost,
        Rect::from_min_max(
            pos2(0.0, 800.0 - theme.bottom_default_size),
            pos2(1000.0, 800.0)
        ),
        "when the dragged container is the last one, the source shelf disappears, so the bottom ghost must use the full future bottom shelf width"
    );
}

#[test]
fn container_drag_bottom_shelf_ghost_releases_source_when_cache_not_ready() {
    let ctx = egui::Context::default();
    let theme = *style::theme().shelf();
    let dragged = Id::new("dragged");
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    let drag = ShelfContainerMoveState {
        container_id: dragged,
        source_shelf: Id::new("source-shelf"),
        source_pane: Id::new("source-pane"),
        source_edge: ShelfEdge::Left,
        cursor: pos2(500.0, 790.0),
        target_edge: Some(ShelfEdge::Bottom),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(200.0, 160.0),
    };

    let ghost = container_drop_rect_for_drag(&ctx, layout, drag, ShelfEdge::Bottom, &theme)
        .expect("bottom edge should be available even before source cache is warm");

    assert_eq!(
        ghost,
        Rect::from_min_max(
            pos2(0.0, 800.0 - theme.bottom_default_size),
            pos2(1000.0, 800.0)
        ),
        "before the source shelf cache is warm, the first drag preview should not keep a phantom side shelf reserved"
    );
}

#[test]
fn container_drag_bottom_shelf_ghost_keeps_non_empty_source_shelf_reserved() {
    let ctx = egui::Context::default();
    let theme = *style::theme().shelf();
    let source_pane = Id::new("source-pane");
    let dragged = Id::new("dragged");
    let sibling = Id::new("sibling");
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(240.0, 0.0), pos2(1000.0, 800.0)),
        Some(Rect::from_min_max(pos2(0.0, 0.0), pos2(240.0, 800.0))),
        None,
        None,
    );
    pane::set_snapshot(
        &ctx,
        source_pane,
        vec![
            pane::RectEntry {
                id: dragged,
                rect: Rect::from_min_size(pos2(16.0, 16.0), vec2(200.0, 160.0)),
                frame: None,
            },
            pane::RectEntry {
                id: sibling,
                rect: Rect::from_min_size(pos2(16.0, 196.0), vec2(200.0, 160.0)),
                frame: None,
            },
        ],
    );
    let drag = ShelfContainerMoveState {
        container_id: dragged,
        source_shelf: Id::new("source-shelf"),
        source_pane,
        source_edge: ShelfEdge::Left,
        cursor: pos2(500.0, 790.0),
        target_edge: Some(ShelfEdge::Bottom),
        target_shelf: None,
        target_pane: None,
        target_slot: None,
        container_size: vec2(200.0, 160.0),
    };

    let ghost = container_drop_rect_for_drag(&ctx, layout, drag, ShelfEdge::Bottom, &theme)
        .expect("bottom edge should be available");

    assert_eq!(
        ghost,
        Rect::from_min_max(
            pos2(240.0, 800.0 - theme.bottom_default_size),
            pos2(1000.0, 800.0)
        ),
        "when another container remains in the side shelf, the bottom ghost must stay inside the future viewport"
    );
}

#[test]
fn container_drag_existing_shelf_does_not_use_full_shelf_drop_rect() {
    let ctx = egui::Context::default();
    let theme = *style::theme().shelf();
    let source_pane = Id::new("source-pane");
    let dragged = Id::new("dragged");
    let layout = test_shelf_layout(
        Rect::from_min_max(pos2(0.0, 0.0), pos2(760.0, 800.0)),
        None,
        Some(Rect::from_min_max(pos2(760.0, 0.0), pos2(1000.0, 800.0))),
        None,
    );
    pane::set_snapshot(
        &ctx,
        source_pane,
        vec![pane::RectEntry {
            id: dragged,
            rect: Rect::from_min_size(pos2(16.0, 16.0), vec2(200.0, 160.0)),
            frame: None,
        }],
    );
    let drag = ShelfContainerMoveState {
        container_id: dragged,
        source_shelf: Id::new("source-shelf"),
        source_pane,
        source_edge: ShelfEdge::Left,
        cursor: pos2(900.0, 400.0),
        target_edge: Some(ShelfEdge::Right),
        target_shelf: Some(Id::new("right-shelf")),
        target_pane: None,
        target_slot: None,
        container_size: vec2(200.0, 160.0),
    };

    assert_eq!(
        container_drop_rect_for_drag(&ctx, layout, drag, ShelfEdge::Right, &theme),
        None,
        "existing target shelves must only use insertion-slot ghosts; the full-shelf ghost is only for creating a new shelf"
    );
}

#[test]
fn moved_container_to_new_bottom_uses_bottom_default_height_for_layout() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let moved = Id::new("moved");
    let sibling = Id::new("sibling");
    let mut state = ShelfState::default();
    state.set_container_location(
        moved,
        Some(detached_shelf_id(source_shelf, moved)),
        ShelfEdge::Bottom,
    );
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
            .default_size(300.0)
            .container(ShelfContainer::tabbed(moved, "Moved", "box", test_tabs()))
            .container(ShelfContainer::tabbed(
                sibling,
                "Sibling",
                "box",
                test_tabs(),
            )),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.left.unwrap().width(), 300.0);
    assert_eq!(
        layout.bottom.unwrap().height(),
        theme.bottom_default_size,
        "a detached bottom shelf created from a side shelf must use the target bottom height, not the source side width"
    );
}

#[test]
fn moved_container_to_new_side_uses_side_default_width_for_layout() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let moved = Id::new("moved");
    let sibling = Id::new("sibling");
    let mut state = ShelfState::default();
    state.set_container_location(
        moved,
        Some(detached_shelf_id(source_shelf, moved)),
        ShelfEdge::Right,
    );
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Bottom, Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(moved, "Moved", "box", test_tabs()))
            .container(ShelfContainer::tabbed(
                sibling,
                "Sibling",
                "box",
                test_tabs(),
            )),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert_eq!(layout.bottom.unwrap().height(), 180.0);
    assert_eq!(
        layout.right.unwrap().width(),
        theme.side_default_size,
        "a detached side shelf created from a bottom shelf must use the target side width, not the source bottom height"
    );
}

#[test]
fn shelf_move_target_rejects_fully_occupied_edges() {
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let occupied = ShelfOccupied {
        left: false,
        right: true,
        bottom: true,
    };

    let source_band_cursor = pos2(12.0, 790.0);
    let target = shelf_move_target(source_band_cursor, available, occupied, ShelfEdge::Left);

    assert_eq!(
        target, None,
        "when the source edge is excluded and every other edge is occupied, no move target is valid"
    );
}

#[test]
fn finishing_shelf_move_without_target_preserves_original_edge() {
    let shelf_id = Id::new("movable-shelf");
    let mut state = ShelfState::default();

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(10.0, 10.0));
    state.update_drag(pos2(400.0, 400.0), None);
    state.finish_drag();

    assert_eq!(
        state.edge(shelf_id, ShelfEdge::Left),
        ShelfEdge::Left,
        "dropping a shelf outside a valid target band must cancel the move"
    );
}

#[test]
fn finishing_shelf_move_preserves_active_container_on_new_edge() {
    let shelf_id = Id::new("movable-shelf");
    let active_container = Id::new("active-container");
    let mut state = ShelfState::default();
    state.set_active_container_for_group(
        shelf_active_container_key_for(shelf_id, ShelfEdge::Left),
        active_container,
    );

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(10.0, 10.0));
    state.update_drag(pos2(990.0, 400.0), Some(ShelfEdge::Right));
    state.finish_drag();

    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Right);
    assert_eq!(
        state
            .active_container_for_group(shelf_active_container_key_for(shelf_id, ShelfEdge::Right)),
        Some(active_container),
        "moving a shelf should carry its rendered-edge active selection to the new edge"
    );
    assert_eq!(
        state.active_container_for_group(shelf_active_container_key_for(shelf_id, ShelfEdge::Left)),
        None,
        "the old rendered-edge active key must not keep stale selection after the shelf moves"
    );
}

#[test]
fn canceling_shelf_move_preserves_original_edge() {
    let shelf_id = Id::new("movable-shelf");
    let mut state = ShelfState::default();

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(10.0, 10.0));
    state.update_drag(pos2(990.0, 400.0), Some(ShelfEdge::Right));
    state.cancel_drag();

    assert_eq!(
        state.edge(shelf_id, ShelfEdge::Left),
        ShelfEdge::Left,
        "escape/cancel should not persist the previewed target edge"
    );
}

#[test]
fn shelf_move_start_rejects_container_rects() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    let container_id = Id::new("container");
    pane::begin_drag_frame(&ctx, pane_id);
    pane::push_rect(
        &ctx,
        pane_id,
        container_id,
        Rect::from_min_size(pos2(40.0, 50.0), vec2(120.0, 180.0)),
    );
    pane::finalize_snapshot(&ctx, pane_id);

    assert!(pointer_over_shelf_container(
        &ctx,
        pane_id,
        pos2(80.0, 100.0)
    ));
    assert!(!pointer_over_shelf_container(
        &ctx,
        pane_id,
        pos2(10.0, 10.0)
    ));
}

#[test]
fn shelf_move_start_uses_container_frame_rect_when_available() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    let container_id = Id::new("container");
    pane::begin_drag_frame(&ctx, pane_id);
    pane::push_rect_with_frame(
        &ctx,
        pane_id,
        container_id,
        Rect::from_min_size(pos2(60.0, 60.0), vec2(80.0, 80.0)),
        Some(Rect::from_min_size(pos2(40.0, 40.0), vec2(120.0, 120.0))),
    );
    pane::finalize_snapshot(&ctx, pane_id);

    assert!(
        pointer_over_shelf_container(&ctx, pane_id, pos2(45.0, 45.0)),
        "frame chrome belongs to the container and must not start a shelf move"
    );
}

#[test]
fn shelf_move_start_rejects_container_dot_handles() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    pane::clear_container_dot_rects(&ctx, pane_id);
    pane::record_container_dot_rect(
        &ctx,
        pane_id,
        Rect::from_min_size(pos2(20.0, 180.0), vec2(220.0, 8.0)),
    );

    assert!(
        pane::pointer_over_container_dots(&ctx, pane_id, pos2(80.0, 184.0)),
        "container resize/reorder dot handles are not empty shelf background"
    );
    assert!(!pane::pointer_over_container_dots(
        &ctx,
        pane_id,
        pos2(80.0, 150.0)
    ));
}

#[test]
fn shelf_resize_direction_matches_edge_handles() {
    assert_eq!(
        resized_shelf_extent(ShelfEdge::Left, 200.0, vec2(35.0, 0.0), 100.0, 400.0),
        235.0,
        "dragging the left shelf handle right should make it wider"
    );
    assert_eq!(
        resized_shelf_extent(ShelfEdge::Right, 200.0, vec2(-35.0, 0.0), 100.0, 400.0),
        235.0,
        "dragging the right shelf handle left should make it wider"
    );
    assert_eq!(
        resized_shelf_extent(ShelfEdge::Bottom, 180.0, vec2(0.0, -40.0), 100.0, 400.0),
        220.0,
        "dragging the bottom shelf handle up should make it taller"
    );
}

#[test]
fn shelf_resize_cursor_matches_handle_axis() {
    assert_eq!(
        shelf_resize_cursor(ShelfEdge::Left),
        crate::layout::CursorIcon::ResizeHorizontal
    );
    assert_eq!(
        shelf_resize_cursor(ShelfEdge::Right),
        crate::layout::CursorIcon::ResizeHorizontal
    );
    assert_eq!(
        shelf_resize_cursor(ShelfEdge::Bottom),
        crate::layout::CursorIcon::ResizeVertical
    );
}

#[test]
fn shelf_background_lowers_to_mara_rect_command() {
    let rect = crate::vocab::Rect::from_min_size(
        crate::vocab::Pos2::new(10.0, 20.0),
        crate::vocab::Vec2::new(100.0, 40.0),
    );
    let fill = crate::vocab::Color32::from_black_alpha(120);

    let cmd = shelf_background_paint_cmd(rect, fill);

    assert!(matches!(
        cmd,
        crate::paint::PaintCmd::RectFilled {
            rect: got_rect,
            fill: got_fill,
            ..
        } if got_rect == rect && got_fill == fill
    ));
}

#[test]
fn shelf_resize_extent_clamps_to_bounds() {
    assert_eq!(
        resized_shelf_extent(ShelfEdge::Left, 200.0, vec2(-500.0, 0.0), 120.0, 360.0),
        120.0
    );
    assert_eq!(
        resized_shelf_extent(ShelfEdge::Bottom, 200.0, vec2(0.0, -500.0), 120.0, 360.0),
        360.0
    );
}

#[test]
fn shelf_resize_handle_rects_sit_on_inner_edges() {
    let theme = ShelfTheme {
        resize_handle_thickness: 8.0,
        ..*style::theme().shelf()
    };
    let left = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 600.0));
    let right = Rect::from_min_max(pos2(600.0, 0.0), pos2(800.0, 600.0));
    let bottom = Rect::from_min_max(pos2(0.0, 450.0), pos2(800.0, 600.0));

    assert_eq!(
        resize_handle_rect(ShelfEdge::Left, left, &theme),
        Rect::from_min_max(pos2(192.0, 0.0), pos2(200.0, 600.0))
    );
    assert_eq!(
        resize_handle_rect(ShelfEdge::Right, right, &theme),
        Rect::from_min_max(pos2(600.0, 0.0), pos2(608.0, 600.0))
    );
    assert_eq!(
        resize_handle_rect(ShelfEdge::Bottom, bottom, &theme),
        Rect::from_min_max(pos2(0.0, 450.0), pos2(800.0, 458.0))
    );
}

#[test]
fn moved_container_renders_inside_target_shelf_group() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).container(
                ShelfContainer::tabbed(Id::new("already-there"), "Target", "box", test_tabs()),
            ),
        ],
        &state,
    );

    let target_group = groups
        .iter()
        .find(|group| group.id == target_shelf && group.edge == ShelfEdge::Right)
        .expect("target shelf group should exist");
    assert!(
        target_group
            .containers
            .iter()
            .any(|container| container.spec.container_id() == moved_container),
        "moved container should render in the existing target shelf group"
    );
    assert!(
        !groups.iter().any(|group| {
            group.id == source_shelf
                && group.edge == ShelfEdge::Right
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
        }),
        "moved container must not create an overlapping source-owned right shelf"
    );
}

#[test]
fn edge_only_moved_container_renders_inside_existing_edge_shelf_group() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_edge(moved_container, ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::LIGHT_BLUE)
                .default_size(260.0)
                .movable()
                .container(ShelfContainer::tabbed(
                    Id::new("target-container"),
                    "Target",
                    "box",
                    test_tabs(),
                )),
        ],
        &state,
    );

    let target_group = groups
        .iter()
        .find(|group| group.id == target_shelf && group.edge == ShelfEdge::Right)
        .expect("existing target edge shelf should own the right group");
    assert_eq!(target_group.containers.len(), 2);
    assert_eq!(
        egui::Color32::from(target_group.accent),
        Color32::LIGHT_BLUE
    );
    assert_eq!(target_group.default_size, Some(260.0));
    assert!(target_group.movable);
    assert!(
        target_group
            .containers
            .iter()
            .any(|container| container.spec.container_id() == moved_container),
        "edge-only moved containers should merge into the existing shelf on that edge"
    );
    assert!(!groups.iter().any(|group| {
        group.id == source_shelf
            && group.edge == ShelfEdge::Right
            && group
                .containers
                .iter()
                .any(|container| container.spec.container_id() == moved_container)
    }));
}

#[test]
fn edge_only_moved_container_renders_inside_overridden_edge_shelf_group() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_edge(target_shelf, ShelfEdge::Right);
    state.set_container_edge(moved_container, ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Bottom, Color32::LIGHT_BLUE)
                .default_size(260.0)
                .movable()
                .container(ShelfContainer::tabbed(
                    Id::new("target-container"),
                    "Target",
                    "box",
                    test_tabs(),
                )),
        ],
        &state,
    );

    let target_group = groups
        .iter()
        .find(|group| group.id == target_shelf && group.edge == ShelfEdge::Right)
        .expect("state-moved target shelf should own the right group");
    assert_eq!(target_group.containers.len(), 2);
    assert_eq!(
        egui::Color32::from(target_group.accent),
        Color32::LIGHT_BLUE
    );
    assert_eq!(target_group.default_size, Some(260.0));
    assert!(target_group.movable);
    assert!(
        target_group
            .containers
            .iter()
            .any(|container| container.spec.container_id() == moved_container),
        "edge-only moved containers should merge into state-moved shelves on that edge"
    );
    assert!(
        !groups
            .iter()
            .any(|group| { group.id == target_shelf && group.edge == ShelfEdge::Bottom })
    );
}

#[test]
fn moved_container_into_empty_target_shelf_does_not_duplicate_area() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE),
        ],
        &state,
    );

    let target_groups = groups
        .iter()
        .filter(|group| group.id == target_shelf && group.edge == ShelfEdge::Right)
        .count();
    assert_eq!(
        target_groups, 1,
        "empty target shelf and moved container should share one rendered area"
    );
}

#[test]
fn moved_container_into_empty_target_shelf_does_not_render_default_edge() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Bottom);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE),
        ],
        &state,
    );

    assert!(
        !groups
            .iter()
            .any(|group| group.id == target_shelf && group.edge == ShelfEdge::Right),
        "an empty target shelf that only owns moved containers must not render a phantom default-edge group"
    );
    let bottom_group = groups
        .iter()
        .find(|group| group.id == target_shelf && group.edge == ShelfEdge::Bottom)
        .expect("the moved container should render in the target shelf on its active edge");
    assert_eq!(bottom_group.containers.len(), 1);
    assert_eq!(
        bottom_group.containers[0].spec.container_id(),
        moved_container
    );
}

#[test]
fn moved_container_into_empty_target_shelf_only_reserves_target_edge() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Bottom);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(
                moved_container,
                "Moved",
                "box",
                test_tabs(),
            )),
        ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).default_size(260.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(
        layout.right.is_none(),
        "an empty target shelf that only owns moved containers must not also reserve its default edge"
    );
    assert_eq!(layout.bottom.unwrap().height(), 260.0);
    assert_eq!(layout.viewport.max.y, 540.0);
    assert_eq!(layout.viewport.max.x, 1000.0);
}

#[test]
fn stale_moved_container_owner_does_not_hide_empty_shelf_layout() {
    let theme = *style::theme().shelf();
    let target_shelf = Id::new("target-shelf");
    let stale_container = Id::new("removed-container");
    let mut state = ShelfState::default();
    state.set_container_location(stale_container, Some(target_shelf), ShelfEdge::Bottom);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves =
        vec![ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).default_size(260.0)];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(
        layout.bottom.is_none(),
        "stale state for removed containers must not create a bottom shelf"
    );
    assert_eq!(
        layout.right.unwrap().width(),
        260.0,
        "an actually declared empty shelf should still reserve/render its default edge"
    );
}

#[test]
fn stale_moved_container_owner_does_not_hide_empty_shelf_render_group() {
    let target_shelf = Id::new("target-shelf");
    let stale_container = Id::new("removed-container");
    let mut state = ShelfState::default();
    state.set_container_location(stale_container, Some(target_shelf), ShelfEdge::Bottom);

    let groups = split_shelf_render_groups(
        vec![ShelfDef::new(
            target_shelf,
            ShelfEdge::Right,
            Color32::WHITE,
        )],
        &state,
    );

    assert!(
        groups
            .iter()
            .any(|group| group.id == target_shelf && group.edge == ShelfEdge::Right),
        "stale owner state for removed containers must not suppress the declared empty shelf"
    );
    assert!(
        !groups
            .iter()
            .any(|group| group.id == target_shelf && group.edge == ShelfEdge::Bottom),
        "stale owner state for removed containers must not create a phantom moved edge"
    );
}

#[test]
fn same_edge_shelves_merge_into_one_render_area() {
    let first_shelf = Id::new("first-shelf");
    let second_shelf = Id::new("second-shelf");

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(first_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(Id::new("first-container"), "First", "box", test_tabs()),
            ),
            ShelfDef::new(second_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(Id::new("second-container"), "Second", "box", test_tabs()),
            ),
        ],
        &ShelfState::default(),
    );

    let left_groups: Vec<_> = groups
        .iter()
        .filter(|group| group.edge == ShelfEdge::Left)
        .collect();
    assert_eq!(
        left_groups.len(),
        1,
        "one edge must produce one shelf render area"
    );
    assert_eq!(left_groups[0].containers.len(), 2);
}

#[test]
fn moved_shelf_cannot_collapse_into_existing_shelf_edge() {
    let moved_shelf = Id::new("moved-shelf");
    let existing_shelf = Id::new("existing-shelf");
    let moved_container = Id::new("moved-container");
    let existing_container = Id::new("existing-container");
    let mut state = ShelfState::default();
    state.set_edge(moved_shelf, ShelfEdge::Right);

    let shelves = vec![
        ShelfDef::new(moved_shelf, ShelfEdge::Left, Color32::WHITE).container(
            ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
        ),
        ShelfDef::new(existing_shelf, ShelfEdge::Right, Color32::LIGHT_BLUE).container(
            ShelfContainer::tabbed(existing_container, "Existing", "box", test_tabs()),
        ),
    ];

    let edges = shelf_layout_edges(&shelves, &state);
    assert!(edges.contains(&ShelfLayoutEntry {
        base_idx: 0,
        shelf_id: moved_shelf,
        edge: ShelfEdge::Left,
    }));
    assert!(edges.contains(&ShelfLayoutEntry {
        base_idx: 1,
        shelf_id: existing_shelf,
        edge: ShelfEdge::Right,
    }));

    let groups = split_shelf_render_groups(shelves, &state);
    assert!(
        groups.iter().any(|group| {
            group.id == moved_shelf
                && group.edge == ShelfEdge::Left
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
        }),
        "a rejected/invalid shelf move must fall back to its original free edge instead of merging into the occupied edge"
    );
    assert!(
        groups.iter().any(|group| {
            group.id == existing_shelf
                && group.edge == ShelfEdge::Right
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == existing_container)
        }),
        "the existing shelf keeps its own identity and containers"
    );
    assert!(
        !groups.iter().any(|group| {
            group.edge == ShelfEdge::Right
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == existing_container)
        }),
        "two different shelves must not collapse into one render group after a conflicting move"
    );
}

#[test]
fn split_shelf_created_by_container_move_can_move_without_merging_with_source() {
    let ctx = egui::Context::default();
    let source_shelf = Id::new("source-shelf");
    let kept_container = Id::new("kept-container");
    let moved_container = Id::new("moved-container");
    let detached_shelf = detached_shelf_id(source_shelf, moved_container);
    let mut state = ShelfState::default();

    commit_container_move(
        &ctx,
        &mut state,
        ShelfContainerMoveState {
            container_id: moved_container,
            source_shelf,
            source_pane: Id::new("source-pane"),
            source_edge: ShelfEdge::Left,
            cursor: pos2(0.0, 0.0),
            target_edge: Some(ShelfEdge::Bottom),
            target_shelf: None,
            target_pane: None,
            target_slot: None,
            container_size: vec2(100.0, 80.0),
        },
    );

    state.begin_drag(detached_shelf, ShelfEdge::Bottom, pos2(0.0, 0.0));
    state.update_drag(pos2(100.0, 100.0), Some(ShelfEdge::Right));
    state.finish_drag();

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
                .container(ShelfContainer::tabbed(
                    kept_container,
                    "Kept",
                    "box",
                    test_tabs(),
                ))
                .container(ShelfContainer::tabbed(
                    moved_container,
                    "Moved",
                    "box",
                    test_tabs(),
                )),
        ],
        &state,
    );

    assert!(
        groups.iter().any(|group| {
            group.id == source_shelf
                && group.edge == ShelfEdge::Left
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == kept_container)
                && !group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
        }),
        "the original shelf must keep only its remaining containers"
    );
    assert!(
        groups.iter().any(|group| {
            group.id == detached_shelf
                && group.edge == ShelfEdge::Right
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
                && !group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == kept_container)
        }),
        "the split-off shelf must move independently without merging back into the source shelf"
    );
}

#[test]
fn moved_container_merges_with_later_declared_target_edge() {
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).container(
                ShelfContainer::tabbed(Id::new("target-container"), "Target", "box", test_tabs()),
            ),
        ],
        &state,
    );

    let right_groups: Vec<_> = groups
        .iter()
        .filter(|group| group.edge == ShelfEdge::Right)
        .collect();
    assert_eq!(right_groups.len(), 1);
    assert_eq!(right_groups[0].containers.len(), 2);
}

#[test]
fn moved_container_uses_target_shelf_extent_for_layout() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(
                moved_container,
                "Moved",
                "box",
                test_tabs(),
            )),
        ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).default_size(260.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert_eq!(layout.right.unwrap().width(), 260.0);
    assert_eq!(layout.viewport.max.x, 740.0);
}

#[test]
fn moved_container_uses_target_shelf_extent_regardless_of_declaration_order() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let target_shelf = Id::new("target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(target_shelf), ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(target_shelf, ShelfEdge::Right, Color32::WHITE).default_size(260.0),
        ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(
                moved_container,
                "Moved",
                "box",
                test_tabs(),
            )),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert_eq!(layout.right.unwrap().width(), 260.0);
    assert_eq!(layout.viewport.max.x, 740.0);
}

#[test]
fn moved_container_with_missing_owner_uses_existing_edge_shelf_extent() {
    let theme = *style::theme().shelf();
    let source_shelf = Id::new("source-shelf");
    let missing_shelf = Id::new("removed-target-shelf");
    let replacement_shelf = Id::new("replacement-target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(missing_shelf), ShelfEdge::Right);
    let available = Rect::from_min_max(pos2(0.0, 0.0), pos2(1000.0, 800.0));
    let shelves = vec![
        ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE)
            .default_size(180.0)
            .container(ShelfContainer::tabbed(
                moved_container,
                "Moved",
                "box",
                test_tabs(),
            )),
        ShelfDef::new(replacement_shelf, ShelfEdge::Right, Color32::WHITE).default_size(260.0),
    ];

    let layout = layout_shelves(available, &shelves, &mut state, &theme);

    assert!(layout.left.is_none());
    assert_eq!(
        layout.right.unwrap().width(),
        260.0,
        "declared containers with stale owner ids should adopt the existing shelf on their edge"
    );
    assert_eq!(layout.viewport.max.x, 740.0);
}

#[test]
fn moved_container_with_missing_owner_renders_in_existing_edge_shelf() {
    let source_shelf = Id::new("source-shelf");
    let missing_shelf = Id::new("removed-target-shelf");
    let replacement_shelf = Id::new("replacement-target-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(missing_shelf), ShelfEdge::Right);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(source_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(replacement_shelf, ShelfEdge::Right, Color32::LIGHT_BLUE)
                .default_size(260.0)
                .container(ShelfContainer::tabbed(
                    Id::new("replacement-container"),
                    "Replacement",
                    "box",
                    test_tabs(),
                )),
        ],
        &state,
    );

    let right_group = groups
        .iter()
        .find(|group| group.id == replacement_shelf && group.edge == ShelfEdge::Right)
        .expect("stale owner ids should fall back to the current shelf on the target edge");
    assert_eq!(egui::Color32::from(right_group.accent), Color32::LIGHT_BLUE);
    assert_eq!(right_group.default_size, Some(260.0));
    assert!(
        right_group
            .containers
            .iter()
            .any(|container| container.spec.container_id() == moved_container)
    );
    assert!(!groups.iter().any(|group| group.id == missing_shelf));
}

#[test]
fn shelf_display_order_prefers_persisted_order_over_hashmap_iteration() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");
    pane::set_section_order(&ctx, pane_id, vec![second, first, third]);

    let order: Vec<Id> =
        shelf_display_order(&ctx, pane_id, [&first, &second, &third].into_iter()).collect();

    assert_eq!(order, vec![second, first, third]);
}

#[test]
fn active_container_fallback_uses_declared_order_not_response_map_order() {
    let ctx = egui::Context::default();
    let pane_id = Id::new("shelf-pane");
    let first = Id::new("first");
    let second = Id::new("second");
    let third = Id::new("third");

    assert_eq!(
        resolve_visible_active_container(&ctx, pane_id, None, &[first, second, third], |id| id
            == second
            || id == third,),
        Some(second),
        "when no active container is visible, shelves should select the first visible container in declared/layout order, not arbitrary HashMap response order"
    );
}

#[test]
fn same_shelf_groups_on_different_edges_have_independent_active_keys() {
    let shelf_id = Id::new("split-shelf");
    let left_group = ShelfDef::new(shelf_id, ShelfEdge::Left, Color32::WHITE);
    let bottom_group = ShelfDef::new(shelf_id, ShelfEdge::Bottom, Color32::WHITE);

    assert_ne!(
        shelf_active_container_key(&left_group),
        shelf_active_container_key(&bottom_group),
        "one shelf can produce render groups on multiple edges, so active-container state must be per rendered edge group"
    );
}

#[test]
fn moved_container_keeps_current_shelf_owner_when_moved_again() {
    let original_shelf = Id::new("original-shelf");
    let adopted_shelf = Id::new("adopted-shelf");
    let moved_container = Id::new("moved-container");
    let mut state = ShelfState::default();
    state.set_container_location(moved_container, Some(adopted_shelf), ShelfEdge::Bottom);

    let groups = split_shelf_render_groups(
        vec![
            ShelfDef::new(original_shelf, ShelfEdge::Left, Color32::WHITE).container(
                ShelfContainer::tabbed(moved_container, "Moved", "box", test_tabs()),
            ),
            ShelfDef::new(adopted_shelf, ShelfEdge::Right, Color32::WHITE),
        ],
        &state,
    );

    assert!(
        groups.iter().any(|group| {
            group.id == adopted_shelf
                && group.edge == ShelfEdge::Bottom
                && group
                    .containers
                    .iter()
                    .any(|container| container.spec.container_id() == moved_container)
        }),
        "a container moved again should keep the shelf owner it was dragged from"
    );
}

#[test]
fn moving_shelf_carries_adopted_containers_on_source_edge() {
    let shelf_id = Id::new("movable-shelf");
    let adopted_container = Id::new("adopted-container");
    let moved_out_container = Id::new("moved-out-container");
    let mut state = ShelfState::default();
    state.set_container_location(adopted_container, Some(shelf_id), ShelfEdge::Left);
    state.set_container_location(moved_out_container, Some(shelf_id), ShelfEdge::Bottom);

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(0.0, 0.0));
    state.update_drag(pos2(100.0, 100.0), Some(ShelfEdge::Right));
    state.finish_drag();

    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Right);
    assert_eq!(
        state.container_location(adopted_container, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(shelf_id),
            edge: ShelfEdge::Right,
        }
    );
    assert_eq!(
        state.container_location(moved_out_container, ShelfEdge::Left),
        ShelfContainerLocation {
            shelf_id: Some(shelf_id),
            edge: ShelfEdge::Bottom,
        },
        "containers already moved away from the dragged edge should not be pulled back"
    );
}

#[test]
fn moving_shelf_preserves_user_resized_extent() {
    let shelf_id = Id::new("movable-shelf");
    let mut state = ShelfState::default();
    state.set_edge_size(shelf_id, ShelfEdge::Left, 344.0);

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(0.0, 0.0));
    state.update_drag(pos2(100.0, 100.0), Some(ShelfEdge::Right));
    state.finish_drag();

    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Right);
    assert_eq!(state.edge_size(shelf_id, ShelfEdge::Right), Some(344.0));
}

#[test]
fn moving_shelf_does_not_copy_side_width_to_bottom_height() {
    let shelf_id = Id::new("movable-shelf");
    let mut state = ShelfState::default();
    state.set_edge_size(shelf_id, ShelfEdge::Left, 344.0);
    state.set_edge_size(shelf_id, ShelfEdge::Bottom, 180.0);

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(0.0, 0.0));
    state.update_drag(pos2(100.0, 100.0), Some(ShelfEdge::Bottom));
    state.finish_drag();

    assert_eq!(state.edge(shelf_id, ShelfEdge::Left), ShelfEdge::Bottom);
    assert_eq!(
        state.edge_size(shelf_id, ShelfEdge::Bottom),
        Some(180.0),
        "side widths and bottom heights are different axes; moving across axes must not overwrite the target edge size"
    );
}

#[test]
fn moving_shelf_clears_resize_start_state_on_source_and_target_edges() {
    let shelf_id = Id::new("movable-shelf");
    let mut state = ShelfState::default();
    state.resize_starts.insert(
        shelf_id.with(ShelfEdge::Left),
        ShelfResizeStart {
            size: 240.0,
            pointer: pos2(240.0, 100.0),
        },
    );
    state.resize_starts.insert(
        shelf_id.with(ShelfEdge::Right),
        ShelfResizeStart {
            size: 320.0,
            pointer: pos2(760.0, 100.0),
        },
    );

    state.begin_drag(shelf_id, ShelfEdge::Left, pos2(0.0, 0.0));
    state.update_drag(pos2(100.0, 100.0), Some(ShelfEdge::Right));
    state.finish_drag();

    assert!(
        !state
            .resize_starts
            .contains_key(&shelf_id.with(ShelfEdge::Left)),
        "moving a shelf should clear stale resize capture on the source edge"
    );
    assert!(
        !state
            .resize_starts
            .contains_key(&shelf_id.with(ShelfEdge::Right)),
        "moving a shelf should clear stale resize capture on the target edge"
    );
}
