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

/// A little solar system of a groove: its tracks as tilted orbits, its steps as dots on them,
/// turning by `turn` (radians; 0 for a still picture)
pub fn preview(p: &egui::Painter, r: Rect, song: &Song, turn: f32) {
    let c = r.center();
    let rx = (r.width() * 0.47).min(r.height() * 1.25);
    for (t, track) in song.tracks.iter().enumerate() {
        let f = 0.3 + 0.7 * (t as f32 + 0.5) / TRACKS as f32;
        let (ox, oy) = (rx * f, rx * f * 0.34);
        let color = track_color(Voice::ALL[t]);
        let ring: Vec<_> = (0..=64)
            .map(|i| {
                let a = i as f32 / 64.0 * std::f32::consts::TAU;
                c + vec2(a.cos() * ox, a.sin() * oy)
            })
            .collect();
        p.add(egui::Shape::line(
            ring,
            stroke(1.0, color.gamma_multiply(if track.mute { 0.08 } else { 0.22 })),
        ));
        // Inner orbits turn a little faster, like real ones
        let spin = turn * (1.6 - f);
        for (s, st) in track.steps.iter().enumerate().filter(|(_, st)| st.on) {
            let a = -std::f32::consts::FRAC_PI_2 + s as f32 / STEPS as f32 * std::f32::consts::TAU + spin;
            let at = c + vec2(a.cos() * ox, a.sin() * oy);
            let near = 0.75 + 0.25 * a.sin();
            let rad = (if st.accent { 3.2 } else { 2.3 }) * (rx / 160.0).clamp(0.6, 1.8) * near;
            p.circle_filled(at, rad, color.gamma_multiply(if track.mute { 0.3 } else { near }));
        }
    }
    glow(p, c, rx * 0.06, ACID, 0.3);
    p.circle_filled(c, rx * 0.045, mix(ACID, Color32::WHITE, 0.15));
}

/// A button on the start screen: an icon, a title, a line under it. `primary` is the white
/// one; the others are glass. Hover only lightens – no colour.
#[allow(clippy::too_many_arguments)]
fn button(ui: &mut Ui, r: Rect, id: &str, icon: &str, title: &str, line: &str, primary: bool, enabled: bool) -> bool {
    let resp = ui.interact(r, ui.id().with(id), if enabled { Sense::click() } else { Sense::hover() });
    let hot = enabled && resp.hovered();
    let p = ui.painter();
    let (fill, ink, sub) = if primary {
        (
            if hot { Color32::WHITE } else { mix(TEXT, Color32::WHITE, 0.3) },
            BG,
            mix(BG, PANEL_HI, 0.6),
        )
    } else if !enabled {
        (Color32::from_rgba_premultiplied(4, 4, 5, 4), FAINT, FAINT)
    } else {
        (
            if hot {
                Color32::from_rgba_premultiplied(16, 16, 18, 16)
            } else {
                Color32::from_rgba_premultiplied(8, 8, 9, 8)
            },
            TEXT,
            DIM,
        )
    };
    p.rect_filled(r, 12.0, fill);
    if !primary {
        p.rect_stroke(
            r,
            12.0,
            stroke(1.0, if hot { alpha(TEXT, 0.35) } else { LINE }),
            egui::StrokeKind::Inside,
        );
    }
    p.text(pos2(r.left() + 18.0, r.center().y), Align2::LEFT_CENTER, icon, body(20.0), ink);
    p.text(
        pos2(r.left() + 50.0, r.center().y - 9.0),
        Align2::LEFT_CENTER,
        title,
        bold(14.5),
        ink,
    );
    p.text(
        pos2(r.left() + 50.0, r.center().y + 10.0),
        Align2::LEFT_CENTER,
        line,
        body(11.5),
        sub,
    );
    if hot {
        p.text(
            pos2(r.right() - 16.0, r.center().y),
            Align2::RIGHT_CENTER,
            ph::ARROW_RIGHT,
            body(15.0),
            ink,
        );
    }
    resp.on_hover_cursor(if enabled {
        egui::CursorIcon::PointingHand
    } else {
        egui::CursorIcon::Default
    })
    .clicked()
}

/// A short line about a groove: vibe, mood, tempo
fn about(song: &Song) -> String {
    let mood = song.mood().map_or(String::new(), |k| format!(" · {}", crate::song::MOODS[k].0));
    format!("{}{mood} · {:.0} BPM", song.vibe.name(), song.bpm)
}

/// The start screen; returns what was clicked. `demo` is shown when there's nothing from last time.
pub fn home(ui: &mut Ui, recent: &[Recent], last: Option<&Song>, demo: &Song, update: Option<&str>) -> Option<Action> {
    let mut action = None;
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let t = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint();
    // Space, all of it, with stars drifting by
    p.rect_filled(full, 0.0, SPACE);
    crate::widgets::star_field(ui, &p, full, "home", 0.25, 1.0);

    let margin = (full.width() * 0.07).clamp(32.0, 110.0);
    let content = full.shrink2(vec2(margin, 0.0));
    let hero_h = (full.height() * 0.5).clamp(320.0, 470.0);
    let hero = Rect::from_min_size(
        pos2(content.left(), full.top() + (full.height() * 0.06).max(24.0)),
        vec2(content.width(), hero_h),
    );

    // The solar system on the right, turning slowly: last time's groove, or a demo
    let system = Rect::from_min_max(
        pos2(hero.left() + 380.0f32.min(hero.width() * 0.45), hero.top() - 20.0),
        pos2(hero.right() + 30.0, hero.bottom() + 10.0),
    );
    preview(&p, system, last.unwrap_or(demo), t * 0.12);

    // The left: the name, a line, the buttons
    let col = 360.0f32.min(hero.width() * 0.48);
    let mut y = hero.top() + 10.0;
    p.text(
        pos2(hero.left(), y),
        Align2::LEFT_TOP,
        "TECHNO FROM ANOTHER PLANET",
        bold(11.0),
        alpha(TEXT, 0.5),
    );
    y += 22.0;
    p.text(pos2(hero.left() - 3.0, y), Align2::LEFT_TOP, "SOLARUM", display(68.0), TEXT);
    y += 88.0;
    let intro = p.layout(
        "Paint a beat on an empty sky, or let Solari, the computer, build a whole set.".to_string(),
        body(14.5),
        DIM,
        col,
    );
    let ih = intro.size().y;
    p.galley(pos2(hero.left(), y), intro, DIM);
    y += ih + 26.0;
    let bh = 58.0;
    let gap = 10.0;
    let last_line = last.map(about).unwrap_or_else(|| "Nothing from last time yet".to_string());
    let items: [(&str, &str, &str, &str, bool, bool, Action); 4] = [
        (
            "new",
            ph::PLUS,
            "New project",
            "An empty sky – build it by hand",
            true,
            true,
            Action::New,
        ),
        (
            "continue",
            ph::CLOCK_COUNTER_CLOCKWISE,
            "Continue",
            &last_line,
            false,
            last.is_some(),
            Action::Continue,
        ),
        (
            "open",
            ph::FOLDER_OPEN,
            "Open a project…",
            "A .solarum (or .bunker) file",
            false,
            true,
            Action::Open,
        ),
        (
            "auto",
            ph::ROBOT,
            "Let Solari play",
            "The auto mode builds a set by itself",
            false,
            true,
            Action::Auto,
        ),
    ];
    for (id, icon, title, line, primary, enabled, act) in items {
        let r = Rect::from_min_size(pos2(hero.left(), y), vec2(col, bh));
        if button(ui, r, id, icon, title, line, primary, enabled) {
            action = Some(act);
        }
        y += bh + gap;
    }

    // Recent projects below
    let ry = hero.bottom() + 28.0;
    p.text(
        pos2(content.left(), ry),
        Align2::LEFT_TOP,
        "RECENT PROJECTS",
        bold(11.0),
        alpha(TEXT, 0.5),
    );
    let grid_top = ry + 26.0;
    let footer_h = 46.0;
    if recent.is_empty() {
        p.text(
            pos2(content.left(), grid_top + 4.0),
            Align2::LEFT_TOP,
            "Projects you save or open show up here – File → Save, or ⌘/Ctrl + S.",
            body(13.0),
            DIM,
        );
    } else {
        let gap = 16.0;
        let cols = ((content.width() + gap) / (220.0 + gap)).floor().clamp(2.0, 6.0) as usize;
        let cw = (content.width() - gap * (cols - 1) as f32) / cols as f32;
        let ch = (cw * 0.62 + 52.0).min(200.0);
        let rows = (((full.bottom() - footer_h - grid_top + gap) / (ch + gap)).floor() as usize).max(1);
        for (i, rec) in recent.iter().enumerate().take(cols * rows) {
            let r = Rect::from_min_size(
                pos2(
                    content.left() + (i % cols) as f32 * (cw + gap),
                    grid_top + (i / cols) as f32 * (ch + gap),
                ),
                vec2(cw, ch),
            );
            let resp = ui.interact(r, ui.id().with(("recent", i)), Sense::click());
            let ok = rec.song.is_some();
            let hot = ok && resp.hovered();
            // A card of glass: the picture on top, the name and a line below
            p.rect_filled(
                r,
                14.0,
                if hot {
                    Color32::from_rgba_premultiplied(14, 14, 16, 14)
                } else {
                    Color32::from_rgba_premultiplied(7, 7, 8, 7)
                },
            );
            p.rect_stroke(
                r,
                14.0,
                stroke(1.0, if hot { alpha(TEXT, 0.35) } else { LINE }),
                egui::StrokeKind::Inside,
            );
            let pic = Rect::from_min_max(r.min + vec2(6.0, 6.0), pos2(r.right() - 6.0, r.bottom() - 52.0));
            p.rect_filled(pic, 10.0, Color32::from_black_alpha(140));
            match &rec.song {
                Some(song) => preview(&p.with_clip_rect(pic), pic.shrink(8.0), song, if hot { t * 0.3 } else { 0.0 }),
                None => {
                    p.text(
                        pic.center(),
                        Align2::CENTER_CENTER,
                        format!("{}  File not found", ph::WARNING_CIRCLE),
                        body(12.0),
                        FAINT,
                    );
                }
            }
            let name = fit_text(&p, &rec.name, bold(13.5), cw - 24.0);
            p.text(
                pos2(r.left() + 12.0, r.bottom() - 34.0),
                Align2::LEFT_CENTER,
                name,
                bold(13.5),
                if ok { TEXT } else { FAINT },
            );
            let line = match &rec.song {
                Some(song) => format!("{} · {}", rec.when, about(song)),
                None => "Removed or moved – right-click to forget it".to_string(),
            };
            let line = fit_text(&p, &line, body(11.0), cw - 24.0);
            p.text(pos2(r.left() + 12.0, r.bottom() - 15.0), Align2::LEFT_CENTER, line, body(11.0), DIM);
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

    // The footer: settings and help on the left, the version (and an update) on the right
    let fy = full.bottom() - footer_h / 2.0;
    let mut x = content.left() - 10.0;
    for (icon, label, act) in [
        (ph::GEAR_SIX, "Settings", Action::Settings),
        (ph::QUESTION, "How it works", Action::Help),
    ] {
        let text = format!("{icon}  {label}");
        let galley = p.layout_no_wrap(text.clone(), bold(12.5), TEXT);
        let r = Rect::from_center_size(pos2(x + galley.size().x / 2.0 + 10.0, fy), galley.size() + vec2(20.0, 12.0));
        let resp = ui.interact(r, ui.id().with(("foot", label)), Sense::click());
        let hot = resp.hovered();
        if hot {
            p.rect_filled(r, 8.0, Color32::from_rgba_premultiplied(14, 14, 16, 14));
        }
        p.text(r.center(), Align2::CENTER_CENTER, text, bold(12.5), if hot { TEXT } else { DIM });
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            action = Some(act);
        }
        x = r.right() + 4.0;
    }
    let version = match update {
        Some(u) => u.to_string(),
        None => concat!("Solarum ", env!("CARGO_PKG_VERSION")).to_string(),
    };
    p.text(
        pos2(content.right(), fy),
        Align2::RIGHT_CENTER,
        version,
        body(12.0),
        if update.is_some() { ACID } else { FAINT },
    );
    action
}
