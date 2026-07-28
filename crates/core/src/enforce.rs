//! # Enforced Mara defaults.
//!
//! Mara's contract is that *using the toolkit at all* yields a correct
//! Mara application: themed, with the permanent top bar, on a sane
//! shelf-layout baseline. Host adapters (the native/Android runners,
//! the Bevy plugin) uphold that contract themselves — but a consumer
//! driving `mara::ui` from their own egui host historically could skip
//! all of it. This module closes that hole.
//!
//! Every Mara surface entry point (panes, shelves, ribbons, views, the
//! command palette) calls [`__internal_enforce_defaults`] first. The
//! rule for each default is:
//!
//! > **If the app did it this pass or the previous pass, Mara stays
//! > out of the way. Otherwise Mara does it.**
//!
//! - **Theme** — if no theme was applied, the active Mara theme
//!   (fonts, visuals, responsive metrics) is applied. Opt-out is
//!   simply applying your own theme.
//! - **Shelf-layout baseline** — if nothing published a shelf layout,
//!   a full-viewport no-shelf layout is published so ribbons/panes read
//!   sane geometry. Opt-out is publishing a real layout.
//! - **The permanent top bar** — if the app didn't render a
//!   [`ShellBar`](crate::ShellBar), Mara renders one. The bar is part
//!   of what makes a Mara app a Mara app. Apps that want the functional
//!   bar (view switcher, menu, events) render it themselves through
//!   `ShellBar::show`, which automatically suppresses the fallback.
//!   The **only** escape hatch is the explicit per-frame opt-out
//!   ([`__internal_opt_out_shell`], exposed as
//!   `MaraHostCtx::opt_out_shell_bar`) — a deliberate call the app must
//!   repeat every frame it wants to run bar-less; there is no passive
//!   flag to forget the bar with.
//!
//! The one-pass hysteresis ("this pass or the previous pass") exists
//! because apps commonly render the bar *after* their content each
//! frame (the runners do exactly this); enforcement must not fire
//! mid-pass and duplicate it. The very first pass a context is seen is
//! a grace pass for the same reason — a late-drawn app bar has not had
//! a chance to stamp anything yet.
//!
//! Enforcement is per host context, keyed in its state store, and
//! triggers only from Mara surface draws — a secondary offscreen
//! context (e.g. the node-graph renderer) that never draws Mara chrome
//! is never touched.

use crate::context::MaraCtx;
use crate::ribbon::{RibbonDrag, RibbonOpen, RibbonPlacement};
use crate::shell::ShellBar;
use crate::vocab::Id;

fn key(name: &'static str) -> Id {
    Id::new(("mara.enforce", name))
}

fn app_shell_pass_key() -> Id {
    key("app_shell_pass")
}

#[doc(hidden)]
pub fn app_theme_pass_key() -> Id {
    key("app_theme_pass")
}

fn app_shelf_pass_key() -> Id {
    key("app_shelf_pass")
}

fn enforcing_key() -> Id {
    key("active")
}

fn grace_pass_key() -> Id {
    key("grace_pass")
}

#[doc(hidden)]
pub fn enforced_shell_pass_key() -> Id {
    key("enforced_shell_pass")
}

fn shell_opt_out_pass_key() -> Id {
    key("shell_opt_out_pass")
}

#[doc(hidden)]
pub fn fallback_state_key() -> Id {
    key("fallback_shell_state")
}

/// Persisted state of the Mara-owned fallback bar (config + featureful
/// ribbon chrome state), kept in the egui data store between passes.
#[derive(Clone, Default)]
#[doc(hidden)]
pub struct FallbackShell {
    pub bar: ShellBar,
    pub open: RibbonOpen,
    pub placement: RibbonPlacement,
    pub drag: RibbonDrag,
}

/// `true` while Mara itself is rendering/applying enforced defaults, so
/// the enforced work never counts as "the app did it" and entry points
/// reached from inside enforcement don't recurse.
#[doc(hidden)]
pub fn enforcing(ctx: &dyn MaraCtx) -> bool {
    ctx.memory()
        .get_temp::<bool>(enforcing_key())
        .unwrap_or(false)
}

#[doc(hidden)]
pub fn set_enforcing(ctx: &dyn MaraCtx, on: bool) {
    ctx.memory().set_temp(enforcing_key(), on);
}

fn stamp(ctx: &dyn MaraCtx, key: Id) {
    if enforcing(ctx) {
        return;
    }
    let pass = ctx.pass_nr();
    ctx.memory().set_temp(key, pass);
}

/// Stamp read: `true` when the app performed the action this pass or
/// the previous one (the hysteresis window described in the module
/// docs).
#[doc(hidden)]
pub fn fresh(ctx: &dyn MaraCtx, key: Id) -> bool {
    let pass = ctx.pass_nr();
    ctx.memory()
        .get_temp::<u64>(key)
        .is_some_and(|s| s.saturating_add(1) >= pass)
}

/// The app rendered a [`ShellBar`](crate::ShellBar) itself. Called by
/// `ShellBar::show`; no-op while enforcement renders the fallback bar.
pub(crate) fn mark_app_shell_shown(ctx: &dyn MaraCtx) {
    stamp(ctx, app_shell_pass_key());
}

/// The app applied a theme (via the host facade or the style hook).
#[doc(hidden)]
pub fn mark_app_theme_applied(ctx: &dyn MaraCtx) {
    stamp(ctx, app_theme_pass_key());
}

/// The app (or a real shelf render) published a shelf layout.
pub(crate) fn mark_app_shelf_published(ctx: &dyn MaraCtx) {
    stamp(ctx, app_shelf_pass_key());
}

/// The pass in which the fallback bar last rendered, if it ever did.
/// Diagnostic/test hook — `None` means the app's own bar (or a host
/// runner) has been in charge.
#[doc(hidden)]
#[must_use]
pub fn __internal_shell_enforced_pass(ctx: &dyn MaraCtx) -> Option<u64> {
    ctx.memory().get_temp::<u64>(enforced_shell_pass_key())
}

/// Explicit, deliberate opt-out from the enforced top bar **for the
/// current frame**. Must be called every frame the app wants to run
/// bar-less — it is a repeated decision, not a persistent flag, so the
/// bar can never be forgotten off by accident.
///
/// Host runners also honor this: they skip their own `ShellBar` render
/// for a frame in which the app opted out.
#[doc(hidden)]
pub fn __internal_opt_out_shell(ctx: &dyn MaraCtx) {
    let pass = ctx.pass_nr();
    ctx.memory().set_temp(shell_opt_out_pass_key(), pass);
}

/// `true` when the app opted out of the enforced bar this pass (or the
/// previous pass — the same hysteresis window every stamp gets, since
/// the opt-out call may land after a surface already drew this pass).
#[doc(hidden)]
#[must_use]
pub fn __internal_shell_opted_out(ctx: &dyn MaraCtx) -> bool {
    fresh(ctx, shell_opt_out_pass_key())
}

/// Whether the Mara-owned fallback bar should render this pass.
///
/// The whole enforcement *policy* — stamps, the grace pass, the
/// opt-out — decided without naming a backend. Only the rendering of
/// the fallback needs one, which keeps the rule that decides "did the
/// app do this, or does Mara?" testable and portable.
///
/// Returns the pass number to stamp when it says yes.
#[doc(hidden)]
pub fn fallback_bar_due(ctx: &dyn MaraCtx) -> Option<u64> {
    // Shelf-layout baseline: publish the full-viewport no-shelf layout
    // unless the app published one. Re-published per pass so floating
    // chrome tracks the live window size.
    if !fresh(ctx, app_shelf_pass_key()) {
        set_enforcing(ctx, true);
        crate::shelf::__internal_publish_shelf_layout(
            ctx,
            crate::shelf::ShelfLayout::full(ctx.content_rect()),
        );
        set_enforcing(ctx, false);
    }

    // The permanent top bar. The only way past the fallback is either
    // rendering the bar or the explicit per-frame opt-out.
    let pass = ctx.pass_nr();
    // Record the first pass enforcement ever ran on this context —
    // unconditionally, so early-outs below (app bar fresh, opt-out)
    // never make a later pass masquerade as the first one.
    let first_seen = match ctx.memory().get_temp::<u64>(grace_pass_key()) {
        Some(first) => first,
        None => {
            ctx.memory().set_temp(grace_pass_key(), pass);
            pass
        }
    };
    if fresh(ctx, app_shell_pass_key()) || __internal_shell_opted_out(ctx) {
        return None;
    }
    // First pass this context is ever seen: give the app the rest of
    // the pass to render its own bar (runners and well-behaved apps
    // draw it after content).
    if first_seen == pass {
        return None;
    }
    Some(pass)
}
