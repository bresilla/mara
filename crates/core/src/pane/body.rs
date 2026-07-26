//! Typed pane-body API.
//!
//! A pane is allowed to host *containers* and nothing else. This
//! module enforces that with two pieces:
//!
//! * [`ContainerSpec`] — one container ready to render. Its body
//!   is one of a fixed set of kinds (a pod list, a tab list, or —
//!   `pub(crate)` only — a raw closure used by mara's own extras
//!   to host non-`'static` content like the node graph). External
//!   callers can only build specs through the typed `normal` /
//!   `tabbed` constructors.
//!
//! * [`PaneBody`] — the typed wrapper the internal pane renderer's
//!   closure receives. It collects [`ContainerSpec`]s through
//!   `add_normal` / `add_tabbed` / `add` and hands them off to
//!   [`render_containers`] when the closure returns. No raw egui
//!   [`Ui`] is ever exposed to the user.

use std::collections::{HashMap, HashSet};

use crate::vocab::Id;
use egui::{Color32, Ui};

use crate::container::{Normal, SeparatorOrient, Tab, container_flow, set_container_flow};
use crate::pod::{Pod, PodResponse};
use crate::vocab::Id as MaraId;

use super::{PaneAnchor, TitleSide, active_drag, paint_container_dots, section_order_for};

/// One container, ready to render inside a pane.
///
/// Construct through the typed entry points:
///
/// * [`ContainerSpec::normal`] — single-body container hosting a
///   `Vec<Pod>`.
/// * [`ContainerSpec::tabbed`] — folder-tabbed container hosting a
///   `Vec<Tab>`.
///
/// Anything else (raw egui closures) lives behind `pub(crate)`
/// constructors used only by `mara_core::extras::*` to host
/// host-widget integrations (node graph / code editor) without
/// leaking arbitrary-closure access to consumer code.
pub struct ContainerSpec<'a> {
    id: MaraId,
    title: String,
    icon: &'static str,
    body: SpecBody<'a>,
}

/// Internal body kind for a [`ContainerSpec`]. `Raw` is
/// `pub(crate)` so external callers can never construct a container
/// whose body is an arbitrary egui closure.
pub(crate) enum SpecBody<'a> {
    Pods(Vec<Pod>),
    Tabs(Vec<Tab>),
    Raw(Box<dyn FnOnce(&mut Ui) + 'a>),
}

/// Shared tab payload/routing scope for one logical workspace.
///
/// A normal floating [`Pane`] builds a fresh local scope from only
/// the containers it is about to render. Shelves are different: a
/// tab may be dropped into a container, then that container can move
/// to another Shelf edge/pane. In that case the tab's *payload* is
/// still declared by its original container, but its *owner* is the
/// moved container. Shelves therefore build one scope across all
/// Shelf containers before rendering individual Shelf panes.
pub(crate) struct TabRoutingScope {
    tab_pool: HashMap<MaraId, Tab>,
    tabbed_specs: HashMap<MaraId, (String, &'static str)>,
    declared_tabs_per_container: HashMap<MaraId, Vec<MaraId>>,
    all_tabs_in_scope: Vec<(MaraId, MaraId)>,
    seen_tab_ids: HashSet<MaraId>,
}

impl TabRoutingScope {
    pub(crate) fn new() -> Self {
        Self {
            tab_pool: HashMap::new(),
            tabbed_specs: HashMap::new(),
            declared_tabs_per_container: HashMap::new(),
            all_tabs_in_scope: Vec::new(),
            seen_tab_ids: HashSet::new(),
        }
    }

    pub(crate) fn absorb_spec(&mut self, spec: &mut ContainerSpec<'_>) {
        let SpecBody::Tabs(tabs) = &mut spec.body else {
            return;
        };
        self.tabbed_specs
            .insert(spec.id, (spec.title.clone(), spec.icon));
        let mut ids = Vec::with_capacity(tabs.len());
        for tab in std::mem::take(tabs) {
            let tid = tab.id();
            assert!(
                self.seen_tab_ids.insert(tid),
                "tabbed containers in one tab routing scope require globally unique tab ids"
            );
            ids.push(tid);
            self.all_tabs_in_scope.push((tid, spec.id));
            self.tab_pool.insert(tid, tab);
        }
        self.declared_tabs_per_container.insert(spec.id, ids);
    }

    pub(crate) fn absorb_specs<'a>(&mut self, specs: &mut [ContainerSpec<'a>]) {
        for spec in specs {
            self.absorb_spec(spec);
        }
    }

    fn is_tabbed_container(&self, container_id: MaraId) -> bool {
        self.tabbed_specs.contains_key(&container_id)
    }
}

impl<'a> ContainerSpec<'a> {
    /// Single-body container hosting a pod list.
    #[must_use]
    pub fn normal(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        pods: Vec<Pod>,
    ) -> Self {
        let title = title.into();
        assert_container_title(&title);
        assert_container_icon(icon);
        Self {
            id: id.into().into(),
            title,
            icon,
            body: SpecBody::Pods(pods),
        }
    }

    /// Folder-tabbed container hosting a tab list.
    #[must_use]
    pub fn tabbed(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        tabs: Vec<Tab>,
    ) -> Self {
        let title = title.into();
        assert_container_title(&title);
        assert_container_icon(icon);
        assert!(
            !tabs.is_empty(),
            "tabbed containers require at least one tab"
        );
        let mut seen = HashSet::with_capacity(tabs.len());
        assert!(
            tabs.iter().all(|tab| seen.insert(tab.egui_id())),
            "tabbed containers require unique tab ids"
        );
        Self {
            id: id.into().into(),
            title,
            icon,
            body: SpecBody::Tabs(tabs),
        }
    }

    /// First-party raw-closure constructor. Used by `mara::extras::*`
    /// to wrap host-widget integrations (node graph, code editor) that
    /// need non-`'static` borrows. Doc-hidden; not a stable API.
    #[doc(hidden)]
    #[must_use]
    pub fn raw_internal<F>(
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        body: F,
    ) -> Self
    where
        F: FnOnce(&mut Ui) + 'a,
    {
        let title = title.into();
        assert_container_title(&title);
        assert_container_icon(icon);
        Self {
            id: id.into(),
            title,
            icon,
            body: SpecBody::Raw(Box::new(body)),
        }
    }

    /// The stable container id (used by reorder persistence + the
    /// pod response map).
    #[must_use]
    pub fn container_id(&self) -> MaraId {
        self.id
    }

    pub(crate) fn egui_container_id(&self) -> Id {
        self.id.into()
    }
}

fn assert_container_title(title: &str) {
    assert!(
        !title.trim().is_empty(),
        "pane containers require a non-empty title"
    );
}

fn assert_container_icon(icon: &'static str) {
    assert!(
        !icon.trim().is_empty(),
        "pane containers require a non-empty icon"
    );
}

/// Typed wrapper around a pane's body Ui. Only exposes operations
/// that add containers — there is no way to get at the inner
/// [`egui::Ui`] from outside `mara_core`, so the closure body
/// passed to the internal pane renderer cannot paint raw egui widgets.
///
/// Imperative builder: call [`add_normal`](Self::add_normal),
/// [`add_tabbed`](Self::add_tabbed), or the generic
/// [`add`](Self::add) any number of times. Containers paint in the
/// order returned by [`section_order_for`] (so the user's
/// drag-reorder persists across frames), not in call order.
pub struct PaneBody<'ui, 'spec> {
    ui: &'ui mut Ui,
    pane_id: Id,
    anchor: PaneAnchor,
    accent: Color32,
    pending: Vec<ContainerSpec<'spec>>,
}

impl<'ui, 'spec> PaneBody<'ui, 'spec> {
    pub(crate) fn new(ui: &'ui mut Ui, pane_id: Id, anchor: PaneAnchor, accent: Color32) -> Self {
        Self {
            ui,
            pane_id,
            anchor,
            accent,
            pending: Vec::new(),
        }
    }

    /// The anchor of the pane this body is in.
    #[must_use]
    pub fn anchor(&self) -> PaneAnchor {
        self.anchor
    }

    /// The accent colour the pane was built with.
    #[must_use]
    pub fn accent(&self) -> crate::vocab::Color32 {
        self.accent.into()
    }

    /// The pane's stable id.
    #[must_use]
    pub fn pane_id(&self) -> MaraId {
        self.pane_id.into()
    }

    /// Current text of the `search_idx`-th search widget inside the
    /// pod `pod_id`. Sealed equivalent of
    /// [`Pod::search_query`](crate::pod::Pod::search_query), for
    /// pane bodies that filter their own content by a search pod.
    #[must_use]
    pub fn search_query(&self, pod_id: impl Into<MaraId>, search_idx: usize) -> String {
        let pod_id: Id = pod_id.into().into();
        crate::pod::Pod::search_query(self.ui.ctx(), pod_id, search_idx)
    }

    /// Read a frame-temporary `String` (e.g. a selection path a
    /// tree widget published) keyed by `id`.
    #[must_use]
    pub fn temp_string(&self, id: impl Into<MaraId>) -> Option<String> {
        let id: Id = id.into().into();
        crate::memory::MaraMemoryCtx::new(self.ui.ctx()).get_temp::<String>(id)
    }

    /// Append a normal container (single body, pod list).
    pub fn add_normal(
        &mut self,
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        pods: Vec<Pod>,
    ) -> &mut Self {
        self.pending
            .push(ContainerSpec::normal(id, title, icon, pods));
        self
    }

    /// Append a folder-tabbed container.
    pub fn add_tabbed(
        &mut self,
        id: impl Into<MaraId>,
        title: impl Into<String>,
        icon: &'static str,
        tabs: Vec<Tab>,
    ) -> &mut Self {
        self.pending
            .push(ContainerSpec::tabbed(id, title, icon, tabs));
        self
    }

    /// Append a pre-built [`ContainerSpec`]. Useful when an
    /// extension (e.g. `mara_core::extras::graph`) provides a
    /// typed constructor that returns a `ContainerSpec` for you
    /// to forward in.
    pub fn add(&mut self, spec: ContainerSpec<'spec>) -> &mut Self {
        self.pending.push(spec);
        self
    }

    /// Paint every container queued so far and return the per-
    /// container pod-response map. Useful when a pane body needs
    /// to wire `PodResponse` changes back into Bevy `Resource`s
    /// or eframe app state **inside the same closure** (e.g. a
    /// theme picker that updates an `AccentColor` resource from
    /// the colour pod). After the call, the queue is empty and
    /// further `add_*` calls accumulate again. Internal pane rendering
    /// invokes `render` (via the crate-internal `finish`) once
    /// after the closure returns, so an unconsumed queue is
    /// painted automatically.
    pub fn render(&mut self) -> HashMap<MaraId, Vec<PodResponse>> {
        self.render_raw()
            .into_iter()
            .map(|(id, responses)| (id.into(), responses))
            .collect()
    }

    fn render_raw(&mut self) -> HashMap<Id, Vec<PodResponse>> {
        let specs = std::mem::take(&mut self.pending);
        render_containers(self.ui, self.pane_id, self.anchor, self.accent, specs)
    }

    /// Crate-internal: drain any remaining containers and return
    /// their pod-response maps. Called by internal pane rendering once the
    /// user's body closure returns.
    pub(crate) fn finish(mut self) -> HashMap<Id, Vec<PodResponse>> {
        self.render_raw()
    }
}

/// Render a stack of containers inside a pane body — same layout
/// the demo's old `render_containers` performed, now owned by
/// `mara_core` so every consumer gets identical behaviour:
///
/// * Containers paint in the order from [`section_order_for`] so
///   drag-reorder persists.
/// * Between containers (and after the last), [`paint_container_dots`]
///   paints the three-dot drag handle.
/// * Dragging a handle updates the persisted container flow via
///   [`set_container_flow`]; folded containers ignore drag so the
///   user can't silently grow / shrink an invisible region.
pub(crate) fn render_containers<'a>(
    body_ui: &mut Ui,
    pane_id: Id,
    anchor: PaneAnchor,
    accent: Color32,
    mut containers: Vec<ContainerSpec<'a>>,
) -> HashMap<Id, Vec<PodResponse>> {
    let mut tab_scope = TabRoutingScope::new();
    tab_scope.absorb_specs(&mut containers);
    render_containers_with_tab_scope(
        body_ui,
        pane_id,
        pane_id,
        anchor,
        accent,
        containers,
        &mut tab_scope,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_containers_with_tab_scope<'a>(
    body_ui: &mut Ui,
    pane_id: Id,
    tab_routing_id: Id,
    anchor: PaneAnchor,
    accent: Color32,
    containers: Vec<ContainerSpec<'a>>,
    tab_scope: &mut TabRoutingScope,
    tabbed_strip_side: Option<TitleSide>,
) -> HashMap<Id, Vec<PodResponse>> {
    let mut seen_container_ids: HashSet<MaraId> = HashSet::with_capacity(containers.len());
    assert!(
        containers
            .iter()
            .all(|container| seen_container_ids.insert(container.id)),
        "pane containers require unique container ids"
    );
    // The reorder store is still keyed by backend ids (WS-E4 step 5a),
    // and `MaraId -> backend Id` is a re-hash, so the trip back is not
    // identity. Keep the mapping rather than converting twice — a
    // container that renders but cannot be found by its own id is the
    // failure this avoids.
    // Declaration order is the fallback ordering, so `defaults` must
    // keep it — a map's iteration order would shuffle the containers.
    let defaults: Vec<Id> = containers.iter().map(|c| c.id.into()).collect();
    let backend_to_mara: HashMap<Id, MaraId> =
        containers.iter().map(|c| (c.id.into(), c.id)).collect();
    let order = section_order_for(body_ui.ctx(), pane_id, &defaults);
    let mut by_id: HashMap<MaraId, ContainerSpec<'a>> =
        containers.into_iter().map(|c| (c.id, c)).collect();

    let containers_stack_horizontally = !anchor.title_side().is_horizontal_strip();
    let dots_orient = if containers_stack_horizontally {
        SeparatorOrient::Vertical
    } else {
        SeparatorOrient::Horizontal
    };
    let title_at_end = anchor.title_side().is_at_end();
    let pane_horizontal_strip = anchor.title_side().is_horizontal_strip();

    let mut responses: HashMap<Id, Vec<PodResponse>> = HashMap::new();
    for cid in order.into_iter() {
        let Some(&cid_mara) = backend_to_mara.get(&cid) else {
            continue;
        };
        // Tabbed containers — pull routed tabs from the pool.
        if tab_scope.is_tabbed_container(cid_mara) {
            let Some((title, icon)) = tab_scope.tabbed_specs.get(&cid_mara).cloned() else {
                continue;
            };
            let _ = by_id.remove(&cid_mara);
            let empty: Vec<MaraId> = Vec::new();
            let defaults_here = tab_scope
                .declared_tabs_per_container
                .get(&cid_mara)
                .unwrap_or(&empty);
            let routed_ids = super::tab_drag::route(
                body_ui.ctx(),
                tab_routing_id.into(),
                cid_mara,
                defaults_here,
                &tab_scope.all_tabs_in_scope,
            );
            let mut routed_tabs: Vec<crate::container::Tab> = Vec::with_capacity(routed_ids.len());
            for tid in &routed_ids {
                if let Some(tab) = tab_scope.tab_pool.remove(tid) {
                    routed_tabs.push(tab);
                }
            }
            if routed_tabs.is_empty() {
                // No tabs land in this container after routing —
                // skip render entirely so an empty strip doesn't
                // paint a phantom container.
                continue;
            }
            let mut normal = Normal::new(title.as_str(), anchor, accent, cid).icon(icon);
            if let Some(side) = tabbed_strip_side {
                normal = normal.tabbed_strip_side(side);
            }
            let resp = normal.show_tabs(body_ui, routed_tabs);
            responses.insert(cid, resp);
            let dragging_self = active_drag(body_ui.ctx())
                .and_then(|(_, s)| s.item)
                .map(|item| item == cid)
                .unwrap_or(false);
            if dragging_self {
                continue;
            }
            let dot_resp = paint_container_dots(body_ui, dots_orient, cid, accent);
            let body_open: bool = crate::memory::MaraMemoryCtx::new(body_ui.ctx())
                .get_persisted::<bool>(cid.with("body_open"))
                .unwrap_or(true);
            if dot_resp.dragged() && body_open {
                let cur = container_flow(body_ui.ctx(), cid, pane_horizontal_strip);
                let raw = if containers_stack_horizontally {
                    dot_resp.drag_delta.x
                } else {
                    dot_resp.drag_delta.y
                };
                let delta = if title_at_end { -raw } else { raw };
                set_container_flow(body_ui.ctx(), cid, cur + delta, pane_horizontal_strip);
            }
            continue;
        }

        let Some(spec) = by_id.remove(&cid_mara) else {
            continue;
        };
        let normal = Normal::new(spec.title.as_str(), anchor, accent, cid).icon(spec.icon);
        let resp = match spec.body {
            SpecBody::Pods(pods) => normal.show(body_ui, pods),
            SpecBody::Tabs(_tabs) => {
                // Tabs are handled by the tab-pool branch above; this
                // arm is unreachable because the pool drained every
                // `SpecBody::Tabs`. Keep the match exhaustive.
                Vec::new()
            }
            SpecBody::Raw(body) => {
                normal.show_raw(body_ui, body);
                // Raw bodies don't produce pod responses — return
                // an empty Vec so the response map stays consistent.
                Vec::new()
            }
        };
        responses.insert(cid, resp);

        // Skip the dot handle while THIS container is being
        // drag-reordered — the floating preview already paints a
        // copy with its handle.
        let dragging_self = active_drag(body_ui.ctx())
            .and_then(|(_, s)| s.item)
            .map(|item| item == cid)
            .unwrap_or(false);
        if dragging_self {
            continue;
        }

        let dot_resp = paint_container_dots(body_ui, dots_orient, cid, accent);
        let body_open: bool = crate::memory::MaraMemoryCtx::new(body_ui.ctx())
            .get_persisted::<bool>(cid.with("body_open"))
            .unwrap_or(true);
        if dot_resp.dragged() && body_open {
            let cur = container_flow(body_ui.ctx(), cid, pane_horizontal_strip);
            let raw = if containers_stack_horizontally {
                dot_resp.drag_delta.x
            } else {
                dot_resp.drag_delta.y
            };
            let delta = if title_at_end { -raw } else { raw };
            set_container_flow(body_ui.ctx(), cid, cur + delta, pane_horizontal_strip);
        }
    }
    super::tab_drag::retain_containers(
        body_ui.ctx(),
        pane_id.into(),
        responses.keys().map(|id| MaraId::from(*id)),
    );
    responses
}

#[cfg(test)]
mod tests {
    #![allow(deprecated)]

    use super::*;
    use crate::pane::{RailZone, active_pane_key, tab_drag};

    #[test]
    fn tabbed_container_requires_at_least_one_tab() {
        let result = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed("empty", "Empty", "settings", Vec::new());
        });

        assert!(result.is_err());
    }

    #[test]
    fn containers_require_non_empty_icons() {
        let normal = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::normal("no-icon", "No Icon", "  ", Vec::new());
        });
        let tabbed = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs-no-icon",
                "Tabs",
                "",
                vec![Tab::new("main", "Main", "settings")],
            );
        });
        let raw = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::raw_internal("raw-no-icon", "Raw", "", |_| {});
        });

        assert!(normal.is_err());
        assert!(tabbed.is_err());
        assert!(raw.is_err());
    }

    #[test]
    fn containers_require_non_empty_titles() {
        let normal = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::normal("no-title", " ", "settings", Vec::new());
        });
        let tabbed = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs-no-title",
                "",
                "settings",
                vec![Tab::new("main", "Main", "settings")],
            );
        });
        let raw = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::raw_internal("raw-no-title", " ", "settings", |_| {});
        });

        assert!(normal.is_err());
        assert!(tabbed.is_err());
        assert!(raw.is_err());
    }

    #[test]
    fn tabbed_container_accepts_tabs_with_icons() {
        let spec = ContainerSpec::tabbed(
            "tabs",
            "Tabs",
            "settings",
            vec![Tab::new("main", "Main", "settings")],
        );

        assert_eq!(spec.container_id(), Id::new("tabs"));
    }

    #[test]
    fn tabbed_container_rejects_duplicate_tab_ids() {
        let result = std::panic::catch_unwind(|| {
            let _ = ContainerSpec::tabbed(
                "tabs",
                "Tabs",
                "settings",
                vec![
                    Tab::new("duplicate", "First", "settings"),
                    Tab::new("duplicate", "Second", "info"),
                ],
            );
        });

        assert!(result.is_err());
    }

    #[test]
    fn pane_rejects_duplicate_tab_ids_across_containers() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            egui::CentralPanel::default().show(&ctx, |ui| {
                crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(active_pane_key(), pane_id);
                let _ = render_containers(
                    ui,
                    pane_id,
                    PaneAnchor::LeftRail(RailZone::Middle),
                    Color32::from_rgb(120, 160, 220),
                    vec![
                        ContainerSpec::tabbed(
                            "first",
                            "First",
                            "settings",
                            vec![Tab::new("shared-tab", "Shared A", "settings")],
                        ),
                        ContainerSpec::tabbed(
                            "second",
                            "Second",
                            "info",
                            vec![Tab::new("shared-tab", "Shared B", "info")],
                        ),
                    ],
                );
            });
        }));
        let _ = ctx.end_pass();

        assert!(
            result.is_err(),
            "tab ids route per pane, so two containers in one pane must not reuse the same tab id"
        );
    }

    #[test]
    fn pane_rejects_duplicate_container_ids() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            egui::CentralPanel::default().show(&ctx, |ui| {
                crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(active_pane_key(), pane_id);
                let _ = render_containers(
                    ui,
                    pane_id,
                    PaneAnchor::LeftRail(RailZone::Middle),
                    Color32::from_rgb(120, 160, 220),
                    vec![
                        ContainerSpec::normal("duplicate", "First", "settings", Vec::new()),
                        ContainerSpec::normal("duplicate", "Second", "info", Vec::new()),
                    ],
                );
            });
        }));
        let _ = ctx.end_pass();

        assert!(
            result.is_err(),
            "duplicate container ids would silently overwrite routing/render state"
        );
    }

    #[test]
    fn single_tabbed_container_still_registers_tab_strip() {
        let ctx = egui::Context::default();
        let pane_id = Id::new("pane");
        let container_id = Id::new("single-tab-container");
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        egui::CentralPanel::default().show(&ctx, |ui| {
            crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(active_pane_key(), pane_id);
            let responses = render_containers(
                ui,
                pane_id,
                PaneAnchor::LeftRail(RailZone::Middle),
                Color32::from_rgb(120, 160, 220),
                vec![ContainerSpec::tabbed(
                    container_id,
                    "One Tab",
                    "settings",
                    vec![Tab::new("only", "Only", "settings")],
                )],
            );
            assert!(responses.contains_key(&container_id));
            let strips = tab_drag::strip_cache(ui.ctx(), pane_id.into());
            let buttons = tab_drag::button_cache(ui.ctx(), pane_id.into());
            assert_eq!(
                strips
                    .iter()
                    .filter(|strip| strip.container_id == container_id)
                    .count(),
                1,
                "single-tab tabbed containers must still paint/register their tab strip"
            );
            assert_eq!(
                buttons
                    .iter()
                    .filter(|button| button.container_id == container_id)
                    .count(),
                1,
                "single-tab tabbed containers must still expose one tab button"
            );
        });
        let _ = ctx.end_pass();
    }

    #[test]
    fn shared_tab_scope_renders_moved_tab_with_container_after_pane_change() {
        let ctx = egui::Context::default();
        let routing_id = Id::new("shelf-tab-routing");
        let target_pane = Id::new("target-shelf-pane");
        let source = Id::new("source-container");
        let target = Id::new("target-container");
        let moved_tab = Id::new("moved-tab");
        let source_stay = Id::new("source-stay");
        let target_own = Id::new("target-own");

        let mut source_specs = vec![ContainerSpec::tabbed(
            source,
            "Source",
            "box",
            vec![
                Tab::new(moved_tab, "Moved", "settings"),
                Tab::new(source_stay, "Stay", "info"),
            ],
        )];
        let mut target_specs = vec![ContainerSpec::tabbed(
            target,
            "Target",
            "box",
            vec![Tab::new(target_own, "Own", "settings")],
        )];
        let mut scope = TabRoutingScope::new();
        scope.absorb_specs(&mut source_specs);
        scope.absorb_specs(&mut target_specs);

        tab_drag::commit_drop(
            &ctx,
            routing_id.into(),
            moved_tab.into(),
            source.into(),
            target.into(),
            0,
        );
        assert_eq!(
            tab_drag::route(
                &ctx,
                routing_id.into(),
                target.into(),
                scope
                    .declared_tabs_per_container
                    .get(&MaraId::from(target))
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
                &scope.all_tabs_in_scope,
            ),
            vec![moved_tab, target_own],
            "shared routing scope should keep moved tabs attached to their new owner before rendering"
        );

        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            ..Default::default()
        });
        egui::CentralPanel::default().show(&ctx, |ui| {
            crate::memory::MaraMemoryCtx::new(ui.ctx()).set_temp(active_pane_key(), target_pane);
            let responses = render_containers_with_tab_scope(
                ui,
                target_pane,
                routing_id,
                PaneAnchor::LeftRail(RailZone::Middle),
                Color32::from_rgb(120, 160, 220),
                target_specs,
                &mut scope,
                None,
            );

            assert!(responses.contains_key(&target));
            let mut target_buttons: Vec<MaraId> =
                tab_drag::button_cache(ui.ctx(), target_pane.into())
                .into_iter()
                .filter(|button| button.container_id == MaraId::from(target))
                .map(|button| button.tab_id)
                .collect();
            target_buttons.sort_by_key(|id| format!("{id:?}"));
            assert_eq!(
                target_buttons,
                {
                    let mut expected = vec![moved_tab, target_own];
                    expected.sort_by_key(|id| format!("{id:?}"));
                    expected
                },
                "a tab dropped into a container must render with that container after the container moves to a different Shelf pane"
            );
        });
        let _ = ctx.end_pass();
    }
}
