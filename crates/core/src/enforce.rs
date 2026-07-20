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
//! Enforcement is per [`egui::Context`], keyed in its data store, and
//! triggers only from Mara surface draws — a secondary offscreen
//! context (e.g. the node-graph renderer) that never draws Mara chrome
//! is never touched.

use crate::ribbon::{RibbonDrag, RibbonOpen, RibbonPlacement};
use crate::shell::ShellBar;

fn key(name: &'static str) -> egui::Id {
    egui::Id::new(("mara.enforce", name))
}

fn app_shell_pass_key() -> egui::Id {
    key("app_shell_pass")
}

fn app_theme_pass_key() -> egui::Id {
    key("app_theme_pass")
}

fn app_shelf_pass_key() -> egui::Id {
    key("app_shelf_pass")
}

fn enforcing_key() -> egui::Id {
    key("active")
}

fn grace_pass_key() -> egui::Id {
    key("grace_pass")
}

fn enforced_shell_pass_key() -> egui::Id {
    key("enforced_shell_pass")
}

fn shell_opt_out_pass_key() -> egui::Id {
    key("shell_opt_out_pass")
}

fn fallback_state_key() -> egui::Id {
    key("fallback_shell_state")
}

/// Persisted state of the Mara-owned fallback bar (config + featureful
/// ribbon chrome state), kept in the egui data store between passes.
#[derive(Clone, Default)]
struct FallbackShell {
    bar: ShellBar,
    open: RibbonOpen,
    placement: RibbonPlacement,
    drag: RibbonDrag,
}

/// `true` while Mara itself is rendering/applying enforced defaults, so
/// the enforced work never counts as "the app did it" and entry points
/// reached from inside enforcement don't recurse.
pub(crate) fn enforcing(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(enforcing_key()))
        .unwrap_or(false)
}

fn set_enforcing(ctx: &egui::Context, on: bool) {
    ctx.data_mut(|d| d.insert_temp(enforcing_key(), on));
}

fn stamp(ctx: &egui::Context, key: egui::Id) {
    if enforcing(ctx) {
        return;
    }
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(key, pass));
}

/// Stamp read: `true` when the app performed the action this pass or
/// the previous one (the hysteresis window described in the module
/// docs).
fn fresh(ctx: &egui::Context, key: egui::Id) -> bool {
    let pass = ctx.cumulative_pass_nr();
    ctx.data(|d| d.get_temp::<u64>(key))
        .is_some_and(|s| s.saturating_add(1) >= pass)
}

/// The app rendered a [`ShellBar`](crate::ShellBar) itself. Called by
/// `ShellBar::show`; no-op while enforcement renders the fallback bar.
pub(crate) fn mark_app_shell_shown(ctx: &egui::Context) {
    stamp(ctx, app_shell_pass_key());
}

/// The app applied a theme (via the host facade or the style hook).
pub(crate) fn mark_app_theme_applied(ctx: &egui::Context) {
    stamp(ctx, app_theme_pass_key());
}

/// The app (or a real shelf render) published a shelf layout.
pub(crate) fn mark_app_shelf_published(ctx: &egui::Context) {
    stamp(ctx, app_shelf_pass_key());
}

/// The pass in which the fallback bar last rendered, if it ever did.
/// Diagnostic/test hook — `None` means the app's own bar (or a host
/// runner) has been in charge.
#[doc(hidden)]
#[must_use]
pub fn __internal_shell_enforced_pass(ctx: &egui::Context) -> Option<u64> {
    ctx.data(|d| d.get_temp::<u64>(enforced_shell_pass_key()))
}

/// Explicit, deliberate opt-out from the enforced top bar **for the
/// current frame**. Must be called every frame the app wants to run
/// bar-less — it is a repeated decision, not a persistent flag, so the
/// bar can never be forgotten off by accident.
///
/// Host runners also honor this: they skip their own `ShellBar` render
/// for a frame in which the app opted out.
#[doc(hidden)]
pub fn __internal_opt_out_shell(ctx: &egui::Context) {
    let pass = ctx.cumulative_pass_nr();
    ctx.data_mut(|d| d.insert_temp(shell_opt_out_pass_key(), pass));
}

/// `true` when the app opted out of the enforced bar this pass (or the
/// previous pass — the same hysteresis window every stamp gets, since
/// the opt-out call may land after a surface already drew this pass).
#[doc(hidden)]
#[must_use]
pub fn __internal_shell_opted_out(ctx: &egui::Context) -> bool {
    fresh(ctx, shell_opt_out_pass_key())
}

/// Enforce Mara's defaults for this pass. Called by every Mara surface
/// entry point; cheap after the first call of a pass (stamp reads).
#[doc(hidden)]
pub fn __internal_enforce_defaults(ctx: &egui::Context) {
    if enforcing(ctx) {
        return;
    }

    // Theme: apply the active Mara theme unless the app applied one.
    // `__internal_apply_theme` de-dupes internally, so re-applying every
    // pass for theme-less apps is cheap and tracks resizes.
    if !fresh(ctx, app_theme_pass_key()) {
        set_enforcing(ctx, true);
        crate::style::__internal_apply_theme(
            ctx,
            crate::style::AccentColor(crate::style::raw_accent()),
            crate::style::glass_opacity(),
        );
        set_enforcing(ctx, false);
    }

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
    let pass = ctx.cumulative_pass_nr();
    // Record the first pass enforcement ever ran on this context —
    // unconditionally, so early-outs below (app bar fresh, opt-out)
    // never make a later pass masquerade as the first one.
    let first_seen = match ctx.data(|d| d.get_temp::<u64>(grace_pass_key())) {
        Some(first) => first,
        None => {
            ctx.data_mut(|d| d.insert_temp(grace_pass_key(), pass));
            pass
        }
    };
    if fresh(ctx, app_shell_pass_key()) || __internal_shell_opted_out(ctx) {
        return;
    }
    // First pass this context is ever seen: give the app the rest of
    // the pass to render its own bar (runners and well-behaved apps
    // draw it after content).
    if first_seen == pass {
        return;
    }

    // The app has had its chance — render the Mara-owned fallback bar.
    // The default `ShellBar` (app-menu + injected host controls, no
    // views) — the same bar a bare runner app gets. An items-less bar
    // would paint nothing, which is no enforcement at all. Events are
    // dropped — there is no app wired to receive them; apps that want
    // the functional bar render `ShellBar` themselves, which
    // suppresses this fallback.
    set_enforcing(ctx, true);
    let mut state = ctx
        .data(|d| d.get_temp::<FallbackShell>(fallback_state_key()))
        .unwrap_or_default();
    let _ = state
        .bar
        .show(ctx, &mut state.open, &mut state.placement, &mut state.drag);
    ctx.data_mut(|d| {
        d.insert_temp(fallback_state_key(), state);
        d.insert_temp(enforced_shell_pass_key(), pass);
    });
    set_enforcing(ctx, false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run_pass(ctx: &egui::Context, f: impl FnOnce()) -> egui::FullOutput {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 800.0),
            )),
            ..Default::default()
        };
        ctx.begin_pass(input);
        f();
        ctx.end_pass()
    }

    /// A consumer that draws Mara surfaces but never renders the bar
    /// gets the enforced fallback bar from the second pass onward
    /// (pass one is the grace pass).
    #[test]
    fn fallback_bar_kicks_in_after_grace_pass() {
        let ctx = egui::Context::default();

        let out1 = run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "grace pass must not draw the fallback bar"
        );

        let _ = run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_some(),
            "second pass without an app bar must enforce the fallback"
        );

        // And it keeps rendering every subsequent pass (the stamp
        // advances pass over pass). egui areas are invisible on their
        // first frame (sizing pass), so paint is asserted on this
        // settled pass, not the pass the fallback first fired.
        let after_second = __internal_shell_enforced_pass(&ctx).expect("stamped above");
        let out3 = run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        let after_third = __internal_shell_enforced_pass(&ctx).expect("still enforced");
        assert!(
            after_third > after_second,
            "fallback must re-render every pass without an app bar"
        );
        assert!(
            out3.shapes.len() > out1.shapes.len(),
            "the enforced bar must actually paint something"
        );
    }

    /// An app that renders its own `ShellBar` each pass never triggers
    /// the fallback — even though enforcement runs every pass too.
    #[test]
    fn app_bar_suppresses_fallback() {
        let ctx = egui::Context::default();
        let mut bar = ShellBar::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();

        for _ in 0..4 {
            run_pass(&ctx, || {
                // Content first, bar last — the common host pattern.
                __internal_enforce_defaults(&ctx);
                let _ = bar.show(&ctx, &mut open, &mut placement, &mut drag);
            });
        }
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "fallback must never fire while the app renders the bar"
        );
    }

    /// The explicit per-frame opt-out suppresses the fallback — but
    /// only for frames it is repeated in; going silent brings the bar
    /// back.
    #[test]
    fn explicit_opt_out_suppresses_fallback_per_frame() {
        let ctx = egui::Context::default();

        for _ in 0..3 {
            run_pass(&ctx, || {
                __internal_opt_out_shell(&ctx);
                __internal_enforce_defaults(&ctx);
            });
        }
        assert!(
            __internal_shell_enforced_pass(&ctx).is_none(),
            "opted-out frames must not draw the fallback bar"
        );

        // Opt-out stops being called → hysteresis covers one pass,
        // then the enforced bar returns.
        run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_none());
        run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(
            __internal_shell_enforced_pass(&ctx).is_some(),
            "the bar must come back once the opt-out is no longer repeated"
        );
    }

    /// An app that stops rendering its bar loses the argument: the
    /// fallback takes over after the hysteresis window.
    #[test]
    fn fallback_takes_over_when_app_bar_stops() {
        let ctx = egui::Context::default();
        let mut bar = ShellBar::default();
        let mut open = RibbonOpen::default();
        let mut placement = RibbonPlacement::default();
        let mut drag = RibbonDrag::default();

        for _ in 0..2 {
            run_pass(&ctx, || {
                __internal_enforce_defaults(&ctx);
                let _ = bar.show(&ctx, &mut open, &mut placement, &mut drag);
            });
        }
        // App goes silent; hysteresis covers one pass, then Mara draws.
        run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_none());
        run_pass(&ctx, || __internal_enforce_defaults(&ctx));
        assert!(__internal_shell_enforced_pass(&ctx).is_some());
    }
}
