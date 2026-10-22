//! The planets: the groove as a little solar system seen from an angle. Every track is
//! an orbit round the sun, every step that plays a planet on it. A radar beam sweeps
//! round once a bar – like in the orbit – and every planet it passes lights up and
//! sends out a ring. Bass and synth planets float above the plane: the higher, the
//! higher the note – drag one up or down and a ladder of scale degrees shows where it is.

use crate::face::{additive, halo};
use crate::orbit::{scroll_notches, step_context_menu, Live, OrbitOut, Paint};
use crate::song::{note_name, Song, Step, Voice, NOTE_LIMITS, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::glow;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Mesh, Pos2, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::f32::consts::{PI, TAU};

/// How flat the orbits look: 1 = from straight above, 0 = edge-on
const TILT: f32 = 0.42;
/// How far to drag for one scale degree
const DEGREE_PX: f32 = 14.0;
/// How far up or down a press must move before it becomes a note drag
const DRAG_START: f32 = 5.0;

/// A bass or synth planet held down: let go where it was and it goes off, drag it up or
/// down and its note moves by scale degrees
#[derive(Clone, Copy, Debug)]
pub struct NoteDrag {
    pub track: usize,
    pub step: usize,
    /// Where the press started
    pub from: Pos2,
    /// The note when it started
    pub note: i8,
    /// Dragged far enough up or down: now it changes the note
    pub moved: bool,
}

/// A repeatable random number 0…1
fn hash01(i: u32) -> f32 {
    let mut x = i.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

pub struct System {
    pub c: Pos2,
    pub r_max: f32,
}

impl System {
    pub fn new(rect: Rect) -> Self {
        let r_max = (rect.width() / 2.0 - 16.0).min((rect.height() / 2.0 - 30.0) / TILT).max(40.0);
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
        self.r_max * 0.085
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

    /// How big a planet is drawn: nearer ones bigger, accents bigger still
    fn body(&self, step: Step, depth: f32) -> f32 {
        self.planet_radius() * (1.0 + 0.18 * depth) * if step.accent { 1.35 } else { 1.0 }
    }

    /// The step under `p`, if any. On a planet it's the planet you see there – the one
    /// drawn last, as `planets` draws far to near and track by track. Elsewhere it's the
    /// nearest, counting from a planet's edge, so a bass or synth planet lifted over the
    /// inner orbits wins against the little specks it hangs over
    pub fn hit(&self, p: Pos2, song: &Song) -> Option<(usize, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        let mut on_top: Option<(f32, usize, usize)> = None;
        for (t, voice) in Voice::ALL.into_iter().enumerate() {
            for s in 0..STEPS {
                let step = song.tracks[t].steps[s];
                let h = if step.on { self.lift(voice, step.note) } else { 0.0 };
                let (at, depth) = self.place(t, s, h);
                let near = at.distance(p);
                let body = if step.on { self.body(step, depth) } else { 0.0 };
                if near < body && on_top.is_none_or(|top| depth >= top.0) {
                    on_top = Some((depth, t, s));
                }
                let d = near - depth * 0.5 - body;
                if near < self.planet_radius() * 2.0 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, t, s));
                }
            }
        }
        on_top.or(best).map(|(_, t, s)| (t, s))
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
            } else if step.on && Voice::ALL[t].is_synth() {
                // Not off yet: it may be dragged up or down
                paint.note = pointer.map(|from| NoteDrag {
                    track: t,
                    step: s,
                    from,
                    note: step.note,
                    moved: false,
                });
            } else {
                *paint = Paint {
                    track: t,
                    on: !step.on,
                    active: true,
                    scroll: 0.0,
                    note: None,
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
    if let (Some(mut drag), Some(pt)) = (paint.note, pointer.filter(|_| down)) {
        let d = pt - drag.from;
        if !drag.moved && d.y.abs() > DRAG_START && d.y.abs() > d.x.abs() {
            drag.moved = true;
        }
        if drag.moved {
            let degrees = (-d.y / DEGREE_PX).round() as i32;
            song.tracks[drag.track].steps[drag.step].note = song.scale.shift(drag.note, degrees);
            paint.note = Some(drag);
        } else if let Some((t, s)) = hovered.filter(|&(t, s)| t == drag.track && s != drag.step) {
            // Along the orbit instead: paint the planets off, this one first
            song.tracks[t].steps[drag.step].on = false;
            song.tracks[t].steps[s].on = false;
            *paint = Paint {
                track: t,
                on: false,
                active: true,
                scroll: 0.0,
                note: None,
            };
        }
    }
    if released {
        paint.active = false;
        if let Some(drag) = paint.note.take() {
            if !drag.moved {
                song.tracks[drag.track].steps[drag.step].on = false;
            }
        }
    }
    // A planet on the move keeps its highlight, wherever the pointer is
    let dragging = paint.note.filter(|d| d.moved);
    let hovered = dragging.map(|d| (d.track, d.step)).or(hovered);
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
    if dragging.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    } else if on_sun || hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Drawing ──
    let kick = if live.playing { live.flash(0) } else { 0.0 };
    let build = live.build.unwrap_or(0.0);
    let now = live.playing.then(|| live.step() as usize % STEPS);
    let pr = sys.planet_radius();
    let (r_in, r_out) = (sys.orbit(0) - sys.half_gap(), sys.orbit(TRACKS - 1) + sys.half_gap());

    // Where the bar starts and which way it runs: a short, faint arrow outside the first
    // steps (no ring, no numbers – the planets have the room)
    {
        let r = r_out + 8.0;
        let ink = alpha(TEXT, 0.16);
        let pts: Vec<Pos2> = (0..=10).map(|i| sys.at(r, System::angle(0.7 * i as f32 / 10.0), 0.0)).collect();
        let (tip, before) = (pts[10], pts[9]);
        p.add(egui::Shape::line(pts, stroke(1.0, ink)));
        let dir = (tip - before).normalized();
        let side = vec2(-dir.y, dir.x);
        p.add(egui::Shape::convex_polygon(
            vec![tip + dir * 3.5, tip - dir * 2.0 + side * 2.5, tip - dir * 2.0 - side * 2.5],
            ink,
            egui::Stroke::NONE,
        ));
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
        let r = sys.body(step, depth);
        let lit = if now == Some(s) && !muted { live.flash(t) } else { 0.0 };
        if lit > 0.01 {
            halo(&p, at, r * 4.0, color, 0.7 * lit);
            // The beam just passed it: a ring goes out
            let age = live.hit_age[t];
            if age < 0.45 {
                p.circle_stroke(at, r + age * 80.0, stroke(1.5, alpha(color, 0.6 * (1.0 - age / 0.45))));
            }
        }
        // Accents wear a ring, like Saturn: its far half behind the planet, its near half in front
        let ink = |c: Color32| alpha(c, strength);
        let ring = |from: f32| {
            let pts: Vec<Pos2> = (0..=20)
                .map(|i| {
                    let b = from + i as f32 / 20.0 * PI;
                    at + vec2(b.cos() * r * 1.8, b.sin() * r * 0.5)
                })
                .collect();
            egui::Shape::line(pts, stroke(1.5, ink(mix(color, Color32::WHITE, 0.3))))
        };
        if step.accent {
            p.add(ring(PI));
        }
        // A little planet, lit from the sun: its own colour, a little darker on the far side
        p.circle_filled(at, r, ink(mix(SPACE, color, 0.72)));
        let to_sun = (sys.c - at).normalized();
        p.circle_filled(at + to_sun * r * 0.14, r * 0.86, ink(mix(color, Color32::WHITE, 0.03 + 0.6 * lit)));
        if step.accent {
            p.add(ring(0.0));
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
    // A solid ball of fire with rings of light around it: lime while it waits, warming
    // slowly to orange when the beat starts, red through a build-up
    let heat = ui.ctx().animate_bool_with_time(egui::Id::new("sun_heat"), live.playing, 1.5);
    let rs = sys.sun() * (1.0 + 0.06 * kick + 0.2 * build);
    let warm = mix(ACID, mix(mix(RED, track_color(Voice::Clap), 0.55), RED, build), heat);
    glow(
        &p,
        sys.c,
        rs * 1.3,
        warm,
        0.16 + 0.3 * kick + 0.45 * build + if on_sun { 0.1 } else { 0.0 },
    );
    let skin = mix(warm, Color32::WHITE, 0.08 + 0.12 * kick);
    p.circle_filled(sys.c, rs, skin);
    // Its surface: a few hotter and cooler patches, turning slowly with the sun
    let time = ui.input(|i| i.time) as f32;
    for k in 0..9u32 {
        let r = rs * (0.12 + 0.12 * hash01(k * 3 + 1));
        let lat = (hash01(k * 3 + 2) - 0.5) * 1.5 * (rs - r);
        let lon = hash01(k * 3) * TAU + time * 0.12;
        // Round the back and out of sight again: only the visible half
        if lon.cos() < -0.1 {
            continue;
        }
        let w = (rs - r) * (1.0 - (lat / rs).powi(2)).max(0.0).sqrt();
        let at = sys.c + vec2(w * lon.sin(), lat);
        let hot = k % 3 != 0;
        let col = if hot {
            mix(warm, Color32::WHITE, 0.4)
        } else {
            mix(warm, SPACE, 0.3)
        };
        p.circle_filled(at, r * lon.cos().max(0.35).sqrt(), mix(skin, col, lon.cos().max(0.0) * 0.8));
    }
    let icon = if live.playing { ph::STOP } else { ph::PLAY };
    let ink = mix(warm, SPACE, 0.8);
    p.text(sys.c - vec2(0.0, rs * 0.16), Align2::CENTER_CENTER, icon, body(rs * 0.6), ink);
    let info = live.info();
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

    if let Some(drag) = dragging {
        degree_ladder(&p, &sys, rect, song, drag);
    }

    // Hover info in the corner
    let info_pos = pos2(rect.left() + 4.0, rect.bottom() - 4.0);
    if let Some((t, s)) = hovered {
        let v = Voice::ALL[t];
        let step = song.tracks[t].steps[s];
        let mut text = format!("{} · step {}", v.name().to_uppercase(), s + 1);
        if step.on && v.is_synth() {
            let degree = song.scale.degree(step.note);
            text += &format!(" · degree {} · {}", degree_label(degree), note_name(song.root, step.note));
        }
        if step.on && step.chance < 100 {
            text += &format!(" · {}%", step.chance);
        }
        p.text(info_pos, Align2::LEFT_BOTTOM, text, bold(12.5), track_color(v));
    }
    out
}

/// A scale degree as the ladder shows it: 0 is the root, then +1, +2 … or -1, -2 …
fn degree_label(degree: i32) -> String {
    if degree == 0 {
        "0".into()
    } else {
        format!("{degree:+}")
    }
}

/// Beside a planet on the move: a ladder of the scale's degrees, the one it's on lit
/// and labelled, the roots marked – the rung under the pointer is the note
fn degree_ladder(p: &egui::Painter, sys: &System, rect: Rect, song: &Song, drag: NoteDrag) {
    let voice = Voice::ALL[drag.track];
    let color = track_color(voice);
    let note = song.tracks[drag.track].steps[drag.step].note;
    let scale = song.scale;
    let per_octave = scale.intervals().len() as i32;
    let (start, now) = (scale.degree(drag.note), scale.degree(note));
    let (lo, hi) = (scale.degree(*NOTE_LIMITS.start()), scale.degree(*NOTE_LIMITS.end()));
    let (at, _) = sys.place(drag.track, drag.step, sys.lift(voice, note));
    // Beside the planet, on whichever side has room
    let side = if at.x + 120.0 > rect.right() { -1.0 } else { 1.0 };
    let x = at.x + side * 30.0;
    let y_of = |k: i32| drag.from.y - (k - start) as f32 * DEGREE_PX;
    const SHOWN: i32 = 6;
    let (first, last) = ((now - SHOWN).max(lo), (now + SHOWN).min(hi));
    let fade = |k: i32| 1.0 - (k - now).abs() as f32 / (SHOWN + 1) as f32;
    let label_x = x + side * 14.0;
    // A dark backing so it reads over the orbits
    let (x0, x1) = (x - side * 10.0, label_x + side * 24.0);
    let back = Rect::from_x_y_ranges(x0.min(x1)..=x0.max(x1), y_of(last) - 10.0..=y_of(first) + 10.0);
    p.rect_filled(back, 8.0, alpha(SPACE, 0.7));
    // The spine, fading out at both ends
    for k in first..last {
        let a = fade(k).min(fade(k + 1));
        p.line_segment([pos2(x, y_of(k)), pos2(x, y_of(k + 1))], stroke(1.0, alpha(color, 0.35 * a)));
    }
    let align = if side > 0.0 { Align2::LEFT_CENTER } else { Align2::RIGHT_CENTER };
    for k in first..=last {
        let y = y_of(k);
        let a = fade(k);
        let root = k.rem_euclid(per_octave) == 0;
        let w = if root { 7.0 } else { 4.0 };
        p.line_segment(
            [pos2(x - w, y), pos2(x + w, y)],
            stroke(if root { 1.5 } else { 1.0 }, alpha(color, 0.6 * a)),
        );
        // Not right next to the lit one: its tag covers them
        let near = (k - now).abs();
        if near > 1 && (near <= 4 || root) {
            p.text(pos2(label_x, y), align, degree_label(k), bold(10.5), alpha(FAINT, a));
        }
    }
    // The planet hangs from its rung by a thread
    let y = y_of(now);
    p.line_segment([at, pos2(x, y)], stroke(1.0, alpha(color, 0.4)));
    p.circle_filled(pos2(x, y), 3.5, color);
    // The rung it's on: degree big, note name small
    let degree = degree_label(now);
    let name = note_name(song.root, note);
    let galley = p.layout_no_wrap(degree.clone(), bold(15.0), SPACE);
    let tag_w = galley.size().x + 44.0;
    let tag = if side > 0.0 {
        Rect::from_min_size(pos2(label_x - 4.0, y - 11.0), vec2(tag_w, 22.0))
    } else {
        Rect::from_min_size(pos2(label_x + 4.0 - tag_w, y - 11.0), vec2(tag_w, 22.0))
    };
    p.rect_filled(tag, 11.0, color);
    p.text(pos2(tag.left() + 10.0, y), Align2::LEFT_CENTER, degree, bold(15.0), SPACE);
    p.text(
        pos2(tag.right() - 10.0, y),
        Align2::RIGHT_CENTER,
        name,
        bold(11.0),
        alpha(SPACE, 0.75),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Vec2;

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

    #[test]
    fn a_lifted_planet_wins_over_the_specks_it_hangs_over() {
        let sys = System::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(1160.0, 1000.0)));
        let mut overlaps = 0;
        for t in [6, 7] {
            let voice = Voice::ALL[t];
            for s in 0..STEPS {
                for note in NOTE_LIMITS {
                    let mut song = Song::default();
                    song.tracks[t - 1].steps[s].on = false;
                    song.tracks[t].steps[s] = Step {
                        on: true,
                        note,
                        ..Step::default()
                    };
                    let (below, _) = sys.place(t - 1, s, 0.0);
                    let (at, depth) = sys.place(t, s, sys.lift(voice, note));
                    let body = sys.body(song.tracks[t].steps[s], depth);
                    // Only where the planet hangs over the resting step of the orbit inside
                    if !(0.5..body).contains(&at.distance(below)) {
                        continue;
                    }
                    overlaps += 1;
                    // Clicked off-centre, towards the speck below: still the planet
                    let p = at + (below - at).normalized() * body * 0.8;
                    assert_eq!(sys.hit(p, &song), Some((t, s)), "track {t} step {s} note {note}");
                }
            }
        }
        assert!(overlaps > 10, "{overlaps}");
    }

    /// Press on a step of the default song's bass, move the pointer by `moves` (one frame
    /// each) and let go; returns the song after
    fn press_and_drag(moves: &[Vec2]) -> (Song, usize, usize) {
        press_and_drag_from(Song::default(), moves)
    }

    fn press_and_drag_from(mut song: Song, moves: &[Vec2]) -> (Song, usize, usize) {
        use crate::offscreen::Offscreen;
        use egui::{Event, PointerButton};
        let t = Voice::ALL.iter().position(|v| v.is_synth()).unwrap();
        let s = (0..STEPS).find(|&s| song.tracks[t].steps[s].on).unwrap();
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0));
        let sys = System::new(rect);
        let (start, _) = sys.place(t, s, sys.lift(Voice::ALL[t], song.tracks[t].steps[s].note));
        let mut snap = Offscreen::new(900.0, 700.0);
        crate::theme::install(&snap.ctx);
        let (live, mut paint) = (Live::default(), Paint::default());
        let button = |pos, pressed| Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        let mut frames = vec![vec![Event::PointerMoved(start)], vec![button(start, true)]];
        let mut at = start;
        for &m in moves {
            at += m;
            frames.push(vec![Event::PointerMoved(at)]);
        }
        frames.push(vec![button(at, false)]);
        frames.push(vec![]);
        for (i, events) in frames.into_iter().enumerate() {
            snap.frame_with_events(i as f64 * 0.016, events, |ctx| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
                    planets(ui, rect, &mut song, &live, &mut paint);
                });
            });
        }
        (song, t, s)
    }

    #[test]
    fn dragging_a_bass_planet_moves_its_note_by_scale_degrees() {
        let before = Song::default();
        let up = vec2(0.0, -DEGREE_PX);
        let (song, t, s) = press_and_drag(&[up, up, up]);
        let (was, now) = (before.tracks[t].steps[s], song.tracks[t].steps[s]);
        assert!(now.on);
        assert_eq!(now.note, song.scale.shift(was.note, 3));
        assert_eq!(song.scale.degree(now.note), song.scale.degree(was.note) + 3);

        // A generated line may sit higher than the step menu goes: it moves all the same
        let mut high = Song::default();
        let t = Voice::ALL.iter().position(|v| v.is_synth()).unwrap();
        for step in &mut high.tracks[t].steps {
            step.note = 31;
        }
        let (song, t, s) = press_and_drag_from(high, &[up]);
        assert_eq!(song.tracks[t].steps[s].note, high.scale.shift(31, 1));
        let (song, t, s) = press_and_drag_from(high, &[-up, -up]);
        assert_eq!(song.tracks[t].steps[s].note, high.scale.shift(31, -2));

        // Far down it stops at the lowest note a step can hold
        let (song, t, s) = press_and_drag(&[vec2(0.0, 400.0), vec2(0.0, 400.0)]);
        assert_eq!(song.tracks[t].steps[s].note, *NOTE_LIMITS.start());
    }

    #[test]
    fn a_click_on_a_bass_planet_still_turns_it_off() {
        let (song, t, s) = press_and_drag(&[]);
        assert!(!song.tracks[t].steps[s].on);
        // A wobble smaller than a degree is still a click
        let (song, t, s) = press_and_drag(&[vec2(1.0, -2.0)]);
        assert!(!song.tracks[t].steps[s].on);
        assert_eq!(song.tracks[t].steps[s].note, Song::default().tracks[t].steps[s].note);
    }
}
