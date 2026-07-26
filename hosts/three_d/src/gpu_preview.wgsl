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
