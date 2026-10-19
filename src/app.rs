//! The window: top bar with the vibes, track list, the orbit (or pads) with the patterns
//! below, sound knobs, auto mode and live effects.

use crate::audio::{self, Audio};
use crate::engine::{Arrangement, Cmd, Event, Fx, Shared};
use crate::file::{self, BunkerFile};
use crate::form::FORMS;
use crate::gen;
use crate::midi::{Midi, MidiIn};
use crate::orbit::{self, Live, Paint};
use crate::pads;
use crate::rng::Rng;
use crate::song::{Song, Step, Track, Vibe, Voice, MOODS, STEPS, TRACKS};
use crate::theme::*;
use crate::update::{self, Release};
use crate::visual;
use crate::widgets::{self, chip, chip_sized, chip_small, concrete, glow, heading};
use eframe::egui::{self, pos2, vec2, Align2, Color32, Key, Rect, Sense, Ui};
use egui_phosphor::regular as ph;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;

/// Favorites: saved grooves (the first four are the patterns of AUTO SONG)
pub const SLOTS: usize = 12;
const UNDO_LIMIT: usize = 60;
const FX_KEYS: [Key; 6] = [Key::Q, Key::W, Key::E, Key::X, Key::C, Key::V];

/// A stored song (undo step, saved slot) with the current volume and auto-mode settings –
/// those belong to the moment, not to the groove
fn keep_live_settings(mut song: Song, now: &Song) -> Song {
    song.master = now.master;
    song.auto = now.auto;
    song.auto_bars = now.auto_bars;
    song.curve = now.curve;
    song.auto_fx = now.auto_fx;
    song.eq = now.eq;
    song
}

/// The same groove, whatever the live settings (energy, volume …) are
fn same_groove(a: &Song, b: &Song) -> bool {
    a.tracks == b.tracks && a.root == b.root && a.scale == b.scale && a.prog == b.prog && a.vibe == b.vibe
}

/// `text` cut with "…" so it fits into `max_w`
fn fit_text(p: &egui::Painter, text: &str, font: egui::FontId, max_w: f32) -> String {
    let width = |t: &str| p.layout_no_wrap(t.to_string(), font.clone(), TEXT).size().x;
    if width(text) <= max_w {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() {
        chars.pop();
        let t: String = chars.iter().collect::<String>() + "…";
        if width(&t) <= max_w {
            return t;
        }
    }
    String::new()
}

/// How much Bunker uses an effect by itself right now: every build-up runs the gate,
/// harder towards the drop – the button lights up and flickers with the chop
fn auto_fx(fx: Fx, live: &Live) -> f32 {
    match (fx, live.build) {
        (Fx::Gate, Some(p)) if live.playing => {
            let chop = if live.pos.fract() < 0.5 { 1.0 } else { 0.55 };
            (p * 1.3).clamp(0.25, 1.0) * chop
        }
        _ => 0.0,
    }
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
    Planets,
}

/// The recording of a song (SONG): offered for saving when it ends
enum Export {
    Closed,
    Recorded,
    Saving(Receiver<Result<PathBuf, String>>),
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
    /// `Cmd::Song`s sent to this engine – its songs carry how many it had seen
    songs_sent: u64,
    /// Song mode: the performance as 16-bit stereo, its sample rate, and whether it's still coming in
    recording: Vec<i16>,
    rec_sr: u32,
    rec_live: bool,
    /// The whole session is being recorded (the record-the-set button)
    pub capture: bool,
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
    /// Four patterns; the active one (`slot_now`) keeps every edit
    pub slots: [Option<Song>; SLOTS],
    /// Names for the favorites (empty: the vibe's name is shown)
    pub names: [String; SLOTS],
    /// The favorite being renamed, and the name typed so far
    renaming: Option<usize>,
    rename_buf: String,
    pub slot_now: Option<usize>,
    /// A pattern picked while playing, waiting for the next bar
    pub queued_slot: Option<usize>,
    /// Song mode: the form playing (index into `form::FORMS`)
    pub song_form: Option<usize>,
    /// Song mode: what plays where no pattern is saved (the groove when the song started)
    song_fallback: Song,
    /// The settings window is open
    pub settings: bool,
    /// Visualizer: the whole window is a light show
    pub visual: bool,
    /// …and it's the face, not the lights
    pub face: bool,
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
        // eframe brings the window back as it was – quit inside a visualizer, and that was
        // fullscreen. Bunker always starts in its window; only a visualizer goes fullscreen.
        cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        let mut song: Song = cc.storage.and_then(|s| eframe::get_value(s, "song")).unwrap_or_default();
        song.sanitize();
        let view = cc.storage.and_then(|s| eframe::get_value(s, "view")).unwrap_or(View::Orbit);
        let mut app = Self::headless(song, view, true);
        // (Before 0.4.2 there were four slots)
        let slots = cc.storage.and_then(|s| {
            eframe::get_value::<[Option<Song>; SLOTS]>(s, "slots").or_else(|| {
                eframe::get_value::<[Option<Song>; 4]>(s, "slots").map(|old| {
                    let mut all = [None; SLOTS];
                    all[..4].copy_from_slice(&old);
                    all
                })
            })
        });
        if let Some(mut slots) = slots {
            for s in slots.iter_mut().flatten() {
                s.sanitize();
            }
            app.slots = slots;
        }
        if let Some(names) = cc.storage.and_then(|s| eframe::get_value::<Vec<String>>(s, "fav_names")) {
            for (slot, name) in app.names.iter_mut().zip(names) {
                *slot = name;
            }
        }
        app.slot_now = cc
            .storage
            .and_then(|s| eframe::get_value::<Option<usize>>(s, "slot_now"))
            .flatten()
            .filter(|&i| i < SLOTS && app.slots[i].is_some());
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
            songs_sent: 0,
            recording: Vec::new(),
            rec_sr: 44_100,
            rec_live: false,
            capture: false,
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
            names: std::array::from_fn(|_| String::new()),
            renaming: None,
            rename_buf: String::new(),
            slot_now: None,
            queued_slot: None,
            song_form: None,
            song_fallback: song,
            visual: false,
            face: false,
            settings: false,
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
        self.songs_sent = 0;
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
        self.send_arrangement();
        // A session recording goes on in the new engine
        if self.capture {
            self.send(Cmd::Capture(true));
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

    /// Click on a pattern: an empty one keeps what you hear, a full one switches to it –
    /// while playing on the next bar, with a clap roll before
    fn slot_clicked(&mut self, i: usize, save: bool, now: f64) {
        if save || self.slots[i].is_none() {
            self.slots[i] = Some(self.song);
            self.slot_now = Some(i);
            self.say(format!("Saved as favorite {} – changes now go into it", i + 1), now);
        } else if let Some(s) = self.slots[i] {
            if self.playing() {
                self.queued_slot = Some(i);
                self.send(Cmd::Queue(Box::new(keep_live_settings(s, &self.song)), i));
                self.say(format!("{} on the next bar", self.fav_name(i)), now);
            } else {
                self.song = keep_live_settings(s, &self.song);
                self.slot_now = Some(i);
                self.say(format!("{} · {}", i + 1, self.fav_name(i)), now);
            }
        }
        if self.song_form.is_some() {
            self.send_arrangement();
        }
    }

    fn clear_slot(&mut self, i: usize, now: f64) {
        self.slots[i] = None;
        if self.slot_now == Some(i) {
            self.slot_now = None;
        }
        if self.song_form.is_some() {
            self.send_arrangement();
        }
        self.names[i].clear();
        self.say(format!("Favorite {} cleared", i + 1), now);
    }

    /// The name shown on a favorite: yours, or else its vibe
    fn fav_name(&self, i: usize) -> String {
        if !self.names[i].is_empty() {
            self.names[i].clone()
        } else {
            self.slots[i].map_or(String::new(), |s| s.vibe.name().to_string())
        }
    }

    /// The favorites AUTO SONG can use (the forms play 1, then 2)
    fn song_favorites(&self) -> [Option<Song>; 4] {
        std::array::from_fn(|i| self.slots[i])
    }

    /// Naming a favorite
    fn rename_window(&mut self, ctx: &egui::Context) {
        let Some(i) = self.renaming else { return };
        let mut done = None;
        egui::Window::new(format!("Name favorite {}", i + 1))
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let field = ui.add(egui::TextEdit::singleline(&mut self.rename_buf).char_limit(24).desired_width(220.0));
                field.request_focus();
                let enter = ui.input(|i| i.key_pressed(Key::Enter));
                let esc = ui.input(|i| i.key_pressed(Key::Escape));
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("Save").font(bold(13.0))).clicked() || enter {
                        done = Some(true);
                    }
                    if ui.button("Cancel").clicked() || esc {
                        done = Some(false);
                    }
                });
            });
        match done {
            Some(true) => {
                self.names[i] = self.rename_buf.trim().to_string();
                self.renaming = None;
            }
            Some(false) => self.renaming = None,
            None => {}
        }
    }

    /// Tell the engine about the song (or that there is none)
    fn send_arrangement(&self) {
        let a = self.song_form.map(|form| {
            Box::new(Arrangement {
                patterns: self.song_favorites(),
                form,
                fallback: self.song_fallback,
            })
        });
        self.send(Cmd::Arrange(a));
    }

    /// Song mode: play a form with the patterns, from the top
    fn start_song(&mut self, form: usize, now: f64) {
        // Everything you hear from here on is recorded, to save as a WAV at the end
        // (already the case when the whole session is being recorded)
        if !self.capture {
            self.recording.clear();
            self.rec_live = true;
            self.rec_sr = self.audio.as_ref().map_or(44_100, |a| a.sample_rate);
        }
        self.song.auto = false;
        self.song.curve = true;
        self.song_fallback = self.song;
        self.song_form = Some(form);
        self.send_arrangement();
        if !self.playing() {
            self.send(Cmd::Play);
        }
        let f = &FORMS[form];
        let mins = f.bars() as f32 * 4.0 / self.song.bpm;
        self.say(format!("Song: {} – about {:.0} minutes", f.name, mins.max(1.0)), now);
    }

    fn stop_song(&mut self, now: f64) {
        self.song_form = None;
        self.send_arrangement();
        self.say("Song off – the pattern keeps playing", now);
    }

    fn toggle_auto(&mut self, now: f64) {
        self.song.auto = !self.song.auto;
        if self.song.auto && self.song_form.is_some() {
            self.song_form = None;
            self.send_arrangement();
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
        if self.playing() {
            // Stopping ends a song too
            self.song_form = None;
            self.queued_slot = None;
        }
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
            " · click an empty pattern to keep it"
        } else {
            ""
        };
        self.say(
            format!("{} · {} · {} BPM{keep}", vibe.name(), self.song.root_name(), self.song.bpm),
            now,
        );
    }

    /// The custom vibe: every track empty (locked ones stay), sounds, mood and tempo stay
    fn custom(&mut self, now: f64) {
        let mut s = self.song;
        s.vibe = Vibe::Custom;
        for t in s.tracks.iter_mut().filter(|t| !t.lock) {
            t.steps = [Step::default(); STEPS];
            t.len = STEPS as u8;
            t.mute = false;
        }
        self.song = s;
        self.solo = None;
        self.slot_now = None;
        self.say("Custom – an empty pattern: click the rings to paint your groove", now);
    }

    /// Bunker steers the energy itself: the auto mode with AUTO FX on
    fn energy_locked(&self) -> bool {
        self.song_form.is_some() || (self.song.auto && self.song.auto_fx)
    }

    /// Why the energy bar doesn't move
    fn energy_hint(&self) -> &'static str {
        if self.song_form.is_some() {
            "The song steers the energy – stop AUTO SONG to take it"
        } else {
            "Bunker steers the energy – switch AUTO FX off to take it"
        }
    }

    /// Energy by hand – in a song it takes over until the next part; in the auto mode with
    /// AUTO FX the energy is Bunker's
    fn set_energy(&mut self, e: f32, now: f64) {
        if self.energy_locked() {
            self.say(self.energy_hint(), now);
            return;
        }
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
            scenes: self.slots.to_vec(),
            names: self.names.to_vec(),
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
        for i in 0..SLOTS {
            self.slots[i] = f.scenes.get(i).copied().flatten();
            self.names[i] = f.names.get(i).cloned().unwrap_or_default();
        }
        self.slot_now = None;
        self.queued_slot = None;
        self.song_form = None;
        self.solo = None;
        self.send_arrangement();
        self.say(format!("Opened {name} – undo goes back"), now);
    }

    fn hit(&mut self, voice: Voice) {
        let record = self.record.load(Ordering::Relaxed);
        self.send(Cmd::Hit { voice, vel: 0.9, record });
    }

    // ───────────── Per-frame state from the engine ─────────────

    fn sync(&mut self, ctx: &egui::Context, now: f64, dt: f32) -> Live {
        while let Ok(ev) = self.events.try_recv() {
            let (song, change, ack) = match ev {
                Event::Song(song, change, ack) => (song, change, ack),
                Event::Audio(piece) => {
                    if self.rec_live {
                        self.recording.extend(piece.iter().map(|&s| crate::wav::to_i16(s)));
                        // An hour is plenty (about 350 MB in memory)
                        if self.capture && self.recording.len() >= self.rec_sr as usize * 2 * 3600 {
                            self.toggle_capture(now);
                        }
                    }
                    continue;
                }
                Event::Recorded => {
                    self.capture = false;
                    // At least two seconds: worth keeping
                    if std::mem::take(&mut self.rec_live) && self.recording.len() > self.rec_sr as usize * 4 {
                        self.export = Export::Recorded;
                    }
                    continue;
                }
            };
            // A song the engine sent before it had our last edit would undo that edit
            if ack >= self.songs_sent {
                self.song = *song;
                self.sent = self.song;
                // The lit favorite only stays lit while the groove is exactly what's saved
                let loaded = matches!(change, Some(gen::Change::Scene(_) | gen::Change::Part(..)));
                if let Some(i) = self.slot_now.filter(|_| !loaded) {
                    if !self.slots[i].is_some_and(|s| same_groove(&s, &self.song)) {
                        self.slot_now = None;
                    }
                }
            }
            if let Some(c) = change {
                match c {
                    gen::Change::Scene(i) => {
                        self.slot_now = Some(i);
                        self.queued_slot = None;
                    }
                    gen::Change::Part(_, _, slot) => {
                        self.slot_now = slot;
                        self.queued_slot = None;
                    }
                    gen::Change::SongEnd => self.song_form = None,
                    _ => {}
                }
                let text = match c {
                    gen::Change::Scene(i) => format!("{} · {}", i + 1, self.fav_name(i)),
                    _ => c.describe(),
                };
                self.say(text, now);
            }
        }
        while let Ok(m) = self.midi_rx.try_recv() {
            match m {
                Midi::Macro(0, v) => self.set_energy(v, now),
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
        let part = match self.shared.part.load(Ordering::Relaxed) {
            0 => None,
            k => Some((k as usize - 1, self.shared.part_start.load(Ordering::Relaxed))),
        };
        if step != self.step_seen {
            self.step_seen = step;
            self.step_time = now;
        }
        let step_dur = 15.0 / self.song.bpm as f64;
        let pos = step as f64 + ((now - self.step_time) / step_dur).min(0.999);
        if playing || self.hit_age.iter().any(|&a| a < 1.0) || self.filter != 0.0 {
            ctx.request_repaint();
        } else if self.capture {
            // The recording timer keeps counting while the music is paused
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        Live {
            playing,
            pos,
            hit_age: self.hit_age,
            build: self.shared.build_progress(),
            bar: step / STEPS as u64,
            part: part
                .filter(|_| self.song_form.is_some())
                .map(|(i, start)| (i, (pos / STEPS as f64 - start as f64).max(0.0))),
            caption: self.caption.as_ref().map(|(t, at)| (t.clone(), (now - at) as f32)),
            bands: self.shared.bands(),
            level: self.shared.level(),
        }
    }

    fn keys(&mut self, ctx: &egui::Context, now: f64) {
        // Naming a favorite: the keys belong to the text field
        if self.renaming.is_some() {
            return;
        }
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
            // Orbit → planets → classic → orbit
            self.view = match self.view {
                View::Orbit => View::Planets,
                View::Planets => View::Pads,
                View::Pads => View::Orbit,
            };
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
                self.settings = false;
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
                Key::ArrowRight => self.set_energy(self.song.macros.energy + 0.05, now),
                Key::ArrowLeft => self.set_energy(self.song.macros.energy - 0.05, now),
                Key::P => self.show(ctx, false),
                Key::O => self.show(ctx, true),
                Key::ArrowUp => self.song.bpm = (self.song.bpm + 1.0).min(180.0),
                Key::ArrowDown => self.song.bpm = (self.song.bpm - 1.0).max(90.0),
                Key::R => self.toggle_capture(now),
                _ => {}
            }
        }
    }

    /// Record the session: everything you hear until it's switched off, then save it as a WAV
    fn toggle_capture(&mut self, _now: f64) {
        if self.capture {
            self.capture = false;
            self.send(Cmd::Capture(false));
        } else {
            self.recording.clear();
            self.rec_live = true;
            self.rec_sr = self.audio.as_ref().map_or(44_100, |a| a.sample_rate);
            self.capture = true;
            self.send(Cmd::Capture(true));
        }
    }

    /// LIGHTS or FACE: opens that visualizer, switches to it, or closes it if it's on
    fn show(&mut self, ctx: &egui::Context, face: bool) {
        if self.visual && self.face != face {
            self.face = face;
        } else {
            self.face = face;
            self.toggle_visual(ctx);
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
                if icon_button(ui, ph::GEAR_SIX, "Settings: volume, sound output, MIDI, updates").clicked() {
                    self.settings = !self.settings;
                }
                let mut action = None;
                ui.menu_button(egui::RichText::new(format!("{}  File", ph::FOLDER_SIMPLE)).font(bold(13.0)), |ui| {
                    ui.set_min_width(230.0);
                    for (i, label) in [
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
                        egui::RichText::new(".bunker files keep the groove and all four patterns – to share or back up")
                            .font(body(11.0))
                            .color(FAINT),
                    );
                });
                match action {
                    Some(0) => self.save_file(now),
                    Some(1) => self.open_file(None, now),
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
                // No sound: say so right here (volume, device and MIDI are in the settings)
                // (tests and screenshots run without a sound card)
                if let Err(e) = &self.audio {
                    if self.with_devices {
                        let e = e.clone();
                        ui.label(
                            egui::RichText::new(format!("{}  No sound", ph::SPEAKER_SLASH))
                                .font(bold(13.0))
                                .color(RED),
                        )
                        .on_hover_text(e);
                    }
                }
                ui.add_space(8.0);
                // Record: everything you hear, to a WAV (red with a timer while it runs)
                let text = if self.capture {
                    let secs = self.recording.len() as u32 / 2 / self.rec_sr.max(1);
                    format!("{}  {}:{:02}", ph::RECORD, secs / 60, secs % 60)
                } else {
                    ph::RECORD.to_string()
                };
                if widgets::rec_chip(ui, &text, self.capture, now)
                    .on_hover_text(if self.capture {
                        "Recording – click to stop and save it as a WAV (R)"
                    } else {
                        "Record what you hear – auto mode, jamming, songs, effects – until you click again, then save it as a WAV (R)"
                    })
                    .clicked()
                {
                    self.toggle_capture(now);
                }
                ui.add_space(4.0);
                // Two visualizers for a projector: double-click or Esc comes back
                if chip(ui, ph::MASK_HAPPY, self.visual && self.face, RED)
                    .on_hover_text("FACE: a low-poly head that nods in time – it screams on the drop (O)")
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.show(&ctx, true);
                }
                if chip(ui, ph::SPARKLE, self.visual && !self.face, ACID)
                    .on_hover_text("LIGHTS: the whole screen becomes a light show (P)")
                    .clicked()
                {
                    let ctx = ui.ctx().clone();
                    self.show(&ctx, false);
                }
                ui.add_space(4.0);
                // The view: ORBIT, PLANETS or CLASSIC (pads and grid)
                let order = [View::Orbit, View::Planets, View::Pads];
                let picked = order.map(|v| self.view == v);
                // In a small window only the view you're in has its name
                let compact = ui.ctx().screen_rect().width() < 1250.0;
                let label = |i: usize, icon: &str, name: &str| {
                    if compact && !picked[i] {
                        icon.to_string()
                    } else {
                        format!("{icon}  {name}")
                    }
                };
                let views = [
                    (label(0, ph::CIRCLES_THREE, "ORBIT"), "Orbit: the tracks as rings (Tab)"),
                    (
                        label(1, ph::PLANET, "PLANETS"),
                        "Planets: every track a planet going round the sun (Tab)",
                    ),
                    (label(2, ph::GRID_FOUR, "CLASSIC"), "Classic: pads and a step grid (Tab)"),
                ];
                if let Some(i) = widgets::segmented(ui, &views, &picked, ACID) {
                    self.view = order[i];
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
            for v in Vibe::ALL.into_iter().chain([Vibe::Custom]) {
                if v == Vibe::Custom {
                    ui.separator();
                }
                let (lo, hi) = v.bpm();
                let mut job = egui::text::LayoutJob::default();
                let fmt = |font, color| egui::TextFormat {
                    font_id: font,
                    color,
                    ..Default::default()
                };
                job.append(v.name(), 0.0, fmt(bold(13.5), if v == vibe { ACID } else { TEXT }));
                if v != Vibe::Custom {
                    job.append(&format!("   {lo:.0}–{hi:.0} BPM"), 0.0, fmt(body(11.0), FAINT));
                }
                job.append(&format!("\n{}", v.blurb()), 0.0, fmt(body(11.5), DIM));
                if ui.selectable_label(v == vibe, job).clicked() {
                    if v == Vibe::Custom {
                        self.custom(now);
                    } else {
                        self.shuffle(v, now);
                    }
                }
            }
        });
        // Short on room (small window plus an update notice): just the icon
        let text = if ui.available_width() > 760.0 {
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

    /// Settings: volume, sound output, MIDI, update check, and the way to the code
    fn settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.settings;
        egui::Window::new("Settings")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(380.0)
            .anchor(Align2::RIGHT_TOP, vec2(-16.0, 64.0))
            .show(ctx, |ui| {
                let section = |ui: &mut Ui, title: &str| {
                    ui.add_space(4.0);
                    heading(ui, title);
                };
                section(ui, "VOLUME");
                ui.horizontal(|ui| {
                    widgets::bar_slider(ui, &mut self.song.master, 300.0, TEXT);
                    ui.label(egui::RichText::new(format!("{:.0}", self.song.master * 100.0)).font(bold(12.0)));
                });
                let sound = match &self.audio {
                    Ok(a) => format!("Sound: {} · {} Hz", a.device, a.sample_rate),
                    Err(e) => format!("No sound: {e}"),
                };
                ui.label(egui::RichText::new(sound).font(body(11.5)).color(DIM));
                ui.label(
                    egui::RichText::new("Plug in headphones any time – Bunker follows within a few seconds.")
                        .font(body(11.0))
                        .color(FAINT),
                );

                section(ui, "MIDI KEYBOARD");
                let names = self.midi.as_ref().filter(|m| m.is_connected()).map(|m| m.names.join(", "));
                ui.label(
                    egui::RichText::new(match &names {
                        Some(n) => format!("{}  {n}", ph::PIANO_KEYS),
                        None => format!("{}  None connected – plug one in, Bunker finds it", ph::PIANO_KEYS),
                    })
                    .font(bold(12.0))
                    .color(if names.is_some() { ACID } else { DIM }),
                );
                ui.label(
                    egui::RichText::new(
                        "Keys below middle C play the sounds (C kick, D clap, E rim, F hat, G open hat, A ride, B bass). \
                         Hold a key from middle C up to transpose bass and synth. Mod wheel = acid, pitch bend = filter, \
                         sustain = echo, knobs CC 70–73 (or 74/71/91/93) = energy, dark, acid, space.",
                    )
                    .font(body(11.0))
                    .color(FAINT),
                );

                section(ui, "UPDATES");
                ui.checkbox(&mut self.check_updates, "Tell me when a new version is out")
                    .on_hover_text("Asks GitHub once at start – nothing else is sent");

                ui.add_space(6.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(concat!("Bunker ", env!("CARGO_PKG_VERSION")))
                            .font(body(11.5))
                            .color(FAINT),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.hyperlink_to(format!("{}  github.com/tj537/bunker", ph::GITHUB_LOGO), update::REPO_URL);
                    });
                });
            });
        self.settings = open;
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
            // Low energy takes this track out (a break)
            let layer = crate::engine::LAYERS[i];
            let out = !muted && self.song.macros.energy < layer;
            let p = ui.painter().clone();
            if resp.hovered() {
                p.rect_filled(row, 10.0, PANEL_HI);
            }
            let top = row.top() + (row_h - 42.0) / 2.0;
            let dot = pos2(row.left() + 11.0, top + 12.0);
            glow(&p, dot, 5.0, color, if muted || out { 0.0 } else { 0.3 + flash });
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
                } else if out {
                    DIM
                } else {
                    TEXT
                },
            );
            ui.allocate_rect(name_rect, Sense::hover()).on_hover_text(voice.blurb());
            // …say so next to its name
            if out {
                let tag = Rect::from_min_size(pos2(name_rect.right() + 6.0, name_rect.top() + 1.5), vec2(30.0, 16.0));
                let cool = track_color(Voice::OpenHat);
                p.rect_filled(tag, 8.0, mix(PANEL, cool, 0.16));
                p.rect_stroke(tag, 8.0, stroke(1.0, alpha(cool, 0.7)), egui::StrokeKind::Inside);
                p.text(tag.center(), Align2::CENTER_CENTER, "OUT", bold(9.5), cool);
                ui.allocate_rect(tag, Sense::hover()).on_hover_text(format!(
                    "The energy is low, so {} sits out – it comes back at {:.0}",
                    voice.name(),
                    layer * 100.0
                ));
            }

            // Buttons on the right: solo, mute, dice, lock
            let mut x = row.right() - 2.0;
            let mut icon_btn = |ui: &mut Ui, icon: &str, tip: &str, on: bool, col: Color32| -> egui::Response {
                let r = Rect::from_min_size(pos2(x - 25.0, top), vec2(25.0, 22.0));
                x -= 26.0;
                let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tip);
                let hot = resp.hovered();
                if on {
                    ui.painter().rect_filled(r, 7.0, mix(PANEL, col, 0.22));
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

            // Second line: sound, volume
            let y = top + 23.0;
            let sound_rect = Rect::from_min_size(pos2(row.left() + 22.0, y), vec2(84.0, 18.0));
            let sound = ui.allocate_rect(sound_rect, Sense::click());
            let hot = sound.hovered();
            p.rect_filled(sound_rect, 9.0, mix(PANEL, color, if hot { 0.3 } else { 0.18 }));
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
            // Volume
            let vx = sound_rect.right() + 8.0;
            let vr = Rect::from_min_max(pos2(vx, y + 2.0), pos2(row.right() - 6.0, y + 16.0));
            let mut vol = self.song.tracks[i].volume;
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(vr), |ui| {
                widgets::bar_slider(ui, &mut vol, vr.width(), color).on_hover_text("Volume");
            });
            self.song.tracks[i].volume = vol;
        }

        ui.add_space(8.0);
        heading(ui, "MOOD");
        let mood = self.song.mood();
        let full_w = ui.available_width();
        for row in MOODS.chunks(3).enumerate() {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 5.0;
                let cw = (full_w - 10.0) / 3.0;
                for (j, &(name, blurb, scale, prog)) in row.1.iter().enumerate() {
                    let k = row.0 * 3 + j;
                    if chip_small(ui, name, mood == Some(k), ACID, cw).on_hover_text(blurb).clicked() && mood != Some(k) {
                        self.song.scale = scale;
                        self.song.prog = prog;
                        gen::refit_notes(&mut self.song, &mut self.rng);
                        // The synth carries the mood – a mood with it muted would hardly be heard
                        let synth = self.song.track_mut(Voice::Acid);
                        if !synth.lock {
                            synth.mute = false;
                        }
                        self.say(format!("{name} – bass and synth play it"), now);
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let tip = "How high or low bass and synth sit – drums stay where they are";
            ui.add_sized(
                vec2(52.0, 22.0),
                egui::Label::new(egui::RichText::new("Pitch").font(body(12.5)).color(DIM)).sense(Sense::hover()),
            )
            .on_hover_text(tip);
            if ui.small_button(ph::CARET_LEFT).on_hover_text("Lower").clicked() && self.song.root > 36 {
                self.song.root -= 1;
            }
            // Where in its range it sits: twelve steps from low to high
            let (r, _) = ui.allocate_exact_size(vec2(96.0, 22.0), Sense::hover());
            let seg = r.width() / 12.0;
            for k in 0..12 {
                let x = r.left() + seg * (k as f32 + 0.5);
                let on = k as u8 <= self.song.root - 36;
                ui.painter()
                    .circle_filled(pos2(x, r.center().y), 2.6, if on { ACID } else { PANEL_HI });
            }
            if ui.small_button(ph::CARET_RIGHT).on_hover_text("Higher").clicked() && self.song.root < 47 {
                self.song.root += 1;
            }
            let tr = self.shared.transpose.load(Ordering::Relaxed);
            if tr != 0 {
                ui.label(egui::RichText::new(format!("{tr:+}")).font(bold(12.0)).color(ACID))
                    .on_hover_text("Transposed by the MIDI keyboard");
            }
        });
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
        let sw = (w - 14.0) / 2.0;
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 14.0;
                for col in 0..2 {
                    let i = row * 2 + col;
                    let r = widgets::macro_slider(ui, &mut vals[i], def[i], names[i], colors[i], sw);
                    if r.hovered() || r.dragged() {
                        hint = Some(i);
                    }
                }
            });
        }
        if tall {
            let text = hint.map_or("Drag along a bar · double-click resets", |i| tips[i]);
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
            p.rect_filled(r, 12.0, if on { mix(PANEL, ACID, 0.14) } else { PANEL_HI });
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
        // How often it changes something – only there while the auto mode runs
        if self.song.auto {
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
        }

        ui.add_space(2.0);
        // AUTO FX: in the auto mode Bunker arranges by itself (energy, breaks, build-ups with effects, drops)
        ui.horizontal(|ui| {
            heading(ui, "LIVE");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // AUTO FX only means something in the auto mode
                let on = self.song.auto_fx;
                if self.song.auto
                    && chip_small(ui, "AUTO FX", on, ACID, 74.0)
                        .on_hover_text(
                            "On: in auto mode Bunker arranges by itself – the energy glides, breakdowns, build-ups \
                         with effects (low-pass, gate, echo) and drops. The energy bar only shows where it is.\n\
                         Off: the energy bar is yours – low = break, 85 = everything, 90 = drop. The auto mode \
                         still changes patterns, sounds and notes, but never takes a track out.",
                        )
                        .clicked()
                {
                    self.song.auto_fx = !on;
                    if on && self.song.auto {
                        // The bar decides now: everything Bunker took out comes back
                        for t in self.song.tracks.iter_mut().filter(|t| !t.lock) {
                            t.mute = false;
                        }
                    }
                    self.say(
                        if on {
                            "Auto FX off – the energy bar is yours: low = break, 85 = everything, 90 = drop"
                        } else {
                            "Auto FX on – Bunker plays breaks and drops by itself"
                        },
                        now,
                    );
                }
            });
        });
        // Energy: the big lever of the arrangement
        let mut e = self.song.macros.energy;
        // A song or the auto mode with AUTO FX steer the energy: then it's a slim display
        let locked = self.energy_locked();
        let h = if locked {
            30.0
        } else if tall {
            58.0
        } else {
            50.0
        };
        let label = if self.song_form.is_some() { "SONG" } else { "AUTO" };
        let out = widgets::energy_fader(ui, &mut e, w, h, locked.then_some(label));
        if out.changed {
            self.set_energy(e, now);
        }
        if out.locked_clicked {
            self.say(self.energy_hint(), now);
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
                    let lit = auto_fx(fx, live);
                    widgets::fx_button(ui, r, &resp, fx_icon(fx), fx.name(), fx.key(), fx_color(fx), active, lit);
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
                mix(mix(PANEL, RED, 0.3), RED, pulse * 0.5)
            } else if hot {
                mix(PANEL, RED, 0.3)
            } else {
                mix(PANEL, RED, 0.18)
            },
        );
        if let Some(b) = building {
            let fill = Rect::from_min_size(r.min, vec2(r.width() * b, r.height()));
            p.rect_filled(fill, 14.0, mix(PANEL, RED, 0.6));
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

    /// The strip under the orbit: twelve favorites in two rows, and AUTO SONG
    fn patterns_ui(&mut self, ui: &mut Ui, rect: Rect, live: &Live, now: f64) {
        let p = ui.painter().clone();
        let left_w = 96.0;
        p.text(rect.left_top() + vec2(4.0, 8.0), Align2::LEFT_TOP, "FAVORITES", bold(11.0), FAINT);
        let sub = if self.slots.iter().all(|s| s.is_none()) {
            "+ keeps what\nyou hear"
        } else {
            "click to switch\non the next bar"
        };
        p.text(rect.left_top() + vec2(4.0, 25.0), Align2::LEFT_TOP, sub, body(10.5), FAINT);
        let info = ui.allocate_rect(Rect::from_min_size(rect.min, vec2(left_w - 8.0, rect.height())), Sense::hover());
        info.on_hover_text(
            "Favorites are twelve memory slots for grooves.\n\
             • Click an empty one (+) to keep the groove you hear.\n\
             • Click a full one to switch to it – while playing it comes in on the next bar, with a clap roll.\n\
             • The lit one plays exactly as saved and keeps your changes; once the groove changes (auto mode) it goes dark.\n\
             • Right-click: save over it, rename or clear it.\n\
             • AUTO SONG plays a whole track from favorite 1 (and favorite 2 after the first drop).",
        );

        let song_w = 84.0;
        let cards = Rect::from_min_max(
            pos2(rect.left() + left_w, rect.top()),
            pos2(rect.right() - song_w - 10.0, rect.bottom()),
        );
        let (cols, gap) = (6, 6.0);
        let cw = (cards.width() - gap * (cols - 1) as f32) / cols as f32;
        let ch = (cards.height() - 8.0 - gap) / 2.0;
        for i in 0..SLOTS {
            let (row, col) = (i / cols, i % cols);
            let r = Rect::from_min_size(
                pos2(cards.left() + col as f32 * (cw + gap), cards.top() + 4.0 + row as f32 * (ch + gap)),
                vec2(cw, ch),
            );
            let resp = ui.allocate_rect(r, Sense::click());
            let filled = self.slots[i].is_some();
            let current = self.slot_now == Some(i);
            let queued = self.queued_slot == Some(i);
            let hot = resp.hovered();
            let num = (i + 1).to_string();
            let name = self.fav_name(i);
            if filled {
                if current && live.playing {
                    widgets::glow_rect(&p, r, 8.0, ACID, 0.12 + live.flash(0) * 0.25);
                }
                p.rect_filled(
                    r,
                    8.0,
                    if current {
                        mix(PANEL, ACID, 0.12)
                    } else if hot {
                        PANEL_HI
                    } else {
                        mix(PANEL, PANEL_HI, 0.6)
                    },
                );
                // Waiting for the next bar: the border blinks
                let border = if queued {
                    alpha(ACID, 0.4 + 0.6 * ((now * 8.0).sin() * 0.5 + 0.5) as f32)
                } else if current {
                    ACID
                } else {
                    LINE
                };
                p.rect_stroke(
                    r,
                    8.0,
                    stroke(if current || queued { 1.5 } else { 1.0 }, border),
                    egui::StrokeKind::Inside,
                );
                let label = if queued { "next bar…".to_string() } else { name.clone() };
                let (num_col, name_col) = if current {
                    (ACID, TEXT)
                } else if queued {
                    (TEXT, TEXT)
                } else {
                    (TEXT, DIM)
                };
                if cw >= 110.0 {
                    // Wide: number and name side by side
                    p.text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, &num, display(15.0), num_col);
                    let label = fit_text(&p, &label, bold(11.0), cw - 34.0);
                    p.text(r.left_center() + vec2(28.0, 0.0), Align2::LEFT_CENTER, label, bold(11.0), name_col);
                } else {
                    // Narrow: number on top, the name below it with the whole width
                    p.text(r.left_top() + vec2(7.0, 4.0), Align2::LEFT_TOP, &num, display(13.0), num_col);
                    let label = fit_text(&p, &label, bold(10.0), cw - 12.0);
                    p.text(r.left_bottom() + vec2(7.0, -5.0), Align2::LEFT_BOTTOM, label, bold(10.0), name_col);
                }
            } else {
                p.rect_stroke(r, 8.0, stroke(1.0, if hot { FAINT } else { LINE }), egui::StrokeKind::Inside);
                let label = if cw > 54.0 {
                    format!("{}  {num}", ph::PLUS)
                } else {
                    ph::PLUS.to_string()
                };
                p.text(r.center(), Align2::CENTER_CENTER, label, bold(12.0), if hot { TEXT } else { FAINT });
            }
            let tip = match (filled, current) {
                (false, _) => format!("Empty – click to keep the groove you hear now as favorite {num}"),
                (true, true) => {
                    format!("{num} · {name} is playing – your changes are kept in it.\nRight-click: save over it, rename or clear")
                }
                (true, false) => {
                    format!("{num} · {name}\nClick: switch to it (on the next bar)\nRight-click: save over it, rename or clear")
                }
            };
            let resp = resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
            if resp.clicked() {
                self.slot_clicked(i, false, now);
            }
            if filled {
                resp.context_menu(|ui| {
                    if ui.button(format!("{}  Save what you hear here", ph::FLOPPY_DISK)).clicked() {
                        self.slot_clicked(i, true, now);
                        ui.close_menu();
                    }
                    if ui.button(format!("{}  Rename…", ph::PENCIL_SIMPLE)).clicked() {
                        self.rename_buf = self.fav_name(i);
                        self.renaming = Some(i);
                        ui.close_menu();
                    }
                    if ui.button(format!("{}  Clear", ph::TRASH)).clicked() {
                        self.clear_slot(i, now);
                        ui.close_menu();
                    }
                });
            }
        }

        // AUTO SONG: a whole track out of favorites 1–4
        // A smaller button, centred next to the favorites
        let song_h = ((rect.height() - 8.0) * 0.62).max(44.0);
        let cr = Rect::from_center_size(pos2(rect.right() - song_w / 2.0, rect.center().y), vec2(song_w, song_h));
        let resp = ui.allocate_rect(cr, Sense::click());
        let on = self.song_form.is_some();
        widgets::fx_button(
            ui,
            cr,
            &resp,
            if on { ph::STOP } else { ph::PLAY_PAUSE },
            "AUTO SONG",
            "",
            ACID,
            on,
            0.0,
        );
        let popup = ui.make_persistent_id("forms");
        let resp = resp
            .on_hover_text(if on {
                "Click: stop the song – the groove keeps playing"
            } else {
                "Play a whole track from favorite 1 (favorite 2 after the first drop): intro, groove, break, drop, outro"
            })
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            if on {
                self.stop_song(now);
            } else {
                ui.memory_mut(|m| m.toggle_popup(popup));
            }
        }
        let full = self.song_favorites().map(|s| s.is_some());
        egui::popup::popup_above_or_below_widget(
            ui,
            popup,
            &resp,
            egui::AboveOrBelow::Above,
            egui::PopupCloseBehavior::CloseOnClick,
            |ui| {
                ui.set_min_width(360.0);
                ui.label(egui::RichText::new("Play a song from your favorite").font(bold(13.0)).color(TEXT));
                ui.label(
                    egui::RichText::new(
                        "Favorite 1 starts – quietly, then track by track. The first drop comes early; favorite 2 takes over after it and favorite 1 returns for the last one (with one favorite, each drop is a new chapter of it). Bunker steers the energy, varies the grooves, breaks and drops – you can jam along.",
                    )
                    .font(body(11.0))
                    .color(FAINT),
                );
                ui.add_space(4.0);
                for (k, f) in FORMS.iter().enumerate() {
                    let mins = f.bars() as f32 * 4.0 / self.song.bpm;
                    let parts: Vec<String> = f
                        .sections
                        .iter()
                        .map(|sec| {
                            let letter = crate::form::pattern_slot(sec.pattern, full).map_or("·".to_string(), |i| (i + 1).to_string());
                            format!("{} {letter}", sec.part.name())
                        })
                        .collect();
                    let mut job = egui::text::LayoutJob::default();
                    let fmt = |font, color| egui::TextFormat {
                        font_id: font,
                        color,
                        ..Default::default()
                    };
                    job.append(f.name, 0.0, fmt(bold(13.5), ACID));
                    job.append(&format!("   ~{:.0} min · {}\n", mins.max(1.0), f.blurb), 0.0, fmt(body(11.0), DIM));
                    job.append(&parts.join(" · "), 0.0, fmt(body(10.5), FAINT));
                    if ui.selectable_label(false, job).clicked() {
                        self.start_song(k, now);
                    }
                }
            },
        );
    }

    /// Song mode: the parts of the form, where it is, click to jump
    fn song_strip(&mut self, ui: &mut Ui, rect: Rect, live: &Live, now: f64) {
        let Some(form) = self.song_form.map(|f| &FORMS[f % FORMS.len()]) else {
            return;
        };
        let p = ui.painter().clone();
        let left_w = 96.0;
        p.text(rect.left_center() + vec2(4.0, -7.0), Align2::LEFT_CENTER, "SONG", bold(11.0), FAINT);
        p.text(
            rect.left_center() + vec2(4.0, 8.0),
            Align2::LEFT_CENTER,
            form.name,
            bold(12.0),
            ACID,
        );
        let full = self.song_favorites().map(|s| s.is_some());
        let area = Rect::from_min_max(pos2(rect.left() + left_w, rect.top()), rect.max);
        let gap = 3.0;
        let n = form.sections.len() as f32;
        let unit = (area.width() - gap * (n - 1.0)) / form.bars() as f32;
        let mut x = area.left();
        for (i, sec) in form.sections.iter().enumerate() {
            let w = unit * sec.bars as f32;
            let r = Rect::from_min_size(pos2(x, area.top()), vec2(w, area.height()));
            x += w + gap;
            let col = match sec.part {
                crate::form::Part::Intro | crate::form::Part::Outro => DIM,
                crate::form::Part::Groove => ACID,
                crate::form::Part::Break => Color32::from_rgb(69, 200, 255),
                crate::form::Part::Peak => RED,
            };
            let now_here = live.part.filter(|&(k, _)| k == i);
            let resp = ui.allocate_rect(r, Sense::click());
            let hot = resp.hovered();
            p.rect_filled(
                r,
                7.0,
                mix(
                    PANEL,
                    col,
                    if now_here.is_some() {
                        0.28
                    } else if hot {
                        0.2
                    } else {
                        0.12
                    },
                ),
            );
            if let Some((_, bars_in)) = now_here {
                let done = (bars_in as f32 / sec.bars as f32).clamp(0.0, 1.0);
                p.rect_filled(
                    Rect::from_min_size(r.min, vec2(r.width() * done, r.height())),
                    7.0,
                    mix(PANEL, col, 0.4),
                );
            }
            p.rect_stroke(
                r,
                7.0,
                stroke(
                    if now_here.is_some() { 1.5 } else { 1.0 },
                    alpha(col, if now_here.is_some() { 1.0 } else { 0.4 }),
                ),
                egui::StrokeKind::Inside,
            );
            let letter = crate::form::pattern_slot(sec.pattern, full).map_or(String::new(), |k| (k + 1).to_string());
            // The longest label that fits: "Peak C", "Peak", smaller "Peak", "P"
            let name = sec.part.name();
            let fits = |text: &str, size: f32| p.layout_no_wrap(text.into(), bold(size), TEXT).size().x + 10.0 <= w;
            let (label, size) = [
                (format!("{name} {letter}"), 11.0),
                (name.to_string(), 11.0),
                (name.to_string(), 9.0),
            ]
            .into_iter()
            .find(|(t, size)| fits(t, *size))
            .unwrap_or((name[..1].to_string(), 11.0));
            p.text(
                r.center(),
                Align2::CENTER_CENTER,
                label,
                bold(size),
                if now_here.is_some() { TEXT } else { alpha(col, 0.9) },
            );
            let tip = format!(
                "{} · {} bars{}\nClick: jump here on the next bar",
                sec.part.name(),
                sec.bars,
                if letter.is_empty() {
                    String::new()
                } else {
                    format!(" · pattern {letter}")
                }
            );
            if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.send(Cmd::Jump(i));
                self.say(format!("{} on the next bar", sec.part.name()), now);
            }
        }
    }

    /// A recorded song: save it as a WAV or let it go
    fn export_window(&mut self, ctx: &egui::Context, now: f64) {
        let mut save = false;
        let mut close = false;
        match &self.export {
            Export::Closed => return,
            Export::Recorded => {
                let secs = self.recording.len() as f32 / 2.0 / self.rec_sr as f32;
                egui::Window::new("Your recording is ready")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "{}:{:02} · everything you heard, live moves included · {} Hz stereo",
                                secs as u32 / 60,
                                secs as u32 % 60,
                                self.rec_sr
                            ))
                            .color(DIM),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            if ui
                                .button(egui::RichText::new(format!("{}  Save WAV…", ph::DOWNLOAD_SIMPLE)).font(bold(14.0)))
                                .clicked()
                            {
                                save = true;
                            }
                            if ui.button("Discard").clicked() {
                                close = true;
                            }
                        });
                    });
            }
            Export::Saving(rx) => match rx.try_recv() {
                Ok(Ok(path)) => {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    self.say(format!("Saved {name}"), now);
                    close = true;
                }
                Ok(Err(e)) => {
                    self.say(format!("Could not save: {e}"), now);
                    close = true;
                }
                Err(_) => {
                    egui::Window::new("Saving")
                        .collapsible(false)
                        .resizable(false)
                        .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                        .show(ctx, |ui| {
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Writing the WAV…");
                            });
                        });
                    ctx.request_repaint_after(std::time::Duration::from_millis(100));
                }
            },
        }
        if save {
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
                let samples = std::mem::take(&mut self.recording);
                let sr = self.rec_sr;
                std::thread::spawn(move || {
                    let _ = tx.send(crate::wav::write_i16(&path, &samples, sr).map(|_| path).map_err(|e| e.to_string()));
                });
                self.export = Export::Saving(rx);
            }
        } else if close {
            self.export = Export::Closed;
            self.recording = Vec::new();
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
                    row(ui, ph::PLAY, "1 · Press play", "Space, or the button in the middle. Pick a vibe at the top – every pick rolls a new groove in that style (Custom starts empty). SHUFFLE (Enter) rolls again. ⌘/Ctrl + Z undoes anything, a shuffle too.");
                    row(ui, ph::CIRCLES_THREE, "2 · Read the orbit", "Each ring is one sound, once round is one bar of 16 steps, the numbers 1–4 are the beats and the beam is where it plays. A hollow circle is a step that's off, a filled dot one that plays. A white core: accent (louder). A coloured ring around it: it only plays sometimes. An arc to the dot before: the synth slides into it. A white ring: playing right now. A faded ring: that track is muted. PLANETS at the top shows the same groove as a solar system: every track a planet that plays the beads it passes, notes as stalks – the higher, the higher.");
                    row(ui, ph::HAND_POINTING, "3 · Change the groove", "Click a dot to switch it on or off, drag to paint. Right-click a dot (or a tile in CLASSIC, the pads and step grid – Tab) for its menu: on, accent, slide, how often it plays and – on bass and synth – its note. The mouse wheel on a bass or synth dot moves its note too.");
                    row(ui, ph::LIST, "4 · Tracks", "S solos a track, the speaker mutes it, the dice rolls a new rhythm for it (right-click: clear), the lock keeps shuffle and the auto mode away from it. The coloured pill is its sound – click it for the list, tune and decay.");
                    row(ui, ph::MUSIC_NOTES, "5 · Mood & pitch", "MOOD picks the notes and chords of bass and synth – from Dark and Hypnotic to Euphoric, Sunny or Mysterious. Pitch moves them lower or higher.");
                    row(ui, ph::LIGHTNING, "6 · Energy & drop", "The energy bar is the arrangement, not the volume (← →): under 30 a break with no kick and bass, then the tracks come in one by one, from 70 everything plays (a track the energy takes out says OUT next to its name), at 90 the build-up starts and two bars later it drops. DROP (B) does the same any time.");
                    row(ui, ph::WAVEFORM, "7 · Effects & sound", "Hold STUTTER (Q), ECHO (W), WASH (E), CRUSH (X), GATE (C) or TAPE STOP (V) and combine them; FILTER springs back when you let go. SOUND: DARK, ACID, SPACE and PUMP. MASTER: low, mid, high and drive.");
                    row(ui, ph::INFINITY, "8 · Auto mode", "Bunker keeps the groove moving: something changes every 2, 4 or 8 bars. With AUTO FX on it also steers the energy and plays breakdowns, build-ups and drops by itself. With AUTO FX off the energy bar is yours and no track is ever taken out.");
                    row(ui, ph::STACK, "9 · Favorites & auto song", "Click + under the orbit to keep what you hear (twelve slots), click a full one to switch on the next bar, right-click to rename, save over or clear. AUTO SONG makes a whole track from favorite 1 – a quiet intro, two or three drops, favorite 2 after the first – while you jam along. Click a part in the strip to jump there.");
                    row(ui, ph::RECORD, "10 · Record", "The red record button at the top (R) records everything you hear until you click it again – then save it as a WAV. An AUTO SONG records itself.");
                    row(ui, ph::SPARKLE, "11 · Visualizers", "Two fullscreen shows for a projector, with a small energy bar at the bottom: LIGHTS (P) – a spectrum ring, shockwaves and stars – and FACE (O) – a low-poly head that nods in time, glitches in a build-up, and on the drop screams with laser eyes, cracks and bursts apart. All keys keep working. Esc or a double-click goes back.");
                    row(ui, ph::FOLDER_SIMPLE, "12 · Files & settings", "File → Save keeps the groove and your favorites in a .bunker file; open it from the menu or drag it onto the window. The gear holds volume, sound output, MIDI keyboard and the update check.");
                    row(ui, ph::KEYBOARD, "Keys", "Space play · Enter shuffle · A–K pads · 1–8 mute · ← → energy · ↑ ↓ tempo · B drop · Q W E  X C V effects (hold) · R record · Tab orbit/planets/classic · P lights · O face · ⌘/Ctrl Z undo · Esc close");
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
        eframe::set_value(storage, "fav_names", &self.names.to_vec());
        eframe::set_value(storage, "slot_now", &self.slot_now);
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
            egui::CentralPanel::default().frame(frame(SPACE, 0.0)).show(ctx, |ui| {
                let rect = ui.max_rect();
                if self.face {
                    crate::face::face(ui, rect, &self.song, &live, now);
                } else {
                    visual::visual(ui, rect, &self.song, &live, now);
                }
                // Double-click anywhere goes back
                if ui.interact(rect, ui.id().with("visual"), Sense::click()).double_clicked() {
                    self.toggle_visual(ctx);
                }
            });
            ctx.request_repaint();
            self.finish_frame(now);
            return;
        }
        // Bunker concrete behind everything; the panels sit on it
        concrete(&ctx.layer_painter(egui::LayerId::background()), ctx.screen_rect());
        let clear = Color32::TRANSPARENT;
        egui::TopBottomPanel::top("top")
            .show_separator_line(false)
            .exact_height(58.0)
            .frame(frame(clear, 6.0))
            .show(ctx, |ui| self.top_bar(ui, &live, now));
        egui::SidePanel::left("tracks")
            .show_separator_line(false)
            .exact_width(266.0)
            .resizable(false)
            .frame(frame(clear, 14.0))
            .show(ctx, |ui| self.tracks_panel(ui, &live, now));
        egui::SidePanel::right("live")
            .show_separator_line(false)
            .exact_width(286.0)
            .resizable(false)
            .frame(frame(clear, 14.0))
            .show(ctx, |ui| self.live_panel(ui, &live, now));
        egui::CentralPanel::default().frame(frame(clear, 10.0)).show(ctx, |ui| {
            let area = ui.available_rect_before_wrap();
            let scenes_h = 100.0;
            let song_h = if self.song_form.is_some() { 34.0 } else { 0.0 };
            let rect = Rect::from_min_max(area.min, pos2(area.right(), area.bottom() - scenes_h - song_h - 6.0));
            // A window into deep space, cut into the concrete: stars drifting past
            {
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 22.0, SPACE);
                let speed = if live.playing { 0.6 + self.song.macros.energy * 0.9 } else { 0.3 };
                // (kept clear of the rounded corners)
                widgets::star_field(ui, &p, rect.shrink(18.0), "space", speed, 1.0);
                if !live.playing {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
                }
            }
            // The cut edge: a dark rim and a faint light edge below it
            ui.painter()
                .rect_stroke(rect, 22.0, stroke(2.0, alpha(Color32::BLACK, 0.6)), egui::StrokeKind::Inside);
            ui.painter().rect_stroke(
                rect.expand(1.5),
                23.0,
                stroke(1.0, alpha(Color32::WHITE, 0.06)),
                egui::StrokeKind::Outside,
            );
            // The output level: a thin light along the window's lower edge, between its corners
            {
                let level = live.level.min(1.0);
                let (x0, x1, y) = (rect.left() + 22.0, rect.right() - 22.0, rect.bottom() - 1.0);
                let col = if level > 0.94 {
                    RED
                } else {
                    mix(ACID, Color32::from_rgb(255, 154, 60), level)
                };
                if level > 0.001 {
                    ui.painter()
                        .line_segment([pos2(x0, y), pos2(x0 + (x1 - x0) * level, y)], stroke(1.5, alpha(col, 0.8)));
                }
            }
            let inner = rect.shrink(18.0);
            match self.view {
                View::Orbit => {
                    let out = orbit::orbit(ui, inner, &mut self.song, &live, &mut self.paint);
                    if out.toggle_play {
                        self.toggle_play();
                    }
                }
                View::Planets => {
                    let out = crate::planets::planets(ui, inner, &mut self.song, &live, &mut self.paint);
                    if out.toggle_play {
                        self.toggle_play();
                    }
                }
                View::Pads => {
                    let out = pads::pads(ui, inner, &mut self.song, &live, &mut self.paint, &self.key_down);
                    for v in out.hits {
                        self.hit(v);
                    }
                }
            }
            let strip = Rect::from_min_max(pos2(area.left(), area.bottom() - scenes_h), area.max);
            self.patterns_ui(ui, strip, &live, now);
            if song_h > 0.0 {
                let r = Rect::from_min_max(pos2(area.left(), strip.top() - song_h + 2.0), pos2(area.right(), strip.top() - 4.0));
                self.song_strip(ui, r, &live, now);
            }
        });

        self.export_window(ctx, now);
        self.rename_window(ctx);
        self.settings_window(ctx);
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
                        if self.song_form.is_some() {
                            self.send_arrangement();
                        }
                    }
                }
            }
            self.sent = self.song;
            self.send(Cmd::Song(Box::new(self.song)));
            self.songs_sent += 1;
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

    /// The app wired to a real engine (no sound card): the engine renders on demand
    fn with_engine() -> (BunkerApp, crate::engine::Engine, Receiver<Cmd>) {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        let (tx, rx) = channel();
        let (ev_tx, events) = channel();
        app.tx = tx;
        app.events = events;
        let engine = crate::engine::Engine::new(44_100.0, app.song, app.shared.clone(), Some(ev_tx), 1);
        (app, engine, rx)
    }

    #[test]
    fn the_first_start_is_hypnotic_and_waits_for_play() {
        let (mut app, mut e, rx) = with_engine();
        let ctx = egui::Context::default();
        let mut buf = vec![0.0; 2 * 44_100];
        e.drain(&rx);
        e.render(&mut buf);
        app.sync(&ctx, 0.0, 0.016);
        assert_eq!(app.song.vibe, Vibe::Hypnotic);
        assert!(!app.playing() && !app.song.auto, "silent until play or Space");
        assert!(buf.iter().all(|s| s.abs() < 1e-3), "not a sound before play");
    }

    #[test]
    fn a_set_recording_ends_in_the_save_dialog() {
        let (mut app, mut e, rx) = with_engine();
        let ctx = egui::Context::default();
        let mut buf = vec![0.0; 2 * 44_100];
        let mut tick = |app: &mut BunkerApp, e: &mut crate::engine::Engine, secs: usize| {
            for _ in 0..secs {
                e.drain(&rx);
                e.render(&mut buf);
                app.sync(&ctx, 0.0, 0.016);
            }
        };
        app.toggle_capture(0.0);
        app.toggle_play();
        tick(&mut app, &mut e, 3);
        app.toggle_play(); // stop the music, then the recording
        tick(&mut app, &mut e, 1);
        app.toggle_capture(0.0);
        tick(&mut app, &mut e, 3);
        assert!(matches!(app.export, Export::Recorded), "the save dialog is up");
        let secs = app.recording.len() as f32 / 2.0 / app.rec_sr as f32;
        assert!(secs > 5.0, "{secs} s recorded");
    }

    #[test]
    fn a_favorite_goes_dark_once_the_groove_changes() {
        let (mut app, mut e, rx) = with_engine();
        let ctx = egui::Context::default();
        app.slot_clicked(0, true, 0.0);
        app.finish_frame(0.0);
        e.drain(&rx);
        assert_eq!(app.slot_now, Some(0));
        // Only the energy moves: still the saved groove (a build-up makes the engine send its song)
        e.handle(Cmd::Play);
        e.song.macros.energy = 0.2;
        e.handle(Cmd::Drop);
        app.sync(&ctx, 0.1, 0.016);
        assert!(app.song.macros.energy < 0.3, "the engine's song came in");
        assert_eq!(app.slot_now, Some(0), "energy is not the groove");
        // The auto mode changes a pattern: it's not the saved groove any more
        e.handle(Cmd::Stop);
        e.song.tracks[2].steps[5].on = !e.song.tracks[2].steps[5].on;
        e.handle(Cmd::Play);
        e.handle(Cmd::Drop);
        app.sync(&ctx, 0.2, 0.016);
        assert_eq!(app.slot_now, None);
    }

    #[test]
    fn song_mode_and_auto_mode_take_turns() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        // A song needs no saved patterns – it plays what you hear
        app.song.auto = true;
        app.start_song(1, 0.0);
        assert_eq!(app.song_form, Some(1));
        assert!(!app.song.auto && app.song.curve);
        app.toggle_auto(0.0);
        assert_eq!(app.song_form, None);
        app.start_song(0, 0.0);
        app.stop_song(0.0);
        assert_eq!(app.song_form, None);
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
    fn energy_by_hand_unless_auto_fx_steers_it() {
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        app.song.auto = true;
        app.song.auto_fx = true;
        let before = app.song.macros.energy;
        app.set_energy(0.2, 0.0);
        assert_eq!(app.song.macros.energy, before, "AUTO FX steers it");
        app.song.auto_fx = false;
        app.set_energy(0.2, 0.0);
        assert_eq!(app.song.macros.energy, 0.2);
        app.set_energy(3.0, 0.0);
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
        f.names[1] = "Sunday".into();
        app.song.master = 0.4;
        app.load_file(BunkerFile::from_text(&f.to_text()).unwrap(), "test.bunker", 0.0);
        assert_eq!(app.song.tracks, f.song.tracks);
        assert_eq!(app.song.master, 0.4, "volume belongs to the moment");
        assert_eq!(app.slots.to_vec(), f.scenes);
        assert_eq!(app.names[1], "Sunday");
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
