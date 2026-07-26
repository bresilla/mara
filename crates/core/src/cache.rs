//! Frame-scoped memoisation — PLAN.md WS-D1.3.
//!
//! A [`MaraCache`] holds derived data that is expensive to recompute and
//! cheap to keep, and drops whatever went untouched for a frame.
//!
//! ## Why it exists
//!
//! Surfaces that derive geometry from a model — a node graph's bezier
//! wires, a syntax highlighter's token runs — recompute the same result
//! every frame from unchanged inputs. The backend has a frame-cache
//! subsystem for exactly this, but reaching it means naming backend
//! types — which is why `mara_graph`'s wire cache blocks its port.
//!
//! This is the sealed equivalent, and deliberately the *smallest* thing
//! that works: Mara owns the eviction contract, the caller owns the
//! keying and the stored values.
//!
//! ## The contract
//!
//! Implement [`SweptCache`] for your cache type. [`sweep`](SweptCache::sweep)
//! is called **once per frame**, before the first access of that frame,
//! and must drop entries that were not used in the previous frame — the
//! usual shape is a generation counter compared against a stamp on each
//! entry.
//!
//! ```ignore
//! #[derive(Default)]
//! struct WireGeometry {
//!     generation: u64,
//!     entries: HashMap<WireId, Curve>,
//! }
//!
//! impl SweptCache for WireGeometry {
//!     fn sweep(&mut self) {
//!         let current = self.generation;
//!         self.entries.retain(|_, e| e.generation == current);
//!         self.generation += 1;
//!     }
//! }
//! ```
//!
//! Access it through [`crate::memory::MaraMemoryCtx::cache`], which
//! creates the cache on first use and sweeps it once per frame.

use std::sync::{Arc, Mutex, MutexGuard};

/// A cache that evicts on a frame boundary.
pub trait SweptCache: Default + Send + Sync + 'static {
    /// Drop entries unused since the last call. Called once per frame.
    fn sweep(&mut self);
}

/// Shared handle to a [`SweptCache`].
///
/// Cloning shares the same storage — the handle is how the cache
/// survives between frames while living in backend-neutral memory.
pub struct MaraCache<T: SweptCache> {
    inner: Arc<Mutex<T>>,
}

impl<T: SweptCache> Clone for MaraCache<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

impl<T: SweptCache> Default for MaraCache<T> {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(T::default())),
        }
    }
}

impl<T: SweptCache> MaraCache<T> {
    /// Borrow the cache contents.
    ///
    /// Returns `None` only if a previous holder panicked while the lock
    /// was held — callers should recompute rather than propagate, since
    /// a cache miss is always a valid outcome.
    #[must_use]
    pub fn lock(&self) -> Option<MutexGuard<'_, T>> {
        self.inner.lock().ok()
    }

    /// Run `body` against the contents, recomputing on a poisoned lock.
    pub fn with<R>(&self, body: impl FnOnce(&mut T) -> R) -> Option<R> {
        self.lock().map(|mut guard| body(&mut guard))
    }

    pub(crate) fn sweep(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.sweep();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Counted {
        generation: u64,
        entries: HashMap<u32, u64>,
    }

    impl SweptCache for Counted {
        fn sweep(&mut self) {
            let current = self.generation;
            self.entries.retain(|_, stamp| *stamp == current);
            self.generation += 1;
        }
    }

    /// Touching an entry each frame keeps it; skipping a frame drops it.
    /// This is the whole contract — if it holds, a wire cache built on it
    /// behaves like the backend cache it replaces.
    #[test]
    fn sweep_keeps_touched_entries_and_drops_stale_ones() {
        let cache = MaraCache::<Counted>::default();

        cache.with(|c| c.entries.insert(1, c.generation)).unwrap();
        cache.with(|c| c.entries.insert(2, c.generation)).unwrap();
        assert_eq!(cache.with(|c| c.entries.len()), Some(2));

        // Frame boundary, then touch only entry 1.
        cache.sweep();
        cache.with(|c| c.entries.insert(1, c.generation)).unwrap();

        // Next boundary evicts entry 2, which went a frame untouched.
        cache.sweep();
        assert_eq!(cache.with(|c| c.entries.len()), Some(1));
        assert_eq!(cache.with(|c| c.entries.contains_key(&1)), Some(true));
    }

    #[test]
    fn handles_share_one_store() {
        let a = MaraCache::<Counted>::default();
        let b = a.clone();
        a.with(|c| c.entries.insert(7, 0)).unwrap();
        assert_eq!(b.with(|c| c.entries.get(&7).copied()), Some(Some(0)));
    }
}
