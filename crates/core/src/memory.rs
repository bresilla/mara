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

pub struct MaraMemoryCtx<'a> {
    pub(crate) ctx: &'a egui::Context,
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
}
