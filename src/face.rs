//! The face visualizer: a low-poly head floating in space that lives with the music.
//! It nods in time, its eyes glow with the kick and its edges flicker with the hats. In
//! a build-up it warps and glitches – the colour channels drift apart, it nods twice as
//! fast; on the drop it rushes at you, tears its mouth open and screams, bursts into
//! shards that fly out of the picture, and pulls itself back together.
//!
//! The head is made here: rings of a squared-off profile (skull, temples, cheekbones,
//! jaw, chin) with bumps for sockets, brows, nose, lips and chin, and a jaw that drops.
//! Everything is drawn from this frame's state plus the moment of the last drop, so
//! screenshots look like the screen.

use crate::orbit::Live;
use crate::song::{Song, Voice};
use crate::theme::*;
use crate::widgets::star_field;
use eframe::egui::{self, vec2, Color32, Mesh, Pos2, Rect, Ui};

/// The head's grid: columns across the face, rows from the crown to the chin
const COLS: usize = 22;
const ROWS: usize = 22;
/// Where the lips meet (before the jaw drops) and half the mouth's width
const MOUTH_Y: f32 = 0.41;
const MOUTH_W: f32 = 0.24;
/// Camera distance in head heights – the closer, the stronger the perspective
const CAMERA: f32 = 3.2;

fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

fn bump(x: f32, y: f32, cx: f32, cy: f32, sx: f32, sy: f32) -> f32 {
    let (dx, dy) = ((x - cx) / sx, (y - cy) / sy);
    (-(dx * dx + dy * dy)).exp()
}

/// Half the head's width and its depth at height `y` (-1 crown … 1 chin): a round
/// skull, flat temples, cheekbones, then the jaw narrowing to the chin
fn profile(y: f32) -> (f32, f32) {
    const P: [(f32, f32, f32); 12] = [
        (-1.0, 0.0, 0.0),
        (-0.96, 0.26, 0.3),
        (-0.88, 0.47, 0.52),
        (-0.72, 0.58, 0.66),
        (-0.5, 0.61, 0.72),
        (-0.25, 0.63, 0.75),
        (0.05, 0.66, 0.74),
        (0.3, 0.6, 0.7),
        (0.55, 0.51, 0.64),
        (0.78, 0.37, 0.56),
        (0.94, 0.2, 0.48),
        (1.0, 0.0, 0.4),
    ];
    let y = y.clamp(-1.0, 1.0);
    let i = P.iter().rposition(|p| p.0 <= y).unwrap_or(0).min(P.len() - 2);
    let (a, b) = (P[i], P[i + 1]);
    let t = (y - a.0) / (b.0 - a.0);
    (a.1 + (b.1 - a.1) * t, a.2 + (b.2 - a.2) * t)
}

/// A point of the head for grid coordinates `u` (-1 left … 1 right, round to the back)
/// and `v` (-1 crown … 1 chin): x right, y down, z towards you. `jaw` 0…1 opens the mouth.
fn head(u: f32, v: f32, jaw: f32) -> [f32; 3] {
    // Columns closer together at the front, where the face is
    let a = 1.75 * u * (0.55 + 0.45 * u.abs());
    let squared = |s: f32| s.signum() * s.abs().powf(0.85);
    let (w, d) = profile(v);
    let mut y = v;
    let x = w * squared(a.sin());
    let mut z = d * squared(a.cos());
    for side in [-1.0f32, 1.0] {
        z -= 0.13 * bump(x, y, 0.25 * side, -0.1, 0.12, 0.07); // eye sockets
        z += 0.08 * bump(x, y, 0.24 * side, -0.23, 0.16, 0.05); // brows
        z += 0.06 * bump(x, y, 0.42 * side, 0.02, 0.12, 0.08); // cheekbones
        z -= 0.05 * bump(x, y, 0.4 * side, 0.35, 0.12, 0.12); // hollow cheeks
        z += 0.07 * bump(x, y, 0.08 * side, 0.27, 0.05, 0.04); // nostrils
    }
    z += 0.03 * bump(x, y, 0.0, -0.2, 0.08, 0.05); // between the brows
    z += 0.3 * bump(x, y, 0.0, 0.07, 0.08, 0.21); // nose
    z += 0.1 * bump(x, y, 0.0, 0.24, 0.08, 0.05); // its tip
    z += 0.04 * bump(x, y, 0.0, 0.37, 0.15, 0.035); // upper lip
    z += 0.05 * bump(x, y, 0.0, 0.47, 0.13, 0.04); // lower lip
    z += 0.06 * bump(x, y, 0.0, 0.82, 0.15, 0.08); // chin
                                                   // The jaw drops: everything under the lips moves down and a little back, most in the middle
    if y > MOUTH_Y {
        let lens = (1.0 - (x / MOUTH_W).powi(2)).max(0.0).powf(1.4);
        let chin = (1.0 - x.abs() / 0.6).clamp(0.0, 1.0);
        let open = jaw * (0.4 * lens + 0.06 * chin);
        y += open;
        z -= open * 0.3;
    }
    [x, y, z]
}

/// The grid column (0…1, the right half) where the face is `x` wide at height `y`
fn column_at(x: f32, y: f32) -> f32 {
    let (mut lo, mut hi) = (0.0, 0.6);
    for _ in 0..30 {
        let mid = (lo + hi) / 2.0;
        if head(mid, y, 0.0)[0] < x.abs() {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo * x.signum()
}

fn rotate(p: [f32; 3], yaw: f32, pitch: f32) -> [f32; 3] {
    let [x, y, z] = p;
    let (x1, z1) = (x * yaw.cos() + z * yaw.sin(), -x * yaw.sin() + z * yaw.cos());
    let (y2, z2) = (y * pitch.cos() - z1 * pitch.sin(), y * pitch.sin() + z1 * pitch.cos());
    [x1, y2, z2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn norm(a: [f32; 3]) -> [f32; 3] {
    let l = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt().max(1e-6);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// A triangle of the head: three corners and an id for its randomness
type Tri = ([f32; 3], [f32; 3], [f32; 3], u32);

/// What the last drop is doing now
#[derive(Clone, Copy, Default)]
struct Moment {
    building: bool,
    drop_at: f64,
}

/// A colour that adds light (premultiplied with no alpha), `k` 0…1 of it
fn additive(color: Color32, k: f32) -> Color32 {
    let k = k.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * k) as u8,
        (color.g() as f32 * k) as u8,
        (color.b() as f32 * k) as u8,
        0,
    )
}

/// A beam of light from `from` along `dir`: brightest along its middle and at its start,
/// soft at the edges, `w0` wide at the start and `w1` at the end
#[allow(clippy::too_many_arguments)]
fn beam(p: &egui::Painter, from: Pos2, dir: egui::Vec2, len: f32, w0: f32, w1: f32, color: Color32, k: f32) {
    let n = vec2(-dir.y, dir.x);
    let to = from + dir * len;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(from, additive(color, k));
    mesh.colored_vertex(to, additive(color, k * 0.12));
    for q in [from + n * w0, to + n * w1, from - n * w0, to - n * w1] {
        mesh.colored_vertex(q, Color32::TRANSPARENT);
    }
    for [a, b, c] in [[0, 2, 3], [0, 3, 1], [0, 4, 5], [0, 5, 1]] {
        mesh.add_triangle(a, b, c);
    }
    p.add(egui::Shape::mesh(mesh));
}

/// A soft round light: bright in the middle, fading smoothly to nothing (added on top)
fn halo(p: &egui::Painter, c: Pos2, r: f32, color: Color32, k: f32) {
    let k = k.clamp(0.0, 1.0);
    let light = |f: f32| additive(color, k * f);
    let mut mesh = Mesh::default();
    mesh.colored_vertex(c, light(1.0));
    const N: u32 = 48;
    for (ring, f) in [(0.25, 0.55), (0.55, 0.18), (1.0, 0.0)] {
        for i in 0..N {
            let a = i as f32 / N as f32 * std::f32::consts::TAU;
            mesh.colored_vertex(c + vec2(a.cos(), a.sin()) * r * ring, light(f));
        }
    }
    for i in 0..N {
        let j = (i + 1) % N;
        mesh.add_triangle(0, 1 + i, 1 + j);
        for ring in 0..2 {
            let (a, b) = (1 + ring * N, 1 + (ring + 1) * N);
            mesh.add_triangle(a + i, a + j, b + i);
            mesh.add_triangle(a + j, b + j, b + i);
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

pub fn face(ui: &mut Ui, rect: Rect, song: &Song, live: &Live, now: f64) {
    let p = ui.painter_at(rect);
    let t = now as f32;
    let energy = song.macros.energy;
    let playing = live.playing;
    let kick = if playing { live.flash(0) } else { 0.0 };
    let hats = if playing { live.flash(3).max(live.flash(5)) } else { 0.0 };
    let build = live.build.unwrap_or(0.0);

    // The drop: the moment a build-up ends
    let id = egui::Id::new("face_moment");
    let mut m: Moment = ui.data(|d| d.get_temp(id)).unwrap_or_default();
    if m.building && live.build.is_none() {
        m.drop_at = now;
    }
    m.building = live.build.is_some();
    ui.data_mut(|d| d.insert_temp(id, m));
    let since = if m.drop_at > 0.0 { (now - m.drop_at) as f32 } else { 99.0 };

    // The drop, in phases (seconds after it): rush at you, tear the mouth open and scream
    // with lasers in the eyes; cracks glow through the face; the shards fly out at you,
    // spinning, and come back to make the head again
    const CRACK: f32 = 1.4;
    const FLY: f32 = 1.65;
    const BACK: f32 = 2.45;
    const DONE: f32 = 3.7;
    let rush = if since < BACK {
        ease(since / 0.4)
    } else {
        1.0 - ease((since - BACK) / (DONE - BACK))
    };
    // 0…1: how far out the shards are – faster and faster on the way out, slowing to land
    let fly = if (FLY..BACK).contains(&since) {
        ((since - FLY) / (BACK - FLY)).powi(2)
    } else if (BACK..DONE).contains(&since) {
        1.0 - ease((since - BACK) / (DONE - BACK))
    } else {
        0.0
    };
    let crack = if since < CRACK {
        0.0
    } else if since < BACK {
        ease((since - CRACK) / 0.25)
    } else {
        1.0 - ease((since - BACK) / (DONE - BACK))
    };
    // The hit: in fast, out slowly – nothing switches on or off at once
    let scream = ease(since / 0.15) * (1.0 - ease((since - 0.5) / 1.0));
    let laser = ease((since - 0.1) / 0.25) * (1.0 - ease((since - 1.2) / 0.35));
    // Distortion: grows slowly through the build-up, fades out after the drop
    let warp = build * build + 0.7 * scream;

    // How the head moves and feels
    // The mouth stays shut – it only tears open on the drop
    let jaw = if since < BACK {
        ease(since / 0.12)
    } else {
        1.0 - ease((since - BACK) / 0.5)
    };
    // It nods on every beat (on every 8th late in a build-up): down on the hit, back up
    let beat = live.pos / if build > 0.5 { 2.0 } else { 4.0 };
    let nod = if playing { (1.0 - beat.fract() as f32).powi(3) } else { 0.0 };
    let yaw = 0.3 * (t * 0.27).sin() + 0.08 * warp * (t * 7.0).sin();
    let pitch = 0.06 * (t * 0.21).sin() - 0.02 - 0.13 * nod;
    let jitter = 0.004 + 0.006 * hats + 0.05 * warp;
    let zoom = 1.0 + 0.035 * kick + 0.55 * rush;
    let size = rect.width().min(rect.height()) * 0.34 * zoom;
    let c = rect.center() + vec2(0.0, -rect.height() * 0.02 + size * 0.035 * nod);
    let cool = track_color(Voice::OpenHat);
    let hot = mix(RED, track_color(Voice::Clap), 0.4);
    // Redder through the build-up, and it stays red for a while after the drop
    let red = (build * 0.7).max(0.7 * (1.0 - ease((since - 0.3) / 2.5)));
    let tint = mix(crate::visual::mood(energy), RED, red);

    // Space
    p.rect_filled(rect, 0.0, SPACE);
    star_field(ui, &p, rect, "face", 0.8 + energy * 2.0 + build * 3.0, 0.8);
    halo(&p, c, size * 1.6, tint, 0.12 + kick * 0.12 + build * 0.2 + rush * 0.2);

    // Glitch: a few horizontal bands shifted sideways, new ones ten times a second
    let seed = (t * 10.0) as u32;
    let bands: Vec<(f32, f32, f32)> = (0..(warp * 7.0).round() as u32)
        .map(|k| {
            let y0 = rect.top() + hash(seed * 31 + k) * rect.height();
            let h = 6.0 + hash(seed * 31 + k + 7) * 40.0;
            let dx = (hash(seed * 31 + k + 13) - 0.5) * 110.0 * warp;
            (y0, y0 + h, dx)
        })
        .collect();
    let project = |q: [f32; 3]| -> (Pos2, f32) {
        let f = CAMERA / (CAMERA - q[2]).max(0.2);
        let mut at = c + vec2(q[0], q[1]) * size * f;
        for &(y0, y1, dx) in &bands {
            if at.y >= y0 && at.y < y1 {
                at.x += dx;
            }
        }
        (at, q[2])
    };

    // The surface, jittered into irregular low-poly triangles
    let frame = (t * 20.0) as u32;
    let mut grid = vec![[0.0f32; 3]; (COLS + 1) * (ROWS + 1)];
    let mut lower = vec![false; (COLS + 1) * (ROWS + 1)];
    for j in 0..=ROWS {
        for i in 0..=COLS {
            let k = (j * (COLS + 1) + i) as u32;
            let edge = i == 0 || i == COLS || j == 0 || j == ROWS;
            let (du, dv) = if edge {
                (0.0, 0.0)
            } else {
                ((hash(k) - 0.5) * 0.5 / COLS as f32, (hash(k + 999) - 0.5) * 0.5 / ROWS as f32)
            };
            let u = -1.0 + 2.0 * i as f32 / COLS as f32 + du;
            let v = -1.0 + 2.0 * j as f32 / ROWS as f32 + dv;
            lower[k as usize] = head(u, v, 0.0)[1] > MOUTH_Y;
            let mut q = head(u, v, jaw);
            for (a, n) in q.iter_mut().enumerate() {
                *n += (hash(k * 3 + a as u32 + frame * 7919) - 0.5) * jitter;
            }
            grid[k as usize] = q;
        }
    }
    // Triangles back to front, facing you only; the ones across the open mouth stay away
    let light = norm([-0.2, -0.45, 1.0]);
    let mut tris: Vec<Tri> = Vec::new();
    for j in 0..ROWS {
        for i in 0..COLS {
            let a = j * (COLS + 1) + i;
            let (b, cc, d) = (a + 1, a + COLS + 1, a + COLS + 2);
            for (n, [x, y, z]) in [[a, b, d], [a, d, cc]].into_iter().enumerate() {
                let mixed = lower[x] != lower[y] || lower[y] != lower[z];
                let mid_x = (grid[x][0] + grid[y][0] + grid[z][0]) / 3.0;
                if mixed && mid_x.abs() < MOUTH_W && jaw > 0.12 {
                    continue;
                }
                tris.push((grid[x], grid[y], grid[z], (a * 2 + n) as u32));
            }
        }
    }
    let shards: Vec<_> = tris
        .iter()
        .map(|&(a, b, d, id)| {
            let mid = [(a[0] + b[0] + d[0]) / 3.0, (a[1] + b[1] + d[1]) / 3.0, (a[2] + b[2] + d[2]) / 3.0];
            // Facing you as part of the head? (a spinning shard still shows both sides)
            let (ra, rb, rd) = (rotate(a, yaw, pitch), rotate(b, yaw, pitch), rotate(d, yaw, pitch));
            let facing = cross(sub(rb, ra), sub(rd, ra))[2] > 0.0;
            // Cracks: every facet pulls back a little, leaving gaps that glow; then it flies
            // out and towards you, spinning its own way
            let shrink = 1.0 - 0.14 * crack - 0.4 * fly;
            let dir = norm([mid[0], mid[1] - 0.1, 0.5]);
            let k = fly * (1.5 + hash(id + 5) * 2.5);
            let off = [dir[0] * k, dir[1] * k, dir[2] * k * 0.6];
            let (s1, s2) = ((hash(id + 11) - 0.5) * 7.0 * fly, (hash(id + 17) - 0.5) * 7.0 * fly);
            let mv = |q: [f32; 3]| {
                let r = rotate(
                    [(q[0] - mid[0]) * shrink, (q[1] - mid[1]) * shrink, (q[2] - mid[2]) * shrink],
                    s1,
                    s2,
                );
                rotate([mid[0] + r[0] + off[0], mid[1] + r[1] + off[1], mid[2] + r[2] + off[2]], yaw, pitch)
            };
            (mv(a), mv(b), mv(d), id, facing)
        })
        .collect();
    let mut order: Vec<usize> = (0..shards.len()).collect();
    let depth = |i: usize| shards[i].0[2] + shards[i].1[2] + shards[i].2[2];
    order.sort_by(|&x, &y| depth(x).total_cmp(&depth(y)));

    // Inside the mouth: dark, with teeth on both lips and a fire deep in the throat.
    // Drawn first, it shows through the hole the open jaw leaves.
    let lips: Vec<([f32; 3], [f32; 3])> = (0..=COLS)
        .filter_map(|i| {
            let j = (0..ROWS).find(|&j| lower[(j + 1) * (COLS + 1) + i])?;
            let (up, down) = (grid[j * (COLS + 1) + i], grid[(j + 1) * (COLS + 1) + i]);
            (up[0].abs() < MOUTH_W && up[2] > 0.0).then_some((rotate(up, yaw, pitch), rotate(down, yaw, pitch)))
        })
        .collect();
    if jaw > 0.1 && fly < 0.02 {
        let hot = mix(RED, track_color(Voice::Clap), 0.4);
        let mut mesh = Mesh::default();
        for w in lips.windows(2) {
            let base = mesh.vertices.len() as u32;
            for q in [w[0].0, w[1].0, w[1].1, w[0].1] {
                mesh.colored_vertex(project(q).0, mix(SPACE, hot, 0.12));
            }
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base, base + 2, base + 3);
        }
        p.add(egui::Shape::mesh(mesh));
        let (throat, _) = project(rotate([0.0, MOUTH_Y + jaw * 0.2, 0.45], yaw, pitch));
        halo(&p, throat, size * 0.35 * jaw, hot, 0.5 + 0.5 * jaw + 0.3 * kick);
        // Low-poly teeth: a jagged row hanging from each lip, longer the wider it opens
        let fang = size * 0.07 * jaw.min(0.7);
        let bone = mix(TEXT, tint, 0.25);
        for (n, w) in lips.windows(2).enumerate() {
            for (lip, dir) in [(0, 1.0f32), (1, -1.0)] {
                let (a, b) = if lip == 0 { (w[0].0, w[1].0) } else { (w[0].1, w[1].1) };
                let (pa, pb) = (project(a).0, project(b).0);
                let long = if n % 2 == 0 { 1.0 } else { 0.6 };
                let tip = pa + (pb - pa) * 0.5 + vec2(0.0, dir * fang * long);
                p.add(egui::Shape::convex_polygon(
                    vec![pa, pb, tip],
                    mix(SPACE, bone, 0.8),
                    stroke(0.0, bone),
                ));
            }
        }
    }

    // Shards fade as they fly; the colour channels drift apart with the distortion, but
    // not while it breaks, so the shatter stays clean
    let fade = 1.0 - fly;
    let split = (9.0 * build * build + 8.0 * scream) * (1.0 - crack);
    let line = mix(tint, Color32::WHITE, 0.35);
    let mut mesh = Mesh::default();
    let mut edges: Vec<(Pos2, Pos2, f32, Color32)> = Vec::new();
    for &i in &order {
        let (a, b, d, id, facing) = shards[i];
        if !facing || a[2].max(b[2]).max(d[2]) > CAMERA - 0.6 {
            continue;
        }
        let n = norm(cross(sub(b, a), sub(d, a)));
        let shade = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).abs();
        let (pa, _) = project(a);
        let (pb, _) = project(b);
        let (pd, _) = project(d);
        // Flat-shaded facets: the light picks out every plane
        let fill = mix(SPACE, mix(tint, Color32::WHITE, 0.2), 0.05 + 0.55 * shade.powf(1.6) + 0.25 * fly).gamma_multiply(fade.sqrt());
        let base = mesh.vertices.len() as u32;
        for q in [pa, pb, pd] {
            mesh.colored_vertex(q, fill);
        }
        mesh.add_triangle(base, base + 1, base + 2);
        // Some edges flash with the hats; cracks burn hot
        let flash = if hash(id * 13 + (t * 12.0) as u32) < 0.08 + hats * 0.25 {
            1.0
        } else {
            0.0
        };
        let k = 0.06 + 0.16 * shade + 0.5 * flash + 0.12 * kick;
        // (outlines go first, so flying shards are glowing facets, not wire)
        let col = alpha(mix(line, hot, crack), ((k + 0.8 * crack) * fade.powi(4)).min(1.0));
        edges.extend([(pa, pb, k, col), (pb, pd, k, col), (pd, pa, k, col)]);
    }
    p.add(egui::Shape::mesh(mesh));
    if split > 0.5 {
        let o = vec2(split, 0.0);
        let s = (split / 4.0).min(1.0) * 0.35;
        for &(a, b, k, _) in &edges {
            p.line_segment([a - o, b - o], stroke(1.0, alpha(RED, k * s)));
            p.line_segment([a + o, b + o], stroke(1.0, alpha(cool, k * s)));
        }
    }
    for &(a, b, _, col) in &edges {
        p.line_segment([a, b], stroke(1.0 + crack, col));
    }

    // The shut mouth: a dark line where the lips meet
    if jaw <= 0.1 && fly < 0.02 && lips.len() > 1 {
        let seam: Vec<Pos2> = lips
            .iter()
            .map(|&(up, down)| project([(up[0] + down[0]) / 2.0, (up[1] + down[1]) / 2.0, (up[2] + down[2]) / 2.0]).0)
            .collect();
        p.add(egui::Shape::line(seam, stroke(2.5, mix(SPACE, tint, 0.1))));
    }

    // Eyes: they glow with the beat, burn in the build-up and blaze on the drop
    let eye_glow = (0.35 + 0.7 * kick + 1.4 * build + 2.0 * scream.max(laser)) * (0.3 + 0.7 * fade);
    let eye_col = mix(
        mix(tint, Color32::WHITE, 0.5),
        mix(RED, Color32::WHITE, 0.3),
        (build * 1.6 - 0.6).max(laser).clamp(0.0, 1.0),
    );
    // A blink every few seconds: the lids meet for a moment
    let phase = (t / 4.3).fract();
    let blink = if phase < 0.04 {
        ((phase - 0.02).abs() / 0.02).min(1.0)
    } else {
        1.0
    };
    // Lasers: charging late in the build-up, full blast on the drop, sweeping slowly
    let blast = laser.max(((build - 0.75) / 0.25).clamp(0.0, 1.0) * 0.3) * (0.85 + 0.15 * kick);
    let reach = rect.width().hypot(rect.height());
    for side in [-1.0f32, 1.0] {
        // A point on the face around the eye, lifted a hair off the skin
        let on_face = |dx: f32, dy: f32| {
            let (x, y) = (0.25 * side + dx * side, -0.1 + dy);
            let [x, y, z] = head(column_at(x, y), y, 0.0);
            project(rotate([x, y, z + 0.015], yaw, pitch)).0
        };
        let eye = on_face(0.0, 0.0);
        halo(&p, eye, size * (0.09 + 0.05 * eye_glow.min(3.0)), eye_col, 0.2 * eye_glow);
        // The eye: a low-poly almond (inner corner towards the nose), lids that blink now
        // and then – but never while it screams – and a glowing iris
        let open = if since < BACK { 1.0 } else { blink };
        let almond: Vec<Pos2> = [
            (-0.1, 0.0),
            (-0.04, -0.045),
            (0.05, -0.042),
            (0.1, 0.004),
            (0.04, 0.034),
            (-0.05, 0.03),
        ]
        .iter()
        .map(|&(dx, dy)| on_face(dx * 1.25, dy * 1.3 * open))
        .collect();
        let lit = (eye_glow / 2.5).min(1.0);
        p.add(egui::Shape::convex_polygon(
            almond.clone(),
            mix(mix(SPACE, eye_col, 0.12), eye_col, lit * lit),
            stroke(1.2, alpha(eye_col, 0.5 + 0.5 * lit)),
        ));
        if open > 0.3 {
            let r = size * 0.038 * open.min(1.0);
            p.circle_filled(eye, r, mix(eye_col, Color32::WHITE, 0.3 * lit));
            p.circle_filled(eye, r * 0.45, mix(SPACE, eye_col, lit));
            p.circle_filled(eye + vec2(-0.35, -0.35) * r, r * 0.22, alpha(Color32::WHITE, 0.8));
        }
        if blast > 0.01 {
            let a = side * (0.55 + 0.2 * (t * 1.3 + side).sin());
            let dir = vec2(a.sin(), a.cos());
            let w = size * 0.035;
            beam(&p, eye, dir, reach, w * 5.0, size * 0.9, RED, 0.35 * blast);
            beam(&p, eye, dir, reach, w * 2.2, size * 0.4, mix(RED, hot, 0.5), 0.7 * blast);
            beam(&p, eye, dir, reach, w * 0.8, size * 0.1, mix(eye_col, Color32::WHITE, 0.6), blast);
            halo(&p, eye, size * 0.5 * blast, eye_col, blast);
        }
    }

    crate::visual::energy_bar(&p, rect, energy, tint);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_head_has_a_face_and_a_jaw() {
        let at = |x: f32, y: f32| head(column_at(x, y), y, 0.0);
        // The nose sticks out further than the cheeks, the eyes sit deeper than the brows
        assert!(at(0.0, 0.1)[2] > at(0.42, 0.1)[2] + 0.1);
        assert!(at(0.25, -0.1)[2] < at(0.25, -0.24)[2]);
        // Not a balloon: wider at the cheekbones than at the jaw, and a narrow chin
        assert!(profile(0.05).0 > profile(0.6).0 + 0.1 && profile(0.94).0 < 0.25);
        // The chin drops when the mouth opens, the forehead doesn't move
        let chin = |jaw| head(0.0, 0.9, jaw);
        assert!(chin(1.0)[1] > chin(0.0)[1] + 0.3);
        assert_eq!(head(0.0, -0.8, 1.0), head(0.0, -0.8, 0.0));
    }
}
