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
const TECH_LIGHT_AMBIENT: f32 = 0.34;
const TECH_LIGHT_CONTRAST: f32 = 0.42;
const TECH_LIGHT_KEY: Vec3 = [1.0, 2.0, 3.0];
const TECH_LIGHT_FILL: Vec3 = [-1.0, -3.0, -5.0];
const TECH_LIGHT_FILL_STRENGTH: f32 = 0.32;
const TECH_LIGHT_RIM_STRENGTH: f32 = 0.18;
const OBJECT_TRIANGLE_MAX_SCREEN_FRAC: f32 = 1.8;
const OBJECT_SSAA_SCALE: usize = 3;
const OBJECT_SSAA_MAX_DIMENSION: usize = 2400;

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

/// A retained triangle mesh. All 3D scene objects reduce to this.
#[derive(Clone, Debug, PartialEq)]
pub struct TriangleMesh3d {
    pub vertices: Vec<Vec3>,
    pub indices: Vec<[u32; 3]>,
    pub normals: Vec<Vec3>,
}

impl TriangleMesh3d {
    #[must_use]
    pub fn new(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>) -> Self {
        Self {
            vertices,
            indices,
            normals: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_normals(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>, normals: Vec<Vec3>) -> Self {
        Self {
            vertices,
            indices,
            normals,
        }
    }
}

/// Retained drawable geometry. Deliberately triangle-only: cubes, spheres,
/// cylinders, imported assets, and projected 2D paths are all stored as
/// triangle meshes.
#[derive(Clone, Debug, PartialEq)]
pub enum Primitive3d {
    Triangles(TriangleMesh3d),
}

impl Primitive3d {
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        "triangles"
    }

    #[must_use]
    pub fn mesh(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>) -> Self {
        Self::Triangles(TriangleMesh3d::new(vertices, indices))
    }

    #[must_use]
    pub fn triangle(vertices: [Vec3; 3]) -> Self {
        Self::mesh(vertices.to_vec(), vec![[0, 1, 2]])
    }

    /// Build one filled polygon from an outline.
    ///
    /// The retained primitive is still triangle-only: this helper only
    /// triangulates the outline into a [`TriangleMesh3d`]. The outline may
    /// live in 3D; triangulation uses a projected plane to choose triangle
    /// indices, while the original 3D vertex positions are preserved.
    #[must_use]
    pub fn polygon(outline: Vec<Vec3>) -> Self {
        let outline = cleaned_polygon_outline(outline);
        if outline.len() < 3 {
            return Self::mesh(Vec::new(), Vec::new());
        }

        let indices = triangulate_polygon_outline(&outline);
        let normal = polygon_normal(&outline);
        let normals = vec![normal; outline.len()];
        Self::Triangles(TriangleMesh3d::with_normals(outline, indices, normals))
    }

    #[must_use]
    pub fn cube(size: f32) -> Self {
        let h = size * 0.5;
        let mut vertices = Vec::with_capacity(24);
        let mut normals = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(12);
        let faces = [
            (
                [0.0, 0.0, -1.0],
                [[-h, -h, -h], [h, -h, -h], [h, h, -h], [-h, h, -h]],
            ),
            (
                [0.0, 0.0, 1.0],
                [[-h, -h, h], [-h, h, h], [h, h, h], [h, -h, h]],
            ),
            (
                [0.0, -1.0, 0.0],
                [[-h, -h, -h], [-h, -h, h], [h, -h, h], [h, -h, -h]],
            ),
            (
                [0.0, 1.0, 0.0],
                [[-h, h, -h], [h, h, -h], [h, h, h], [-h, h, h]],
            ),
            (
                [1.0, 0.0, 0.0],
                [[h, -h, -h], [h, -h, h], [h, h, h], [h, h, -h]],
            ),
            (
                [-1.0, 0.0, 0.0],
                [[-h, -h, -h], [-h, h, -h], [-h, h, h], [-h, -h, h]],
            ),
        ];
        for (normal, points) in faces {
            let base = vertices.len() as u32;
            vertices.extend(points);
            normals.extend([normal; 4]);
            indices.push([base, base + 1, base + 2]);
            indices.push([base, base + 2, base + 3]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn cylinder(radius: f32, height: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let mut vertices = Vec::with_capacity(2 + segments * 2);
        let bottom_center = 0_u32;
        let top_center = 1_u32;
        vertices.push([0.0, -height * 0.5, 0.0]);
        vertices.push([0.0, height * 0.5, 0.0]);
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let x = radius * angle.cos();
            let z = radius * angle.sin();
            vertices.push([x, -height * 0.5, z]);
            vertices.push([x, height * 0.5, z]);
        }

        let mut indices = Vec::with_capacity(segments * 4);
        for i in 0..segments {
            let next = (i + 1) % segments;
            let b0 = 2 + (i * 2) as u32;
            let t0 = b0 + 1;
            let b1 = 2 + (next * 2) as u32;
            let t1 = b1 + 1;
            indices.push([b0, b1, t1]);
            indices.push([b0, t1, t0]);
            indices.push([bottom_center, b1, b0]);
            indices.push([top_center, t0, t1]);
        }
        Self::mesh(vertices, indices)
    }

    #[must_use]
    pub fn sphere(radius: f32, segments: u32) -> Self {
        let longitude = segments.max(8) as usize;
        let latitude = (longitude / 2).max(4);
        let mut vertices = Vec::with_capacity(2 + (latitude - 1) * longitude);
        let mut normals = Vec::with_capacity(2 + (latitude - 1) * longitude);
        vertices.push([0.0, radius, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        for lat in 1..latitude {
            let theta = std::f32::consts::PI * lat as f32 / latitude as f32;
            let y = radius * theta.cos();
            let r = radius * theta.sin();
            for lon in 0..longitude {
                let phi = std::f32::consts::TAU * lon as f32 / longitude as f32;
                let normal = [
                    theta.sin() * phi.cos(),
                    theta.cos(),
                    theta.sin() * phi.sin(),
                ];
                vertices.push([r * phi.cos(), y, r * phi.sin()]);
                normals.push(normal);
            }
        }
        let bottom = vertices.len() as u32;
        vertices.push([0.0, -radius, 0.0]);
        normals.push([0.0, -1.0, 0.0]);

        let ring = |lat: usize, lon: usize| -> u32 { 1 + ((lat - 1) * longitude + lon) as u32 };
        let mut indices = Vec::new();
        for lon in 0..longitude {
            indices.push([0, ring(1, lon), ring(1, (lon + 1) % longitude)]);
        }
        for lat in 1..(latitude - 1) {
            for lon in 0..longitude {
                let a = ring(lat, lon);
                let b = ring(lat, (lon + 1) % longitude);
                let c = ring(lat + 1, lon);
                let d = ring(lat + 1, (lon + 1) % longitude);
                indices.push([a, c, d]);
                indices.push([a, d, b]);
            }
        }
        for lon in 0..longitude {
            indices.push([
                bottom,
                ring(latitude - 1, (lon + 1) % longitude),
                ring(latitude - 1, lon),
            ]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }
}

impl Default for Primitive3d {
    fn default() -> Self {
        Self::cube(1.0)
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
        let cube = scene.add_object("Cube", Primitive3d::cube(1.0), MaterialId(1));
        if let Some(object) = scene.object_mut(cube) {
            object.transform.translation = [-0.75, 0.55, 0.0];
            object.selected = true;
        }
        let small = scene.add_object("Small cube", Primitive3d::cube(0.65), washed);
        if let Some(object) = scene.object_mut(small) {
            object.transform.translation = [0.85, 0.35, -0.35];
        }
        let sphere = scene.add_object("Sphere", Primitive3d::sphere(0.38, 32), washed);
        if let Some(object) = scene.object_mut(sphere) {
            object.transform.translation = [0.35, 0.38, 0.85];
        }
        let triangle = scene.add_object(
            "Triangle",
            Primitive3d::triangle([[-0.45, 0.02, -0.35], [0.45, 0.02, -0.25], [0.0, 0.02, 0.45]]),
            washed,
        );
        if let Some(object) = scene.object_mut(triangle) {
            object.transform.translation = [-0.15, 0.02, -1.35];
        }
        let spiral = scene.add_object("Spiral polygon", spiral_polygon_mesh(), MaterialId(1));
        if let Some(object) = scene.object_mut(spiral) {
            object.transform.translation = [1.25, 0.0, 0.85];
        }
        let mesh = scene.add_object(
            "Mesh pyramid",
            Primitive3d::mesh(
                vec![
                    [-0.45, 0.0, -0.45],
                    [0.45, 0.0, -0.45],
                    [0.45, 0.0, 0.45],
                    [-0.45, 0.0, 0.45],
                    [0.0, 0.75, 0.0],
                ],
                vec![
                    [0, 1, 4],
                    [1, 2, 4],
                    [2, 3, 4],
                    [3, 0, 4],
                    [0, 3, 2],
                    [0, 2, 1],
                ],
            ),
            washed,
        );
        if let Some(object) = scene.object_mut(mesh) {
            object.transform.translation = [-1.4, 0.02, 1.05];
        }
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

fn spiral_polygon_mesh() -> Primitive3d {
    const STEPS: usize = 48;
    const HALF_WIDTH: f32 = 0.038;
    const START_HEIGHT: f32 = 0.78;
    const DESCENT_PER_STEP: f32 = 0.026;

    let centerline: Vec<Vec3> = (0..STEPS)
        .map(|i| {
            let t = i as f32 * 0.34;
            let r = 0.018 * i as f32;
            [
                r * t.cos(),
                START_HEIGHT - i as f32 * DESCENT_PER_STEP,
                r * t.sin(),
            ]
        })
        .collect();

    let mut left = Vec::with_capacity(STEPS);
    let mut right = Vec::with_capacity(STEPS);
    for i in 0..STEPS {
        let center = centerline[i];
        let prev = centerline[i.saturating_sub(1)];
        let next = centerline[(i + 1).min(STEPS - 1)];
        let tangent = normalize3(sub3(next, prev));
        let mut side = cross3(WORLD_UP, tangent);
        if dot3(side, side) <= 1.0e-5 {
            side = [1.0, 0.0, 0.0];
        }
        let side = mul3(normalize3(side), HALF_WIDTH);
        left.push(add3(center, side));
        right.push(sub3(center, side));
    }

    right.reverse();
    left.extend(right);
    Primitive3d::polygon(left)
}

fn cleaned_polygon_outline(outline: Vec<Vec3>) -> Vec<Vec3> {
    const EPS: f32 = 1.0e-6;
    let mut cleaned = Vec::with_capacity(outline.len());
    for point in outline {
        let duplicate = cleaned
            .last()
            .is_some_and(|last| dot3(sub3(point, *last), sub3(point, *last)) <= EPS);
        if !duplicate {
            cleaned.push(point);
        }
    }
    if cleaned.len() > 1
        && dot3(
            sub3(cleaned[0], *cleaned.last().expect("checked len")),
            sub3(cleaned[0], *cleaned.last().expect("checked len")),
        ) <= EPS
    {
        cleaned.pop();
    }
    cleaned
}

fn triangulate_polygon_outline(outline: &[Vec3]) -> Vec<[u32; 3]> {
    const EPS: f32 = 1.0e-6;
    if outline.len() < 3 {
        return Vec::new();
    }
    if outline.len() == 3 {
        return vec![[0, 1, 2]];
    }

    let normal = polygon_normal(outline);
    let projected: Vec<[f32; 2]> = outline
        .iter()
        .copied()
        .map(|point| project_polygon_point(point, normal))
        .collect();
    let area = polygon_area2(&projected);
    if area.abs() <= EPS {
        return Vec::new();
    }
    let ccw = area > 0.0;
    let mut remaining: Vec<usize> = (0..outline.len()).collect();
    let mut indices = Vec::with_capacity(outline.len().saturating_sub(2));
    let mut guard = 0;

    while remaining.len() > 3 && guard < outline.len() * outline.len() {
        guard += 1;
        let mut clipped = false;

        for i in 0..remaining.len() {
            let prev = remaining[(i + remaining.len() - 1) % remaining.len()];
            let curr = remaining[i];
            let next = remaining[(i + 1) % remaining.len()];
            let a = projected[prev];
            let b = projected[curr];
            let c = projected[next];
            let turn = cross2(sub2(b, a), sub2(c, b));
            if (ccw && turn <= EPS) || (!ccw && turn >= -EPS) {
                continue;
            }
            let contains_other = remaining.iter().copied().any(|other| {
                other != prev
                    && other != curr
                    && other != next
                    && point_in_triangle2(projected[other], a, b, c)
            });
            if contains_other {
                continue;
            }

            if ccw {
                indices.push([prev as u32, curr as u32, next as u32]);
            } else {
                indices.push([prev as u32, next as u32, curr as u32]);
            }
            remaining.remove(i);
            clipped = true;
            break;
        }

        if !clipped {
            break;
        }
    }

    if remaining.len() == 3 {
        if ccw {
            indices.push([
                remaining[0] as u32,
                remaining[1] as u32,
                remaining[2] as u32,
            ]);
        } else {
            indices.push([
                remaining[0] as u32,
                remaining[2] as u32,
                remaining[1] as u32,
            ]);
        }
    }

    indices
}

fn polygon_normal(outline: &[Vec3]) -> Vec3 {
    if outline.len() < 3 {
        return WORLD_UP;
    }
    let mut normal = [0.0, 0.0, 0.0];
    for i in 0..outline.len() {
        let current = outline[i];
        let next = outline[(i + 1) % outline.len()];
        normal[0] += (current[1] - next[1]) * (current[2] + next[2]);
        normal[1] += (current[2] - next[2]) * (current[0] + next[0]);
        normal[2] += (current[0] - next[0]) * (current[1] + next[1]);
    }
    if dot3(normal, normal) <= 1.0e-6 {
        face_normal([outline[0], outline[1], outline[2]])
    } else {
        normalize3(normal)
    }
}

fn project_polygon_point(point: Vec3, normal: Vec3) -> [f32; 2] {
    let abs = [normal[0].abs(), normal[1].abs(), normal[2].abs()];
    if abs[0] >= abs[1] && abs[0] >= abs[2] {
        [point[1], point[2]]
    } else if abs[1] >= abs[2] {
        [point[0], point[2]]
    } else {
        [point[0], point[1]]
    }
}

fn polygon_area2(points: &[[f32; 2]]) -> f32 {
    let mut area = 0.0;
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        area += a[0] * b[1] - b[0] * a[1];
    }
    area * 0.5
}

fn point_in_triangle2(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    const EPS: f32 = 1.0e-6;
    let ab = cross2(sub2(b, a), sub2(point, a));
    let bc = cross2(sub2(c, b), sub2(point, b));
    let ca = cross2(sub2(a, c), sub2(point, c));
    let has_negative = ab < -EPS || bc < -EPS || ca < -EPS;
    let has_positive = ab > EPS || bc > EPS || ca > EPS;
    !(has_negative && has_positive)
}

fn sub2(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

fn cross2(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
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
#[derive(Clone)]
pub struct View3d {
    id: egui::Id,
    scene: Scene3d,
    orbit: Orbit3d,
    preview_texture: Option<egui::TextureHandle>,
}

impl View3d {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, scene: Scene3d) -> Self {
        Self {
            id: egui::Id::new(id),
            scene,
            orbit: Orbit3d::default(),
            preview_texture: None,
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
            (rect.width() * ppp).ceil().max(1.0) as u32,
            (rect.height() * ppp).ceil().max(1.0) as u32,
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

        self.paint_grid(&painter, rect, &camera, 1.0, accent);

        for object in &self.scene.objects {
            if !object.visible {
                continue;
            }
            match &object.primitive {
                Primitive3d::Triangles(mesh) => {
                    self.collect_mesh_faces(
                        &mut faces,
                        rect,
                        &camera,
                        object,
                        &mesh.vertices,
                        &mesh.indices,
                        &mesh.normals,
                    );
                }
            }
        }

        faces.sort_by(|a, b: &PreviewFace| b.depth.total_cmp(&a.depth));
        paint_faces_supersampled(ui, &painter, rect, &mut self.preview_texture, faces);

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

    fn collect_mesh_faces(
        &self,
        faces: &mut Vec<PreviewFace>,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
        vertices: &[Vec3],
        indices: &[[u32; 3]],
        normals: &[Vec3],
    ) {
        if vertices.is_empty() || indices.is_empty() {
            return;
        }

        let world: Vec<Vec3> = vertices
            .iter()
            .copied()
            .map(|point| transform_point(&object.transform, point))
            .collect();
        let world_normals: Vec<Vec3> = if normals.len() == vertices.len() {
            normals
                .iter()
                .copied()
                .map(|normal| transform_normal(&object.transform, normal))
                .collect()
        } else {
            Vec::new()
        };
        let base = self.material_color(object.material);

        for triangle in indices {
            let triangle = triangle.map(|index| index as usize);
            if triangle.iter().any(|index| *index >= world.len()) {
                continue;
            }
            let world_triangle = [world[triangle[0]], world[triangle[1]], world[triangle[2]]];
            let face_normal_world = face_normal(world_triangle);

            let mut points = Vec::with_capacity(3);
            let mut depths = Vec::with_capacity(3);
            let mut depth = 0.0;
            let mut visible = true;
            for index in triangle {
                let Some((point, z)) = camera.project(rect, world[index]) else {
                    visible = false;
                    break;
                };
                points.push(point);
                depths.push(z);
                depth += z;
            }
            if visible {
                if !triangle_screen_is_stable(rect, [points[0], points[1], points[2]]) {
                    continue;
                }
                faces.push(PreviewFace {
                    depth: depth / 3.0,
                    points: [points[0], points[1], points[2]],
                    depths: [depths[0], depths[1], depths[2]],
                    fills: if world_normals.len() == world.len() {
                        [
                            shade_color(base, world_normals[triangle[0]], camera),
                            shade_color(base, world_normals[triangle[1]], camera),
                            shade_color(base, world_normals[triangle[2]], camera),
                        ]
                    } else {
                        let fill = shade_color(base, face_normal_world, camera);
                        [fill; 3]
                    },
                });
            }
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
            if !object.visible {
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
        let local_radius = match &object.primitive {
            Primitive3d::Triangles(mesh) => local_radius(&mesh.vertices),
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
    points: [egui::Pos2; 3],
    depths: [f32; 3],
    fills: [egui::Color32; 3],
}

fn paint_faces_supersampled(
    ui: &egui::Ui,
    painter: &egui::Painter,
    rect: egui::Rect,
    texture: &mut Option<egui::TextureHandle>,
    faces: Vec<PreviewFace>,
) {
    let low_width = rect.width().round().max(1.0) as usize;
    let low_height = rect.height().round().max(1.0) as usize;
    let scale = OBJECT_SSAA_SCALE
        .min((OBJECT_SSAA_MAX_DIMENSION / low_width.max(low_height)).max(1))
        .max(1);
    let width = low_width * scale;
    let height = low_height * scale;
    let mut pixels = vec![egui::Color32::TRANSPARENT; width * height];
    let mut depth = vec![f32::INFINITY; width * height];
    let scale = scale as f32;

    for face in faces {
        rasterize_face(rect, scale, width, height, &mut pixels, &mut depth, &face);
    }

    let image = egui::ColorImage {
        size: [width, height],
        pixels,
        source_size: egui::vec2(width as f32, height as f32),
    };

    match texture {
        Some(texture) if texture.size() == [width, height] => {
            texture.set(image, egui::TextureOptions::LINEAR);
        }
        _ => {
            *texture = Some(ui.ctx().load_texture(
                "mara_3d_supersampled_preview",
                image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

fn rasterize_face(
    rect: egui::Rect,
    scale: f32,
    width: usize,
    height: usize,
    pixels: &mut [egui::Color32],
    depth: &mut [f32],
    face: &PreviewFace,
) {
    let points = face.points.map(|point| {
        egui::pos2(
            (point.x - rect.left()) * scale,
            (point.y - rect.top()) * scale,
        )
    });
    let area = edge_function(points[0], points[1], points[2]);
    if area.abs() <= f32::EPSILON {
        return;
    }

    let min_x = points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_x = points
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((width - 1) as f32) as usize;
    let min_y = points
        .iter()
        .map(|point| point.y)
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as usize;
    let max_y = points
        .iter()
        .map(|point| point.y)
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min((height - 1) as f32) as usize;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let p = egui::pos2(x as f32 + 0.5, y as f32 + 0.5);
            let w0 = edge_function(points[1], points[2], p);
            let w1 = edge_function(points[2], points[0], p);
            let w2 = edge_function(points[0], points[1], p);
            let inside = if area > 0.0 {
                w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
            } else {
                w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
            };
            if !inside {
                continue;
            }

            let inv_area = area.recip();
            let b0 = w0 * inv_area;
            let b1 = w1 * inv_area;
            let b2 = w2 * inv_area;
            let z = face.depths[0] * b0 + face.depths[1] * b1 + face.depths[2] * b2;
            let index = y * width + x;
            if z >= depth[index] {
                continue;
            }
            depth[index] = z;
            pixels[index] = interpolate_color(face.fills, [b0, b1, b2]);
        }
    }
}

fn edge_function(a: egui::Pos2, b: egui::Pos2, c: egui::Pos2) -> f32 {
    (c.x - a.x) * (b.y - a.y) - (c.y - a.y) * (b.x - a.x)
}

fn interpolate_color(colors: [egui::Color32; 3], weights: [f32; 3]) -> egui::Color32 {
    let channel = |values: [u8; 3]| -> u8 {
        (values[0] as f32 * weights[0]
            + values[1] as f32 * weights[1]
            + values[2] as f32 * weights[2])
            .round()
            .clamp(0.0, 255.0) as u8
    };
    egui::Color32::from_rgba_unmultiplied(
        channel([colors[0].r(), colors[1].r(), colors[2].r()]),
        channel([colors[0].g(), colors[1].g(), colors[2].g()]),
        channel([colors[0].b(), colors[1].b(), colors[2].b()]),
        channel([colors[0].a(), colors[1].a(), colors[2].a()]),
    )
}

fn triangle_screen_is_stable(rect: egui::Rect, points: [egui::Pos2; 3]) -> bool {
    let diag = rect.size().length().max(1.0);
    let max_edge = points[0]
        .distance(points[1])
        .max(points[1].distance(points[2]))
        .max(points[2].distance(points[0]));
    max_edge.is_finite() && max_edge <= diag * OBJECT_TRIANGLE_MAX_SCREEN_FRAC
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
    let scaled = [
        point[0] * transform.scale[0],
        point[1] * transform.scale[1],
        point[2] * transform.scale[2],
    ];
    add3(
        transform.translation,
        rotate3_by_quat(scaled, transform.rotation_xyzw),
    )
}

fn transform_normal(transform: &Transform3d, normal: Vec3) -> Vec3 {
    normalize3(rotate3_by_quat(normal, transform.rotation_xyzw))
}

fn local_radius(points: &[Vec3]) -> f32 {
    points
        .iter()
        .map(|point| dot3(*point, *point).sqrt())
        .fold(0.0, f32::max)
}

fn rotate3_by_quat(point: Vec3, quat_xyzw: Quat) -> Vec3 {
    let q = normalize_quat(quat_xyzw);
    let q_vec = [q[0], q[1], q[2]];
    let t = mul3(cross3(q_vec, point), 2.0);
    add3(point, add3(mul3(t, q[3]), cross3(q_vec, t)))
}

fn normalize_quat(value: Quat) -> Quat {
    let length =
        (value[0] * value[0] + value[1] * value[1] + value[2] * value[2] + value[3] * value[3])
            .sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [
            value[0] / length,
            value[1] / length,
            value[2] / length,
            value[3] / length,
        ]
    }
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

fn face_normal(points: [Vec3; 3]) -> Vec3 {
    normalize3(cross3(
        sub3(points[1], points[0]),
        sub3(points[2], points[0]),
    ))
}

fn shade_color(base: egui::Color32, normal: Vec3, camera: &PreviewCamera) -> egui::Color32 {
    let mut normal = normalize3(normal);
    if dot3(normal, camera.forward) > 0.0 {
        normal = mul3(normal, -1.0);
    }

    let key = dot3(normalize3(TECH_LIGHT_KEY), normal).max(0.0);
    let fill = dot3(normalize3(TECH_LIGHT_FILL), normal).max(0.0) * TECH_LIGHT_FILL_STRENGTH;
    let view = dot3(normal, mul3(camera.forward, -1.0))
        .abs()
        .clamp(0.0, 1.0);
    let rim = (1.0 - view).powf(2.0) * TECH_LIGHT_RIM_STRENGTH;
    let diffuse = (TECH_LIGHT_AMBIENT + key * 0.9 + fill + rim).clamp(0.0, 1.0);
    let value = (1.0 - TECH_LIGHT_CONTRAST) + diffuse * TECH_LIGHT_CONTRAST;
    shade_scalar(base, value)
}

fn shade_scalar(base: egui::Color32, value: f32) -> egui::Color32 {
    let value = value.clamp(0.0, 1.0);
    egui::Color32::from_rgba_unmultiplied(
        ((base.r() as f32) * value).round().clamp(0.0, 255.0) as u8,
        ((base.g() as f32) * value).round().clamp(0.0, 255.0) as u8,
        ((base.b() as f32) * value).round().clamp(0.0, 255.0) as u8,
        base.a(),
    )
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
