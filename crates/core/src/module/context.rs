use std::collections::HashSet;

use crate::{
    ribbon::{RibbonOverrideLayer, RibbonScope, RibbonSlotDef},
    vocab::{Color32 as MaraColor32, Id as MaraId},
    workspace::{
        WorkspaceBar, WorkspaceLevelState, WorkspacePolicy, WorkspaceStack,
        validate_workspace_bar_item,
    },
};

/// Options for a module's inline pod representation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleInlineOptions {
    pub allow_workspace: bool,
    pub units: usize,
}

impl Default for ModuleInlineOptions {
    fn default() -> Self {
        Self {
            allow_workspace: true,
            units: 10,
        }
    }
}

/// Context passed to `MaraModule::inline`.
pub struct ModuleInlineCtx<'a> {
    pub pod_id: MaraId,
    pub slot_index: usize,
    pub accent: MaraColor32,
    pub options: ModuleInlineOptions,
    pub workspace: Option<&'a mut WorkspaceStack>,
}

impl ModuleInlineCtx<'_> {
    #[must_use]
    pub fn can_enter_workspace(&self) -> bool {
        self.options.allow_workspace
            && self
                .workspace
                .as_ref()
                .is_none_or(|stack| stack.current_policy().allow_module_workspace_push)
    }
}

/// Result of rendering a module inline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModuleResponse {
    pub enter_workspace: bool,
}

impl ModuleResponse {
    #[must_use]
    pub const fn none() -> Self {
        Self {
            enter_workspace: false,
        }
    }

    #[must_use]
    pub const fn enter_workspace() -> Self {
        Self {
            enter_workspace: true,
        }
    }
}

/// Context passed to `MaraModule::workspace`.
///
/// This is intentionally small for the first implementation slice:
/// it carries workspace identity, policy, bars, accent, and stack
/// mutation. Rendering panes/containers into an L1 workspace will be
/// layered on top of this type in the next phase.
pub struct WorkspaceCtx<'a> {
    pub level: WorkspaceLevelState,
    pub policy: WorkspacePolicy,
    pub accent: MaraColor32,
    stack: &'a mut WorkspaceStack,
    bars: Vec<WorkspaceBar>,
    ribbons: Vec<RibbonSlotDef>,
    ribbon_overrides: Vec<RibbonOverrideLayer>,
}

impl<'a> WorkspaceCtx<'a> {
    #[must_use]
    pub fn new(stack: &'a mut WorkspaceStack, accent: impl Into<MaraColor32>) -> Self {
        let level = stack.current();
        let policy = stack.current_policy();
        Self {
            level,
            policy,
            accent: accent.into(),
            stack,
            bars: Vec::new(),
            ribbons: Vec::new(),
            ribbon_overrides: Vec::new(),
        }
    }

    pub fn add_bar(&mut self, bar: WorkspaceBar) {
        assert!(
            !self.bars.iter().any(|existing| existing.id == bar.id),
            "workspace bars require unique ids within one workspace level"
        );
        let mut seen_items = HashSet::with_capacity(bar.items.len());
        for item in &bar.items {
            assert!(
                seen_items.insert(item.id),
                "workspace bar items require unique ids within one bar"
            );
            validate_workspace_bar_item(item);
        }
        self.bars.push(bar);
    }

    #[must_use]
    pub fn bars(&self) -> &[WorkspaceBar] {
        &self.bars
    }

    pub fn add_ribbon(&mut self, ribbon: RibbonSlotDef) {
        assert!(
            matches!(ribbon.scope, RibbonScope::WorkspaceLevel(id) if id == self.level.id),
            "WorkspaceCtx::add_ribbon only accepts ribbons scoped to the current workspace level"
        );
        assert!(
            !self.ribbons.iter().any(|existing| existing.id == ribbon.id),
            "workspace ribbons require unique ids within one workspace level"
        );
        self.ribbons.push(ribbon);
    }

    #[must_use]
    pub fn ribbons(&self) -> &[RibbonSlotDef] {
        &self.ribbons
    }

    pub fn add_ribbon_override(&mut self, layer: RibbonOverrideLayer) {
        self.ribbon_overrides.push(layer);
    }

    #[must_use]
    pub fn ribbon_overrides(&self) -> &[RibbonOverrideLayer] {
        &self.ribbon_overrides
    }

    pub fn push_module_workspace(&mut self, module_id: impl Into<MaraId>) -> WorkspaceLevelState {
        self.stack.push_module(module_id)
    }

    pub fn pop_workspace(
        &mut self,
    ) -> Result<WorkspaceLevelState, crate::workspace::WorkspaceStackError> {
        self.stack.pop()
    }

    /// Current responsive size class for this frame. Module workspaces
    /// consult this to reflow on small screens.
    #[must_use]
    pub fn breakpoint(&self) -> crate::style::Breakpoint {
        crate::style::screen_class()
    }

    /// Convenience: phone-class (the most aggressive reflow tier).
    #[must_use]
    pub fn is_compact(&self) -> bool {
        self.breakpoint().is_compact()
    }

    /// Convenience: phone or tablet (not the full desktop shell).
    #[must_use]
    pub fn is_handheld(&self) -> bool {
        self.breakpoint().is_handheld()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        RibbonCluster, RibbonEdge, WorkspaceBarItem,
        ribbon::{RibbonScope, RibbonSlotDef},
    };

    fn module_workspace_ctx() -> (WorkspaceStack, MaraColor32) {
        let mut stack = WorkspaceStack::new("root");
        stack.push_module("module");
        (stack, MaraColor32::WHITE)
    }

    #[test]
    fn workspace_ctx_rejects_duplicate_bar_ids() {
        let (mut stack, accent) = module_workspace_ctx();
        let mut ctx = WorkspaceCtx::new(&mut stack, accent);
        let id = MaraId::new("bar");
        ctx.add_bar(WorkspaceBar::new(
            id,
            crate::WorkspaceBarEdge::Top,
            crate::WorkspaceBarCluster::Middle,
        ));

        let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.add_bar(WorkspaceBar::new(
                id,
                crate::WorkspaceBarEdge::Left,
                crate::WorkspaceBarCluster::Start,
            ));
        }));

        assert!(duplicate.is_err());
    }

    #[test]
    fn workspace_ctx_rejects_duplicate_bar_item_ids() {
        let (mut stack, accent) = module_workspace_ctx();
        let mut ctx = WorkspaceCtx::new(&mut stack, accent);
        let item_id = MaraId::new("item");
        let bar = WorkspaceBar {
            id: "bar".into(),
            edge: crate::WorkspaceBarEdge::Top,
            cluster: crate::WorkspaceBarCluster::Middle,
            items: vec![
                WorkspaceBarItem::command(item_id, "Add", Some("add")),
                WorkspaceBarItem::command(item_id, "Remove", Some("dismiss")),
            ],
        };

        let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.add_bar(bar);
        }));

        assert!(duplicate.is_err());
    }

    #[test]
    fn workspace_ctx_rejects_ribbons_scoped_outside_current_level() {
        let (mut stack, accent) = module_workspace_ctx();
        let mut ctx = WorkspaceCtx::new(&mut stack, accent);
        let wrong_scope = RibbonSlotDef::new(
            egui::Id::new("wrong.ribbon"),
            RibbonScope::View(crate::ViewId::new("canvas")),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            Vec::new(),
        );

        let rejected = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.add_ribbon(wrong_scope);
        }));

        assert!(rejected.is_err());
    }

    #[test]
    fn workspace_ctx_rejects_duplicate_ribbon_ids() {
        let (mut stack, accent) = module_workspace_ctx();
        let mut ctx = WorkspaceCtx::new(&mut stack, accent);
        let ribbon_id = egui::Id::new("duplicate.ribbon");
        let first = RibbonSlotDef::new(
            ribbon_id,
            RibbonScope::WorkspaceLevel(ctx.level.id),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            Vec::new(),
        );
        let second = RibbonSlotDef::new(
            ribbon_id,
            RibbonScope::WorkspaceLevel(ctx.level.id),
            RibbonEdge::Right,
            RibbonCluster::Middle,
            Vec::new(),
        );

        ctx.add_ribbon(first);
        let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ctx.add_ribbon(second);
        }));

        assert!(duplicate.is_err());
    }

    #[test]
    fn workspace_ctx_accepts_current_level_ribbons() {
        let (mut stack, accent) = module_workspace_ctx();
        let mut ctx = WorkspaceCtx::new(&mut stack, accent);
        let ribbon = RibbonSlotDef::new(
            egui::Id::new("level.ribbon"),
            RibbonScope::WorkspaceLevel(ctx.level.id),
            RibbonEdge::Top,
            RibbonCluster::Middle,
            Vec::new(),
        );

        ctx.add_ribbon(ribbon);

        assert_eq!(ctx.ribbons().len(), 1);
    }
}
