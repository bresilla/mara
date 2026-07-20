//! Headless recording backend — the second [`UiBackend`] implementation.
//!
//! Records the [`PaintCmd`] stream a widget emits instead of lowering it
//! to a rendering engine. This is the proof-of-seam backend from
//! `PLAN.md` Phase 1 / ADR 0001: anything that renders through it is
//! demonstrably egui-free, and its captured commands power the golden
//! paint tests.
//!
//! Semantics are deliberately simple and FROZEN — goldens and 18 widget
//! test suites depend on them:
//!
//! - `allocate` places at `available.min` (no flowing cursor).
//! - `interact` returns the injected [`RecordingBackend::interaction`]
//!   when set, else a synthetic hover-less response at the given rect.
//! - `measure_text` is `chars * size * 0.5` wide and `size` tall.
//!   These constants are a contract; changing them invalidates every
//!   golden snapshot.

use std::any::Any;
use std::collections::HashMap;

use crate::layout::{AreaHost, Sense, UiBackend};
use crate::memory::MaraMemory;
use crate::mui::MaraResponse;
use crate::paint::PaintCmd;
use crate::vocab::{Id, Rect, Vec2};

/// Headless [`MaraMemory`] store — a type-erased `HashMap` per lane.
/// The backend-neutral state cores (popup/focus/scroll) and widget
/// tests persist through this without an egui context.
#[derive(Default)]
pub struct RecordingMemory {
    temp: HashMap<Id, Box<dyn Any + Send + Sync>>,
    persisted: HashMap<Id, Box<dyn Any + Send + Sync>>,
}

impl MaraMemory for RecordingMemory {
    fn get_persisted<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.persisted
            .get(&id)
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
    }

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.persisted.insert(id, Box::new(value));
    }

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp
            .get(&id)
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
    }

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp.insert(id, Box::new(value));
    }
}

/// Headless [`UiBackend`] that records paint commands and clip pushes.
///
/// All fields are public so tests can inject interaction responses and
/// assert on the captured stream directly.
#[derive(Default)]
pub struct RecordingBackend {
    /// The rect `begin_area` was last given; `allocate` places here.
    pub available: Rect,
    /// Append-only log of every clip rect pushed. `pop_clip` does NOT
    /// remove entries — assertions want the full push history, and no
    /// trait consumer reads the live clip state back.
    pub clips: Vec<Rect>,
    /// Every paint command emitted, in order.
    pub paints: Vec<PaintCmd>,
    /// When set, `interact` returns a clone of this response instead of
    /// a synthetic one — lets tests simulate hover/click/drag.
    pub interaction: Option<MaraResponse>,
}

impl RecordingBackend {
    /// Backend spanning `rect`, as if `begin_area` had run.
    pub fn at(rect: Rect) -> Self {
        Self {
            available: rect,
            ..Self::default()
        }
    }
}

impl UiBackend for RecordingBackend {
    fn begin_area(&mut self, _host: AreaHost, rect: Rect) {
        self.available = rect;
    }

    fn allocate(&mut self, size: Vec2, _sense: Sense) -> MaraResponse {
        MaraResponse::synthetic(Rect::from_min_size(self.available.min, size))
    }

    fn interact(&mut self, rect: Rect, _id: Id, _sense: Sense) -> MaraResponse {
        self.interaction
            .clone()
            .unwrap_or_else(|| MaraResponse::synthetic(rect))
    }

    fn available_rect(&self) -> Rect {
        self.available
    }

    fn push_clip(&mut self, rect: Rect) {
        self.clips.push(rect);
    }

    fn pop_clip(&mut self) {}

    fn measure_text(&self, text: &str, size: f32, _mono: bool) -> Vec2 {
        Vec2::new(text.chars().count() as f32 * size * 0.5, size)
    }

    fn paint(&mut self, cmd: PaintCmd) {
        self.paints.push(cmd);
    }
}
