//! # bevy_mara — Bevy integration for the mara UI kit.
//!
//! All UI primitives (widgets, ribbons, floating panes, shelves,
//! node-graph wrapper, code editor, theme) live in the
//! framework-agnostic [`mara_core`] crate. This crate adds:
//!
//! * [`MaraPlugin`] — one-line install that registers mara_core's
//!   state types as Bevy `Resource`s and runs the theme + ghost
//!   systems every frame.
//! * [`ThemePlugin`] / [`RibbonPlugin`] — granular alternatives if
//!   you want just one piece.
//! * [`GizmoMaterial`] — always-on-top transform-gizmo material
//!   extension (Bevy-specific).
//!
//! Consumers using `use bevy_mara::prelude::*;` keep the same API
//! they had before the workspace split — this crate re-exports
//! everything from `mara_core` verbatim and adds the plugins on top.
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_mara::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins(DefaultPlugins)
//!         .add_plugins(bevy_egui::EguiPlugin::default())
//!         .add_plugins(MaraPlugin)
//!         .run();
//! }
//! ```

pub mod embedded_view;
pub mod gizmo_material;
pub mod node_view_backend;
pub mod prelude;
pub mod window_chrome;

// `extras` (vendored graph + code_editor + maximize) lives in
// `mara_core` so the egui-only `egui_mara` facade can ship the same
// graph + code wrappers without dragging Bevy in. Re-exported here
// at the legacy `bevy_mara::extras::*` path so existing call
// sites stay put.
pub use embedded_view::{
    BevyEmbeddedView, BevyViewportBridge, BevyViewportInput, BevyViewportTexture,
    BevyViewportWgpuResources, CapturedBevyFrame, make_viewport_render_target,
    spawn_viewport_camera,
};
pub use mara_core::extras;

// Re-export `mara_core` so apps can keep going through `bevy_mara::*`
// for state types, widgets, the pane / ribbon / pod systems, etc.
pub use mara_core::*;

use bevy::ecs::message::{MessageReader, Messages};
use bevy::input::ButtonState;
use bevy::input::mouse::{
    AccumulatedMouseMotion, AccumulatedMouseScroll, MouseButtonInput, MouseWheel,
};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPreUpdateSet, EguiPrimaryContextPass, egui};
use std::collections::HashSet;

// ─── Theme ──────────────────────────────────────────────────────────

/// Registers [`mara_core::style::AccentColor`] +
/// [`mara_core::style::GlassOpacity`] as Bevy resources and runs
/// [`mara_core::style::apply_theme`] every frame.
pub struct ThemePlugin;

impl Plugin for ThemePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<mara_core::style::AccentColor>()
            .init_resource::<mara_core::style::GlassOpacity>()
            .add_systems(PreUpdate, sync_glass_opacity_system)
            .add_systems(EguiPrimaryContextPass, apply_theme_system);
    }
}

fn sync_glass_opacity_system(opacity: Res<mara_core::style::GlassOpacity>) {
    mara_core::style::set_glass_opacity(opacity.0);
}

fn apply_theme_system(
    mut contexts: EguiContexts,
    accent: Res<mara_core::style::AccentColor>,
    opacity: Res<mara_core::style::GlassOpacity>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    mara_core::style::apply_theme(ctx, *accent, *opacity);
}

// ─── Ribbons ────────────────────────────────────────────────────────

/// SystemSet the ribbon paint pipeline lives in. Downstream plugins
/// can order their own ribbon-painting panels around this set.
#[derive(SystemSet, Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct RibbonGhostSet;

/// Registers the mara_core ribbon `Resource`s + the F12 debug toggle.
/// [`MaraPlugin`] installs this transitively.
pub struct RibbonPlugin;

impl Plugin for RibbonPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<mara_core::ribbon::RibbonOpen>()
            .init_resource::<mara_core::ribbon::RibbonWidth>()
            .init_resource::<mara_core::ribbon::RibbonPlacement>()
            .init_resource::<mara_core::ribbon::RibbonDrag>()
            .configure_sets(
                EguiPrimaryContextPass,
                RibbonGhostSet.after(apply_theme_system),
            )
            .add_systems(EguiPrimaryContextPass, debug_toggle_system);
    }
}

/// **F12** — toggle egui's "show interactive widget bounds" overlay.
/// Renders a colored outline around every widget egui knows about,
/// plus the layout rects driving it. Use this to show the dev where
/// a layout is breaking. Bound globally on the primary egui ctx.
///
/// `Style.debug` is `#[cfg(debug_assertions)]`-gated by egui itself,
/// so this toggle only compiles in debug builds. `make run` runs
/// `--release` — to use F12, run a debug build of a consumer app
/// or override `debug-assertions = true` in the workspace release
/// profile.
fn debug_toggle_system(mut contexts: EguiContexts) {
    let Ok(_ctx) = contexts.ctx_mut() else { return };
    #[cfg(debug_assertions)]
    {
        let pressed = _ctx.input_mut(|i| {
            i.consume_key(bevy_egui::egui::Modifiers::NONE, bevy_egui::egui::Key::F12)
        });
        if pressed {
            _ctx.style_mut(|s| {
                s.debug.show_interactive_widgets = !s.debug.show_interactive_widgets;
                s.debug.show_widget_hits = s.debug.show_interactive_widgets;
            });
        }
    }
}

// ─── Pointer-event firewall ────────────────────────────────────────

/// Pointer-event firewall for clicks / drags / scroll over a mara
/// pane. Selectively blocks Bevy-side consumers from seeing input
/// that's "for the UI", without breaking ongoing interactions that
/// originated outside the UI.
///
/// ## What it filters
///
/// 1. **Mouse wheel**: cleared whenever the cursor sits inside any
///    `mara_core::pane::published_pane_rects`. Wheel events are
///    one-shot and don't have an "ongoing" semantic, so a flat
///    cursor-over-pane gate is correct.
///
/// 2. **Mouse buttons (polled `ButtonInput<MouseButton>`)**: only
///    *the buttons whose CURRENT hold started over a pane* get
///    `release(button)` called on them. Buttons whose press
///    happened on the 3D viewport remain "pressed" in the polled
///    state for the entire hold, even if the cursor moves over a
///    pane mid-drag. This is what makes middle-click pan
///    (start-on-viewport) keep working continuously instead of
///    dropping to "released" the moment the cursor crosses a pane.
///
/// We do NOT filter `Messages<MouseButtonInput>` events because
/// bevy_glacial (and most Bevy camera/picking code) reads polled
/// `ButtonInput` rather than raw events. Filtering events would
/// add complexity (drain-and-rewrite plumbing) without changing
/// the practical behaviour for typical consumers.
///
/// ## Why not `is_pointer_over_area` / `layer_id_at`?
///
/// `is_pointer_over_area` returns `false` for `Order::Background`
/// layers when no `CentralPanel` is installed (mara panes are
/// Background; we don't install a CentralPanel). `layer_id_at`
/// has modal / tooltip-area edge cases. The published-rects
/// approach works for any pane order without those gotchas.
///
/// ## Ordering
///
/// `.after(EguiPreUpdateSet::ProcessInput)` so bevy_egui's input
/// forwarder has already copied events into egui's own
/// `EguiInput`. The UI keeps responding to clicks / scrolls
/// normally; only the Bevy-side polled state is masked.
#[allow(clippy::too_many_arguments)]
fn consume_egui_input_system(
    primary_window: Query<&Window, With<PrimaryWindow>>,
    mut contexts: EguiContexts,
    mut wheel_events: ResMut<Messages<MouseWheel>>,
    mut button_events: MessageReader<MouseButtonInput>,
    mut mouse_buttons: ResMut<ButtonInput<MouseButton>>,
    mut accumulated_scroll: ResMut<AccumulatedMouseScroll>,
    mut accumulated_motion: ResMut<AccumulatedMouseMotion>,
    mut pressed_over_pane: Local<HashSet<MouseButton>>,
) {
    let Ok(window) = primary_window.single() else {
        return;
    };
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else { return };
    let pos = egui::pos2(cursor.x, cursor.y);
    let pane_rects = mara_core::pane::published_pane_rects(ctx);
    let cursor_over_pane = pane_rects.iter().any(|r| r.contains(pos));

    // `egui.is_using_pointer()` is true whenever ANY egui widget has
    // claimed the pointer this frame — text-edit drag in the code
    // editor, graph canvas pan/wheel-zoom, slider drag, etc. Catches
    // host widgets like `mara_node_graph` that use a secondary
    // `egui::Context` underneath (the parent's pane rect contains
    // the cursor, but the secondary ctx is the one with focus). We
    // also check `wants_pointer_input` so a fresh click landing on
    // an interactive widget is masked the same frame, not one frame
    // late.
    let egui_owns_pointer = ctx.is_using_pointer() || ctx.wants_pointer_input();
    // When ANY mara widget is in its fullscreen overlay (graph or
    // code editor maximised), the entire screen IS the UI — so the
    // Bevy 3D layer below must never see pointer events, no matter
    // where the cursor sits or whether the parent ctx has claimed
    // it (the graph drives input through a SECONDARY egui context,
    // so the parent's `is_using_pointer` stays false while the user
    // pans the graph).
    let fs_active = mara_core::embed::is_any_fullscreen(ctx);
    let ui_owns_pointer = cursor_over_pane || egui_owns_pointer || fs_active;

    // Track which mouse buttons were pressed while the cursor was
    // over a pane. Released → drop from the set. The whole point
    // of the set is to remember presses across frames so a
    // subsequent over-pane mouse move during a viewport drag
    // doesn't accidentally classify the hold as "over pane".
    for ev in button_events.read() {
        match ev.state {
            ButtonState::Pressed => {
                if ui_owns_pointer {
                    pressed_over_pane.insert(ev.button);
                }
            }
            ButtonState::Released => {
                pressed_over_pane.remove(&ev.button);
            }
        }
    }

    // Mask only the buttons whose current hold belongs to the UI.
    // `release` clears the `pressed` set; `clear_just_pressed` clears
    // the `just_pressed` set. Both are needed — `ButtonInput::release`
    // ALONE leaves `just_pressed(btn)` returning true on the press
    // frame, so a click on a mara-pane button still fires viewport
    // pickers gated only by `just_pressed`.
    for &btn in pressed_over_pane.iter() {
        mouse_buttons.release(btn);
        mouse_buttons.clear_just_pressed(btn);
    }

    // Wheel: simple cursor-over-pane gate — wheel events are
    // one-shot, no ongoing-interaction concept. Drain BOTH the
    // raw `MouseWheel` event queue AND Bevy's pre-accumulated
    // `AccumulatedMouseScroll` resource — the latter is what most
    // camera scripts read (`Res<AccumulatedMouseScroll>`), and it's
    // already populated by Bevy's input pipeline by the time this
    // system runs (`.after(EguiPreUpdateSet::ProcessInput)`), so
    // clearing the events alone leaves the accumulated delta
    // visible to downstream consumers.
    //
    // Same story for `AccumulatedMouseMotion` — orbit-style cameras
    // read it directly to drive yaw/pitch on RMB drag, and would
    // otherwise see motion deltas accumulated over the UI. Buttons
    // on the pane are already masked above, so the camera shouldn't
    // be in "drag" mode at this point — but zero the motion delta
    // anyway to defend against viewport cameras that orbit on plain
    // mouse motion.
    //
    // Drain when EITHER the cursor sits on a pane OR egui has
    // grabbed the pointer (e.g. an ongoing graph pan whose sub-
    // context is consuming the wheel for canvas zoom).
    if ui_owns_pointer {
        wheel_events.clear();
        accumulated_scroll.delta = Vec2::ZERO;
        accumulated_motion.delta = Vec2::ZERO;
    }

    // Clear the published-rects list now that we've consumed it.
    // The next egui pass either repopulates it (open panes call
    // `Pane::show` → publish) or leaves it empty (no panes shown
    // this frame). Without this, closing every pane would leave the
    // last-seen rects stuck in ctx data — `Pane::show` is the only
    // other reset path, and it doesn't fire when no panes paint.
    mara_core::pane::clear_published_pane_rects(ctx);
}

/// Standalone plugin that installs only the egui pointer-event
/// firewall — useful for apps that can't take the full
/// [`MaraPlugin`] (e.g. apps already wiring their own theme +
/// ribbon resources from a different source). Add this alone
/// alongside `EguiPlugin` and you get the same input-absorption
/// behaviour without dragging in `ThemePlugin` / `RibbonPlugin`.
pub struct EguiInputAbsorbPlugin;

impl Plugin for EguiInputAbsorbPlugin {
    fn build(&self, app: &mut App) {
        // Run `.after(EguiPreUpdateSet::ProcessInput)` — bevy_egui's
        // set that copies `Messages<MouseWheel>` into egui's
        // `EguiInput`. If we cleared the queue earlier the UI would
        // miss the scroll entirely. After this set, egui has its
        // copy and we're free to drain so downstream `Update` systems
        // (e.g. bevy_glacial's chase-camera zoom) see nothing.
        app.add_systems(
            PreUpdate,
            consume_egui_input_system.after(EguiPreUpdateSet::ProcessInput),
        );
    }
}

// ─── Combined install ──────────────────────────────────────────────

/// Full mara install — `ThemePlugin` + `RibbonPlugin` +
/// [`EguiInputAbsorbPlugin`]. Idempotent; safe to add alongside any
/// other Bevy plugins.
pub struct MaraPlugin;

impl Plugin for MaraPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ThemePlugin>() {
            app.add_plugins(ThemePlugin);
        }
        if !app.is_plugin_added::<RibbonPlugin>() {
            app.add_plugins(RibbonPlugin);
        }
        if !app.is_plugin_added::<EguiInputAbsorbPlugin>() {
            app.add_plugins(EguiInputAbsorbPlugin);
        }
        if !app.is_plugin_added::<node_view_backend::NodeViewPlugin>() {
            app.add_plugins(node_view_backend::NodeViewPlugin);
        }
    }
}
