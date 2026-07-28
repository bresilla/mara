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
//! - `allocate`/`reserve_space` place at a top-down flow cursor and
//!   advance it, so widgets stack vertically like egui's default `Ui`.
//! - `interact` returns the injected [`RecordingBackend::interaction`]
//!   when set, else a synthetic hover-less response at the given rect.
//! - `measure_text` is `chars * size * 0.5` wide and `size` tall.
//!   These constants are a contract; changing them invalidates every
//!   golden snapshot.

use std::any::TypeId;
use std::collections::HashMap;

use crate::layout::{AreaHost, Sense, UiBackend};
use crate::memory::{MaraAnim, MaraMemory};
use crate::mui::MaraResponse;
use crate::paint::PaintCmd;
use crate::vocab::{Id, Pos2, Rect, Vec2};

/// Headless [`MaraMemory`] store — a type-erased `HashMap` per lane.
/// The backend-neutral state cores (popup/focus/scroll) and widget
/// tests persist through this without an egui context.
///
/// Keyed by id **and** type, matching the egui store. Keying on id
/// alone would let a headless run silently overwrite a value that the
/// same code keeps distinct on a real backend — a divergence that
/// shows up as a test passing for the wrong reason.
#[derive(Default)]
pub struct RecordingMemory {
    temp: HashMap<(Id, TypeId), crate::memory::StateCell>,
    persisted: HashMap<(Id, TypeId), crate::memory::StateCell>,
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
            .get(&(id, TypeId::of::<T>()))
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
    }

    fn set_persisted<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.persisted
            .insert((id, TypeId::of::<T>()), std::sync::Arc::new(value));
    }

    fn get_temp<T>(&self, id: Id) -> Option<T>
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp
            .get(&(id, TypeId::of::<T>()))
            .and_then(|value| value.downcast_ref::<T>())
            .cloned()
    }

    fn set_temp<T>(&mut self, id: Id, value: T)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp
            .insert((id, TypeId::of::<T>()), std::sync::Arc::new(value));
    }

    fn remove_temp<T>(&mut self, id: Id)
    where
        T: Clone + Send + Sync + 'static,
    {
        self.temp.remove(&(id, TypeId::of::<T>()));
    }
}

/// Headless animation is instant, so the store needs no clock — but it
/// still has to answer as a [`MaraStore`](crate::memory::MaraStore) so
/// a recording surface can vend a real [`MaraMemoryCtx`].
impl crate::memory::MaraStore for std::cell::RefCell<RecordingMemory> {
    fn get_any(&self, id: Id, persisted: bool, ty: TypeId) -> Option<crate::memory::StateCell> {
        let memory = self.borrow();
        let lane = if persisted {
            &memory.persisted
        } else {
            &memory.temp
        };
        lane.get(&(id, ty)).cloned()
    }

    fn set_any(&self, id: Id, persisted: bool, ty: TypeId, value: crate::memory::StateCell) {
        let mut memory = self.borrow_mut();
        let lane = if persisted {
            &mut memory.persisted
        } else {
            &mut memory.temp
        };
        lane.insert((id, ty), value);
    }

    fn remove_any(&self, id: Id, ty: TypeId) {
        self.borrow_mut().temp.remove(&(id, ty));
    }

    fn animate_bool(&self, _id: Id, value: bool, _animation_time: f32) -> f32 {
        if value { 1.0 } else { 0.0 }
    }

    fn animate_value(&self, _id: Id, target: f32, _animation_time: f32) -> f32 {
        target
    }

    fn animate_bool_responsive(&self, _id: Id, value: bool) -> f32 {
        if value { 1.0 } else { 0.0 }
    }

    fn pass_nr(&self) -> u64 {
        0
    }
}

/// Headless [`UiBackend`] that records paint commands and clip pushes.
///
/// All fields are public so tests can inject interaction responses and
/// assert on the captured stream directly.
#[derive(Default)]
pub struct RecordingBackend {
    /// The region `begin_area` was last given — the flow container.
    pub available: Rect,
    /// Top-down layout cursor: `allocate`/`reserve_space`/`add_space`
    /// place at the cursor and advance it down, so a sequence of
    /// widgets stacks vertically like egui's default top-down `Ui`
    /// (rather than overlapping at the region origin). Reset to the
    /// region's top-left by `begin_area`.
    pub cursor: Pos2,
    /// Append-only log of every clip rect pushed. `pop_clip` does NOT
    /// remove entries — assertions want the full push history.
    pub clips: Vec<Rect>,
    /// Live clip stack, kept alongside the history because a scoped
    /// clip has to be readable back: a surface asks what it is clipped
    /// to, and a headless backend that always answered "everything"
    /// would let a clipping bug pass its own tests.
    pub(crate) clip_stack: Vec<Rect>,
    /// Every paint command emitted, in order.
    pub paints: Vec<PaintCmd>,
    /// When set, `interact` returns a clone of this response instead of
    /// a synthetic one — lets tests simulate hover/click/drag.
    pub interaction: Option<MaraResponse>,
    /// Headless state store vended through [`UiBackend::memory`].
    pub memory: std::cell::RefCell<RecordingMemory>,
    /// When set (inside an [`UiBackend::in_scope`] horizontal scope),
    /// `allocate` advances the cursor rightward instead of downward.
    pub flow_horizontal: bool,
    /// Deepest bottom edge reached in the current horizontal row, so
    /// the parent can resume flow below the tallest item.
    pub row_bottom: f32,
    /// Nested id-scope salts pushed by [`UiBackend::in_id_scope`], so
    /// `id()` yields unique ids per scope (egui's id stack, headless).
    pub id_stack: Vec<Id>,
    /// Every overlay opened this pass, so a headless test can assert a
    /// menu appeared and where it was anchored.
    pub overlays: Vec<(Id, Pos2)>,
    /// Last transform applied via [`UiBackend::set_layer_transform`],
    /// so a headless test can assert a surface panned/zoomed.
    pub layer_transform: Option<crate::transform::Transform>,
    /// Bounds actually occupied so far — `None` until something is
    /// allocated or explicitly expanded into.
    pub occupied: Option<Rect>,
    /// Every canvas painter handed out by [`UiBackend::make_painter`],
    /// retained so a test can read back what a module's `on_draw` /
    /// canvas body emitted. `MaraPainter` is `Clone` and shares its
    /// command buffer, so a stored clone sees the same commands the
    /// caller drew into. Interior mutability because `make_painter`
    /// takes `&self`.
    pub canvas_painters: std::cell::RefCell<Vec<crate::mui::MaraPainter>>,
}

impl RecordingBackend {
    /// Backend spanning `rect`, as if `begin_area` had run — the flow
    /// cursor starts at the region's top-left.
    pub fn at(rect: Rect) -> Self {
        Self {
            available: rect,
            cursor: rect.min,
            ..Self::default()
        }
    }

    /// Every `PaintCmd` drawn into a canvas painter this backend handed
    /// out via [`UiBackend::make_painter`], flattened in creation order.
    /// This is how a headless test reads back what a module's canvas
    /// body / `on_draw` emitted.
    #[must_use]
    pub fn canvas_commands(&self) -> Vec<PaintCmd> {
        self.canvas_painters
            .borrow()
            .iter()
            .flat_map(|p| p.__internal_recorded_commands())
            .collect()
    }

    /// Place `size` at the cursor and advance it along the current flow
    /// axis (down by default, right inside a horizontal scope).
    fn advance(&mut self, size: Vec2) -> Rect {
        let rect = Rect::from_min_size(self.cursor, size);
        if self.flow_horizontal {
            self.cursor.x += size.x;
            self.row_bottom = self.row_bottom.max(rect.max.y);
        } else {
            self.cursor.y += size.y;
        }
        rect
    }
}

/// A recording surface is its own context.
///
/// Every surface can hand out a [`MaraCtx`](crate::context::MaraCtx) —
/// that is what lets render code reach frame state without being given
/// a backend handle. Headless, "the frame" is just this backend: the
/// region it was told to fill, the store it already owns, and a clock
/// that never advances.
impl crate::context::MaraCtx for RecordingBackend {
    fn input(&self) -> crate::mui::MaraInput {
        crate::mui::MaraInput::default()
    }

    fn pass_nr(&self) -> u64 {
        0
    }

    fn content_rect(&self) -> Rect {
        self.available
    }

    fn pixels_per_point(&self) -> f32 {
        1.0
    }

    fn request_repaint(&self) {}

    fn request_repaint_after(&self, _after: std::time::Duration) {}

    fn now(&self) -> f64 {
        0.0
    }

    fn dt(&self) -> f32 {
        0.0
    }

    fn memory(&self) -> crate::memory::MaraMemoryCtx<'_> {
        crate::memory::MaraMemoryCtx::__internal_from_backend_ctx(&self.memory)
    }
}

impl UiBackend for RecordingBackend {
    fn begin_area(&mut self, _host: AreaHost, rect: Rect) {
        self.available = rect;
        self.cursor = rect.min;
    }

    fn allocate(&mut self, size: Vec2, _sense: Sense) -> MaraResponse {
        MaraResponse::synthetic(self.advance(size))
    }

    fn reserve_space(&mut self, size: Vec2) -> Rect {
        self.advance(size)
    }

    fn add_space(&mut self, spec: crate::layout::SpaceSpec) {
        self.cursor.y += spec.size.y;
    }

    fn interact(&mut self, rect: Rect, _id: Id, _sense: Sense) -> MaraResponse {
        self.interaction
            .clone()
            .unwrap_or_else(|| MaraResponse::synthetic(rect))
    }

    fn available_rect(&self) -> Rect {
        // Remaining flow region below the cursor.
        Rect::from_min_max(self.cursor, self.available.max)
    }

    fn inline_picker_scope(
        &mut self,
        _spec: crate::layout::InlinePickerSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        // No per-scope style here; the picker still draws.
        body(self);
    }

    fn constrain_to(&mut self, rect: Rect) {
        // Headless: the surface's own extent *is* its available rect,
        // so pinning it is what "constrained" means here.
        self.available = rect;
        self.cursor = rect.min;
    }

    fn push_clip(&mut self, rect: Rect) {
        self.clips.push(rect);
        // Clips only ever shrink.
        let effective = match self.clip_stack.last() {
            Some(current) => current.intersect(rect),
            None => rect,
        };
        self.clip_stack.push(effective);
    }

    fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    fn measure_text(&self, text: &str, size: f32, _mono: bool) -> Vec2 {
        Vec2::new(text.chars().count() as f32 * size * 0.5, size)
    }

    fn paint(&mut self, cmd: PaintCmd) {
        self.paints.push(cmd);
    }

    fn make_painter(&self, spec: crate::layout::PaintSurfaceSpec) -> crate::mui::MaraPainter {
        // Same clip resolution as the trait default, but retain a clone
        // of the painter so `canvas_commands()` can read back what the
        // caller drew (the default impl drops the only handle).
        let clip = match spec.region {
            crate::layout::PaintSurfaceRegion::ClipRect(rect) => rect,
            crate::layout::PaintSurfaceRegion::RemainingAvailable => self
                .clip_stack
                .last()
                .copied()
                .unwrap_or_else(|| self.available_rect()),
        };
        let painter = crate::mui::MaraPainter::recording(clip);
        self.canvas_painters.borrow_mut().push(painter.clone());
        painter
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

    fn id(&self) -> Id {
        let mut id = Id::new("mara-record-backend");
        for salt in &self.id_stack {
            id = id.with(*salt);
        }
        id
    }

    fn in_id_scope(&mut self, salt: Id, body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend)) {
        self.id_stack.push(salt);
        body(self);
        self.id_stack.pop();
    }

    fn memory(&self) -> crate::memory::BackendMemory<'_> {
        crate::memory::BackendMemory::Recording(&self.memory)
    }

    fn ctx(&self) -> &dyn crate::context::MaraCtx {
        self
    }

    /// Headless: the body slot is the surface itself. Nothing scrolls,
    /// so content lays out in place and its height is what the cursor
    /// advanced by.
    fn body_slot(
        &mut self,
        _spec: crate::layout::ContainerBodySpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> f32 {
        let start = self.cursor.y;
        body(self);
        (self.cursor.y - start).max(0.0)
    }

    /// Headless: the child surface is this one, scoped to the region
    /// and restored afterwards so the parent's flow continues.
    fn in_region(
        &mut self,
        region: crate::layout::ChildRegion,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        let saved = (self.available, self.cursor);
        self.available = region.rect;
        self.cursor = region.rect.min;
        body(self);
        self.available = saved.0;
        self.cursor = saved.1;
    }

    fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> Rect {
        // Reserve a paint slot so the frame lands *behind* the body —
        // painting it first would work here but not on a backend that
        // batches, and the ordering is the contract.
        let slot = self.reserve_paint_slot();
        let margin = spec.inner_margin;
        let outer = spec.outer_margin;
        // The outer margin sits between the parent's cursor and the
        // frame's border, so the border starts inside it.
        let start = Pos2::new(
            self.cursor.x + outer.left as f32,
            self.cursor.y + outer.top as f32,
        );
        self.cursor = Pos2::new(start.x + margin.left as f32, start.y + margin.top as f32);
        body(self);
        let content_bottom = self.cursor.y + margin.bottom as f32;
        let rect = Rect::from_min_max(start, Pos2::new(self.available.max.x, content_bottom));
        self.cursor = Pos2::new(self.cursor.x, content_bottom + outer.bottom as f32);
        self.fill_paint_slot(
            slot,
            Some(PaintCmd::RectFilled {
                rect,
                corner: spec.corner,
                fill: spec.fill,
            }),
        );
        self.cursor = Pos2::new(start.x, content_bottom);
        self.expand_to_include(rect);
        rect
    }

    fn in_row(
        &mut self,
        size: Vec2,
        align: crate::layout::CrossAlign,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        let rect = self.advance(size);
        let saved_cursor = self.cursor;
        let saved_flow = self.flow_horizontal;
        let saved_row = self.row_bottom;
        // Items flow rightward from the row's left edge, offset on the
        // cross axis so `Center` sits mid-row rather than at the top.
        self.cursor = Pos2::new(
            rect.min.x,
            match align {
                crate::layout::CrossAlign::Start => rect.min.y,
                crate::layout::CrossAlign::Center => rect.min.y + rect.height() * 0.5,
                crate::layout::CrossAlign::End => rect.max.y,
            },
        );
        self.flow_horizontal = true;
        self.row_bottom = rect.max.y;
        body(self);
        self.cursor = saved_cursor;
        self.flow_horizontal = saved_flow;
        self.row_bottom = saved_row;
        self.expand_to_include(rect);
    }

    fn overlay_at(&mut self, id: Id, pos: Pos2, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        self.overlays.push((id, pos));
        // Run inline so a headless assertion still sees the contents.
        body(self);
    }

    fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        self.layer_transform = Some(transform);
    }

    fn child_at(&mut self, rect: Rect, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        // An explicit-rect child gets its own cursor and must not
        // disturb where the parent places its next widget.
        let saved_available = self.available;
        let saved_cursor = self.cursor;
        let saved_horizontal = self.flow_horizontal;
        self.available = rect;
        self.cursor = rect.min;
        self.flow_horizontal = false;
        body(self);
        self.available = saved_available;
        self.cursor = saved_cursor;
        self.flow_horizontal = saved_horizontal;
        self.expand_to_include(rect);
    }

    fn advance_cursor_past(&mut self, rect: Rect) {
        if self.flow_horizontal {
            self.cursor.x = self.cursor.x.max(rect.max.x);
            self.row_bottom = self.row_bottom.max(rect.max.y);
        } else {
            self.cursor.y = self.cursor.y.max(rect.max.y);
        }
        self.expand_to_include(rect);
    }

    fn expand_to_include(&mut self, rect: Rect) {
        self.occupied = Some(match self.occupied {
            Some(current) => Rect::from_min_max(
                Pos2::new(current.min.x.min(rect.min.x), current.min.y.min(rect.min.y)),
                Pos2::new(current.max.x.max(rect.max.x), current.max.y.max(rect.max.y)),
            ),
            None => rect,
        });
    }

    fn occupied_rect(&self) -> Rect {
        self.occupied
            .unwrap_or_else(|| Rect::from_min_size(self.available.min, Vec2::ZERO))
    }

    fn cursor(&self) -> Pos2 {
        self.cursor
    }

    fn in_child(
        &mut self,
        _id: Id,
        inset_left: f32,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) {
        let saved_available = self.available;
        let saved_cursor = self.cursor;
        // Scope to an indented sub-region flowing from the cursor.
        self.available = Rect::from_min_max(
            Pos2::new(self.available.min.x + inset_left, self.cursor.y),
            self.available.max,
        );
        self.cursor = self.available.min;
        body(self);
        // Restore the parent region; continue flow below the child.
        let child_end_y = self.cursor.y;
        self.available = saved_available;
        self.cursor = Pos2::new(saved_cursor.x, child_end_y.max(saved_cursor.y));
    }

    fn in_scope(
        &mut self,
        horizontal: bool,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) {
        if !horizontal {
            body(self);
            return;
        }
        let saved_cursor = self.cursor;
        let saved_flow = self.flow_horizontal;
        let saved_row = self.row_bottom;
        self.flow_horizontal = true;
        self.row_bottom = self.cursor.y;
        body(self);
        let bottom = self.row_bottom;
        self.flow_horizontal = saved_flow;
        self.row_bottom = saved_row;
        // Parent resumes below the tallest item in the row.
        self.cursor = Pos2::new(saved_cursor.x, bottom.max(saved_cursor.y));
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

    #[test]
    fn in_child_indents_body_and_resumes_parent_flow() {
        use crate::layout::{Sense, UiBackend};
        let mut backend = frame();
        let _outer = backend.allocate(Vec2::new(100.0, 10.0), Sense::Hover); // y 0..10
        let mut child_first_min = Pos2::ZERO;
        backend.in_child(Id::new("sec"), 16.0, &mut |child| {
            // Child content is inset by 16px and flows below the outer.
            let r = child.allocate(Vec2::new(50.0, 20.0), Sense::Hover);
            child_first_min = r.rect.min;
        });
        assert_eq!(
            child_first_min,
            Pos2::new(16.0, 10.0),
            "indented + below outer"
        );
        // Parent flow resumes below the child region.
        let after = backend.allocate(Vec2::new(100.0, 10.0), Sense::Hover);
        assert_eq!(
            after.rect.min,
            Pos2::new(0.0, 30.0),
            "parent resumes past child"
        );
    }

    #[test]
    fn in_id_scope_makes_ids_unique_per_scope() {
        use crate::layout::UiBackend;
        let mut backend = frame();
        let base = backend.id();
        let mut a = base;
        let mut b = base;
        backend.in_id_scope(Id::new(0u32), &mut |s| a = s.id());
        backend.in_id_scope(Id::new(1u32), &mut |s| b = s.id());
        assert_ne!(a, base, "scope changes the id");
        assert_ne!(a, b, "sibling scopes get distinct ids");
        // Scope is popped afterward — back to the base id.
        assert_eq!(backend.id(), base);
    }

    #[test]
    fn scroll_region_runs_body_as_flow_headless() {
        use crate::layout::{ScrollRegion, Sense, UiBackend};
        let mut backend = frame();
        let region = ScrollRegion::new(Id::new("results"), [false, false], 200.0, 2.0);
        let mut a = Pos2::ZERO;
        let mut b = Pos2::ZERO;
        backend.scroll_region(region, &mut |view| {
            a = view.allocate(Vec2::new(50.0, 10.0), Sense::Hover).rect.min;
            b = view.allocate(Vec2::new(50.0, 10.0), Sense::Hover).rect.min;
        });
        // Headless: no clip/offset — content just flows top-down.
        assert_eq!(a, Pos2::new(0.0, 0.0));
        assert_eq!(b, Pos2::new(0.0, 10.0));
    }

    #[test]
    fn in_scope_horizontal_flows_right_then_parent_resumes_below() {
        use crate::layout::{Sense, UiBackend};
        let mut backend = frame();
        let mut a = Pos2::ZERO;
        let mut b = Pos2::ZERO;
        backend.in_scope(true, &mut |row| {
            a = row.allocate(Vec2::new(40.0, 12.0), Sense::Hover).rect.min;
            b = row.allocate(Vec2::new(30.0, 20.0), Sense::Hover).rect.min;
        });
        assert_eq!(a, Pos2::new(0.0, 0.0));
        assert_eq!(b, Pos2::new(40.0, 0.0), "second item flows to the right");
        // Parent resumes below the tallest item in the row (20px).
        let after = backend.allocate(Vec2::new(10.0, 10.0), Sense::Hover);
        assert_eq!(after.rect.min, Pos2::new(0.0, 20.0));
    }

    #[test]
    fn flow_cursor_stacks_allocations_top_down() {
        use crate::layout::{Sense, UiBackend};
        let mut backend = frame();
        let a = backend.allocate(Vec2::new(100.0, 20.0), Sense::Hover);
        let b = backend.allocate(Vec2::new(100.0, 30.0), Sense::Hover);
        // Second allocation sits directly below the first (no overlap).
        assert_eq!(a.rect.min, Pos2::new(0.0, 0.0));
        assert_eq!(b.rect.min, Pos2::new(0.0, 20.0));
        // available_rect shrinks from the top as the cursor advances.
        assert_eq!(backend.available_rect().min, Pos2::new(0.0, 50.0));
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

        let mut backend = MaraBackend::Recording(Box::new(frame()));
        {
            let mut mui = MaraUi::over(&mut backend, ACCENT);
            mui.label("headless");
            mui.button("apply");
            let mut on = true;
            mui.toggle("dark", &mut on);
            let mut value = 0.5_f64;
            mui.slider("gain", &mut value, 0.0..=1.0, 2, "");
        }
        let MaraBackend::Recording(recorded) = backend else {
            unreachable!("constructed with the recording backend");
        };
        golden_check("mara_ui_over_recording", &recorded.paints);
    }
}
