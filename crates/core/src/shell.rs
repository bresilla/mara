//! # Shell — the host-neutral permanent top bar.
//!
//! The permanent top bar (app-menu + view switcher + window controls)
//! is **UI**, not window chrome, so it lives here in `mara_core` and is
//! identical on every host. Each host adapter — the Bevy plugin, the
//! `mara::window`/`mara::android` runners — renders it once per frame
//! by rendering it through the host facade and reacting to the returned
//! [`ShellEvent`]s. That is what makes the bar *enforced* and
//! *cross-platform*: there is one implementation, invoked by the host,
//! not the app.
//!
//! The bar is enforced: if neither the host nor the app rendered a
//! `ShellBar` in a pass, [`crate::enforce`] renders a Mara-owned
//! fallback bar the moment any Mara surface draws. An app that wants
//! the *functional* bar (views, menu, events) renders it through this
//! API, which suppresses the fallback. The single deliberate escape
//! hatch is the per-frame host opt-out (`opt_out_shell_bar`) — there is
//! no passive flag to forget the bar with.
//!
//! The bar is responsive (the slot renderer reflows it per
//! [`Breakpoint`](crate::style::Breakpoint)) and adaptive: the window
//! controls (maximize/close) and shelf toggles are injected by the slot
//! renderer only where the host advertises the matching capabilities /
//! shelf presence. A browser or android host advertises nothing, so the
//! same bar simply drops those buttons.

use crate::ribbon::{
    __internal_draw_slot_ribbons_featureful_egui, ResolvedSlotRibbon, RibbonAction, RibbonCluster,
    RibbonDrag, RibbonEdge, RibbonMode, RibbonOpen, RibbonPlacement, RibbonRole, RibbonScope,
    RibbonSlotItem, app_menu_command_id, bottom_shelf_command_id, left_shelf_command_id,
    right_shelf_command_id,
};
use crate::vocab::Id as MaraId;

const TOP_BAR_CHROME_ID: &str = "mara.shell.topbar";
const TOP_BAR_VIEWS_CHROME_ID: &str = "mara.shell.topbar.views";
const APP_MENU_ITEM_ID: &str = "system.app_menu.item";

/// One button in the permanent top bar's view switcher. The buttons
/// are icon-only, so the human string is the hover tooltip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShellView {
    /// Stable id — also the chrome id used for drag/reorder and the
    /// payload of [`ShellEvent::ViewSelected`].
    pub id: &'static str,
    /// Fluent icon glyph name.
    pub icon: &'static str,
    /// Hover tooltip / accessible name (must be non-empty).
    pub tooltip: &'static str,
}

impl ShellView {
    #[must_use]
    pub fn new(id: &'static str, icon: &'static str, tooltip: &'static str) -> Self {
        Self { id, icon, tooltip }
    }
}

/// Configuration + selection state for the enforced permanent top bar.
///
/// On Bevy this is a `Resource`; the eframe runner stores it on the
/// app. The bar has no disable flag — if nothing renders it,
/// [`crate::enforce`] renders a fallback bar. The single deliberate
/// escape hatch is the per-frame host opt-out
/// ([`crate::enforce::__internal_opt_out_shell`], exposed as
/// `MaraHostCtx::opt_out_shell_bar`).
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug)]
pub struct ShellBar {
    /// Show the left-edge application-menu button.
    pub app_menu: bool,
    /// View-switcher buttons. Rendered in [`ShellBar::views_cluster`]
    /// (centred by default), between the left-edge app-menu and the
    /// right-edge window controls.
    pub views: Vec<ShellView>,
    /// Currently active view id, highlighted in the switcher. Updated
    /// automatically when the bar render reports a
    /// [`ShellEvent::ViewSelected`].
    pub active: Option<&'static str>,
    /// Where along the top bar the tab/view switcher sits. Default:
    /// [`RibbonCluster::Middle`] (the center zone). Apps may move it to
    /// `Start` or `End`; the app-menu and window controls keep their
    /// edges either way.
    pub views_cluster: RibbonCluster,
}

impl Default for ShellBar {
    fn default() -> Self {
        Self {
            app_menu: true,
            views: Vec::new(),
            active: None,
            views_cluster: RibbonCluster::Middle,
        }
    }
}

/// What the user did in the top bar this frame.
///
/// The window actions ([`ShellEvent::CloseRequested`] /
/// [`ShellEvent::MaximizeToggleRequested`]) are meant for the host
/// adapter (it owns the window); the rest are app-level. On Bevy this
/// is also a `Message`.
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Message))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShellEvent {
    /// A view-switcher button was clicked (carries [`ShellView::id`]).
    ViewSelected(&'static str),
    /// The application-menu button was clicked.
    MenuOpened,
    /// The left shelf toggle was clicked.
    LeftShelfToggled,
    /// The right shelf toggle was clicked.
    RightShelfToggled,
    /// The bottom shelf toggle was clicked.
    BottomShelfToggled,
    /// The close window control was clicked. Host adapters close the
    /// window; apps usually ignore it.
    CloseRequested,
    /// The maximize/restore window control was clicked. Host adapters
    /// toggle the native window.
    MaximizeToggleRequested,
}

impl ShellBar {
    /// Render the bar into `ctx` and return the events it produced.
    ///
    /// `open` / `placement` / `drag` are the featureful-ribbon state
    /// the host owns (a Bevy `Resource` trio, or runner-owned state in
    /// eframe). The accent is read from the active theme. `active` is
    /// updated in place when a view is selected, so the host doesn't
    /// have to echo it back.
    ///
    /// First-party egui hook: takes the shared `egui::Context`, so it
    /// is hidden — apps render the bar through their host facade
    /// (`MaraHostCtx::show_shell_bar` or the runner/plugin), never by
    /// holding the raw backend context.
    #[doc(hidden)]
    pub fn __internal_show_egui(
        &mut self,
        ctx: &egui::Context,
        open: &mut RibbonOpen,
        placement: &mut RibbonPlacement,
        drag: &mut RibbonDrag,
    ) -> Vec<ShellEvent> {
        // Mark the app as having rendered the bar this pass so the
        // enforcement fallback stays out of the way.
        crate::enforce::mark_app_shell_shown(ctx);
        let accent = crate::style::active_accent();
        let view_ids: Vec<&'static str> = self.views.iter().map(|v| v.id).collect();

        let ribbons = self.build_ribbons();
        let clicks = __internal_draw_slot_ribbons_featureful_egui(
            ctx, accent, &ribbons, open, placement, drag,
        );

        let mut events = Vec::new();
        for click in clicks {
            match click.action {
                RibbonAction::CloseApp => events.push(ShellEvent::CloseRequested),
                RibbonAction::ToggleMaximize => events.push(ShellEvent::MaximizeToggleRequested),
                RibbonAction::Command(id) => {
                    if id == app_menu_command_id() {
                        events.push(ShellEvent::MenuOpened);
                    } else if id == left_shelf_command_id() {
                        events.push(ShellEvent::LeftShelfToggled);
                    } else if id == right_shelf_command_id() {
                        events.push(ShellEvent::RightShelfToggled);
                    } else if id == bottom_shelf_command_id() {
                        events.push(ShellEvent::BottomShelfToggled);
                    } else if let Some(view_id) =
                        view_ids.iter().copied().find(|v| view_command_id(v) == id)
                    {
                        self.active = Some(view_id);
                        events.push(ShellEvent::ViewSelected(view_id));
                    }
                }
                _ => {}
            }
        }
        events
    }

    fn build_ribbons(&self) -> Vec<ResolvedSlotRibbon> {
        // Conventional top-bar layout:
        //   * Start (far left): the app-menu button.
        //   * Middle (centred): the view switcher.
        //   * End (far right): window controls (maximize/close) + shelf
        //     toggles — injected by the slot renderer from the published
        //     host capabilities, so the bar adapts to web/android.
        let mut start_items: Vec<RibbonSlotItem> = Vec::new();
        if self.app_menu {
            start_items.push(
                RibbonSlotItem::featureful(
                    APP_MENU_ITEM_ID,
                    "line-horizontal-3",
                    "Menu",
                    "Open application menu",
                    RibbonAction::Command(app_menu_command_id()),
                )
                .with_role(RibbonRole::Icon),
            );
        }

        let mut view_items: Vec<RibbonSlotItem> = Vec::new();
        // A single-tab app shows NO tab chrome: with nothing to switch
        // between, a lone highlighted icon is dead UI, so the switcher
        // renders only when there are 2+ views to pick from.
        let show_switcher = self.views.len() > 1;
        for view in self.views.iter().filter(|_| show_switcher) {
            let mut item = RibbonSlotItem::featureful(
                view.id,
                view.icon,
                // Icon-only buttons: label is never shown, so reuse the
                // tooltip (it just has to be non-empty).
                view.tooltip,
                view.tooltip,
                RibbonAction::Command(view_command_id(view.id)),
            )
            .with_role(RibbonRole::Icon);
            item.active = self.active == Some(view.id);
            view_items.push(item);
        }

        // The Start ribbon is always emitted (even empty) — it is the
        // permanent top bar the slot renderer attaches injected window
        // controls / shelf toggles to.
        let mut ribbons = vec![ResolvedSlotRibbon {
            id: MaraId::new(TOP_BAR_CHROME_ID),
            chrome_id: Some(TOP_BAR_CHROME_ID),
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: RibbonRole::Icon,
            mode: RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items: start_items,
        }];

        // The view switcher rides `views_cluster` — Middle by default so
        // it sits centred in the bar, but apps can dock it Start/End;
        // the app-menu and window controls keep their edges either way.
        if !view_items.is_empty() {
            ribbons.push(ResolvedSlotRibbon {
                id: MaraId::new(TOP_BAR_VIEWS_CHROME_ID),
                chrome_id: Some(TOP_BAR_VIEWS_CHROME_ID),
                scope: RibbonScope::Permanent,
                edge: RibbonEdge::Top,
                role: RibbonRole::Icon,
                mode: RibbonMode::ThreeSided,
                cluster: self.views_cluster,
                accepts: &[],
                items: view_items,
            });
        }

        ribbons
    }
}

/// The `RibbonAction::Command` id carried by a view-switcher button.
#[must_use]
fn view_command_id(view_id: &'static str) -> MaraId {
    MaraId::new(("mara.topbar.view", view_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rendering a bar with an app-menu + views must build valid slot
    /// items (non-empty label/tooltip) and not panic. Regression for
    /// the empty-tooltip assert that crashed the native demo.
    /// A single-tab app shows no tab chrome: with nothing to switch
    /// between, the switcher ribbon must not be emitted at all.
    #[test]
    fn single_view_bar_emits_no_switcher() {
        let bar = ShellBar {
            views: vec![ShellView::new("v.only", "cube", "Only")],
            active: Some("v.only"),
            ..Default::default()
        };
        let ribbons = bar.build_ribbons();
        assert!(
            !ribbons
                .iter()
                .any(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID)),
            "one view => no switcher ribbon"
        );

        let two = ShellBar {
            views: vec![
                ShellView::new("v.a", "cube", "A"),
                ShellView::new("v.b", "pen", "B"),
            ],
            ..Default::default()
        };
        assert!(
            two.build_ribbons()
                .iter()
                .any(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID)),
            "two views => switcher present"
        );
    }

    /// The switcher rides `views_cluster` — Middle by default, movable
    /// to Start/End by the app.
    #[test]
    fn switcher_cluster_is_configurable_default_middle() {
        let mut bar = ShellBar {
            views: vec![
                ShellView::new("v.a", "cube", "A"),
                ShellView::new("v.b", "pen", "B"),
            ],
            ..Default::default()
        };
        let cluster_of = |bar: &ShellBar| {
            bar.build_ribbons()
                .into_iter()
                .find(|ribbon| ribbon.chrome_id == Some(TOP_BAR_VIEWS_CHROME_ID))
                .map(|ribbon| ribbon.cluster)
        };
        assert_eq!(cluster_of(&bar), Some(RibbonCluster::Middle));
        bar.views_cluster = RibbonCluster::End;
        assert_eq!(cluster_of(&bar), Some(RibbonCluster::End));
    }

    #[test]
    fn shell_bar_renders_without_panicking() {
        let bar = ShellBar {
            views: vec![
                ShellView::new("v.scene", "cube", "Scene"),
                ShellView::new("v.graph", "pen", "Graph"),
            ],
            active: Some("v.scene"),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();
        ctx.begin_pass(egui::RawInput::default());
        let mut bar = bar;
        let events = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
        let _ = ctx.end_pass();
        // No interaction in a headless pass → no events.
        assert!(events.is_empty());
    }

    /// The bar render always paints — the bar has no disable flag.
    /// (The explicit per-frame opt-out lives in `crate::enforce` and is
    /// tested there.)
    #[test]
    fn shell_bar_show_always_renders() {
        let mut bar = ShellBar {
            views: vec![ShellView::new("v", "cube", "V")],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();
        ctx.begin_pass(egui::RawInput::default());
        let events = bar.__internal_show_egui(&ctx, &mut open, &mut placement, &mut drag);
        let output = ctx.end_pass();
        assert!(events.is_empty());
        assert!(
            !output.shapes.is_empty(),
            "the bar must render unconditionally"
        );
    }
}
