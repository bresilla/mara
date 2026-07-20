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
use crate::memory::{MaraAnim, MaraMemory};
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

/// Headless animation: completes instantly. Goldens and tests pin the
/// settled endpoints; a frame-clock impl can replace this when a real
/// non-egui host needs live animation.
impl MaraAnim for RecordingMemory {
    fn animate_bool(&mut self, _id: Id, value: bool, _animation_time: f32) -> f32 {
        if value { 1.0 } else { 0.0 }
    }

    fn animate_value(&mut self, _id: Id, target: f32, _animation_time: f32) -> f32 {
        target
    }

    fn animate_bool_responsive(&mut self, _id: Id, value: bool) -> f32 {
        if value { 1.0 } else { 0.0 }
    }
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

    /// Id-keyed (the store holds one value per id, unlike egui's
    /// id+type keying — Mara code never stores two types under one id).
    fn remove_temp<T>(&mut self, id: Id)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp.remove(&id);
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
    /// Headless state store vended through [`UiBackend::memory`].
    pub memory: std::cell::RefCell<RecordingMemory>,
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

    fn reserve_paint_slot(&mut self) -> crate::layout::PaintSlot {
        self.paints.push(PaintCmd::Noop);
        crate::layout::PaintSlot(self.paints.len() - 1)
    }

    fn fill_paint_slot(&mut self, slot: crate::layout::PaintSlot, cmd: Option<PaintCmd>) {
        if let (Some(cmd), Some(existing)) = (cmd, self.paints.get_mut(slot.0)) {
            *existing = cmd;
        }
    }

    fn memory(&self) -> crate::memory::BackendMemory<'_> {
        crate::memory::BackendMemory::Recording(&self.memory)
    }
}

/// Golden paint tests — PLAN.md Phase 1.2.
///
/// Each test renders one widget through its `*_backend` fn against a
/// [`RecordingBackend`] and compares the `{:#?}` of the captured
/// [`PaintCmd`] stream to a committed snapshot in
/// `crates/core/tests/golden/`. Regenerate with
/// `MARA_UPDATE_GOLDEN=1 cargo test -p mara_core golden`.
///
/// Goldens render under the process-default theme (`theme_pro` dark);
/// they deliberately do not call `set_theme` (a parallel test doing so
/// mid-render would race — only `style.rs`'s own test does, and it
/// restores the default before finishing).
#[cfg(test)]
mod golden {
    use super::*;
    use crate::vocab::{Color32, Pos2};

    fn golden_check(name: &str, paints: &[PaintCmd]) {
        let path = format!("{}/tests/golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
        let actual = format!("{:#?}\n", paints);
        if std::env::var("MARA_UPDATE_GOLDEN").is_ok() {
            std::fs::create_dir_all(format!("{}/tests/golden", env!("CARGO_MANIFEST_DIR")))
                .expect("create golden dir");
            std::fs::write(&path, &actual).expect("write golden");
            return;
        }
        let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
            panic!("missing golden snapshot {path}; regenerate with MARA_UPDATE_GOLDEN=1")
        });
        assert_eq!(
            expected, actual,
            "golden mismatch for '{name}'; if the change is intentional regenerate with MARA_UPDATE_GOLDEN=1"
        );
    }

    fn frame() -> RecordingBackend {
        RecordingBackend::at(Rect::from_min_size(
            Pos2::new(0.0, 0.0),
            Vec2::new(320.0, 64.0),
        ))
    }

    const ACCENT: Color32 = Color32::from_rgb(120, 180, 255);

    #[test]
    fn golden_label() {
        let mut backend = frame();
        let _ = crate::widget::label::label_backend(&mut backend, "mara", Color32::WHITE);
        golden_check("label", &backend.paints);
    }

    #[test]
    fn golden_toggle_off() {
        let mut backend = frame();
        let mut on = false;
        let _ = crate::widget::toggle::toggle_backend(&mut backend, "dark", &mut on, ACCENT, 24.0);
        golden_check("toggle_off", &backend.paints);
    }

    #[test]
    fn golden_button() {
        let mut backend = frame();
        let _ = crate::widget::button::button_backend(&mut backend, "apply", ACCENT, 24.0);
        golden_check("button", &backend.paints);
    }

    /// A tree row renders headlessly through the converted `tree_row`
    /// — proof the whole widget (indent guides, chevron, label, gutter
    /// slots) lowers to `PaintCmd` with zero egui in the call path.
    #[test]
    fn golden_tree_row() {
        let mut backend = frame();
        let mut expanded = true;
        let mut eye_on = true;
        let mut slots = [crate::widget::tree::TreeIconSlot::new(
            crate::widget::tree::TreeIconKind::Eye,
            &mut eye_on,
        )];
        let _ = crate::widget::tree::tree_row(
            &mut backend,
            "node",
            1,
            Some(&mut expanded),
            Some("folder"),
            "assets",
            true,
            ACCENT,
            &mut slots,
        );
        golden_check("tree_row", &backend.paints);
    }

    /// The Phase 3 exit gate: a full `MaraUi` — the sealed surface app
    /// code uses — renders label/button/toggle/slider over the
    /// recording backend with zero egui in the call path.
    #[test]
    fn golden_mara_ui_over_recording() {
        use crate::mui::{MaraBackend, MaraUi};

        let mut mui = MaraUi::over(MaraBackend::Recording(Box::new(frame())), ACCENT);
        mui.label("headless");
        mui.button("apply");
        let mut on = true;
        mui.toggle("dark", &mut on);
        let mut value = 0.5_f64;
        mui.slider("gain", &mut value, 0.0..=1.0, 2, "");

        let MaraBackend::Recording(backend) = mui.into_backend() else {
            unreachable!("constructed with the recording backend");
        };
        golden_check("mara_ui_over_recording", &backend.paints);
    }
}
