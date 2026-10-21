//! The pads of the right panel: the character pad (dark × acid, a ring for space), the EQ
//! curve with three points to drag, and the filter pad (low-pass ← off → high-pass × resonance)

use crate::theme::*;
use crate::widgets::glow;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Response, Sense, Ui};
use egui_phosphor::regular as ph;

/// The low-pass and high-pass sides of the filter
pub const LOW_PASS: Color32 = Color32::from_rgb(69, 200, 255);
pub const HIGH_PASS: Color32 = Color32::from_rgb(255, 154, 60);

fn pad_frame(p: &egui::Painter, rect: Rect) {
    p.rect_filled(rect, 12.0, Color32::from_black_alpha(80));
    p.rect_stroke(rect, 12.0, stroke(1.0, LINE), egui::StrokeKind::Inside);
    for k in 1..4 {
        let x = rect.left() + rect.width() * k as f32 / 4.0;
        let y = rect.top() + rect.height() * k as f32 / 4.0;
        p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], stroke(1.0, alpha(LINE, 0.5)));
        p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], stroke(1.0, alpha(LINE, 0.5)));
    }
}

fn dashed(p: &egui::Painter, a: Pos2, b: Pos2, color: Color32) {
    p.add(egui::Shape::dashed_line(&[a, b], stroke(1.0, color), 4.0, 4.0));
}

/// Dark (up) × acid (right) in one pad: drag anywhere, double-click resets. `shown_acid` is
/// the acid as the engine plays it (the auto mode sweeps it) – the puck follows it.
pub fn character(ui: &mut Ui, dark: &mut f32, acid: &mut f32, def: [f32; 2], shown_acid: Option<f32>, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    if resp.double_clicked() {
        [*dark, *acid] = def;
        resp.mark_changed();
    } else if let Some(pos) = resp
        .interact_pointer_pos()
        .filter(|_| resp.is_pointer_button_down_on() || resp.dragged())
    {
        let a = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        let d = ((rect.bottom() - pos.y) / rect.height()).clamp(0.0, 1.0);
        if (a - *acid).abs() + (d - *dark).abs() > 1e-4 {
            (*acid, *dark) = (a, d);
            resp.mark_changed();
        }
    }

    let p = ui.painter_at(rect.expand(2.0));
    pad_frame(&p, rect);
    let held = resp.is_pointer_button_down_on() || resp.dragged();
    let shown = if held { *acid } else { shown_acid.unwrap_or(*acid) };
    let at = pos2(rect.left() + shown * rect.width(), rect.bottom() - *dark * rect.height());
    dashed(&p, pos2(rect.left(), at.y), pos2(rect.right(), at.y), alpha(TEXT, 0.25));
    dashed(&p, pos2(at.x, rect.top()), pos2(at.x, rect.bottom()), alpha(TEXT, 0.25));
    let pad = vec2(10.0, 8.0);
    let dark_col = track_color(crate::song::Voice::Ride);
    p.text(
        rect.left_top() + pad,
        Align2::LEFT_TOP,
        format!("DARK {}", ph::ARROW_UP),
        bold(10.5),
        dark_col,
    );
    p.text(
        rect.right_top() + vec2(-pad.x, pad.y),
        Align2::RIGHT_TOP,
        "dirty",
        body(10.5),
        FAINT,
    );
    p.text(
        rect.left_bottom() + vec2(pad.x, -pad.y),
        Align2::LEFT_BOTTOM,
        "clean",
        body(10.5),
        FAINT,
    );
    p.text(
        rect.right_bottom() - pad,
        Align2::RIGHT_BOTTOM,
        format!("ACID {}", ph::ARROW_RIGHT),
        bold(10.5),
        ACID,
    );
    let hot = resp.hovered() || resp.dragged();
    // The same puck as the filter pad's
    if hot {
        glow(&p, at, 11.0, TEXT, 0.25);
    }
    p.circle_filled(at, 11.0, mix(PANEL, BG, 0.5));
    p.circle_stroke(at, 11.0, stroke(2.0, TEXT));
    resp.on_hover_cursor(egui::CursorIcon::Crosshair)
}

/// The master EQ as a curve: three points (low, mid, high) to drag up or down, ±12 dB;
/// double-click a point to flatten it
pub fn eq_curve(ui: &mut Ui, bands: &mut [f32; 3], colors: [Color32; 3], width: f32, height: f32) -> Response {
    let (all, mut resp) = ui.allocate_exact_size(vec2(width, height + 18.0), Sense::click_and_drag());
    let rect = Rect::from_min_size(all.min, vec2(width, height));
    let xs = [1.0 / 6.0, 0.5, 5.0 / 6.0].map(|f| rect.left() + f * rect.width());
    let span = rect.height() / 2.0 - 8.0;
    let y_of = |v: f32| rect.center().y - (v - 0.5) * 2.0 * span;
    let nearest = |x: f32| (0..3).min_by(|&a, &b| (xs[a] - x).abs().total_cmp(&(xs[b] - x).abs())).unwrap_or(1);
    // Which point the hand has (it stays the same through a drag)
    let id = resp.id.with("band");
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.double_clicked() {
            bands[nearest(pos.x)] = 0.5;
            resp.mark_changed();
        } else if resp.is_pointer_button_down_on() || resp.dragged() {
            let k = if resp.drag_started() || resp.is_pointer_button_down_on() && !resp.dragged() {
                let k = nearest(pos.x);
                ui.data_mut(|d| d.insert_temp(id, k));
                k
            } else {
                ui.data(|d| d.get_temp(id)).unwrap_or_else(|| nearest(pos.x))
            };
            let v = (0.5 + (rect.center().y - pos.y) / span / 2.0).clamp(0.0, 1.0);
            if (v - bands[k]).abs() > 1e-4 {
                bands[k] = v;
                resp.mark_changed();
            }
        }
    }

    let p = ui.painter_at(all.expand(2.0));
    p.rect_filled(rect, 12.0, Color32::from_black_alpha(80));
    p.rect_stroke(rect, 12.0, stroke(1.0, LINE), egui::StrokeKind::Inside);
    dashed(
        &p,
        pos2(rect.left() + 8.0, rect.center().y),
        pos2(rect.right() - 8.0, rect.center().y),
        alpha(LINE, 0.9),
    );
    // The response: a low shelf, a bell and a high shelf added up
    let g = bands.map(|v| (v - 0.5) * 2.0);
    let curve: Vec<Pos2> = (0..=64)
        .map(|i| {
            let f = i as f32 / 64.0;
            let x = rect.left() + 4.0 + f * (rect.width() - 8.0);
            let s = |d: f32| 1.0 / (1.0 + (-d / 0.06).exp());
            let gain = g[0] * s(1.0 / 6.0 + 0.1 - f) + g[1] * (-((f - 0.5) / 0.16).powi(2)).exp() + g[2] * s(f - 5.0 / 6.0 + 0.1);
            pos2(x, rect.center().y - gain.clamp(-1.0, 1.0) * span)
        })
        .collect();
    p.add(egui::Shape::line(curve, stroke(2.0, alpha(TEXT, 0.55))));
    let hot = resp.hovered() || resp.dragged();
    let names = ["LOW", "MID", "HIGH"];
    for k in 0..3 {
        let at = pos2(xs[k], y_of(bands[k]));
        if hot {
            glow(&p, at, 7.0, colors[k], 0.5);
        }
        p.circle_filled(at, 7.0, colors[k]);
        let db = (bands[k] - 0.5) * 24.0;
        p.text(
            pos2(xs[k], all.bottom()),
            Align2::CENTER_BOTTOM,
            format!("{} {:+.0}", names[k], db).replace("+0", "0").replace("-0", "0"),
            bold(10.5),
            if hot { TEXT } else { DIM },
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}

/// The filter as a pad: left low-pass, right high-pass, the middle off; up adds resonance.
/// Let go and it springs back to off.
/// `engine_pos` is where the engine's filter is (smoothed, or swept by a breakdown) – the
/// puck shows it while no hand is on the pad.
pub fn filter_pad(ui: &mut Ui, pos: &mut f32, res: &mut f32, engine_pos: f32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let held = resp.is_pointer_button_down_on() || resp.dragged();
    let rest_y = rect.center().y + rect.height() * 0.15;
    if let Some(at) = resp.interact_pointer_pos().filter(|_| held) {
        *pos = ((at.x - rect.center().x) / (rect.width() / 2.0 - 12.0)).clamp(-1.0, 1.0);
        *res = ((rest_y - at.y) / (rest_y - rect.top() - 12.0)).clamp(0.0, 1.0);
        resp.mark_changed();
    } else if pos.abs() > 0.0 || *res > 0.0 {
        // Spring back
        *pos *= 0.75;
        *res *= 0.75;
        if pos.abs() < 0.01 {
            *pos = 0.0;
        }
        if *res < 0.01 {
            *res = 0.0;
        }
        resp.mark_changed();
        ui.ctx().request_repaint();
    }

    let p = ui.painter_at(rect.expand(2.0));
    pad_frame(&p, rect);
    dashed(
        &p,
        pos2(rect.center().x, rect.top() + 22.0),
        pos2(rect.center().x, rect.bottom()),
        alpha(TEXT, 0.25),
    );
    let color = if (if held || pos.abs() > 0.01 { *pos } else { engine_pos }) < 0.0 {
        LOW_PASS
    } else {
        HIGH_PASS
    };
    let pad = vec2(10.0, 8.0);
    p.text(rect.center_top() + vec2(0.0, pad.y), Align2::CENTER_TOP, "OFF", bold(10.0), FAINT);
    p.text(
        rect.left_top() + pad,
        Align2::LEFT_TOP,
        format!("RES {}", ph::ARROW_UP),
        bold(10.0),
        DIM,
    );
    p.text(
        rect.left_bottom() + vec2(pad.x, -pad.y),
        Align2::LEFT_BOTTOM,
        format!("{} LOW-PASS", ph::ARROW_LEFT),
        bold(10.0),
        if *pos < -0.01 { LOW_PASS } else { DIM },
    );
    p.text(
        rect.right_bottom() - pad,
        Align2::RIGHT_BOTTOM,
        format!("HIGH-PASS {}", ph::ARROW_RIGHT),
        bold(10.0),
        if *pos > 0.01 { HIGH_PASS } else { DIM },
    );
    let shown = if held || pos.abs() > 0.01 { *pos } else { engine_pos };
    let at = pos2(
        rect.center().x + shown * (rect.width() / 2.0 - 12.0),
        rest_y - *res * (rest_y - rect.top() - 12.0),
    );
    let amount = shown.abs().max(*res);
    if shown.abs() > 0.01 {
        p.line_segment([pos2(rect.center().x, at.y), at], stroke(3.0, alpha(color, 0.6)));
    }
    glow(&p, at, 11.0, color, amount);
    p.circle_filled(at, 11.0, mix(PANEL, BG, 0.5));
    p.circle_stroke(at, 11.0, stroke(2.0, if amount > 0.01 { color } else { TEXT }));
    resp.on_hover_cursor(egui::CursorIcon::Grab)
}

/// A card's frame and its label row; returns the picture area
fn card(p: &egui::Painter, rect: Rect, label: &str, value: &str, color: Color32, hot: bool) -> Rect {
    p.rect_filled(rect, 10.0, Color32::from_black_alpha(80));
    p.rect_stroke(
        rect,
        10.0,
        stroke(1.0, if hot { alpha(color, 0.7) } else { LINE }),
        egui::StrokeKind::Inside,
    );
    let row = rect.bottom() - 11.0;
    p.text(
        pos2(rect.left() + 8.0, row),
        Align2::LEFT_CENTER,
        label,
        bold(10.0),
        if hot { TEXT } else { DIM },
    );
    p.text(pos2(rect.right() - 8.0, row), Align2::RIGHT_CENTER, value, bold(11.0), color);
    Rect::from_min_max(rect.min + vec2(8.0, 8.0), pos2(rect.right() - 8.0, rect.bottom() - 22.0))
}

/// Drag up or down (or scroll) to change a 0…1 value, double-click resets
fn drag_value(ui: &Ui, resp: &mut Response, value: &mut f32, default: f32) {
    let mut v = *value;
    if resp.double_clicked() {
        v = default;
    } else {
        if resp.dragged() {
            v -= resp.drag_delta().y / 120.0;
        }
        if resp.hovered() {
            v += ui.input(|i| i.raw_scroll_delta.y) / 400.0;
        }
    }
    let v = v.clamp(0.0, 1.0);
    if (v - *value).abs() > 1e-5 {
        *value = v;
        resp.mark_changed();
    }
}

/// SPACE as the room it makes: echoes rippling out, wider and longer the more
pub fn space_card(ui: &mut Ui, value: &mut f32, default: f32, color: Color32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    drag_value(ui, &mut resp, value, default);
    let p = ui.painter_at(rect);
    let hot = resp.hovered() || resp.dragged();
    let area = card(&p, rect, "SPACE", &format!("{:.0}", *value * 100.0), color, hot);
    let pic = area;
    let c = pos2(pic.left() + 6.0, pic.center().y);
    p.circle_filled(c, 3.5, color);
    // Rings like echoes: more of them, further out and slower to fade with more space
    let reach = pic.width() - 8.0;
    let n = 1 + (*value * 4.0).round() as usize;
    for k in 0..n {
        let f = (k + 1) as f32 / 5.0;
        let r = 6.0 + f * reach * (0.35 + 0.65 * *value);
        let fade = (1.0 - k as f32 / 5.0) * (0.35 + 0.5 * *value);
        // Only the right half: sound going out into the room
        let pts: Vec<Pos2> = (0..=24)
            .map(|i| {
                let a = -1.2 + 2.4 * i as f32 / 24.0;
                c + vec2(a.cos(), a.sin()) * r
            })
            .filter(|q| pic.expand(1.0).contains(*q))
            .collect();
        if pts.len() > 1 {
            p.add(egui::Shape::line(pts, stroke(1.5, alpha(color, fade))));
        }
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}

/// PUMP as what it does: the mix ducking under four kicks and breathing back
pub fn pump_card(ui: &mut Ui, value: &mut f32, default: f32, color: Color32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    drag_value(ui, &mut resp, value, default);
    let p = ui.painter_at(rect);
    let hot = resp.hovered() || resp.dragged();
    let area = card(&p, rect, "PUMP", &format!("{:.0}", *value * 100.0), color, hot);
    let depth = *value * 0.9;
    let pts: Vec<Pos2> = (0..=80)
        .map(|i| {
            let f = i as f32 / 80.0;
            let phase = (f * 4.0).fract();
            let env = 1.0 - depth * (-phase * 5.0).exp();
            pos2(area.left() + f * area.width(), area.top() + (1.0 - env) * area.height())
        })
        .collect();
    // A soft fill under the curve, column by column (premultiplied: `alpha` would glare)
    for pt in pts.iter().step_by(2) {
        p.line_segment([*pt, pos2(pt.x, area.bottom())], stroke(1.5, color.gamma_multiply(0.14)));
    }
    p.add(egui::Shape::line(pts, stroke(2.0, color)));
    for k in 0..4 {
        let x = area.left() + k as f32 / 4.0 * area.width();
        p.circle_filled(pos2(x, area.bottom()), 2.0, alpha(TEXT, 0.5));
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}

/// DRIVE as what it does: a wave pushed into saturation, the clean one behind it
pub fn drive_card(ui: &mut Ui, value: &mut f32, default: f32, color: Color32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    drag_value(ui, &mut resp, value, default);
    let p = ui.painter_at(rect);
    let hot = resp.hovered() || resp.dragged();
    let area = card(&p, rect, "DRIVE", &format!("{:.0}", *value * 100.0), color, hot);
    let k = 1.0 + *value * 9.0;
    let wave = |shape: &dyn Fn(f32) -> f32| -> Vec<Pos2> {
        (0..=80)
            .map(|i| {
                let f = i as f32 / 80.0;
                let s = (f * std::f32::consts::TAU * 1.5).sin();
                pos2(area.left() + f * area.width(), area.center().y - shape(s) * area.height() / 2.0)
            })
            .collect()
    };
    p.add(egui::Shape::line(wave(&|s| s), stroke(1.0, alpha(TEXT, 0.18))));
    p.add(egui::Shape::line(wave(&|s| (k * s).tanh() / k.tanh()), stroke(2.0, color)));
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}

/// PITCH as a little keyboard: the key bass and synth sit on (0 = C … 11 = B); drag up or
/// down (or scroll), double-click resets
pub fn pitch_card(ui: &mut Ui, semis: &mut u8, default: u8, label: &str, color: Color32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let p = ui.painter_at(rect);
    let hot = resp.hovered() || resp.dragged();
    let area = card(&p, rect, "PITCH", label, color, hot);
    const WHITE: [u8; 7] = [0, 2, 4, 5, 7, 9, 11];
    // Black keys: the semitone and the white key it sits after
    const BLACK: [(u8, usize); 5] = [(1, 0), (3, 1), (6, 3), (8, 4), (10, 5)];
    let kw = area.width() / 7.0;
    let white_rect = |i: usize| Rect::from_min_size(pos2(area.left() + i as f32 * kw, area.top()), vec2(kw - 1.5, area.height()));
    let black_rect = |after: usize| {
        Rect::from_center_size(
            pos2(area.left() + (after + 1) as f32 * kw - 0.75, area.top() + area.height() * 0.3),
            vec2(kw * 0.6, area.height() * 0.6),
        )
    };
    // A slider over the keys: drag up or down (or scroll), a semitone every few pixels
    let mut picked = *semis;
    let id = resp.id.with("acc");
    let mut acc: f32 = ui.data(|d| d.get_temp(id)).unwrap_or(0.0);
    if resp.double_clicked() {
        picked = default;
        acc = 0.0;
    } else {
        if resp.drag_started() {
            acc = 0.0;
        }
        if resp.dragged() {
            acc -= resp.drag_delta().y;
        }
        if resp.hovered() {
            acc += ui.input(|i| i.raw_scroll_delta.y) * 0.4;
        }
        let step = 10.0;
        while acc.abs() >= step {
            picked = (picked as i32 + acc.signum() as i32).clamp(0, 11) as u8;
            acc -= step * acc.signum();
        }
    }
    ui.data_mut(|d| d.insert_temp(id, acc));
    if picked != *semis {
        *semis = picked;
        resp.mark_changed();
    }
    for (i, &s) in WHITE.iter().enumerate() {
        let r = white_rect(i);
        let on = s == *semis;
        p.rect_filled(r, 3.0, if on { color } else { mix(PANEL_HI, Color32::WHITE, 0.55) });
    }
    for &(s, after) in &BLACK {
        let r = black_rect(after);
        let on = s == *semis;
        p.rect_filled(r, 2.0, if on { color } else { mix(BG, Color32::BLACK, 0.3) });
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}
