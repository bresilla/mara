//! Egui backend adapter.

use egui::{FontId, layers::ShapeIdx};

use crate::{
    layout::{
        AreaHost, AreaSlotSpec, CanvasRectSpec, CanvasSlotSpec, ChildRegion, ColorPickerAlpha,
        ContainerBodySpec, CursorIcon, FrameHostSpec, IndentedBodySpec, InlinePickerSpec,
        ItemSpacingSpec, Layer, PaintSurfaceRegion, PaintSurfaceSpec, PaneBodyScrollAxis,
        PaneBodyScrollSpec, PaneFlexSpec, PopupAlign, PopupListSpec, PopupSpec, PopupTrigger,
        ScrollAxis, ScrollRegion, Sense, SlotRibbonLayoutSpec, SpaceSpec, StackAlign,
        StackDirection, StackScopeSpec, TextEditRegion, TextEditSpec, TextMeasureSpec, UiBackend,
    },
    memory::MaraMemoryCtx,
    mui::{MaraInput, MaraKey, MaraResponse},
    paint::{PaintCmd, TextFamily, TextRun},
    vocab,
};

pub(crate) fn egui_frame_for_style_spec(spec: crate::style::FrameSpec) -> egui::Frame {
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
        .shadow(shadow)
}

pub(crate) struct EguiUiBackend<'a> {
    ui: &'a mut egui::Ui,
    clip_stack: Vec<egui::Rect>,
}

pub(crate) struct DeferredPaintSlot(ShapeIdx);

impl<'a> EguiUiBackend<'a> {
    pub(crate) fn new(ui: &'a mut egui::Ui) -> Self {
        Self {
            ui,
            clip_stack: Vec::new(),
        }
    }

    /// Shared access to the hosted egui `Ui`. This is the migration
    /// seam: `MaraUi` and first-party module code reach the concrete
    /// backend `Ui` through here while individual operations have not
    /// yet been promoted to `UiBackend`/spec contracts.
    pub(crate) fn ui(&self) -> &egui::Ui {
        &*self.ui
    }

    /// Mutable access to the hosted egui `Ui`. See [`Self::ui`].
    pub(crate) fn ui_mut(&mut self) -> &mut egui::Ui {
        &mut *self.ui
    }
}

pub(crate) fn reserve_deferred_paint_cmd_slot(ui: &mut egui::Ui) -> DeferredPaintSlot {
    DeferredPaintSlot(ui.painter().add(egui::Shape::Noop))
}

pub(crate) fn fill_deferred_paint_cmd_slot(
    ui: &egui::Ui,
    slot: DeferredPaintSlot,
    cmd: Option<PaintCmd>,
) {
    let shape = cmd.map(shape_from_paint_cmd).unwrap_or(egui::Shape::Noop);
    ui.painter().set(slot.0, shape);
}

pub(crate) fn paint_cmd_for_ui(ui: &egui::Ui, cmd: PaintCmd) {
    render_paint_cmd(ui.painter(), cmd);
}

pub(crate) fn paint_cmds_for_ui(ui: &egui::Ui, commands: impl IntoIterator<Item = PaintCmd>) {
    for cmd in commands {
        paint_cmd_for_ui(ui, cmd);
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

pub(crate) fn measure_text_runs_for_ui(ui: &egui::Ui, runs: &[TextRun]) -> vocab::Vec2 {
    let Some((job, _fallback_color)) = layout_job_for_text_runs(ui.painter(), runs) else {
        return vocab::Vec2::ZERO;
    };
    ui.painter().layout_job(job).size().into()
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
        let mara: MaraResponse = response.into();
        probe_record_response(self.ui.ctx(), "alloc", None, &mara);
        mara
    }

    fn reserve_space(&mut self, size: vocab::Vec2) -> vocab::Rect {
        self.ui.allocate_space(size.into()).1.into()
    }

    fn reserve_rect(&mut self, rect: vocab::Rect, sense: Sense) -> MaraResponse {
        let mara: MaraResponse = self.ui.allocate_rect(rect.into(), egui_sense(sense)).into();
        probe_record_response(self.ui.ctx(), "reserve", None, &mara);
        mara
    }

    fn interact(&mut self, rect: vocab::Rect, id: vocab::Id, sense: Sense) -> MaraResponse {
        let mara: MaraResponse = self
            .ui
            .interact(rect.into(), id.into(), egui_sense(sense))
            .into();
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
        MaraKey::Enter => egui::Key::Enter,
    }
}

pub(crate) fn consume_key(ctx: &egui::Context, key: MaraKey) -> bool {
    ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui_key(key)))
}

pub(crate) fn consume_keys<const N: usize>(
    ctx: &egui::Context,
    keys: [MaraKey; N],
) -> Vec<MaraKey> {
    keys.into_iter()
        .filter(|key| consume_key(ctx, *key))
        .collect()
}

pub(crate) fn key_pressed(ctx: &egui::Context, key: MaraKey) -> bool {
    ctx.input(|input| input.key_pressed(egui_key(key)))
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

pub(crate) fn request_repaint_after_ms(ctx: &egui::Context, ms: u64) {
    ctx.request_repaint_after(std::time::Duration::from_millis(ms));
}

pub(crate) fn unstable_dt(ctx: &egui::Context) -> f32 {
    ctx.input(|input| input.unstable_dt).max(0.0)
}

pub(crate) fn animate_value_with_time(
    ctx: &egui::Context,
    id: impl Into<vocab::Id>,
    target: f32,
    animation_time: f32,
) -> f32 {
    ctx.animate_value_with_time(id.into().into(), target, animation_time)
}

pub(crate) fn animate_bool_with_time(
    ctx: &egui::Context,
    id: impl Into<vocab::Id>,
    value: bool,
    animation_time: f32,
) -> f32 {
    ctx.animate_bool_with_time(id.into().into(), value, animation_time)
}

pub(crate) fn animate_bool_with_time_for_ui(
    ui: &egui::Ui,
    id: impl Into<vocab::Id>,
    value: bool,
    animation_time: f32,
) -> f32 {
    animate_bool_with_time(ui.ctx(), id, value, animation_time)
}

pub(crate) fn context_for_ui(ui: &egui::Ui) -> egui::Context {
    ui.ctx().clone()
}

pub(crate) fn animate_bool_responsive(
    ctx: &egui::Context,
    id: impl Into<vocab::Id>,
    value: bool,
) -> f32 {
    ctx.animate_bool_responsive(id.into().into(), value)
}

pub(crate) fn animate_bool_responsive_for_ui(
    ui: &egui::Ui,
    id: impl Into<vocab::Id>,
    value: bool,
) -> f32 {
    animate_bool_responsive(ui.ctx(), id, value)
}

pub(crate) fn context_painter_for_layer(
    ctx: &egui::Context,
    layer: Layer,
    id: vocab::Id,
    clip: vocab::Rect,
) -> egui::Painter {
    egui::Painter::new(
        ctx.clone(),
        egui::LayerId::new(egui_order_for_layer(layer), id.into()),
        clip.into(),
    )
}

pub(crate) fn pointer_interact_pos(ctx: &egui::Context) -> Option<vocab::Pos2> {
    ctx.pointer_interact_pos().map(Into::into)
}

pub(crate) fn pointer_latest_pos(ctx: &egui::Context) -> Option<vocab::Pos2> {
    ctx.pointer_latest_pos().map(Into::into)
}

pub(crate) fn pointer_any_released(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.pointer.any_released())
}

pub(crate) fn primary_pointer_pressed_interact_pos(ctx: &egui::Context) -> Option<vocab::Pos2> {
    ctx.input(|input| {
        if input.pointer.button_pressed(egui::PointerButton::Primary) {
            input.pointer.interact_pos().map(Into::into)
        } else {
            None
        }
    })
}

pub(crate) fn viewport_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|input| input.viewport().maximized)
        .unwrap_or(false)
}

pub(crate) fn color32_for_backend(color: vocab::Color32) -> egui::Color32 {
    color.into()
}

pub(crate) fn show_color_picker_for_ui(
    ui: &mut egui::Ui,
    color: &mut vocab::Color32,
    alpha: ColorPickerAlpha,
) -> bool {
    let mut backend_color: egui::Color32 = (*color).into();
    let alpha = match alpha {
        ColorPickerAlpha::Opaque => egui::color_picker::Alpha::Opaque,
        ColorPickerAlpha::OnlyBlend => egui::color_picker::Alpha::OnlyBlend,
    };
    let changed = egui::color_picker::color_picker_color32(ui, &mut backend_color, alpha);
    if changed {
        *color = backend_color.into();
    }
    changed
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
    egui::Area::new(host.id.into())
        .order(egui_order_for_layer(host.layer))
        .fixed_pos(Into::<egui::Pos2>::into(host.pos))
        .interactable(host.interactable)
}

pub(crate) fn show_area_for_host<R>(
    ctx: &egui::Context,
    host: AreaHost,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let host_id = host.id;
    let host_layer = host.layer;
    let inner = area_for_host(host).show(ctx, body);
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
    area.id_salt(Into::<egui::Id>::into(region.id))
        .auto_shrink(region.auto_shrink)
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

pub(crate) fn show_child_sticky_scroll_region<R>(
    ui: &mut egui::Ui,
    child_region: ChildRegion,
    scroll_region: ScrollRegion,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::containers::scroll_area::ScrollAreaOutput<R> {
    let mut child = child_ui_for_region(ui, child_region);
    apply_scroll_region_spacing(&mut child, scroll_region);
    let scroll_area = scroll_area_for_region(scroll_region);
    crate::scroll::show_sticky_scroll_area(
        &mut child,
        sticky_scroll_axis_for_region(scroll_region),
        scroll_area,
        |ui| {
            apply_scroll_region_spacing(ui, scroll_region);
            body(ui)
        },
    )
}

fn sticky_scroll_axis_for_region(region: ScrollRegion) -> crate::scroll::StickyScrollAxis {
    match region.axis {
        ScrollAxis::Horizontal => crate::scroll::StickyScrollAxis::Horizontal,
        ScrollAxis::Vertical => crate::scroll::StickyScrollAxis::Vertical,
    }
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

pub(crate) fn add_pane_body_gap(ui: &mut egui::Ui, spec: PaneFlexSpec) {
    if spec.body_gap > 0.0 {
        add_space_for_spec(ui, SpaceSpec::vertical(spec.body_gap));
    }
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

pub(crate) fn show_slot_ribbon_area<R>(
    ctx: &egui::Context,
    spec: SlotRibbonLayoutSpec,
    layer: Layer,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    show_slot_ribbon_area_with_interactivity(ctx, spec, layer, true, body)
}

pub(crate) fn show_slot_ribbon_area_with_interactivity<R>(
    ctx: &egui::Context,
    spec: SlotRibbonLayoutSpec,
    layer: Layer,
    interactable: bool,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    let host = AreaHost::new(spec.id, spec.pos, layer);
    let host = if interactable {
        host
    } else {
        host.non_interactive()
    };
    show_area_slot(ctx, AreaSlotSpec::new(host, spec.size), body)
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
    egui::TextEdit::singleline(text)
        .desired_width(spec.region.desired_width())
        .frame(spec.frame)
        .hint_text(hint_text)
        .text_color(spec.text_color.into())
        .background_color(spec.background_color.into())
        .font(egui::FontId::proportional(spec.region.font_size))
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
        response: response.into(),
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

pub(crate) fn is_ui_rect_visible(ui: &egui::Ui, rect: vocab::Rect) -> bool {
    ui.is_rect_visible(rect.into())
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

pub(crate) fn show_indented_body_for_spec<R>(
    ui: &mut egui::Ui,
    spec: IndentedBodySpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<R> {
    ui.indent(Into::<egui::Id>::into(spec.id), body)
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

pub(crate) fn show_with_deferred_paint_cmd_slots<R, C>(
    ui: &mut egui::Ui,
    slot_count: usize,
    body: impl FnOnce(&mut egui::Ui) -> (R, C),
) -> R
where
    C: IntoIterator<Item = PaintCmd>,
{
    let slots: Vec<_> = (0..slot_count)
        .map(|_| ui.painter().add(egui::Shape::Noop))
        .collect();
    let (output, commands) = body(ui);
    for (slot, cmd) in slots.into_iter().zip(commands) {
        ui.painter().set(slot, shape_from_paint_cmd(cmd));
    }
    output
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

pub(crate) fn show_stack_scope_for_ui<R>(
    ui: &mut egui::Ui,
    spec: StackScopeSpec,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    match spec.direction {
        StackDirection::LeftToRight => ui.horizontal(body).inner,
        StackDirection::TopDown => ui.vertical(body).inner,
        StackDirection::RightToLeft | StackDirection::BottomUp => {
            ui.with_layout(
                egui_layout_for_stack_direction(spec.direction, StackAlign::Min),
                body,
            )
            .inner
        }
    }
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

pub(crate) fn child_ui_with_current_layout_for_rect(
    ui: &mut egui::Ui,
    rect: vocab::Rect,
) -> egui::Ui {
    ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.into())
            .layout(*ui.layout()),
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

pub(crate) fn interact_for_ui_rect(
    ui: &mut egui::Ui,
    rect: vocab::Rect,
    id: vocab::Id,
    sense: Sense,
) -> MaraResponse {
    ui.interact(rect.into(), id.into(), egui_sense(sense))
        .into()
}

pub(crate) fn allocate_canvas_slot_for_ui(
    ui: &mut egui::Ui,
    spec: CanvasSlotSpec,
) -> (egui::Painter, MaraResponse) {
    let response = {
        let mut backend = EguiUiBackend::new(ui);
        backend.allocate(spec.size, spec.sense)
    };
    let painter = painter_for_ui_rect(ui, response.rect);
    (painter, response)
}

pub(crate) fn interact_canvas_rect_for_ui(
    ui: &mut egui::Ui,
    spec: CanvasRectSpec,
) -> (egui::Painter, MaraResponse) {
    let response = interact_for_ui_rect(ui, spec.rect, spec.id, spec.sense);
    let painter = painter_for_ui_clip(ui, spec.rect);
    (painter, response)
}

pub(crate) fn painter_for_ui_rect(ui: &egui::Ui, rect: vocab::Rect) -> egui::Painter {
    let rect: egui::Rect = rect.into();
    ui.painter_at(rect)
        .with_clip_rect(rect.intersect(ui.clip_rect()))
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

pub(crate) fn hover_text_for_ui_response(ui: &egui::Ui, response: &MaraResponse, text: &str) {
    hover_text(ui.ctx(), response.backend_response_id(), text);
}

pub(crate) fn move_area_response_to_top(ctx: &egui::Context, response: &egui::Response) {
    ctx.move_to_top(response.layer_id);
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
        scroll_delta: i.smooth_scroll_delta.into(),
        pointer_delta: i.pointer.delta().into(),
        zoom_delta: i.zoom_delta(),
        modifiers_shift: i.modifiers.shift,
        modifiers_ctrl: i.modifiers.ctrl,
        modifiers_alt: i.modifiers.alt,
    })
}

pub(crate) fn input_snapshot_for_ui(ui: &egui::Ui) -> MaraInput {
    input_snapshot(ui.ctx())
}

pub(crate) fn memory_ctx_for_ui(ui: &egui::Ui) -> MaraMemoryCtx<'_> {
    MaraMemoryCtx::new(ui.ctx())
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

pub(crate) fn render_paint_cmd(painter: &egui::Painter, cmd: PaintCmd) {
    match cmd {
        PaintCmd::Ellipse { rect, fill, stroke } => {
            let rect: egui::Rect = rect.into();
            painter.add(egui::epaint::EllipseShape {
                center: rect.center(),
                radius: rect.size() / 2.0,
                fill: fill.into(),
                stroke: Into::<egui::Stroke>::into(stroke),
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
        PaintCmd::Svg { .. } => {}
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
        PaintCmd::Clip { rect, children } => {
            let clipped = painter_with_clip(painter, rect);
            for child in children {
                render_paint_cmd(&clipped, child);
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
        | PaintCmd::Clip { .. } => egui::Shape::Noop,
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

fn svg_stable_hash(svg: &str) -> u64 {
    let mut h: u64 = 5381;
    for b in svg.as_bytes() {
        h = h.wrapping_mul(33).wrapping_add(u64::from(*b));
    }
    h
}

fn egui_mesh_from_mara(vertices: Vec<crate::paint::PaintVertex>, indices: Vec<u32>) -> egui::Mesh {
    let mut mesh = egui::Mesh::default();
    mesh.indices = indices;
    mesh.vertices = vertices
        .into_iter()
        .map(|vertex| egui::epaint::Vertex {
            pos: vertex.pos.into(),
            uv: egui::epaint::WHITE_UV,
            color: vertex.color.into(),
        })
        .collect();
    mesh
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
    fn context_painter_for_layer_uses_mara_layer_and_clip() {
        let ctx = egui::Context::default();
        let clip = Rect::from_min_size(Pos2::new(1.0, 2.0), Vec2::new(30.0, 10.0));

        assert_eq!(
            egui_order_for_layer(Layer::Foreground),
            egui::Order::Foreground
        );

        let painter =
            context_painter_for_layer(&ctx, Layer::Foreground, vocab::Id::new("test"), clip);

        assert_eq!(painter_clip_rect(&painter), clip);
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
