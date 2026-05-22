use egui::{Color32, Context, Id, Rect, Response, Sense, Vec2, pos2, vec2};

use super::{
    RibbonAction, RibbonCluster, RibbonDrag, RibbonEdge, RibbonOpen, RibbonPlacement, RibbonScope,
    RibbonSlotItem,
    chrome::draw_unified_ribbon_chrome,
    paint::{paint_ribbon_button, paint_ribbon_glyph, ribbon_button_fg},
};

const LEFT_SHELF_RIBBON_CHROME_ID: &str = "mara.system.left_shelf.ribbon";
const LEFT_SHELF_ITEM_CHROME_ID: &str = "mara.system.left_shelf.item";
const RIGHT_SHELF_RIBBON_CHROME_ID: &str = "mara.system.right_shelf.ribbon";
const RIGHT_SHELF_ITEM_CHROME_ID: &str = "mara.system.right_shelf.item";
const BOTTOM_SHELF_RIBBON_CHROME_ID: &str = "mara.system.bottom_shelf.ribbon";
const BOTTOM_SHELF_ITEM_CHROME_ID: &str = "mara.system.bottom_shelf.item";
const APP_MENU_RIBBON_CHROME_ID: &str = "mara.system.app_menu.ribbon";
const APP_MENU_ITEM_CHROME_ID: &str = "mara.system.app_menu.item";
const CLOSE_RIBBON_CHROME_ID: &str = "mara.system.close.ribbon";
const CLOSE_ITEM_CHROME_ID: &str = "mara.system.close.item";

#[derive(Clone, Debug)]
pub struct ResolvedSlotRibbon {
    pub id: Id,
    pub chrome_id: Option<&'static str>,
    pub scope: RibbonScope,
    pub edge: RibbonEdge,
    pub role: super::RibbonRole,
    pub mode: super::RibbonMode,
    pub cluster: RibbonCluster,
    pub accepts: &'static [&'static str],
    pub items: Vec<RibbonSlotItem>,
}

#[derive(Clone, Debug)]
pub struct RibbonSlotClick {
    pub ribbon: Id,
    pub item: Id,
    pub action: RibbonAction,
    pub response: Option<Response>,
}

/// Paint already-resolved slot ribbons as simple icon/button rails.
///
/// Simple renderer for resolved slot ribbons. Most app chrome should
/// use [`draw_slot_ribbons_featureful`] so it gets drag/reorder,
/// panel toggles, placement, and fullscreen layering too.
pub fn draw_slot_ribbons(
    ctx: &Context,
    accent: Color32,
    ribbons: &[ResolvedSlotRibbon],
) -> Vec<RibbonSlotClick> {
    let augmented = shelf_augmented_ribbons(ctx, ribbons, ShelfButtonOrder::Simple);
    let ribbons = augmented.as_deref().unwrap_or(ribbons);
    draw_slot_ribbons_inner(ctx, accent, ribbons)
}

fn draw_slot_ribbons_inner(
    ctx: &Context,
    accent: Color32,
    ribbons: &[ResolvedSlotRibbon],
) -> Vec<RibbonSlotClick> {
    assert_resolved_ribbon_icons(ribbons);
    let mut clicks = Vec::new();
    for ribbon in ribbons {
        draw_one_slot_ribbon(ctx, accent, ribbon, &mut clicks);
    }
    clicks
}

/// Draw slot ribbons through the featureful chrome path whenever
/// the slot declarations provide static chrome ids.
///
/// This is the convergence point for the single ribbon API and the
/// featureful renderer capabilities: drag/reorder, cross-ribbon
/// placement, panel toggle state, pane anchoring, and fullscreen rail
/// layering are preserved. If any ribbon/item lacks a chrome id, the
/// function falls back to the simple slot painter for that frame.
pub fn draw_slot_ribbons_featureful(
    ctx: &Context,
    accent: Color32,
    ribbons: &[ResolvedSlotRibbon],
    open: &mut RibbonOpen,
    placement: &mut RibbonPlacement,
    drag: &mut RibbonDrag,
) -> Vec<RibbonSlotClick> {
    let featureful_augmented = shelf_augmented_ribbons(ctx, ribbons, ShelfButtonOrder::Featureful);
    let featureful_ribbons = featureful_augmented.as_deref().unwrap_or(ribbons);
    if !can_use_featureful_chrome(featureful_ribbons) {
        let simple_augmented = shelf_augmented_ribbons(ctx, ribbons, ShelfButtonOrder::Simple);
        return draw_slot_ribbons_inner(
            ctx,
            accent,
            simple_augmented.as_deref().unwrap_or(ribbons),
        );
    }

    let ribbons = featureful_ribbons;
    assert_resolved_ribbon_icons(ribbons);
    let clicks = draw_unified_ribbon_chrome(ctx, accent, ribbons, open, placement, drag, |id| {
        ribbons
            .iter()
            .flat_map(|ribbon| ribbon.items.iter())
            .find(|item| item.chrome_id == Some(id))
            .is_some_and(|item| item.active)
    });
    clicks
        .into_iter()
        .filter_map(|click| {
            ribbons
                .iter()
                .find_map(|ribbon| {
                    ribbon
                        .items
                        .iter()
                        .find(|item| item.id == click)
                        .map(|item| (ribbon.id, item))
                })
                .map(|(ribbon_id, item)| RibbonSlotClick {
                    ribbon: ribbon_id,
                    item: item.id,
                    action: item.action,
                    response: None,
                })
        })
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ShelfButtonOrder {
    /// Simple slot painting lays horizontal end-cluster items left to
    /// right, so the close/restore button stays at the outer edge when
    /// the shelf button is inserted before it.
    Simple,
    /// Featureful chrome lays end-cluster slot zero at the outer edge,
    /// so the close/restore button stays at the outer edge when the
    /// shelf button is inserted after it.
    Featureful,
}

fn shelf_augmented_ribbons(
    ctx: &Context,
    ribbons: &[ResolvedSlotRibbon],
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    let visible_layout = crate::shelf::shelf_layout(ctx);
    augment_shelf_buttons_with_chrome(
        ribbons,
        crate::window_chrome::window_chrome_host_capabilities(ctx),
        crate::shelf::published_shelf_presence(ctx),
        visible_layout.is_some_and(|layout| layout.left.is_some()),
        visible_layout.is_some_and(|layout| layout.right.is_some()),
        visible_layout.is_some_and(|layout| layout.bottom.is_some()),
        order,
    )
}

#[cfg(test)]
fn augment_shelf_buttons(
    ribbons: &[ResolvedSlotRibbon],
    presence: crate::shelf::ShelfPresence,
    left_visible: bool,
    right_visible: bool,
    bottom_visible: bool,
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    augment_shelf_buttons_with_chrome(
        ribbons,
        crate::window_chrome::WindowChromeHostCapabilities {
            system_menu: false,
            system_close: false,
            ..Default::default()
        },
        presence,
        left_visible,
        right_visible,
        bottom_visible,
        order,
    )
}

fn augment_shelf_buttons_with_chrome(
    ribbons: &[ResolvedSlotRibbon],
    chrome: crate::window_chrome::WindowChromeHostCapabilities,
    presence: crate::shelf::ShelfPresence,
    left_visible: bool,
    right_visible: bool,
    bottom_visible: bool,
    order: ShelfButtonOrder,
) -> Option<Vec<ResolvedSlotRibbon>> {
    let has_left = presence.left;
    let has_right = presence.right;
    let has_bottom = presence.bottom;
    let mut out = ribbons.to_vec();
    let mut changed = false;
    if chrome.system_menu && !contains_item(&out, app_menu_item_id()) {
        insert_app_menu_button(&mut out);
        changed = true;
    }
    if chrome.system_close && !contains_item(&out, close_item_id()) {
        insert_close_button(&mut out, order);
        changed = true;
    }
    if has_left && !contains_item(&out, left_shelf_item_id()) {
        insert_left_shelf_button(&mut out, left_visible);
        changed = true;
    }
    if has_right && !contains_item(&out, right_shelf_item_id()) {
        insert_right_shelf_button(&mut out, right_visible, order);
        changed = true;
    }
    if has_bottom && !contains_item(&out, bottom_shelf_item_id()) {
        insert_bottom_shelf_button(&mut out, bottom_visible, order);
        changed = true;
    }
    changed.then_some(out)
}

fn contains_item(ribbons: &[ResolvedSlotRibbon], item_id: Id) -> bool {
    ribbons
        .iter()
        .any(|ribbon| ribbon.items.iter().any(|item| item.id == item_id))
}

fn insert_app_menu_button(ribbons: &mut Vec<ResolvedSlotRibbon>) {
    let item = app_menu_item();
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::Start) {
        ribbon.items.insert(0, item);
    } else {
        ribbons.push(system_button_ribbon(
            APP_MENU_RIBBON_CHROME_ID,
            RibbonCluster::Start,
            item,
        ));
    }
}

fn insert_close_button(ribbons: &mut Vec<ResolvedSlotRibbon>, order: ShelfButtonOrder) {
    let item = close_item();
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon.items.len(),
            ShelfButtonOrder::Featureful => 0,
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(system_button_ribbon(
            CLOSE_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn insert_left_shelf_button(ribbons: &mut Vec<ResolvedSlotRibbon>, active: bool) {
    let item = left_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::Start) {
        let insert = ribbon
            .items
            .iter()
            .position(|item| item.id == Id::new("system.app_menu.item"))
            .map_or_else(|| (!ribbon.items.is_empty()) as usize, |idx| idx + 1);
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            LEFT_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::Start,
            item,
        ));
    }
}

fn insert_right_shelf_button(
    ribbons: &mut Vec<ResolvedSlotRibbon>,
    active: bool,
    order: ShelfButtonOrder,
) {
    let item = right_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon.items.len().saturating_sub(1),
            ShelfButtonOrder::Featureful => 1.min(ribbon.items.len()),
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            RIGHT_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn insert_bottom_shelf_button(
    ribbons: &mut Vec<ResolvedSlotRibbon>,
    active: bool,
    order: ShelfButtonOrder,
) {
    let item = bottom_shelf_item(active);
    if let Some(ribbon) = find_top_permanent_cluster_mut(ribbons, RibbonCluster::End) {
        let insert = match order {
            ShelfButtonOrder::Simple => ribbon
                .items
                .iter()
                .position(|item| item.id == right_shelf_item_id())
                .unwrap_or_else(|| ribbon.items.len().saturating_sub(1)),
            ShelfButtonOrder::Featureful => ribbon
                .items
                .iter()
                .position(|item| item.id == right_shelf_item_id())
                .map_or_else(|| 1.min(ribbon.items.len()), |idx| idx + 1),
        };
        ribbon.items.insert(insert, item);
    } else {
        ribbons.push(shelf_button_ribbon(
            BOTTOM_SHELF_RIBBON_CHROME_ID,
            RibbonCluster::End,
            item,
        ));
    }
}

fn find_top_permanent_cluster_mut(
    ribbons: &mut [ResolvedSlotRibbon],
    cluster: RibbonCluster,
) -> Option<&mut ResolvedSlotRibbon> {
    ribbons
        .iter_mut()
        .find(|ribbon| is_top_permanent_ribbon(ribbon) && ribbon.cluster == cluster)
}

fn is_top_permanent_ribbon(ribbon: &ResolvedSlotRibbon) -> bool {
    ribbon.scope == RibbonScope::Permanent && ribbon.edge == RibbonEdge::Top
}

fn system_button_ribbon(
    chrome_id: &'static str,
    cluster: RibbonCluster,
    item: RibbonSlotItem,
) -> ResolvedSlotRibbon {
    shelf_button_ribbon(chrome_id, cluster, item)
}

fn shelf_button_ribbon(
    chrome_id: &'static str,
    cluster: RibbonCluster,
    item: RibbonSlotItem,
) -> ResolvedSlotRibbon {
    ResolvedSlotRibbon {
        id: Id::new(chrome_id),
        chrome_id: Some(chrome_id),
        scope: RibbonScope::Permanent,
        edge: RibbonEdge::Top,
        role: super::RibbonRole::Icon,
        mode: super::RibbonMode::ThreeSided,
        cluster,
        accepts: &[],
        items: vec![item],
    }
}

fn app_menu_item() -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        APP_MENU_ITEM_CHROME_ID,
        "line-horizontal-3",
        "Menu",
        "Open application menu",
        RibbonAction::Command(crate::ribbon::app_menu_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = app_menu_item_id();
    item
}

fn close_item() -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        CLOSE_ITEM_CHROME_ID,
        crate::style::theme().views.close_icon,
        "Close",
        "Close application",
        RibbonAction::CloseApp,
    )
    .with_role(super::RibbonRole::Icon);
    item.id = close_item_id();
    item
}

fn left_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        LEFT_SHELF_ITEM_CHROME_ID,
        "panel-left",
        "Left shelf",
        "Left shelf",
        RibbonAction::Command(crate::ribbon::left_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = left_shelf_item_id();
    item.active = active;
    item
}

fn right_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        RIGHT_SHELF_ITEM_CHROME_ID,
        "panel-right",
        "Right shelf",
        "Right shelf",
        RibbonAction::Command(crate::ribbon::right_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = right_shelf_item_id();
    item.active = active;
    item
}

fn bottom_shelf_item(active: bool) -> RibbonSlotItem {
    let mut item = RibbonSlotItem::featureful(
        BOTTOM_SHELF_ITEM_CHROME_ID,
        "panel-bottom",
        "Bottom shelf",
        "Bottom shelf",
        RibbonAction::Command(crate::ribbon::bottom_shelf_command_id()),
    )
    .with_role(super::RibbonRole::Icon);
    item.id = bottom_shelf_item_id();
    item.active = active;
    item
}

fn left_shelf_item_id() -> Id {
    Id::new("system.left_shelf.item")
}

fn app_menu_item_id() -> Id {
    Id::new("system.app_menu.item")
}

fn close_item_id() -> Id {
    Id::new("system.close_app")
}

fn right_shelf_item_id() -> Id {
    Id::new("system.right_shelf.item")
}

fn bottom_shelf_item_id() -> Id {
    Id::new("system.bottom_shelf.item")
}

fn assert_resolved_ribbon_icons(ribbons: &[ResolvedSlotRibbon]) {
    for ribbon in ribbons {
        for item in &ribbon.items {
            assert!(
                crate::icons::is_icon_payload(item.icon),
                "ribbon slot items require an icon that resolves to a bundled font icon or inline SVG"
            );
        }
    }
}

fn can_use_featureful_chrome(ribbons: &[ResolvedSlotRibbon]) -> bool {
    ribbons.iter().all(|ribbon| {
        ribbon.chrome_id.is_some()
            && ribbon.items.iter().all(|item| {
                item.chrome_id.is_some()
                    && item.chrome_tooltip.is_some()
                    && crate::icons::is_icon_payload(item.icon)
            })
    })
}

fn draw_one_slot_ribbon(
    ctx: &Context,
    accent: Color32,
    ribbon: &ResolvedSlotRibbon,
    clicks: &mut Vec<RibbonSlotClick>,
) {
    if ribbon.items.is_empty() {
        return;
    }

    let screen = ctx.content_rect();
    let chrome = chrome_for_scope(ribbon.scope);
    let count = ribbon.items.len() as f32;
    let vertical = ribbon.edge.is_vertical();
    let span = count * chrome.button_size + (count - 1.0).max(0.0) * chrome.button_gap;
    let size = if vertical {
        vec2(chrome.button_size, span)
    } else {
        vec2(span, chrome.button_size)
    };
    let pos = ribbon_origin(screen, ribbon.edge, ribbon.cluster, size, chrome.edge_gap);
    let area_id = Id::new(("mara_slot_ribbon", ribbon.id));

    egui::Area::new(area_id)
        .order(egui::Order::Foreground)
        .fixed_pos(pos)
        .show(ctx, |ui| {
            ui.set_min_size(size);
            for (idx, item) in ribbon.items.iter().enumerate() {
                let offset = idx as f32 * (chrome.button_size + chrome.button_gap);
                let min = if vertical {
                    pos2(0.0, offset)
                } else {
                    pos2(offset, 0.0)
                };
                let rect = Rect::from_min_size(min, Vec2::splat(chrome.button_size));
                let response = ui
                    .interact(rect, ui.id().with(item.id), Sense::click())
                    .on_hover_text(item.tooltip.clone());
                paint_ribbon_button(ui.painter(), rect, accent, item.active, response.hovered());
                let glyph = glyph_for_item(item);
                paint_ribbon_glyph(
                    ui,
                    rect,
                    glyph,
                    ribbon_button_fg(accent, item.active, response.hovered(), glyph),
                );
                if response.clicked() {
                    clicks.push(RibbonSlotClick {
                        ribbon: ribbon.id,
                        item: item.id,
                        action: item.action,
                        response: Some(response),
                    });
                }
            }
        });
}

fn chrome_for_scope(scope: RibbonScope) -> crate::style::RibbonChromeTheme {
    let ribbon = crate::style::theme().ribbon;
    match scope {
        RibbonScope::Permanent => ribbon.permanent,
        RibbonScope::View(_) => ribbon.view_local,
        RibbonScope::WorkspaceLevel(_) => ribbon.workspace,
    }
}

fn ribbon_origin(
    screen: Rect,
    edge: RibbonEdge,
    cluster: RibbonCluster,
    size: Vec2,
    margin: f32,
) -> egui::Pos2 {
    match edge {
        RibbonEdge::Left => {
            let y = cluster_axis_pos(screen.top(), screen.bottom(), size.y, cluster, margin);
            pos2(screen.left() + margin, y)
        }
        RibbonEdge::Right => {
            let y = cluster_axis_pos(screen.top(), screen.bottom(), size.y, cluster, margin);
            pos2(screen.right() - margin - size.x, y)
        }
        RibbonEdge::Top => {
            let x = cluster_axis_pos(screen.left(), screen.right(), size.x, cluster, margin);
            pos2(x, screen.top() + margin)
        }
        RibbonEdge::Bottom => {
            let x = cluster_axis_pos(screen.left(), screen.right(), size.x, cluster, margin);
            pos2(x, screen.bottom() - margin - size.y)
        }
    }
}

fn cluster_axis_pos(min: f32, max: f32, span: f32, cluster: RibbonCluster, margin: f32) -> f32 {
    match cluster {
        RibbonCluster::Start => min + margin,
        RibbonCluster::Middle => (min + max - span) * 0.5,
        RibbonCluster::End => max - margin - span,
    }
}

fn glyph_for_item(item: &RibbonSlotItem) -> super::RibbonGlyph {
    if item.icon.trim_start().starts_with("<svg") || item.icon.trim_start().starts_with("<?xml") {
        super::RibbonGlyph::Svg(item.icon)
    } else {
        super::RibbonGlyph::Icon(item.icon)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presence(left: bool, right: bool, bottom: bool) -> crate::shelf::ShelfPresence {
        crate::shelf::ShelfPresence {
            left,
            right,
            bottom,
        }
    }

    fn item(id: &'static str, icon: &'static str, action: RibbonAction) -> RibbonSlotItem {
        RibbonSlotItem::featureful(id, icon, id, id, action)
            .with_role(super::super::RibbonRole::Icon)
    }

    fn top_ribbon(cluster: RibbonCluster, items: Vec<RibbonSlotItem>) -> ResolvedSlotRibbon {
        ResolvedSlotRibbon {
            id: Id::new(("top", cluster)),
            chrome_id: Some("top"),
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: super::super::RibbonRole::Icon,
            mode: super::super::RibbonMode::ThreeSided,
            cluster,
            accepts: &[],
            items,
        }
    }

    #[test]
    fn shelf_buttons_are_absent_without_side_shelves() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item(
                "system.app_menu.item",
                "line-horizontal-3",
                RibbonAction::Noop,
            )],
        )];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(false, false, false),
                false,
                false,
                false,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "no published side shelves should not alter the top bar"
        );
    }

    #[test]
    fn shelf_buttons_need_an_existing_top_bar() {
        let ribbons = vec![ResolvedSlotRibbon {
            id: Id::new("left.rail"),
            chrome_id: Some("left.rail"),
            scope: RibbonScope::View(crate::ViewId::new("test.view")),
            edge: RibbonEdge::Left,
            role: super::super::RibbonRole::Icon,
            mode: super::super::RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items: vec![item("tool", "cube", RibbonAction::Noop)],
        }];

        assert!(
            augment_shelf_buttons(
                &ribbons,
                presence(true, true, true),
                true,
                true,
                true,
                ShelfButtonOrder::Featureful
            )
            .is_none(),
            "shelf buttons attach to an existing permanent top bar, not to side rails alone"
        );
    }

    #[test]
    fn left_shelf_button_is_inserted_after_menu_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![
                item(
                    "system.app_menu.item",
                    "line-horizontal-3",
                    RibbonAction::Noop,
                ),
                item("view.switch.item", "cube", RibbonAction::Noop),
            ],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            true,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("left shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                Id::new("system.app_menu.item"),
                left_shelf_item_id(),
                Id::new("view.switch.item"),
            ]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-left");
        assert!(augmented[0].items[1].active);
    }

    #[test]
    fn hidden_declared_shelf_keeps_inactive_top_bar_button() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::Start,
            vec![item(
                "system.app_menu.item",
                "line-horizontal-3",
                RibbonAction::Noop,
            )],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(true, false, false),
            false,
            false,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("declared hidden left shelf should keep a button for re-opening");
        assert_eq!(augmented[0].items[1].id, left_shelf_item_id());
        assert!(!augmented[0].items[1].active);
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_featureful_chrome() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Featureful,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![Id::new("system.close_app"), right_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-right");
    }

    #[test]
    fn right_shelf_button_keeps_close_at_outer_edge_for_simple_painter() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, false),
            false,
            true,
            false,
            ShelfButtonOrder::Simple,
        )
        .expect("right shelf should add a button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![right_shelf_item_id(), Id::new("system.close_app")]
        );
        assert_eq!(augmented[0].items[0].icon, "panel-right");
    }

    #[test]
    fn bottom_shelf_button_uses_right_side_of_permanent_bar() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, false, true),
            false,
            false,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("bottom shelf should add a right-side top-bar button");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![Id::new("system.close_app"), bottom_shelf_item_id()]
        );
        assert_eq!(augmented[0].items[1].icon, "panel-bottom");
    }

    #[test]
    fn bottom_shelf_button_stays_left_of_right_shelf_button_when_both_exist() {
        let ribbons = vec![top_ribbon(
            RibbonCluster::End,
            vec![item("system.close_app", "dismiss", RibbonAction::CloseApp)],
        )];

        let augmented = augment_shelf_buttons(
            &ribbons,
            presence(false, true, true),
            false,
            true,
            true,
            ShelfButtonOrder::Featureful,
        )
        .expect("right and bottom shelves should add right-side top-bar buttons");
        let ids: Vec<_> = augmented[0].items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec![
                Id::new("system.close_app"),
                right_shelf_item_id(),
                bottom_shelf_item_id(),
            ]
        );
    }
}
