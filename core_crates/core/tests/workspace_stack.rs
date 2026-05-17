use mara_core::{WorkspaceLevelState, WorkspaceOwner, WorkspaceStack, WorkspaceStackError};

#[test]
fn workspace_stack_tracks_root_and_module_levels() {
    let mut stack = WorkspaceStack::new("root");

    assert_eq!(stack.depth(), 0);
    assert!(stack.is_root_active());
    assert!(matches!(stack.current().owner, WorkspaceOwner::Root));
    assert!(stack.current_policy().allow_app_window_controls);
    assert!(stack.current_policy().allow_root_ribbon);
    assert!(!stack.current_policy().allow_module_bars);
    assert!(stack.current_policy().allow_shelves);
    assert!(!stack.current_policy().inherit_root_shelves);

    let l1 = stack.push_module(egui::Id::new("graph"));
    assert_eq!(l1.depth, 1);
    assert_eq!(stack.depth(), 1);
    assert!(!stack.is_root_active());
    assert!(matches!(stack.current().owner, WorkspaceOwner::Module(_)));
    assert!(!stack.current_policy().allow_app_window_controls);
    assert!(!stack.current_policy().allow_root_ribbon);
    assert!(stack.current_policy().allow_module_bars);
    assert!(stack.current_policy().allow_shelves);
    assert!(stack.current_policy().inherit_root_shelves);
    assert!(stack.current_policy().restore_to_parent);

    let l2 = stack.push_module(egui::Id::new("image"));
    assert_eq!(l2.depth, 2);
    assert_eq!(stack.depth(), 2);

    let popped = stack.pop().expect("L2 can pop");
    assert_eq!(popped.depth, 2);
    assert_eq!(stack.depth(), 1);

    stack.pop().expect("L1 can pop");
    assert_eq!(stack.depth(), 0);
    assert_eq!(stack.pop(), Err(WorkspaceStackError::CannotPopRoot));
}

#[test]
fn module_workspace_levels_must_be_l1_or_deeper() {
    let result = std::panic::catch_unwind(|| {
        let _ = WorkspaceLevelState::module(egui::Id::new("bad-level"), 0, egui::Id::new("module"));
    });

    assert!(result.is_err());
}

#[test]
fn workspace_stack_rejects_depth_overflow() {
    let mut stack = WorkspaceStack::new("root");
    for index in 1..=u8::MAX {
        let level = stack.push_module(egui::Id::new(("module", index)));
        assert_eq!(level.depth, index);
    }

    let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = stack.push_module(egui::Id::new("overflow"));
    }));

    assert!(overflow.is_err());
}
