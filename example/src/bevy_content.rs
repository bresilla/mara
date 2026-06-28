//! Example-owned Bevy scene content.
//!
//! The Mara library owns the viewport/window/embed mechanics. This
//! file is only the demo scene: planet/clouds, swatch cubes, picking,
//! and the accent-colour handoff.

use bevy::camera::RenderTarget;
use bevy::input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll};
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use eframe::egui;
use mara::ui::modules::bevy::{
    BevyViewportInput, BevyViewportPickedColor, BevyViewportRenderTarget, BevyViewportSet,
    ChaseCamera, GroundGrid, apply_rig, apply_viewport_camera_input_system,
};

const PLANET_RADIUS: f32 = 6_371_000.0;
const CLOUD_ALTITUDE_M: f32 = 4_000.0;

pub fn configure_app(app: &mut App) {
    app.insert_resource(ClearColor(Color::srgb_u8(10, 12, 16)))
        .insert_resource(GroundGrid {
            visible: true,
            color: Color::srgba(0.30, 0.38, 0.50, 0.42),
        })
        .init_resource::<BevyViewportInput>()
        .init_resource::<SelectedSwatch>()
        .add_systems(Startup, setup_scene.after(BevyViewportSet::SetupTarget))
        .add_systems(Update, (pick_cube, update_swatch_selection));
}

/// Configure the same demo Bevy scene when Bevy owns the top-level
/// app/window. This path does not use the offscreen
/// `BevyViewportRenderTarget`; the camera renders into Bevy's normal
/// frame. The canonical Mara app path embeds this scene as viewport
/// content instead of drawing Mara UI through a Bevy egui bridge.
pub fn configure_bevy_host_app(app: &mut App) {
    app.insert_resource(ClearColor(Color::srgb_u8(10, 12, 16)))
        .insert_resource(BevyHostSceneProfile)
        .init_resource::<BevyHostSceneVisible>()
        .insert_resource(GroundGrid {
            visible: true,
            color: Color::srgba(0.30, 0.38, 0.50, 0.42),
        })
        .init_resource::<BevyViewportInput>()
        .init_resource::<BevyViewportPickedColor>()
        .init_resource::<SelectedSwatch>()
        .add_systems(Startup, setup_scene)
        .add_systems(
            Update,
            (
                sync_bevy_host_viewport_input,
                apply_viewport_camera_input_system,
                apply_bevy_host_scene_visibility,
                pick_cube,
                update_swatch_selection,
            )
                .chain(),
        );
}

#[derive(Component)]
struct ColorCube {
    egui_col: egui::Color32,
    base_color: Color,
}

#[derive(Resource, Default)]
struct SelectedSwatch(Option<Entity>);

#[derive(Resource)]
struct BevyHostSceneProfile;

#[derive(Resource)]
pub struct BevyHostSceneVisible(pub bool);

impl Default for BevyHostSceneVisible {
    fn default() -> Self {
        Self(true)
    }
}

#[derive(Component)]
struct BevyHostSceneObject;

fn sync_bevy_host_viewport_input(
    primary_window: Query<&Window, With<PrimaryWindow>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    visible: Res<BevyHostSceneVisible>,
    mut input: ResMut<BevyViewportInput>,
) {
    if !visible.0 {
        *input = BevyViewportInput::default();
        return;
    }

    let pointer_pos = primary_window
        .single()
        .ok()
        .and_then(|window| window.cursor_position())
        .map(|pos| [pos.x, pos.y]);

    *input = BevyViewportInput {
        pointer_pos,
        drag_delta: if mouse_buttons.pressed(MouseButton::Left)
            && !mouse_buttons.pressed(MouseButton::Middle)
        {
            [motion.delta.x, motion.delta.y]
        } else {
            [0.0, 0.0]
        },
        pan_delta: if mouse_buttons.pressed(MouseButton::Middle) {
            [motion.delta.x, motion.delta.y]
        } else {
            [0.0, 0.0]
        },
        scroll_delta: scroll.delta.y / 120.0,
        primary_clicked: mouse_buttons.just_pressed(MouseButton::Left),
    };
}

fn apply_bevy_host_scene_visibility(
    visible: Res<BevyHostSceneVisible>,
    mut grid: ResMut<GroundGrid>,
    mut objects: Query<&mut Visibility, With<BevyHostSceneObject>>,
) {
    if !visible.is_changed() {
        return;
    }
    grid.visible = visible.0;
    let target = if visible.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for mut object_visibility in &mut objects {
        *object_visibility = target;
    }
}

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    render_target: Option<Res<BevyViewportRenderTarget>>,
    bevy_host: Option<Res<BevyHostSceneProfile>>,
) {
    let bevy_host = bevy_host.is_some();
    let (planet_u, planet_v, cloud_u, cloud_v) = if bevy_host {
        (192, 96, 32, 16)
    } else {
        (1024, 512, 64, 32)
    };

    let planet_mesh = meshes.add(Sphere::new(PLANET_RADIUS).mesh().uv(planet_u, planet_v));
    let planet_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.62, 0.48, 0.33),
        perceptual_roughness: 0.95,
        ..default()
    });
    commands.spawn((
        Name::new("Planet"),
        Transform::from_xyz(0.0, -PLANET_RADIUS, 0.0),
        Mesh3d(planet_mesh),
        MeshMaterial3d(planet_mat),
        Visibility::Inherited,
        BevyHostSceneObject,
        NotShadowCaster,
        NotShadowReceiver,
    ));

    let shell_radius = PLANET_RADIUS + CLOUD_ALTITUDE_M;
    let cloud_mesh = meshes.add(Sphere::new(shell_radius).mesh().uv(cloud_u, cloud_v));
    let cloud_mat = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.35),
        alpha_mode: AlphaMode::Blend,
        double_sided: true,
        cull_mode: None,
        unlit: false,
        perceptual_roughness: 1.0,
        ..default()
    });
    commands.spawn((
        Name::new("CloudShell"),
        Transform::from_xyz(0.0, -PLANET_RADIUS, 0.0),
        Mesh3d(cloud_mesh),
        MeshMaterial3d(cloud_mat),
        Visibility::Inherited,
        BevyHostSceneObject,
        NotShadowCaster,
    ));

    let cube_mesh = meshes.add(Cuboid::from_length(1.0));
    let swatch: [(f32, f32, f32); 6] = [
        (0.90, 0.30, 0.30),
        (0.95, 0.65, 0.20),
        (0.95, 0.90, 0.30),
        (0.35, 0.85, 0.45),
        (0.30, 0.60, 0.95),
        (0.75, 0.45, 0.95),
    ];
    const GRID_COLS: usize = 3;
    const GRID_SPACING: f32 = 2.0;
    for (i, &(r, g, b)) in swatch.iter().enumerate() {
        let col = (i % GRID_COLS) as f32;
        let row = (i / GRID_COLS) as f32;
        let x = (col - (GRID_COLS as f32 - 1.0) * 0.5) * GRID_SPACING;
        let z = (row - 0.5) * GRID_SPACING;
        let bevy_col = Color::srgb(r, g, b);
        let egui_col = egui::Color32::from_rgb(
            (r * 255.0).round() as u8,
            (g * 255.0).round() as u8,
            (b * 255.0).round() as u8,
        );
        commands.spawn((
            Name::new(format!("Swatch[{i}]")),
            Mesh3d(cube_mesh.clone()),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: bevy_col,
                perceptual_roughness: 0.6,
                ..default()
            })),
            Transform::from_xyz(x, 0.5, z),
            Visibility::Inherited,
            BevyHostSceneObject,
            ColorCube {
                egui_col,
                base_color: bevy_col,
            },
        ));
    }

    let sun_shadow = CascadeShadowConfigBuilder {
        num_cascades: 1,
        minimum_distance: 0.1,
        maximum_distance: 100.0,
        first_cascade_far_bound: 100.0,
        overlap_proportion: 0.0,
    }
    .build();
    commands.spawn((
        Name::new("Sun"),
        Transform::from_xyz(5.0, 50.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
        DirectionalLight {
            illuminance: 10_000.0,
            shadow_maps_enabled: !bevy_host,
            ..default()
        },
        sun_shadow,
    ));

    let projection = Projection::Perspective(PerspectiveProjection {
        near: 0.1,
        far: PLANET_RADIUS * 2.5,
        ..default()
    });
    let fog = (!bevy_host).then_some(DistanceFog {
        color: Color::srgb(0.10, 0.13, 0.20),
        falloff: FogFalloff::Atmospheric {
            extinction: Vec3::new(0.00008, 0.00012, 0.00020),
            inscattering: Vec3::new(0.00010, 0.00015, 0.00025),
        },
        ..default()
    });
    let chase = ChaseCamera::default();
    let mut cam_tr = Transform::default();
    apply_rig(&chase, &mut cam_tr);
    let mut camera = commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        cam_tr,
        projection,
        AmbientLight {
            color: Color::WHITE,
            brightness: 120.0,
            ..default()
        },
        chase,
    ));
    if let Some(fog) = fog {
        camera.insert(fog);
    }
    if let Some(render_target) = render_target {
        camera.insert(RenderTarget::from(render_target.0.clone()));
    }
}

fn pick_cube(
    input: Res<BevyViewportInput>,
    visible: Option<Res<BevyHostSceneVisible>>,
    bevy_cameras: Query<(&Camera, &GlobalTransform)>,
    cubes: Query<(Entity, &Transform, &ColorCube)>,
    mut picked: ResMut<BevyViewportPickedColor>,
    mut selected: ResMut<SelectedSwatch>,
) {
    picked.0 = None;
    if visible.as_ref().is_some_and(|visible| !visible.0) {
        return;
    }
    if !input.primary_clicked {
        return;
    }
    let Some([x, y]) = input.pointer_pos else {
        return;
    };
    let Ok((camera, cam_tr)) = bevy_cameras.single() else {
        return;
    };
    let Ok(ray) = camera.viewport_to_world(cam_tr, Vec2::new(x, y)) else {
        return;
    };
    let origin = ray.origin;
    let direction = *ray.direction;
    let mut best: Option<(f32, Entity, egui::Color32)> = None;
    for (entity, tr, cube) in &cubes {
        let min = tr.translation - Vec3::splat(0.5);
        let max = tr.translation + Vec3::splat(0.5);
        if let Some(t) = ray_aabb_hit(origin, direction, min, max) {
            match best {
                Some((bt, _, _)) if bt <= t => {}
                _ => best = Some((t, entity, cube.egui_col)),
            }
        }
    }
    if let Some((_, entity, color)) = best {
        picked.0 = Some(color.into());
        selected.0 = Some(entity);
    }
}

fn update_swatch_selection(
    time: Res<Time>,
    selected: Res<SelectedSwatch>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut cubes: Query<(
        Entity,
        &ColorCube,
        &MeshMaterial3d<StandardMaterial>,
        &mut Transform,
    )>,
) {
    const REST_Y: f32 = 0.5;
    const LIFT_Y: f32 = 0.9;
    const EASE: f32 = 8.0;
    let k = (EASE * time.delta_secs()).min(0.9);
    for (entity, cube, mat_handle, mut tr) in &mut cubes {
        let is_sel = selected.0 == Some(entity);
        let target_y = if is_sel { LIFT_Y } else { REST_Y };
        tr.translation.y += (target_y - tr.translation.y) * k;
        if let Some(mut mat) = materials.get_mut(&mat_handle.0) {
            mat.base_color = cube.base_color;
            let base = cube.base_color.to_linear();
            let gain = if is_sel { 1.8 } else { 0.0 };
            mat.emissive =
                LinearRgba::new(base.red * gain, base.green * gain, base.blue * gain, 1.0);
        }
    }
}

fn ray_aabb_hit(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut tmin = 0.0_f32;
    let mut tmax = f32::INFINITY;
    for i in 0..3 {
        let (o, d, lo, hi) = match i {
            0 => (origin.x, direction.x, min.x, max.x),
            1 => (origin.y, direction.y, min.y, max.y),
            _ => (origin.z, direction.z, min.z, max.z),
        };
        if d.abs() < 1e-6 {
            if o < lo || o > hi {
                return None;
            }
        } else {
            let mut t1 = (lo - o) / d;
            let mut t2 = (hi - o) / d;
            if t1 > t2 {
                std::mem::swap(&mut t1, &mut t2);
            }
            tmin = tmin.max(t1);
            tmax = tmax.min(t2);
            if tmin > tmax {
                return None;
            }
        }
    }
    Some(tmin.max(0.0))
}
