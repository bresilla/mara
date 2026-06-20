//! # Shell — the host-neutral permanent top bar.
//!
//! The permanent top bar (app-menu + view switcher + window controls)
//! is **UI**, not window chrome, so it lives here in `mara_core` and is
//! identical on every host. Each host adapter — the Bevy plugin, the
//! `mara::window` eframe runner — renders it once per frame by calling
//! [`ShellBar::show`] and reacting to the returned [`ShellEvent`]s.
//! That is what makes the bar *enforced* and *cross-platform*: there is
//! one implementation, invoked by the host, not the app.
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
/// app. Set `enabled = false` to opt out of the bar entirely (the
/// single, explicit escape hatch).
#[cfg_attr(feature = "bevy", derive(bevy::prelude::Resource))]
#[derive(Clone, Debug)]
pub struct ShellBar {
    /// Render the permanent top bar at all. Default `true`.
    pub enabled: bool,
    /// Show the left-edge application-menu button.
    pub app_menu: bool,
    /// View-switcher buttons (left cluster, after the menu).
    pub views: Vec<ShellView>,
    /// Currently active view id, highlighted in the switcher. Updated
    /// automatically when [`ShellBar::show`] reports a
    /// [`ShellEvent::ViewSelected`].
    pub active: Option<&'static str>,
}

impl Default for ShellBar {
    fn default() -> Self {
        Self {
            enabled: true,
            app_menu: true,
            views: Vec::new(),
            active: None,
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
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        open: &mut RibbonOpen,
        placement: &mut RibbonPlacement,
        drag: &mut RibbonDrag,
    ) -> Vec<ShellEvent> {
        if !self.enabled {
            return Vec::new();
        }
        let accent = crate::style::active_accent();
        let view_ids: Vec<&'static str> = self.views.iter().map(|v| v.id).collect();

        let ribbon = self.build_ribbon();
        let clicks = __internal_draw_slot_ribbons_featureful_egui(
            ctx,
            accent,
            std::slice::from_ref(&ribbon),
            open,
            placement,
            drag,
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

    fn build_ribbon(&self) -> ResolvedSlotRibbon {
        // Left cluster: app-menu, then the view switcher. Window
        // controls (maximize/close) and shelf toggles are injected by
        // the slot renderer from the published host capabilities +
        // shelf presence, so we never build them here — that is what
        // makes the bar adapt to web/android automatically.
        let mut items: Vec<RibbonSlotItem> = Vec::new();
        if self.app_menu {
            items.push(
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
        for view in &self.views {
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
            items.push(item);
        }

        // Always emit the permanent top bar even when `items` is empty,
        // so the slot renderer has a top permanent Start-cluster ribbon
        // to attach the injected window controls / shelf toggles to.
        ResolvedSlotRibbon {
            id: MaraId::new(TOP_BAR_CHROME_ID),
            chrome_id: Some(TOP_BAR_CHROME_ID),
            scope: RibbonScope::Permanent,
            edge: RibbonEdge::Top,
            role: RibbonRole::Icon,
            mode: RibbonMode::ThreeSided,
            cluster: RibbonCluster::Start,
            accepts: &[],
            items,
        }
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
        let events = bar.show(&ctx, &mut open, &mut placement, &mut drag);
        let _ = ctx.end_pass();
        // No interaction in a headless pass → no events.
        assert!(events.is_empty());
    }

    /// A disabled bar renders nothing and emits nothing.
    #[test]
    fn disabled_shell_bar_is_inert() {
        let mut bar = ShellBar {
            enabled: false,
            views: vec![ShellView::new("v", "cube", "V")],
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();
        ctx.begin_pass(egui::RawInput::default());
        let events = bar.show(&ctx, &mut open, &mut placement, &mut drag);
        let _ = ctx.end_pass();
        assert!(events.is_empty());
    }
}
