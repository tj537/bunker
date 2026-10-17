//! The window: top bar, track list, the orbit (or pads), macros and live controls.

use crate::audio::{self, Audio};
use crate::engine::{self, Cmd, Event, Shared};
use crate::gen;
use crate::midi::{Midi, MidiIn};
use crate::orbit::{self, Live, Paint};
use crate::pads;
use crate::rng::Rng;
use crate::song::{Scale, Song, Vibe, Voice, STEPS, TRACKS};
use crate::theme::*;
use crate::widgets::{self, big_button, chip, glow, heading, knob};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Key, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const SLOTS: usize = 4;
const UNDO_LIMIT: usize = 60;
const SLOT_NAMES: [&str; SLOTS] = ["A", "B", "C", "D"];

/// A stored song (undo step, saved slot) with the current volume and auto-mode settings –
/// those belong to the moment, not to the groove
fn keep_live_settings(mut song: Song, now: &Song) -> Song {
    song.master = now.master;
    song.auto = now.auto;
    song.auto_bars = now.auto_bars;
    song
}

#[derive(Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum View {
    Orbit,
    Pads,
}

enum Export {
    Closed,
    Open { bars: u32 },
    Rendering(Receiver<Result<PathBuf, String>>),
}

pub struct BunkerApp {
    pub song: Song,
    sent: Song,
    tx: Sender<Cmd>,
    events: Receiver<Event>,
    shared: Arc<Shared>,
    audio: Result<Audio, String>,
    midi: Option<MidiIn>,
    midi_rx: Receiver<Midi>,
    midi_tx: Sender<Midi>,
    midi_ports: usize,
    midi_scan_at: f64,
    record: Arc<AtomicBool>,
    pub view: View,
    paint: Paint,
    rng: Rng,

    hit_seen: [u32; TRACKS],
    hit_age: [f32; TRACKS],
    step_seen: u64,
    step_time: f64,
    caption: Option<(String, f64)>,
    filter: f32,
    echo: bool,
    stutter: bool,
    echo_mouse: bool,
    stutter_mouse: bool,
    key_down: [bool; TRACKS],
    export: Export,
    pub help: bool,
    audio_failed: Arc<AtomicBool>,
    audio_device: Option<String>,
    /// False in tests: never touch the sound card or MIDI there
    with_devices: bool,
    bpm_scroll: f32,
    /// Undo history: songs before each edit (grouped when edits come quickly)
    undo: Vec<Song>,
    redo: Vec<Song>,
    last_edit: f64,
    /// Set by undo/redo so the restored song doesn't count as a new edit
    restoring: bool,
    /// Four saved grooves
    pub slots: [Option<Song>; SLOTS],
    slot_now: Option<usize>,
    /// Solo: the soloed track and everyone's mute state before
    solo: Option<(usize, [bool; TRACKS])>,
    /// For screenshots: pretend to play at this position
    pub fake_live: Option<Live>,
}

impl BunkerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        install(&cc.egui_ctx);
        let mut song: Song = cc.storage.and_then(|s| eframe::get_value(s, "song")).unwrap_or_default();
        song.sanitize();
        let view = cc.storage.and_then(|s| eframe::get_value(s, "view")).unwrap_or(View::Orbit);
        let mut app = Self::headless(song, view, true);
        if let Some(mut slots) = cc.storage.and_then(|s| eframe::get_value::<[Option<Song>; SLOTS]>(s, "slots")) {
            for s in slots.iter_mut().flatten() {
                s.sanitize();
            }
            app.slots = slots;
        }
        // First start: show how it works
        let seen: bool = cc.storage.and_then(|s| eframe::get_value(s, "seen_help")).unwrap_or(false);
        app.help = !seen;
        let ctx = cc.egui_ctx.clone();
        app.connect_midi(move || ctx.request_repaint());
        app
    }

    /// Without a window (and optionally without sound) – used by `new` and the screenshot tests
    pub fn headless(song: Song, view: View, with_audio: bool) -> Self {
        let (tx, rx) = channel();
        let (ev_tx, events) = channel();
        let (midi_tx, midi_rx) = channel();
        let shared = Shared::new();
        let audio_failed = Arc::new(AtomicBool::new(false));
        let audio = if with_audio {
            audio::start(song, rx, ev_tx, shared.clone(), audio_failed.clone())
        } else {
            drop((rx, ev_tx));
            Err("Sound is off".into())
        };
        BunkerApp {
            song,
            sent: song,
            tx,
            events,
            shared,
            audio,
            midi: None,
            midi_rx,
            midi_tx,
            midi_ports: 0,
            midi_scan_at: 0.0,
            record: Arc::new(AtomicBool::new(false)),
            view,
            paint: Paint::default(),
            rng: Rng::from_clock(),
            hit_seen: [0; TRACKS],
            hit_age: [9.0; TRACKS],
            step_seen: u64::MAX,
            step_time: 0.0,
            caption: Some(("Press play – or roll a new groove with SHUFFLE".into(), 0.0)),
            filter: 0.0,
            echo: false,
            stutter: false,
            echo_mouse: false,
            stutter_mouse: false,
            key_down: [false; TRACKS],
            export: Export::Closed,
            help: false,
            audio_device: if with_audio { audio::default_device_name() } else { None },
            audio_failed,
            with_devices: with_audio,
            bpm_scroll: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: -10.0,
            restoring: false,
            slots: [None; SLOTS],
            slot_now: None,
            solo: None,
            fake_live: None,
        }
    }

    fn connect_midi(&mut self, wake: impl Fn() + Clone + Send + 'static) {
        if let Some(m) = self.midi.take() {
            m.close();
        }
        self.midi_ports = MidiIn::port_count();
        let m = MidiIn::connect(&self.tx, &self.midi_tx, self.record.clone(), wake);
        if !m.names.is_empty() {
            self.say(format!("{} connected", m.names.join(", ")), 0.0);
        }
        self.midi = Some(m);
    }

    /// Start (or restart) the audio output – after the device changed or failed
    fn restart_audio(&mut self, ctx: &egui::Context, now: f64) {
        let was_playing = self.playing();
        let (tx, rx) = channel();
        let (ev_tx, events) = channel();
        self.audio_failed.store(false, Ordering::Relaxed);
        self.audio = Err("Reconnecting…".into()); // close the old stream first
        self.audio = audio::start(self.song, rx, ev_tx, self.shared.clone(), self.audio_failed.clone());
        self.tx = tx;
        self.events = events;
        self.sent = self.song;
        self.audio_device = audio::default_device_name();
        if was_playing {
            self.send(Cmd::Play);
        }
        if self.filter != 0.0 {
            self.send(Cmd::Filter(self.filter));
        }
        // MIDI talks to the engine directly – reconnect it to the new one
        let c = ctx.clone();
        self.connect_midi(move || c.request_repaint());
        match &self.audio {
            Ok(a) => {
                let name = a.device.clone();
                self.say(format!("Sound now on {name}"), now);
            }
            Err(e) => {
                let e = e.clone();
                self.say(format!("No sound: {e}"), now);
            }
        }
    }

    fn undo(&mut self, now: f64) {
        if let Some(prev) = self.undo.pop() {
            self.redo.push(self.song);
            self.song = keep_live_settings(prev, &self.song);
            self.restoring = true;
            self.say("Undo", now);
        }
    }

    fn redo(&mut self, now: f64) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.song);
            self.song = keep_live_settings(next, &self.song);
            self.restoring = true;
            self.say("Redo", now);
        }
    }

    fn slot_clicked(&mut self, i: usize, save: bool, now: f64) {
        if save || self.slots[i].is_none() {
            self.slots[i] = Some(self.song);
            self.slot_now = Some(i);
            self.say(format!("Groove saved in slot {}", SLOT_NAMES[i]), now);
        } else if let Some(s) = self.slots[i] {
            self.song = keep_live_settings(s, &self.song);
            self.slot_now = Some(i);
            self.say(format!("Slot {} loaded", SLOT_NAMES[i]), now);
        }
    }

    fn toggle_solo(&mut self, i: usize, now: f64) {
        match self.solo {
            Some((t, before)) if t == i => {
                for (tr, m) in self.song.tracks.iter_mut().zip(before) {
                    tr.mute = m;
                }
                self.solo = None;
                self.say("Solo off", now);
            }
            other => {
                let before = other.map_or_else(|| self.song.tracks.map(|t| t.mute), |(_, b)| b);
                for (j, tr) in self.song.tracks.iter_mut().enumerate() {
                    tr.mute = j != i;
                }
                self.solo = Some((i, before));
                self.say(format!("Solo: {}", Voice::ALL[i].name()), now);
            }
        }
    }

    fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }

    fn say(&mut self, text: impl Into<String>, now: f64) {
        self.caption = Some((text.into(), now));
    }

    fn playing(&self) -> bool {
        self.shared.playing.load(Ordering::Relaxed)
    }

    fn toggle_play(&mut self) {
        self.send(if self.playing() { Cmd::Stop } else { Cmd::Play });
        // Show the new state right away (the engine confirms on its next buffer)
        self.shared.playing.store(!self.playing(), Ordering::Relaxed);
    }

    fn shuffle(&mut self, vibe: Vibe, now: f64) {
        let keep_bpm = self.playing() && vibe == self.song.vibe;
        let mut s = gen::generate(vibe, &self.song.macros, &mut self.rng);
        s.master = self.song.master;
        s.auto = self.song.auto;
        s.auto_bars = self.song.auto_bars;
        for (t, old) in s.tracks.iter_mut().zip(&self.song.tracks) {
            t.volume = old.volume;
        }
        if keep_bpm {
            s.bpm = self.song.bpm;
        }
        self.song = s;
        self.say(format!("{} · {} · {} BPM", vibe.name(), self.song.root_name(), self.song.bpm), now);
    }

    fn hit(&mut self, voice: Voice) {
        let record = self.record.load(Ordering::Relaxed);
        self.send(Cmd::Hit { voice, vel: 0.9, record });
    }

    // ───────────── Per-frame state from the engine ─────────────

    fn sync(&mut self, ctx: &egui::Context, now: f64, dt: f32) -> Live {
        while let Ok(Event::Song(song, change)) = self.events.try_recv() {
            self.song = *song;
            self.sent = self.song;
            if let Some(c) = change {
                self.say(c.describe(), now);
            }
        }
        while let Ok(m) = self.midi_rx.try_recv() {
            match m {
                Midi::Macro(i, v) => {
                    let m = &mut self.song.macros;
                    *[&mut m.energy, &mut m.dark, &mut m.acid, &mut m.space][i] = v;
                }
                Midi::Filter(x) => self.filter = x,
                Midi::Transpose(Some(t)) if t != 0 => self.say(format!("Transposed {t:+}"), now),
                _ => {}
            }
        }
        if let Some(live) = &self.fake_live {
            return live.clone();
        }
        for i in 0..TRACKS {
            let n = self.shared.hits[i].load(Ordering::Relaxed);
            if n != self.hit_seen[i] {
                self.hit_seen[i] = n;
                self.hit_age[i] = 0.0;
            } else {
                self.hit_age[i] += dt;
            }
        }
        let playing = self.playing();
        let step = self.shared.step.load(Ordering::Relaxed);
        if step != self.step_seen {
            self.step_seen = step;
            self.step_time = now;
        }
        let step_dur = 15.0 / self.song.bpm as f64;
        let pos = step as f64 + ((now - self.step_time) / step_dur).min(0.999);
        if playing || self.hit_age.iter().any(|&a| a < 1.0) || self.filter != 0.0 {
            ctx.request_repaint();
        }
        Live {
            playing,
            pos,
            hit_age: self.hit_age,
            build: self.shared.build_progress(),
            bar: step / STEPS as u64,
            caption: self.caption.as_ref().map(|(t, at)| (t.clone(), (now - at) as f32)),
        }
    }

    fn keys(&mut self, ctx: &egui::Context, now: f64) {
        // No text fields here: never let a focused button eat Space or Enter
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        // Window lost focus while Q/W was held? Let go, or the effect would stick
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)))) {
            if self.echo {
                self.echo = false;
                self.send(Cmd::Echo(false));
            }
            if self.stutter {
                self.stutter = false;
                self.send(Cmd::Stutter(false));
            }
            self.key_down = [false; TRACKS];
        }
        let tab = ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Tab));
        if tab {
            self.view = if self.view == View::Orbit { View::Pads } else { View::Orbit };
        }
        let events = ctx.input(|i| i.events.clone());
        let pad_keys = [Key::A, Key::S, Key::D, Key::F, Key::G, Key::H, Key::J, Key::K];
        let num_keys = [
            Key::Num1,
            Key::Num2,
            Key::Num3,
            Key::Num4,
            Key::Num5,
            Key::Num6,
            Key::Num7,
            Key::Num8,
        ];
        for e in events {
            let egui::Event::Key {
                key,
                pressed,
                repeat,
                modifiers,
                ..
            } = e
            else {
                continue;
            };
            if pressed && (modifiers.command || modifiers.ctrl) {
                match key {
                    Key::Z if modifiers.shift => self.redo(now),
                    Key::Z => self.undo(now),
                    Key::Y => self.redo(now),
                    _ => {}
                }
                continue;
            }
            if modifiers.command || modifiers.ctrl {
                continue;
            }
            if pressed && key == Key::Escape {
                self.help = false;
                if matches!(self.export, Export::Open { .. }) {
                    self.export = Export::Closed;
                }
                continue;
            }
            if let Some(i) = pad_keys.iter().position(|&k| k == key) {
                if pressed && !repeat {
                    self.hit(Voice::ALL[i]);
                }
                self.key_down[i] = pressed;
                continue;
            }
            match (key, pressed, repeat) {
                (Key::Q, p, false) => {
                    self.stutter = p;
                    self.send(Cmd::Stutter(p));
                }
                (Key::W, p, false) => {
                    self.echo = p;
                    self.send(Cmd::Echo(p));
                }
                _ => {}
            }
            if !pressed || repeat {
                continue;
            }
            if let Some(i) = num_keys.iter().position(|&k| k == key) {
                self.song.tracks[i].mute = !self.song.tracks[i].mute;
                continue;
            }
            match key {
                Key::Space => self.toggle_play(),
                Key::Enter => self.shuffle(self.song.vibe, now),
                Key::B => self.drop_now(now),
                Key::ArrowUp => self.song.bpm = (self.song.bpm + 1.0).min(180.0),
                Key::ArrowDown => self.song.bpm = (self.song.bpm - 1.0).max(90.0),
                Key::R => {
                    let r = !self.record.load(Ordering::Relaxed);
                    self.record.store(r, Ordering::Relaxed);
                }
                _ => {}
            }
        }
    }

    fn drop_now(&mut self, now: f64) {
        if self.playing() {
            self.send(Cmd::Drop);
        } else {
            self.say("Start the beat first – then build up to the drop", now);
        }
    }

    // ───────────── Panels ─────────────

    fn top_bar(&mut self, ui: &mut Ui, live: &Live, now: f64) {
        ui.horizontal_centered(|ui| {
            ui.add_space(6.0);
            // Logo: a warning light that flashes with the kick
            let (r, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
            let k = if live.playing { live.flash(0) } else { 0.0 };
            glow(ui.painter(), r.center(), 7.0, RED, 0.5 + k);
            ui.painter()
                .circle_filled(r.center(), 7.0, mix(alpha(RED, 0.7), Color32::WHITE, k * 0.4));
            ui.label(
                egui::RichText::new("BUNKER")
                    .font(display(30.0))
                    .color(TEXT)
                    .extra_letter_spacing(2.0),
            );
            ui.add_space(18.0);

            for vibe in Vibe::ALL {
                let on = self.song.vibe == vibe;
                if chip(ui, vibe.name(), on, ACID)
                    .on_hover_text(format!("{} – rolls a new groove", vibe.blurb()))
                    .clicked()
                {
                    self.shuffle(vibe, now);
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                if ui
                    .add(egui::Button::new(egui::RichText::new(ph::QUESTION).size(18.0)).frame(false))
                    .on_hover_text("How it works")
                    .clicked()
                {
                    self.help = !self.help;
                }
                let export_btn = ui.add(egui::Button::new(
                    egui::RichText::new(format!("{}  Export", ph::DOWNLOAD_SIMPLE)).font(bold(13.0)),
                ));
                for (icon, tip, is_redo) in [
                    (ph::ARROW_CLOCKWISE, "Redo (⌘/Ctrl + Shift + Z)", true),
                    (ph::ARROW_COUNTER_CLOCKWISE, "Undo (⌘/Ctrl + Z)", false),
                ] {
                    let enabled = if is_redo { !self.redo.is_empty() } else { !self.undo.is_empty() };
                    let b = egui::Button::new(egui::RichText::new(icon).size(18.0)).frame(false);
                    if ui.add_enabled(enabled, b).on_hover_text(tip).clicked() {
                        if is_redo {
                            self.redo(now);
                        } else {
                            self.undo(now);
                        }
                    }
                }
                if export_btn.on_hover_text("Save the beat as a WAV file").clicked() {
                    self.export = Export::Open { bars: 16 };
                }
                let midi_on = self.midi.as_ref().is_some_and(|m| m.is_connected());
                let names = self.midi.as_ref().map(|m| m.names.join(", ")).unwrap_or_default();
                ui.label(
                    egui::RichText::new(ph::PIANO_KEYS)
                        .size(20.0)
                        .color(if midi_on { ACID } else { FAINT }),
                )
                .on_hover_text(if midi_on {
                    format!("MIDI: {names}")
                } else {
                    "No MIDI keyboard connected".into()
                });
                ui.add_space(8.0);
                // View switch
                for (v, icon, tip) in [
                    (View::Pads, ph::GRID_FOUR, "Pads & grid (Tab)"),
                    (View::Orbit, ph::CIRCLES_THREE, "Orbit (Tab)"),
                ] {
                    let on = self.view == v;
                    if chip(ui, icon, on, TEXT).on_hover_text(tip).clicked() {
                        self.view = v;
                    }
                }
                ui.add_space(12.0);
                self.bpm_box(ui);
            });
        });
    }

    fn bpm_box(&mut self, ui: &mut Ui) {
        let (rect, resp) = ui.allocate_exact_size(vec2(104.0, 46.0), Sense::click_and_drag());
        if resp.dragged() {
            let d = resp.drag_delta();
            self.song.bpm = (self.song.bpm + (d.x - d.y) * 0.15).clamp(90.0, 180.0);
        }
        if resp.hovered() {
            let s = ui.input(|i| i.raw_scroll_delta.y);
            let n = orbit::scroll_notches(&mut self.bpm_scroll, s, 24.0);
            if n != 0 {
                self.song.bpm = (self.song.bpm.round() + n as f32).clamp(90.0, 180.0);
            }
        }
        if !resp.dragged() {
            self.song.bpm = self.song.bpm.round();
        }
        if resp.double_clicked() {
            self.song.bpm = 130.0;
        }
        let p = ui.painter();
        let hot = resp.hovered() || resp.dragged();
        p.rect_filled(rect, 10.0, if hot { PANEL_HI } else { PANEL });
        p.rect_stroke(rect, 10.0, stroke(1.0, LINE), egui::StrokeKind::Inside);
        p.text(
            rect.left_center() + vec2(12.0, 0.0),
            Align2::LEFT_CENTER,
            format!("{:.0}", self.song.bpm),
            display(28.0),
            TEXT,
        );
        p.text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "BPM", bold(11.0), DIM);
        resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
            .on_hover_text("Drag or scroll to change the tempo (↑/↓)");
    }

    fn tracks_panel(&mut self, ui: &mut Ui, live: &Live, now: f64) {
        heading(ui, "TRACKS");
        ui.add_space(2.0);
        for (i, voice) in Voice::ALL.into_iter().enumerate() {
            let color = track_color(voice);
            let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
            let soloed = self.solo.is_some_and(|(t, _)| t == i);
            let resp = resp.on_hover_text("Click: solo – hear only this track");
            if resp.clicked() {
                self.toggle_solo(i, now);
            }
            let flash = live.flash(i);
            let muted = self.song.tracks[i].mute;
            let p = ui.painter().clone();
            if resp.hovered() {
                p.rect_filled(row, 10.0, PANEL_HI);
            }
            let dot = row.left_center() + vec2(14.0, -8.0);
            glow(&p, dot, 5.0, color, if muted { 0.0 } else { 0.3 + flash });
            p.circle_filled(dot, 5.0, if muted { FAINT } else { color });
            let name_rect = p.text(
                row.left_top() + vec2(28.0, 6.0),
                Align2::LEFT_TOP,
                voice.name(),
                bold(14.0),
                if soloed {
                    ACID
                } else if muted {
                    FAINT
                } else {
                    TEXT
                },
            );
            if soloed {
                p.text(
                    name_rect.right_center() + vec2(6.0, 0.0),
                    Align2::LEFT_CENTER,
                    "SOLO",
                    bold(9.5),
                    ACID,
                );
            }
            let t = &self.song.tracks[i];
            p.text(
                row.left_top() + vec2(28.0, 25.0),
                Align2::LEFT_TOP,
                format!("{} steps · {} hits", t.len, t.hits()),
                body(11.0),
                FAINT,
            );
            // Buttons on the right
            let mut x = row.right() - 6.0;
            let mut icon_btn = |ui: &mut Ui, icon: &str, tip: &str, on: bool, col: Color32| -> egui::Response {
                let r = Rect::from_min_size(pos2(x - 28.0, row.top() + 4.0), vec2(28.0, 26.0));
                x -= 30.0;
                let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tip);
                let hot = resp.hovered();
                if hot {
                    ui.painter().rect_filled(r, 7.0, mix(PANEL_HI, Color32::WHITE, 0.06));
                }
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    icon,
                    body(16.0),
                    if on {
                        col
                    } else if hot {
                        TEXT
                    } else {
                        DIM
                    },
                );
                resp
            };
            let dice = icon_btn(ui, ph::DICE_FIVE, "New pattern for this track (right-click: clear)", false, ACID);
            if dice.secondary_clicked() {
                let t = &mut self.song.tracks[i];
                for st in t.steps.iter_mut() {
                    *st = crate::song::Step::default();
                }
                self.say(format!("{} cleared", voice.name()), now);
            }
            if dice.clicked() {
                let fresh = gen::track_for(voice, self.song.vibe, self.song.macros.energy, &self.song, &mut self.rng);
                let t = &mut self.song.tracks[i];
                *t = crate::song::Track {
                    mute: false,
                    volume: t.volume,
                    ..fresh
                };
                self.say(format!("New {} pattern", voice.name().to_lowercase()), now);
            }
            if icon_btn(
                ui,
                if muted { ph::SPEAKER_SLASH } else { ph::SPEAKER_HIGH },
                &format!("Mute ({})", i + 1),
                muted,
                RED,
            )
            .clicked()
            {
                self.song.tracks[i].mute = !muted;
            }
            // Length stepper
            let lr = Rect::from_min_size(pos2(x - 34.0, row.top() + 6.0), vec2(34.0, 22.0));
            let lresp = ui
                .allocate_rect(lr, Sense::click())
                .on_hover_text("Steps in this loop: click −1, right-click +1. Fewer steps = polyrhythm");
            ui.painter().rect_stroke(
                lr,
                11.0,
                stroke(1.0, if lresp.hovered() { FAINT } else { LINE }),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                lr.center(),
                Align2::CENTER_CENTER,
                format!("{}", self.song.tracks[i].len),
                bold(11.5),
                DIM,
            );
            let len = &mut self.song.tracks[i].len;
            if lresp.clicked() {
                *len = if *len <= 1 { 16 } else { *len - 1 };
            }
            if lresp.secondary_clicked() {
                *len = if *len >= 16 { 1 } else { *len + 1 };
            }
            // Volume
            let vr = Rect::from_min_size(pos2(row.right() - 104.0, row.bottom() - 17.0), vec2(96.0, 14.0));
            let mut vol = self.song.tracks[i].volume;
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(vr), |ui| {
                widgets::bar_slider(ui, &mut vol, 96.0, color).on_hover_text("Volume");
            });
            self.song.tracks[i].volume = vol;
        }

        ui.add_space(14.0);
        heading(ui, "KEY & GROOVE");
        ui.horizontal(|ui| {
            if ui.small_button(ph::CARET_LEFT).clicked() {
                self.song.root = if self.song.root <= 36 { 47 } else { self.song.root - 1 };
            }
            let label = egui::RichText::new(self.song.root_name()).font(bold(14.0));
            if ui
                .add(egui::Button::new(label).frame(false))
                .on_hover_text("Click for another scale")
                .clicked()
            {
                self.song.scale = match self.song.scale {
                    Scale::Minor => Scale::Phrygian,
                    Scale::Phrygian => Scale::Dorian,
                    Scale::Dorian => Scale::Minor,
                };
            }
            if ui.small_button(ph::CARET_RIGHT).clicked() {
                self.song.root = if self.song.root >= 47 { 36 } else { self.song.root + 1 };
            }
            let tr = self.shared.transpose.load(Ordering::Relaxed);
            if tr != 0 {
                ui.label(egui::RichText::new(format!("{tr:+}")).font(bold(12.0)).color(ACID))
                    .on_hover_text("Transposed by the MIDI keyboard");
            }
        });
        let w = ui.available_width() - 70.0;
        for (label, which) in [("Swing", 0), ("Volume", 1)] {
            ui.horizontal(|ui| {
                ui.add_sized(
                    vec2(58.0, 14.0),
                    egui::Label::new(egui::RichText::new(label).font(body(12.0)).color(DIM)),
                );
                let v = if which == 0 { &mut self.song.swing } else { &mut self.song.master };
                widgets::bar_slider(ui, v, w, if which == 0 { ACID } else { TEXT });
            });
        }
        // Output level
        let level = self.shared.level().min(1.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
        ui.painter().rect_filled(r, 3.0, PANEL_HI);
        let col = if level > 0.94 {
            RED
        } else {
            mix(ACID, Color32::from_rgb(255, 154, 60), level)
        };
        ui.painter()
            .rect_filled(Rect::from_min_size(r.min, vec2(r.width() * level, r.height())), 3.0, col);
    }

    fn live_panel(&mut self, ui: &mut Ui, live: &Live, now: f64) {
        let w = ui.available_width();
        heading(ui, "MACROS");
        // Short window: four small knobs in a row instead of a 2 × 2 block
        let compact = ui.available_height() < 715.0;
        let ks = if compact { (w - 24.0) / 4.0 } else { (w / 2.0 - 8.0).min(118.0) };
        let m = self.song.macros;
        let mut vals = [m.energy, m.dark, m.acid, m.space];
        let defaults = crate::song::Macros::default();
        let def = [defaults.energy, defaults.dark, defaults.acid, defaults.space];
        let names = ["ENERGY", "DARKNESS", "ACID", "SPACE"];
        let tips = [
            "Busier hats, harder drive. Also used when rolling new grooves.",
            "Longer kick, deep rumble, darker sound.",
            "Opens and squeezes the acid filter.",
            "Reverb and echo.",
        ];
        let colors = [
            Color32::from_rgb(255, 154, 60),
            track_color(Voice::Ride),
            ACID,
            track_color(Voice::OpenHat),
        ];
        let (rows, cols) = if compact { (1, 4) } else { (2, 2) };
        for row in 0..rows {
            ui.horizontal(|ui| {
                for col in 0..cols {
                    let i = row * cols + col;
                    knob(ui, &mut vals[i], def[i], names[i], colors[i], ks).on_hover_text(tips[i]);
                }
            });
        }
        let mm = &mut self.song.macros;
        mm.energy = vals[0];
        mm.dark = vals[1];
        mm.acid = vals[2];
        mm.space = vals[3];

        ui.add_space(10.0);
        heading(ui, "AUTO");
        // Auto mode toggle
        let (r, resp) = ui.allocate_exact_size(vec2(w, 54.0), Sense::click());
        let on = self.song.auto;
        {
            let p = ui.painter();
            p.rect_filled(r, 12.0, if on { alpha(ACID, 0.12) } else { PANEL_HI });
            p.rect_stroke(r, 12.0, stroke(1.2, if on { ACID } else { LINE }), egui::StrokeKind::Inside);
            p.text(
                r.left_top() + vec2(14.0, 10.0),
                Align2::LEFT_TOP,
                format!("{}  AUTO MODE", ph::INFINITY),
                bold(14.0),
                if on { ACID } else { TEXT },
            );
            let sub = if on {
                format!("New ideas every {} bars", self.song.auto_bars)
            } else {
                "Let Bunker play a whole set".into()
            };
            p.text(r.left_top() + vec2(14.0, 31.0), Align2::LEFT_TOP, sub, body(11.5), DIM);
            // Switch
            let sw = Rect::from_center_size(r.right_center() - vec2(30.0, 0.0), vec2(36.0, 20.0));
            p.rect_filled(sw, 10.0, if on { ACID } else { LINE });
            let knob_x = if on { sw.right() - 10.0 } else { sw.left() + 10.0 };
            p.circle_filled(pos2(knob_x, sw.center().y), 7.0, if on { BG } else { DIM });
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.song.auto = !on;
            self.say(
                if self.song.auto {
                    "Auto mode on – sit back"
                } else {
                    "Auto mode off"
                },
                now,
            );
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Every").font(body(12.0)).color(DIM));
            for bars in [4u8, 8, 16] {
                if chip(ui, &format!("{bars} bars"), self.song.auto_bars == bars, ACID).clicked() {
                    self.song.auto_bars = bars;
                }
            }
        });
        if big_button(ui, vec2(w, 48.0), ph::SHUFFLE, "SHUFFLE", ACID, false)
            .on_hover_text("A whole new groove in this vibe (Enter)")
            .clicked()
        {
            self.shuffle(self.song.vibe, now);
        }

        ui.add_space(10.0);
        heading(ui, "LIVE");
        let before = self.filter;
        widgets::filter_slider(ui, &mut self.filter, w);
        if self.filter != before {
            self.send(Cmd::Filter(self.filter));
        }
        ui.horizontal(|ui| {
            let half = (w - 8.0) / 2.0;
            for (label, icon, key, is_echo) in [("ECHO", ph::WAVEFORM, "W", true), ("STUTTER", ph::REPEAT, "Q", false)] {
                let (r, resp) = ui.allocate_exact_size(vec2(half, 46.0), Sense::click_and_drag());
                let mouse = resp.is_pointer_button_down_on();
                let (key_held, prev) = if is_echo {
                    (self.echo, &mut self.echo_mouse)
                } else {
                    (self.stutter, &mut self.stutter_mouse)
                };
                if mouse != *prev {
                    *prev = mouse;
                    let on = mouse || key_held;
                    let _ = self.tx.send(if is_echo { Cmd::Echo(on) } else { Cmd::Stutter(on) });
                }
                let color = track_color(if is_echo { Voice::OpenHat } else { Voice::Rim });
                widgets::paint_big_button(ui, r, &resp, icon, label, color, mouse || key_held);
                resp.on_hover_text(format!("Hold ({key})"));
            }
        });
        // The drop
        let (r, resp) = ui.allocate_exact_size(vec2(w, 64.0), Sense::click());
        let building = live.build;
        let p = ui.painter();
        let hot = resp.hovered();
        let pulse = building.map_or(0.0, |b| ((now * (4.0 + b as f64 * 12.0)).sin() * 0.5 + 0.5) as f32);
        widgets::glow_rect(
            p,
            r,
            14.0,
            RED,
            if building.is_some() {
                0.6 + pulse * 0.8
            } else if hot {
                0.5
            } else {
                0.2
            },
        );
        p.rect_filled(
            r,
            14.0,
            if building.is_some() {
                mix(alpha(RED, 0.25), RED, pulse * 0.5)
            } else if hot {
                alpha(RED, 0.22)
            } else {
                alpha(RED, 0.12)
            },
        );
        if let Some(b) = building {
            let fill = Rect::from_min_size(r.min, vec2(r.width() * b, r.height()));
            p.rect_filled(fill, 14.0, alpha(RED, 0.55));
        }
        p.rect_stroke(r, 14.0, stroke(1.5, RED), egui::StrokeKind::Inside);
        let text = if building.is_some() { "BUILDING…" } else { "DROP" };
        p.text(r.center() - vec2(0.0, 6.0), Align2::CENTER_CENTER, text, display(26.0), TEXT);
        p.text(
            r.center() + vec2(0.0, 17.0),
            Align2::CENTER_CENTER,
            "build-up → drop on the next bar (B)",
            body(10.5),
            alpha(TEXT, 0.6),
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.drop_now(now);
        }
    }

    /// Four slots for grooves you like: click an empty one to save, a full one to load
    fn slots_ui(&mut self, ui: &mut Ui, top_right: egui::Pos2, now: f64) {
        let size = 36.0;
        let gap = 6.0;
        let left = top_right.x - SLOTS as f32 * (size + gap) + gap;
        ui.painter().text(
            pos2(left - 10.0, top_right.y + size / 2.0),
            Align2::RIGHT_CENTER,
            "GROOVES",
            bold(10.5),
            FAINT,
        );
        for (i, name) in SLOT_NAMES.into_iter().enumerate() {
            let r = Rect::from_min_size(pos2(left + i as f32 * (size + gap), top_right.y), vec2(size, size));
            let resp = ui.allocate_rect(r, Sense::click());
            let filled = self.slots[i];
            let current = self.slot_now == Some(i);
            let hot = resp.hovered();
            let p = ui.painter();
            match filled {
                Some(song) => {
                    p.rect_filled(r, 9.0, if hot { PANEL_HI } else { mix(PANEL, PANEL_HI, 0.6) });
                    p.rect_stroke(
                        r,
                        9.0,
                        stroke(if current { 1.5 } else { 1.0 }, if current { ACID } else { LINE }),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        r.center() - vec2(0.0, 4.0),
                        Align2::CENTER_CENTER,
                        name,
                        display(16.0),
                        if current { ACID } else { TEXT },
                    );
                    // Tiny dots: which tracks play in this groove
                    for (t, v) in Voice::ALL.into_iter().enumerate() {
                        let tr = song.tracks[t];
                        let x = r.left() + 6.0 + t as f32 * (size - 12.0) / 7.0;
                        let c = if tr.mute || tr.hits() == 0 {
                            alpha(FAINT, 0.5)
                        } else {
                            track_color(v)
                        };
                        p.circle_filled(pos2(x, r.bottom() - 6.0), 1.6, c);
                    }
                }
                None => {
                    p.rect_stroke(r, 9.0, stroke(1.0, if hot { FAINT } else { LINE }), egui::StrokeKind::Inside);
                    p.text(
                        r.center(),
                        Align2::CENTER_CENTER,
                        if hot { ph::PLUS } else { name },
                        body(14.0),
                        if hot { TEXT } else { FAINT },
                    );
                }
            }
            let tip = if filled.is_some() {
                format!("Load groove {} (right-click: save over it or clear)", name)
            } else {
                format!("Save this groove in slot {}", name)
            };
            let resp = resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.slot_clicked(i, false, now);
            }
            if filled.is_some() {
                resp.context_menu(|ui| {
                    if ui.button(format!("{}  Save current groove here", ph::FLOPPY_DISK)).clicked() {
                        self.slot_clicked(i, true, now);
                        ui.close_menu();
                    }
                    if ui.button(format!("{}  Clear slot", ph::TRASH)).clicked() {
                        self.slots[i] = None;
                        if self.slot_now == Some(i) {
                            self.slot_now = None;
                        }
                        ui.close_menu();
                    }
                });
            }
        }
    }

    fn hints(&self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            let keys =
                "Space play  ·  Enter shuffle  ·  A–K pads  ·  1–8 mute  ·  B drop  ·  Q stutter  ·  W echo  ·  R record  ·  Tab view  ·  ⌘/Ctrl Z undo";
            ui.label(egui::RichText::new(keys).font(body(11.5)).color(FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                match &self.audio {
                    Ok(a) => ui.label(
                        egui::RichText::new(format!("{}  {} · {} Hz", ph::SPEAKER_HIGH, a.device, a.sample_rate))
                            .font(body(11.5))
                            .color(FAINT),
                    ),
                    Err(e) => ui.label(egui::RichText::new(format!("{}  {e}", ph::WARNING)).font(body(11.5)).color(RED)),
                };
            });
        });
    }

    fn export_window(&mut self, ctx: &egui::Context, now: f64) {
        let mut close = false;
        let mut start: Option<u32> = None;
        match &mut self.export {
            Export::Closed => return,
            Export::Open { bars } => {
                egui::Window::new("Export WAV")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(egui::RichText::new("How long should the recording be?").color(DIM));
                        ui.horizontal(|ui| {
                            for b in [4u32, 8, 16, 32, 64] {
                                if chip(ui, &format!("{b} bars"), *bars == b, ACID).clicked() {
                                    *bars = b;
                                }
                            }
                        });
                        let secs = *bars as f32 * 4.0 * 60.0 / self.song.bpm;
                        let auto = if self.song.auto { " · auto mode included" } else { "" };
                        ui.label(
                            egui::RichText::new(format!("{:.0} seconds · 44.1 kHz stereo{auto}", secs))
                                .font(body(12.0))
                                .color(FAINT),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(egui::RichText::new(format!("{}  Save…", ph::DOWNLOAD_SIMPLE)).font(bold(14.0)))
                                .clicked()
                            {
                                start = Some(*bars);
                            }
                            if ui.button("Cancel").clicked() {
                                close = true;
                            }
                        });
                    });
            }
            Export::Rendering(rx) => match rx.try_recv() {
                Ok(Ok(path)) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.say(format!("Saved {name}"), now);
                    close = true;
                }
                Ok(Err(e)) => {
                    self.say(format!("Export failed: {e}"), now);
                    close = true;
                }
                Err(_) => {
                    egui::Window::new("Export WAV")
                        .collapsible(false)
                        .resizable(false)
                        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Rendering…");
                            });
                        });
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            },
        }
        if let Some(bars) = start {
            let name = format!(
                "bunker-{}-{:.0}bpm.wav",
                self.song.vibe.name().to_lowercase().replace(' ', "-"),
                self.song.bpm
            );
            if let Some(path) = rfd::FileDialog::new()
                .set_file_name(&name)
                .add_filter("WAV audio", &["wav"])
                .save_file()
            {
                let (tx, rx) = channel();
                let song = self.song;
                std::thread::spawn(move || {
                    let sr = 44_100;
                    let buf = engine::render_offline(&song, bars, sr, 1);
                    let _ = tx.send(crate::wav::write(&path, &buf, sr).map(|_| path).map_err(|e| e.to_string()));
                });
                self.export = Export::Rendering(rx);
            }
        } else if close {
            self.export = Export::Closed;
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.help;
        egui::Window::new("How Bunker works")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(460.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let row = |ui: &mut Ui, icon: &str, title: &str, text: &str| {
                    ui.horizontal_top(|ui| {
                        ui.label(egui::RichText::new(icon).size(20.0).color(ACID));
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(title).font(bold(14.0)));
                            ui.label(egui::RichText::new(text).font(body(12.5)).color(DIM));
                        });
                    });
                    ui.add_space(4.0);
                };
                let max_h = ctx.screen_rect().height() - 180.0;
                egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                    row(ui, ph::PLAY, "Press play", "Space or the button in the middle. Pick a vibe at the top to roll a fresh groove.");
                    row(ui, ph::CIRCLES_THREE, "The orbit", "Each ring is one sound, each dot one step. Click to switch a dot on or off, drag to paint. Right-click: accent (on acid: slide). Scroll on bass/acid dots to change the note. Shift-click: chance.");
                    row(ui, ph::GRID_FOUR, "Pads", "Tab switches to big pads and a classic grid. Hit RECORD and play along – your hits land on the beat.");
                    row(ui, ph::FLOPPY_DISK, "Grooves A–D", "Like what you hear? Click an empty slot (top right) to keep it, click it again later to bring it back. Right-click a full slot to overwrite or clear it.");
                    row(ui, ph::ARROW_COUNTER_CLOCKWISE, "Undo", "⌘/Ctrl + Z takes back any change – a shuffle too. ⌘/Ctrl + Shift + Z redoes it.");
                    row(ui, ph::INFINITY, "Auto mode", "Bunker keeps changing the groove on its own and plays breakdowns and drops every 32 bars.");
                    row(ui, ph::SLIDERS, "Macros & live", "Four big knobs shape the sound. FILTER springs back when you let go. Hold ECHO or STUTTER. DROP builds up and hits on the next bar. Click a track name to solo it, right-click its dice to clear it.");
                    row(ui, ph::PIANO_KEYS, "MIDI keyboard", "Keys below middle C play the sounds. Hold a key from middle C up to transpose bass and acid. Mod wheel = acid, pitch bend = filter, sustain = echo, knobs (CC 70–73) = macros.");
                });
            });
        self.help = open;
    }
}

impl eframe::App for BunkerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, "song", &self.song);
        eframe::set_value(storage, "view", &self.view);
        eframe::set_value(storage, "slots", &self.slots);
        eframe::set_value(storage, "seen_help", &true);
    }
}

impl BunkerApp {
    /// One frame of the whole window
    pub fn ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let dt = ctx.input(|i| i.stable_dt).min(0.1);
        let live = self.sync(ctx, now, dt);
        self.keys(ctx, now);
        if let Some((_, at)) = &self.caption {
            if now - at > 4.5 {
                self.caption = None;
            }
        }

        // MIDI keyboard or headphones plugged in or out? Check every few seconds
        if now > self.midi_scan_at && self.with_devices {
            self.midi_scan_at = now + 3.0;
            if MidiIn::port_count() != self.midi_ports {
                let c = ctx.clone();
                self.connect_midi(move || c.request_repaint());
            }
            let device = audio::default_device_name();
            if self.audio_failed.load(Ordering::Relaxed) || (device.is_some() && device != self.audio_device) {
                self.restart_audio(ctx, now);
            }
        }
        if self.with_devices {
            // Keep the 3-second checks running even when nothing moves
            ctx.request_repaint_after(std::time::Duration::from_secs(3));
        }

        let frame = |fill: Color32, margin: f32| egui::Frame::new().fill(fill).inner_margin(margin);
        egui::TopBottomPanel::top("top")
            .exact_height(66.0)
            .frame(frame(BG, 8.0))
            .show(ctx, |ui| self.top_bar(ui, &live, now));
        egui::TopBottomPanel::bottom("hints")
            .exact_height(28.0)
            .frame(frame(BG, 2.0))
            .show(ctx, |ui| self.hints(ui));
        egui::SidePanel::left("tracks")
            .exact_width(262.0)
            .resizable(false)
            .frame(frame(BG, 14.0))
            .show(ctx, |ui| self.tracks_panel(ui, &live, now));
        egui::SidePanel::right("live")
            .exact_width(286.0)
            .resizable(false)
            .frame(frame(BG, 14.0))
            .show(ctx, |ui| self.live_panel(ui, &live, now));
        egui::CentralPanel::default().frame(frame(BG, 10.0)).show(ctx, |ui| {
            let rect = ui.available_rect_before_wrap();
            // Concrete slab behind the main view
            ui.painter().rect_filled(rect, 22.0, PANEL);
            ui.painter().rect_stroke(rect, 22.0, stroke(1.0, LINE), egui::StrokeKind::Inside);
            let inner = rect.shrink(18.0);
            match self.view {
                View::Orbit => {
                    let out = orbit::orbit(ui, inner, &mut self.song, &live, &mut self.paint);
                    if out.toggle_play {
                        self.toggle_play();
                    }
                }
                View::Pads => {
                    let rec = self.record.load(Ordering::Relaxed);
                    let out = pads::pads(ui, inner, &mut self.song, &live, &mut self.paint, rec, &self.key_down);
                    for v in out.hits {
                        self.hit(v);
                    }
                    if out.toggle_record {
                        self.record.store(!rec, Ordering::Relaxed);
                    }
                }
            }
            self.slots_ui(ui, inner.right_top(), now);
        });

        self.export_window(ctx, now);
        self.help_window(ctx);

        if self.song != self.sent {
            if self.restoring {
                self.restoring = false;
            } else {
                // Quick successive edits (a knob turn, painting dots) are one undo step
                if now - self.last_edit > 0.8 {
                    self.undo.push(self.sent);
                    if self.undo.len() > UNDO_LIMIT {
                        self.undo.remove(0);
                    }
                }
                self.redo.clear();
                self.last_edit = now;
            }
            self.sent = self.song;
            self.send(Cmd::Song(Box::new(self.song)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offscreen::Offscreen;

    fn frame(app: &mut BunkerApp, snap: &mut Offscreen, t: f64) {
        snap.frame(t, |ctx| app.ui(ctx));
    }

    #[test]
    fn undo_and_redo_a_shuffle() {
        let mut snap = Offscreen::new(1360.0, 860.0);
        install(&snap.ctx);
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        frame(&mut app, &mut snap, 1.0);
        let original = app.song;
        app.shuffle(Vibe::Industrial, 2.0);
        frame(&mut app, &mut snap, 2.0);
        let shuffled = app.song;
        assert_ne!(shuffled, original);

        app.undo(3.0);
        frame(&mut app, &mut snap, 3.0);
        assert_eq!(app.song.tracks, original.tracks);
        app.redo(4.0);
        frame(&mut app, &mut snap, 4.0);
        assert_eq!(app.song.tracks, shuffled.tracks);
        // Redo history survives the restore itself, and a new edit clears it
        app.undo(5.0);
        frame(&mut app, &mut snap, 5.0);
        assert_eq!(app.redo.len(), 1);
        app.song.tracks[0].mute = true;
        frame(&mut app, &mut snap, 7.0);
        assert!(app.redo.is_empty());
    }

    #[test]
    fn quick_edits_are_one_undo_step() {
        let mut snap = Offscreen::new(1360.0, 860.0);
        install(&snap.ctx);
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        frame(&mut app, &mut snap, 1.0);
        for (i, t) in [2.0, 2.1, 2.2, 2.3].into_iter().enumerate() {
            app.song.macros.energy = 0.1 * i as f32;
            frame(&mut app, &mut snap, t);
        }
        assert_eq!(app.undo.len(), 1);
    }

    #[test]
    fn slots_save_and_load() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        let first = app.song;
        app.slot_clicked(0, false, 0.0); // empty → save
        assert_eq!(app.slots[0], Some(first));
        app.shuffle(Vibe::Acid, 0.0);
        app.song.master = 0.3;
        app.slot_clicked(0, false, 0.0); // full → load
        assert_eq!(app.song.tracks, first.tracks);
        assert_eq!(app.song.master, 0.3, "volume belongs to the moment");
    }

    #[test]
    fn solo_and_back() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        let before = app.song.tracks.map(|t| t.mute);
        app.toggle_solo(2, 0.0);
        for (i, t) in app.song.tracks.iter().enumerate() {
            assert_eq!(t.mute, i != 2);
        }
        app.toggle_solo(5, 0.0); // switch solo to another track
        assert!(!app.song.tracks[5].mute && app.song.tracks[2].mute);
        app.toggle_solo(5, 0.0);
        assert_eq!(app.song.tracks.map(|t| t.mute), before);
    }
}
