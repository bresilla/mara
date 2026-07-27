//! Backend-neutral memory facade.
//!
//! This is the first public contract for Mara-owned widget/view
//! memory. The current implementation is backed by egui's context
//! data store, but callers use Mara [`Id`] keys and do not receive a
//! raw backend context.

use crate::vocab::Id;
use std::any::{Any, TypeId};
use std::sync::Arc;

/// A type-erased state value.
///
/// `Arc` rather than `Box` because a read hands the value back out
/// while the store keeps it: the generic wrapper downcasts, clones the
/// `T` it wanted, and drops the handle. A `Box` would force the store
/// to give up ownership just to be read.
pub type StateCell = Arc<dyn Any + Send + Sync>;

/// The object-safe half of memory — what a backend must supply.
///
/// [`MaraMemory`] cannot ride behind `dyn`: every method is generic
/// over the stored type. This is the erased form underneath it. Keys
/// carry a [`TypeId`] so two values of different types can share an
/// [`Id`], which is what the backend stores already allow and what
/// callers rely on.
///
/// Every method takes `&self`: a frame's state is written through
/// shared handles, so interior mutability belongs to the
/// implementation rather than to every caller's borrow.
pub trait MaraStore {
    /// Fetch the value of type `ty` under `id`, from the persisted or
    /// the temp half of the store.
    fn get_any(&self, id: Id, persisted: bool, ty: TypeId) -> Option<StateCell>;

    /// Store `value` under `id`, replacing any value of the same type.
    fn set_any(&self, id: Id, persisted: bool, ty: TypeId, value: StateCell);

    /// Drop the temp value of type `ty` under `id`, if any.
    fn remove_any(&self, id: Id, ty: TypeId);

    /// 0.0→1.0 progress toward `value` over `animation_time` seconds.
    fn animate_bool(&self, id: Id, value: bool, animation_time: f32) -> f32;

    /// Value eased toward `target` over `animation_time`.
    fn animate_value(&self, id: Id, target: f32, animation_time: f32) -> f32;

    /// [`animate_bool`](MaraStore::animate_bool) at the host's default
    /// duration.
    fn animate_bool_responsive(&self, id: Id, value: bool) -> f32;

    /// Monotonic frame counter — the sweep clock behind
    /// [`MaraMemoryCtx::cache`].
    fn pass_nr(&self) -> u64;
}

pub trait MaraMemory {
    fn get_persisted<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static;

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static;

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static;

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static;

    /// Remove the temp value of type `T` stored under `id`, if any.
    fn remove_temp<T>(&mut self, id: Id)
    where
        T: Clone + Send + Sync + 'static;
}

/// Animation clock on the memory contract — PLAN.md Phase 2.1.
///
/// Widgets animate through this trait instead of the egui context, so
/// a non-egui backend only has to supply a clock. Object-safe: takes
/// concrete [`Id`]s so it can ride behind `dyn` with the memory store.
pub trait MaraAnim {
    /// 0.0→1.0 progress toward `value` over `animation_time` seconds.
    fn animate_bool(&mut self, id: Id, value: bool, animation_time: f32) -> f32;
    /// Persisted value eased toward `target` over `animation_time`.
    fn animate_value(&mut self, id: Id, target: f32, animation_time: f32) -> f32;
    /// [`MaraAnim::animate_bool`] with the backend's default duration.
    fn animate_bool_responsive(&mut self, id: Id, value: bool) -> f32;
}

/// Memory handle vended by [`UiBackend::memory`](crate::layout::UiBackend::memory)
/// — PLAN.md Phase 2.2.
///
/// A closed enum rather than a trait object because the generic
/// [`MaraMemory`]/[`MaraAnim`] methods are not object-safe; dispatch
/// is by match, and the [`UiBackend`](crate::layout::UiBackend) trait
/// stays object-safe because this return type is concrete. The
/// *erasure* lives one level down, in [`MaraStore`].
pub enum BackendMemory<'a> {
    /// Live egui context store (interior-mutable).
    Egui(MaraMemoryCtx<'a>),
    /// Headless recording store; `RefCell` because reads come through
    /// `&self` backends while the map needs `&mut` internally.
    Recording(&'a std::cell::RefCell<crate::backend::record::RecordingMemory>),
}

impl MaraMemory for BackendMemory<'_> {
    fn get_persisted<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        match self {
            Self::Egui(memory) => memory.get_persisted(id),
            Self::Recording(cell) => cell.borrow().get_persisted(id),
        }
    }

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        match self {
            Self::Egui(memory) => memory.set_persisted(id, value),
            Self::Recording(cell) => cell.borrow_mut().set_persisted(id, value),
        }
    }

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        match self {
            Self::Egui(memory) => memory.get_temp(id),
            Self::Recording(cell) => cell.borrow().get_temp(id),
        }
    }

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        match self {
            Self::Egui(memory) => memory.set_temp(id, value),
            Self::Recording(cell) => cell.borrow_mut().set_temp(id, value),
        }
    }

    fn remove_temp<T>(&mut self, id: Id)
    where
        T: Clone + Send + Sync + 'static,
    {
        match self {
            Self::Egui(memory) => MaraMemory::remove_temp::<T>(memory, id),
            Self::Recording(cell) => MaraMemory::remove_temp::<T>(&mut *cell.borrow_mut(), id),
        }
    }
}

impl MaraAnim for BackendMemory<'_> {
    fn animate_bool(&mut self, id: Id, value: bool, animation_time: f32) -> f32 {
        match self {
            Self::Egui(memory) => memory.animate_bool(id, value, animation_time),
            Self::Recording(cell) => cell.borrow_mut().animate_bool(id, value, animation_time),
        }
    }

    fn animate_value(&mut self, id: Id, target: f32, animation_time: f32) -> f32 {
        match self {
            Self::Egui(memory) => memory.animate_value(id, target, animation_time),
            Self::Recording(cell) => cell.borrow_mut().animate_value(id, target, animation_time),
        }
    }

    fn animate_bool_responsive(&mut self, id: Id, value: bool) -> f32 {
        match self {
            Self::Egui(memory) => memory.animate_bool_responsive(id, value),
            Self::Recording(cell) => cell.borrow_mut().animate_bool_responsive(id, value),
        }
    }
}

/// Typed memory over an erased [`MaraStore`].
///
/// The store answers in [`StateCell`]s; this puts the types back on,
/// so callers keep writing `get_temp::<Foo>(id)` and never see the
/// erasure underneath.
pub struct MaraMemoryCtx<'a> {
    pub(crate) store: &'a dyn MaraStore,
}

impl MaraAnim for MaraMemoryCtx<'_> {
    fn animate_bool(&mut self, id: Id, value: bool, animation_time: f32) -> f32 {
        self.store.animate_bool(id, value, animation_time)
    }

    fn animate_value(&mut self, id: Id, target: f32, animation_time: f32) -> f32 {
        self.store.animate_value(id, target, animation_time)
    }

    fn animate_bool_responsive(&mut self, id: Id, value: bool) -> f32 {
        self.store.animate_bool_responsive(id, value)
    }
}

impl<'a> MaraMemoryCtx<'a> {
    pub(crate) fn new(store: &'a dyn MaraStore) -> Self {
        Self { store }
    }

    /// First-party hook: build a memory facade from a host's backend
    /// context. Used by `mara::extras::*`, which lives outside this
    /// crate but is still first-party. Doc-hidden; not a stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_from_backend_ctx(store: &'a dyn MaraStore) -> Self {
        Self::new(store)
    }

    /// Read `T` out of the store, putting the type back on.
    fn read<T>(&self, id: Id, persisted: bool) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.store
            .get_any(id, persisted, TypeId::of::<T>())?
            .downcast_ref::<T>()
            .cloned()
    }

    /// A frame-scoped memoisation cache, created on first use.
    ///
    /// Swept once per frame — the first call in a frame drops entries
    /// the previous frame did not touch (see [`crate::cache`]). Use it
    /// for derived geometry that is expensive to recompute and cheap to
    /// keep: bezier curves, tessellated glyph runs, laid-out text.
    ///
    /// `id` separates independent caches of the same type, so two graph
    /// instances do not evict each other's entries.
    #[must_use]
    pub fn cache<T: crate::cache::SweptCache>(
        &mut self,
        id: impl Into<Id>,
    ) -> crate::cache::MaraCache<T> {
        let id = id.into();
        let handle = match self.get_temp::<crate::cache::MaraCache<T>>(id) {
            Some(existing) => existing,
            None => {
                let fresh = crate::cache::MaraCache::<T>::default();
                self.set_temp(id, fresh.clone());
                fresh
            }
        };

        // Sweep at most once per frame, keyed by the host's pass number
        // so repeated access within a frame is free.
        let pass = self.store.pass_nr();
        let swept_key = id.with("mara_cache_swept_pass");
        if self.get_temp::<u64>(swept_key) != Some(pass) {
            handle.sweep();
            self.set_temp(swept_key, pass);
        }
        handle
    }

    #[must_use]
    pub fn get_persisted<T>(&self, id: impl Into<Id>) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        <Self as MaraMemory>::get_persisted(self, id.into())
    }

    pub fn set_persisted<T>(&mut self, id: impl Into<Id>, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        <Self as MaraMemory>::set_persisted(self, id.into(), value);
    }

    #[must_use]
    pub fn get_temp<T>(&self, id: impl Into<Id>) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        <Self as MaraMemory>::get_temp(self, id.into())
    }

    pub fn set_temp<T>(&mut self, id: impl Into<Id>, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        <Self as MaraMemory>::set_temp(self, id.into(), value);
    }

    pub fn remove_temp<T>(&mut self, id: impl Into<Id>)
    where
        T: Clone + Send + Sync + 'static,
    {
        <Self as MaraMemory>::remove_temp::<T>(self, id.into());
    }
}

impl MaraMemory for MaraMemoryCtx<'_> {
    fn get_persisted<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.read(id, true)
    }

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.store
            .set_any(id, true, TypeId::of::<T>(), Arc::new(value));
    }

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.read(id, false)
    }

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.store
            .set_any(id, false, TypeId::of::<T>(), Arc::new(value));
    }

    fn remove_temp<T>(&mut self, id: Id)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.store.remove_any(id, TypeId::of::<T>());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_ctx_uses_mara_ids_for_temp_and_persisted_values() {
        let ctx = egui::Context::default();
        let key = Id::new("memory-test");

        let mut memory = MaraMemoryCtx::new(&ctx);
        memory.set_temp(key, "frame".to_owned());
        memory.set_persisted(key.with("persisted"), 7_u32);

        assert_eq!(memory.get_temp::<String>(key), Some("frame".to_owned()));
        assert_eq!(memory.get_persisted::<u32>(key.with("persisted")), Some(7));
    }

    /// The store keys on `(id, persisted, type)`. Each component of
    /// that key is load-bearing, and getting one wrong loses or
    /// aliases state silently — a read simply returns `None` and the
    /// caller falls back to a default. These pin all three.
    #[test]
    fn erased_store_keys_on_id_and_type_and_half() {
        let ctx = egui::Context::default();
        let key = Id::new("erasure");
        let mut memory = MaraMemoryCtx::new(&ctx);

        // Type is part of the key: two types share one id without
        // clobbering each other, as the backend stores allow.
        memory.set_temp(key, 1_u32);
        memory.set_temp(key, "one".to_owned());
        assert_eq!(memory.get_temp::<u32>(key), Some(1));
        assert_eq!(memory.get_temp::<String>(key), Some("one".to_owned()));

        // A type that was never stored reads as absent rather than
        // downcasting some other value that happens to share the id.
        assert_eq!(memory.get_temp::<i64>(key), None);

        // Persisted and temp are separate halves of the store, so the
        // same id and type can hold a different value in each.
        memory.set_persisted(key, 2_u32);
        assert_eq!(memory.get_persisted::<u32>(key), Some(2));
        assert_eq!(
            memory.get_temp::<u32>(key),
            Some(1),
            "writing the persisted half must not disturb the temp one"
        );

        // `remove_temp` names the temp half only.
        memory.remove_temp::<u32>(key);
        assert_eq!(memory.get_temp::<u32>(key), None);
        assert_eq!(
            memory.get_persisted::<u32>(key),
            Some(2),
            "removing a temp value must leave the persisted one alone"
        );
        assert_eq!(
            memory.get_temp::<String>(key),
            Some("one".to_owned()),
            "removing one type must leave another type at the same id alone"
        );
    }

    /// Two facades built from the same context see one store — state
    /// is keyed by the host, not by the handle that reached it.
    #[test]
    fn facades_over_one_context_share_a_store() {
        let ctx = egui::Context::default();
        let key = Id::new("shared");

        MaraMemoryCtx::new(&ctx).set_temp(key, 5_u32);
        assert_eq!(MaraMemoryCtx::new(&ctx).get_temp::<u32>(key), Some(5));
    }

    #[test]
    fn backend_memory_recording_roundtrip_and_instant_anim() {
        use crate::layout::UiBackend;

        let backend = crate::backend::record::RecordingBackend::default();
        let mut memory = backend.memory();
        let key = Id::new("record-test");

        memory.set_persisted(key, 7_u32);
        memory.set_temp(key, "frame".to_owned());
        assert_eq!(memory.get_persisted::<u32>(key), Some(7));
        assert_eq!(memory.get_temp::<String>(key), Some("frame".to_owned()));

        // Same keying contract as the egui store — see
        // `erased_store_keys_on_id_and_type_and_half`. A headless run
        // that aliased these would make a test pass for the wrong
        // reason.
        memory.set_temp(key, 1_u8);
        assert_eq!(memory.get_temp::<String>(key), Some("frame".to_owned()));
        assert_eq!(memory.get_temp::<u8>(key), Some(1));
        memory.remove_temp::<u8>(key);
        assert_eq!(memory.get_temp::<String>(key), Some("frame".to_owned()));
        assert_eq!(memory.get_persisted::<u32>(key), Some(7));

        assert_eq!(memory.animate_bool(key.with("a"), true, 0.25), 1.0);
        assert_eq!(memory.animate_bool(key.with("a"), false, 0.25), 0.0);
        assert_eq!(memory.animate_value(key.with("v"), 3.5, 0.25), 3.5);
    }
}
