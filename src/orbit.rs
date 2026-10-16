//! The orbit: every track is a ring, every step a dot on it, a beam of light
//! circles like a radar. Shorter tracks have fewer dots and drift against the
//! others – polyrhythms you can see.

use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::{arc, glow};
use eframe::egui::{self, epaint::Vertex, pos2, vec2, Align2, Color32, Mesh, Pos2, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::f32::consts::{PI, TAU};

/// What the UI knows about playback this frame
#[derive(Clone, Default)]
pub struct Live {
    pub playing: bool,
    /// Global step position with fraction (for the smooth beam)
    pub pos: f64,
    /// Seconds since each track last played
    pub hit_age: [f32; TRACKS],
    pub build: Option<f32>,
    pub bar: u64,
    pub caption: Option<(String, f32)>,
}

impl Live {
    pub fn step(&self) -> u64 {
        self.pos.floor().max(0.0) as u64
    }

    pub fn flash(&self, track: usize) -> f32 {
        (-self.hit_age[track] * 7.0).exp()
    }
}

/// Mouse painting: dragging over dots sets them all to the same state
#[derive(Clone, Copy, Default)]
pub struct Paint {
    pub track: usize,
    pub on: bool,
    pub active: bool,
}

pub struct Geometry {
    pub c: Pos2,
    pub r_in: f32,
    pub r_out: f32,
}

impl Geometry {
    pub fn new(rect: Rect) -> Self {
        let r_out = rect.width().min(rect.height()) / 2.0 - 34.0;
        Geometry {
            c: rect.center(),
            r_in: r_out * 0.32,
            r_out,
        }
    }

    pub fn spacing(&self) -> f32 {
        (self.r_out - self.r_in) / TRACKS as f32
    }

    pub fn ring(&self, track: usize) -> f32 {
        self.r_in + (track as f32 + 0.5) * self.spacing()
    }

    pub fn angle(step: usize, len: usize) -> f32 {
        -PI / 2.0 + TAU * step as f32 / len.max(1) as f32
    }

    pub fn dot(&self, track: usize, step: usize, len: usize) -> Pos2 {
        let a = Self::angle(step, len);
        self.c + vec2(a.cos(), a.sin()) * self.ring(track)
    }

    pub fn dot_radius(&self, track: usize, len: usize) -> f32 {
        let along = TAU * self.ring(track) / len.max(1) as f32;
        (self.spacing() * 0.3).min(along * 0.3).max(3.0)
    }

    /// The dot under `p`, if any
    pub fn hit(&self, p: Pos2, song: &Song) -> Option<(usize, usize)> {
        let d = p - self.c;
        let dist = d.length();
        if dist < self.r_in || dist > self.r_out {
            return None;
        }
        let track = (((dist - self.r_in) / self.spacing()) as usize).min(TRACKS - 1);
        let len = song.tracks[track].len as usize;
        let a = (d.y.atan2(d.x) + PI / 2.0).rem_euclid(TAU);
        let step = ((a / TAU * len as f32).round() as usize) % len;
        let dot = self.dot(track, step, len);
        (dot.distance(p) < self.dot_radius(track, len) * 2.0).then_some((track, step))
    }
}

pub struct OrbitOut {
    pub toggle_play: bool,
}

pub fn orbit(ui: &mut Ui, rect: Rect, song: &mut Song, live: &Live, paint: &mut Paint) -> OrbitOut {
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let g = Geometry::new(rect);
    let p = ui.painter_at(rect);
    let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
    let hovered = pointer.and_then(|pt| g.hit(pt, song));
    let on_center = pointer.is_some_and(|pt| pt.distance(g.c) < g.r_in * 0.66);
    let mut out = OrbitOut { toggle_play: false };

    // ── Input ──
    let (pressed, down, released, shift) = ui.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.modifiers.shift,
        )
    });
    if pressed && resp.hovered() {
        if let Some((t, s)) = hovered {
            let step = &mut song.tracks[t].steps[s];
            if shift {
                // Shift-click: how likely the step plays (100 → 75 → 50 → 25 → 100)
                step.on = true;
                step.chance = match step.chance {
                    100 => 75,
                    75 => 50,
                    50 => 25,
                    _ => 100,
                };
            } else {
                *paint = Paint {
                    track: t,
                    on: !step.on,
                    active: true,
                };
                step.on = paint.on;
            }
        }
    }
    if down && paint.active {
        if let Some((t, s)) = hovered {
            if t == paint.track {
                song.tracks[t].steps[s].on = paint.on;
            }
        }
    }
    if released {
        paint.active = false;
    }
    if resp.clicked() && on_center {
        out.toggle_play = true;
    }
    if resp.secondary_clicked() {
        if let Some((t, s)) = hovered {
            let step = &mut song.tracks[t].steps[s];
            step.accent = !step.accent;
            step.on = true;
            if Voice::ALL[t] == Voice::Acid && !step.accent {
                // Second right-click on an acid step: toggle the slide instead
                step.slide = !step.slide;
            }
        }
    }
    if let Some((t, s)) = hovered {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        if Voice::ALL[t].is_synth() && scroll.abs() > 0.5 {
            let scale = song.scale;
            let step = &mut song.tracks[t].steps[s];
            step.on = true;
            step.note = scale.shift(step.note, if scroll > 0.0 { 1 } else { -1 });
        }
    }
    if on_center || hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Drawing ──
    let kick = live.flash(0);
    // The floor breathes with the kick
    p.circle_filled(g.c, g.r_out + 12.0, alpha(BG, 0.55));
    p.circle_stroke(g.c, g.r_out + 12.0, stroke(1.0, LINE));
    let kc = track_color(Voice::Kick);
    for i in 0..8 {
        let k = i as f32 / 8.0;
        p.circle_filled(
            g.c,
            g.r_in + (g.r_out - g.r_in) * (1.0 - k),
            alpha(kc, (0.006 + kick * 0.009) * (0.4 + k)),
        );
    }

    // Beat numbers outside the rings
    for (i, label) in ["1", "2", "3", "4"].iter().enumerate() {
        let a = Geometry::angle(i * 4, STEPS);
        let at = g.c + vec2(a.cos(), a.sin()) * (g.r_out + 24.0);
        let current = live.playing && (live.step() % 16) / 4 == i as u64;
        p.text(at, Align2::CENTER_CENTER, *label, display(15.0), if current { TEXT } else { FAINT });
    }

    // The radar beam
    if live.playing {
        let a = -PI / 2.0 + TAU * (live.pos.rem_euclid(STEPS as f64) as f32) / STEPS as f32;
        beam(&p, g.c, g.r_in, g.r_out + 10.0, a, live.build.unwrap_or(0.0));
    }

    for (t, voice) in Voice::ALL.into_iter().enumerate() {
        let track = &song.tracks[t];
        let len = track.len as usize;
        let color = track_color(voice);
        let r = g.ring(t);
        let flash = live.flash(t);
        let muted = track.mute;
        let base = if muted { 0.07 } else { 0.16 + flash * 0.4 };
        p.circle_stroke(g.c, r, stroke(1.0 + flash * 1.5, alpha(color, base)));
        let current = live.playing.then(|| track.step_at(live.step()));
        let dr = g.dot_radius(t, len);

        for s in 0..len {
            let step = track.steps[s];
            let at = g.dot(t, s, len);
            let is_hover = hovered == Some((t, s));
            let is_now = current == Some(s);
            if step.on {
                let strength = if muted { 0.25 } else { 0.45 + 0.55 * step.chance as f32 / 100.0 };
                let rad = if step.accent { dr * 1.2 } else { dr };
                if is_now && !muted {
                    glow(&p, at, rad, color, flash * 1.6);
                }
                // Slides: a short line along the ring to the previous dot
                if step.slide && s > 0 {
                    let a0 = Geometry::angle(s - 1, len);
                    let a1 = Geometry::angle(s, len);
                    arc(&p, g.c, r, a0, a1, stroke(2.0, alpha(color, strength * 0.7)));
                }
                p.circle_filled(at, rad * (1.0 + if is_now { flash * 0.25 } else { 0.0 }), alpha(color, strength));
                if step.accent {
                    p.circle_filled(at, rad * 0.38, alpha(Color32::WHITE, strength));
                }
                if step.chance < 100 {
                    p.circle_stroke(at, rad + 2.5, stroke(1.0, alpha(color, 0.5)));
                }
            } else {
                p.circle_stroke(at, dr * 0.5, stroke(1.0, alpha(color, if muted { 0.1 } else { 0.28 })));
            }
            if is_now {
                p.circle_stroke(at, dr * 1.55, stroke(1.5, alpha(Color32::WHITE, 0.55)));
            }
            if is_hover {
                p.circle_stroke(at, dr * 1.55, stroke(2.0, TEXT));
            }
        }
    }

    // ── Centre: the play button ──
    let rb = g.r_in * 0.62;
    let hot = on_center;
    let pulse = if live.playing { kick } else { 0.0 };
    glow(
        &p,
        g.c,
        rb * 0.8,
        if live.playing { RED } else { ACID },
        0.18 + pulse * 0.35 + if hot { 0.12 } else { 0.0 },
    );
    p.circle_filled(g.c, rb * (1.0 + pulse * 0.04), if hot { mix(PANEL_HI, BG, 0.1) } else { PANEL });
    p.circle_stroke(g.c, rb, stroke(2.0, if live.playing { alpha(RED, 0.8) } else { alpha(ACID, 0.8) }));
    let icon = if live.playing { ph::STOP } else { ph::PLAY };
    p.text(
        g.c - vec2(0.0, rb * 0.12),
        Align2::CENTER_CENTER,
        icon,
        body(rb * 0.62),
        if live.playing { RED } else { ACID },
    );
    let info = if let Some(b) = live.build {
        format!("BUILD {}%", (b * 100.0) as u32)
    } else if live.playing {
        format!("BAR {}", live.bar + 1)
    } else {
        "SPACE".into()
    };
    p.text(g.c + vec2(0.0, rb * 0.48), Align2::CENTER_CENTER, info, bold(11.5), DIM);

    // Hover info and captions in the corners
    let info_pos = pos2(rect.left() + 4.0, rect.bottom() - 4.0);
    let caption_pos = pos2(rect.left() + 4.0, rect.top() + 4.0);
    if let Some((t, s)) = hovered {
        let v = Voice::ALL[t];
        let step = song.tracks[t].steps[s];
        let mut text = format!("{} · step {} of {}", v.name().to_uppercase(), s + 1, song.tracks[t].len);
        if step.on && v.is_synth() {
            text += &format!(" · note {:+}", step.note);
        }
        if step.on && step.chance < 100 {
            text += &format!(" · {}%", step.chance);
        }
        p.text(info_pos, Align2::LEFT_BOTTOM, text, bold(12.5), track_color(v));
    } else {
        p.text(
            info_pos,
            Align2::LEFT_BOTTOM,
            "Click: on/off · drag: paint\nRight-click: accent · Shift: chance\nScroll on bass/acid: note",
            body(11.5),
            FAINT,
        );
    }
    if let Some((text, age)) = &live.caption {
        let a = (1.0 - (age - 2.5).max(0.0) / 1.5).clamp(0.0, 1.0);
        p.text(
            caption_pos,
            Align2::LEFT_TOP,
            format!("{}  {}", ph::SPARKLE, text),
            bold(14.0),
            alpha(ACID, a),
        );
    }
    out
}

/// A soft wedge trailing behind angle `a`
fn beam(p: &egui::Painter, c: Pos2, r0: f32, r1: f32, a: f32, build: f32) {
    let trail = 0.9;
    let n = 24;
    let mut mesh = Mesh::default();
    let tint = mix(Color32::WHITE, RED, build);
    for i in 0..=n {
        let k = i as f32 / n as f32;
        let ang = a - trail * (1.0 - k);
        let dir = vec2(ang.cos(), ang.sin());
        let col = alpha(tint, 0.10 * k * k);
        mesh.vertices.push(Vertex {
            pos: c + dir * r0,
            uv: egui::epaint::WHITE_UV,
            color: col,
        });
        mesh.vertices.push(Vertex {
            pos: c + dir * r1,
            uv: egui::epaint::WHITE_UV,
            color: alpha(tint, 0.03 * k * k),
        });
        if i > 0 {
            let b = (i as u32) * 2;
            mesh.add_triangle(b - 2, b - 1, b);
            mesh.add_triangle(b - 1, b + 1, b);
        }
    }
    p.add(mesh);
    let dir = vec2(a.cos(), a.sin());
    p.line_segment([c + dir * r0, c + dir * r1], stroke(1.5, alpha(tint, 0.35)));
}
