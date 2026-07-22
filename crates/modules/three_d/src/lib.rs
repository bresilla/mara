//! `mara_3d` — retained 3D scene/view module for Mara.
//!
//! This crate owns Mara's host-agnostic 3D document model and exposes
//! the [`three_d`] backend for renderer implementations. It deliberately
//! does **not** create a window or run an event loop; hosts decide whether
//! the scene is rendered inside Mara-owned chrome, a Bevy-owned app, web,
//! or another integration.

#![allow(clippy::too_many_arguments, clippy::question_mark)]

use mara_core::{
    MaraModule, MaraView, ModuleInlineCtx, ModuleResponse, RibbonAction, RibbonCluster, RibbonEdge,
    RibbonOverridePolicy, RibbonScope, RibbonSlot, RibbonSlotDef, RibbonSlotId, RibbonSlotItem,
    ViewCtx, ViewId, WorkspaceBar, WorkspaceBarCluster, WorkspaceBarEdge, WorkspaceCtx,
    vocab::{Color32 as MaraColor32, Pos2 as MaraPos2, Vec2 as MaraVec2},
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
const TECH_LIGHT_AMBIENT: f32 = 0.36;
const TECH_LIGHT_CONTRAST: f32 = 0.56;
const TECH_LIGHT_KEY: Vec3 = [0.8, 1.8, 1.25];
const TECH_LIGHT_FILL: Vec3 = [-1.2, 0.65, -1.8];
const TECH_LIGHT_FILL_STRENGTH: f32 = 0.20;
const TECH_LIGHT_HEADLIGHT_STRENGTH: f32 = 0.18;
const TECH_LIGHT_RIM_STRENGTH: f32 = 0.11;
const TECH_LIGHT_SPECULAR_STRENGTH: f32 = 0.13;
const TECH_LIGHT_SPECULAR_POWER: f32 = 34.0;
const TECH_LIGHT_SKY_STRENGTH: f32 = 0.10;
const GIZMO_SIZE: f32 = 60.0;
const GIZMO_STROKE_WIDTH: f32 = 3.2;
const GIZMO_INACTIVE_ALPHA: f32 = 0.7;
const GIZMO_ARROW_FADE_START: f32 = 0.95;
const GIZMO_ARROW_FADE_END: f32 = 0.99;
const GIZMO_PLANE_FADE_START: f32 = 0.70;
const GIZMO_PLANE_FADE_END: f32 = 0.86;
const GIZMO_ARC_FADE_START: f32 = 0.990;
const GIZMO_ARC_FADE_END: f32 = 0.995;
const GIZMO_ROTATION_SEGMENTS: usize = 72;
const GIZMO_PICK_DISTANCE: f32 = 9.0;
const GIZMO_HIGHLIGHT_WIDTH_SCALE: f32 = 1.35;
const OBJECT_TRIANGLE_MAX_SCREEN_FRAC: f32 = 1.8;
const OBJECT_SSAA_SCALE: usize = 3;
const OBJECT_SSAA_MAX_DIMENSION: usize = 2400;
const INTERACTIVE_SELECTED_TRIANGLE_BUDGET: usize = 18_000;
const INTERACTIVE_BACKGROUND_TRIANGLE_BUDGET: usize = 4_000;

/// Re-export of the renderer backend used by this module.
///
/// `three-d` is an OpenGL/WebGL/OpenGL ES renderer that can be used
/// without its optional window helper, so Mara can provide the app chrome
/// while backend integrations provide the actual GL/WebGL context.
pub use three_d as backend;

pub type Vec3 = [f32; 3];
pub type Quat = [f32; 4];
pub type Color = MaraColor32;

/// Stable object id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

/// Stable gizmo id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GizmoId(pub u64);

/// Stable light id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LightId(pub u64);

/// Stable material id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MaterialId(pub u64);

/// Stable texture id inside a retained 3D scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TextureId(pub u64);

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
    pub uvs: Vec<[f32; 2]>,
    pub vertex_colors: Vec<Color>,
}

impl TriangleMesh3d {
    #[must_use]
    pub fn new(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>) -> Self {
        Self {
            vertices,
            indices,
            normals: Vec::new(),
            uvs: Vec::new(),
            vertex_colors: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_normals(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>, normals: Vec<Vec3>) -> Self {
        Self {
            vertices,
            indices,
            normals,
            uvs: Vec::new(),
            vertex_colors: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_uvs(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>, uvs: Vec<[f32; 2]>) -> Self {
        Self {
            vertices,
            indices,
            normals: Vec::new(),
            uvs,
            vertex_colors: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_vertex_colors(
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
        vertex_colors: Vec<Color>,
    ) -> Self {
        // Per-face flat shading: duplicate each triangle's vertices so
        // the face carries a single face normal at all three corners.
        // Per-vertex colors are duplicated alongside so the original
        // color gradient still interpolates across the face — but the
        // shading no longer drifts because of normals averaged across
        // faces with different orientations (the bug visible on the
        // apex of a vertex-colored pyramid).
        let mut out_v = Vec::with_capacity(indices.len() * 3);
        let mut out_n = Vec::with_capacity(indices.len() * 3);
        let mut out_c = Vec::with_capacity(indices.len() * 3);
        let mut out_i = Vec::with_capacity(indices.len());
        for tri in &indices {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
                continue;
            }
            let a = vertices[i0];
            let b = vertices[i1];
            let c = vertices[i2];
            let n = face_normal([a, b, c]);
            let base = out_v.len() as u32;
            out_v.push(a);
            out_v.push(b);
            out_v.push(c);
            out_n.push(n);
            out_n.push(n);
            out_n.push(n);
            out_c.push(vertex_colors.get(i0).copied().unwrap_or(MaraColor32::WHITE));
            out_c.push(vertex_colors.get(i1).copied().unwrap_or(MaraColor32::WHITE));
            out_c.push(vertex_colors.get(i2).copied().unwrap_or(MaraColor32::WHITE));
            out_i.push([base, base + 1, base + 2]);
        }
        Self {
            vertices: out_v,
            indices: out_i,
            normals: out_n,
            uvs: Vec::new(),
            vertex_colors: out_c,
        }
    }

    #[must_use]
    pub fn with_generated_normals(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>) -> Self {
        let mut normals = vec![[0.0, 0.0, 0.0]; vertices.len()];
        for triangle in &indices {
            let triangle = triangle.map(|index| index as usize);
            if triangle.iter().any(|index| *index >= vertices.len()) {
                continue;
            }
            let normal = face_normal([
                vertices[triangle[0]],
                vertices[triangle[1]],
                vertices[triangle[2]],
            ]);
            for index in triangle {
                normals[index] = add3(normals[index], normal);
            }
        }
        for normal in &mut normals {
            *normal = normalize3(*normal);
        }
        Self {
            vertices,
            indices,
            normals,
            uvs: Vec::new(),
            vertex_colors: Vec::new(),
        }
    }

    #[must_use]
    pub fn with_generated_normals_and_uvs(
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
        uvs: Vec<[f32; 2]>,
    ) -> Self {
        let mut mesh = Self::with_generated_normals(vertices, indices);
        mesh.uvs = uvs;
        mesh
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
    pub fn tri_mesh(mesh: TriangleMesh3d) -> Self {
        Self::Triangles(mesh)
    }

    #[must_use]
    pub fn mesh_with_uvs(vertices: Vec<Vec3>, indices: Vec<[u32; 3]>, uvs: Vec<[f32; 2]>) -> Self {
        Self::Triangles(TriangleMesh3d::with_generated_normals_and_uvs(
            vertices, indices, uvs,
        ))
    }

    #[must_use]
    pub fn mesh_with_vertex_colors(
        vertices: Vec<Vec3>,
        indices: Vec<[u32; 3]>,
        vertex_colors: Vec<Color>,
    ) -> Self {
        Self::Triangles(TriangleMesh3d::with_vertex_colors(
            vertices,
            indices,
            vertex_colors,
        ))
    }

    #[must_use]
    pub fn triangle(vertices: [Vec3; 3]) -> Self {
        Self::mesh(vertices.to_vec(), vec![[0, 1, 2]])
    }

    #[must_use]
    pub fn plane(width: f32, depth: f32) -> Self {
        let hw = width * 0.5;
        let hd = depth * 0.5;
        Self::Triangles(TriangleMesh3d::with_normals(
            vec![
                [-hw, 0.0, -hd],
                [hw, 0.0, -hd],
                [hw, 0.0, hd],
                [-hw, 0.0, hd],
            ],
            vec![[0, 1, 2], [0, 2, 3]],
            vec![[0.0, 1.0, 0.0]; 4],
        ))
    }

    #[must_use]
    pub fn rectangle(width: f32, height: f32) -> Self {
        Self::plane(width, height)
    }

    #[must_use]
    pub fn square(size: f32) -> Self {
        Self::plane(size, size)
    }

    #[must_use]
    pub fn disc(radius: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let mut vertices = Vec::with_capacity(segments + 1);
        let mut normals = Vec::with_capacity(segments + 1);
        let mut indices = Vec::with_capacity(segments);
        vertices.push([0.0, 0.0, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([radius * angle.cos(), 0.0, radius * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
        }
        for i in 0..segments {
            indices.push([0, 1 + i as u32, 1 + ((i + 1) % segments) as u32]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn regular_polygon(radius: f32, sides: u32) -> Self {
        let sides = sides.max(3) as usize;
        let outline = (0..sides)
            .map(|i| {
                let angle =
                    std::f32::consts::TAU * i as f32 / sides as f32 + std::f32::consts::FRAC_PI_2;
                [radius * angle.cos(), 0.0, radius * angle.sin()]
            })
            .collect();
        Self::polygon(outline)
    }

    #[must_use]
    pub fn star(outer_radius: f32, inner_radius: f32, points: u32) -> Self {
        let points = points.max(2) as usize;
        let count = points * 2;
        let outline = (0..count)
            .map(|i| {
                let radius = if i % 2 == 0 {
                    outer_radius
                } else {
                    inner_radius
                };
                let angle =
                    std::f32::consts::TAU * i as f32 / count as f32 + std::f32::consts::FRAC_PI_2;
                [radius * angle.cos(), 0.0, radius * angle.sin()]
            })
            .collect();
        Self::polygon(outline)
    }

    #[must_use]
    pub fn annulus(inner_radius: f32, outer_radius: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let inner_radius = inner_radius.min(outer_radius).max(0.0);
        let outer_radius = outer_radius.max(inner_radius + 1.0e-4);
        let mut vertices = Vec::with_capacity(segments * 2);
        let mut normals = Vec::with_capacity(segments * 2);
        let mut indices = Vec::with_capacity(segments * 2);
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let dir = [angle.cos(), 0.0, angle.sin()];
            vertices.push(mul3(dir, outer_radius));
            vertices.push(mul3(dir, inner_radius));
            normals.extend([[0.0, 1.0, 0.0]; 2]);
        }
        for i in 0..segments {
            let next = (i + 1) % segments;
            let o0 = (i * 2) as u32;
            let i0 = o0 + 1;
            let o1 = (next * 2) as u32;
            let i1 = o1 + 1;
            indices.push([o0, o1, i1]);
            indices.push([o0, i1, i0]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn polyline(points: Vec<Vec3>, width: f32) -> Self {
        ribbon_mesh(points, width, false)
    }

    #[must_use]
    pub fn closed_polyline(points: Vec<Vec3>, width: f32) -> Self {
        ribbon_mesh(points, width, true)
    }

    #[must_use]
    pub fn path(points: Vec<Vec3>, width: f32, closed: bool) -> Self {
        ribbon_mesh(points, width, closed)
    }

    /// Rerun-style `LineStrips3D`: a real scene-radius 3D tube path.
    #[must_use]
    pub fn line_strip_3d(points: Vec<Vec3>, radius: f32, segments: u32) -> Self {
        tube_path_mesh(points, radius, segments, false)
    }

    /// Closed variant of [`Self::line_strip_3d`].
    #[must_use]
    pub fn closed_line_strip_3d(points: Vec<Vec3>, radius: f32, segments: u32) -> Self {
        tube_path_mesh(points, radius, segments, true)
    }

    /// Rerun-style `Points3D`: points expanded to small triangle spheres.
    #[must_use]
    pub fn points_3d(points: Vec<Vec3>, radius: f32, segments: u32) -> Self {
        point_cloud_mesh(points, radius, segments)
    }

    /// Rerun-style `Arrows3D`: a shaft plus cone head, aligned to `vector`.
    #[must_use]
    pub fn arrow_3d(vector: Vec3, radius: f32, segments: u32) -> Self {
        arrow_mesh([0.0, 0.0, 0.0], vector, radius, segments)
    }

    #[must_use]
    pub fn arrow_2d(length: f32, shaft_width: f32, head_length: f32, head_width: f32) -> Self {
        let length = length.max(1.0e-4);
        let head_length = head_length.clamp(0.0, length);
        let shaft_half = shaft_width.max(1.0e-4) * 0.5;
        let head_half = head_width.max(shaft_width) * 0.5;
        let body = length - head_length;
        Self::polygon(vec![
            [0.0, 0.0, -shaft_half],
            [body, 0.0, -shaft_half],
            [body, 0.0, -head_half],
            [length, 0.0, 0.0],
            [body, 0.0, head_half],
            [body, 0.0, shaft_half],
            [0.0, 0.0, shaft_half],
        ])
    }

    #[must_use]
    pub fn cross_2d(width: f32, height: f32, bar: f32) -> Self {
        let hw = width * 0.5;
        let hh = height * 0.5;
        let hb = bar * 0.5;
        Self::polygon(vec![
            [-hb, 0.0, -hh],
            [hb, 0.0, -hh],
            [hb, 0.0, -hb],
            [hw, 0.0, -hb],
            [hw, 0.0, hb],
            [hb, 0.0, hb],
            [hb, 0.0, hh],
            [-hb, 0.0, hh],
            [-hb, 0.0, hb],
            [-hw, 0.0, hb],
            [-hw, 0.0, -hb],
            [-hb, 0.0, -hb],
        ])
    }

    #[must_use]
    pub fn cuboid(size: Vec3) -> Self {
        let hx = size[0] * 0.5;
        let hy = size[1] * 0.5;
        let hz = size[2] * 0.5;
        let mut vertices = Vec::with_capacity(24);
        let mut normals = Vec::with_capacity(24);
        let mut indices = Vec::with_capacity(12);
        let faces = [
            (
                [0.0, 0.0, -1.0],
                [
                    [-hx, -hy, -hz],
                    [hx, -hy, -hz],
                    [hx, hy, -hz],
                    [-hx, hy, -hz],
                ],
            ),
            (
                [0.0, 0.0, 1.0],
                [[-hx, -hy, hz], [-hx, hy, hz], [hx, hy, hz], [hx, -hy, hz]],
            ),
            (
                [0.0, -1.0, 0.0],
                [
                    [-hx, -hy, -hz],
                    [-hx, -hy, hz],
                    [hx, -hy, hz],
                    [hx, -hy, -hz],
                ],
            ),
            (
                [0.0, 1.0, 0.0],
                [[-hx, hy, -hz], [hx, hy, -hz], [hx, hy, hz], [-hx, hy, hz]],
            ),
            (
                [1.0, 0.0, 0.0],
                [[hx, -hy, -hz], [hx, -hy, hz], [hx, hy, hz], [hx, hy, -hz]],
            ),
            (
                [-1.0, 0.0, 0.0],
                [
                    [-hx, -hy, -hz],
                    [-hx, hy, -hz],
                    [-hx, hy, hz],
                    [-hx, -hy, hz],
                ],
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
        Self::Triangles(TriangleMesh3d::with_generated_normals(outline, indices))
    }

    #[must_use]
    pub fn cube(size: f32) -> Self {
        Self::cuboid([size, size, size])
    }

    #[must_use]
    pub fn pyramid(size: f32, height: f32) -> Self {
        let h = size * 0.5;
        let vertices = vec![
            [-h, 0.0, -h],
            [h, 0.0, -h],
            [h, 0.0, h],
            [-h, 0.0, h],
            [0.0, height, 0.0],
        ];
        // Side faces wound so face_normal points outward (apex first
        // for the slanted sides), base faces wound so the normal
        // points down. The original winding pointed every normal
        // inward, which only rendered correctly thanks to the
        // two-sided auto-flip in `shade_color` — that flip cannot
        // pick a stable side when the face is near edge-on to the
        // camera, so adjacent faces ended up shading inconsistently.
        let indices = vec![
            [0, 4, 1],
            [1, 4, 2],
            [2, 4, 3],
            [3, 4, 0],
            [0, 2, 3],
            [0, 1, 2],
        ];
        Self::Triangles(flat_shaded_mesh(&vertices, &indices))
    }

    #[must_use]
    pub fn cone(radius: f32, height: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let r = radius.max(0.0);
        let h = height;
        let slant = (r * r + h * h).sqrt().max(1.0e-5);
        // Slope-tilted normal at angle phi on a cone with apex up and
        // bottom radius r, height h. Derivation: normal = (cos*h, r,
        // sin*h) / sqrt(r²+h²).
        let side_normal =
            |angle: f32| -> Vec3 { [angle.cos() * h / slant, r / slant, angle.sin() * h / slant] };

        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let bottom_center = vertices.len() as u32;
        vertices.push([0.0, -h * 0.5, 0.0]);
        normals.push([0.0, -1.0, 0.0]);
        let bottom_cap = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([r * angle.cos(), -h * 0.5, r * angle.sin()]);
            normals.push([0.0, -1.0, 0.0]);
        }
        // Side ring: bottom verts (smooth around axis) + per-segment
        // apex verts whose normal matches the segment midpoint so the
        // tip of the cone shades continuously with its slanted side.
        let side_ring = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([r * angle.cos(), -h * 0.5, r * angle.sin()]);
            normals.push(side_normal(angle));
        }
        let side_apex = vertices.len() as u32;
        for i in 0..segments {
            let mid = std::f32::consts::TAU * (i as f32 + 0.5) / segments as f32;
            vertices.push([0.0, h * 0.5, 0.0]);
            normals.push(side_normal(mid));
        }

        for i in 0..segments {
            let next = (i + 1) % segments;
            indices.push([
                bottom_center,
                bottom_cap + next as u32,
                bottom_cap + i as u32,
            ]);
            let b0 = side_ring + i as u32;
            let b1 = side_ring + next as u32;
            let apex = side_apex + i as u32;
            indices.push([b0, b1, apex]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn frustum(bottom_radius: f32, top_radius: f32, height: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let rb = bottom_radius.max(0.0);
        let rt = top_radius.max(0.0);
        let hh = height * 0.5;
        // Tilt of the side surface from vertical: dr/dy = (rb - rt) / h.
        // The outward normal at angle phi is (cos*h, rb-rt, sin*h)
        // normalized. This degenerates correctly to a cylinder when
        // rb == rt (ny becomes 0).
        let dr = rb - rt;
        let slant = (dr * dr + height * height).sqrt().max(1.0e-5);
        let nx_scale = height / slant;
        let ny = dr / slant;
        let side_normal =
            |angle: f32| -> Vec3 { [angle.cos() * nx_scale, ny, angle.sin() * nx_scale] };

        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let bottom_center = vertices.len() as u32;
        vertices.push([0.0, -hh, 0.0]);
        normals.push([0.0, -1.0, 0.0]);
        let bottom_cap = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([rb * angle.cos(), -hh, rb * angle.sin()]);
            normals.push([0.0, -1.0, 0.0]);
        }
        let top_center = vertices.len() as u32;
        vertices.push([0.0, hh, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        let top_cap = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([rt * angle.cos(), hh, rt * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
        }
        let side = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let normal = side_normal(angle);
            vertices.push([rb * angle.cos(), -hh, rb * angle.sin()]);
            normals.push(normal);
            vertices.push([rt * angle.cos(), hh, rt * angle.sin()]);
            normals.push(normal);
        }

        for i in 0..segments {
            let next = (i + 1) % segments;
            indices.push([
                bottom_center,
                bottom_cap + next as u32,
                bottom_cap + i as u32,
            ]);
            indices.push([top_center, top_cap + i as u32, top_cap + next as u32]);
            let b0 = side + (i * 2) as u32;
            let t0 = b0 + 1;
            let b1 = side + (next * 2) as u32;
            let t1 = b1 + 1;
            indices.push([b0, b1, t1]);
            indices.push([b0, t1, t0]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn cylinder(radius: f32, height: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let r = radius.max(0.0);
        let hh = height * 0.5;

        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        let bottom_center = vertices.len() as u32;
        vertices.push([0.0, -hh, 0.0]);
        normals.push([0.0, -1.0, 0.0]);
        let bottom_cap = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([r * angle.cos(), -hh, r * angle.sin()]);
            normals.push([0.0, -1.0, 0.0]);
        }
        let top_center = vertices.len() as u32;
        vertices.push([0.0, hh, 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        let top_cap = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            vertices.push([r * angle.cos(), hh, r * angle.sin()]);
            normals.push([0.0, 1.0, 0.0]);
        }
        // Side ring: radial normals, paired bottom/top per angle so the
        // side surface shades smoothly around the axis but stays
        // independent of the flat cap normals.
        let side = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let normal: Vec3 = [angle.cos(), 0.0, angle.sin()];
            vertices.push([r * angle.cos(), -hh, r * angle.sin()]);
            normals.push(normal);
            vertices.push([r * angle.cos(), hh, r * angle.sin()]);
            normals.push(normal);
        }

        for i in 0..segments {
            let next = (i + 1) % segments;
            indices.push([
                bottom_center,
                bottom_cap + next as u32,
                bottom_cap + i as u32,
            ]);
            indices.push([top_center, top_cap + i as u32, top_cap + next as u32]);
            let b0 = side + (i * 2) as u32;
            let t0 = b0 + 1;
            let b1 = side + (next * 2) as u32;
            let t1 = b1 + 1;
            indices.push([b0, b1, t1]);
            indices.push([b0, t1, t0]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn tube(inner_radius: f32, outer_radius: f32, height: f32, segments: u32) -> Self {
        let segments = segments.max(3) as usize;
        let inner_radius = inner_radius.min(outer_radius).max(0.0);
        let outer_radius = outer_radius.max(inner_radius + 1.0e-4);
        let hh = height * 0.5;

        let mut vertices = Vec::new();
        let mut normals = Vec::new();
        let mut indices = Vec::new();

        // Outer wall: outward radial normal, paired bottom/top per
        // angle so it shades smoothly around the axis.
        let outer = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let c = angle.cos();
            let s = angle.sin();
            let normal: Vec3 = [c, 0.0, s];
            vertices.push([outer_radius * c, -hh, outer_radius * s]);
            normals.push(normal);
            vertices.push([outer_radius * c, hh, outer_radius * s]);
            normals.push(normal);
        }
        // Inner wall: inward radial normal.
        let inner = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let c = angle.cos();
            let s = angle.sin();
            let normal: Vec3 = [-c, 0.0, -s];
            vertices.push([inner_radius * c, -hh, inner_radius * s]);
            normals.push(normal);
            vertices.push([inner_radius * c, hh, inner_radius * s]);
            normals.push(normal);
        }
        // Top ring: outer + inner with up-normals.
        let top_ring = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let c = angle.cos();
            let s = angle.sin();
            vertices.push([outer_radius * c, hh, outer_radius * s]);
            normals.push([0.0, 1.0, 0.0]);
            vertices.push([inner_radius * c, hh, inner_radius * s]);
            normals.push([0.0, 1.0, 0.0]);
        }
        // Bottom ring: outer + inner with down-normals.
        let bot_ring = vertices.len() as u32;
        for i in 0..segments {
            let angle = std::f32::consts::TAU * i as f32 / segments as f32;
            let c = angle.cos();
            let s = angle.sin();
            vertices.push([outer_radius * c, -hh, outer_radius * s]);
            normals.push([0.0, -1.0, 0.0]);
            vertices.push([inner_radius * c, -hh, inner_radius * s]);
            normals.push([0.0, -1.0, 0.0]);
        }

        for i in 0..segments {
            let next = (i + 1) % segments;
            // Outer wall
            let o0b = outer + (i * 2) as u32;
            let o0t = o0b + 1;
            let o1b = outer + (next * 2) as u32;
            let o1t = o1b + 1;
            indices.push([o0b, o1b, o1t]);
            indices.push([o0b, o1t, o0t]);
            // Inner wall (winding reversed so the inward face is front)
            let i0b = inner + (i * 2) as u32;
            let i0t = i0b + 1;
            let i1b = inner + (next * 2) as u32;
            let i1t = i1b + 1;
            indices.push([i0b, i0t, i1t]);
            indices.push([i0b, i1t, i1b]);
            // Top annular cap
            let to0 = top_ring + (i * 2) as u32;
            let ti0 = to0 + 1;
            let to1 = top_ring + (next * 2) as u32;
            let ti1 = to1 + 1;
            indices.push([to0, to1, ti1]);
            indices.push([to0, ti1, ti0]);
            // Bottom annular cap (winding reversed for downward facing)
            let bo0 = bot_ring + (i * 2) as u32;
            let bi0 = bo0 + 1;
            let bo1 = bot_ring + (next * 2) as u32;
            let bi1 = bo1 + 1;
            indices.push([bo0, bi1, bo1]);
            indices.push([bo0, bi0, bi1]);
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn tetrahedron(size: f32) -> Self {
        let s = size * 0.5;
        let vertices = vec![[s, s, s], [-s, -s, s], [-s, s, -s], [s, -s, -s]];
        // Winding chosen so face_normal points outward from the
        // centroid (origin). The previous winding pointed inward, which
        // only rendered acceptably via the two-sided auto-flip in
        // `shade_color` and produced inconsistent flat-shaded faces.
        let indices = vec![[0, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        Self::Triangles(flat_shaded_mesh(&vertices, &indices))
    }

    #[must_use]
    pub fn octahedron(size: f32) -> Self {
        let s = size * 0.5;
        let vertices = vec![
            [0.0, s, 0.0],
            [s, 0.0, 0.0],
            [0.0, 0.0, s],
            [-s, 0.0, 0.0],
            [0.0, 0.0, -s],
            [0.0, -s, 0.0],
        ];
        // Winding flipped so face_normal points outward (away from
        // origin) on every face.
        let indices = vec![
            [0, 2, 1],
            [0, 3, 2],
            [0, 4, 3],
            [0, 1, 4],
            [5, 1, 2],
            [5, 2, 3],
            [5, 3, 4],
            [5, 4, 1],
        ];
        Self::Triangles(flat_shaded_mesh(&vertices, &indices))
    }

    #[must_use]
    pub fn icosahedron(radius: f32) -> Self {
        let phi = (1.0 + 5.0_f32.sqrt()) * 0.5;
        let raw = [
            [-1.0, phi, 0.0],
            [1.0, phi, 0.0],
            [-1.0, -phi, 0.0],
            [1.0, -phi, 0.0],
            [0.0, -1.0, phi],
            [0.0, 1.0, phi],
            [0.0, -1.0, -phi],
            [0.0, 1.0, -phi],
            [phi, 0.0, -1.0],
            [phi, 0.0, 1.0],
            [-phi, 0.0, -1.0],
            [-phi, 0.0, 1.0],
        ];
        let vertices = raw
            .into_iter()
            .map(|point| mul3(normalize3(point), radius))
            .collect();
        Self::Triangles(TriangleMesh3d::with_generated_normals(
            vertices,
            vec![
                [0, 11, 5],
                [0, 5, 1],
                [0, 1, 7],
                [0, 7, 10],
                [0, 10, 11],
                [1, 5, 9],
                [5, 11, 4],
                [11, 10, 2],
                [10, 7, 6],
                [7, 1, 8],
                [3, 9, 4],
                [3, 4, 2],
                [3, 2, 6],
                [3, 6, 8],
                [3, 8, 9],
                [4, 9, 5],
                [2, 4, 11],
                [6, 2, 10],
                [8, 6, 7],
                [9, 8, 1],
            ],
        ))
    }

    #[must_use]
    pub fn triangular_prism(width: f32, height: f32, depth: f32) -> Self {
        let hw = width * 0.5;
        let hd = depth * 0.5;
        let vertices = vec![
            [-hw, 0.0, -hd],
            [hw, 0.0, -hd],
            [0.0, height, -hd],
            [-hw, 0.0, hd],
            [hw, 0.0, hd],
            [0.0, height, hd],
        ];
        // Winding chosen so each face's normal points outward:
        // back triangle → -z, front triangle → +z, bottom → -y,
        // left slanted → outward-left, right slanted → outward-right.
        let indices = vec![
            [0, 2, 1],
            [3, 4, 5],
            [0, 4, 3],
            [0, 1, 4],
            [1, 5, 4],
            [1, 2, 5],
            [2, 3, 5],
            [2, 0, 3],
        ];
        Self::Triangles(flat_shaded_mesh(&vertices, &indices))
    }

    #[must_use]
    pub fn wedge(size: Vec3) -> Self {
        let hx = size[0] * 0.5;
        let hy = size[1];
        let hz = size[2] * 0.5;
        let vertices = vec![
            [-hx, 0.0, -hz],
            [hx, 0.0, -hz],
            [-hx, 0.0, hz],
            [hx, 0.0, hz],
            [-hx, hy, hz],
            [hx, hy, hz],
        ];
        // 5 faces, 8 triangles. The previous index list included two
        // reverse-winding duplicates of the left and right side
        // triangles — those caused z-fighting and are dropped here.
        let indices = vec![
            [0, 1, 3],
            [0, 3, 2],
            [2, 3, 5],
            [2, 5, 4],
            [0, 2, 4],
            [0, 4, 1],
            [1, 4, 5],
            [1, 5, 3],
        ];
        Self::Triangles(flat_shaded_mesh(&vertices, &indices))
    }

    #[must_use]
    pub fn torus(
        major_radius: f32,
        minor_radius: f32,
        major_segments: u32,
        minor_segments: u32,
    ) -> Self {
        let major_segments = major_segments.max(3) as usize;
        let minor_segments = minor_segments.max(3) as usize;
        let mut vertices = Vec::with_capacity(major_segments * minor_segments);
        let mut normals = Vec::with_capacity(major_segments * minor_segments);
        let mut indices = Vec::with_capacity(major_segments * minor_segments * 2);

        for major in 0..major_segments {
            let u = std::f32::consts::TAU * major as f32 / major_segments as f32;
            let center = [major_radius * u.cos(), 0.0, major_radius * u.sin()];
            let radial = normalize3([u.cos(), 0.0, u.sin()]);
            for minor in 0..minor_segments {
                let v = std::f32::consts::TAU * minor as f32 / minor_segments as f32;
                let normal = normalize3(add3(mul3(radial, v.cos()), [0.0, v.sin(), 0.0]));
                vertices.push(add3(center, mul3(normal, minor_radius)));
                normals.push(normal);
            }
        }

        let vertex = |major: usize, minor: usize| -> u32 {
            ((major % major_segments) * minor_segments + (minor % minor_segments)) as u32
        };
        for major in 0..major_segments {
            for minor in 0..minor_segments {
                let a = vertex(major, minor);
                let b = vertex(major + 1, minor);
                let c = vertex(major + 1, minor + 1);
                let d = vertex(major, minor + 1);
                indices.push([a, b, c]);
                indices.push([a, c, d]);
            }
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    #[must_use]
    pub fn capsule(radius: f32, height: f32, segments: u32) -> Self {
        let longitude = segments.max(8) as usize;
        let latitude = (longitude / 2).max(4);
        let cylinder_half = (height * 0.5 - radius).max(0.0);
        let mut vertices = Vec::new();
        let mut normals = Vec::new();

        for lat in 0..=latitude {
            let theta = std::f32::consts::FRAC_PI_2 * lat as f32 / latitude as f32;
            let y = cylinder_half + radius * theta.cos();
            let r = radius * theta.sin();
            for lon in 0..longitude {
                let phi = std::f32::consts::TAU * lon as f32 / longitude as f32;
                let normal = normalize3([r * phi.cos(), y - cylinder_half, r * phi.sin()]);
                vertices.push([r * phi.cos(), y, r * phi.sin()]);
                normals.push(normal);
            }
        }
        for lat in 1..=latitude {
            let theta = std::f32::consts::FRAC_PI_2 * lat as f32 / latitude as f32;
            let y = -cylinder_half - radius * theta.sin();
            let r = radius * theta.cos();
            for lon in 0..longitude {
                let phi = std::f32::consts::TAU * lon as f32 / longitude as f32;
                let normal = normalize3([r * phi.cos(), y + cylinder_half, r * phi.sin()]);
                vertices.push([r * phi.cos(), y, r * phi.sin()]);
                normals.push(normal);
            }
        }

        let rings = vertices.len() / longitude;
        let mut indices = Vec::with_capacity((rings.saturating_sub(1)) * longitude * 2);
        let vertex =
            |ring: usize, lon: usize| -> u32 { (ring * longitude + lon % longitude) as u32 };
        for ring in 0..rings.saturating_sub(1) {
            for lon in 0..longitude {
                let a = vertex(ring, lon);
                let b = vertex(ring + 1, lon);
                let c = vertex(ring + 1, lon + 1);
                let d = vertex(ring, lon + 1);
                indices.push([a, b, c]);
                indices.push([a, c, d]);
            }
        }
        Self::Triangles(TriangleMesh3d::with_normals(vertices, indices, normals))
    }

    /// Rerun-style `Ellipsoids3D`: a sphere scaled by half-size on each axis.
    #[must_use]
    pub fn ellipsoid(half_sizes: Vec3, segments: u32) -> Self {
        let longitude = segments.max(8) as usize;
        let latitude = (longitude / 2).max(4);
        let radii = [
            half_sizes[0].abs().max(1.0e-5),
            half_sizes[1].abs().max(1.0e-5),
            half_sizes[2].abs().max(1.0e-5),
        ];
        let mut vertices = Vec::with_capacity(2 + (latitude - 1) * longitude);
        let mut normals = Vec::with_capacity(2 + (latitude - 1) * longitude);
        vertices.push([0.0, radii[1], 0.0]);
        normals.push([0.0, 1.0, 0.0]);
        for lat in 1..latitude {
            let theta = std::f32::consts::PI * lat as f32 / latitude as f32;
            let y = theta.cos();
            let r = theta.sin();
            for lon in 0..longitude {
                let phi = std::f32::consts::TAU * lon as f32 / longitude as f32;
                let unit = [r * phi.cos(), y, r * phi.sin()];
                vertices.push([unit[0] * radii[0], unit[1] * radii[1], unit[2] * radii[2]]);
                normals.push(normalize3([
                    unit[0] / radii[0],
                    unit[1] / radii[1],
                    unit[2] / radii[2],
                ]));
            }
        }
        let bottom = vertices.len() as u32;
        vertices.push([0.0, -radii[1], 0.0]);
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

fn point_cloud_mesh(points: Vec<Vec3>, radius: f32, segments: u32) -> Primitive3d {
    let mut out = TriangleMesh3d::new(Vec::new(), Vec::new());
    let radius = radius.max(1.0e-4);
    for point in points {
        append_primitive_transformed(
            &mut out,
            &Primitive3d::sphere(radius, segments.max(8)),
            &Transform3d {
                translation: point,
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        );
    }
    Primitive3d::Triangles(out)
}

fn tube_path_mesh(points: Vec<Vec3>, radius: f32, segments: u32, closed: bool) -> Primitive3d {
    let points = if closed && points.len() > 2 {
        let mut p = points;
        p.push(p[0]);
        p
    } else {
        points
    };
    let mut out = TriangleMesh3d::new(Vec::new(), Vec::new());
    let radius = radius.max(1.0e-4);
    let segments = segments.max(6);
    for pair in points.windows(2) {
        append_cylinder_between(&mut out, pair[0], pair[1], radius, segments);
    }
    for point in points {
        append_primitive_transformed(
            &mut out,
            &Primitive3d::sphere(radius * 1.05, segments.max(8)),
            &Transform3d {
                translation: point,
                rotation_xyzw: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            },
        );
    }
    Primitive3d::Triangles(out)
}

fn arrow_mesh(origin: Vec3, vector: Vec3, radius: f32, segments: u32) -> Primitive3d {
    let length = dot3(vector, vector).sqrt();
    if length <= 1.0e-5 {
        return Primitive3d::mesh(Vec::new(), Vec::new());
    }
    let direction = mul3(vector, length.recip());
    let head_len = (length * 0.24).clamp(radius * 3.0, length * 0.55);
    let shaft_end = add3(origin, mul3(direction, length - head_len));
    let tip = add3(origin, vector);
    let mut out = TriangleMesh3d::new(Vec::new(), Vec::new());
    append_cylinder_between(
        &mut out,
        origin,
        shaft_end,
        radius.max(1.0e-4),
        segments.max(8),
    );
    append_cone_between(
        &mut out,
        shaft_end,
        tip,
        radius.max(1.0e-4) * 2.45,
        segments.max(8),
    );
    Primitive3d::Triangles(out)
}

fn append_primitive_transformed(
    out: &mut TriangleMesh3d,
    primitive: &Primitive3d,
    transform: &Transform3d,
) {
    let Primitive3d::Triangles(mesh) = primitive;
    let base = out.vertices.len() as u32;
    out.vertices.extend(
        mesh.vertices
            .iter()
            .copied()
            .map(|vertex| transform_point(transform, vertex)),
    );
    if mesh.normals.len() == mesh.vertices.len() {
        out.normals.extend(
            mesh.normals
                .iter()
                .copied()
                .map(|normal| transform_normal(transform, normal)),
        );
    }
    if mesh.uvs.len() == mesh.vertices.len() {
        out.uvs.extend(mesh.uvs.iter().copied());
    }
    if mesh.vertex_colors.len() == mesh.vertices.len() {
        out.vertex_colors.extend(mesh.vertex_colors.iter().copied());
    }
    out.indices.extend(
        mesh.indices
            .iter()
            .map(|triangle| [triangle[0] + base, triangle[1] + base, triangle[2] + base]),
    );
}

fn append_cylinder_between(out: &mut TriangleMesh3d, a: Vec3, b: Vec3, radius: f32, segments: u32) {
    let axis = sub3(b, a);
    let length = dot3(axis, axis).sqrt();
    if length <= 1.0e-5 {
        return;
    }
    let dir = mul3(axis, length.recip());
    let (side, up) = orthonormal_basis(dir);
    let base = out.vertices.len() as u32;
    let segments = segments.max(3) as usize;
    for i in 0..segments {
        let angle = std::f32::consts::TAU * i as f32 / segments as f32;
        let normal = normalize3(add3(mul3(side, angle.cos()), mul3(up, angle.sin())));
        out.vertices.push(add3(a, mul3(normal, radius)));
        out.vertices.push(add3(b, mul3(normal, radius)));
        out.normals.push(normal);
        out.normals.push(normal);
    }
    let bottom_center = out.vertices.len() as u32;
    out.vertices.push(a);
    out.normals.push(mul3(dir, -1.0));
    let top_center = out.vertices.len() as u32;
    out.vertices.push(b);
    out.normals.push(dir);
    for i in 0..segments {
        let next = (i + 1) % segments;
        let b0 = base + (i * 2) as u32;
        let t0 = b0 + 1;
        let b1 = base + (next * 2) as u32;
        let t1 = b1 + 1;
        out.indices.push([b0, b1, t1]);
        out.indices.push([b0, t1, t0]);
        out.indices.push([bottom_center, b1, b0]);
        out.indices.push([top_center, t0, t1]);
    }
}

fn append_cone_between(
    out: &mut TriangleMesh3d,
    base_center: Vec3,
    tip: Vec3,
    radius: f32,
    segments: u32,
) {
    let axis = sub3(tip, base_center);
    let length = dot3(axis, axis).sqrt();
    if length <= 1.0e-5 {
        return;
    }
    let dir = mul3(axis, length.recip());
    let (side, up) = orthonormal_basis(dir);
    let base = out.vertices.len() as u32;
    let segments = segments.max(3) as usize;
    out.vertices.push(base_center);
    out.normals.push(mul3(dir, -1.0));
    let tip_index = out.vertices.len() as u32;
    out.vertices.push(tip);
    out.normals.push(dir);
    for i in 0..segments {
        let angle = std::f32::consts::TAU * i as f32 / segments as f32;
        let radial = normalize3(add3(mul3(side, angle.cos()), mul3(up, angle.sin())));
        out.vertices.push(add3(base_center, mul3(radial, radius)));
        out.normals
            .push(normalize3(add3(radial, mul3(dir, radius / length))));
    }
    for i in 0..segments {
        let next = (i + 1) % segments;
        let a = base + 2 + i as u32;
        let b = base + 2 + next as u32;
        out.indices.push([a, b, tip_index]);
        out.indices.push([base, b, a]);
    }
}

fn orthonormal_basis(direction: Vec3) -> (Vec3, Vec3) {
    let direction = normalize3(direction);
    let helper = if direction[1].abs() < 0.92 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let side = normalize3(cross3(direction, helper));
    let up = normalize3(cross3(side, direction));
    (side, up)
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
    pub albedo_texture: Option<TextureId>,
    pub roughness: f32,
    pub metallic: f32,
}

impl Material3d {
    #[must_use]
    pub fn new(id: MaterialId, name: impl Into<String>, base_color: impl Into<Color>) -> Self {
        Self {
            id,
            name: name.into(),
            base_color: base_color.into(),
            albedo_texture: None,
            roughness: 0.65,
            metallic: 0.0,
        }
    }

    #[must_use]
    pub const fn with_texture(mut self, texture: TextureId) -> Self {
        self.albedo_texture = Some(texture);
        self
    }
}

/// Retained CPU-visible texture data for mesh UVs.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture3d {
    pub id: TextureId,
    pub name: String,
    pub size: [usize; 2],
    pub pixels: Vec<Color>,
}

impl Texture3d {
    #[must_use]
    pub fn new(
        id: TextureId,
        name: impl Into<String>,
        size: [usize; 2],
        pixels: Vec<Color>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            size,
            pixels,
        }
    }

    #[must_use]
    pub fn checker(
        id: TextureId,
        name: impl Into<String>,
        size: [usize; 2],
        a: Color,
        b: Color,
        cells: usize,
    ) -> Self {
        let width = size[0].max(1);
        let height = size[1].max(1);
        let cells = cells.max(1);
        let mut pixels = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let cx = x * cells / width;
                let cy = y * cells / height;
                pixels.push(if (cx + cy).is_multiple_of(2) { a } else { b });
            }
        }
        Self::new(id, name, [width, height], pixels)
    }

    #[must_use]
    pub fn sample(&self, uv: [f32; 2]) -> Color {
        let width = self.size[0].max(1);
        let height = self.size[1].max(1);
        if self.pixels.is_empty() {
            return MaraColor32::WHITE;
        }
        let u = uv[0].rem_euclid(1.0);
        let v = uv[1].rem_euclid(1.0);
        let x = (u * width as f32).floor().clamp(0.0, (width - 1) as f32) as usize;
        let y = ((1.0 - v) * height as f32)
            .floor()
            .clamp(0.0, (height - 1) as f32) as usize;
        self.pixels
            .get(y * width + x)
            .copied()
            .unwrap_or(MaraColor32::WHITE)
    }
}

/// A retained object in a 3D scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Object3d {
    pub id: ObjectId,
    pub name: String,
    pub primitive: Primitive3d,
    pub transform: Transform3d,
    pub instances: Vec<Transform3d>,
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
            instances: Vec::new(),
            material,
            selected: false,
            visible: true,
        }
    }

    #[must_use]
    pub fn kind_name(&self) -> &'static str {
        self.primitive.kind_name()
    }

    #[must_use]
    pub fn with_instances(mut self, instances: Vec<Transform3d>) -> Self {
        self.instances = instances;
        self
    }
}

/// Simple colored always-on-top visual drawing, separate from real mesh
/// primitives. Use this for Bevy-style gizmo/debug/annotation marks.
#[derive(Clone, Debug, PartialEq)]
pub struct Gizmo3d {
    pub id: GizmoId,
    pub name: String,
    pub kind: Gizmo3dKind,
    pub style: Gizmo3dStyle,
    pub visible: bool,
}

impl Gizmo3d {
    #[must_use]
    pub fn new(id: GizmoId, name: impl Into<String>, kind: Gizmo3dKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            style: Gizmo3dStyle::default(),
            visible: true,
        }
    }

    #[must_use]
    pub fn with_style(mut self, style: Gizmo3dStyle) -> Self {
        self.style = style;
        self
    }
}

/// Bevy-style immediate/overlay primitives. These are not textured meshes.
#[derive(Clone, Debug, PartialEq)]
pub enum Gizmo3dKind {
    Dot {
        position: Vec3,
    },
    Line {
        a: Vec3,
        b: Vec3,
    },
    Segment {
        a: Vec3,
        b: Vec3,
    },
    Polyline {
        points: Vec<Vec3>,
    },
    Polygon {
        points: Vec<Vec3>,
        closed: bool,
    },
    Rectangle {
        center: Vec3,
        size: [f32; 2],
    },
    Circle {
        center: Vec3,
        radius: f32,
    },
    Ellipse {
        center: Vec3,
        radii: [f32; 2],
    },
    Arc {
        center: Vec3,
        radius: f32,
        start: f32,
        end: f32,
    },
    Axes {
        origin: Vec3,
        size: f32,
    },
}

/// Styling for [`Gizmo3d`] overlay drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Gizmo3dStyle {
    pub color: Color,
    pub width: f32,
    pub radius: f32,
}

impl Gizmo3dStyle {
    #[must_use]
    pub fn new(color: impl Into<Color>) -> Self {
        Self {
            color: color.into(),
            width: 2.0,
            radius: 4.0,
        }
    }

    #[must_use]
    pub const fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    #[must_use]
    pub const fn with_radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

impl Default for Gizmo3dStyle {
    fn default() -> Self {
        Self::new(MaraColor32::WHITE)
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
    pub textures: Vec<Texture3d>,
    pub objects: Vec<Object3d>,
    pub gizmos: Vec<Gizmo3d>,
    pub lights: Vec<Light3d>,
    next_object_id: u64,
    next_gizmo_id: u64,
    next_material_id: u64,
    next_texture_id: u64,
    next_light_id: u64,
}

impl Scene3d {
    #[must_use]
    pub fn new(title: impl Into<String>) -> Self {
        let accent: egui::Color32 = mara_core::style::active_accent().into();
        Self {
            title: title.into(),
            camera: Camera3d::default(),
            background: mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent),
            materials: vec![Material3d::new(MaterialId(1), "Accent", accent)],
            textures: Vec::new(),
            objects: Vec::new(),
            gizmos: Vec::new(),
            lights: vec![
                Light3d::Ambient {
                    id: LightId(1),
                    name: "Ambient".to_owned(),
                    color: MaraColor32::WHITE,
                    intensity: 0.35,
                },
                Light3d::Directional {
                    id: LightId(2),
                    name: "Key".to_owned(),
                    direction: [-0.6, -1.0, -0.45],
                    color: MaraColor32::WHITE,
                    intensity: 1.4,
                },
            ],
            next_object_id: 1,
            next_gizmo_id: 1,
            next_material_id: 2,
            next_texture_id: 1,
            next_light_id: 3,
        }
    }

    #[must_use]
    pub fn demo(title: impl Into<String>) -> Self {
        let mut scene = Self::new(title);
        let accent: egui::Color32 = mara_core::style::active_accent().into();
        let mint = scene.add_material(
            "Mint",
            tint_color(egui::Color32::from_rgb(68, 230, 160), accent, 0.14),
        );
        let sky = scene.add_material(
            "Sky",
            tint_color(egui::Color32::from_rgb(76, 166, 255), accent, 0.12),
        );
        let amber = scene.add_material(
            "Amber",
            tint_color(egui::Color32::from_rgb(255, 184, 72), accent, 0.12),
        );
        let violet = scene.add_material(
            "Violet",
            tint_color(egui::Color32::from_rgb(178, 116, 255), accent, 0.14),
        );
        let rose = scene.add_material(
            "Rose",
            tint_color(egui::Color32::from_rgb(255, 92, 172), accent, 0.12),
        );
        let lime = scene.add_material(
            "Lime",
            tint_color(egui::Color32::from_rgb(172, 245, 80), accent, 0.12),
        );
        let cyan = scene.add_material(
            "Cyan",
            tint_color(egui::Color32::from_rgb(64, 220, 245), accent, 0.12),
        );
        let graphite = scene.add_material(
            "Graphite",
            tint_color(egui::Color32::from_rgb(112, 128, 152), accent, 0.18),
        );
        let crystal_blue = scene.add_material(
            "Crystal blue",
            tint_color(egui::Color32::from_rgb(65, 210, 255), accent, 0.10),
        );
        let crystal_purple = scene.add_material(
            "Crystal purple",
            tint_color(egui::Color32::from_rgb(185, 105, 255), accent, 0.12),
        );
        let crystal_gold = scene.add_material(
            "Crystal gold",
            tint_color(egui::Color32::from_rgb(255, 190, 70), accent, 0.10),
        );
        let checker_texture = scene.add_checker_texture(
            "Checker texture",
            [512, 512],
            tint_color(egui::Color32::from_rgb(245, 245, 245), accent, 0.10),
            tint_color(egui::Color32::from_rgb(38, 45, 58), accent, 0.25),
            16,
        );
        let checker =
            scene.add_material_with_texture("Checker mesh", egui::Color32::WHITE, checker_texture);
        let cube = scene.add_object("Cube", Primitive3d::cube(1.0), MaterialId(1));
        if let Some(object) = scene.object_mut(cube) {
            object.transform.translation = [-0.75, 0.55, 0.0];
            object.selected = true;
        }
        let small = scene.add_object("Small cube", Primitive3d::cube(0.65), mint);
        if let Some(object) = scene.object_mut(small) {
            object.transform.translation = [0.85, 0.35, -0.35];
        }
        let sphere = scene.add_object("Sphere", Primitive3d::sphere(0.38, 32), sky);
        if let Some(object) = scene.object_mut(sphere) {
            object.transform.translation = [0.35, 0.38, 0.85];
        }
        let triangle = scene.add_object(
            "Triangle",
            Primitive3d::triangle([[-0.45, 0.02, -0.35], [0.45, 0.02, -0.25], [0.0, 0.02, 0.45]]),
            amber,
        );
        if let Some(object) = scene.object_mut(triangle) {
            object.transform.translation = [-0.15, 0.02, -1.35];
        }
        let spiral = scene.add_object("Spiral polygon", spiral_polygon_mesh(), violet);
        if let Some(object) = scene.object_mut(spiral) {
            object.transform.translation = [1.25, 0.0, 0.85];
        }
        let mesh = scene.add_object(
            "Mesh pyramid",
            Primitive3d::mesh_with_uvs(
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
                vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]],
            ),
            checker,
        );
        if let Some(object) = scene.object_mut(mesh) {
            object.transform.translation = [-1.4, 0.02, 1.05];
        }
        let plane = scene.add_object("Plane", Primitive3d::plane(0.9, 0.55), graphite);
        if let Some(object) = scene.object_mut(plane) {
            object.transform.translation = [-2.45, 0.025, -1.0];
        }
        let disc = scene.add_object("Disc", Primitive3d::disc(0.34, 48), rose);
        if let Some(object) = scene.object_mut(disc) {
            object.transform.translation = [-1.65, 0.03, -1.05];
        }
        let cuboid = scene.add_object("Cuboid", Primitive3d::cuboid([0.95, 0.42, 0.38]), cyan);
        if let Some(object) = scene.object_mut(cuboid) {
            object.transform.translation = [-2.35, 0.24, 0.15];
        }
        let pyramid = scene.add_object("Pyramid", Primitive3d::pyramid(0.72, 0.78), amber);
        if let Some(object) = scene.object_mut(pyramid) {
            object.transform.translation = [2.15, 0.03, -0.82];
        }
        let cone = scene.add_object("Cone", Primitive3d::cone(0.36, 0.9, 48), lime);
        if let Some(object) = scene.object_mut(cone) {
            object.transform.translation = [2.25, 0.48, 0.08];
        }
        let cylinder = scene.add_object("Cylinder", Primitive3d::cylinder(0.28, 0.82, 48), sky);
        if let Some(object) = scene.object_mut(cylinder) {
            object.transform.translation = [1.8, 0.44, 1.05];
        }
        let torus = scene.add_object("Torus", Primitive3d::torus(0.36, 0.095, 56, 18), rose);
        if let Some(object) = scene.object_mut(torus) {
            object.transform.translation = [-0.55, 0.68, 1.65];
        }
        let capsule = scene.add_object("Capsule", Primitive3d::capsule(0.22, 0.92, 32), mint);
        if let Some(object) = scene.object_mut(capsule) {
            object.transform.translation = [0.9, 0.52, 1.7];
        }
        let ellipsoid = scene.add_object(
            "Ellipsoid",
            Primitive3d::ellipsoid([0.52, 0.24, 0.34], 32),
            violet,
        );
        if let Some(object) = scene.object_mut(ellipsoid) {
            object.transform.translation = [3.05, 0.38, 1.75];
        }
        let arrow3d = scene.add_object(
            "3D arrow",
            Primitive3d::arrow_3d([0.85, 0.55, -0.35], 0.055, 24),
            amber,
        );
        if let Some(object) = scene.object_mut(arrow3d) {
            object.transform.translation = [2.85, 0.22, 2.35];
        }
        let line3d = scene.add_object(
            "LineStrips3D tube",
            Primitive3d::line_strip_3d(
                vec![
                    [-0.45, 0.0, -0.15],
                    [-0.18, 0.32, 0.22],
                    [0.18, 0.18, -0.08],
                    [0.46, 0.48, 0.28],
                ],
                0.035,
                16,
            ),
            lime,
        );
        if let Some(object) = scene.object_mut(line3d) {
            object.transform.translation = [3.08, 0.12, -1.1];
        }
        let points3d = scene.add_object(
            "Points3D cloud",
            Primitive3d::points_3d(
                (0..18)
                    .map(|i| {
                        let angle = std::f32::consts::TAU * i as f32 / 18.0;
                        [
                            0.34 * angle.cos(),
                            0.12 * (i % 4) as f32,
                            0.34 * angle.sin(),
                        ]
                    })
                    .collect(),
                0.045,
                10,
            ),
            rose,
        );
        if let Some(object) = scene.object_mut(points3d) {
            object.transform.translation = [2.25, 0.28, -1.42];
        }
        let instanced = scene.add_instanced_object(
            "InstancePoses3D ellipsoids",
            Primitive3d::ellipsoid([0.13, 0.22, 0.13], 18),
            cyan,
            vec![
                Transform3d {
                    translation: [-0.38, 0.18, 0.0],
                    ..Default::default()
                },
                Transform3d {
                    translation: [0.0, 0.32, 0.18],
                    scale: [1.25, 0.75, 1.25],
                    ..Default::default()
                },
                Transform3d {
                    translation: [0.38, 0.18, -0.04],
                    ..Default::default()
                },
            ],
        );
        if let Some(object) = scene.object_mut(instanced) {
            object.transform.translation = [1.25, 0.04, 2.45];
        }
        let colored_mesh = scene.add_object(
            "Vertex-color mesh",
            Primitive3d::mesh_with_vertex_colors(
                vec![
                    [-0.34, 0.0, -0.34],
                    [0.34, 0.0, -0.34],
                    [0.34, 0.0, 0.34],
                    [-0.34, 0.0, 0.34],
                    [0.0, 0.52, 0.0],
                ],
                vec![
                    [0, 4, 1],
                    [1, 4, 2],
                    [2, 4, 3],
                    [3, 4, 0],
                    [0, 2, 3],
                    [0, 1, 2],
                ],
                vec![
                    MaraColor32::from_rgb(255, 80, 120),
                    MaraColor32::from_rgb(255, 220, 80),
                    MaraColor32::from_rgb(80, 235, 170),
                    MaraColor32::from_rgb(80, 170, 255),
                    MaraColor32::from_rgb(240, 120, 255),
                ],
            ),
            MaterialId(1),
        );
        if let Some(object) = scene.object_mut(colored_mesh) {
            object.transform.translation = [3.15, 0.04, -1.9];
        }
        let square = scene.add_object("2D square", Primitive3d::square(0.46), cyan);
        if let Some(object) = scene.object_mut(square) {
            object.transform.translation = [-3.05, 0.035, -1.85];
        }
        let pentagon = scene.add_object("2D polygon", Primitive3d::regular_polygon(0.29, 5), amber);
        if let Some(object) = scene.object_mut(pentagon) {
            object.transform.translation = [-2.35, 0.035, -1.85];
        }
        let star = scene.add_object("2D star", Primitive3d::star(0.34, 0.15, 5), rose);
        if let Some(object) = scene.object_mut(star) {
            object.transform.translation = [-1.65, 0.035, -1.88];
        }
        let annulus = scene.add_object("2D annulus", Primitive3d::annulus(0.16, 0.34, 48), lime);
        if let Some(object) = scene.object_mut(annulus) {
            object.transform.translation = [-0.9, 0.035, -1.9];
        }
        let arrow = scene.add_object(
            "2D arrow",
            Primitive3d::arrow_2d(0.75, 0.16, 0.28, 0.42),
            sky,
        );
        if let Some(object) = scene.object_mut(arrow) {
            object.transform.translation = [-0.12, 0.035, -1.92];
        }
        let cross = scene.add_object("2D cross", Primitive3d::cross_2d(0.58, 0.58, 0.18), violet);
        if let Some(object) = scene.object_mut(cross) {
            object.transform.translation = [0.82, 0.035, -1.88];
        }
        let polyline = scene.add_object(
            "2D polyline",
            Primitive3d::polyline(
                vec![
                    [-0.36, 0.0, -0.18],
                    [-0.12, 0.0, 0.22],
                    [0.18, 0.0, -0.08],
                    [0.42, 0.0, 0.24],
                ],
                0.085,
            ),
            mint,
        );
        if let Some(object) = scene.object_mut(polyline) {
            object.transform.translation = [1.62, 0.035, -1.9];
        }
        let closed_path = scene.add_object(
            "2D path",
            Primitive3d::closed_polyline(
                vec![
                    [-0.28, 0.0, -0.2],
                    [0.25, 0.0, -0.24],
                    [0.36, 0.0, 0.18],
                    [-0.05, 0.0, 0.34],
                    [-0.36, 0.0, 0.08],
                ],
                0.07,
            ),
            graphite,
        );
        if let Some(object) = scene.object_mut(closed_path) {
            object.transform.translation = [2.48, 0.035, -1.9];
        }
        let tetra = scene.add_object("Tetrahedron", Primitive3d::tetrahedron(0.62), rose);
        if let Some(object) = scene.object_mut(tetra) {
            object.transform.translation = [-3.05, 0.44, 0.9];
        }
        let octa = scene.add_object("Octahedron", Primitive3d::octahedron(0.72), lime);
        if let Some(object) = scene.object_mut(octa) {
            object.transform.translation = [-2.35, 0.46, 1.72];
        }
        let ico = scene.add_object("Icosahedron", Primitive3d::icosahedron(0.38), cyan);
        if let Some(object) = scene.object_mut(ico) {
            object.transform.translation = [-1.55, 0.48, 2.1];
        }
        let frustum = scene.add_object("Frustum", Primitive3d::frustum(0.36, 0.2, 0.78, 48), amber);
        if let Some(object) = scene.object_mut(frustum) {
            object.transform.translation = [1.7, 0.44, 2.05];
        }
        let tube = scene.add_object("Tube", Primitive3d::tube(0.18, 0.34, 0.72, 48), violet);
        if let Some(object) = scene.object_mut(tube) {
            object.transform.translation = [2.48, 0.42, 1.72];
        }
        let prism = scene.add_object(
            "Triangular prism",
            Primitive3d::triangular_prism(0.62, 0.56, 0.55),
            graphite,
        );
        if let Some(object) = scene.object_mut(prism) {
            object.transform.translation = [3.1, 0.05, 0.85];
        }
        let wedge = scene.add_object("Wedge", Primitive3d::wedge([0.72, 0.54, 0.54]), mint);
        if let Some(object) = scene.object_mut(wedge) {
            object.transform.translation = [3.08, 0.05, -0.08];
        }
        let imported = scene.add_mesh_object("OBJ-like mesh", obj_like_demo_mesh(), checker);
        if let Some(object) = scene.object_mut(imported) {
            object.transform.translation = [0.12, 1.05, -2.45];
        }
        let crystal_a = scene.add_mesh_object(
            "OBJ-like crystal blue",
            low_poly_crystal_mesh(0.28, 1.05, 6, 0.0),
            crystal_blue,
        );
        if let Some(object) = scene.object_mut(crystal_a) {
            object.transform.translation = [-1.1, 0.02, -2.75];
        }
        let crystal_b = scene.add_mesh_object(
            "OBJ-like crystal purple",
            low_poly_crystal_mesh(0.22, 0.78, 5, 0.35),
            crystal_purple,
        );
        if let Some(object) = scene.object_mut(crystal_b) {
            object.transform.translation = [-0.72, 0.02, -2.62];
            object.transform.rotation_xyzw = axis_angle_quat(WORLD_UP, 0.35);
        }
        let crystal_c = scene.add_mesh_object(
            "OBJ-like crystal gold",
            low_poly_crystal_mesh(0.18, 0.62, 7, -0.22),
            crystal_gold,
        );
        if let Some(object) = scene.object_mut(crystal_c) {
            object.transform.translation = [-1.42, 0.02, -2.48];
            object.transform.rotation_xyzw = axis_angle_quat(WORLD_UP, -0.45);
        }
        let _ = scene.add_gizmo_with_style(
            "Gizmo trajectory",
            Gizmo3dKind::Polyline {
                points: vec![
                    [-2.8, 0.82, -0.65],
                    [-1.75, 1.0, -0.2],
                    [-0.65, 0.78, 0.25],
                    [0.35, 1.05, 0.2],
                    [1.35, 0.92, -0.25],
                ],
            },
            Gizmo3dStyle::new(tint_color(
                egui::Color32::from_rgb(255, 210, 72),
                accent,
                0.18,
            ))
            .with_width(3.0),
        );
        let _ = scene.add_gizmo_with_style(
            "Gizmo target",
            Gizmo3dKind::Circle {
                center: [1.35, 0.92, -0.25],
                radius: 0.24,
            },
            Gizmo3dStyle::new(tint_color(
                egui::Color32::from_rgb(255, 90, 128),
                accent,
                0.14,
            ))
            .with_width(2.4),
        );
        let _ = scene.add_gizmo_with_style(
            "Gizmo dot",
            Gizmo3dKind::Dot {
                position: [-2.8, 0.82, -0.65],
            },
            Gizmo3dStyle::new(tint_color(
                egui::Color32::from_rgb(80, 235, 170),
                accent,
                0.12,
            ))
            .with_radius(5.5),
        );
        let _ = scene.add_gizmo_with_style(
            "Gizmo polygon",
            Gizmo3dKind::Polygon {
                points: vec![
                    [-0.42, 1.28, -0.5],
                    [0.1, 1.44, -0.42],
                    [0.38, 1.22, -0.1],
                    [-0.18, 1.16, 0.18],
                ],
                closed: true,
            },
            Gizmo3dStyle::new(tint_color(
                egui::Color32::from_rgb(120, 180, 255),
                accent,
                0.18,
            ))
            .with_width(2.0),
        );
        scene
    }

    #[must_use]
    pub fn add_material(
        &mut self,
        name: impl Into<String>,
        base_color: impl Into<Color>,
    ) -> MaterialId {
        let id = MaterialId(self.next_material_id);
        self.next_material_id += 1;
        self.materials.push(Material3d::new(id, name, base_color));
        id
    }

    #[must_use]
    pub fn add_material_with_texture(
        &mut self,
        name: impl Into<String>,
        base_color: impl Into<Color>,
        texture: TextureId,
    ) -> MaterialId {
        let id = MaterialId(self.next_material_id);
        self.next_material_id += 1;
        self.materials
            .push(Material3d::new(id, name, base_color).with_texture(texture));
        id
    }

    #[must_use]
    pub fn add_texture(
        &mut self,
        name: impl Into<String>,
        size: [usize; 2],
        pixels: Vec<Color>,
    ) -> TextureId {
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.textures.push(Texture3d::new(id, name, size, pixels));
        id
    }

    #[must_use]
    pub fn add_checker_texture(
        &mut self,
        name: impl Into<String>,
        size: [usize; 2],
        a: impl Into<Color>,
        b: impl Into<Color>,
        cells: usize,
    ) -> TextureId {
        let id = TextureId(self.next_texture_id);
        self.next_texture_id += 1;
        self.textures.push(Texture3d::checker(
            id,
            name,
            size,
            a.into(),
            b.into(),
            cells,
        ));
        id
    }

    #[must_use]
    pub fn material(&self, id: MaterialId) -> Option<&Material3d> {
        self.materials.iter().find(|candidate| candidate.id == id)
    }

    #[must_use]
    pub fn texture(&self, id: TextureId) -> Option<&Texture3d> {
        self.textures.iter().find(|candidate| candidate.id == id)
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
    pub fn add_mesh_object(
        &mut self,
        name: impl Into<String>,
        mesh: TriangleMesh3d,
        material: MaterialId,
    ) -> ObjectId {
        self.add_object(name, Primitive3d::tri_mesh(mesh), material)
    }

    #[must_use]
    pub fn add_instanced_object(
        &mut self,
        name: impl Into<String>,
        primitive: Primitive3d,
        material: MaterialId,
        instances: Vec<Transform3d>,
    ) -> ObjectId {
        let id = ObjectId(self.next_object_id);
        self.next_object_id += 1;
        self.objects
            .push(Object3d::new(id, name, primitive, material).with_instances(instances));
        id
    }

    #[must_use]
    pub fn add_gizmo(&mut self, name: impl Into<String>, kind: Gizmo3dKind) -> GizmoId {
        let id = GizmoId(self.next_gizmo_id);
        self.next_gizmo_id += 1;
        self.gizmos.push(Gizmo3d::new(id, name, kind));
        id
    }

    #[must_use]
    pub fn add_gizmo_with_style(
        &mut self,
        name: impl Into<String>,
        kind: Gizmo3dKind,
        style: Gizmo3dStyle,
    ) -> GizmoId {
        let id = GizmoId(self.next_gizmo_id);
        self.next_gizmo_id += 1;
        self.gizmos
            .push(Gizmo3d::new(id, name, kind).with_style(style));
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

fn obj_like_demo_mesh() -> TriangleMesh3d {
    let vertices = vec![
        [-0.42, 0.0, -0.42],
        [0.42, 0.0, -0.42],
        [0.5, 0.0, 0.22],
        [0.0, 0.0, 0.54],
        [-0.5, 0.0, 0.22],
        [-0.24, 0.55, -0.18],
        [0.24, 0.55, -0.18],
        [0.0, 0.68, 0.26],
        [0.0, 1.0, 0.0],
    ];
    let indices = vec![
        [0, 1, 2],
        [0, 2, 3],
        [0, 3, 4],
        [0, 5, 6],
        [0, 6, 1],
        [1, 6, 7],
        [1, 7, 2],
        [2, 7, 3],
        [3, 7, 5],
        [3, 5, 4],
        [4, 5, 0],
        [5, 8, 6],
        [6, 8, 7],
        [7, 8, 5],
    ];
    let uvs = vec![
        [0.0, 0.0],
        [1.0, 0.0],
        [1.0, 0.55],
        [0.5, 1.0],
        [0.0, 0.55],
        [0.25, 0.25],
        [0.75, 0.25],
        [0.5, 0.75],
        [0.5, 0.5],
    ];
    TriangleMesh3d::with_generated_normals_and_uvs(vertices, indices, uvs)
}

fn low_poly_crystal_mesh(radius: f32, height: f32, sides: u32, twist: f32) -> TriangleMesh3d {
    let sides = sides.max(3) as usize;
    let lower_y = height * 0.12;
    let upper_y = height * 0.72;
    let bottom = 0_u32;
    let top = 1_u32;
    let mut vertices = Vec::with_capacity(2 + sides * 2);
    let mut uvs = Vec::with_capacity(2 + sides * 2);
    vertices.push([0.0, 0.0, 0.0]);
    vertices.push([0.0, height, 0.0]);
    uvs.push([0.5, 0.0]);
    uvs.push([0.5, 1.0]);

    for i in 0..sides {
        let angle = std::f32::consts::TAU * i as f32 / sides as f32;
        vertices.push([
            radius * 0.78 * angle.cos(),
            lower_y,
            radius * 0.78 * angle.sin(),
        ]);
        uvs.push([i as f32 / sides as f32, 0.18]);
    }
    for i in 0..sides {
        let angle = std::f32::consts::TAU * i as f32 / sides as f32 + twist;
        vertices.push([radius * angle.cos(), upper_y, radius * angle.sin()]);
        uvs.push([i as f32 / sides as f32, 0.74]);
    }

    let lower = |i: usize| -> u32 { 2 + (i % sides) as u32 };
    let upper = |i: usize| -> u32 { 2 + sides as u32 + (i % sides) as u32 };
    let mut indices = Vec::with_capacity(sides * 4);
    for i in 0..sides {
        indices.push([bottom, lower(i + 1), lower(i)]);
        indices.push([lower(i), lower(i + 1), upper(i + 1)]);
        indices.push([lower(i), upper(i + 1), upper(i)]);
        indices.push([upper(i), upper(i + 1), top]);
    }

    TriangleMesh3d::with_generated_normals_and_uvs(vertices, indices, uvs)
}

fn ribbon_mesh(points: Vec<Vec3>, width: f32, closed: bool) -> Primitive3d {
    let points = cleaned_line_points(points, closed);
    if points.len() < 2 || width <= 0.0 || (closed && points.len() < 3) {
        return Primitive3d::mesh(Vec::new(), Vec::new());
    }

    // Build one connected triangle strip with mitered joins. The previous
    // implementation emitted one independent quad per segment, which made
    // elbows look broken because adjacent segment triangles did not share a
    // real joint. Here every path vertex owns exactly one left/right pair,
    // and neighbouring segments share that pair.
    let half = width * 0.5;
    let mut vertices = Vec::with_capacity(points.len() * 2);
    for i in 0..points.len() {
        let point = points[i];
        let previous = if i == 0 {
            if closed {
                points[points.len() - 1]
            } else {
                points[i]
            }
        } else {
            points[i - 1]
        };
        let next = if i + 1 == points.len() {
            if closed { points[0] } else { points[i] }
        } else {
            points[i + 1]
        };

        let offset = if !closed && i == 0 {
            mul3(ribbon_segment_side(point, next), half)
        } else if !closed && i + 1 == points.len() {
            mul3(ribbon_segment_side(previous, point), half)
        } else {
            let prev_side = ribbon_segment_side(previous, point);
            let next_side = ribbon_segment_side(point, next);
            let mut miter = add3(prev_side, next_side);
            if dot3(miter, miter) <= 1.0e-6 {
                miter = next_side;
            }
            let miter = normalize3(miter);
            let denom = dot3(miter, next_side).abs().max(0.24);
            mul3(miter, (half / denom).min(half * 3.0))
        };

        vertices.push(add3(point, offset));
        vertices.push(sub3(point, offset));
    }

    let segment_count = if closed {
        points.len()
    } else {
        points.len().saturating_sub(1)
    };
    let mut indices = Vec::with_capacity(segment_count * 2);
    for i in 0..segment_count {
        let next = (i + 1) % points.len();
        let left_a = (i * 2) as u32;
        let right_a = left_a + 1;
        let left_b = (next * 2) as u32;
        let right_b = left_b + 1;
        indices.push([left_a, right_a, right_b]);
        indices.push([left_a, right_b, left_b]);
    }
    Primitive3d::Triangles(TriangleMesh3d::with_generated_normals(vertices, indices))
}

fn cleaned_line_points(points: Vec<Vec3>, closed: bool) -> Vec<Vec3> {
    const EPS: f32 = 1.0e-6;
    let mut cleaned = Vec::with_capacity(points.len());
    for point in points {
        let duplicate = cleaned
            .last()
            .is_some_and(|last| dot3(sub3(point, *last), sub3(point, *last)) <= EPS);
        if !duplicate {
            cleaned.push(point);
        }
    }
    if closed
        && cleaned.len() > 1
        && dot3(
            sub3(cleaned[0], *cleaned.last().expect("checked len")),
            sub3(cleaned[0], *cleaned.last().expect("checked len")),
        ) <= EPS
    {
        cleaned.pop();
    }
    cleaned
}

fn ribbon_segment_side(a: Vec3, b: Vec3) -> Vec3 {
    let tangent = normalize3(sub3(b, a));
    let mut side = cross3(WORLD_UP, tangent);
    if dot3(side, side) <= 1.0e-6 {
        side = [1.0, 0.0, 0.0];
    }
    normalize3(side)
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
    pub pointer_pos: Option<MaraPos2>,
    pub primary_down: bool,
    pub middle_down: bool,
    pub scroll_delta: MaraVec2,
}

impl Viewport3d {
    #[must_use]
    #[doc(hidden)]
    pub(crate) fn __internal_from_backend_response(
        response: &egui::Response,
        pixels: [u32; 2],
        ui: &egui::Ui,
    ) -> Self {
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
            pointer_pos: response.hover_pos().map(Into::into),
            primary_down: input.0,
            middle_down: input.1,
            scroll_delta: input.2.into(),
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
        _position: MaraPos2,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GizmoAxis {
    X,
    Y,
    Z,
}

impl GizmoAxis {
    const fn normal(self) -> Vec3 {
        match self {
            Self::X => [1.0, 0.0, 0.0],
            Self::Y => [0.0, 1.0, 0.0],
            Self::Z => [0.0, 0.0, 1.0],
        }
    }

    const fn plane_axes(self) -> (Vec3, Vec3) {
        match self {
            Self::X => ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
            Self::Y => ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
            Self::Z => ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::X => 0,
            Self::Y => 1,
            Self::Z => 2,
        }
    }

    fn rotation_tangent(self, camera: &PreviewCamera) -> Vec3 {
        let tangent = match self {
            Self::X | Self::Y => [0.0, 0.0, 1.0],
            Self::Z => [0.0, -1.0, 0.0],
        };
        let normal = self.normal();
        let projected = sub3(tangent, mul3(normal, dot3(tangent, normal)));
        if dot3(projected, projected) <= 1.0e-5 {
            normalize3(cross3(normal, camera.right))
        } else {
            normalize3(projected)
        }
    }
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

    fn world_per_screen_point(&self, rect: egui::Rect, depth: f32) -> f32 {
        let focal = 0.5 * rect.height() / (self.fov_y * 0.5).tan();
        depth / focal.max(1.0)
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

#[derive(Clone, Debug)]
struct GizmoDragState {
    object_id: ObjectId,
    operation: GizmoOperation,
    start_pointer: egui::Pos2,
    origin_screen: egui::Pos2,
    origin_world: Vec3,
    start_transform: Transform3d,
    world_per_point: f32,
    object_radius: f32,
    start_angle: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GizmoOperation {
    TranslateAxis(GizmoAxis),
    TranslatePlane(GizmoAxis),
    TranslateView,
    ScaleAxis(GizmoAxis),
    RotateAxis(GizmoAxis),
    RotateView,
}

/// A Mara surface for a retained 3D scene.
#[derive(Clone)]
pub struct View3d {
    id: egui::Id,
    scene: Scene3d,
    orbit: Orbit3d,
    preview_texture: Option<egui::TextureHandle>,
    gizmo_drag: Option<GizmoDragState>,
    #[cfg(feature = "gpu-preview")]
    gpu_callback_id: u64,
    #[cfg(feature = "gpu-preview")]
    gpu_target_format: Option<wgpu::TextureFormat>,
    #[cfg(feature = "gpu-preview")]
    gpu_scene_cache: GpuSceneGeometryCache,
}

impl View3d {
    #[must_use]
    pub fn new(id: impl std::hash::Hash, scene: Scene3d) -> Self {
        Self {
            id: egui::Id::new(id),
            scene,
            orbit: Orbit3d::default(),
            preview_texture: None,
            gizmo_drag: None,
            #[cfg(feature = "gpu-preview")]
            gpu_callback_id: next_gpu_callback_id(),
            #[cfg(feature = "gpu-preview")]
            gpu_target_format: None,
            #[cfg(feature = "gpu-preview")]
            gpu_scene_cache: GpuSceneGeometryCache::default(),
        }
    }

    /// Enable GPU triangle fill for hosts backed by `egui-wgpu`.
    ///
    /// This only switches the filled mesh triangles to the GPU preview
    /// painter. Grid, dots, gizmo, camera math, and Mara's technical
    /// shading remain the same as the CPU preview path.
    #[cfg(feature = "gpu-preview")]
    #[doc(hidden)]
    pub fn __internal_set_gpu_render_state(&mut self, render_state: Option<&egui_wgpu::RenderState>) {
        self.gpu_target_format = render_state.map(|state| state.target_format);
    }

    /// Enable GPU triangle fill when the host already knows the egui-wgpu
    /// output format.
    #[cfg(feature = "gpu-preview")]
    pub const fn set_gpu_target_format(&mut self, format: Option<wgpu::TextureFormat>) {
        self.gpu_target_format = format;
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
    #[doc(hidden)]
    pub(crate) fn __internal_allocate_viewport(
        &mut self,
        ui: &mut egui::Ui,
    ) -> (egui::Response, Viewport3d) {
        let available = ui.available_size_before_wrap();
        let size = egui::vec2(available.x.max(180.0), available.y.max(140.0));
        let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let pixels = [
            (rect.width() * ppp).ceil().max(1.0) as u32,
            (rect.height() * ppp).ceil().max(1.0) as u32,
        ];
        let viewport = Viewport3d::__internal_from_backend_response(&response, pixels, ui);
        (response, viewport)
    }

    fn view_ribbon(&self, scope: RibbonScope) -> RibbonSlotDef {
        let orbit = RibbonSlotItem::new(
            mara_core::vocab::Id::new(("three_d.orbit", self.id)),
            "orbit",
            "Orbit",
            "Orbit camera",
            RibbonAction::Command(mara_core::vocab::Id::new((
                "three_d.orbit.command",
                self.id,
            ))),
        );
        let fit = RibbonSlotItem::new(
            mara_core::vocab::Id::new(("three_d.fit", self.id)),
            "fit",
            "Fit",
            "Frame selected objects",
            RibbonAction::Command(mara_core::vocab::Id::new(("three_d.fit.command", self.id))),
        );
        RibbonSlotDef::new(
            mara_core::vocab::Id::new(("three_d.ribbon", self.id)),
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
        let camera = PreviewCamera::from_orbit(self.orbit, &self.scene.camera);
        let gizmo_used = self.update_gizmo_interaction(ui, rect, response, &camera);
        if !gizmo_used {
            self.update_orbit(ui, response);
            self.update_selection(ui, rect, response);
        } else if response.has_focus()
            && ui.input(|input| input.key_pressed(egui::Key::Delete))
            && self.scene.remove_selected_object().is_some()
        {
            self.gizmo_drag = None;
            ui.ctx().request_repaint();
        }

        let painter = ui.painter_at(rect);
        let accent: egui::Color32 = mara_core::style::active_accent().into();
        let background: egui::Color32 =
            mara_core::style::fill_for(mara_core::style::FillRole::Pane, accent).into();
        painter.rect_filled(rect, 0.0, background);
        let interactive_preview = gizmo_used
            || self.gizmo_drag.is_some()
            || response.dragged()
            || ui.input(|input| input.pointer.any_down());

        self.paint_grid(&painter, rect, &camera, 1.0, accent);

        #[cfg(feature = "gpu-preview")]
        if let Some(target_format) = self.gpu_target_format {
            let geometry = self.gpu_scene_geometry().clone();
            paint_scene_gpu(
                &painter,
                rect,
                self.gpu_callback_id,
                target_format,
                &camera,
                &self.scene,
                &geometry,
            );
        } else {
            let mut faces = Vec::new();
            self.collect_preview_faces(&mut faces, rect, &camera, interactive_preview);
            faces.sort_by(|a, b: &PreviewFace| b.depth.total_cmp(&a.depth));
            paint_faces_supersampled(ui, &painter, rect, &mut self.preview_texture, faces);
        }
        #[cfg(not(feature = "gpu-preview"))]
        {
            let mut faces = Vec::new();
            self.collect_preview_faces(&mut faces, rect, &camera, interactive_preview);
            faces.sort_by(|a, b: &PreviewFace| b.depth.total_cmp(&a.depth));
            paint_faces_supersampled(ui, &painter, rect, &mut self.preview_texture, faces);
        }
        self.paint_scene_gizmos(&painter, rect, &camera);
        let active_operation = self.gizmo_drag.as_ref().map(|drag| drag.operation);
        let hover_operation = active_operation.or_else(|| {
            response
                .hovered()
                .then(|| ui.input(|input| input.pointer.hover_pos()))
                .flatten()
                .and_then(|pointer| {
                    self.pick_gizmo(rect, &camera, pointer)
                        .map(|(_, operation, _, _, _, _)| operation)
                })
        });
        self.paint_transform_gizmo(&painter, rect, &camera, hover_operation, active_operation);

        if response.hovered() || response.dragged() {
            ui.ctx().request_repaint();
        }
    }

    #[cfg(feature = "gpu-preview")]
    fn gpu_scene_geometry(&mut self) -> &GpuSceneGeometryCache {
        let signature = gpu_scene_signature(&self.scene);
        if self.gpu_scene_cache.signature != Some(signature) {
            self.gpu_scene_cache = build_gpu_scene_geometry(&self.scene, signature);
        }
        &self.gpu_scene_cache
    }

    fn collect_preview_faces(
        &self,
        faces: &mut Vec<PreviewFace>,
        rect: egui::Rect,
        camera: &PreviewCamera,
        interactive_preview: bool,
    ) {
        for object in &self.scene.objects {
            if !object.visible {
                continue;
            }
            match &object.primitive {
                Primitive3d::Triangles(mesh) => {
                    let triangle_stride =
                        interactive_triangle_stride(mesh, object.selected, interactive_preview);
                    self.collect_mesh_faces(
                        faces,
                        rect,
                        camera,
                        object,
                        &object.transform,
                        &mesh.vertices,
                        &mesh.indices,
                        &mesh.normals,
                        &mesh.uvs,
                        &mesh.vertex_colors,
                        triangle_stride,
                    );
                    for instance in &object.instances {
                        let transform = combine_transform(&object.transform, instance);
                        self.collect_mesh_faces(
                            faces,
                            rect,
                            camera,
                            object,
                            &transform,
                            &mesh.vertices,
                            &mesh.indices,
                            &mesh.normals,
                            &mesh.uvs,
                            &mesh.vertex_colors,
                            triangle_stride,
                        );
                    }
                }
            }
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

    fn update_gizmo_interaction(
        &mut self,
        ui: &egui::Ui,
        rect: egui::Rect,
        response: &egui::Response,
        camera: &PreviewCamera,
    ) -> bool {
        if self.gizmo_drag.is_some() && ui.input(|input| input.pointer.primary_released()) {
            self.gizmo_drag = None;
            ui.ctx().request_repaint();
            return true;
        }

        if let Some(pointer) = ui.input(|input| input.pointer.interact_pos()) {
            if let Some(drag) = self.gizmo_drag.clone() {
                self.apply_gizmo_drag(rect, camera, &drag, pointer);
                ui.ctx().request_repaint();
                return true;
            }

            if response.hovered() && ui.input(|input| input.pointer.primary_pressed()) {
                response.request_focus();
                if let Some((
                    object_id,
                    operation,
                    origin_screen,
                    origin_world,
                    world_per_point,
                    object_radius,
                )) = self.pick_gizmo(rect, camera, pointer)
                {
                    let Some(object) = self.scene.object(object_id) else {
                        return false;
                    };
                    self.gizmo_drag = Some(GizmoDragState {
                        object_id,
                        operation,
                        start_pointer: pointer,
                        origin_screen,
                        origin_world,
                        start_transform: object.transform.clone(),
                        world_per_point,
                        object_radius,
                        start_angle: pointer_angle(origin_screen, pointer),
                    });
                    ui.ctx().request_repaint();
                    return true;
                }
            }
        }

        self.gizmo_drag.is_some()
    }

    fn pick_gizmo(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        pointer: egui::Pos2,
    ) -> Option<(ObjectId, GizmoOperation, egui::Pos2, Vec3, f32, f32)> {
        let object = self
            .scene
            .selected_object()
            .filter(|object| object.visible)?;
        let origin = transform_point(&object.transform, [0.0, 0.0, 0.0]);
        let (origin_screen, depth) = camera.project(rect, origin)?;
        let world_per_point = camera.world_per_screen_point(rect, depth);
        if !world_per_point.is_finite() || world_per_point <= 0.0 {
            return None;
        }
        let object_radius = object_world_radius(object).max(0.01);

        let inner_radius = GIZMO_SIZE * 0.2;
        let outer_radius = GIZMO_SIZE + GIZMO_STROKE_WIDTH + 5.0;
        let center_distance = origin_screen.distance(pointer);
        if center_distance <= inner_radius + GIZMO_PICK_DISTANCE {
            return Some((
                object.id,
                GizmoOperation::TranslateView,
                origin_screen,
                origin,
                world_per_point,
                object_radius,
            ));
        }
        if (center_distance - outer_radius).abs() <= GIZMO_PICK_DISTANCE {
            return Some((
                object.id,
                GizmoOperation::RotateView,
                origin_screen,
                origin,
                world_per_point,
                object_radius,
            ));
        }

        let mut best: Option<(GizmoOperation, f32)> = None;
        for axis in [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z] {
            if let Some(distance) = self.gizmo_axis_handle_distance(
                rect,
                camera,
                origin,
                world_per_point,
                pointer,
                axis,
                true,
            ) {
                update_best_gizmo_pick(&mut best, GizmoOperation::TranslateAxis(axis), distance);
            }
            if let Some(distance) = self.gizmo_axis_handle_distance(
                rect,
                camera,
                origin,
                world_per_point,
                pointer,
                axis,
                false,
            ) {
                update_best_gizmo_pick(&mut best, GizmoOperation::ScaleAxis(axis), distance);
            }
            if let Some(distance) = self.gizmo_plane_handle_distance(
                rect,
                camera,
                origin,
                world_per_point,
                pointer,
                axis,
            ) {
                update_best_gizmo_pick(&mut best, GizmoOperation::TranslatePlane(axis), distance);
            }
            if let Some(distance) = self.gizmo_rotation_arc_distance(
                rect,
                camera,
                origin,
                world_per_point,
                pointer,
                axis,
            ) {
                update_best_gizmo_pick(&mut best, GizmoOperation::RotateAxis(axis), distance);
            }
        }

        best.map(|(operation, _)| {
            (
                object.id,
                operation,
                origin_screen,
                origin,
                world_per_point,
                object_radius,
            )
        })
    }

    fn apply_gizmo_drag(
        &mut self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        drag: &GizmoDragState,
        pointer: egui::Pos2,
    ) {
        let delta = pointer - drag.start_pointer;
        let view_forward = camera.forward;
        let view_right = camera.right;
        let view_up = camera.up;
        let Some(object) = self.scene.object_mut(drag.object_id) else {
            return;
        };
        match drag.operation {
            GizmoOperation::TranslateAxis(axis) => {
                let screen_axis = project_screen_direction(
                    rect,
                    camera,
                    drag.origin_world,
                    axis.normal(),
                    drag.world_per_point,
                );
                let amount = delta.dot(screen_axis) * drag.world_per_point;
                object.transform.translation = add3(
                    drag.start_transform.translation,
                    mul3(axis.normal(), amount),
                );
            }
            GizmoOperation::TranslatePlane(axis) => {
                let (a, b) = axis.plane_axes();
                let screen_a = project_screen_direction(
                    rect,
                    camera,
                    drag.origin_world,
                    a,
                    drag.world_per_point,
                );
                let screen_b = project_screen_direction(
                    rect,
                    camera,
                    drag.origin_world,
                    b,
                    drag.world_per_point,
                );
                object.transform.translation = add3(
                    drag.start_transform.translation,
                    add3(
                        mul3(a, delta.dot(screen_a) * drag.world_per_point),
                        mul3(b, delta.dot(screen_b) * drag.world_per_point),
                    ),
                );
            }
            GizmoOperation::TranslateView => {
                // View-plane move: screen X/Y directly maps to camera right/up at the gizmo depth.
                object.transform.translation = add3(
                    drag.start_transform.translation,
                    add3(
                        mul3(view_right, delta.x * drag.world_per_point),
                        mul3(view_up, -delta.y * drag.world_per_point),
                    ),
                );
            }
            GizmoOperation::ScaleAxis(axis) => {
                let screen_axis = project_screen_direction(
                    rect,
                    camera,
                    drag.origin_world,
                    axis.normal(),
                    drag.world_per_point,
                );
                let amount = delta.dot(screen_axis) * drag.world_per_point;
                let factor = (1.0 + amount / drag.object_radius).max(0.05);
                object.transform.scale = drag.start_transform.scale;
                object.transform.scale[axis.index()] =
                    (drag.start_transform.scale[axis.index()] * factor).max(0.01);
            }
            GizmoOperation::RotateAxis(axis) => {
                let current = pointer_angle(drag.origin_screen, pointer);
                let delta_angle = wrap_angle(current - drag.start_angle);
                object.transform.rotation_xyzw = quat_mul(
                    axis_angle_quat(axis.normal(), delta_angle),
                    drag.start_transform.rotation_xyzw,
                );
            }
            GizmoOperation::RotateView => {
                let current = pointer_angle(drag.origin_screen, pointer);
                let delta_angle = wrap_angle(current - drag.start_angle);
                object.transform.rotation_xyzw = quat_mul(
                    axis_angle_quat(view_forward, delta_angle),
                    drag.start_transform.rotation_xyzw,
                );
            }
        }
    }

    fn update_orbit(&mut self, ui: &egui::Ui, response: &egui::Response) {
        if response.dragged_by(egui::PointerButton::Primary) {
            let delta = ui.input(|input| input.pointer.delta());
            self.orbit.yaw -= delta.x * 0.006;
            self.orbit.pitch = (self.orbit.pitch + delta.y * 0.006).clamp(-1.45, 1.45);
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
        self.scene.material(material).map_or_else(
            || mara_core::style::active_accent().into(),
            |material| material.base_color.into(),
        )
    }

    fn paint_scene_gizmos(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
    ) {
        for gizmo in &self.scene.gizmos {
            if !gizmo.visible {
                continue;
            }
            self.paint_scene_gizmo(painter, rect, camera, gizmo);
        }
    }

    fn paint_scene_gizmo(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        gizmo: &Gizmo3d,
    ) {
        let stroke = egui::Stroke::new(gizmo.style.width.max(0.5), gizmo.style.color);
        match &gizmo.kind {
            Gizmo3dKind::Dot { position } => {
                if let Some((screen, _)) = camera.project(rect, *position) {
                    painter.circle_filled(screen, gizmo.style.radius.max(1.0), gizmo.style.color);
                }
            }
            Gizmo3dKind::Line { a, b } | Gizmo3dKind::Segment { a, b } => {
                if let (Some((a, _)), Some((b, _))) =
                    (camera.project(rect, *a), camera.project(rect, *b))
                {
                    painter.line_segment([a, b], stroke);
                }
            }
            Gizmo3dKind::Polyline { points } => {
                paint_projected_polyline(painter, rect, camera, points, false, stroke);
            }
            Gizmo3dKind::Polygon { points, closed } => {
                paint_projected_polyline(painter, rect, camera, points, *closed, stroke);
            }
            Gizmo3dKind::Rectangle { center, size } => {
                let half = [size[0] * 0.5, size[1] * 0.5];
                let points = [
                    [center[0] - half[0], center[1], center[2] - half[1]],
                    [center[0] + half[0], center[1], center[2] - half[1]],
                    [center[0] + half[0], center[1], center[2] + half[1]],
                    [center[0] - half[0], center[1], center[2] + half[1]],
                ];
                paint_projected_polyline(painter, rect, camera, &points, true, stroke);
            }
            Gizmo3dKind::Circle { center, radius } => {
                let points = sampled_gizmo_ellipse(
                    *center,
                    [*radius, *radius],
                    64,
                    0.0,
                    std::f32::consts::TAU,
                );
                paint_projected_polyline(painter, rect, camera, &points, true, stroke);
            }
            Gizmo3dKind::Ellipse { center, radii } => {
                let points = sampled_gizmo_ellipse(*center, *radii, 64, 0.0, std::f32::consts::TAU);
                paint_projected_polyline(painter, rect, camera, &points, true, stroke);
            }
            Gizmo3dKind::Arc {
                center,
                radius,
                start,
                end,
            } => {
                let points = sampled_gizmo_ellipse(*center, [*radius, *radius], 48, *start, *end);
                paint_projected_polyline(painter, rect, camera, &points, false, stroke);
            }
            Gizmo3dKind::Axes { origin, size } => {
                let x = add3(*origin, [*size, 0.0, 0.0]);
                let y = add3(*origin, [0.0, *size, 0.0]);
                let z = add3(*origin, [0.0, 0.0, *size]);
                paint_gizmo_axis_segment(
                    painter,
                    rect,
                    camera,
                    *origin,
                    x,
                    GizmoAxis::X,
                    gizmo.style.width,
                );
                paint_gizmo_axis_segment(
                    painter,
                    rect,
                    camera,
                    *origin,
                    y,
                    GizmoAxis::Y,
                    gizmo.style.width,
                );
                paint_gizmo_axis_segment(
                    painter,
                    rect,
                    camera,
                    *origin,
                    z,
                    GizmoAxis::Z,
                    gizmo.style.width,
                );
            }
        }
    }

    fn paint_transform_gizmo(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        hover_operation: Option<GizmoOperation>,
        active_operation: Option<GizmoOperation>,
    ) {
        let Some(object) = self.scene.selected_object().filter(|object| object.visible) else {
            return;
        };
        let origin = transform_point(&object.transform, [0.0, 0.0, 0.0]);
        let Some((origin_screen, depth)) = camera.project(rect, origin) else {
            return;
        };
        let world_per_point = camera.world_per_screen_point(rect, depth);
        if !world_per_point.is_finite() || world_per_point <= 0.0 {
            return;
        }

        self.paint_gizmo_rotation_arc(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::X,
            gizmo_operation_highlighted(
                GizmoOperation::RotateAxis(GizmoAxis::X),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_rotation_arc(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Y,
            gizmo_operation_highlighted(
                GizmoOperation::RotateAxis(GizmoAxis::Y),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_rotation_arc(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Z,
            gizmo_operation_highlighted(
                GizmoOperation::RotateAxis(GizmoAxis::Z),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_view_circle(
            painter,
            origin_screen,
            gizmo_operation_highlighted(
                GizmoOperation::TranslateView,
                hover_operation,
                active_operation,
            ),
            gizmo_operation_highlighted(
                GizmoOperation::RotateView,
                hover_operation,
                active_operation,
            ),
        );

        self.paint_gizmo_plane(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::X,
            gizmo_operation_highlighted(
                GizmoOperation::TranslatePlane(GizmoAxis::X),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_plane(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Y,
            gizmo_operation_highlighted(
                GizmoOperation::TranslatePlane(GizmoAxis::Y),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_plane(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Z,
            gizmo_operation_highlighted(
                GizmoOperation::TranslatePlane(GizmoAxis::Z),
                hover_operation,
                active_operation,
            ),
        );

        self.paint_gizmo_arrow(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::X,
            gizmo_operation_highlighted(
                GizmoOperation::TranslateAxis(GizmoAxis::X),
                hover_operation,
                active_operation,
            ),
            gizmo_operation_highlighted(
                GizmoOperation::ScaleAxis(GizmoAxis::X),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_arrow(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Y,
            gizmo_operation_highlighted(
                GizmoOperation::TranslateAxis(GizmoAxis::Y),
                hover_operation,
                active_operation,
            ),
            gizmo_operation_highlighted(
                GizmoOperation::ScaleAxis(GizmoAxis::Y),
                hover_operation,
                active_operation,
            ),
        );
        self.paint_gizmo_arrow(
            painter,
            rect,
            camera,
            origin,
            world_per_point,
            GizmoAxis::Z,
            gizmo_operation_highlighted(
                GizmoOperation::TranslateAxis(GizmoAxis::Z),
                hover_operation,
                active_operation,
            ),
            gizmo_operation_highlighted(
                GizmoOperation::ScaleAxis(GizmoAxis::Z),
                hover_operation,
                active_operation,
            ),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_gizmo_arrow(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        axis: GizmoAxis,
        translate_highlighted: bool,
        scale_highlighted: bool,
    ) {
        let direction = axis.normal();
        let visibility = gizmo_arrow_visibility(camera, origin, direction);
        if visibility <= 1.0e-4 {
            return;
        }
        let scale_color = gizmo_axis_color(axis, visibility, scale_highlighted);
        let translate_color = gizmo_axis_color(axis, visibility, translate_highlighted);
        let scale_width = highlighted_width(GIZMO_STROKE_WIDTH, scale_highlighted);
        let translate_width = highlighted_width(GIZMO_STROKE_WIDTH, translate_highlighted);

        // Scale handle: the original gizmo draws a thick terminal segment
        // when scale is enabled on the same axis.
        let scale_start = GIZMO_SIZE * 0.2 + GIZMO_STROKE_WIDTH * 0.5;
        let scale_end = GIZMO_SIZE;
        let scale_tip_start = scale_end - GIZMO_STROKE_WIDTH * 2.4;
        let scale_points = [
            add3(origin, mul3(direction, scale_start * world_per_point)),
            add3(origin, mul3(direction, scale_tip_start * world_per_point)),
            add3(origin, mul3(direction, scale_end * world_per_point)),
        ];
        if let (Some((a, _)), Some((b, _)), Some((c, _))) = (
            camera.project(rect, scale_points[0]),
            camera.project(rect, scale_points[1]),
            camera.project(rect, scale_points[2]),
        ) {
            painter.line_segment([a, b], egui::Stroke::new(scale_width, scale_color));
            painter.line_segment([b, c], egui::Stroke::new(scale_width * 2.4, scale_color));
        }

        // Translation handle: the original gizmo offsets movement arrows
        // past the scale handle when both translate and scale modes exist.
        let translate_start = GIZMO_SIZE + GIZMO_STROKE_WIDTH * 3.0;
        let translate_end = translate_start + GIZMO_SIZE * 0.2 + GIZMO_STROKE_WIDTH;
        let translate_tip_start = translate_end - GIZMO_STROKE_WIDTH * 2.4;
        let translate_points = [
            add3(origin, mul3(direction, translate_start * world_per_point)),
            add3(
                origin,
                mul3(direction, translate_tip_start * world_per_point),
            ),
            add3(origin, mul3(direction, translate_end * world_per_point)),
        ];
        let (Some((a, _)), Some((b, _)), Some((c, _))) = (
            camera.project(rect, translate_points[0]),
            camera.project(rect, translate_points[1]),
            camera.project(rect, translate_points[2]),
        ) else {
            return;
        };
        painter.line_segment([a, b], egui::Stroke::new(translate_width, translate_color));
        paint_gizmo_arrow_head(painter, b, c, translate_color, translate_width);
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_gizmo_plane(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        axis: GizmoAxis,
        highlighted: bool,
    ) {
        let normal = axis.normal();
        let visibility = gizmo_plane_visibility(camera, origin, normal);
        if visibility <= 1.0e-4 {
            return;
        }
        let offset = GIZMO_SIZE * 0.5 * world_per_point;
        let half = (GIZMO_SIZE * 0.1 + GIZMO_STROKE_WIDTH * 2.0) * 0.5 * world_per_point;
        let (a, b) = axis.plane_axes();
        let center = add3(origin, mul3(add3(a, b), offset));
        let corners = [
            sub3(sub3(center, mul3(a, half)), mul3(b, half)),
            add3(sub3(center, mul3(a, half)), mul3(b, half)),
            add3(add3(center, mul3(a, half)), mul3(b, half)),
            sub3(add3(center, mul3(a, half)), mul3(b, half)),
        ];
        let mut projected = Vec::with_capacity(4);
        for corner in corners {
            let Some((point, _)) = camera.project(rect, corner) else {
                return;
            };
            projected.push(point);
        }
        let color = gizmo_axis_color(
            axis,
            visibility * if highlighted { 0.78 } else { 0.42 },
            highlighted,
        );
        painter.add(egui::Shape::convex_polygon(
            projected,
            color,
            egui::Stroke::new(
                if highlighted {
                    GIZMO_STROKE_WIDTH * 0.75
                } else {
                    0.0
                },
                gizmo_axis_color(axis, visibility, highlighted),
            ),
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_gizmo_rotation_arc(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        axis: GizmoAxis,
        highlighted: bool,
    ) {
        let normal = axis.normal();
        let dot = dot3(normal, camera.forward).abs();
        let arc_t = ((dot - GIZMO_ARC_FADE_START) / (GIZMO_ARC_FADE_END - GIZMO_ARC_FADE_START))
            .clamp(0.0, 1.0);
        let angle = std::f32::consts::FRAC_PI_2 + arc_t * std::f32::consts::FRAC_PI_2;
        let radius = GIZMO_SIZE * world_per_point;
        let tangent = axis.rotation_tangent(camera);
        let bitangent = normalize3(cross3(normal, tangent));
        let start = std::f32::consts::FRAC_PI_2 - angle;
        let end = std::f32::consts::FRAC_PI_2 + angle;
        let mut screen_points = Vec::with_capacity(GIZMO_ROTATION_SEGMENTS + 1);
        for i in 0..=GIZMO_ROTATION_SEGMENTS {
            let t = i as f32 / GIZMO_ROTATION_SEGMENTS as f32;
            let angle = start + (end - start) * t;
            let world = add3(
                origin,
                add3(
                    mul3(tangent, angle.cos() * radius),
                    mul3(bitangent, angle.sin() * radius),
                ),
            );
            let Some((screen, _)) = camera.project(rect, world) else {
                continue;
            };
            screen_points.push(screen);
        }
        if screen_points.len() >= 2 {
            painter.add(egui::Shape::line(
                screen_points,
                egui::Stroke::new(
                    highlighted_width(GIZMO_STROKE_WIDTH, highlighted),
                    gizmo_axis_color(axis, 1.0, highlighted),
                ),
            ));
        }
    }

    fn paint_gizmo_view_circle(
        &self,
        painter: &egui::Painter,
        origin: egui::Pos2,
        translate_highlighted: bool,
        rotate_highlighted: bool,
    ) {
        painter.circle_stroke(
            origin,
            GIZMO_SIZE + GIZMO_STROKE_WIDTH + 5.0,
            egui::Stroke::new(
                highlighted_width(GIZMO_STROKE_WIDTH, rotate_highlighted),
                gizmo_view_color(rotate_highlighted),
            ),
        );
        painter.circle_stroke(
            origin,
            GIZMO_SIZE * 0.2,
            egui::Stroke::new(
                highlighted_width(GIZMO_STROKE_WIDTH, translate_highlighted),
                gizmo_view_color(translate_highlighted),
            ),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn gizmo_axis_handle_distance(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        pointer: egui::Pos2,
        axis: GizmoAxis,
        translate: bool,
    ) -> Option<f32> {
        let direction = axis.normal();
        if gizmo_arrow_visibility(camera, origin, direction) <= 1.0e-4 {
            return None;
        }
        let (start, end) = if translate {
            let start = GIZMO_SIZE + GIZMO_STROKE_WIDTH * 3.0;
            (start, start + GIZMO_SIZE * 0.2 + GIZMO_STROKE_WIDTH)
        } else {
            (GIZMO_SIZE * 0.2 + GIZMO_STROKE_WIDTH * 0.5, GIZMO_SIZE)
        };
        let a = add3(origin, mul3(direction, start * world_per_point));
        let b = add3(origin, mul3(direction, end * world_per_point));
        let (Some((a, _)), Some((b, _))) = (camera.project(rect, a), camera.project(rect, b))
        else {
            return None;
        };
        let distance = distance_to_screen_segment(pointer, a, b);
        let pick_width = if translate {
            GIZMO_PICK_DISTANCE
        } else {
            GIZMO_PICK_DISTANCE.max(GIZMO_STROKE_WIDTH * 2.4)
        };
        (distance <= pick_width).then_some(distance)
    }

    fn gizmo_plane_handle_distance(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        pointer: egui::Pos2,
        axis: GizmoAxis,
    ) -> Option<f32> {
        let normal = axis.normal();
        if gizmo_plane_visibility(camera, origin, normal) <= 1.0e-4 {
            return None;
        }
        let offset = GIZMO_SIZE * 0.5 * world_per_point;
        let half = (GIZMO_SIZE * 0.1 + GIZMO_STROKE_WIDTH * 2.0) * 0.5 * world_per_point;
        let (a, b) = axis.plane_axes();
        let center = add3(origin, mul3(add3(a, b), offset));
        let corners = [
            sub3(sub3(center, mul3(a, half)), mul3(b, half)),
            add3(sub3(center, mul3(a, half)), mul3(b, half)),
            add3(add3(center, mul3(a, half)), mul3(b, half)),
            sub3(add3(center, mul3(a, half)), mul3(b, half)),
        ];
        let mut projected = Vec::with_capacity(4);
        for corner in corners {
            let (point, _) = camera.project(rect, corner)?;
            projected.push(point);
        }
        if point_in_screen_polygon(pointer, &projected) {
            Some(0.0)
        } else {
            let distance = projected
                .iter()
                .enumerate()
                .map(|(i, point)| {
                    distance_to_screen_segment(
                        pointer,
                        *point,
                        projected[(i + 1) % projected.len()],
                    )
                })
                .fold(f32::INFINITY, f32::min);
            (distance <= GIZMO_PICK_DISTANCE).then_some(distance)
        }
    }

    fn gizmo_rotation_arc_distance(
        &self,
        rect: egui::Rect,
        camera: &PreviewCamera,
        origin: Vec3,
        world_per_point: f32,
        pointer: egui::Pos2,
        axis: GizmoAxis,
    ) -> Option<f32> {
        let normal = axis.normal();
        let dot = dot3(normal, camera.forward).abs();
        let arc_t = ((dot - GIZMO_ARC_FADE_START) / (GIZMO_ARC_FADE_END - GIZMO_ARC_FADE_START))
            .clamp(0.0, 1.0);
        let angle = std::f32::consts::FRAC_PI_2 + arc_t * std::f32::consts::FRAC_PI_2;
        let radius = GIZMO_SIZE * world_per_point;
        let tangent = axis.rotation_tangent(camera);
        let bitangent = normalize3(cross3(normal, tangent));
        let start = std::f32::consts::FRAC_PI_2 - angle;
        let end = std::f32::consts::FRAC_PI_2 + angle;
        let mut last = None;
        let mut best = f32::INFINITY;
        for i in 0..=GIZMO_ROTATION_SEGMENTS {
            let t = i as f32 / GIZMO_ROTATION_SEGMENTS as f32;
            let angle = start + (end - start) * t;
            let world = add3(
                origin,
                add3(
                    mul3(tangent, angle.cos() * radius),
                    mul3(bitangent, angle.sin() * radius),
                ),
            );
            let Some((screen, _)) = camera.project(rect, world) else {
                continue;
            };
            if let Some(last) = last {
                best = best.min(distance_to_screen_segment(pointer, last, screen));
            }
            last = Some(screen);
        }
        (best <= GIZMO_PICK_DISTANCE).then_some(best)
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

    #[allow(clippy::too_many_arguments)]
    fn collect_mesh_faces(
        &self,
        faces: &mut Vec<PreviewFace>,
        rect: egui::Rect,
        camera: &PreviewCamera,
        object: &Object3d,
        transform: &Transform3d,
        vertices: &[Vec3],
        indices: &[[u32; 3]],
        normals: &[Vec3],
        uvs: &[[f32; 2]],
        vertex_colors: &[Color],
        triangle_stride: usize,
    ) {
        if vertices.is_empty() || indices.is_empty() {
            return;
        }

        let world: Vec<Vec3> = vertices
            .iter()
            .copied()
            .map(|point| transform_point(transform, point))
            .collect();
        let world_normals: Vec<Vec3> = if normals.len() == vertices.len() {
            normals
                .iter()
                .copied()
                .map(|normal| transform_normal(transform, normal))
                .collect()
        } else {
            Vec::new()
        };
        let base = self.material_color(object.material);
        let material_texture = self
            .scene
            .material(object.material)
            .and_then(|material| material.albedo_texture)
            .and_then(|texture| self.scene.texture(texture))
            .cloned();

        for triangle in indices.iter().step_by(triangle_stride.max(1)) {
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
                    uvs: if uvs.len() == world.len() {
                        Some([uvs[triangle[0]], uvs[triangle[1]], uvs[triangle[2]]])
                    } else {
                        None
                    },
                    texture: material_texture.clone(),
                    fills: if world_normals.len() == world.len() {
                        [
                            shade_vertex_color(
                                base,
                                vertex_colors,
                                triangle[0],
                                world_normals[triangle[0]],
                                camera,
                            ),
                            shade_vertex_color(
                                base,
                                vertex_colors,
                                triangle[1],
                                world_normals[triangle[1]],
                                camera,
                            ),
                            shade_vertex_color(
                                base,
                                vertex_colors,
                                triangle[2],
                                world_normals[triangle[2]],
                                camera,
                            ),
                        ]
                    } else {
                        let fill = shade_color(base, face_normal_world, camera);
                        [
                            vertex_color_or_base(fill, vertex_colors, triangle[0]),
                            vertex_color_or_base(fill, vertex_colors, triangle[1]),
                            vertex_color_or_base(fill, vertex_colors, triangle[2]),
                        ]
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
        let edge = add3(center, mul3(camera.right, object_world_radius(object)));
        match (camera.project(rect, center), camera.project(rect, edge)) {
            (Some((center, _)), Some((edge, _))) => center.distance(edge),
            _ => 0.0,
        }
    }
}

impl MaraView for View3d {
    fn id(&self) -> ViewId {
        ViewId::from(self.id)
    }

    fn title(&self) -> &str {
        &self.scene.title
    }

    fn icon(&self) -> &'static str {
        "cube"
    }

    fn ribbons(&mut self) -> Vec<RibbonSlotDef> {
        vec![self.view_ribbon(RibbonScope::View(ViewId::from(self.id)))]
    }

    fn show(&mut self, ctx: &mut ViewCtx<'_>) {
        // Pull the host-published render target format so the 3D pipeline
        // matches the surface, without a per-frame setter — lets a View3d
        // be hosted as a plain `ViewNode` leaf.
        if let Some(format) = ctx
            .__internal_egui_ctx()
            .data(|d| d.get_temp::<wgpu::TextureFormat>(egui::Id::new("mara_gpu_target_format")))
        {
            self.gpu_target_format = Some(format);
        }
        // Render into this node's REGION, not a window-grabbing panel
        // (ADR 0002 / PLAN WS6): a View3d hosted as a split cell draws
        // and interacts inside its cell rect, so 3D views tile like any
        // other leaf. The whole-window case is just the one-leaf tree.
        let region = ctx.screen_rect();
        let rect: egui::Rect = region.into();
        egui::Area::new(egui::Id::new(("mara_three_d_view", self.id)))
            .order(egui::Order::Background)
            .fixed_pos(rect.min)
            .show(ctx.__internal_egui_ctx(), |ui| {
                ui.set_clip_rect(rect);
                let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());
                self.paint_preview(ui, response.rect, &response);
            });
    }
}

impl MaraModule for View3d {
    fn id(&self) -> mara_core::vocab::Id {
        self.id.into()
    }

    fn title(&self) -> &str {
        &self.scene.title
    }

    fn icon(&self) -> &'static str {
        "cube"
    }

    fn inline(
        &mut self,
        mui: &mut mara_core::MaraUi<'_>,
        ctx: ModuleInlineCtx<'_>,
    ) -> ModuleResponse {
        mui.label(&format!("3D scene: {}", self.scene.title));
        mui.label(&format!("{} objects", self.scene.objects.len()));
        if ctx.can_enter_workspace() && mui.button("Open 3D workspace").clicked() {
            ModuleResponse::enter_workspace()
        } else {
            ModuleResponse::none()
        }
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
    uvs: Option<[[f32; 2]; 3]>,
    texture: Option<Texture3d>,
    fills: [egui::Color32; 3],
}

#[cfg(feature = "gpu-preview")]
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct GpuPreviewCallback {
    id: u64,
    target_format: wgpu::TextureFormat,
    viewport_points: [f32; 2],
    vertices: Vec<GpuPreviewVertex>,
    batches: Vec<GpuPreviewBatch>,
    textures: Vec<GpuPreviewTextureSource>,
}

#[cfg(feature = "gpu-preview")]
#[derive(Clone, Debug)]
struct GpuSceneCallback {
    id: u64,
    geometry_signature: u64,
    target_format: wgpu::TextureFormat,
    viewport_points: [f32; 2],
    uniform: GpuSceneUniform,
    vertices: std::sync::Arc<[GpuSceneVertex]>,
    batches: Vec<GpuPreviewBatch>,
    textures: std::sync::Arc<[GpuPreviewTextureSource]>,
}

#[cfg(feature = "gpu-preview")]
#[derive(Clone, Debug)]
struct GpuPreviewBatch {
    start: u32,
    end: u32,
    texture: Option<TextureId>,
}

#[cfg(feature = "gpu-preview")]
#[derive(Clone, Debug)]
struct GpuPreviewTextureSource {
    id: TextureId,
    size: [u32; 2],
    pixels: Vec<egui::Color32>,
}

#[cfg(feature = "gpu-preview")]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuPreviewVertex {
    position: [f32; 3],
    uv: [f32; 2],
    color: u32,
}

#[cfg(feature = "gpu-preview")]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSceneVertex {
    position: [f32; 3],
    normal: [f32; 3],
    uv: [f32; 2],
    color: u32,
}

#[cfg(feature = "gpu-preview")]
#[derive(Clone, Default)]
struct GpuSceneGeometryCache {
    signature: Option<u64>,
    vertices: std::sync::Arc<[GpuSceneVertex]>,
    batches: Vec<GpuPreviewBatch>,
    textures: std::sync::Arc<[GpuPreviewTextureSource]>,
}

#[cfg(feature = "gpu-preview")]
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuSceneUniform {
    eye: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    forward: [f32; 4],
    params: [f32; 4],
}

#[cfg(feature = "gpu-preview")]
#[derive(Default)]
struct GpuPreviewResources {
    pipeline_format: Option<wgpu::TextureFormat>,
    mesh_pipeline: Option<wgpu::RenderPipeline>,
    scene_pipeline: Option<wgpu::RenderPipeline>,
    quad_pipeline: Option<wgpu::RenderPipeline>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    scene_uniform_layout: Option<wgpu::BindGroupLayout>,
    sampler: Option<wgpu::Sampler>,
    white_bind_group: Option<wgpu::BindGroup>,
    textures: std::collections::HashMap<TextureId, GpuPreviewTextureResource>,
    scene_vertex_buffers: std::collections::HashMap<u64, GpuSceneVertexBuffer>,
    prepared: std::collections::HashMap<u64, GpuPreparedPreview>,
}

#[cfg(feature = "gpu-preview")]
struct GpuPreviewTextureResource {
    hash: u64,
    bind_group: wgpu::BindGroup,
}

#[cfg(feature = "gpu-preview")]
struct GpuSceneVertexBuffer {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

#[cfg(feature = "gpu-preview")]
struct GpuPreparedPreview {
    vertex_buffer: wgpu::Buffer,
    #[allow(dead_code)]
    uniform_buffer: Option<wgpu::Buffer>,
    #[allow(dead_code)]
    uniform_bind_group: Option<wgpu::BindGroup>,
    vertex_count: u32,
    batches: Vec<GpuPreviewBatch>,
    target_bind_group: wgpu::BindGroup,
    target_size: [u32; 2],
    #[allow(dead_code)]
    target_texture: wgpu::Texture,
    #[allow(dead_code)]
    target_view: wgpu::TextureView,
    #[allow(dead_code)]
    depth_texture: wgpu::Texture,
}

#[cfg(feature = "gpu-preview")]
struct GpuPreviewTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    depth_texture: wgpu::Texture,
    depth_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

#[cfg(feature = "gpu-preview")]
impl GpuPreviewCallback {
    #[allow(dead_code)]
    fn from_faces(
        id: u64,
        target_format: wgpu::TextureFormat,
        rect: egui::Rect,
        faces: Vec<PreviewFace>,
    ) -> Self {
        let mut vertices = Vec::with_capacity(faces.len() * 3);
        let mut batches = Vec::new();
        let mut textures = std::collections::HashMap::<TextureId, GpuPreviewTextureSource>::new();
        let mut active_texture = None;
        let mut active_start = 0_u32;

        for face in faces {
            let texture_id = face.texture.as_ref().map(|texture| texture.id);
            if texture_id != active_texture {
                let current = vertices.len() as u32;
                if current > active_start {
                    batches.push(GpuPreviewBatch {
                        start: active_start,
                        end: current,
                        texture: active_texture,
                    });
                }
                active_texture = texture_id;
                active_start = current;
            }

            if let Some(texture) = face.texture.as_ref() {
                textures
                    .entry(texture.id)
                    .or_insert_with(|| GpuPreviewTextureSource {
                        id: texture.id,
                        size: [texture.size[0].max(1) as u32, texture.size[1].max(1) as u32],
                        pixels: texture.pixels.iter().copied().map(Into::into).collect(),
                    });
            }

            for i in 0..3 {
                let [x, y] = point_to_viewport_ndc(rect, face.points[i]);
                vertices.push(GpuPreviewVertex {
                    position: [x, y, gpu_depth(face.depths[i])],
                    uv: face.uvs.map_or([0.0, 0.0], |uvs| uvs[i]),
                    color: pack_color32(face.fills[i]),
                });
            }
        }

        let current = vertices.len() as u32;
        if current > active_start {
            batches.push(GpuPreviewBatch {
                start: active_start,
                end: current,
                texture: active_texture,
            });
        }

        Self {
            id,
            target_format,
            viewport_points: [rect.width().max(1.0), rect.height().max(1.0)],
            vertices,
            batches,
            textures: textures.into_values().collect(),
        }
    }
}

#[cfg(feature = "gpu-preview")]
impl GpuSceneCallback {
    fn from_geometry(
        id: u64,
        target_format: wgpu::TextureFormat,
        rect: egui::Rect,
        camera: &PreviewCamera,
        scene: &Scene3d,
        geometry: &GpuSceneGeometryCache,
    ) -> Self {
        Self {
            id,
            geometry_signature: geometry.signature.unwrap_or(0),
            target_format,
            viewport_points: [rect.width().max(1.0), rect.height().max(1.0)],
            uniform: GpuSceneUniform::new(rect, camera, scene),
            vertices: geometry.vertices.clone(),
            batches: geometry.batches.clone(),
            textures: geometry.textures.clone(),
        }
    }
}

#[cfg(feature = "gpu-preview")]
fn build_gpu_scene_geometry(scene: &Scene3d, signature: u64) -> GpuSceneGeometryCache {
    let mut vertices = Vec::new();
    let mut batches = Vec::new();
    let mut textures = std::collections::HashMap::<TextureId, GpuPreviewTextureSource>::new();
    let mut active_texture = None;
    let mut active_start = 0_u32;

    let push_batch = |vertices_len: usize,
                      batches: &mut Vec<GpuPreviewBatch>,
                      active_texture: Option<TextureId>,
                      active_start: &mut u32| {
        let current = vertices_len as u32;
        if current > *active_start {
            batches.push(GpuPreviewBatch {
                start: *active_start,
                end: current,
                texture: active_texture,
            });
        }
        *active_start = current;
    };

    for object in &scene.objects {
        if !object.visible {
            continue;
        }
        let Primitive3d::Triangles(mesh) = &object.primitive;
        let texture = scene
            .material(object.material)
            .and_then(|material| material.albedo_texture)
            .and_then(|texture| scene.texture(texture));
        let texture_id = texture.map(|texture| texture.id);
        if texture_id != active_texture {
            push_batch(
                vertices.len(),
                &mut batches,
                active_texture,
                &mut active_start,
            );
            active_texture = texture_id;
        }
        if let Some(texture) = texture {
            textures
                .entry(texture.id)
                .or_insert_with(|| GpuPreviewTextureSource {
                    id: texture.id,
                    size: [texture.size[0].max(1) as u32, texture.size[1].max(1) as u32],
                    pixels: texture.pixels.iter().copied().map(Into::into).collect(),
                });
        }
        append_gpu_scene_mesh(&mut vertices, scene, object, &object.transform, mesh);
        for instance in &object.instances {
            let transform = combine_transform(&object.transform, instance);
            append_gpu_scene_mesh(&mut vertices, scene, object, &transform, mesh);
        }
    }
    push_batch(
        vertices.len(),
        &mut batches,
        active_texture,
        &mut active_start,
    );

    GpuSceneGeometryCache {
        signature: Some(signature),
        vertices: vertices.into(),
        batches,
        textures: textures.into_values().collect::<Vec<_>>().into(),
    }
}

#[cfg(feature = "gpu-preview")]
impl GpuSceneUniform {
    fn new(rect: egui::Rect, camera: &PreviewCamera, scene: &Scene3d) -> Self {
        let aspect = rect.width().max(1.0) / rect.height().max(1.0);
        let tan_half = (camera.fov_y * 0.5).tan().max(1.0e-4);
        let far = scene.camera.far.max(camera.near + 1.0);
        Self {
            eye: [camera.eye[0], camera.eye[1], camera.eye[2], 0.0],
            right: [camera.right[0], camera.right[1], camera.right[2], 0.0],
            up: [camera.up[0], camera.up[1], camera.up[2], 0.0],
            forward: [camera.forward[0], camera.forward[1], camera.forward[2], 0.0],
            params: [1.0 / (aspect * tan_half), 1.0 / tan_half, camera.near, far],
        }
    }
}

#[cfg(feature = "gpu-preview")]
fn append_gpu_scene_mesh(
    out: &mut Vec<GpuSceneVertex>,
    scene: &Scene3d,
    object: &Object3d,
    transform: &Transform3d,
    mesh: &TriangleMesh3d,
) {
    if mesh.vertices.is_empty() || mesh.indices.is_empty() {
        return;
    }
    let base = scene
        .material(object.material)
        .map_or(egui::Color32::WHITE, |material| material.base_color.into());
    for triangle in &mesh.indices {
        let triangle = triangle.map(|index| index as usize);
        if triangle.iter().any(|index| *index >= mesh.vertices.len()) {
            continue;
        }
        let world = [
            transform_point(transform, mesh.vertices[triangle[0]]),
            transform_point(transform, mesh.vertices[triangle[1]]),
            transform_point(transform, mesh.vertices[triangle[2]]),
        ];
        let face_normal = face_normal(world);
        for index in triangle {
            let normal = if mesh.normals.len() == mesh.vertices.len() {
                transform_normal(transform, mesh.normals[index])
            } else {
                face_normal
            };
            let color = mesh
                .vertex_colors
                .get(index)
                .copied()
                .map_or(base, |vertex| multiply_color(base, vertex.into()));
            out.push(GpuSceneVertex {
                position: transform_point(transform, mesh.vertices[index]),
                normal,
                uv: if mesh.uvs.len() == mesh.vertices.len() {
                    mesh.uvs[index]
                } else {
                    [0.0, 0.0]
                },
                color: pack_color32(color),
            });
        }
    }
}

#[cfg(feature = "gpu-preview")]
fn gpu_scene_signature(scene: &Scene3d) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    use std::hash::{Hash, Hasher};

    scene.objects.len().hash(&mut hasher);
    scene.materials.len().hash(&mut hasher);
    scene.textures.len().hash(&mut hasher);

    for material in &scene.materials {
        material.id.hash(&mut hasher);
        material.name.hash(&mut hasher);
        hash_color(&mut hasher, material.base_color);
        material.albedo_texture.hash(&mut hasher);
    }
    for texture in &scene.textures {
        texture.id.hash(&mut hasher);
        texture.name.hash(&mut hasher);
        texture.size.hash(&mut hasher);
        texture.pixels.len().hash(&mut hasher);
        hash_color_samples(&mut hasher, &texture.pixels);
    }
    for object in &scene.objects {
        object.id.hash(&mut hasher);
        object.name.hash(&mut hasher);
        object.visible.hash(&mut hasher);
        object.material.hash(&mut hasher);
        hash_transform(&mut hasher, &object.transform);
        object.instances.len().hash(&mut hasher);
        for instance in &object.instances {
            hash_transform(&mut hasher, instance);
        }
        let Primitive3d::Triangles(mesh) = &object.primitive;
        hash_mesh_shape(&mut hasher, mesh);
    }

    hasher.finish()
}

#[cfg(feature = "gpu-preview")]
fn hash_mesh_shape(hasher: &mut impl std::hash::Hasher, mesh: &TriangleMesh3d) {
    use std::hash::Hash;

    mesh.vertices.len().hash(hasher);
    mesh.indices.len().hash(hasher);
    mesh.normals.len().hash(hasher);
    mesh.uvs.len().hash(hasher);
    mesh.vertex_colors.len().hash(hasher);
    (mesh.vertices.as_ptr() as usize).hash(hasher);
    (mesh.indices.as_ptr() as usize).hash(hasher);
    (mesh.normals.as_ptr() as usize).hash(hasher);
    (mesh.uvs.as_ptr() as usize).hash(hasher);
    (mesh.vertex_colors.as_ptr() as usize).hash(hasher);
    hash_vec3_samples(hasher, &mesh.vertices);
    hash_index_samples(hasher, &mesh.indices);
    hash_vec3_samples(hasher, &mesh.normals);
    hash_uv_samples(hasher, &mesh.uvs);
    hash_color_samples(hasher, &mesh.vertex_colors);
}

#[cfg(feature = "gpu-preview")]
fn hash_transform(hasher: &mut impl std::hash::Hasher, transform: &Transform3d) {
    hash_vec3(hasher, transform.translation);
    for value in transform.rotation_xyzw {
        hash_f32(hasher, value);
    }
    hash_vec3(hasher, transform.scale);
}

#[cfg(feature = "gpu-preview")]
fn hash_vec3_samples(hasher: &mut impl std::hash::Hasher, values: &[Vec3]) {
    hash_samples(hasher, values, |hasher, value| hash_vec3(hasher, *value));
}

#[cfg(feature = "gpu-preview")]
fn hash_index_samples(hasher: &mut impl std::hash::Hasher, values: &[[u32; 3]]) {
    hash_samples(hasher, values, |hasher, value| {
        for index in *value {
            std::hash::Hash::hash(&index, hasher);
        }
    });
}

#[cfg(feature = "gpu-preview")]
fn hash_uv_samples(hasher: &mut impl std::hash::Hasher, values: &[[f32; 2]]) {
    hash_samples(hasher, values, |hasher, value| {
        hash_f32(hasher, value[0]);
        hash_f32(hasher, value[1]);
    });
}

#[cfg(feature = "gpu-preview")]
fn hash_color_samples(hasher: &mut impl std::hash::Hasher, values: &[Color]) {
    hash_samples(hasher, values, |hasher, value| hash_color(hasher, *value));
}

#[cfg(feature = "gpu-preview")]
fn hash_samples<T, H: std::hash::Hasher>(
    hasher: &mut H,
    values: &[T],
    mut hash_value: impl FnMut(&mut H, &T),
) {
    if values.is_empty() {
        return;
    }
    let last = values.len() - 1;
    let middle = values.len() / 2;
    let quarter = values.len() / 4;
    let three_quarter = values.len() * 3 / 4;
    for index in [0, quarter, middle, three_quarter, last] {
        std::hash::Hash::hash(&index, hasher);
        hash_value(hasher, &values[index]);
    }
}

#[cfg(feature = "gpu-preview")]
fn hash_vec3(hasher: &mut impl std::hash::Hasher, value: Vec3) {
    hash_f32(hasher, value[0]);
    hash_f32(hasher, value[1]);
    hash_f32(hasher, value[2]);
}

#[cfg(feature = "gpu-preview")]
fn hash_f32(hasher: &mut impl std::hash::Hasher, value: f32) {
    std::hash::Hash::hash(&value.to_bits(), hasher);
}

#[cfg(feature = "gpu-preview")]
fn hash_color(hasher: &mut impl std::hash::Hasher, value: Color) {
    std::hash::Hash::hash(&value.to_srgba_unmultiplied(), hasher);
}

#[cfg(feature = "gpu-preview")]
impl egui_wgpu::CallbackTrait for GpuPreviewCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources = callback_resources
            .entry::<GpuPreviewResources>()
            .or_insert_with(GpuPreviewResources::default);
        resources.ensure_pipeline(device, self.target_format);
        resources.ensure_white_texture(device, queue);
        for texture in self.textures.iter() {
            resources.ensure_texture(device, queue, texture);
        }

        let vertex_buffer = create_vertex_buffer(
            device,
            "mara_3d_gpu_preview_vertices",
            bytemuck::cast_slice(&self.vertices),
        );
        let target_size = gpu_preview_target_size(self.viewport_points, screen_descriptor);
        let target = create_gpu_preview_target(device, queue, resources, target_size);

        {
            let mut render_pass = egui_encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mara_3d_gpu_preview_offscreen_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();

            let (Some(mesh_pipeline), Some(white_bind_group)) = (
                resources.mesh_pipeline.as_ref(),
                resources.white_bind_group.as_ref(),
            ) else {
                return Vec::new();
            };
            render_pass.set_pipeline(mesh_pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            for batch in &self.batches {
                let bind_group = batch
                    .texture
                    .and_then(|id| {
                        resources
                            .textures
                            .get(&id)
                            .map(|texture| &texture.bind_group)
                    })
                    .unwrap_or(white_bind_group);
                render_pass.set_bind_group(0, bind_group, &[]);
                render_pass.draw(batch.start..batch.end, 0..1);
            }
        }

        resources.prepared.insert(
            self.id,
            GpuPreparedPreview {
                vertex_buffer,
                uniform_buffer: None,
                uniform_bind_group: None,
                vertex_count: self.vertices.len() as u32,
                batches: self.batches.clone(),
                target_bind_group: target.bind_group,
                target_size,
                target_texture: target.texture,
                target_view: target.view,
                depth_texture: target.depth_texture,
            },
        );

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<GpuPreviewResources>() else {
            return;
        };
        let Some(pipeline) = resources.quad_pipeline.as_ref() else {
            return;
        };
        let Some(prepared) = resources.prepared.get(&self.id) else {
            return;
        };
        if prepared.vertex_count == 0 {
            return;
        }

        let _keep_alive = (
            &prepared.vertex_buffer,
            prepared.vertex_count,
            &prepared.batches,
            prepared.target_size,
        );
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &prepared.target_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

#[cfg(feature = "gpu-preview")]
impl egui_wgpu::CallbackTrait for GpuSceneCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen_descriptor: &egui_wgpu::ScreenDescriptor,
        egui_encoder: &mut wgpu::CommandEncoder,
        callback_resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let resources = callback_resources
            .entry::<GpuPreviewResources>()
            .or_insert_with(GpuPreviewResources::default);
        resources.ensure_pipeline(device, self.target_format);
        resources.ensure_white_texture(device, queue);
        for texture in self.textures.iter() {
            resources.ensure_texture(device, queue, texture);
        }

        let (vertex_buffer, vertex_count) = {
            let cached_scene_buffer = resources.scene_vertex_buffer(
                device,
                self.geometry_signature,
                bytemuck::cast_slice(self.vertices.as_ref()),
            );
            (
                cached_scene_buffer.buffer.clone(),
                cached_scene_buffer.vertex_count,
            )
        };
        let uniform_buffer = create_uniform_buffer(
            device,
            "mara_3d_gpu_scene_uniform",
            bytemuck::bytes_of(&self.uniform),
        );
        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mara_3d_gpu_scene_uniform_bind_group"),
            layout: resources
                .scene_uniform_layout
                .as_ref()
                .expect("pipeline creates scene uniform layout"),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniform_buffer.as_entire_binding(),
            }],
        });
        let target_size = gpu_preview_target_size(self.viewport_points, screen_descriptor);
        let target = create_gpu_preview_target(device, queue, resources, target_size);

        {
            let mut render_pass = egui_encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mara_3d_gpu_scene_offscreen_pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &target.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &target.depth_view,
                        depth_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(1.0),
                            store: wgpu::StoreOp::Store,
                        }),
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();

            let (Some(scene_pipeline), Some(white_bind_group)) = (
                resources.scene_pipeline.as_ref(),
                resources.white_bind_group.as_ref(),
            ) else {
                return Vec::new();
            };
            render_pass.set_pipeline(scene_pipeline);
            render_pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            render_pass.set_bind_group(1, &uniform_bind_group, &[]);
            for batch in &self.batches {
                let bind_group = batch
                    .texture
                    .and_then(|id| {
                        resources
                            .textures
                            .get(&id)
                            .map(|texture| &texture.bind_group)
                    })
                    .unwrap_or(white_bind_group);
                render_pass.set_bind_group(0, bind_group, &[]);
                render_pass.draw(batch.start..batch.end, 0..1);
            }
        }

        resources.prepared.insert(
            self.id,
            GpuPreparedPreview {
                vertex_buffer,
                uniform_buffer: Some(uniform_buffer),
                uniform_bind_group: Some(uniform_bind_group),
                vertex_count,
                batches: self.batches.clone(),
                target_bind_group: target.bind_group,
                target_size,
                target_texture: target.texture,
                target_view: target.view,
                depth_texture: target.depth_texture,
            },
        );

        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &egui_wgpu::CallbackResources,
    ) {
        let Some(resources) = callback_resources.get::<GpuPreviewResources>() else {
            return;
        };
        let Some(pipeline) = resources.quad_pipeline.as_ref() else {
            return;
        };
        let Some(prepared) = resources.prepared.get(&self.id) else {
            return;
        };
        if prepared.vertex_count == 0 {
            return;
        }

        let _keep_alive = (
            &prepared.vertex_buffer,
            prepared.vertex_count,
            &prepared.batches,
            prepared.target_size,
        );
        render_pass.set_pipeline(pipeline);
        render_pass.set_bind_group(0, &prepared.target_bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }
}

#[cfg(feature = "gpu-preview")]
impl GpuPreviewResources {
    fn scene_vertex_buffer(
        &mut self,
        device: &wgpu::Device,
        signature: u64,
        vertices: &[u8],
    ) -> &GpuSceneVertexBuffer {
        if !self.scene_vertex_buffers.contains_key(&signature) {
            if self.scene_vertex_buffers.len() > 8 {
                self.scene_vertex_buffers.clear();
            }
            let buffer = create_vertex_buffer(device, "mara_3d_gpu_scene_vertices", vertices);
            self.scene_vertex_buffers.insert(
                signature,
                GpuSceneVertexBuffer {
                    buffer,
                    vertex_count: (vertices.len() / std::mem::size_of::<GpuSceneVertex>()) as u32,
                },
            );
        }
        self.scene_vertex_buffers
            .get(&signature)
            .expect("scene vertex buffer was inserted")
    }

    fn ensure_pipeline(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat) {
        if self.mesh_pipeline.is_some()
            && self.scene_pipeline.is_some()
            && self.quad_pipeline.is_some()
            && self.pipeline_format == Some(format)
        {
            return;
        }

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mara_3d_gpu_preview_texture_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let scene_uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mara_3d_gpu_scene_uniform_layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mara_3d_gpu_preview_pipeline_layout"),
            bind_group_layouts: &[Some(&bind_group_layout)],
            immediate_size: 0,
        });
        let scene_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mara_3d_gpu_scene_pipeline_layout"),
                bind_group_layouts: &[Some(&bind_group_layout), Some(&scene_uniform_layout)],
                immediate_size: 0,
            });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mara_3d_gpu_preview_shader"),
            source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(GPU_PREVIEW_WGSL)),
        });
        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mara_3d_gpu_preview_mesh_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuPreviewVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x2,
                        2 => Uint32
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_mesh_gamma"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let scene_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mara_3d_gpu_scene_pipeline"),
            layout: Some(&scene_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_scene"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<GpuSceneVertex>() as wgpu::BufferAddress,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x3,
                        1 => Float32x3,
                        2 => Float32x2,
                        3 => Uint32
                    ],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_scene"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });
        let quad_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mara_3d_gpu_preview_quad_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_quad"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some(if format.is_srgb() {
                    "fs_quad_linear_framebuffer"
                } else {
                    "fs_quad_gamma_framebuffer"
                }),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::One,
                            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::OneMinusDstAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

        self.pipeline_format = Some(format);
        self.mesh_pipeline = Some(mesh_pipeline);
        self.scene_pipeline = Some(scene_pipeline);
        self.quad_pipeline = Some(quad_pipeline);
        self.bind_group_layout = Some(bind_group_layout);
        self.scene_uniform_layout = Some(scene_uniform_layout);
        self.sampler = Some(device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mara_3d_gpu_preview_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            address_mode_w: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        }));
        self.white_bind_group = None;
        self.textures.clear();
    }

    fn ensure_white_texture(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.white_bind_group.is_some() {
            return;
        }
        let source = GpuPreviewTextureSource {
            id: TextureId(0),
            size: [1, 1],
            pixels: vec![egui::Color32::WHITE],
        };
        self.white_bind_group = Some(create_texture_bind_group(device, queue, self, &source));
    }

    fn ensure_texture(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        source: &GpuPreviewTextureSource,
    ) {
        let hash = texture_source_hash(source);
        if self
            .textures
            .get(&source.id)
            .is_some_and(|texture| texture.hash == hash)
        {
            return;
        }
        let bind_group = create_texture_bind_group(device, queue, self, source);
        self.textures
            .insert(source.id, GpuPreviewTextureResource { hash, bind_group });
    }
}

#[cfg(feature = "gpu-preview")]
fn create_vertex_buffer(device: &wgpu::Device, label: &str, bytes: &[u8]) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::VERTEX,
        mapped_at_creation: true,
    });
    {
        let mut mapped = buffer.slice(..).get_mapped_range_mut();
        mapped.slice(..bytes.len()).copy_from_slice(bytes);
    }
    buffer.unmap();
    buffer
}

#[cfg(feature = "gpu-preview")]
fn create_uniform_buffer(device: &wgpu::Device, label: &str, bytes: &[u8]) -> wgpu::Buffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: bytes.len().max(4) as wgpu::BufferAddress,
        usage: wgpu::BufferUsages::UNIFORM,
        mapped_at_creation: true,
    });
    {
        let mut mapped = buffer.slice(..).get_mapped_range_mut();
        mapped.slice(..bytes.len()).copy_from_slice(bytes);
    }
    buffer.unmap();
    buffer
}

#[cfg(feature = "gpu-preview")]
fn create_texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    resources: &GpuPreviewResources,
    source: &GpuPreviewTextureSource,
) -> wgpu::BindGroup {
    let size = wgpu::Extent3d {
        width: source.size[0].max(1),
        height: source.size[1].max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mara_3d_gpu_preview_texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let bytes = texture_pixels_as_bytes(&source.pixels, size.width as usize, size.height as usize);
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(4 * size.width),
            rows_per_image: Some(size.height),
        },
        size,
    );
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mara_3d_gpu_preview_texture_bind_group"),
        layout: resources
            .bind_group_layout
            .as_ref()
            .expect("pipeline creates bind group layout before textures"),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(
                    resources
                        .sampler
                        .as_ref()
                        .expect("pipeline creates sampler before textures"),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    })
}

#[cfg(feature = "gpu-preview")]
fn create_gpu_preview_target(
    device: &wgpu::Device,
    _queue: &wgpu::Queue,
    resources: &GpuPreviewResources,
    size: [u32; 2],
) -> GpuPreviewTarget {
    let extent = wgpu::Extent3d {
        width: size[0].max(1),
        height: size[1].max(1),
        depth_or_array_layers: 1,
    };
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mara_3d_gpu_preview_target"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mara_3d_gpu_preview_depth"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Depth32Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let depth_view = depth_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mara_3d_gpu_preview_target_bind_group"),
        layout: resources
            .bind_group_layout
            .as_ref()
            .expect("pipeline creates bind group layout before targets"),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Sampler(
                    resources
                        .sampler
                        .as_ref()
                        .expect("pipeline creates sampler before targets"),
                ),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&view),
            },
        ],
    });
    GpuPreviewTarget {
        texture,
        view,
        depth_texture,
        depth_view,
        bind_group,
    }
}

#[cfg(feature = "gpu-preview")]
fn gpu_preview_target_size(
    viewport_points: [f32; 2],
    screen_descriptor: &egui_wgpu::ScreenDescriptor,
) -> [u32; 2] {
    let low_width = (viewport_points[0] * screen_descriptor.pixels_per_point)
        .round()
        .max(1.0) as usize;
    let low_height = (viewport_points[1] * screen_descriptor.pixels_per_point)
        .round()
        .max(1.0) as usize;
    let scale = OBJECT_SSAA_SCALE
        .min((OBJECT_SSAA_MAX_DIMENSION / low_width.max(low_height)).max(1))
        .max(1);
    [(low_width * scale) as u32, (low_height * scale) as u32]
}

#[cfg(feature = "gpu-preview")]
fn texture_pixels_as_bytes(pixels: &[egui::Color32], width: usize, height: usize) -> Vec<u8> {
    let mut bytes = vec![255_u8; width * height * 4];
    for (i, pixel) in pixels.iter().take(width * height).enumerate() {
        let offset = i * 4;
        bytes[offset] = pixel.r();
        bytes[offset + 1] = pixel.g();
        bytes[offset + 2] = pixel.b();
        bytes[offset + 3] = pixel.a();
    }
    bytes
}

#[cfg(feature = "gpu-preview")]
fn texture_source_hash(source: &GpuPreviewTextureSource) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.id.hash(&mut hasher);
    source.size.hash(&mut hasher);
    for pixel in &source.pixels {
        pixel.to_array().hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(feature = "gpu-preview")]
#[allow(dead_code)]
fn point_to_viewport_ndc(rect: egui::Rect, point: egui::Pos2) -> [f32; 2] {
    [
        ((point.x - rect.left()) / rect.width().max(1.0)) * 2.0 - 1.0,
        1.0 - ((point.y - rect.top()) / rect.height().max(1.0)) * 2.0,
    ]
}

#[cfg(feature = "gpu-preview")]
#[allow(dead_code)]
fn gpu_depth(depth: f32) -> f32 {
    (depth / 10_000.0).clamp(0.0, 1.0)
}

#[cfg(feature = "gpu-preview")]
fn pack_color32(color: egui::Color32) -> u32 {
    u32::from(color.r())
        | (u32::from(color.g()) << 8)
        | (u32::from(color.b()) << 16)
        | (u32::from(color.a()) << 24)
}

#[cfg(feature = "gpu-preview")]
fn next_gpu_callback_id() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(feature = "gpu-preview")]
const GPU_PREVIEW_WGSL: &str = r#"
struct VertexOut {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @builtin(position) position: vec4<f32>,
};

struct SceneOut {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) normal: vec3<f32>,
    @builtin(position) position: vec4<f32>,
};

struct QuadOut {
    @location(0) uv: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

struct SceneUniform {
    eye: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    forward: vec4<f32>,
    params: vec4<f32>,
};

fn linear_from_gamma_rgb(srgb: vec3<f32>) -> vec3<f32> {
    let cutoff = srgb < vec3<f32>(0.04045);
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, cutoff);
}

fn unpack_color(color: u32) -> vec4<f32> {
    return vec4<f32>(
        f32(color & 255u),
        f32((color >> 8u) & 255u),
        f32((color >> 16u) & 255u),
        f32((color >> 24u) & 255u),
    ) / 255.0;
}

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: u32,
) -> VertexOut {
    var out: VertexOut;
    out.position = vec4<f32>(position, 1.0);
    out.uv = uv;
    out.color = unpack_color(color);
    return out;
}

@vertex
fn vs_quad(@builtin(vertex_index) vertex_index: u32) -> QuadOut {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -3.0),
        vec2<f32>(-1.0, 1.0),
        vec2<f32>(3.0, 1.0),
    );
    var uvs = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 2.0),
        vec2<f32>(0.0, 0.0),
        vec2<f32>(2.0, 0.0),
    );
    var out: QuadOut;
    out.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    out.uv = uvs[vertex_index];
    return out;
}

@group(0) @binding(0) var r_sampler: sampler;
@group(0) @binding(1) var r_texture: texture_2d<f32>;
@group(1) @binding(0) var<uniform> scene: SceneUniform;

fn preview_color(in: VertexOut) -> vec4<f32> {
    return in.color * textureSample(r_texture, r_sampler, in.uv);
}

@fragment
fn fs_mesh_gamma(in: VertexOut) -> @location(0) vec4<f32> {
    return preview_color(in);
}

@vertex
fn vs_scene(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: u32,
) -> SceneOut {
    let rel = position - scene.eye.xyz;
    let z = max(dot(rel, scene.forward.xyz), scene.params.z);
    let ndc = vec2<f32>(
        dot(rel, scene.right.xyz) * scene.params.x / z,
        dot(rel, scene.up.xyz) * scene.params.y / z,
    );
    var out: SceneOut;
    out.position = vec4<f32>(
        ndc.x,
        ndc.y,
        clamp((z - scene.params.z) / max(scene.params.w - scene.params.z, 1.0), 0.0, 1.0),
        1.0,
    );
    out.uv = uv;
    out.color = unpack_color(color);
    out.normal = normal;
    return out;
}

fn shade_scene_color(base: vec4<f32>, normal_raw: vec3<f32>) -> vec4<f32> {
    var normal = normalize(normal_raw);
    if dot(normal, scene.forward.xyz) > 0.0 {
        normal = -normal;
    }
    let key_dir = normalize(vec3<f32>(0.8, 1.8, 1.25));
    let fill_dir = normalize(vec3<f32>(-1.2, 0.65, -1.8));
    let view_dir = -scene.forward.xyz;
    let key = pow(max(dot(key_dir, normal), 0.0), 0.72);
    let fill = max(dot(fill_dir, normal), 0.0) * 0.20;
    let headlight = max(dot(view_dir, normal), 0.0) * 0.18;
    let sky = max(normal.y, 0.0) * 0.10;
    let view = clamp(abs(dot(normal, view_dir)), 0.0, 1.0);
    let rim = pow(1.0 - view, 2.35) * 0.11;
    let half_vector = normalize(key_dir + view_dir);
    let specular = pow(max(dot(normal, half_vector), 0.0), 34.0) * 0.13;
    let diffuse = clamp(0.36 + key * 0.68 + fill + headlight + sky, 0.0, 1.35);
    let value = clamp((1.0 - 0.56) + diffuse * 0.56, 0.42, 1.12);
    var color = vec4<f32>(base.rgb * min(value, 1.0), base.a);
    if value > 1.0 {
        color = vec4<f32>(mix(color.rgb, vec3<f32>(1.0), (value - 1.0) * 0.55), color.a);
    }
    color = vec4<f32>(mix(color.rgb, vec3<f32>(1.0), clamp(specular + rim * 0.55, 0.0, 0.22)), color.a);
    return color;
}

@fragment
fn fs_scene(in: SceneOut) -> @location(0) vec4<f32> {
    let tex = textureSample(r_texture, r_sampler, in.uv);
    return shade_scene_color(in.color * tex, in.normal);
}

@fragment
fn fs_quad_linear_framebuffer(in: QuadOut) -> @location(0) vec4<f32> {
    let color = textureSample(r_texture, r_sampler, in.uv);
    return vec4<f32>(linear_from_gamma_rgb(color.rgb), color.a);
}

@fragment
fn fs_quad_gamma_framebuffer(in: QuadOut) -> @location(0) vec4<f32> {
    return textureSample(r_texture, r_sampler, in.uv);
}

@fragment
fn fs_main_linear_framebuffer(in: VertexOut) -> @location(0) vec4<f32> {
    let color = preview_color(in);
    return vec4<f32>(linear_from_gamma_rgb(color.rgb), color.a);
}

@fragment
fn fs_main_gamma_framebuffer(in: VertexOut) -> @location(0) vec4<f32> {
    return preview_color(in);
}
"#;

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

#[cfg(feature = "gpu-preview")]
#[allow(dead_code)]
fn paint_faces_gpu(
    painter: &egui::Painter,
    rect: egui::Rect,
    callback_id: u64,
    target_format: wgpu::TextureFormat,
    faces: Vec<PreviewFace>,
) {
    if faces.is_empty() {
        return;
    }
    let callback = GpuPreviewCallback::from_faces(callback_id, target_format, rect, faces);
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
}

#[cfg(feature = "gpu-preview")]
fn paint_scene_gpu(
    painter: &egui::Painter,
    rect: egui::Rect,
    callback_id: u64,
    target_format: wgpu::TextureFormat,
    camera: &PreviewCamera,
    scene: &Scene3d,
    geometry: &GpuSceneGeometryCache,
) {
    let callback =
        GpuSceneCallback::from_geometry(callback_id, target_format, rect, camera, scene, geometry);
    if callback.vertices.is_empty() {
        return;
    }
    painter.add(egui_wgpu::Callback::new_paint_callback(rect, callback));
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
            let shaded = interpolate_color(face.fills, [b0, b1, b2]);
            pixels[index] = if let (Some(uvs), Some(texture)) = (face.uvs, face.texture.as_ref()) {
                let uv = [
                    uvs[0][0] * b0 + uvs[1][0] * b1 + uvs[2][0] * b2,
                    uvs[0][1] * b0 + uvs[1][1] * b1 + uvs[2][1] * b2,
                ];
                multiply_color(shaded, texture.sample(uv).into())
            } else {
                shaded
            };
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

fn multiply_color(a: egui::Color32, b: egui::Color32) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(
        ((a.r() as u16 * b.r() as u16) / 255) as u8,
        ((a.g() as u16 * b.g() as u16) / 255) as u8,
        ((a.b() as u16 * b.b() as u16) / 255) as u8,
        ((a.a() as u16 * b.a() as u16) / 255) as u8,
    )
}

fn paint_gizmo_arrow_head(
    painter: &egui::Painter,
    base_start: egui::Pos2,
    tip: egui::Pos2,
    color: egui::Color32,
    stroke_width: f32,
) {
    let screen_dir = tip - base_start;
    if screen_dir.length_sq() <= 1.0e-4 {
        return;
    }
    let screen_dir = screen_dir.normalized();
    let side = egui::vec2(-screen_dir.y, screen_dir.x);
    let tip_len = stroke_width * 2.4;
    let tip_width = tip_len * 0.58;
    let base = tip - screen_dir * tip_len;
    painter.add(egui::Shape::convex_polygon(
        vec![tip, base + side * tip_width, base - side * tip_width],
        color,
        egui::Stroke::NONE,
    ));
}

fn update_best_gizmo_pick(
    best: &mut Option<(GizmoOperation, f32)>,
    operation: GizmoOperation,
    distance: f32,
) {
    if distance <= GIZMO_PICK_DISTANCE
        && best.is_none_or(|(_, best_distance)| distance < best_distance)
    {
        *best = Some((operation, distance));
    }
}

fn gizmo_operation_highlighted(
    operation: GizmoOperation,
    hover_operation: Option<GizmoOperation>,
    active_operation: Option<GizmoOperation>,
) -> bool {
    active_operation == Some(operation)
        || active_operation.is_none() && hover_operation == Some(operation)
}

fn highlighted_width(width: f32, highlighted: bool) -> f32 {
    if highlighted {
        width * GIZMO_HIGHLIGHT_WIDTH_SCALE
    } else {
        width
    }
}

fn project_screen_direction(
    rect: egui::Rect,
    camera: &PreviewCamera,
    origin: Vec3,
    direction: Vec3,
    world_per_point: f32,
) -> egui::Vec2 {
    let distance = (world_per_point * 64.0).max(0.01);
    let Some((a, _)) = camera.project(rect, origin) else {
        return egui::Vec2::X;
    };
    let Some((b, _)) = camera.project(rect, add3(origin, mul3(direction, distance))) else {
        return egui::Vec2::X;
    };
    let delta = b - a;
    if delta.length_sq() <= 1.0e-6 {
        egui::Vec2::X
    } else {
        delta.normalized()
    }
}

fn distance_to_screen_segment(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_sq();
    if len_sq <= f32::EPSILON {
        return point.distance(a);
    }
    let t = ((point - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    point.distance(a + ab * t)
}

fn point_in_screen_polygon(point: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y).max(1.0e-6) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn paint_projected_polyline(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &PreviewCamera,
    points: &[Vec3],
    closed: bool,
    stroke: egui::Stroke,
) {
    if points.len() < 2 {
        return;
    }
    let mut projected = Vec::with_capacity(points.len() + usize::from(closed));
    for point in points {
        if let Some((screen, _)) = camera.project(rect, *point) {
            projected.push(screen);
        }
    }
    if closed && projected.len() > 2 {
        projected.push(projected[0]);
    }
    if projected.len() >= 2 {
        painter.add(egui::Shape::line(projected, stroke));
    }
}

fn sampled_gizmo_ellipse(
    center: Vec3,
    radii: [f32; 2],
    segments: usize,
    start: f32,
    end: f32,
) -> Vec<Vec3> {
    let segments = segments.max(3);
    (0..=segments)
        .map(|i| {
            let t = i as f32 / segments as f32;
            let angle = start + (end - start) * t;
            [
                center[0] + radii[0] * angle.cos(),
                center[1],
                center[2] + radii[1] * angle.sin(),
            ]
        })
        .collect()
}

fn paint_gizmo_axis_segment(
    painter: &egui::Painter,
    rect: egui::Rect,
    camera: &PreviewCamera,
    a: Vec3,
    b: Vec3,
    axis: GizmoAxis,
    width: f32,
) {
    if let (Some((a, _)), Some((b, _))) = (camera.project(rect, a), camera.project(rect, b)) {
        let color = gizmo_axis_color(axis, 1.0, true);
        painter.line_segment([a, b], egui::Stroke::new(width.max(1.0), color));
        paint_gizmo_arrow_head(painter, a, b, color, width.max(1.0));
    }
}

fn pointer_angle(origin: egui::Pos2, pointer: egui::Pos2) -> f32 {
    let delta = pointer - origin;
    delta.y.atan2(delta.x)
}

fn wrap_angle(angle: f32) -> f32 {
    (angle + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
}

fn axis_angle_quat(axis: Vec3, angle: f32) -> Quat {
    let axis = normalize3(axis);
    let half = angle * 0.5;
    let sin = half.sin();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, half.cos()]
}

fn quat_mul(a: Quat, b: Quat) -> Quat {
    normalize_quat([
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ])
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

fn combine_transform(parent: &Transform3d, child: &Transform3d) -> Transform3d {
    Transform3d {
        translation: transform_point(parent, child.translation),
        rotation_xyzw: quat_mul(parent.rotation_xyzw, child.rotation_xyzw),
        scale: [
            parent.scale[0] * child.scale[0],
            parent.scale[1] * child.scale[1],
            parent.scale[2] * child.scale[2],
        ],
    }
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

fn interactive_triangle_stride(
    mesh: &TriangleMesh3d,
    selected: bool,
    interactive_preview: bool,
) -> usize {
    if !interactive_preview {
        return 1;
    }
    let budget = if selected {
        INTERACTIVE_SELECTED_TRIANGLE_BUDGET
    } else {
        INTERACTIVE_BACKGROUND_TRIANGLE_BUDGET
    };
    mesh.indices.len().div_ceil(budget.max(1)).max(1)
}

fn object_world_radius(object: &Object3d) -> f32 {
    let local = match &object.primitive {
        Primitive3d::Triangles(mesh) => local_radius(&mesh.vertices),
    };
    local
        * object
            .transform
            .scale
            .iter()
            .copied()
            .fold(0.0_f32, |acc, value| acc.max(value.abs()))
}

fn gizmo_axis_color(axis: GizmoAxis, visibility: f32, highlighted: bool) -> egui::Color32 {
    let (r, g, b) = match axis {
        GizmoAxis::X => (255, 0, 125),
        GizmoAxis::Y => (0, 255, 125),
        GizmoAxis::Z => (0, 125, 255),
    };
    let alpha_base = if highlighted {
        1.0
    } else {
        GIZMO_INACTIVE_ALPHA
    };
    let color = egui::Color32::from_rgba_unmultiplied(r, g, b, 255);
    let color = if highlighted {
        tint_color(color, egui::Color32::WHITE, 0.22)
    } else {
        color
    };
    let alpha = (255.0 * alpha_base * visibility.clamp(0.0, 1.0))
        .round()
        .clamp(0.0, 255.0) as u8;
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}

fn gizmo_view_color(highlighted: bool) -> egui::Color32 {
    let alpha = (255.0
        * if highlighted {
            1.0
        } else {
            GIZMO_INACTIVE_ALPHA
        })
    .round() as u8;
    egui::Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
}

fn gizmo_arrow_visibility(camera: &PreviewCamera, origin: Vec3, direction: Vec3) -> f32 {
    let eye_to_model = normalize3(sub3(origin, camera.eye));
    let dot = dot3(eye_to_model, direction).abs();
    (1.0 - (dot - GIZMO_ARROW_FADE_START) / (GIZMO_ARROW_FADE_END - GIZMO_ARROW_FADE_START))
        .min(1.0)
}

fn gizmo_plane_visibility(camera: &PreviewCamera, origin: Vec3, normal: Vec3) -> f32 {
    let eye_to_model = normalize3(sub3(origin, camera.eye));
    let dot = dot3(eye_to_model, normal).abs();
    (1.0 - ((1.0 - dot) - GIZMO_PLANE_FADE_START) / (GIZMO_PLANE_FADE_END - GIZMO_PLANE_FADE_START))
        .min(1.0)
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
    tint_color(
        mara_core::style::on_panel_dim().into(),
        accent,
        GRID_ACCENT_MIX,
    )
}

fn face_normal(points: [Vec3; 3]) -> Vec3 {
    normalize3(cross3(
        sub3(points[1], points[0]),
        sub3(points[2], points[0]),
    ))
}

/// Build a flat-shaded mesh by duplicating vertices per triangle so each
/// face carries its own normal. Use this for hard-edged polyhedra where
/// averaging normals at shared vertices would smear the shading across
/// faces with different orientations.
fn flat_shaded_mesh(vertices: &[Vec3], indices: &[[u32; 3]]) -> TriangleMesh3d {
    let mut out_v = Vec::with_capacity(indices.len() * 3);
    let mut out_n = Vec::with_capacity(indices.len() * 3);
    let mut out_i = Vec::with_capacity(indices.len());
    for tri in indices {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;
        if i0 >= vertices.len() || i1 >= vertices.len() || i2 >= vertices.len() {
            continue;
        }
        let a = vertices[i0];
        let b = vertices[i1];
        let c = vertices[i2];
        let n = face_normal([a, b, c]);
        let base = out_v.len() as u32;
        out_v.push(a);
        out_v.push(b);
        out_v.push(c);
        out_n.push(n);
        out_n.push(n);
        out_n.push(n);
        out_i.push([base, base + 1, base + 2]);
    }
    TriangleMesh3d::with_normals(out_v, out_i, out_n)
}

fn shade_color(base: egui::Color32, normal: Vec3, camera: &PreviewCamera) -> egui::Color32 {
    let mut normal = normalize3(normal);
    if dot3(normal, camera.forward) > 0.0 {
        normal = mul3(normal, -1.0);
    }

    let key_dir = normalize3(TECH_LIGHT_KEY);
    let fill_dir = normalize3(TECH_LIGHT_FILL);
    let view_dir = mul3(camera.forward, -1.0);

    let key = dot3(key_dir, normal).max(0.0).powf(0.72);
    let fill = dot3(fill_dir, normal).max(0.0) * TECH_LIGHT_FILL_STRENGTH;
    let headlight = dot3(view_dir, normal).max(0.0) * TECH_LIGHT_HEADLIGHT_STRENGTH;
    let sky = normal[1].max(0.0) * TECH_LIGHT_SKY_STRENGTH;
    let view = dot3(normal, view_dir).abs().clamp(0.0, 1.0);
    let rim = (1.0 - view).powf(2.35) * TECH_LIGHT_RIM_STRENGTH;
    let half_vector = normalize3(add3(key_dir, view_dir));
    let specular = dot3(normal, half_vector)
        .max(0.0)
        .powf(TECH_LIGHT_SPECULAR_POWER)
        * TECH_LIGHT_SPECULAR_STRENGTH;

    let diffuse = (TECH_LIGHT_AMBIENT + key * 0.68 + fill + headlight + sky).clamp(0.0, 1.35);
    let value = ((1.0 - TECH_LIGHT_CONTRAST) + diffuse * TECH_LIGHT_CONTRAST).clamp(0.42, 1.12);
    let mut color = shade_scalar(base, value.min(1.0));
    if value > 1.0 {
        color = tint_color(color, egui::Color32::WHITE, (value - 1.0) * 0.55);
    }
    tint_color(
        color,
        egui::Color32::WHITE,
        (specular + rim * 0.55).clamp(0.0, 0.22),
    )
}

fn shade_vertex_color(
    base: egui::Color32,
    vertex_colors: &[Color],
    index: usize,
    normal: Vec3,
    camera: &PreviewCamera,
) -> egui::Color32 {
    let base = vertex_colors
        .get(index)
        .copied()
        .map_or(base, |vertex| multiply_color(base, vertex.into()));
    shade_color(base, normal, camera)
}

fn vertex_color_or_base(
    shaded_base: egui::Color32,
    vertex_colors: &[Color],
    index: usize,
) -> egui::Color32 {
    vertex_colors
        .get(index)
        .copied()
        .map_or(shaded_base, |vertex| {
            multiply_color(shaded_base, vertex.into())
        })
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
        Camera3d, Color, Gizmo3d, Gizmo3dKind, Gizmo3dStyle, GizmoId, Light3d, LightId, Material3d,
        MaterialId, Object3d, ObjectId, Orbit3d, Primitive3d, Renderer3d, Scene3d, Texture3d,
        TextureId, Transform3d, Vec3, View3d, Viewport3d,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Characterization tests (plans/012): pin the current behaviour of
    // the retained scene state machine and CPU geometry so later rework
    // has a regression net. `three_d` renders through wgpu, not
    // `PaintCmd`, so this is *not* a headless-portability proof — it is a
    // behaviour freeze of the parts that need no GPU/window.

    #[test]
    fn scene_new_seeds_one_material_two_lights_no_objects() {
        let scene = Scene3d::new("Test");
        assert_eq!(scene.objects.len(), 0);
        assert_eq!(scene.materials.len(), 1);
        assert_eq!(scene.lights.len(), 2);
        assert!(scene.material(MaterialId(1)).is_some());
        assert!(scene.material(MaterialId(999)).is_none());
    }

    #[test]
    fn scene_add_object_allocates_incrementing_ids_and_retrieves() {
        let mut scene = Scene3d::new("Test");
        let base = MaterialId(1);
        let first = scene.add_object("a", Primitive3d::square(1.0), base);
        let second = scene.add_object(
            "b",
            Primitive3d::triangle([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]),
            base,
        );
        assert_eq!(first, ObjectId(1));
        assert_eq!(second, ObjectId(2));
        assert_eq!(scene.objects.len(), 2);
        assert!(scene.object(first).is_some());
        assert!(scene.object(ObjectId(999)).is_none());
    }

    #[test]
    fn scene_add_material_allocates_from_two_and_retrieves() {
        let mut scene = Scene3d::new("Test");
        let added = scene.add_material("Custom", MaraColor32::WHITE);
        assert_eq!(added, MaterialId(2));
        assert!(scene.material(added).is_some());
        assert_eq!(scene.materials.len(), 2);
    }

    #[test]
    fn primitive_plane_lowers_to_four_vertex_two_triangle_mesh() {
        let Primitive3d::Triangles(mesh) = Primitive3d::square(2.0);
        assert_eq!(mesh.vertices.len(), 4);
        assert_eq!(mesh.indices.len(), 2);
    }

    #[test]
    fn triangle_mesh_new_keeps_supplied_geometry() {
        let mesh = TriangleMesh3d::new(
            vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            vec![[0, 1, 2]],
        );
        assert_eq!(mesh.vertices.len(), 3);
        assert_eq!(mesh.indices.len(), 1);
    }
}
