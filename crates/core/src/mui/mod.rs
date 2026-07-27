//! # `mui` — the sealed Mara UI surface
//!
//! [`MaraUi`] is the only thing Mara hands to consumer drawing code
//! (module inline bodies, view bodies, foldable sections). It holds a
//! backend adapter (`backend::egui::EguiUiBackend`) rather than a raw
//! `egui::Ui`, and routes reads/layout/paint through the
//! [`crate::layout::UiBackend`] contract; only operations not yet
//! promoted to that contract reach the concrete `Ui` through the
//! backend's crate-internal `ui()`/`ui_mut()` seam. It exposes exactly
//! Mara's widget set plus a typed canvas/painter, so an app can
//! compose UI without ever holding a raw `egui::Ui`.
//!
//! Capability rules:
//!
//! * widget methods return [`MaraResponse`] — a plain data snapshot,
//!   not `egui::Response` (whose public `ctx` field would leak the
//!   whole `egui::Context`).
//! * custom drawing goes through [`MaraPainter`], which only speaks
//!   [`crate::vocab`] data types and the theme's fonts.
//! * input is read through [`MaraInput`], a per-frame snapshot.
//! * raw backend handles stay behind crate-internal first-party
//!   adapter hooks, not public app APIs.

use std::{cell::RefCell, ops::RangeInclusive, rc::Rc};

use crate::backend;
use crate::layout::{PaintSurfaceSpec, Sense as MaraSense, SpaceSpec, UiBackend};
use crate::paint::{PaintCmd, PaintList};
use crate::pod::{Pod, PodResponse};
use crate::vocab;
use crate::widget::TreeBody;
use crate::widget::badge::badge_row_backend;
use crate::widget::button::{ActionButtonResponse, button_backend, card_action_button};
use crate::widget::chip::{chip_colored_backend, chip_fill};
use crate::widget::color::{color_rgb, color_rgba};
use crate::widget::context_menu::context_menu_mara;
use crate::widget::drag_value::drag_value_backend;
use crate::widget::dropdown::dropdown;
use crate::widget::foldable::section_backend;
use crate::widget::keybinding::keybinding_row_backend;
use crate::widget::label::label_backend;
use crate::widget::progressbar::progressbar_backend;
use crate::widget::readout::readout_backend;
use crate::widget::select::{HybridSelectResponse, hybrid_select_row_backend, select_row_backend};
use crate::widget::slider::slider_backend;
use crate::widget::text_input::text_input;
use crate::widget::toggle::{toggle_backend, toggle_track_only_backend};

// ─── MaraResponse ─────────────────────────────────────────────────

/// Plain-data interaction snapshot returned by every [`MaraUi`]
/// widget method.
///
/// Mara cannot return `egui::Response` from sealed surfaces because
/// its public `ctx` field hands out the entire `egui::Context`.
/// This type copies the flags consumer code actually branches on.
/// Backend-specific follow-ups (e.g. [`MaraUi::context_menu`]) use a
/// backend side-table keyed by this snapshot, not a raw backend
/// response stored inside the public response type.
#[derive(Clone, Debug)]
pub struct MaraResponse {
    pub clicked: bool,
    pub double_clicked: bool,
    pub secondary_clicked: bool,
    pub hovered: bool,
    pub changed: bool,
    pub dragged: bool,
    pub drag_started: bool,
    pub drag_stopped: bool,
    pub pointer_button_down: bool,
    pub drag_delta: vocab::Vec2,
    /// Pointer position during click/drag interaction with this
    /// widget, if any.
    pub interact_pointer: Option<vocab::Pos2>,
    /// Pointer position while hovering this widget, if any.
    pub hover_pos: Option<vocab::Pos2>,
    pub rect: vocab::Rect,
    /// Per-button click flags, indexed by [`vocab::PointerButton`].
    /// Captured at construction because a `MaraResponse` is a snapshot
    /// — there is no live backend response to re-query later.
    clicked_by: [bool; 3],
    /// Per-button drag flags, same indexing as `clicked_by`.
    dragged_by: [bool; 3],
    backend_response: vocab::Id,
}

impl From<egui::Response> for MaraResponse {
    fn from(inner: egui::Response) -> Self {
        let backend_response = backend::egui::remember_response(&inner);
        let clicked_by = vocab::PointerButton::ALL
            .map(|button| inner.clicked_by(backend::egui::egui_pointer_button(button)));
        let dragged_by = vocab::PointerButton::ALL
            .map(|button| inner.dragged_by(backend::egui::egui_pointer_button(button)));
        Self {
            clicked_by,
            dragged_by,
            clicked: inner.clicked(),
            double_clicked: inner.double_clicked(),
            secondary_clicked: inner.secondary_clicked(),
            hovered: inner.hovered(),
            changed: inner.changed(),
            dragged: inner.dragged(),
            drag_started: inner.drag_started(),
            drag_stopped: inner.drag_stopped(),
            pointer_button_down: inner.is_pointer_button_down_on(),
            drag_delta: inner.drag_delta().into(),
            interact_pointer: inner.interact_pointer_pos().map(Into::into),
            hover_pos: inner.hover_pos().map(Into::into),
            rect: inner.rect.into(),
            backend_response,
        }
    }
}

impl MaraResponse {
    /// Inert response at `rect` — used by non-interactive backends
    /// (the recording backend) and tests.
    pub(crate) fn synthetic(rect: vocab::Rect) -> Self {
        Self {
            clicked: false,
            double_clicked: false,
            secondary_clicked: false,
            hovered: false,
            changed: false,
            dragged: false,
            drag_started: false,
            drag_stopped: false,
            pointer_button_down: false,
            drag_delta: vocab::Vec2::ZERO,
            interact_pointer: None,
            hover_pos: None,
            rect,
            clicked_by: [false; 3],
            dragged_by: [false; 3],
            backend_response: vocab::Id::new(("mara_response", "synthetic")),
        }
    }

    /// Was this widget clicked with `button` specifically?
    #[must_use]
    pub fn clicked_by(&self, button: vocab::PointerButton) -> bool {
        self.clicked_by[button.index()]
    }

    /// Is this widget being dragged with `button` specifically?
    #[must_use]
    pub fn dragged_by(&self, button: vocab::PointerButton) -> bool {
        self.dragged_by[button.index()]
    }

    /// Headless-test harness: an inert response at `rect`, for driving a
    /// module's paint path outside a live backend. Doc-hidden; not a
    /// stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_synthetic(rect: vocab::Rect) -> Self {
        Self::synthetic(rect)
    }

    #[must_use]
    pub fn clicked(&self) -> bool {
        self.clicked
    }

    #[must_use]
    pub fn double_clicked(&self) -> bool {
        self.double_clicked
    }

    #[must_use]
    pub fn secondary_clicked(&self) -> bool {
        self.secondary_clicked
    }

    #[must_use]
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    #[must_use]
    pub fn changed(&self) -> bool {
        self.changed
    }

    #[must_use]
    pub fn dragged(&self) -> bool {
        self.dragged
    }

    #[must_use]
    pub fn drag_started(&self) -> bool {
        self.drag_started
    }

    #[must_use]
    pub fn drag_stopped(&self) -> bool {
        self.drag_stopped
    }

    #[must_use]
    pub fn pointer_button_down(&self) -> bool {
        self.pointer_button_down
    }

    pub(crate) fn backend_response_id(&self) -> vocab::Id {
        self.backend_response
    }
}

// ─── MaraInput ────────────────────────────────────────────────────

/// Per-frame input snapshot for custom (canvas-style) surfaces.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaraInput {
    /// Latest pointer position, if the pointer is over the window.
    pub pointer: Option<vocab::Pos2>,
    /// Current interaction pointer position for active drag/click gestures.
    pub interact_pointer: Option<vocab::Pos2>,
    pub primary_down: bool,
    pub primary_pressed: bool,
    pub primary_released: bool,
    pub any_released: bool,
    pub secondary_down: bool,
    pub secondary_pressed: bool,
    pub middle_down: bool,
    pub middle_pressed: bool,
    pub middle_released: bool,
    /// Smooth scroll delta this frame.
    pub scroll_delta: vocab::Vec2,
    /// Pointer movement since last frame.
    pub pointer_delta: vocab::Vec2,
    /// Pinch/ctrl-scroll zoom factor this frame (1.0 = none).
    pub zoom_delta: f32,
    pub modifiers_shift: bool,
    pub modifiers_ctrl: bool,
    pub modifiers_alt: bool,
    /// Keys that went down this frame. Surfaces that own their own
    /// key handling (map, 3D, canvas) read this instead of reaching
    /// for the backend's input state.
    pub keys_pressed: MaraKeySet,
}

impl MaraInput {
    /// Did `key` go down this frame?
    #[must_use]
    pub fn key_pressed(&self, key: MaraKey) -> bool {
        self.keys_pressed.contains(key)
    }

    /// Is `button` currently held?
    #[must_use]
    pub fn button_down(&self, button: vocab::PointerButton) -> bool {
        match button {
            vocab::PointerButton::Primary => self.primary_down,
            vocab::PointerButton::Secondary => self.secondary_down,
            vocab::PointerButton::Middle => self.middle_down,
        }
    }

    /// Did `button` go down this frame?
    #[must_use]
    pub fn button_pressed(&self, button: vocab::PointerButton) -> bool {
        match button {
            vocab::PointerButton::Primary => self.primary_pressed,
            vocab::PointerButton::Secondary => self.secondary_pressed,
            vocab::PointerButton::Middle => self.middle_pressed,
        }
    }
}

/// A set of [`MaraKey`]s, held as a bitset so [`MaraInput`] stays
/// `Copy` and snapshotting input allocates nothing per frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MaraKeySet(u128);

impl MaraKey {
    /// Every key, in declaration order.
    pub const ALL: [Self; 66] = [
        Self::Escape,
        Self::ArrowDown,
        Self::ArrowUp,
        Self::ArrowLeft,
        Self::ArrowRight,
        Self::Enter,
        Self::Tab,
        Self::Space,
        Self::Backspace,
        Self::Delete,
        Self::Insert,
        Self::Home,
        Self::End,
        Self::PageUp,
        Self::PageDown,
        Self::Minus,
        Self::Plus,
        Self::Equals,
        Self::A,
        Self::B,
        Self::C,
        Self::D,
        Self::E,
        Self::F,
        Self::G,
        Self::H,
        Self::I,
        Self::J,
        Self::K,
        Self::L,
        Self::M,
        Self::N,
        Self::O,
        Self::P,
        Self::Q,
        Self::R,
        Self::S,
        Self::T,
        Self::U,
        Self::V,
        Self::W,
        Self::X,
        Self::Y,
        Self::Z,
        Self::Num0,
        Self::Num1,
        Self::Num2,
        Self::Num3,
        Self::Num4,
        Self::Num5,
        Self::Num6,
        Self::Num7,
        Self::Num8,
        Self::Num9,
        Self::F1,
        Self::F2,
        Self::F3,
        Self::F4,
        Self::F5,
        Self::F6,
        Self::F7,
        Self::F8,
        Self::F9,
        Self::F10,
        Self::F11,
        Self::F12,
    ];
}

impl MaraKeySet {
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn contains(self, key: MaraKey) -> bool {
        self.0 & (1u128 << (key as u8)) != 0
    }

    pub const fn insert(&mut self, key: MaraKey) {
        self.0 |= 1u128 << (key as u8);
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The keys in this set, in [`MaraKey::ALL`] order.
    pub fn iter(self) -> impl Iterator<Item = MaraKey> {
        MaraKey::ALL.into_iter().filter(move |&k| self.contains(k))
    }
}

impl FromIterator<MaraKey> for MaraKeySet {
    fn from_iter<T: IntoIterator<Item = MaraKey>>(iter: T) -> Self {
        let mut set = Self::empty();
        for key in iter {
            set.insert(key);
        }
        set
    }
}

/// Keyboard keys Mara surfaces can react to.
///
/// `repr(u8)` and the declaration order are load-bearing:
/// [`MaraKeySet`] indexes its bitset by `key as u8`, so the count must
/// stay at or below 128 and existing variants must not be reordered
/// across a release.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaraKey {
    Escape,
    ArrowDown,
    ArrowUp,
    ArrowLeft,
    ArrowRight,
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Minus,
    Plus,
    Equals,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

// ─── MaraPainter ──────────────────────────────────────────────────

#[derive(Clone)]
enum MaraPainterSink {
    Egui(egui::Painter),
    /// Records draw commands instead of rasterising — used by non-egui
    /// backends and tests.
    Commands {
        commands: Rc<RefCell<PaintList>>,
        clip: vocab::Rect,
    },
}

impl MaraPainterSink {
    fn clip_rect(&self) -> vocab::Rect {
        match self {
            Self::Egui(painter) => backend::egui::painter_clip_rect(painter),
            Self::Commands { clip, .. } => *clip,
        }
    }
}

/// Typed draw surface for custom content (gauges, plots, overlays).
///
/// Sinks Mara [`PaintCmd`] data into the current backend. The egui
/// backend renders commands immediately today; retained/future
/// backends can collect the same command stream without exposing
/// `egui::Painter` as the semantic drawing model. Every public
/// method speaks [`crate::vocab`] data types only. Text always
/// renders in the theme's font families, so custom drawing cannot
/// drift away from Mara's typography.
#[derive(Clone)]
pub struct MaraPainter {
    sink: MaraPainterSink,
}

impl MaraPainter {
    pub(crate) fn new(painter: egui::Painter) -> Self {
        Self {
            sink: MaraPainterSink::Egui(painter),
        }
    }

    /// First-party hook: wrap a backend painter.
    ///
    /// Exists for incremental renderer ports (PLAN.md WS-D1.3), where a
    /// ported leaf draws through `MaraPainter` while its still-unported
    /// caller holds the backend's painter. Doc-hidden; the seam shrinks
    /// to nothing as the port completes.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_from_egui(painter: egui::Painter) -> Self {
        Self::new(painter)
    }

    /// A painter that records into an internal command list rather than
    /// an egui painter — used by non-egui backends (their painter output
    /// isn't rasterised, so it's discarded) and by tests.
    pub(crate) fn recording(clip: impl Into<vocab::Rect>) -> Self {
        Self {
            sink: MaraPainterSink::Commands {
                commands: Rc::new(RefCell::new(PaintList::new())),
                clip: clip.into(),
            },
        }
    }

    /// Headless-test harness: a command-recording painter. Lets module
    /// crates (Board, Canvas, …) drive their `PaintCmd`-only draw path
    /// with no egui in scope and assert what they emit — the concrete
    /// proof a module is backend-portable. Doc-hidden; not a stable API.
    #[doc(hidden)]
    pub fn __internal_recording(clip: impl Into<vocab::Rect>) -> Self {
        Self::recording(clip)
    }

    /// Headless-test harness: the `PaintCmd`s this painter has recorded.
    /// Empty for an egui-backed painter (its output rasterises straight
    /// to the GPU). Doc-hidden; not a stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_recorded_commands(&self) -> Vec<PaintCmd> {
        match &self.sink {
            MaraPainterSink::Egui(_) => Vec::new(),
            MaraPainterSink::Commands { commands, .. } => commands.borrow().commands().to_vec(),
        }
    }

    /// The rect drawing is clipped to.
    #[must_use]
    pub fn clip_rect(&self) -> vocab::Rect {
        self.sink.clip_rect()
    }

    /// A sub-painter hard-clipped to `rect` (intersected with the
    /// current clip, so clips only ever shrink).
    #[must_use]
    pub fn with_clip(&self, rect: impl Into<vocab::Rect>) -> MaraPainter {
        let rect = rect.into();
        match &self.sink {
            MaraPainterSink::Egui(painter) => {
                MaraPainter::new(backend::egui::painter_with_clip(painter, rect))
            }
            MaraPainterSink::Commands { commands, clip } => MaraPainter {
                sink: MaraPainterSink::Commands {
                    commands: Rc::clone(commands),
                    clip: clip.intersect(rect),
                },
            },
        }
    }

    pub fn line_segment(
        &self,
        a: impl Into<vocab::Pos2>,
        b: impl Into<vocab::Pos2>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Line {
            a: a.into(),
            b: b.into(),
            stroke: stroke.into(),
        });
    }

    /// Open polyline through `points`.
    pub fn polyline(&self, points: Vec<vocab::Pos2>, stroke: impl Into<vocab::Stroke>) {
        self.paint_cmd(PaintCmd::Polyline {
            points,
            stroke: stroke.into(),
        });
    }

    /// Closed path with independent fill and stroke.
    pub fn polygon(
        &self,
        points: Vec<vocab::Pos2>,
        fill: impl Into<vocab::Color32>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Polygon {
            points,
            fill: fill.into(),
            stroke: stroke.into(),
        });
    }

    pub fn rect_filled(
        &self,
        rect: impl Into<vocab::Rect>,
        corner: impl Into<vocab::CornerRadius>,
        fill: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::RectFilled {
            rect: rect.into(),
            corner: corner.into(),
            fill: fill.into(),
        });
    }

    pub fn rect_stroke(
        &self,
        rect: impl Into<vocab::Rect>,
        corner: impl Into<vocab::CornerRadius>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::RectStroke {
            rect: rect.into(),
            corner: corner.into(),
            stroke: stroke.into(),
        });
    }

    pub fn circle_filled(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: f32,
        fill: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::CircleFilled {
            center: center.into(),
            radius,
            fill: fill.into(),
        });
    }

    pub fn circle_stroke(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: f32,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::CircleStroke {
            center: center.into(),
            radius,
            stroke: stroke.into(),
        });
    }

    /// Filled axis-aligned ellipse bounded by `rect`.
    pub fn ellipse_filled(&self, rect: impl Into<vocab::Rect>, fill: impl Into<vocab::Color32>) {
        self.paint_cmd(PaintCmd::Ellipse {
            rect: rect.into(),
            fill: fill.into(),
            stroke: vocab::Stroke::NONE,
        });
    }

    /// Outlined axis-aligned ellipse bounded by `rect`.
    pub fn ellipse_stroke(&self, rect: impl Into<vocab::Rect>, stroke: impl Into<vocab::Stroke>) {
        self.paint_cmd(PaintCmd::Ellipse {
            rect: rect.into(),
            fill: vocab::Color32::TRANSPARENT,
            stroke: stroke.into(),
        });
    }

    /// Open (elliptical) arc — a curved line. Angles in radians, `0` at
    /// the +x axis, increasing clockwise (screen y-down).
    pub fn arc(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: impl Into<vocab::Vec2>,
        start_angle: f32,
        end_angle: f32,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Arc {
            center: center.into(),
            radius: radius.into(),
            start_angle,
            end_angle,
            stroke: stroke.into(),
        });
    }

    /// Filled pie sector from `center` across the arc (angles as in
    /// [`MaraPainter::arc`]).
    pub fn sector(
        &self,
        center: impl Into<vocab::Pos2>,
        radius: impl Into<vocab::Vec2>,
        start_angle: f32,
        end_angle: f32,
        fill: impl Into<vocab::Color32>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Sector {
            center: center.into(),
            radius: radius.into(),
            start_angle,
            end_angle,
            fill: fill.into(),
            stroke: stroke.into(),
        });
    }

    pub fn arrow(
        &self,
        origin: impl Into<vocab::Pos2>,
        vec: impl Into<vocab::Vec2>,
        stroke: impl Into<vocab::Stroke>,
    ) {
        self.paint_cmd(PaintCmd::Arrow {
            origin: origin.into(),
            vec: vec.into(),
            stroke: stroke.into(),
        });
    }

    /// Text in the theme's proportional family. Returns the painted
    /// rect.
    pub fn text(
        &self,
        pos: impl Into<vocab::Pos2>,
        anchor: impl Into<vocab::Align2>,
        text: impl ToString,
        size: f32,
        color: impl Into<vocab::Color32>,
    ) -> vocab::Rect {
        self.paint_text_cmd(PaintCmd::Text {
            pos: pos.into(),
            anchor: anchor.into(),
            text: text.to_string(),
            size,
            color: color.into(),
            mono: false,
        })
    }

    /// Text in the theme's monospace family. Returns the painted
    /// rect.
    pub fn text_mono(
        &self,
        pos: impl Into<vocab::Pos2>,
        anchor: impl Into<vocab::Align2>,
        text: impl ToString,
        size: f32,
        color: impl Into<vocab::Color32>,
    ) -> vocab::Rect {
        self.paint_text_cmd(PaintCmd::Text {
            pos: pos.into(),
            anchor: anchor.into(),
            text: text.to_string(),
            size,
            color: color.into(),
            mono: true,
        })
    }

    /// Textured quad. `uv` is in 0..=1 texture space; pass
    /// `Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))` (or
    /// [`MaraPainter::full_uv`]) for the whole texture.
    pub fn image(
        &self,
        texture: impl Into<vocab::TextureId>,
        rect: impl Into<vocab::Rect>,
        uv: impl Into<vocab::Rect>,
        tint: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::Image {
            texture: texture.into(),
            rect: rect.into(),
            uv: uv.into(),
            tint: tint.into(),
        });
    }

    /// The full 0..=1 UV rect, for [`MaraPainter::image`].
    #[must_use]
    pub fn full_uv() -> vocab::Rect {
        vocab::Rect::from_min_max(vocab::Pos2::ZERO, vocab::Pos2::new(1.0, 1.0))
    }

    /// Gouraud-shaded triangle mesh — the primitive for gradients and
    /// any fill a solid colour cannot express. `indices` are triplets
    /// into `vertices`; a length that is not a multiple of three, or an
    /// index past the end of `vertices`, draws nothing.
    pub fn mesh(&self, vertices: Vec<crate::paint::PaintVertex>, indices: Vec<u32>) {
        if indices.len() % 3 != 0 {
            return;
        }
        let len = vertices.len() as u32;
        if indices.iter().any(|&i| i >= len) {
            return;
        }
        self.paint_cmd(PaintCmd::Mesh { vertices, indices });
    }

    /// Soft drop shadow cast by `rect`. `offset` is in points,
    /// `blur`/`spread` in pixels.
    pub fn shadow(
        &self,
        rect: impl Into<vocab::Rect>,
        corner: impl Into<vocab::CornerRadius>,
        offset: [i8; 2],
        blur: u8,
        spread: u8,
        color: impl Into<vocab::Color32>,
    ) {
        self.paint_cmd(PaintCmd::Shadow {
            rect: rect.into(),
            corner: corner.into(),
            offset,
            blur,
            spread,
            color: color.into(),
        });
    }

    /// Size `text` would occupy at `size` points, without painting it.
    ///
    /// Lets drawing code lay out labels (collision tests, centring,
    /// leader lines) without reaching for the backend's text engine.
    /// Command-recording painters have no font atlas and return the
    /// same coarse estimate the recording backend uses, so headless
    /// layout stays deterministic rather than collapsing to zero.
    /// Size of a run sequence laid out as one line.
    ///
    /// [`measure_text`](MaraPainter::measure_text) covers a single
    /// uniform string; a title that mixes weights or families needs the
    /// runs measured together, because per-run widths do not sum to the
    /// laid-out width once spacing and kerning apply.
    #[must_use]
    pub fn measure_text_runs(&self, runs: &[crate::paint::TextRun]) -> vocab::Vec2 {
        match &self.sink {
            MaraPainterSink::Egui(painter) => {
                backend::egui::measure_text_runs_for_painter(painter, runs)
            }
            MaraPainterSink::Commands { .. } => runs.iter().fold(vocab::Vec2::ZERO, |acc, run| {
                let one = self.measure_text(&run.text, run.size, false);
                vocab::Vec2::new(
                    acc.x + one.x + run.leading_space,
                    acc.y.max(one.y),
                )
            }),
        }
    }

    #[must_use]
    pub fn measure_text(&self, text: &str, size: f32, mono: bool) -> vocab::Vec2 {
        match &self.sink {
            MaraPainterSink::Egui(painter) => backend::egui::measure_text_for_spec(
                painter,
                &crate::layout::TextMeasureSpec::new(text, size, mono),
            ),
            MaraPainterSink::Commands { .. } => {
                vocab::Vec2::new(text.chars().count() as f32 * size * 0.5, size)
            }
        }
    }

    /// Render a Mara paint command through the current backend.
    ///
    /// Today this translates immediately to egui. Later this method
    /// becomes the seam where commands are buffered or sent to a
    /// non-egui renderer.
    pub fn paint_cmd(&self, cmd: PaintCmd) {
        match &self.sink {
            MaraPainterSink::Egui(painter) => backend::egui::render_paint_cmd(painter, cmd),
            MaraPainterSink::Commands { commands, clip } => {
                commands.borrow_mut().push(PaintCmd::Clip {
                    rect: *clip,
                    children: vec![cmd],
                });
            }
        }
    }

    fn paint_text_cmd(&self, cmd: PaintCmd) -> vocab::Rect {
        match &self.sink {
            MaraPainterSink::Egui(painter) => backend::egui::render_text_cmd(painter, cmd),
            MaraPainterSink::Commands { .. } => {
                let rect = match &cmd {
                    PaintCmd::Text { pos, .. }
                    | PaintCmd::TextWithFamily { pos, .. }
                    | PaintCmd::TextRuns { pos, .. } => {
                        vocab::Rect::from_min_size(*pos, vocab::Vec2::ZERO)
                    }
                    _ => vocab::Rect::NOTHING,
                };
                self.paint_cmd(cmd);
                rect
            }
        }
    }
}

// ─── MaraUi ───────────────────────────────────────────────────────

/// Which concrete backend a [`MaraUi`] drives — PLAN.md Phase 3.
///
/// A closed enum (like [`crate::memory::BackendMemory`]) rather than
/// `Box<dyn UiBackend>`: `EguiUiBackend<'a>` borrows the host `Ui`, so
/// it is not `'static` and cannot be `Any`-downcast; and `MaraUi` must
/// keep *owning* its backend (`__internal_from_raw` returns an owning
/// `MaraUi` from a bare `&mut egui::Ui`), so a `&mut dyn` reference
/// won't do. The enum gives zero-alloc dynamic dispatch and a clean
/// `match` for the shrinking set of operations still egui-bound.
pub(crate) enum MaraBackend<'a> {
    Egui(backend::egui::EguiUiBackend<'a>),
    /// Headless recording backend — golden paint tests render a full
    /// `MaraUi` over this with zero egui in the call path. Constructed
    /// only in tests today; a headless/pilot host (PLAN.md Phase 6)
    /// will construct it in production.
    #[allow(dead_code)]
    Recording(Box<backend::record::RecordingBackend>),
}

impl crate::layout::UiBackend for MaraBackend<'_> {
    fn begin_area(&mut self, host: crate::layout::AreaHost, rect: vocab::Rect) {
        match self {
            Self::Egui(b) => b.begin_area(host, rect),
            Self::Recording(b) => b.begin_area(host, rect),
        }
    }
    fn allocate(&mut self, size: vocab::Vec2, sense: MaraSense) -> MaraResponse {
        match self {
            Self::Egui(b) => b.allocate(size, sense),
            Self::Recording(b) => b.allocate(size, sense),
        }
    }
    fn reserve_rect(&mut self, rect: vocab::Rect, sense: MaraSense) -> MaraResponse {
        match self {
            Self::Egui(b) => b.reserve_rect(rect, sense),
            Self::Recording(b) => b.reserve_rect(rect, sense),
        }
    }
    fn interact(&mut self, rect: vocab::Rect, id: vocab::Id, sense: MaraSense) -> MaraResponse {
        match self {
            Self::Egui(b) => b.interact(rect, id, sense),
            Self::Recording(b) => b.interact(rect, id, sense),
        }
    }
    fn available_rect(&self) -> vocab::Rect {
        match self {
            Self::Egui(b) => b.available_rect(),
            Self::Recording(b) => b.available_rect(),
        }
    }
    fn id(&self) -> vocab::Id {
        match self {
            Self::Egui(b) => b.id(),
            Self::Recording(b) => b.id(),
        }
    }
    fn available_width(&self) -> f32 {
        match self {
            Self::Egui(b) => b.available_width(),
            Self::Recording(b) => b.available_width(),
        }
    }
    fn available_height(&self) -> f32 {
        match self {
            Self::Egui(b) => b.available_height(),
            Self::Recording(b) => b.available_height(),
        }
    }
    fn input(&self) -> MaraInput {
        match self {
            Self::Egui(b) => b.input(),
            Self::Recording(b) => b.input(),
        }
    }
    fn memory(&self) -> crate::memory::BackendMemory<'_> {
        match self {
            Self::Egui(b) => b.memory(),
            Self::Recording(b) => b.memory(),
        }
    }
    fn add_space(&mut self, spec: SpaceSpec) {
        match self {
            Self::Egui(b) => b.add_space(spec),
            Self::Recording(b) => b.add_space(spec),
        }
    }
    fn push_clip(&mut self, rect: vocab::Rect) {
        match self {
            Self::Egui(b) => b.push_clip(rect),
            Self::Recording(b) => b.push_clip(rect),
        }
    }
    fn pop_clip(&mut self) {
        match self {
            Self::Egui(b) => b.pop_clip(),
            Self::Recording(b) => b.pop_clip(),
        }
    }
    fn measure_text(&self, text: &str, size: f32, mono: bool) -> vocab::Vec2 {
        match self {
            Self::Egui(b) => b.measure_text(text, size, mono),
            Self::Recording(b) => b.measure_text(text, size, mono),
        }
    }
    fn paint(&mut self, cmd: PaintCmd) {
        match self {
            Self::Egui(b) => b.paint(cmd),
            Self::Recording(b) => b.paint(cmd),
        }
    }
    fn reserve_paint_slot(&mut self) -> crate::layout::PaintSlot {
        match self {
            Self::Egui(b) => b.reserve_paint_slot(),
            Self::Recording(b) => b.reserve_paint_slot(),
        }
    }
    fn fill_paint_slot(&mut self, slot: crate::layout::PaintSlot, cmd: Option<PaintCmd>) {
        match self {
            Self::Egui(b) => b.fill_paint_slot(slot, cmd),
            Self::Recording(b) => b.fill_paint_slot(slot, cmd),
        }
    }
    fn hover_text(&mut self, response: &MaraResponse, text: &str) {
        match self {
            Self::Egui(b) => b.hover_text(response, text),
            Self::Recording(b) => b.hover_text(response, text),
        }
    }
    fn is_rect_visible(&self, rect: vocab::Rect) -> bool {
        match self {
            Self::Egui(b) => b.is_rect_visible(rect),
            Self::Recording(b) => b.is_rect_visible(rect),
        }
    }
    fn __internal_egui_ui_mut(&mut self) -> Option<&mut egui::Ui> {
        match self {
            Self::Egui(b) => b.__internal_egui_ui_mut(),
            Self::Recording(b) => b.__internal_egui_ui_mut(),
        }
    }
    fn __internal_egui_ui_ref(&self) -> Option<&egui::Ui> {
        match self {
            Self::Egui(b) => b.__internal_egui_ui_ref(),
            Self::Recording(b) => b.__internal_egui_ui_ref(),
        }
    }
    fn load_texture(
        &mut self,
        name: &str,
        image: vocab::ColorImage,
        options: vocab::TextureOptions,
    ) -> Option<vocab::TextureHandle> {
        match self {
            Self::Egui(b) => b.load_texture(name, image, options),
            Self::Recording(b) => b.load_texture(name, image, options),
        }
    }
    fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> vocab::Rect {
        match self {
            Self::Egui(b) => b.framed(spec, body),
            Self::Recording(b) => b.framed(spec, body),
        }
    }
    fn in_row(
        &mut self,
        size: vocab::Vec2,
        align: crate::layout::CrossAlign,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        match self {
            Self::Egui(b) => b.in_row(size, align, body),
            Self::Recording(b) => b.in_row(size, align, body),
        }
    }
    fn overlay_at(
        &mut self,
        id: vocab::Id,
        pos: vocab::Pos2,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        match self {
            Self::Egui(b) => b.overlay_at(id, pos, body),
            Self::Recording(b) => b.overlay_at(id, pos, body),
        }
    }
    fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        match self {
            Self::Egui(b) => b.set_layer_transform(transform),
            Self::Recording(b) => b.set_layer_transform(transform),
        }
    }
    fn child_at(&mut self, rect: vocab::Rect, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        match self {
            Self::Egui(b) => b.child_at(rect, body),
            Self::Recording(b) => b.child_at(rect, body),
        }
    }
    fn advance_cursor_past(&mut self, rect: vocab::Rect) {
        match self {
            Self::Egui(b) => b.advance_cursor_past(rect),
            Self::Recording(b) => b.advance_cursor_past(rect),
        }
    }
    fn expand_to_include(&mut self, rect: vocab::Rect) {
        match self {
            Self::Egui(b) => b.expand_to_include(rect),
            Self::Recording(b) => b.expand_to_include(rect),
        }
    }
    fn occupied_rect(&self) -> vocab::Rect {
        match self {
            Self::Egui(b) => b.occupied_rect(),
            Self::Recording(b) => b.occupied_rect(),
        }
    }
    fn cursor(&self) -> vocab::Pos2 {
        match self {
            Self::Egui(b) => b.cursor(),
            Self::Recording(b) => b.cursor(),
        }
    }
    fn in_child(
        &mut self,
        id: vocab::Id,
        inset_left: f32,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        match self {
            Self::Egui(b) => b.in_child(id, inset_left, body),
            Self::Recording(b) => b.in_child(id, inset_left, body),
        }
    }
    fn in_scope(&mut self, horizontal: bool, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        match self {
            Self::Egui(b) => b.in_scope(horizontal, body),
            Self::Recording(b) => b.in_scope(horizontal, body),
        }
    }
    fn make_painter(&self, spec: crate::layout::PaintSurfaceSpec) -> MaraPainter {
        match self {
            Self::Egui(b) => b.make_painter(spec),
            Self::Recording(b) => b.make_painter(spec),
        }
    }
    fn now(&self) -> f64 {
        match self {
            Self::Egui(b) => b.now(),
            Self::Recording(b) => b.now(),
        }
    }
    fn text_typed(&self) -> String {
        match self {
            Self::Egui(b) => b.text_typed(),
            Self::Recording(b) => b.text_typed(),
        }
    }
    fn pixels_per_point(&self) -> f32 {
        match self {
            Self::Egui(b) => b.pixels_per_point(),
            Self::Recording(b) => b.pixels_per_point(),
        }
    }
    fn request_repaint(&self) {
        match self {
            Self::Egui(b) => b.request_repaint(),
            Self::Recording(b) => b.request_repaint(),
        }
    }
    fn request_repaint_after(&self, after: std::time::Duration) {
        match self {
            Self::Egui(b) => b.request_repaint_after(after),
            Self::Recording(b) => b.request_repaint_after(after),
        }
    }
    fn scroll_region(
        &mut self,
        region: crate::layout::ScrollRegion,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        match self {
            Self::Egui(b) => b.scroll_region(region, body),
            Self::Recording(b) => b.scroll_region(region, body),
        }
    }
    fn in_id_scope(&mut self, salt: vocab::Id, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        match self {
            Self::Egui(b) => b.in_id_scope(salt, body),
            Self::Recording(b) => b.in_id_scope(salt, body),
        }
    }
}

/// Opaque owned backend handle for host plugins — created by
/// [`MaraUi::__internal_backend_from_raw`], lent to
/// [`MaraUi::__internal_over`]. Doc-hidden; not semver-stable.
#[doc(hidden)]
pub struct MaraRawBackend<'a>(pub(crate) MaraBackend<'a>);

impl MaraRawBackend<'static> {
    /// Headless-test harness: a raw backend over a fresh recording
    /// backend spanning `rect`. Lend it to [`MaraUi::__internal_over`]
    /// to drive a module's view/inline body with no egui in scope, then
    /// read what it drew with
    /// [`MaraRawBackend::__internal_recorded_canvas_commands`]. Doc-hidden;
    /// not a stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_recording(rect: impl Into<vocab::Rect>) -> Self {
        MaraRawBackend(MaraBackend::Recording(Box::new(
            crate::backend::record::RecordingBackend::at(rect.into()),
        )))
    }
}

impl MaraRawBackend<'_> {
    /// Headless-test harness: the `PaintCmd`s a module drew into the
    /// canvas painters this backend handed out. Empty for an egui-backed
    /// raw backend. Doc-hidden; not a stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_recorded_canvas_commands(&self) -> Vec<PaintCmd> {
        match &self.0 {
            MaraBackend::Recording(b) => b.canvas_commands(),
            MaraBackend::Egui(_) => Vec::new(),
        }
    }
}

/// The sealed widget surface handed to consumer drawing code.
///
/// Carries an ambient accent colour so widget calls stay terse;
/// override it per-scope with [`MaraUi::set_accent`].
pub struct MaraUi<'a> {
    /// The backend `MaraUi` drives through the
    /// [`crate::layout::UiBackend`] contract, held by mutable
    /// reference so nested/child regions can lend a scoped view of the
    /// same backend (PLAN.md Phase 4 / ADR 0002). Operations not yet
    /// promoted to the contract reach egui through
    /// [`crate::layout::UiBackend::__internal_egui_ui_mut`] (tracked by the
    /// coupling ratchet).
    pub(crate) backend: &'a mut dyn UiBackend,
    accent: vocab::Color32,
}

impl<'a> MaraUi<'a> {
    /// Construct over a borrowed backend. The caller owns the backend
    /// (typically a local `MaraBackend::Egui` wrapping an `egui::Ui`)
    /// for at least as long as this `MaraUi`.
    pub(crate) fn over(backend: &'a mut dyn UiBackend, accent: impl Into<vocab::Color32>) -> Self {
        Self {
            backend,
            accent: accent.into(),
        }
    }

    /// The concrete egui `Ui` behind this surface. Panics on a
    /// non-egui backend — the shrinking set of operations still
    /// egui-bound (stack scopes, canvas, pod, context menu, painter,
    /// the raw hatch) go through here. Each call is a coupling-ratchet
    /// escape.
    pub(crate) fn egui_ui(&mut self) -> &mut egui::Ui {
        self.backend
            .__internal_egui_ui_mut()
            .expect("this MaraUi operation requires the egui backend")
    }

    /// The egui `Ui` if this is the egui backend, else `None` — the
    /// still-egui-bound widget fns use this to degrade to an inert
    /// no-op response on headless/recording backends instead of
    /// panicking (PLAN.md WS2). Each caller is a coupling-ratchet
    /// escape, same as [`MaraUi::egui_ui`].
    pub(crate) fn egui_ui_opt(&mut self) -> Option<&mut egui::Ui> {
        self.backend.__internal_egui_ui_mut()
    }

    /// Inert zero-rect response for operations skipped on non-egui
    /// backends: never hovered, never clicked.
    fn noop_response() -> MaraResponse {
        MaraResponse::__internal_synthetic(vocab::Rect::from_min_size(
            vocab::Pos2::new(0.0, 0.0),
            vocab::Vec2::new(0.0, 0.0),
        ))
    }

    /// Internal first-party accessor — NOT part of the public API
    /// and not semver-stable. First-party Mara module crates
    /// (canvas, image, map, …) use this for backend adapter work
    /// while ordinary app code stays on typed Mara APIs.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_raw_ui(&mut self) -> &mut egui::Ui {
        self.egui_ui()
    }

    /// Internal first-party backend handle — NOT part of the public
    /// API and not semver-stable. Host plugins that own the egui pass
    /// (e.g. `bevy_mara`) create one from their `egui::Ui`, then lend
    /// it to [`MaraUi::__internal_over`]. Two steps because `MaraUi`
    /// now *borrows* its backend (ADR 0002), so the caller must own it.
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_backend_from_raw(ui: &'a mut egui::Ui) -> MaraRawBackend<'a> {
        MaraRawBackend(MaraBackend::Egui(backend::egui::EguiUiBackend::new(ui)))
    }

    /// Headless-test harness: drive the sealed surface over any
    /// backend — in practice the recording one — so a widget's
    /// behaviour can be asserted without a live context.
    /// Doc-hidden; not a stable API.
    /// Headless-test harness returning the body's value.
    /// Doc-hidden; not a stable API.
    #[doc(hidden)]
    pub fn __internal_over_backend_ret<R>(
        backend: &mut dyn UiBackend,
        accent: impl Into<vocab::Color32>,
        body: impl FnOnce(&mut MaraUi<'_>) -> R,
    ) -> R {
        let mut ui = MaraUi::over(backend, accent);
        body(&mut ui)
    }

    #[doc(hidden)]
    pub fn __internal_over_backend(
        backend: &mut dyn UiBackend,
        accent: impl Into<vocab::Color32>,
        body: &mut dyn FnMut(&mut MaraUi<'_>),
    ) {
        let mut ui = MaraUi::over(backend, accent);
        body(&mut ui);
    }

    /// Internal first-party constructor — NOT part of the public API
    /// and not semver-stable. Borrows a [`MaraRawBackend`] made by
    /// [`MaraUi::__internal_backend_from_raw`].
    #[doc(hidden)]
    #[must_use]
    pub fn __internal_over(
        backend: &'a mut MaraRawBackend<'_>,
        accent: impl Into<vocab::Color32>,
    ) -> Self {
        Self::over(&mut backend.0, accent)
    }

    // ── ambient state ────────────────────────────────────────────

    #[must_use]
    pub fn accent(&self) -> vocab::Color32 {
        self.accent
    }

    pub fn set_accent(&mut self, accent: impl Into<vocab::Color32>) {
        self.accent = accent.into();
    }

    /// Stable id of the underlying scope, for salting widget ids.
    #[must_use]
    pub fn id(&self) -> vocab::Id {
        self.backend.id()
    }

    #[must_use]
    pub fn available_width(&self) -> f32 {
        self.backend.available_width()
    }

    #[must_use]
    pub fn available_height(&self) -> f32 {
        self.backend.available_height()
    }

    #[must_use]
    pub fn available_rect(&self) -> vocab::Rect {
        self.backend.available_rect()
    }

    /// Per-frame input snapshot for custom interaction logic.
    #[must_use]
    pub fn input(&self) -> MaraInput {
        self.backend.input()
    }

    /// Device pixels per logical point — the scale factor a surface
    /// rendering into its own pixel buffer must size that buffer by.
    #[must_use]
    pub fn pixels_per_point(&self) -> f32 {
        self.backend.pixels_per_point()
    }

    /// Seconds since the host started, for time-based animation and
    /// throttling. Monotonic within a run; not a wall clock.
    #[must_use]
    pub fn now(&self) -> f64 {
        self.backend.now()
    }

    /// Ask the host to schedule another frame.
    pub fn request_repaint(&self) {
        self.backend.request_repaint();
    }

    /// Ask the host to schedule a frame no later than `after` — for
    /// surfaces polling off-thread work (tile decodes, streamed frames)
    /// that would otherwise stall until the next input event.
    pub fn request_repaint_after(&self, after: std::time::Duration) {
        self.backend.request_repaint_after(after);
    }

    /// Backend-neutral memory facade for persisted or frame-temp UI
    /// state. Uses Mara IDs and does not expose the raw backend
    /// context.
    #[must_use]
    pub fn memory(&self) -> crate::memory::BackendMemory<'_> {
        self.backend.memory()
    }

    // ── layout ───────────────────────────────────────────────────

    pub fn space(&mut self, amount: f32) {
        self.backend.add_space(SpaceSpec::vertical(amount));
    }

    /// Theme-coloured thin horizontal rule.
    pub fn separator(&mut self) {
        let w = self.backend.available_rect().width();
        let response = self
            .backend
            .allocate(vocab::Vec2::new(w, 5.0), MaraSense::Hover);
        self.backend.paint(separator_paint_cmd(
            response.rect,
            vocab::Stroke::new(1.0, crate::style::outline_base()),
        ));
    }

    pub fn horizontal<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        self.stack_scope(true, body)
    }

    pub fn vertical<R>(&mut self, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        self.stack_scope(false, body)
    }

    fn stack_scope<R>(&mut self, horizontal: bool, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        let mut body_opt = Some(body);
        let mut result = None;
        self.backend
            .in_scope(horizontal, &mut |child: &mut dyn UiBackend| {
                let mut ui = MaraUi::over(child, accent);
                if let Some(body) = body_opt.take() {
                    result = Some(body(&mut ui));
                }
            });
        result.expect("in_scope runs the body exactly once")
    }

    // ── text ─────────────────────────────────────────────────────

    /// Body text in the theme's foreground colour.
    pub fn label(&mut self, text: &str) -> MaraResponse {
        let backend = &mut self.backend;
        label_backend(
            backend,
            text,
            crate::style::theme().palette.text_primary.into(),
        )
    }

    /// Label with an explicit size, colour and family (PLAN.md WS-A6).
    ///
    /// Use it for titles, captions and any text that needs to differ
    /// from the body font while still taking part in layout.
    pub fn label_spec(
        &mut self,
        text: &str,
        spec: &crate::widget::label::LabelSpec,
    ) -> MaraResponse {
        crate::widget::label::label_spec_backend(&mut *self.backend, text, spec)
    }

    pub fn label_colored(&mut self, text: &str, color: impl Into<vocab::Color32>) -> MaraResponse {
        let backend = &mut self.backend;
        label_backend(backend, text, color.into())
    }

    // ── widgets (ambient accent) ─────────────────────────────────

    pub fn button(&mut self, label: &str) -> MaraResponse {
        let accent = self.accent;
        let height = crate::style::theme().widgets.button.row_h;
        button_backend(&mut self.backend, label, accent, height)
    }

    pub fn button_h(&mut self, label: &str, height: f32) -> MaraResponse {
        let accent = self.accent;
        button_backend(&mut self.backend, label, accent, height)
    }

    pub fn card_action_button(
        &mut self,
        glyph: &str,
        name: &str,
        subtitle: &str,
        action_glyph: &str,
        action_tooltip: &str,
    ) -> ActionButtonResponse {
        let accent = self.accent;
        card_action_button(
            &mut self.backend,
            glyph,
            name,
            subtitle,
            action_glyph,
            action_tooltip,
            accent,
        )
    }

    pub fn toggle(&mut self, label: &str, on: &mut bool) -> MaraResponse {
        let backend = &mut self.backend;
        toggle_backend(
            backend,
            label,
            on,
            self.accent,
            crate::style::theme().widgets.toggle.row_h,
        )
    }

    pub fn toggle_track_only(&mut self, on: &mut bool) -> MaraResponse {
        let toggle = crate::style::theme().widgets.toggle;
        let backend = &mut self.backend;
        toggle_track_only_backend(backend, on, self.accent, toggle.track_w, toggle.row_h)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        &mut self,
        label: &str,
        value: &mut f64,
        range: RangeInclusive<f64>,
        decimals: usize,
        suffix: &str,
    ) -> MaraResponse {
        let accent = self.accent;
        let row_height = crate::style::theme().widgets.slider.row_h;
        slider_backend(
            &mut self.backend,
            label,
            value,
            range,
            decimals,
            suffix,
            accent,
            row_height,
        )
    }

    pub fn drag_value(
        &mut self,
        label: &str,
        value: &mut f64,
        speed: f64,
        range: RangeInclusive<f64>,
        decimals: usize,
        suffix: &str,
    ) -> MaraResponse {
        let row_height = crate::style::theme().widgets.drag_value.row_h;
        drag_value_backend(
            &mut self.backend,
            label,
            value,
            speed,
            range,
            decimals,
            suffix,
            row_height,
        )
    }

    pub fn dropdown(
        &mut self,
        id_salt: impl std::hash::Hash,
        selected: &mut usize,
        options: &[&str],
    ) -> MaraResponse {
        let accent = self.accent;
        let Some(ui) = self.egui_ui_opt() else {
            return Self::noop_response();
        };
        dropdown(ui, id_salt, selected, options, accent)
    }

    pub fn select_row(
        &mut self,
        id_salt: impl std::hash::Hash,
        label: &str,
        trailing: Option<&str>,
        selected: bool,
    ) -> MaraResponse {
        let accent = self.accent;
        let height = crate::style::theme().widgets.select.row_h;
        select_row_backend(
            &mut self.backend,
            id_salt,
            label,
            trailing,
            selected,
            accent,
            height,
        )
    }

    pub fn hybrid_select_row(
        &mut self,
        id_salt: impl std::hash::Hash,
        label: &str,
        trailing: Option<&str>,
        selected: bool,
        radio_on: bool,
    ) -> HybridSelectResponse {
        let accent = self.accent;
        let height = crate::style::theme().widgets.select.row_h;
        hybrid_select_row_backend(
            &mut self.backend,
            id_salt,
            label,
            trailing,
            selected,
            radio_on,
            accent,
            height,
        )
    }

    /// Run `body` inside a themed frame and return the rect it took.
    ///
    /// Fill, stroke, corner radius, inner margin and shadow come from
    /// the [`crate::style::FrameSpec`], and the frame paints behind the
    /// content rather than over it.
    /// Take `size` from the flow and return the rect it landed in.
    ///
    /// The ordinary way to place content: the cursor advances, so the
    /// next item lands after this one. Use
    /// [`interact`](MaraUi::interact) for content placed at a rect you
    /// computed yourself.
    pub fn allocate(&mut self, size: impl Into<vocab::Vec2>, sense: crate::layout::Sense) -> MaraResponse {
        self.backend.allocate(size.into(), sense)
    }

    /// Whether `rect` is inside the visible clip — lets a surface skip
    /// painting work that would be clipped away entirely.
    #[must_use]
    pub fn is_rect_visible(&self, rect: impl Into<vocab::Rect>) -> bool {
        self.backend.is_rect_visible(rect.into())
    }

    /// Hit-test `rect` under `id` without allocating layout space.
    ///
    /// For content placed explicitly — a ribbon button at a computed
    /// rect, a hotspot over already-painted pixels — where `allocate`
    /// would disturb the flow.
    pub fn interact(
        &mut self,
        rect: impl Into<vocab::Rect>,
        id: impl Into<vocab::Id>,
        sense: crate::layout::Sense,
    ) -> MaraResponse {
        self.backend.interact(rect.into(), id.into(), sense)
    }

    /// Show `cursor` while the pointer is over `response`.
    pub fn hover_cursor(&mut self, response: &MaraResponse, cursor: crate::layout::CursorIcon) {
        self.backend.hover_cursor(response, cursor);
    }

    /// Lend this surface's backend to code written against
    /// [`UiBackend`].
    ///
    /// Not an escape hatch: `UiBackend` *is* the sealed drawing trait.
    /// It exists so helpers already written backend-neutrally can be
    /// called from a `MaraUi` without re-wrapping a raw backend.
    pub fn backend_mut(&mut self) -> &mut dyn UiBackend {
        &mut *self.backend
    }

    /// Attach hover text to a response.
    pub fn hover_text(&mut self, response: &MaraResponse, text: &str) {
        self.backend.hover_text(response, text);
    }

    /// Submit a paint command to this surface.
    ///
    /// Differs from `painter().paint_cmd` in that the surface can
    /// render commands needing more than a painter — notably
    /// [`PaintCmd::Svg`], which resolves through the host's image
    /// loader.
    pub fn paint(&mut self, cmd: crate::paint::PaintCmd) {
        self.backend.paint(cmd);
    }

    /// The rect drawing on this surface is clipped to.
    #[must_use]
    pub fn clip_rect(&self) -> vocab::Rect {
        self.painter().clip_rect()
    }

    /// The rect this surface has actually filled so far.
    ///
    /// Grows as content is placed. A parent reads it to size itself to
    /// its children — the "how big did that turn out to be?" a layout
    /// needs after the fact rather than in advance.
    #[must_use]
    pub fn occupied_rect(&self) -> vocab::Rect {
        self.backend.occupied_rect()
    }

    /// Where the next item will be placed.
    #[must_use]
    pub fn cursor(&self) -> vocab::Pos2 {
        self.backend.cursor()
    }

    /// Grow [`occupied_rect`](MaraUi::occupied_rect) to cover `rect`.
    ///
    /// Content placed outside the normal flow — an overlay, a pin
    /// straddling a node's edge — is invisible to the parent's sizing
    /// unless it says so here.
    pub fn expand_to_include(&mut self, rect: impl Into<vocab::Rect>) {
        self.backend.expand_to_include(rect.into());
    }

    /// Move the flow cursor past `rect`, so later items land after it.
    pub fn advance_cursor_past(&mut self, rect: impl Into<vocab::Rect>) {
        self.backend.advance_cursor_past(rect.into());
    }

    /// Run `body` with drawing clipped to `rect`.
    ///
    /// Scoped rather than a push/pop pair: an unbalanced clip silently
    /// corrupts every later draw on the surface, and a scope cannot be
    /// left unbalanced. Clips only ever shrink — `rect` is intersected
    /// with the current one.
    pub fn clipped<R>(&mut self, rect: impl Into<vocab::Rect>, body: impl FnOnce(&mut MaraUi<'_>) -> R) -> R {
        let accent = self.accent;
        self.backend.push_clip(rect.into());
        let out = {
            let mut inner = MaraUi::over(&mut *self.backend, accent);
            body(&mut inner)
        };
        self.backend.pop_clip();
        out
    }

    /// Pan and zoom everything drawn on this surface's layer.
    ///
    /// The transform applies to the layer as a whole, so content is
    /// laid out in its own coordinates and moved as one — which is what
    /// a zoomable canvas wants, rather than every child scaling itself.
    pub fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        self.backend.set_layer_transform(transform);
    }

    /// Reserve a place in the paint order to fill in later.
    ///
    /// Paint order is submission order, so a surface that must draw
    /// *behind* content whose size it only learns afterwards — a halo
    /// around a node, a highlight behind a row — cannot simply draw
    /// later. It reserves a slot first, then fills it once the geometry
    /// is known, and the command lands at the reserved depth.
    ///
    /// Pair with [`MaraUi::fill_paint_slot`]. An unfilled slot paints
    /// nothing.
    #[must_use]
    pub fn reserve_paint_slot(&mut self) -> crate::layout::PaintSlot {
        self.backend.reserve_paint_slot()
    }

    /// Fill a slot from [`MaraUi::reserve_paint_slot`].
    ///
    /// `None` leaves the slot inert, so a caller that reserves
    /// unconditionally and decides later needs no special case.
    pub fn fill_paint_slot(
        &mut self,
        slot: crate::layout::PaintSlot,
        cmd: Option<crate::paint::PaintCmd>,
    ) {
        self.backend.fill_paint_slot(slot, cmd);
    }

    /// Multiply this surface's style metrics by `factor`.
    ///
    /// Used by zoomable surfaces that render at a magnified style and
    /// scale down, so text stays crisp instead of being blown up after
    /// rasterisation.
    pub fn scale_style(&mut self, factor: f32) {
        self.backend.scale_style(factor);
    }

    pub fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: impl FnOnce(&mut MaraUi<'_>),
    ) -> vocab::Rect {
        self.framed_with(spec, body).0
    }

    /// [`MaraUi::framed`], keeping what the body returned.
    ///
    /// The frame's rect is only known after the body runs, so a caller
    /// that needs both the geometry and a value computed inside — the
    /// row a node's header ended up on, a hit test against content —
    /// would otherwise have to smuggle it out through a captured
    /// variable.
    ///
    /// Returns `(rect, inner)`.
    pub fn framed_with<R>(
        &mut self,
        spec: crate::style::FrameSpec,
        body: impl FnOnce(&mut MaraUi<'_>) -> R,
    ) -> (vocab::Rect, R) {
        let accent = self.accent;
        let mut body = Some(body);
        let mut inner = None;
        let rect = self.backend.framed(spec, &mut |backend| {
            if let Some(body) = body.take() {
                let mut mara = MaraUi::over(backend, accent);
                inner = Some(body(&mut mara));
            }
        });
        // The backend contract is that `framed` runs the body exactly
        // once. A backend that skipped it would be broken in ways no
        // fallback here could paper over, so say so plainly.
        let inner = inner.expect("UiBackend::framed must run its body exactly once");
        (rect, inner)
    }

    /// Fixed-size row laid out left-to-right, contents aligned on the
    /// cross axis.
    ///
    /// The sealed equivalent of allocating a sized region with a
    /// layout — how a node renderer builds pin rows whose contents sit
    /// centred rather than hanging from the top edge.
    pub fn row(
        &mut self,
        size: impl Into<vocab::Vec2>,
        align: crate::layout::CrossAlign,
        body: impl FnOnce(&mut MaraUi<'_>),
    ) {
        let accent = self.accent;
        let mut body = Some(body);
        self.backend.in_row(size.into(), align, &mut |backend| {
            if let Some(body) = body.take() {
                let mut mara = MaraUi::over(backend, accent);
                body(&mut mara);
            }
        });
    }

    /// Upload CPU pixels as a texture and return a handle to paint it
    /// with [`MaraPainter::image`]. `None` on backends with no texture
    /// store (the recording one), so callers must handle a miss.
    pub fn load_texture(
        &mut self,
        name: &str,
        image: vocab::ColorImage,
        options: vocab::TextureOptions,
    ) -> Option<vocab::TextureHandle> {
        self.backend.load_texture(name, image, options)
    }

    /// Shut the menu opened by [`MaraUi::menu_button`] with this id.
    ///
    /// Menu items call this after acting, so the menu dismisses the way
    /// a user expects rather than staying open behind the change.
    pub fn close_menu(&mut self, id: impl Into<vocab::Id>) {
        let id = id.into();
        let mut memory = self.backend.memory();
        let mut state = crate::popup::PopupState::load(&memory, id);
        state.close();
        state.store(&mut memory, id);
    }

    /// A button that toggles a floating menu below itself
    /// (PLAN.md WS-E1.1).
    ///
    /// Open state lives in [`crate::popup::PopupState`] under `id`, so
    /// it survives between frames and two menus cannot fight over one
    /// key. `body` renders into an overlay-layer surface anchored under
    /// the button; it runs only while open.
    ///
    /// The sealed replacement for the backend's menu widget — the last
    /// thing `mara_graph`'s viewer needed that `MaraUi` could not
    /// express.
    pub fn menu_button(
        &mut self,
        id: impl Into<vocab::Id>,
        label: &str,
        body: impl FnOnce(&mut MaraUi<'_>),
    ) {
        let id = id.into();
        let response = self.button(label);

        let mut state = {
            let memory = self.backend.memory();
            crate::popup::PopupState::load(&memory, id)
        };
        if response.clicked {
            state.toggle();
            let mut memory = self.backend.memory();
            state.store(&mut memory, id);
        }
        if !state.is_open() {
            return;
        }

        let anchor = vocab::Pos2::new(response.rect.min.x, response.rect.max.y);
        let accent = self.accent;
        let mut body = Some(body);
        self.backend.overlay_at(id, anchor, &mut |backend| {
            if let Some(body) = body.take() {
                let mut mara = MaraUi::over(backend, accent);
                body(&mut mara);
            }
        });
    }

    /// Multi-line text editing surface — the sealed counterpart to a
    /// code editor's text pane (PLAN.md WS-A8).
    ///
    /// Build it with [`crate::widget::text_area::MaraTextArea`] to set
    /// rows, font size and a per-line syntax highlighter.
    pub fn text_area(
        &mut self,
        area: crate::widget::text_area::MaraTextArea<'_>,
        text: &mut String,
    ) -> crate::widget::text_area::MaraTextAreaResponse {
        area.show(&mut *self.backend, text)
    }

    pub fn text_input(&mut self, text: &mut String, placeholder: &str) -> MaraResponse {
        let accent = self.accent;
        let Some(ui) = self.egui_ui_opt() else {
            return Self::noop_response();
        };
        text_input(ui, text, placeholder, accent)
    }

    pub fn readout(&mut self, label: &str, value: &str) -> MaraResponse {
        let backend = &mut self.backend;
        readout_backend(
            backend,
            label,
            value,
            crate::style::theme().widgets.readout.row_h,
        )
    }

    pub fn readout_h(&mut self, label: &str, value: &str, height: f32) -> MaraResponse {
        let backend = &mut self.backend;
        readout_backend(backend, label, value, height)
    }

    pub fn chip(&mut self, label: &str) -> MaraResponse {
        let backend = &mut self.backend;
        chip_colored_backend(backend, label, chip_fill(self.accent), self.accent)
    }

    pub fn chip_colored(&mut self, label: &str, fill: impl Into<vocab::Color32>) -> MaraResponse {
        let backend = &mut self.backend;
        chip_colored_backend(backend, label, fill.into(), self.accent)
    }

    pub fn badge_row(&mut self, label: &str, badges: &[&str]) -> MaraResponse {
        let backend = &mut self.backend;
        badge_row_backend(backend, label, badges, None, self.accent)
    }

    pub fn badge_row_colored(
        &mut self,
        label: &str,
        badges: &[(&str, Option<vocab::Color32>)],
    ) -> MaraResponse {
        let labels: Vec<&str> = badges.iter().map(|(label, _)| *label).collect();
        let fills: Vec<Option<vocab::Color32>> = badges.iter().map(|(_, fill)| *fill).collect();
        let backend = &mut self.backend;
        badge_row_backend(backend, label, &labels, Some(&fills), self.accent)
    }

    pub fn keybinding_row(&mut self, keys: &str, action: &str) -> MaraResponse {
        let backend = &mut self.backend;
        keybinding_row_backend(
            backend,
            keys,
            action,
            crate::style::theme().widgets.keybinding.row_h,
            self.accent,
        )
    }

    pub fn progressbar(&mut self, label: &str, fraction: f32, text: &str) -> MaraResponse {
        let backend = &mut self.backend;
        progressbar_backend(
            backend,
            label,
            fraction,
            text,
            self.accent,
            crate::style::theme().widgets.progress.row_h,
        )
    }

    pub fn color_rgb(&mut self, label: &str, rgb: &mut [f32; 3]) -> MaraResponse {
        let accent = self.accent;
        let Some(ui) = self.egui_ui_opt() else {
            return Self::noop_response();
        };
        color_rgb(ui, label, rgb, accent)
    }

    pub fn color_rgba(&mut self, label: &str, rgba: &mut [f32; 4]) -> MaraResponse {
        let accent = self.accent;
        let Some(ui) = self.egui_ui_opt() else {
            return Self::noop_response();
        };
        color_rgba(ui, label, rgba, accent)
    }

    /// Foldable titled section whose body is itself a sealed
    /// [`MaraUi`].
    pub fn section(
        &mut self,
        id_salt: &str,
        title: &str,
        default_open: bool,
        body: impl FnOnce(&mut MaraUi<'_>),
    ) {
        let accent = self.accent;
        let mut body_opt = Some(body);
        section_backend(
            &mut *self.backend,
            id_salt,
            title,
            accent,
            default_open,
            &mut |child: &mut dyn UiBackend| {
                let mut ui = MaraUi::over(child, accent);
                if let Some(body) = body_opt.take() {
                    body(&mut ui);
                }
            },
        );
    }

    /// Mara-styled right-click context menu on a previous response.
    pub fn context_menu(&mut self, resp: &MaraResponse, body: impl FnOnce(&mut MaraUi<'_>)) {
        let accent = self.accent;
        let Some(ui) = self.egui_ui_opt() else {
            return;
        };
        backend::egui::with_response_for_ui(ui, resp, |raw| {
            context_menu_mara(raw, accent, |ui| {
                let mut backend = MaraBackend::Egui(backend::egui::EguiUiBackend::new(ui));
                body(&mut MaraUi::over(&mut backend, accent));
            });
        });
    }

    /// Recursive tree built from Mara tree rows. The closure only
    /// sees [`TreeBody`].
    pub fn tree(&mut self, body: impl FnOnce(&mut TreeBody<'_>)) {
        let mut tb = TreeBody::new(&mut self.backend);
        body(&mut tb);
    }

    /// Render a fully-typed [`Pod`] inline.
    pub fn pod(&mut self, pod: Pod) -> PodResponse {
        let Some(ui) = self.egui_ui_opt() else {
            return PodResponse::default();
        };
        pod.show(ui)
    }

    // ── custom drawing ───────────────────────────────────────────

    /// Allocate a custom-drawing slot of `desired_size`, returning
    /// a clipped [`MaraPainter`] plus the slot's interaction
    /// response. This is the sealed equivalent of egui's
    /// `allocate_painter` — the primitive for gauges, plots, and
    /// other bespoke visuals.
    pub fn canvas(&mut self, desired_size: impl Into<vocab::Vec2>) -> (MaraPainter, MaraResponse) {
        let response = self
            .backend
            .allocate(desired_size.into(), MaraSense::ClickAndDrag);
        let painter = self
            .backend
            .make_painter(PaintSurfaceSpec::clipped(response.rect));
        (painter, response)
    }

    /// Interactive custom-drawing surface over an exact screen-space
    /// rect (e.g. a shelf-aware viewport). Returns a clipped
    /// [`MaraPainter`] plus the rect's click/drag response. Unlike
    /// [`MaraUi::canvas`], this does not advance the layout cursor.
    pub fn canvas_at(&mut self, rect: impl Into<vocab::Rect>) -> (MaraPainter, MaraResponse) {
        let rect = rect.into();
        let id = canvas_at_id(self.backend.id(), rect);
        let response = self.backend.interact(rect, id, MaraSense::ClickAndDrag);
        let painter = self.backend.make_painter(PaintSurfaceSpec::clipped(rect));
        (painter, response)
    }

    /// A painter over the remaining available rect, without
    /// allocating it (drawing only, no interaction).
    #[must_use]
    pub fn painter(&self) -> MaraPainter {
        self.backend
            .make_painter(PaintSurfaceSpec::remaining_available())
    }
}

fn separator_paint_cmd(rect: vocab::Rect, stroke: vocab::Stroke) -> PaintCmd {
    PaintCmd::Line {
        a: rect.left_center(),
        b: rect.right_center(),
        stroke,
    }
}

fn canvas_at_id(scope_id: vocab::Id, rect: vocab::Rect) -> vocab::Id {
    scope_id.with(("mara_canvas_at", rect.min.x as i64, rect.min.y as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> vocab::Rect {
        vocab::Rect::from_min_size(vocab::Pos2::new(x, y), vocab::Vec2::new(w, h))
    }

    #[test]
    fn recording_painter_sinks_commands_behind_current_clip() {
        let clip = rect(0.0, 0.0, 100.0, 100.0);
        let painter = MaraPainter::recording(clip);

        painter.line_segment(
            vocab::Pos2::new(1.0, 2.0),
            vocab::Pos2::new(3.0, 4.0),
            vocab::Stroke::new(2.0, vocab::Color32::WHITE),
        );

        let commands = painter.__internal_recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, clip);

        let [PaintCmd::Line { a, b, stroke }] = children.as_slice() else {
            panic!("expected clipped line child command");
        };
        assert_eq!(*a, vocab::Pos2::new(1.0, 2.0));
        assert_eq!(*b, vocab::Pos2::new(3.0, 4.0));
        assert_eq!(*stroke, vocab::Stroke::new(2.0, vocab::Color32::WHITE));
    }

    #[test]
    fn recording_painter_with_clip_only_shrinks_recorded_clip() {
        let root_clip = rect(0.0, 0.0, 100.0, 100.0);
        let sub_clip = rect(10.0, 20.0, 40.0, 30.0);
        let painter = MaraPainter::recording(root_clip);
        let clipped = painter.with_clip(sub_clip);

        clipped.rect_filled(
            rect(0.0, 0.0, 80.0, 80.0),
            vocab::CornerRadius::ZERO,
            vocab::Color32::from_rgb(10, 20, 30),
        );

        assert_eq!(clipped.clip_rect(), sub_clip);

        let commands = painter.__internal_recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, root_clip.intersect(sub_clip));

        let [
            PaintCmd::RectFilled {
                rect: painted_rect,
                corner,
                fill,
            },
        ] = children.as_slice()
        else {
            panic!("expected clipped rect fill child command");
        };
        assert_eq!(*painted_rect, rect(0.0, 0.0, 80.0, 80.0));
        assert_eq!(*corner, vocab::CornerRadius::ZERO);
        assert_eq!(*fill, vocab::Color32::from_rgb(10, 20, 30));
    }

    #[test]
    fn recording_painter_text_returns_position_rect_and_records_text() {
        let clip = rect(0.0, 0.0, 100.0, 100.0);
        let painter = MaraPainter::recording(clip);
        let pos = vocab::Pos2::new(5.0, 6.0);

        let painted = painter.text(
            pos,
            vocab::Align2::LEFT_TOP,
            "hello",
            13.0,
            vocab::Color32::GRAY,
        );

        assert_eq!(painted, vocab::Rect::from_min_size(pos, vocab::Vec2::ZERO));

        let commands = painter.__internal_recorded_commands();
        let [
            PaintCmd::Clip {
                rect: recorded_clip,
                children,
            },
        ] = commands.as_slice()
        else {
            panic!("expected one clipped retained command");
        };
        assert_eq!(*recorded_clip, clip);

        let [
            PaintCmd::Text {
                pos: recorded_pos,
                anchor,
                text,
                size,
                color,
                mono,
            },
        ] = children.as_slice()
        else {
            panic!("expected clipped text child command");
        };
        assert_eq!(*recorded_pos, pos);
        assert_eq!(*anchor, vocab::Align2::LEFT_TOP);
        assert_eq!(text, "hello");
        assert_eq!(*size, 13.0);
        assert_eq!(*color, vocab::Color32::GRAY);
        assert!(!mono);
    }

    #[test]
    fn separator_lowers_to_backend_neutral_line_command() {
        let rect = rect(10.0, 20.0, 120.0, 5.0);
        let stroke = vocab::Stroke::new(1.0, vocab::Color32::WHITE);
        let PaintCmd::Line { a, b, stroke: s } = separator_paint_cmd(rect, stroke) else {
            panic!("expected separator line command");
        };

        assert_eq!(a, rect.left_center());
        assert_eq!(b, rect.right_center());
        assert_eq!(s, stroke);
    }

    #[test]
    fn canvas_at_id_is_scope_and_position_stable() {
        let scope = vocab::Id::new("scope");
        let rect = rect(10.25, 20.75, 120.0, 80.0);

        assert_eq!(
            canvas_at_id(scope, rect),
            scope.with(("mara_canvas_at", 10_i64, 20_i64))
        );
    }
}
