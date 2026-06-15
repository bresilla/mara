//! # Bevy adapter for the enforced cross-platform shell bar.
//!
//! The bar itself — config, rendering, click→event mapping — lives in
//! [`mara_core::shell`] so it is identical on every host. This module
//! is the thin Bevy wiring: it stores [`mara_core::ShellBar`] as a
//! `Resource`, renders it each frame, and translates the resulting
//! [`mara_core::ShellEvent`]s into Bevy actions (close the app, toggle
//! the native window) while forwarding the app-level ones as a
//! `Message` for game/app systems to read.
//!
//! [`MaraShellPlugin`] is installed automatically by
//! [`MaraPlugin`](crate::MaraPlugin), so the bar is enforced on every
//! Bevy platform (desktop, web/wasm, android). Configure it via the
//! [`ShellBar`] resource; opt out with `ShellBar { enabled: false, .. }`.
//!
//! ```ignore
//! use bevy::prelude::*;
//! use bevy_mara::prelude::*;
//!
//! App::new()
//!     .add_plugins(DefaultPlugins)
//!     .add_plugins(bevy_egui::EguiPlugin::default())
//!     .add_plugins(bevy_mara::MaraPlugin)               // bar enforced here
//!     .add_systems(Startup, |mut bar: ResMut<ShellBar>| {
//!         bar.views = vec![ShellView::new("scene", "cube", "Scene")];
//!         bar.active = Some("scene");
//!     })
//!     .add_systems(Update, |mut events: MessageReader<ShellEvent>| {
//!         for ev in events.read() {
//!             if let ShellEvent::ViewSelected(id) = ev { /* switch view */ }
//!         }
//!     })
//!     .run();
//! ```

use bevy::app::AppExit;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, EguiPrimaryContextPass};

use mara_core::ribbon::{RibbonDrag, RibbonOpen, RibbonPlacement};
use mara_core::{ShellBar, ShellEvent};

/// Installs the enforced permanent top bar on every Bevy platform.
/// Added automatically by [`MaraPlugin`](crate::MaraPlugin) — you
/// normally don't add this directly.
pub struct MaraShellPlugin;

impl Plugin for MaraShellPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ShellBar>()
            .add_message::<ShellEvent>()
            .add_systems(
                EguiPrimaryContextPass,
                // After theme (style is live) and after the chrome
                // claim/caps publication when that plugin is present —
                // ordering against an unconfigured set is a no-op, so
                // this is safe on web/android where no chrome runs.
                render_shell_bar_system
                    .after(crate::apply_theme_system)
                    .after(crate::window_chrome::MaraWindowChromeSet::ReleaseClaim),
            );
    }
}

#[allow(clippy::too_many_arguments)]
fn render_shell_bar_system(
    mut contexts: EguiContexts,
    mut bar: ResMut<ShellBar>,
    mut open: ResMut<RibbonOpen>,
    mut placement: ResMut<RibbonPlacement>,
    mut drag: ResMut<RibbonDrag>,
    mut exit: MessageWriter<AppExit>,
    mut events: MessageWriter<ShellEvent>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
    mut maximized: Local<bool>,
) {
    let Ok(ctx) = contexts.ctx_mut() else { return };
    for event in bar.show(ctx, &mut open, &mut placement, &mut drag) {
        match event {
            ShellEvent::CloseRequested => {
                exit.write(AppExit::Success);
            }
            ShellEvent::MaximizeToggleRequested => {
                *maximized = !*maximized;
                if let Ok(mut window) = windows.single_mut() {
                    window.set_maximized(*maximized);
                }
            }
            // App-level events: forward for game/app systems to handle.
            other => {
                events.write(other);
            }
        }
    }
}
