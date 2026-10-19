//! The visualizer: the whole window becomes a light show that follows the
//! music – a radial spectrum, shockwaves from the kick, a star field that
//! speeds up with the energy, one moon per track that flashes when it plays.
//!
//! Everything is computed from the playback state of this frame (no particles
//! to keep), so it looks the same in screenshots as on screen.

use crate::orbit::Live;
use crate::song::{Song, Voice, STEPS};
use crate::theme::*;
use crate::widgets::{glow, star_field};
use eframe::egui::{pos2, vec2, Color32, Rect, Ui};
use std::f32::consts::{PI, TAU};

/// A stable pseudo-random number 0…1
fn hash(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

/// The colour of the moment: cool when calm, red hot at full energy
pub fn mood(energy: f32) -> Color32 {
    let cool = track_color(Voice::OpenHat);
    let mid = track_color(Voice::Bass);
    if energy < 0.5 {
        mix(cool, mid, energy * 2.0)
    } else {
        mix(mid, RED, (energy - 0.5) * 2.0)
    }
}

pub fn visual(ui: &mut Ui, rect: Rect, song: &Song, live: &Live, t: f64) {
    let p = ui.painter_at(rect);
    let c = rect.center();
    let big = rect.width().min(rect.height()) * 0.5;
    let energy = song.macros.energy;
    let kick = if live.playing { live.flash(0) } else { 0.0 };
    let tint = mix(mood(energy), Color32::WHITE, live.build.unwrap_or(0.0) * 0.3);
    let tint = if live.build.is_some() { mix(tint, RED, 0.6) } else { tint };
    let t = t as f32;

    p.rect_filled(rect, 0.0, SPACE);
    // Far behind: stars drifting past in layers
    star_field(ui, &p, rect, "visual", 0.8 + energy * 2.0, 0.8);
    // Breathing floor
    glow(&p, c, big * 0.45, tint, 0.06 + kick * 0.22 + live.level * 0.06);

    // Star field: faster with energy
    let speed = 0.06 + energy * 0.35 + if live.playing { 0.05 } else { 0.0 };
    for i in 0..220u32 {
        let a = hash(i) * TAU;
        let ph = (t * speed * (0.4 + hash(i + 999) * 0.9) + hash(i + 77)).fract();
        let r = ph * ph * big * 1.6;
        let at = c + vec2(a.cos(), a.sin()) * r;
        if !rect.contains(at) {
            continue;
        }
        let col = if i % 5 == 0 {
            track_color(Voice::ALL[i as usize % 8])
        } else {
            TEXT
        };
        p.circle_filled(at, 0.6 + ph * 2.2, alpha(col, ph * (0.25 + live.level * 0.6)));
    }

    // Shockwaves from the last kicks
    if live.playing && !song.track(Voice::Kick).mute {
        let beat = 60.0 / song.bpm;
        for k in 0..4 {
            let age = live.hit_age[0] + k as f32 * beat;
            let life = (age / (beat * 3.5)).min(1.0);
            if life >= 1.0 {
                continue;
            }
            let r = big * (0.3 + life * 1.1);
            p.circle_stroke(c, r, stroke(1.0 + 5.0 * (1.0 - life), alpha(tint, 0.5 * (1.0 - life).powi(2))));
        }
    }

    // Radial spectrum, mirrored, turning slowly with the beat
    let n = 120;
    let spin = (live.pos / STEPS as f64) as f32 * TAU * 0.125;
    let r0 = big * 0.3 * (1.0 + kick * 0.06);
    for j in 0..n {
        let x = j as f32 / n as f32;
        let m = 1.0 - (x * 2.0 - 1.0).abs(); // 0 … 1 … 0 around the circle
        let pos = m * (live.bands.len() - 1) as f32;
        let (i0, f) = (pos.floor() as usize, pos.fract());
        let i1 = (i0 + 1).min(live.bands.len() - 1);
        let v = live.bands[i0] * (1.0 - f) + live.bands[i1] * f;
        let v = (v * 5.0).sqrt().min(1.2);
        let a = -PI / 2.0 + x * TAU + spin;
        let dir = vec2(a.cos(), a.sin());
        let len = 3.0 + v * big * 0.42;
        let col = mix(track_color(Voice::ALL[(m * 7.0) as usize]), Color32::WHITE, v * 0.3);
        p.line_segment([c + dir * r0, c + dir * (r0 + len)], stroke(2.4, alpha(col, 0.35 + v * 0.6)));
    }

    // One moon per track, flashing on its hits
    for (i, v) in Voice::ALL.into_iter().enumerate() {
        let tr = song.track(v);
        if tr.mute {
            continue;
        }
        let flash = live.flash(i);
        let a = -PI / 2.0 + (live.pos as f32 / STEPS as f32) * TAU * (0.25 + i as f32 * 0.07) + i as f32 * 0.8;
        let r = big * (0.82 + i as f32 * 0.035);
        let at = c + vec2(a.cos(), a.sin()) * r;
        let col = track_color(v);
        glow(&p, at, 6.0, col, 0.3 + flash * 1.5);
        p.circle_filled(at, 3.5 + flash * 4.0, mix(col, Color32::WHITE, flash * 0.5));
    }

    // Centre: a quiet ring that beats with the kick
    let rb = r0 * 0.82;
    p.circle_filled(c, rb, alpha(SPACE, 0.85));
    p.circle_stroke(c, rb, stroke(2.0, alpha(tint, 0.6 + kick * 0.4)));

    // Build-up: the whole screen gets hot
    if let Some(b) = live.build {
        p.rect_filled(rect, 0.0, alpha(RED, b * b * 0.18));
    }

    // No text – just the energy, small at the bottom: where you are (← → move it)
    energy_bar(&p, rect, energy, tint);
}

/// The energy as a small bar at the bottom of a visualizer, with its break and drop zones
pub fn energy_bar(p: &eframe::egui::Painter, rect: Rect, energy: f32, tint: Color32) {
    let c = rect.center();
    let w = (rect.width() * 0.22).clamp(160.0, 320.0);
    let bar = Rect::from_center_size(pos2(c.x, rect.bottom() - 28.0), vec2(w, 4.0));
    let at = |e: f32| bar.left() + bar.width() * e;
    p.rect_filled(bar, 2.0, alpha(TEXT, 0.08));
    p.rect_filled(
        Rect::from_min_max(bar.min, pos2(at(crate::engine::BREAK_ZONE), bar.bottom())),
        2.0,
        alpha(track_color(Voice::OpenHat), 0.18),
    );
    p.rect_filled(
        Rect::from_min_max(pos2(at(crate::engine::DROP_ZONE), bar.top()), bar.max),
        2.0,
        alpha(RED, 0.25),
    );
    p.rect_filled(Rect::from_min_max(bar.min, pos2(at(energy), bar.bottom())), 2.0, alpha(tint, 0.55));
    p.circle_filled(pos2(at(energy), bar.center().y), 3.5, alpha(TEXT, 0.7));
}
