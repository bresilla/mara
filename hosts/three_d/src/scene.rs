//! Retained 3D document model: cameras, transforms, meshes, materials,
//! textures, objects, gizmos, lights, and the [`Scene3d`] that owns them,
//! plus the mesh-construction and polygon-triangulation helpers they are
//! built from.
//!
//! This is the bottom of the crate's dependency stack — it describes
//! geometry and knows nothing about viewports, rendering, or input. Split
//! out of `lib.rs` verbatim (PLAN.md WS-C2.1) so the rendering code above
//! it is reviewable on its own.

use super::*;

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
