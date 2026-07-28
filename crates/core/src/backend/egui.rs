//! Egui backend adapter.

use egui::{FontId, layers::ShapeIdx};

use crate::{
    layout::{
        AreaHost, AreaSlotSpec, ChildRegion, ContainerBodySpec, CursorIcon, FrameHostSpec,
        InlinePickerSpec, ItemSpacingSpec, Layer, PaintSurfaceRegion, PaintSurfaceSpec,
        PaneBodyScrollAxis, PaneBodyScrollSpec, PaneFlexSpec, PopupAlign, PopupListSpec, PopupSpec,
        PopupTrigger, ScrollAxis, ScrollRegion, Sense, SpaceSpec, StackAlign, StackDirection,
        TextEditRegion, TextEditSpec, TextMeasureSpec, UiBackend,
    },
    memory::MaraMemoryCtx,
    mui::{MaraInput, MaraKey, MaraResponse},
    paint::{PaintCmd, TextFamily, TextRun},
    vocab,
};

#[doc(hidden)]
pub fn egui_frame_for_style_spec(spec: crate::style::FrameSpec) -> egui::Frame {
    let shadow = spec
        .shadow
        .map(|shadow| egui::epaint::Shadow {
            offset: shadow.offset,
            blur: shadow.blur,
            spread: shadow.spread,
            color: shadow.color.into(),
        })
        .unwrap_or(egui::epaint::Shadow::NONE);

    egui::Frame::new()
        .fill(spec.fill.into())
        .stroke(spec.stroke)
        .corner_radius(spec.corner)
        .inner_margin(egui::Margin::from(spec.inner_margin))
        .outer_margin(egui::Margin::from(spec.outer_margin))
        .shadow(shadow)
}

#[doc(hidden)]
pub struct EguiUiBackend<'a> {
    ui: &'a mut egui::Ui,
    clip_stack: Vec<egui::Rect>,
    /// The frame context, owned so [`UiBackend::ctx`] can hand out a
    /// reference to it.
    ///
    /// Owned rather than borrowed because `ui` is already held
    /// mutably — a `&egui::Context` derived from it would alias. The
    /// clone is an `Arc` bump: `egui::Context` is a handle, not the
    /// context itself.
    ctx: EguiCtx,
}

/// The backend context, wrapped so Mara's traits can be implemented on
/// it.
///
/// `impl MaraCtx for egui::Context` is legal only while the trait and
/// the type share a crate. Once `backend/` moves out, both are foreign
/// there and the orphan rule forbids it. A local newtype is the shape
/// that survives the split, and it owns its handle so a surface can
/// lend a reference to one.
#[derive(Clone)]
pub struct EguiCtx(pub egui::Context);

/// Deref to the backend context so the trait impls below can call
/// egui's own methods directly.
///
/// Note this shadows same-named trait methods at call sites — reach
/// `MaraCtx::input(&seam)` explicitly rather than `seam.input()`, which
/// resolves to egui's.
impl std::ops::Deref for EguiCtx {
    type Target = egui::Context;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl EguiCtx {
    /// Wrap a borrowed context, cloning the handle.
    #[doc(hidden)]
    #[must_use]
    pub fn new(ctx: &egui::Context) -> Self {
        Self(ctx.clone())
    }
}

impl<'a> EguiUiBackend<'a> {
    #[doc(hidden)]
    pub fn new(ui: &'a mut egui::Ui) -> Self {
        let ctx = EguiCtx::new(ui.ctx());
        Self {
            ui,
            clip_stack: Vec::new(),
            ctx,
        }
    }
}

#[allow(dead_code)]
pub(crate) fn egui_rich_text_for_style_spec(spec: crate::style::TextSpec) -> egui::RichText {
    let mut text = egui::RichText::new(spec.text);
    if spec.strong {
        text = text.strong();
    }
    if spec.small {
        text = text.small();
    }
    if spec.italics {
        text = text.italics();
    }
    if let Some(size) = spec.size {
        text = text.size(size);
    }
    if let Some(color) = spec.color {
        text = text.color(color);
    }
    if spec.extra_letter_spacing > 0.0 {
        text = text.extra_letter_spacing(spec.extra_letter_spacing);
    }
    text
}

pub(crate) fn egui_font_family_for_text_family(family: TextFamily) -> egui::FontFamily {
    match family {
        TextFamily::Proportional => egui::FontFamily::Proportional,
        TextFamily::Monospace => egui::FontFamily::Monospace,
        TextFamily::Named(name) => egui::FontFamily::Name(name.into()),
    }
}

pub(crate) fn available_text_family_for_ui(ui: &egui::Ui, family: TextFamily) -> TextFamily {
    let backend_family = egui_font_family_for_text_family(family.clone());
    if ui.fonts(|fonts| fonts.families().contains(&backend_family)) {
        family
    } else {
        TextFamily::Proportional
    }
}

pub(crate) fn measure_text_for_spec(
    painter: &egui::Painter,
    spec: &TextMeasureSpec,
) -> vocab::Vec2 {
    let font = if spec.mono {
        FontId::monospace(spec.size)
    } else {
        FontId::proportional(spec.size)
    };
    painter
        .layout_no_wrap(spec.text.clone(), font, egui::Color32::WHITE)
        .size()
        .into()
}

pub(crate) fn measure_text_runs_for_painter(
    painter: &egui::Painter,
    runs: &[TextRun],
) -> vocab::Vec2 {
    let Some((job, _fallback_color)) = layout_job_for_text_runs(painter, runs) else {
        return vocab::Vec2::ZERO;
    };
    painter.layout_job(job).size().into()
}

impl UiBackend for EguiUiBackend<'_> {
    fn begin_area(&mut self, _host: AreaHost, rect: vocab::Rect) {
        let rect: egui::Rect = rect.into();
        self.ui.set_clip_rect(rect);
        self.ui.set_min_size(rect.size());
        self.ui.set_max_size(rect.size());
    }

    fn allocate(&mut self, size: vocab::Vec2, sense: Sense) -> MaraResponse {
        let (_rect, response) = self.ui.allocate_exact_size(size.into(), egui_sense(sense));
        let mara: MaraResponse = mara_response_from(&response);
        probe_record_response(self.ui.ctx(), "alloc", None, &mara);
        mara
    }

    fn reserve_space(&mut self, size: vocab::Vec2) -> vocab::Rect {
        self.ui.allocate_space(size.into()).1.into()
    }

    fn reserve_rect(&mut self, rect: vocab::Rect, sense: Sense) -> MaraResponse {
        let mara: MaraResponse =
            mara_response_from(&self.ui.allocate_rect(rect.into(), egui_sense(sense)));
        probe_record_response(self.ui.ctx(), "reserve", None, &mara);
        mara
    }

    fn interact(&mut self, rect: vocab::Rect, id: vocab::Id, sense: Sense) -> MaraResponse {
        let mara: MaraResponse =
            mara_response_from(&self.ui.interact(rect.into(), id.into(), egui_sense(sense)));
        probe_record_response(self.ui.ctx(), "interact", Some(id), &mara);
        mara
    }

    fn available_rect(&self) -> vocab::Rect {
        self.ui.available_rect_before_wrap().into()
    }

    fn id(&self) -> vocab::Id {
        ui_id(self.ui)
    }

    fn available_width(&self) -> f32 {
        ui_available_width(self.ui)
    }

    fn available_height(&self) -> f32 {
        ui_available_height(self.ui)
    }

    fn ctx(&self) -> &dyn crate::context::MaraCtx {
        &self.ctx
    }

    fn opacity(&self) -> f32 {
        self.ui.opacity()
    }

    fn in_region(
        &mut self,
        region: crate::layout::ChildRegion,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) {
        let mut child = child_ui_for_region(self.ui, region);
        let mut inner = EguiUiBackend::new(&mut child);
        body(&mut inner);
    }

    fn min_rect(&self) -> vocab::Rect {
        self.ui.min_rect().into()
    }

    fn stack_direction(&self) -> StackDirection {
        stack_direction_for_ui(self.ui)
    }

    fn set_item_spacing(&mut self, spec: ItemSpacingSpec) {
        apply_item_spacing_spec(self.ui, spec);
    }

    fn reserve_title_slot(&mut self, spec: PaneFlexSpec) -> vocab::Rect {
        reserve_pane_title_slot(self.ui, spec)
    }

    fn apply_flex_spec(&mut self, spec: PaneFlexSpec) {
        apply_pane_flex_spec(self.ui, spec);
    }

    fn pane_body_slot(
        &mut self,
        spec: PaneBodyScrollSpec,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) {
        show_pane_body_scroll_slot(self.ui, spec, |ui| {
            let mut inner = EguiUiBackend::new(ui);
            body(&mut inner);
        });
    }

    fn set_opacity(&mut self, opacity: f32) {
        self.ui.set_opacity(opacity);
    }

    fn multiply_opacity(&mut self, factor: f32) {
        self.ui.multiply_opacity(factor);
    }

    fn body_slot(
        &mut self,
        spec: ContainerBodySpec,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) -> f32 {
        let (_, height) = show_container_body_slot(self.ui, spec, |ui| {
            let mut inner = EguiUiBackend::new(ui);
            body(&mut inner);
        });
        height
    }

    fn paint_on_z_layer(
        &mut self,
        id: vocab::Id,
        tier: u16,
        rect: vocab::Rect,
        opacity: f32,
        cmd: PaintCmd,
    ) {
        render_paint_cmd_on_z_layer(self.ui, id, tier, rect, opacity, cmd);
    }

    fn available_text_family(&self, family: TextFamily) -> TextFamily {
        available_text_family_for_ui(self.ui, family)
    }

    fn memory(&self) -> crate::memory::BackendMemory<'_> {
        crate::memory::BackendMemory::Egui(MaraMemoryCtx::new(&self.ctx))
    }

    fn input(&self) -> MaraInput {
        input_snapshot_for_ui(self.ui)
    }

    fn add_space(&mut self, spec: SpaceSpec) {
        add_space_for_spec(self.ui, spec);
    }

    fn push_clip(&mut self, rect: vocab::Rect) {
        self.clip_stack.push(self.ui.clip_rect());
        let rect: egui::Rect = rect.into();
        self.ui.set_clip_rect(rect.intersect(self.ui.clip_rect()));
    }

    fn pop_clip(&mut self) {
        if let Some(rect) = self.clip_stack.pop() {
            self.ui.set_clip_rect(rect);
        }
    }

    fn measure_text(&self, text: &str, size: f32, mono: bool) -> vocab::Vec2 {
        measure_text_for_spec(self.ui.painter(), &TextMeasureSpec::new(text, size, mono))
    }

    fn paint(&mut self, cmd: PaintCmd) {
        render_paint_cmd_ui(self.ui, cmd);
    }

    fn reserve_paint_slot(&mut self) -> crate::layout::PaintSlot {
        // The slot carries the layer's own shape index, not a position
        // in some per-wrapper table. A caller may reserve through one
        // short-lived `EguiUiBackend` and fill through another over the
        // same `Ui` — an indirection local to the wrapper would lose
        // the slot silently, painting nothing.
        crate::layout::PaintSlot(self.ui.painter().add(egui::Shape::Noop).0)
    }

    fn fill_paint_slot(&mut self, slot: crate::layout::PaintSlot, cmd: Option<PaintCmd>) {
        let shape = cmd.map(shape_from_paint_cmd).unwrap_or(egui::Shape::Noop);
        self.ui.painter().set(ShapeIdx(slot.0), shape);
    }

    fn inline_picker_scope(
        &mut self,
        spec: crate::layout::InlinePickerSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        show_inline_picker_scope(self.ui, spec, |ui| {
            let mut child = EguiUiBackend::new(ui);
            body(&mut child);
        });
    }

    fn constrain_to(&mut self, rect: vocab::Rect) {
        constrain_ui_to_rect(self.ui, rect);
    }

    fn set_cursor_icon(&mut self, cursor: crate::layout::CursorIcon) {
        set_cursor_icon_for_ui(self.ui, cursor);
    }

    fn hover_cursor(&mut self, response: &MaraResponse, cursor: crate::layout::CursorIcon) {
        hover_cursor_for_ui_response(self.ui, response, cursor);
    }

    fn hover_text(&mut self, response: &MaraResponse, text: &str) {
        hover_text(self.ui.ctx(), response.backend_response_id(), text);
    }

    fn is_rect_visible(&self, rect: vocab::Rect) -> bool {
        self.ui.is_rect_visible(rect.into())
    }

    fn __internal_egui_ui_mut(&mut self) -> Option<&mut egui::Ui> {
        Some(self.ui)
    }

    fn load_texture(
        &mut self,
        name: &str,
        image: vocab::ColorImage,
        options: vocab::TextureOptions,
    ) -> Option<vocab::TextureHandle> {
        let image: egui::ColorImage = image.into();
        Some(
            self.ui
                .ctx()
                .load_texture(name, image, options.into())
                .into(),
        )
    }

    fn scale_style(&mut self, factor: f32) {
        use egui_scale::EguiScale;
        self.ui.style_mut().scale(factor);
    }

    fn framed(
        &mut self,
        spec: crate::style::FrameSpec,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) -> vocab::Rect {
        let frame = egui_frame_for_style_spec(spec);
        frame
            .show(self.ui, |inner| {
                let mut child = EguiUiBackend::new(inner);
                body(&mut child);
            })
            .response
            .rect
            .into()
    }

    fn in_row(
        &mut self,
        size: vocab::Vec2,
        align: crate::layout::CrossAlign,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        let align = match align {
            crate::layout::CrossAlign::Start => egui::Align::Min,
            crate::layout::CrossAlign::Center => egui::Align::Center,
            crate::layout::CrossAlign::End => egui::Align::Max,
        };
        self.ui.allocate_ui_with_layout(
            size.into(),
            egui::Layout::left_to_right(align),
            |child_ui| {
                let mut child = EguiUiBackend::new(child_ui);
                body(&mut child);
            },
        );
    }

    fn overlay_at(
        &mut self,
        id: vocab::Id,
        pos: vocab::Pos2,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        let ctx = self.ui.ctx().clone();
        show_area_for_host(&ctx, AreaHost::new(id, pos, Layer::Overlay), |ui| {
            let mut backend = EguiUiBackend::new(ui);
            body(&mut backend);
        });
    }

    fn set_layer_transform(&mut self, transform: crate::transform::Transform) {
        self.ui.ctx().set_transform_layer(
            self.ui.layer_id(),
            egui::emath::TSTransform {
                scaling: transform.scaling,
                translation: transform.translation.into(),
            },
        );
    }

    fn child_at(&mut self, rect: vocab::Rect, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        let rect: egui::Rect = rect.into();
        let layout = *self.ui.layout();
        let mut child = self
            .ui
            .new_child(egui::UiBuilder::new().max_rect(rect).layout(layout));
        let mut backend = EguiUiBackend::new(&mut child);
        body(&mut backend);
    }

    fn advance_cursor_past(&mut self, rect: vocab::Rect) {
        self.ui.advance_cursor_after_rect(rect.into());
    }

    fn expand_to_include(&mut self, rect: vocab::Rect) {
        self.ui.expand_to_include_rect(rect.into());
    }

    fn occupied_rect(&self) -> vocab::Rect {
        self.ui.min_rect().into()
    }

    fn cursor(&self) -> vocab::Pos2 {
        self.ui.next_widget_position().into()
    }

    fn in_child(
        &mut self,
        id: vocab::Id,
        _inset_left: f32,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        // egui applies its own theme indent spacing for the visual;
        // `inset_left` is honoured by headless backends.
        self.ui.indent(Into::<egui::Id>::into(id), |child_ui| {
            let mut child = EguiUiBackend::new(child_ui);
            body(&mut child);
        });
    }

    fn text_input(
        &mut self,
        text: &mut String,
        placeholder: &str,
        height: f32,
        accent: vocab::Color32,
    ) -> MaraResponse {
        crate::widget::text_input::text_input_h(self.ui, text, placeholder, accent, height)
    }

    fn dropdown(
        &mut self,
        id_salt: vocab::Id,
        selected: &mut usize,
        options: &[&str],
        accent: vocab::Color32,
    ) -> MaraResponse {
        crate::widget::dropdown::dropdown(self.ui, id_salt, selected, options, accent)
    }

    fn text_edit_at(
        &mut self,
        text: &mut String,
        spec: TextEditSpec,
        focus_when_unfocused: bool,
    ) -> MaraResponse {
        show_text_edit_with_focus_policy(self.ui, text, spec, focus_when_unfocused)
    }

    fn frame_host(
        &mut self,
        spec: FrameHostSpec,
        body: &mut dyn FnMut(&mut dyn crate::layout::UiBackend),
    ) -> vocab::Rect {
        show_frame_for_spec(self.ui, spec, |ui| {
            let mut inner = EguiUiBackend::new(ui);
            body(&mut inner);
        })
        .response
        .rect
        .into()
    }

    fn context_menu(
        &mut self,
        response: &MaraResponse,
        accent: vocab::Color32,
        body: &mut dyn FnMut(&mut dyn UiBackend),
    ) {
        with_response_for_ui(self.ui, response, |raw| {
            crate::widget::context_menu::context_menu_mara(raw, accent, |ui| {
                let mut inner = EguiUiBackend::new(ui);
                body(&mut inner);
            });
        });
    }

    fn in_wrapped_row(&mut self, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        self.ui.horizontal_wrapped(|child_ui| {
            let mut child = EguiUiBackend::new(child_ui);
            body(&mut child);
        });
    }

    fn in_scope(&mut self, horizontal: bool, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        let run = |child_ui: &mut egui::Ui| {
            let mut child = EguiUiBackend::new(child_ui);
            body(&mut child);
        };
        if horizontal {
            self.ui.horizontal(run);
        } else {
            self.ui.vertical(run);
        }
    }

    fn make_painter(&self, spec: PaintSurfaceSpec) -> crate::mui::MaraPainter {
        crate::mui::MaraPainter::from_sink(Box::new(EguiSink(painter_for_ui_surface(
            self.ui, spec,
        ))))
    }

    fn now(&self) -> f64 {
        input_time(self.ui.ctx())
    }

    fn text_typed(&self) -> String {
        self.ui.ctx().input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Text(text) => Some(text.as_str()),
                    _ => None,
                })
                .collect()
        })
    }

    fn pixels_per_point(&self) -> f32 {
        self.ui.ctx().pixels_per_point()
    }

    fn request_repaint(&self) {
        self.ui.ctx().request_repaint();
    }

    fn request_repaint_after(&self, after: std::time::Duration) {
        self.ui.ctx().request_repaint_after(after);
    }

    fn scroll_region(&mut self, region: ScrollRegion, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        show_vertical_scroll_region(self.ui, region, |ui| {
            let mut child = EguiUiBackend::new(ui);
            body(&mut child);
        });
    }

    fn in_id_scope(&mut self, salt: vocab::Id, body: &mut dyn FnMut(&mut dyn UiBackend)) {
        self.ui.push_id(Into::<egui::Id>::into(salt), |ui| {
            let mut child = EguiUiBackend::new(ui);
            body(&mut child);
        });
    }
}

pub(crate) fn egui_sense(sense: Sense) -> egui::Sense {
    match sense {
        Sense::Hover => egui::Sense::hover(),
        Sense::Click => egui::Sense::click(),
        Sense::Drag => egui::Sense::drag(),
        Sense::ClickAndDrag => egui::Sense::click_and_drag(),
    }
}

pub(crate) fn egui_cursor_icon(cursor: CursorIcon) -> egui::CursorIcon {
    match cursor {
        CursorIcon::PointingHand => egui::CursorIcon::PointingHand,
        CursorIcon::Grabbing => egui::CursorIcon::Grabbing,
        CursorIcon::ResizeHorizontal => egui::CursorIcon::ResizeHorizontal,
        CursorIcon::ResizeVertical => egui::CursorIcon::ResizeVertical,
    }
}

pub(crate) fn egui_key(key: MaraKey) -> egui::Key {
    match key {
        MaraKey::Escape => egui::Key::Escape,
        MaraKey::ArrowDown => egui::Key::ArrowDown,
        MaraKey::ArrowUp => egui::Key::ArrowUp,
        MaraKey::ArrowLeft => egui::Key::ArrowLeft,
        MaraKey::ArrowRight => egui::Key::ArrowRight,
        MaraKey::Enter => egui::Key::Enter,
        MaraKey::Tab => egui::Key::Tab,
        MaraKey::Space => egui::Key::Space,
        MaraKey::Backspace => egui::Key::Backspace,
        MaraKey::Delete => egui::Key::Delete,
        MaraKey::Insert => egui::Key::Insert,
        MaraKey::Home => egui::Key::Home,
        MaraKey::End => egui::Key::End,
        MaraKey::PageUp => egui::Key::PageUp,
        MaraKey::PageDown => egui::Key::PageDown,
        MaraKey::Minus => egui::Key::Minus,
        MaraKey::Plus => egui::Key::Plus,
        MaraKey::Equals => egui::Key::Equals,
        MaraKey::A => egui::Key::A,
        MaraKey::B => egui::Key::B,
        MaraKey::C => egui::Key::C,
        MaraKey::D => egui::Key::D,
        MaraKey::E => egui::Key::E,
        MaraKey::F => egui::Key::F,
        MaraKey::G => egui::Key::G,
        MaraKey::H => egui::Key::H,
        MaraKey::I => egui::Key::I,
        MaraKey::J => egui::Key::J,
        MaraKey::K => egui::Key::K,
        MaraKey::L => egui::Key::L,
        MaraKey::M => egui::Key::M,
        MaraKey::N => egui::Key::N,
        MaraKey::O => egui::Key::O,
        MaraKey::P => egui::Key::P,
        MaraKey::Q => egui::Key::Q,
        MaraKey::R => egui::Key::R,
        MaraKey::S => egui::Key::S,
        MaraKey::T => egui::Key::T,
        MaraKey::U => egui::Key::U,
        MaraKey::V => egui::Key::V,
        MaraKey::W => egui::Key::W,
        MaraKey::X => egui::Key::X,
        MaraKey::Y => egui::Key::Y,
        MaraKey::Z => egui::Key::Z,
        MaraKey::Num0 => egui::Key::Num0,
        MaraKey::Num1 => egui::Key::Num1,
        MaraKey::Num2 => egui::Key::Num2,
        MaraKey::Num3 => egui::Key::Num3,
        MaraKey::Num4 => egui::Key::Num4,
        MaraKey::Num5 => egui::Key::Num5,
        MaraKey::Num6 => egui::Key::Num6,
        MaraKey::Num7 => egui::Key::Num7,
        MaraKey::Num8 => egui::Key::Num8,
        MaraKey::Num9 => egui::Key::Num9,
        MaraKey::F1 => egui::Key::F1,
        MaraKey::F2 => egui::Key::F2,
        MaraKey::F3 => egui::Key::F3,
        MaraKey::F4 => egui::Key::F4,
        MaraKey::F5 => egui::Key::F5,
        MaraKey::F6 => egui::Key::F6,
        MaraKey::F7 => egui::Key::F7,
        MaraKey::F8 => egui::Key::F8,
        MaraKey::F9 => egui::Key::F9,
        MaraKey::F10 => egui::Key::F10,
        MaraKey::F11 => egui::Key::F11,
        MaraKey::F12 => egui::Key::F12,
    }
}

pub(crate) fn egui_pointer_button(button: vocab::PointerButton) -> egui::PointerButton {
    match button {
        vocab::PointerButton::Primary => egui::PointerButton::Primary,
        vocab::PointerButton::Secondary => egui::PointerButton::Secondary,
        vocab::PointerButton::Middle => egui::PointerButton::Middle,
    }
}

pub(crate) fn consume_key(ctx: &egui::Context, key: MaraKey) -> bool {
    ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui_key(key)))
}

// ─── Layout pose probe ──────────────────────────────────────────────
//
// Backend-side storage + recording for `crate::probe`. A shared log is
// stashed in ctx temp data while enabled; the `UiBackend` allocation /
// interaction seams and area hosts record into it so a host can dump the
// whole frame's layout (see `crate::probe::format`).

type PoseLog = std::sync::Arc<std::sync::Mutex<Vec<crate::probe::ElementPose>>>;

/// Lock-free fast path: when the probe is disabled (the normal case),
/// `probe_record_response` is called from every allocate/interact, so it
/// must not touch the egui data lock. This atomic gates that.
static PROBE_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn pose_log_key() -> egui::Id {
    egui::Id::new("mara.probe.pose_log")
}

/// Install a fresh empty pose log (enable) or remove it (disable).
/// Call once per frame before running the UI to capture that frame.
pub(crate) fn probe_set_enabled(ctx: &egui::Context, on: bool) {
    PROBE_ENABLED.store(on, std::sync::atomic::Ordering::Relaxed);
    ctx.data_mut(|d| {
        if on {
            d.insert_temp::<PoseLog>(pose_log_key(), PoseLog::default());
        } else {
            d.remove::<PoseLog>(pose_log_key());
        }
    });
}

pub(crate) fn probe_enabled(ctx: &egui::Context) -> bool {
    PROBE_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
        && ctx.data(|d| d.get_temp::<PoseLog>(pose_log_key()).is_some())
}

pub(crate) fn probe_record(ctx: &egui::Context, pose: crate::probe::ElementPose) {
    if let Some(log) = ctx.data(|d| d.get_temp::<PoseLog>(pose_log_key()))
        && let Ok(mut v) = log.lock()
    {
        v.push(pose);
    }
}

pub(crate) fn probe_drain(ctx: &egui::Context) -> Vec<crate::probe::ElementPose> {
    ctx.data(|d| d.get_temp::<PoseLog>(pose_log_key()))
        .map(|log| {
            log.lock()
                .map(|mut v| std::mem::take(&mut *v))
                .unwrap_or_default()
        })
        .unwrap_or_default()
}

/// Record a response-producing element (allocation / interaction).
fn probe_record_response(
    ctx: &egui::Context,
    kind: &'static str,
    id: Option<vocab::Id>,
    resp: &MaraResponse,
) {
    if !probe_enabled(ctx) {
        return;
    }
    let mut pose =
        crate::probe::ElementPose::new(kind, resp.rect).interactive(resp.hovered, resp.clicked);
    if let Some(id) = id {
        pose = pose.with_id(id);
    }
    probe_record(ctx, pose);
}

pub(crate) fn context_content_rect(ctx: &egui::Context) -> vocab::Rect {
    ctx.content_rect().into()
}

pub(crate) fn input_time(ctx: &egui::Context) -> f64 {
    ctx.input(|input| input.time)
}

pub(crate) fn request_repaint(ctx: &egui::Context) {
    ctx.request_repaint();
}

pub(crate) fn request_repaint_after(ctx: &egui::Context, after: std::time::Duration) {
    ctx.request_repaint_after(after);
}

pub(crate) fn context_pixels_per_point(ctx: &egui::Context) -> f32 {
    ctx.pixels_per_point()
}

pub(crate) fn unstable_dt(ctx: &egui::Context) -> f32 {
    ctx.input(|input| input.unstable_dt).max(0.0)
}

pub(crate) fn viewport_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().maximized)
        .unwrap_or(false)
}

#[doc(hidden)]
pub fn color32_for_backend(color: vocab::Color32) -> egui::Color32 {
    color.into()
}

pub(crate) fn egui_order_for_layer(layer: Layer) -> egui::Order {
    match layer {
        Layer::Background => egui::Order::Background,
        Layer::Middle => egui::Order::Middle,
        Layer::Foreground => egui::Order::Foreground,
        // Mara overlays are top transient UI. In the egui backend
        // that maps to Tooltip order so floating palettes stay above
        // foreground scrims.
        Layer::Overlay => egui::Order::Tooltip,
    }
}

pub(crate) fn area_for_host(host: AreaHost) -> egui::Area {
    let area = egui::Area::new(host.id.into())
        .order(egui_order_for_layer(host.layer))
        .fixed_pos(Into::<egui::Pos2>::into(host.pos))
        .interactable(host.interactable)
        .movable(host.movable)
        .fade_in(host.fade_in);
    match host.default_size {
        Some(size) => area.default_size(Into::<egui::Vec2>::into(size)),
        None => area,
    }
}

pub(crate) fn show_area_for_host<R>(
    ctx: &egui::Context,
    host: AreaHost,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let host_id = host.id;
    let host_layer = host.layer;
    let raise = host.bring_to_top;
    let inner = area_for_host(host).show(ctx, body);
    if raise {
        ctx.move_to_top(inner.response.layer_id);
    }
    if probe_enabled(ctx) {
        probe_record(
            ctx,
            crate::probe::ElementPose::new("area", inner.response.rect.into())
                .with_id(host_id)
                .with_label(format!("{host_layer:?}")),
        );
    }
    inner
}

/// A painter whose layer is a REGISTERED egui `Area` (empty,
/// non-interactive) rather than a free `layer_painter` layer. Within one
/// egui order, registered areas composite in registration order and free
/// layers composite after ALL registered areas — so a view backdrop
/// painted on a free layer would cover every Background-order pane. An
/// area-registered painter keeps its registration slot (first-seen
/// order), letting later-opened areas (panes) stack above it.
/// Rasterising sink over the backend's painter.
/// A [`MaraPainter`](crate::mui::MaraPainter) over a backend painter.
///
/// First-party hook for incremental renderer ports, where a converted
/// leaf draws through `MaraPainter` while its caller still holds the
/// backend's own painter.
#[doc(hidden)]
#[must_use]
pub fn __internal_painter_from_egui(painter: egui::Painter) -> crate::mui::MaraPainter {
    crate::mui::MaraPainter::from_sink(Box::new(EguiSink(painter)))
}

/// An owned backend handle over `ui`, for host plugins that own the
/// egui pass and lend the surface to `MaraUi::__internal_over`.
#[doc(hidden)]
#[must_use]
pub fn __internal_backend_from_raw(ui: &mut egui::Ui) -> crate::mui::MaraRawBackend<'_> {
    crate::mui::MaraRawBackend::__internal_from_boxed(Box::new(EguiUiBackend::new(ui)))
}

/// Paint the deepest tag from this frame and clear the slot. Call
/// once at the END of the top-level UI callback. No-op when the
/// inspector is off, or when no tag captured the cursor this frame.
pub fn paint(ctx: &egui::Context) {
    let seam = crate::backend::egui::EguiCtx::new(ctx);
    if !crate::debug::is_enabled(&seam) {
        return;
    }
    let mut memory = crate::context::MaraCtx::memory(&seam);
    let best: Option<crate::debug::Best> =
        memory.get_temp::<crate::debug::Best>(crate::debug::best_id());
    memory.remove_temp::<crate::debug::Best>(crate::debug::best_id());
    let Some(best) = best else {
        return;
    };
    let p = ctx.debug_painter();
    let outline = egui::Color32::from_rgb(255, 80, 80);
    p.rect_stroke(
        best.rect,
        0.0,
        egui::Stroke::new(2.0, outline),
        egui::StrokeKind::Inside,
    );

    // Label chip — placed OUTSIDE the highlighted rect so it
    // doesn't cover the widget's actual content (text input,
    // title text, etc.). Default position is just above the rect's
    // top edge; if the rect is near the top of the viewport and
    // there's no room above, fall through to just below the rect's
    // bottom edge.
    let font = egui::FontId::monospace(11.0);
    let galley = p.layout_no_wrap(best.label.clone(), font, egui::Color32::WHITE);
    let pad = egui::vec2(5.0, 2.0);
    let chip_size = galley.size() + pad * 2.0;
    let viewport = ctx.content_rect();
    let above_y = best.rect.min.y - chip_size.y - 4.0;
    let below_y = best.rect.max.y + 4.0;
    let chip_top_y = if above_y >= viewport.min.y + 2.0 {
        above_y
    } else {
        below_y
    };
    let chip_origin = egui::pos2(best.rect.min.x, chip_top_y);
    let chip_rect = egui::Rect::from_min_size(chip_origin, chip_size);
    p.rect_filled(
        chip_rect,
        2.0,
        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 220),
    );
    p.rect_stroke(
        chip_rect,
        2.0,
        egui::Stroke::new(1.0, outline),
        egui::StrokeKind::Inside,
    );
    p.galley(chip_origin + pad, galley, egui::Color32::WHITE);
}

#[doc(hidden)]
pub struct EguiSink(#[doc(hidden)] pub egui::Painter);

impl crate::mui::PainterSink for EguiSink {
    fn boxed_clone(&self) -> Box<dyn crate::mui::PainterSink> {
        Box::new(Self(self.0.clone()))
    }

    fn clip_rect(&self) -> vocab::Rect {
        painter_clip_rect(&self.0)
    }

    fn with_clip(&self, rect: vocab::Rect) -> Box<dyn crate::mui::PainterSink> {
        Box::new(Self(painter_with_clip(&self.0, rect)))
    }

    fn render(&self, cmd: PaintCmd) {
        render_paint_cmd(&self.0, cmd);
    }

    fn render_text(&self, cmd: PaintCmd) -> vocab::Rect {
        render_text_cmd(&self.0, cmd)
    }

    fn measure_text(&self, text: &str, size: f32, mono: bool) -> vocab::Vec2 {
        measure_text_for_spec(
            &self.0,
            &crate::layout::TextMeasureSpec::new(text, size, mono),
        )
    }

    fn measure_text_runs(&self, runs: &[crate::paint::TextRun]) -> vocab::Vec2 {
        measure_text_runs_for_painter(&self.0, runs)
    }
}

pub(crate) fn area_registered_painter(
    ctx: &egui::Context,
    layer: Layer,
    id: vocab::Id,
    clip: vocab::Rect,
) -> egui::Painter {
    let clip_rect: egui::Rect = clip.into();
    let painter = egui::Area::new(id.into())
        .order(egui_order_for_layer(layer))
        .fixed_pos(clip_rect.min)
        .interactable(false)
        .show(ctx, |ui| ui.painter().clone())
        .inner;
    painter.with_clip_rect(clip_rect)
}

pub(crate) fn show_area_slot<R>(
    ctx: &egui::Context,
    spec: AreaSlotSpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    show_area_for_host(ctx, spec.host, |ui| {
        ui.set_min_size(spec.size.into());
        body(ui)
    })
}

pub(crate) fn constrain_ui_to_rect(ui: &mut egui::Ui, rect: vocab::Rect) {
    let rect: egui::Rect = rect.into();
    ui.set_clip_rect(rect);
    ui.set_min_size(rect.size());
    ui.set_max_size(rect.size());
}

pub(crate) fn vertical_scroll_area_for_region(region: ScrollRegion) -> egui::ScrollArea {
    debug_assert_eq!(region.axis, ScrollAxis::Vertical);
    scroll_area_for_region(region)
}

pub(crate) fn scroll_area_for_region(region: ScrollRegion) -> egui::ScrollArea {
    let area = match region.axis {
        ScrollAxis::Horizontal => egui::ScrollArea::horizontal().max_width(region.max_extent),
        ScrollAxis::Vertical => egui::ScrollArea::vertical().max_height(region.max_extent),
    };
    let area = area
        .id_salt(Into::<egui::Id>::into(region.id))
        .auto_shrink(region.auto_shrink);
    match (region.axis, region.min_scrolled_extent) {
        (_, None) => area,
        (ScrollAxis::Vertical, Some(extent)) => area.min_scrolled_height(extent),
        (ScrollAxis::Horizontal, Some(extent)) => area.min_scrolled_width(extent),
    }
}

pub(crate) fn apply_scroll_region_spacing(ui: &mut egui::Ui, region: ScrollRegion) {
    ui.spacing_mut().item_spacing = region.item_spacing.into();
}

pub(crate) fn show_vertical_scroll_region<R>(
    ui: &mut egui::Ui,
    region: ScrollRegion,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::containers::scroll_area::ScrollAreaOutput<R> {
    vertical_scroll_area_for_region(region).show(ui, |ui| {
        apply_scroll_region_spacing(ui, region);
        body(ui)
    })
}

pub(crate) fn show_container_body_slot<R>(
    ui: &mut egui::Ui,
    spec: ContainerBodySpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, f32) {
    let slot_size = ui_available_rect(ui).size();
    ui.allocate_ui_with_layout(
        slot_size.into(),
        egui::Layout::top_down(egui::Align::Min),
        move |ui| {
            if spec.horizontal_strip {
                ui.set_max_width(spec.span_inner);
            } else {
                ui.set_max_height(spec.span_inner);
                if let Some(max_flow) = spec.max_flow {
                    ui.set_max_width(max_flow);
                }
            }

            let scroll = egui::ScrollArea::vertical()
                .id_salt("mara_body_scroll_v")
                .auto_shrink([false, false])
                .min_scrolled_height(0.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    let result = body(ui);
                    if spec.end_pad > 0.0 {
                        add_space_for_spec(ui, SpaceSpec::vertical(spec.end_pad));
                    }
                    result
                });
            (scroll.inner, scroll.content_size.y)
        },
    )
    .inner
}

pub(crate) fn apply_pane_flex_spec(ui: &mut egui::Ui, spec: PaneFlexSpec) {
    if spec.horizontal_strip {
        ui.set_max_width(spec.span_inner);
    } else {
        ui.set_max_height(spec.span_inner);
    }
    apply_item_spacing_spec(ui, ItemSpacingSpec::new(spec.item_spacing));
}

pub(crate) fn reserve_pane_title_slot(ui: &mut egui::Ui, spec: PaneFlexSpec) -> vocab::Rect {
    let (rect, _) = ui.allocate_exact_size(spec.title_size().into(), egui::Sense::hover());
    rect.into()
}

pub(crate) fn show_pane_body_scroll_slot<R>(
    ui: &mut egui::Ui,
    spec: PaneBodyScrollSpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let max_body_flow = match spec.axis {
        PaneBodyScrollAxis::FlowVertical => ui_available_height(ui),
        PaneBodyScrollAxis::FlowHorizontal => ui_available_width(ui),
    }
    .max(0.0);

    let scroll_area = match spec.axis {
        PaneBodyScrollAxis::FlowVertical => egui::ScrollArea::vertical()
            .max_height(max_body_flow)
            .min_scrolled_width(spec.span_inner),
        PaneBodyScrollAxis::FlowHorizontal => egui::ScrollArea::horizontal()
            .max_width(max_body_flow)
            .min_scrolled_height(spec.span_inner),
    }
    .id_salt(Into::<egui::Id>::into(spec.id))
    .auto_shrink([false, false]);

    crate::scroll::show_sticky_scroll_area(
        ui,
        match spec.axis {
            PaneBodyScrollAxis::FlowVertical => crate::scroll::StickyScrollAxis::Vertical,
            PaneBodyScrollAxis::FlowHorizontal => crate::scroll::StickyScrollAxis::Horizontal,
        },
        scroll_area,
        |ui| {
            match spec.axis {
                PaneBodyScrollAxis::FlowVertical => {
                    ui.set_min_width(spec.span_inner);
                    ui.set_max_width(spec.span_inner);
                }
                PaneBodyScrollAxis::FlowHorizontal => {
                    ui.set_min_height(spec.span_inner);
                    ui.set_max_height(spec.span_inner);
                }
            }
            apply_item_spacing_spec(ui, ItemSpacingSpec::new(spec.item_spacing));
            body(ui)
        },
    )
    .inner
}

pub(crate) fn egui_popup_align(align: PopupAlign) -> egui::RectAlign {
    match align {
        PopupAlign::BottomStart => egui::RectAlign::BOTTOM_START,
    }
}

/// Show an anchored popup whose open-state is owned by the caller via
/// `open` (Mara `PopupState`/`MaraMemory`), rather than egui's internal
/// popup memory.
///
/// egui keeps its anchoring and dismissal behaviour: it renders only
/// while `*open` is true and sets `*open = false` on the same
/// click-outside / Escape conditions as `from_toggle_button_response`.
/// The caller is responsible for toggling `*open` on trigger click
/// (the same `response.clicked()` condition egui used internally), so
/// behaviour is preserved while the persistent state moves to Mara.
pub(crate) fn show_popup_open_bool<R>(
    response: &egui::Response,
    open: &mut bool,
    spec: PopupSpec,
    frame: crate::style::FrameSpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<R>> {
    egui::Popup::from_response(response)
        .open_bool(open)
        .align(egui_popup_align(spec.align))
        .gap(spec.gap)
        .width(spec.width)
        .frame(egui_frame_for_style_spec(frame).inner_margin(egui::Margin::same(spec.inner_margin)))
        .show(body)
}

pub(crate) fn popup_toggle_response(
    ctx: &egui::Context,
    trigger: PopupTrigger,
) -> Option<egui::Response> {
    with_response(ctx, trigger.response_id, |raw| egui::Response {
        id: trigger.popup_id.into(),
        ..raw.clone()
    })
}

pub(crate) fn popup_toggle_response_for_ui(
    ui: &egui::Ui,
    trigger: PopupTrigger,
) -> Option<egui::Response> {
    popup_toggle_response(ui.ctx(), trigger)
}

pub(crate) fn apply_popup_list_spec(ui: &mut egui::Ui, spec: PopupListSpec) {
    ui.spacing_mut().item_spacing = spec.item_spacing.into();
}

pub(crate) fn child_ui_for_text_edit_region(ui: &mut egui::Ui, region: TextEditRegion) -> egui::Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(region.text_rect.into())
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    )
}

pub(crate) fn singleline_text_edit_for_spec<'a>(
    text: &'a mut String,
    spec: TextEditSpec,
) -> egui::TextEdit<'a> {
    let hint_text = egui::WidgetText::from(
        egui::RichText::new(spec.hint.clone())
            .color(Into::<egui::Color32>::into(spec.hint_color))
            .size(spec.region.font_size),
    );
    let edit = egui::TextEdit::singleline(text)
        .desired_width(spec.region.desired_width())
        .hint_text(hint_text)
        .text_color(spec.text_color.into())
        .background_color(spec.background_color.into())
        .font(egui::FontId::proportional(spec.region.font_size));
    if spec.frame {
        edit
    } else {
        edit.frame(egui::Frame::NONE)
    }
}

pub(crate) struct TextEditOutput {
    pub response: MaraResponse,
    pub has_focus: bool,
}

pub(crate) fn show_singleline_text_edit_for_spec(
    ui: &mut egui::Ui,
    text: &mut String,
    spec: TextEditSpec,
) -> TextEditOutput {
    let mut child_ui = child_ui_for_text_edit_region(ui, spec.region);
    let rect = spec.region.rect;
    let edit = singleline_text_edit_for_spec(text, spec);
    let mut response = child_ui.add(edit);
    response.rect = rect.into();
    let has_focus = response.has_focus();
    TextEditOutput {
        response: mara_response_from(&response),
        has_focus,
    }
}

pub(crate) fn show_text_edit_with_focus_policy(
    ui: &mut egui::Ui,
    text: &mut String,
    spec: TextEditSpec,
    focus_when_unfocused: bool,
) -> MaraResponse {
    let output = show_singleline_text_edit_for_spec(ui, text, spec);
    if focus_when_unfocused && !output.has_focus {
        request_focus_for_ui_response(ui, &output.response);
    }
    output.response
}

pub(crate) fn request_focus_for_response(ctx: &egui::Context, response: &MaraResponse) {
    let _ = with_response(ctx, response.backend_response_id(), |raw| {
        raw.clone().request_focus();
    });
}

pub(crate) fn request_focus_for_ui_response(ui: &egui::Ui, response: &MaraResponse) {
    request_focus_for_response(ui.ctx(), response);
}

pub(crate) fn hover_cursor_for_response(
    ctx: &egui::Context,
    response: &MaraResponse,
    cursor: CursorIcon,
) {
    let _ = with_response(ctx, response.backend_response_id(), |raw| {
        raw.clone().on_hover_cursor(egui_cursor_icon(cursor));
    });
}

pub(crate) fn hover_cursor_for_ui_response(
    ui: &egui::Ui,
    response: &MaraResponse,
    cursor: CursorIcon,
) {
    hover_cursor_for_response(ui.ctx(), response, cursor);
}

pub(crate) fn set_cursor_icon_for_context(ctx: &egui::Context, cursor: CursorIcon) {
    ctx.set_cursor_icon(egui_cursor_icon(cursor));
}

pub(crate) fn set_cursor_icon_for_ui(ui: &egui::Ui, cursor: CursorIcon) {
    set_cursor_icon_for_context(ui.ctx(), cursor);
}

pub(crate) fn show_inline_picker_scope<R>(
    ui: &mut egui::Ui,
    spec: InlinePickerSpec,
    content: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.scope(|ui| {
        ui.spacing_mut().slider_width = spec.slider_width;
        let clip = ui.clip_rect().expand(spec.clip_expand);
        ui.set_clip_rect(clip);
        content(ui)
    })
    .inner
}

pub(crate) fn show_frame_for_spec<R>(
    ui: &mut egui::Ui,
    spec: FrameHostSpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.set_max_width(spec.outer_width);
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .stroke(egui::Stroke::NONE)
        .corner_radius(Into::<egui::CornerRadius>::into(spec.corner))
        .inner_margin(egui::Margin::symmetric(
            spec.inner_margin[0],
            spec.inner_margin[1],
        ))
        .shadow(egui::epaint::Shadow::NONE)
        .show(ui, |ui| {
            ui.set_width(spec.content_width);
            body(ui)
        })
}

pub(crate) fn render_paint_cmd_on_z_layer(
    ui: &mut egui::Ui,
    id: vocab::Id,
    tier: u16,
    rect: vocab::Rect,
    opacity: f32,
    cmd: PaintCmd,
) {
    let layer_id = crate::layer::layer_id(Into::<egui::Id>::into(id), tier);
    match cmd {
        PaintCmd::Svg { .. } => {
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .layer_id(layer_id)
                    .max_rect(rect.into())
                    .layout(egui::Layout::default()),
            );
            child.set_opacity(opacity);
            render_paint_cmd_ui(&mut child, cmd);
        }
        cmd => {
            let mut painter = ui.ctx().layer_painter(layer_id);
            painter.set_opacity(opacity);
            render_paint_cmd(&painter, cmd);
        }
    }
}

pub(crate) fn add_space_for_spec(ui: &mut egui::Ui, spec: SpaceSpec) {
    if spec.size.x == 0.0 {
        ui.add_space(spec.size.y);
    } else {
        ui.allocate_space(spec.size.into());
    }
}

pub(crate) fn apply_item_spacing_spec(ui: &mut egui::Ui, spec: ItemSpacingSpec) {
    ui.spacing_mut().item_spacing = spec.item_spacing.into();
}

pub(crate) fn stack_direction_for_ui(ui: &egui::Ui) -> StackDirection {
    match ui.layout().main_dir() {
        egui::Direction::TopDown => StackDirection::TopDown,
        egui::Direction::BottomUp => StackDirection::BottomUp,
        egui::Direction::LeftToRight => StackDirection::LeftToRight,
        egui::Direction::RightToLeft => StackDirection::RightToLeft,
    }
}

pub(crate) fn egui_align_for_stack_align(align: StackAlign) -> egui::Align {
    match align {
        StackAlign::Min => egui::Align::Min,
        StackAlign::Center => egui::Align::Center,
        StackAlign::Max => egui::Align::Max,
    }
}

pub(crate) fn egui_layout_for_child_region(region: ChildRegion) -> egui::Layout {
    egui_layout_for_stack_direction(region.direction, region.align)
}

pub(crate) fn egui_layout_for_stack_direction(
    direction: StackDirection,
    align: StackAlign,
) -> egui::Layout {
    let align = egui_align_for_stack_align(align);
    match direction {
        StackDirection::TopDown => egui::Layout::top_down(align),
        StackDirection::BottomUp => egui::Layout::bottom_up(align),
        StackDirection::LeftToRight => egui::Layout::left_to_right(align),
        StackDirection::RightToLeft => egui::Layout::right_to_left(align),
    }
}

pub(crate) fn child_ui_for_region(ui: &mut egui::Ui, region: ChildRegion) -> egui::Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(region.rect.into())
            .layout(egui_layout_for_child_region(region)),
    )
}

pub(crate) fn ui_available_rect(ui: &egui::Ui) -> vocab::Rect {
    ui.available_rect_before_wrap().into()
}

pub(crate) fn ui_available_width(ui: &egui::Ui) -> f32 {
    ui.available_width()
}

pub(crate) fn ui_available_height(ui: &egui::Ui) -> f32 {
    ui.available_height()
}

pub(crate) fn ui_id(ui: &egui::Ui) -> vocab::Id {
    ui.id().into()
}

pub(crate) fn painter_for_ui_clip(ui: &egui::Ui, rect: vocab::Rect) -> egui::Painter {
    let rect: egui::Rect = rect.into();
    ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()))
}

pub(crate) fn painter_for_ui_surface(ui: &egui::Ui, spec: PaintSurfaceSpec) -> egui::Painter {
    match spec.region {
        PaintSurfaceRegion::RemainingAvailable => painter_for_ui_clip(ui, ui_available_rect(ui)),
        PaintSurfaceRegion::ClipRect(rect) => painter_for_ui_clip(ui, rect),
    }
}

/// Snapshot an egui response into Mara's plain-data form.
///
/// Was `impl From<egui::Response> for MaraResponse`; moved here because
/// that impl cannot live in a backend crate — both types would be
/// foreign to it. Public because host-tier code and the vendored graph
/// renderer still hold raw responses and need the same snapshot; they
/// call this rather than reimplementing the flag capture.
pub fn mara_response_from(inner: &egui::Response) -> MaraResponse {
    let mut mara = MaraResponse::__internal_from_backend(
        inner.rect.into(),
        vocab::PointerButton::ALL.map(|button| inner.clicked_by(egui_pointer_button(button))),
        vocab::PointerButton::ALL.map(|button| inner.dragged_by(egui_pointer_button(button))),
        remember_response(inner),
    );
    mara.clicked = inner.clicked();
    mara.double_clicked = inner.double_clicked();
    mara.secondary_clicked = inner.secondary_clicked();
    mara.hovered = inner.hovered();
    mara.changed = inner.changed();
    mara.dragged = inner.dragged();
    mara.drag_started = inner.drag_started();
    mara.drag_stopped = inner.drag_stopped();
    mara.pointer_button_down = inner.is_pointer_button_down_on();
    mara.drag_delta = inner.drag_delta().into();
    mara.interact_pointer = inner.interact_pointer_pos().map(Into::into);
    mara.hover_pos = inner.hover_pos().map(Into::into);
    mara
}

pub(crate) fn remember_response(response: &egui::Response) -> vocab::Id {
    let rect = response.rect;
    let key = response.id.with((
        "mara_response",
        response.ctx.cumulative_frame_nr(),
        rect.min.x.to_bits(),
        rect.min.y.to_bits(),
        rect.max.x.to_bits(),
        rect.max.y.to_bits(),
    ));
    response
        .ctx
        .data_mut(|data| data.insert_temp(key, response.clone()));
    key.into()
}

pub(crate) fn with_response<R>(
    ctx: &egui::Context,
    key: vocab::Id,
    body: impl FnOnce(&egui::Response) -> R,
) -> Option<R> {
    let key: egui::Id = key.into();
    ctx.data(|data| data.get_temp::<egui::Response>(key))
        .map(|response| body(&response))
}

pub(crate) fn with_response_for_ui<R>(
    ui: &egui::Ui,
    response: &MaraResponse,
    body: impl FnOnce(&egui::Response) -> R,
) -> Option<R> {
    with_response(ui.ctx(), response.backend_response_id(), body)
}

pub(crate) fn hover_text(ctx: &egui::Context, key: vocab::Id, text: &str) {
    let _ = with_response(ctx, key, |response| {
        response.clone().on_hover_text(text);
    });
}

pub(crate) fn input_snapshot(ctx: &egui::Context) -> MaraInput {
    ctx.input(|i| MaraInput {
        pointer: i.pointer.latest_pos().map(Into::into),
        interact_pointer: i.pointer.interact_pos().map(Into::into),
        primary_down: i.pointer.primary_down(),
        primary_pressed: i.pointer.primary_pressed(),
        primary_released: i.pointer.primary_released(),
        any_released: i.pointer.any_released(),
        secondary_down: i.pointer.secondary_down(),
        secondary_pressed: i.pointer.secondary_pressed(),
        middle_down: i.pointer.middle_down(),
        middle_pressed: i.pointer.button_pressed(egui::PointerButton::Middle),
        middle_released: i.pointer.button_released(egui::PointerButton::Middle),
        scroll_delta: i.smooth_scroll_delta.into(),
        pointer_delta: i.pointer.delta().into(),
        zoom_delta: i.zoom_delta(),
        modifiers_shift: i.modifiers.shift,
        modifiers_ctrl: i.modifiers.ctrl,
        modifiers_alt: i.modifiers.alt,
        keys_pressed: MaraKey::ALL
            .into_iter()
            .filter(|&key| i.key_pressed(egui_key(key)))
            .collect(),
    })
}

pub(crate) fn input_snapshot_for_ui(ui: &egui::Ui) -> MaraInput {
    input_snapshot(ui.ctx())
}

/// The store behind `ui`, for callers that want a memory facade.
///
/// Returns the wrapper rather than the facade: `MaraMemoryCtx` borrows
/// its store, so the caller has to own one for as long as it holds the
/// facade.
pub(crate) fn store_for_ui(ui: &egui::Ui) -> EguiCtx {
    EguiCtx::new(ui.ctx())
}

pub(crate) fn painter_clip_rect(painter: &egui::Painter) -> vocab::Rect {
    painter.clip_rect().into()
}

pub(crate) fn painter_with_clip(painter: &egui::Painter, rect: vocab::Rect) -> egui::Painter {
    let rect: egui::Rect = rect.into();
    painter.with_clip_rect(rect.intersect(painter.clip_rect()))
}

/// Sample an (elliptical) arc into a polyline. Angles in radians, 0 at
/// +x, increasing clockwise (screen y-down). Segment count scales with
/// the swept angle so small arcs stay cheap and big ones stay smooth.
fn arc_polyline(
    center: vocab::Pos2,
    radius: vocab::Vec2,
    start_angle: f32,
    end_angle: f32,
) -> Vec<egui::Pos2> {
    let c: egui::Pos2 = center.into();
    let r: egui::Vec2 = radius.into();
    let sweep = (end_angle - start_angle).abs();
    let segments = ((sweep / std::f32::consts::TAU * 64.0).ceil() as usize).clamp(2, 512);
    (0..=segments)
        .map(|i| {
            let t = start_angle + (end_angle - start_angle) * (i as f32 / segments as f32);
            egui::pos2(c.x + r.x * t.cos(), c.y + r.y * t.sin())
        })
        .collect()
}

/// Internal egui adapter for first-party crates that have already
/// lowered drawing semantics into Mara [`PaintCmd`] values but still
/// need to render through the current egui backend.
///
/// This is not app-facing API; future backends should consume
/// `PaintCmd` directly through their own renderer.
#[cfg(feature = "backend-egui-conv")]
#[doc(hidden)]
pub fn __internal_render_paint_cmd_egui(painter: &egui::Painter, cmd: PaintCmd) {
    render_paint_cmd(painter, cmd);
}

pub(crate) fn render_paint_cmd(painter: &egui::Painter, cmd: PaintCmd) {
    match cmd {
        PaintCmd::Ellipse { rect, fill, stroke } => {
            let rect: egui::Rect = rect.into();
            painter.add(egui::epaint::EllipseShape {
                center: rect.center(),
                radius: rect.size() / 2.0,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke),
                angle: 0.0,
            });
        }
        PaintCmd::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            stroke,
        } => {
            painter.add(egui::Shape::line(
                arc_polyline(center, radius, start_angle, end_angle),
                Into::<egui::Stroke>::into(stroke),
            ));
        }
        PaintCmd::Sector {
            center,
            radius,
            start_angle,
            end_angle,
            fill,
            stroke,
        } => {
            let mut points = vec![center.into()];
            points.extend(arc_polyline(center, radius, start_angle, end_angle));
            painter.add(egui::epaint::PathShape {
                points,
                closed: true,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke).into(),
            });
        }
        PaintCmd::Line { a, b, stroke } => {
            painter.line_segment([a.into(), b.into()], Into::<egui::Stroke>::into(stroke));
        }
        PaintCmd::Polyline { points, stroke } => {
            painter.line(
                points.into_iter().map(Into::into).collect(),
                Into::<egui::Stroke>::into(stroke),
            );
        }
        PaintCmd::Polygon {
            points,
            fill,
            stroke,
        } => {
            painter.add(egui::epaint::PathShape {
                points: points.into_iter().map(Into::into).collect(),
                closed: true,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke).into(),
            });
        }
        PaintCmd::RectFilled { rect, corner, fill } => {
            painter.rect_filled(
                rect.into(),
                Into::<egui::CornerRadius>::into(corner),
                Into::<egui::Color32>::into(fill),
            );
        }
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke,
        } => {
            painter.rect_stroke(
                rect.into(),
                Into::<egui::CornerRadius>::into(corner),
                Into::<egui::Stroke>::into(stroke),
                egui::StrokeKind::Inside,
            );
        }
        PaintCmd::RectStrokeOutside {
            rect,
            corner,
            stroke,
        } => {
            painter.rect_stroke(
                rect.into(),
                Into::<egui::CornerRadius>::into(corner),
                Into::<egui::Stroke>::into(stroke),
                egui::StrokeKind::Outside,
            );
        }
        PaintCmd::CircleFilled {
            center,
            radius,
            fill,
        } => {
            painter.circle_filled(center.into(), radius, Into::<egui::Color32>::into(fill));
        }
        PaintCmd::CircleStroke {
            center,
            radius,
            stroke,
        } => {
            painter.circle_stroke(center.into(), radius, Into::<egui::Stroke>::into(stroke));
        }
        PaintCmd::Arrow {
            origin,
            vec,
            stroke,
        } => {
            painter.arrow(
                origin.into(),
                vec.into(),
                Into::<egui::Stroke>::into(stroke),
            );
        }
        PaintCmd::Text { .. } | PaintCmd::TextWithFamily { .. } => {
            let _ = render_text_cmd(painter, cmd);
        }
        PaintCmd::TextRuns {
            pos,
            anchor,
            angle,
            runs,
        } => {
            let _ = render_text_runs_cmd(painter, pos, anchor, angle, runs);
        }
        PaintCmd::Image {
            texture,
            rect,
            uv,
            tint,
        } => {
            painter.image(texture.into(), rect.into(), uv.into(), tint.into());
        }
        PaintCmd::Svg { svg, rect, tint } => {
            render_svg_cmd_painter(painter, svg, rect, tint);
        }
        PaintCmd::Mesh { vertices, indices } => {
            painter.add(egui::Shape::mesh(egui_mesh_from_mara(vertices, indices)));
        }
        PaintCmd::Shadow {
            rect,
            corner,
            offset,
            blur,
            spread,
            color,
        } => {
            painter.add(
                egui::epaint::Shadow {
                    offset,
                    blur,
                    spread,
                    color: color.into(),
                }
                .as_shape(rect.into(), Into::<egui::CornerRadius>::into(corner)),
            );
        }
        PaintCmd::Noop => {}
        PaintCmd::Clip { rect, children } => {
            let clipped = painter_with_clip(painter, rect);
            for child in children {
                render_paint_cmd(&clipped, child);
            }
        }
        PaintCmd::Group(children) => {
            for child in children {
                render_paint_cmd(painter, child);
            }
        }
    }
}

pub(crate) fn shape_from_paint_cmd(cmd: PaintCmd) -> egui::Shape {
    match cmd {
        PaintCmd::Ellipse { rect, fill, stroke } => {
            let rect: egui::Rect = rect.into();
            egui::Shape::Ellipse(egui::epaint::EllipseShape {
                center: rect.center(),
                radius: rect.size() / 2.0,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke),
                angle: 0.0,
            })
        }
        PaintCmd::Arc {
            center,
            radius,
            start_angle,
            end_angle,
            stroke,
        } => egui::Shape::line(
            arc_polyline(center, radius, start_angle, end_angle),
            Into::<egui::Stroke>::into(stroke),
        ),
        PaintCmd::Sector {
            center,
            radius,
            start_angle,
            end_angle,
            fill,
            stroke,
        } => {
            let mut points = vec![center.into()];
            points.extend(arc_polyline(center, radius, start_angle, end_angle));
            egui::Shape::Path(egui::epaint::PathShape {
                points,
                closed: true,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke).into(),
            })
        }
        PaintCmd::Line { a, b, stroke } => {
            egui::Shape::line_segment([a.into(), b.into()], Into::<egui::Stroke>::into(stroke))
        }
        PaintCmd::Polyline { points, stroke } => egui::Shape::line(
            points.into_iter().map(Into::into).collect(),
            Into::<egui::Stroke>::into(stroke),
        ),
        PaintCmd::Polygon {
            points,
            fill,
            stroke,
        } => egui::Shape::Path(egui::epaint::PathShape {
            points: points.into_iter().map(Into::into).collect(),
            closed: true,
            fill: fill.into(),
            stroke: Into::<egui::Stroke>::into(stroke).into(),
        }),
        PaintCmd::RectFilled { rect, corner, fill } => egui::Shape::rect_filled(
            rect.into(),
            Into::<egui::CornerRadius>::into(corner),
            Into::<egui::Color32>::into(fill),
        ),
        PaintCmd::RectStroke {
            rect,
            corner,
            stroke,
        } => egui::Shape::rect_stroke(
            rect.into(),
            Into::<egui::CornerRadius>::into(corner),
            Into::<egui::Stroke>::into(stroke),
            egui::StrokeKind::Inside,
        ),
        PaintCmd::RectStrokeOutside {
            rect,
            corner,
            stroke,
        } => egui::Shape::rect_stroke(
            rect.into(),
            Into::<egui::CornerRadius>::into(corner),
            Into::<egui::Stroke>::into(stroke),
            egui::StrokeKind::Outside,
        ),
        PaintCmd::CircleFilled {
            center,
            radius,
            fill,
        } => egui::Shape::circle_filled(center.into(), radius, Into::<egui::Color32>::into(fill)),
        PaintCmd::CircleStroke {
            center,
            radius,
            stroke,
        } => egui::Shape::circle_stroke(center.into(), radius, Into::<egui::Stroke>::into(stroke)),
        PaintCmd::Mesh { vertices, indices } => {
            egui::Shape::mesh(egui_mesh_from_mara(vertices, indices))
        }
        PaintCmd::Shadow {
            rect,
            corner,
            offset,
            blur,
            spread,
            color,
        } => egui::epaint::Shadow {
            offset,
            blur,
            spread,
            color: color.into(),
        }
        .as_shape(rect.into(), Into::<egui::CornerRadius>::into(corner))
        .into(),
        PaintCmd::Arrow { .. }
        | PaintCmd::Text { .. }
        | PaintCmd::TextWithFamily { .. }
        | PaintCmd::TextRuns { .. }
        | PaintCmd::Image { .. }
        | PaintCmd::Svg { .. }
        | PaintCmd::Clip { .. }
        | PaintCmd::Noop => egui::Shape::Noop,
        PaintCmd::Group(children) => {
            egui::Shape::Vec(children.into_iter().map(shape_from_paint_cmd).collect())
        }
    }
}

pub(crate) fn render_paint_cmd_ui(ui: &mut egui::Ui, cmd: PaintCmd) {
    match cmd {
        PaintCmd::Svg { svg, rect, tint } => {
            render_svg_cmd(ui, svg, rect, tint);
        }
        cmd => render_paint_cmd(ui.painter(), cmd),
    }
}

fn render_svg_cmd(ui: &mut egui::Ui, svg: String, rect: vocab::Rect, tint: vocab::Color32) {
    let rect: egui::Rect = rect.into();
    let uri = format!("bytes://mara_svg_paint_{:016x}.svg", svg_stable_hash(&svg));
    let image = egui::Image::from_bytes(uri, svg.into_bytes())
        .tint(Into::<egui::Color32>::into(tint))
        .fit_to_exact_size(rect.size());
    image.paint_at(ui, rect);
}

/// Painter-side SVG rendering.
///
/// The `Ui` path can lean on [`egui::Image`], which needs a `Ui` to
/// paint into. A painter has none, so this resolves the texture through
/// the context's loader chain itself and emits a plain textured quad.
/// Without this, [`PaintCmd::Svg`] silently drew nothing whenever a
/// surface painted through [`crate::MaraPainter`] rather than a `Ui` —
/// which is every sealed module.
fn render_svg_cmd_painter(
    painter: &egui::Painter,
    svg: String,
    rect: vocab::Rect,
    tint: vocab::Color32,
) {
    let rect: egui::Rect = rect.into();
    if !rect.is_positive() {
        return;
    }
    let ctx = painter.ctx();
    let uri = format!("bytes://mara_svg_paint_{:016x}.svg", svg_stable_hash(&svg));
    ctx.include_bytes(uri.clone(), svg.into_bytes());

    let pixels_per_point = ctx.pixels_per_point();
    let size_hint = egui::load::SizeHint::Size {
        width: (rect.width() * pixels_per_point).round().max(1.0) as u32,
        height: (rect.height() * pixels_per_point).round().max(1.0) as u32,
        maintain_aspect_ratio: true,
    };

    match ctx.try_load_texture(&uri, egui::TextureOptions::LINEAR, size_hint) {
        Ok(egui::load::TexturePoll::Ready { texture }) => {
            painter.image(
                texture.id,
                rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                tint.into(),
            );
        }
        // Rasterising happens off-frame; ask for another so the marker
        // appears rather than waiting for the next unrelated repaint.
        Ok(egui::load::TexturePoll::Pending { .. }) => ctx.request_repaint(),
        Err(_) => {}
    }
}

fn svg_stable_hash(svg: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in svg.as_bytes() {
        h = h.wrapping_mul(33).wrapping_add(u64::from(*b));
    }
    h
}

fn egui_mesh_from_mara(vertices: Vec<crate::paint::PaintVertex>, indices: Vec<u32>) -> egui::Mesh {
    let vertices = vertices
        .into_iter()
        .map(|vertex| egui::epaint::Vertex {
            pos: vertex.pos.into(),
            uv: egui::epaint::WHITE_UV,
            color: vertex.color.into(),
        })
        .collect();
    egui::Mesh {
        indices,
        vertices,
        ..Default::default()
    }
}

pub(crate) fn render_text_cmd(painter: &egui::Painter, cmd: PaintCmd) -> vocab::Rect {
    let (pos, anchor, text, color, font) = match cmd {
        PaintCmd::Text {
            pos,
            anchor,
            text,
            size,
            color,
            mono,
        } => {
            let font = if mono {
                FontId::monospace(size)
            } else {
                FontId::proportional(size)
            };
            (pos, anchor, text, color, Some(font))
        }
        PaintCmd::TextWithFamily {
            pos,
            anchor,
            text,
            size,
            color,
            family,
        } => {
            let family = egui_font_family_for_text_family(family);
            if !painter.fonts(|fonts| fonts.families().contains(&family)) {
                return vocab::Rect::NOTHING;
            }
            (pos, anchor, text, color, Some(FontId::new(size, family)))
        }
        _ => return vocab::Rect::NOTHING,
    };
    let Some(font) = font else {
        return vocab::Rect::NOTHING;
    };
    painter
        .text(pos.into(), anchor.into(), text, font, color.into())
        .into()
}

pub(crate) fn render_text_runs_cmd(
    painter: &egui::Painter,
    pos: vocab::Pos2,
    anchor: vocab::Align2,
    angle: f32,
    runs: Vec<TextRun>,
) -> vocab::Rect {
    let Some((job, fallback_color)) = layout_job_for_text_runs(painter, &runs) else {
        return vocab::Rect::NOTHING;
    };

    let galley = painter.layout_job(job);
    let rect = Into::<egui::Align2>::into(anchor)
        .anchor_rect(egui::Rect::from_min_size(pos.into(), galley.size()));
    let mut shape = egui::epaint::TextShape::new(rect.min, galley, fallback_color);
    shape.angle = angle;
    painter.add(shape);
    rect.into()
}

fn layout_job_for_text_runs(
    painter: &egui::Painter,
    runs: &[TextRun],
) -> Option<(egui::text::LayoutJob, egui::Color32)> {
    if runs.is_empty() {
        return None;
    }

    let mut job = egui::text::LayoutJob::default();
    let mut fallback_color = egui::Color32::WHITE;
    let mut appended_any = false;

    for run in runs {
        let family = egui_font_family_for_text_family(run.family.clone());
        if matches!(family, egui::FontFamily::Name(_))
            && !painter.fonts(|fonts| fonts.families().contains(&family))
        {
            continue;
        }

        let color: egui::Color32 = run.color.into();
        if color.a() > 0 {
            fallback_color = color;
        }
        job.append(
            &run.text,
            run.leading_space,
            egui::TextFormat {
                font_id: FontId::new(run.size, family),
                color,
                extra_letter_spacing: run.extra_letter_spacing,
                ..Default::default()
            },
        );
        appended_any = true;
    }

    appended_any.then_some((job, fallback_color))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vocab::{Color32, CornerRadius, Pos2, Rect, Stroke, Vec2};

    /// A floating surface paints with the accent its host names, not
    /// the process-wide one.
    ///
    /// A scoped view node is handed its own accent. Before the host
    /// carried it, `MaraCtx::area` reached for `active_accent()`, so a
    /// node's accent applied to everything it drew *except* the body it
    /// owns — a mismatch nothing would have reported.
    #[test]
    fn area_body_paints_with_the_hosts_accent() {
        use crate::context::MaraCtx;

        let want = Color32::from_rgb(1, 2, 3);
        assert_ne!(
            want,
            crate::style::active_accent(),
            "the global accent must differ, or this test passes vacuously"
        );

        let raw = egui::Context::default();
        let ctx = crate::backend::egui::EguiCtx::new(&raw);
        let mut seen = None;
        let _ = ctx.run_ui(Default::default(), |ui| {
            MaraCtx::area(
                &EguiCtx::new(ui.ctx()),
                AreaHost::new(vocab::Id::new("accented"), Pos2::ZERO, Layer::Foreground)
                    .accent(want),
                &mut |mara| seen = Some(mara.accent()),
            );
        });

        assert_eq!(seen, Some(want));
    }

    #[test]
    fn egui_order_mapping_preserves_layer_rank() {
        // The egui backend must honour the Layer contract: a
        // higher-rank Mara layer maps to an egui order that paints
        // no lower than a lower-rank one.
        let layers = [
            Layer::Background,
            Layer::Middle,
            Layer::Foreground,
            Layer::Overlay,
        ];
        for pair in layers.windows(2) {
            assert!(
                egui_order_for_layer(pair[0]) <= egui_order_for_layer(pair[1]),
                "egui order must be monotonic with Layer::rank"
            );
        }
    }

    #[test]
    fn arc_polyline_hits_endpoints_and_scales_with_sweep() {
        use std::f32::consts::{FRAC_PI_2, TAU};
        // Quarter arc, radius 10, 0 → 90° (clockwise: +x toward +y screen-down).
        let quarter = arc_polyline(Pos2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.0, FRAC_PI_2);
        assert!(quarter.len() >= 3);
        let first = quarter.first().unwrap();
        let last = quarter.last().unwrap();
        assert!((first.x - 10.0).abs() < 0.01 && first.y.abs() < 0.01);
        assert!(last.x.abs() < 0.01 && (last.y - 10.0).abs() < 0.01);
        // A full sweep is sampled with more segments than a quarter sweep.
        let full = arc_polyline(Pos2::new(0.0, 0.0), Vec2::new(10.0, 10.0), 0.0, TAU);
        assert!(full.len() > quarter.len());
    }

    #[test]
    fn shape_from_paint_cmd_maps_ellipse_arc_sector() {
        let ellipse = shape_from_paint_cmd(PaintCmd::Ellipse {
            rect: Rect::from_min_size(Pos2::new(0.0, 0.0), Vec2::new(20.0, 10.0)),
            fill: Color32::WHITE,
            stroke: Stroke::NONE,
        });
        assert!(matches!(ellipse, egui::Shape::Ellipse(_)));

        let arc = shape_from_paint_cmd(PaintCmd::Arc {
            center: Pos2::new(0.0, 0.0),
            radius: Vec2::new(5.0, 5.0),
            start_angle: 0.0,
            end_angle: 1.0,
            stroke: Stroke::new(1.0, Color32::WHITE),
        });
        assert!(matches!(arc, egui::Shape::Path(_)));

        let sector = shape_from_paint_cmd(PaintCmd::Sector {
            center: Pos2::new(0.0, 0.0),
            radius: Vec2::new(5.0, 5.0),
            start_angle: 0.0,
            end_angle: 1.0,
            fill: Color32::WHITE,
            stroke: Stroke::NONE,
        });
        assert!(matches!(sector, egui::Shape::Path(_)));
    }

    #[test]
    fn egui_backend_can_make_shape_from_mara_rect_command() {
        let shape = shape_from_paint_cmd(PaintCmd::RectFilled {
            rect: Rect::from_min_size(Pos2::new(1.0, 2.0), Vec2::new(3.0, 4.0)),
            corner: CornerRadius::same(2),
            fill: Color32::WHITE,
        });

        assert!(!matches!(shape, egui::Shape::Noop));
    }

    #[test]
    fn egui_backend_maps_mara_layers_to_host_orders() {
        assert_eq!(
            egui_order_for_layer(Layer::Background),
            egui::Order::Background
        );
        assert_eq!(
            egui_order_for_layer(Layer::Foreground),
            egui::Order::Foreground
        );
        assert_eq!(egui_order_for_layer(Layer::Middle), egui::Order::Middle);
        assert_eq!(egui_order_for_layer(Layer::Overlay), egui::Order::Tooltip);
    }

    #[test]
    fn egui_backend_maps_mara_popup_align_to_host_align() {
        assert_eq!(
            egui_popup_align(PopupAlign::BottomStart),
            egui::RectAlign::BOTTOM_START
        );
    }

    #[test]
    fn egui_backend_maps_mara_keys_to_host_keys() {
        assert_eq!(egui_key(MaraKey::Escape), egui::Key::Escape);
        assert_eq!(egui_key(MaraKey::ArrowDown), egui::Key::ArrowDown);
        assert_eq!(egui_key(MaraKey::ArrowUp), egui::Key::ArrowUp);
        assert_eq!(egui_key(MaraKey::Enter), egui::Key::Enter);
    }

    #[test]
    fn egui_backend_maps_mara_cursor_icons_to_host_cursors() {
        assert_eq!(
            egui_cursor_icon(CursorIcon::PointingHand),
            egui::CursorIcon::PointingHand
        );
        assert_eq!(
            egui_cursor_icon(CursorIcon::Grabbing),
            egui::CursorIcon::Grabbing
        );
        assert_eq!(
            egui_cursor_icon(CursorIcon::ResizeHorizontal),
            egui::CursorIcon::ResizeHorizontal
        );
        assert_eq!(
            egui_cursor_icon(CursorIcon::ResizeVertical),
            egui::CursorIcon::ResizeVertical
        );
    }

    #[test]
    fn egui_backend_can_make_shape_from_mara_line_command() {
        let shape = shape_from_paint_cmd(PaintCmd::Line {
            a: Pos2::ZERO,
            b: Pos2::new(1.0, 1.0),
            stroke: Stroke::new(1.0, Color32::WHITE),
        });

        assert!(!matches!(shape, egui::Shape::Noop));
    }

    #[test]
    fn egui_backend_can_make_shape_from_mara_shadow_command() {
        let shape = shape_from_paint_cmd(PaintCmd::Shadow {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(20.0, 10.0)),
            corner: CornerRadius::same(4),
            offset: [0, 4],
            blur: 12,
            spread: 0,
            color: Color32::from_black_alpha(120),
        });

        assert!(!matches!(shape, egui::Shape::Noop));
    }

    #[test]
    fn egui_backend_can_make_shape_from_mara_outside_stroke_command() {
        let shape = shape_from_paint_cmd(PaintCmd::RectStrokeOutside {
            rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(20.0, 10.0)),
            corner: CornerRadius::same(4),
            stroke: Stroke::new(1.0, Color32::WHITE),
        });

        assert!(!matches!(shape, egui::Shape::Noop));
    }
}

// ─── Offscreen UI surfaces (PLAN.md WS-A7) ────────────────────────
//
// Lives here rather than in its own `offscreen.rs` because it is
// egui-wgpu backend code: the coupling ratchet counts files under
// `crates/core/src` that name egui, and `backend/` is where that
// coupling is accounted for.
#[cfg(feature = "gpu")]
mod offscreen {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use crate::mui::MaraUi;
    use crate::vocab;

    /// A surface's retained GPU + context state, reused frame to frame.
    struct OffscreenSurface {
        /// Independent context: its own `pixels_per_point`, font atlas and
        /// tessellator, which is the whole point — the parent's scale must
        /// not drive this subtree's rasterisation.
        ctx: egui::Context,
        renderer: egui_wgpu::Renderer,
        target: Option<OffscreenTarget>,
    }

    struct OffscreenTarget {
        /// Owns the GPU resource. `view` borrows from it, and the parent's
        /// registered texture id points at it, so it must outlive both.
        #[allow(dead_code)]
        texture: wgpu::Texture,
        view: wgpu::TextureView,
        size_pixels: [u32; 2],
        parent_texture: Option<egui::TextureId>,
    }

    /// Per-context registry of live surfaces, keyed by caller-supplied id.
    #[derive(Clone, Default)]
    struct OffscreenRegistry(Arc<Mutex<HashMap<vocab::Id, OffscreenSurface>>>);

    fn registry(ctx: &egui::Context) -> OffscreenRegistry {
        let key = egui::Id::new("mara_offscreen_registry");
        if let Some(existing) = ctx.data(|data| data.get_temp::<OffscreenRegistry>(key)) {
            return existing;
        }
        let fresh = OffscreenRegistry::default();
        ctx.data_mut(|data| data.insert_temp(key, fresh.clone()));
        fresh
    }

    /// Render `body` into an offscreen texture and return its id.
    ///
    /// `size_points` is the logical size of the surface; `scale` multiplies
    /// it to get the rasterisation resolution (`2.0` renders at twice the
    /// linear detail). Returns `None` when the surface cannot be prepared —
    /// a degenerate size, or a GPU allocation that failed — in which case
    /// the caller should paint a fallback rather than assume a texture.
    pub(crate) fn render_offscreen(
        parent: &egui::Context,
        gpu: mara_gpu::MaraRenderState<'_>,
        id: vocab::Id,
        size_points: vocab::Vec2,
        scale: f32,
        accent: vocab::Color32,
        input: OffscreenInput,
        body: &mut dyn FnMut(&mut MaraUi<'_>),
    ) -> Option<vocab::TextureId> {
        let scale = scale.clamp(0.05, 8.0);
        let pixels = [
            (size_points.x * scale).round().max(1.0) as u32,
            (size_points.y * scale).round().max(1.0) as u32,
        ];
        if size_points.x < 1.0 || size_points.y < 1.0 {
            return None;
        }

        let render_state = gpu.__internal_raw();
        let device = render_state.device.clone();
        let queue = render_state.queue.clone();
        let format = render_state.target_format;

        let registry = registry(parent);
        let mut surfaces = registry.0.lock().ok()?;
        let surface = surfaces.entry(id).or_insert_with(|| OffscreenSurface {
            ctx: egui::Context::default(),
            // `msaa_samples = 1` matches the target's sample count;
            // `dithering = false` keeps colours exact across formats, so the
            // composited texture matches what the parent would have drawn.
            renderer: egui_wgpu::Renderer::new(
                &device,
                format,
                egui_wgpu::RendererOptions {
                    msaa_samples: 1,
                    depth_stencil_format: None,
                    dithering: false,
                    predictable_texture_filtering: false,
                },
            ),
            target: None,
        });

        ensure_target(surface, render_state, &device, format, pixels);
        let target = surface.target.as_ref()?;
        let parent_texture = target.parent_texture?;
        let view = target.view.clone();

        // Drive the sub-context at the requested scale, then lower its
        // output into our own target rather than the window's surface.
        surface.ctx.set_pixels_per_point(scale);
        let raw_input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::pos2(0.0, 0.0),
                egui::vec2(size_points.x, size_points.y),
            )),
            events: offscreen_events(&input),
            modifiers: egui::Modifiers {
                shift: input.modifiers_shift,
                ctrl: input.modifiers_ctrl,
                alt: input.modifiers_alt,
                command: input.modifiers_ctrl,
                mac_cmd: false,
            },
            ..Default::default()
        };
        let output = surface.ctx.run_ui(raw_input, |ui| {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            body(&mut MaraUi::over(&mut backend, accent));
        });
        let primitives = surface
            .ctx
            .tessellate(output.shapes, surface.ctx.pixels_per_point());

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: pixels,
            pixels_per_point: scale,
        };
        for (texture_id, delta) in &output.textures_delta.set {
            surface
                .renderer
                .update_texture(&device, &queue, *texture_id, delta);
        }
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("mara_offscreen_encoder"),
        });
        surface
            .renderer
            .update_buffers(&device, &queue, &mut encoder, &primitives, &screen);
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mara_offscreen_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            surface.renderer.render(&mut pass, &primitives, &screen);
        }
        queue.submit(Some(encoder.finish()));
        for texture_id in &output.textures_delta.free {
            surface.renderer.free_texture(texture_id);
        }

        Some(parent_texture.into())
    }

    /// Pointer and keyboard state to feed into an offscreen surface,
    /// in the surface's OWN coordinate space.
    ///
    /// Without this an offscreen surface is inert: it has no window, so
    /// it receives no events unless the host forwards them. The caller
    /// maps window coordinates into surface-local ones — it is the only
    /// party that knows where the composited texture was drawn.
    #[derive(Clone, Copy, Debug, Default)]
    pub(crate) struct OffscreenInput {
        /// Pointer position in surface-local points, or `None` when the
        /// pointer is elsewhere.
        pub pointer: Option<vocab::Pos2>,
        pub primary_down: bool,
        pub secondary_down: bool,
        pub middle_down: bool,
        pub scroll_delta: vocab::Vec2,
        pub modifiers_shift: bool,
        pub modifiers_ctrl: bool,
        pub modifiers_alt: bool,
    }

    /// Translate [`OffscreenInput`] into the event stream the sub-context
    /// expects. Buttons become press/release pairs around the pointer
    /// position, which is what an immediate-mode context needs to see.
    fn offscreen_events(input: &OffscreenInput) -> Vec<egui::Event> {
        let mut events = Vec::new();
        let Some(pointer) = input.pointer else {
            events.push(egui::Event::PointerGone);
            return events;
        };
        let pos = egui::pos2(pointer.x, pointer.y);
        let modifiers = egui::Modifiers {
            shift: input.modifiers_shift,
            ctrl: input.modifiers_ctrl,
            alt: input.modifiers_alt,
            command: input.modifiers_ctrl,
            mac_cmd: false,
        };
        events.push(egui::Event::PointerMoved(pos));
        for (down, button) in [
            (input.primary_down, egui::PointerButton::Primary),
            (input.secondary_down, egui::PointerButton::Secondary),
            (input.middle_down, egui::PointerButton::Middle),
        ] {
            if down {
                events.push(egui::Event::PointerButton {
                    pos,
                    button,
                    pressed: true,
                    modifiers,
                });
            }
        }
        if input.scroll_delta != vocab::Vec2::ZERO {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(input.scroll_delta.x, input.scroll_delta.y),
                // The host has already smoothed the delta; the sub-context
                // only needs a phase it can treat as an ordinary scroll.
                phase: egui::TouchPhase::Move,
                modifiers,
            });
        }
        events
    }

    /// (Re)allocate the render target when the pixel size changes, and
    /// register it with the PARENT renderer so the parent can sample it.
    fn ensure_target(
        surface: &mut OffscreenSurface,
        render_state: &egui_wgpu::RenderState,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        size_pixels: [u32; 2],
    ) {
        if surface
            .target
            .as_ref()
            .is_some_and(|target| target.size_pixels == size_pixels)
        {
            return;
        }
        // Release the old registration before allocating, so the parent
        // renderer's texture map does not accumulate dead entries.
        if let Some(old) = surface.target.take()
            && let Some(parent_texture) = old.parent_texture
        {
            render_state.renderer.write().free_texture(&parent_texture);
        }

        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mara_offscreen_target"),
            size: wgpu::Extent3d {
                width: size_pixels[0].max(1),
                height: size_pixels[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let parent_texture = render_state.renderer.write().register_native_texture(
            device,
            &view,
            wgpu::FilterMode::Linear,
        );
        surface.target = Some(OffscreenTarget {
            texture,
            view,
            size_pixels,
            parent_texture: Some(parent_texture),
        });
    }
}

#[cfg(feature = "gpu")]
pub(crate) use offscreen::{OffscreenInput, render_offscreen};

// ─── The context seam (PLAN.md WS-E3) ─────────────────────────────
//
// Lives here, like the offscreen renderer, because it is backend code:
// the ratchet accounts for egui coupling under `backend/`, and this is
// the one place `MaraCtx` is allowed to know what a context is.

/// Mara's own state map, carried by the backend context.
///
/// egui's store is generic on read — `get_temp::<T>` needs the type at
/// the call site — so no erased `dyn` store can wrap it: there is no
/// way to ask it for a value without naming the type. Owning the map
/// instead and keeping one handle in the context sidesteps that
/// entirely: egui carries it, Mara reads it.
///
/// Nothing is given up by not using egui's store. This workspace
/// builds egui without `persistence`, so its store is in-memory too —
/// `persisted` and `temp` differ only in whether a value survives a
/// sweep, which this map tracks itself with the flag in its key.
#[derive(Clone, Default)]
struct MaraStateMap(
    std::sync::Arc<
        std::sync::Mutex<
            std::collections::HashMap<
                (vocab::Id, bool, std::any::TypeId),
                crate::memory::StateCell,
            >,
        >,
    >,
);

fn mara_state_map_key() -> egui::Id {
    egui::Id::new("mara_state_map")
}

/// The context's state map, created on first use.
///
/// Clones the handle out from under egui's lock rather than working
/// inside it — the two locks are never held at once, so a read that
/// happens to trigger another read cannot deadlock.
fn mara_state_map(ctx: &egui::Context) -> MaraStateMap {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<MaraStateMap>(mara_state_map_key())
            .clone()
    })
}

impl crate::memory::MaraStore for EguiCtx {
    fn get_any(
        &self,
        id: vocab::Id,
        persisted: bool,
        ty: std::any::TypeId,
    ) -> Option<crate::memory::StateCell> {
        let map = mara_state_map(self);
        let guard = map.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.get(&(id, persisted, ty)).cloned()
    }

    fn set_any(
        &self,
        id: vocab::Id,
        persisted: bool,
        ty: std::any::TypeId,
        value: crate::memory::StateCell,
    ) {
        let map = mara_state_map(self);
        let mut guard = map.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.insert((id, persisted, ty), value);
    }

    fn remove_any(&self, id: vocab::Id, ty: std::any::TypeId) {
        let map = mara_state_map(self);
        let mut guard = map.0.lock().unwrap_or_else(|e| e.into_inner());
        guard.remove(&(id, false, ty));
    }

    fn animate_bool(&self, id: vocab::Id, value: bool, animation_time: f32) -> f32 {
        self.animate_bool_with_time(id.into(), value, animation_time)
    }

    fn animate_value(&self, id: vocab::Id, target: f32, animation_time: f32) -> f32 {
        self.animate_value_with_time(id.into(), target, animation_time)
    }

    fn animate_bool_responsive(&self, id: vocab::Id, value: bool) -> f32 {
        egui::Context::animate_bool_responsive(self, id.into(), value)
    }

    fn pass_nr(&self) -> u64 {
        self.cumulative_pass_nr()
    }
}

impl crate::context::MaraCtx for EguiCtx {
    fn input(&self) -> MaraInput {
        input_snapshot(self)
    }

    fn pass_nr(&self) -> u64 {
        self.cumulative_pass_nr()
    }

    fn content_rect(&self) -> vocab::Rect {
        context_content_rect(self)
    }

    fn pixels_per_point(&self) -> f32 {
        context_pixels_per_point(self)
    }

    fn request_repaint(&self) {
        request_repaint(self);
    }

    fn request_repaint_after(&self, after: std::time::Duration) {
        request_repaint_after(self, after);
    }

    fn now(&self) -> f64 {
        input_time(self)
    }

    fn dt(&self) -> f32 {
        unstable_dt(self)
    }

    fn area(
        &self,
        host: crate::layout::AreaHost,
        body: &mut dyn FnMut(&mut crate::MaraUi<'_>),
    ) -> vocab::Rect {
        let accent = host.accent.unwrap_or_else(crate::style::active_accent);
        show_area_for_host(self, host, |ui| {
            let mut backend = EguiUiBackend::new(ui);
            let mut mara = crate::MaraUi::over(&mut backend, accent);
            body(&mut mara);
        })
        .response
        .rect
        .into()
    }

    fn area_slot(
        &self,
        spec: crate::layout::AreaSlotSpec,
        body: &mut dyn FnMut(&mut crate::MaraUi<'_>),
    ) -> vocab::Rect {
        let accent = spec.host.accent.unwrap_or_else(crate::style::active_accent);
        show_area_slot(self, spec, |ui| {
            let mut backend = EguiUiBackend::new(ui);
            let mut mara = crate::MaraUi::over(&mut backend, accent);
            body(&mut mara);
        })
        .response
        .rect
        .into()
    }

    fn layer_painter(
        &self,
        layer: crate::layout::Layer,
        id: vocab::Id,
        clip: vocab::Rect,
    ) -> crate::MaraPainter {
        crate::MaraPainter::from_sink(Box::new(EguiSink(area_registered_painter(
            self, layer, id, clip,
        ))))
    }

    fn set_cursor_icon(&self, cursor: CursorIcon) {
        set_cursor_icon_for_context(self, cursor);
    }

    fn window_rect(&self) -> vocab::Rect {
        self.viewport_rect().into()
    }

    fn consume_keys(&self, keys: &[MaraKey]) -> Vec<MaraKey> {
        keys.iter()
            .copied()
            .filter(|k| consume_key(self, *k))
            .collect()
    }

    fn viewport_maximized(&self) -> bool {
        viewport_maximized(self)
    }

    fn probe_enabled(&self) -> bool {
        probe_enabled(self)
    }

    fn probe_record(&self, pose: crate::probe::ElementPose) {
        probe_record(self, pose);
    }

    fn probe_set_enabled(&self, on: bool) {
        probe_set_enabled(self, on);
    }

    fn probe_drain(&self) -> Vec<crate::probe::ElementPose> {
        probe_drain(self)
    }

    fn enforce_defaults(&self) {
        crate::enforce::__internal_enforce_defaults(self);
    }

    fn load_texture(
        &self,
        name: &str,
        image: vocab::ColorImage,
        options: vocab::TextureOptions,
    ) -> Option<vocab::TextureHandle> {
        let image: egui::ColorImage = image.into();
        Some(self.0.load_texture(name, image, options.into()).into())
    }

    fn memory(&self) -> MaraMemoryCtx<'_> {
        MaraMemoryCtx::__internal_from_backend_ctx(self)
    }

    fn boxed_clone(&self) -> Box<dyn crate::context::MaraCtx + '_> {
        Box::new(self.clone())
    }
}
