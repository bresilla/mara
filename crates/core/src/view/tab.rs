//! The Tab model: one type owns everything a tab is (PLAN.md WS8).
//!
//! `Tab = { switcher entry, shelves state, root: ViewNode }` — selecting
//! a tab swaps its shelves and its whole view tree together, while the
//! top bar (window frame) persists. [`Tabs`] is the collection + active
//! selection, and syncs both ways with the shell bar: it feeds the
//! switcher's buttons ([`Tabs::apply_to_bar`]) and consumes its click
//! events ([`Tabs::on_shell_event`]). There is ONE source of truth for
//! "which tab is active" — this type — killing the app-side pattern of
//! a view enum kept manually in sync with `ShellBar::active`.
//!
//! A single-tab app is fully supported: the shell bar hides the
//! switcher when only one tab exists (no dead icon), and the lone tab
//! still renders through the same path.

use crate::shelf::ShelfState;
use crate::shell::ShellView;
use crate::workspace::WorkspaceStack;

use super::ViewNode;

/// One tab: its switcher entry, its shelf state, and its content tree.
///
/// The shelf *state* (sizes, dock choices, active containers) is
/// per-tab so switching tabs swaps the whole shelf configuration; the
/// shelf *defs* (what panels exist) stay app-supplied per frame for the
/// active tab, like every other Mara surface.
pub struct Tab {
    view: ShellView,
    root: ViewNode,
    workspace: WorkspaceStack,
    shelf_state: ShelfState,
}

impl Tab {
    /// Build a tab from its switcher entry and content tree. The tab's
    /// workspace stack is derived from the entry id.
    #[must_use]
    pub fn new(view: ShellView, root: ViewNode) -> Self {
        let workspace = WorkspaceStack::new(crate::vocab::Id::new(("mara.tab", view.id)));
        Self {
            view,
            root,
            workspace,
            shelf_state: ShelfState::default(),
        }
    }

    /// The switcher entry (id, icon, tooltip) this tab shows as.
    #[must_use]
    pub fn view(&self) -> ShellView {
        self.view
    }

    /// Stable id — the `ShellEvent::ViewSelected` payload that selects
    /// this tab.
    #[must_use]
    pub fn id(&self) -> &'static str {
        self.view.id
    }

    /// The tab's content tree.
    #[must_use]
    pub fn root_mut(&mut self) -> &mut ViewNode {
        &mut self.root
    }

    /// This tab's shelf state (persisted across tab switches).
    #[must_use]
    pub fn shelf_state_mut(&mut self) -> &mut ShelfState {
        &mut self.shelf_state
    }
}

/// The app's tabs plus the single source of truth for which is active.
pub struct Tabs {
    tabs: Vec<Tab>,
    active: usize,
}

impl Tabs {
    /// Build from at least one tab; the first is initially active.
    ///
    /// # Panics
    /// When `tabs` is empty — an app always has at least one tab (a
    /// single-view app is the one-tab case, it just shows no switcher).
    #[must_use]
    pub fn new(tabs: Vec<Tab>) -> Self {
        assert!(!tabs.is_empty(), "an app has at least one tab");
        Self { tabs, active: 0 }
    }

    /// The active tab's id.
    #[must_use]
    pub fn active_id(&self) -> &'static str {
        self.tabs[self.active].view.id
    }

    /// Select the tab with `id`. Returns whether it existed.
    pub fn select(&mut self, id: &str) -> bool {
        match self.tabs.iter().position(|tab| tab.view.id == id) {
            Some(idx) => {
                self.active = idx;
                true
            }
            None => false,
        }
    }

    /// The active tab — its tree, workspace, and shelf state, ready for
    /// a host to render: build a `ViewCtx` over the workspace and call
    /// `root.render(...)`.
    pub fn active_mut(&mut self) -> (&mut ViewNode, &mut WorkspaceStack, &mut ShelfState) {
        let tab = &mut self.tabs[self.active];
        (&mut tab.root, &mut tab.workspace, &mut tab.shelf_state)
    }

    /// Mutable access to a tab's tree by id (runtime split/unsplit on a
    /// background tab). `None` when no tab has `id`.
    pub fn root_mut(&mut self, id: &str) -> Option<&mut ViewNode> {
        self.tabs
            .iter_mut()
            .find(|tab| tab.view.id == id)
            .map(|tab| &mut tab.root)
    }

    /// Push this collection into the shell bar: switcher buttons from
    /// the tabs, highlight from the active selection. The bar hides the
    /// switcher automatically when there is only one tab.
    pub fn apply_to_bar(&self, bar: &mut crate::shell::ShellBar) {
        bar.views = self.tabs.iter().map(|tab| tab.view).collect();
        bar.active = Some(self.active_id());
    }

    /// Route a shell event: a `ViewSelected` for one of these tabs
    /// selects it and returns `true`; everything else returns `false`
    /// for the app to handle.
    pub fn on_shell_event(&mut self, event: &crate::shell::ShellEvent) -> bool {
        match event {
            crate::shell::ShellEvent::ViewSelected(id) => self.select(id),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view::{MaraView, ViewCtx, ViewId};

    struct DummyView;
    impl MaraView for DummyView {
        fn id(&self) -> ViewId {
            ViewId::new("dummy")
        }
        fn title(&self) -> &str {
            "dummy"
        }
        fn icon(&self) -> &'static str {
            "square"
        }
        fn show(&mut self, _ctx: &mut ViewCtx<'_>) {}
    }

    fn tab(id: &'static str) -> Tab {
        Tab::new(
            ShellView::new(id, "square", "Tab"),
            ViewNode::leaf(DummyView),
        )
    }

    #[test]
    fn tabs_select_and_sync_bar() {
        let mut tabs = Tabs::new(vec![tab("t.a"), tab("t.b")]);
        assert_eq!(tabs.active_id(), "t.a");

        assert!(tabs.on_shell_event(&crate::shell::ShellEvent::ViewSelected("t.b")));
        assert_eq!(tabs.active_id(), "t.b");
        assert!(!tabs.select("t.missing"));
        assert_eq!(tabs.active_id(), "t.b", "failed select keeps active");

        let mut bar = crate::shell::ShellBar::default();
        tabs.apply_to_bar(&mut bar);
        assert_eq!(bar.views.len(), 2);
        assert_eq!(bar.active, Some("t.b"));

        assert!(
            !tabs.on_shell_event(&crate::shell::ShellEvent::MenuOpened),
            "non-selection events pass through to the app"
        );
    }

    #[test]
    fn each_tab_owns_workspace_and_shelf_state() {
        let mut tabs = Tabs::new(vec![tab("t.a"), tab("t.b")]);
        let (_, ws_a, _) = tabs.active_mut();
        let a = ws_a.current().id;
        tabs.select("t.b");
        let (_, ws_b, _) = tabs.active_mut();
        assert_ne!(a, ws_b.current().id, "tabs have distinct workspaces");
    }
}
