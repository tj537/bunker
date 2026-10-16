//! Draws egui user interfaces into an image without a window.
//!
//! Per frame egui delivers triangles with colours and texture coordinates. A
//! small software rasterizer paints them into an image – the screenshots of
//! the tests (`snapshot.rs`). There is no way to drive the real window from a
//! terminal, so this is how the UI gets checked.

use eframe::egui::{
    self,
    epaint::{ClippedPrimitive, ImageData, Primitive, TextureId},
    Color32, Pos2, Rect, Vec2,
};
use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;

struct Texture {
    width: usize,
    height: usize,
    pixels: Vec<Color32>,
}

pub struct Offscreen {
    pub ctx: egui::Context,
    textures: HashMap<TextureId, Texture>,
    pub size: Vec2,
    /// Pixels per point (2 = Retina)
    scale: f32,
}

impl Offscreen {
    #[allow(dead_code)]
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            ctx: egui::Context::default(),
            textures: HashMap::new(),
            size: Vec2::new(width, height),
            scale: 1.0,
        }
    }

    /// Like `new`, but rasterized with `scale` pixels per point (sharper)
    pub fn retina(width: f32, height: f32, scale: f32) -> Self {
        Self {
            scale,
            ..Self::new(width, height)
        }
    }

    /// Run one frame and return the finished triangles
    pub fn frame(&mut self, time: f64, run_ui: impl FnMut(&egui::Context)) -> Vec<ClippedPrimitive> {
        self.frame_with_events(time, Vec::new(), run_ui)
    }

    /// Like `frame`, but with input (clicks, keys) – for tests of the controls
    pub fn frame_with_events(&mut self, time: f64, events: Vec<egui::Event>, run_ui: impl FnMut(&egui::Context)) -> Vec<ClippedPrimitive> {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
            time: Some(time),
            events,
            ..Default::default()
        };
        input.viewports.entry(egui::ViewportId::ROOT).or_default().native_pixels_per_point = Some(self.scale);
        let output = self.ctx.run(input, run_ui);
        for (id, delta) in output.textures_delta.set {
            let (width, height, pixels) = match &delta.image {
                ImageData::Color(image) => (image.size[0], image.size[1], image.pixels.clone()),
                ImageData::Font(font) => (font.size[0], font.size[1], font.srgba_pixels(None).collect()),
            };
            match delta.pos {
                None => {
                    self.textures.insert(id, Texture { width, height, pixels });
                }
                Some([x0, y0]) => {
                    if let Some(texture) = self.textures.get_mut(&id) {
                        for y in 0..height {
                            for x in 0..width {
                                texture.pixels[(y0 + y) * texture.width + x0 + x] = pixels[y * width + x];
                            }
                        }
                    }
                }
            }
        }
        self.ctx.tessellate(output.shapes, output.pixels_per_point)
    }

    /// Rasterize triangles; returns width, height and RGBA pixels (premultiplied)
    pub fn rgba(&self, primitives: &[ClippedPrimitive]) -> (u32, u32, Vec<u8>) {
        let scale = self.scale;
        let (w, h) = ((self.size.x * scale) as usize, (self.size.y * scale) as usize);
        // Premultiplied colours 0…1, as egui delivers them
        let mut buffer = vec![[0.0_f32; 4]; w * h];
        let rgba = |c: Color32| c.to_array().map(|v| v as f32 / 255.0);
        let edge = |a: Pos2, b: Pos2, p: Pos2| (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x);

        for primitive in primitives {
            let Primitive::Mesh(mesh) = &primitive.primitive else {
                continue;
            };
            let clip = Rect::from_min_max(primitive.clip_rect.min * scale, primitive.clip_rect.max * scale)
                .intersect(Rect::from_min_size(Pos2::ZERO, self.size * scale));
            let texture = self.textures.get(&mesh.texture_id);
            for tri in mesh.indices.as_chunks::<3>().0 {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| {
                    let mut v = mesh.vertices[i as usize];
                    v.pos = (v.pos.to_vec2() * scale).to_pos2();
                    v
                });
                let area = edge(a.pos, b.pos, c.pos);
                if area.abs() < 1e-6 {
                    continue;
                }
                let min = a.pos.min(b.pos).min(c.pos).max(clip.min);
                let max = a.pos.max(b.pos).max(c.pos).min(clip.max);
                if min.x >= max.x || min.y >= max.y {
                    continue;
                }
                let (ca, cb, cc) = (rgba(a.color), rgba(b.color), rgba(c.color));
                for py in min.y.floor() as usize..(max.y.ceil() as usize).min(h) {
                    for px in min.x.floor() as usize..(max.x.ceil() as usize).min(w) {
                        let p = Pos2::new(px as f32 + 0.5, py as f32 + 0.5);
                        let w0 = edge(b.pos, c.pos, p) / area;
                        let w1 = edge(c.pos, a.pos, p) / area;
                        let w2 = edge(a.pos, b.pos, p) / area;
                        if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                            continue;
                        }
                        let uv = Pos2::new(a.uv.x * w0 + b.uv.x * w1 + c.uv.x * w2, a.uv.y * w0 + b.uv.y * w1 + c.uv.y * w2);
                        let texel = texture
                            .map(|t| {
                                let x = ((uv.x * t.width as f32) as usize).min(t.width - 1);
                                let y = ((uv.y * t.height as f32) as usize).min(t.height - 1);
                                rgba(t.pixels[y * t.width + x])
                            })
                            .unwrap_or([1.0; 4]);
                        let mut src = [0.0; 4];
                        for k in 0..4 {
                            src[k] = (ca[k] * w0 + cb[k] * w1 + cc[k] * w2) * texel[k];
                        }
                        let dst = &mut buffer[py * w + px];
                        for k in 0..4 {
                            dst[k] = src[k] + dst[k] * (1.0 - src[3]);
                        }
                    }
                }
            }
        }

        let pixels = buffer
            .iter()
            .flat_map(|px| px.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))
            .collect();
        (w as u32, h as u32, pixels)
    }

    /// Rasterize triangles and save them as PNG (for tests)
    #[cfg(test)]
    pub fn save(&self, primitives: &[ClippedPrimitive], path: &Path) {
        let (w, h, mut pixels) = self.rgba(primitives);
        // Un-premultiply for PNG
        for px in pixels.chunks_exact_mut(4) {
            let a = px[3] as f32 / 255.0;
            if a > 0.0 && a < 1.0 {
                for c in &mut px[..3] {
                    *c = ((*c as f32 / a).min(255.0)) as u8;
                }
            }
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).unwrap();
        }
        let file = std::fs::File::create(path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&pixels).unwrap();
    }
}
