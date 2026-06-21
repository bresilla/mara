use crate::view::ViewId;
use crate::vocab::Id;

/// Action emitted by slot-based ribbon items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RibbonAction {
    Command(Id),
    SwitchView(ViewId),
    PushModuleWorkspace(Id),
    PopWorkspace,
    CloseApp,
    /// Toggle the host window between maximized and restored. Emitted by
    /// the left-edge window control; gated by the same window-controls
    /// policy as [`RibbonAction::CloseApp`].
    ToggleMaximize,
    Noop,
}
