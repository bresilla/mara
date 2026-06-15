//! Bevy-owned Mara example.
//!
//! This is the opposite host model from `native.rs`: Bevy owns the
//! window, frame, render loop, scene, and input pipeline. Mara is used
//! as UI inside `bevy_egui` through the Bevy plugin path.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use bevy_glacial::prelude::*;
use bevy_mara::{ShellBar, ShellEvent};
use mara::host::{MaraHostCtx, MaraWindowHost};
use mara_example::{DemoApp, app::ui_system, bevy_content};

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: "Mara example — Bevy host".into(),
            resolution: (1440, 920).into(),
            ..default()
        }),
        ..default()
    }))
    .add_plugins(EguiPlugin::default())
    .add_plugins(bevy_mara::MaraPlugin)
    .add_plugins(GlacialPlugins)
    .insert_non_send_resource(DemoApp::new_bevy_hosted())
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Name::new("MaraBevyHost"));
    })
    // The permanent top bar is the enforced shell from `MaraPlugin`.
    // The demo configures it from its own state and receives the
    // resulting `ShellEvent`s — dogfooding the same bar the library
    // ships, instead of hand-rolling one.
    .add_systems(
        EguiPrimaryContextPass,
        (
            mara_ui_system.after(bevy_mara::RibbonGhostSet),
            sync_shell_bar,
        ),
    )
    .add_systems(Update, read_shell_events);
    bevy_content::configure_bevy_host_app(&mut app);
    app.run();
}

fn mara_ui_system(
    mut contexts: EguiContexts,
    mut demo: NonSendMut<DemoApp>,
    mut scene_visible: ResMut<bevy_content::BevyHostSceneVisible>,
    picked: Res<bevy_mara::BevyViewportPickedColor>,
) {
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    if let Some(color) = picked.0 {
        demo.set_accent_color(color);
    }

    let mut host = MaraHostCtx::new(ctx, None, MaraWindowHost::None);
    ui_system(&mut demo, &mut host);
    scene_visible.0 = demo.bevy_host_scene_visible();
}

/// Push the demo's current view switcher / active selection into the
/// enforced `ShellBar` so the plugin renders the right bar.
fn sync_shell_bar(demo: NonSend<DemoApp>, mut bar: ResMut<ShellBar>) {
    demo.configure_shell_bar(&mut bar);
}

/// Deliver top-bar interactions back into the demo's dispatch.
fn read_shell_events(mut demo: NonSendMut<DemoApp>, mut events: MessageReader<ShellEvent>) {
    for event in events.read() {
        demo.queue_shell_event(*event);
    }
}
