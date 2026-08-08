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

/// Bilinear tap into the font atlas.
///
/// Nearest-neighbour was making every glyph read as a solid blob, which
/// is not a property of the renderer under review — it made text and
/// icons impossible to judge from a snapshot, and cost a wrong
/// diagnosis of the icon font as broken.
fn sample_bilinear(atlas: &egui::ColorImage, u: f32, v: f32) -> [f32; 4] {
    let (w, h) = (atlas.size[0], atlas.size[1]);
    let x = (u * w as f32 - 0.5).clamp(0.0, (w - 1) as f32);
    let y = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);

    let at = |px: usize, py: usize| atlas.pixels[py * w + px].to_array();
    let (p00, p10, p01, p11) = (at(x0, y0), at(x1, y0), at(x0, y1), at(x1, y1));

    let mut out = [0.0_f32; 4];
    for c in 0..4 {
        let top = f32::from(p00[c]) * (1.0 - fx) + f32::from(p10[c]) * fx;
        let bot = f32::from(p01[c]) * (1.0 - fx) + f32::from(p11[c]) * fx;
        out[c] = top * (1.0 - fy) + bot * fy;
    }
    out
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
                let texel = sample_bilinear(atlas, u, vv);
                for ch in 0..4 {
                    rgba[ch] = rgba[ch] * texel[ch] / 255.0;
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
    // Install the icon family into THIS context explicitly. Applying
    // the theme only installs when the requested weights differ from
    // the ones a previous call recorded, and those are process-global —
    // so a fresh context in a test binary can silently miss the install
    // and render every icon as `.notdef`.
    mara_backend_egui::theme::__internal_install_fonts(
        &ctx,
        mara_core::style::font_weight(),
        mara_core::style::title_weight(),
    );
    // Twice, deliberately. The first apply installs the fonts; the flag
    // that tells paint sites the icon family is actually bound only
    // flips on a later apply, and until it does every icon silently
    // falls back to the proportional family and renders as `.notdef`.
    for _ in 0..2 {
        mara_backend_egui::theme::__internal_apply_theme(
            &ctx,
            mara_core::style::AccentColor(accent),
            mara_core::style::GlassOpacity::default(),
        );
    }

    let mut buf = vec![[18_u8, 18, 20, 255]; W * H];
    let mut atlas = egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
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

        // egui streams the font atlas as deltas: the first carries the
        // whole image, later ones patch a sub-rect at `pos`. Treating a
        // patch as the entire atlas puts every glyph UV in the wrong
        // place, which is why text rendered as solid blobs and cost a
        // wrong diagnosis of the icon font.
        for (_, d) in &out.textures_delta.set {
            let egui::ImageData::Color(src) = &d.image;
            match d.pos {
                None => atlas = (**src).clone(),
                Some([ox, oy]) => {
                    let (aw, ah) = (atlas.size[0], atlas.size[1]);
                    for y in 0..src.size[1] {
                        for x in 0..src.size[0] {
                            let (dx, dy) = (ox + x, oy + y);
                            if dx < aw && dy < ah {
                                atlas.pixels[dy * aw + dx] = src.pixels[y * src.size[0] + x];
                            }
                        }
                    }
                }
            }
        }
        let atlas = &atlas;

        buf.iter_mut().for_each(|p| *p = [18, 18, 20, 255]);
        for prim in ctx.tessellate(out.shapes, out.pixels_per_point) {
            if let egui::epaint::Primitive::Mesh(m) = prim.primitive {
                draw_mesh(&mut buf, &m, atlas, prim.clip_rect);
            }
        }
    }

    let flat: Vec<u8> = buf.iter().flat_map(|p| p.iter().copied()).collect();
    image::save_buffer(path, &flat, W as u32, H as u32, image::ColorType::Rgba8)
        .expect("write png");
    eprintln!("wrote {path}");
}
