//! Backend-neutral memory facade.
//!
//! This is the first public contract for Mara-owned widget/view
//! memory. The current implementation is backed by egui's context
//! data store, but callers use Mara [`Id`] keys and do not receive a
//! raw backend context.

use crate::vocab::Id;

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
/// A closed enum rather than a trait object: egui's data store is
/// typed (generic get/set only), so a type-erased `dyn` store cannot
/// wrap it without breaking its persistence model. Each backend adds
/// a variant; the generic [`MaraMemory`]/[`MaraAnim`] impls dispatch
/// by match, and the [`UiBackend`](crate::layout::UiBackend) trait
/// stays object-safe because this return type is concrete.
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

pub struct MaraMemoryCtx<'a> {
    pub(crate) ctx: &'a egui::Context,
}

impl MaraAnim for MaraMemoryCtx<'_> {
    fn animate_bool(&mut self, id: Id, value: bool, animation_time: f32) -> f32 {
        self.ctx
            .animate_bool_with_time(id.into(), value, animation_time)
    }

    fn animate_value(&mut self, id: Id, target: f32, animation_time: f32) -> f32 {
        self.ctx
            .animate_value_with_time(id.into(), target, animation_time)
    }

    fn animate_bool_responsive(&mut self, id: Id, value: bool) -> f32 {
        self.ctx.animate_bool_responsive(id.into(), value)
    }
}

impl<'a> MaraMemoryCtx<'a> {
    pub(crate) fn new(ctx: &'a egui::Context) -> Self {
        Self { ctx }
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
}

impl MaraMemory for MaraMemoryCtx<'_> {
    fn get_persisted<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.ctx.data_mut(|data| data.get_persisted::<T>(id.into()))
    }

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.ctx
            .data_mut(|data| data.insert_persisted(id.into(), value));
    }

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.ctx.data(|data| data.get_temp::<T>(id.into()))
    }

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.ctx.data_mut(|data| data.insert_temp(id.into(), value));
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

        assert_eq!(memory.animate_bool(key.with("a"), true, 0.25), 1.0);
        assert_eq!(memory.animate_bool(key.with("a"), false, 0.25), 0.0);
        assert_eq!(memory.animate_value(key.with("v"), 3.5, 0.25), 3.5);
    }
}
