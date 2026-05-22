use egui::Id;

use crate::view::ViewId;

/// Action emitted by slot-based ribbon items.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RibbonAction {
    Command(Id),
    SwitchView(ViewId),
    PushModuleWorkspace(Id),
    PopWorkspace,
    CloseApp,
    Noop,
}
