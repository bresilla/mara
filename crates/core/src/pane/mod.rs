//! Floating pane primitive.
//!
//! A pane that paints a theme-aware **title strip** in any of 12
//! anchor positions (4 screen rails × 3 zones each) and reserves
//! the remainder for typed container bodies.
//!
//! ## Submodule layout
//!
//! * [`anchor`] — `PaneAnchor`, `RailZone`, the per-anchor
//!   `title_side` / `title_reversed` / `is_middle` decisions, and
//!   the `far_flags` table that drives bottom/right inset choice.
//! * [`layout`] — `compute_pane_pos` (anchor → screen position).
//! * [`title`] — `paint_pane_title` (theme-aware strip painter).
//! * `mod.rs` (this file) — `Pane` builder + render entry point.

mod anchor;
mod body;
mod dots;
mod drag;
mod layout;
pub(crate) mod tab_drag;
mod title;

pub use body::{ContainerSpec, PaneBody};
pub(crate) use body::{TabRoutingScope, render_containers_with_tab_scope};
pub(crate) use dots::paint_container_dots;

pub use anchor::{PaneAnchor, RailZone, TitleSide};
#[cfg(test)]
pub(crate) use dots::record_container_dot_rect;
pub(crate) use dots::{clear_container_dot_rects, pointer_over_container_dots};
pub use drag::{
    DragState, RectEntry, active_drag, begin_frame as begin_drag_frame, clear_drag, compute_target,
    current_cache, dragged_entry, dragged_size, finalize_snapshot, paint_drag_preview,
    paint_ghost_gap_entry_inline, paint_ghost_gap_inline, push_rect, push_rect_with_frame,
    section_order_for, set_drag, set_section_order, snapshot, state as drag_state, target_cache,
};
pub(crate) use drag::{ghost_gap_suppressed, set_ghost_gap_suppressed, set_snapshot};

use crate::context::MaraCtx;
use crate::memory::MaraAnim;
use crate::vocab::Id;
use egui::Color32;

use crate::layout::{AreaHost, PaneBodyScrollSpec, PaneFlexSpec};
use crate::style;
use crate::vocab::{
    Color32 as MaraColor32, Id as MaraId, Pos2 as MaraPos2, Rect as MaraRect, Vec2 as MaraVec2,
};

// ─── Sizing constants ──────────────────────────────────────────────

/// Pane Frame's `inner_margin` per side. Used both literally (in the
/// `Frame { inner_margin: … }` builder) and to compute the inner
/// span-axis available to the body via `cross - 2 * PANE_INNER_MARGIN`.
/// Keep these in sync — if you change the Frame margin, recompute the
/// available space.
const PANE_INNER_MARGIN: f32 = 2.0;
/// Total chrome (both sides) the pane Frame steals from the inner ui
/// — used in the body flow-axis size lerp so the pane's outer height
/// includes the chrome both above and below the body.
const PANE_FRAME_CHROME: f32 = PANE_INNER_MARGIN * 2.0;

/// Pane outer span-axis size. The pane is square in span-axis
/// regardless of which rail it lives on, and the container inside
/// clamps its own cross to `outer_avail` so it always fits — no
/// per-orientation tuning needed.
pub const PANE_OUTER_SPAN: f32 = 320.0;

/// Thickness of the title strip on its flow axis (perpendicular to
/// the strip's reading direction).
pub const TITLE_STRIP_THICKNESS: f32 = 25.0;

/// Animation duration for the body's open/close transition. Shared
/// between [`Pane`] (for size animation) and
/// [`crate::container::Normal`] (for body content animation), so
/// both lerp at the same rate.
pub const BODY_ANIMATION_TIME: f32 = 0.18;

/// Container outer flow-axis size when the body is fully expanded.
/// Equal to `crate::container::Normal::CONTAINER_DEFAULT_*`, so the
/// pane and container agree on the fully-open size.
pub const DEFAULT_FLOW_OPEN: f32 = 280.0;
/// Hit-region thickness (in screen pixels) for the resize handles
/// that overlay each enabled pane edge. The handles do NOT allocate
/// layout space — they sit inside the pane's own painted rect, so
/// the pane never grows just to expose them.
pub const RESIZE_HANDLE_THICKNESS: f32 = 10.0;
/// Lower bound on user-resized pane body main extent — keeps the
/// pane from collapsing past usability.
pub const MIN_USER_FLOW: f32 = 80.0;
/// Upper bound on user-resized pane body main extent.
pub const MAX_USER_FLOW: f32 = 1200.0;
/// Lower bound on user-resized pane CROSS extent.
pub const MIN_USER_SPAN: f32 = 120.0;
/// Upper bound on user-resized pane CROSS extent.
pub const MAX_USER_SPAN: f32 = 1200.0;
/// Container's title-strip thickness and outer-margin reservation
/// — used to compute the collapsed body main size from the active
/// theme each frame (see `body_flow_collapsed`). Themes differ in
/// `section_padding` (PRO 4×3, GAME 6×8) so a hardcoded constant
/// can't get this right for both.
const CONTAINER_TITLE_THICKNESS: f32 = 22.0;

/// Compute the pane's animated openness 0..=1 for `pane_id`. Both
/// `Pane` and `Normal` call this with the same id so they lerp in
/// lockstep and the pane size is known in-frame (no anchor drift).
pub(crate) fn body_openness(ctx: &dyn crate::context::MaraCtx, pane_id: impl Into<MaraId>) -> f32 {
    let pane_id: Id = pane_id.into().into();
    let mut memory = ctx.memory();
    let open: bool = match memory.get_persisted::<bool>(pane_id.with("body_open")) {
        Some(open) => open,
        None => {
            memory.set_persisted(pane_id.with("body_open"), true);
            true
        }
    };
    ctx.memory().animate_bool(
        pane_id.with("body_open").with("anim").into(),
        open,
        BODY_ANIMATION_TIME,
    )
}

/// User-controlled body main extent for `pane_id`, persisted across
/// runs. Defaults to [`DEFAULT_FLOW_OPEN`] until the user drags
/// the pane's inner-edge resize handle. Vertical-strip panes
/// (LEFT/RIGHT rails) interpret this as the pane WIDTH, horizontal
/// -strip panes (TOP/BOTTOM rails) as the pane HEIGHT — the handle
/// always grows the pane along its flow axis.
pub(crate) fn user_flow(ctx: &dyn crate::context::MaraCtx, pane_id: impl Into<MaraId>) -> f32 {
    let pane_id: Id = pane_id.into().into();
    sanitize_user_extent(
        ctx.memory()
            .get_persisted::<f32>(pane_id.with("mara_pane_user_body_main"))
            .unwrap_or(DEFAULT_FLOW_OPEN),
        DEFAULT_FLOW_OPEN,
        MIN_USER_FLOW,
        MAX_USER_FLOW,
    )
}

/// Persist the user-set body main extent for `pane_id`. Clamped to
/// [`MIN_USER_FLOW`] .. [`MAX_USER_FLOW`].
pub(crate) fn set_user_flow(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: impl Into<MaraId>,
    value: f32,
) {
    let pane_id: Id = pane_id.into().into();
    let clamped = sanitize_user_extent(value, DEFAULT_FLOW_OPEN, MIN_USER_FLOW, MAX_USER_FLOW);
    ctx.memory()
        .set_persisted(pane_id.with("mara_pane_user_body_main"), clamped);
}

/// User-controlled CROSS extent for `pane_id`, persisted across runs.
/// Defaults to [`PANE_OUTER_SPAN`]. Only consulted when the caller
/// enables `PaneResize::cross` on the builder; otherwise the pane
/// keeps its baseline cross size.
pub(crate) fn user_span(ctx: &dyn crate::context::MaraCtx, pane_id: impl Into<MaraId>) -> f32 {
    let pane_id: Id = pane_id.into().into();
    sanitize_user_extent(
        ctx.memory()
            .get_persisted::<f32>(pane_id.with("mara_pane_user_cross_main"))
            .unwrap_or(PANE_OUTER_SPAN),
        PANE_OUTER_SPAN,
        MIN_USER_SPAN,
        MAX_USER_SPAN,
    )
}

/// Persist the user-set CROSS extent for `pane_id`. Clamped to
/// [`MIN_USER_SPAN`] .. [`MAX_USER_SPAN`].
pub(crate) fn set_user_span(
    ctx: &dyn crate::context::MaraCtx,
    pane_id: impl Into<MaraId>,
    value: f32,
) {
    let pane_id: Id = pane_id.into().into();
    let clamped = sanitize_user_extent(value, PANE_OUTER_SPAN, MIN_USER_SPAN, MAX_USER_SPAN);
    ctx.memory()
        .set_persisted(pane_id.with("mara_pane_user_cross_main"), clamped);
}

fn sanitize_user_extent(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback.clamp(min, max)
    }
}

/// Per-pane resize affordance — opt-in via [`Pane::resize`].
///
/// `flow` adds an invisible handle on the pane's inner edge (the
/// side facing AWAY from the rail), letting the user drag the pane
/// along its FLOW axis — the direction the body extends from the
/// title strip. Vertical-strip panes (LEFT/RIGHT rails) resize
/// horizontally; horizontal-strip panes (TOP/BOTTOM rails) resize
/// vertically.
///
/// `span` adds an invisible handle on the pane's SPAN axis (the
/// axis parallel to the title strip). For zone-end / zone-start
/// anchors only one side is resizable (the unanchored side); for
/// `Middle` anchors both span sides are resizable and the pane
/// grows symmetrically about its centre.
#[derive(Copy, Clone, Debug, Default)]
pub struct PaneResize {
    pub flow: bool,
    pub span: bool,
}

impl PaneResize {
    pub const NONE: PaneResize = PaneResize {
        flow: false,
        span: false,
    };
    pub const FLOW: PaneResize = PaneResize {
        flow: true,
        span: false,
    };
    pub const SPAN: PaneResize = PaneResize {
        flow: false,
        span: true,
    };
    pub const BOTH: PaneResize = PaneResize {
        flow: true,
        span: true,
    };
}

/// Shared ctx-data key that points to the **currently active**
/// `Pane`'s id. Pane writes this at the top of `show` so children
/// (e.g. `Normal`) can look up their parent pane's stagger state
/// without needing the pane id wired through their constructors.
/// Multiple panes' bodies run sequentially within a frame so the
/// pointer is well-defined while any one body callback runs.
pub(crate) fn active_pane_key() -> Id {
    Id::new("mara_active_pane_id")
}

fn ribbon_pane_ids_key() -> Id {
    Id::new("mara_registered_ribbon_pane_ids")
}

/// Publish the pane ids that are reachable from the current ribbon set.
///
/// When this registry is present, internal pane rendering refuses to paint
/// panes whose id was not registered by a ribbon/panel button. This keeps app
/// chrome honest: a pane must have a corresponding ribbon affordance instead
/// of being slapped onto the canvas directly.
#[doc(hidden)]
pub fn __internal_publish_ribbon_pane_ids(
    ctx: &dyn crate::context::MaraCtx,
    ids: impl IntoIterator<Item = impl Into<MaraId>>,
) {
    let ids = ids
        .into_iter()
        .map(|id| {
            let id: MaraId = id.into();
            Id::from(id)
        })
        .collect::<Vec<_>>();
    ctx.memory().set_temp(ribbon_pane_ids_key(), ids);
}

/// Whether the pane may render under the ribbon-affordance registry.
///
/// No registry published (headless tests, minimal hosts) → permissive,
/// exactly as the publish doc states. A published registry that lacks
/// this pane id → the pane is SKIPPED (debug builds also assert, so the
/// mistake is loud in development) — a user action must never abort the
/// host app over a chrome-bookkeeping slip.
fn pane_has_ribbon_button(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> bool {
    let Some(ids) = ctx.memory().get_temp::<Vec<Id>>(ribbon_pane_ids_key()) else {
        return true;
    };
    let registered = ids.contains(&pane_id);
    debug_assert!(
        registered,
        "pane {pane_id:?} was rendered without a registered ribbon button; add a ribbon item for it or do not render the pane"
    );
    registered
}

pub(crate) fn active_tabbed_container_rect_key() -> Id {
    Id::new("mara_active_tabbed_container_rect")
}

pub(crate) fn active_container_frame_rect_key() -> Id {
    Id::new("mara_active_container_frame_rect")
}

/// Toggle the pane's body open state. Called from the container's
/// title-strip click handler. Also bumps a per-pane "fold version"
/// counter — animation effects that should retrigger on every fold
/// or unfold (cipher decode, chromatic aberration, etc.) salt
/// their state ids with this counter so each toggle starts a fresh
/// cycle.
pub(crate) fn toggle_body(ctx: &dyn crate::context::MaraCtx, pane_id: Id) {
    let key = pane_id.with("body_open");
    let ver_key = pane_id.with("body_fold_version");
    let touch_key = pane_id.with("body_open_touched_at");
    let now = ctx.now();
    let mut memory = ctx.memory();
    let cur: bool = memory.get_persisted(key).unwrap_or(true);
    memory.set_persisted(key, !cur);
    let v: u64 = memory.get_persisted(ver_key).unwrap_or(0);
    memory.set_persisted(ver_key, v.wrapping_add(1));
    // Stamp the toggle time so the parent `Pane` auto-fold
    // walk can pick "fold the OLDEST-touched open container
    // first" instead of just blindly chopping the tail. The
    // user's most recent unfold wins — older opens yield space.
    memory.set_persisted(touch_key, now);
}

/// Read the timestamp (egui's `i.time` seconds) of the most recent
/// user toggle of `body_open` for this container. Returns `0.0` if
/// never toggled. Used by internal pane rendering's auto-fold-tail walk to
/// preserve the user's most recent unfold over older opens.
pub(crate) fn body_open_touched_at(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> f64 {
    ctx.memory()
        .get_persisted::<f64>(pane_id.with("body_open_touched_at"))
        .unwrap_or(0.0)
}

/// Read the per-pane fold-version counter. Bumped by
/// [`toggle_body`] on every fold/unfold; widgets salt their
/// animation state ids with it so each toggle re-triggers.
pub(crate) fn fold_version(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> u64 {
    ctx.memory()
        .get_persisted::<u64>(pane_id.with("body_fold_version"))
        .unwrap_or(0)
}

fn container_mins_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_container_min_widths")
}

fn container_min_flows_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_container_min_flows")
}

/// Read the list of container min widths registered against `pane_id`
/// during the previous frame's body callback. Returned in container
/// order. Empty when no [`crate::container::Normal`] children
/// painted under this pane.
pub(crate) fn container_min_widths(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> Vec<f32> {
    ctx.memory()
        .get_temp::<Vec<f32>>(container_mins_key(pane_id))
        .unwrap_or_default()
}

/// Read the list of container minimum FLOW-axis chrome sizes
/// registered against `pane_id` during the previous frame's body
/// callback. Each entry is what the container needs along the pane's
/// flow axis at body-flow = 0 (title strip + title/body gap + section
/// padding + stroke + outer margins). Used by horizontal-strip pane
/// resize handlers as the shrink floor: the pane can collapse until
/// each container is body-empty, then stops — preventing the
/// "containers overlap" artefact that comes from egui's
/// `available_rect_before_wrap` collapsing to zero when the pane body
/// runs out of space.
pub(crate) fn container_min_flows(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> Vec<f32> {
    ctx.memory()
        .get_temp::<Vec<f32>>(container_min_flows_key(pane_id))
        .unwrap_or_default()
}

/// Clear all four per-pane body-bookkeeping accumulators (min
/// widths, min flows, container cids, extra body flow). Called by
/// internal pane rendering at the top of every frame so the body callback
/// can re-register fresh.
pub(crate) fn clear_container_min_widths(ctx: &dyn crate::context::MaraCtx, pane_id: Id) {
    {
        let mut memory = ctx.memory();
        memory.remove_temp::<Vec<f32>>(container_mins_key(pane_id));
        memory.remove_temp::<Vec<f32>>(container_min_flows_key(pane_id));
        memory.remove_temp::<Vec<Id>>(container_cids_key(pane_id));
        memory.remove_temp::<f32>(body_extra_flow_key(pane_id));
    };
}

fn container_cids_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_container_cids")
}

fn body_extra_flow_key(pane_id: Id) -> Id {
    pane_id.with("mara_pane_body_extra_flow")
}

/// Read the list of container CIDs registered against `pane_id`
/// during this frame's body callback. Returned in container
/// declaration order. Used by internal pane rendering to compute the
/// pane's auto-flow when `PaneResize::flow` is off — pane size =
/// sum of `crate::container::container_flow(cid)` over these cids
/// + per-container chrome + sum of extra body chrome + pane chrome.
///
/// Storing CIDs (not flow values) lets the pane re-fetch each
/// container's LIVE persisted flow when it sizes itself, so a drag
/// that updates the persisted value at the END of frame N is
/// visible to the pane sizer on frame N+1's first read — without
/// the extra publish-vs-render lag that comes from caching values.
pub(crate) fn published_container_cids(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> Vec<Id> {
    ctx.memory()
        .get_temp::<Vec<Id>>(container_cids_key(pane_id))
        .unwrap_or_default()
}

/// Append `cid` to the per-pane container CID list. Called by
/// `Normal::show` each frame.
pub(crate) fn publish_container_cid(ctx: &dyn crate::context::MaraCtx, pane_id: Id, cid: Id) {
    let mut memory = ctx.memory();
    let key = container_cids_key(pane_id);
    let mut acc: Vec<Id> = memory.get_temp(key).unwrap_or_default();
    assert!(
        !acc.contains(&cid),
        "pane containers require unique container ids per pane frame"
    );
    acc.push(cid);
    memory.set_temp(key, acc);
}

/// Sum of additional body-flow allocations (e.g. inter-container
/// drag handles painted via [`paint_container_dots`]) registered
/// against `pane_id` this frame. Pane auto-flow accounting adds
/// this on top of `published_container_body_flows + per-container
/// chrome` so the pane stays sized to fit everything its body
/// callback paints, not just the containers themselves.
pub(crate) fn published_body_extra_flow(ctx: &dyn crate::context::MaraCtx, pane_id: Id) -> f32 {
    ctx.memory()
        .get_temp::<f32>(body_extra_flow_key(pane_id))
        .unwrap_or(0.0)
}

/// Add `flow` to the per-pane "extra body chrome" total. Called by
/// any caller that paints extra flow-axis content inside a
/// `Pane` body — the inter-container drag-handle in
/// [`paint_container_dots`] uses this to make sure the pane
/// auto-grows to include each handle's strip height.
pub(crate) fn publish_body_extra_flow(ctx: &dyn crate::context::MaraCtx, pane_id: Id, flow: f32) {
    let flow = if flow.is_finite() { flow.max(0.0) } else { 0.0 };
    let mut memory = ctx.memory();
    let key = body_extra_flow_key(pane_id);
    let cur: f32 = memory.get_temp(key).unwrap_or(0.0);
    memory.set_temp(key, cur + flow);
}

/// Global ctx-data key under which every internal pane render call
/// publishes its painted rect each frame. Read by host integrations
/// (e.g. `bevy_mara::EguiInputAbsorbPlugin`) to decide whether the
/// cursor is currently over an interactable mara pane — a
/// reliable substitute for `egui::Context::layer_id_at`, which has
/// too many edge cases (modal handling, ribbon button tooltip
/// areas, …) to use as a "is the cursor over the UI?" check.
fn published_pane_rects_key() -> Id {
    Id::new("mara_published_pane_rects")
}

/// Read every pane's painted rect that was published THIS FRAME.
/// Empty when no panes are currently rendering. The list resets at
/// the start of each frame and entries are appended by
/// internal pane rendering.
///
/// Internal first-party host hook for input-firewall adapters. App
/// code should not read raw backend context data to discover panes.
#[doc(hidden)]
pub fn __internal_published_pane_rects(ctx: &dyn crate::context::MaraCtx) -> Vec<MaraRect> {
    ctx.memory()
        .get_temp::<Vec<MaraRect>>(published_pane_rects_key())
        .unwrap_or_default()
}

/// Clear the global pane-rects list unconditionally. Host
/// integrations call this once per frame BEFORE any internal pane rendering
/// runs (e.g. the bevy_mara firewall, after it has consumed the
/// previous frame's rects), so the list reflects ONLY panes that
/// actually painted in the most recent egui pass — without this,
/// closing every visible pane leaves the last-seen rects stuck in
/// `__internal_published_pane_rects` forever, since internal pane rendering is
/// the only other entry point that resets the list.
///
/// Internal first-party host hook for input-firewall adapters.
#[doc(hidden)]
pub fn __internal_clear_published_pane_rects(ctx: &dyn crate::context::MaraCtx) {
    {
        let mut memory = ctx.memory();
        memory.remove_temp::<Vec<MaraRect>>(published_pane_rects_key());
    };
}

/// Append `rect` to the global pane-rects list. Called by
/// internal pane rendering after the Frame paints. The list lives in egui
/// ctx data and is reset by [`maybe_reset_published_pane_rects`].
fn publish_pane_rect(ctx: &dyn crate::context::MaraCtx, rect: impl Into<MaraRect>) {
    let mut memory = ctx.memory();
    let key = published_pane_rects_key();
    let mut acc: Vec<MaraRect> = memory.get_temp(key).unwrap_or_default();
    acc.push(rect.into());
    memory.set_temp(key, acc);
}

/// Clear the pane-rects list. Called once per frame, before any
/// internal pane rendering runs, so the list reflects ONLY this frame's
/// painted panes. Reset is keyed off `cumulative_pass_nr` — the
/// list resets the first time a pane is rendered in a new
/// pass and stays accumulating until the next pass starts.
fn maybe_reset_published_pane_rects(ctx: &dyn crate::context::MaraCtx) {
    let key = Id::new("mara_published_pane_rects_pass");
    let now = ctx.pass_nr();
    let last: u64 = ctx.memory().get_temp(key).unwrap_or(u64::MAX);
    if last != now {
        {
            let mut memory = ctx.memory();
            memory.remove_temp::<Vec<MaraRect>>(published_pane_rects_key());
            memory.set_temp(key, now);
        };
    }
}

/// Inset from each screen edge: `EDGE_GAP + SIDE_BTN_SIZE +
/// RAIL_PANEL_GAP`. The pane sits 4 px past the rail's button
/// strip on top/left edges; bottom/right add a wider `far` inset
/// for anchors whose far edge meets a perpendicular rail's button
/// (see [`anchor::far_flags`]).
pub const RAIL_INSET: f32 = crate::ribbon::EDGE_GAP + crate::ribbon::SIDE_BTN_SIZE + RAIL_PANEL_GAP;

/// Read which screen edges currently host an active ribbon, as
/// `[left, right, top, bottom]`. Published every frame by
/// the internal featureful ribbon renderer; returns `[true; 4]` when no
/// ribbons have been drawn yet (conservative default — reserve
/// space for ribbons on every side until we know better).
pub(crate) fn published_ribbon_edges(ctx: &dyn crate::context::MaraCtx) -> [bool; 4] {
    ctx.memory()
        .get_temp::<[bool; 4]>(egui::Id::new("mara_published_ribbon_edges"))
        .unwrap_or([true; 4])
}

/// Visual gap between the ribbon's button strip and the pane edge.
const RAIL_PANEL_GAP: f32 = 8.0;

// ─── Builder ───────────────────────────────────────────────────────

/// A single floating window keyed by `id` and pinned to one of 12
/// screen positions. Build with [`Pane::new`], then render through a
/// sealed Mara host/view context each frame the pane should be visible.
pub struct Pane {
    id: Id,
    title: String,
    anchor: PaneAnchor,
    accent: MaraColor32,
    resize: PaneResize,
    order: crate::layout::Layer,
}

impl Pane {
    /// Enable user-resize on the pane's edges. See [`PaneResize`].
    pub fn resize(mut self, resize: PaneResize) -> Self {
        self.resize = resize;
        self
    }

    /// Override the backend-neutral layer for this pane.
    ///
    /// Normal panes live in [`crate::layout::Layer::Middle`] — the
    /// docked-chrome band — so they always float above view bodies
    /// and backdrops (Background) even when a click raises the view
    /// within its own band, while ribbon chrome (Foreground) stays
    /// above pane shadows. Hosts can lift persistent panes to
    /// [`crate::layout::Layer::Foreground`] when they intentionally
    /// render over fullscreen/module overlays.
    pub fn order(mut self, order: crate::layout::Layer) -> Self {
        self.order = match order {
            crate::layout::Layer::Background => crate::layout::Layer::Middle,
            other => other,
        };
        self
    }

    /// Construct a pane builder. `id` is used to scope the
    /// `egui::Area` and any title-strip animations.
    pub fn new(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        anchor: PaneAnchor,
        accent: impl Into<MaraColor32>,
    ) -> Self {
        let title = title.into();
        assert!(!title.trim().is_empty(), "panes require a non-empty title");
        let id: Id = id.into().into();
        Self {
            id,
            title,
            anchor,
            accent: accent.into(),
            resize: PaneResize::NONE,
            order: crate::layout::Layer::Middle,
        }
    }

    /// Render the pane this frame. `body` runs after the title strip
    /// is laid out; its `Ui` covers the rest of the pane.
    ///
    /// Pane sizing is content-driven: an empty `body` collapses the
    /// pane to JUST the title strip thickness, and adding content
    /// extends the pane along the title's perpendicular axis (a
    /// horizontal title bar grows down with stacked containers; a
    /// vertical title strip grows right). The span axis (the one
    /// the title spans) is fixed per anchor.
    #[doc(hidden)]
    pub fn __internal_show<'spec>(
        self,
        ctx: &egui::Context,
        region: MaraRect,
        body: impl FnOnce(&mut PaneBody<'_, 'spec>),
    ) {
        crate::enforce::__internal_enforce_defaults(ctx);
        if !pane_has_ribbon_button(ctx, self.id) {
            return;
        }
        let (align, offset) = layout::anchor_align(self.anchor);
        let area_id = self.id.with("pane2_area");

        let title_side = self.anchor.title_side();
        let horizontal_strip = title_side.is_horizontal_strip();
        // Cross extent: user-resized when `PaneResize::cross` is on,
        // baseline `PANE_OUTER_SPAN` otherwise.
        let mut span_outer = if self.resize.span {
            user_span(ctx, self.id)
        } else {
            PANE_OUTER_SPAN
        };

        // ── Per-pane staggered fade-in clock. ──
        //
        // Tracks elapsed seconds since this pane became visible.
        // The `cumulative_pass_nr + 1 < frame_now` check detects
        // a paint gap (= the pane was hidden last frame, e.g.
        // user just clicked its ribbon button) and resets the
        // clock to 0. Stored under `self.id.with(...)` so each
        // pane has its own independent timer.
        let pane_open_elapsed: f32 = {
            let frame_key = self.id.with("mara_pane_anim_frame");
            let state_key = self.id.with("mara_pane_anim_elapsed");
            let frame_now = MaraCtx::pass_nr(ctx);
            let last_frame: u64 = MaraCtx::memory(ctx).get_temp(frame_key).unwrap_or(0);
            let mut elapsed: f32 = MaraCtx::memory(ctx).get_temp(state_key).unwrap_or(99.0);
            if last_frame + 1 < frame_now {
                elapsed = 0.0;
            }
            let dt = MaraCtx::dt(ctx);
            elapsed += dt;
            {
                let mut memory = MaraCtx::memory(ctx);
                memory.set_temp(state_key, elapsed);
                memory.set_temp(frame_key, frame_now);
            };
            // Repaint while any reasonably-staged section is still
            // animating in (~12 sections × 0.18 stagger + 0.45 fade
            // ≈ 2.6 s — keep some headroom).
            if elapsed < 3.0 {
                ctx.request_repaint();
            }
            elapsed
        };
        // Publish the active pane's id PLUS its current elapsed
        // and a fresh `section_idx = 0` counter under that id.
        // The active-pane pointer lives at a single global key so
        // `Normal::show` (whose own `pane_id` field is the
        // CONTAINER's id, not Pane's) can find its parent pane.
        {
            let mut memory = MaraCtx::memory(ctx);
            memory.set_temp(active_pane_key(), self.id);
            memory.set_temp(self.id.with("mara_pane_open_elapsed"), pane_open_elapsed);
            memory.set_temp(self.id.with("mara_pane_section_idx"), 0u32);
        };
        // Auto-grow the user-resized extents to satisfy the previous
        // frame's container min widths. Without this, a vertical-strip
        // pane (LEFT / RIGHT rail) opens at the baseline
        // `DEFAULT_FLOW_OPEN` (= 280) — way too narrow to fit
        // 3 containers × `CONTAINER_DEFAULT_MIN_WIDTH` (= 286). The
        // user would have to drag the pane wider FIRST before any
        // container could reach its declared min, and then the
        // shrink-floor would lock them at the cramped width. Reading
        // the accumulator BEFORE the clear gives us last frame's
        // values; on the very first open the accumulator is empty
        // and no auto-grow happens (default sizes apply).
        let title_side_for_pane = self.anchor.title_side();
        let horizontal_strip_pane = title_side_for_pane.is_horizontal_strip();
        let prev_mins = container_min_widths(ctx, self.id);
        // Snapshot LAST frame's container cids and extra body flow
        // BEFORE the clear below wipes them — these feed the pane's
        // auto-flow calculation further down (line ~565).
        let prev_cids_snapshot = published_container_cids(ctx, self.id);
        let prev_extra_flow_snapshot = published_body_extra_flow(ctx, self.id);
        if !prev_mins.is_empty() {
            if self.resize.flow && !horizontal_strip_pane {
                let need_main: f32 = prev_mins.iter().sum();
                let cur_main = user_flow(ctx, self.id);
                if cur_main < need_main {
                    set_user_flow(ctx, self.id, need_main);
                }
            }
            if self.resize.span && horizontal_strip_pane {
                let need_cross: f32 = prev_mins.iter().copied().fold(0.0, f32::max);
                let cur_cross = user_span(ctx, self.id);
                if cur_cross < need_cross {
                    set_user_span(ctx, self.id, need_cross);
                }
            }
        }
        // Reset the container-min-width accumulator before the body
        // callback runs — `Normal::show` (called from inside the
        // body) appends each container's min width to it. The
        // resize handles read the result AFTER the body finishes.
        clear_container_min_widths(ctx, self.id);

        // Compute pane main from the body's animation state in
        // THIS frame. Both `Pane` and `Normal` call
        // `body_openness(ctx, pane_id)` with the same `pane_id`, so
        // egui returns the same value to both — meaning the pane's
        // size and the container's content are in lockstep, with
        // ZERO 1-frame lag. egui::Area's anchor math then uses this
        // `state.size` (we lock it via set_min/max_size) and the
        // anchored corner stays pixel-pinned during the animation.
        let openness = body_openness(ctx, self.id);
        // Container's collapsed outer size differs per theme (PRO
        // uses section_padding 4×3 + outer_margin 3, GAME uses 6×8 +
        // outer_margin 9 main / 1 cross). Compute from the active
        // theme so the pane main lerp matches the container's
        // actual rendered size on both axes.
        let theme_now = style::theme();
        let pad = style::section_padding();
        let container_pad_flow = if horizontal_strip {
            (pad.top as f32) + (pad.bottom as f32)
        } else {
            (pad.left as f32) + (pad.right as f32)
        };
        let container_outer_main_total = (theme_now.section_outer_margin_flow_title as f32)
            + (theme_now.section_outer_margin_flow_body as f32);
        let body_flow_collapsed =
            CONTAINER_TITLE_THICKNESS + container_pad_flow + container_outer_main_total;
        // Extra space `lay_out_flex` allocates between the pane
        // title strip and the first container — keeps that gap in
        // sync between the layout pass and the size computation.
        let pane_title_to_body_pad = theme_now.section_outer_margin_flow_title as f32;
        let collapsed_flow = TITLE_STRIP_THICKNESS
            + pane_title_to_body_pad
            + body_flow_collapsed
            + PANE_FRAME_CHROME;
        // Pane main when fully open = title + body + frame chrome.
        //
        // When `PaneResize::flow` is ON, the user drives this with
        // the inner-edge resize handle and the body slot is split
        // evenly across containers.
        //
        // When `PaneResize::flow` is OFF — the new "individually
        // resizable containers" model — the pane auto-sizes from
        // the previous frame's per-container body-flow registrations,
        // i.e. each container's persisted flow PLUS the per-
        // container chrome (title strip + padding + outer margins).
        // Empty accumulator (first frame, no body callback yet) →
        // fall back to `DEFAULT_FLOW_OPEN` so the pane appears at
        // a reasonable size before the body has had a chance to
        // run.
        let body_flow_open = if self.resize.flow {
            user_flow(ctx, self.id)
        } else if prev_cids_snapshot.is_empty() {
            DEFAULT_FLOW_OPEN
        } else {
            let chrome_per_container = body_flow_collapsed;
            let sum_body: f32 = prev_cids_snapshot
                .iter()
                .map(|cid| crate::container::container_flow(ctx, *cid, horizontal_strip))
                .sum();
            sum_body
                + chrome_per_container * (prev_cids_snapshot.len() as f32)
                + prev_extra_flow_snapshot
        };
        let expanded_flow =
            TITLE_STRIP_THICKNESS + pane_title_to_body_pad + body_flow_open + PANE_FRAME_CHROME;
        let mut pane_flow = collapsed_flow + (expanded_flow - collapsed_flow) * openness;

        // ── Auto-fold-tail when the pane would overflow the screen ──
        //
        // Hard rule: a pane MUST NOT exceed the screen extent along
        // its flow axis (otherwise `Middle`-anchored panes center
        // the overflow off-screen and the user can't see the title /
        // header strip). On a frame where the natural pane flow
        // exceeds `screen_flow_avail`, walk the previous frame's
        // container list from END to START and force-fold the
        // tail containers (write `body_open = false`) until the
        // natural sum fits. The user can drag the corresponding
        // ribbon button to that container to unfold it; if doing so
        // exceeds the budget, the next frame's walk will fold a
        // different tail container to compensate.
        // Anchor within the caller's region (the node's rect). For the
        // root this is the whole window, so the intersection with the
        // published chrome bounds reproduces the window-level anchoring;
        // for a cell it clamps the pane inside the cell.
        let screen = region.intersect(
            MaraCtx::memory(ctx)
                .get_temp::<MaraRect>(crate::ribbon::chrome::chrome_bounds_key())
                .unwrap_or_else(|| MaraCtx::content_rect(ctx)),
        );
        // Reserve `RAIL_INSET` on the pane's OWN rail (its title
        // strip lives there); on the opposite side only reserve
        // when there's actually a ribbon hosted there. The
        // `published_ribbon_edges` registry is filled every frame
        // by the ribbon renderer so the pane always sees current
        // truth.
        let edges = published_ribbon_edges(ctx);
        let [has_left, has_right, has_top, has_bottom] = edges;
        let title_side = self.anchor.title_side();
        let (own_present, opposite_present) = match title_side {
            anchor::TitleSide::Left => (has_left, has_right),
            anchor::TitleSide::Right => (has_right, has_left),
            anchor::TitleSide::Top => (has_top, has_bottom),
            anchor::TitleSide::Bottom => (has_bottom, has_top),
        };
        // Reserve exactly one ribbon button row. Do not reserve a
        // second hidden "breathing" row: Pane2 should be allowed to
        // grow close to the opposite ribbon/button bar.
        let own_inset = if own_present { RAIL_INSET } else { 0.0 };
        let opp_inset = if opposite_present { RAIL_INSET } else { 0.0 };
        let screen_flow_avail = if horizontal_strip {
            (screen.height() - own_inset - opp_inset).max(MIN_USER_FLOW)
        } else {
            (screen.width() - own_inset - opp_inset).max(MIN_USER_FLOW)
        };
        if !self.resize.flow && !prev_cids_snapshot.is_empty() && pane_flow > screen_flow_avail {
            let title_chrome = TITLE_STRIP_THICKNESS + pane_title_to_body_pad + PANE_FRAME_CHROME;
            let chrome_per_container = body_flow_collapsed;
            let mut budget = (screen_flow_avail
                - title_chrome
                - prev_extra_flow_snapshot
                - chrome_per_container * prev_cids_snapshot.len() as f32)
                .max(0.0);
            // Collect every currently-open container with its
            // `container_flow` and the most recent user-toggle
            // timestamp. Sort DESCENDING by timestamp — the user's
            // most recent unfold is at the front, so it gets first
            // dibs on the budget. Older opens (and never-toggled
            // containers, which carry timestamp 0.0 by default) are
            // the ones that yield space when overflow forces a
            // re-fold.
            let mut opens: Vec<(Id, f64, f32)> = prev_cids_snapshot
                .iter()
                .filter_map(|cid| {
                    let open: bool = MaraCtx::memory(ctx)
                        .get_persisted::<bool>(cid.with("body_open"))
                        .unwrap_or(true);
                    if !open {
                        return None;
                    }
                    let touched_at = body_open_touched_at(ctx, *cid);
                    let cf = crate::container::container_flow(ctx, *cid, horizontal_strip);
                    Some((*cid, touched_at, cf))
                })
                .collect();
            opens.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (idx, (cid, _, cf)) in opens.iter().enumerate() {
                if idx == 0 {
                    // Keep one container open. If this last open bar
                    // exceeds the pane budget, the body ScrollArea below
                    // clips/scrolls it instead of closing everything.
                    budget = (budget - cf).max(0.0);
                    continue;
                }
                if budget >= *cf {
                    budget -= cf;
                } else {
                    // Older-toggled (or never-toggled) open
                    // container — fold to free space for the
                    // newer-toggled ones above.
                    MaraCtx::memory(ctx).set_persisted(cid.with("body_open"), false);
                }
            }
            // Recompute pane_flow with the folds applied. After the
            // walk every open container fits in `budget`; the flow
            // sum is guaranteed ≤ screen_flow_avail (with one frame
            // of lag on the fold-state animation, which is fine
            // since `body_openness` interpolates).
            let sum_body: f32 = prev_cids_snapshot
                .iter()
                .map(|cid| {
                    let open: bool = MaraCtx::memory(ctx)
                        .get_persisted::<bool>(cid.with("body_open"))
                        .unwrap_or(true);
                    if open {
                        crate::container::container_flow(ctx, *cid, horizontal_strip)
                    } else {
                        0.0
                    }
                })
                .sum();
            let new_expanded = TITLE_STRIP_THICKNESS
                + pane_title_to_body_pad
                + sum_body
                + chrome_per_container * prev_cids_snapshot.len() as f32
                + prev_extra_flow_snapshot
                + PANE_FRAME_CHROME;
            pane_flow = collapsed_flow + (new_expanded - collapsed_flow) * openness;
        }
        // Final safety clamp — even with auto-folds we never let the
        // pane outgrow the screen. Clip is a no-op when the auto-fold
        // walk above already brought us under budget; it catches the
        // first-frame case where prev_cids_snapshot was empty.
        let body_needs_flow_scroll = pane_flow > screen_flow_avail;
        MaraCtx::memory(ctx).set_temp(
            self.id.with("mara_pane_body_scroll_enabled"),
            body_needs_flow_scroll,
        );
        pane_flow = pane_flow.min(screen_flow_avail);

        // SPAN-axis clamp — perpendicular to flow. `Middle`-anchored
        // panes center the span axis on screen, so a span larger
        // than the available perpendicular extent bleeds past BOTH
        // perpendicular ribbons at once. Min/Max-aligned anchors
        // overflow only on one side, but the same clamp covers
        // them: subtract the inset on each perpendicular ribbon
        // that's actually present.
        let (span_lead_present, span_trail_present) = if horizontal_strip {
            (has_left, has_right)
        } else {
            (has_top, has_bottom)
        };
        let span_lead_inset = if span_lead_present { RAIL_INSET } else { 0.0 };
        let span_trail_inset = if span_trail_present { RAIL_INSET } else { 0.0 };
        let screen_span_avail = if horizontal_strip {
            (screen.width() - span_lead_inset - span_trail_inset).max(MIN_USER_SPAN)
        } else {
            (screen.height() - span_lead_inset - span_trail_inset).max(MIN_USER_SPAN)
        };
        span_outer = span_outer.min(screen_span_avail);
        // Publish the clamped span so `lay_out_flex` (which runs
        // inside the Area's body and would otherwise call
        // `user_span` again, getting the unclamped raw drag value)
        // sizes its Frame to the same dimension we just sized the
        // Area to. Without this the inner Frame paints at the raw
        // `user_span`, bleeding past the Area's clip rect into the
        // perpendicular ribbon — the "going above the ribbon"
        // symptom. We keep `user_span` unmodified so the user's
        // drag intent survives a window-shrink + re-enlarge cycle.
        MaraCtx::memory(ctx).set_temp::<f32>(self.id.with("mara_pane_effective_span"), span_outer);

        let outer_size = layout::pane_outer_size(horizontal_strip, span_outer, pane_flow);

        // Compute position MANUALLY from `outer_size` using the
        // anchor + offset + screen rect. egui's `Area::anchor()`
        // would use `state.size` from the previous frame, which
        // lags during animation by the per-frame size delta — that
        // was the visible drift on right/bottom-anchored panes.
        // With `fixed_pos`, position is computed in-frame from
        // our just-computed size, so the anchored corner is
        // pinned with ZERO lag.
        let placement = layout::PanePlacement::new(align, offset, screen, outer_size);
        let pane_pos = placement.pos;
        if crate::probe::__internal_enabled(ctx) {
            crate::probe::__internal_record(
                ctx,
                crate::probe::ElementPose::new("pane", placement.rect)
                    .with_id(self.id.into())
                    .with_label(format!(
                        "{:?} screen=({:.0},{:.0} {:.0}x{:.0})",
                        self.anchor,
                        screen.min.x,
                        screen.min.y,
                        screen.width(),
                        screen.height()
                    )),
            );
        }
        // Outer pane rect — used as the initial / fallback rect for
        // the resize handles. The Frame inside the Area shrinks
        // when containers fold; we capture its real rendered rect
        // below so the handles track the painted edge instead of
        // the (always-expanded) Area bounds.
        let pane_rect_mara = placement.rect;
        let pane_rect: egui::Rect = pane_rect_mara.into();
        let outer_size_egui: egui::Vec2 = outer_size.into();
        // Capture fields needed AFTER `self` is moved into the Area's
        // body closure (which moves `self` into `lay_out_flex`).
        let pane_id = self.id;
        let pane_anchor = self.anchor;
        let pane_resize = self.resize;
        let pane_accent: Color32 = self.accent.into();
        let pane_title_dbg = self.title.clone();
        // Slot for the Frame's actual rendered rect. The pane Area
        // closure writes this after `Frame::show` returns; the
        // resize-handle Area below reads it so the flow-axis
        // handle (which sits at the inner edge of the painted
        // pane) follows the Frame as it shrinks / grows with the
        // open / fold animation. Cross handles also benefit when
        // span-axis is animating.
        let painted_rect = std::cell::Cell::new(pane_rect);
        // Hierarchical clip rect — taken from the PREVIOUS frame's
        // painted rect (persisted via ctx data) unioned with this
        // frame's `pane_rect`. Reasons:
        //
        // * `pane_rect` is computed from `pane_flow`, which uses last
        //   frame's container snapshots. On the first frame after
        //   content grows (a new container appears, a fold animation
        //   ramps up, …) `pane_rect` undersizes the actual paint and
        //   `shrink_clip_rect(pane_rect)` would slice the body's far
        //   edge — exactly the symptom of "the side opposite the
        //   title is being clipped".
        // * Last frame's actual `painted_rect` is the ground truth
        //   for what fit on the previous render; unioning with
        //   `pane_rect` covers the case where content shrank (so
        //   `pane_rect` is now the larger, accurate bound).
        //
        // The union is the smallest rect that's known to contain
        // both the predicted bounds and the most recently observed
        // bounds, so a single frame of growth still renders fully
        // — only growth that exceeds last frame's paint by more than
        // one frame would clip, which is the rare transient.
        let clip_key = pane_id.with("mara_pane_painted_rect_for_clip");
        let last_painted_rect: MaraRect = MaraCtx::memory(ctx)
            .get_temp::<MaraRect>(clip_key)
            .unwrap_or(pane_rect_mara);
        let pane_clip_rect = pane_rect_mara.union(last_painted_rect);

        crate::backend::egui::area_for_host(AreaHost::new(area_id.into(), pane_pos, self.order))
            // `Order::Background` keeps the pane's drop shadow
            // BELOW the ribbon buttons — buttons paint over any
            // shadow bleed. Removes the need for a tight clip_rect
            // (which was slicing the title strip on the rail-side
            // edge by a couple of pixels).
            .movable(false)
            .interactable(true)
            .fade_in(false)
            .default_size(outer_size_egui)
            .show(ctx, |outer_ui| {
                // egui's Area constrains its content_ui.max_rect to
                // `state.size` from the PREVIOUS frame. During
                // animation that prev value is smaller than this
                // frame's `outer_size`, so anything that uses
                // `available_size` (e.g. `allocate_ui_with_layout`)
                // would clamp content too small and clip it.
                // Workaround: bypass `outer_ui` and create a child
                // with EXPLICIT `max_rect = pane_rect`, then
                // `allocate_rect` on the parent so its `min_rect`
                // reaches `pane_rect` and `state.size` (next frame)
                // matches our computed value.
                // Same rect as the outer `pane_rect`; recompute here
                // from `outer_ui.cursor()` so the inside of the
                // closure doesn't depend on the outer capture order.
                let pane_rect = egui::Rect::from_min_size(outer_ui.cursor().min, outer_size_egui);
                // Use the title-at-end layout DIRECTLY on the outer
                // child_ui — not via a `with_layout(bottom_up)` inside
                // a top_down parent. egui tracks `min_rect` by union
                // with the parent's initial cursor (top-left for
                // top_down). When the title strip lands at the far
                // edge (Bottom/Right rails) and the body folds to 0,
                // the allocated strip sits at the bottom/right of
                // `pane_rect`. Union-ed with the parent's top-left
                // cursor, the resulting min_rect spans the FULL pane
                // height/width — and the Frame paints across the
                // whole pane instead of shrinking. Pushing the
                // bottom_up/right_to_left layout one level up so the
                // child_ui's cursor starts at the anchor edge keeps
                // min_rect tight to the strip.
                let title_at_end = title_side.is_at_end();
                let direction = if horizontal_strip {
                    if title_at_end {
                        crate::layout::StackDirection::BottomUp
                    } else {
                        crate::layout::StackDirection::TopDown
                    }
                } else if title_at_end {
                    crate::layout::StackDirection::RightToLeft
                } else {
                    crate::layout::StackDirection::LeftToRight
                };
                let mut child_ui = crate::backend::egui::child_ui_for_region(
                    outer_ui,
                    crate::layout::ChildRegion::new(
                        pane_rect.into(),
                        direction,
                        crate::layout::StackAlign::Min,
                    ),
                );
                // ── Hierarchical clip invariant (root) ──
                //
                // The pane Area is created at `Order::Background` with
                // a screen-sized clip rect by default — descendants
                // (containers, pods, individual widgets) inherit that
                // wide-open clip via egui's painter cloning in
                // `new_child` and `painter_at`, which means a pod
                // whose content overflows can paint past the pane's
                // painted bounds. Narrowing the child_ui's clip
                // anchors the top of the hierarchy: every descendant
                // now has `clip ⊆ pane_clip_rect` automatically (each
                // level's `painter_at(rect)` intersects against the
                // inherited clip). Use `shrink_clip_rect` rather than
                // `set_clip_rect` so we INTERSECT with whatever egui
                // set on the Area — `set_` can grow the clip and is
                // footgunny per egui docs.
                child_ui.shrink_clip_rect(pane_clip_rect.into());
                {
                    let ui = &mut child_ui;
                    let theme = style::theme();
                    let fill: Color32 = if theme.pane.fill_visible {
                        style::fill_for(style::FillRole::Pane, pane_accent).into()
                    } else {
                        Color32::TRANSPARENT
                    };
                    let shadow = egui::epaint::Shadow {
                        offset: [0, theme.pane.shadow_y],
                        blur: theme.pane.shadow_blur,
                        spread: 0,
                        color: Color32::from_black_alpha(115),
                    };
                    let frame_response = egui::Frame {
                        inner_margin: egui::Margin::same(PANE_INNER_MARGIN as i8),
                        outer_margin: egui::Margin::ZERO,
                        fill,
                        stroke: style::stroke_for(style::StrokeRole::WidgetBorder, pane_accent)
                            .into(),
                        corner_radius: style::radius_for(style::RadiusRole::Pane).into(),
                        shadow,
                    }
                    .show(ui, |ui| {
                        self.lay_out_flex(ui, body);
                    });
                    // The Frame's response.rect IS the painted outer
                    // rect (= content_min_rect + frame margins). Use it
                    // to position the resize handles below — they sit
                    // exactly on the painted edge, even when fold
                    // animation has shrunk the frame.
                    painted_rect.set(frame_response.response.rect);
                    // Persist this frame's actual painted rect so next
                    // frame's `pane_clip_rect` (= pane_rect ∪
                    // last_painted_rect) accurately bounds the body —
                    // critical for the first frame after content grows,
                    // where `pane_rect` lags by one frame and would
                    // otherwise slice the body's far edge.
                    crate::memory::MaraMemoryCtx::new(outer_ui.ctx())
                        .set_temp(clip_key, MaraRect::from(frame_response.response.rect));
                }
                // Publish this pane's painted rect to the global
                // ctx-data list so host integrations (e.g.
                // `bevy_mara::EguiInputAbsorbPlugin`) can ask
                // "is the cursor over any mara pane?" without
                // going through egui's quirky `layer_id_at`.
                maybe_reset_published_pane_rects(outer_ui.ctx());
                publish_pane_rect(outer_ui.ctx(), MaraRect::from(painted_rect.get()));
                // Custom debug inspector — paint the pane's frame
                // rect with a `Pane[<title>]` label when the user
                // toggles the inspector and hovers inside.
                crate::debug::tag(
                    outer_ui.ctx(),
                    painted_rect.get(),
                    format!("Pane[{}]", pane_title_dbg),
                );
                let _ = outer_ui.allocate_rect(pane_rect, egui::Sense::hover());

                // ── Resize handles (in-Area) ──
                //
                // Registered directly on the pane's own `outer_ui`
                // (`Order::Background`) instead of a separate Area.
                // Within a single layer, egui's hit-test prefers
                // LATER-registered widgets — so these `interact`
                // calls (added after the Frame's title widgets)
                // win for clicks at the handle rects, while clicks
                // at non-handle positions fall through to the
                // earlier-registered title widgets and trigger the
                // fold toggle as expected. Putting the handles in a
                // separate `Order::Middle` Area broke this on
                // corner-anchored panes whose Area state collapsed
                // to a degenerate clip.
                if pane_resize.flow || pane_resize.span {
                    let mut backend = crate::mui::MaraBackend::Egui(
                        crate::backend::egui::EguiUiBackend::new(outer_ui),
                    );
                    let mut mara = crate::MaraUi::over(&mut backend, pane_accent);
                    paint_resize_handles_inline(
                        &mut mara,
                        pane_id,
                        pane_accent.into(),
                        pane_anchor,
                        pane_resize,
                        painted_rect.get().into(),
                    );
                }
            });
    }

    /// Inner flex layout: split the pane Ui into a fixed-thickness
    /// title strip + a content-sized body. Direction comes from
    /// `title_side` (per-anchor) — horizontal strips need a vertical
    /// flex, vertical strips need a horizontal flex.
    ///
    /// Sizing is content-driven: the span axis (the dimension the
    /// title spans) is locked per anchor, while the flow axis (the
    /// dimension perpendicular to the title) is left free so the
    /// pane is exactly tall/wide enough to fit the title strip plus
    /// whatever the body closure allocates. Empty body → pane is
    /// just the strip.
    fn lay_out_flex<'spec>(self, ui: &mut egui::Ui, body: impl FnOnce(&mut PaneBody<'_, 'spec>)) {
        let Pane {
            id,
            title,
            anchor,
            accent,
            resize,
            order: _,
        } = self;
        let accent: Color32 = accent.into();
        let title_side = anchor.title_side();
        let horizontal_strip = title_side.is_horizontal_strip();

        // Cross axis = the dimension the title strip spans. Tracks
        // the SAME `span_outer` value the internal pane renderer used to size
        // the pane Area. The internal renderer publishes the post-clamp
        // effective span under `mara_pane_effective_span` for this
        // pane id; that value already accounts for the screen-edge
        // / perpendicular-ribbon clamp so the Frame paints flush
        // with the Area's clipped rect. Falling back to the raw
        // `user_span` (or `PANE_OUTER_SPAN`) only matters on the
        // first frame before `show` has published.
        let span_outer = crate::memory::MaraMemoryCtx::new(ui.ctx())
            .get_temp::<f32>(id.with("mara_pane_effective_span"))
            .unwrap_or_else(|| {
                if resize.span {
                    user_span(ui.ctx(), id)
                } else {
                    PANE_OUTER_SPAN
                }
            });
        let span_inner = span_outer - PANE_FRAME_CHROME;

        let flex_spec = PaneFlexSpec::new(
            horizontal_strip,
            span_inner,
            TITLE_STRIP_THICKNESS,
            style::theme().section_outer_margin_flow_title as f32,
        );

        // Plain-egui layout (no flex). Cross axis is locked via
        // `set_max_*` so `ui.available_*` is stable for child
        // widgets; flow axis is content-driven by `body(ui)`. Title
        // strip and body are placed in the natural reading order
        // dictated by `title_at_end` (decided by internal pane rendering when
        // building the outer child_ui's layout).
        let title_text = title.clone();
        let paint_title_strip = |ui: &mut egui::Ui| {
            let alloc_rect: egui::Rect =
                crate::backend::egui::reserve_pane_title_slot(ui, flex_spec).into();
            {
                let ctx = ui.ctx().clone();
                let mut raw = crate::MaraUi::__internal_backend_from_raw(ui);
                let mut mara = crate::MaraUi::__internal_over(&mut raw, accent);
                title::paint_pane_title(
                    &mut mara,
                    &ctx,
                    alloc_rect.into(),
                    id,
                    &title_text,
                    anchor,
                    accent.into(),
                );
            }
        };

        // The outer child_ui already carries the correct layout
        // (top_down / bottom_up / left_to_right / right_to_left)
        // chosen by internal pane rendering so the cursor starts at the anchor
        // edge — see the comment there for why we *don't* rewrap in
        // a `with_layout(bottom_up)` here. We just clamp the cross
        // axis and zero the item-spacing.
        crate::backend::egui::apply_pane_flex_spec(ui, flex_spec);
        // SAME order in both directions: title FIRST (lands at the
        // anchor edge thanks to the layout direction), body SECOND
        // (fills outward). Reversed layouts handle visual placement
        // automatically — `bottom_up` puts first-allocated at the
        // BOTTOM, `right_to_left` at the RIGHT, etc.
        paint_title_strip(ui);
        // Extra breathing space between the pane title strip and the
        // FIRST container. The container's own
        // `section_outer_margin_flow_title` already gives a small gap
        // (3 px PRO / 6 px GAME), but the user wants roughly DOUBLE
        // that on the first container only — without affecting
        // inter-container gaps. Allocating it here, between the title
        // strip and the body callback, hits exactly the first container
        // (no other paint runs between title and body) and leaves
        // every subsequent container's stacking gap unchanged.
        if flex_spec.body_gap > 0.0 {
            crate::MaraUi::__internal_over(
                &mut crate::MaraUi::__internal_backend_from_raw(ui),
                accent,
            )
            .add_space(crate::layout::SpaceSpec::vertical(flex_spec.body_gap));
        }
        let mut body = Some(body);
        let mut render_body = |ui: &mut egui::Ui| {
            // Reset per-frame drag bookkeeping (current cache + section
            // idx counter). Snapshot from prev frame stays available
            // for size lookups.
            drag::begin_frame(ui.ctx(), id);
            dots::clear_container_dot_rects(ui.ctx(), id);

            // Update cursor BEFORE body runs so `Normal::show`'s
            // target_idx computation sees this frame's cursor.
            let pre_body_drag = drag::state(ui.ctx(), id);
            if let (Some(item), Some(pos)) = (
                pre_body_drag.item,
                MaraCtx::input(ui.ctx()).interact_pointer.map(Into::into),
            ) {
                drag::set_drag(
                    ui.ctx(),
                    id,
                    drag::DragState {
                        item: Some(item),
                        cursor: Some(pos),
                    },
                );
            }

            // Wrap the body Ui in the typed `PaneBody` builder — the
            // user closure only sees the typed API, never the raw Ui.
            // After the closure returns, `PaneBody::finish` dispatches
            // the accumulated container specs through `render_containers`.
            tab_drag::begin_frame(ui.ctx(), id.into());
            let mut pane_body = PaneBody::new(ui, id, anchor, accent);
            let body = body
                .take()
                .expect("pane body renderer must only be called once");
            body(&mut pane_body);
            let _ = pane_body.finish();

            // Stack axis: matches `body` layout direction — BottomRail /
            // TopRail panes stack vertically (Y), LeftRail / RightRail
            // stack horizontally (X).
            let horizontal_stack = !title_side.is_horizontal_strip();

            // ── Trailing ghost gap ──
            //
            // If the cursor's target slot is AFTER the last rendered
            // container (target == total non-dragged), paint the ghost
            // gap inline at the end of the body layout. The inline gaps
            // inside `Normal::show` handle every other position.
            let drag_state = drag::state(ui.ctx(), id);
            if let Some(dragged_id) = drag_state.item
                && !drag::ghost_gap_suppressed(ui.ctx(), id)
            {
                let snap = drag::target_cache(ui.ctx(), id);
                let total = drag::current_cache(ui.ctx(), id).len();
                let cursor = MaraCtx::input(ui.ctx())
                    .interact_pointer
                    .map(Into::into)
                    .or(drag_state.cursor);
                if let Some(c) = cursor {
                    let cursor_axis = if horizontal_stack { c.x } else { c.y };
                    let target_idx =
                        drag::compute_target(&snap, dragged_id, cursor_axis, horizontal_stack);
                    if target_idx >= total
                        && let Some(entry) = drag::dragged_entry(&snap, dragged_id)
                    {
                        drag::paint_ghost_gap_entry_inline(
                            &mut crate::MaraUi::__internal_over(
                                &mut crate::MaraUi::__internal_backend_from_raw(ui),
                                accent,
                            ),
                            entry,
                            accent.into(),
                            horizontal_stack,
                        );
                    }
                }
            }

            // ── Build snapshot for next frame ──
            //
            // current cache (this frame's renders) + dragged entry
            // carried forward from prev snapshot.
            drag::finalize_snapshot(ui.ctx(), id);

            // ── Floating preview + cursor + release commit ──
            if let Some(dragged_id) = drag_state.item {
                let snap = drag::target_cache(ui.ctx(), id);
                let cursor = MaraCtx::input(ui.ctx())
                    .interact_pointer
                    .map(Into::into)
                    .or(drag_state.cursor);
                if let Some(c) = cursor {
                    drag::paint_drag_preview(ui.ctx(), id, &snap, dragged_id, c, accent.into());
                    crate::MaraUi::__internal_over(
                        &mut crate::MaraUi::__internal_backend_from_raw(ui),
                        accent,
                    )
                    .set_cursor_icon(crate::layout::CursorIcon::Grabbing);
                }

                if MaraCtx::input(ui.ctx()).any_released {
                    if let Some(c) = cursor {
                        let cursor_axis = if horizontal_stack { c.x } else { c.y };
                        let target_idx =
                            drag::compute_target(&snap, dragged_id, cursor_axis, horizontal_stack);
                        let defaults: Vec<Id> = snap.iter().map(|e| e.id).collect();
                        let mut order = drag::section_order_for(ui.ctx(), id, &defaults);
                        order.retain(|cid| *cid != dragged_id);
                        let clamped = target_idx.min(order.len());
                        order.insert(clamped, dragged_id);
                        drag::set_section_order(ui.ctx(), id, order);
                    }
                    drag::clear_drag(ui.ctx(), id);
                }
            }

            // ── Tab drag: preview + commit-on-release ──
            if let Some(tab_drag_state) = tab_drag::drag_state(ui.ctx(), id.into()) {
                let cursor = MaraCtx::input(ui.ctx())
                    .pointer
                    .map(Into::into)
                    .or(tab_drag_state.cursor);
                if let Some(c) = cursor {
                    // Persist the cursor pos so next frame can paint at
                    // the right spot even if egui drops the input.
                    tab_drag::set_drag(
                        ui.ctx(),
                        id.into(),
                        tab_drag::TabDragState {
                            cursor: Some(c),
                            ..tab_drag_state
                        },
                    );
                    // Floating preview at the cursor, carrying the tab's
                    // own icon so the drag affordance doesn't turn into a
                    // blank accent card while crossing containers.
                    let preview_size = MaraVec2::new(28.0, 28.0);
                    tab_drag::paint_drag_preview(
                        ui.ctx(),
                        id.into(),
                        preview_size,
                        c,
                        accent.into(),
                        "",
                        tab_drag_state.icon,
                    );
                    crate::MaraUi::__internal_over(
                        &mut crate::MaraUi::__internal_backend_from_raw(ui),
                        accent,
                    )
                    .set_cursor_icon(crate::layout::CursorIcon::Grabbing);
                }
                if MaraCtx::input(ui.ctx()).any_released {
                    if let Some(c) = cursor
                        && let Some((tgt_cid, slot)) = tab_drag::find_drop_target_for_drag(
                            ui.ctx(),
                            id.into(),
                            c,
                            tab_drag_state,
                        )
                    {
                        tab_drag::commit_drop(
                            ui.ctx(),
                            id.into(),
                            tab_drag_state.tab_id,
                            tab_drag_state.source_container,
                            tgt_cid,
                            slot,
                        );
                    }
                    tab_drag::clear_drag(ui.ctx(), id.into());
                }
            }
        };

        let body_scroll_enabled = crate::memory::MaraMemoryCtx::new(ui.ctx())
            .get_temp::<bool>(id.with("mara_pane_body_scroll_enabled"))
            .unwrap_or(false);
        if body_scroll_enabled {
            crate::backend::egui::show_pane_body_scroll_slot(
                ui,
                PaneBodyScrollSpec::new(
                    MaraId::from(id.with("mara_pane_body_scroll")),
                    horizontal_strip,
                    span_inner,
                ),
                |ui| {
                    render_body(ui);
                },
            );
        } else {
            render_body(ui);
        }
    }
}

/// Register the resize handles directly on the pane's own `Ui`
/// (Order::Background), skipping the separate `Order::Middle` Area
/// used by [`paint_resize_handles`]. Within a single layer egui's
/// hit-test prefers later-registered widgets, so the drag handles
/// take precedence at their small edge rects without intercepting
/// clicks anywhere else (which would break the container title-
/// strip clicks that fold / unfold).
fn paint_resize_handles_inline(
    mara: &mut crate::MaraUi<'_>,
    pane_id: Id,
    accent: MaraColor32,
    anchor: PaneAnchor,
    resize: PaneResize,
    pane_rect: MaraRect,
) {
    let title_side = anchor.title_side();
    let horizontal_strip = title_side.is_horizontal_strip();
    let zone = anchor.zone();
    paint_resize_handles_inner(
        mara,
        pane_id,
        accent,
        anchor,
        resize,
        pane_rect,
        title_side,
        horizontal_strip,
        zone,
    );
}

#[allow(clippy::too_many_arguments)]
fn paint_resize_handles_inner(
    mara: &mut crate::MaraUi<'_>,
    pane_id: Id,
    accent: MaraColor32,
    anchor: PaneAnchor,
    resize: PaneResize,
    pane_rect: MaraRect,
    title_side: TitleSide,
    horizontal_strip: bool,
    _zone: RailZone,
) {
    // Container-derived lower bounds, computed once per call.
    //
    //   * Horizontal-strip pane (TOP / BOTTOM rail): containers stack
    //     along the pane's flow axis, all sharing the same cross
    //     extent. Min cross = max of registered container widths.
    //     Min main has no container contribution → keep the global
    //     [`MIN_USER_FLOW`] floor.
    //   * Vertical-strip pane (LEFT / RIGHT rail): containers stack
    //     along the pane's flow axis (horizontal). Each one takes
    //     a slice, so min main = SUM of registered widths. Cross
    //     keeps the global [`MIN_USER_SPAN`] floor.
    let container_mins = container_min_widths(mara.ctx(), pane_id);
    let container_min_flows_v = container_min_flows(mara.ctx(), pane_id);
    let max_min = container_mins.iter().copied().fold(0.0_f32, f32::max);
    let sum_min: f32 = container_mins.iter().sum();
    let sum_min_flow: f32 = container_min_flows_v.iter().sum();
    let min_flow_bound = if horizontal_strip {
        // Horizontal-strip pane: containers stack along the pane's
        // flow axis. Floor = sum of each container's chrome-only
        // flow extent (title + title/body gap + padding + stroke +
        // outer margins) — already pre-computed at the active
        // openness, so a pane with all containers folded can still
        // shrink almost to title-only. Below this, egui's
        // `available_rect_before_wrap` collapses to zero-height and
        // subsequent Frame allocations overhang their starting
        // cursor by their bottom-side chrome, making the last
        // container paint over the previous one.
        sum_min_flow.max(MIN_USER_FLOW)
    } else {
        sum_min.max(MIN_USER_FLOW)
    };
    let min_span_bound = if horizontal_strip {
        max_min.max(MIN_USER_SPAN)
    } else {
        MIN_USER_SPAN
    };
    let accent_mara = accent;
    let paint_indicator =
        |mara: &mut crate::MaraUi<'_>, rect: MaraRect, hovered: bool, dragged: bool| {
            if let Some(cmd) = pane_resize_indicator_paint_cmd(rect, accent_mara, hovered, dragged)
            {
                mara.paint(cmd);
            }
        };
    let pane_rect_mara = pane_rect;

    // ── Main-axis handle (inner edge) ──
    if resize.flow {
        let handle_rect_mara = pane_main_resize_handle_rect(pane_rect_mara, title_side);
        let id = pane_id.with("mara_pane_resize_main");
        let resp = { mara.interact(handle_rect_mara, id, crate::layout::Sense::ClickAndDrag) };
        let hovered = resp.hovered();
        let dragged = resp.dragged();
        if hovered || dragged {
            mara.set_cursor_icon(pane_main_resize_cursor(horizontal_strip));
        }
        paint_indicator(mara, handle_rect_mara, hovered, dragged);
        if dragged {
            let delta = resp.drag_delta;
            let flow_delta = if horizontal_strip { delta.y } else { delta.x };
            // Whether the main-anchored edge is at the FAR side of
            // the pane (so the handle sits at the NEAR side and
            // dragging it OUTWARD means a negative delta). This is
            // a property of the title side — `Bottom` / `Right`
            // titles always pin the main-max edge, regardless of
            // which rail the pane lives on.
            let invert = matches!(title_side, TitleSide::Bottom | TitleSide::Right);
            let signed = if invert { -flow_delta } else { flow_delta };
            let cur = user_flow(mara.ctx(), pane_id);
            // Container-derived floor: vertical-strip panes (LEFT /
            // RIGHT rails) refuse to shrink below the SUM of their
            // containers' min widths so each container fits.
            let new_v = (cur + signed).max(min_flow_bound);
            set_user_flow(mara.ctx(), pane_id, new_v);
        }
    }

    // ── Cross-axis handle(s) ──
    //
    // The pane's span axis is parallel to the rail. Which cross
    // edges are resizable depends on the anchor zone:
    //   * `Start` is anchored at the cross-min edge, so only the
    //     cross-max side is resizable.
    //   * `End` is anchored at the cross-max edge, so only the
    //     cross-min side is resizable.
    //   * `Middle` is centred — both cross sides are resizable and
    //     the pane grows symmetrically about its centre.
    if resize.span {
        // For horizontal-strip panes (TOP / BOTTOM rails) the cross
        // axis is X; for vertical-strip panes (LEFT / RIGHT rails)
        // the span axis is Y.
        let (span_min_rect, span_max_rect) =
            pane_span_resize_handle_rects(pane_rect_mara, horizontal_strip);

        let icon = pane_span_resize_cursor(horizontal_strip);

        let mut handle_one = |rect: MaraRect, salt: &'static str, sign: f32, factor: f32| {
            let id = pane_id.with(salt);
            let resp = mara.interact(rect, id, crate::layout::Sense::ClickAndDrag);
            let hovered = resp.hovered();
            let dragged = resp.dragged();
            if hovered || dragged {
                mara.set_cursor_icon(icon);
            }
            paint_indicator(mara, rect, hovered, dragged);
            if dragged {
                let delta = resp.drag_delta;
                let span_delta = if horizontal_strip { delta.x } else { delta.y };
                let signed = sign * span_delta * factor;
                let cur = user_span(mara.ctx(), pane_id);
                // Container-derived floor: horizontal-strip panes
                // (TOP / BOTTOM rails) refuse to shrink below the
                // largest container min width so the widest
                // container still fits cross-wise.
                let new_v = (cur + signed).max(min_span_bound);
                set_user_span(mara.ctx(), pane_id, new_v);
            }
        };

        // Pick which cross side(s) are user-resizable from the
        // pane's actual anchor alignment, not from the rail zone.
        // `LeftRail::End` for instance has a horizontal-strip title
        // (cross = X), and its X-min edge is anchored to the LEFT
        // rail — so the resizable side is the X-MAX (right edge),
        // even though the rail zone is `End`.
        let span_align = layout::anchor_span_align(anchor, horizontal_strip);
        match span_align {
            layout::AxisAlign::Min => {
                // cross-min anchored → grow from cross-max edge.
                handle_one(span_max_rect, "mara_pane_resize_cross_max", 1.0, 1.0);
            }
            layout::AxisAlign::Max => {
                // cross-max anchored → grow from cross-min edge
                // (drag in the negative direction = grow → flip
                // sign).
                handle_one(span_min_rect, "mara_pane_resize_cross_min", -1.0, 1.0);
            }
            layout::AxisAlign::Center => {
                // Centred on cross — both edges move symmetrically
                // about the centre, so each handle's drag delta
                // contributes 2× to the cross extent.
                handle_one(span_max_rect, "mara_pane_resize_cross_max", 1.0, 2.0);
                handle_one(span_min_rect, "mara_pane_resize_cross_min", -1.0, 2.0);
            }
        }
    }
}

fn pane_main_resize_handle_rect(pane_rect: MaraRect, title_side: TitleSide) -> MaraRect {
    let t = RESIZE_HANDLE_THICKNESS;
    match title_side {
        // Title at the top → inner edge is the bottom.
        TitleSide::Top => MaraRect::from_min_max(
            MaraPos2::new(pane_rect.min.x, pane_rect.max.y - t),
            pane_rect.max,
        ),
        // Title at the bottom → inner edge is the top.
        TitleSide::Bottom => MaraRect::from_min_max(
            pane_rect.min,
            MaraPos2::new(pane_rect.max.x, pane_rect.min.y + t),
        ),
        // Title at the left → inner edge is the right.
        TitleSide::Left => MaraRect::from_min_max(
            MaraPos2::new(pane_rect.max.x - t, pane_rect.min.y),
            pane_rect.max,
        ),
        // Title at the right → inner edge is the left.
        TitleSide::Right => MaraRect::from_min_max(
            pane_rect.min,
            MaraPos2::new(pane_rect.min.x + t, pane_rect.max.y),
        ),
    }
}

fn pane_span_resize_handle_rects(
    pane_rect: MaraRect,
    horizontal_strip: bool,
) -> (MaraRect, MaraRect) {
    let t = RESIZE_HANDLE_THICKNESS;
    if horizontal_strip {
        (
            MaraRect::from_min_max(
                pane_rect.min,
                MaraPos2::new(pane_rect.min.x + t, pane_rect.max.y),
            ),
            MaraRect::from_min_max(
                MaraPos2::new(pane_rect.max.x - t, pane_rect.min.y),
                pane_rect.max,
            ),
        )
    } else {
        (
            MaraRect::from_min_max(
                pane_rect.min,
                MaraPos2::new(pane_rect.max.x, pane_rect.min.y + t),
            ),
            MaraRect::from_min_max(
                MaraPos2::new(pane_rect.min.x, pane_rect.max.y - t),
                pane_rect.max,
            ),
        )
    }
}

fn pane_resize_indicator_paint_cmd(
    rect: MaraRect,
    accent: MaraColor32,
    hovered: bool,
    dragged: bool,
) -> Option<crate::paint::PaintCmd> {
    let alpha: u8 = if dragged {
        180
    } else if hovered {
        110
    } else {
        return None; // fully invisible at rest
    };
    Some(crate::paint::PaintCmd::RectFilled {
        rect,
        corner: crate::vocab::CornerRadius::ZERO,
        fill: MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), alpha),
    })
}

fn pane_main_resize_cursor(horizontal_strip: bool) -> crate::layout::CursorIcon {
    if horizontal_strip {
        crate::layout::CursorIcon::ResizeVertical
    } else {
        crate::layout::CursorIcon::ResizeHorizontal
    }
}

fn pane_span_resize_cursor(horizontal_strip: bool) -> crate::layout::CursorIcon {
    if horizontal_strip {
        crate::layout::CursorIcon::ResizeHorizontal
    } else {
        crate::layout::CursorIcon::ResizeVertical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_pane_rects_are_mara_vocab_for_host_firewalls() {
        let ctx = egui::Context::default();
        let raw = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(30.0, 50.0));

        publish_pane_rect(&ctx, raw);
        assert_eq!(
            __internal_published_pane_rects(&ctx),
            vec![MaraRect::from(raw)]
        );

        __internal_clear_published_pane_rects(&ctx);
        assert_eq!(
            __internal_published_pane_rects(&ctx),
            Vec::<MaraRect>::new()
        );
    }

    #[test]
    fn user_extents_sanitize_non_finite_values() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");

        set_user_flow(&ctx, pane_id, f32::NAN);
        assert_eq!(user_flow(&ctx, pane_id), DEFAULT_FLOW_OPEN);
        set_user_flow(&ctx, pane_id, f32::INFINITY);
        assert_eq!(user_flow(&ctx, pane_id), DEFAULT_FLOW_OPEN);

        set_user_span(&ctx, pane_id, f32::NAN);
        assert_eq!(user_span(&ctx, pane_id), PANE_OUTER_SPAN);
        set_user_span(&ctx, pane_id, f32::NEG_INFINITY);
        assert_eq!(user_span(&ctx, pane_id), PANE_OUTER_SPAN);
    }

    #[test]
    fn user_extents_clamp_to_safe_bounds() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");

        set_user_flow(&ctx, pane_id, -100.0);
        assert_eq!(user_flow(&ctx, pane_id), MIN_USER_FLOW);
        set_user_flow(&ctx, pane_id, MAX_USER_FLOW * 2.0);
        assert_eq!(user_flow(&ctx, pane_id), MAX_USER_FLOW);

        set_user_span(&ctx, pane_id, -100.0);
        assert_eq!(user_span(&ctx, pane_id), MIN_USER_SPAN);
        set_user_span(&ctx, pane_id, MAX_USER_SPAN * 2.0);
        assert_eq!(user_span(&ctx, pane_id), MAX_USER_SPAN);
    }

    #[test]
    fn body_extra_flow_accumulates_and_clears_per_pane() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");

        publish_body_extra_flow(&ctx, pane_id, 6.0);
        publish_body_extra_flow(&ctx, pane_id, 4.0);
        assert_eq!(published_body_extra_flow(&ctx, pane_id), 10.0);

        clear_container_min_widths(&ctx, pane_id);
        assert_eq!(published_body_extra_flow(&ctx, pane_id), 0.0);
    }

    #[test]
    fn body_extra_flow_sanitizes_invalid_values() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");

        publish_body_extra_flow(&ctx, pane_id, f32::NAN);
        publish_body_extra_flow(&ctx, pane_id, f32::INFINITY);
        publish_body_extra_flow(&ctx, pane_id, -42.0);
        publish_body_extra_flow(&ctx, pane_id, 12.0);

        assert_eq!(published_body_extra_flow(&ctx, pane_id), 12.0);
    }

    #[test]
    fn pane_main_resize_handle_rect_uses_mara_geometry() {
        let pane_rect =
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(90.0, 70.0));
        let t = RESIZE_HANDLE_THICKNESS;

        assert_eq!(
            pane_main_resize_handle_rect(pane_rect, TitleSide::Top),
            MaraRect::from_min_max(MaraPos2::new(10.0, 70.0 - t), MaraPos2::new(90.0, 70.0))
        );
        assert_eq!(
            pane_main_resize_handle_rect(pane_rect, TitleSide::Bottom),
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(90.0, 20.0 + t))
        );
        assert_eq!(
            pane_main_resize_handle_rect(pane_rect, TitleSide::Left),
            MaraRect::from_min_max(MaraPos2::new(90.0 - t, 20.0), MaraPos2::new(90.0, 70.0))
        );
        assert_eq!(
            pane_main_resize_handle_rect(pane_rect, TitleSide::Right),
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(10.0 + t, 70.0))
        );
    }

    #[test]
    fn pane_span_resize_handle_rects_use_mara_geometry() {
        let pane_rect =
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(90.0, 70.0));
        let t = RESIZE_HANDLE_THICKNESS;

        let (span_min, span_max) = pane_span_resize_handle_rects(pane_rect, true);
        assert_eq!(
            span_min,
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(10.0 + t, 70.0))
        );
        assert_eq!(
            span_max,
            MaraRect::from_min_max(MaraPos2::new(90.0 - t, 20.0), MaraPos2::new(90.0, 70.0))
        );

        let (span_min, span_max) = pane_span_resize_handle_rects(pane_rect, false);
        assert_eq!(
            span_min,
            MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(90.0, 20.0 + t))
        );
        assert_eq!(
            span_max,
            MaraRect::from_min_max(MaraPos2::new(10.0, 70.0 - t), MaraPos2::new(90.0, 70.0))
        );
    }

    #[test]
    fn pane_resize_indicator_lowers_to_mara_paint_command() {
        let rect = MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(90.0, 70.0));
        let accent = MaraColor32::from_rgb(1, 2, 3);

        assert!(pane_resize_indicator_paint_cmd(rect, accent, false, false).is_none());

        let Some(crate::paint::PaintCmd::RectFilled {
            rect: painted_rect,
            fill,
            ..
        }) = pane_resize_indicator_paint_cmd(rect, accent, true, false)
        else {
            panic!("hovered resize indicator should lower to a filled Mara rect");
        };
        assert_eq!(painted_rect, rect);
        assert_eq!(fill, MaraColor32::from_rgba_unmultiplied(1, 2, 3, 110));

        let Some(crate::paint::PaintCmd::RectFilled { fill, .. }) =
            pane_resize_indicator_paint_cmd(rect, accent, true, true)
        else {
            panic!("dragged resize indicator should lower to a filled Mara rect");
        };
        assert_eq!(fill, MaraColor32::from_rgba_unmultiplied(1, 2, 3, 180));
    }

    #[test]
    fn pane_resize_cursors_are_backend_neutral_axis_policy() {
        assert_eq!(
            pane_main_resize_cursor(true),
            crate::layout::CursorIcon::ResizeVertical
        );
        assert_eq!(
            pane_main_resize_cursor(false),
            crate::layout::CursorIcon::ResizeHorizontal
        );
        assert_eq!(
            pane_span_resize_cursor(true),
            crate::layout::CursorIcon::ResizeHorizontal
        );
        assert_eq!(
            pane_span_resize_cursor(false),
            crate::layout::CursorIcon::ResizeVertical
        );
    }

    #[test]
    fn published_container_cids_are_frame_local() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        let first = Id::new("first");
        let second = Id::new("second");

        publish_container_cid(&ctx, pane_id, first);
        publish_container_cid(&ctx, pane_id, second);
        assert_eq!(published_container_cids(&ctx, pane_id), vec![first, second]);

        clear_container_min_widths(&ctx, pane_id);
        assert!(published_container_cids(&ctx, pane_id).is_empty());
    }

    #[test]
    fn published_container_cids_reject_duplicates_in_one_frame() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        let container = Id::new("container");

        publish_container_cid(&ctx, pane_id, container);
        let duplicate = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            publish_container_cid(&ctx, pane_id, container);
        }));
        assert!(duplicate.is_err());
    }

    #[test]
    fn pane_titles_must_be_visible() {
        let result = std::panic::catch_unwind(|| {
            let _ = Pane::new(
                egui::Id::new("blank-pane-title"),
                " ",
                PaneAnchor::TopRail(RailZone::Middle),
                Color32::WHITE,
            );
        });

        assert!(result.is_err());
    }
}
