//! Backend-neutral layout/backend contract.
//!
//! This is intentionally small. Mara's editor UI language does not
//! need to mirror every egui primitive; it needs enough allocation,
//! clipping and paint submission to move widgets away from direct
//! backend calls one family at a time.

use crate::{
    mui::{MaraInput, MaraResponse},
    paint::PaintCmd,
    vocab::{Color32, CornerRadius, Id, Pos2, Rect, Vec2},
};

/// Cross-axis alignment for [`UiBackend::in_row`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossAlign {
    Start,
    Center,
    End,
}

/// The z-ordering band an [`AreaHost`] paints and interacts in — the
/// backend-neutral layer contract (PLAN.md Phase 4).
///
/// Bands paint back-to-front in variant order: **`Background` <
/// `Middle` < `Foreground` < `Overlay`** ([`Layer::rank`] gives the
/// numeric order). Areas in a higher band paint over — and take
/// pointer input ahead of — every area in a lower band. Within one
/// band, ties break by paint order (later wins), matching immediate
/// mode. A backend MUST honour this ordering; the egui backend maps it
/// onto `egui::Order` (`Overlay` → `Tooltip`, so floating palettes sit
/// above foreground scrims).
///
/// Occlusion is governed by [`AreaHost::interactable`]: a
/// non-interactive area paints but never consumes pointer input, so
/// clicks fall through to whatever is beneath it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// Behind all content — backdrops, scrims, the root body fill.
    Background,
    /// Default band for docked chrome (panes, shelves, ribbons).
    Middle,
    /// Above docked chrome — drag ghosts, active-drag surfaces.
    Foreground,
    /// Top transient UI — command palette, dropdown popups, tooltips,
    /// context menus. Always paints and hit-tests above everything.
    Overlay,
}

impl Layer {
    /// The layer's back-to-front rank (`Background` = 0 …
    /// `Overlay` = 3). Backends without egui's `Order` sort areas by
    /// this; the value equals the enum's `Ord` position.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Layer::Background => 0,
            Layer::Middle => 1,
            Layer::Foreground => 2,
            Layer::Overlay => 3,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaHost {
    pub id: Id,
    pub pos: Pos2,
    pub layer: Layer,
    pub interactable: bool,
}

impl AreaHost {
    #[must_use]
    pub const fn new(id: Id, pos: Pos2, layer: Layer) -> Self {
        Self {
            id,
            pos,
            layer,
            interactable: true,
        }
    }

    #[must_use]
    pub const fn non_interactive(mut self) -> Self {
        self.interactable = false;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaSlotSpec {
    pub host: AreaHost,
    pub size: Vec2,
}

impl AreaSlotSpec {
    #[must_use]
    pub const fn new(host: AreaHost, size: Vec2) -> Self {
        Self { host, size }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasSlotSpec {
    pub size: Vec2,
    pub sense: Sense,
}

impl CanvasSlotSpec {
    #[must_use]
    pub const fn new(size: Vec2, sense: Sense) -> Self {
        Self { size, sense }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasRectSpec {
    pub id: Id,
    pub rect: Rect,
    pub sense: Sense,
}

impl CanvasRectSpec {
    #[must_use]
    pub const fn new(id: Id, rect: Rect, sense: Sense) -> Self {
        Self { id, rect, sense }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaintSurfaceRegion {
    RemainingAvailable,
    ClipRect(Rect),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaintSurfaceSpec {
    pub region: PaintSurfaceRegion,
}

impl PaintSurfaceSpec {
    #[must_use]
    pub const fn remaining_available() -> Self {
        Self {
            region: PaintSurfaceRegion::RemainingAvailable,
        }
    }

    #[must_use]
    pub const fn clipped(rect: Rect) -> Self {
        Self {
            region: PaintSurfaceRegion::ClipRect(rect),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollRegion {
    pub id: Id,
    pub axis: ScrollAxis,
    pub auto_shrink: [bool; 2],
    pub max_extent: f32,
    pub item_spacing: Vec2,
}

impl ScrollRegion {
    #[must_use]
    pub const fn new(id: Id, auto_shrink: [bool; 2], max_extent: f32, row_spacing_y: f32) -> Self {
        Self::vertical(id, auto_shrink, max_extent, Vec2::new(0.0, row_spacing_y))
    }

    #[must_use]
    pub const fn vertical(
        id: Id,
        auto_shrink: [bool; 2],
        max_extent: f32,
        item_spacing: Vec2,
    ) -> Self {
        Self {
            id,
            axis: ScrollAxis::Vertical,
            auto_shrink,
            max_extent,
            item_spacing,
        }
    }

    #[must_use]
    pub const fn horizontal(
        id: Id,
        auto_shrink: [bool; 2],
        max_extent: f32,
        item_spacing: Vec2,
    ) -> Self {
        Self {
            id,
            axis: ScrollAxis::Horizontal,
            auto_shrink,
            max_extent,
            item_spacing,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PopupAlign {
    BottomStart,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupSpec {
    pub align: PopupAlign,
    pub gap: f32,
    pub width: f32,
    pub inner_margin: i8,
}

impl PopupSpec {
    #[must_use]
    pub const fn new(align: PopupAlign, gap: f32, width: f32, inner_margin: i8) -> Self {
        Self {
            align,
            gap,
            width,
            inner_margin,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PopupTrigger {
    pub response_id: Id,
    pub popup_id: Id,
}

impl PopupTrigger {
    #[must_use]
    pub const fn new(response_id: Id, popup_id: Id) -> Self {
        Self {
            response_id,
            popup_id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupListSpec {
    pub item_spacing: Vec2,
}

impl PopupListSpec {
    #[must_use]
    pub const fn new(item_spacing: Vec2) -> Self {
        Self { item_spacing }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextEditRegion {
    pub rect: Rect,
    pub text_rect: Rect,
    pub font_size: f32,
}

impl TextEditRegion {
    #[must_use]
    pub const fn new(rect: Rect, text_rect: Rect, font_size: f32) -> Self {
        Self {
            rect,
            text_rect,
            font_size,
        }
    }

    #[must_use]
    pub fn desired_width(&self) -> f32 {
        self.text_rect.width().max(0.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMeasureSpec {
    pub text: String,
    pub size: f32,
    pub mono: bool,
}

impl TextMeasureSpec {
    #[must_use]
    pub fn new(text: impl Into<String>, size: f32, mono: bool) -> Self {
        Self {
            text: text.into(),
            size,
            mono,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextEditSpec {
    pub region: TextEditRegion,
    pub hint: String,
    pub text_color: Color32,
    pub hint_color: Color32,
    pub background_color: Color32,
    pub frame: bool,
}

impl TextEditSpec {
    #[must_use]
    pub fn singleline(
        region: TextEditRegion,
        hint: impl Into<String>,
        text_color: Color32,
        hint_color: Color32,
    ) -> Self {
        Self {
            region,
            hint: hint.into(),
            text_color,
            hint_color,
            background_color: Color32::TRANSPARENT,
            frame: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InlinePickerSpec {
    pub slider_width: f32,
    pub clip_expand: f32,
}

impl InlinePickerSpec {
    #[must_use]
    pub const fn new(slider_width: f32, clip_expand: f32) -> Self {
        Self {
            slider_width,
            clip_expand,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ColorPickerAlpha {
    Opaque,
    OnlyBlend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IndentedBodySpec {
    pub id: Id,
}

impl IndentedBodySpec {
    #[must_use]
    pub const fn new(id: Id) -> Self {
        Self { id }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameHostSpec {
    pub outer_width: f32,
    pub content_width: f32,
    pub inner_margin: [i8; 2],
    pub corner: CornerRadius,
}

impl FrameHostSpec {
    #[must_use]
    pub const fn new(
        outer_width: f32,
        content_width: f32,
        inner_margin: [i8; 2],
        corner: CornerRadius,
    ) -> Self {
        Self {
            outer_width,
            content_width,
            inner_margin,
            corner,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpaceSpec {
    pub size: Vec2,
}

impl SpaceSpec {
    #[must_use]
    pub const fn new(size: Vec2) -> Self {
        Self { size }
    }

    #[must_use]
    pub const fn vertical(height: f32) -> Self {
        Self {
            size: Vec2::new(0.0, height),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ItemSpacingSpec {
    pub item_spacing: Vec2,
}

impl ItemSpacingSpec {
    #[must_use]
    pub const fn new(item_spacing: Vec2) -> Self {
        Self { item_spacing }
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self::new(Vec2::ZERO)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StackDirection {
    TopDown,
    BottomUp,
    LeftToRight,
    RightToLeft,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StackAlign {
    Min,
    Center,
    Max,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StackScopeSpec {
    pub direction: StackDirection,
}

impl StackScopeSpec {
    #[must_use]
    pub const fn horizontal() -> Self {
        Self {
            direction: StackDirection::LeftToRight,
        }
    }

    #[must_use]
    pub const fn vertical() -> Self {
        Self {
            direction: StackDirection::TopDown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ChildRegion {
    pub rect: Rect,
    pub direction: StackDirection,
    pub align: StackAlign,
}

impl ChildRegion {
    #[must_use]
    pub const fn new(rect: Rect, direction: StackDirection, align: StackAlign) -> Self {
        Self {
            rect,
            direction,
            align,
        }
    }

    #[must_use]
    pub const fn top_down(rect: Rect, align: StackAlign) -> Self {
        Self::new(rect, StackDirection::TopDown, align)
    }

    #[must_use]
    pub const fn left_to_right(rect: Rect, align: StackAlign) -> Self {
        Self::new(rect, StackDirection::LeftToRight, align)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ContainerBodySpec {
    /// `true` when the owning title strip runs horizontally. The
    /// body always stacks vertically, but this tells the backend
    /// which parent axis is locked by `span_inner`.
    pub horizontal_strip: bool,
    pub span_inner: f32,
    pub max_flow: Option<f32>,
    pub end_pad: f32,
}

impl ContainerBodySpec {
    #[must_use]
    pub const fn new(
        horizontal_strip: bool,
        span_inner: f32,
        max_flow: Option<f32>,
        end_pad: f32,
    ) -> Self {
        Self {
            horizontal_strip,
            span_inner,
            max_flow,
            end_pad,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneFlexSpec {
    pub horizontal_strip: bool,
    pub span_inner: f32,
    pub title_thickness: f32,
    pub body_gap: f32,
    pub item_spacing: Vec2,
}

impl PaneFlexSpec {
    #[must_use]
    pub const fn new(
        horizontal_strip: bool,
        span_inner: f32,
        title_thickness: f32,
        body_gap: f32,
    ) -> Self {
        Self {
            horizontal_strip,
            span_inner,
            title_thickness,
            body_gap,
            item_spacing: Vec2::ZERO,
        }
    }

    #[must_use]
    pub const fn title_size(&self) -> Vec2 {
        if self.horizontal_strip {
            Vec2::new(self.span_inner, self.title_thickness)
        } else {
            Vec2::new(self.title_thickness, self.span_inner)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PaneBodyScrollAxis {
    FlowVertical,
    FlowHorizontal,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneBodyScrollSpec {
    pub id: Id,
    pub axis: PaneBodyScrollAxis,
    pub span_inner: f32,
    pub item_spacing: Vec2,
}

impl PaneBodyScrollSpec {
    #[must_use]
    pub const fn new(id: Id, horizontal_strip: bool, span_inner: f32) -> Self {
        Self {
            id,
            axis: if horizontal_strip {
                PaneBodyScrollAxis::FlowVertical
            } else {
                PaneBodyScrollAxis::FlowHorizontal
            },
            span_inner,
            item_spacing: Vec2::ZERO,
        }
    }

    #[must_use]
    pub const fn horizontal_strip(&self) -> bool {
        matches!(self.axis, PaneBodyScrollAxis::FlowVertical)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlotRibbonLayoutSpec {
    pub id: Id,
    pub pos: Pos2,
    pub size: Vec2,
    pub vertical: bool,
    pub button_size: f32,
    pub button_gap: f32,
    pub count: usize,
}

impl SlotRibbonLayoutSpec {
    #[must_use]
    pub fn new(
        id: Id,
        pos: Pos2,
        vertical: bool,
        count: usize,
        button_size: f32,
        button_gap: f32,
    ) -> Self {
        let count_f = count as f32;
        let span = count_f * button_size + (count_f - 1.0).max(0.0) * button_gap;
        let size = if vertical {
            Vec2::new(button_size, span)
        } else {
            Vec2::new(span, button_size)
        };
        Self {
            id,
            pos,
            size,
            vertical,
            button_size,
            button_gap,
            count,
        }
    }

    /// Item rect in **local** coordinates (relative to the ribbon's
    /// own origin, i.e. item 0 starts at `(0, 0)`). Use this only for
    /// layout math; for egui interaction/paint inside the area opened at
    /// [`Self::pos`] use [`Self::item_screen_rect`].
    #[must_use]
    pub fn item_rect(&self, idx: usize) -> Option<Rect> {
        if idx >= self.count {
            return None;
        }
        let offset = idx as f32 * (self.button_size + self.button_gap);
        let min = if self.vertical {
            Pos2::new(0.0, offset)
        } else {
            Pos2::new(offset, 0.0)
        };
        Some(Rect::from_min_size(
            min,
            Vec2::new(self.button_size, self.button_size),
        ))
    }

    /// Item rect in **screen** coordinates — [`Self::item_rect`] offset
    /// by this ribbon's screen position [`Self::pos`].
    ///
    /// The ribbon's buttons live inside an `Area` positioned at `pos`,
    /// and egui interaction/paint operate in screen space, so this is
    /// the rect to feed `UiBackend::interact` and paint commands. Using
    /// the local [`Self::item_rect`] there places every button at the
    /// window origin (top-left).
    #[must_use]
    pub fn item_screen_rect(&self, idx: usize) -> Option<Rect> {
        self.item_rect(idx)
            .map(|r| r.translate(Vec2::new(self.pos.x, self.pos.y)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sense {
    Hover,
    Click,
    Drag,
    ClickAndDrag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    PointingHand,
    Grabbing,
    ResizeHorizontal,
    ResizeVertical,
}

pub trait UiBackend {
    fn begin_area(&mut self, host: AreaHost, rect: Rect);
    fn allocate(&mut self, size: Vec2, sense: Sense) -> MaraResponse;
    fn reserve_space(&mut self, size: Vec2) -> Rect {
        let rect = Rect::from_min_size(self.available_rect().min, size);
        let _ = self.reserve_rect(rect, Sense::Hover);
        rect
    }
    fn reserve_rect(&mut self, rect: Rect, sense: Sense) -> MaraResponse {
        self.interact(
            rect,
            Id::new((
                "mara-reserved-rect",
                rect.min.x.to_bits(),
                rect.min.y.to_bits(),
                rect.max.x.to_bits(),
                rect.max.y.to_bits(),
            )),
            sense,
        )
    }
    fn interact(&mut self, rect: Rect, id: Id, sense: Sense) -> MaraResponse;
    fn available_rect(&self) -> Rect;
    /// Stable identity of the current scope, for salting widget ids.
    ///
    /// A real layout backend must override this with its scope's
    /// stable id; the default constant is only adequate for stateless
    /// recording/test backends that never salt persisted state.
    fn id(&self) -> Id {
        Id::new("mara-ui-backend")
    }
    /// Remaining width before wrap. Defaults to `available_rect().width()`;
    /// backends may override for an exact native read.
    fn available_width(&self) -> f32 {
        self.available_rect().width()
    }
    /// Remaining height before wrap. Defaults to `available_rect().height()`;
    /// backends may override for an exact native read.
    fn available_height(&self) -> f32 {
        self.available_rect().height()
    }
    /// Per-frame input snapshot for custom interaction logic. Defaults
    /// to an empty snapshot for backends with no live input source.
    fn input(&self) -> MaraInput {
        MaraInput::default()
    }
    /// Backend-neutral persisted/temp state + animation clock for the
    /// current scope. Widgets reach state ONLY through this — never a
    /// raw backend context (PLAN.md Phase 2.2, ADR 0001).
    fn memory(&self) -> crate::memory::BackendMemory<'_>;
    /// Advance the layout cursor by a fixed gap. No-op default for
    /// backends that do not track a flowing layout cursor.
    fn add_space(&mut self, _spec: SpaceSpec) {}
    fn push_clip(&mut self, rect: Rect);
    fn pop_clip(&mut self);
    fn measure_text(&self, text: &str, size: f32, mono: bool) -> Vec2;
    fn paint(&mut self, cmd: PaintCmd);

    /// Reserve a paint slot at the current z-position, to be filled
    /// later with [`UiBackend::fill_paint_slot`]. Lets a widget paint
    /// a background *beneath* content drawn after the reservation
    /// (e.g. a tree row's selection fill under its label). The default
    /// degrades to inline painting (no z-deferral); backends with a
    /// shape list override it.
    fn reserve_paint_slot(&mut self) -> PaintSlot {
        PaintSlot::INLINE
    }

    /// Fill (or clear, with `None`) a slot from
    /// [`UiBackend::reserve_paint_slot`]. The default paints `cmd`
    /// immediately at the current position.
    fn fill_paint_slot(&mut self, slot: PaintSlot, cmd: Option<PaintCmd>) {
        let _ = slot;
        if let Some(cmd) = cmd {
            self.paint(cmd);
        }
    }

    /// Show hover-tooltip `text` for a previously-returned response.
    /// No-op on backends without an overlay layer.
    /// Show `cursor` while the pointer is over `response`.
    ///
    /// The default does nothing: a backend with no pointer cursor
    /// (a recording backend, a headless test) is not wrong to ignore
    /// this, and the drag still works.
    fn hover_cursor(&mut self, response: &MaraResponse, cursor: CursorIcon) {
        let _ = (response, cursor);
    }

    fn hover_text(&mut self, response: &MaraResponse, text: &str) {
        let _ = (response, text);
    }

    /// Whether `rect` is within the visible viewport — a culling hint
    /// widgets use to skip offscreen paint work. Defaults to always
    /// visible for backends without a viewport.
    fn is_rect_visible(&self, rect: Rect) -> bool {
        let _ = rect;
        true
    }

    /// The concrete `egui::Ui` this backend drives, if it is the egui
    /// backend. `None` on every other backend.
    ///
    /// This is the object-safe escape hatch for the shrinking set of
    /// `MaraUi` operations not yet promoted to the contract (stack
    /// scopes, canvas, pod, context menu, painter, the raw hatch). It
    /// keeps `MaraUi` able to hold `&mut dyn UiBackend` — a plain
    /// downcast can't reach `EguiUiBackend`, which is not `'static`.
    /// Tracked by the coupling ratchet's `ui_escapes` metric. Returns
    /// `None` by default so non-egui backends need not implement it.
    /// First-party hook — hidden and `__internal_` like every other
    /// seal escape; consumer code must never reach a raw `egui::Ui`.
    #[doc(hidden)]
    fn __internal_egui_ui_mut(&mut self) -> Option<&mut egui::Ui> {
        None
    }

    /// Shared reference to the concrete `egui::Ui`, if this is the egui
    /// backend. See [`UiBackend::__internal_egui_ui_mut`]. First-party
    /// hook — hidden like every other seal escape.
    #[doc(hidden)]
    fn __internal_egui_ui_ref(&self) -> Option<&egui::Ui> {
        None
    }

    /// Run `body` in a child region inset by `inset_left` px from the
    /// current region's left edge (an indent). Content flows inside the
    /// child; afterwards the parent's layout continues below it. This
    /// is the backend-neutral nesting primitive chrome uses to lay out
    /// bodies inside frames (PLAN.md Phase 4). Object-safe: the closure
    /// takes `&mut dyn UiBackend`, so `MaraUi` (which holds
    /// `&mut dyn UiBackend`) can wrap the child. `id` salts the child
    /// scope's persisted state.
    /// Run `body` in a sub-region occupying exactly `rect`.
    ///
    /// Unlike [`UiBackend::in_child`], which insets and inherits the
    /// parent's flow, this places the child at an explicit rect — what a
    /// renderer that computes its own geometry needs (node bodies,
    /// headers, pin rows). The parent's cursor is untouched; call
    /// [`UiBackend::advance_cursor_past`] if the child should consume
    /// flow space.
    /// Apply `transform` to this region's whole layer — content space
    /// to screen space (see [`crate::transform`]).
    ///
    /// Everything painted into the layer moves and scales together, so
    /// a pannable canvas neither re-lays-out nor re-rasterises on every
    /// gesture frame. Backends without a layer transform ignore it, and
    /// the surface simply does not pan.
    /// Run `body` in a floating surface anchored at `pos`, on the
    /// overlay layer — above all docked chrome.
    ///
    /// The sealed replacement for the backend's popup/menu machinery
    /// (PLAN.md WS-E1.1). Open/close state is the caller's, held in
    /// [`crate::popup::PopupState`]; this only places the surface.
    /// The default draws **nothing** — it cannot run `body` against
    /// itself and stay object-safe. Every real backend overrides it;
    /// the recording backend runs `body` inline so headless assertions
    /// still see overlay content.
    /// Run `body` in a fixed-size row laid out left-to-right, with
    /// items aligned on the cross axis (PLAN.md WS-A6/E1.4).
    ///
    /// The sealed equivalent of the backend's "allocate a sized region
    /// with a layout" call, which is how a node renderer builds pin
    /// rows: a fixed slot whose contents sit centred rather than
    /// hanging from the top edge.
    ///
    /// The default draws **nothing** — it cannot run `body` against
    /// itself and stay object-safe. Both real backends override it.
    /// Upload CPU pixels as a texture and return a handle to paint it.
    ///
    /// Surfaces that generate imagery each frame (noise previews,
    /// thumbnails, plots rendered to a buffer) need this without
    /// reaching for a backend context. The handle keeps the texture
    /// alive; drop it to release.
    ///
    /// The default returns `None` — a backend with no texture store
    /// cannot honour it, and callers must already handle that.
    fn load_texture(
        &mut self,
        name: &str,
        image: crate::vocab::ColorImage,
        options: crate::vocab::TextureOptions,
    ) -> Option<crate::vocab::TextureHandle> {
        let _ = (name, image, options);
        None
    }

    /// Run `body` inside a themed frame — fill, stroke, corner radius,
    /// inner margin and optional shadow — and report the rect it took.
    ///
    /// The sealed equivalent of the backend's frame widget, which is
    /// how a node renderer draws node bodies and headers. The frame
    /// paints *behind* `body`, so content is never occluded by its own
    /// background.
    ///
    /// The default draws no frame and runs nothing — it cannot pass
    /// itself to `body` and stay object-safe. Both real backends
    /// override it.
    fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> Rect {
        let _ = (spec, body);
        Rect::NOTHING
    }

    /// Multiply this surface's style metrics — text sizes, spacing,
    /// stroke widths — by `factor`.
    ///
    /// A zoomable surface that wants text to stay crisp when magnified
    /// renders at a larger style and scales the result down, rather
    /// than magnifying already-rasterised glyphs. Scaling a backend
    /// style is backend-specific, so it lives here rather than in the
    /// surface that asks for it.
    ///
    /// The default does nothing: a backend with no notion of a scalable
    /// style renders at its natural size, which is correct, just not
    /// crisper.
    fn scale_style(&mut self, factor: f32) {
        let _ = factor;
    }

    fn in_row(&mut self, size: Vec2, align: CrossAlign, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        let _ = (size, align, body);
    }

    fn overlay_at(&mut self, id: Id, pos: Pos2, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        let _ = (id, pos, body);
    }

    fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        let _ = transform;
    }

    fn child_at(&mut self, rect: Rect, body: &mut dyn FnMut(&mut dyn UiBackend));

    /// Move the flow cursor past `rect`, so subsequent `allocate` calls
    /// land below (or right of) it.
    fn advance_cursor_past(&mut self, rect: Rect);

    /// Grow this region's occupied bounds to include `rect`, so the
    /// parent sizes around content placed at an explicit position.
    fn expand_to_include(&mut self, rect: Rect);

    /// Bounds actually occupied so far — the union of everything
    /// allocated or expanded into. Starts empty, unlike
    /// [`UiBackend::available_rect`].
    fn occupied_rect(&self) -> Rect;

    /// Current flow cursor position.
    fn cursor(&self) -> Pos2;

    fn in_child(&mut self, id: Id, inset_left: f32, body: &mut dyn FnMut(&mut dyn UiBackend));

    /// Run `body` in a sub-scope that flows horizontally (left→right)
    /// when `horizontal`, else vertically. Afterwards the parent's
    /// layout continues below the scope. Same object-safe shape as
    /// [`UiBackend::in_child`].
    fn in_scope(&mut self, horizontal: bool, body: &mut dyn FnMut(&mut dyn UiBackend));

    /// A [`MaraPainter`](crate::mui::MaraPainter) drawing into the
    /// surface described by `spec` (the remaining region or an explicit
    /// clip). The default records into an internal command list (its
    /// output is discarded on non-rasterising backends); the egui
    /// backend returns a painter over its live `Ui`.
    fn make_painter(&self, spec: PaintSurfaceSpec) -> crate::mui::MaraPainter {
        let clip = match spec.region {
            PaintSurfaceRegion::ClipRect(rect) => rect,
            PaintSurfaceRegion::RemainingAvailable => self.available_rect(),
        };
        crate::mui::MaraPainter::recording(clip)
    }

    /// Current time in seconds from the host's frame clock, for
    /// animation timing. Defaults to `0.0` for clockless backends
    /// (a first slice of the Phase 5 host-services contract).
    fn now(&self) -> f64 {
        0.0
    }

    /// Text typed this frame (composed characters, not raw keys).
    ///
    /// Separate from [`UiBackend::input`] because [`crate::MaraInput`]
    /// is `Copy` and snapshotted every frame — a `String` there would
    /// cost an allocation per frame for every surface, typing or not.
    /// Only text-editing surfaces ask for this.
    fn text_typed(&self) -> String {
        String::new()
    }

    /// Device pixels per logical point. Surfaces that render into a
    /// pixel-sized target (embedded renderers, offscreen textures) size
    /// that target by `logical_size * pixels_per_point`. Backends with
    /// no display scaling report `1.0`.
    fn pixels_per_point(&self) -> f32 {
        1.0
    }

    /// Ask the host to schedule another frame (e.g. an animation is in
    /// flight). No-op on backends without an event loop.
    fn request_repaint(&self) {}

    /// Ask the host to schedule a frame no later than `after`. Surfaces
    /// waiting on off-thread work (tile decodes, streamed frames) use
    /// this to poll without burning a repaint every frame. No-op on
    /// backends without an event loop.
    fn request_repaint_after(&self, after: std::time::Duration) {
        let _ = after;
    }

    /// Run `body` inside a vertical scroll viewport described by
    /// `region`. The egui backend clips and offsets a real scroll area;
    /// the default just runs `body` as normal flow (no clipping), which
    /// is correct for headless/measuring backends. Same object-safe
    /// shape as [`UiBackend::in_child`].
    fn scroll_region(&mut self, region: ScrollRegion, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        let _ = region;
        self.in_scope(false, body);
    }

    /// Run `body` in a nested id scope salted by `salt`, so widget ids
    /// derived from [`UiBackend::id`] inside it are unique per scope
    /// (egui's `push_id`). Used by containers (e.g. pods) to keep the
    /// Nth slot's widgets from colliding with a same-labelled widget in
    /// a sibling. Object-safe like [`UiBackend::in_child`].
    fn in_id_scope(&mut self, salt: Id, body: &mut dyn FnMut(&mut dyn UiBackend));
}

/// Blanket impl so a `&mut` to any backend (notably `&mut dyn
/// UiBackend`, which `MaraUi` holds) is itself a [`UiBackend`]. Lets
/// widget `*_backend(&mut impl UiBackend, …)` functions accept the
/// backend `MaraUi` carries without every one becoming `?Sized`.
impl<T: UiBackend + ?Sized> UiBackend for &mut T {
    fn begin_area(&mut self, host: AreaHost, rect: Rect) {
        (**self).begin_area(host, rect)
    }
    fn allocate(&mut self, size: Vec2, sense: Sense) -> MaraResponse {
        (**self).allocate(size, sense)
    }
    fn reserve_space(&mut self, size: Vec2) -> Rect {
        (**self).reserve_space(size)
    }
    fn reserve_rect(&mut self, rect: Rect, sense: Sense) -> MaraResponse {
        (**self).reserve_rect(rect, sense)
    }
    fn interact(&mut self, rect: Rect, id: Id, sense: Sense) -> MaraResponse {
        (**self).interact(rect, id, sense)
    }
    fn available_rect(&self) -> Rect {
        (**self).available_rect()
    }
    fn id(&self) -> Id {
        (**self).id()
    }
    fn available_width(&self) -> f32 {
        (**self).available_width()
    }
    fn available_height(&self) -> f32 {
        (**self).available_height()
    }
    fn input(&self) -> MaraInput {
        (**self).input()
    }
    fn memory(&self) -> crate::memory::BackendMemory<'_> {
        (**self).memory()
    }
    fn add_space(&mut self, spec: SpaceSpec) {
        (**self).add_space(spec)
    }
    fn push_clip(&mut self, rect: Rect) {
        (**self).push_clip(rect)
    }
    fn pop_clip(&mut self) {
        (**self).pop_clip()
    }
    fn measure_text(&self, text: &str, size: f32, mono: bool) -> Vec2 {
        (**self).measure_text(text, size, mono)
    }
    fn paint(&mut self, cmd: PaintCmd) {
        (**self).paint(cmd)
    }
    fn reserve_paint_slot(&mut self) -> PaintSlot {
        (**self).reserve_paint_slot()
    }
    fn fill_paint_slot(&mut self, slot: PaintSlot, cmd: Option<PaintCmd>) {
        (**self).fill_paint_slot(slot, cmd)
    }
    fn hover_cursor(&mut self, response: &MaraResponse, cursor: CursorIcon) {
        (**self).hover_cursor(response, cursor)
    }
    fn hover_text(&mut self, response: &MaraResponse, text: &str) {
        (**self).hover_text(response, text)
    }
    fn is_rect_visible(&self, rect: Rect) -> bool {
        (**self).is_rect_visible(rect)
    }
    fn __internal_egui_ui_mut(&mut self) -> Option<&mut egui::Ui> {
        (**self).__internal_egui_ui_mut()
    }
    fn __internal_egui_ui_ref(&self) -> Option<&egui::Ui> {
        (**self).__internal_egui_ui_ref()
    }
    fn load_texture(
        &mut self,
        name: &str,
        image: crate::vocab::ColorImage,
        options: crate::vocab::TextureOptions,
    ) -> Option<crate::vocab::TextureHandle> {
        (**self).load_texture(name, image, options)
    }
    fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> Rect {
        (**self).framed(spec, body)
    }
    fn scale_style(&mut self, factor: f32) {
        (**self).scale_style(factor)
    }
    fn in_row(&mut self, size: Vec2, align: CrossAlign, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).in_row(size, align, body)
    }
    fn overlay_at(&mut self, id: Id, pos: Pos2, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).overlay_at(id, pos, body)
    }
    fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        (**self).set_layer_transform(transform)
    }
    fn child_at(&mut self, rect: Rect, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).child_at(rect, body)
    }
    fn advance_cursor_past(&mut self, rect: Rect) {
        (**self).advance_cursor_past(rect)
    }
    fn expand_to_include(&mut self, rect: Rect) {
        (**self).expand_to_include(rect)
    }
    fn occupied_rect(&self) -> Rect {
        (**self).occupied_rect()
    }
    fn cursor(&self) -> Pos2 {
        (**self).cursor()
    }
    fn in_child(&mut self, id: Id, inset_left: f32, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).in_child(id, inset_left, body)
    }
    fn in_scope(&mut self, horizontal: bool, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).in_scope(horizontal, body)
    }
    fn make_painter(&self, spec: PaintSurfaceSpec) -> crate::mui::MaraPainter {
        (**self).make_painter(spec)
    }
    fn now(&self) -> f64 {
        (**self).now()
    }
    fn text_typed(&self) -> String {
        (**self).text_typed()
    }
    fn pixels_per_point(&self) -> f32 {
        (**self).pixels_per_point()
    }
    fn request_repaint(&self) {
        (**self).request_repaint()
    }
    fn request_repaint_after(&self, after: std::time::Duration) {
        (**self).request_repaint_after(after)
    }
    fn scroll_region(&mut self, region: ScrollRegion, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).scroll_region(region, body)
    }
    fn in_id_scope(&mut self, salt: Id, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        (**self).in_id_scope(salt, body)
    }
}

/// Opaque handle to a paint slot reserved via
/// [`UiBackend::reserve_paint_slot`]. The inner index is
/// backend-interpreted; [`PaintSlot::INLINE`] is the sentinel the
/// default (non-deferring) implementation returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaintSlot(pub(crate) usize);

impl PaintSlot {
    pub(crate) const INLINE: PaintSlot = PaintSlot(usize::MAX);
}

/// Hidden egui measurement adapter for first-party crates that have
/// already expressed text measurement as Mara-owned data but still run
/// on the current egui backend.
#[doc(hidden)]
pub fn __internal_measure_text_egui(painter: &egui::Painter, spec: &TextMeasureSpec) -> Vec2 {
    crate::backend::egui::measure_text_for_spec(painter, spec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Color32, Pos2, Stroke};

    use crate::backend::record::RecordingBackend;

    #[test]
    fn backend_contract_allocates_and_records_paint_without_egui() {
        let mut backend = RecordingBackend::default();
        let area = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(100.0, 80.0));
        backend.begin_area(
            AreaHost::new(Id::new("area"), area.min, Layer::Background),
            area,
        );

        // Top-down flow: the first allocation sits at the region
        // origin, the next flows directly below it.
        let response = backend.allocate(Vec2::new(12.0, 8.0), Sense::ClickAndDrag);
        assert_eq!(
            response.rect,
            Rect::from_min_size(area.min, Vec2::new(12.0, 8.0))
        );

        let space = backend.reserve_space(Vec2::new(24.0, 16.0));
        assert_eq!(
            space,
            Rect::from_min_size(
                Pos2::new(area.min.x, area.min.y + 8.0),
                Vec2::new(24.0, 16.0)
            )
        );

        let reserved = backend.reserve_rect(
            Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(40.0, 50.0)),
            Sense::Hover,
        );
        assert_eq!(
            reserved.rect,
            Rect::from_min_size(Pos2::new(20.0, 30.0), Vec2::new(40.0, 50.0))
        );

        backend.paint(PaintCmd::Line {
            a: response.rect.min,
            b: response.rect.max,
            stroke: Stroke::new(1.0, Color32::WHITE),
        });

        assert_eq!(backend.paints.len(), 1);
    }

    #[test]
    fn area_host_carries_backend_neutral_identity_position_and_layer() {
        let host = AreaHost::new(Id::new("palette"), Pos2::new(10.0, 20.0), Layer::Overlay);

        assert_eq!(host.id, Id::new("palette"));
        assert_eq!(host.pos, Pos2::new(10.0, 20.0));
        assert_eq!(host.layer, Layer::Overlay);
        assert!(host.interactable);

        let passive = host.non_interactive();
        assert!(!passive.interactable);
    }

    #[test]
    fn layer_ranks_form_the_documented_total_order() {
        // Back-to-front: Background < Middle < Foreground < Overlay.
        let order = [
            Layer::Background,
            Layer::Middle,
            Layer::Foreground,
            Layer::Overlay,
        ];
        for (i, layer) in order.iter().enumerate() {
            assert_eq!(layer.rank() as usize, i, "rank must equal position");
        }
        // rank() agrees with the derived Ord, and every band is distinct.
        for pair in order.windows(2) {
            assert!(pair[0] < pair[1], "Ord must match documented order");
            assert!(pair[0].rank() < pair[1].rank());
        }
    }

    #[test]
    fn area_slot_spec_keeps_host_and_size_in_mara_data() {
        let host = AreaHost::new(Id::new("slot"), Pos2::new(12.0, 34.0), Layer::Foreground)
            .non_interactive();
        let spec = AreaSlotSpec::new(host, Vec2::new(56.0, 78.0));

        assert_eq!(spec.host, host);
        assert_eq!(spec.size, Vec2::new(56.0, 78.0));
    }

    #[test]
    fn canvas_specs_carry_backend_neutral_canvas_policy() {
        let slot = CanvasSlotSpec::new(Vec2::new(320.0, 180.0), Sense::ClickAndDrag);
        assert_eq!(slot.size, Vec2::new(320.0, 180.0));
        assert_eq!(slot.sense, Sense::ClickAndDrag);

        let rect = Rect::from_min_size(Pos2::new(4.0, 8.0), Vec2::new(64.0, 32.0));
        let absolute = CanvasRectSpec::new(Id::new("canvas-at"), rect, Sense::Drag);
        assert_eq!(absolute.id, Id::new("canvas-at"));
        assert_eq!(absolute.rect, rect);
        assert_eq!(absolute.sense, Sense::Drag);
    }

    #[test]
    fn paint_surface_spec_carries_backend_neutral_painter_policy() {
        assert_eq!(
            PaintSurfaceSpec::remaining_available().region,
            PaintSurfaceRegion::RemainingAvailable
        );

        let rect = Rect::from_min_size(Pos2::new(8.0, 12.0), Vec2::new(80.0, 24.0));
        assert_eq!(
            PaintSurfaceSpec::clipped(rect).region,
            PaintSurfaceRegion::ClipRect(rect)
        );
    }

    #[test]
    fn scroll_region_carries_backend_neutral_scroll_host_policy() {
        let region = ScrollRegion::new(Id::new("palette-list"), [false, true], 320.0, 1.0);

        assert_eq!(region.id, Id::new("palette-list"));
        assert_eq!(region.axis, ScrollAxis::Vertical);
        assert_eq!(region.auto_shrink, [false, true]);
        assert_eq!(region.max_extent, 320.0);
        assert_eq!(region.item_spacing, Vec2::new(0.0, 1.0));

        let horizontal =
            ScrollRegion::horizontal(Id::new("shelf-row"), [false, false], 512.0, Vec2::ZERO);
        assert_eq!(horizontal.axis, ScrollAxis::Horizontal);
        assert_eq!(horizontal.max_extent, 512.0);
    }

    #[test]
    fn child_region_carries_backend_neutral_child_layout_policy() {
        let rect = Rect::from_min_size(Pos2::new(4.0, 5.0), Vec2::new(100.0, 40.0));

        let row = ChildRegion::left_to_right(rect, StackAlign::Min);
        assert_eq!(row.rect, rect);
        assert_eq!(row.direction, StackDirection::LeftToRight);
        assert_eq!(row.align, StackAlign::Min);

        let centered_column = ChildRegion::top_down(rect, StackAlign::Center);
        assert_eq!(centered_column.direction, StackDirection::TopDown);
        assert_eq!(centered_column.align, StackAlign::Center);
    }

    #[test]
    fn stack_scope_spec_carries_backend_neutral_scope_direction() {
        assert_eq!(
            StackScopeSpec::horizontal().direction,
            StackDirection::LeftToRight
        );
        assert_eq!(
            StackScopeSpec::vertical().direction,
            StackDirection::TopDown
        );
    }

    #[test]
    fn container_body_spec_carries_backend_neutral_body_layout_policy() {
        let spec = ContainerBodySpec::new(false, 240.0, Some(512.0), 8.0);

        assert!(!spec.horizontal_strip);
        assert_eq!(spec.span_inner, 240.0);
        assert_eq!(spec.max_flow, Some(512.0));
        assert_eq!(spec.end_pad, 8.0);
    }

    #[test]
    fn pane_flex_spec_derives_title_size_without_backend_types() {
        let horizontal = PaneFlexSpec::new(true, 300.0, 25.0, 6.0);
        let vertical = PaneFlexSpec::new(false, 300.0, 25.0, 6.0);

        assert_eq!(horizontal.title_size(), Vec2::new(300.0, 25.0));
        assert_eq!(vertical.title_size(), Vec2::new(25.0, 300.0));
        assert_eq!(horizontal.body_gap, 6.0);
    }

    #[test]
    fn pane_body_scroll_spec_maps_strip_orientation_to_flow_axis() {
        let vertical = PaneBodyScrollSpec::new(Id::new("pane"), true, 280.0);
        let horizontal = PaneBodyScrollSpec::new(Id::new("pane"), false, 280.0);

        assert_eq!(vertical.axis, PaneBodyScrollAxis::FlowVertical);
        assert_eq!(horizontal.axis, PaneBodyScrollAxis::FlowHorizontal);
        assert!(vertical.horizontal_strip());
        assert!(!horizontal.horizontal_strip());
    }

    #[test]
    fn slot_ribbon_layout_spec_derives_rail_size_and_item_rects() {
        let vertical = SlotRibbonLayoutSpec::new(
            Id::new("slot-ribbon"),
            Pos2::new(4.0, 8.0),
            true,
            3,
            34.0,
            4.0,
        );

        assert_eq!(vertical.size, Vec2::new(34.0, 110.0));
        assert_eq!(
            vertical.item_rect(1),
            Some(Rect::from_min_size(
                Pos2::new(0.0, 38.0),
                Vec2::new(34.0, 34.0)
            ))
        );
        assert_eq!(vertical.item_rect(3), None);

        let horizontal = SlotRibbonLayoutSpec::new(
            Id::new("slot-ribbon"),
            Pos2::new(4.0, 8.0),
            false,
            2,
            34.0,
            4.0,
        );
        assert_eq!(horizontal.size, Vec2::new(72.0, 34.0));
        assert_eq!(
            horizontal.item_rect(1),
            Some(Rect::from_min_size(
                Pos2::new(38.0, 0.0),
                Vec2::new(34.0, 34.0)
            ))
        );

        // `item_screen_rect` offsets the local rect by the ribbon's
        // screen position — this is what interaction/paint must use, and
        // skipping it is what pinned every button to the window origin.
        assert_eq!(
            horizontal.item_screen_rect(0),
            Some(Rect::from_min_size(
                Pos2::new(4.0, 8.0),
                Vec2::new(34.0, 34.0)
            ))
        );
        assert_eq!(
            horizontal.item_screen_rect(1),
            Some(Rect::from_min_size(
                Pos2::new(42.0, 8.0),
                Vec2::new(34.0, 34.0)
            ))
        );
    }

    #[test]
    fn item_spacing_spec_carries_backend_neutral_stack_spacing_policy() {
        let spec = ItemSpacingSpec::new(Vec2::new(2.0, 3.0));

        assert_eq!(spec.item_spacing, Vec2::new(2.0, 3.0));
        assert_eq!(ItemSpacingSpec::zero().item_spacing, Vec2::ZERO);
    }

    #[test]
    fn popup_spec_carries_backend_neutral_popup_policy() {
        let spec = PopupSpec::new(PopupAlign::BottomStart, 2.0, 240.0, 2);

        assert_eq!(spec.align, PopupAlign::BottomStart);
        assert_eq!(spec.gap, 2.0);
        assert_eq!(spec.width, 240.0);
        assert_eq!(spec.inner_margin, 2);
    }

    #[test]
    fn popup_trigger_carries_backend_neutral_response_and_popup_ids() {
        let trigger = PopupTrigger::new(Id::new("response"), Id::new("popup"));

        assert_eq!(trigger.response_id, Id::new("response"));
        assert_eq!(trigger.popup_id, Id::new("popup"));
    }

    #[test]
    fn popup_list_spec_carries_backend_neutral_list_spacing() {
        let spec = PopupListSpec::new(Vec2::new(0.0, 1.0));

        assert_eq!(spec.item_spacing, Vec2::new(0.0, 1.0));
    }

    #[test]
    fn cursor_icon_is_backend_neutral_policy() {
        assert_eq!(CursorIcon::ResizeVertical, CursorIcon::ResizeVertical);
        assert_ne!(CursorIcon::ResizeVertical, CursorIcon::ResizeHorizontal);
    }

    #[test]
    fn text_edit_region_carries_backend_neutral_field_geometry() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 24.0));
        let text_rect = Rect::from_min_size(Pos2::new(18.0, 20.0), Vec2::new(184.0, 24.0));

        let region = TextEditRegion::new(rect, text_rect, 13.0);

        assert_eq!(region.rect, rect);
        assert_eq!(region.text_rect, text_rect);
        assert_eq!(region.font_size, 13.0);
        assert_eq!(region.desired_width(), 184.0);
    }

    #[test]
    fn text_edit_spec_carries_backend_neutral_singleline_policy() {
        let rect = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 24.0));
        let text_rect = Rect::from_min_size(Pos2::new(18.0, 20.0), Vec2::new(184.0, 24.0));
        let region = TextEditRegion::new(rect, text_rect, 13.0);

        let spec = TextEditSpec::singleline(
            region,
            "Type a command…",
            Color32::WHITE,
            Color32::from_black_alpha(160),
        );

        assert_eq!(spec.region, region);
        assert_eq!(spec.hint, "Type a command…");
        assert_eq!(spec.text_color, Color32::WHITE);
        assert_eq!(spec.hint_color, Color32::from_black_alpha(160));
        assert_eq!(spec.background_color, Color32::TRANSPARENT);
        assert!(!spec.frame);
    }

    #[test]
    fn text_measure_spec_carries_backend_neutral_font_policy() {
        let spec = TextMeasureSpec::new("Road 42", 11.0, false);
        assert_eq!(spec.text, "Road 42");
        assert_eq!(spec.size, 11.0);
        assert!(!spec.mono);
    }

    #[test]
    fn inline_picker_spec_carries_backend_neutral_picker_host_policy() {
        let spec = InlinePickerSpec::new(240.0, 28.0);

        assert_eq!(spec.slider_width, 240.0);
        assert_eq!(spec.clip_expand, 28.0);
    }

    #[test]
    fn color_picker_alpha_is_backend_neutral_policy() {
        assert_eq!(ColorPickerAlpha::Opaque, ColorPickerAlpha::Opaque);
        assert_ne!(ColorPickerAlpha::Opaque, ColorPickerAlpha::OnlyBlend);
    }
}
