//! The pad view: eight big pads to play live, with a classic
//! step grid underneath.

use crate::orbit::{Live, Paint};
use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::glow_rect;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, Sense, Ui};
use egui_phosphor::regular as ph;

pub const KEYS: [&str; TRACKS] = ["A", "S", "D", "F", "G", "H", "J", "K"];

pub struct PadsOut {
    pub hits: Vec<Voice>,
}

pub fn pads(ui: &mut Ui, rect: Rect, song: &mut Song, live: &Live, paint: &mut Paint, pressed: &[bool; TRACKS]) -> PadsOut {
    let mut out = PadsOut { hits: Vec::new() };
    // The tracks get more room than the pads
    let grid_h = (rect.height() * 0.58).clamp(260.0, 560.0);
    let pad_area = Rect::from_min_max(rect.min, pos2(rect.right(), rect.bottom() - grid_h - 16.0));
    let grid_area = Rect::from_min_max(pos2(rect.left(), rect.bottom() - grid_h), rect.max);

    // ── Pads: 4 × 2 ──
    let gap = 14.0;
    let header = 40.0;
    let pads_rect = Rect::from_min_max(pad_area.min + vec2(0.0, header), pad_area.max);
    let pw = (pads_rect.width() - gap * 3.0) / 4.0;
    let ph_ = ((pads_rect.height() - gap) / 2.0).min(pw * 0.9);
    let total_h = ph_ * 2.0 + gap;
    let top = pads_rect.top() + (pads_rect.height() - total_h) / 2.0;

    ui.painter().text(
        pos2(pad_area.left(), pad_area.top() + 16.0),
        Align2::LEFT_CENTER,
        "Play the pads with the mouse or keys A–K",
        body(12.0),
        FAINT,
    );

    for (i, voice) in Voice::ALL.into_iter().enumerate() {
        let (row, col) = (i / 4, i % 4);
        let r = Rect::from_min_size(
            pos2(pads_rect.left() + col as f32 * (pw + gap), top + row as f32 * (ph_ + gap)),
            vec2(pw, ph_),
        );
        let resp = ui.allocate_rect(r, Sense::click());
        if resp.is_pointer_button_down_on() && ui.input(|inp| inp.pointer.primary_pressed()) {
            out.hits.push(voice);
        }
        let color = track_color(voice);
        let flash = live.flash(i);
        let down = resp.is_pointer_button_down_on() || pressed[i];
        let r = if down { r.shrink(3.0) } else { r };
        let p = ui.painter();
        glow_rect(p, r, 18.0, color, flash * 1.4);
        let fill = mix(PANEL_HI, color, 0.10 + flash * 0.55 + if resp.hovered() { 0.05 } else { 0.0 });
        p.rect_filled(r, 18.0, fill);
        p.rect_stroke(r, 18.0, stroke(1.5, alpha(color, 0.35 + flash * 0.65)), egui::StrokeKind::Inside);
        // Top edge highlight
        p.line_segment(
            [r.left_top() + vec2(18.0, 1.5), r.right_top() + vec2(-18.0, 1.5)],
            stroke(1.0, alpha(Color32::WHITE, 0.08)),
        );
        let muted = song.tracks[i].mute;
        p.text(
            r.left_top() + vec2(16.0, 14.0),
            Align2::LEFT_TOP,
            voice.name().to_uppercase(),
            display((pw * 0.13).min(22.0)),
            if muted { FAINT } else { mix(color, Color32::WHITE, flash * 0.6) },
        );
        p.text(
            r.right_top() + vec2(-14.0, 14.0),
            Align2::RIGHT_TOP,
            KEYS[i],
            bold(13.0),
            alpha(TEXT, 0.4),
        );
        p.text(
            r.left_top() + vec2(16.0, 14.0 + (pw * 0.13).min(22.0) * 1.35),
            Align2::LEFT_TOP,
            song.sound_name(voice),
            bold(12.0),
            alpha(color, 0.7),
        );
        if muted {
            p.text(
                r.left_bottom() + vec2(16.0, -12.0),
                Align2::LEFT_BOTTOM,
                format!("{} muted", ph::SPEAKER_SLASH),
                body(12.0),
                FAINT,
            );
        }
    }

    // ── Step grid ──
    let label_w = 74.0;
    let row_h = (grid_area.height() / TRACKS as f32).min(60.0);
    let cell_w = (grid_area.width() - label_w) / STEPS as f32;
    let resp = ui.allocate_rect(grid_area, Sense::click_and_drag());
    let pointer = resp.hover_pos().or(resp.interact_pointer_pos());
    let cell_at = |pt: egui::Pos2| -> Option<(usize, usize)> {
        let x = pt.x - grid_area.left() - label_w;
        let y = pt.y - grid_area.top();
        if x < 0.0 || y < 0.0 {
            return None;
        }
        let (t, s) = ((y / row_h) as usize, (x / cell_w) as usize);
        (t < TRACKS && s < STEPS).then_some((t, s))
    };
    let hovered = pointer.and_then(cell_at);
    let (pressed_now, down, released) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_down(), i.pointer.primary_released()));
    if pressed_now && resp.hovered() {
        if let Some((t, s)) = hovered {
            if s < song.tracks[t].len as usize {
                *paint = Paint {
                    track: t,
                    on: !song.tracks[t].steps[s].on,
                    active: true,
                    scroll: 0.0,
                    note: None,
                };
                song.tracks[t].steps[s].on = paint.on;
            }
        }
    }
    if down && paint.active {
        if let Some((t, s)) = hovered {
            if t == paint.track && s < song.tracks[t].len as usize {
                song.tracks[t].steps[s].on = paint.on;
            }
        }
    }
    if released {
        paint.active = false;
    }
    crate::orbit::step_context_menu(ui, &resp, song, hovered);
    let p = ui.painter_at(grid_area.expand(2.0));
    p.rect_filled(grid_area.expand(2.0), 12.0, alpha(SPACE, 0.6));
    for (t, voice) in Voice::ALL.into_iter().enumerate() {
        let track = &song.tracks[t];
        let color = track_color(voice);
        let y = grid_area.top() + t as f32 * row_h;
        let label = if track.lock {
            format!("{} {}", voice.name(), ph::LOCK_SIMPLE)
        } else {
            voice.name().to_string()
        };
        p.text(
            pos2(grid_area.left() + 10.0, y + row_h / 2.0),
            Align2::LEFT_CENTER,
            label,
            bold(12.0),
            if track.mute { FAINT } else { color },
        );
        let now = live.playing.then(|| track.step_at(live.step()));
        for s in 0..STEPS {
            let cell =
                Rect::from_min_size(pos2(grid_area.left() + label_w + s as f32 * cell_w, y), vec2(cell_w, row_h)).shrink2(vec2(2.0, 3.0));
            let inside = s < track.len as usize;
            let step = track.steps[s];
            let beat = s % 4 == 0;
            let base = if !inside {
                alpha(PANEL, 0.3)
            } else if beat {
                PANEL_HI
            } else {
                mix(PANEL, PANEL_HI, 0.5)
            };
            p.rect_filled(cell, 4.0, base);
            if inside && step.on {
                let a = if track.mute { 0.3 } else { 0.5 + 0.5 * step.chance as f32 / 100.0 };
                p.rect_filled(
                    cell,
                    4.0,
                    alpha(
                        color,
                        a * if step.accent {
                            1.0
                        } else if step.ghost {
                            0.4
                        } else {
                            0.7
                        },
                    ),
                );
                if step.accent {
                    p.rect_filled(
                        Rect::from_center_size(cell.center(), vec2(4.0, 4.0)),
                        2.0,
                        alpha(Color32::WHITE, 0.8),
                    );
                }
            }
            if now == Some(s) {
                p.rect_stroke(cell, 4.0, stroke(1.5, alpha(Color32::WHITE, 0.7)), egui::StrokeKind::Outside);
            }
            if hovered == Some((t, s)) && inside {
                p.rect_stroke(cell, 4.0, stroke(1.0, TEXT), egui::StrokeKind::Outside);
            }
        }
    }
    out
}
