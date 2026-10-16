//! Custom controls: knobs, glowing buttons, the bipolar filter slider.

use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui, Vec2};
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

/// A chunky button with an icon and an optional glow when active
pub fn big_button(ui: &mut Ui, size: Vec2, icon: &str, text: &str, color: Color32, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_big_button(ui, rect, &resp, icon, text, color, active);
    resp
}

pub fn paint_big_button(ui: &Ui, rect: Rect, resp: &Response, icon: &str, text: &str, color: Color32, active: bool) {
    let p = ui.painter();
    let hot = resp.hovered();
    let down = resp.is_pointer_button_down_on();
    let rect = if down { rect.shrink(1.5) } else { rect };
    if active {
        p.rect_filled(rect.expand(4.0), 14.0, alpha(color, 0.12));
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
        stroke(
            1.0,
            if active {
                color
            } else {
                alpha(color, if hot { 0.7 } else { 0.35 })
            },
        ),
        egui::StrokeKind::Inside,
    );
    let fg = if active { BG } else { color };
    let label = if text.is_empty() {
        icon.to_string()
    } else {
        format!("{icon}  {text}")
    };
    p.text(rect.center(), Align2::CENTER_CENTER, label, bold(15.0), fg);
}

/// Small pill toggle (chips)
pub fn chip(ui: &mut Ui, text: &str, on: bool, color: Color32) -> Response {
    let galley = ui.painter().layout_no_wrap(text.to_string(), bold(13.0), TEXT);
    let size = vec2(galley.size().x + 22.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    let hot = resp.hovered();
    if on {
        p.rect_filled(rect, 15.0, alpha(color, 0.16));
        p.rect_stroke(rect, 15.0, stroke(1.2, color), egui::StrokeKind::Inside);
    } else {
        p.rect_filled(rect, 15.0, if hot { PANEL_HI } else { Color32::TRANSPARENT });
        p.rect_stroke(rect, 15.0, stroke(1.0, if hot { FAINT } else { LINE }), egui::StrokeKind::Inside);
    }
    p.text(
        rect.center(),
        Align2::CENTER_CENTER,
        text,
        bold(13.0),
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

/// Bipolar slider −1…1 that springs back to the middle when released
pub fn filter_slider(ui: &mut Ui, value: &mut f32, width: f32) -> Response {
    let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 54.0), Sense::click_and_drag());
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
