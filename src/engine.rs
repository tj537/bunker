//! The audio engine: sequencer clock, voices, mixer and master effects.
//!
//! It runs on the audio thread. The UI talks to it with [`Cmd`] messages and
//! reads the playhead and hit lights from [`Shared`] atomics; when the engine
//! itself changes the song (auto mode, recording) it sends the new song back
//! as an [`Event`]. The same engine renders WAV exports offline.

use crate::gen::{self, Change};
use crate::rng::Rng;
use crate::song::{Song, Voice, STEPS, TRACKS};
use crate::synth::*;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

pub enum Cmd {
    Song(Box<Song>),
    Play,
    Stop,
    /// Play a sound now (pads, MIDI); `record` writes it into the pattern
    Hit {
        voice: Voice,
        vel: f32,
        record: bool,
    },
    /// −1 (low-pass) … 0 (off) … +1 (high-pass)
    Filter(f32),
    Echo(bool),
    Stutter(bool),
    Drop,
    /// Semitones for bass and acid (MIDI keyboard)
    Transpose(i8),
}

pub enum Event {
    Song(Box<Song>, Option<Change>),
}

/// Lock-free state the UI reads every frame
pub struct Shared {
    pub playing: AtomicBool,
    /// Global index of the step that played last
    pub step: AtomicU64,
    /// Counts hits per track – the UI flashes when the number changes
    pub hits: [AtomicU32; TRACKS],
    /// Output peak (f32 bits)
    pub level: AtomicU32,
    /// Build-up progress in thousandths, 0 = no build-up
    pub build: AtomicU32,
    pub transpose: AtomicI32,
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Shared {
            playing: AtomicBool::new(false),
            step: AtomicU64::new(0),
            hits: Default::default(),
            level: AtomicU32::new(0),
            build: AtomicU32::new(0),
            transpose: AtomicI32::new(0),
        })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn build_progress(&self) -> Option<f32> {
        match self.build.load(Ordering::Relaxed) {
            0 => None,
            p => Some(p as f32 / 1000.0),
        }
    }
}

// Mixer: pan (−1…1), delay send, reverb send – per track, in Voice::ALL order
const PAN: [f32; TRACKS] = [0.0, 0.0, 0.3, -0.25, 0.22, -0.35, 0.0, 0.12];
const DELAY_SEND: [f32; TRACKS] = [0.0, 0.18, 0.4, 0.12, 0.18, 0.22, 0.0, 0.35];
const VERB_SEND: [f32; TRACKS] = [0.04, 0.55, 0.4, 0.15, 0.3, 0.35, 0.0, 0.25];

struct Build {
    start: u64,
    len: u64,
}

pub struct Engine {
    sr: f32,
    pub song: Song,
    playing: bool,
    /// Kick and bass were muted by the auto mode's breakdown
    breakdown: bool,
    phase: f64,
    last_fired: Option<u64>,
    half_fired: Option<u64>,
    rng: Rng,
    auto_rng: Rng,

    kick: Kick,
    clap: Clap,
    rim: Rim,
    hat: Hat,
    open_hat: Hat,
    ride: Hat,
    bass: Bass,
    acid: Acid,
    boom: Boom,

    delay: Delay,
    reverb: Reverb,
    rumble: Rumble,
    filter_l: Svf,
    filter_r: Svf,
    build_hp_l: Svf,
    build_hp_r: Svf,
    tone_l: Svf,
    tone_r: Svf,
    riser: Svf,
    crash: f32,
    filter: f32,
    filter_target: f32,
    echo: bool,
    echo_amount: f32,
    stutter: Option<u64>,
    build: Option<Build>,
    transpose: i8,
    limiter: f32,
    peak: f32,

    shared: Arc<Shared>,
    events: Option<Sender<Event>>,
}

impl Engine {
    pub fn new(sr: f32, song: Song, shared: Arc<Shared>, events: Option<Sender<Event>>, seed: u64) -> Self {
        Engine {
            sr,
            song,
            playing: false,
            breakdown: false,
            phase: 0.0,
            last_fired: None,
            half_fired: None,
            rng: Rng::new(seed),
            auto_rng: Rng::new(seed ^ 0xA5A5),
            kick: Kick::new(),
            clap: Clap::new(sr),
            rim: Rim::new(sr),
            hat: Hat::new(sr, HatKind::Closed),
            open_hat: Hat::new(sr, HatKind::Open),
            ride: Hat::new(sr, HatKind::Ride),
            bass: Bass::new(sr),
            acid: Acid::new(sr),
            boom: Boom::default(),
            delay: Delay::new(sr),
            reverb: Reverb::new(sr),
            rumble: Rumble::new(sr),
            filter_l: Svf::new(1000.0, 0.8, sr),
            filter_r: Svf::new(1000.0, 0.8, sr),
            build_hp_l: Svf::new(20.0, 0.7, sr),
            build_hp_r: Svf::new(20.0, 0.7, sr),
            tone_l: Svf::new(18_000.0, 0.6, sr),
            tone_r: Svf::new(18_000.0, 0.6, sr),
            riser: Svf::new(500.0, 3.0, sr),
            crash: 0.0,
            filter: 0.0,
            filter_target: 0.0,
            echo: false,
            echo_amount: 0.0,
            stutter: None,
            build: None,
            transpose: 0,
            limiter: 1.0,
            peak: 0.0,
            shared,
            events,
        }
    }

    pub fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Song(s) => {
                self.song = *s;
                self.song.sanitize();
                // Auto mode switched off in the middle of a breakdown: bring the beat back
                if self.breakdown && !self.song.auto {
                    self.end_breakdown();
                }
            }
            Cmd::Play => {
                if !self.playing {
                    self.playing = true;
                    self.phase = 0.0;
                    self.last_fired = None;
                    self.half_fired = None;
                }
            }
            Cmd::Stop => {
                self.playing = false;
                if self.breakdown {
                    self.end_breakdown();
                }
                self.build = None;
                self.stutter = None;
                self.shared.build.store(0, Ordering::Relaxed);
            }
            Cmd::Hit { voice, vel, record } => {
                // Pads play the root – the acid one octave up, where its lines live
                let note = if voice == Voice::Acid { 12 } else { 0 };
                self.trigger(voice, vel, false, note, 0, false);
                if record && self.playing {
                    self.record(voice);
                }
            }
            Cmd::Filter(x) => self.filter_target = x.clamp(-1.0, 1.0),
            Cmd::Echo(on) => self.echo = on,
            Cmd::Stutter(on) => {
                self.stutter = if on { Some(self.last_fired.map_or(0, |s| s + 1)) } else { None };
            }
            Cmd::Drop => {
                if self.playing && self.build.is_none() {
                    self.start_build();
                }
            }
            Cmd::Transpose(t) => {
                self.transpose = t;
                self.shared.transpose.store(t as i32, Ordering::Relaxed);
            }
        }
    }

    pub fn drain(&mut self, rx: &Receiver<Cmd>) {
        while let Ok(cmd) = rx.try_recv() {
            self.handle(cmd);
        }
    }

    fn step_len(&self) -> f64 {
        self.sr as f64 * 60.0 / self.song.bpm.clamp(60.0, 200.0) as f64 / 4.0
    }

    fn send_song(&self, change: Option<Change>) {
        if let Some(tx) = &self.events {
            let _ = tx.send(Event::Song(Box::new(self.song), change));
        }
    }

    /// Write a live hit into the nearest step of the pattern
    fn record(&mut self, voice: Voice) {
        let idx = self.phase.floor() as u64;
        let target = if self.phase.fract() < 0.5 { idx } else { idx + 1 };
        let t = self.song.track_mut(voice);
        let si = t.step_at(target);
        t.steps[si].on = true;
        t.steps[si].chance = 100;
        t.mute = false;
        // Recorded right after the step already played? Don't wait a whole loop to hear it
        self.send_song(None);
    }

    fn start_build(&mut self) {
        // Build-up until the end of the next bar, the drop lands on the bar after
        let next = self.last_fired.map_or(0, |s| s + 1);
        let bar_end = (next / STEPS as u64 + 1) * STEPS as u64;
        let end = bar_end + STEPS as u64;
        self.build = Some(Build {
            start: next,
            len: end - next,
        });
        self.send_song(Some(Change::Build));
    }

    fn build_progress(&self, idx: f64) -> Option<f32> {
        self.build
            .as_ref()
            .map(|b| ((idx - b.start as f64) / b.len as f64).clamp(0.0, 1.0) as f32)
    }

    fn end_breakdown(&mut self) {
        self.breakdown = false;
        self.song.track_mut(Voice::Kick).mute = false;
        self.song.track_mut(Voice::Bass).mute = false;
        self.send_song(None);
    }

    fn drop_now(&mut self) {
        self.build = None;
        self.breakdown = false;
        self.shared.build.store(0, Ordering::Relaxed);
        self.song.track_mut(Voice::Kick).mute = false;
        self.song.track_mut(Voice::Bass).mute = false;
        self.boom.trigger();
        self.crash = 1.0;
        if self.song.auto {
            gen::evolve(&mut self.song, &mut self.auto_rng);
        }
        self.send_song(Some(Change::Drop));
    }

    /// Bar starts: drops, and in auto mode the arrangement
    fn new_bar(&mut self, idx: u64) {
        let bar = idx / STEPS as u64;
        if let Some(b) = &self.build {
            if idx >= b.start + b.len {
                self.drop_now();
                return;
            }
        }
        if !self.song.auto || bar == 0 {
            return;
        }
        // 32-bar cycle: 24 bars groove (changing every few bars), 6 bars breakdown, 2 bars build-up, drop
        let b = bar % 32;
        if b == 24 {
            self.breakdown = true;
            self.song.track_mut(Voice::Kick).mute = true;
            self.song.track_mut(Voice::Bass).mute = true;
            self.send_song(Some(Change::Breakdown));
        } else if b == 30 && self.build.is_none() {
            self.build = Some(Build {
                start: idx,
                len: 2 * STEPS as u64,
            });
            self.send_song(Some(Change::Build));
        } else if b < 24 && bar.is_multiple_of(self.song.auto_bars.max(1) as u64) {
            let change = gen::evolve(&mut self.song, &mut self.auto_rng);
            self.send_song(Some(change));
        }
    }

    fn fire(&mut self, idx: u64) {
        if idx.is_multiple_of(STEPS as u64) {
            self.new_bar(idx);
        }
        self.shared.step.store(idx, Ordering::Relaxed);
        let build = self.build_progress(idx as f64);
        let play_idx = match self.stutter {
            Some(origin) if idx >= origin => origin + (idx - origin) % 2,
            _ => idx,
        };
        let step_len = self.step_len() as f32;
        for (ti, voice) in Voice::ALL.into_iter().enumerate() {
            let track = self.song.tracks[ti];
            if track.mute {
                continue;
            }
            if build.is_some() && matches!(voice, Voice::Kick | Voice::Bass | Voice::OpenHat) {
                continue;
            }
            let si = track.step_at(play_idx);
            let step = track.steps[si];
            if !step.on || (step.chance < 100 && self.rng.below(100) >= step.chance as u32) {
                continue;
            }
            let vel = track.volume * if step.accent { 1.0 } else { 0.72 };
            let next = track.steps[(si + 1) % track.len.max(1) as usize];
            let legato = voice == Voice::Acid && next.on && next.slide;
            let gate = if legato { step_len * 1.05 } else { step_len * 0.5 };
            self.trigger(voice, vel, step.accent, step.note, gate as u32, step.slide);
        }
        // Build-up: a clap roll that gets faster and louder
        if let Some(p) = build {
            if p < 0.5 && idx.is_multiple_of(2) || p >= 0.5 {
                let v = 0.35 + 0.55 * p;
                self.clap.trigger(v, 0.08, self.sr);
                self.count_hit(Voice::Clap);
            }
        }
    }

    fn count_hit(&self, v: Voice) {
        self.shared.hits[v.index()].fetch_add(1, Ordering::Relaxed);
    }

    /// Start a sound. `note` in semitones above the root (synths only)
    fn trigger(&mut self, voice: Voice, vel: f32, accent: bool, note: i8, gate: u32, slide: bool) {
        let sr = self.sr;
        let m = self.song.macros;
        let gate = if gate == 0 { self.step_len() as f32 * 0.6 } else { gate as f32 };
        let pitch = (self.song.root as i32 + note as i32 + self.transpose as i32) as f32;
        match voice {
            Voice::Kick => {
                self.kick.tune = 47.0 + (self.song.root % 12) as f32 * 0.6;
                self.kick.drive = 1.4
                    + m.energy * 1.4
                    + if self.song.vibe == crate::song::Vibe::Industrial {
                        1.5
                    } else {
                        0.0
                    };
                self.kick.trigger(vel, 0.11 + m.dark * 0.16, sr);
            }
            Voice::Clap => self.clap.trigger(vel * 1.3, 0.1 + m.space * 0.12, sr),
            Voice::Rim => self.rim.trigger(vel * 1.4, if accent { 1.06 } else { 1.0 }),
            Voice::Hat => {
                self.open_hat.choke(sr);
                self.hat.trigger(vel * 1.05, 0.03 + m.energy * 0.035, sr);
            }
            Voice::OpenHat => self.open_hat.trigger(vel * 0.85, 0.18 + m.energy * 0.2, sr),
            Voice::Ride => self.ride.trigger(vel * 0.9, 0.45 + m.space * 0.4, sr),
            Voice::Bass => self.bass.trigger(pitch, vel, gate, sr),
            Voice::Acid => {
                if slide {
                    self.acid.slide(pitch, accent, gate, sr);
                } else {
                    self.acid.trigger(pitch, vel, accent, gate, sr);
                }
            }
        }
        self.count_hit(voice);
    }

    /// Fill an interleaved stereo buffer
    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        let m = self.song.macros;
        let vol: [f32; TRACKS] = std::array::from_fn(|i| {
            let live = match Voice::ALL[i] {
                Voice::Hat | Voice::Ride | Voice::Rim => 0.65 + m.energy * 0.55,
                _ => 1.0,
            };
            live * if self.song.tracks[i].mute { 0.0 } else { 1.0 }
        });
        let step_len = self.step_len();
        let delay_time = (step_len * 3.0) as f32;
        let space = m.space;
        let dark = m.dark;
        // Darkness closes the master tone filter a bit
        let tone = 18_000.0 * (1.0 - dark * 0.65);
        self.tone_l.set(tone, 0.6, sr);
        self.tone_r.set(tone, 0.6, sr);

        for frame in out.chunks_exact_mut(2) {
            // ── Clock ──
            if self.playing {
                self.phase += 1.0 / step_len;
                let idx = self.phase.floor() as u64;
                let frac = self.phase - idx as f64;
                let swing = if idx % 2 == 1 { self.song.swing as f64 * 0.5 } else { 0.0 };
                if self.last_fired != Some(idx) && frac >= swing {
                    self.last_fired = Some(idx);
                    self.fire(idx);
                }
                // Last quarter of the build-up: 32nd-note rolls
                if let Some(p) = self.build_progress(self.phase) {
                    self.shared.build.store((p * 1000.0).max(1.0) as u32, Ordering::Relaxed);
                    if p > 0.75 && frac >= 0.5 && self.half_fired != Some(idx) {
                        self.half_fired = Some(idx);
                        self.clap.trigger(0.5 + 0.4 * p, 0.06, sr);
                    }
                }
            }

            // ── Voices ──
            let kick_raw = self.kick.next(sr, &mut self.rng);
            let duck = self.kick.envelope();
            let v = [
                kick_raw * vol[0],
                self.clap.next(sr, &mut self.rng) * vol[1],
                self.rim.next(sr, &mut self.rng) * vol[2],
                self.hat.next(sr, &mut self.rng) * vol[3],
                self.open_hat.next(sr, &mut self.rng) * vol[4],
                self.ride.next(sr, &mut self.rng) * vol[5],
                self.bass.next(sr, m.energy, dark) * vol[6] * 2.0 * (1.0 - duck * 0.7),
                self.acid.next(sr, m.acid) * vol[7] * (1.0 - duck * 0.3),
            ];
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let (mut dl, mut dr, mut rv) = (0.0f32, 0.0f32, 0.0f32);
            let echo_send = self.echo_amount;
            for i in 0..TRACKS {
                let x = v[i];
                let p = PAN[i];
                l += x * (1.0 - p).min(1.0);
                r += x * (1.0 + p).min(1.0);
                let ds = DELAY_SEND[i] * space * 1.2 + if i == 0 || i == 6 { 0.0 } else { echo_send };
                dl += x * ds * (1.0 - p * 0.5);
                dr += x * ds * (1.0 + p * 0.5);
                rv += x * VERB_SEND[i] * (0.25 + space);
            }
            // Rumble and the drop
            let rumble = self.rumble.run(kick_raw * vol[0], duck) * dark * 0.8;
            l += rumble;
            r += rumble;
            let boom = self.boom.next(sr) * 0.8;
            l += boom;
            r += boom;
            if self.crash > 1e-4 {
                let n = self.rng.noise() * self.crash;
                rv += n * 0.9;
                l += n * 0.12;
                r += self.rng.noise() * self.crash * 0.12;
                self.crash *= 0.99995;
            }
            // Build-up: a noise riser sweeping up
            let build = self.build_progress(self.phase).filter(|_| self.playing);
            if let Some(p) = build {
                self.riser.set(400.0 * 30f32.powf(p), 2.5, sr);
                let noise = self.rng.noise();
                let n = self.riser.run(noise).bp * p * p * 0.5;
                l += n;
                r += n;
                rv += n;
            }

            // ── Sends ──
            self.echo_amount += ((if self.echo { 0.9 } else { 0.0 }) - self.echo_amount) * 0.0005;
            let fb = 0.3 + space * 0.25 + self.echo_amount * 0.35;
            let (el, er) = self.delay.run(dl, dr, delay_time, fb.min(0.85));
            l += el;
            r += er;
            let (rl, rr) = self.reverb.run(rv, 0.55 + space * 0.4, 0.3 + dark * 0.5);
            l += rl;
            r += rr;

            // ── Master ──
            self.filter += (self.filter_target - self.filter) * 0.002;
            let f = self.filter;
            if f < -0.02 {
                let fc = 18_000.0 * 0.012f32.powf(-f);
                self.filter_l.set(fc, 1.6, sr);
                self.filter_r.set(fc, 1.6, sr);
                l = self.filter_l.run(l).lp;
                r = self.filter_r.run(r).lp;
            } else if f > 0.02 {
                let fc = 20.0 * 120f32.powf(f);
                self.filter_l.set(fc, 1.6, sr);
                self.filter_r.set(fc, 1.6, sr);
                l = self.filter_l.run(l).hp;
                r = self.filter_r.run(r).hp;
            }
            if let Some(p) = build {
                let fc = 20.0 * 80f32.powf(p);
                self.build_hp_l.set(fc, 0.9, sr);
                self.build_hp_r.set(fc, 0.9, sr);
                l = self.build_hp_l.run(l).hp;
                r = self.build_hp_r.run(r).hp;
            }
            l = self.tone_l.run(l).lp;
            r = self.tone_r.run(r).lp;

            // Glue: a little saturation, more with energy
            let drive = 0.9 + m.energy * 0.6;
            l = sat(l * drive * 0.7);
            r = sat(r * drive * 0.7);
            // Brick-wall-ish limiter
            let pk = l.abs().max(r.abs());
            let want = if pk > 0.95 { 0.95 / pk } else { 1.0 };
            self.limiter = if want < self.limiter {
                want
            } else {
                self.limiter + (want - self.limiter) * 0.0002
            };
            let g = self.limiter * self.song.master;
            l *= g;
            r *= g;
            self.peak = self.peak.max(l.abs().max(r.abs()));
            frame[0] = l;
            frame[1] = r;
        }
        // Peak meter with a slow fall
        let shown = f32::from_bits(self.shared.level.load(Ordering::Relaxed));
        let fallen = (shown * 0.85).max(self.peak);
        self.shared.level.store(fallen.to_bits(), Ordering::Relaxed);
        self.peak = 0.0;
        self.shared.playing.store(self.playing, Ordering::Relaxed);
    }
}

/// Render `bars` bars of the song (plus a reverb tail) into interleaved stereo
pub fn render_offline(song: &Song, bars: u32, sr: u32, seed: u64) -> Vec<f32> {
    let mut e = Engine::new(sr as f32, *song, Shared::new(), None, seed);
    e.handle(Cmd::Play);
    let frames = (e.step_len() * (bars as usize * STEPS) as f64) as usize + sr as usize * 2;
    let mut out = vec![0.0f32; frames * 2];
    let stop_at = (e.step_len() * (bars as usize * STEPS) as f64) as usize * 2;
    for (i, chunk) in out.chunks_mut(512).enumerate() {
        if i * 512 >= stop_at {
            e.handle(Cmd::Stop);
        }
        e.render(chunk);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::song::Vibe;

    fn stats(buf: &[f32]) -> (f32, f32) {
        let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let rms = (buf.iter().map(|x| x * x).sum::<f32>() / buf.len() as f32).sqrt();
        (peak, rms)
    }

    #[test]
    fn renders_loud_clean_audio_for_every_vibe() {
        for (i, vibe) in Vibe::ALL.into_iter().enumerate() {
            let song = gen::generate(vibe, &Default::default(), &mut Rng::new(i as u64));
            let buf = render_offline(&song, 4, 44_100, 1);
            assert!(buf.iter().all(|x| x.is_finite()));
            let (peak, rms) = stats(&buf);
            assert!(peak <= 1.0, "{vibe:?} clips: {peak}");
            assert!(rms > 0.05, "{vibe:?} too quiet: {rms}");
        }
    }

    #[test]
    fn steps_fire_on_time() {
        let mut song = Song::empty();
        song.bpm = 120.0;
        let shared = Shared::new();
        let mut e = Engine::new(48_000.0, song, shared.clone(), None, 1);
        e.handle(Cmd::Play);
        // 120 bpm → one 16th = 0.125 s = 6000 samples
        let mut buf = vec![0.0; 47_000 * 2];
        e.render(&mut buf);
        assert_eq!(shared.step.load(Ordering::Relaxed), 7);
        e.render(&mut buf[..2 * 2000]);
        assert_eq!(shared.step.load(Ordering::Relaxed), 8);
    }

    #[test]
    fn auto_mode_arranges_breakdown_build_and_drop() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(5));
        song.auto = true;
        song.auto_bars = 4;
        song.bpm = 200.0; // fast, to keep the test short
        let mut e = Engine::new(22_050.0, song, Shared::new(), Some(tx), 9);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar * 33];
        e.render(&mut buf);
        let changes: Vec<Change> = rx.try_iter().filter_map(|Event::Song(_, c)| c).collect();
        assert!(changes.contains(&Change::Breakdown), "{changes:?}");
        assert!(changes.contains(&Change::Build));
        assert_eq!(changes.last(), Some(&Change::Drop));
        // Changes every 4 bars before the breakdown
        assert!(changes.len() >= 5 + 3, "{changes:?}");
        assert!(!e.song.track(Voice::Kick).mute);
    }

    #[test]
    fn breakdown_ends_when_auto_mode_stops() {
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(5));
        song.auto = true;
        song.bpm = 200.0;
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 9);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar * 25 + 200];
        e.render(&mut buf); // into bar 24: breakdown
        assert!(e.song.track(Voice::Kick).mute);
        let mut off = e.song;
        off.auto = false;
        e.handle(Cmd::Song(Box::new(off)));
        assert!(!e.song.track(Voice::Kick).mute && !e.song.track(Voice::Bass).mute);

        // Same when the music stops
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 9);
        e.handle(Cmd::Play);
        e.render(&mut buf);
        e.handle(Cmd::Stop);
        assert!(!e.song.track(Voice::Kick).mute);
    }

    #[test]
    fn manual_drop_waits_for_the_bar_after_next() {
        let (tx, rx) = std::sync::mpsc::channel();
        let shared = Shared::new();
        let mut e = Engine::new(22_050.0, Song::default(), shared.clone(), Some(tx), 2);
        e.handle(Cmd::Play);
        let step = e.step_len() as usize;
        let mut buf = vec![0.0; 2 * step * 5];
        e.render(&mut buf); // into bar 0
        e.handle(Cmd::Drop);
        assert!(e.build.is_some());
        let mut buf = vec![0.0; 2 * step * 26];
        e.render(&mut buf);
        assert!(shared.build_progress().is_some());
        let mut buf = vec![0.0; 2 * step * 2];
        e.render(&mut buf);
        assert!(e.build.is_none(), "drop on step 32");
        let changes: Vec<Change> = rx.try_iter().filter_map(|Event::Song(_, c)| c).collect();
        assert_eq!(changes, vec![Change::Build, Change::Drop]);
    }

    #[test]
    fn recording_writes_into_the_pattern() {
        let mut song = Song::empty();
        song.bpm = 120.0;
        let (tx, rx) = std::sync::mpsc::channel();
        let mut e = Engine::new(48_000.0, song, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Play);
        let mut buf = vec![0.0; 2 * 6000 * 3 + 2 * 1000];
        e.render(&mut buf); // a little into step 3
        e.handle(Cmd::Hit {
            voice: Voice::Clap,
            vel: 1.0,
            record: true,
        });
        assert!(e.song.track(Voice::Clap).steps[3].on);
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn stutter_repeats_two_steps() {
        let mut song = Song::empty();
        song.bpm = 120.0;
        song.track_mut(Voice::Rim).steps[5].on = true;
        let shared = Shared::new();
        let mut e = Engine::new(48_000.0, song, shared.clone(), None, 1);
        e.handle(Cmd::Play);
        let mut buf = vec![0.0; 2 * 6000 * 4 + 20];
        e.render(&mut buf); // steps 0…4 fired
        e.handle(Cmd::Stutter(true)); // loops steps 5 and 6
        let before = shared.hits[Voice::Rim.index()].load(Ordering::Relaxed);
        let mut buf = vec![0.0; 2 * 6000 * 8];
        e.render(&mut buf);
        let rims = shared.hits[Voice::Rim.index()].load(Ordering::Relaxed) - before;
        assert_eq!(rims, 4, "step 5 repeats every other step");
    }
}

#[cfg(test)]
mod levels {
    use super::*;
    use crate::song::Vibe;

    /// Prints the loudness of each track on its own – for balancing the mix:
    /// `cargo test --release print_levels -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_levels() {
        let base = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(11));
        for (i, v) in Voice::ALL.into_iter().enumerate() {
            let mut s = base;
            for (j, t) in s.tracks.iter_mut().enumerate() {
                t.mute = j != i;
            }
            s.macros.space = 0.0;
            s.macros.dark = 0.0;
            let buf = render_offline(&s, 4, 44_100, 1);
            let n = buf.len() - 44_100 * 4; // without the tail
            let rms = (buf[..n].iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
            let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
            println!(
                "{:10} rms {:6.1} dB  peak {:6.1} dB",
                v.name(),
                20.0 * rms.log10(),
                20.0 * peak.log10()
            );
        }
    }
}
