//! `Normal` container — a flex-based, two-part block (title zone +
//! body zone) dropped into a [`crate::pane::Pane`] body. The
//! container's title sits on the **same side** of the block as the
//! parent pane's title strip, so nested chrome chords with the pane
//! chrome.
//!
//! Layout is plain egui (no flex). Title strip hit-testing goes
//! through the Mara backend contract; the body is rendered into a child UI
//! whose `max_rect` is the FULL body extent and whose `clip_rect`
//! lerps with the memory contract's `animate_bool(...)` — same recipe egui's
//! `CollapsingState::show_body_unindented` uses.
//!
//! ```ignore
//! Normal::new("Properties", anchor, accent).show(ui, |ui| {
//!     ui.label("body content");
//! });
//! ```

use crate::memory::MaraAnim;
use crate::vocab::Id;
use egui::{Color32, Frame, Rect, Ui};

use super::body::Body;
use crate::icons::Icon;
use crate::paint::{PaintCmd, TextFamily, TextRun};
use crate::pane::{self, PaneAnchor, TitleSide};
use crate::style;
use crate::vocab::{
    Align2 as MaraAlign2, Color32 as MaraColor32, CornerRadius as MaraCornerRadius, Id as MaraId,
    Pos2 as MaraPos2, Rect as MaraRect, Stroke as MaraStroke, Vec2 as MaraVec2,
};

/// Title-bar thickness (perpendicular to the strip's long axis).
pub const TITLE_ZONE_THICKNESS: f32 = 22.0;
/// Inset between strip edge and the title text's reading-start.
pub const TITLE_INSET: f32 = 6.0;
/// Inset on each end of the title/body divider line so it stops
/// short of the container's frame corners.
pub const DIVIDER_INSET: f32 = 6.0;
/// Padding on EACH side of the divider — `TITLE_BODY_GAP_HALF` of
/// breathing space between the title text and the divider line, and
/// the same on the body side. Total flex gap = 2 × this constant.
pub const TITLE_BODY_GAP_HALF: f32 = 4.0;
/// Padding between title strip and body (currently rendered as gap=0
/// so the hairline divider reads cleanly; kept as a knob for later
/// tuning).
const _BODY_PAD: f32 = 6.0;
/// Default span-axis size. Used as the container's locked cross
/// dimension — width for horizontal-title containers, height for
/// vertical-title containers. The MAIN axis stays content-driven
/// (capped via `Body::max_flow` for vertical-title to stop a body
/// like `text_input` from growing the pane unboundedly along X).
/// Pane's locked span axis matches this constant so the pane and
/// container share the same outer cross dimension.
pub const CONTAINER_DEFAULT_WIDTH: f32 = 280.0;
pub const CONTAINER_DEFAULT_HEIGHT: f32 = 280.0;
/// Default lower bound on a container's WIDTH. Bumped 30 % above
/// the old floating-pane minimum (= 220) so containers
/// don't open at a cramped slim width — vertical-strip panes
/// stack containers side-by-side, so a too-small default leaves
/// each one barely wider than its title strip until the user
/// drags. Used when a caller doesn't override via
/// [`Normal::min_width`]. The parent pane's resize handles consult
/// the maximum of its containers' min widths (span axis) or the
/// sum (flow axis) and refuse to shrink below it.
pub const CONTAINER_DEFAULT_MIN_WIDTH: f32 = 286.0;
// Container outer margins now come from the active theme:
//   `theme.section_outer_margin_main`  — flow-axis (between stacked
//      containers and between first container ↔ pane title strip).
//   `theme.section_outer_margin_span` — span-axis (between the
//      container's painted edge and the pane's left/right or
//      top/bottom chrome). PRO ≈ 3/3, GAME ≈ 9/1.

/// A labelled, single-body container. Build with [`Normal::new`],
/// then [`Normal::show`] each frame. The `anchor` is forwarded to
/// pick the title side; pass the same anchor the parent
/// [`crate::pane::Pane`] uses. The `accent` drives the frame fill,
/// border, and (in PRO theme) title text colour.
pub struct Normal {
    title: String,
    anchor: PaneAnchor,
    accent: Color32,
    /// Parent pane's id. Used to look up / toggle the shared
    /// `body_open` state and the animation's `openness`, so
    /// `Pane` and the container animate in lockstep.
    pane_id: Id,
    /// Optional title icon. Either a Fluent name or raw SVG markup.
    /// In PRO theme (`section_icon_at_end = false`) the icon is
    /// inlined into the title paint runs at the reading-start. In
    /// GAME theme (`section_icon_at_end = true`) it floats at the
    /// strip's far end and grows when the body unfolds.
    icon: Option<Icon<'static>>,
    /// Optional override for the body slot's flow-axis size. Default
    /// derives from `CONTAINER_DEFAULT_HEIGHT/WIDTH` minus chrome,
    /// which is right for one-container-per-pane layouts. When you
    /// stack multiple containers in a single pane, divide the pane's
    /// available main extent and pass each container its share via
    /// this builder so they don't all claim the full pane.
    body_flow: Option<f32>,
    /// Optional per-container override for the autofit cap
    /// (vertically-stacked containers in horizontal-strip panes —
    /// TM/BM) or the fixed default-flow size (horizontally-stacked
    /// containers in vertical-strip panes — LM/RM). Set via
    /// [`Normal::initial_flow`]. When `None`, the global
    /// [`crate::container::CONTAINER_AUTOFIT_CAP`] (= 8U) /
    /// [`crate::container::CONTAINER_HORIZONTAL_DEFAULT_FLOW`]
    /// (= 12U) apply.
    initial_flow: Option<f32>,
    /// Minimum WIDTH this container will accept. The parent pane's
    /// user-resize handles consult the registered minimums (one per
    /// container painted this frame) and stop shrinking once the
    /// pane reaches that bound. Defaults to
    /// [`CONTAINER_DEFAULT_MIN_WIDTH`] when not set.
    min_width: Option<f32>,
    /// `Some(side)` when [`Normal::show_tabs`] is rendering this
    /// container — folder-tabs project from `side` (chosen
    /// perpendicular to the title strip). The Frame reacts: the
    /// `side`-facing outer_margin is zeroed (so the active tab can
    /// extend across the container's painted edge with no gap) and
    /// the two corners adjacent to `side` are squared (so the
    /// active-tab notch lands on a flat edge). Stroke is kept — it
    /// reads as the container's outline framing the inactive empty
    /// tabs alongside it. Internal; not a builder.
    tabbed_strip_side: Option<TitleSide>,
    /// Per-instance override for the title row's thickness
    /// (perpendicular to the title strip's long axis). When `None`,
    /// the global [`TITLE_ZONE_THICKNESS`] applies. Set by the
    /// GAME-tabbed render path to double the title row so the
    /// icon + label tab buttons have room to stack vertically.
    title_thickness_override: Option<f32>,
    /// `true` to suppress the GAME-theme accent banner that
    /// otherwise paints across the title row. The GAME-tabbed
    /// render path sets this so each tab cell can paint its OWN
    /// background colour independently — without the banner under
    /// them, transparent inactive cells reveal the pane bg
    /// (instead of the title's accent fill bleeding through).
    suppress_banner: bool,
    reserve_tab_strip_in_parent: bool,
    /// Minimum body flow-axis size requested by container chrome
    /// outside the active tab content. Used by folder-tabbed
    /// containers so a short active tab body cannot collapse the
    /// container until side/top tab buttons are clipped away.
    min_body_flow: Option<f32>,
}

impl Normal {
    pub fn new(
        title: impl Into<String>,
        anchor: PaneAnchor,
        accent: Color32,
        pane_id: impl Into<Id>,
    ) -> Self {
        Self {
            title: title.into(),
            anchor,
            accent,
            pane_id: pane_id.into(),
            icon: None,
            body_flow: None,
            initial_flow: None,
            min_width: None,
            tabbed_strip_side: None,
            title_thickness_override: None,
            suppress_banner: false,
            reserve_tab_strip_in_parent: true,
            min_body_flow: None,
        }
    }

    /// Control whether folder tabs reserve parent layout space.
    ///
    /// Floating panes need this so auto-sizing can include the
    /// projected tab strip. Docked shelves already own a fixed
    /// viewport rect, and reserving the tab-strip union in the
    /// parent corrupts subsequent container placement after tab
    /// clicks/resizes.
    #[must_use]
    pub fn reserve_tab_strip_in_parent(mut self, reserve: bool) -> Self {
        self.reserve_tab_strip_in_parent = reserve;
        self
    }

    #[must_use]
    pub fn min_body_flow(mut self, flow: f32) -> Self {
        self.min_body_flow = Some(flow.max(0.0));
        self
    }

    /// Override the per-container default flow size. Replaces the
    /// global autofit cap (vertically-stacked: 8U) or the fixed
    /// default (horizontally-stacked: 12U) for this container only.
    /// Once the user drags the inter-container resize handle, the
    /// drag value takes precedence — the override is only used
    /// while the container is in its untouched state.
    ///
    /// Example: a console pane that should open at 12U instead of
    /// the default 8U auto-fit cap:
    ///
    /// ```ignore
    /// Normal::new(title, anchor, accent, cid)
    ///     .initial_flow(12.0 * mara_core::UNIT)
    ///     .show(ui, pods);
    /// ```
    pub fn initial_flow(mut self, flow: f32) -> Self {
        self.initial_flow = Some(flow.max(0.0));
        self
    }

    /// Set the container's minimum WIDTH. The parent pane's resize
    /// handles refuse to shrink the pane below the largest min
    /// width registered by its containers (or the sum, when the
    /// containers stack along the pane's flow axis). Defaults to
    /// [`CONTAINER_DEFAULT_MIN_WIDTH`] (220 px) when unset.
    pub fn min_width(mut self, w: f32) -> Self {
        self.min_width = Some(w.max(0.0));
        self
    }

    /// Attach a title icon. Accepts a Fluent icon name (e.g.
    /// `"settings"`) or raw SVG markup — `Icon::from(&str)` picks
    /// the right variant from the leading characters.
    pub fn icon(mut self, icon: impl Into<Icon<'static>>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    /// Override which edge hosts the folder-tab strip for tabbed
    /// containers. This is used by structural shelves so the right
    /// shelf can mirror the left shelf instead of sharing the same
    /// left-side tab strip.
    pub(crate) fn tabbed_strip_side(mut self, side: TitleSide) -> Self {
        self.tabbed_strip_side = Some(side);
        self
    }

    /// Override the body slot's flow-axis size. Use when stacking
    /// multiple containers in a pane so each gets a slice of the
    /// available main extent instead of all claiming the full pane.
    pub fn body_flow(mut self, main: f32) -> Self {
        self.body_flow = Some(main.max(0.0));
        self
    }

    /// Render the container with one or more pods stacked in the
    /// body slot. Containers only accept [`crate::pod::Pod`]s — raw
    /// widgets / closures are intentionally not supported. Returns
    /// one [`crate::pod::PodResponse`] per pod, in declaration
    /// order. Pass via `vec![pod1, pod2, ...]` or any
    /// `IntoIterator<Item = Pod>`.
    pub(crate) fn show(
        self,
        ui: &mut Ui,
        pods: impl IntoIterator<Item = crate::pod::Pod>,
    ) -> Vec<crate::pod::PodResponse> {
        // Push a per-container `id` salt so every widget the
        // container creates — `Frame::show`'s anonymous content_ui,
        // the body's `ScrollArea`, the pods' `text_input`s, etc. —
        // gets an id chain rooted at THIS container's `pane_id`.
        // Without this, multiple containers in the same parent
        // body_ui inherit `body_ui.id().with("child")` (egui's
        // default fallback in `Ui::new_child`), which is identical
        // across containers, and any widget inside would trip
        // egui's "id reused" check_for_id_clash on every frame.
        let pane_id = self.pane_id;
        ui.push_id(pane_id, |ui| self.show_inner(ui, pods)).inner
    }

    /// Render the container as a tabbed panel: folder-tabs project
    /// from the title-facing edge of the container. The ACTIVE tab
    /// fills with the container's body colour and merges into the
    /// container body (notching the container's outline at that
    /// x-range). Inactive tabs are 1 px outlined empty boxes
    /// floating above the container — the parent pane bg shows
    /// through them.
    ///
    /// 0 tabs: no-op (returns empty `Vec`).
    /// ≥1    : strip rendered, active tab's pods drive the body.
    ///
    /// Even a single-tab container paints its tab affordance. Shelves
    /// and panes rely on tab buttons as structural chrome, so hiding
    /// the strip for the one-tab case makes a tabbed container look
    /// like a different container kind and breaks drag/drop affordance
    /// consistency.
    ///
    /// The active index persists per-container (keyed on `pane_id`)
    /// across frames.
    ///
    /// v1 supports top-title containers (strip projects upward).
    /// Other anchors fall back to plain [`Normal::show`] of the
    /// active tab; full strip support per anchor will land later.
    pub(crate) fn show_tabs(
        self,
        ui: &mut Ui,
        tabs: Vec<super::Tab>,
    ) -> Vec<crate::pod::PodResponse> {
        if tabs.is_empty() {
            return Vec::new();
        }
        let pane_id = self.pane_id;
        ui.push_id(pane_id, |ui| self.show_inner_tabbed(ui, tabs))
            .inner
    }

    fn show_inner_tabbed(self, ui: &mut Ui, tabs: Vec<super::Tab>) -> Vec<crate::pod::PodResponse> {
        let tab_theme = style::theme().tabs;
        if matches!(tab_theme.layout, style::TabLayout::TitleRowSegmented) {
            return self.show_inner_tabbed_title_row(ui, tabs);
        }

        let mut tabs = tabs;
        // Container flow extent is locked to the tallest tab. Switching
        // tabs never resizes the container; shorter tabs sit with
        // trailing whitespace, taller content (= a tab content larger
        // than the user-set container size) overflows into the body's
        // own clip just like any over-tall section. The user can drag
        // the inter-container dot handle to resize the container; that
        // resize sticks regardless of which tab is active.
        let max_tab_body_h = max_tab_natural_body_h(&tabs);
        let title_side = self.anchor.title_side();
        // Strip ALWAYS sits perpendicular to the title:
        //   Top/Bottom title (horizontal title) → strip on Left.
        //   Left/Right title (vertical title)   → strip on Top.
        // The strip projects FROM that container edge, so corner
        // squaring + outer-margin zeroing live on the same edge.
        let strip_side = self.tabbed_strip_side.unwrap_or(match title_side {
            TitleSide::Top | TitleSide::Bottom => TitleSide::Left,
            TitleSide::Left | TitleSide::Right => TitleSide::Top,
        });

        let tab_meta: Vec<(String, Icon<'static>)> =
            tabs.iter().map(|t| (t.title.clone(), t.icon)).collect();
        let tab_ids: Vec<Id> = tabs.iter().map(|t| t.id()).collect();
        let active_idx_key = self.pane_id.with("mara_normal_active_tab");
        let active_idx = resolve_active_tab_idx(ui.ctx(), active_idx_key, &tab_ids);
        let active_pods = std::mem::take(&mut tabs[active_idx].pods);
        let active_title = tab_meta[active_idx].0.clone();
        let active_icon = tab_meta[active_idx].1;
        let tab_buttons_extent = (tabs.len() as f32 * tab_theme.tab_len)
            + (tabs.len().saturating_sub(1) as f32 * tab_theme.tab_gap);

        let me = Self {
            title: active_title,
            icon: Some(active_icon),
            tabbed_strip_side: Some(strip_side),
            min_body_flow: Some(
                self.min_body_flow
                    .unwrap_or(0.0)
                    .max(tab_buttons_extent)
                    .max(max_tab_body_h),
            ),
            ..self
        };
        let reserve_tab_strip_in_parent = me.reserve_tab_strip_in_parent;
        let accent = me.accent;
        let pane_id = me.pane_id;

        let strip_thickness = tab_theme.strip_thickness;

        // Title-side chrome that sits between the container's outer
        // edge and the body's leading edge:
        //   outer.title — Frame's outer-margin on the title-facing side
        //   TITLE_THK   — title row itself
        //   2 × HALF    — full title-body gap (both halves)
        // The strip's title-facing end is offset by this amount so
        // its first tab sits at the body's leading edge, not in the
        // gap or the title.
        let theme_now = style::theme();
        let outer_title = theme_now.section_outer_margin_flow_title as f32;
        let title_offset = outer_title
            + theme_now.container.title_zone_thickness
            + theme_now.container.title_body_gap_half * 2.0;

        // Carve the container's max_rect from the parent's available
        // rect, leaving STRIP_THICKNESS of headroom on the strip side.
        // The container itself renders inside this max_rect with the
        // parent's layout (TopDown / BottomUp / LeftToRight /
        // RightToLeft), so title/body stacking matches what a
        // non-tabbed Normal would produce under the same anchor.
        //
        // PRO themes leave breathing room on the TAB side/outer side.
        // Keep this at half the container's cross-axis outer margin:
        // the tab strip itself is outside the body frame, so including
        // body inner padding here makes left/right shelf containers
        // visibly off-centre (right shelves look shifted left and
        // left shelves look shifted right).
        let avail = crate::backend::egui::ui_available_rect(ui);
        let strip_outer_inset = tabbed_strip_outer_inset(tab_theme, &theme_now);
        let container_max_rect =
            tabbed_container_max_rect(avail, strip_side, strip_thickness, strip_outer_inset);
        crate::memory::MaraMemoryCtx::new(ui.ctx())
            .remove_temp::<egui::Rect>(pane::active_container_frame_rect_key());
        let mut child =
            crate::backend::egui::child_ui_with_current_layout_for_rect(ui, container_max_rect);
        let out = me.show(&mut child, active_pods);
        if pane::active_drag(ui.ctx())
            .and_then(|(_, state)| state.item)
            .map(|dragged| dragged == pane_id)
            .unwrap_or(false)
        {
            return out;
        }
        // Take the frame rect the body published, so a later container
        // cannot read a stale one.
        let used = {
            let key = pane::active_container_frame_rect_key();
            let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
            let rect = memory.get_temp::<egui::Rect>(key);
            memory.remove_temp::<egui::Rect>(key);
            rect
        }
        .unwrap_or_else(|| child.min_rect());

        // Place the strip ALIGNED to where the container actually
        // rendered. `used` already accounts for parent layout
        // direction (BottomUp / RightToLeft anchor at the
        // far edge), so deriving strip_rect from `used` keeps the
        // strip sitting flush against the container regardless of
        // anchor. The strip's title-facing END pulls back by
        // `title_offset` so it doesn't overlap the title row.
        let strip_rect = folder_tab_strip_rect(
            used.into(),
            strip_side,
            title_side,
            strip_thickness,
            title_offset,
        );
        paint_folder_tabs(
            ui,
            strip_rect,
            &tab_meta,
            &tab_ids,
            active_idx,
            accent,
            pane_id,
            active_idx_key,
            strip_side,
            tab_theme.tab_len,
            tab_theme.tab_gap,
            tab_theme.tab_overlap,
        );

        // Advance the parent layout past the union of strip + body.
        // `allocate_rect` takes a concrete rect (in absolute coords)
        // and updates the parent's used-rect / cursor accordingly,
        // which works for any layout direction (TopDown advances
        // downward, BottomUp upward, etc.).
        let union_rect = strip_rect.union(used.into());
        crate::memory::MaraMemoryCtx::new(ui.ctx())
            .set_temp::<egui::Rect>(pane::active_tabbed_container_rect_key(), union_rect.into());
        // Overwrite the drag snapshot entry: `me.show()` already
        // pushed the body-only frame rect to the parent pane's
        // current cache (it runs BEFORE `strip_rect` is known), so
        // the ghost-gap allocator would size itself to just the
        // body and clip the tab strip out. Re-push the full
        // strip+body union so the ghost matches what's actually
        // dragged.
        //
        // SKIP when THIS container is the one being dragged:
        // `show_with_body` early-returns in that case (so `used`
        // is the empty `new_child` rect, making `union_rect`
        // degenerate), and `finalize_snapshot` is designed to
        // carry the dragged container's PREV-frame rect forward
        // exactly when no push happens. A wrong push here would
        // overwrite the carry-forward with garbage.
        let dragging_self = pane::active_drag(ui.ctx())
            .and_then(|(_, s)| s.item)
            .map(|item| item == pane_id)
            .unwrap_or(false);
        if !dragging_self
            && let Some(parent_pane_id) =
                crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp::<Id>(pane::active_pane_key())
        {
            pane::push_rect_with_frame(
                ui.ctx(),
                parent_pane_id.into(),
                pane_id,
                union_rect.into(),
                Some(used),
            );
        }
        if reserve_tab_strip_in_parent {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::layout::UiBackend::reserve_rect(
                &mut backend,
                union_rect,
                crate::layout::Sense::Hover,
            );
        }
        out
    }

    /// Title-row tabbed render: the container's TITLE ROW is divided
    /// into N equal slots — one per tab — each slot showing icon
    /// stacked above label. The active slot fills with a contrasting
    /// colour against the title banner. The container's regular title
    /// text + floating icon are suppressed (replaced by the tab
    /// strip). Body still renders normally underneath.
    fn show_inner_tabbed_title_row(
        self,
        ui: &mut Ui,
        mut tabs: Vec<super::Tab>,
    ) -> Vec<crate::pod::PodResponse> {
        // Lock the body flow to the tallest tab so switching tabs
        // doesn't resize the container — see `show_inner_tabbed` for
        // the same logic.
        let max_tab_body_h = max_tab_natural_body_h(&tabs);
        let tab_meta: Vec<(String, Icon<'static>)> =
            tabs.iter().map(|t| (t.title.clone(), t.icon)).collect();
        let tab_ids: Vec<Id> = tabs.iter().map(|t| t.id()).collect();
        let active_idx_key = self.pane_id.with("mara_normal_active_tab");
        let active_idx = resolve_active_tab_idx(ui.ctx(), active_idx_key, &tab_ids);
        let active_pods = std::mem::take(&mut tabs[active_idx].pods);

        // Render the container with NO title text and NO floating
        // icon. The theme-selected title-row layout stamps tab
        // buttons on top of the title zone. `tabbed_strip_side`
        // stays `None` because this is not side-strip chrome.
        let pane_id = self.pane_id;
        let accent = self.accent;
        let theme_now = style::theme();
        let title_multiplier = theme_now.tabs.title_row_height_multiplier;
        let me = Self {
            title: String::new(),
            icon: None,
            tabbed_strip_side: None,
            title_thickness_override: Some(
                theme_now.container.title_zone_thickness * title_multiplier,
            ),
            suppress_banner: true,
            min_body_flow: Some(self.min_body_flow.unwrap_or(0.0).max(max_tab_body_h)),
            ..self
        };
        let out = me.show(ui, active_pods);

        // Read the title rect that `paint_title` stashed during
        // render. If absent (folded container or first frame), skip
        // the tab paint — there's nothing to overlay on.
        let title_rect: Option<egui::Rect> = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp(pane_id.with("mara_normal_title_rect"));
        let Some(title_rect) = title_rect else {
            return out;
        };
        // Extend the rect outward by `inner_margin` so cells reach
        // the Frame's painted edge (= where the banner used to draw)
        // and there's no uncoloured ring of pane bg around the strip.
        let theme = style::theme();
        let inner_x = theme.section_pad_x as f32;
        let inner_y = theme.section_pad_y as f32;
        let title_rect = top_tab_title_rect(title_rect.into(), inner_x, inner_y);

        paint_top_tabs(
            ui,
            title_rect.into(),
            &tab_meta,
            &tab_ids,
            active_idx,
            accent,
            pane_id,
            active_idx_key,
        );
        out
    }

    fn show_inner(
        self,
        ui: &mut Ui,
        pods: impl IntoIterator<Item = crate::pod::Pod>,
    ) -> Vec<crate::pod::PodResponse> {
        let container_theme = style::theme().container;
        let pod_pad_x = container_theme.pod_pad_x;
        let pod_pad_y = container_theme.pod_pad_y;
        let pods: Vec<crate::pod::Pod> = pods.into_iter().collect();
        let pods_total = pods.len();
        let pods_accent = self.accent;
        let mut out: Vec<crate::pod::PodResponse> = Vec::with_capacity(pods_total);
        // ── Fill-pod pre-pass ──
        //
        // Snapshot the FIRST pod marked `fill()` (if any) — its
        // height is computed once we know the container body's
        // available_height, and stashed in ctx data BEFORE the pod
        // iteration so `Pod::show` can read it. Per-pod chrome
        // (Frame inner_margin = pod_pad_y on each side) plus
        // separator strip thickness is included in the "other pods"
        // budget so the fill pod's slot really is the leftover.
        let fill_pod_idx = pods.iter().position(|p| p.is_fill());
        // Per-pod chrome (top + bottom Frame inner_margin) on each pod.
        let pod_chrome_each = (pod_pad_y as f32) * 2.0;
        let separator_total_h = if pods_total > 1 {
            (pods_total - 1) as f32 * crate::container::separator::separator_strip_h()
        } else {
            0.0
        };
        // Sum of every pod's natural height + per-pod chrome — this
        // is the container's NATURAL body height, used to override
        // the dynamic content measurement when a fill pod is present
        // (otherwise the fill pod's stretched height feeds back into
        // the auto-fit and the container grows monotonically).
        let pods_natural_total_h: f32 = pods
            .iter()
            .map(|p| p.natural_h() + pod_chrome_each)
            .sum::<f32>()
            + separator_total_h;
        let body_flow_floor = pods_natural_total_h.max(self.min_body_flow.unwrap_or(0.0));
        let fill_pod_id_and_others_h: Option<(Id, f32)> = fill_pod_idx.map(|fi| {
            let mut others_h = 0.0_f32;
            for (i, p) in pods.iter().enumerate() {
                if i == fi {
                    continue;
                }
                others_h += p.natural_h() + pod_chrome_each;
            }
            others_h += separator_total_h;
            (pods[fi].egui_id(), others_h)
        });
        // When a fill pod is present, stash the natural total for
        // `record_container_intrinsic` to pick up — it'll record THAT
        // instead of the dynamically measured content_h, breaking the
        // grow-loop. Container's pane id == its CID (passed to
        // `Normal::new` and threaded through here as `pane_id`).
        let intrinsic_override_key = self
            .pane_id
            .with("mara_container_intrinsic_natural_override");
        let intrinsic_floor_key = self.pane_id.with("mara_container_intrinsic_natural_floor");
        if fill_pod_idx.is_some() {
            {
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                memory.set_temp::<f32>(intrinsic_override_key, body_flow_floor);
                memory.remove_temp::<f32>(intrinsic_floor_key);
            }
        } else {
            {
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                memory.remove_temp::<f32>(intrinsic_override_key);
                memory.set_temp::<f32>(intrinsic_floor_key, body_flow_floor);
            }
        }
        self.show_with_body(ui, |body_ui| {
            // Compute the fill pod's height NOW that we're inside
            // the container body and know its available_height.
            // Stash it in ctx data so `Pod::show` picks it up when
            // the fill pod's iteration arrives.
            //
            // The math:
            //   body_avail = container_flow - body_top_pad  (top_pad
            //     is consumed via `add_space` BEFORE body_cfg.paint
            //     allocates its slot, so it's already excluded).
            //   total budget = Σ (pod_natural + pod_chrome) + Σ separators
            //
            // `others_h` = sum of non-fill pod naturals + their chromes
            //   + ALL separators. The fill pod's OWN Frame chrome
            //   (`pod_chrome_each` = `pod_pad_y * 2`) is NOT included
            //   — it wraps the fill pod's slot from outside. Subtract
            //   it explicitly here so the slot we allocate for the
            //   fill pod fits *inside* its chrome with the rest of
            //   the body fitting around it.
            //
            // Subtracting `top_pad` here would be a double subtraction
            // (it's already gone from body_avail) — that was the
            // earlier off-by-12 bug that pushed the bottom pod past
            // the body and clipped it.
            if let Some((fill_id, others_h)) = fill_pod_id_and_others_h {
                let body_avail = body_ui.available_height();
                let fill_h =
                    (body_avail - others_h - pod_chrome_each).max(style::theme().pod.min_widget_h);
                crate::memory::MaraMemoryCtx::new(body_ui.ctx())
                    .set_temp(crate::pod::Pod::forced_height_key(fill_id), fill_h);
            }
            for (i, pod) in pods.into_iter().enumerate() {
                // Capture metadata BEFORE the pod is consumed by `show`.
                let pod_id = pod.egui_id();
                let pod_is_resizable = pod.is_resizable();
                let _pod_widget_count = pod.widget_count();
                let separator_after = if i + 1 < pods_total {
                    pod.separator_style()
                } else if pod_is_resizable {
                    // Last pod that's resizable still paints its
                    // dotted handle so the user can drag the bottom
                    // edge to shrink/grow the pod (reveal / hide
                    // rows). Without this the resize affordance
                    // disappears whenever the resizable pod ends up
                    // at the bottom of its container — which is
                    // exactly when the user expects to find the
                    // handle there.
                    pod.separator_style()
                } else {
                    // Last non-resizable pod — no separator below;
                    // there's nothing to divide it from.
                    crate::container::SeparatorStyle::None
                };
                let frame_resp = Frame::new()
                    .inner_margin(egui::Margin::from(style::MarginSpec::symmetric(pod_pad_x, pod_pad_y)))
                    .show(body_ui, |inner_ui| {
                        out.push(pod.show(inner_ui));
                    });
                crate::debug::tag(
                    body_ui,
                    frame_resp.response.rect,
                    format!("Pod[{:?}]", pod_id),
                );
                if separator_after != crate::container::SeparatorStyle::None {
                    let sep_rect_before = body_ui.cursor();
                    let resizable_handle = pod_is_resizable
                        && separator_after == crate::container::SeparatorStyle::LineDots;
                    if resizable_handle {
                        // Interactive variant: drag delta updates
                        // the pod's persisted per-widget height,
                        // divided by widget_count so the cursor
                        // tracks the pod's bottom edge (each
                        // widget grows by delta/N).
                        let resp = crate::container::paint_separator_resize(
                            body_ui,
                            separator_after,
                            // Inter-pod separators are always
                            // horizontal — the body forces a
                            // top_down layout, so pods stack
                            // vertically inside the container
                            // regardless of the parent pane's
                            // orientation.
                            crate::container::SeparatorOrient::Horizontal,
                            pod_id,
                            pods_accent,
                        );
                        if resp.dragged() {
                            // Drag delta maps 1:1 to the pod's
                            // viewport height — no division by
                            // widget count, since `Pod::show` now
                            // treats this value as the slot's total
                            // pixel height (clipping content beyond)
                            // rather than scaling individual
                            // widgets.
                            let key: egui::Id = crate::pod::Pod::widget_height_key(pod_id).into();
                            let cur = crate::memory::MaraMemoryCtx::new(body_ui.ctx()).get_persisted::<f32>(key).unwrap_or(crate::style::UNIT);
                            let new = (cur + resp.drag_delta.y).clamp(
                                style::theme().pod.min_widget_h,
                                style::theme().pod.max_widget_h,
                            );
                            crate::memory::MaraMemoryCtx::new(body_ui.ctx()).set_persisted(key, new);
                        }
                    } else {
                        crate::container::paint_separator(
                            body_ui,
                            separator_after,
                            crate::container::SeparatorOrient::Horizontal,
                        );
                    }
                    let sep_rect_after = body_ui.cursor();
                    // Tag the separator strip for the F10 inspector
                    // so the user can see which boundary owns
                    // which style. Use the cursor delta since the
                    // separator paint functions don't return rects.
                    let strip_rect =
                        separator_debug_rect(sep_rect_before.into(), sep_rect_after.into());
                    crate::debug::tag(
                        body_ui,
                        strip_rect.into(),
                        format!("separator[{:?}]", separator_after),
                    );
                }
            }
        });
        out
    }

    /// Paint the container chrome (title strip, accent banner,
    /// frame, fold animation) with a caller-supplied body closure
    /// instead of the canonical pod list. Use only for
    /// adapters that need to host non-`'static` content
    /// (e.g. closures borrowing Bevy `Res`/`ResMut` parameters);
    /// regular call sites should still go through
    /// [`Normal::show`] with [`crate::pod::Pod`] entries so the
    /// pod separator / fill / resize plumbing stays wired.
    pub(crate) fn show_raw(self, ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
        let pane_id = self.pane_id;
        ui.push_id(pane_id, |ui| self.show_with_body(ui, body));
    }

    fn show_with_body(self, ui: &mut Ui, body: impl FnOnce(&mut Ui)) {
        // Register this container's MIN WIDTH with the parent pane
        // so the pane's resize handles can refuse to shrink the
        // pane below the union of its containers' bounds. Keyed
        // on the active pane id (`pane::active_pane_key`) which
        // internal pane rendering writes at the top of every frame, then
        // clears the accumulator before running the body callback.
        // First-frame fallback: if no active pane is set yet,
        // register against the container's own pane_id so the
        // entry isn't lost.
        let parent_pane_id: Id = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp(pane::active_pane_key())
            .unwrap_or(self.pane_id);
        let min_w = self
            .min_width
            .unwrap_or_else(|| style::theme().container.default_min_width);
        {
            let key = parent_pane_id.with("mara_pane_container_min_widths");
            let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
            let mut acc: Vec<f32> = memory.get_temp(key).unwrap_or_default();
            acc.push(min_w);
            memory.set_temp(key, acc);
        }

        // Per-container default-flow override (set via
        // `Normal::initial_flow`). Persist on every frame the
        // builder supplies a value so `crate::container::container_flow`
        // (called from both this Normal AND the parent Pane's
        // auto-flow sum) sees the same target.
        if let Some(initial) = self.initial_flow {
            crate::container::set_container_initial_flow(ui.ctx(), self.pane_id, initial);
        }

        let title_side = self.anchor.title_side();
        let horizontal_strip = title_side.is_horizontal_strip();

        let theme_now = style::theme();

        // Register this container's MINIMUM flow-axis chrome with the
        // parent pane so horizontal-strip pane resize handles can
        // refuse to shrink past where containers would start
        // overlapping. egui's `available_rect_before_wrap` collapses
        // to zero-height once the layout cursor overshoots
        // `max_rect`; subsequent Frame allocations still draw
        // their content + margins, which extends below the cursor by
        // the bottom-side chrome (inner_margin + stroke +
        // outer_margin) and visually overlaps the previous container.
        // Floor = sum of (TITLE_ZONE_THICKNESS + title-body gap +
        //                 inner_margin both sides + stroke ×2 +
        //                 outer_margin both sides) for every container,
        // computed at the current `openness` so the floor naturally
        // shrinks to title-only when all containers are folded.
        let openness_for_min = pane::body_openness(ui.ctx(), self.pane_id);
        let pad_for_min = style::section_padding();
        let pad_flow_for_min = if horizontal_strip {
            (pad_for_min.top as f32) + (pad_for_min.bottom as f32)
        } else {
            (pad_for_min.left as f32) + (pad_for_min.right as f32)
        };
        let outer_flow_for_min = (theme_now.section_outer_margin_flow_title as f32)
            + (theme_now.section_outer_margin_flow_body as f32);
        let stroke_for_min = if style::section_show_frame() {
            theme_now.border_width * 2.0
        } else {
            0.0
        };
        let container_theme = theme_now.container;
        let min_flow = container_theme.title_zone_thickness
            + container_theme.title_body_gap_half * 2.0 * openness_for_min
            + pad_flow_for_min
            + outer_flow_for_min
            + stroke_for_min;
        {
            let key = parent_pane_id.with("mara_pane_container_min_flows");
            let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
            let mut acc: Vec<f32> = memory.get_temp(key).unwrap_or_default();
            acc.push(min_flow);
            memory.set_temp(key, acc);
        }
        let pad = style::section_padding();
        let pad_w = (pad.left as f32) + (pad.right as f32);
        let pad_h = (pad.top as f32) + (pad.bottom as f32);
        // Frame chrome that sits OUTSIDE the span_inner slot:
        //   `pad_*` — Frame's `inner_margin` (theme `section_padding`).
        //   `outer_*` — Frame's `outer_margin`, per-axis from theme.
        //   `stroke_*` — border drawn on either side (PRO=1, GAME=0).
        // Subtract them so the Frame's resulting outer rect fits
        // inside `outer_avail` exactly — no 2-px overflow into the
        // pane's stroke or shadow when the theme has a visible border.
        // Total outer-margin on each axis. Cross-axis is symmetric
        // (`2 × cross`); flow-axis sums the per-side title-facing
        // and body-facing margins.
        let flow_outer_total = (theme_now.section_outer_margin_flow_title as f32)
            + (theme_now.section_outer_margin_flow_body as f32);
        let span_outer_total = (theme_now.section_outer_margin_span as f32) * 2.0;
        // X axis = cross when horizontal-strip, main when vertical-strip.
        let outer_w = if horizontal_strip {
            span_outer_total
        } else {
            flow_outer_total
        };
        // Y axis = main when horizontal-strip, cross when vertical-strip.
        let outer_h = if horizontal_strip {
            flow_outer_total
        } else {
            span_outer_total
        };
        let stroke_w = if style::section_show_frame() {
            theme_now.border_width * 2.0
        } else {
            0.0
        };

        // Cross axis = the dim the title strip spans. Track the
        // PARENT's available cross extent so the container grows
        // along with the (user-resized) pane instead of staying
        // capped at `CONTAINER_DEFAULT_*`. Subtract the Frame
        // chrome on each side so the inner content slot fits
        // inside the painted Frame.
        let outer_avail = ui.available_size();
        let span_inner = if horizontal_strip {
            (outer_avail.x - pad_w - outer_w - stroke_w).max(0.0)
        } else {
            (outer_avail.y - pad_h - outer_h - stroke_w).max(0.0)
        };

        // Resolve the title row's perpendicular extent: per-instance
        // override (set by GAME-tabbed render path) wins over the
        // global default. Used both for the title's allocated slot
        // and for the GAME banner's bottom edge below.
        let title_thickness = self
            .title_thickness_override
            .unwrap_or(container_theme.title_zone_thickness);
        let title_size = title_slot_size(horizontal_strip, span_inner, title_thickness);

        // Shared body recipe — applies the span-axis clamp so child
        // widgets see a stable `ui.available_*` regardless of the
        // surrounding layout's measurement passes.
        let body_cfg = Body::new(horizontal_strip, span_inner);

        let title_text = self.title.clone();
        let anchor = self.anchor;
        let accent = self.accent;
        let icon = self.icon;

        // GAME themes paint an accent banner across the title row.
        // Suppress it when `suppress_banner` is set (GAME tabbed path)
        // so each tab cell owns its own background and a transparent
        // inactive cell reveals pane bg, not the banner accent.
        let banner_filled = style::theme().title_strip_filled && !self.suppress_banner;

        // Open state + animation are stored on the parent pane's
        // id (NOT `ui.id()`) so pane rendering and `Normal::show`
        // both compute the SAME `openness` from the same
        // `animate_bool` call within a frame. That synchronises the
        // pane's outer size and the container's body slot — no
        // anchor lag, no per-frame edge drift.
        let pane_id = self.pane_id;
        // Defaults to open, and the default is persisted so a later
        // read sees the same answer.
        let open: bool = {
            let key = pane_id.with("body_open");
            let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
            match memory.get_persisted::<bool>(key) {
                Some(open) => open,
                None => {
                    memory.set_persisted(key, true);
                    true
                }
            }
        };
        let openness = pane::body_openness(ui.ctx(), pane_id);
        // Body's full flow-axis size when fully open. Used as the
        // child UI's `max_rect` extent so widgets ALWAYS render at
        // their natural size; only the clip mask animates.
        //
        // Resolution order (first non-`None` wins):
        //   1. `Normal::body_flow` builder override — explicit caller
        //      control, e.g. for tests or fixed-height containers.
        //   2. `crate::container::container_flow(self.pane_id)` —
        //      the per-container persisted flow size, written by
        //      the inter-container drag-resize handle.
        //   3. The `CONTAINER_DEFAULT_*` fallback computed from
        //      title strip + chrome.
        let full_body_flow = self.body_flow.unwrap_or_else(|| {
            // Persisted per-container flow takes precedence over
            // the static fallback. Returns
            // `CONTAINER_DEFAULT_FLOW` clamped on first read.
            crate::container::container_flow(ui.ctx(), pane_id, horizontal_strip)
        });
        // Publish this container's cid to the parent pane so
        // pane rendering can sum each container's LIVE persisted
        // flow when it auto-sizes (`PaneResize::flow` off).
        pane::publish_container_cid(ui.ctx(), parent_pane_id, pane_id);
        // Body slot size LERPS with `openness` to match Pane's
        // lerp (both compute openness from the SAME `animate_bool`
        // call, so they animate in lockstep — no anchor drift).
        let body_visible = openness > 0.0;
        let total_gap = container_theme.title_body_gap_half * 2.0 * openness;
        let visible_body_flow = openness * full_body_flow;

        // ── Per-section staggered fade-in. ──
        //
        // Look up the parent Pane's id via the global "active
        // pane" pointer (Normal's own `pane_id` field is the
        // container's body-open id, NOT Pane's id, so we can't
        // use it for the stagger lookup). Pane rendering populates
        // `mara_pane_open_elapsed` and resets
        // `mara_pane_section_idx` to 0 on every frame; we
        // post-increment to claim THIS container's index.
        const STAGGER_BASE: f32 = 0.18;
        const FADE_BASE: f32 = 0.45;
        let stagger_opacity: f32 = {
            let theme_now = style::theme();
            let scale = theme_now.pane_fade_scale.max(0.01);
            let stagger = STAGGER_BASE * scale;
            let fade = FADE_BASE * scale;
            {
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                let pane2_id: Id = memory
                    .get_temp::<Id>(pane::active_pane_key())
                    .unwrap_or(pane_id);
                let elapsed: f32 = memory
                    .get_temp(pane2_id.with("mara_pane_open_elapsed"))
                    .unwrap_or(99.0);
                // The index advances per container, which is what
                // staggers them; it must be read and bumped together.
                let idx_key = pane2_id.with("mara_pane_section_idx");
                let idx: u32 = memory.get_temp(idx_key).unwrap_or(0);
                memory.set_temp(idx_key, idx + 1);
                let start = (idx as f32) * stagger;
                let raw = ((elapsed - start) / fade).clamp(0.0, 1.0);
                raw * raw * (3.0 - 2.0 * raw) // smoothstep
            }
        };
        let prev_opacity = ui.opacity();
        if stagger_opacity < 1.0 {
            ui.multiply_opacity(stagger_opacity);
        }

        // Drag-lift: if this container IS the one being dragged,
        // bail out entirely — no layout slot, no paint. The other
        // containers below collapse upward to fill the gap, and
        // the floating preview painted by `Pane`'s finalize
        // shows what's being held.
        let active = pane::active_drag(ui.ctx());
        let is_dragging_self = active
            .and_then(|(_, s)| s.item)
            .map(|id| id == pane_id)
            .unwrap_or(false);
        if is_dragging_self {
            ui.set_opacity(prev_opacity);
            return;
        }

        // Inline ghost gap: if the cursor's target slot equals
        // THIS container's position in the non-dragged sequence,
        // allocate + paint a ghost rect of the dragged size
        // BEFORE rendering. Pushes this container (and the rest)
        // along the stack axis so the drop slot is visible.
        if let Some((parent_pane_id, drag_state)) = active
            && let (Some(dragged_id), Some(cursor)) = (drag_state.item, drag_state.cursor)
            && !pane::ghost_gap_suppressed(ui.ctx(), parent_pane_id)
        {
            let snap = pane::snapshot(ui.ctx(), parent_pane_id);
            let horizontal_stack = !title_side.is_horizontal_strip();
            let cursor_axis = if horizontal_stack { cursor.x } else { cursor.y };
            let target_idx = pane::compute_target(&snap, dragged_id, cursor_axis, horizontal_stack);
            let cur_idx = pane::current_cache(ui.ctx(), parent_pane_id).len();
            if cur_idx == target_idx
                && let Some(entry) = pane::dragged_entry(&snap, dragged_id)
            {
                pane::paint_ghost_gap_entry_inline(ui, entry, accent, horizontal_stack);
            }
        }

        let frame = self.theme_frame();
        let frame_response = frame.show(ui, |ui| {
            crate::backend::egui::show_with_deferred_paint_cmd_slots(
                ui,
                usize::from(banner_filled),
                |ui| {
                    // ── Manual layout (no flex) ──
                    // egui's `CollapsingState` recipe: title is allocated at
                    // its exact size, the body is rendered at FULL size into
                    // a clipped child UI, and only the VISIBLE portion is
                    // allocated to the parent ui (`force_set_min_rect` /
                    // `allocate_rect`). So:
                    //   • body's content widgets keep their natural
                    //     `available_*` width — no per-frame text_input
                    //     shrinking,
                    //   • the parent's min_rect lerps smoothly with
                    //     `openness`, which animates the container chrome
                    //     and the parent pane's `fixed_pos` together,
                    //   • no flex item state changes, no `request_discard`
                    //     storm, no PERF WARNING overlay.
                    // Inherit the parent's layout direction directly into
                    // the Frame's content_ui — DON'T create a child with a
                    // forced `top_down`. Frame computes its outer rect from
                    // `content_ui.min_rect()`, so the inner allocations
                    // determine where the Frame lands inside the pane body.
                    // Forcing `top_down` made the container always appear
                    // at the TOP of available area (since cursor starts at
                    // max_rect.min for top_down), which in a `bottom_up`
                    // pane parent left every container at the FAR edge from
                    // the rail instead of stacking against the title strip.
                    // Inheriting the parent layout makes:
                    //   • TopDown    → first allocation at top  (TopRail).
                    //   • BottomUp   → first allocation at bottom (BottomRail).
                    //   • LeftToRight→ first allocation at left  (LeftRail).
                    //   • RightToLeft→ first allocation at right (RightRail).
                    // Always render TITLE first then BODY: layout direction
                    // does the visual placement work, no `if title_at_end`
                    // swap needed at this level.
                    crate::backend::egui::apply_item_spacing_spec(
                        ui,
                        crate::layout::ItemSpacingSpec::zero(),
                    );

                    let render_title = |ui: &mut Ui| {
                        // Title strip is also the drag handle: `click_and_drag`
                        // sense reports both — `clicked()` toggles the body
                        // open state, `drag_started()` lifts this container
                        // for reorder via the parent pane's drag machine.
                        let resp = {
                            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                            crate::layout::UiBackend::allocate(
                                &mut backend,
                                title_size,
                                crate::layout::Sense::ClickAndDrag,
                            )
                        };
                        let rect: Rect = resp.rect.into();
                        if resp.hovered() {
                            crate::backend::egui::set_cursor_icon_for_ui(
                                ui,
                                crate::layout::CursorIcon::PointingHand,
                            );
                        }
                        if resp.clicked() {
                            pane::toggle_body(ui.ctx(), pane_id);
                        }
                        if resp.drag_started()
                            && let Some(active_pane_id) =
                                crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp::<Id>(pane::active_pane_key())
                        {
                            pane::set_drag(
                                ui.ctx(),
                                active_pane_id,
                                pane::DragState {
                                    item: Some(pane_id),
                                    cursor: crate::backend::egui::pointer_interact_pos(ui.ctx())
                                        .map(Into::into),
                                },
                            );
                        }
                        paint_title(
                            ui,
                            rect,
                            &title_text,
                            anchor,
                            accent,
                            open,
                            openness,
                            icon,
                            pane_id,
                        );
                    };

                    let render_body = |ui: &mut Ui, body: Box<dyn FnOnce(&mut Ui)>| {
                        if !body_visible || full_body_flow <= 0.0 {
                            return;
                        }
                        let body_slots = body_slot_sizes(
                            horizontal_strip,
                            span_inner,
                            visible_body_flow,
                            full_body_flow,
                        );
                        // `allocate_space` respects the parent's layout
                        // direction, so `visible_rect` lands at the correct
                        // edge (bottom for BottomUp, right for RightToLeft).
                        let visible_rect = {
                            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                            crate::layout::UiBackend::reserve_space(
                                &mut backend,
                                body_slots.visible,
                            )
                        };
                        let visible_rect: MaraRect = visible_rect;
                        // `full_rect` extends the visible slot to the full
                        // body size in the layout direction (so body
                        // widgets render at natural size and only the clip
                        // mask animates). For reversed layouts we anchor
                        // `full_rect`'s FAR edge to `visible_rect`'s far
                        // edge — the body grows AWAY from the title strip
                        // direction.
                        let body_direction = crate::backend::egui::stack_direction_for_ui(ui);
                        let full_rect =
                            body_full_rect(visible_rect, body_slots.full, body_direction);
                        // Body's child layout matches parent direction so
                        // body widgets anchor against the title strip side
                        // (BottomRail body → widgets stack from bottom up).
                        let body_region = crate::layout::ChildRegion::new(
                            full_rect,
                            body_direction,
                            crate::layout::StackAlign::Min,
                        );
                        let mut child = crate::backend::egui::child_ui_for_region(ui, body_region);
                        let parent_clip = ui.clip_rect();
                        child.set_clip_rect(parent_clip.intersect(visible_rect.into()));
                        // Inner top-pad on the title-facing edge of the
                        // body (theme-driven). Allocated FIRST in the body
                        // layout so the cursor advances past it before the
                        // user's body callback runs — pushes the first
                        // widget away from the title strip without changing
                        // the title's own thickness or the inter-container
                        // gap. PRO = 0 (no-op); GAME ≈ 8.
                        let body_top_pad = style::theme().section_body_inner_top_pad;
                        if body_top_pad > 0.0 {
                            crate::backend::egui::add_space_for_spec(
                                &mut child,
                                crate::layout::SpaceSpec::vertical(body_top_pad),
                            );
                        }
                        let (_, content_h) = body_cfg.paint(&mut child, body);
                        // Record the body's intrinsic content height so
                        // next frame's `container_flow` auto-fit path can
                        // size the container. Two cases:
                        //
                        // * Fill pod present → `intrinsic_override_key` was
                        //   stashed earlier with the natural sum of all pods
                        //   + chrome + separators. Use THAT instead of the
                        //   measured `content_h` — otherwise the fill pod's
                        //   stretched height feeds back into the intrinsic
                        //   and the container grows monotonically each frame.
                        // * No fill pod → `content_h` IS the natural sum
                        //   (each pod allocates its own natural height), so
                        //   use the measurement directly. Lets expandable
                        //   widgets (color picker, etc.) still grow the
                        //   container.
                        let recorded_h = {
                            let memory = crate::memory::MaraMemoryCtx::new(child.ctx());
                            match memory.get_temp::<f32>(
                                pane_id.with("mara_container_intrinsic_natural_override"),
                            ) {
                                Some(exact) => exact,
                                None => {
                                    let floor = memory
                                        .get_temp::<f32>(
                                            pane_id.with("mara_container_intrinsic_natural_floor"),
                                        )
                                        .unwrap_or(0.0);
                                    content_h.max(floor)
                                }
                            }
                        };
                        crate::container::record_container_intrinsic(
                            child.ctx(),
                            pane_id,
                            recorded_h + body_top_pad,
                        );
                    };

                    // ALWAYS title FIRST, body SECOND. Layout direction
                    // (inherited from pane parent) handles which edge the
                    // title lands at.
                    render_title(ui);
                    if total_gap > 0.0 {
                        crate::backend::egui::add_space_for_spec(
                            ui,
                            crate::layout::SpaceSpec::vertical(total_gap),
                        );
                    }
                    let body_box: Box<dyn FnOnce(&mut Ui)> = Box::new(body);
                    render_body(ui, body_box);

                    // After flex is laid out, paint the GAME banner into
                    // the deferred shape index. Banner extends from the
                    // frame's painted edge (= ui.min_rect() expanded by
                    // section_padding) through the title strip and into
                    // half the flex gap. Equivalent to `foldable.rs`'s
                    // banner trick — the painted accent zone covers the
                    // title slot AND the inner_margin around it.
                    let banner_cmd = if banner_filled {
                        let pad = style::section_padding();
                        let banner = title_banner_rect(
                            ui.min_rect().into(),
                            pad,
                            title_side,
                            title_thickness,
                            container_theme.title_body_gap_half,
                            open,
                        );
                        Some(PaintCmd::RectFilled {
                            rect: banner,
                            corner: MaraCornerRadius::ZERO,
                            fill: accent.into(),
                        })
                    } else {
                        None
                    };

                    // Corner ticks (GAME): L-shaped marks at each corner of
                    // the container's outer rect, with a slow breathing
                    // pulse. PRO has `section_corner_ticks = 0` so this is
                    // a no-op there.
                    let used_outer =
                        rect_expanded_by_margin(ui.min_rect().into(), style::section_padding());
                    paint_corner_ticks(
                        ui,
                        used_outer.into(),
                        accent,
                        title_side,
                        openness,
                        pane_id,
                    );
                    ((), banner_cmd)
                },
            );
        });
        // Restore the parent ui's opacity so subsequent containers
        // in the same body callback start from a clean baseline.
        ui.set_opacity(prev_opacity);
        crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(
            pane::active_container_frame_rect_key(),
            frame_response.response.rect,
        );

        // Publish the rendered Frame's outer rect to the parent
        // pane's per-frame cache. `Pane`'s finalize builds next
        // frame's snapshot from this (with the dragged
        // container's prev rect carried forward).
        if let Some((active_pane_id, _)) = active {
            // Take, not read: a tabbed container publishes the union
            // of strip+body here, and leaving it behind would let the
            // next container inherit this one's rect.
            let published_rect = {
                let key = pane::active_tabbed_container_rect_key();
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                let rect = memory.get_temp::<egui::Rect>(key);
                memory.remove_temp::<egui::Rect>(key);
                rect
            }
            .unwrap_or(frame_response.response.rect);
            pane::push_rect(ui.ctx(), active_pane_id, pane_id, published_rect);
        }
        // Custom debug inspector — outline the container's full
        // painted Frame rect with a `Normal[<title>]` label.
        crate::debug::tag(
            ui,
            frame_response.response.rect,
            format!("Normal[{}]", title_text),
        );
    }

    /// Outer frame recipe: glass-card fill, accent-tinted border, theme
    /// `radius_md` corners. When the active theme has
    /// `section_show_frame = false` (GAME) we drop the visuals and
    /// keep just the inner padding so body content sits flush.
    /// `outer_margin` is per-side from the theme:
    ///   • flow-axis title-FACING side — sets the gap between the
    ///     pane title strip and the FIRST container.
    ///   • flow-axis body-FACING side — combines with the next
    ///     container's title-side margin to produce the
    ///     inter-container gap.
    ///   • span-axis sides — breathing space against the pane's
    ///     left/right (or top/bottom for vertical-strip) chrome.
    fn theme_frame(&self) -> Frame {
        let theme = style::theme();
        let title_side = self.anchor.title_side();
        let main_title = theme.section_outer_margin_flow_title;
        let main_body = theme.section_outer_margin_flow_body;
        let cross = theme.section_outer_margin_span;
        // Each title side puts the title-facing margin on a
        // different edge of the container's outer rect; the
        // body-facing margin lives on the OPPOSITE edge. Cross-axis
        // (the two sides parallel to the title strip) always uses
        // `cross`.
        let mut outer = match title_side {
            TitleSide::Top => style::MarginSpec {
                left: cross,
                right: cross,
                top: main_title,
                bottom: main_body,
            },
            TitleSide::Bottom => style::MarginSpec {
                left: cross,
                right: cross,
                top: main_body,
                bottom: main_title,
            },
            TitleSide::Left => style::MarginSpec {
                top: cross,
                bottom: cross,
                left: main_title,
                right: main_body,
            },
            TitleSide::Right => style::MarginSpec {
                top: cross,
                bottom: cross,
                left: main_body,
                right: main_title,
            },
        };
        let corners = style::radius_for(style::RadiusRole::Section);
        if let Some(side) = self.tabbed_strip_side {
            // Zero the strip-side outer margin so the active tab
            // (allocated to that side in the outer wrapper) can
            // extend across the Frame's painted edge with no gap of
            // pane bg between them. Keep the container's own corners
            // rounded: tabs are attached chrome, not a reason to
            // flatten the card/shelf container shape.
            match side {
                TitleSide::Top => {
                    outer.top = 0;
                }
                TitleSide::Bottom => {
                    outer.bottom = 0;
                }
                TitleSide::Left => {
                    outer.left = 0;
                }
                TitleSide::Right => {
                    outer.right = 0;
                }
            }
        }
        if style::section_show_frame() {
            Frame::new()
                .fill(style::fill_for(style::FillRole::Section, self.accent).into())
                .corner_radius(corners)
                .stroke(style::stroke_for(
                    style::StrokeRole::SectionBorder,
                    self.accent,
                ))
                .inner_margin(style::section_padding())
                .outer_margin(outer)
        } else {
            Frame::new()
                .inner_margin(style::section_padding())
                .outer_margin(outer)
        }
    }
}

fn tabbed_container_max_rect(
    avail: MaraRect,
    strip_side: TitleSide,
    strip_thickness: f32,
    strip_outer_inset: f32,
) -> MaraRect {
    let reserved = strip_outer_inset + strip_thickness;
    match strip_side {
        TitleSide::Left => MaraRect::from_min_max(
            MaraPos2::new((avail.left() + reserved).min(avail.right()), avail.top()),
            avail.max,
        ),
        TitleSide::Right => MaraRect::from_min_max(
            avail.min,
            MaraPos2::new((avail.right() - reserved).max(avail.left()), avail.bottom()),
        ),
        TitleSide::Top => MaraRect::from_min_max(
            MaraPos2::new(avail.left(), (avail.top() + reserved).min(avail.bottom())),
            avail.max,
        ),
        TitleSide::Bottom => MaraRect::from_min_max(
            avail.min,
            MaraPos2::new(avail.right(), (avail.bottom() - reserved).max(avail.top())),
        ),
    }
}

fn folder_tab_strip_rect(
    used: MaraRect,
    strip_side: TitleSide,
    title_side: TitleSide,
    strip_thickness: f32,
    title_offset: f32,
) -> MaraRect {
    match (strip_side, title_side) {
        (TitleSide::Left, TitleSide::Top) => MaraRect::from_min_max(
            MaraPos2::new(used.left() - strip_thickness, used.top() + title_offset),
            MaraPos2::new(used.left(), used.bottom()),
        ),
        (TitleSide::Left, TitleSide::Bottom) => MaraRect::from_min_max(
            MaraPos2::new(used.left() - strip_thickness, used.top()),
            MaraPos2::new(used.left(), used.bottom() - title_offset),
        ),
        (TitleSide::Right, TitleSide::Top) => MaraRect::from_min_max(
            MaraPos2::new(used.right(), used.top() + title_offset),
            MaraPos2::new(used.right() + strip_thickness, used.bottom()),
        ),
        (TitleSide::Right, TitleSide::Bottom) => MaraRect::from_min_max(
            MaraPos2::new(used.right(), used.top()),
            MaraPos2::new(used.right() + strip_thickness, used.bottom() - title_offset),
        ),
        (TitleSide::Top, TitleSide::Left) => MaraRect::from_min_max(
            MaraPos2::new(used.left() + title_offset, used.top() - strip_thickness),
            MaraPos2::new(used.right(), used.top()),
        ),
        (TitleSide::Top, TitleSide::Right) => MaraRect::from_min_max(
            MaraPos2::new(used.left(), used.top() - strip_thickness),
            MaraPos2::new(used.right() - title_offset, used.top()),
        ),
        _ => MaraRect::from_min_size(used.min, MaraVec2::ZERO),
    }
}

fn top_tab_title_rect(title_rect: MaraRect, inner_x: f32, inner_y: f32) -> MaraRect {
    MaraRect::from_min_max(
        MaraPos2::new(title_rect.left() - inner_x, title_rect.top() - inner_y),
        MaraPos2::new(title_rect.right() + inner_x, title_rect.bottom()),
    )
}

fn separator_debug_rect(before: MaraRect, after: MaraRect) -> MaraRect {
    MaraRect::from_min_max(before.min, MaraPos2::new(before.right(), after.top()))
}

fn rect_expanded_by_margin(rect: MaraRect, margin: style::MarginSpec) -> MaraRect {
    MaraRect::from_min_max(
        MaraPos2::new(
            rect.left() - margin.left as f32,
            rect.top() - margin.top as f32,
        ),
        MaraPos2::new(
            rect.right() + margin.right as f32,
            rect.bottom() + margin.bottom as f32,
        ),
    )
}

fn title_banner_rect(
    used: MaraRect,
    padding: style::MarginSpec,
    title_side: TitleSide,
    title_thickness: f32,
    title_body_gap_half: f32,
    open: bool,
) -> MaraRect {
    let painted = rect_expanded_by_margin(used, padding);
    if !open {
        return painted;
    }

    let title_span = title_thickness + title_body_gap_half;
    match title_side {
        TitleSide::Top => MaraRect::from_min_max(
            painted.min,
            MaraPos2::new(painted.right(), used.top() + title_span),
        ),
        TitleSide::Bottom => MaraRect::from_min_max(
            MaraPos2::new(painted.left(), used.bottom() - title_span),
            painted.max,
        ),
        TitleSide::Left => MaraRect::from_min_max(
            painted.min,
            MaraPos2::new(used.left() + title_span, painted.bottom()),
        ),
        TitleSide::Right => MaraRect::from_min_max(
            MaraPos2::new(used.right() - title_span, painted.top()),
            painted.max,
        ),
    }
}

fn floating_icon_geometry(
    strip_rect: MaraRect,
    anchor: PaneAnchor,
    size: f32,
    offset: f32,
    openness_t: f32,
) -> FloatingIconGeometry {
    let title_side = anchor.title_side();
    let reversed = anchor.title_reversed();
    let icon_size = MaraVec2::new(size, size);

    match title_side {
        TitleSide::Top | TitleSide::Bottom => {
            let cy = if title_side == TitleSide::Top {
                (strip_rect.center().y - offset).round()
            } else {
                const BIAS: f32 = 6.0;
                (strip_rect.center().y + offset + BIAS * openness_t).round()
            };
            let on_far_end_left = reversed;
            if on_far_end_left {
                let pos = MaraPos2::new((strip_rect.left() + 6.0).round(), cy);
                let rect = if title_side == TitleSide::Top {
                    MaraRect::from_min_size(pos, icon_size)
                } else {
                    MaraRect::from_min_size(MaraPos2::new(pos.x, pos.y - size), icon_size)
                };
                let align = if title_side == TitleSide::Top {
                    MaraAlign2::LEFT_TOP
                } else {
                    MaraAlign2::LEFT_BOTTOM
                };
                FloatingIconGeometry { pos, align, rect }
            } else {
                let pos = MaraPos2::new((strip_rect.right() - 6.0).round(), cy);
                let rect = if title_side == TitleSide::Top {
                    MaraRect::from_min_size(MaraPos2::new(pos.x - size, pos.y), icon_size)
                } else {
                    MaraRect::from_min_size(MaraPos2::new(pos.x - size, pos.y - size), icon_size)
                };
                let align = if title_side == TitleSide::Top {
                    MaraAlign2::RIGHT_TOP
                } else {
                    MaraAlign2::RIGHT_BOTTOM
                };
                FloatingIconGeometry { pos, align, rect }
            }
        }
        TitleSide::Left | TitleSide::Right => {
            let cx = if title_side == TitleSide::Left {
                (strip_rect.center().x - offset).round()
            } else {
                (strip_rect.center().x + offset).round()
            };
            let on_right_side = title_side == TitleSide::Right;
            let top_to_bottom = on_right_side ^ reversed;
            if top_to_bottom {
                let pos = MaraPos2::new(cx, (strip_rect.bottom() - 6.0).round());
                let rect = if title_side == TitleSide::Left {
                    MaraRect::from_min_size(MaraPos2::new(pos.x, pos.y - size), icon_size)
                } else {
                    MaraRect::from_min_size(MaraPos2::new(pos.x - size, pos.y - size), icon_size)
                };
                let align = if title_side == TitleSide::Left {
                    MaraAlign2::LEFT_BOTTOM
                } else {
                    MaraAlign2::RIGHT_BOTTOM
                };
                FloatingIconGeometry { pos, align, rect }
            } else {
                let pos = MaraPos2::new(cx, (strip_rect.top() + 6.0).round());
                let rect = if title_side == TitleSide::Left {
                    MaraRect::from_min_size(pos, icon_size)
                } else {
                    MaraRect::from_min_size(MaraPos2::new(pos.x - size, pos.y), icon_size)
                };
                let align = if title_side == TitleSide::Left {
                    MaraAlign2::LEFT_TOP
                } else {
                    MaraAlign2::RIGHT_TOP
                };
                FloatingIconGeometry { pos, align, rect }
            }
        }
    }
}

fn tabbed_strip_outer_inset(tab_theme: style::TabTheme, theme: &style::Theme) -> f32 {
    match tab_theme.outer_inset {
        style::TabOuterInset::None => 0.0,
        style::TabOuterInset::MirrorBodyInset => (theme.section_outer_margin_span as f32) * 0.5,
    }
}

fn active_tab_id_key(active_idx_key: Id) -> Id {
    active_idx_key.with("tab_id")
}

fn resolve_active_tab_idx(ctx: &egui::Context, active_idx_key: Id, tab_ids: &[Id]) -> usize {
    debug_assert!(!tab_ids.is_empty());
    let mut memory = crate::memory::MaraMemoryCtx::new(ctx);
    if let Some(active_id) = memory.get_persisted::<Id>(active_tab_id_key(active_idx_key))
        && let Some(idx) = tab_ids.iter().position(|id| *id == active_id)
    {
        memory.set_persisted(active_idx_key, idx);
        return idx;
    }

    let stored = memory.get_persisted::<usize>(active_idx_key).unwrap_or(0);
    let clamped = stored.min(tab_ids.len() - 1);
    if clamped != stored {
        memory.set_persisted(active_idx_key, clamped);
    }
    memory.set_persisted(active_tab_id_key(active_idx_key), tab_ids[clamped]);
    clamped
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FolderTabCellGeometry {
    base: MaraRect,
    active: MaraRect,
    corners: MaraCornerRadius,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TopTabCellGeometry {
    rect: MaraRect,
    icon_center: MaraPos2,
    label_center_base: MaraPos2,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct FloatingIconGeometry {
    pos: MaraPos2,
    align: MaraAlign2,
    rect: MaraRect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BodySlotSizes {
    visible: MaraVec2,
    full: MaraVec2,
}

fn title_slot_size(horizontal_strip: bool, span_inner: f32, title_thickness: f32) -> MaraVec2 {
    if horizontal_strip {
        MaraVec2::new(span_inner, title_thickness)
    } else {
        MaraVec2::new(title_thickness, span_inner)
    }
}

fn body_slot_sizes(
    horizontal_strip: bool,
    span_inner: f32,
    visible_body_flow: f32,
    full_body_flow: f32,
) -> BodySlotSizes {
    if horizontal_strip {
        BodySlotSizes {
            visible: MaraVec2::new(span_inner, visible_body_flow),
            full: MaraVec2::new(span_inner, full_body_flow),
        }
    } else {
        BodySlotSizes {
            visible: MaraVec2::new(visible_body_flow, span_inner),
            full: MaraVec2::new(full_body_flow, span_inner),
        }
    }
}

fn body_full_rect(
    visible_rect: MaraRect,
    full_size: MaraVec2,
    body_direction: crate::layout::StackDirection,
) -> MaraRect {
    match body_direction {
        crate::layout::StackDirection::BottomUp => MaraRect::from_min_size(
            MaraPos2::new(visible_rect.left(), visible_rect.bottom() - full_size.y),
            full_size,
        ),
        crate::layout::StackDirection::RightToLeft => MaraRect::from_min_size(
            MaraPos2::new(visible_rect.right() - full_size.x, visible_rect.top()),
            full_size,
        ),
        _ => MaraRect::from_min_size(visible_rect.min, full_size),
    }
}

fn folder_tab_cell_geometry(
    strip_rect: MaraRect,
    strip_side: TitleSide,
    cell_idx: usize,
    tab_len: f32,
    tab_gap: f32,
    tab_overlap: f32,
    tab_radius: u8,
) -> Option<FolderTabCellGeometry> {
    let advance = (cell_idx as f32) * (tab_len + tab_gap);
    match strip_side {
        TitleSide::Left => {
            let cell_top = strip_rect.top() + advance;
            if cell_top + tab_len > strip_rect.bottom() + 0.5 {
                return None;
            }
            let base = MaraRect::from_min_size(
                MaraPos2::new(strip_rect.left(), cell_top),
                MaraVec2::new(strip_rect.width(), tab_len),
            );
            Some(FolderTabCellGeometry {
                base,
                active: MaraRect::from_min_max(
                    base.min,
                    MaraPos2::new(base.max.x + tab_overlap, base.max.y),
                ),
                corners: MaraCornerRadius::from_corners(tab_radius, 0, tab_radius, 0),
            })
        }
        TitleSide::Right => {
            let cell_top = strip_rect.top() + advance;
            if cell_top + tab_len > strip_rect.bottom() + 0.5 {
                return None;
            }
            let base = MaraRect::from_min_size(
                MaraPos2::new(strip_rect.left(), cell_top),
                MaraVec2::new(strip_rect.width(), tab_len),
            );
            Some(FolderTabCellGeometry {
                base,
                active: MaraRect::from_min_max(
                    MaraPos2::new(base.min.x - tab_overlap, base.min.y),
                    base.max,
                ),
                corners: MaraCornerRadius::from_corners(0, tab_radius, 0, tab_radius),
            })
        }
        TitleSide::Top => {
            let cell_left = strip_rect.left() + advance;
            if cell_left + tab_len > strip_rect.right() + 0.5 {
                return None;
            }
            let base = MaraRect::from_min_size(
                MaraPos2::new(cell_left, strip_rect.top()),
                MaraVec2::new(tab_len, strip_rect.height()),
            );
            Some(FolderTabCellGeometry {
                base,
                active: MaraRect::from_min_max(
                    base.min,
                    MaraPos2::new(base.max.x, base.max.y + tab_overlap),
                ),
                corners: MaraCornerRadius::from_corners(tab_radius, tab_radius, 0, 0),
            })
        }
        TitleSide::Bottom => {
            let cell_left = strip_rect.left() + advance;
            if cell_left + tab_len > strip_rect.right() + 0.5 {
                return None;
            }
            let base = MaraRect::from_min_size(
                MaraPos2::new(cell_left, strip_rect.top()),
                MaraVec2::new(tab_len, strip_rect.height()),
            );
            Some(FolderTabCellGeometry {
                base,
                active: MaraRect::from_min_max(
                    MaraPos2::new(base.min.x, base.min.y - tab_overlap),
                    base.max,
                ),
                corners: MaraCornerRadius::from_corners(0, 0, tab_radius, tab_radius),
            })
        }
    }
}

fn top_tab_cell_geometry(
    title_rect: MaraRect,
    cell_idx: usize,
    cell_count: usize,
) -> Option<TopTabCellGeometry> {
    if cell_count == 0 {
        return None;
    }
    let cell_w = (title_rect.width() / cell_count as f32).max(0.0);
    if cell_w <= 0.0 {
        return None;
    }

    let cell_left = title_rect.left() + (cell_idx as f32) * cell_w;
    let rect = MaraRect::from_min_size(
        MaraPos2::new(cell_left, title_rect.top()),
        MaraVec2::new(cell_w, title_rect.height()),
    );
    let cx = rect.center().x;

    Some(TopTabCellGeometry {
        rect,
        icon_center: MaraPos2::new(cx, rect.top() + rect.height() * 0.32),
        label_center_base: MaraPos2::new(cx, rect.top() + rect.height() * 0.74),
    })
}

/// Paint folder-style tabs into `strip_rect`, projecting from
/// `strip_side` of the container.
///
/// Each tab is a `tab_len`-by-`STRIP_THICKNESS` rectangle with
/// rounded corners on the OUTER edge (away from the container) and
/// square corners on the INNER edge (towards the container) — that's
/// the folder-tab silhouette tipped on its side.
///
/// * **Active tab**: filled with `glass_fill(section_fill, accent, …)`
///   — the SAME color the container body paints — and the rect is
///   extended `tab_overlap` px INWARD so the fill draws over the
///   container's adjacent stroke at this tab's range. Result: no
///   visible seam between tab and body — they read as one shape.
/// * **Inactive tab**: 1 px stroke around all four sides, no fill.
///   Pane bg shows through. Hover adds a faint accent overlay.
#[allow(clippy::too_many_arguments)]
fn paint_folder_tabs(
    ui: &mut Ui,
    strip_rect: MaraRect,
    tab_meta: &[(String, Icon<'static>)],
    tab_ids: &[Id],
    active_idx: usize,
    accent: Color32,
    pane_id: Id,
    active_idx_key: Id,
    strip_side: TitleSide,
    tab_len: f32,
    tab_gap: f32,
    tab_overlap: f32,
) {
    let theme = style::theme();
    let active_fill = style::glass_fill(
        style::section_fill(accent),
        accent,
        style::glass_alpha_card(),
    );
    let icon_size = theme.tabs.folder_icon_size;
    let tab_radius = theme.tabs.folder_active_radius;
    let game_glyph_col = if theme.is_light {
        Color32::BLACK
    } else {
        Color32::WHITE
    };
    let inactive_base = match theme.tabs.inactive_glyph_color {
        style::TabInactiveGlyphColor::TextSecondary => theme.text_secondary.into(),
        style::TabInactiveGlyphColor::HighContrast => game_glyph_col,
    };
    // Inactive cells paint their icon at REDUCED alpha across all
    // themes — at full strength they competed with the active tab.
    // ~78 % lands between "fully visible" and "ghosted out": the
    // active tab still dominates, but inactive icons stay legible
    // enough to read as switchable options at a glance.
    let inactive_glyph_col = inactive_base.gamma_multiply(0.78);
    // Strip orientation: vertical (stack top-to-bottom) for Left/Right
    // strips, horizontal (flow left-to-right) for Top/Bottom strips.
    let strip_horizontal = matches!(strip_side, TitleSide::Top | TitleSide::Bottom);

    // ── Tab drag state (cross-container reorder within this pane) ──
    //
    // `find_drop_target` slots are computed against the cached
    // button rects — which are LAST frame's positions until we
    // push this frame's. Read the drop target FIRST (last-frame
    // basis), then reset this container's button cache so this
    // frame's `push_button` calls replace the stale entries
    // cleanly.
    let parent_pane_id: Id = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp(pane::active_pane_key())
        .unwrap_or(pane_id);
    let drag = pane::tab_drag::drag_state(ui.ctx(), parent_pane_id.into());
    let cursor_pos = crate::backend::egui::pointer_latest_pos(ui.ctx()).map(Into::into);
    let drop_target = match (drag, cursor_pos) {
        (Some(drag), Some(p)) => {
            pane::tab_drag::find_drop_target_for_drag(ui.ctx(), parent_pane_id.into(), p, drag)
        }
        _ => None,
    };
    pane::tab_drag::reset_container_buttons(ui.ctx(), parent_pane_id.into(), pane_id.into());

    // Build the visible cell list. `Some(i)` = paint tab_meta[i] in
    // this cell; `None` = ghost gap (drop preview). Filters out the
    // source-dragged tab when this strip is its source, and inserts
    // a gap at the drop slot when this strip is the drop target.
    let visible: Vec<Option<usize>> = {
        let mut out: Vec<Option<usize>> = Vec::with_capacity(tab_meta.len() + 1);
        if let Some(d) = drag {
            let source_idx_here = if d.source_container == pane_id {
                tab_ids.iter().position(|id| *id == d.tab_id)
            } else {
                None
            };
            for i in 0..tab_meta.len() {
                if Some(i) == source_idx_here {
                    continue;
                }
                out.push(Some(i));
            }
            if let Some((tgt_cid, slot)) = drop_target
                && tgt_cid == pane_id
            {
                let s = slot.min(out.len());
                out.insert(s, None);
            }
        } else {
            for i in 0..tab_meta.len() {
                out.push(Some(i));
            }
        }
        out
    };

    for (cell_idx, slot) in visible.iter().enumerate() {
        let Some(cell) = folder_tab_cell_geometry(
            strip_rect,
            strip_side,
            cell_idx,
            tab_len,
            tab_gap,
            tab_overlap,
            tab_radius,
        ) else {
            break;
        };
        let base_rect: Rect = cell.base.into();
        let Some(&i) = slot.as_ref() else {
            // Drop-slot ghost gap — translucent accent fill so the
            // user sees exactly where the tab will land.
            paint_tab_rect_chrome(
                ui,
                cell.base,
                cell.corners,
                MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 36),
                Some(MaraStroke::new(1.5, accent.into())),
            );
            continue;
        };
        let icn = &tab_meta[i].1;
        let tab_id = tab_ids[i];
        let is_active = i == active_idx;
        let paint_rect = if is_active { cell.active } else { cell.base };
        let resp = {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::layout::UiBackend::interact(
                &mut backend,
                cell.base,
                pane_id.with("mara_tab_btn").with(tab_id).into(),
                crate::layout::Sense::ClickAndDrag,
            )
        };
        pane::tab_drag::push_button(
            ui.ctx(),
            parent_pane_id.into(),
            pane::tab_drag::TabButtonEntry {
                container_id: pane_id.into(),
                tab_id: tab_id.into(),
                rect: base_rect,
            },
        );
        if resp.hovered() && drag.is_none() {
            crate::backend::egui::set_cursor_icon_for_ui(
                ui,
                crate::layout::CursorIcon::PointingHand,
            );
        }
        if resp.clicked() && drag.is_none() {
            {
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                memory.set_persisted(active_idx_key, i);
                memory.set_persisted(active_tab_id_key(active_idx_key), tab_id);
            }
        }
        if resp.drag_started() {
            pane::tab_drag::set_drag(
                ui.ctx(),
                parent_pane_id.into(),
                pane::tab_drag::TabDragState {
                    tab_id: tab_id.into(),
                    source_container: pane_id.into(),
                    cursor: crate::backend::egui::pointer_latest_pos(ui.ctx()).map(Into::into),
                    icon: Some(*icn),
                },
            );
        }
        if is_active {
            // Active tab background — same rounded rect we had before
            // (rounded on the strip-outer edges, square on the
            // body-facing edges, extending `tab_overlap` past the
            // body's edge so the fill overpaints the container's
            // adjacent stroke at this tab's range).
            paint_tab_rect_chrome(ui, paint_rect, cell.corners, active_fill, None);
            // Only the SELECTED tab gets a border, and only on its three
            // OUTER sides — the body-facing side stays open so the tab's
            // outline flows straight into the body's border (folder tab).
            // The fill above already erased the body's border under the
            // tab, so the open ends meet the body border seamlessly.
            let border = style::stroke_for(style::StrokeRole::SectionBorder, accent);
            let points = active_tab_border_points(cell.base, f32::from(tab_radius), strip_side);
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::layout::UiBackend::paint(
                &mut backend,
                PaintCmd::Polyline {
                    points,
                    stroke: border,
                },
            );
            paint_icon_or_svg(
                ui,
                cell.base.center().into(),
                crate::vocab::Align2::CENTER_CENTER,
                *icn,
                icon_size,
                style::contrast_text_for(active_fill).into(),
            );
        } else {
            // Inactive tabs paint NO background — bare icon at
            // reduced alpha so the active tab dominates the strip.
            paint_icon_or_svg(
                ui,
                cell.base.center().into(),
                crate::vocab::Align2::CENTER_CENTER,
                *icn,
                icon_size,
                inactive_glyph_col,
            );
        }
        crate::debug::tag(
            ui,
            base_rect,
            format!("Tab[{}]{}", i, if is_active { "*" } else { "" }),
        );
    }
    pane::tab_drag::push_strip(
        ui.ctx(),
        parent_pane_id.into(),
        pane::tab_drag::TabStripEntry {
            container_id: pane_id.into(),
            rect: strip_rect.into(),
            axis_horizontal: strip_horizontal,
        },
    );
    crate::debug::tag(ui, strip_rect.into(), "TabStrip".to_string());
}

/// Paint GAME-theme tab buttons over the container's title rect,
/// dividing the row into N equal slots — one per tab. Each slot
/// stacks an icon on top of a short label. The active slot fills
/// with `pane_fill(accent)` (= the body's dark fill) so it inverts
/// out of the surrounding accent banner; inactive slots stay
/// transparent and let the banner show through. Click on any slot
/// persists the new active idx for next frame.
#[allow(clippy::too_many_arguments)]
fn paint_top_tabs(
    ui: &mut Ui,
    title_rect: egui::Rect,
    tab_meta: &[(String, Icon<'static>)],
    tab_ids: &[Id],
    active_idx: usize,
    accent: Color32,
    pane_id: Id,
    active_idx_key: Id,
) {
    if tab_meta.is_empty() {
        return;
    }
    // ── Tab drag state (cross-container reorder within this pane) ──
    let parent_pane_id: Id = crate::memory::MaraMemoryCtx::new(ui.ctx()).get_temp(pane::active_pane_key())
        .unwrap_or(pane_id);
    let drag = pane::tab_drag::drag_state(ui.ctx(), parent_pane_id.into());
    let cursor_pos = crate::backend::egui::pointer_latest_pos(ui.ctx()).map(Into::into);
    let drop_target = match (drag, cursor_pos) {
        (Some(drag), Some(p)) => {
            pane::tab_drag::find_drop_target_for_drag(ui.ctx(), parent_pane_id.into(), p, drag)
        }
        _ => None,
    };
    pane::tab_drag::reset_container_buttons(ui.ctx(), parent_pane_id.into(), pane_id.into());

    // Visible cell list — same logic as paint_folder_tabs.
    let visible: Vec<Option<usize>> = {
        let mut out: Vec<Option<usize>> = Vec::with_capacity(tab_meta.len() + 1);
        if let Some(d) = drag {
            let source_idx_here = if d.source_container == pane_id {
                tab_ids.iter().position(|id| *id == d.tab_id)
            } else {
                None
            };
            for i in 0..tab_meta.len() {
                if Some(i) == source_idx_here {
                    continue;
                }
                out.push(Some(i));
            }
            if let Some((tgt_cid, slot)) = drop_target
                && tgt_cid == pane_id
            {
                let s = slot.min(out.len());
                out.insert(s, None);
            }
        } else {
            for i in 0..tab_meta.len() {
                out.push(Some(i));
            }
        }
        out
    };
    // Inverted-from-default tab states for testing:
    //   inactive → solid accent fill.
    //   active   → transparent (pane bg shows through).
    // Glyphs use the contrast colour appropriate for the surface
    // each cell ends up sitting on.
    let inactive_fill = accent;
    let active_text_col = style::contrast_text_for(style::pane_fill(accent));
    let inactive_text_col = style::contrast_text_for(inactive_fill);

    // Icon sizing — inactive cells render at 80 % of the base size
    // (smaller, quieter). Active cell starts ~37 % bigger than
    // inactive when folded and grows to roughly the cell's full
    // height when unfolded, so it reads as a prominent "lift"
    // without fully covering the label below. (Bumped 5 % over
    // the previous 1.3 / 1.7 multipliers per the user.)
    let base_icon_size: f32 = 22.0;
    let inactive_icon_size = base_icon_size * 0.8;
    let active_folded = inactive_icon_size * 1.365;
    let active_unfolded = active_folded * 1.785;
    let openness = pane::body_openness(ui.ctx(), pane_id);
    let openness_t = smoothstep(openness);
    let active_icon_size = crate::vocab::lerp(active_folded, active_unfolded, openness_t);
    let label_font_size: f32 = 11.0;
    let title_rect: MaraRect = title_rect.into();
    for (cell_idx, slot) in visible.iter().enumerate() {
        let Some(cell) = top_tab_cell_geometry(title_rect, cell_idx, visible.len()) else {
            return;
        };
        let cell_rect = cell.rect;
        let cell_rect_egui: Rect = cell_rect.into();
        let Some(&i) = slot.as_ref() else {
            paint_tab_rect_chrome(
                ui,
                cell_rect,
                MaraCornerRadius::ZERO,
                MaraColor32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 56),
                Some(MaraStroke::new(1.5, accent.into())),
            );
            continue;
        };
        let (title, icn) = (&tab_meta[i].0, &tab_meta[i].1);
        let tab_id = tab_ids[i];
        let resp = {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::layout::UiBackend::interact(
                &mut backend,
                cell_rect,
                pane_id.with("mara_top_tab").with(tab_id).into(),
                crate::layout::Sense::ClickAndDrag,
            )
        };
        pane::tab_drag::push_button(
            ui.ctx(),
            parent_pane_id.into(),
            pane::tab_drag::TabButtonEntry {
                container_id: pane_id.into(),
                tab_id: tab_id.into(),
                rect: cell_rect_egui,
            },
        );
        if resp.hovered() && drag.is_none() {
            crate::backend::egui::set_cursor_icon_for_ui(
                ui,
                crate::layout::CursorIcon::PointingHand,
            );
        }
        if resp.clicked() && drag.is_none() {
            {
                let mut memory = crate::memory::MaraMemoryCtx::new(ui.ctx());
                memory.set_persisted(active_idx_key, i);
                memory.set_persisted(active_tab_id_key(active_idx_key), tab_id);
            }
        }
        if resp.drag_started() {
            pane::tab_drag::set_drag(
                ui.ctx(),
                parent_pane_id.into(),
                pane::tab_drag::TabDragState {
                    tab_id: tab_id.into(),
                    source_container: pane_id.into(),
                    cursor: crate::backend::egui::pointer_latest_pos(ui.ctx()).map(Into::into),
                    icon: Some(*icn),
                },
            );
        }
        let is_active = i == active_idx;
        let glyph_col = if is_active {
            active_text_col
        } else {
            inactive_text_col
        };
        if !is_active {
            paint_tab_rect_chrome(
                ui,
                cell_rect,
                MaraCornerRadius::ZERO,
                inactive_fill.into(),
                None,
            );
        }
        // Stack the icon and label vertically inside the cell.
        // Icon centred in the upper half, label in the lower half.
        // Each cell tracks an `active_t` (0..1) animated value —
        // 1 while it's the active tab, 0 otherwise. The icon size
        // lerps between the inactive and active sizes via that
        // value, so when a different tab is clicked the previously-
        // active icon SHRINKS and the newly-active icon GROWS at
        // the same time, smoothly, instead of popping in/out.
        let active_target = if is_active { 1.0 } else { 0.0 };
        let active_t = crate::memory::MaraMemoryCtx::new(ui.ctx()).animate_value(
            pane_id.with("mara_top_tab_active").with(i).into(),
            active_target,
            0.2,
        );
        let icon_size = crate::vocab::lerp(inactive_icon_size, active_icon_size, active_t);
        paint_icon_or_svg(
            ui,
            cell.icon_center.into(),
            crate::vocab::Align2::CENTER_CENTER,
            *icn,
            icon_size,
            glyph_col.into(),
        );
        // Active label slides DOWN by one font-size on click —
        // egui's `animate_value_with_time` smooths the shift so
        // the move reads as an animation, not a teleport.
        let shift_target = if is_active { label_font_size } else { 0.0 };
        let label_shift = crate::memory::MaraMemoryCtx::new(ui.ctx()).animate_value(
            pane_id.with("mara_top_tab_label_shift").with(i).into(),
            shift_target,
            0.2,
        );
        let label_center = MaraPos2::new(
            cell.label_center_base.x,
            cell.label_center_base.y + label_shift,
        );
        paint_cmd(
            ui,
            top_tab_label_paint_cmd(label_center, title, label_font_size, glyph_col),
        );
        crate::debug::tag(
            ui,
            cell_rect_egui,
            format!("TopTab[{}]{}", i, if is_active { "*" } else { "" }),
        );
    }
    pane::tab_drag::push_strip(
        ui.ctx(),
        parent_pane_id.into(),
        pane::tab_drag::TabStripEntry {
            container_id: pane_id.into(),
            rect: title_rect.into(),
            axis_horizontal: true,
        },
    );
    crate::debug::tag(ui, title_rect.into(), "TopTabStrip".to_string());
}

/// Outline points for the SELECTED folder tab: a path along the tab's
/// three OUTER sides with rounded corners on the strip-outer edges, left
/// OPEN on the body-facing edge so it flows into the body's border.
fn active_tab_border_points(base: MaraRect, radius: f32, strip_side: TitleSide) -> Vec<MaraPos2> {
    let r = radius.clamp(0.0, base.width().min(base.height()) * 0.5);
    let (l, rt, t, bm) = (base.left(), base.right(), base.top(), base.bottom());
    let arc = |cx: f32, cy: f32, a0: f32, a1: f32| -> Vec<MaraPos2> {
        let steps = 8;
        (0..=steps)
            .map(|i| {
                let f = i as f32 / steps as f32;
                let a = (a0 + (a1 - a0) * f).to_radians();
                MaraPos2::new(cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    };
    let mut p = Vec::new();
    match strip_side {
        // strip on top → body below → open bottom, round top corners.
        TitleSide::Top => {
            p.push(MaraPos2::new(l, bm));
            p.extend(arc(l + r, t + r, 180.0, 270.0));
            p.extend(arc(rt - r, t + r, 270.0, 360.0));
            p.push(MaraPos2::new(rt, bm));
        }
        // strip on bottom → body above → open top, round bottom corners.
        TitleSide::Bottom => {
            p.push(MaraPos2::new(l, t));
            p.extend(arc(l + r, bm - r, 180.0, 90.0));
            p.extend(arc(rt - r, bm - r, 90.0, 0.0));
            p.push(MaraPos2::new(rt, t));
        }
        // strip on left → body right → open right, round left corners.
        TitleSide::Left => {
            p.push(MaraPos2::new(rt, t));
            p.extend(arc(l + r, t + r, 270.0, 180.0));
            p.extend(arc(l + r, bm - r, 180.0, 90.0));
            p.push(MaraPos2::new(rt, bm));
        }
        // strip on right → body left → open left, round right corners.
        TitleSide::Right => {
            p.push(MaraPos2::new(l, t));
            p.extend(arc(rt - r, t + r, 270.0, 360.0));
            p.extend(arc(rt - r, bm - r, 0.0, 90.0));
            p.push(MaraPos2::new(l, bm));
        }
    }
    p
}

fn paint_tab_rect_chrome(
    ui: &mut Ui,
    rect: MaraRect,
    corner: MaraCornerRadius,
    fill: MaraColor32,
    stroke: Option<MaraStroke>,
) {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    for cmd in tab_rect_chrome_paint_cmds(rect, corner, fill, stroke) {
        crate::layout::UiBackend::paint(&mut backend, cmd);
    }
}

fn tab_rect_chrome_paint_cmds(
    rect: MaraRect,
    corner: MaraCornerRadius,
    fill: MaraColor32,
    stroke: Option<MaraStroke>,
) -> Vec<PaintCmd> {
    let mut commands = vec![PaintCmd::RectFilled { rect, corner, fill }];
    if let Some(stroke) = stroke {
        commands.push(PaintCmd::RectStroke {
            rect,
            corner,
            stroke,
        });
    }
    commands
}

fn top_tab_label_paint_cmd(pos: MaraPos2, title: &str, size: f32, color: MaraColor32) -> PaintCmd {
    PaintCmd::Text {
        pos,
        anchor: MaraAlign2::CENTER_CENTER,
        text: title.to_uppercase(),
        size,
        color,
        mono: false,
    }
}

fn paint_icon_or_svg(
    ui: &mut Ui,
    pos: egui::Pos2,
    align: crate::vocab::Align2,
    icon: Icon<'_>,
    size: f32,
    color: Color32,
) {
    match icon {
        Icon::Name(name) => {
            if let Some(cmd) =
                icon_name_paint_cmd(pos.into(), align.into(), name, size, color.into())
            {
                let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
                crate::layout::UiBackend::paint(&mut backend, cmd);
            }
        }
        Icon::Svg(svg) => {
            let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
            crate::layout::UiBackend::paint(
                &mut backend,
                icon_svg_paint_cmd(pos.into(), align.into(), svg, size, color.into()),
            );
        }
    }
}

fn icon_name_paint_cmd(
    pos: MaraPos2,
    anchor: MaraAlign2,
    name: &str,
    size: f32,
    color: MaraColor32,
) -> Option<PaintCmd> {
    crate::icons::icon_paint_cmd(Icon::Name(name), pos, anchor, size, color)
}

fn icon_svg_paint_cmd(
    pos: MaraPos2,
    anchor: MaraAlign2,
    svg: &str,
    size: f32,
    tint: MaraColor32,
) -> PaintCmd {
    crate::icons::icon_paint_cmd(Icon::Svg(svg), pos, anchor, size, tint)
        .expect("SVG icon payloads always lower to Mara paint commands")
}

fn title_divider_paint_cmd(
    rect: MaraRect,
    title_side: TitleSide,
    gap_half: f32,
    inset: f32,
    color: MaraColor32,
) -> PaintCmd {
    match title_side {
        TitleSide::Top | TitleSide::Bottom => {
            let y = match title_side {
                TitleSide::Top => (rect.bottom() + gap_half).round() + 0.5,
                _ => (rect.top() - gap_half).round() - 0.5,
            };
            PaintCmd::Line {
                a: MaraPos2::new(rect.left() + inset, y),
                b: MaraPos2::new(rect.right() - inset, y),
                stroke: MaraStroke::new(1.0, color),
            }
        }
        TitleSide::Left | TitleSide::Right => {
            let x = match title_side {
                TitleSide::Left => (rect.right() + gap_half).round() + 0.5,
                _ => (rect.left() - gap_half).round() - 0.5,
            };
            PaintCmd::Line {
                a: MaraPos2::new(x, rect.top() + inset),
                b: MaraPos2::new(x, rect.bottom() - inset),
                stroke: MaraStroke::new(1.0, color),
            }
        }
    }
}

/// Paint the title strip into `rect`. Theme-aware:
/// * Title size, letter-spacing, font family, brackets, chevron all
///   from `theme()`.
/// * UPPERCASE always.
/// * `[ TITLE ]` brackets when `theme.section_title_brackets` —
///   layout space is reserved even when invisible so the title text
///   doesn't shift between collapsed / open.
/// * Chevron prefix when `theme.show_section_chevron` (PRO).
/// * Hairline divider on the body-facing edge in PRO; banner cover
///   in GAME (painted by caller).
#[allow(clippy::too_many_arguments)]
fn paint_title(
    ui: &mut Ui,
    rect: egui::Rect,
    title: &str,
    anchor: PaneAnchor,
    accent: Color32,
    open: bool,
    openness: f32,
    icon: Option<Icon<'_>>,
    pane_id: Id,
) {
    // Publish the title's allocated rect for callers that need to
    // know exactly where the title row landed — used by
    // `Normal::show_tabs` (GAME path) to overlay tab buttons on the
    // title row after the container has rendered.
    ui.ctx().data_mut(|d| {
        d.insert_temp(egui::Id::from(pane_id.with("mara_normal_title_rect")), rect);
    });

    let theme = style::theme();
    let container_theme = theme.container;
    let title_side = anchor.title_side();
    let filled = theme.title_strip_filled;
    let title_col: Color32 = if filled {
        style::contrast_text_for(accent).into()
    } else {
        style::section_title_color(accent).into()
    };

    let title_family =
        crate::backend::egui::available_text_family_for_ui(ui, style::title_font_family());
    let bracket_visible = theme.section_title_brackets && !open;
    let any_brackets = theme.section_title_brackets;
    let title_uc = title.to_uppercase();
    // Inline icon dispatch:
    //   PRO (`section_icon_at_end = false`): icon glyph is prepended
    //     to the title text runs so it tracks the same scramble /
    //     glitch / rotation pipeline as the title.
    //   GAME (`section_icon_at_end = true`): icon floats at the
    //     strip's far end after the title text is painted, with a
    //     smoothstep-eased size lerp keyed off `openness` (small when
    //     folded → large overflowing the strip when open).
    // SVG icons can't be inlined into title text runs, so they fall
    // through to the floating-paint path even in PRO.
    let inline_icon = !theme.section_icon_at_end;
    // GAME theme: scramble-decode the title each time the container
    // reappears (matching the old `section_tracked` recipe), AND
    // every time the user folds / unfolds the container. The
    // scramble id is salted with two values:
    //   • `appearance_session(...)` — bumps when the title widget
    //     was missing for a frame and reappeared (e.g. pane closed
    //     and reopened).
    //   • `pane::fold_version(pane_id)` — bumps inside `toggle_body`
    //     on every fold / unfold click.
    // Either one changing produces a fresh `scramble_id`, which
    // makes `scramble_text` see no stored prev for this id and
    // restart the decode cycle from t = 0.
    let displayed = if theme.scramble_titles {
        let session_id = ui.id().with(("mara_normal_title_session", title));
        let session = style::appearance_session(ui.ctx(), session_id);
        let fold_ver = pane::fold_version(ui.ctx(), pane_id);
        let scramble_id = session_id.with(session).with(fold_ver);
        let active = ui.opacity() >= 0.95;
        let scrambled = style::scramble_text(ui.ctx(), scramble_id, &title_uc, active);
        // Post-stabilisation glitch: every ~5 s a random letter
        // momentarily becomes a scramble symbol and reverts.
        style::glitch_text(ui.ctx(), session_id.with("glitch"), &scrambled)
    } else {
        title_uc
    };

    let title_color: MaraColor32 = title_col.into();
    let bracket_color = if bracket_visible {
        title_color
    } else {
        MaraColor32::TRANSPARENT
    };
    let mut title_runs = Vec::new();
    let mut push_title_run =
        |text: String, leading_space: f32, size: f32, family: TextFamily, color: MaraColor32| {
            if !text.is_empty() {
                title_runs.push(TextRun {
                    text,
                    size,
                    color,
                    family,
                    extra_letter_spacing: theme.section_title_letter_spacing,
                    leading_space,
                });
            }
        };

    // Optional theme prefix (PRO only — drops when bracket framing
    // is on so `▸ [ … ]` doesn't read as cluttered).
    if let (Some(prefix), false) = (theme.section_title_prefix, any_brackets) {
        push_title_run(
            prefix.to_owned(),
            0.0,
            theme.section_title_size,
            title_family.clone(),
            title_color,
        );
        push_title_run(
            " ".to_owned(),
            0.0,
            theme.section_title_size,
            title_family.clone(),
            title_color,
        );
    }
    if any_brackets {
        push_title_run(
            "[ ".to_owned(),
            0.0,
            theme.section_title_size,
            title_family.clone(),
            bracket_color,
        );
    }
    // Resolve the inline-icon glyph + family ONCE, then decide
    // whether it appears before or after the title text. The chevron
    // paints separately at the strip's reading-start, so the icon
    // wants to sit BETWEEN the chevron and the title text:
    //   • horizontal non-reversed (TM, BM, BS-as-Left? actually just
    //     anchors with `title_reversed = false`): chevron on LEFT,
    //     title run renders LTR, so `icon, title` is correct.
    //   • horizontal reversed (RS = RightRail Start → Top, RE =
    //     RightRail End → Bottom): chevron on RIGHT, title runs still
    //     renders LTR, so `title, icon` puts icon adjacent to the
    //     chevron. Without this swap the icon ended up on the FAR
    //     left of the strip — opposite the chevron — and the user's
    //     chevron→icon→title reading order broke.
    //   • vertical strips: rotated title runs place the
    //     first character closest to the chevron regardless of
    //     direction (CW for top_to_bottom, CCW otherwise), so
    //     `icon, title` is always correct.
    let icon_after_title = title_side.is_horizontal_strip() && anchor.title_reversed();
    let inline_glyph: Option<(String, String)> =
        if inline_icon && crate::icons::icon_fonts_ready() {
            match icon {
                Some(Icon::Name(name)) => crate::icons::icon_glyph(name)
                    .map(|(glyph, family)| (glyph.to_string(), family)),
                _ => None,
            }
        } else {
            None
        };
    // Inline-icon glyph is rendered 20 % larger than the title text
    // — Fluent glyphs are designed at a square optical size and
    // visually feel small next to a same-pt UPPERCASE caption, so a
    // small bump pulls the icon weight up to match the title.
    let icon_theme = theme.icons;
    let inline_icon_size = theme.section_title_size * icon_theme.section_inline_scale;
    // Px gap between the icon and the title — applied via egui's
    // `leading_space` on the next segment, which produces a clean
    // horizontal gap independent of the chosen separator character.
    if !icon_after_title && let Some((glyph, family)) = &inline_glyph {
        push_title_run(
            glyph.clone(),
            0.0,
            inline_icon_size,
            TextFamily::Named(family.clone()),
            title_color,
        );
    }
    let title_lead = if !icon_after_title && inline_glyph.is_some() {
        icon_theme.section_icon_title_gap
    } else {
        0.0
    };
    push_title_run(
        displayed,
        title_lead,
        theme.section_title_size,
        title_family.clone(),
        title_color,
    );
    if icon_after_title && let Some((glyph, family)) = &inline_glyph {
        push_title_run(
            glyph.clone(),
            icon_theme.section_icon_title_gap,
            inline_icon_size,
            TextFamily::Named(family.clone()),
            title_color,
        );
    }
    if any_brackets {
        push_title_run(
            " ]".to_owned(),
            0.0,
            theme.section_title_size,
            title_family,
            bracket_color,
        );
    }
    let title_size = crate::backend::egui::measure_text_runs_for_ui(ui, &title_runs);

    match title_side {
        TitleSide::Top | TitleSide::Bottom => {
            // Optional chevron painted ahead of the title text.
            let mut text_inset = container_theme.title_inset;
            if theme.show_section_chevron {
                let chevron_x = if anchor.title_reversed() {
                    rect.right() - container_theme.title_inset - icon_theme.section_chevron_w * 0.5
                } else {
                    rect.left() + container_theme.title_inset + icon_theme.section_chevron_w * 0.5
                };
                paint_chevron_h(
                    ui,
                    rect.into(),
                    MaraPos2::new(chevron_x, rect.center().y),
                    title_side,
                    if open { 1.0 } else { 0.0 },
                    title_col.into(),
                );
                text_inset = container_theme.title_inset
                    + icon_theme.section_chevron_w
                    + icon_theme.section_chevron_gap;
            }

            let (text_pos, text_anchor) = if anchor.title_reversed() {
                (
                    MaraPos2::new(rect.right() - text_inset, rect.center().y),
                    MaraAlign2::RIGHT_CENTER,
                )
            } else {
                (
                    MaraPos2::new(rect.left() + text_inset, rect.center().y),
                    MaraAlign2::LEFT_CENTER,
                )
            };
            paint_cmd_clipped(
                ui,
                rect.into(),
                PaintCmd::TextRuns {
                    pos: text_pos,
                    anchor: text_anchor,
                    angle: 0.0,
                    runs: title_runs.clone(),
                },
            );

            // Body-facing divider — PRO only, when expanded.
            if !filled && open {
                paint_cmd(
                    ui,
                    title_divider_paint_cmd(
                        rect.into(),
                        title_side,
                        container_theme.title_body_gap_half,
                        container_theme.divider_inset,
                        theme.border_subtle.into(),
                    ),
                );
            }
        }
        TitleSide::Left | TitleSide::Right => {
            let cx = rect.center().x;
            let on_right_side = title_side == TitleSide::Right;
            let top_to_bottom = on_right_side ^ anchor.title_reversed();

            // Optional chevron at the reading-start of the title.
            let mut text_inset = container_theme.title_inset;
            if theme.show_section_chevron {
                let chevron_y = if top_to_bottom {
                    rect.top() + container_theme.title_inset + icon_theme.section_chevron_w * 0.5
                } else {
                    rect.bottom() - container_theme.title_inset - icon_theme.section_chevron_w * 0.5
                };
                paint_chevron_h(
                    ui,
                    rect.into(),
                    MaraPos2::new(cx, chevron_y),
                    title_side,
                    if open { 1.0 } else { 0.0 },
                    title_col.into(),
                );
                text_inset = container_theme.title_inset
                    + icon_theme.section_chevron_w
                    + icon_theme.section_chevron_gap;
            }

            let (text_pos, angle) = if top_to_bottom {
                (
                    MaraPos2::new(
                        (cx + title_size.y * 0.5).round(),
                        (rect.min.y + text_inset).round(),
                    ),
                    std::f32::consts::FRAC_PI_2,
                )
            } else {
                (
                    MaraPos2::new(
                        (cx - title_size.y * 0.5).round(),
                        (rect.max.y - text_inset).round(),
                    ),
                    -std::f32::consts::FRAC_PI_2,
                )
            };
            paint_cmd_clipped(
                ui,
                rect.into(),
                PaintCmd::TextRuns {
                    pos: text_pos,
                    anchor: MaraAlign2::LEFT_TOP,
                    angle,
                    runs: title_runs.clone(),
                },
            );

            if !filled && open {
                paint_cmd(
                    ui,
                    title_divider_paint_cmd(
                        rect.into(),
                        title_side,
                        container_theme.title_body_gap_half,
                        container_theme.divider_inset,
                        theme.border_subtle.into(),
                    ),
                );
            }
        }
    }

    // Floating icon (GAME mode) — paints AFTER the title text so it
    // rides on top of the banner. Small when folded so it tucks inside the collapsed
    // banner, big when open so it overflows the strip and reads as a
    // floating ornament. The growth is `smoothstep`-eased so it pops
    // through `cubic-bezier(0.42, 0, 0.58, 1)` rather than linear.
    if !inline_icon && let Some(icon_src) = icon {
        paint_floating_icon(ui, rect, anchor, title_col, openness, icon_src);
    }
}

/// Paint a "floating" icon on the title strip — small when folded,
/// big when open. The icon overflows the strip's body-facing edge
/// when fully open, framed by clipping +8 px around the painted
/// rect. Vertical strips paint the icon centred (no rotation —
/// Fluent glyphs read fine in either orientation, and rotating
/// would require rotated rich text just for a decoration).
///
/// Painted on `Order::Foreground` so the icon sits ABOVE the ribbon
/// buttons (`Order::Middle`) and the pane chrome (`Order::Background`).
fn paint_floating_icon(
    ui: &mut Ui,
    strip_rect: egui::Rect,
    anchor: PaneAnchor,
    title_col: Color32,
    openness: f32,
    icon_src: Icon<'_>,
) {
    let theme = style::theme();
    let base_size = theme.section_icon_size.max(0.0);
    if base_size <= 0.0 {
        return;
    }
    // Tuned values — keep them in sync if the section icon animation
    // gets re-tuned.
    let folded_size = base_size * 0.85;
    let unfolded_size = base_size * 2.9106;
    const UNFOLDED_OFFSET: f32 = 29.294;
    let folded_offset = folded_size * 0.5;
    let t = smoothstep(openness);
    let size = crate::vocab::lerp(folded_size, unfolded_size, t);
    let offset = crate::vocab::lerp(folded_offset, UNFOLDED_OFFSET, t);

    let icon = floating_icon_geometry(strip_rect.into(), anchor, size, offset, t);
    // Floating icon paints at the `CONTAINER_FLOATING_ICON` tier —
    // above container chrome and corner ticks, below any
    // fullscreen / maximize overlay so the icon doesn't bleed
    // through a maximised node graph / code editor.
    // Foreground-layer painters do NOT inherit the parent ui's
    // opacity, so during the stagger fade the icon would otherwise
    // pop in at full alpha while the container chrome was still
    // fading. Mirror the parent's opacity onto this layer's
    // painter so the icon fades with its container.
    let layer_id: MaraId = ui.id().with("mara_floating_icon_layer").into();
    let parent_opacity = ui.opacity();
    match icon_src {
        Icon::Name(name) => {
            if let Some(cmd) =
                icon_name_paint_cmd(icon.pos, icon.align, name, size, title_col.into())
            {
                crate::backend::egui::render_paint_cmd_on_z_layer(
                    ui,
                    layer_id,
                    crate::layer::z::CONTAINER_FLOATING_ICON,
                    icon.rect,
                    parent_opacity,
                    cmd,
                );
            }
        }
        Icon::Svg(svg) => {
            crate::backend::egui::render_paint_cmd_on_z_layer(
                ui,
                layer_id,
                crate::layer::z::CONTAINER_FLOATING_ICON,
                icon.rect,
                parent_opacity,
                icon_svg_paint_cmd(icon.pos, icon.align, svg, size, title_col.into()),
            );
        }
    }
}

fn paint_cmd(ui: &mut Ui, cmd: PaintCmd) {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    crate::layout::UiBackend::paint(&mut backend, cmd);
}

fn paint_cmd_clipped(ui: &mut Ui, clip: MaraRect, cmd: PaintCmd) {
    let mut backend = crate::backend::egui::EguiUiBackend::new(ui);
    crate::layout::UiBackend::push_clip(&mut backend, clip);
    crate::layout::UiBackend::paint(&mut backend, cmd);
    crate::layout::UiBackend::pop_clip(&mut backend);
}

/// Polynomial smoothstep, `t * t * (3 - 2t)`. Approximates
/// `cubic-bezier(0.42, 0, 0.58, 1)` for a gentle ease-in-ease-out —
/// used by the foldable-section animation.
#[inline]
fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// "Ease-out-elastic" — exponentially damped sine that overshoots
/// past 1.0 once before settling. `t = 0 → 0`, `t = 1 → 1` exactly
/// (both endpoints early-return). Tuned subtle: a fast decay
/// (`exp(-5.0 t)`) plus an `AMP = 0.45` scale on the deviation
/// keeps the overshoot small (~5 %) and the undershoot barely
/// perceptible — a hint of bounce, not a wobble.
#[inline]
fn ease_out_elastic(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    if t == 0.0 {
        return 0.0;
    }
    if t == 1.0 {
        return 1.0;
    }
    const AMP: f32 = 0.45;
    let c = std::f32::consts::TAU / 3.0;
    -(AMP * (-5.0 * t).exp() * ((t * 3.5 - 0.75) * c).sin()) + 1.0
}

/// Paint a chevron at `center` rotated to match the title side and
/// `openness` 0..=1. Glyph reads `›` (closed) → `⌄` (open) for a
/// Top title; mirrored / rotated for the other three sides.
fn paint_chevron_h(
    ui: &mut Ui,
    clip: MaraRect,
    center: MaraPos2,
    title_side: TitleSide,
    openness: f32,
    tint: MaraColor32,
) {
    paint_cmd_clipped(
        ui,
        clip,
        chevron_h_paint_cmd(center, title_side, openness, tint),
    );
}

fn chevron_h_paint_cmd(
    center: MaraPos2,
    title_side: TitleSide,
    openness: f32,
    tint: MaraColor32,
) -> PaintCmd {
    const GLYPH_W: f32 = 8.0;
    const GLYPH_H: f32 = 5.0;
    let hw = GLYPH_W * 0.5;
    let hh = GLYPH_H * 0.5;
    // Base shape `⌄`: arms at top corners, apex at bottom centre.
    let raw = [(-hw, -hh), (0.0, hh), (hw, -hh)];
    use std::f32::consts::TAU;
    // Closed → open angle ranges per side:
    //   Top:    -90° → 0°   (›  → ⌄)
    //   Bottom: -90° → 180° (›  → ^)
    //   Left:    0°  → -90° (⌄  → ›)
    //   Right:   0°  →  90° (⌄  → ‹)
    let (closed, open) = match title_side {
        TitleSide::Top => (-TAU / 4.0, 0.0),
        TitleSide::Bottom => (-TAU / 4.0, TAU / 2.0),
        TitleSide::Left => (0.0, -TAU / 4.0),
        TitleSide::Right => (0.0, TAU / 4.0),
    };
    let angle = closed + (open - closed) * openness.clamp(0.0, 1.0);
    let (sin, cos) = angle.sin_cos();
    let points: Vec<MaraPos2> = raw
        .iter()
        .map(|&(x, y)| {
            let rx = x * cos - y * sin;
            let ry = x * sin + y * cos;
            MaraPos2::new(center.x + rx, center.y + ry)
        })
        .collect();
    PaintCmd::Polyline {
        points,
        stroke: MaraStroke::new(1.6, tint),
    }
}

/// Paint L-shaped corner ticks around `outer_rect`. Gated on
/// `theme.section_corner_ticks > 0` (GAME enables, PRO disables —
/// PRO ships `0.0` so this whole function returns early there).
/// Title-side corners use the contrast colour (white-on-banner);
/// body-side corners use a breathing-accent so they pulse against
/// the panel surface.
///
/// `openness` drives a **corner-bracket snap** on container open:
///
/// * Brackets start `START_OFFSET` px outside the rest position
///   when the container is collapsed.
/// * `snap_t = (openness × SNAP_RATIO).clamp(0, 1)` reaches `1` when
///   openness ≈ `1 / SNAP_RATIO`, so the snap completes BEFORE the
///   body finishes opening — chrome lands first, body fills in.
/// * `ease_out_back` produces a small overshoot past rest before
///   settling, plus a fade-in driven by the same `snap_t` so a
///   collapsed container doesn't have ticks "floating" outside it.
// Per-container stable id passed in as `container_id`. `ui.id()`
// inside the function is the Frame's content_ui id which collapses
// to `parent.with("child")` — the SAME id for every sibling Frame
// in the same parent — so we can't key per-container snap state on
// it. The caller passes the Normal's own `pane_id` (= the
// container's `cid`, unique per stack slot) and we key state under
// that.
fn paint_corner_ticks(
    ui: &mut Ui,
    outer_rect: egui::Rect,
    accent: Color32,
    title_side: TitleSide,
    openness: f32,
    container_id: Id,
) {
    let theme = style::theme();
    let tick_len = theme.section_corner_ticks;
    if tick_len <= 0.0 {
        return;
    }
    let rest_inset = theme.section_corner_ticks_inset;
    // Snap-in animation parameters. The snap clock starts only
    // when `ui.opacity() >= 0.95` — i.e. AFTER the per-section
    // staggered fade-in has essentially finished — so the user
    // actually sees the brackets fly in instead of having the
    // animation play out invisibly under the fade. Same gating
    // pattern the cipher uses.
    const APPEAR_DUR: f32 = 1.0;
    // Brackets fly in from this many pixels OUTSIDE the rest position.
    // Reduced 10 → 7 so a fully-collapsed container's corner ticks
    // sit 3 px closer to the frame edge (the user complained the old
    // value left them visibly floating outside the container).
    const START_OFFSET: f32 = 7.0;
    // Gate at `1.0 - ε` (not `0.95`) so the snap starts only AFTER
    // this container's stagger fade has fully completed, not 5 %
    // before the end. `stagger_opacity` reaches exactly `1.0` at
    // the end of the fade (smoothstep at `t = 1.0` is `1.0`), and
    // `multiply_opacity` is skipped when `stagger_opacity == 1.0`
    // → `ui.opacity()` jumps to exactly `1.0` — `0.999` is just
    // a float-tolerance cushion against rounding.
    const OPACITY_GATE: f32 = 0.999;
    /// Extra delay between the fade completing and the snap
    /// starting — the brackets sit motionless at their start
    /// position for this long after the container becomes fully
    /// opaque, then fly in. Gives the eye a beat to register
    /// "container has arrived" before the next motion starts.
    const DELAY_AFTER_FADE: f64 = 0.25;
    let snap_id = container_id.with("mara_corner_snap");
    let prev_active_id = snap_id.with("prev_active");
    let prev_body_open_id = snap_id.with("prev_body_open");
    let first_seen_id = snap_id.with("first_seen");
    let now = crate::backend::egui::input_time(ui.ctx());
    let opacity_active = ui.opacity() >= OPACITY_GATE;
    let body_open_now: bool = ui.ctx().data_mut(|d| {
        d.get_persisted::<bool>(egui::Id::from(container_id.with("body_open")))
            .unwrap_or(true)
    });
    // `first_seen` is the start-of-snap timestamp. It's set on
    // either of two events and otherwise left alone, so idle paints
    // never replay the animation:
    //   1. Opacity transitions INACTIVE → ACTIVE — i.e. the per-
    //      section staggered fade-in finishes after a real
    //      reappearance (pane just opened or toggled). Fires for
    //      ALL containers in the pane.
    //   2. THIS container's `body_open` flips false → true — the
    //      user unfolded the section. Fires for the single
    //      affected container only; folding (true → false) doesn't
    //      re-fire (the container is going away, the brackets just
    //      track its shrinking edge).
    let first_seen: Option<f64> = ui.ctx().data_mut(|d| {
        let prev_active = d.get_temp::<bool>(egui::Id::from(prev_active_id)).unwrap_or(false);
        d.insert_temp(egui::Id::from(prev_active_id), opacity_active);
        let became_inactive = prev_active && !opacity_active;

        let prev_body_open = d
            .get_temp::<bool>(egui::Id::from(prev_body_open_id))
            .unwrap_or(body_open_now);
        d.insert_temp(egui::Id::from(prev_body_open_id), body_open_now);
        let just_unfolded = !prev_body_open && body_open_now;

        if became_inactive || just_unfolded {
            // Either the whole pane started fading out (will fade
            // back in shortly), or the user just unfolded this
            // section. Drop the recorded `first_seen` so the next
            // active frame re-arms the snap.
            d.remove::<f64>(egui::Id::from(first_seen_id));
        }
        let existing = d.get_temp::<f64>(egui::Id::from(first_seen_id));
        match (existing, opacity_active) {
            (Some(t), _) => Some(t),
            (None, true) => {
                // Bias `first_seen` into the future by
                // `DELAY_AFTER_FADE` so `appear = now - first_seen`
                // stays negative (clamped to 0) for that delay,
                // pinning brackets at the start position. The
                // snap then kicks off naturally once `now` catches
                // up with the biased first_seen.
                let biased = now + DELAY_AFTER_FADE;
                d.insert_temp(egui::Id::from(first_seen_id), biased);
                Some(biased)
            }
            (None, false) => None,
        }
    });
    let appear = match first_seen {
        Some(t) => (((now - t) as f32) / APPEAR_DUR).clamp(0.0, 1.0),
        None => 0.0,
    };
    if appear < 1.0 {
        crate::backend::egui::request_repaint(ui.ctx());
    }
    // Snap progress is driven by `appear` ALONE — re-arming events
    // (pane launch, single-container unfold) drop `first_seen`,
    // which restarts `appear` at 0 and lets `ease_out_elastic`
    // bounce the brackets in over `APPEAR_DUR`. We deliberately do
    // NOT factor `openness` in here: the previous version used
    // `appear.min(openness_t)`, which made the brackets fly OUT
    // during a fold (openness 1 → 0 dragged the easing curve
    // backward through its overshoot region) and re-fly-in during
    // an unfold. With elastic that produced a visible vertical
    // shift on folded panes — the bracket landed past rest, then
    // the outer_rect shrank around it so it appeared offset DOWN
    // from where it should be. By using `appear` alone, the
    // brackets sit exactly at `rest_inset` whenever no snap is
    // playing — folded or open, the placement is identical
    // relative to the (animated) outer_rect.
    let snap_t = appear;
    let snap = ease_out_elastic(snap_t);
    let extra = crate::vocab::lerp(-START_OFFSET, 0.0, snap);
    // Resting inset lerps with `openness`: when fully open, brackets
    // sit `rest_inset` px INSIDE the painted outer_rect (theme
    // value, gives breathing room from the frame stroke). When
    // fully folded, they slide out to `FOLDED_INSET` — slightly
    // OUTSIDE the painted edge — so the title strip reads as a
    // self-contained mark with the brackets clinging to its
    // border, not nested inside a small box. `extra` (the snap-in
    // offset) is added on top, so the elastic bounce still plays
    // around whatever resting inset the current fold state picks.
    const FOLDED_INSET: f32 = -1.0;
    let resting = crate::vocab::lerp(FOLDED_INSET, rest_inset, openness);
    let inset = resting + extra;
    let r = outer_rect.shrink(inset);

    // Snap the L-bracket corner positions for a 2-px stroke. The
    // line is drawn centred on `(snap_low|snap_high)(edge)`; with
    // `width = 2.0` the stroke straddles ±1 px around the centre.
    // Using `+ 0.5 / - 0.5` (the right offsets for a 1-px line)
    // pushed half the stroke OUTSIDE the rect on the min edges
    // (left / top) while keeping max edges flush — the visible 1-px
    // overflow on left + bottom. `+ 1.0 / - 1.0` centres the stroke
    // 1 px inside the rounded edge so the full 2-px bar sits inside
    // the rect on every side.
    let snap_low = |v: f32| v.round() + 1.0;
    let snap_high = |v: f32| v.round() - 1.0;
    let lx = snap_low(r.min.x);
    let ty = snap_low(r.min.y);
    let rx = snap_high(r.max.x);
    let by = snap_high(r.max.y);
    let len = tick_len;

    let contrast_col = style::contrast_text_for(accent);
    // Body-side corner ticks paint in the EXACT accent the caller
    // passed — not the brightness-adjusted `high_contrast_accent`
    // variant. The user picks an accent and expects to see THAT
    // colour; the brightness lift was producing a tick that read
    // off-hue from every other accent surface.
    //
    // Brackets sit at full opacity at rest. There's no breathing
    // pulse — a slow alpha sine on the body-side accent reads as
    // unwanted motion in the user's peripheral vision and forces a
    // 30-fps repaint loop just to drive the fade. Snap-in still
    // animates on first appearance / fold-unfold; once that
    // settles, the brackets are static.
    let bracket_accent = accent;
    let accent_col = Color32::from_rgba_unmultiplied(
        bracket_accent.r(),
        bracket_accent.g(),
        bracket_accent.b(),
        255,
    );
    let contrast_col =
        Color32::from_rgba_unmultiplied(contrast_col.r(), contrast_col.g(), contrast_col.b(), 255);
    // Body-side bracket colour LERPS from contrast (folded) to
    // accent (unfolded). Folded → all four corners paint in the
    // contrast colour (the "other" colour against the accent panel).
    // As the body unfolds, the body-side pair fades to the accent.
    // Title-side ticks stay contrast throughout (they sit on the
    // accent banner regardless of fold state, so contrast is the
    // only readable choice there).
    let lerp_u8 = |a: u8, b: u8, t: f32| ((a as f32) * (1.0 - t) + (b as f32) * t).round() as u8;
    let body_side_col = Color32::from_rgba_unmultiplied(
        lerp_u8(contrast_col.r(), accent_col.r(), openness),
        lerp_u8(contrast_col.g(), accent_col.g(), openness),
        lerp_u8(contrast_col.b(), accent_col.b(), openness),
        lerp_u8(contrast_col.a(), accent_col.a(), openness),
    );
    // Pick which corners are "title-side" vs "body-side" per anchor.
    let (tl, tr, bl, br) = match title_side {
        TitleSide::Top => (contrast_col, contrast_col, body_side_col, body_side_col),
        TitleSide::Bottom => (body_side_col, body_side_col, contrast_col, contrast_col),
        TitleSide::Left => (contrast_col, body_side_col, contrast_col, body_side_col),
        TitleSide::Right => (body_side_col, contrast_col, body_side_col, contrast_col),
    };
    // L-brackets paint at the `CONTAINER_TICKS` tier — above the
    // pane content / container chrome, below any fullscreen
    // overlay (`FULLSCREEN` tier) and above tab-cell fills.
    let layer_id: MaraId = container_id.with("mara_corner_ticks").into();
    for cmd in corner_tick_paint_cmds(
        lx,
        ty,
        rx,
        by,
        len,
        [tl.into(), tr.into(), bl.into(), br.into()],
    ) {
        crate::backend::egui::render_paint_cmd_on_z_layer(
            ui,
            layer_id,
            crate::layer::z::CONTAINER_TICKS,
            outer_rect.into(),
            ui.opacity(),
            cmd,
        );
    }
}

fn corner_tick_paint_cmds(
    lx: f32,
    ty: f32,
    rx: f32,
    by: f32,
    len: f32,
    colors: [MaraColor32; 4],
) -> Vec<PaintCmd> {
    // Doubled-thickness stroke (was 1.0) so the corner ticks read
    // as bold marks rather than hairlines — easier to spot and
    // gives the GAME chrome more visual weight.
    let stroke = |c: MaraColor32| MaraStroke::new(2.0, c);
    let [tl, tr, bl, br] = colors;
    vec![
        // ┌ top-left
        PaintCmd::Line {
            a: MaraPos2::new(lx, ty),
            b: MaraPos2::new(lx + len, ty),
            stroke: stroke(tl),
        },
        PaintCmd::Line {
            a: MaraPos2::new(lx, ty),
            b: MaraPos2::new(lx, ty + len),
            stroke: stroke(tl),
        },
        // ┐ top-right
        PaintCmd::Line {
            a: MaraPos2::new(rx - len, ty),
            b: MaraPos2::new(rx, ty),
            stroke: stroke(tr),
        },
        PaintCmd::Line {
            a: MaraPos2::new(rx, ty),
            b: MaraPos2::new(rx, ty + len),
            stroke: stroke(tr),
        },
        // └ bottom-left
        PaintCmd::Line {
            a: MaraPos2::new(lx, by - len),
            b: MaraPos2::new(lx, by),
            stroke: stroke(bl),
        },
        PaintCmd::Line {
            a: MaraPos2::new(lx, by),
            b: MaraPos2::new(lx + len, by),
            stroke: stroke(bl),
        },
        // ┘ bottom-right
        PaintCmd::Line {
            a: MaraPos2::new(rx - len, by),
            b: MaraPos2::new(rx, by),
            stroke: stroke(br),
        },
        PaintCmd::Line {
            a: MaraPos2::new(rx, by - len),
            b: MaraPos2::new(rx, by),
            stroke: stroke(br),
        },
    ]
}

/// Compute the maximum natural body height across a tabbed
/// container's [`super::Tab`] list. The body height of one tab is
/// `sum(pod.natural_h() + per-pod chrome)` plus inter-pod separator
/// strips. The container body's flow extent is locked to this max
/// so the container's size stays constant when switching tabs:
/// shorter tabs leave trailing whitespace; the body's own clip
/// rect handles any rare case where a tab's content exceeds the
/// max (it shouldn't, since max is by definition ≥ every tab).
fn max_tab_natural_body_h(tabs: &[super::Tab]) -> f32 {
    let container_theme = style::theme().container;
    let pod_chrome_each = (container_theme.pod_pad_y as f32) * 2.0;
    let sep_h = crate::container::separator::separator_strip_h();
    tabs.iter()
        .map(|t| {
            let n = t.pods.len();
            let pods_h: f32 = t.pods.iter().map(|p| p.natural_h() + pod_chrome_each).sum();
            let sep_total = if n > 1 { (n - 1) as f32 * sep_h } else { 0.0 };
            pods_h + sep_total
        })
        .fold(0.0_f32, f32::max)
}

#[cfg(test)]
mod active_tab_tests {
    use crate::paint::TextFamily;

    use super::*;

    #[test]
    fn tabbed_container_max_rect_reserves_right_strip_inside_available_rect() {
        let avail = MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(310.0, 620.0));
        let rect = tabbed_container_max_rect(avail, TitleSide::Right, 26.0, 14.0);

        assert_eq!(rect.left(), avail.left());
        assert_eq!(rect.right(), avail.right() - 40.0);
        assert_eq!(rect.top(), avail.top());
        assert_eq!(rect.bottom(), avail.bottom());
    }

    #[test]
    fn tabbed_container_max_rect_mirrors_left_and_right_strip_reservation() {
        let avail = MaraRect::from_min_max(MaraPos2::new(10.0, 20.0), MaraPos2::new(310.0, 620.0));
        let left = tabbed_container_max_rect(avail, TitleSide::Left, 26.0, 14.0);
        let right = tabbed_container_max_rect(avail, TitleSide::Right, 26.0, 14.0);

        assert_eq!(left.width(), right.width());
        assert_eq!(left.left(), avail.left() + 40.0);
        assert_eq!(right.right(), avail.right() - 40.0);
    }

    #[test]
    fn title_and_body_slot_sizes_use_mara_vectors_for_both_orientations() {
        assert_eq!(
            title_slot_size(true, 280.0, 22.0),
            MaraVec2::new(280.0, 22.0)
        );
        assert_eq!(
            title_slot_size(false, 280.0, 22.0),
            MaraVec2::new(22.0, 280.0)
        );

        assert_eq!(
            body_slot_sizes(true, 280.0, 120.0, 200.0),
            BodySlotSizes {
                visible: MaraVec2::new(280.0, 120.0),
                full: MaraVec2::new(280.0, 200.0),
            }
        );
        assert_eq!(
            body_slot_sizes(false, 280.0, 120.0, 200.0),
            BodySlotSizes {
                visible: MaraVec2::new(120.0, 280.0),
                full: MaraVec2::new(200.0, 280.0),
            }
        );
    }

    #[test]
    fn body_full_rect_anchors_to_reversed_layout_edges() {
        let visible =
            MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(220.0, 260.0));
        let full_size = MaraVec2::new(120.0, 180.0);

        assert_eq!(
            body_full_rect(visible, full_size, crate::layout::StackDirection::BottomUp,),
            MaraRect::from_min_size(MaraPos2::new(100.0, 80.0), full_size)
        );
        assert_eq!(
            body_full_rect(
                visible,
                full_size,
                crate::layout::StackDirection::RightToLeft,
            ),
            MaraRect::from_min_size(MaraPos2::new(100.0, 200.0), full_size)
        );
        assert_eq!(
            body_full_rect(visible, full_size, crate::layout::StackDirection::TopDown),
            MaraRect::from_min_size(MaraPos2::new(100.0, 200.0), full_size)
        );
    }

    #[test]
    fn folder_tab_strip_rect_offsets_title_facing_end_with_mara_geometry() {
        let used = MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(380.0, 500.0));

        let left_top = folder_tab_strip_rect(used, TitleSide::Left, TitleSide::Top, 26.0, 34.0);
        let top_right = folder_tab_strip_rect(used, TitleSide::Top, TitleSide::Right, 26.0, 34.0);

        assert_eq!(
            left_top,
            MaraRect::from_min_max(MaraPos2::new(74.0, 234.0), MaraPos2::new(100.0, 500.0))
        );
        assert_eq!(
            top_right,
            MaraRect::from_min_max(MaraPos2::new(100.0, 174.0), MaraPos2::new(346.0, 200.0))
        );
    }

    #[test]
    fn top_tab_title_rect_expands_to_frame_edge_with_mara_geometry() {
        let title =
            MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(380.0, 224.0));

        let expanded = top_tab_title_rect(title, 8.0, 5.0);

        assert_eq!(
            expanded,
            MaraRect::from_min_max(MaraPos2::new(92.0, 195.0), MaraPos2::new(388.0, 224.0))
        );
    }

    #[test]
    fn separator_debug_rect_uses_cursor_delta_as_mara_geometry() {
        let before = MaraRect::from_min_max(MaraPos2::new(15.0, 30.0), MaraPos2::new(115.0, 42.0));
        let after = MaraRect::from_min_max(MaraPos2::new(15.0, 48.0), MaraPos2::new(115.0, 60.0));

        let strip = separator_debug_rect(before, after);

        assert_eq!(
            strip,
            MaraRect::from_min_max(MaraPos2::new(15.0, 30.0), MaraPos2::new(115.0, 48.0))
        );
    }

    #[test]
    fn title_banner_rect_uses_mara_geometry_for_open_and_collapsed_states() {
        let used = MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(380.0, 500.0));
        let pad = style::MarginSpec {
            left: 2,
            right: 3,
            top: 4,
            bottom: 5,
        };

        assert_eq!(
            title_banner_rect(used, pad, TitleSide::Top, 20.0, 6.0, false),
            MaraRect::from_min_max(MaraPos2::new(98.0, 196.0), MaraPos2::new(383.0, 505.0))
        );
        assert_eq!(
            title_banner_rect(used, pad, TitleSide::Right, 20.0, 6.0, true),
            MaraRect::from_min_max(MaraPos2::new(354.0, 196.0), MaraPos2::new(383.0, 505.0))
        );
    }

    #[test]
    fn floating_icon_geometry_uses_mara_rects_for_horizontal_title() {
        let strip =
            MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(380.0, 224.0));

        let icon = floating_icon_geometry(
            strip,
            PaneAnchor::TopRail(crate::pane::RailZone::Middle),
            40.0,
            10.0,
            0.0,
        );

        assert_eq!(icon.pos, MaraPos2::new(374.0, 202.0));
        assert_eq!(icon.align, MaraAlign2::RIGHT_TOP);
        assert_eq!(
            icon.rect,
            MaraRect::from_min_size(MaraPos2::new(334.0, 202.0), MaraVec2::new(40.0, 40.0))
        );
    }

    #[test]
    fn floating_icon_geometry_uses_mara_rects_for_reversed_vertical_title() {
        let strip =
            MaraRect::from_min_max(MaraPos2::new(100.0, 200.0), MaraPos2::new(124.0, 500.0));

        let icon = floating_icon_geometry(
            strip,
            PaneAnchor::BottomRail(crate::pane::RailZone::End),
            40.0,
            10.0,
            0.0,
        );

        assert_eq!(icon.pos, MaraPos2::new(122.0, 206.0));
        assert_eq!(icon.align, MaraAlign2::RIGHT_TOP);
        assert_eq!(
            icon.rect,
            MaraRect::from_min_size(MaraPos2::new(82.0, 206.0), MaraVec2::new(40.0, 40.0))
        );
    }

    #[test]
    fn folder_tab_cell_geometry_uses_mara_rects_for_left_projection() {
        let strip = MaraRect::from_min_size(MaraPos2::new(10.0, 20.0), MaraVec2::new(26.0, 90.0));

        let cell = folder_tab_cell_geometry(strip, TitleSide::Left, 1, 30.0, 4.0, 3.0, 6)
            .expect("second cell should fit");

        assert_eq!(
            cell.base,
            MaraRect::from_min_size(MaraPos2::new(10.0, 54.0), MaraVec2::new(26.0, 30.0))
        );
        assert_eq!(
            cell.active,
            MaraRect::from_min_max(MaraPos2::new(10.0, 54.0), MaraPos2::new(39.0, 84.0))
        );
        assert_eq!(cell.corners, MaraCornerRadius::from_corners(6, 0, 6, 0));
    }

    #[test]
    fn folder_tab_cell_geometry_clips_when_cell_overflows_strip() {
        let strip = MaraRect::from_min_size(MaraPos2::new(10.0, 20.0), MaraVec2::new(90.0, 26.0));

        assert!(folder_tab_cell_geometry(strip, TitleSide::Top, 3, 30.0, 4.0, 3.0, 6).is_none());
    }

    #[test]
    fn top_tab_cell_geometry_uses_mara_rects_and_centers() {
        let title = MaraRect::from_min_size(MaraPos2::new(20.0, 30.0), MaraVec2::new(120.0, 50.0));

        let cell = top_tab_cell_geometry(title, 2, 3).expect("third cell should exist");

        assert_eq!(
            cell.rect,
            MaraRect::from_min_size(MaraPos2::new(100.0, 30.0), MaraVec2::new(40.0, 50.0))
        );
        assert_eq!(cell.icon_center, MaraPos2::new(120.0, 46.0));
        assert_eq!(cell.label_center_base, MaraPos2::new(120.0, 67.0));
    }

    #[test]
    fn top_tab_cell_geometry_rejects_empty_or_zero_width_rows() {
        let zero_count =
            MaraRect::from_min_size(MaraPos2::new(20.0, 30.0), MaraVec2::new(120.0, 50.0));
        let zero_width =
            MaraRect::from_min_size(MaraPos2::new(20.0, 30.0), MaraVec2::new(0.0, 50.0));

        assert!(top_tab_cell_geometry(zero_count, 0, 0).is_none());
        assert!(top_tab_cell_geometry(zero_width, 0, 3).is_none());
    }

    #[test]
    fn pro_folder_tabs_use_half_tab_side_outer_padding_not_body_padding() {
        let pro = style::theme_pro(style::Mode::Dark);

        assert_eq!(
            tabbed_strip_outer_inset(pro.tabs, &pro),
            (pro.section_outer_margin_span as f32) * 0.5
        );
        assert_ne!(
            tabbed_strip_outer_inset(pro.tabs, &pro),
            (pro.section_outer_margin_span + pro.section_pad_x) as f32,
            "side shelf tab strips must not include body inner padding in the outer inset"
        );
    }

    #[test]
    fn tab_rect_chrome_backend_lowers_to_mara_paint_commands() {
        let rect = MaraRect::from_min_size(MaraPos2::ZERO, crate::vocab::Vec2::new(24.0, 12.0));

        let commands = tab_rect_chrome_paint_cmds(
            rect,
            MaraCornerRadius::same(3),
            MaraColor32::from_rgb(1, 2, 3),
            Some(MaraStroke::new(1.5, MaraColor32::WHITE)),
        );

        assert!(matches!(commands[0], PaintCmd::RectFilled { .. }));
        assert!(matches!(commands[1], PaintCmd::RectStroke { .. }));
    }

    #[test]
    fn top_tab_label_backend_lowers_to_mara_text_command() {
        let cmd =
            top_tab_label_paint_cmd(MaraPos2::new(5.0, 6.0), "layers", 11.0, MaraColor32::WHITE);

        let PaintCmd::Text {
            text, anchor, mono, ..
        } = cmd
        else {
            panic!("top tab labels should lower to Mara text commands");
        };
        assert_eq!(text, "LAYERS");
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        assert!(!mono);
    }

    #[test]
    fn container_icon_backend_lowers_named_icons_to_mara_text_family() {
        let cmd = icon_name_paint_cmd(
            MaraPos2::new(5.0, 6.0),
            MaraAlign2::CENTER_CENTER,
            "search",
            16.0,
            MaraColor32::WHITE,
        )
        .expect("search icon should be bundled");

        let PaintCmd::TextWithFamily {
            text,
            family,
            anchor,
            ..
        } = cmd
        else {
            panic!("named container icons should lower to Mara named-font text commands");
        };
        assert_eq!(text.chars().count(), 1);
        assert_eq!(anchor, MaraAlign2::CENTER_CENTER);
        let TextFamily::Named(family) = family else {
            panic!("named container icons should keep a named text family");
        };
        assert!(!family.is_empty());
    }

    #[test]
    fn container_svg_icon_backend_lowers_to_mara_svg_command() {
        let svg = "<svg viewBox='0 0 8 8'></svg>";
        let cmd = icon_svg_paint_cmd(
            MaraPos2::new(10.0, 20.0),
            MaraAlign2::CENTER_CENTER,
            svg,
            16.0,
            MaraColor32::WHITE,
        );

        let PaintCmd::Svg {
            svg: retained,
            rect,
            tint,
        } = cmd
        else {
            panic!("svg container icons should lower to Mara SVG paint commands");
        };
        assert_eq!(retained, svg);
        assert_eq!(
            rect,
            MaraRect::from_min_max(MaraPos2::new(2.0, 12.0), MaraPos2::new(18.0, 28.0))
        );
        assert_eq!(tint, MaraColor32::WHITE);
    }

    #[test]
    fn container_chevron_backend_lowers_to_polyline_command() {
        let cmd = chevron_h_paint_cmd(
            MaraPos2::new(10.0, 20.0),
            TitleSide::Top,
            1.0,
            MaraColor32::WHITE,
        );

        let PaintCmd::Polyline { points, stroke } = cmd else {
            panic!("container chevron should lower to a polyline command");
        };
        assert_eq!(points.len(), 3);
        assert_eq!(stroke.width, 1.6);
    }

    #[test]
    fn title_divider_backend_lowers_to_line_command() {
        let rect = MaraRect::from_min_size(
            MaraPos2::new(10.0, 20.0),
            crate::vocab::Vec2::new(100.0, 22.0),
        );

        let top = title_divider_paint_cmd(rect, TitleSide::Top, 4.0, 6.0, MaraColor32::WHITE);
        let PaintCmd::Line { a, b, stroke } = top else {
            panic!("horizontal title dividers should lower to line commands");
        };
        assert_eq!(a, MaraPos2::new(16.0, 46.5));
        assert_eq!(b, MaraPos2::new(104.0, 46.5));
        assert_eq!(stroke.width, 1.0);

        let left = title_divider_paint_cmd(rect, TitleSide::Left, 4.0, 6.0, MaraColor32::WHITE);
        let PaintCmd::Line { a, b, .. } = left else {
            panic!("vertical title dividers should lower to line commands");
        };
        assert_eq!(a, MaraPos2::new(114.5, 26.0));
        assert_eq!(b, MaraPos2::new(114.5, 36.0));
    }

    #[test]
    fn corner_ticks_backend_lower_to_line_commands() {
        let commands = corner_tick_paint_cmds(1.0, 2.0, 21.0, 22.0, 5.0, [MaraColor32::WHITE; 4]);

        assert_eq!(commands.len(), 8);
        assert!(
            commands
                .iter()
                .all(|cmd| matches!(cmd, PaintCmd::Line { .. }))
        );
        let PaintCmd::Line { a, b, stroke } = commands[0] else {
            panic!("corner ticks should lower to line commands");
        };
        assert_eq!(a, MaraPos2::new(1.0, 2.0));
        assert_eq!(b, MaraPos2::new(6.0, 2.0));
        assert_eq!(stroke.width, 2.0);
    }

    #[test]
    fn active_tab_resolution_prefers_stable_tab_id_over_stale_index() {
        let ctx = egui::Context::default();
        let key = Id::new("active-tabs");
        let first = Id::new("first");
        let moved = Id::new("moved");
        let last = Id::new("last");
        {
            let mut memory = crate::memory::MaraMemoryCtx::new(&ctx);
            memory.set_persisted(key, 0usize);
            memory.set_persisted(active_tab_id_key(key), moved);
        };

        let idx = resolve_active_tab_idx(&ctx, key, &[first, moved, last]);

        assert_eq!(idx, 1);
        assert_eq!(
            crate::memory::MaraMemoryCtx::new(&ctx).get_persisted::<usize>(key),
            Some(1)
        );
    }

    #[test]
    fn active_tab_resolution_clamps_index_and_repairs_active_id() {
        let ctx = egui::Context::default();
        let key = Id::new("active-tabs");
        let only = Id::new("only");
        crate::memory::MaraMemoryCtx::new(&ctx).set_persisted(key, 99usize);

        let idx = resolve_active_tab_idx(&ctx, key, &[only]);

        assert_eq!(idx, 0);
        assert_eq!(
            crate::memory::MaraMemoryCtx::new(&ctx).get_persisted::<usize>(key),
            Some(0)
        );
        assert_eq!(
            crate::memory::MaraMemoryCtx::new(&ctx).get_persisted::<Id>(active_tab_id_key(key)),
            Some(only)
        );
    }
}
