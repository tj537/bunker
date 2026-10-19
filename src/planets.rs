//! The planets: a little solar system seen from an angle. Every track is a planet that
//! goes round the sun once a bar; its steps are beads on its orbit, and it plays when
//! it passes one. The orbits are turned against each other, so the beats form spiral
//! arms. Bass and synth beads stand on stalks: the higher, the higher the note.

use crate::orbit::{scroll_notches, step_context_menu, Live, OrbitOut, Paint};
use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::glow;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::f32::consts::{PI, TAU};

/// How flat the orbits look: 1 = from straight above, 0 = edge-on
const TILT: f32 = 0.42;
/// Every orbit is turned this much further than the one inside it
const SPIRAL: f32 = TAU / STEPS as f32;

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

    pub fn sun(&self) -> f32 {
        self.r_max * 0.12
    }

    /// The angle of a point `steps` (with fraction) into the bar on a track's orbit:
    /// step 1 of the innermost orbit is at the front
    pub fn angle(track: usize, steps: f32) -> f32 {
        PI / 2.0 + TAU * steps / STEPS as f32 + track as f32 * SPIRAL
    }

    /// Where an angle on a track's orbit lands on the screen, lifted by `height`, and how
    /// near it is (-1 behind the sun … 1 in front)
    pub fn place(&self, track: usize, a: f32, height: f32) -> (Pos2, f32) {
        let r = self.orbit(track);
        (self.c + vec2(r * a.cos(), r * a.sin() * TILT - height), a.sin())
    }

    /// How high a note lifts a bead
    pub fn lift(&self, voice: Voice, note: i8) -> f32 {
        if voice.is_synth() {
            note as f32 * self.r_max * 0.006
        } else {
            0.0
        }
    }

    pub fn bead_radius(&self) -> f32 {
        (self.r_max * 0.018).clamp(3.0, 7.0)
    }

    /// The step bead under `p`, if any – the nearest, the front one on a tie
    pub fn hit(&self, p: Pos2, song: &Song) -> Option<(usize, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for (t, voice) in Voice::ALL.into_iter().enumerate() {
            for s in 0..STEPS {
                let step = song.tracks[t].steps[s];
                let h = if step.on { self.lift(voice, step.note) } else { 0.0 };
                let (at, depth) = self.place(t, Self::angle(t, s as f32), h);
                let d = at.distance(p) - depth * 0.5;
                if d < self.bead_radius() * 2.2 && best.is_none_or(|b| d < b.0) {
                    best = Some((d, t, s));
                }
            }
        }
        best.map(|(_, t, s)| (t, s))
    }
}

/// Points along an orbit between two angles
fn arc_points(sys: &System, track: usize, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = 48;
    (0..=n)
        .map(|i| sys.place(track, a0 + (a1 - a0) * i as f32 / n as f32, 0.0).0)
        .collect()
}

/// Something to draw, at a depth, so the near things cover the far ones
enum Thing {
    Bead { t: usize, s: usize },
    Planet { t: usize },
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
    let bar = if live.playing {
        live.pos.rem_euclid(STEPS as f64) as f32
    } else {
        0.0
    };
    let now = live.playing.then(|| live.step() as usize % STEPS);
    let br = sys.bead_radius();

    // The plane of the system, faintly
    let rim = sys.r_max + 18.0;
    let plane: Vec<Pos2> = (0..96)
        .map(|i| {
            let a = i as f32 / 96.0 * TAU;
            sys.c + vec2(rim * a.cos(), rim * a.sin() * TILT)
        })
        .collect();
    p.add(egui::Shape::convex_polygon(plane, alpha(SPACE, 0.5), stroke(1.0, alpha(LINE, 0.6))));

    // Beat numbers outside the outer orbit, where its beats are
    let outer = TRACKS - 1;
    for (i, label) in ["1", "2", "3", "4"].iter().enumerate() {
        let a = System::angle(outer, (i * 4) as f32);
        let r = rim + 14.0;
        let at = sys.c + vec2(r * a.cos(), r * a.sin() * TILT + if a.sin() > 0.0 { 6.0 } else { -6.0 });
        let current = now.is_some_and(|n| n / 4 == i);
        p.text(at, Align2::CENTER_CENTER, *label, display(15.0), if current { TEXT } else { FAINT });
    }

    // Orbit lines: the far halves now, the near halves after the sun
    let orbit_line = |t: usize, near: bool| {
        let voice = Voice::ALL[t];
        let muted = song.tracks[t].mute;
        let flash = if live.playing { live.flash(t) } else { 0.0 };
        let k = if muted { 0.08 } else { 0.18 + 0.4 * flash + 0.15 * build };
        let (a0, a1) = if near { (0.0, PI) } else { (PI, TAU) };
        egui::Shape::line(arc_points(&sys, t, a0, a1), stroke(1.0 + flash * 1.5, alpha(track_color(voice), k)))
    };
    for t in 0..TRACKS {
        p.add(orbit_line(t, false));
    }

    // Beads and planets, far to near
    let mut things: Vec<(f32, Thing)> = Vec::new();
    for t in 0..TRACKS {
        for s in 0..STEPS {
            things.push((System::angle(t, s as f32).sin(), Thing::Bead { t, s }));
        }
        things.push((System::angle(t, bar).sin() + 0.01, Thing::Planet { t }));
    }
    things.sort_by(|a, b| a.0.total_cmp(&b.0));
    let draw = |thing: &Thing| match *thing {
        Thing::Bead { t, s } => {
            let voice = Voice::ALL[t];
            let track = &song.tracks[t];
            let step = track.steps[s];
            let color = track_color(voice);
            let muted = track.mute;
            let a = System::angle(t, s as f32);
            let (base, depth) = sys.place(t, a, 0.0);
            let scale = 1.0 + 0.18 * depth;
            let is_now = now == Some(s);
            let flash = if live.playing { live.flash(t) } else { 0.0 };
            if step.on {
                let lift = sys.lift(voice, step.note);
                let (at, _) = sys.place(t, a, lift);
                let strength = if muted { 0.25 } else { 0.5 + 0.5 * step.chance as f32 / 100.0 };
                if lift.abs() > 0.5 {
                    p.line_segment([base, at], stroke(1.0, alpha(color, strength * 0.5)));
                    p.circle_filled(base, 1.5, alpha(color, strength * 0.5));
                }
                // A slide: a line from the bead before
                if step.slide && s > 0 && track.steps[s - 1].on {
                    let prev = track.steps[s - 1];
                    let (from, _) = sys.place(t, System::angle(t, (s - 1) as f32), sys.lift(voice, prev.note));
                    p.line_segment([from, at], stroke(2.0, alpha(color, strength * 0.7)));
                }
                let r = br * scale * if step.accent { 1.3 } else { 1.0 };
                if is_now && !muted {
                    glow(&p, at, r, color, flash * 1.8);
                    // The planet just passed it: a ring goes out
                    let age = live.hit_age[t];
                    if age < 0.45 {
                        p.circle_stroke(at, r + age * 80.0, stroke(1.5, alpha(color, 0.6 * (1.0 - age / 0.45))));
                    }
                }
                p.circle_filled(at, r, alpha(color, strength));
                if step.accent {
                    p.circle_filled(at, r * 0.4, alpha(Color32::WHITE, strength));
                }
                if step.chance < 100 {
                    p.circle_stroke(at, r + 2.5, stroke(1.0, alpha(color, 0.5)));
                }
                if hovered == Some((t, s)) {
                    p.circle_stroke(at, r * 1.7, stroke(2.0, TEXT));
                }
            } else {
                // An empty spot: a speck, a bit bigger on the beat
                let r = if s % 4 == 0 { 2.0 } else { 1.3 } * scale;
                p.circle_filled(base, r, alpha(color, if muted { 0.12 } else { 0.35 }));
                if hovered == Some((t, s)) {
                    p.circle_stroke(base, br * scale * 1.5, stroke(2.0, TEXT));
                }
            }
        }
        Thing::Planet { t } => {
            let voice = Voice::ALL[t];
            let muted = song.tracks[t].mute;
            let color = if muted {
                mix(track_color(voice), FAINT, 0.7)
            } else {
                track_color(voice)
            };
            let a = System::angle(t, bar);
            let (at, depth) = sys.place(t, a, 0.0);
            let r = br * 1.9 * (1.0 + 0.2 * depth);
            let flash = if live.playing && !muted { live.flash(t) } else { 0.0 };
            glow(&p, at, r, color, 0.25 + flash * 1.4 + build * 0.5);
            // Lit from the sun: a dark ball with its bright side turned to the middle
            p.circle_filled(at, r, mix(SPACE, color, 0.35));
            let to_sun = (sys.c - at).normalized();
            p.circle_filled(at + to_sun * r * 0.22, r * 0.78, mix(color, Color32::WHITE, 0.15 + 0.4 * flash));
            if voice == Voice::Acid {
                // The synth gets a ring, like Saturn
                let ring: Vec<Pos2> = (0..=40)
                    .map(|i| {
                        let b = i as f32 / 40.0 * TAU;
                        at + vec2(b.cos() * r * 1.9, b.sin() * r * 0.55)
                    })
                    .collect();
                p.add(egui::Shape::line(ring, stroke(1.5, alpha(color, 0.7))));
            }
        }
    };
    let split = things.partition_point(|(d, _)| *d < 0.0);
    for (_, thing) in &things[..split] {
        draw(thing);
    }

    // ── The sun: the play button ──
    let rs = sys.sun() * (1.0 + 0.06 * kick + 0.25 * build);
    let warm = if live.playing {
        mix(mix(RED, track_color(Voice::Clap), 0.45), RED, build)
    } else {
        ACID
    };
    glow(
        &p,
        sys.c,
        rs * 1.3,
        warm,
        0.3 + kick * 0.5 + build * 0.6 + if on_sun { 0.15 } else { 0.0 },
    );
    p.circle_filled(sys.c, rs, mix(SPACE, warm, 0.55 + 0.25 * kick));
    p.circle_filled(sys.c - vec2(rs, rs) * 0.18, rs * 0.7, mix(SPACE, warm, 0.75 + 0.25 * kick));
    let icon = if live.playing { ph::STOP } else { ph::PLAY };
    p.text(sys.c - vec2(0.0, rs * 0.18), Align2::CENTER_CENTER, icon, body(rs * 0.6), SPACE);
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
        SPACE,
    );

    for t in 0..TRACKS {
        p.add(orbit_line(t, true));
    }
    for (_, thing) in &things[split..] {
        draw(thing);
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
            "Every planet goes round once a bar and plays its beads\nClick: on/off · drag: paint · right-click: the step's menu\nScroll on bass/synth: note (the higher, the higher)",
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
    fn every_bead_can_be_hit() {
        let sys = System::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(900.0, 700.0)));
        let mut song = Song::default();
        for t in 0..TRACKS {
            for s in 0..STEPS {
                let step = song.tracks[t].steps[s];
                let h = if step.on { sys.lift(Voice::ALL[t], step.note) } else { 0.0 };
                let (at, _) = sys.place(t, System::angle(t, s as f32), h);
                assert_eq!(sys.hit(at, &song), Some((t, s)), "track {t} step {s}");
            }
        }
        // The sun is not a bead
        song.tracks[0].steps[0].on = false;
        assert_eq!(sys.hit(sys.c, &song), None);
    }
}
