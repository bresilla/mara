//! Backend-neutral popup open-state + dismissal contract.
//!
//! This is the first slice of a Mara-owned popup subsystem. Today the
//! dropdown/select popups delegate open-state, click-outside
//! dismissal, and escape handling to egui's `Popup` system. To make
//! popups backend-agnostic, Mara needs to own that behaviour as plain
//! data + pure decisions that any backend can drive.
//!
//! This module provides exactly the engine-independent core:
//!
//! * [`PopupState`] — open/closed state persisted through
//!   [`MaraMemory`] under a stable Mara [`Id`], with `toggle` / `open`
//!   / `close`.
//! * [`popup_should_dismiss`] — a pure decision (no egui) for whether
//!   an open popup should close this frame, given a [`MaraInput`]
//!   snapshot, the popup body rect, the trigger rect, and whether an
//!   escape key was consumed.
//!
//! The egui backend will host the concrete `Area` and feed these the
//! input/keys it already snapshots; a future custom backend implements
//! the same two seams and reuses this logic verbatim.

use crate::memory::MaraMemory;
use crate::mui::MaraInput;
use crate::vocab::{Id, Rect};

/// Open/closed state for a Mara popup, keyed by a stable popup id.
///
/// Stored as frame-temp memory: popups are transient UI and should not
/// survive a reload, matching egui's popup semantics today.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PopupState {
    pub open: bool,
}

impl PopupState {
    #[must_use]
    pub const fn new(open: bool) -> Self {
        Self { open }
    }

    /// Read the current open-state for `popup_id` (default: closed).
    #[must_use]
    pub fn load(memory: &impl MaraMemory, popup_id: Id) -> Self {
        Self {
            open: memory.get_temp::<bool>(popup_id).unwrap_or(false),
        }
    }

    /// Persist this open-state for `popup_id`.
    pub fn store(self, memory: &mut impl MaraMemory, popup_id: Id) {
        memory.set_temp(popup_id, self.open);
    }

    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    pub fn open(&mut self) {
        self.open = true;
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    #[must_use]
    pub fn is_open(self) -> bool {
        self.open
    }
}

/// Pure dismissal decision for an already-open popup.
///
/// Returns `true` when the popup should close this frame:
///
/// * an escape key was consumed (`escape_pressed`), or
/// * a primary press landed outside **both** the popup body
///   (`popup_rect`) and its trigger (`trigger_rect`) — the trigger is
///   excluded so the press that toggles the popup closed is handled by
///   the trigger itself, not double-counted as an outside dismissal.
///
/// This is backend-neutral: the egui backend (and any future backend)
/// supplies the [`MaraInput`] snapshot and the consumed-escape flag.
#[must_use]
pub fn popup_should_dismiss(
    input: &MaraInput,
    popup_rect: Rect,
    trigger_rect: Rect,
    escape_pressed: bool,
) -> bool {
    if escape_pressed {
        return true;
    }
    if input.primary_pressed
        && let Some(pointer) = input.interact_pointer.or(input.pointer)
    {
        return !popup_rect.contains(pointer) && !trigger_rect.contains(pointer);
    }
    false
}

/// One full backend-neutral popup interaction step.
///
/// Folds trigger toggling and dismissal into a single pure transition,
/// so a widget's per-frame popup logic becomes:
///
/// 1. read [`PopupState::load`],
/// 2. render the trigger and (if open) the body, collecting
///    `trigger_clicked`, the popup body rect, and the trigger rect,
/// 3. call `step_popup(...)`,
/// 4. [`PopupState::store`] the result.
///
/// Semantics:
///
/// * closed + trigger clicked → open;
/// * open + trigger clicked → close (the trigger acts as a toggle);
/// * open + [`popup_should_dismiss`] → close;
/// * otherwise unchanged.
///
/// The trigger-click branch is checked before the outside-press
/// dismissal, and [`popup_should_dismiss`] already excludes the trigger
/// rect, so the press that toggles a popup closed is never
/// double-counted as an outside dismissal.
pub fn step_popup(
    state: &mut PopupState,
    trigger_clicked: bool,
    input: &MaraInput,
    popup_rect: Rect,
    trigger_rect: Rect,
    escape_pressed: bool,
) {
    if state.open {
        // Trigger-click closes (toggle); `popup_should_dismiss` already
        // excludes the trigger rect, so an outside press never
        // double-counts. `||` short-circuits, so a trigger click closes
        // without re-evaluating the dismissal check.
        if trigger_clicked || popup_should_dismiss(input, popup_rect, trigger_rect, escape_pressed)
        {
            state.close();
        }
    } else if trigger_clicked {
        state.open();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Pos2, Vec2};
    use std::any::Any;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockMemory {
        temp: HashMap<Id, Box<dyn Any + Send + Sync>>,
        persisted: HashMap<Id, Box<dyn Any + Send + Sync>>,
    }

    impl MaraMemory for MockMemory {
        fn get_persisted<T: Clone + Send + Sync + 'static>(&self, id: Id) -> Option<T> {
            self.persisted
                .get(&id)
                .and_then(|v| v.downcast_ref::<T>())
                .cloned()
        }
        fn set_persisted<T: Clone + Send + Sync + 'static>(&mut self, id: Id, value: T) {
            self.persisted.insert(id, Box::new(value));
        }
        fn get_temp<T: Clone + Send + Sync + 'static>(&self, id: Id) -> Option<T> {
            self.temp
                .get(&id)
                .and_then(|v| v.downcast_ref::<T>())
                .cloned()
        }
        fn set_temp<T: Clone + Send + Sync + 'static>(&mut self, id: Id, value: T) {
            self.temp.insert(id, Box::new(value));
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h))
    }

    #[test]
    fn open_state_defaults_closed_and_round_trips_through_memory() {
        let mut memory = MockMemory::default();
        let id = Id::new("popup");

        assert!(!PopupState::load(&memory, id).is_open());

        let mut state = PopupState::load(&memory, id);
        state.toggle();
        assert!(state.is_open());
        state.store(&mut memory, id);

        assert!(PopupState::load(&memory, id).is_open());

        let mut reopened = PopupState::load(&memory, id);
        reopened.close();
        reopened.store(&mut memory, id);
        assert!(!PopupState::load(&memory, id).is_open());
    }

    #[test]
    fn open_close_toggle_transitions() {
        let mut state = PopupState::default();
        assert!(!state.is_open());
        state.open();
        assert!(state.is_open());
        state.open();
        assert!(state.is_open());
        state.toggle();
        assert!(!state.is_open());
    }

    fn press_at(pointer: Pos2) -> MaraInput {
        MaraInput {
            primary_pressed: true,
            interact_pointer: Some(pointer),
            ..MaraInput::default()
        }
    }

    #[test]
    fn escape_always_dismisses() {
        let input = MaraInput::default();
        assert!(popup_should_dismiss(
            &input,
            rect(0.0, 0.0, 10.0, 10.0),
            rect(0.0, 20.0, 10.0, 10.0),
            true,
        ));
    }

    #[test]
    fn press_inside_popup_or_trigger_does_not_dismiss() {
        let popup = rect(0.0, 0.0, 100.0, 50.0);
        let trigger = rect(0.0, 60.0, 100.0, 20.0);

        assert!(!popup_should_dismiss(
            &press_at(Pos2::new(10.0, 10.0)),
            popup,
            trigger,
            false,
        ));
        assert!(!popup_should_dismiss(
            &press_at(Pos2::new(10.0, 65.0)),
            popup,
            trigger,
            false,
        ));
    }

    #[test]
    fn press_outside_both_dismisses() {
        let popup = rect(0.0, 0.0, 100.0, 50.0);
        let trigger = rect(0.0, 60.0, 100.0, 20.0);
        assert!(popup_should_dismiss(
            &press_at(Pos2::new(200.0, 200.0)),
            popup,
            trigger,
            false,
        ));
    }

    #[test]
    fn no_press_no_escape_keeps_open() {
        let input = MaraInput::default();
        assert!(!popup_should_dismiss(
            &input,
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            false,
        ));
    }

    #[test]
    fn step_opens_closed_popup_on_trigger_click() {
        let mut state = PopupState::default();
        step_popup(
            &mut state,
            /* trigger_clicked */ true,
            &MaraInput::default(),
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            false,
        );
        assert!(state.is_open());
    }

    #[test]
    fn step_toggles_open_popup_closed_on_trigger_click() {
        let mut state = PopupState::new(true);
        // A press lands on the trigger this frame; trigger-click must
        // win and close, NOT be treated as an outside-press dismissal.
        step_popup(
            &mut state,
            /* trigger_clicked */ true,
            &press_at(Pos2::new(10.0, 65.0)),
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            false,
        );
        assert!(!state.is_open());
    }

    #[test]
    fn step_dismisses_open_popup_on_outside_press() {
        let mut state = PopupState::new(true);
        step_popup(
            &mut state,
            /* trigger_clicked */ false,
            &press_at(Pos2::new(500.0, 500.0)),
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            false,
        );
        assert!(!state.is_open());
    }

    #[test]
    fn step_keeps_open_on_press_inside_body() {
        let mut state = PopupState::new(true);
        step_popup(
            &mut state,
            /* trigger_clicked */ false,
            &press_at(Pos2::new(10.0, 10.0)),
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            false,
        );
        assert!(state.is_open());
    }

    #[test]
    fn step_dismisses_open_popup_on_escape() {
        let mut state = PopupState::new(true);
        step_popup(
            &mut state,
            /* trigger_clicked */ false,
            &MaraInput::default(),
            rect(0.0, 0.0, 100.0, 50.0),
            rect(0.0, 60.0, 100.0, 20.0),
            /* escape */ true,
        );
        assert!(!state.is_open());
    }
}
