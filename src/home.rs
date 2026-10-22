//! The start screen: what Solarum shows when it opens, and behind the logo – a new project,
//! the last session, opening a file, letting Solari play, the recent projects as cards with
//! a little picture of each groove, settings and help

use crate::file::SolarumFile;
use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::{fit_text, glow};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::path::PathBuf;

/// A project from the recent list, read once when the start screen opens
pub struct Recent {
    pub path: PathBuf,
    pub name: String,
    /// When it was last saved, in words ("today", "3 days ago")
    pub when: String,
    /// None: the file is gone or can't be read
    pub song: Option<Song>,
}

/// What a click on the start screen asks for
pub enum Action {
    New,
    Continue,
    Open,
    Auto,
    Recent(usize),
    Forget(usize),
    Settings,
    Help,
}

/// Read the recent projects (their grooves for the pictures, and when they were saved)
pub fn load_recent(paths: &[PathBuf]) -> Vec<Recent> {
    paths
        .iter()
        .map(|path| {
            let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let when = std::fs::metadata(path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| match d.as_secs() / 86_400 {
                    0 => "today".to_string(),
                    1 => "yesterday".to_string(),
                    n => format!("{n} days ago"),
                })
                .unwrap_or_else(|| "missing".to_string());
            let song = SolarumFile::load(path).ok().map(|f| f.song);
            Recent {
                path: path.clone(),
                name,
                when,
                song,
            }
        })
        .collect()
}

/// A little solar system of a groove: its tracks as tilted orbits, its steps as dots on them
pub fn preview(p: &egui::Painter, r: Rect, song: &Song) {
    let c = r.center();
    let rx = r.width() * 0.46;
    for (t, track) in song.tracks.iter().enumerate() {
        let f = 0.3 + 0.7 * (t as f32 + 0.5) / TRACKS as f32;
        let (ox, oy) = (rx * f, rx * f * 0.34);
        let color = track_color(Voice::ALL[t]);
        let ring: Vec<_> = (0..=40)
            .map(|i| {
                let a = i as f32 / 40.0 * std::f32::consts::TAU;
                c + vec2(a.cos() * ox, a.sin() * oy)
            })
            .collect();
        p.add(egui::Shape::line(
            ring,
            stroke(0.8, color.gamma_multiply(if track.mute { 0.1 } else { 0.3 })),
        ));
        for (s, st) in track.steps.iter().enumerate().filter(|(_, st)| st.on) {
            let a = -std::f32::consts::FRAC_PI_2 + s as f32 / STEPS as f32 * std::f32::consts::TAU;
            let at = c + vec2(a.cos() * ox, a.sin() * oy);
            p.circle_filled(
                at,
                if st.accent { 2.4 } else { 1.7 },
                color.gamma_multiply(if track.mute { 0.3 } else { 1.0 }),
            );
        }
    }
    glow(p, c, rx * 0.1, ACID, 0.4);
    p.circle_filled(c, rx * 0.07, ACID);
}

/// One big button: an icon, a title, a line under it
fn action_card(ui: &mut Ui, size: egui::Vec2, icon: &str, title: &str, line: &str, accent: Color32, enabled: bool) -> bool {
    let (r, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    let hot = enabled && resp.hovered();
    let p = ui.painter();
    p.rect_filled(
        r,
        14.0,
        if hot {
            mix(PANEL_HI, accent, 0.12)
        } else {
            mix(PANEL_HI, TEXT, 0.04)
        },
    );
    p.rect_stroke(
        r,
        14.0,
        stroke(1.0, if hot { accent } else { alpha(TEXT, 0.16) }),
        egui::StrokeKind::Inside,
    );
    let ink = if enabled { accent } else { FAINT };
    p.text(r.left_top() + vec2(16.0, 14.0), Align2::LEFT_TOP, icon, body(24.0), ink);
    p.text(
        r.left_top() + vec2(16.0, 50.0),
        Align2::LEFT_TOP,
        title,
        bold(15.0),
        if enabled { TEXT } else { FAINT },
    );
    p.text(
        r.left_top() + vec2(16.0, 72.0),
        Align2::LEFT_TOP,
        line,
        body(11.5),
        if enabled { DIM } else { FAINT },
    );
    resp.on_hover_cursor(if enabled {
        egui::CursorIcon::PointingHand
    } else {
        egui::CursorIcon::Default
    })
    .clicked()
}

/// The start screen; returns what was clicked
pub fn home(ui: &mut Ui, recent: &[Recent], last: Option<&Song>, update: Option<&str>) -> Option<Action> {
    let mut action = None;
    let full = ui.max_rect();
    let w = (full.width() - 64.0).min(980.0);
    let left = full.center().x - w / 2.0;
    let mut y = full.top() + (full.height() * 0.08).max(28.0);
    let p = ui.painter().clone();

    // The logo and a line
    p.text(pos2(left, y), Align2::LEFT_TOP, "SOLARUM", display(54.0), TEXT);
    p.text(
        pos2(left + 2.0, y + 70.0),
        Align2::LEFT_TOP,
        "Techno from another planet – paint a beat on an empty sky, or let Solari play a set.",
        body(14.0),
        DIM,
    );
    p.text(
        pos2(left + w, y + 8.0),
        Align2::RIGHT_TOP,
        concat!("v", env!("CARGO_PKG_VERSION")),
        body(12.0),
        FAINT,
    );
    y += 108.0;

    // The big four
    let gap = 14.0;
    let cw = (w - 3.0 * gap) / 4.0;
    let size = vec2(cw, 100.0);
    let cards: [(&str, &str, &str, Color32, bool, Action); 4] = [
        (ph::PLUS, "New project", "An empty sky – build it by hand", ACID, true, Action::New),
        (
            ph::CLOCK_COUNTER_CLOCKWISE,
            "Continue",
            if last.is_some() {
                "Where you left off last time"
            } else {
                "Nothing from last time yet"
            },
            TEXT,
            last.is_some(),
            Action::Continue,
        ),
        (ph::FOLDER_OPEN, "Open…", "A .solarum (or .bunker) file", TEXT, true, Action::Open),
        (ph::ROBOT, "Let Solari play", "The auto mode builds a set", ACID, true, Action::Auto),
    ];
    for (i, (icon, title, line, accent, enabled, act)) in cards.into_iter().enumerate() {
        let r = Rect::from_min_size(pos2(left + i as f32 * (cw + gap), y), size);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
        if action_card(&mut child, size, icon, title, line, accent, enabled) {
            action = Some(act);
        }
    }
    y += size.y + 34.0;

    // Recent projects
    p.text(pos2(left, y), Align2::LEFT_TOP, "RECENT PROJECTS", bold(11.0), FAINT);
    y += 22.0;
    if recent.is_empty() {
        p.text(
            pos2(left, y + 6.0),
            Align2::LEFT_TOP,
            "Nothing here yet – projects you save or open show up here (File → Save).",
            body(13.0),
            DIM,
        );
    } else {
        let cols = 4;
        let rw = (w - (cols - 1) as f32 * gap) / cols as f32;
        let rh = 150.0;
        for (i, rec) in recent.iter().enumerate() {
            let (col, row) = (i % cols, i / cols);
            let r = Rect::from_min_size(pos2(left + col as f32 * (rw + gap), y + row as f32 * (rh + gap)), vec2(rw, rh));
            if r.bottom() > full.bottom() - 60.0 {
                break;
            }
            let resp = ui.interact(r, ui.id().with(("recent", i)), Sense::click());
            let hot = resp.hovered();
            let ok = rec.song.is_some();
            p.rect_filled(
                r,
                14.0,
                if hot {
                    mix(PANEL_HI, TEXT, 0.09)
                } else {
                    mix(PANEL_HI, TEXT, 0.04)
                },
            );
            p.rect_stroke(
                r,
                14.0,
                stroke(1.0, if hot { alpha(TEXT, 0.4) } else { alpha(TEXT, 0.14) }),
                egui::StrokeKind::Inside,
            );
            let pic = Rect::from_min_max(r.min + vec2(8.0, 8.0), pos2(r.right() - 8.0, r.bottom() - 46.0));
            p.rect_filled(pic, 10.0, SPACE);
            if let Some(song) = &rec.song {
                preview(&p.with_clip_rect(pic), pic.shrink(6.0), song);
            } else {
                p.text(pic.center(), Align2::CENTER_CENTER, "File not found", body(11.0), FAINT);
            }
            p.text(
                pos2(r.left() + 12.0, r.bottom() - 30.0),
                Align2::LEFT_CENTER,
                fit_text(&p, &rec.name, bold(13.0), rw - 24.0),
                bold(13.0),
                if ok { TEXT } else { FAINT },
            );
            p.text(
                pos2(r.left() + 12.0, r.bottom() - 13.0),
                Align2::LEFT_CENTER,
                &rec.when,
                body(11.0),
                DIM,
            );
            let resp = resp.on_hover_text(rec.path.display().to_string()).on_hover_cursor(if ok {
                egui::CursorIcon::PointingHand
            } else {
                egui::CursorIcon::Default
            });
            if resp.clicked() && ok {
                action = Some(Action::Recent(i));
            }
            resp.context_menu(|ui| {
                if ui.button(format!("{}  Remove from the list", ph::X)).clicked() {
                    action = Some(Action::Forget(i));
                    ui.close_menu();
                }
            });
        }
    }

    // The footer: settings, help, the update
    let fy = full.bottom() - 34.0;
    let mut x = left;
    for (icon, label, act) in [
        (ph::GEAR_SIX, "Settings", Action::Settings),
        (ph::QUESTION, "How it works", Action::Help),
    ] {
        let text = format!("{icon}  {label}");
        let galley = p.layout_no_wrap(text.clone(), bold(13.0), TEXT);
        let r = Rect::from_min_size(pos2(x, fy), galley.size() + vec2(20.0, 12.0));
        let resp = ui.interact(r, ui.id().with(("foot", label)), Sense::click());
        if resp.hovered() {
            p.rect_filled(r, 8.0, PANEL_HI);
        }
        p.text(
            r.center(),
            Align2::CENTER_CENTER,
            text,
            bold(13.0),
            if resp.hovered() { TEXT } else { DIM },
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            action = Some(act);
        }
        x = r.right() + 8.0;
    }
    if let Some(update) = update {
        p.text(pos2(left + w, fy + 12.0), Align2::RIGHT_CENTER, update, bold(12.0), ACID);
    }
    action
}
