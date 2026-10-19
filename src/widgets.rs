//! Custom controls: knobs, glowing buttons, the bipolar filter slider.

use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui};
use std::f32::consts::PI;

/// Soft glow: stacked translucent circles
pub fn glow(p: &egui::Painter, c: Pos2, r: f32, color: Color32, strength: f32) {
    if strength <= 0.01 {
        return;
    }
    for i in 0..6 {
        let k = i as f32 / 5.0;
        p.circle_filled(c, r * (1.0 + k * 1.4), alpha(color, strength * 0.16 * (1.0 - k)));
    }
}

/// Soft glow around a rounded rectangle
pub fn glow_rect(p: &egui::Painter, r: Rect, radius: f32, color: Color32, strength: f32) {
    if strength <= 0.01 {
        return;
    }
    for i in 1..=5 {
        let k = i as f32;
        p.rect_filled(r.expand(k * 3.0), radius + k * 3.0, alpha(color, strength * 0.07 * (6.0 - k) / 5.0));
    }
}

/// Arc as a polyline from angle `a0` to `a1` (radians, 0 = right, clockwise on screen)
pub fn arc(p: &egui::Painter, c: Pos2, r: f32, a0: f32, a1: f32, stroke: Stroke) {
    let n = ((a1 - a0).abs() * r / 4.0).clamp(4.0, 128.0) as usize;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(Shape::line(pts, stroke));
}

/// A big rotary knob. Drag up/down (or scroll); double-click resets.
pub fn knob(ui: &mut Ui, value: &mut f32, default: f32, label: &str, color: Color32, size: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(size, size + 22.0), Sense::click_and_drag());
    let c = pos2(rect.center().x, rect.top() + size / 2.0);
    let r = size * 0.36;
    if resp.dragged() {
        let d = resp.drag_delta();
        let fine = if ui.input(|i| i.modifiers.shift) { 0.25 } else { 1.0 };
        *value = (*value + (d.x - d.y) * 0.005 * fine).clamp(0.0, 1.0);
        resp.mark_changed();
    }
    if resp.hovered() {
        let scroll = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll != 0.0 {
            *value = (*value + scroll * 0.002).clamp(0.0, 1.0);
            resp.mark_changed();
        }
    }
    if resp.double_clicked() {
        *value = default;
        resp.mark_changed();
    }
    let p = ui.painter();
    let start = PI * 0.75;
    let sweep = PI * 1.5;
    let hot = resp.hovered() || resp.dragged();
    // Body
    p.circle_filled(c, r * 1.25, PANEL_HI);
    p.circle_stroke(c, r * 1.25, stroke(1.0, LINE));
    // Track and value arc
    arc(p, c, r * 1.02, start, start + sweep, stroke(5.0, alpha(Color32::WHITE, 0.07)));
    let end = start + sweep * *value;
    if *value > 0.002 {
        arc(p, c, r * 1.02, start, end, stroke(9.0, alpha(color, 0.18)));
        arc(p, c, r * 1.02, start, end, stroke(5.0, color));
    }
    // Indicator
    let tip = c + vec2(end.cos(), end.sin()) * r * 1.02;
    glow(p, tip, 4.0, color, if hot { 1.0 } else { 0.6 });
    p.circle_filled(tip, 4.5, Color32::WHITE);
    // Value in the middle
    p.text(
        c,
        Align2::CENTER_CENTER,
        format!("{}", (*value * 100.0).round()),
        display(size * 0.22),
        if hot { TEXT } else { DIM },
    );
    p.text(
        pos2(c.x, rect.bottom() - 9.0),
        Align2::CENTER_CENTER,
        label,
        bold(if size < 80.0 { 10.0 } else { 11.5 }),
        DIM,
    );
    resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
}

/// Small pill toggle (chips)
pub fn chip(ui: &mut Ui, text: &str, on: bool, color: Color32) -> Response {
    chip_sized(ui, text, on, color, None)
}

/// A chip with a fixed width (`None`: as wide as its text)
pub fn chip_sized(ui: &mut Ui, text: &str, on: bool, color: Color32, width: Option<f32>) -> Response {
    chip_shaped(ui, text, on, color, width, 30.0, 13.0)
}

/// A smaller chip for rows of presets
pub fn chip_small(ui: &mut Ui, text: &str, on: bool, color: Color32, width: f32) -> Response {
    chip_shaped(ui, text, on, color, Some(width), 24.0, 11.5)
}

fn chip_shaped(ui: &mut Ui, text: &str, on: bool, color: Color32, width: Option<f32>, height: f32, font: f32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), bold(font), TEXT);
    let size = vec2(width.unwrap_or(galley.size().x + 22.0), height);
    let r = height / 2.0;
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    if on {
        p.rect_filled(rect, r, alpha(color, 0.16));
        p.rect_stroke(rect, r, stroke(1.2, color), egui::StrokeKind::Inside);
    } else {
        p.rect_filled(rect, r, if hot { PANEL_HI } else { Color32::TRANSPARENT });
        p.rect_stroke(rect, r, stroke(1.0, if hot { FAINT } else { LINE }), egui::StrokeKind::Inside);
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        bold(font),
        if on {
            color
        } else if hot {
            TEXT
        } else {
            DIM
        },
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A square effect button: icon on top, name below, its key in the corner
#[allow(clippy::too_many_arguments)]
pub fn fx_button(ui: &Ui, rect: Rect, resp: &Response, icon: &str, name: &str, key: &str, color: Color32, active: bool) {
    let p = ui.painter();
    let hot = resp.hovered();
    let rect = if active { rect.shrink(1.5) } else { rect };
    if active {
        glow_rect(p, rect, 12.0, color, 0.9);
    }
    let fill = if active {
        color
    } else if hot {
        PANEL_HI
    } else {
        mix(PANEL, PANEL_HI, 0.5)
    };
    p.rect_filled(rect, 12.0, fill);
    p.rect_stroke(
        rect,
        12.0,
        stroke(1.0, if active { color } else { alpha(color, if hot { 0.8 } else { 0.4 }) }),
        egui::StrokeKind::Inside,
    );
    let fg = if active { BG } else { color };
    let compact = rect.height() < 46.0;
    if compact {
        p.text(
            rect.center() - vec2(4.0, 0.0),
            Align2::CENTER_CENTER,
            format!("{icon} {name}"),
            bold(11.0),
            fg,
        );
    } else {
        p.text(rect.center() - vec2(0.0, 8.0), Align2::CENTER_CENTER, icon, body(18.0), fg);
        p.text(rect.center() + vec2(0.0, 12.0), Align2::CENTER_CENTER, name, bold(11.0), fg);
    }
    p.text(
        rect.right_top() + vec2(-7.0, 5.0),
        Align2::RIGHT_TOP,
        key,
        bold(9.5),
        if active { alpha(BG, 0.6) } else { alpha(TEXT, 0.35) },
    );
}

/// Bipolar slider −1…1 that springs back to the middle when released
pub fn filter_slider(ui: &mut Ui, value: &mut f32, width: f32, height: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let track = Rect::from_center_size(rect.center() - vec2(0.0, 6.0), vec2(width - 24.0, 8.0));
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.is_pointer_button_down_on() {
            *value = (((pos.x - track.left()) / track.width()) * 2.0 - 1.0).clamp(-1.0, 1.0);
            resp.mark_changed();
        }
    }
    if !resp.is_pointer_button_down_on() && !resp.dragged() && value.abs() > 0.0 {
        // Spring back
        *value *= 0.75;
        if value.abs() < 0.01 {
            *value = 0.0;
        }
        resp.mark_changed();
        ui.ctx().request_repaint();
    }
    let p = ui.painter();
    p.rect_filled(track, 4.0, PANEL_HI);
    let mid = track.center().x;
    let x = mid + *value * track.width() / 2.0;
    let color = if *value < 0.0 {
        Color32::from_rgb(69, 200, 255)
    } else {
        Color32::from_rgb(255, 154, 60)
    };
    if value.abs() > 0.01 {
        let r = Rect::from_x_y_ranges(mid.min(x)..=mid.max(x), track.y_range());
        p.rect_filled(r, 4.0, color);
    }
    p.line_segment([pos2(mid, track.top() - 5.0), pos2(mid, track.bottom() + 5.0)], stroke(1.5, FAINT));
    let knob = pos2(x, track.center().y);
    glow(p, knob, 8.0, color, value.abs());
    p.circle_filled(knob, 10.0, TEXT);
    p.circle_filled(knob, 4.0, if value.abs() > 0.01 { color } else { FAINT });
    let y = rect.bottom() - 9.0;
    p.text(
        pos2(track.left(), y),
        Align2::LEFT_CENTER,
        "LOW-PASS",
        bold(10.5),
        if *value < -0.05 { color } else { FAINT },
    );
    p.text(pos2(mid, y), Align2::CENTER_CENTER, "FILTER", bold(10.5), DIM);
    p.text(
        pos2(track.right(), y),
        Align2::RIGHT_CENTER,
        "HIGH-PASS",
        bold(10.5),
        if *value > 0.05 { color } else { FAINT },
    );
    resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
}

pub struct EnergyOut {
    pub changed: bool,
    pub curve_clicked: bool,
    pub locked_clicked: bool,
}

/// The energy fader: drag to strip the groove down or build it up. Dots below show
/// where each track comes in. `curve`: None = auto mode off, Some(on) = the auto
/// mode's energy curve is (not) driving it – the pill switches it. The red end is the
/// drop zone: pushed in there by hand, the energy starts a build-up.
/// `locked`: Bunker steers it (auto mode with AUTO FX) – a slim bar that only shows where the set is.
pub fn energy_fader(ui: &mut Ui, value: &mut f32, width: f32, height: f32, curve: Option<bool>, locked: bool) -> EnergyOut {
    if locked {
        return energy_shown(ui, *value, width, height);
    }
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click_and_drag());
    let mut out = EnergyOut {
        changed: false,
        curve_clicked: false,
        locked_clicked: false,
    };
    let p = ui.painter();
    let top = rect.top() + 8.0;
    p.text(pos2(rect.left(), top), Align2::LEFT_CENTER, "ENERGY", bold(10.5), DIM);
    // The curve pill
    let pill = Rect::from_min_size(pos2(rect.right() - 62.0, top - 9.0), vec2(62.0, 18.0));
    if let Some(on) = curve {
        let hot = ui.rect_contains_pointer(pill);
        p.rect_filled(
            pill,
            9.0,
            if on {
                alpha(ACID, 0.18)
            } else if hot {
                PANEL_HI
            } else {
                Color32::TRANSPARENT
            },
        );
        p.rect_stroke(pill, 9.0, stroke(1.0, if on { ACID } else { LINE }), egui::StrokeKind::Inside);
        p.text(
            pill.center(),
            Align2::CENTER_CENTER,
            if on { "CURVE" } else { "BY HAND" },
            bold(9.5),
            if on { ACID } else { DIM },
        );
    }
    let e = *value;
    p.text(
        pos2(if curve.is_some() { pill.left() - 8.0 } else { rect.right() }, top),
        Align2::RIGHT_CENTER,
        format!("{:.0}", e * 100.0),
        bold(12.0),
        TEXT,
    );
    let track = Rect::from_min_size(pos2(rect.left() + 6.0, top + 14.0), vec2(width - 12.0, 10.0));
    let on_pill = curve.is_some() && resp.interact_pointer_pos().is_some_and(|p| pill.contains(p));
    if resp.clicked() && on_pill {
        out.curve_clicked = true;
    } else if let Some(pos) = resp.interact_pointer_pos() {
        if (resp.dragged() || resp.is_pointer_button_down_on()) && !on_pill {
            let v = ((pos.x - track.left()) / track.width()).clamp(0.0, 1.0);
            if (v - *value).abs() > 1e-4 {
                *value = v;
                out.changed = true;
            }
        }
    }
    let p = ui.painter();
    let cool = Color32::from_rgb(69, 200, 255);
    let hot_col = mix(cool, RED, *value);
    p.rect_filled(track, 5.0, PANEL_HI);
    // The break zone and the drop zone
    let break_x = track.left() + track.width() * crate::engine::BREAK_ZONE;
    p.rect_filled(Rect::from_min_max(track.min, pos2(break_x, track.bottom())), 5.0, alpha(cool, 0.22));
    let zone_x = track.left() + track.width() * crate::engine::DROP_ZONE;
    p.rect_filled(Rect::from_min_max(pos2(zone_x, track.top()), track.max), 5.0, alpha(RED, 0.35));
    let fill = Rect::from_min_max(track.min, pos2(track.left() + track.width() * *value, track.bottom()));
    p.rect_filled(fill, 5.0, alpha(hot_col, 0.85));
    let knob = pos2(fill.right(), track.center().y);
    glow(p, knob, 7.0, hot_col, 0.6);
    p.circle_filled(knob, 8.0, TEXT);
    let y = track.bottom() + 1.0;
    if height > 52.0 {
        p.text(
            pos2(track.right(), y + 9.0),
            Align2::RIGHT_CENTER,
            "drop",
            bold(9.5),
            alpha(RED, 0.8),
        );
        p.text(
            pos2(track.left(), y + 9.0),
            Align2::LEFT_CENTER,
            "break",
            bold(9.5),
            alpha(cool, 0.8),
        );
        p.text(
            pos2(track.left() + track.width() * 0.6, y + 9.0),
            Align2::CENTER_CENTER,
            "groove",
            bold(9.5),
            FAINT,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal).on_hover_text(
        "Energy is the arrangement – it changes what plays, not how loud. ← → on the keyboard.\n\
         • Break (blue, under 30): no kick, no bass, the sound closes and opens into the reverb.\n\
         • Groove: kick, bass, open hat and ride come in one by one.\n\
         • Drop (red, 90 and up): the build-up starts on the next bar, the drop two bars later.\n\
         So pull it down, bring it up slowly, and you play a whole track with one fader.\n\
         In auto mode and songs CURVE lets Bunker move it; touch the fader and break and drop are yours.",
    );
    out
}

/// The energy while Bunker steers it: a slim bar with the break and drop zones, no knob
fn energy_shown(ui: &mut Ui, value: f32, width: f32, height: f32) -> EnergyOut {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::click());
    let p = ui.painter();
    let top = rect.top() + 8.0;
    p.text(pos2(rect.left(), top), Align2::LEFT_CENTER, "ENERGY", bold(10.5), DIM);
    p.text(
        pos2(rect.right(), top),
        Align2::RIGHT_CENTER,
        format!("AUTO · {:.0}", value * 100.0),
        bold(10.5),
        ACID,
    );
    let cool = Color32::from_rgb(69, 200, 255);
    let track = Rect::from_min_size(pos2(rect.left() + 6.0, top + 12.0), vec2(width - 12.0, 4.0));
    let at = |e: f32| track.left() + track.width() * e;
    p.rect_filled(track, 2.0, PANEL_HI);
    p.rect_filled(
        Rect::from_min_max(track.min, pos2(at(crate::engine::BREAK_ZONE), track.bottom())),
        2.0,
        alpha(cool, 0.22),
    );
    p.rect_filled(
        Rect::from_min_max(pos2(at(crate::engine::DROP_ZONE), track.top()), track.max),
        2.0,
        alpha(RED, 0.35),
    );
    let fill = Rect::from_min_max(track.min, pos2(at(value), track.bottom()));
    p.rect_filled(fill, 2.0, alpha(mix(cool, RED, value), 0.8));
    let clicked = resp
        .on_hover_text("Bunker steers the energy now – breaks and drops by itself.\nSwitch AUTO FX off to take it over.")
        .clicked();
    EnergyOut {
        changed: false,
        curve_clicked: false,
        locked_clicked: clicked,
    }
}

/// Thin horizontal value bar (track volume, swing, master)
pub fn bar_slider(ui: &mut Ui, value: &mut f32, width: f32, color: Color32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 14.0), Sense::click_and_drag());
    if let Some(pos) = resp.interact_pointer_pos() {
        if resp.dragged() || resp.clicked() {
            *value = ((pos.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            resp.mark_changed();
        }
    }
    let p = ui.painter();
    let bar = Rect::from_center_size(rect.center(), vec2(rect.width(), 4.0));
    p.rect_filled(bar, 2.0, PANEL_HI);
    let fill = Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * *value, bar.bottom()));
    p.rect_filled(fill, 2.0, alpha(color, if resp.hovered() || resp.dragged() { 1.0 } else { 0.7 }));
    if resp.hovered() || resp.dragged() {
        p.circle_filled(pos2(fill.right(), bar.center().y), 5.0, TEXT);
    }
    resp.on_hover_cursor(egui::CursorIcon::ResizeHorizontal)
}

/// Section heading in small caps
pub fn heading(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(bold(11.0)).color(FAINT).extra_letter_spacing(1.6));
}
