use egui::Id;

use crate::{
    view::{ViewId, ViewRouter, ViewRouterError},
    workspace::WorkspaceStackError,
};

use super::RibbonAction;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonActionError {
    View(ViewRouterError),
    Workspace(WorkspaceStackError),
    AppWindowControlsDenied,
}

impl From<ViewRouterError> for RibbonActionError {
    fn from(value: ViewRouterError) -> Self {
        Self::View(value)
    }
}

impl From<WorkspaceStackError> for RibbonActionError {
    fn from(value: WorkspaceStackError) -> Self {
        Self::Workspace(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RibbonActionResult {
    None,
    Command(Id),
    SwitchedView(ViewId),
    PushedModuleWorkspace(Id),
    PoppedWorkspace,
    CloseAppRequested,
    ToggleMaximizeRequested,
}

/// Dispatch a slot-based ribbon action against the active view router.
///
/// Rendering layers may decide how to handle `Command` and
/// `CloseAppRequested`; structural actions update the router or the
/// active view's workspace stack immediately.
pub fn dispatch_ribbon_action(
    action: RibbonAction,
    router: &mut ViewRouter,
) -> Result<RibbonActionResult, RibbonActionError> {
    match action {
        RibbonAction::Command(id) => Ok(RibbonActionResult::Command(id)),
        RibbonAction::SwitchView(view_id) => {
            router.set_active(view_id)?;
            Ok(RibbonActionResult::SwitchedView(view_id))
        }
        RibbonAction::PushModuleWorkspace(module_id) => {
            router.active_workspace_mut()?.push_module(module_id);
            Ok(RibbonActionResult::PushedModuleWorkspace(module_id))
        }
        RibbonAction::PopWorkspace => {
            router.active_workspace_mut()?.pop()?;
            Ok(RibbonActionResult::PoppedWorkspace)
        }
        RibbonAction::CloseApp => {
            if router
                .active_workspace()?
                .current_policy()
                .allow_app_window_controls
            {
                Ok(RibbonActionResult::CloseAppRequested)
            } else {
                Err(RibbonActionError::AppWindowControlsDenied)
            }
        }
        RibbonAction::ToggleMaximize => {
            if router
                .active_workspace()?
                .current_policy()
                .allow_app_window_controls
            {
                Ok(RibbonActionResult::ToggleMaximizeRequested)
            } else {
                Err(RibbonActionError::AppWindowControlsDenied)
            }
        }
        RibbonAction::Noop => Ok(RibbonActionResult::None),
    }
}
