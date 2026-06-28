//! Backend-neutral focus registry + keyboard traversal.
//!
//! This is the first slice of a Mara-owned focus subsystem. Today
//! focusable widgets (text input, dropdown, command palette) rely on
//! egui's focus/tab handling. To make focus backend-agnostic, Mara
//! needs to own which widget currently holds focus and how Tab /
//! Shift+Tab move between widgets, as plain data + pure operations any
//! backend can drive.
//!
//! This module provides exactly the engine-independent core:
//!
//! * [`FocusRegistry`] — per-frame ordered list of focusable widget
//!   [`Id`]s plus the currently-focused id, with `request_focus`,
//!   `clear`, `is_focused`, and `focus_next` / `focus_prev` (Tab /
//!   Shift+Tab) traversal that wraps and recovers gracefully when the
//!   stored focus is stale.
//! * [`load_focus`] / [`store_focus`] — persist the focused id between
//!   frames through [`MaraMemory`].
//!
//! Per frame a backend: loads the focused id, builds a registry,
//! `register`s each focusable widget in tab order, applies traversal
//! from consumed Tab/Shift+Tab keys, then stores the result. The egui
//! backend will feed it; a future backend reuses this verbatim.

use crate::memory::MaraMemory;
use crate::vocab::Id;

/// Stable memory key under which the focused widget id is persisted.
fn focus_key(scope: Id) -> Id {
    scope.with("mara_focus_owner")
}

/// Read the focused widget id for `scope` (default: none).
#[must_use]
pub fn load_focus(memory: &impl MaraMemory, scope: Id) -> Option<Id> {
    memory
        .get_temp::<Option<Id>>(focus_key(scope))
        .unwrap_or(None)
}

/// Persist the focused widget id for `scope`.
pub fn store_focus(memory: &mut impl MaraMemory, scope: Id, focused: Option<Id>) {
    memory.set_temp(focus_key(scope), focused);
}

/// Per-frame focus state: the focusable widgets registered this frame
/// (in tab order) plus which one holds focus.
#[derive(Clone, Debug, Default)]
pub struct FocusRegistry {
    order: Vec<Id>,
    focused: Option<Id>,
}

impl FocusRegistry {
    #[must_use]
    pub fn new(focused: Option<Id>) -> Self {
        Self {
            order: Vec::new(),
            focused,
        }
    }

    /// Register a focusable widget in tab order. Call once per
    /// focusable widget per frame, in the order Tab should visit them.
    pub fn register(&mut self, id: Id) {
        self.order.push(id);
    }

    #[must_use]
    pub fn current(&self) -> Option<Id> {
        self.focused
    }

    #[must_use]
    pub fn is_focused(&self, id: Id) -> bool {
        self.focused == Some(id)
    }

    pub fn request_focus(&mut self, id: Id) {
        self.focused = Some(id);
    }

    pub fn clear(&mut self) {
        self.focused = None;
    }

    /// Index of the focused id in this frame's order, if it is still
    /// present (it may have been removed since last frame).
    fn focused_index(&self) -> Option<usize> {
        let focused = self.focused?;
        self.order.iter().position(|&id| id == focused)
    }

    /// Move focus to the next focusable widget (Tab). Wraps to the
    /// first; if nothing is focused or the stored focus is stale,
    /// focuses the first widget.
    pub fn focus_next(&mut self) {
        if self.order.is_empty() {
            self.focused = None;
            return;
        }
        let next = match self.focused_index() {
            Some(i) => (i + 1) % self.order.len(),
            None => 0,
        };
        self.focused = Some(self.order[next]);
    }

    /// Move focus to the previous focusable widget (Shift+Tab). Wraps
    /// to the last; if nothing is focused or the stored focus is stale,
    /// focuses the last widget.
    pub fn focus_prev(&mut self) {
        if self.order.is_empty() {
            self.focused = None;
            return;
        }
        let prev = match self.focused_index() {
            Some(0) | None => self.order.len() - 1,
            Some(i) => i - 1,
        };
        self.focused = Some(self.order[prev]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MockMemory {
        temp: HashMap<Id, Box<dyn Any + Send + Sync>>,
    }
    impl MaraMemory for MockMemory {
        fn get_persisted<T: Clone + Send + Sync + 'static>(&self, _id: Id) -> Option<T> {
            None
        }
        fn set_persisted<T: Clone + Send + Sync + 'static>(&mut self, _id: Id, _value: T) {}
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

    fn ids() -> (Id, Id, Id) {
        (Id::new("a"), Id::new("b"), Id::new("c"))
    }

    fn registry_of(focused: Option<Id>) -> FocusRegistry {
        let (a, b, c) = ids();
        let mut r = FocusRegistry::new(focused);
        r.register(a);
        r.register(b);
        r.register(c);
        r
    }

    #[test]
    fn focus_persists_through_memory() {
        let mut memory = MockMemory::default();
        let scope = Id::new("scope");
        let (a, _, _) = ids();
        assert_eq!(load_focus(&memory, scope), None);
        store_focus(&mut memory, scope, Some(a));
        assert_eq!(load_focus(&memory, scope), Some(a));
        store_focus(&mut memory, scope, None);
        assert_eq!(load_focus(&memory, scope), None);
    }

    #[test]
    fn tab_from_none_focuses_first() {
        let mut r = registry_of(None);
        r.focus_next();
        assert_eq!(r.current(), Some(ids().0));
    }

    #[test]
    fn shift_tab_from_none_focuses_last() {
        let mut r = registry_of(None);
        r.focus_prev();
        assert_eq!(r.current(), Some(ids().2));
    }

    #[test]
    fn tab_advances_and_wraps() {
        let (a, b, c) = ids();
        let mut r = registry_of(Some(a));
        r.focus_next();
        assert_eq!(r.current(), Some(b));
        r.focus_next();
        assert_eq!(r.current(), Some(c));
        r.focus_next();
        assert_eq!(r.current(), Some(a)); // wrap
    }

    #[test]
    fn shift_tab_retreats_and_wraps() {
        let (a, b, c) = ids();
        let mut r = registry_of(Some(a));
        r.focus_prev();
        assert_eq!(r.current(), Some(c)); // wrap to last
        r.focus_prev();
        assert_eq!(r.current(), Some(b));
    }

    #[test]
    fn stale_focus_recovers_to_first_on_tab() {
        let mut r = registry_of(Some(Id::new("gone")));
        r.focus_next();
        assert_eq!(r.current(), Some(ids().0));
    }

    #[test]
    fn request_and_clear_and_is_focused() {
        let (a, b, _) = ids();
        let mut r = registry_of(None);
        r.request_focus(b);
        assert!(r.is_focused(b));
        assert!(!r.is_focused(a));
        r.clear();
        assert_eq!(r.current(), None);
    }

    #[test]
    fn traversal_on_empty_registry_is_noop() {
        let mut r = FocusRegistry::new(Some(Id::new("x")));
        r.focus_next();
        assert_eq!(r.current(), None);
        r.focus_prev();
        assert_eq!(r.current(), None);
    }
}
