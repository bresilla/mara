//! Rasterise a headless Mara pass to a PNG so the result can be
//! *looked at* rather than inferred from vertex counts.
//!
//! Every visual regression in the node graph so far was found by a
//! person running the app and describing it, then guessed at from the
//! description. Geometry assertions prove a shape reached the painter;
//! they say nothing about whether the result is legible. This closes
//! that gap without launching anything: egui hands back tessellated
//! triangles plus its font atlas, and both are enough to reproduce what
//! the GPU would have drawn.

#![allow(dead_code)]

use mara::ui::mara_core;
use mara_core::MaraUi;

pub const W: usize = 1600;
pub const H: usize = 1000;

/// Alpha-over compositing of one pixel.
fn blend(dst: &mut [u8; 4], src: [f32; 4]) {
    let a = src[3];
    for c in 0..3 {
        dst[c] = (src[c] * a + f32::from(dst[c]) * (1.0 - a)).clamp(0.0, 255.0) as u8;
    }
    dst[3] = 255;
}

/// Rasterise one tessellated mesh with barycentric interpolation of
/// colour and texture coordinates.
///
/// The font atlas is sampled for glyphs; egui packs solid shapes onto a
/// white texel of the same atlas, so one sampler serves both and the
/// output matches what the GPU produces.
fn draw_mesh(
    buf: &mut [[u8; 4]],
    mesh: &egui::epaint::Mesh,
    atlas: &egui::ColorImage,
    clip: egui::Rect,
) {
    for tri in mesh.indices.chunks_exact(3) {
        let v = [
            &mesh.vertices[tri[0] as usize],
            &mesh.vertices[tri[1] as usize],
            &mesh.vertices[tri[2] as usize],
        ];
        let min_x = v
            .iter()
            .map(|p| p.pos.x)
            .fold(f32::MAX, f32::min)
            .max(clip.min.x)
            .max(0.0);
        let max_x = v
            .iter()
            .map(|p| p.pos.x)
            .fold(f32::MIN, f32::max)
            .min(clip.max.x)
            .min(W as f32 - 1.0);
        let min_y = v
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::MAX, f32::min)
            .max(clip.min.y)
            .max(0.0);
        let max_y = v
            .iter()
            .map(|p| p.pos.y)
            .fold(f32::MIN, f32::max)
            .min(clip.max.y)
            .min(H as f32 - 1.0);
        if max_x < min_x || max_y < min_y {
            continue;
        }

        let area = (v[1].pos.x - v[0].pos.x) * (v[2].pos.y - v[0].pos.y)
            - (v[2].pos.x - v[0].pos.x) * (v[1].pos.y - v[0].pos.y);
        if area.abs() < 1e-6 {
            continue;
        }

        for y in (min_y as usize)..=(max_y as usize) {
            for x in (min_x as usize)..=(max_x as usize) {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let w0 = ((v[1].pos.x - px) * (v[2].pos.y - py)
                    - (v[2].pos.x - px) * (v[1].pos.y - py))
                    / area;
                let w1 = ((v[2].pos.x - px) * (v[0].pos.y - py)
                    - (v[0].pos.x - px) * (v[2].pos.y - py))
                    / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }

                let mut rgba = [0.0_f32; 4];
                for (i, vert) in v.iter().enumerate() {
                    let w = [w0, w1, w2][i];
                    let c = vert.color.to_array();
                    for ch in 0..4 {
                        rgba[ch] += f32::from(c[ch]) * w;
                    }
                }

                let u = v[0].uv.x * w0 + v[1].uv.x * w1 + v[2].uv.x * w2;
                let vv = v[0].uv.y * w0 + v[1].uv.y * w1 + v[2].uv.y * w2;
                let tx = ((u * atlas.size[0] as f32) as usize).min(atlas.size[0] - 1);
                let ty = ((vv * atlas.size[1] as f32) as usize).min(atlas.size[1] - 1);
                let texel = atlas.pixels[ty * atlas.size[0] + tx].to_array();
                for ch in 0..4 {
                    rgba[ch] = rgba[ch] * f32::from(texel[ch]) / 255.0;
                }

                blend(
                    &mut buf[y * W + x],
                    [rgba[0], rgba[1], rgba[2], rgba[3] / 255.0],
                );
            }
        }
    }
}

/// Run `body` for four passes and write the last one to `path`.
///
/// Four passes because the first is laid out against a stale or empty
/// state; sampling it is how an earlier round of this work drew
/// conclusions from colours that were never on screen.
pub fn snapshot(path: &str, accent: mara_core::vocab::Color32, mut body: impl FnMut(&mut MaraUi<'_>)) {
    let ctx = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(W as f32, H as f32));
    mara_backend_egui::theme::__internal_apply_theme(
        &ctx,
        mara_core::style::AccentColor(accent),
        mara_core::style::GlassOpacity::default(),
    );

    let mut buf = vec![[18_u8, 18, 20, 255]; W * H];
    for _ in 0..4 {
        ctx.begin_pass(egui::RawInput {
            screen_rect: Some(screen),
            ..Default::default()
        });
        #[allow(deprecated)]
        egui::CentralPanel::default().show(&ctx, |ui| {
            let mut b = mara_backend_egui::EguiUiBackend::new(ui);
            MaraUi::__internal_over_backend_ret(&mut b, accent, |mara| body(mara));
        });
        let out = ctx.end_pass();

        let atlas = out
            .textures_delta
            .set
            .iter()
            .find_map(|(_, d)| match &d.image {
                egui::ImageData::Color(c) => Some((**c).clone()),
            })
            .unwrap_or_else(|| egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]));

        buf.iter_mut().for_each(|p| *p = [18, 18, 20, 255]);
        for prim in ctx.tessellate(out.shapes, out.pixels_per_point) {
            if let egui::epaint::Primitive::Mesh(m) = prim.primitive {
                draw_mesh(&mut buf, &m, &atlas, prim.clip_rect);
            }
        }
    }

    let flat: Vec<u8> = buf.iter().flat_map(|p| p.iter().copied()).collect();
    image::save_buffer(path, &flat, W as u32, H as u32, image::ColorType::Rgba8)
        .expect("write png");
    eprintln!("wrote {path}");
}
