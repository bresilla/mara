//! Bevy-owned Mara example.
//!
//! This is the opposite host model from `native.rs`: Bevy owns the
//! window, frame, render loop, scene, and input pipeline. Mara is used
//! as UI inside `bevy_egui` through the Bevy plugin path.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, EguiPlugin, EguiPrimaryContextPass};
use bevy_glacial::prelude::*;
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
    .add_systems(
        EguiPrimaryContextPass,
        mara_ui_system.after(bevy_mara::RibbonGhostSet),
    );
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
