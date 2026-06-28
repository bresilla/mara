//! Host-agnostic app shell helpers.
//!
//! The app shell coordinates permanent ribbons, the active top-level
//! view, and the active view's workspace stack. This is deliberately
//! rendering-light for the first implementation: it resolves slot
//! contents and dispatches actions, while host crates decide how to
//! paint the resolved items.

use crate::{
    WorkspaceCtx,
    ribbon::{
        __internal_draw_slot_ribbons_egui, ResolvedSlotRibbon, RibbonAction, RibbonActionError,
        RibbonActionResult, RibbonOverrideLayer, RibbonScope, RibbonSlotDef, RibbonSlotItem,
        dispatch_ribbon_action, resolve_slot_items, restore_workspace_slot_override,
        slot::validate_ribbon_slot_def,
    },
    view::{ViewCtx, ViewRouter, ViewRouterError},
    vocab::{Color32, Id as MaraId},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedRibbon {
    pub id: MaraId,
    pub chrome_id: Option<&'static str>,
    pub scope: RibbonScope,
    pub edge: crate::ribbon::RibbonEdge,
    pub role: crate::ribbon::RibbonRole,
    pub mode: crate::ribbon::RibbonMode,
    pub cluster: crate::ribbon::RibbonCluster,
    pub accepts: &'static [&'static str],
    pub items: Vec<RibbonSlotItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppShellResolution {
    pub ribbons: Vec<ResolvedRibbon>,
}

/// Whether the app shell owns native-window controls.
///
/// Enabled is the default for windowed app UIs. Fullscreen/game-style
/// shells can opt out explicitly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowControlsPolicy {
    #[default]
    Enabled,
    Hidden,
}

/// Whether the app shell owns the persistent app menu button.
///
/// Enabled by default. Apps that own a fully custom top bar can opt
/// out, or views/workspaces can override/hide the inherited slot by
/// targeting [`crate::app_menu_slot_id`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AppMenuPolicy {
    #[default]
    Enabled,
    Hidden,
}

/// API-level app chrome contract.
///
/// Mara has exactly one persistent top main bar. Active views/workspaces
/// may override or explicitly hide individual slots, but they do not
/// rebuild the main bar by passing additional permanent ribbons.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppShellChrome {
    main_bar: RibbonSlotDef,
    app_menu: AppMenuPolicy,
    window_controls: WindowControlsPolicy,
}

impl AppShellChrome {
    #[must_use]
    pub fn new(main_bar: RibbonSlotDef) -> Self {
        assert!(
            matches!(main_bar.scope, RibbonScope::Permanent),
            "AppShellChrome::new requires a permanent main bar"
        );
        assert!(
            matches!(main_bar.edge, crate::ribbon::RibbonEdge::Top),
            "the persistent main bar must be on the top edge"
        );
        assert!(
            main_bar.accepts.is_empty(),
            "the persistent main bar is fixed and cannot accept icon drops"
        );
        Self {
            main_bar,
            app_menu: AppMenuPolicy::Enabled,
            window_controls: WindowControlsPolicy::Enabled,
        }
    }

    #[must_use]
    pub fn main_bar(&self) -> &RibbonSlotDef {
        &self.main_bar
    }

    #[must_use]
    pub fn permanent_ribbons(&self) -> &[RibbonSlotDef] {
        std::slice::from_ref(&self.main_bar)
    }

    #[must_use]
    pub fn window_controls_policy(&self) -> WindowControlsPolicy {
        self.window_controls
    }

    #[must_use]
    pub fn app_menu_policy(&self) -> AppMenuPolicy {
        self.app_menu
    }

    #[must_use]
    pub fn with_app_menu(mut self, policy: AppMenuPolicy) -> Self {
        self.app_menu = policy;
        self
    }

    #[must_use]
    pub fn without_app_menu(self) -> Self {
        self.with_app_menu(AppMenuPolicy::Hidden)
    }

    #[must_use]
    pub fn app_menu_enabled(&self) -> bool {
        self.app_menu == AppMenuPolicy::Enabled
    }

    #[must_use]
    pub fn with_window_controls(mut self, policy: WindowControlsPolicy) -> Self {
        self.window_controls = policy;
        self
    }

    #[must_use]
    pub fn without_window_controls(self) -> Self {
        self.with_window_controls(WindowControlsPolicy::Hidden)
    }

    #[must_use]
    pub fn window_controls_enabled(&self) -> bool {
        self.window_controls == WindowControlsPolicy::Enabled
    }

    #[must_use]
    pub fn permanent_ribbon_defs(&self) -> Vec<RibbonSlotDef> {
        let mut main = self.main_bar.clone();
        if self.app_menu_enabled() && !has_slot(&main.slots, crate::ribbon::app_menu_slot_id()) {
            main.slots
                .insert(0, crate::ribbon::permanent_app_menu_slot());
        }
        if self.window_controls_enabled() {
            let slot_id = crate::ribbon::system_close_or_restore_slot_id();
            if !has_slot(&main.slots, slot_id) {
                main.slots
                    .push(crate::ribbon::permanent_system_control_slot());
            }
        }
        vec![main]
    }
}

fn has_slot(slots: &[crate::ribbon::RibbonSlot], slot_id: crate::ribbon::RibbonSlotId) -> bool {
    slots.iter().any(|slot| slot.id == slot_id)
}

#[derive(Debug)]
pub enum AppShellError {
    View(ViewRouterError),
    Action(RibbonActionError),
    MissingPermanentRibbon,
    MultiplePermanentRibbons {
        count: usize,
    },
    PermanentRibbonNotTop {
        id: MaraId,
        edge: crate::ribbon::RibbonEdge,
    },
    PermanentRibbonMustBeFixed {
        id: MaraId,
    },
    ViewRibbonWrongScope {
        id: MaraId,
    },
    WorkspaceRibbonWrongScope {
        id: MaraId,
    },
}

impl From<ViewRouterError> for AppShellError {
    fn from(value: ViewRouterError) -> Self {
        Self::View(value)
    }
}

impl From<RibbonActionError> for AppShellError {
    fn from(value: RibbonActionError) -> Self {
        Self::Action(value)
    }
}

impl From<ResolvedRibbon> for ResolvedSlotRibbon {
    fn from(value: ResolvedRibbon) -> Self {
        Self {
            id: value.id,
            chrome_id: value.chrome_id,
            scope: value.scope,
            edge: value.edge,
            role: value.role,
            mode: value.mode,
            cluster: value.cluster,
            accepts: value.accepts,
            items: value.items,
        }
    }
}

impl AppShellResolution {
    #[must_use]
    pub fn as_slot_ribbons(&self) -> Vec<ResolvedSlotRibbon> {
        self.ribbons.iter().cloned().map(Into::into).collect()
    }
}

/// Resolve the mandatory top persistent ribbon + active view ribbons
/// into concrete slot items.
///
/// Override priority follows the PLAN:
///
/// ```text
/// deepest workspace > active view > permanent default
/// ```
///
/// For the current implementation, the active workspace contributes
/// a built-in `system.close_or_restore -> PopWorkspace` override
/// whenever the active view stack is at L1+.
pub fn resolve_app_shell_ribbons(
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
) -> Result<AppShellResolution, AppShellError> {
    validate_single_permanent_ribbon(permanent_ribbons)?;
    resolve_app_shell_ribbons_with_workspace_chrome(router, permanent_ribbons, &[], &[])
}

/// Resolve an API-enforced shell chrome contract.
///
/// Prefer this over passing ad-hoc permanent ribbon slices in new
/// hosts: it guarantees a single persistent main bar is always present
/// and first in resolution order.
pub fn resolve_app_shell_chrome(
    router: &mut ViewRouter,
    chrome: &AppShellChrome,
) -> Result<AppShellResolution, AppShellError> {
    resolve_app_shell_chrome_with_workspace(router, chrome, &[], &[])
}

/// Resolve API-enforced shell chrome with active workspace chrome.
pub fn resolve_app_shell_chrome_with_workspace(
    router: &mut ViewRouter,
    chrome: &AppShellChrome,
    workspace_ribbons: &[RibbonSlotDef],
    workspace_layers: &[RibbonOverrideLayer],
) -> Result<AppShellResolution, AppShellError> {
    let permanent_ribbons = chrome.permanent_ribbon_defs();
    resolve_app_shell_ribbons_with_workspace_chrome(
        router,
        &permanent_ribbons,
        workspace_ribbons,
        workspace_layers,
    )
}

/// Like [`resolve_app_shell_ribbons`] but lets the active
/// workspace/module provide additional override layers.
///
/// `workspace_layers` should be ordered shallowest to deepest. They
/// are applied after the active view override layer. When the active
/// workspace is L1+, the built-in restore override is inserted before
/// caller-provided workspace layers, so a module can deliberately
/// replace even that slot if needed.
pub fn resolve_app_shell_ribbons_with_workspace_layers(
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
    workspace_layers: &[RibbonOverrideLayer],
) -> Result<AppShellResolution, AppShellError> {
    resolve_app_shell_ribbons_with_workspace_chrome(
        router,
        permanent_ribbons,
        &[],
        workspace_layers,
    )
}

/// Resolve permanent + view-local + workspace-local ribbons.
///
/// `workspace_ribbons` are supplied by the active L1/L2 module
/// workspace renderer (usually collected through [`crate::WorkspaceCtx`]).
/// They participate only when their [`RibbonScope::WorkspaceLevel`]
/// matches the active workspace id.
pub fn resolve_app_shell_ribbons_with_workspace_chrome(
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
    workspace_ribbons: &[RibbonSlotDef],
    workspace_layers: &[RibbonOverrideLayer],
) -> Result<AppShellResolution, AppShellError> {
    validate_single_permanent_ribbon(permanent_ribbons)?;
    let active_view_id = router.active()?;
    let active_depth = router.active_workspace()?.depth();

    let (view_ribbons, view_overrides) = {
        let entry = router.active_entry_mut()?;
        (entry.view.ribbons(), entry.view.ribbon_overrides())
    };
    permanent_ribbons.iter().for_each(validate_ribbon_slot_def);
    view_ribbons.iter().for_each(validate_ribbon_slot_def);
    workspace_ribbons.iter().for_each(validate_ribbon_slot_def);
    validate_view_ribbons(active_view_id, &view_ribbons)?;
    let active_workspace_id = router.active_workspace()?.current().id;
    validate_workspace_ribbons(active_workspace_id, workspace_ribbons)?;

    let mut layers = Vec::new();
    layers.push(view_overrides);
    if active_depth > 0 {
        layers.push(RibbonOverrideLayer::new(vec![
            restore_workspace_slot_override(),
        ]));
    }
    layers.extend_from_slice(workspace_layers);

    let mut resolved = AppShellResolution::default();
    for ribbon in permanent_ribbons
        .iter()
        .chain(view_ribbons.iter())
        .chain(workspace_ribbons.iter())
        .filter(|ribbon| match ribbon.scope {
            RibbonScope::Permanent => true,
            RibbonScope::View(id) => id == active_view_id,
            RibbonScope::WorkspaceLevel(id) => router
                .active_workspace()
                .map(|workspace| workspace.current().id == id)
                .unwrap_or(false),
        })
    {
        let items = ribbon
            .slots
            .iter()
            .flat_map(|slot| resolve_slot_items(slot, &layers))
            .collect();
        resolved.ribbons.push(ResolvedRibbon {
            id: ribbon.id,
            chrome_id: ribbon.chrome_id,
            scope: ribbon.scope,
            edge: ribbon.edge,
            role: ribbon.role,
            mode: ribbon.mode,
            cluster: ribbon.cluster,
            accepts: ribbon.accepts,
            items,
        });
    }

    Ok(resolved)
}

fn validate_single_permanent_ribbon(ribbons: &[RibbonSlotDef]) -> Result<(), AppShellError> {
    let permanent: Vec<_> = ribbons
        .iter()
        .filter(|ribbon| matches!(ribbon.scope, RibbonScope::Permanent))
        .collect();
    let count = permanent.len();
    if count == 0 {
        return Err(AppShellError::MissingPermanentRibbon);
    }
    if count > 1 {
        return Err(AppShellError::MultiplePermanentRibbons { count });
    }
    let ribbon = permanent[0];
    if !matches!(ribbon.edge, crate::ribbon::RibbonEdge::Top) {
        return Err(AppShellError::PermanentRibbonNotTop {
            id: ribbon.id,
            edge: ribbon.edge,
        });
    }
    if !ribbon.accepts.is_empty() {
        return Err(AppShellError::PermanentRibbonMustBeFixed { id: ribbon.id });
    }
    Ok(())
}

fn validate_view_ribbons(
    active_view: crate::ViewId,
    ribbons: &[RibbonSlotDef],
) -> Result<(), AppShellError> {
    if let Some(ribbon) = ribbons
        .iter()
        .find(|ribbon| !matches!(ribbon.scope, RibbonScope::View(id) if id == active_view))
    {
        return Err(AppShellError::ViewRibbonWrongScope { id: ribbon.id });
    }
    Ok(())
}

fn validate_workspace_ribbons(
    active_workspace: MaraId,
    ribbons: &[RibbonSlotDef],
) -> Result<(), AppShellError> {
    if let Some(ribbon) = ribbons.iter().find(
        |ribbon| !matches!(ribbon.scope, RibbonScope::WorkspaceLevel(id) if id == active_workspace),
    ) {
        return Err(AppShellError::WorkspaceRibbonWrongScope { id: ribbon.id });
    }
    Ok(())
}

/// Dispatch a root-shell ribbon action.
pub fn dispatch_app_shell_action(
    router: &mut ViewRouter,
    action: RibbonAction,
) -> Result<RibbonActionResult, AppShellError> {
    Ok(dispatch_ribbon_action(action, router)?)
}

/// Minimal root render entry point.
///
/// This calls the active L0 view only when the active workspace stack
/// is at root. L1+ module workspace rendering will layer onto this
/// once modules can register workspace renderers.
/// Internal egui render hook for the current app-shell implementation.
///
/// Public app/host code should prefer resolving shell data and routing
/// rendering through a Mara host facade. This remains public only as a
/// hidden first-party adapter while egui is the sole concrete backend.
#[doc(hidden)]
pub fn __internal_show_app_shell_egui(
    egui_ctx: &egui::Context,
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
    accent: impl Into<Color32>,
) -> Result<AppShellResolution, AppShellError> {
    let accent = accent.into();
    let resolved = resolve_app_shell_ribbons(router, permanent_ribbons)?;
    let depth = router.active_workspace()?.depth();
    if depth == 0 {
        let entry = router.active_entry_mut()?;
        let content_avoidance = entry.view.content_avoidance();
        let mut ctx =
            ViewCtx::__internal_new(egui_ctx, &mut entry.workspace, accent, content_avoidance);
        entry.view.show(&mut ctx);
    }
    Ok(resolved)
}

/// Resolve, paint, and dispatch slot-based app-shell ribbons, then
/// render the active L0 view when the active stack is at root.
/// Internal egui render hook for API-enforced shell chrome.
#[doc(hidden)]
pub fn __internal_show_app_shell_chrome_with_slot_ribbons_egui(
    egui_ctx: &egui::Context,
    router: &mut ViewRouter,
    chrome: &AppShellChrome,
    accent: impl Into<Color32>,
) -> Result<(AppShellResolution, Vec<RibbonActionResult>), AppShellError> {
    let accent = accent.into();
    let permanent_ribbons = chrome.permanent_ribbon_defs();
    __internal_show_app_shell_with_slot_ribbons_egui(egui_ctx, router, &permanent_ribbons, accent)
}

/// Internal egui render hook for resolved slot-ribbon shell chrome.
#[doc(hidden)]
pub fn __internal_show_app_shell_with_slot_ribbons_egui(
    egui_ctx: &egui::Context,
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
    accent: impl Into<Color32>,
) -> Result<(AppShellResolution, Vec<RibbonActionResult>), AppShellError> {
    let accent = accent.into();
    let resolved = resolve_app_shell_ribbons(router, permanent_ribbons)?;
    let clicks = __internal_draw_slot_ribbons_egui(egui_ctx, accent, &resolved.as_slot_ribbons());
    let mut results = Vec::with_capacity(clicks.len());
    for click in clicks {
        results.push(dispatch_app_shell_action(router, click.action)?);
    }

    let depth = router.active_workspace()?.depth();
    if depth == 0 {
        let entry = router.active_entry_mut()?;
        let content_avoidance = entry.view.content_avoidance();
        let mut ctx =
            ViewCtx::__internal_new(egui_ctx, &mut entry.workspace, accent, content_avoidance);
        entry.view.show(&mut ctx);
    }

    Ok((resolved, results))
}

/// Resolve, paint, dispatch, and render either the active L0 view or
/// the active L1+ module workspace through a host-supplied renderer.
///
/// The app shell does not own module instances, so the host maps
/// `WorkspaceCtx::level.owner` / module ids to the concrete active
/// module and calls its `workspace`/body renderer inside
/// `render_workspace`. Any ribbons or override layers added to the
/// [`WorkspaceCtx`] are then folded into the slot-resolution pass
/// before painting permanent/view/workspace chrome.
/// Internal egui render hook for shell chrome plus host-supplied
/// workspace rendering.
#[doc(hidden)]
pub fn __internal_show_app_shell_with_workspace_renderer_egui<F>(
    egui_ctx: &egui::Context,
    router: &mut ViewRouter,
    permanent_ribbons: &[RibbonSlotDef],
    accent: impl Into<Color32>,
    render_workspace: F,
) -> Result<(AppShellResolution, Vec<RibbonActionResult>), AppShellError>
where
    F: FnOnce(&egui::Context, &mut WorkspaceCtx<'_>),
{
    let accent = accent.into();
    let depth = router.active_workspace()?.depth();
    let mut workspace_ribbons = Vec::new();
    let mut workspace_layers = Vec::new();

    if depth > 0 {
        let entry = router.active_entry_mut()?;
        let mut workspace_ctx = WorkspaceCtx::new(&mut entry.workspace, accent);
        render_workspace(egui_ctx, &mut workspace_ctx);
        workspace_ribbons.extend_from_slice(workspace_ctx.ribbons());
        workspace_layers.extend_from_slice(workspace_ctx.ribbon_overrides());
    }

    let resolved = resolve_app_shell_ribbons_with_workspace_chrome(
        router,
        permanent_ribbons,
        &workspace_ribbons,
        &workspace_layers,
    )?;
    let clicks = __internal_draw_slot_ribbons_egui(egui_ctx, accent, &resolved.as_slot_ribbons());
    let mut results = Vec::with_capacity(clicks.len());
    for click in clicks {
        results.push(dispatch_app_shell_action(router, click.action)?);
    }

    if depth == 0 {
        let entry = router.active_entry_mut()?;
        let content_avoidance = entry.view.content_avoidance();
        let mut ctx =
            ViewCtx::__internal_new(egui_ctx, &mut entry.workspace, accent, content_avoidance);
        entry.view.show(&mut ctx);
    }

    Ok((resolved, results))
}
