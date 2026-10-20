//! The orbit: every track is a ring, every step a dot on it, a beam of light
//! circles like a radar. Shorter tracks have fewer dots and drift against the
//! others – polyrhythms you can see.

use crate::song::{note_name, Song, Voice, NOTES, STEPS, TRACKS};
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
    /// Song mode: the section playing and how many bars into it (with fraction)
    pub part: Option<(usize, f64)>,
    pub caption: Option<(String, f32)>,
    /// Spectrum (engine::BANDS) and output peak, for the visualizer
    pub bands: [f32; 8],
    pub level: f32,
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
    /// Scroll collected over frames: trackpads send many tiny deltas
    pub scroll: f32,
    /// A bass or synth planet held down (planets view): dragged up or down it changes its note
    pub note: Option<crate::planets::NoteDrag>,
}

/// Collect scrolling and return whole notches (±1 per `step` points)
pub fn scroll_notches(acc: &mut f32, delta: f32, step: f32) -> i32 {
    *acc += delta;
    let n = (*acc / step).trunc();
    *acc -= n * step;
    n as i32
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

    /// Dots drawn on a ring: a loop that fits into the bar (8, 4, 2 …) is drawn repeated
    /// all the way round, so it turns with the beam; a real polyrhythm (12, 15 …) gets its own
    pub fn shown(len: usize) -> usize {
        let len = len.max(1);
        if STEPS.is_multiple_of(len) {
            STEPS
        } else {
            len
        }
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
        let len = (song.tracks[track].len as usize).max(1);
        let shown = Self::shown(len);
        let a = (d.y.atan2(d.x) + PI / 2.0).rem_euclid(TAU);
        let k = ((a / TAU * shown as f32).round() as usize) % shown;
        let dot = self.dot(track, k, shown);
        (dot.distance(p) < self.dot_radius(track, shown) * 2.0).then_some((track, k % len))
    }
}

/// Right-click on a step (orbit dot or grid tile): its menu
pub fn step_context_menu(ui: &Ui, resp: &egui::Response, song: &mut Song, hovered: Option<(usize, usize)>) {
    let id = resp.id.with("step_menu");
    if resp.secondary_clicked() {
        ui.data_mut(|d| d.insert_temp(id, hovered));
    }
    resp.context_menu(|ui| match ui.data(|d| d.get_temp::<Option<(usize, usize)>>(id)).flatten() {
        Some((t, s)) => step_menu(ui, song, t, s),
        None => ui.close_menu(),
    });
}

/// Everything about one step: on, accent, slide, how often it plays, and its note
fn step_menu(ui: &mut Ui, song: &mut Song, t: usize, s: usize) {
    let voice = Voice::ALL[t];
    let (scale, root) = (song.scale, song.root);
    ui.set_min_width(250.0);
    ui.label(
        egui::RichText::new(format!("{} · step {}", voice.name(), s + 1))
            .font(bold(13.0))
            .color(track_color(voice)),
    );
    let st = &mut song.tracks[t].steps[s];
    ui.checkbox(&mut st.on, "Plays");
    if ui.checkbox(&mut st.accent, "Accent – louder").changed() && st.accent {
        st.on = true;
    }
    if voice == Voice::Acid && ui.checkbox(&mut st.slide, "Slide into this note").changed() && st.slide {
        st.on = true;
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new("How often").font(bold(11.0)).color(FAINT));
    ui.horizontal(|ui| {
        for (chance, label) in [(100u8, "always"), (75, "75 %"), (50, "50 %"), (25, "25 %")] {
            if ui.selectable_label(st.chance == chance, label).clicked() {
                st.chance = chance;
                st.on = true;
            }
        }
    });
    if voice.is_synth() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Note").font(bold(11.0)).color(FAINT));
        // The scale's notes from an octave below the root to two above
        let notes: Vec<i8> = (-7..=14).map(|d| scale.shift(0, d)).filter(|n| NOTES.contains(n)).collect();
        for row in notes.chunks(7) {
            ui.horizontal(|ui| {
                for &n in row {
                    let name = note_name(root, n);
                    let mark = if n < 0 {
                        "↓"
                    } else if n >= 12 {
                        "↑"
                    } else {
                        ""
                    };
                    if ui.selectable_label(st.on && st.note == n, format!("{name}{mark}")).clicked() {
                        st.note = n;
                        st.on = true;
                    }
                }
            });
        }
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
    if released {
        paint.active = false;
    }
    if resp.clicked() && on_center {
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
    if on_center || hovered.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    // ── Drawing ──
    let kick = live.flash(0);
    // The floor breathes with the kick
    p.circle_filled(g.c, g.r_out + 12.0, alpha(SPACE, 0.55));
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
        let len = (track.len as usize).max(1);
        let shown = Geometry::shown(len);
        let color = track_color(voice);
        let r = g.ring(t);
        let flash = live.flash(t);
        let muted = track.mute;
        let base = if muted { 0.07 } else { 0.16 + flash * 0.4 };
        p.circle_stroke(g.c, r, stroke(1.0 + flash * 1.5, alpha(color, base)));
        // The dot playing now, in drawn positions
        let current = live.playing.then(|| (live.step() % shown as u64) as usize);
        let dr = g.dot_radius(t, shown);
        if track.lock {
            // Locked: a small lock between the first two steps
            let a = Geometry::angle(0, shown) + TAU / shown as f32 / 2.0;
            let at = g.c + vec2(a.cos(), a.sin()) * r;
            p.text(
                at,
                Align2::CENTER_CENTER,
                ph::LOCK_SIMPLE,
                body((g.spacing() * 0.55).min(13.0)),
                alpha(color, 0.8),
            );
        }

        for k in 0..shown {
            let s = k % len;
            let step = track.steps[s];
            let at = g.dot(t, k, shown);
            let is_hover = hovered == Some((t, s));
            let is_now = current == Some(k);
            if step.on {
                let strength = if muted { 0.25 } else { 0.45 + 0.55 * step.chance as f32 / 100.0 };
                let rad = if step.accent { dr * 1.2 } else { dr };
                if is_now && !muted {
                    glow(&p, at, rad, color, flash * 1.6);
                }
                // Slides: a short line along the ring to the previous dot
                if step.slide && s > 0 {
                    let a0 = Geometry::angle(k - 1, shown);
                    let a1 = Geometry::angle(k, shown);
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
    p.circle_filled(
        g.c,
        rb * (1.0 + pulse * 0.04),
        if hot { mix(PANEL_HI, SPACE, 0.4) } else { mix(PANEL, SPACE, 0.6) },
    );
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
            "Click: on/off · drag: paint\nRight-click: the step's menu · Shift: how often\nScroll on bass/synth: note",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loops_that_fit_the_bar_turn_with_the_beam() {
        assert_eq!(Geometry::shown(16), 16);
        assert_eq!(Geometry::shown(8), 16);
        assert_eq!(Geometry::shown(4), 16);
        assert_eq!(Geometry::shown(12), 12);
        assert_eq!(Geometry::shown(15), 15);
        // A click on the second copy of an 8-step loop edits the step it repeats
        let g = Geometry::new(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 800.0)));
        let mut song = Song::empty();
        let t = Voice::OpenHat.index();
        song.tracks[t].len = 8;
        let second_copy_of_step_2 = g.dot(t, 10, 16);
        assert_eq!(g.hit(second_copy_of_step_2, &song), Some((t, 2)));
    }
}
