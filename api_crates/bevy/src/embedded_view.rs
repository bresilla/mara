//! Egui-owned embedded Bevy viewport bridge.
//!
//! This module is intentionally windowless: it gives editor-style
//! apps a Bevy-side scene/viewport state object that can be hosted
//! inside an eframe/Mara shell without Bevy taking ownership of the
//! top-level window. The root `example/` crate uses this as the
//! bridge surface today: native builds render a tiny Bevy scene into
//! an offscreen texture, read the latest rendered frame back, and let
//! the egui host upload that frame as its own texture.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::image::{BevyDefault, TextureFormatPixelInfo};
use bevy::light::{CascadeShadowConfigBuilder, NotShadowCaster, NotShadowReceiver};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::render::{
    Extract, Render, RenderApp, RenderSystems,
    render_asset::RenderAssets,
    render_graph::{self, NodeRunError, RenderGraph, RenderGraphContext, RenderLabel},
    render_resource::{
        Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode,
        PollType, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureDimension, TextureFormat,
        TextureUsages,
    },
    renderer::{RenderContext, RenderDevice, RenderQueue},
};
use bevy::window::ExitCondition;
use bevy::winit::WinitPlugin;
use bevy::{app::TerminalCtrlCHandlerPlugin, log::LogPlugin};
use bevy_glacial::prelude::*;
use crossbeam_channel::{Receiver, Sender};

/// Description of the viewport surface the egui host reserves for
/// Bevy. It is host-facing metadata, not a Bevy window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BevyViewportTexture {
    pub width: u32,
    pub height: u32,
}

impl BevyViewportTexture {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            width: width.max(1),
            height: height.max(1),
        }
    }
}

/// CPU-side copy of the latest native Bevy render target.
///
/// This is intentionally plain RGBA bytes so an eframe host can
/// upload it with `egui::Context::load_texture` / `TextureHandle::set`
/// without either renderer taking ownership of the other's swapchain.
#[derive(Debug, Clone)]
pub struct CapturedBevyFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub frame: u64,
}

/// Pointer input forwarded by an egui host into the embedded Bevy
/// scene. Coordinates are in physical pixels relative to the viewport
/// image.
#[derive(Debug, Clone, Copy, Default)]
pub struct BevyViewportInput {
    pub pointer_pos: Option<[f32; 2]>,
    pub drag_delta: [f32; 2],
    pub scroll_delta: f32,
    pub primary_clicked: bool,
}

/// Existing wgpu resources supplied by an egui/eframe host.
///
/// Passing these keeps the embedded Bevy renderer on the same adapter
/// that already successfully opened the eframe window instead of
/// asking wgpu for a second headless adapter.
#[derive(Clone)]
pub struct BevyViewportWgpuResources {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter: wgpu::Adapter,
}

impl BevyViewportWgpuResources {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue, adapter: wgpu::Adapter) -> Self {
        Self {
            device,
            queue,
            adapter,
        }
    }
}

/// Windowless Bevy scene state for an embedded viewport.
///
/// This keeps a real Bevy [`World`] and updates Bevy transforms every
/// frame, but does not install `WinitPlugin` or create a Bevy window.
/// It is deliberately small so consumers can embed it in any egui host.
pub struct BevyViewportBridge {
    world: World,
    cube: Entity,
    frame: u64,
    seconds: f32,
    texture: BevyViewportTexture,
    external_wgpu: Option<BevyViewportWgpuResources>,
    renderer: Option<BevyViewportRenderer>,
    renderer_failed: bool,
    latest_frame: Option<CapturedBevyFrame>,
    input: BevyViewportInput,
    scene_state: Option<EmbeddedViewportSceneState>,
}

impl Default for BevyViewportBridge {
    fn default() -> Self {
        Self::new(BevyViewportTexture::new(512, 512))
    }
}

impl BevyViewportBridge {
    pub fn new(texture: BevyViewportTexture) -> Self {
        let mut world = World::new();
        let cube = world
            .spawn((
                Transform::from_xyz(0.0, 0.0, 0.0),
                GlobalTransform::default(),
            ))
            .id();

        Self {
            world,
            cube,
            frame: 0,
            seconds: 0.0,
            texture,
            external_wgpu: None,
            renderer: None,
            renderer_failed: false,
            latest_frame: None,
            input: BevyViewportInput::default(),
            scene_state: None,
        }
    }

    pub fn with_wgpu_resources(
        texture: BevyViewportTexture,
        resources: BevyViewportWgpuResources,
    ) -> Self {
        let mut bridge = Self::new(texture);
        bridge.external_wgpu = Some(resources);
        bridge
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.texture = BevyViewportTexture::new(width, height);
    }

    pub fn tick(&mut self, dt_seconds: f32) {
        self.frame = self.frame.saturating_add(1);
        self.seconds += dt_seconds.max(0.0);

        if let Some(mut transform) = self.world.get_mut::<Transform>(self.cube) {
            transform.rotation = Quat::from_rotation_y(self.seconds * 0.75)
                * Quat::from_rotation_x(self.seconds * 0.35);
        }
    }

    pub fn frame(&self) -> u64 {
        self.frame
    }

    pub fn seconds(&self) -> f32 {
        self.seconds
    }

    pub fn texture(&self) -> BevyViewportTexture {
        self.texture
    }

    pub fn rotation_angle(&self) -> f32 {
        self.world
            .get::<Transform>(self.cube)
            .map(|transform| transform.rotation.to_euler(EulerRot::YXZ).0)
            .unwrap_or(0.0)
    }

    /// Allocate a Bevy `Image` suitable for an offscreen camera render
    /// target with this bridge's current viewport size.
    pub fn render_target_image(&self, format: TextureFormat) -> Image {
        make_viewport_render_target(self.texture, format)
    }

    /// Render one native Bevy frame for the current viewport size and
    /// return the newest captured RGBA frame if the render pipeline has
    /// produced one.
    ///
    /// This is the safe bridge path from the plan: Bevy owns a
    /// windowless offscreen render target, while egui/eframe remains
    /// the top-level window and uploads the returned pixels as an egui
    /// texture. A future zero-copy path can replace the CPU readback
    /// without changing the app-shell ownership model.
    pub fn render_frame(
        &mut self,
        width: u32,
        height: u32,
        dt_seconds: f32,
    ) -> Option<&CapturedBevyFrame> {
        self.render_frame_with_input(width, height, dt_seconds, BevyViewportInput::default())
    }

    pub fn render_frame_with_input(
        &mut self,
        width: u32,
        height: u32,
        dt_seconds: f32,
        input: BevyViewportInput,
    ) -> Option<&CapturedBevyFrame> {
        self.resize(width, height);
        self.tick(dt_seconds);
        self.input = input;

        if self.renderer_failed {
            return self.latest_frame.as_ref();
        }

        let texture = self.texture;
        let rendered = catch_unwind(AssertUnwindSafe(|| {
            let renderer = self.renderer.get_or_insert_with(|| {
                BevyViewportRenderer::new(texture, self.external_wgpu.clone())
            });
            if renderer.texture() != texture {
                let scene_state = renderer.scene_state().or(self.scene_state);
                if !renderer.resize(texture) {
                    *renderer = BevyViewportRenderer::new(texture, self.external_wgpu.clone());
                    if let Some(scene_state) = scene_state {
                        renderer.apply_scene_state(scene_state);
                    }
                }
            }
            renderer.set_input(self.input);
            let frame = renderer.render_next();
            self.scene_state = renderer.scene_state();
            frame
        }));

        match rendered {
            Ok(Some(mut frame)) => {
                frame.frame = self.frame;
                self.latest_frame = Some(frame);
            }
            Ok(None) => {}
            Err(_) => {
                self.renderer = None;
                self.renderer_failed = true;
            }
        }
        self.latest_frame.as_ref()
    }

    /// Returns true after Bevy failed to initialize or tick its
    /// native renderer, usually because the process has no usable
    /// `wgpu` adapter. The egui host can keep the app shell alive and
    /// display a fallback instead of crashing.
    pub fn renderer_failed(&self) -> bool {
        self.renderer_failed
    }

    /// Color picked by a cube click during the last rendered frame.
    ///
    /// This is event-like: it is `Some` only on a frame where the
    /// embedded Bevy scene hit a swatch cube with a primary click.
    pub fn picked_swatch_color(&self) -> Option<egui::Color32> {
        self.scene_state.and_then(|state| state.picked_color)
    }
}

/// Allocate a Bevy-owned render-target image for an embedded
/// viewport. The usage flags are the important contract:
/// `RENDER_ATTACHMENT` lets a Bevy camera render into it,
/// `TEXTURE_BINDING` lets a UI/material sample it, and `COPY_SRC`
/// leaves room for host bridges that need to copy into another
/// renderer's texture.
pub fn make_viewport_render_target(texture: BevyViewportTexture, format: TextureFormat) -> Image {
    let mut image = Image::new_fill(
        Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        format,
        RenderAssetUsages::default(),
    );
    image.texture_descriptor.usage =
        TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC;
    image
}

/// Spawn a Bevy 3D camera configured to render into a viewport image
/// instead of a window. Consumers add the returned entity to their
/// own windowless Bevy app/world and store `target` in `Assets<Image>`.
pub fn spawn_viewport_camera(world: &mut World, target: Handle<Image>) -> Entity {
    world
        .spawn((
            Camera3d::default(),
            Camera::default(),
            RenderTarget::from(target),
            Transform::from_xyz(0.0, 2.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
            GlobalTransform::default(),
        ))
        .id()
}

/// Native headless Bevy renderer used by [`BevyViewportBridge`].
///
/// The renderer owns a small Bevy [`App`] with no primary window and
/// no `WinitPlugin`. The scene camera targets a Bevy `Image`; the
/// render graph copies that image into a CPU buffer and sends compact
/// RGBA frames back to the main world.
pub struct BevyViewportRenderer {
    app: App,
    texture: BevyViewportTexture,
    input: BevyViewportInput,
    resize_warmup_frames: u8,
}

#[derive(Clone, Copy, Debug)]
struct EmbeddedViewportSceneState {
    focus: Vec3,
    yaw: f32,
    elevation: f32,
    distance: f32,
    selected_index: Option<usize>,
    picked_color: Option<egui::Color32>,
}

impl BevyViewportRenderer {
    pub fn new(
        texture: BevyViewportTexture,
        _resources: Option<BevyViewportWgpuResources>,
    ) -> Self {
        let mut app = App::new();
        let default_plugins = DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .disable::<WinitPlugin>();
        let default_plugins = default_plugins
            .disable::<LogPlugin>()
            .disable::<TerminalCtrlCHandlerPlugin>();

        app.insert_resource(EmbeddedViewportConfig { texture })
            .insert_resource(ClearColor(Color::srgb_u8(10, 12, 16)))
            .insert_resource(GroundGrid {
                visible: true,
                color: Color::srgba(0.30, 0.38, 0.50, 0.42),
            })
            .init_resource::<SelectedSwatch>()
            .add_plugins(default_plugins)
            // The embedded viewport receives pointer/scroll input
            // from egui and applies it manually before each Bevy
            // tick. Do not also run bevy_glacial's native
            // `ChaseCameraPlugin`: its smoothed local zoom target has
            // no corresponding Bevy `MouseWheel` events here, so after
            // an egui-driven zoom it eases the camera back to the old
            // distance and looks like a bounce/reset.
            .add_plugins(GlacialPlugins.build().disable::<ChaseCameraPlugin>())
            .add_plugins(EmbeddedViewportCopyPlugin)
            .add_systems(Startup, setup_embedded_viewport_scene)
            .add_systems(Update, update_swatch_selection);

        // We drive this embedded app manually with `app.update()`
        // instead of `app.run()`, so Bevy's plugin lifecycle must be
        // completed explicitly. Renderer/PBR resources such as
        // `MeshPipeline` are created in plugin finish/cleanup hooks.
        app.finish();
        app.cleanup();

        Self {
            app,
            texture,
            input: BevyViewportInput::default(),
            resize_warmup_frames: 0,
        }
    }

    pub fn texture(&self) -> BevyViewportTexture {
        self.texture
    }

    pub fn resize(&mut self, texture: BevyViewportTexture) -> bool {
        if self.texture == texture {
            return true;
        }

        let world = self.app.world_mut();
        let Some(render_device) = world.get_resource::<RenderDevice>().cloned() else {
            return false;
        };

        let mut copiers = world.query::<&EmbeddedViewportImageCopier>();
        let Some(src_image) = copiers
            .iter(world)
            .next()
            .map(|copier| copier.src_image.clone())
        else {
            return false;
        };

        let mut render_target_image = Image::new_target_texture(
            texture.width,
            texture.height,
            TextureFormat::bevy_default(),
            None,
        );
        render_target_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;

        let Some(mut images) = world.get_resource_mut::<Assets<Image>>() else {
            return false;
        };
        if images.insert(src_image.id(), render_target_image).is_err() {
            return false;
        }
        drop(images);

        let size = Extent3d {
            width: texture.width,
            height: texture.height,
            depth_or_array_layers: 1,
        };
        let mut copiers = world.query::<&mut EmbeddedViewportImageCopier>();
        for mut copier in copiers.iter_mut(world) {
            *copier = EmbeddedViewportImageCopier::new(
                src_image.clone(),
                size,
                render_device.wgpu_device(),
            );
        }

        self.texture = texture;
        self.resize_warmup_frames = 4;
        true
    }

    pub fn set_input(&mut self, input: BevyViewportInput) {
        self.input = input;
    }

    pub fn render_next(&mut self) -> Option<CapturedBevyFrame> {
        apply_embedded_viewport_input(self.app.world_mut(), self.input);
        self.app.update();

        let receiver = self.app.world().resource::<EmbeddedViewportReceiver>();
        let mut latest = None;
        while let Ok(frame) = receiver.try_recv() {
            if self.resize_warmup_frames > 0 {
                self.resize_warmup_frames -= 1;
                continue;
            }
            latest = Some(frame);
        }
        latest
    }

    fn scene_state(&mut self) -> Option<EmbeddedViewportSceneState> {
        let world = self.app.world_mut();
        let mut cameras = world.query::<&ChaseCamera>();
        let camera = cameras.iter(world).next()?;
        let selected = world.get_resource::<SelectedSwatch>().and_then(|selected| {
            let entity = selected.entity?;
            world
                .get::<ColorCube>(entity)
                .map(|cube| (cube.index, cube.egui_col))
        });
        let picked_color = world
            .get_resource::<SelectedSwatch>()
            .and_then(|selected| selected.picked_color);
        Some(EmbeddedViewportSceneState {
            focus: camera.focus,
            yaw: camera.yaw,
            elevation: camera.elevation,
            distance: camera.distance,
            selected_index: selected.map(|(index, _)| index),
            picked_color,
        })
    }

    fn apply_scene_state(&mut self, scene_state: EmbeddedViewportSceneState) {
        let world = self.app.world_mut();
        {
            let mut cameras = world.query::<(&mut ChaseCamera, &mut Transform)>();
            for (mut camera, mut transform) in cameras.iter_mut(world) {
                camera.focus = scene_state.focus;
                camera.yaw = scene_state.yaw;
                camera.elevation = scene_state.elevation;
                camera.distance = scene_state.distance;
                apply_rig(&camera, &mut transform);
            }
        }
        if let Some(index) = scene_state.selected_index {
            let mut cubes = world.query::<(Entity, &ColorCube)>();
            let selected = cubes
                .iter(world)
                .find_map(|(entity, cube)| (cube.index == index).then_some(entity));
            if let Some(mut selected_res) = world.get_resource_mut::<SelectedSwatch>() {
                selected_res.entity = selected;
                selected_res.picked_color = None;
            }
        }
    }
}

fn apply_embedded_viewport_input(world: &mut World, input: BevyViewportInput) {
    if let Some(mut selected) = world.get_resource_mut::<SelectedSwatch>() {
        selected.picked_color = None;
    }

    let drag = Vec2::new(input.drag_delta[0], input.drag_delta[1]);
    if drag != Vec2::ZERO || input.scroll_delta != 0.0 {
        let mut cameras = world.query::<(&mut ChaseCamera, &mut Transform)>();
        for (mut cam, mut tr) in cameras.iter_mut(world) {
            if drag != Vec2::ZERO {
                cam.yaw -= drag.x * cam.orbit_speed;
                cam.elevation += drag.y * cam.orbit_speed;
                cam.elevation = cam.elevation.clamp(cam.min_elevation, cam.max_elevation);
            }
            if input.scroll_delta != 0.0 {
                let log_distance = (cam.distance as f64).max(0.1).log10();
                let target = 10f64.powf(log_distance - input.scroll_delta as f64 * cam.zoom_step);
                cam.distance =
                    target.clamp(cam.min_distance as f64, cam.max_distance as f64) as f32;
            }
            apply_rig(&cam, &mut tr);
        }
    }

    if input.primary_clicked {
        let Some([x, y]) = input.pointer_pos else {
            return;
        };
        let mut cameras = world.query::<(&Camera, &GlobalTransform)>();
        let Ok((camera, cam_tr)) = cameras.single(world) else {
            return;
        };
        let Ok(ray) = camera.viewport_to_world(cam_tr, Vec2::new(x, y)) else {
            return;
        };
        let origin = ray.origin;
        let direction = *ray.direction;
        let mut cubes = world.query::<(Entity, &Transform, &ColorCube)>();
        let mut best: Option<(f32, Entity, egui::Color32)> = None;
        for (entity, tr, cube) in cubes.iter(world) {
            let min = tr.translation - Vec3::splat(0.5);
            let max = tr.translation + Vec3::splat(0.5);
            if let Some(t) = ray_aabb_hit(origin, direction, min, max) {
                match best {
                    Some((bt, _, _)) if bt <= t => {}
                    _ => best = Some((t, entity, cube.egui_col)),
                }
            }
        }
        if let Some((_, entity, color)) = best
            && let Some(mut selected) = world.get_resource_mut::<SelectedSwatch>()
        {
            selected.entity = Some(entity);
            selected.picked_color = Some(color);
        }
    }
}

#[derive(Resource)]
struct EmbeddedViewportConfig {
    texture: BevyViewportTexture,
}

#[derive(Component)]
struct ColorCube {
    index: usize,
    #[allow(dead_code)]
    egui_col: egui::Color32,
    base_color: Color,
}

#[derive(Resource, Default)]
struct SelectedSwatch {
    entity: Option<Entity>,
    picked_color: Option<egui::Color32>,
}

const PLANET_RADIUS: f32 = 6_371_000.0;
const CLOUD_ALTITUDE_M: f32 = 4_000.0;

fn setup_embedded_viewport_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    render_device: Res<RenderDevice>,
    config: Res<EmbeddedViewportConfig>,
) {
    let size = Extent3d {
        width: config.texture.width,
        height: config.texture.height,
        depth_or_array_layers: 1,
    };

    let mut render_target_image =
        Image::new_target_texture(size.width, size.height, TextureFormat::bevy_default(), None);
    render_target_image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let render_target_handle = images.add(render_target_image);

    commands.spawn(EmbeddedViewportImageCopier::new(
        render_target_handle.clone(),
        size,
        render_device.wgpu_device(),
    ));

    let planet_mesh = meshes.add(Sphere::new(PLANET_RADIUS).mesh().uv(1024, 512));
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
        NotShadowCaster,
        NotShadowReceiver,
    ));

    let shell_radius = PLANET_RADIUS + CLOUD_ALTITUDE_M;
    let cloud_mesh = meshes.add(Sphere::new(shell_radius).mesh().uv(64, 32));
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
            ColorCube {
                index: i,
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
            shadows_enabled: true,
            ..default()
        },
        sun_shadow,
    ));

    let projection = Projection::Perspective(PerspectiveProjection {
        near: 0.1,
        far: PLANET_RADIUS * 2.5,
        ..default()
    });
    let fog = DistanceFog {
        color: Color::srgb(0.10, 0.13, 0.20),
        falloff: FogFalloff::Atmospheric {
            extinction: Vec3::new(0.00008, 0.00012, 0.00020),
            inscattering: Vec3::new(0.00010, 0.00015, 0.00025),
        },
        ..default()
    };
    let chase = ChaseCamera::default();
    let mut cam_tr = Transform::default();
    apply_rig(&chase, &mut cam_tr);

    commands.spawn((
        Name::new("Camera"),
        Camera3d::default(),
        RenderTarget::from(render_target_handle),
        cam_tr,
        projection,
        fog,
        AmbientLight {
            color: Color::WHITE,
            brightness: 120.0,
            ..default()
        },
        chase,
    ));
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
        let is_sel = selected.entity == Some(entity);
        let target_y = if is_sel { LIFT_Y } else { REST_Y };
        tr.translation.y += (target_y - tr.translation.y) * k;
        if let Some(mat) = materials.get_mut(&mat_handle.0) {
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

#[derive(Resource)]
struct EmbeddedViewportReceiver(Receiver<CapturedBevyFrame>);

impl std::ops::Deref for EmbeddedViewportReceiver {
    type Target = Receiver<CapturedBevyFrame>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Resource)]
struct EmbeddedViewportSender(Sender<CapturedBevyFrame>);

impl std::ops::Deref for EmbeddedViewportSender {
    type Target = Sender<CapturedBevyFrame>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

struct EmbeddedViewportCopyPlugin;

impl Plugin for EmbeddedViewportCopyPlugin {
    fn build(&self, app: &mut App) {
        let (sender, receiver) = crossbeam_channel::unbounded();
        app.insert_resource(EmbeddedViewportReceiver(receiver));

        let render_app = app.sub_app_mut(RenderApp);
        render_app.insert_resource(EmbeddedViewportSender(sender));

        let mut graph = render_app.world_mut().resource_mut::<RenderGraph>();
        graph.add_node(EmbeddedViewportCopy, EmbeddedViewportCopyDriver);
        graph.add_node_edge(bevy::render::graph::CameraDriverLabel, EmbeddedViewportCopy);

        render_app
            .add_systems(ExtractSchedule, extract_embedded_viewport_copiers)
            .add_systems(
                Render,
                receive_embedded_viewport_frames.after(RenderSystems::Render),
            );
    }
}

#[derive(Clone, Default, Resource)]
struct EmbeddedViewportCopiers(Vec<EmbeddedViewportImageCopier>);

#[derive(Clone, Component)]
struct EmbeddedViewportImageCopier {
    buffer: Buffer,
    enabled: Arc<AtomicBool>,
    src_image: Handle<Image>,
}

impl EmbeddedViewportImageCopier {
    fn new(src_image: Handle<Image>, size: Extent3d, device: &wgpu::Device) -> Self {
        let row_bytes = size.width as usize * 4;
        let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(row_bytes);
        let buffer = device.create_buffer(&BufferDescriptor {
            label: Some("mara_embedded_bevy_viewport_readback"),
            size: padded_bytes_per_row as u64 * size.height as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            buffer: buffer.into(),
            enabled: Arc::new(AtomicBool::new(true)),
            src_image,
        }
    }

    fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

fn extract_embedded_viewport_copiers(
    mut commands: Commands,
    image_copiers: Extract<Query<&EmbeddedViewportImageCopier>>,
) {
    commands.insert_resource(EmbeddedViewportCopiers(
        image_copiers.iter().cloned().collect(),
    ));
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, RenderLabel)]
struct EmbeddedViewportCopy;

#[derive(Default)]
struct EmbeddedViewportCopyDriver;

impl render_graph::Node for EmbeddedViewportCopyDriver {
    fn run(
        &self,
        _graph: &mut RenderGraphContext,
        render_context: &mut RenderContext,
        world: &World,
    ) -> Result<(), NodeRunError> {
        let Some(image_copiers) = world.get_resource::<EmbeddedViewportCopiers>() else {
            return Ok(());
        };
        let Some(gpu_images) =
            world.get_resource::<RenderAssets<bevy::render::texture::GpuImage>>()
        else {
            return Ok(());
        };

        for image_copier in &image_copiers.0 {
            if !image_copier.enabled() {
                continue;
            }
            let Some(src_image) = gpu_images.get(&image_copier.src_image) else {
                continue;
            };

            let block_dimensions = src_image.texture_format.block_dimensions();
            let Some(block_size) = src_image.texture_format.block_copy_size(None) else {
                continue;
            };
            let padded_bytes_per_row = RenderDevice::align_copy_bytes_per_row(
                (src_image.size.width as usize / block_dimensions.0 as usize) * block_size as usize,
            );

            let mut encoder =
                render_context
                    .render_device()
                    .create_command_encoder(&CommandEncoderDescriptor {
                        label: Some("mara_embedded_bevy_viewport_copy_encoder"),
                    });
            encoder.copy_texture_to_buffer(
                src_image.texture.as_image_copy(),
                TexelCopyBufferInfo {
                    buffer: &image_copier.buffer,
                    layout: TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(
                            std::num::NonZero::<u32>::new(padded_bytes_per_row as u32)
                                .expect("non-zero row bytes")
                                .into(),
                        ),
                        rows_per_image: None,
                    },
                },
                src_image.size,
            );

            let Some(render_queue) = world.get_resource::<RenderQueue>() else {
                continue;
            };
            render_queue.submit(std::iter::once(encoder.finish()));
        }

        Ok(())
    }
}

fn receive_embedded_viewport_frames(
    image_copiers: Res<EmbeddedViewportCopiers>,
    render_device: Res<RenderDevice>,
    sender: Res<EmbeddedViewportSender>,
    gpu_images: Res<RenderAssets<bevy::render::texture::GpuImage>>,
) {
    for image_copier in &image_copiers.0 {
        if !image_copier.enabled() {
            continue;
        }
        let Some(src_image) = gpu_images.get(&image_copier.src_image) else {
            continue;
        };
        let Ok(pixel_size) = src_image.texture_format.pixel_size() else {
            continue;
        };
        let width = src_image.size.width;
        let height = src_image.size.height;
        let row_bytes = width as usize * pixel_size;
        let padded_row_bytes = RenderDevice::align_copy_bytes_per_row(row_bytes);

        let buffer_slice = image_copier.buffer.slice(..);
        let (map_sender, map_receiver) = crossbeam_channel::bounded(1);
        buffer_slice.map_async(MapMode::Read, move |result| {
            let _ = map_sender.send(result);
        });
        if render_device
            .poll(PollType::Wait {
                submission_index: None,
                timeout: Some(Duration::from_millis(2)),
            })
            .is_err()
        {
            image_copier.buffer.unmap();
            continue;
        }
        if !matches!(map_receiver.try_recv(), Ok(Ok(()))) {
            image_copier.buffer.unmap();
            continue;
        }

        let mapped = buffer_slice.get_mapped_range();
        let rgba = if row_bytes == padded_row_bytes {
            mapped[..row_bytes * height as usize].to_vec()
        } else {
            mapped
                .chunks(padded_row_bytes)
                .take(height as usize)
                .flat_map(|row| row[..row_bytes.min(row.len())].iter().copied())
                .collect()
        };
        drop(mapped);
        image_copier.buffer.unmap();

        let _ = sender.send(CapturedBevyFrame {
            width,
            height,
            rgba,
            frame: 0,
        });
    }
}

/// Top-level embedded Bevy view model for egui-hosted apps.
#[derive(Default)]
pub struct BevyEmbeddedView {
    bridge: BevyViewportBridge,
}

impl BevyEmbeddedView {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_wgpu_resources(resources: BevyViewportWgpuResources) -> Self {
        Self {
            bridge: BevyViewportBridge::with_wgpu_resources(
                BevyViewportTexture::new(512, 512),
                resources,
            ),
        }
    }

    pub fn bridge(&self) -> &BevyViewportBridge {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut BevyViewportBridge {
        &mut self.bridge
    }

    pub fn render_frame(
        &mut self,
        width: u32,
        height: u32,
        dt_seconds: f32,
    ) -> Option<&CapturedBevyFrame> {
        self.bridge.render_frame(width, height, dt_seconds)
    }

    pub fn render_frame_with_input(
        &mut self,
        width: u32,
        height: u32,
        dt_seconds: f32,
        input: BevyViewportInput,
    ) -> Option<&CapturedBevyFrame> {
        self.bridge
            .render_frame_with_input(width, height, dt_seconds, input)
    }

    pub fn renderer_failed(&self) -> bool {
        self.bridge.renderer_failed()
    }

    pub fn picked_swatch_color(&self) -> Option<egui::Color32> {
        self.bridge.picked_swatch_color()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_view_ticks_bevy_world_without_window() {
        let mut view = BevyEmbeddedView::new();
        assert_eq!(view.bridge().frame(), 0);
        view.bridge_mut().tick(1.0 / 60.0);
        assert_eq!(view.bridge().frame(), 1);
        assert!(view.bridge().rotation_angle().is_finite());
    }

    #[test]
    fn viewport_texture_clamps_to_non_zero_size() {
        let texture = BevyViewportTexture::new(0, 0);
        assert_eq!(texture.width, 1);
        assert_eq!(texture.height, 1);
    }

    #[test]
    fn render_target_image_has_camera_and_sampling_usage() {
        let bridge = BevyViewportBridge::new(BevyViewportTexture::new(64, 32));
        let image = bridge.render_target_image(TextureFormat::Bgra8UnormSrgb);
        assert_eq!(image.texture_descriptor.size.width, 64);
        assert_eq!(image.texture_descriptor.size.height, 32);
        assert!(
            image
                .texture_descriptor
                .usage
                .contains(TextureUsages::RENDER_ATTACHMENT)
        );
        assert!(
            image
                .texture_descriptor
                .usage
                .contains(TextureUsages::TEXTURE_BINDING)
        );
    }

    #[test]
    fn viewport_camera_targets_image_not_window() {
        let mut world = World::new();
        let handle = Handle::<Image>::default();
        let camera = spawn_viewport_camera(&mut world, handle.clone());
        let target = world
            .get::<RenderTarget>(camera)
            .expect("render target component");
        assert_eq!(target.as_image(), Some(&handle));
    }

    #[test]
    #[ignore = "requires a native wgpu adapter; run manually when validating the embedded viewport"]
    fn headless_renderer_produces_rgba_frame() {
        let mut bridge = BevyViewportBridge::new(BevyViewportTexture::new(96, 64));
        let mut captured = None;
        for _ in 0..120 {
            if let Some(frame) = bridge.render_frame(96, 64, 1.0 / 60.0) {
                captured = Some(frame.clone());
                break;
            }
            if bridge.renderer_failed() {
                eprintln!("skipping embedded renderer smoke test: no native wgpu adapter");
                return;
            }
        }

        let frame = captured.expect("headless renderer produced a frame");
        assert_eq!(frame.width, 96);
        assert_eq!(frame.height, 64);
        assert_eq!(frame.rgba.len(), 96 * 64 * 4);
        assert!(frame.rgba.chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
