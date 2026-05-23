//! `mara_3d` — retained 3D scene/view module for Mara.
//!
//! This crate owns Mara's host-agnostic 3D document model and exposes
//! the [`three_d`] backend for renderer implementations. It deliberately
//! does **not** create a window or run an event loop; hosts decide whether
//! the scene is rendered inside Mara-owned chrome, a Bevy-owned app, web,
//! or another integration.

use mara_core::{
    MaraModule, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAction, RibbonCluster, RibbonEdge,
    RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    ViewCtx, ViewId, WorkspaceBar, WorkspaceBarCluster, WorkspaceBarEdge, WorkspaceCtx,
};

const WORLD_UP: Vec3 = [0.0, 1.0, 0.0];
const GLACIAL_LEVEL_SCALE: f32 = 4.0;
const GLACIAL_LEVELS: usize = 4;
const GLACIAL_MAJOR_EVERY: i32 = GLACIAL_LEVEL_SCALE as i32;
const GLACIAL_MAJOR_BOOST: f32 = 3.5;
const GLACIAL_GAUSS_PEAK: f32 = 0.602_06;
const GLACIAL_GAUSS_WIDTH: f32 = 0.55;
const GLACIAL_LINE_CLOSE_FALLOFF: f32 = 2.5;
const GLACIAL_DOT_CLOSE_FALLOFF: f32 = 6.0;
const GLACIAL_DOT_RADIUS_FRAC: f32 = 0.012;
const GRID_ACCENT_MIX: f32 = 0.28;
const GRID_BOUND_SAMPLES: usize = 5;
const GRID_STABLE_SPAN_DISTANCE: f32 = 48.0;
const GRID_STABLE_SPAN_SPACING: f32 = 128.0;
const GRID_MAX_LINES_PER_AXIS: i32 = 900;
const GRID_DOT_MAX_SCREEN_RADIUS: f32 = 3.5;
const GRID_DOT_MIN_SCREEN_RADIUS: f32 = 0.75;
const GRID_VIEW_ALIGNED_MIN_ALPHA: f32 = 0.22;
const GRID_VIEW_ALIGNED_FADE_START: f32 = 0.94;
const GRID_VIEW_ALIGNED_FADE_END: f32 = 0.995;
const GRID_DOT_CENTER_RAY_FADE_RADIUS: f32 = 0.55;
const OBJECT_EDGE_MAX_SCREEN_FRAC: f32 = 0.65;

/// Re-export of the renderer backend used by this module.
///
/// `three-d` is an OpenGL/WebGL/OpenGL ES renderer that can be used
/// without its optional window helper, so Mara can provide the app chrome
/// while backend integrations provide the actual GL/WebGL context.
pub use three_d as backend;

pub type Vec3 = [f32; 3];
pub type Quat = [f32; 4];
pub type Color = egui::Color32;

/// Stable object id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

/// Stable light id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightId(pub u64);

/// Stable material id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u64);

/// Camera definition independent from any renderer/window implementation.
#[derive(Clone, Debug, PartialEq)]
pub struct Camera3d {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub vertical_fov_degrees: f32,
    pub near: f32,
    pub far: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self {
            eye: [3.5, 3.0, 4.5],
            target: [0.0, 0.0, 0.0],
            up: [0.0, 1.0, 0.0],
            vertical_fov_degrees: 45.0,
            near: 0.01,
            far: 10_000.0,
        }
    }
}

/// Object transform in scene-local coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct Transform3d {
    pub translation: Vec3,
    pub rotation_xyzw: Quat,
    pub scale: Vec3,
}

impl Default for Transform3d {
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

/// Retained primitive types that the first `three-d` backend should be
/// able to translate directly into meshes.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive3d {
    Cube {
        size: f32,
    },
    Sphere {
        radius: f32,
        segments: u32,
    },
    Cylinder {
        radius: f32,
        height: f32,
        segments: u32,
    },
    Axes {
        length: f32,
    },
    Grid {
        half_extent: u32,
        step: f32,
    },
}

impl Default for Primitive3d {
    fn default() -> Self {
        Self::Cube { size: 1.0 }
    }
}

impl Primitive3d {
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Cube { .. } => "cube",
            Self::Sphere { .. } => "sphere",
            Self::Cylinder { .. } => "cylinder",
            Self::Axes { .. } => "axes",
            Self::Grid { .. } => "grid",
        }
    }
}

/// Minimal material definition for the retained scene model.
#[derive(Clone, Debug, PartialEq)]
pub struct Material3d {
    pub id: MaterialId,
    pub name: String,
    pub base_color: Color,
    pub roughness: f32,
    pub metallic: f32,
}

impl Material3d {
    #[must_use]
    pub fn new(id: MaterialId, name: impl Into<String>, base_color: Color) -> Self {
        Self {
            id,
            name: name.into(),
            base_color,
            roughness: 0.65,
            metallic: 0.0,
        }
    }
}

/// A retained object in a 3D scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Object3d {
    pub id: ObjectId,
    pub name: String,
    pub primitive: Primitive3d,
    pub transform: Transform3d,
    pub material: MaterialId,
    pub selected: bool,
    pub visible: bool,
}

impl Object3d {
    #[must_use]
    pub fn new(
        id: ObjectId,
        name: impl Into<String>,
        primitive: Primitive3d,
        material: MaterialId,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            primitive,
            transform: Transform3d::default(),
            material,
            selected: false,
            visible: true,
        }
    }

    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        self.primitive.kind_name()
    }
}

/// A retained light definition for renderer backends.
#[derive(Clone, Debug, PartialEq)]
pub enum Light3d {
    Ambient {
        id: LightId,
        name: String,
        color: Color,
        intensity: f32,
    },
    Directional {
        id: LightId,
        name: String,
        direction: Vec3,
        color: Color,
        intensity: f32,
    },
    Point {
        id: LightId,
        name: String,
        position: Vec3,
        color: Color,
        intensity: f32,
        radius: f32,
    },
}

impl Light3d {
    #[must_use]
    pub fn id(&self) -> LightId {
        match self {
            Self::Ambient { id, .. } | Self::Directional { id, .. } | Self::Point { id, .. } => *id,
        }
    }
}

/// Retained 3D scene/document state.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene3d {
    pub title: String,
    pub camera: Camera3d,
    pub background: Color,
    pub materials: Vec<Material3d>,
    pub objects: Vec<Object3d>,
    pub lights: Vec<Light3d>,
    next_object_id: u64,
    next_material_id: u64,
    next_light_id: u64,
}

impl Scene3d {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        let accent = mara_core::style::active_accent();
        Self {
            title: title.into(),
            camera: Camera3d::default(),
            background: mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent),
            materials: vec![Material3d::new(MaterialId(1), "Accent", accent)],
            objects: Vec::new(),
            lights: vec![
                Light3d::Ambient {
                    id: LightId(1),
                    name: "Ambient".to_owned(),
                    color: egui::Color32::WHITE,
                    intensity: 0.35,
                },
                Light3d::Directional {
                    id: LightId(2),
                    name: "Key".to_owned(),
                    direction: [-0.6, -1.0, -0.45],
                    color: egui::Color32::WHITE,
                    intensity: 1.4,
                },
            ],
            next_object_id: 1,
            next_material_id: 2,
            next_light_id: 3,
        }
    }

    #[must_use]
    pub fn demo(title: impl Into<String>) -> Self {
        let mut scene = Self::new(title);
        let washed = scene.add_material(
            "Washed accent",
            tint_color(
                mara_core::style::active_accent(),
                egui::Color32::WHITE,
                0.36,
            ),
        );
        let cube = scene.add_object("Cube", Primitive3d::Cube { size: 1.0 }, MaterialId(1));
        if let Some(object) = scene.object_mut(cube) {
            object.transform.translation = [-0.75, 0.55, 0.0];
            object.selected = true;
        }
        let small = scene.add_object("Small cube", Primitive3d::Cube { size: 0.65 }, washed);
        if let Some(object) = scene.object_mut(small) {
            object.transform.translation = [0.85, 0.35, -0.35];
        }
        let sphere = scene.add_object(
            "Sphere",
            Primitive3d::Sphere {
                radius: 0.38,
                segments: 32,
            },
            washed,
        );
        if let Some(object) = scene.object_mut(sphere) {
            object.transform.translation = [0.35, 0.38, 0.85];
        }
        let _ = scene.add_object(
            "Ground grid",
            Primitive3d::Grid {
                half_extent: 10,
                step: 1.0,
            },
            MaterialId(1),
        );
        scene
    }

    #[must_use]
    pub fn add_material(&mut self, name: impl Into<String>, base_color: Color) -> MaterialId {
        let id = MaterialId(self.next_material_id);
        self.next_material_id += 1;
        self.materials.push(Material3d::new(id, name, base_color));
        id
    }

    #[must_use]
    pub fn add_object(
        &mut self,
        name: impl Into<String>,
        primitive: Primitive3d,
        material: MaterialId,
    ) -> ObjectId {
        let id = ObjectId(self.next_object_id);
        self.next_object_id += 1;
        self.objects
            .push(Object3d::new(id, name, primitive, material));
        id
    }

    #[must_use]
    pub fn add_light(&mut self, light: Light3d) -> LightId {
        let id = light.id();
        self.next_light_id = self.next_light_id.max(id.0 + 1);
        self.lights.push(light);
        id
    }

    #[must_use]
    pub fn object(&self, id: ObjectId) -> Option<&Object3d> {
        self.objects.iter().find(|object| object.id == id)
    }

    #[must_use]
    pub fn object_mut(&mut self, id: ObjectId) -> Option<&mut Object3d> {
        self.objects.iter_mut().find(|object| object.id == id)
    }

    #[must_use]
    pub fn selected_object(&self) -> Option<&Object3d> {
        self.objects.iter().find(|object| object.selected)
    }

    #[must_use]
    pub fn selected_object_mut(&mut self) -> Option<&mut Object3d> {
        self.objects.iter_mut().find(|object| object.selected)
    }

    pub fn select_only(&mut self, id: Option<ObjectId>) {
        for object in &mut self.objects {
            object.selected = Some(object.id) == id;
        }
    }

    pub fn remove_object(&mut self, id: ObjectId) -> Option<Object3d> {
        let index = self.objects.iter().position(|object| object.id == id)?;
        Some(self.objects.remove(index))
    }

    pub fn remove_selected_object(&mut self) -> Option<Object3d> {
        let id = self.selected_object()?.id;
        self.remove_object(id)
    }
}

/// Renderer-facing viewport/input snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct Viewport3d {
    pub pixels: [u32; 2],
    pub scale_factor: f32,
    pub hovered: bool,
    pub pointer_pos: Option<egui::Pos2>,
    pub primary_down: bool,
    pub middle_down: bool,
    pub scroll_delta: egui::Vec2,
}

impl Viewport3d {
    #[must_use]
    pub fn from_response(response: &egui::Response, pixels: [u32; 2], ui: &egui::Ui) -> Self {
        let input = ui.input(|input| {
            (
                input.pointer.primary_down(),
                input.pointer.middle_down(),
                input.smooth_scroll_delta,
            )
        });
        Self {
            pixels,
            scale_factor: ui.ctx().pixels_per_point(),
            hovered: response.hovered(),
            pointer_pos: response.hover_pos(),
            primary_down: input.0,
            middle_down: input.1,
            scroll_delta: input.2,
        }
    }
}

/// Backend contract for concrete renderers built on top of `three-d`.
pub trait Renderer3d {
    type Error;

    fn resize(&mut self, viewport: &Viewport3d) -> Result<(), Self::Error>;

    fn render(&mut self, scene: &Scene3d, viewport: &Viewport3d) -> Result<(), Self::Error>;

    fn pick(
        &mut self,
        _scene: &Scene3d,
        _viewport: &Viewport3d,
        _position: egui::Pos2,
    ) -> Result<Option<ObjectId>, Self::Error> {
        Ok(None)
    }
}

/// Simple orbit state used by the egui preview and reusable by renderer hosts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Orbit3d {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
}

impl Default for Orbit3d {
    fn default() -> Self {
        Self {
            target: [0.0, 0.55, 0.0],
            yaw: 0.78,
            pitch: 0.48,
            distance: 5.25,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PreviewCamera {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    fov_y: f32,
    near: f32,
}

impl PreviewCamera {
    fn from_orbit(orbit: Orbit3d, scene_camera: &Camera3d) -> Self {
        let cp = orbit.pitch.cos();
        let offset = [
            orbit.distance * cp * orbit.yaw.sin(),
            orbit.distance * orbit.pitch.sin(),
            orbit.distance * cp * orbit.yaw.cos(),
        ];
        let eye = add3(orbit.target, offset);
        let forward = normalize3(sub3(orbit.target, eye));
        let right = normalize3(cross3(forward, scene_camera.up));
        let up = normalize3(cross3(right, forward));
        Self {
            eye,
            forward,
            right,
            up,
            fov_y: scene_camera.vertical_fov_degrees.to_radians(),
            near: scene_camera.near.max(0.001),
        }
    }

    fn project(&self, rect: egui::Rect, point: Vec3) -> Option<(egui::Pos2, f32)> {
        let (x, y, z) = self.camera_space(point);
        if z <= self.near {
            return None;
        }
        let focal = 0.5 * rect.height() / (self.fov_y * 0.5).tan();
        let screen = egui::pos2(
            rect.center().x + x * focal / z,
            rect.center().y - y * focal / z,
        );
        if screen.x.is_finite() && screen.y.is_finite() {
            Some((screen, z))
        } else {
            None
        }
    }

    fn camera_space(&self, point: Vec3) -> (f32, f32, f32) {
        let rel = sub3(point, self.eye);
        (
            dot3(rel, self.right),
            dot3(rel, self.up),
            dot3(rel, self.forward),
        )
    }

    fn project_line_clipped(
        &self,
        rect: egui::Rect,
        mut a: Vec3,
        mut b: Vec3,
    ) -> Option<(egui::Pos2, egui::Pos2)> {
        let za = self.camera_space(a).2;
        let zb = self.camera_space(b).2;
        if za <= self.near && zb <= self.near {
            return None;
        }
        if (za <= self.near || zb <= self.near) && (zb - za).abs() > f32::EPSILON {
            let t = ((self.near - za) / (zb - za)).clamp(0.0, 1.0);
            if za <= self.near {
                a = lerp3(a, b, t);
            } else {
                b = lerp3(a, b, t);
            }
        }
        Some((self.project(rect, a)?.0, self.project(rect, b)?.0))
    }

    fn project_line_with_near(
        &self,
        rect: egui::Rect,
        mut a: Vec3,
        mut b: Vec3,
        near: f32,
    ) -> Option<(egui::Pos2, egui::Pos2)> {
        let za = self.camera_space(a).2;
        let zb = self.camera_space(b).2;
        if za <= near && zb <= near {
            return None;
        }
        if (za <= near || zb <= near) && (zb - za).abs() > f32::EPSILON {
            let t = ((near - za) / (zb - za)).clamp(0.0, 1.0);
            if za <= near {
                a = lerp3(a, b, t);
            } else {
                b = lerp3(a, b, t);
            }
        }
        let projected_a = self.project(rect, a)?.0;
        let projected_b = self.project(rect, b)?.0;
        if projected_a.is_finite() && projected_b.is_finite() {
            Some((projected_a, projected_b))
        } else {
            None
        }
    }

    fn ray_to_plane_y0(&self, rect: egui::Rect, pos: egui::Pos2) -> Option<Vec3> {
        let direction = self.ray_direction(rect, pos);
        if direction[1].abs() <= 1.0e-5 {
            return None;
        }
        let t = -self.eye[1] / direction[1];
        if t.is_finite() && t > 0.0 {
            Some(add3(self.eye, mul3(direction, t)))
        } else {
            None
        }
    }

    fn ray_direction(&self, rect: egui::Rect, pos: egui::Pos2) -> Vec3 {
        let focal = 0.5 * rect.height() / (self.fov_y * 0.5).tan();
        let sx = (pos.x - rect.center().x) / focal;
        let sy = -(pos.y - rect.center().y) / focal;
        normalize3(add3(
            self.forward,
            add3(mul3(self.right, sx), mul3(self.up, sy)),
        ))
    }
}

/// A Mara surface for a retained 3D scene.
#[derive(Clone, Debug)]
pub struct View3d {
    id: egui::Id,
    scene: Scene3d,
    orbit: Orbit3d,
}

impl View3d {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, scene: Scene3d) -> Self {
        Self {
            id: egui::Id::new(id),
            scene,
            orbit: Orbit3d::default(),
        }
    }

    #[must_use]
    pub fn scene(&self) -> &Scene3d {
        &self.scene
    }

    #[must_use]
    pub fn scene_mut(&mut self) -> &mut Scene3d {
        &mut self.scene
    }

    #[must_use]
    pub fn orbit(&self) -> Orbit3d {
        self.orbit
    }

    pub fn orbit_mut(&mut self) -> &mut Orbit3d {
        &mut self.orbit
    }

    /// Reserve the 3D viewport rectangle and return a host/backend input snapshot.
    ///
    /// This is intentionally only allocation/input plumbing. Actual `three-d`
    /// rendering must happen in a backend that owns or receives a GL/WebGL
    /// context for this rectangle.
    pub fn allocate_viewport(&mut self, ui: &mut egui::Ui) -> (egui::Response, Viewport3d) {
        let available = ui.available_size_before_wrap();
        let size = egui::vec2(available.x.max(180.0), available.y.max(140.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let pixels = [
            (rect.width() * ppp).round().max(1.0) as u32,
            (rect.height() * ppp).round().max(1.0) as u32,
        ];
        let viewport = Viewport3d::from_response(&response, pixels, ui);
        (response, viewport)
    }

    fn view_ribbon(&self, scope: RibbonScope) -> RibbonSlotDef {
        let orbit = RibbonSlotItem::new(
            egui::Id::new(("three_d.orbit", self.id)),
            "orbit",
            "Orbit",
            "Orbit camera",
            RibbonAction::Command(egui::Id::new(("three_d.orbit.command", self.id))),
        );
        let fit = RibbonSlotItem::new(
            egui::Id::new(("three_d.fit", self.id)),
            "fit",
            "Fit",
            "Frame selected objects",
            RibbonAction::Command(egui::Id::new(("three_d.fit.command", self.id))),
        );
        RibbonSlotDef::new(
            egui::Id::new(("three_d.ribbon", self.id)),
            scope,
            RibbonEdge::Top,
            RibbonCluster::Middle,
            vec![
                RibbonSlot::new(
                    RibbonSlotId::new(("three_d.orbit.slot", self.id)),
                    Some(orbit),
                    RibbonOverridePolicy::Fixed,
                ),
                RibbonSlot::new(
                    RibbonSlotId::new(("three_d.fit.slot", self.id)),
                    Some(fit),
                    RibbonOverridePolicy::LayerOverride,
                ),
            ],
        )
    }

    fn paint_preview(&mut self, ui: &mut egui::Ui, rect: egui::Rect, response: &egui::Response) {
        self.update_orbit(ui, response);
        self.update_selection(ui, rect, response);

        let painter = ui.painter_at(rect);
        let accent = mara_core::style::active_accent();
        let background = mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent);
        painter.rect_filled(rect, 0.0, background);

        let camera = PreviewCamera::from_orbit(self.orbit, &self.scene.camera);
        let mut faces = Vec::new();
        let mut edges = Vec::new();

        for object in &self.scene.objects {
            if !object.visible {
                continue;
            }
            match object.primitive {
                Primitive3d::Grid { step, .. } => {
                    self.paint_grid(&painter, rect, &camera, step, accent);
                }
                Primitive3d::Axes { length } => {
                    self.paint_axes(&painter, rect, &camera, length);
                }
                Primitive3d::Cube { size } => {
                    self.collect_cube_faces_and_edges(
                        &mut faces, &mut edges, rect, &camera, object, size,
                    );
                }
                Primitive3d::Sphere { radius, .. } => {
                    self.paint_sphere(&painter, rect, &camera, object, radius);
                }
                Primitive3d::Cylinder { radius, height, .. } => {
                    self.paint_cylinder_hint(&painter, rect, &camera, object, radius, height);
                }
            }
        }

        faces.sort_by(|a, b: &PreviewFace| b.depth.total_cmp(&a.depth));
        for face in faces {
            painter.add(egui::Shape::convex_polygon(
                face.points,
                face.fill,
                egui::Stroke::new(face.stroke_width, face.stroke),
            ));
        }
        edges.sort_by(|a, b: &PreviewEdge| b.depth.total_cmp(&a.depth));
        for edge in edges {
            painter.line_segment(edge.points, egui::Stroke::new(edge.width, edge.color));
        }

        if response.hovered() || response.dragged() {
            ui.ctx().request_repaint();
        }
    }

    fn update_selection(&mut self, ui: &egui::Ui, rect: egui::Rect, response: &egui::Response) {
        if response.clicked_by(egui::PointerButton::Primary) {
            response.request_focus();
            let camera = PreviewCamera::from_orbit(self.orbit, &self.scene.camera);
            let picked = response
                .interact_pointer_pos()
                .and_then(|pos| self.pick_object(rect, &camera, pos));
            self.scene.select_only(picked);
        }

        if response.has_focus()
            && ui.input(|input| input.key_pressed(egui::Key::Delete))
            && self.scene.remove_selected_object().is_some()
        {
            ui.ctx().request_repaint();
        }
    }

    fn update_orbit(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|input| input.pointer.delta());
            self.orbit.yaw -= delta.x * 0.006;
            self.orbit.pitch = (self.orbit.pitch + delta.y * 0.006).clamp(0.08, 1.35);
            ui.ctx().request_repaint();
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let delta = ui.input(|input| input.pointer.delta());
            let camera = PreviewCamera::from_orbit(self.orbit, &self.scene.camera);
            let scale = (self.orbit.distance * 0.0018).max(0.001);
            self.orbit.target = add3(
                self.orbit.target,
                add3(
                    mul3(camera.right, -delta.x * scale),
                    mul3(camera.up, delta.y * scale),
                ),
            );
            ui.ctx().request_repaint();
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                self.orbit.distance =
                    (self.orbit.distance * (-scroll * 0.0015).exp()).clamp(1.0, 500.0);
                ui.ctx().request_repaint();
            }
        }
    }

    fn material_color(&self, material: MaterialId) -> egui::Color32 {
        self.scene
            .materials
            .iter()
            .find(|candidate| candidate.id == material)
            .map_or_else(mara_core::style::active_accent, |material| {
                material.base_color
            })
    }

    fn paint_grid(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        step: f32,
        accent: egui::Color32,
    ) {
        let base_step = step.max(0.001);
        let cam_dist = self.orbit.distance.max(0.1);
        let grid_color = grid_tint(accent);
        let mut grid_lines = std::collections::BTreeMap::<(u8, i32), GridLineSpec>::new();

        for level in 0..GLACIAL_LEVELS {
            let spacing = base_step * GLACIAL_LEVEL_SCALE.powi(level as i32);
            let line_fade = glacial_level_fade(cam_dist, spacing, GLACIAL_LINE_CLOSE_FALLOFF);
            let dot_fade = glacial_level_fade(cam_dist, spacing, GLACIAL_DOT_CLOSE_FALLOFF);
            if line_fade <= 0.005 && dot_fade <= 0.005 {
                continue;
            }

            let bounds = grid_visible_bounds(rect, camera, spacing, self.orbit.distance);
            let min_x = (bounds[0] / spacing).floor() as i32;
            let max_x = (bounds[1] / spacing).ceil() as i32;
            let min_z = (bounds[2] / spacing).floor() as i32;
            let max_z = (bounds[3] / spacing).ceil() as i32;

            if line_fade > 0.005 {
                for z in min_z..=max_z {
                    let pos = z as f32 * spacing;
                    let major = z.rem_euclid(GLACIAL_MAJOR_EVERY) == 0;
                    let alignment_fade = view_aligned_line_fade(camera, [1.0, 0.0, 0.0]);
                    let alpha = (58.0
                        * line_fade
                        * alignment_fade
                        * if major { GLACIAL_MAJOR_BOOST } else { 1.0 })
                    .min(160.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    merge_grid_line(
                        &mut grid_lines,
                        GridLineSpec {
                            axis: 0,
                            base_index: (pos / base_step).round() as i32,
                            constant: pos,
                            min: min_x as f32 * spacing,
                            max: max_x as f32 * spacing,
                            alpha,
                            width: if major { 1.35 } else { 0.75 },
                        },
                    );
                }

                for x in min_x..=max_x {
                    let pos = x as f32 * spacing;
                    let major = x.rem_euclid(GLACIAL_MAJOR_EVERY) == 0;
                    let alignment_fade = view_aligned_line_fade(camera, [0.0, 0.0, 1.0]);
                    let alpha = (58.0
                        * line_fade
                        * alignment_fade
                        * if major { GLACIAL_MAJOR_BOOST } else { 1.0 })
                    .min(160.0);
                    if alpha <= 0.0 {
                        continue;
                    }
                    merge_grid_line(
                        &mut grid_lines,
                        GridLineSpec {
                            axis: 1,
                            base_index: (pos / base_step).round() as i32,
                            constant: pos,
                            min: min_z as f32 * spacing,
                            max: max_z as f32 * spacing,
                            alpha,
                            width: if major { 1.35 } else { 0.75 },
                        },
                    );
                }
            }

            if dot_fade > 0.005 {
                self.paint_grid_dots(
                    painter, rect, camera, spacing, min_x, max_x, min_z, max_z, grid_color,
                    dot_fade,
                );
            }
        }

        for line in grid_lines.values() {
            let alpha = line.alpha.round().clamp(0.0, 160.0) as u8;
            if alpha == 0 {
                continue;
            }
            let stroke = egui::Stroke::new(
                line.width,
                egui::Color32::from_rgba_unmultiplied(
                    grid_color.r(),
                    grid_color.g(),
                    grid_color.b(),
                    alpha,
                ),
            );
            if line.axis == 0 {
                self.paint_grid_line(
                    painter,
                    rect,
                    camera,
                    [line.min, 0.0, line.constant],
                    [line.max, 0.0, line.constant],
                    stroke,
                );
            } else {
                self.paint_grid_line(
                    painter,
                    rect,
                    camera,
                    [line.constant, 0.0, line.min],
                    [line.constant, 0.0, line.max],
                    stroke,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_grid_dots(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        spacing: f32,
        min_x: i32,
        max_x: i32,
        min_z: i32,
        max_z: i32,
        color: egui::Color32,
        fade: f32,
    ) {
        let stride = (self.orbit.distance / (spacing * 18.0)).ceil().max(1.0) as i32;
        let stable_near = camera.near.max(self.orbit.distance * 0.015);
        let alpha = (150.0 * fade).min(180.0) as u8;
        if alpha == 0 {
            return;
        }

        for x in (min_x..=max_x).step_by(stride as usize) {
            for z in (min_z..=max_z).step_by(stride as usize) {
                let point = [x as f32 * spacing, 0.002, z as f32 * spacing];
                if camera.camera_space(point).2 <= stable_near {
                    continue;
                }
                let Some((screen, _)) = camera.project(rect, point) else {
                    continue;
                };
                if !rect.expand(8.0).contains(screen) {
                    continue;
                }
                let base_world_radius = spacing * GLACIAL_DOT_RADIUS_FRAC * stride as f32;
                let edge = add3(point, mul3(camera.right, base_world_radius));
                let radius = camera
                    .project(rect, edge)
                    .map_or(1.0, |(edge, _)| edge.distance(screen))
                    .clamp(GRID_DOT_MIN_SCREEN_RADIUS, GRID_DOT_MAX_SCREEN_RADIUS);
                let point_alpha = (alpha as f32 * center_ray_dot_fade(camera, point, spacing))
                    .round()
                    .clamp(0.0, 255.0) as u8;
                if point_alpha == 0 {
                    continue;
                }
                painter.circle_filled(
                    screen,
                    radius,
                    egui::Color32::from_rgba_unmultiplied(
                        color.r(),
                        color.g(),
                        color.b(),
                        point_alpha,
                    ),
                );
            }
        }
    }

    fn paint_axes(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        length: f32,
    ) {
        self.paint_projected_line(
            painter,
            rect,
            camera,
            [0.0, 0.015, 0.0],
            [length, 0.015, 0.0],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(230, 76, 76)),
        );
        self.paint_projected_line(
            painter,
            rect,
            camera,
            [0.0, 0.015, 0.0],
            [0.0, length, 0.0],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(89, 217, 115)),
        );
        self.paint_projected_line(
            painter,
            rect,
            camera,
            [0.0, 0.015, 0.0],
            [0.0, 0.015, length],
            egui::Stroke::new(2.0, egui::Color32::from_rgb(76, 153, 242)),
        );
    }

    fn paint_projected_line(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        a: Vec3,
        b: Vec3,
        stroke: egui::Stroke,
    ) {
        if let Some((a, b)) = camera
            .project_line_clipped(rect, a, b)
            .and_then(|(a, b)| clip_screen_segment(rect.expand(2.0), a, b))
            .filter(|(a, b)| a.distance(*b) >= 0.5)
        {
            painter.line_segment([a, b], stroke);
        }
    }

    fn paint_grid_line(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        a: Vec3,
        b: Vec3,
        stroke: egui::Stroke,
    ) {
        let stable_near = camera.near.max(self.orbit.distance * 0.015);
        if let Some((a, b)) = camera
            .project_line_with_near(rect, a, b, stable_near)
            .and_then(|(a, b)| clip_screen_segment(rect.expand(2.0), a, b))
            .filter(|(a, b)| a.distance(*b) >= 0.5)
        {
            painter.line_segment([a, b], stroke);
        }
    }

    fn collect_cube_faces_and_edges(
        &self,
        faces: &mut Vec<PreviewFace>,
        edges: &mut Vec<PreviewEdge>,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
        size: f32,
    ) {
        let h = size * 0.5;
        let local = [
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
        ];
        let world = local.map(|point| transform_point(&object.transform, point));
        const FACE_INDICES: [[usize; 4]; 6] = [
            [0, 1, 2, 3],
            [4, 7, 6, 5],
            [0, 4, 5, 1],
            [3, 2, 6, 7],
            [1, 5, 6, 2],
            [0, 3, 7, 4],
        ];
        const EDGE_INDICES: [(usize, usize); 12] = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];

        let base = self.material_color(object.material);
        for indices in FACE_INDICES {
            let mut points = Vec::with_capacity(4);
            let mut depth = 0.0;
            let mut visible = true;
            for index in indices {
                let Some((point, z)) = camera.project(rect, world[index]) else {
                    visible = false;
                    break;
                };
                points.push(point);
                depth += z;
            }
            if !visible {
                continue;
            }
            depth /= 4.0;
            faces.push(PreviewFace {
                depth,
                points,
                fill: base,
                stroke: tint_color(mara_core::style::on_panel_dim(), base, 0.35),
                stroke_width: 0.0,
            });
        }

        let edge_color = if object.selected {
            mara_core::style::active_accent()
        } else {
            tint_color(mara_core::style::on_panel(), base, 0.35)
        };
        let selected = object.selected;
        let stable_near = camera.near.max(self.orbit.distance * 0.015);
        for (a, b) in EDGE_INDICES {
            let Some((pa, pb)) = camera
                .project_line_with_near(rect, world[a], world[b], stable_near)
                .and_then(|(pa, pb)| clip_screen_segment(rect.expand(2.0), pa, pb))
                .filter(|(pa, pb)| {
                    let length = pa.distance(*pb);
                    length >= 0.5 && length <= rect.size().length() * OBJECT_EDGE_MAX_SCREEN_FRAC
                })
            else {
                continue;
            };
            let da = camera.camera_space(world[a]).2.max(camera.near);
            let db = camera.camera_space(world[b]).2.max(camera.near);
            edges.push(PreviewEdge {
                depth: (da + db) * 0.5,
                points: [pa, pb],
                color: edge_color,
                width: if selected { 2.25 } else { 1.15 },
            });
        }
    }

    fn paint_sphere(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
        radius: f32,
    ) {
        let center = transform_point(&object.transform, [0.0, 0.0, 0.0]);
        let Some((screen, _depth)) = camera.project(rect, center) else {
            return;
        };
        let edge = add3(
            center,
            mul3(camera.right, radius * object.transform.scale[0].abs()),
        );
        let screen_radius = camera
            .project(rect, edge)
            .map_or(8.0, |(edge, _)| edge.distance(screen))
            .clamp(3.0, rect.width().min(rect.height()) * 0.25);
        let fill = self.material_color(object.material);
        painter.circle_filled(screen, screen_radius, fill);
        painter.circle_stroke(
            screen,
            screen_radius,
            egui::Stroke::new(
                if object.selected { 1.8 } else { 0.75 },
                if object.selected {
                    mara_core::style::active_accent()
                } else {
                    tint_color(mara_core::style::on_panel_dim(), fill, 0.25)
                },
            ),
        );
        painter.circle_filled(
            screen + egui::vec2(-screen_radius * 0.28, -screen_radius * 0.32),
            (screen_radius * 0.18).max(1.0),
            tint_color(fill, egui::Color32::WHITE, 0.28),
        );
    }

    fn paint_cylinder_hint(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
        radius: f32,
        height: f32,
    ) {
        let base = self.material_color(object.material);
        let bottom = transform_point(&object.transform, [0.0, -height * 0.5, 0.0]);
        let top = transform_point(&object.transform, [0.0, height * 0.5, 0.0]);
        if let (Some((bottom, _)), Some((top, _))) =
            (camera.project(rect, bottom), camera.project(rect, top))
        {
            let r = radius.max(0.05) * 16.0 / self.orbit.distance.max(0.5);
            painter.line_segment([bottom, top], egui::Stroke::new(r.max(2.0), base));
            painter.circle_stroke(bottom, r.max(3.0), egui::Stroke::new(1.0, base));
            painter.circle_stroke(top, r.max(3.0), egui::Stroke::new(1.0, base));
        }
    }

    fn pick_object(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        pos: egui::Pos2,
    ) -> Option<ObjectId> {
        let mut best: Option<(ObjectId, f32, f32)> = None;
        for object in &self.scene.objects {
            if !object.visible
                || matches!(
                    object.primitive,
                    Primitive3d::Grid { .. } | Primitive3d::Axes { .. }
                )
            {
                continue;
            }
            let Some((center, depth)) =
                camera.project(rect, transform_point(&object.transform, [0.0, 0.0, 0.0]))
            else {
                continue;
            };
            let radius = self.object_screen_radius(rect, camera, object).max(8.0);
            let distance = center.distance(pos);
            if distance <= radius {
                let score = depth + distance * 0.002;
                if best.is_none_or(|(_, best_score, _)| score < best_score) {
                    best = Some((object.id, score, distance));
                }
            }
        }
        best.map(|(id, _, _)| id)
    }

    fn object_screen_radius(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
    ) -> f32 {
        let center = transform_point(&object.transform, [0.0, 0.0, 0.0]);
        let local_radius = match object.primitive {
            Primitive3d::Cube { size } => size * 0.88,
            Primitive3d::Sphere { radius, .. } | Primitive3d::Cylinder { radius, .. } => radius,
            Primitive3d::Axes { length } => length,
            Primitive3d::Grid { half_extent, step } => half_extent as f32 * step,
        };
        let edge = add3(
            center,
            mul3(
                camera.right,
                local_radius
                    * object
                        .transform
                        .scale
                        .iter()
                        .copied()
                        .fold(0.0_f32, |acc, value| acc.max(value.abs())),
            ),
        );
        match (camera.project(rect, center), camera.project(rect, edge)) {
            (Some((center, _)), Some((edge, _))) => center.distance(edge),
            _ => 0.0,
        }
    }
}

impl MaraView for View3d {
    fn id(&self) -> ViewId {
        ViewId(self.id)
    }

    fn title(&self) -> &str {
        &self.scene.title
    }

    fn icon(&self) -> &'static str {
        "cube"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        vec![self.view_ribbon(RibbonScope::View(ViewId(self.id)))]
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::TRANSPARENT)
                    .inner_margin(0.0)
                    .outer_margin(0.0),
            )
            .show(ctx.egui_ctx, |ui| {
                let rect = ctx.content_rect().intersect(ui.max_rect());
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                self.paint_preview(ui, response.rect, &response);
            });
    }
}

impl MaraModule for View3d {
    fn id(&self) -> egui::Id {
        self.id
    }

    fn title(&self) -> &str {
        &self.scene.title
    }

    fn icon(&self) -> &'static str {
        "cube"
    }

    fn inline(&mut self, ui: &mut egui::Ui, ctx: ModuleInlineCtx<'_>) -> ModuleResponse {
        ui.group(|ui| {
            ui.label(format!("3D scene: {}", self.scene.title));
            ui.label(format!("{} objects", self.scene.objects.len()));
            if ctx.can_enter_workspace() && ui.button("Open 3D workspace").clicked() {
                ModuleResponse::enter_workspace()
            } else {
                ModuleResponse::none()
            }
        })
        .inner
    }

    fn workspace(&mut self, ws: &mut WorkspaceCtx<'_>) {
        ws.add_bar(WorkspaceBar::new(
            egui::Id::new(("three_d.workspace.bar", self.id)),
            WorkspaceBarEdge::Top,
            WorkspaceBarCluster::Middle,
        ));
        ws.add_ribbon(self.view_ribbon(RibbonScope::WorkspaceLevel(ws.level.id)));
    }
}

#[derive(Clone, Debug)]
struct PreviewFace {
    depth: f32,
    points: Vec<egui::Pos2>,
    fill: egui::Color32,
    stroke: egui::Color32,
    stroke_width: f32,
}

#[derive(Clone, Debug)]
struct PreviewEdge {
    depth: f32,
    points: [egui::Pos2; 2],
    color: egui::Color32,
    width: f32,
}

#[derive(Clone, Copy, Debug)]
struct GridLineSpec {
    axis: u8,
    base_index: i32,
    constant: f32,
    min: f32,
    max: f32,
    alpha: f32,
    width: f32,
}

fn merge_grid_line(
    lines: &mut std::collections::BTreeMap<(u8, i32), GridLineSpec>,
    incoming: GridLineSpec,
) {
    lines
        .entry((incoming.axis, incoming.base_index))
        .and_modify(|line| {
            line.min = line.min.min(incoming.min);
            line.max = line.max.max(incoming.max);
            if incoming.alpha > line.alpha {
                line.alpha = incoming.alpha;
                line.width = incoming.width;
                line.constant = incoming.constant;
            } else if (incoming.alpha - line.alpha).abs() <= f32::EPSILON {
                line.width = line.width.max(incoming.width);
            }
        })
        .or_insert(incoming);
}

fn transform_point(transform: &Transform3d, point: Vec3) -> Vec3 {
    add3(
        transform.translation,
        [
            point[0] * transform.scale[0],
            point[1] * transform.scale[1],
            point[2] * transform.scale[2],
        ],
    )
}

fn add3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn sub3(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn mul3(a: Vec3, scale: f32) -> Vec3 {
    [a[0] * scale, a[1] * scale, a[2] * scale]
}

fn lerp3(a: Vec3, b: Vec3, t: f32) -> Vec3 {
    add3(a, mul3(sub3(b, a), t))
}

fn dot3(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(value: Vec3) -> Vec3 {
    let length = dot3(value, value).sqrt();
    if length <= f32::EPSILON {
        WORLD_UP
    } else {
        mul3(value, length.recip())
    }
}

fn glacial_level_fade(cam_dist: f32, step: f32, close_falloff: f32) -> f32 {
    let log_ratio = (cam_dist / step).max(1.0e-3).log10();
    let z = (log_ratio - GLACIAL_GAUSS_PEAK) / GLACIAL_GAUSS_WIDTH;
    let adjusted = if z < 0.0 { z * close_falloff } else { z };
    (-0.5 * adjusted * adjusted).exp().clamp(0.0, 1.0)
}

fn grid_tint(accent: egui::Color32) -> egui::Color32 {
    tint_color(mara_core::style::on_panel_dim(), accent, GRID_ACCENT_MIX)
}

fn view_aligned_line_fade(camera: &PreviewCamera, line_dir: Vec3) -> f32 {
    let camera_flat = normalize3([camera.forward[0], 0.0, camera.forward[2]]);
    let line_flat = normalize3([line_dir[0], 0.0, line_dir[2]]);
    let alignment = dot3(camera_flat, line_flat).abs();
    if alignment <= GRID_VIEW_ALIGNED_FADE_START {
        return 1.0;
    }

    let t = ((alignment - GRID_VIEW_ALIGNED_FADE_START)
        / (GRID_VIEW_ALIGNED_FADE_END - GRID_VIEW_ALIGNED_FADE_START))
        .clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    1.0 - smooth * (1.0 - GRID_VIEW_ALIGNED_MIN_ALPHA)
}

fn center_ray_dot_fade(camera: &PreviewCamera, point: Vec3, spacing: f32) -> f32 {
    let eye = [camera.eye[0], 0.0, camera.eye[2]];
    let forward = normalize3([camera.forward[0], 0.0, camera.forward[2]]);
    let rel = sub3([point[0], 0.0, point[2]], eye);
    let along = dot3(rel, forward);
    if along <= 0.0 {
        return 1.0;
    }

    let closest = add3(eye, mul3(forward, along));
    let distance = dot3(
        sub3([point[0], 0.0, point[2]], closest),
        sub3([point[0], 0.0, point[2]], closest),
    )
    .sqrt();
    let radius = (spacing * GRID_DOT_CENTER_RAY_FADE_RADIUS).max(0.001);
    if distance >= radius {
        return 1.0;
    }

    let t = (distance / radius).clamp(0.0, 1.0);
    let smooth = t * t * (3.0 - 2.0 * t);
    GRID_VIEW_ALIGNED_MIN_ALPHA + smooth * (1.0 - GRID_VIEW_ALIGNED_MIN_ALPHA)
}

fn clip_screen_segment(
    rect: egui::Rect,
    a: egui::Pos2,
    b: egui::Pos2,
) -> Option<(egui::Pos2, egui::Pos2)> {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let mut t0 = 0.0;
    let mut t1 = 1.0;

    if !clip_param(-dx, a.x - rect.left(), &mut t0, &mut t1)
        || !clip_param(dx, rect.right() - a.x, &mut t0, &mut t1)
        || !clip_param(-dy, a.y - rect.top(), &mut t0, &mut t1)
        || !clip_param(dy, rect.bottom() - a.y, &mut t0, &mut t1)
    {
        return None;
    }

    Some((
        egui::pos2(a.x + dx * t0, a.y + dy * t0),
        egui::pos2(a.x + dx * t1, a.y + dy * t1),
    ))
}

fn clip_param(p: f32, q: f32, t0: &mut f32, t1: &mut f32) -> bool {
    if p.abs() <= f32::EPSILON {
        return q >= 0.0;
    }

    let r = q / p;
    if p < 0.0 {
        if r > *t1 {
            return false;
        }
        if r > *t0 {
            *t0 = r;
        }
    } else {
        if r < *t0 {
            return false;
        }
        if r < *t1 {
            *t1 = r;
        }
    }
    true
}

fn grid_visible_bounds(
    rect: egui::Rect,
    camera: &PreviewCamera,
    spacing: f32,
    orbit_distance: f32,
) -> [f32; 4] {
    let max_ray_distance = (orbit_distance * 250.0).max(spacing * 256.0);
    let focus = camera
        .ray_to_plane_y0(rect, rect.center())
        .unwrap_or_else(|| {
            [
                camera.eye[0] + camera.forward[0] * orbit_distance,
                0.0,
                camera.eye[2] + camera.forward[2] * orbit_distance,
            ]
        });
    let stable_span =
        (orbit_distance * GRID_STABLE_SPAN_DISTANCE).max(spacing * GRID_STABLE_SPAN_SPACING);
    let mut min_x = focus[0] - stable_span;
    let mut max_x = focus[0] + stable_span;
    let mut min_z = focus[2] - stable_span;
    let mut max_z = focus[2] + stable_span;

    let include_point =
        |point: Vec3, min_x: &mut f32, max_x: &mut f32, min_z: &mut f32, max_z: &mut f32| {
            *min_x = min_x.min(point[0]);
            *max_x = max_x.max(point[0]);
            *min_z = min_z.min(point[2]);
            *max_z = max_z.max(point[2]);
        };

    for yi in 0..GRID_BOUND_SAMPLES {
        let y = if GRID_BOUND_SAMPLES <= 1 {
            rect.center().y
        } else {
            egui::lerp(
                rect.top()..=rect.bottom(),
                yi as f32 / (GRID_BOUND_SAMPLES - 1) as f32,
            )
        };
        for xi in 0..GRID_BOUND_SAMPLES {
            let x = if GRID_BOUND_SAMPLES <= 1 {
                rect.center().x
            } else {
                egui::lerp(
                    rect.left()..=rect.right(),
                    xi as f32 / (GRID_BOUND_SAMPLES - 1) as f32,
                )
            };
            let screen = egui::pos2(x, y);
            if let Some(hit) = camera.ray_to_plane_y0(rect, screen) {
                let from_eye = sub3(hit, camera.eye);
                let distance = dot3(from_eye, from_eye).sqrt();
                let point = if distance <= max_ray_distance {
                    hit
                } else {
                    add3(camera.eye, mul3(normalize3(from_eye), max_ray_distance))
                };
                include_point(point, &mut min_x, &mut max_x, &mut min_z, &mut max_z);
            } else {
                let direction = camera.ray_direction(rect, screen);
                let flat = [direction[0], 0.0, direction[2]];
                if dot3(flat, flat) > 1.0e-5 {
                    include_point(
                        add3(
                            [camera.eye[0], 0.0, camera.eye[2]],
                            mul3(normalize3(flat), stable_span),
                        ),
                        &mut min_x,
                        &mut max_x,
                        &mut min_z,
                        &mut max_z,
                    );
                }
            }
        }
    }

    let center_x = (min_x + max_x) * 0.5;
    let center_z = (min_z + max_z) * 0.5;
    let max_half_span = spacing * GRID_MAX_LINES_PER_AXIS as f32 * 0.5;
    if max_x - min_x > max_half_span * 2.0 {
        min_x = center_x - max_half_span;
        max_x = center_x + max_half_span;
    }
    if max_z - min_z > max_half_span * 2.0 {
        min_z = center_z - max_half_span;
        max_z = center_z + max_half_span;
    }

    let margin = spacing * 4.0;
    [
        min_x - margin,
        max_x + margin,
        min_z - margin,
        max_z + margin,
    ]
}

fn tint_color(a: egui::Color32, b: egui::Color32, amount: f32) -> egui::Color32 {
    let amount = amount.clamp(0.0, 1.0);
    let inv = 1.0 - amount;
    egui::Color32::from_rgba_unmultiplied(
        ((a.r() as f32) * inv + (b.r() as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8,
        ((a.g() as f32) * inv + (b.g() as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8,
        ((a.b() as f32) * inv + (b.b() as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8,
        ((a.a() as f32) * inv + (b.a() as f32) * amount)
            .round()
            .clamp(0.0, 255.0) as u8,
    )
}

pub mod prelude {
    pub use crate::{
        Camera3d, Color, Light3d, LightId, Material3d, MaterialId, Object3d, ObjectId, Orbit3d,
        Primitive3d, Renderer3d, Scene3d, Transform3d, Vec3, View3d, Viewport3d,
    };
}
