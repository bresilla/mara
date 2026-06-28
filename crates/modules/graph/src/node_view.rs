//! Sharp-zoom node graph host. Renders a widget into a SECONDARY
//! `egui::Context` whose `pixels_per_point` compensates for zoom,
//! so glyph rasterisation + shape edges always land on physical
//! pixels at the correct density. The secondary context's
//! tessellated output is rendered to a wgpu texture by `egui-wgpu`,
//! and that texture is composited back into the parent UI as an
//! `egui::Image`.
//!
//! ## Why
//!
//! `egui_graph` (and `egui_node_graph`) scale the entire layout —
//! positions, font sizes, stroke widths — by a `zoom` factor while
//! rendering inside the parent egui context. Because
//! `pixels_per_point` is per-context (not per-widget), shape edges
//! end up on non-integer pixel boundaries and glyphs are
//! rasterised at the parent's density even when the user expects
//! a sharper "zoomed in" view. Result: blur.
//!
//! Blackjack solves this by spinning a SEPARATE `egui::Context`
//! per graph editor and tweaking that context's `pixels_per_point`
//! to mirror the inverse of the zoom level. We follow the same
//! recipe here, but generalise the rendering hook into a backend
//! trait so the same code can run under different wgpu-backed
//! Mara hosts without tying the graph to a specific window owner.
//!
//! ## Lifecycle
//!
//! 1. The host (Bevy plugin or eframe app) creates one
//!    [`NodeViewState`] per graph instance and a single
//!    [`NodeViewBackend`] handle that owns the `wgpu::Device` /
//!    `wgpu::Queue` + the parent egui renderer hooks.
//! 2. Each frame the host calls a render function that takes the
//!    state + backend + viewer (e.g.
//!    `bevy_mara::extras::mara_node_graph`) inside an `egui::Ui`.
//! 3. That function configures input for the secondary context
//!    (forwarding pointer + key events from the parent), runs the
//!    secondary context's frame, tessellates, asks the backend to
//!    paint into a wgpu texture, and composites the texture into
//!    the parent UI.

use egui::epaint::textures::TexturesDelta;
use egui::{ClippedPrimitive, Color32, Rect, Sense, Ui, Vec2};

/// Owns the secondary `egui::Context`, the wgpu render target the
/// graph paints into, and the `egui_wgpu::Renderer` that drives
/// that paint. One instance per graph instance.
///
/// `zoom` is the user-facing camera state — natural convention:
/// `1.0` = no zoom, `2.0` = zoomed in 2×, `0.5` = zoomed out 2×.
/// Pan is owned by the embedded `GraphWidget`'s own
/// `TSTransform.translation`; we drive only the zoom axis from
/// outside the widget (see `node_view::show` for why).
pub struct NodeViewState {
    /// Independent egui context, one per graph editor instance.
    /// Gets its own font atlas + tessellator + texture manager
    /// (`pixels_per_point` is per-context, so we need a fresh one
    /// to compensate for zoom independently of the host's UI).
    sub_ctx: egui::Context,
    /// Reserved for future use — currently unread. The visible pan
    /// lives in `GraphWidget`'s `TSTransform.translation` instead.
    pan: Vec2,
    /// Visible zoom factor — the value driving sub_ppp +
    /// screen_rect each frame. Smoothly chases `zoom_target` so a
    /// wheel notch produces an animated zoom rather than a jump
    /// (the discrete jump version felt janky because each notch
    /// also nudged graph's translation by a one-shot delta;
    /// interpolating the zoom and applying a translation delta
    /// per smoothing frame instead makes the cursor anchor and
    /// the visual zoom move in lockstep at sub-frame granularity).
    zoom: f32,
    /// Where `zoom` is heading. Wheel events update this
    /// immediately; `zoom` interpolates toward it.
    zoom_target: f32,
    /// Last allocated render target. `None` on first frame and
    /// recreated whenever the requested pixel size changes.
    target: Option<NodeViewTarget>,
    /// `egui_wgpu::Renderer` that draws the secondary context's
    /// tessellated output into `target.view`. Created lazily on
    /// the first render so we can borrow the backend's device +
    /// queue at that point.
    renderer: Option<egui_wgpu::Renderer>,
    /// One-shot gate flipped by [`NodeViewState::take_first_frame`].
    /// Hosts use it to run sub-context setup (font install, theme
    /// apply) exactly once per state instance — see the `maraui`
    /// graph wrapper for the canonical usage.
    fonts_installed: bool,
}

struct NodeViewTarget {
    /// Owns the wgpu resource — the `view` field is just a borrow
    /// of this. Held so the texture isn't dropped while the view
    /// (and the parent's registered TextureId) still references it.
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    size_pixels: [u32; 2],
    /// Texture id this view is registered as in the PARENT egui
    /// context, ready to drop into a `egui::Image`.
    parent_tex_id: Option<egui::TextureId>,
}

impl Default for NodeViewState {
    fn default() -> Self {
        Self::new()
    }
}

impl NodeViewState {
    pub fn new() -> Self {
        Self {
            sub_ctx: egui::Context::default(),
            pan: Vec2::ZERO,
            zoom: 1.0,
            zoom_target: 1.0,
            target: None,
            renderer: None,
            fonts_installed: false,
        }
    }

    /// First-party hook for the secondary `egui::Context`.
    ///
    /// The sharp-zoom renderer is still egui-backed internally, but
    /// ordinary graph consumers should go through Mara's graph
    /// wrapper instead of configuring the raw sub-context directly.
    #[doc(hidden)]
    pub fn __internal_ctx(&self) -> &egui::Context {
        &self.sub_ctx
    }

    /// Returns `true` exactly once per state instance — on the
    /// caller's first frame. Subsequent calls return `false`. Used
    /// by hosts to gate one-shot sub-context setup like font
    /// installation:
    ///
    /// ```ignore
    /// if state.take_first_frame() {
    ///     maraui::style::__internal_install_fonts(state.__internal_ctx(), ...);
    /// }
    /// maraui::style::__internal_apply_theme_to(state.__internal_ctx(), ...);
    /// mara_graph::show_with_anchor(...);
    /// ```
    pub fn take_first_frame(&mut self) -> bool {
        let v = !self.fonts_installed;
        self.fonts_installed = true;
        v
    }

    pub fn pan(&self) -> Vec2 {
        self.pan
    }

    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    pub fn set_pan(&mut self, p: Vec2) {
        self.pan = p;
    }

    /// Clamped to `[0.1, 10.0]` so the user can't zoom into a
    /// degenerate state. Sets BOTH the visible zoom and the
    /// smoothing target (so external callers — e.g. the
    /// resize-fit reset in `mara_node_graph_with_opts` — snap
    /// immediately rather than animating to the new value).
    pub fn set_zoom(&mut self, z: f32) {
        let clamped = z.clamp(0.1, 10.0);
        self.zoom = clamped;
        self.zoom_target = clamped;
    }

    /// Adjust zoom multiplicatively. `factor > 1` zooms IN
    /// (matches the natural-convention `zoom` field). The
    /// `anchor_in_graph` arg is currently unused — anchoring at
    /// cursor would require nudging the embedded widget's
    /// `TSTransform.translation`, which lives outside this state.
    pub fn adjust_zoom(&mut self, factor: f32, _anchor_in_graph: Vec2) {
        self.set_zoom(self.zoom * factor);
    }

    /// Drop any cached wgpu resources. Called when the graph
    /// instance is being torn down, or when the host wants to
    /// force a re-allocation on the next frame.
    pub fn release(&mut self, backend: &mut dyn NodeViewBackend) {
        if let Some(t) = self.target.take()
            && let Some(id) = t.parent_tex_id
        {
            backend.unregister_native(id);
        }
        self.renderer = None;
    }

    /// (Re)allocate the wgpu render target if the requested pixel
    /// size doesn't match the cached one. Returns `true` when a
    /// new target was created (= caller should reupload).
    fn ensure_target(&mut self, backend: &mut dyn NodeViewBackend, size_pixels: [u32; 2]) -> bool {
        let need_new = match &self.target {
            None => true,
            Some(t) => t.size_pixels != size_pixels,
        };
        if !need_new {
            return false;
        }
        // Drop the old texture (and its parent registration) before
        // allocating a new one — keeps the renderer's internal
        // texture map tidy.
        if let Some(old) = self.target.take()
            && let Some(id) = old.parent_tex_id
        {
            backend.unregister_native(id);
        }
        let (device, _queue) = backend.wgpu();
        let format = backend.target_format();
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mara_node_view_target"),
            size: wgpu::Extent3d {
                width: size_pixels[0].max(1),
                height: size_pixels[1].max(1),
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            // `RENDER_ATTACHMENT` for egui-wgpu's draw pass,
            // `TEXTURE_BINDING` so the eframe path can sample it
            // directly, `COPY_SRC` so the Bevy backend can copy
            // this texture's contents into a Bevy `Image` asset
            // each frame (the eframe path doesn't use this; wgpu
            // ignores unused usage flags).
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let parent_tex_id =
            backend.register_native(&texture, &view, size_pixels, wgpu::FilterMode::Linear);
        self.target = Some(NodeViewTarget {
            texture,
            view,
            size_pixels,
            parent_tex_id: Some(parent_tex_id),
        });
        true
    }

    /// Lazily build the `egui_wgpu::Renderer` once we have a
    /// device + format. Output format MUST match the parent
    /// renderer's so the texture composites without a colour-space
    /// mismatch.
    fn ensure_renderer(&mut self, backend: &mut dyn NodeViewBackend) {
        if self.renderer.is_some() {
            return;
        }
        let (device, _queue) = backend.wgpu();
        let format = backend.target_format();
        // `msaa_samples = 1` matches our render target's sample count.
        // `dithering = false` keeps shape colour exact across formats.
        self.renderer = Some(egui_wgpu::Renderer::new(
            &device,
            format,
            egui_wgpu::RendererOptions {
                msaa_samples: 1,
                depth_stencil_format: None,
                dithering: false,
                predictable_texture_filtering: false,
            },
        ));
    }
}

/// Host-supplied hooks the mara_core-side widget calls to talk to
/// the wgpu side. Host integrations provide implementations such as
/// `bevy_mara::extras::BevyNodeViewBackend` or
/// `mara::host::EframeNodeViewBackend`.
pub trait NodeViewBackend {
    /// `wgpu::Device` + `wgpu::Queue` used to allocate the
    /// offscreen texture and submit the secondary context's
    /// render pass. wgpu 27's `Device`/`Queue` are cheap to clone
    /// (internally `Arc`-counted), so callers return clones.
    fn wgpu(&self) -> (wgpu::Device, wgpu::Queue);

    /// The texture format the PARENT egui renderer outputs to —
    /// the offscreen target uses the same format so colours match
    /// when the texture is drawn as `egui::Image` in the parent UI.
    fn target_format(&self) -> wgpu::TextureFormat;

    /// Register a `wgpu::TextureView` with the parent context's
    /// egui renderer so the parent UI can sample it via the
    /// returned `egui::TextureId`. `size_pixels` is the texture's
    /// physical dimensions — backends that can't query a view's
    /// size (e.g. Bevy needs it to allocate a matching `Image`
    /// asset) read it from here. The actual `wgpu::Texture` the
    /// view aliases is also passed as `texture` so the backend
    /// can keep a clone for cross-world / deferred-render flows
    /// (Bevy's render world copies from this texture into a
    /// Bevy-owned `Image` asset's GpuImage each frame).
    fn register_native(
        &mut self,
        texture: &wgpu::Texture,
        view: &wgpu::TextureView,
        size_pixels: [u32; 2],
        filter: wgpu::FilterMode,
    ) -> egui::TextureId;

    /// Release a previously-registered texture id.
    fn unregister_native(&mut self, id: egui::TextureId);

    /// Hook called AFTER each frame's `egui-wgpu` render pass
    /// finishes writing into the mara_core-allocated wgpu texture.
    /// Default impl is a no-op (eframe doesn't need it — the
    /// render pass writes into the same texture egui samples
    /// directly). Hosts with separate render worlds can override
    /// this to queue a copy into their own texture asset.
    fn after_render(
        &mut self,
        _texture: &wgpu::Texture,
        _tex_id: egui::TextureId,
        _size_pixels: [u32; 2],
    ) {
    }
}

/// Render the secondary context's already-tessellated output into
/// the state's wgpu render target. Internal to this module — the
/// public widget calls it after running its sub-context frame.
fn render_into_target(
    state: &mut NodeViewState,
    backend: &mut dyn NodeViewBackend,
    primitives: Vec<ClippedPrimitive>,
    textures_delta: TexturesDelta,
    size_pixels: [u32; 2],
    pixels_per_point: f32,
) {
    let (device, queue) = backend.wgpu();
    let target = state
        .target
        .as_ref()
        .expect("ensure_target must run before render_into_target");
    let renderer = state
        .renderer
        .as_mut()
        .expect("ensure_renderer must run before render_into_target");
    // Apply texture deltas (egui's atlas updates) before issuing the
    // render pass — the renderer needs all the new glyph atlases
    // available before it tessellates draw calls.
    for (id, image_delta) in &textures_delta.set {
        renderer.update_texture(&device, &queue, *id, image_delta);
    }
    let screen_descriptor = egui_wgpu::ScreenDescriptor {
        size_in_pixels: size_pixels,
        pixels_per_point,
    };
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("mara_node_view_encoder"),
    });
    let _cmd_buffers = renderer.update_buffers(
        &device,
        &queue,
        &mut encoder,
        &primitives,
        &screen_descriptor,
    );
    {
        let mut rpass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mara_node_view_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.view,
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
        renderer.render(&mut rpass, &primitives, &screen_descriptor);
    }
    queue.submit(Some(encoder.finish()));
    for id in &textures_delta.free {
        renderer.free_texture(id);
    }
}

/// Run the secondary context's frame, render to the wgpu target,
/// and composite the texture into the parent UI as an `Image`.
///
/// `body` runs INSIDE the secondary context's central panel — it's
/// where the host renders the actual graph widget (e.g.
/// `GraphWidget::show(graph, viewer, ui)`).
pub fn show<R>(
    parent_ui: &mut Ui,
    state: &mut NodeViewState,
    backend: &mut dyn NodeViewBackend,
    desired_size: Vec2,
    body: impl FnOnce(&mut Ui) -> R,
) -> egui::Response {
    show_with_anchor(parent_ui, state, backend, desired_size, |_, _| {}, body)
}

/// Variant of [`show`] with a per-zoom-step anchor callback —
/// invoked AFTER `state.zoom` updates from a wheel event but
/// BEFORE the body runs, so the embedded widget can shift its
/// own pan (e.g. `GraphWidget`'s `TSTransform.translation`) to
/// keep the cursor-under content stationary across the zoom.
///
/// Args passed to `on_zoom_anchor`:
/// * `&egui::Context` — the secondary context the body draws into.
///   Use this to read/write the embedded widget's saved state via
///   ctx data.
/// * `egui::Vec2` — the delta (in sub-context points) by which
///   the cursor's sub-coord position moved as a result of the
///   zoom step. Add this to the embedded widget's pan / offset
///   to anchor at the cursor.
pub fn show_with_anchor<R>(
    parent_ui: &mut Ui,
    state: &mut NodeViewState,
    backend: &mut dyn NodeViewBackend,
    desired_size: Vec2,
    mut on_zoom_anchor: impl FnMut(&egui::Context, egui::Vec2),
    body: impl FnOnce(&mut Ui) -> R,
) -> egui::Response {
    // Sub-context setup (font install, theme bridging) is the
    // caller's responsibility — see [`NodeViewState::take_first_frame`]
    // and [`NodeViewState::__internal_ctx`]. The mara-themed wrapper in
    // `maraui::extras::graph` runs Mara's internal font install
    // + theme-visual hooks against the sub-context before invoking
    // this function; standalone consumers can leave the sub-context
    // with egui's default visuals.

    let parent_ppp = parent_ui.ctx().pixels_per_point();
    let viewport_in_points = desired_size.max(Vec2::new(64.0, 64.0));
    let viewport_in_pixels = (viewport_in_points * parent_ppp).round();
    let size_pixels = [
        (viewport_in_pixels.x as u32).max(1),
        (viewport_in_pixels.y as u32).max(1),
    ];

    // Allocate the canvas region in the parent UI so input + layout
    // see it. Click+drag sense lets us forward pan + node grabs.
    let (rect, response) =
        parent_ui.allocate_exact_size(viewport_in_points, Sense::click_and_drag());

    // ── Outside-in zoom — the sharp-text path ──
    //
    // `state.zoom` is our visible zoom factor (1.0 = no zoom, 2.0
    // = zoomed in 2×). We DRIVE it from the outside instead of
    // letting graph's `TSTransform` scale geometry, because a
    // bitmap glyph atlas stretched past 1× is the source of the
    // bilinear blur we're trying to avoid. Graph's scaling is
    // locked to 1.0 (see `mara_node_graph_style`), so its
    // `register_pan_and_zoom` only updates translation — perfect
    // for drag-pan, but we own the zoom.
    //
    // Two coordinated changes per frame implement zoom:
    //
    //   1. `sub_ppp = parent_ppp × state.zoom` — the secondary
    //      egui context renders at this pixels-per-point. Higher
    //      ppp = atlas is rasterised at MORE pixels per glyph =
    //      sharper text. Set above the parent's ppp (typically 1
    //      on standard-DPI, 1.5+ on HiDPI) and you get crisper
    //      glyphs at the same physical pixel size.
    //   2. `sub_screen_rect` is `(0, 0)` to `size_pixels / sub_ppp`
    //      sub points — i.e. it shrinks as `state.zoom` grows. The
    //      sub context lays out into that smaller logical area, so
    //      the same scene-coord nodes occupy a bigger fraction of
    //      the visible texture: visual zoom in.
    //
    // Net effect: wheel-up grows `state.zoom`, atlas re-rasterises
    // at the new ppp, layout area shrinks, visual zoom-in. Glyphs
    // never get stretched past 1× of their atlas resolution, so
    // they stay sharp at any zoom level.

    // Pointer-over check, and cache the parent's hover position so
    // we can synthesise a `PointerMoved` for the sub context every
    // frame the pointer hovers the rect (egui maintains pointer
    // state across frames, but the sub context starts blind on
    // first paint and won't update its `hover_pos` until it actually
    // sees a `PointerMoved` event — without this synthesis, hover
    // works only on the exact frame the user wiggles the mouse and
    // node hover/click never registers on a static cursor).
    let parent_hover = parent_ui.ctx().input(|i| i.pointer.hover_pos());
    let pointer_over_sub = parent_hover.map(|p| rect.contains(p)).unwrap_or(false);

    // ── Wheel → zoom_target & smooth animation ──
    //
    // Wheel events fire as discrete notches; each one can move the
    // zoom by ~10 %. Applying that as a one-shot jump per notch
    // looks janky, so we let the wheel update only the TARGET
    // zoom and let `state.zoom` chase it exponentially across
    // frames. Each smoothing step also nudges the embedded
    // widget's pan via `on_zoom_anchor` so the cursor's
    // scene-content stays under the cursor for the whole
    // animation, not just the final frame.
    let mut events_for_sub = Vec::new();
    let mut parent_had_motion = false;
    if pointer_over_sub {
        let mut parent_events = parent_ui.ctx().input(|i| i.events.clone());
        parent_had_motion = parent_events
            .iter()
            .any(|e| matches!(e, egui::Event::PointerMoved(_)));
        // Strip mouse-wheel Line/Page events into a zoom-line
        // accumulator; leave touchpad Point events (they pass
        // through as graph pan via smooth_scroll_delta).
        let mut wheel_zoom_lines = 0.0_f32;
        parent_events.retain(|e| match e {
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page,
                delta,
                ..
            } => {
                wheel_zoom_lines += delta.y;
                false
            }
            _ => true,
        });
        if wheel_zoom_lines.abs() > 0.0 {
            // ~10 % zoom per notch into the target.
            let factor = (wheel_zoom_lines * 0.1).exp();
            state.zoom_target = (state.zoom_target * factor).clamp(0.1, 10.0);
        }
        events_for_sub = parent_events;
    }

    // Smoothing step. Reach 90 % of the remaining gap in ~120 ms;
    // gives a buttery feel without lagging behind several wheel
    // notches.
    let dt = parent_ui.ctx().input(|i| i.predicted_dt).max(0.0);
    if (state.zoom_target - state.zoom).abs() > 1e-4 {
        const TIME_CONSTANT: f32 = 0.05;
        let alpha = (1.0 - (-dt / TIME_CONSTANT).exp()).clamp(0.0, 1.0);
        let z_old = state.zoom;
        let z_new = z_old + (state.zoom_target - z_old) * alpha;
        state.zoom = z_new.clamp(0.1, 10.0);
        // Cursor-anchor for THIS frame's sub-step. Each smoothing
        // tick shifts content by a tiny delta so the scene point
        // under the cursor stays put for the whole animation.
        if (z_new - z_old).abs() > f32::EPSILON
            && let Some(p) = parent_hover
        {
            let cursor_offset = p - rect.min;
            let delta = cursor_offset / z_new - cursor_offset / z_old;
            on_zoom_anchor(&state.sub_ctx.clone(), delta);
        }
        // Keep repainting until we've settled — egui can otherwise
        // idle-sleep mid-animation.
        parent_ui.ctx().request_repaint();
    } else if (state.zoom_target - state.zoom).abs() > 0.0 {
        // Snap the last sub-epsilon gap so we don't repaint forever.
        state.zoom = state.zoom_target;
    }

    let zoom = state.zoom.max(0.1);
    let sub_ppp = (parent_ppp * zoom).max(0.05);
    let sub_screen_pixels = Vec2::new(size_pixels[0] as f32, size_pixels[1] as f32);
    let sub_screen_points = sub_screen_pixels / sub_ppp;
    let sub_screen_rect = Rect::from_min_size(egui::pos2(0.0, 0.0), sub_screen_points);

    // Coordinate scale: a pointer at parent point `p` lies at
    // pixel `(p - rect.min) × parent_ppp` inside the rect, and
    // the sub context interprets that pixel as a sub point at
    // `pixel / sub_ppp = (p - rect.min) × (parent_ppp / sub_ppp)
    // = (p - rect.min) / zoom`. So `pos_scale = 1 / zoom`.
    let pos_scale = parent_ppp / sub_ppp;

    // Build a fresh RawInput each frame — start from defaults and
    // pass only the pointer/keyboard slice we want the secondary
    // context to see, plus the resized screen_rect + ppp.
    let mut raw = egui::RawInput {
        screen_rect: Some(sub_screen_rect),
        ..Default::default()
    };
    // egui 0.33 accepts `pixels_per_point` via input_mut OR by
    // mutating after begin_frame; setting the field on RawInput
    // is the documented public path.
    raw.viewport_id = Default::default();
    raw.viewports.insert(
        Default::default(),
        egui::ViewportInfo {
            native_pixels_per_point: Some(sub_ppp),
            ..Default::default()
        },
    );
    raw.modifiers = parent_ui.ctx().input(|i| i.modifiers);
    if pointer_over_sub {
        // `events_for_sub` was pre-extracted above (with mouse-
        // wheel Line/Page events stripped — those drive zoom
        // smoothing, not graph pan). Translate position-carrying
        // events into sub-context space and forward.
        for ev in events_for_sub.iter_mut() {
            translate_event_to_sub(ev, rect, pos_scale);
        }
        raw.events.extend(events_for_sub);

        // Only synthesise a `PointerMoved` when this frame had no
        // real motion event of its own. Without this, egui-graph
        // would see `hover_pos = None` on idle frames and node
        // hover / click would go dead the moment the cursor stops.
        // With this, idle frames re-affirm the cursor position;
        // motion frames are left untouched so we never replay the
        // user's pointer history (which manifested as judder during
        // drags).
        if !parent_had_motion && let Some(p) = parent_hover {
            let local = (p - rect.min.to_vec2()) * pos_scale;
            raw.events
                .push(egui::Event::PointerMoved(egui::pos2(local.x, local.y)));
        }
    } else {
        // Pointer left the rect — tell the sub context so it drops
        // any hover / drag highlights on its own widgets.
        raw.events.push(egui::Event::PointerGone);
    }

    // Run the secondary context's frame. We use `begin_pass` /
    // `end_pass` rather than `run(...)` so the body closure
    // (which is `FnOnce`) can be called exactly once without
    // egui's `FnMut` wrapper rejecting it.
    state.ensure_renderer(backend);
    state.ensure_target(backend, size_pixels);
    let sub_ctx = state.sub_ctx.clone();
    sub_ctx.begin_pass(raw);
    #[allow(deprecated)]
    {
        egui::CentralPanel::default()
            .frame(egui::Frame::new())
            .show(&sub_ctx, |ui| {
                body(ui);
            });
    }
    let full = sub_ctx.end_pass();
    let primitives = sub_ctx.tessellate(full.shapes, sub_ppp);
    render_into_target(
        state,
        backend,
        primitives,
        full.textures_delta,
        size_pixels,
        sub_ppp,
    );

    // Composite the rendered texture into the parent UI.
    if let Some(target) = &state.target
        && let Some(tex_id) = target.parent_tex_id
    {
        parent_ui.painter().image(
            tex_id,
            rect,
            Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        // Per-frame backend hook — Bevy queues the
        // source-to-GpuImage copy here so the parent UI sees
        // this frame's render. eframe ignores it.
        backend.after_render(&target.texture, tex_id, target.size_pixels);
    }

    response
}

/// Translate a single `egui::Event` from the parent context's
/// coordinate space into the secondary context's. The transform is
/// `sub_pos = (parent_pos - rect.min) * pos_scale`, where
/// `pos_scale = parent_ppp / sub_ppp` — that single factor accounts
/// for both the host's HiDPI scaling and the sub context's
/// zoom-driven `pixels_per_point` so positional events stay aligned
/// at any zoom level. Non-positional events (key, text, scroll,
/// modifier) pass through unchanged.
fn translate_event_to_sub(ev: &mut egui::Event, rect: Rect, pos_scale: f32) {
    let translate = |p: egui::Pos2| -> egui::Pos2 {
        let v = (p - rect.min.to_vec2()) * pos_scale;
        egui::pos2(v.x, v.y)
    };
    match ev {
        egui::Event::PointerMoved(p) => *p = translate(*p),
        egui::Event::PointerButton { pos, .. } => *pos = translate(*pos),
        egui::Event::Touch { pos, .. } => *pos = translate(*pos),
        egui::Event::MouseMoved(delta) => {
            // Movement deltas need scaling but no rect-min offset.
            *delta *= pos_scale;
        }
        _ => {}
    }
}
