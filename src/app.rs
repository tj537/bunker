//! The window: top bar with the vibes, track list, the orbit (or pads) with the scenes
//! below, sound knobs, auto mode and live effects.

use crate::audio::{self, Audio};
use crate::engine::{self, Chain, Cmd, Event, Fx, Shared};
use crate::file::{self, BunkerFile};
use crate::gen;
use crate::midi::{Midi, MidiIn};
use crate::orbit::{self, Live, Paint};
use crate::pads;
use crate::rng::Rng;
use crate::song::{Scale, Song, Track, Vibe, Voice, PROGRESSIONS, STEPS, TRACKS};
use crate::theme::*;
use crate::update::{self, Release};
use crate::visual;
use crate::widgets::{self, chip, chip_sized, glow, heading, knob};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Key, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

pub const SLOTS: usize = 4;
const UNDO_LIMIT: usize = 60;
const SLOT_NAMES: [&str; SLOTS] = ["A", "B", "C", "D"];
const FX_KEYS: [Key; 6] = [Key::Q, Key::W, Key::E, Key::X, Key::C, Key::V];

/// A stored song (undo step, saved slot) with the current volume and auto-mode settings –
/// those belong to the moment, not to the groove
fn keep_live_settings(mut song: Song, now: &Song) -> Song {
    song.master = now.master;
    song.auto = now.auto;
    song.auto_bars = now.auto_bars;
    song.curve = now.curve;
    song.rise = now.rise;
    song.eq = now.eq;
    song
}

fn fx_icon(fx: Fx) -> &'static str {
    match fx {
        Fx::Stutter => ph::REPEAT,
        Fx::Echo => ph::WAVEFORM,
        Fx::Wash => ph::DROP,
        Fx::Crush => ph::CPU,
        Fx::Gate => ph::BARCODE,
        Fx::Tape => ph::VINYL_RECORD,
    }
}

fn fx_color(fx: Fx) -> Color32 {
    match fx {
        Fx::Stutter => track_color(Voice::Rim),
        Fx::Echo => track_color(Voice::OpenHat),
        Fx::Wash => track_color(Voice::Ride),
        Fx::Crush => track_color(Voice::Clap),
        Fx::Gate => track_color(Voice::Hat),
        Fx::Tape => track_color(Voice::Bass),
    }
}

/// 303 lines and stab/pluck/hoover lines are written differently
fn synth_family(sound: u8) -> u8 {
    (sound >= 2) as u8 + (sound >= 3) as u8 + (sound >= 4) as u8
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
    /// Live effects held with the keyboard / the mouse, and what the engine was told
    pub fx_key: [bool; 6],
    fx_mouse: [bool; 6],
    fx_sent: [bool; 6],
    key_down: [bool; TRACKS],
    export: Export,
    pub help: bool,
    audio_failed: Arc<AtomicBool>,
    audio_device: Option<String>,
    /// False in tests: never touch the sound card or MIDI there
    with_devices: bool,
    bpm_scroll: f32,
    sound_scroll: f32,
    /// Undo history: songs before each edit (grouped when edits come quickly)
    undo: Vec<Song>,
    redo: Vec<Song>,
    last_edit: f64,
    /// Set by undo/redo so the restored song doesn't count as a new edit
    restoring: bool,
    /// Four scenes; the active one (`slot_now`) keeps every edit
    pub slots: [Option<Song>; SLOTS],
    pub slot_now: Option<usize>,
    /// Song mode: the scenes play in turn
    pub chain: bool,
    /// Song mode: bars per scene, and whether to build up and drop into it
    pub scene_bars: [u8; SLOTS],
    pub scene_drop: [bool; SLOTS],
    /// Visualizer: the whole window is a light show
    pub visual: bool,
    /// The right panel shows the master section instead of the sound knobs
    pub master_tab: bool,
    last_preview: f64,
    /// Solo: the soloed track and everyone's mute state before
    solo: Option<(usize, [bool; TRACKS])>,
    /// Play this track once the engine has the new song (sound preview)
    preview: Option<Voice>,
    /// Which sound knob the mouse is on – its description shows below
    knob_hint: Option<usize>,
    update_rx: Option<Receiver<Release>>,
    pub new_version: Option<Release>,
    pub check_updates: bool,
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
        app.slot_now = cc
            .storage
            .and_then(|s| eframe::get_value::<Option<usize>>(s, "slot_now"))
            .flatten()
            .filter(|&i| i < SLOTS && app.slots[i].is_some());
        if let Some(b) = cc.storage.and_then(|s| eframe::get_value::<[u8; SLOTS]>(s, "scene_bars")) {
            app.scene_bars = b.map(|b| b.clamp(1, 64));
        }
        if let Some(d) = cc.storage.and_then(|s| eframe::get_value::<[bool; SLOTS]>(s, "scene_drop")) {
            app.scene_drop = d;
        }
        // First start: show how it works
        let seen: bool = cc.storage.and_then(|s| eframe::get_value(s, "seen_help")).unwrap_or(false);
        app.help = !seen;
        app.check_updates = cc.storage.and_then(|s| eframe::get_value(s, "check_updates")).unwrap_or(true);
        let ctx = cc.egui_ctx.clone();
        if app.check_updates {
            let c = ctx.clone();
            app.update_rx = Some(update::check(move || c.request_repaint()));
        }
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
            caption: Some(("Press play – or pick a vibe above to roll a new groove".into(), 0.0)),
            filter: 0.0,
            fx_key: [false; 6],
            fx_mouse: [false; 6],
            fx_sent: [false; 6],
            key_down: [false; TRACKS],
            export: Export::Closed,
            help: false,
            audio_device: if with_audio { audio::default_device_name() } else { None },
            audio_failed,
            with_devices: with_audio,
            bpm_scroll: 0.0,
            sound_scroll: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: -10.0,
            restoring: false,
            slots: [None; SLOTS],
            slot_now: None,
            chain: false,
            scene_bars: [8; SLOTS],
            scene_drop: [false; SLOTS],
            visual: false,
            master_tab: false,
            last_preview: -1.0,
            solo: None,
            preview: None,
            knob_hint: None,
            update_rx: None,
            new_version: None,
            check_updates: true,
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
        self.fx_sent = [false; 6];
        self.audio_device = audio::default_device_name();
        if was_playing {
            self.send(Cmd::Play);
        }
        if self.filter != 0.0 {
            self.send(Cmd::Filter(self.filter));
        }
        self.send_chain();
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
            self.slot_now = None;
            self.say("Undo", now);
        }
    }

    fn redo(&mut self, now: f64) {
        if let Some(next) = self.redo.pop() {
            self.undo.push(self.song);
            self.song = keep_live_settings(next, &self.song);
            self.restoring = true;
            self.slot_now = None;
            self.say("Redo", now);
        }
    }

    /// Click on a scene: an empty one keeps what you hear, a full one switches to it
    fn slot_clicked(&mut self, i: usize, save: bool, now: f64) {
        if save || self.slots[i].is_none() {
            self.slots[i] = Some(self.song);
            self.slot_now = Some(i);
            self.say(format!("Saved as scene {} – changes now go into it", SLOT_NAMES[i]), now);
        } else if let Some(s) = self.slots[i] {
            self.song = keep_live_settings(s, &self.song);
            self.slot_now = Some(i);
            self.say(format!("Scene {}", SLOT_NAMES[i]), now);
        }
        self.send_chain();
    }

    fn clear_slot(&mut self, i: usize, now: f64) {
        self.slots[i] = None;
        if self.slot_now == Some(i) {
            self.slot_now = None;
        }
        if self.chain && self.slots.iter().flatten().count() < 2 {
            self.chain = false;
        }
        self.send_chain();
        self.say(format!("Scene {} cleared", SLOT_NAMES[i]), now);
    }

    /// Tell the engine about the chain (or that there is none)
    fn send_chain(&self) {
        let chain = self.chain.then(|| {
            Box::new(Chain {
                songs: self.slots,
                bars: self.scene_bars,
                drop_in: self.scene_drop,
                at: self.slot_now.unwrap_or(0),
            })
        });
        self.send(Cmd::Chain(chain));
    }

    fn toggle_chain(&mut self, now: f64) {
        if self.chain {
            self.chain = false;
            self.say("Song mode off – the scene keeps playing", now);
        } else {
            let filled: Vec<usize> = (0..SLOTS).filter(|&i| self.slots[i].is_some()).collect();
            if filled.len() < 2 {
                self.say("Save at least two scenes – then they play one after another", now);
                return;
            }
            if self.slot_now.is_none_or(|i| self.slots[i].is_none()) {
                self.slot_clicked(filled[0], false, now);
            }
            self.song.auto = false;
            self.chain = true;
            let order: Vec<String> = filled
                .iter()
                .map(|&i| format!("{} {}", SLOT_NAMES[i], self.scene_bars[i]))
                .collect();
            self.say(format!("Song: {} bars", order.join(" → ")), now);
        }
        self.send_chain();
    }

    fn toggle_auto(&mut self, now: f64) {
        self.song.auto = !self.song.auto;
        if self.song.auto && self.chain {
            self.chain = false;
            self.send_chain();
        }
        self.say(
            if self.song.auto {
                "Auto mode on – sit back"
            } else {
                "Auto mode off"
            },
            now,
        );
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

    /// Another sound for a track. The synth gets a new line when the sound needs another kind
    fn set_sound(&mut self, i: usize, sound: u8, now: f64) {
        let voice = Voice::ALL[i];
        let old = self.song.tracks[i].sound;
        if old == sound {
            return;
        }
        self.song.tracks[i].sound = sound;
        let name = voice.sounds()[sound as usize];
        if voice == Voice::Acid && synth_family(old) != synth_family(sound) {
            self.roll_track(i);
            self.say(format!("{name}: new line to fit the sound (undo keeps the old one)"), now);
        } else {
            self.say(format!("{}: {name}", voice.name()), now);
        }
        if !self.playing() {
            self.preview = Some(voice);
        }
    }

    /// A new rhythm for one track (keeps its sound, volume and mute)
    fn roll_track(&mut self, i: usize) {
        if self.song.tracks[i].lock {
            return;
        }
        let voice = Voice::ALL[i];
        let fresh = gen::track_for(voice, self.song.vibe, self.song.macros.energy, &self.song, &mut self.rng);
        let t = &mut self.song.tracks[i];
        *t = Track {
            mute: t.mute,
            volume: t.volume,
            ..fresh
        };
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
        let keep = self.song.tracks.map(|t| t.lock.then_some(t));
        let mut s = gen::generate_keeping(vibe, &self.song.macros, &mut self.rng, &keep);
        s = keep_live_settings(s, &self.song);
        for (t, old) in s.tracks.iter_mut().zip(&self.song.tracks) {
            t.volume = old.volume;
        }
        if keep_bpm {
            s.bpm = self.song.bpm;
        }
        self.song = s;
        self.solo = None;
        // A new groove is not the scene it was rolled in – keep the scene as it was
        let keep = if self.slot_now.take().is_some() || self.slots.iter().any(|s| s.is_some()) {
            " · click an empty scene to keep it"
        } else {
            ""
        };
        self.say(
            format!("{} · {} · {} BPM{keep}", vibe.name(), self.song.root_name(), self.song.bpm),
            now,
        );
    }

    /// Energy by hand: the auto mode's curve lets go
    fn set_energy(&mut self, e: f32) {
        self.song.macros.energy = e.clamp(0.0, 1.0);
        self.song.curve = false;
    }

    /// Save or open a .bunker file
    fn save_file(&mut self, now: f64) {
        let name = format!(
            "{}-{:.0}bpm.{}",
            self.song.vibe.name().to_lowercase().replace(' ', "-"),
            self.song.bpm,
            file::EXTENSION
        );
        let Some(path) = rfd::FileDialog::new()
            .set_file_name(&name)
            .add_filter("Bunker", &[file::EXTENSION])
            .save_file()
        else {
            return;
        };
        let f = BunkerFile {
            song: self.song,
            scenes: self.slots,
            scene_bars: self.scene_bars,
            scene_drop: self.scene_drop,
            ..Default::default()
        };
        match f.save(&path) {
            Ok(()) => self.say(format!("Saved {}", path.file_name().unwrap_or_default().to_string_lossy()), now),
            Err(e) => self.say(format!("Could not save: {e}"), now),
        }
    }

    fn open_file(&mut self, path: Option<PathBuf>, now: f64) {
        let path = path.or_else(|| rfd::FileDialog::new().add_filter("Bunker", &[file::EXTENSION]).pick_file());
        let Some(path) = path else { return };
        match BunkerFile::load(&path) {
            Ok(f) => self.load_file(f, &path.file_name().unwrap_or_default().to_string_lossy(), now),
            Err(e) => self.say(format!("Could not open it: {e}"), now),
        }
    }

    fn load_file(&mut self, f: BunkerFile, name: &str, now: f64) {
        self.song = keep_live_settings(f.song, &self.song);
        self.song.auto = f.song.auto;
        self.slots = f.scenes;
        self.scene_bars = f.scene_bars;
        self.scene_drop = f.scene_drop;
        self.slot_now = None;
        self.chain = false;
        self.solo = None;
        self.send_chain();
        self.say(format!("Opened {name} – undo goes back"), now);
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
                if let gen::Change::Scene(i) = c {
                    self.slot_now = Some(i);
                }
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
        if let Some(rx) = &self.update_rx {
            if let Ok(r) = rx.try_recv() {
                self.say(format!("Bunker {} is out – click the green button at the top", r.version), now);
                self.new_version = Some(r);
                self.update_rx = None;
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
            bands: self.shared.bands(),
            level: self.shared.level(),
        }
    }

    fn keys(&mut self, ctx: &egui::Context, now: f64) {
        // No text fields here: never let a focused button eat Space or Enter
        ctx.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        // Window lost focus while an effect key was held? Let go, or the effect would stick
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::WindowFocused(false)))) {
            self.fx_key = [false; 6];
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
                if self.visual {
                    self.toggle_visual(ctx);
                }
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
            if let Some(i) = FX_KEYS.iter().position(|&k| k == key) {
                if !repeat {
                    self.fx_key[i] = pressed;
                }
                continue;
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
                Key::ArrowRight => self.set_energy(self.song.macros.energy + 0.05),
                Key::ArrowLeft => self.set_energy(self.song.macros.energy - 0.05),
                Key::P => self.toggle_visual(ctx),
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

    fn toggle_visual(&mut self, ctx: &egui::Context) {
        self.visual = !self.visual;
        if self.with_devices {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.visual));
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
            ui.label(
                egui::RichText::new(concat!("v", env!("CARGO_PKG_VERSION")))
                    .font(body(11.0))
                    .color(FAINT),
            );
            ui.add_space(18.0);
            self.vibe_picker(ui, now);
            ui.add_space(10.0);
            if let Some(r) = &self.new_version {
                let text = format!("{}  Update to {}", ph::ARROW_CIRCLE_UP, r.version);
                if chip(ui, &text, true, ACID)
                    .on_hover_text("A new version is on GitHub – opens the download page")
                    .clicked()
                {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(update::RELEASES_URL));
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(6.0);
                let icon_button = |ui: &mut Ui, icon: &str, tip: &str| {
                    ui.add(egui::Button::new(egui::RichText::new(icon).size(18.0)).frame(false))
                        .on_hover_text(tip)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                };
                if icon_button(ui, ph::QUESTION, "How it works").clicked() {
                    self.help = !self.help;
                }
                if icon_button(ui, ph::GITHUB_LOGO, "Bunker on GitHub – code, downloads, ideas").clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(update::REPO_URL));
                }
                let mut action = None;
                ui.menu_button(egui::RichText::new(format!("{}  File", ph::FOLDER_SIMPLE)).font(bold(13.0)), |ui| {
                    ui.set_min_width(230.0);
                    for (i, label) in [
                        format!("{}  Export audio (WAV)…", ph::WAVEFORM),
                        format!("{}  Save as .bunker file…", ph::FLOPPY_DISK),
                        format!("{}  Open .bunker file…", ph::FOLDER_OPEN),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if ui.button(egui::RichText::new(label).font(bold(13.0))).clicked() {
                            action = Some(i);
                            ui.close_menu();
                        }
                    }
                    ui.label(
                        egui::RichText::new(".bunker files keep the groove, all scenes and the song – to share or back up")
                            .font(body(11.0))
                            .color(FAINT),
                    );
                });
                match action {
                    Some(0) => self.export = Export::Open { bars: 16 },
                    Some(1) => self.save_file(now),
                    Some(2) => self.open_file(None, now),
                    _ => {}
                }
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
                if chip(ui, ph::MONITOR_PLAY, self.visual, ACID)
                    .on_hover_text("Visualizer: the whole screen becomes a light show (P)")
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.toggle_visual(&ctx);
                }
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

    /// The presets: a dropdown of vibes (picking one rolls a new groove in it) and SHUFFLE
    fn vibe_picker(&mut self, ui: &mut Ui, now: f64) {
        ui.label(egui::RichText::new("VIBE").font(bold(11.0)).color(FAINT).extra_letter_spacing(1.6))
            .on_hover_text("Pick a style – every pick rolls a new groove in it");
        let vibe = self.song.vibe;
        let (lo, hi) = vibe.bpm();
        let resp = chip_sized(ui, &format!("{}  {}", vibe.name(), ph::CARET_DOWN), true, ACID, Some(140.0)).on_hover_text(format!(
            "{} · {lo:.0}–{hi:.0} BPM\nClick for the list – picking a vibe rolls a new groove in it",
            vibe.blurb()
        ));
        let popup = ui.make_persistent_id("vibes");
        if resp.clicked() {
            ui.memory_mut(|m| m.toggle_popup(popup));
        }
        egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
            ui.set_min_width(320.0);
            for v in Vibe::ALL {
                let (lo, hi) = v.bpm();
                let mut job = egui::text::LayoutJob::default();
                let fmt = |font, color| egui::TextFormat {
                    font_id: font,
                    color,
                    ..Default::default()
                };
                job.append(v.name(), 0.0, fmt(bold(13.5), if v == vibe { ACID } else { TEXT }));
                job.append(&format!("   {lo:.0}–{hi:.0} BPM\n"), 0.0, fmt(body(11.0), FAINT));
                job.append(v.blurb(), 0.0, fmt(body(11.5), DIM));
                if ui.selectable_label(v == vibe, job).clicked() {
                    self.shuffle(v, now);
                }
            }
        });
        // Short on room (small window plus an update notice): just the icon
        let text = if ui.available_width() > 620.0 {
            format!("{}  SHUFFLE", ph::SHUFFLE)
        } else {
            ph::SHUFFLE.to_string()
        };
        if chip(ui, &text, false, ACID)
            .on_hover_text("Roll a whole new groove in this vibe (Enter)")
            .clicked()
        {
            self.shuffle(self.song.vibe, now);
        }
    }

    fn bpm_box(&mut self, ui: &mut Ui) {
        let (rect, resp) = ui.allocate_exact_size(vec2(104.0, 42.0), Sense::click_and_drag());
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
            display(26.0),
            TEXT,
        );
        p.text(rect.right_center() - vec2(12.0, 0.0), Align2::RIGHT_CENTER, "BPM", bold(11.0), DIM);
        resp.on_hover_cursor(egui::CursorIcon::ResizeVertical)
            .on_hover_text("Drag or scroll to change the tempo (↑/↓)");
    }

    fn tracks_panel(&mut self, ui: &mut Ui, live: &Live, now: f64) {
        heading(ui, "TRACKS");
        let w = ui.available_width();
        let row_h = ((ui.available_height() - 214.0) / TRACKS as f32).clamp(42.0, 54.0);
        for (i, voice) in Voice::ALL.into_iter().enumerate() {
            let color = track_color(voice);
            let (row, resp) = ui.allocate_exact_size(vec2(w, row_h), Sense::hover());
            let soloed = self.solo.is_some_and(|(t, _)| t == i);
            let flash = live.flash(i);
            let muted = self.song.tracks[i].mute;
            let locked = self.song.tracks[i].lock;
            let p = ui.painter().clone();
            if resp.hovered() {
                p.rect_filled(row, 10.0, PANEL_HI);
            }
            let top = row.top() + (row_h - 42.0) / 2.0;
            let dot = pos2(row.left() + 11.0, top + 12.0);
            glow(&p, dot, 5.0, color, if muted { 0.0 } else { 0.3 + flash });
            p.circle_filled(dot, 5.0, if muted { FAINT } else { color });
            let name_rect = p.text(
                pos2(row.left() + 24.0, top + 3.0),
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
            ui.allocate_rect(name_rect, Sense::hover()).on_hover_text(voice.blurb());

            // Buttons on the right: solo, mute, dice, lock
            let mut x = row.right() - 2.0;
            let mut icon_btn = |ui: &mut Ui, icon: &str, tip: &str, on: bool, col: Color32| -> egui::Response {
                let r = Rect::from_min_size(pos2(x - 25.0, top), vec2(25.0, 22.0));
                x -= 26.0;
                let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tip);
                let hot = resp.hovered();
                if on {
                    ui.painter().rect_filled(r, 7.0, alpha(col, 0.18));
                } else if hot {
                    ui.painter().rect_filled(r, 7.0, mix(PANEL_HI, Color32::WHITE, 0.06));
                }
                ui.painter().text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    icon,
                    if icon.len() == 1 { bold(12.5) } else { body(15.0) },
                    if on {
                        col
                    } else if hot {
                        TEXT
                    } else {
                        DIM
                    },
                );
                resp.on_hover_cursor(egui::CursorIcon::PointingHand)
            };
            let lock = icon_btn(
                ui,
                if locked { ph::LOCK_SIMPLE } else { ph::LOCK_SIMPLE_OPEN },
                if locked {
                    "Locked: shuffle, vibes, the dice and the auto mode leave this track alone.\nYou can still edit it by hand. Click to unlock"
                } else {
                    "Lock this track: shuffle, vibes, the dice and the auto mode will leave it alone"
                },
                locked,
                TEXT,
            );
            if lock.clicked() {
                self.song.tracks[i].lock = !locked;
                self.say(
                    format!("{} {}", voice.name(), if locked { "unlocked" } else { "locked – stays as it is" }),
                    now,
                );
            }
            let dice_tip = if locked {
                "Locked – unlock it to roll a new rhythm".to_string()
            } else {
                format!(
                    "New {} rhythm\nRolls a fresh pattern for this track only – the sound stays.\nRight-click: clear all steps",
                    voice.name().to_lowercase()
                )
            };
            let dice = icon_btn(ui, if locked { "" } else { ph::DICE_FIVE }, &dice_tip, false, ACID);
            if locked {
                p.text(
                    dice.rect.center(),
                    Align2::CENTER_CENTER,
                    ph::DICE_FIVE,
                    body(15.0),
                    alpha(DIM, 0.3),
                );
            }
            if !locked && dice.secondary_clicked() {
                for st in self.song.tracks[i].steps.iter_mut() {
                    *st = crate::song::Step::default();
                }
                self.say(format!("{} cleared", voice.name()), now);
            }
            if !locked && dice.clicked() {
                self.roll_track(i);
                self.song.tracks[i].mute = false;
                self.say(format!("New {} rhythm", voice.name().to_lowercase()), now);
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
            if icon_btn(ui, "S", "Solo: hear only this track (click again to end)", soloed, ACID).clicked() {
                self.toggle_solo(i, now);
            }

            // Second line: sound, loop length, volume
            let y = top + 23.0;
            let sound_rect = Rect::from_min_size(pos2(row.left() + 22.0, y), vec2(84.0, 18.0));
            let sound = ui.allocate_rect(sound_rect, Sense::click());
            let hot = sound.hovered();
            p.rect_filled(sound_rect, 9.0, if hot { alpha(color, 0.22) } else { alpha(color, 0.1) });
            p.text(
                sound_rect.left_center() + vec2(8.0, 0.0),
                Align2::LEFT_CENTER,
                self.song.sound_name(voice),
                bold(11.0),
                if muted { DIM } else { mix(color, TEXT, 0.35) },
            );
            p.text(
                sound_rect.right_center() - vec2(7.0, 0.0),
                Align2::RIGHT_CENTER,
                ph::CARET_DOWN,
                body(10.0),
                DIM,
            );
            let popup = ui.make_persistent_id(("sound", i));
            if sound.hovered() {
                let s = ui.input(|inp| inp.raw_scroll_delta.y);
                let n = orbit::scroll_notches(&mut self.sound_scroll, s, 24.0);
                if n != 0 {
                    let count = voice.sounds().len() as i32;
                    let next = (self.song.tracks[i].sound as i32 - n.signum()).rem_euclid(count);
                    self.set_sound(i, next as u8, now);
                }
            }
            let sound = sound
                .on_hover_text("Sound: click for the list, scroll to flip through")
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            if sound.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            let mut picked = None;
            let mut shaped = false;
            egui::popup_below_widget(ui, popup, &sound, egui::PopupCloseBehavior::CloseOnClickOutside, |ui| {
                ui.set_min_width(170.0);
                ui.label(
                    egui::RichText::new(format!("{} SOUND", voice.name().to_uppercase()))
                        .font(bold(10.5))
                        .color(FAINT),
                );
                for (k, name) in voice.sounds().iter().enumerate() {
                    if ui
                        .selectable_label(self.song.tracks[i].sound as usize == k, egui::RichText::new(*name).font(bold(13.0)))
                        .clicked()
                    {
                        picked = Some(k as u8);
                    }
                }
                ui.separator();
                // Tune and decay: shape the sound
                let t = &mut self.song.tracks[i];
                let mut tune = (t.tune as f32 + 12.0) / 24.0;
                let mut decay = t.decay;
                for (label, v, text, tip) in [
                    (
                        "Tune",
                        &mut tune,
                        format!("{:+}", t.tune),
                        "Pitch in semitones (double-click: reset)",
                    ),
                    (
                        "Decay",
                        &mut decay,
                        format!("{:.1}×", t.decay_factor()),
                        "Shorter or longer (double-click: reset)",
                    ),
                ] {
                    ui.horizontal(|ui| {
                        ui.add_sized(
                            vec2(44.0, 16.0),
                            egui::Label::new(egui::RichText::new(label).font(body(12.0)).color(DIM)),
                        );
                        let r = widgets::bar_slider(ui, v, 86.0, color).on_hover_text(tip);
                        if r.double_clicked() {
                            *v = 0.5;
                        }
                        ui.label(egui::RichText::new(text).font(bold(11.0)).color(TEXT));
                    });
                }
                let new_tune = (tune * 24.0 - 12.0).round() as i8;
                if new_tune != t.tune || (decay - t.decay).abs() > 1e-4 {
                    t.tune = new_tune;
                    t.decay = decay;
                    shaped = true;
                }
            });
            if let Some(k) = picked {
                self.set_sound(i, k, now);
            }
            // Hear the change while stopped (not more often than every 150 ms)
            if shaped && !self.playing() && now - self.last_preview > 0.15 {
                self.last_preview = now;
                self.preview = Some(voice);
            }
            // Shaped sounds show it on the pill
            let t = &self.song.tracks[i];
            if t.tune != 0 || (t.decay - 0.5).abs() > 0.02 {
                p.circle_filled(sound_rect.right_top() + vec2(-3.0, 3.0), 3.0, ACID);
            }
            // Loop length
            let lr = Rect::from_min_size(pos2(sound_rect.right() + 6.0, y), vec2(30.0, 18.0));
            let lresp = ui
                .allocate_rect(lr, Sense::click())
                .on_hover_text("Steps in this loop: click −1, right-click +1.\nFewer than 16 = it drifts against the beat (polyrhythm)");
            p.rect_stroke(
                lr,
                9.0,
                stroke(1.0, if lresp.hovered() { FAINT } else { LINE }),
                egui::StrokeKind::Inside,
            );
            let len = self.song.tracks[i].len;
            p.text(
                lr.center(),
                Align2::CENTER_CENTER,
                format!("{len}"),
                bold(10.5),
                if len < 16 { ACID } else { DIM },
            );
            let len = &mut self.song.tracks[i].len;
            if lresp.clicked() {
                *len = if *len <= 1 { 16 } else { *len - 1 };
            }
            if lresp.secondary_clicked() {
                *len = if *len >= 16 { 1 } else { *len + 1 };
            }
            // Volume
            let vx = lr.right() + 8.0;
            let vr = Rect::from_min_max(pos2(vx, y + 2.0), pos2(row.right() - 6.0, y + 16.0));
            let mut vol = self.song.tracks[i].volume;
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(vr), |ui| {
                widgets::bar_slider(ui, &mut vol, vr.width(), color).on_hover_text("Volume");
            });
            self.song.tracks[i].volume = vol;
        }

        ui.add_space(8.0);
        heading(ui, "KEY & SWING");
        let label = |ui: &mut Ui, text: &str, tip: &str| {
            ui.add_sized(
                vec2(52.0, 22.0),
                egui::Label::new(egui::RichText::new(text).font(body(12.5)).color(DIM)).sense(Sense::hover()),
            )
            .on_hover_text(tip);
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let tip = "The key: which notes bass and synth play.\nThe arrows move everything a semitone down or up, the scale button next to them changes the mood.";
            label(ui, "Key", tip);
            if ui.small_button(ph::CARET_LEFT).on_hover_text("Down a semitone").clicked() {
                self.song.root = if self.song.root <= 36 { 47 } else { self.song.root - 1 };
            }
            ui.add_sized(
                vec2(28.0, 22.0),
                egui::Label::new(egui::RichText::new(self.song.note_name()).font(bold(15.0)).color(TEXT)),
            )
            .on_hover_text(tip);
            if ui.small_button(ph::CARET_RIGHT).on_hover_text("Up a semitone").clicked() {
                self.song.root = if self.song.root >= 47 { 36 } else { self.song.root + 1 };
            }
            let scale = self.song.scale;
            let resp = chip(ui, &format!("{} {}", scale.name(), ph::CARET_DOWN), false, ACID)
                .on_hover_text(format!("Scale: {} – {}\nClick for the list", scale.name(), scale.blurb()));
            let popup = ui.make_persistent_id("scales");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(230.0);
                for sc in Scale::ALL {
                    let text = egui::RichText::new(format!("{} – {}", sc.name(), sc.blurb())).font(body(12.5));
                    if ui.selectable_label(sc == scale, text).clicked() {
                        self.song.scale = sc;
                    }
                }
            });
            let tr = self.shared.transpose.load(Ordering::Relaxed);
            if tr != 0 {
                ui.label(egui::RichText::new(format!("{tr:+}")).font(bold(12.0)).color(ACID))
                    .on_hover_text("Transposed by the MIDI keyboard");
            }
        });
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            label(
                ui,
                "Chords",
                "Chord progression: bass and synth move to another chord every bar, over four bars",
            );
            let resp = chip(
                ui,
                &format!("{} {}", self.song.prog_name(), ph::CARET_DOWN),
                self.song.prog != 0,
                ACID,
            )
            .on_hover_text("Bass and synth follow these chords, one per bar – click for the list");
            let popup = ui.make_persistent_id("chords");
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
            egui::popup_below_widget(ui, popup, &resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
                ui.set_min_width(190.0);
                for (k, (name, _)) in PROGRESSIONS.iter().enumerate() {
                    let text = egui::RichText::new(*name).font(bold(12.5));
                    if ui.selectable_label(self.song.prog as usize == k, text).clicked() {
                        self.song.prog = k as u8;
                    }
                }
            });
        });
        let w = ui.available_width() - 100.0;
        for which in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                if which == 0 {
                    label(
                        ui,
                        "Swing",
                        "Pushes every second 16th note a little late: from straight (0) to bouncy (100)",
                    );
                } else {
                    label(ui, "Volume", "Master volume");
                }
                let v = if which == 0 { &mut self.song.swing } else { &mut self.song.master };
                widgets::bar_slider(ui, v, w, if which == 0 { ACID } else { TEXT });
                ui.label(egui::RichText::new(format!("{:.0}", *v * 100.0)).font(bold(11.0)).color(FAINT));
            });
        }
        // Output level
        let level = self.shared.level().min(1.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 5.0), Sense::hover());
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
        let tall = ui.available_height() > 600.0;
        // SOUND | MASTER: one row of knobs, two pages
        ui.horizontal(|ui| {
            for (label, master) in [("SOUND", false), ("MASTER", true)] {
                let on = self.master_tab == master;
                let text = egui::RichText::new(label)
                    .font(bold(11.0))
                    .color(if on { TEXT } else { FAINT })
                    .extra_letter_spacing(1.6);
                let tip = if master {
                    "Master: EQ for the whole mix and drive"
                } else {
                    "Shape the sound of the groove"
                };
                let resp = ui
                    .add(egui::Label::new(text).sense(Sense::click()))
                    .on_hover_text(tip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                if on {
                    let r = resp.rect;
                    ui.painter().line_segment([r.left_bottom(), r.right_bottom()], stroke(1.5, ACID));
                }
                if resp.clicked() {
                    self.master_tab = master;
                }
            }
        });
        let ks = ((w - 24.0) / 4.0).min(if tall { 64.0 } else { 58.0 });
        let m = self.song.macros;
        let eq = self.song.eq;
        let defaults = crate::song::Macros::default();
        let eq_def = crate::song::MasterEq::default();
        let (mut vals, def, names, tips, colors) = if self.master_tab {
            (
                [eq.low, eq.mid, eq.high, eq.drive],
                [eq_def.low, eq_def.mid, eq_def.high, eq_def.drive],
                ["LOW", "MID", "HIGH", "DRIVE"],
                [
                    "Bass and kick: ±12 dB below ~110 Hz (50 = flat)",
                    "Mids: ±12 dB around 1 kHz (50 = flat)",
                    "Highs: ±12 dB above ~7 kHz (50 = flat)",
                    "Saturation on the whole mix – warmer and louder",
                ],
                [track_color(Voice::Bass), track_color(Voice::Rim), track_color(Voice::Hat), RED],
            )
        } else {
            (
                [m.dark, m.acid, m.space, m.pump],
                [defaults.dark, defaults.acid, defaults.space, defaults.pump],
                ["DARK", "ACID", "SPACE", "PUMP"],
                [
                    "Darker sound, longer kick, deep rumble under it",
                    "Opens and squeezes the synth filter – and brings the synth line in",
                    "Reverb and echo on everything",
                    "Everything ducks under the kick and breathes with it",
                ],
                [
                    track_color(Voice::Ride),
                    ACID,
                    track_color(Voice::OpenHat),
                    track_color(Voice::Clap),
                ],
            )
        };
        let mut hint = None;
        ui.horizontal(|ui| {
            for i in 0..4 {
                let r = knob(ui, &mut vals[i], def[i], names[i], colors[i], ks);
                if r.hovered() || r.dragged() {
                    hint = Some(i);
                }
            }
        });
        if tall {
            let text = hint.map_or("Drag a knob up or down · double-click resets", |i| tips[i]);
            ui.add_sized(
                vec2(w, 28.0),
                egui::Label::new(
                    egui::RichText::new(text)
                        .font(body(11.0))
                        .color(if hint.is_some() { DIM } else { FAINT }),
                )
                .wrap(),
            );
        } else if let Some(i) = hint {
            egui::show_tooltip_text(ui.ctx(), ui.layer_id(), egui::Id::new("knob_tip"), tips[i]);
        }
        self.knob_hint = hint;
        if self.master_tab {
            let e = &mut self.song.eq;
            [e.low, e.mid, e.high, e.drive] = vals;
        } else {
            // Turning up ACID while the synth sits muted: bring it in, or the knob seems broken
            if vals[1] > m.acid + 0.001 && self.song.track(Voice::Acid).mute && self.solo.is_none() {
                self.song.track_mut(Voice::Acid).mute = false;
                self.say(format!("Synth in: {}", self.song.sound_name(Voice::Acid)), now);
            }
            let mm = &mut self.song.macros;
            [mm.dark, mm.acid, mm.space, mm.pump] = vals;
        }

        ui.add_space(2.0);
        heading(ui, "AUTO");
        // Auto mode toggle
        let (r, resp) = ui.allocate_exact_size(vec2(w, 50.0), Sense::click());
        let on = self.song.auto;
        {
            let p = ui.painter();
            p.rect_filled(r, 12.0, if on { alpha(ACID, 0.12) } else { PANEL_HI });
            p.rect_stroke(r, 12.0, stroke(1.2, if on { ACID } else { LINE }), egui::StrokeKind::Inside);
            p.text(
                r.left_top() + vec2(14.0, 9.0),
                Align2::LEFT_TOP,
                format!("{}  AUTO MODE", ph::INFINITY),
                bold(14.0),
                if on { ACID } else { TEXT },
            );
            let sub = if on {
                format!("Changes every {} bars", self.song.auto_bars)
            } else {
                "Let Bunker play a whole set".into()
            };
            p.text(r.left_top() + vec2(14.0, 29.0), Align2::LEFT_TOP, sub, body(11.0), DIM);
            // Switch
            let sw = Rect::from_center_size(r.right_center() - vec2(28.0, 0.0), vec2(36.0, 20.0));
            p.rect_filled(sw, 10.0, if on { ACID } else { LINE });
            let knob_x = if on { sw.right() - 10.0 } else { sw.left() + 10.0 };
            p.circle_filled(pos2(knob_x, sw.center().y), 7.0, if on { BG } else { DIM });
        }
        if resp
            .on_hover_text("New patterns, sounds, fills, breakdowns and drops – all by itself")
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.toggle_auto(now);
        }
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Change every").font(body(12.0)).color(DIM));
            for bars in [2u8, 4, 8] {
                if chip_sized(ui, &format!("{bars}"), self.song.auto_bars == bars, ACID, Some(36.0))
                    .on_hover_text(format!("The auto mode changes one thing every {bars} bars"))
                    .clicked()
                {
                    self.song.auto_bars = bars;
                }
            }
            ui.label(egui::RichText::new("bars").font(body(12.0)).color(DIM));
        });

        ui.add_space(2.0);
        heading(ui, "LIVE");
        // Energy: the big lever of the arrangement
        let mut e = self.song.macros.energy;
        let curve = self.song.auto.then_some(self.song.curve);
        let out = widgets::energy_fader(ui, &mut e, w, if tall { 58.0 } else { 50.0 }, curve, self.song.rise);
        if out.changed {
            self.set_energy(e);
        }
        if out.rise_clicked {
            self.song.rise = !self.song.rise;
            self.say(
                if self.song.rise {
                    "Rise: pull the energy down, push it up slowly – it builds up and drops"
                } else {
                    "Rise off: the energy fader is just energy"
                },
                now,
            );
        }
        if out.curve_clicked {
            if self.song.auto {
                self.song.curve = !self.song.curve;
                self.say(
                    if self.song.curve {
                        "Energy curve: the auto mode moves the energy"
                    } else {
                        "Energy by hand"
                    },
                    now,
                );
            } else {
                self.say("The energy curve belongs to the auto mode – switch it on first", now);
            }
        }
        let before = self.filter;
        widgets::filter_slider(ui, &mut self.filter, w, if tall { 54.0 } else { 46.0 });
        if self.filter != before {
            self.send(Cmd::Filter(self.filter));
        }
        // Effects: hold to use
        let gap = 8.0;
        let bw = (w - gap * 2.0) / 3.0;
        let bh = if tall { 48.0 } else { 38.0 };
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for col in 0..3 {
                    let i = row * 3 + col;
                    let fx = Fx::ALL[i];
                    let (r, resp) = ui.allocate_exact_size(vec2(bw, bh), Sense::click_and_drag());
                    self.fx_mouse[i] = resp.is_pointer_button_down_on();
                    let active = self.fx_mouse[i] || self.fx_key[i];
                    widgets::fx_button(ui, r, &resp, fx_icon(fx), fx.name(), fx.key(), fx_color(fx), active);
                    resp.on_hover_text(format!("{} – hold ({})", fx.blurb(), fx.key()))
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                }
            });
        }
        // The drop
        let (r, resp) = ui.allocate_exact_size(vec2(w, if tall { 62.0 } else { 48.0 }), Sense::click());
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
        let big = r.height() > 56.0;
        p.text(
            r.center() - vec2(0.0, if big { 6.0 } else { 5.0 }),
            Align2::CENTER_CENTER,
            text,
            display(if big { 26.0 } else { 21.0 }),
            TEXT,
        );
        p.text(
            r.center() + vec2(0.0, if big { 17.0 } else { 13.0 }),
            Align2::CENTER_CENTER,
            "two bars build-up from the next bar (B)",
            body(10.5),
            alpha(TEXT, 0.6),
        );
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.drop_now(now);
        }
    }

    /// The scene strip under the orbit: four slots and song mode
    fn scenes_ui(&mut self, ui: &mut Ui, rect: Rect, live: &Live, now: f64) {
        let p = ui.painter().clone();
        let left_w = 96.0;
        p.text(rect.left_top() + vec2(4.0, 8.0), Align2::LEFT_TOP, "SCENES", bold(11.0), FAINT);
        let sub = if self.slots.iter().all(|s| s.is_none()) {
            "+ keeps what\nyou hear"
        } else {
            "click to switch"
        };
        p.text(rect.left_top() + vec2(4.0, 25.0), Align2::LEFT_TOP, sub, body(10.5), FAINT);
        let info = ui.allocate_rect(Rect::from_min_size(rect.min, vec2(left_w - 8.0, rect.height())), Sense::hover());
        info.on_hover_text(
            "Scenes are four memory slots for grooves.\n\
             • Click an empty one (+) to keep the groove you hear.\n\
             • Click a full one to switch to it – the lit one keeps your changes.\n\
             • The number on a scene is how many bars it plays in song mode, the lightning builds up and drops into it.\n\
             • SONG plays the full scenes one after another.\n\
             • Right-click a scene to overwrite or clear it.",
        );

        let song_w = 92.0;
        let cards = Rect::from_min_max(
            pos2(rect.left() + left_w, rect.top()),
            pos2(rect.right() - song_w - 10.0, rect.bottom()),
        );
        let gap = 8.0;
        let cw = (cards.width() - gap * 3.0) / SLOTS as f32;
        for (i, name) in SLOT_NAMES.into_iter().enumerate() {
            let r = Rect::from_min_size(
                pos2(cards.left() + i as f32 * (cw + gap), cards.top() + 4.0),
                vec2(cw, rect.height() - 8.0),
            );
            let resp = ui.allocate_rect(r, Sense::click());
            let filled = self.slots[i];
            let current = self.slot_now == Some(i);
            let hot = resp.hovered();
            match filled {
                Some(song) => {
                    let playing_song = current && self.chain && live.playing;
                    if playing_song {
                        widgets::glow_rect(&p, r, 10.0, ACID, 0.25 + live.flash(0) * 0.5);
                    }
                    p.rect_filled(
                        r,
                        10.0,
                        if current {
                            mix(PANEL, ACID, 0.12)
                        } else if hot {
                            PANEL_HI
                        } else {
                            mix(PANEL, PANEL_HI, 0.6)
                        },
                    );
                    p.rect_stroke(
                        r,
                        10.0,
                        stroke(if current { 1.5 } else { 1.0 }, if current { ACID } else { LINE }),
                        egui::StrokeKind::Inside,
                    );
                    p.text(
                        r.left_top() + vec2(9.0, 3.0),
                        Align2::LEFT_TOP,
                        name,
                        display(18.0),
                        if current { ACID } else { TEXT },
                    );
                    if cw > 128.0 {
                        p.text(
                            r.left_top() + vec2(28.0, 8.0),
                            Align2::LEFT_TOP,
                            song.vibe.name(),
                            bold(11.0),
                            if current { TEXT } else { DIM },
                        );
                    }
                    // Tiny dots: which tracks play in this scene
                    let dots_w = (r.width() - 20.0).min(80.0);
                    for (t, v) in Voice::ALL.into_iter().enumerate() {
                        let tr = song.tracks[t];
                        let x = r.left() + 10.0 + t as f32 * dots_w / 7.0;
                        let c = if tr.mute || tr.hits() == 0 {
                            alpha(FAINT, 0.5)
                        } else {
                            track_color(v)
                        };
                        p.circle_filled(pos2(x, r.bottom() - 9.0), 2.2, c);
                    }
                }
                None => {
                    p.rect_stroke(r, 10.0, stroke(1.0, if hot { FAINT } else { LINE }), egui::StrokeKind::Inside);
                    let label = if cw > 80.0 {
                        format!("{}  {name}", ph::PLUS)
                    } else {
                        ph::PLUS.to_string()
                    };
                    p.text(r.center(), Align2::CENTER_CENTER, label, bold(13.0), if hot { TEXT } else { FAINT });
                }
            }
            let tip = match (filled.is_some(), current) {
                (false, _) => format!("Empty – click to keep the groove you hear now as scene {name}"),
                (true, true) => format!("Scene {name} is playing – your changes are kept in it.\nRight-click: overwrite or clear"),
                (true, false) => format!("Click: switch to scene {name}\nRight-click: overwrite or clear"),
            };
            let resp = resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
            // Song mode settings on full scenes: length and drop-in
            let mut pill_clicked = false;
            if filled.is_some() {
                let lr = Rect::from_min_size(pos2(r.right() - 30.0, r.top() + 5.0), vec2(24.0, 17.0));
                let dr = Rect::from_min_size(pos2(lr.left() - 22.0, lr.top()), vec2(19.0, 17.0));
                let len = ui.allocate_rect(lr, Sense::click());
                let drop = ui.allocate_rect(dr, Sense::click());
                let bars = self.scene_bars[i];
                let dropping = self.scene_drop[i];
                p.rect_filled(lr, 8.0, if len.hovered() { PANEL_HI } else { alpha(BG, 0.5) });
                p.text(lr.center(), Align2::CENTER_CENTER, format!("{bars}"), bold(10.5), DIM);
                p.text(
                    dr.center(),
                    Align2::CENTER_CENTER,
                    ph::LIGHTNING,
                    body(12.0),
                    if dropping {
                        RED
                    } else if drop.hovered() {
                        TEXT
                    } else {
                        alpha(FAINT, 0.6)
                    },
                );
                if len
                    .on_hover_text(format!("Song mode: scene {name} plays {bars} bars – click for more (2 → 32)"))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.scene_bars[i] = match bars {
                        0..=2 => 4,
                        3..=4 => 8,
                        5..=8 => 16,
                        9..=16 => 32,
                        _ => 2,
                    };
                    pill_clicked = true;
                }
                let drop_tip = if dropping {
                    format!("Song mode drops into scene {name}: two bars build-up before it. Click: no drop")
                } else {
                    format!("Click: build up and drop into scene {name} in song mode")
                };
                if drop
                    .on_hover_text(drop_tip)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.scene_drop[i] = !dropping;
                    pill_clicked = true;
                }
                if pill_clicked {
                    self.send_chain();
                }
            }
            if resp.clicked() && !pill_clicked {
                self.slot_clicked(i, false, now);
            }
            if filled.is_some() {
                resp.context_menu(|ui| {
                    if ui.button(format!("{}  Save what you hear here", ph::FLOPPY_DISK)).clicked() {
                        self.slot_clicked(i, true, now);
                        ui.close_menu();
                    }
                    if ui.button(format!("{}  Clear scene", ph::TRASH)).clicked() {
                        self.clear_slot(i, now);
                        ui.close_menu();
                    }
                });
            }
        }

        // SONG: play the scenes in turn
        let cr = Rect::from_min_size(pos2(rect.right() - song_w, rect.top() + 4.0), vec2(song_w, rect.height() - 8.0));
        let resp = ui.allocate_rect(cr, Sense::click());
        widgets::fx_button(ui, cr, &resp, ph::PLAY_PAUSE, "SONG", "", ACID, self.chain);
        let total: u32 = (0..SLOTS)
            .filter(|&i| self.slots[i].is_some())
            .map(|i| self.scene_bars[i] as u32)
            .sum();
        if resp
            .on_hover_text(format!(
                "Song mode: the full scenes play one after another, each for its number of bars ({total} bars in all), then from the top"
            ))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.toggle_chain(now);
        }
    }

    fn hints(&self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(8.0);
            let keys = "Space play  ·  Enter shuffle  ·  A–K pads  ·  1–8 mute  ·  ← → energy  ·  B drop  ·  Q W E  X C V effects  ·  R record  ·  Tab view  ·  P visuals  ·  ⌘/Ctrl Z undo";
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
            .default_width(480.0)
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
                let max_h = ctx.screen_rect().height() - 200.0;
                egui::ScrollArea::vertical().max_height(max_h).show(ui, |ui| {
                    row(ui, ph::PLAY, "Press play", "Space or the button in the middle. Pick a vibe in the bar at the top – each click rolls a fresh groove in that style. SHUFFLE rolls again.");
                    row(ui, ph::CIRCLES_THREE, "The orbit", "Each ring is one sound, each dot one step. Click to switch a dot on or off, drag to paint. Right-click: accent (on the synth: slide). Scroll on bass/synth dots to change the note. Shift-click: chance.");
                    row(ui, ph::LIST, "Tracks", "S solos a track, the speaker mutes it, the dice rolls a new rhythm for just that track (right-click: clear), the lock keeps shuffle, vibes, dice and auto mode away from it. The coloured pill is its sound – click it for the list, tune and decay, or scroll on it. The number is the loop length: under 16 it drifts against the beat.");
                    row(ui, ph::MUSIC_NOTES, "Key & swing", "Key = which notes bass and synth play: the arrows shift them, the scale list changes the mood. Chords moves them through a progression, one chord per bar. Swing pushes every second 16th late for more bounce.");
                    row(ui, ph::STACK, "Scenes", "Four memory slots under the orbit. Click + to keep what you hear, click a full scene to switch – the lit one keeps your changes. The number is its length in song mode, the lightning builds up and drops into it. SONG plays them one after another.");
                    row(ui, ph::INFINITY, "Auto mode", "Bunker changes something every 2, 4 or 8 bars – patterns, sounds, notes, fills – and every 32 bars plays a breakdown, a build-up and a drop. Sometimes the drop starts a new chapter with a new bassline or key. With CURVE on the energy fader it also moves the energy; touch the fader to take over.");
                    row(ui, ph::SLIDERS, "Sound & master", "DARK closes the sound and adds rumble. ACID opens the synth filter. SPACE: reverb and echo. PUMP ducks everything under the kick. MASTER: low, mid and high EQ and drive for the whole mix.");
                    row(ui, ph::LIGHTNING, "Live effects", "ENERGY strips the groove down or builds it up (← →) – pull it down, hit DROP and everything comes back. With RISE on, a slow push up from low energy builds up and drops by itself. FILTER springs back when you let go. Hold STUTTER (Q), ECHO (W), WASH (E), CRUSH (X), GATE (C) or TAPE STOP (V) – combine them.");
                    row(ui, ph::MONITOR_PLAY, "Visualizer", "P turns the window into a fullscreen light show – all keys keep working. Esc goes back.");
                    row(ui, ph::FOLDER_SIMPLE, "Files", "File → Save as .bunker keeps the groove, the scenes and the song in one file. Open it from the menu or drag it onto the window. File → Export audio writes a WAV.");
                    row(ui, ph::GRID_FOUR, "Pads", "Tab switches to big pads and a classic grid. Hit RECORD and play along – your hits land on the beat.");
                    row(ui, ph::ARROW_COUNTER_CLOCKWISE, "Undo", "⌘/Ctrl + Z takes back any change – a shuffle too. ⌘/Ctrl + Shift + Z redoes it.");
                    row(ui, ph::PIANO_KEYS, "MIDI keyboard", "Keys below middle C play the sounds. Hold a key from middle C up to transpose bass and synth. Mod wheel = acid, pitch bend = filter, sustain = echo, knobs (CC 70–73) = sound knobs.");
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.check_updates, "Tell me when a new version is out")
                        .on_hover_text("Asks GitHub once at start – nothing else is sent");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.hyperlink_to(format!("{}  github.com/tj537/bunker", ph::GITHUB_LOGO), update::REPO_URL);
                    });
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
        eframe::set_value(storage, "slot_now", &self.slot_now);
        eframe::set_value(storage, "scene_bars", &self.scene_bars);
        eframe::set_value(storage, "scene_drop", &self.scene_drop);
        eframe::set_value(storage, "check_updates", &self.check_updates);
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

        // A .bunker file dropped onto the window
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect());
        if let Some(path) = dropped.into_iter().find(|p| p.extension().is_some_and(|e| e == file::EXTENSION)) {
            self.open_file(Some(path), now);
        }

        let frame = |fill: Color32, margin: f32| egui::Frame::new().fill(fill).inner_margin(margin);
        if self.visual {
            egui::CentralPanel::default().frame(frame(BG, 0.0)).show(ctx, |ui| {
                let rect = ui.max_rect();
                visual::visual(ui, rect, &self.song, &live, self.slot_now, now);
                // Double-click anywhere goes back
                if ui.interact(rect, ui.id().with("visual"), Sense::click()).double_clicked() {
                    self.toggle_visual(ctx);
                }
            });
            ctx.request_repaint();
            self.finish_frame(now);
            return;
        }
        egui::TopBottomPanel::top("top")
            .exact_height(58.0)
            .frame(frame(BG, 6.0))
            .show(ctx, |ui| self.top_bar(ui, &live, now));
        egui::TopBottomPanel::bottom("hints")
            .exact_height(28.0)
            .frame(frame(BG, 2.0))
            .show(ctx, |ui| self.hints(ui));
        egui::SidePanel::left("tracks")
            .exact_width(266.0)
            .resizable(false)
            .frame(frame(BG, 14.0))
            .show(ctx, |ui| self.tracks_panel(ui, &live, now));
        egui::SidePanel::right("live")
            .exact_width(286.0)
            .resizable(false)
            .frame(frame(BG, 14.0))
            .show(ctx, |ui| self.live_panel(ui, &live, now));
        egui::CentralPanel::default().frame(frame(BG, 10.0)).show(ctx, |ui| {
            let area = ui.available_rect_before_wrap();
            let scenes_h = 58.0;
            let rect = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - scenes_h - 6.0));
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
            let strip = Rect::from_min_max(pos2(area.left(), area.bottom() - scenes_h), area.max);
            self.scenes_ui(ui, strip, &live, now);
        });

        self.export_window(ctx, now);
        self.help_window(ctx);
        self.finish_frame(now);
    }

    /// End of every frame: tell the engine what changed
    fn finish_frame(&mut self, now: f64) {
        // Live effects: held with a key or the mouse
        for i in 0..Fx::ALL.len() {
            let on = self.fx_key[i] || self.fx_mouse[i];
            if on != self.fx_sent[i] {
                self.fx_sent[i] = on;
                self.send(Cmd::Fx(Fx::ALL[i], on));
            }
        }

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
                // The lit scene keeps every change
                if let Some(i) = self.slot_now {
                    if self.slots[i].is_some() {
                        self.slots[i] = Some(self.song);
                        if self.chain {
                            self.send_chain();
                        }
                    }
                }
            }
            self.sent = self.song;
            self.send(Cmd::Song(Box::new(self.song)));
        }
        if let Some(v) = self.preview.take() {
            self.send(Cmd::Hit {
                voice: v,
                vel: 0.85,
                record: false,
            });
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
        assert_eq!(app.slot_now, None, "a shuffle leaves the scene alone");
        app.song.master = 0.3;
        app.slot_clicked(0, false, 0.0); // full → load
        assert_eq!(app.song.tracks, first.tracks);
        assert_eq!(app.song.master, 0.3, "volume belongs to the moment");
    }

    #[test]
    fn the_lit_scene_keeps_changes() {
        let mut snap = Offscreen::new(1360.0, 860.0);
        install(&snap.ctx);
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        frame(&mut app, &mut snap, 1.0);
        app.slot_clicked(1, false, 1.0);
        frame(&mut app, &mut snap, 1.1);
        app.song.tracks[2].steps[5].on = !app.song.tracks[2].steps[5].on;
        frame(&mut app, &mut snap, 2.0);
        assert_eq!(app.slots[1].unwrap().tracks[2], app.song.tracks[2]);
        // Undo doesn't write into a scene
        app.undo(3.0);
        frame(&mut app, &mut snap, 3.0);
        assert_eq!(app.slot_now, None);
        assert_ne!(app.slots[1].unwrap().tracks[2], app.song.tracks[2]);
    }

    #[test]
    fn chain_needs_two_scenes_and_stops_auto() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        app.toggle_chain(0.0);
        assert!(!app.chain);
        app.slot_clicked(0, false, 0.0);
        app.shuffle(Vibe::Dub, 0.0);
        app.slot_clicked(2, false, 0.0);
        app.song.auto = true;
        app.toggle_chain(0.0);
        assert!(app.chain);
        assert!(!app.song.auto);
        app.toggle_auto(0.0);
        assert!(!app.chain);
        // Clearing down to one scene ends the chain
        app.toggle_chain(0.0);
        app.clear_slot(0, 0.0);
        assert!(!app.chain);
    }

    #[test]
    fn new_synth_sound_gets_a_fitting_line() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        let i = Voice::Acid.index();
        app.song.tracks[i].sound = 0;
        let before = app.song.tracks[i].steps;
        app.set_sound(i, 1, 0.0); // 303 saw → square: same line
        assert_eq!(app.song.tracks[i].steps, before);
        app.set_sound(i, 2, 0.0); // → stab: a new line
        assert_eq!(app.song.tracks[i].sound, 2);
        assert!(app.song.tracks[i].hits() <= 6, "stabs are sparse");
    }

    #[test]
    fn shuffle_keeps_locked_tracks() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        app.song.tracks[6].lock = true;
        app.song.tracks[6].tune = 3;
        let bass = app.song.tracks[6];
        for vibe in Vibe::ALL {
            app.shuffle(vibe, 0.0);
            assert_eq!(app.song.tracks[6], bass);
        }
        app.roll_track(6);
        assert_eq!(app.song.tracks[6], bass, "the dice leaves it alone too");
    }

    #[test]
    fn energy_by_hand_stops_the_curve() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        app.song.auto = true;
        assert!(app.song.curve);
        app.set_energy(0.2);
        assert!(!app.song.curve);
        assert_eq!(app.song.macros.energy, 0.2);
        app.set_energy(3.0);
        assert_eq!(app.song.macros.energy, 1.0);
    }

    #[test]
    fn bunker_file_loads_everything() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        let mut f = BunkerFile {
            song: gen::generate(Vibe::Electro, &Default::default(), &mut Rng::new(3)),
            ..Default::default()
        };
        f.scenes[1] = Some(gen::generate(Vibe::Dub, &Default::default(), &mut Rng::new(4)));
        f.scene_bars = [4, 16, 8, 8];
        app.song.master = 0.4;
        app.load_file(BunkerFile::from_text(&f.to_text()).unwrap(), "test.bunker", 0.0);
        assert_eq!(app.song.tracks, f.song.tracks);
        assert_eq!(app.song.master, 0.4, "volume belongs to the moment");
        assert_eq!(app.slots, f.scenes);
        assert_eq!(app.scene_bars, [4, 16, 8, 8]);
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
