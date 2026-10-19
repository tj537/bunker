//! The planets: the groove as a little solar system seen from an angle. Every track is
//! an orbit round the sun, every step that plays a planet on it. A radar beam sweeps
//! round once a bar – like in the orbit – and every planet it passes lights up and
//! sends out a ring. Bass and synth planets float above the plane: the higher, the
//! higher the note.

use crate::face::{additive, halo};
use crate::orbit::{scroll_notches, step_context_menu, Live, OrbitOut, Paint};
use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Mesh, Pos2, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::f32::consts::{PI, TAU};

/// How flat the orbits look: 1 = from straight above, 0 = edge-on
const TILT: f32 = 0.42;

pub struct System {
    pub c: Pos2,
    pub r_max: f32,
}

impl System {
    pub fn new(rect: Rect) -> Self {
        let r_max = (rect.width() / 2.0 - 34.0).min((rect.height() / 2.0 - 46.0) / TILT).max(40.0);
        System {
            c: rect.center() + vec2(0.0, 8.0),
            r_max,
        }
    }

    pub fn orbit(&self, track: usize) -> f32 {
        self.r_max * (0.3 + 0.7 * (track as f32 + 0.5) / TRACKS as f32)
    }

    /// Half the gap between two orbits
    fn half_gap(&self) -> f32 {
        self.r_max * 0.35 / TRACKS as f32
    }

    pub fn sun(&self) -> f32 {
        self.r_max * 0.12
    }

    /// The angle `steps` (with fraction) into the bar: step 1 at the back, like 12 o'clock
    /// on the orbit, then clockwise – the same on every orbit, so the planets line up
    pub fn angle(steps: f32) -> f32 {
        -PI / 2.0 + TAU * steps / STEPS as f32
    }

    /// A point at radius `r` and angle `a` on the plane, lifted by `height`
    fn at(&self, r: f32, a: f32, height: f32) -> Pos2 {
        self.c + vec2(r * a.cos(), r * a.sin() * TILT - height)
    }

    /// Where a step of a track lands on the screen, lifted by `height`, and how near it
    /// is (-1 behind the sun … 1 in front)
    pub fn place(&self, track: usize, step: usize, height: f32) -> (Pos2, f32) {
        let a = Self::angle(step as f32);
        (self.at(self.orbit(track), a, height), a.sin())
    }

    /// How high a note lifts a planet
    pub fn lift(&self, voice: Voice, note: i8) -> f32 {
        if voice.is_synth() {
            note as f32 * self.r_max * 0.006
        } else {
            0.0
        }
    }

    pub fn planet_radius(&self) -> f32 {
        (self.r_max * 0.022).clamp(3.5, 8.5)
    }

    /// The step under `p`, if any – the nearest, the front one on a tie
    pub fn hit(&self, p: Pos2, song: &Song) -> Option<(usize, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for (t, voice) in Voice::ALL.into_iter().enumerate() {
            for s in 0..STEPS {
                let step = song.tracks[t].steps[s];
                let h = if step.on { self.lift(voice, step.note) } else { 0.0 };
                let (at, depth) = self.place(t, s, h);
                let d = at.distance(p) - depth * 0.5;
                if d < self.planet_radius() * 2.0 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, t, s));
                }
            }
        }
        best.map(|(_, t, s)| (t, s))
    }
}

pub fn planets(ui: &mut Ui, rect: Rect, song: &mut Song, live: &Live, paint: &mut Paint) -> OrbitOut {
    let resp = ui.allocate_rect(rect, Sense::click_and_drag());
    let sys = System::new(rect);
    let p = ui.painter_at(rect);
    let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
    let on_sun = pointer.is_some_and(|pt| pt.distance(sys.c) < sys.sun() * 1.1);
    let hovered = if on_sun { None } else { pointer.and_then(|pt| sys.hit(pt, song)) };
    let mut out = OrbitOut { toggle_play: false };

    // ── Input: the same as the orbit's ──
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
                    scroll: 0.0,
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
    if resp.clicked() && on_sun {
        out.toggle_play = true;
    }
    step_context_menu(ui, &resp, song, hovered);
    if let Some((t, s)) = hovered {
        let scroll = ui.input(|i| i.raw_scroll_delta.y);
        let notches = scroll_notches(&mut paint.scroll, scroll, 24.0);
        if Voice::ALL[t].is_synth() && notches != 0 {
            let scale = song.scale;
            let step = &mut song.tracks[t].steps[s];
            step.on = true;
            step.note = scale.shift(step.note, notches.signum());
        }
    } else {
        paint.scroll = 0.0;
    }
    if on_sun || hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Drawing ──
    let kick = if live.playing { live.flash(0) } else { 0.0 };
    let build = live.build.unwrap_or(0.0);
    let now = live.playing.then(|| live.step() as usize % STEPS);
    let pr = sys.planet_radius();
    let (r_in, r_out) = (sys.orbit(0) - sys.half_gap(), sys.orbit(TRACKS - 1) + sys.half_gap());

    // The plane of the system, faintly
    let rim = r_out + 12.0;
    let plane: Vec<Pos2> = (0..96).map(|i| sys.at(rim, i as f32 / 96.0 * TAU, 0.0)).collect();
    p.add(egui::Shape::convex_polygon(plane, alpha(SPACE, 0.5), stroke(1.0, alpha(LINE, 0.6))));

    // Beat numbers outside, where the beats are
    for (i, label) in ["1", "2", "3", "4"].iter().enumerate() {
        let a = System::angle((i * 4) as f32);
        let at = sys.at(rim + 16.0, a, 0.0) + vec2(0.0, a.sin() * 8.0);
        let current = now.is_some_and(|n| n / 4 == i);
        p.text(at, Align2::CENTER_CENTER, *label, display(15.0), if current { TEXT } else { FAINT });
    }

    // The radar beam, lying on the plane
    if live.playing {
        let a = System::angle(live.pos.rem_euclid(STEPS as f64) as f32);
        let light = mix(Color32::WHITE, RED, build);
        let mut mesh = Mesh::default();
        let n = 32;
        for i in 0..=n {
            let f = i as f32 / n as f32;
            let b = a - 0.9 * (1.0 - f);
            let k = 0.35 * f * f * (1.0 + build);
            mesh.colored_vertex(sys.at(r_in, b, 0.0), additive(light, k * 0.5));
            mesh.colored_vertex(sys.at(r_out + 8.0, b, 0.0), additive(light, k));
            if i > 0 {
                let j = 2 * i as u32;
                mesh.add_triangle(j - 2, j - 1, j);
                mesh.add_triangle(j - 1, j + 1, j);
            }
        }
        p.add(egui::Shape::mesh(mesh));
        p.line_segment([sys.at(r_in, a, 0.0), sys.at(r_out + 8.0, a, 0.0)], stroke(1.5, alpha(light, 0.55)));
    }

    // Orbit lines: the far halves now, the near halves after the sun
    let orbit_line = |t: usize, near: bool| {
        let muted = song.tracks[t].mute;
        let flash = if live.playing { live.flash(t) } else { 0.0 };
        let k = if muted { 0.08 } else { 0.18 + 0.4 * flash + 0.15 * build };
        let (a0, a1) = if near { (0.0, PI) } else { (PI, TAU) };
        let r = sys.orbit(t);
        let points: Vec<Pos2> = (0..=48).map(|i| sys.at(r, a0 + (a1 - a0) * i as f32 / 48.0, 0.0)).collect();
        egui::Shape::line(points, stroke(1.0 + flash * 1.5, alpha(track_color(Voice::ALL[t]), k)))
    };
    for t in 0..TRACKS {
        p.add(orbit_line(t, false));
    }

    // Every step, far to near: a planet if it plays, a speck if not
    let mut steps: Vec<(f32, usize, usize)> = (0..TRACKS)
        .flat_map(|t| (0..STEPS).map(move |s| (System::angle(s as f32).sin(), t, s)))
        .collect();
    steps.sort_by(|a, b| a.0.total_cmp(&b.0));
    let draw = |t: usize, s: usize| {
        let voice = Voice::ALL[t];
        let track = &song.tracks[t];
        let step = track.steps[s];
        let color = track_color(voice);
        let muted = track.mute;
        let (base, depth) = sys.place(t, s, 0.0);
        let scale = 1.0 + 0.18 * depth;
        let is_hover = hovered == Some((t, s));
        if !step.on {
            let r = if s.is_multiple_of(4) { 2.0 } else { 1.3 } * scale;
            p.circle_filled(base, r, alpha(color, if muted { 0.12 } else { 0.35 }));
            if is_hover {
                p.circle_stroke(base, pr * scale * 1.5, stroke(2.0, TEXT));
            }
            return;
        }
        let lift = sys.lift(voice, step.note);
        let (at, _) = sys.place(t, s, lift);
        let strength = if muted { 0.3 } else { 0.55 + 0.45 * step.chance as f32 / 100.0 };
        let color = if muted { mix(color, FAINT, 0.6) } else { color };
        if lift.abs() > 0.5 {
            p.line_segment([base, at], stroke(1.0, alpha(color, strength * 0.5)));
            p.circle_filled(base, 1.5, alpha(color, strength * 0.5));
        }
        // A slide: a trail from the planet before
        if step.slide && s > 0 && track.steps[s - 1].on {
            let prev = track.steps[s - 1];
            let (from, _) = sys.place(t, s - 1, sys.lift(voice, prev.note));
            p.line_segment([from, at], stroke(2.0, alpha(color, strength * 0.7)));
        }
        let r = pr * scale * if step.accent { 1.35 } else { 1.0 };
        let lit = if now == Some(s) && !muted { live.flash(t) } else { 0.0 };
        if lit > 0.01 {
            halo(&p, at, r * 4.0, color, 0.7 * lit);
            // The beam just passed it: a ring goes out
            let age = live.hit_age[t];
            if age < 0.45 {
                p.circle_stroke(at, r + age * 80.0, stroke(1.5, alpha(color, 0.6 * (1.0 - age / 0.45))));
            }
        }
        // A little planet, lit from the sun: dark with its bright side to the middle
        let ink = |c: Color32| alpha(c, strength);
        p.circle_filled(at, r, ink(mix(SPACE, color, 0.4)));
        let to_sun = (sys.c - at).normalized();
        p.circle_filled(at + to_sun * r * 0.22, r * 0.78, ink(mix(color, Color32::WHITE, 0.1 + 0.6 * lit)));
        if step.accent {
            // Accents wear a ring, like Saturn
            let ring: Vec<Pos2> = (0..=40)
                .map(|i| {
                    let b = i as f32 / 40.0 * TAU;
                    at + vec2(b.cos() * r * 1.8, b.sin() * r * 0.5)
                })
                .collect();
            p.add(egui::Shape::line(ring, stroke(1.5, ink(mix(color, Color32::WHITE, 0.3)))));
        }
        if step.chance < 100 {
            p.circle_stroke(at, r + 3.0, stroke(1.0, alpha(color, 0.5)));
        }
        if is_hover {
            p.circle_stroke(at, r * 1.8, stroke(2.0, TEXT));
        }
    };
    let split = steps.partition_point(|(d, _, _)| *d < 0.0);
    for &(_, t, s) in &steps[..split] {
        draw(t, s);
    }

    // ── The sun: the play button ──
    // A soft glowing ball: lime while it waits, warming slowly to orange when the beat
    // starts, red through a build-up
    let heat = ui.ctx().animate_bool_with_time(egui::Id::new("sun_heat"), live.playing, 1.5);
    let rs = sys.sun() * (1.0 + 0.06 * kick + 0.2 * build);
    let warm = mix(ACID, mix(mix(RED, track_color(Voice::Clap), 0.55), RED, build), heat);
    halo(
        &p,
        sys.c,
        rs * 3.0,
        warm,
        0.3 + 0.35 * kick + 0.4 * build + if on_sun { 0.15 } else { 0.0 },
    );
    // Bright where the light catches it, deeper towards the edge
    let shine = sys.c - vec2(rs, rs) * 0.3;
    let mut ball = Mesh::default();
    ball.colored_vertex(shine, mix(warm, Color32::WHITE, 0.7 + 0.2 * kick));
    let n = 64;
    for i in 0..n {
        let a = i as f32 / n as f32 * TAU;
        ball.colored_vertex(sys.c + vec2(a.cos(), a.sin()) * rs, mix(warm, SPACE, 0.15));
        ball.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(egui::Shape::mesh(ball));
    p.circle_stroke(sys.c, rs, stroke(1.0, alpha(mix(warm, Color32::WHITE, 0.5), 0.6)));
    let icon = if live.playing { ph::STOP } else { ph::PLAY };
    let ink = mix(warm, SPACE, 0.8);
    p.text(sys.c - vec2(0.0, rs * 0.16), Align2::CENTER_CENTER, icon, body(rs * 0.6), ink);
    let info = if let Some(b) = live.build {
        format!("BUILD {}%", (b * 100.0) as u32)
    } else if live.playing {
        format!("BAR {}", live.bar + 1)
    } else {
        "SPACE".into()
    };
    p.text(
        sys.c + vec2(0.0, rs * 0.42),
        Align2::CENTER_CENTER,
        info,
        bold((rs * 0.22).clamp(9.0, 12.0)),
        ink,
    );
    if live.playing {
        ui.ctx().request_repaint();
    }

    for t in 0..TRACKS {
        p.add(orbit_line(t, true));
    }
    for &(_, t, s) in &steps[split..] {
        draw(t, s);
    }

    // Hover info and captions in the corners
    let info_pos = pos2(rect.left() + 4.0, rect.bottom() - 4.0);
    if let Some((t, s)) = hovered {
        let v = Voice::ALL[t];
        let step = song.tracks[t].steps[s];
        let mut text = format!("{} · step {}", v.name().to_uppercase(), s + 1);
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
            "Every orbit is a track, every planet a step that plays\nClick: on/off · drag: paint · right-click: the step's menu\nScroll on bass/synth: note (the higher, the higher)",
            body(11.5),
            FAINT,
        );
    }
    if let Some((text, age)) = &live.caption {
        let a = (1.0 - (age - 2.5).max(0.0) / 1.5).clamp(0.0, 1.0);
        p.text(
            pos2(rect.left() + 4.0, rect.top() + 4.0),
            Align2::LEFT_TOP,
            format!("{}  {}", ph::SPARKLE, text),
            bold(14.0),
            alpha(ACID, a),
        );
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_step_can_be_hit() {
        let sys = System::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0)));
        let mut song = Song::default();
        for t in 0..TRACKS {
            for s in 0..STEPS {
                let step = song.tracks[t].steps[s];
                let h = if step.on { sys.lift(Voice::ALL[t], step.note) } else { 0.0 };
                let (at, _) = sys.place(t, s, h);
                assert_eq!(sys.hit(at, &song), Some((t, s)), "track {t} step {s}");
            }
        }
        // The sun is not a step
        song.tracks[0].steps[0].on = false;
        assert_eq!(sys.hit(sys.c, &song), None);
    }
}
