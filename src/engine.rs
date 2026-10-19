//! The audio engine: sequencer clock, voices, mixer and master effects.
//!
//! It runs on the audio thread. The UI talks to it with [`Cmd`] messages and
//! reads the playhead and hit lights from [`Shared`] atomics; when the engine
//! itself changes the song (auto mode, recording, song mode) it sends the new
//! song back as an [`Event`]. The same engine renders WAV exports offline.

use crate::form::{pattern_slot, Part, FORMS};
use crate::gen::{self, Change};
use crate::rng::Rng;
use crate::song::{Song, Vibe, Voice, STEPS, TRACKS};
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
    /// A live effect held down (true) or let go
    Fx(Fx, bool),
    Drop,
    /// Semitones for bass and acid (MIDI keyboard)
    Transpose(i8),
    /// Song mode: play a form with the patterns (None: stop). The same form again only
    /// updates the patterns.
    Arrange(Option<Box<Arrangement>>),
    /// Song mode: go to this section on the next bar
    Jump(usize),
    /// Switch to this pattern (slot `usize`) on the next bar, with a fill before
    Queue(Box<Song>, usize),
}

pub enum Event {
    Song(Box<Song>, Option<Change>),
    /// Song mode records what plays: the next piece of interleaved stereo
    Audio(Vec<f32>),
    /// The recording is complete (song over or stopped, plus the tail)
    Recorded,
}

impl Event {
    #[cfg(test)]
    pub fn change(self) -> Option<Change> {
        match self {
            Event::Song(_, c) => c,
            _ => None,
        }
    }
}

/// Samples per recorded piece sent to the UI
const REC_CHUNK: usize = 32_768;

/// The live effects you hold down
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fx {
    Stutter,
    Echo,
    Wash,
    Crush,
    Gate,
    Tape,
}

impl Fx {
    pub const ALL: [Fx; 6] = [Fx::Stutter, Fx::Echo, Fx::Wash, Fx::Crush, Fx::Gate, Fx::Tape];

    pub fn name(self) -> &'static str {
        match self {
            Fx::Stutter => "STUTTER",
            Fx::Echo => "ECHO",
            Fx::Wash => "WASH",
            Fx::Crush => "CRUSH",
            Fx::Gate => "GATE",
            Fx::Tape => "TAPE STOP",
        }
    }

    /// Keyboard key (shown on the button)
    pub fn key(self) -> &'static str {
        match self {
            Fx::Stutter => "Q",
            Fx::Echo => "W",
            Fx::Wash => "E",
            Fx::Crush => "X",
            Fx::Gate => "C",
            Fx::Tape => "V",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Fx::Stutter => "Repeats two steps over and over",
            Fx::Echo => "Throws everything into a long echo",
            Fx::Wash => "Drowns the beat in a huge reverb",
            Fx::Crush => "Lo-fi: fewer bits, lower sample rate",
            Fx::Gate => "Chops the sound into 16th notes",
            Fx::Tape => "Slows down like a turntable switched off",
        }
    }

    pub fn index(self) -> usize {
        Fx::ALL.iter().position(|&f| f == self).unwrap_or(0)
    }
}

/// Song mode: a form (`form::FORMS`) played with the saved patterns
#[derive(Clone)]
pub struct Arrangement {
    pub patterns: [Option<Song>; 4],
    pub form: usize,
    /// What plays where no pattern is saved
    pub fallback: Song,
}

/// `section_start` of a song that starts on the next bar
const PENDING: u64 = u64::MAX;

/// Bands of the spectrum the visualizer shows (centre frequencies in Hz)
pub const BANDS: [f32; 8] = [55.0, 120.0, 260.0, 550.0, 1_200.0, 2_600.0, 5_500.0, 11_000.0];

/// Energy below which a track drops out (Voice::ALL order). The energy is the
/// arrangement, not the volume: below `BREAK_ZONE` it's a break (no kick, no bass),
/// then kick, bass, open hat and ride come in, and from `DROP_ZONE` up it drops.
pub const LAYERS: [f32; TRACKS] = [0.3, 0.2, 0.1, 0.0, 0.45, 0.55, 0.35, 0.0];

/// Below this energy the groove is a break: kick and bass out, the sound closes and
/// opens up into the reverb the lower it goes
pub const BREAK_ZONE: f32 = 0.3;

/// How far the energy may move per step when Bunker steers it (curve, song mode)
const GLIDE: f32 = 0.02;

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
    /// Song mode: section playing now + 1 (0 = no song) and the bar it started on
    pub part: AtomicU32,
    pub part_start: AtomicU64,
    pub transpose: AtomicI32,
    /// Loudness per `BANDS` band (f32 bits), for the visualizer
    pub bands: [AtomicU32; 8],
}

impl Shared {
    pub fn new() -> Arc<Self> {
        Arc::new(Shared {
            playing: AtomicBool::new(false),
            step: AtomicU64::new(0),
            hits: Default::default(),
            level: AtomicU32::new(0),
            build: AtomicU32::new(0),
            part: AtomicU32::new(0),
            part_start: AtomicU64::new(0),
            transpose: AtomicI32::new(0),
            bands: Default::default(),
        })
    }

    pub fn bands(&self) -> [f32; 8] {
        std::array::from_fn(|i| f32::from_bits(self.bands[i].load(Ordering::Relaxed)))
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
    style: BuildStyle,
}

/// How a build-up sounds on top of the groove turning into it. DROP by hand is always
/// classic; the auto mode and songs vary it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum BuildStyle {
    /// A high-pass opens upwards
    Classic,
    /// A low-pass slowly closes, the drop throws it open
    Lowpass,
    /// The gate chops harder and harder
    Gate,
    /// Classic, and the last hits are thrown into the echo for the silence
    Echo,
}

const STYLES: [BuildStyle; 5] = [
    BuildStyle::Classic,
    BuildStyle::Lowpass,
    BuildStyle::Gate,
    BuildStyle::Echo,
    BuildStyle::Lowpass,
];

/// How much of the mix goes into the reverb at full wash
const WASH_SEND: f32 = 1.2;

/// What a track plays at step `idx` of a build-up that started at `start` (progress `p`)
#[derive(Clone, Copy, PartialEq, Debug)]
enum BuildPlay {
    /// Its own pattern
    Pattern,
    Rest,
    /// A hit at this velocity, whatever the pattern says
    Hit(f32),
    /// The pattern looping a short piece from step `at`, `up` semitones higher
    Loop {
        at: u64,
        up: i8,
    },
}

/// The build-up made from the groove: in the first bar everything plays on, the kick
/// already rolling in quarters. In the second bar the kick rolls in 8ths, then 16ths,
/// the hats run in 16ths, the open hat rests, and bass, synth and percussion loop the
/// bar's first beat, then half a beat – the synths an octave up for the last stretch.
fn build_play(voice: Voice, p: f32, idx: u64, start: u64) -> BuildPlay {
    let second = start + STEPS as u64;
    let late = idx >= second;
    let last = p >= 0.75;
    match voice {
        Voice::Kick => {
            let every = if !late {
                4
            } else if !last {
                2
            } else {
                1
            };
            if (idx - start).is_multiple_of(every) {
                BuildPlay::Hit(0.6 + 0.4 * p)
            } else {
                BuildPlay::Rest
            }
        }
        Voice::Hat if late => BuildPlay::Hit(0.45 + 0.45 * (p - 0.5) * 2.0),
        Voice::OpenHat if late => BuildPlay::Rest,
        Voice::Bass | Voice::Acid | Voice::Rim | Voice::Ride if late => {
            let len = if last { 2 } else { 4 };
            let synth = matches!(voice, Voice::Bass | Voice::Acid);
            BuildPlay::Loop {
                at: second + (idx - second) % len,
                up: if last && synth { 12 } else { 0 },
            }
        }
        _ => BuildPlay::Pattern,
    }
}

/// Steps of silence before a drop lands (half a beat)
const DROP_GAP: u64 = 2;

/// Energy by hand pushed up to this starts a build-up; the drop lands two bars after
/// the next bar starts. It arms again once the energy is back below `DROP_REARM`.
pub const DROP_ZONE: f32 = 0.9;
const DROP_REARM: f32 = 0.88;

/// How the auto mode arranges one 32-bar cycle
#[derive(Clone, Copy)]
struct Plan {
    /// Bar of the cycle where the breakdown starts (it lasts until the build-up at bar 30)
    breakdown_at: u64,
    /// Tracks that leave for the breakdown
    out: &'static [Voice],
    /// A low-pass filter opens slowly through the breakdown
    sweep: bool,
}

const PLANS: [Plan; 4] = [
    // Classic: kick and bass out for six bars
    Plan {
        breakdown_at: 24,
        out: &[Voice::Kick, Voice::Bass],
        sweep: false,
    },
    // Short and filtered: only the kick leaves, for two bars
    Plan {
        breakdown_at: 28,
        out: &[Voice::Kick],
        sweep: true,
    },
    // Stripped: just hats, percussion and the synth line
    Plan {
        breakdown_at: 24,
        out: &[Voice::Kick, Voice::Bass, Voice::Clap, Voice::Ride],
        sweep: false,
    },
    // Filtered: no kick, everything else under a slowly opening filter
    Plan {
        breakdown_at: 24,
        out: &[Voice::Kick],
        sweep: true,
    },
];

struct Breakdown {
    start: u64,
    len: u64,
    out: &'static [Voice],
    /// Mute state of every track before the breakdown
    before: [bool; TRACKS],
    sweep: bool,
}

/// A one-bar fill at the end of an 8-bar phrase (auto mode)
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Fill {
    /// Clap roll into the next phrase
    Roll,
    /// Kick and bass drop out for the last beat
    Gap,
    /// Falling toms
    Toms,
    /// Open hats on every 8th
    OpenHats,
    /// The clap thrown into the echo
    Throw,
}

const FILLS: [Fill; 5] = [Fill::Roll, Fill::Gap, Fill::Toms, Fill::OpenHats, Fill::Throw];

/// A stable pseudo-random number 0…1 for a key – same key, same number
fn hash01(key: u64) -> f32 {
    let mut z = key.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) >> 40) as f32 / (1u64 << 24) as f32
}

pub struct Engine {
    sr: f32,
    pub song: Song,
    playing: bool,
    breakdown: Option<Breakdown>,
    plan: Plan,
    /// Auto mode: the bar the current cycle started on and its length in bars
    cycle_start: u64,
    cycle_len: u64,
    fill: Option<(u64, Fill)>,
    arrangement: Option<Box<Arrangement>>,
    /// Song mode: the section playing, the bar it started on (`PENDING`: next bar) and a jump
    section: usize,
    section_start: u64,
    jump: Option<usize>,
    /// A pattern waiting for the next bar
    queued: Option<(Box<Song>, usize)>,
    /// The slot whose pattern plays (as far as the engine changed it)
    pattern_now: Option<usize>,
    /// The bar playing now (chord progression)
    bar_now: u64,
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
    /// Held live effects, in `Fx::ALL` order
    fx: [bool; 6],
    echo_amount: f32,
    /// Echo throw from a fill, fades out by itself
    throw: f32,
    wash: f32,
    /// The wash's room: grows with it, shrinks slowly after, so the tail rings out
    wash_room: f32,
    /// Keeps kick and bass out of the wash, so it drowns instead of booming
    wash_hp: Svf,
    crush: f32,
    crush_count: f32,
    crush_hold: (f32, f32),
    gate: f32,
    gate_env: f32,
    /// Tape stop: the last two seconds of output, read back slower and slower
    tape: Vec<f32>,
    tape_w: usize,
    tape_r: f64,
    tape_rate: f32,
    taping: bool,
    /// Pump: everything ducks under the kick
    pump_env: f32,
    eq_low: [Svf; 2],
    eq_mid: [Svf; 2],
    eq_high: [Svf; 2],
    /// Spectrum for the visualizer: one band-pass and envelope per band
    band_filters: [Svf; 8],
    band_env: [f32; 8],
    band_peak: [f32; 8],
    stutter: Option<u64>,
    build: Option<Build>,
    /// Build-up filter amount; it lets go quickly after the drop instead of jumping
    build_cut: f32,
    /// The style of the last build-up – its filter lets go in the same style
    cut_style: BuildStyle,
    riser_gain: f32,
    /// The energy was below the drop zone since the last drop
    zone_armed: bool,
    /// Where the energy curve (auto, song) wants to go – the energy glides there step by step
    energy_goal: f32,
    /// The energy the layers follow: taken on every beat, so tracks come and go in time
    layer_energy: f32,
    /// The energy smoothed for the sound (break filter, space, bass brightness)
    energy_smooth: f32,
    break_lp_l: Svf,
    break_lp_r: Svf,
    /// Auto mode: drops since the last new chapter
    drops_since_chapter: u32,
    /// AUTO FX: an effect at the end of a phrase – bar, effect, from step
    fx_moment: Option<(u64, Fx, u64)>,
    /// The stutter running now was started by an effect moment
    auto_stutter: bool,
    transpose: i8,
    limiter: f32,
    peak: f32,

    shared: Arc<Shared>,
    events: Option<Sender<Event>>,
    /// Song mode recording: on while a song plays, then `rec_tail` more frames for the reverb
    recording: bool,
    rec: Vec<f32>,
    rec_tail: u32,
}

impl Engine {
    pub fn new(sr: f32, song: Song, shared: Arc<Shared>, events: Option<Sender<Event>>, seed: u64) -> Self {
        Engine {
            sr,
            song,
            playing: false,
            breakdown: None,
            plan: PLANS[0],
            cycle_start: 0,
            cycle_len: 32,
            fill: None,
            arrangement: None,
            section: 0,
            section_start: PENDING,
            jump: None,
            queued: None,
            pattern_now: None,
            bar_now: 0,
            phase: 0.0,
            last_fired: None,
            half_fired: None,
            rng: Rng::new(seed),
            auto_rng: Rng::new(seed ^ 0xA5A5),
            kick: Kick::new(),
            clap: Clap::new(sr),
            rim: Rim::new(sr),
            hat: Hat::new(sr, &HATS[0]),
            open_hat: Hat::new(sr, &OPEN_HATS[0]),
            ride: Hat::new(sr, &RIDES[0]),
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
            fx: [false; 6],
            echo_amount: 0.0,
            throw: 0.0,
            wash: 0.0,
            wash_room: 0.0,
            wash_hp: Svf::new(280.0, 0.7, sr),
            crush: 0.0,
            crush_count: 0.0,
            crush_hold: (0.0, 0.0),
            gate: 0.0,
            gate_env: 1.0,
            tape: vec![0.0; (sr * 2.0) as usize * 2],
            tape_w: 0,
            tape_r: 0.0,
            tape_rate: 1.0,
            taping: false,
            pump_env: 0.0,
            eq_low: [Svf::new(110.0, 0.7, sr), Svf::new(110.0, 0.7, sr)],
            eq_mid: [Svf::new(1_000.0, 0.8, sr), Svf::new(1_000.0, 0.8, sr)],
            eq_high: [Svf::new(7_000.0, 0.7, sr), Svf::new(7_000.0, 0.7, sr)],
            band_filters: BANDS.map(|f| Svf::new(f, 1.4, sr)),
            band_env: [0.0; 8],
            band_peak: [0.0; 8],
            stutter: None,
            build: None,
            build_cut: 0.0,
            cut_style: BuildStyle::Classic,
            riser_gain: 0.0,
            zone_armed: false,
            energy_goal: song.macros.energy,
            layer_energy: song.macros.energy,
            energy_smooth: song.macros.energy,
            break_lp_l: Svf::new(18_000.0, 0.7, sr),
            break_lp_r: Svf::new(18_000.0, 0.7, sr),
            drops_since_chapter: 0,
            fx_moment: None,
            auto_stutter: false,
            transpose: 0,
            limiter: 1.0,
            peak: 0.0,
            shared,
            events,
            recording: false,
            rec: Vec::new(),
            rec_tail: 0,
        }
    }

    pub fn handle(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::Song(s) => {
                // With the energy curve on (auto or song mode), the engine owns the energy
                let energy = self.song.macros.energy;
                self.song = *s;
                if (self.song.auto || self.arrangement.is_some()) && self.song.curve {
                    self.song.macros.energy = energy;
                } else {
                    self.watch_drop_zone();
                }
                self.song.sanitize();
                // Auto mode switched off in the middle of a breakdown: bring the beat back
                if self.breakdown.is_some() && !self.song.auto && self.arrangement.is_none() {
                    self.end_breakdown();
                }
            }
            Cmd::Play => {
                if !self.playing {
                    self.playing = true;
                    self.phase = 0.0;
                    self.last_fired = None;
                    self.half_fired = None;
                    self.fill = None;
                    self.plan = PLANS[0];
                    self.cycle_start = 0;
                    self.cycle_len = 32;
                }
            }
            Cmd::Stop => {
                self.playing = false;
                self.arrangement = None;
                self.end_recording();
                self.queued = None;
                self.shared.part.store(0, Ordering::Relaxed);
                if self.breakdown.is_some() {
                    self.end_breakdown();
                }
                self.build = None;
                self.stutter = None;
                self.fill = None;
                self.shared.build.store(0, Ordering::Relaxed);
            }
            Cmd::Hit { voice, vel, record } => {
                // Pads play the root – the synth one octave up, where its lines live
                let note = if voice == Voice::Acid { 12 } else { 0 };
                self.trigger(voice, vel, false, note, 0, false);
                if record && self.playing {
                    self.record(voice);
                }
            }
            Cmd::Filter(x) => self.filter_target = x.clamp(-1.0, 1.0),
            Cmd::Fx(fx, on) => {
                self.fx[fx.index()] = on;
                if fx == Fx::Stutter {
                    self.stutter = if on { Some(self.last_fired.map_or(0, |s| s + 1)) } else { None };
                }
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
            Cmd::Arrange(Some(a)) => {
                let same = self.arrangement.as_ref().is_some_and(|old| old.form == a.form);
                if !same {
                    if self.breakdown.is_some() {
                        self.end_breakdown();
                    }
                    self.section = 0;
                    self.section_start = PENDING;
                    self.jump = None;
                }
                self.arrangement = Some(a);
            }
            Cmd::Arrange(None) => {
                if self.arrangement.take().is_some() && self.breakdown.is_some() {
                    self.end_breakdown();
                }
                self.end_recording();
                self.shared.part.store(0, Ordering::Relaxed);
            }
            Cmd::Jump(i) => self.jump = Some(i),
            Cmd::Queue(s, slot) => {
                if self.playing {
                    self.queued = Some((s, slot));
                    // A clap roll leads into it, unless a build-up is on
                    let bar = self.last_fired.map_or(0, |i| i / STEPS as u64);
                    if self.build.is_none() {
                        self.fill = Some((bar, Fill::Roll));
                    }
                } else {
                    self.load_pattern(*s, Some(slot));
                    self.send_song(Some(Change::Scene(slot)));
                }
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
        // Two bars of build-up from the next bar on, so it always starts and lands on the one
        let next = self.last_fired.map_or(0, |s| s + 1);
        self.build = Some(Build {
            start: next.div_ceil(STEPS as u64) * STEPS as u64,
            len: 2 * STEPS as u64,
            style: BuildStyle::Classic,
        });
        self.send_song(Some(Change::Build));
    }

    /// A build-up the arrangement starts now (auto mode, song mode), in a random style
    fn auto_build(&mut self, idx: u64) {
        self.build = Some(Build {
            start: idx,
            len: 2 * STEPS as u64,
            style: if self.song.auto_fx {
                *self.auto_rng.pick(&STYLES)
            } else {
                BuildStyle::Classic
            },
        });
        self.send_song(Some(Change::Build));
    }

    /// 0…1 through the build-up; None before it starts
    fn build_progress(&self, idx: f64) -> Option<f32> {
        self.build
            .as_ref()
            .filter(|b| idx >= b.start as f64)
            .map(|b| ((idx - b.start as f64) / b.len as f64).clamp(0.0, 1.0) as f32)
    }

    /// The breath before the drop: the last half beat of the build-up
    fn in_drop_gap(&self, idx: f64) -> bool {
        self.build.as_ref().is_some_and(|b| idx >= (b.start + b.len - DROP_GAP) as f64)
    }

    /// Energy by hand pushed into the drop zone (90 and up) builds up and drops
    fn watch_drop_zone(&mut self) {
        let e = self.song.macros.energy;
        if e < DROP_REARM {
            self.zone_armed = true;
        } else if e >= DROP_ZONE && self.zone_armed {
            self.zone_armed = false;
            if self.playing && self.build.is_none() {
                self.start_build();
            }
        }
    }

    /// The last bar of a phrase: a fill – or, with AUTO FX, now and then an effect moment
    fn phrase_end(&mut self, bar: u64) {
        if self.song.auto_fx && self.auto_rng.chance(0.4) {
            let (fx, from) = *self
                .auto_rng
                .pick(&[(Fx::Stutter, 12u64), (Fx::Gate, 8), (Fx::Crush, 12), (Fx::Echo, 12)]);
            self.fx_moment = Some((bar, fx, from));
            self.fill = None;
        } else {
            self.fill = Some((bar, *self.auto_rng.pick(&FILLS)));
        }
    }

    /// An effect moment on now? (`last_fired` step)
    fn moment(&self, fx: Fx) -> bool {
        match (self.fx_moment, self.last_fired) {
            (Some((bar, f, from)), Some(i)) => f == fx && i / STEPS as u64 == bar && i % STEPS as u64 >= from,
            _ => false,
        }
    }

    fn start_breakdown(&mut self, idx: u64) {
        let plan = self.plan;
        let before = self.song.tracks.map(|t| t.mute);
        for &v in plan.out {
            self.song.track_mut(v).mute = true;
        }
        self.breakdown = Some(Breakdown {
            start: idx,
            len: (self.cycle_len - 2).saturating_sub(self.breakdown_at()).max(1) * STEPS as u64,
            out: plan.out,
            before,
            sweep: plan.sweep,
        });
        self.send_song(Some(Change::Breakdown));
    }

    /// Bring back what the breakdown took out
    fn restore_breakdown(&mut self) {
        if let Some(b) = self.breakdown.take() {
            for &v in b.out {
                self.song.track_mut(v).mute = b.before[v.index()];
            }
        }
    }

    fn end_breakdown(&mut self) {
        self.restore_breakdown();
        self.send_song(None);
    }

    fn drop_now(&mut self) {
        self.build = None;
        self.restore_breakdown();
        self.shared.build.store(0, Ordering::Relaxed);
        self.song.track_mut(Voice::Kick).mute = false;
        self.song.track_mut(Voice::Bass).mute = false;
        self.boom.trigger();
        self.crash = 1.0;
        // Energy turned down before the drop? It comes back with it
        let m = &mut self.song.macros;
        m.energy = m.energy.max(0.85);
        let mut change = Change::Drop;
        if self.song.auto {
            self.new_cycle(self.bar_now);
            // A new chapter only every few drops – the set keeps its identity for a while
            if self.drops_since_chapter >= 2 && self.auto_rng.chance(0.5) {
                change = gen::new_chapter(&mut self.song, &mut self.auto_rng);
                self.drops_since_chapter = 0;
            } else {
                gen::evolve(&mut self.song, &mut self.auto_rng);
                self.drops_since_chapter += 1;
            }
            if self.song.curve {
                self.song.macros.energy = self.curve_target(0);
            }
        }
        // The drop lands at once; from here the curve glides on
        self.energy_goal = self.song.macros.energy;
        self.layer_energy = self.song.macros.energy;
        self.send_song(Some(change));
    }

    /// Put a pattern in place, keeping the live settings (volume, EQ, energy …); a running
    /// break keeps its tracks out
    fn load_pattern(&mut self, mut s: Song, slot: Option<usize>) {
        s.master = self.song.master;
        s.auto = self.song.auto && self.arrangement.is_none();
        s.auto_bars = self.song.auto_bars;
        s.curve = self.song.curve;
        s.auto_fx = self.song.auto_fx;
        s.eq = self.song.eq;
        s.macros.energy = self.song.macros.energy;
        s.sanitize();
        if let Some(b) = &mut self.breakdown {
            b.before = s.tracks.map(|t| t.mute);
            for &v in b.out {
                s.track_mut(v).mute = true;
            }
        }
        self.song = s;
        self.pattern_now = slot;
    }

    /// Song mode at a bar start: the next section when one is over (or on a jump), the
    /// energy curve, a break's build-up, fills
    fn arrange_bar(&mut self, idx: u64, bar: u64, dropped: bool) {
        let Some(a) = &self.arrangement else { return };
        let form = &FORMS[a.form % FORMS.len()];
        let full = a.patterns.map(|p| p.is_some());
        let over = self.section_start == PENDING
            || self.jump.is_some()
            || bar >= self.section_start + form.sections[self.section.min(form.sections.len() - 1)].bars as u64;
        if over {
            let next = match self.jump.take() {
                Some(j) => j,
                None if self.section_start == PENDING => self.section,
                None => self.section + 1,
            };
            if next >= form.sections.len() {
                self.end_song();
            } else {
                self.enter_section(next, idx, bar, dropped);
            }
            return;
        }
        let sec = form.sections[self.section];
        let (played, n) = (bar - self.section_start, sec.bars as u64);
        if self.song.curve {
            self.energy_goal = sec.part.energy(played, n);
        }
        if sec.part == Part::Break && played + 2 == n && self.build.is_none() {
            self.auto_build(idx);
        } else if played + 1 == n && sec.part != Part::Break {
            // A clap roll into the next pattern
            let next = form.sections.get(self.section + 1).map(|nx| pattern_slot(nx.pattern, full));
            if next.is_some_and(|slot| slot != self.pattern_now) {
                self.fill = Some((bar, Fill::Roll));
            }
        } else if matches!(sec.part, Part::Groove | Part::Peak)
            && (played + 1).is_multiple_of(8)
            && played + 1 < n
            && self.auto_rng.chance(0.7)
        {
            self.phrase_end(bar);
        }
    }

    fn enter_section(&mut self, i: usize, idx: u64, bar: u64, dropped: bool) {
        let Some(a) = &self.arrangement else { return };
        let sec = FORMS[a.form % FORMS.len()].sections[i];
        let slot = pattern_slot(sec.pattern, a.patterns.map(|p| p.is_some()));
        let pattern = slot.and_then(|k| a.patterns[k]).unwrap_or(a.fallback);
        let first = self.section_start == PENDING;
        if first && self.events.is_some() {
            self.finish_recording();
            self.recording = true;
            self.rec = Vec::with_capacity(REC_CHUNK + 4096);
        }
        // A jump out of a break: kick and bass come back, its build-up is off
        if self.breakdown.is_some() {
            self.restore_breakdown();
        }
        if !dropped && self.build.is_some() {
            self.build = None;
            self.shared.build.store(0, Ordering::Relaxed);
        }
        self.section = i;
        self.section_start = bar;
        if first || slot != self.pattern_now {
            self.load_pattern(pattern, slot);
        }
        match sec.part {
            Part::Break => {
                let before = self.song.tracks.map(|t| t.mute);
                let out: &'static [Voice] = &[Voice::Kick, Voice::Bass];
                for &v in out {
                    self.song.track_mut(v).mute = true;
                }
                self.breakdown = Some(Breakdown {
                    start: idx,
                    len: sec.bars as u64 * STEPS as u64,
                    out,
                    before,
                    sweep: false,
                });
            }
            Part::Peak if !dropped => self.drop_now(),
            _ => {}
        }
        self.song.curve = true;
        self.energy_goal = sec.part.energy(0, sec.bars as u64);
        if sec.part == Part::Peak {
            self.song.macros.energy = self.energy_goal;
            self.layer_energy = self.energy_goal;
        }
        self.shared.part.store(i as u32 + 1, Ordering::Relaxed);
        self.shared.part_start.store(bar, Ordering::Relaxed);
        self.send_song(Some(Change::Part(sec.part, i, slot)));
    }

    /// The song is over: two more seconds for the tail, then the recording is complete
    fn end_recording(&mut self) {
        if self.recording && self.rec_tail == 0 {
            self.rec_tail = (self.sr * 2.0) as u32;
        }
    }

    /// Send what's left of the recording and say it's complete
    fn finish_recording(&mut self) {
        if !self.recording {
            return;
        }
        self.recording = false;
        self.rec_tail = 0;
        if let Some(tx) = &self.events {
            let _ = tx.send(Event::Audio(std::mem::take(&mut self.rec)));
            let _ = tx.send(Event::Recorded);
        }
    }

    /// Song mode: keep what was just rendered
    fn keep_recording(&mut self, out: &[f32]) {
        if !self.recording {
            return;
        }
        let frames = out.len() / 2;
        let take = if self.rec_tail > 0 {
            let n = frames.min(self.rec_tail as usize);
            self.rec_tail -= n as u32;
            n
        } else {
            frames
        };
        self.rec.extend_from_slice(&out[..take * 2]);
        if self.rec.len() >= REC_CHUNK {
            if let Some(tx) = &self.events {
                let full = std::mem::replace(&mut self.rec, Vec::with_capacity(REC_CHUNK + 4096));
                let _ = tx.send(Event::Audio(full));
            }
        }
        if take < frames || (self.rec_tail == 0 && !self.playing) {
            self.finish_recording();
        }
    }

    fn end_song(&mut self) {
        self.handle(Cmd::Stop);
        self.send_song(Some(Change::SongEnd));
    }

    /// Auto mode's energy: settles after the drop, rises towards the breakdown,
    /// drops low in it and climbs through the build-up
    fn curve_target(&self, b: u64) -> f32 {
        let (at, build) = (self.breakdown_at(), self.cycle_len - 2);
        if b < 8 {
            0.95 - 0.25 * b as f32 / 8.0
        } else if b < at {
            0.7 + 0.15 * (b - 8) as f32 / (at - 8).max(1) as f32
        } else if b < build {
            0.2
        } else {
            0.5 + (b - build) as f32 * 0.2
        }
    }

    /// Bar of the auto cycle where the breakdown starts – as far before its end as the plan says
    fn breakdown_at(&self) -> u64 {
        self.cycle_len - (32 - self.plan.breakdown_at)
    }

    /// A new auto cycle from `bar` on: another kind of breakdown and, in the calmer vibes,
    /// sometimes a longer groove before it
    fn new_cycle(&mut self, bar: u64) {
        self.cycle_start = bar;
        self.plan = PLANS[self.auto_rng.below(PLANS.len() as u32) as usize];
        let lens: &[u64] = match self.song.vibe.rules() {
            Vibe::Hypnotic | Vibe::Minimal | Vibe::Dub | Vibe::Melodic => &[32, 48, 64, 48],
            _ => &[32, 32, 48],
        };
        self.cycle_len = *self.auto_rng.pick(lens);
    }

    /// Bar starts: drops, a waiting pattern, song mode, and in auto mode the arrangement
    fn new_bar(&mut self, idx: u64) {
        let bar = idx / STEPS as u64;
        self.bar_now = bar;
        // An effect moment ends with its bar
        if self.fx_moment.is_some_and(|(b, ..)| b < bar) {
            self.fx_moment = None;
            if std::mem::take(&mut self.auto_stutter) && !self.fx[Fx::Stutter.index()] {
                self.stutter = None;
            }
        }
        let mut dropped = false;
        if let Some(b) = &self.build {
            if idx >= b.start + b.len {
                self.drop_now();
                dropped = true;
            }
        }
        // A pattern picked during the bar comes in now
        if let Some((s, slot)) = self.queued.take() {
            self.load_pattern(*s, Some(slot));
            self.send_song(Some(Change::Scene(slot)));
        }
        if self.arrangement.is_some() {
            self.arrange_bar(idx, bar, dropped);
            return;
        }
        if dropped || !self.song.auto || bar == 0 {
            return;
        }
        // A cycle of 32–64 bars: groove (changing every few bars, a fill every 8), breakdown,
        // 2 bars build-up, drop. Every drop – also one by hand – starts the next cycle.
        if bar < self.cycle_start || bar - self.cycle_start >= self.cycle_len {
            self.new_cycle(bar);
        }
        let b = bar - self.cycle_start;
        let at = self.breakdown_at();
        // With the curve on, Bunker arranges: energy, breakdown, build-up, drop. With the
        // energy by hand, those are yours (low = break, 90 = drop) – the groove still evolves.
        if self.song.curve {
            self.energy_goal = self.curve_target(b);
            if b == at && self.build.is_none() {
                self.start_breakdown(idx);
            } else if b == self.cycle_len - 2 && self.build.is_none() {
                self.auto_build(idx);
            }
        }
        if b < at && bar.is_multiple_of(self.song.auto_bars.max(1) as u64) {
            if let Some(change) = gen::evolve(&mut self.song, &mut self.auto_rng) {
                self.send_song(Some(change));
            }
        }
        if b + 1 < at && (b + 1).is_multiple_of(8) && self.auto_rng.chance(0.8) {
            self.phrase_end(bar);
        }
    }

    fn fire(&mut self, idx: u64) {
        if idx.is_multiple_of(STEPS as u64) {
            self.new_bar(idx);
        }
        // Bunker steering the energy: it glides towards the curve instead of jumping
        if self.song.curve && (self.song.auto || self.arrangement.is_some()) {
            let e = self.song.macros.energy;
            let next = e + (self.energy_goal - e).clamp(-GLIDE, GLIDE);
            if (next - e).abs() > 1e-4 {
                self.song.macros.energy = next;
                if idx.is_multiple_of(4) {
                    self.send_song(None);
                }
            }
        }
        // Tracks come and go on the beat
        if idx.is_multiple_of(4) || self.build.is_some() {
            self.layer_energy = self.song.macros.energy;
        }
        self.shared.step.store(idx, Ordering::Relaxed);
        let build = self.build_progress(idx as f64);
        let gap = self.in_drop_gap(idx as f64);
        // Echo build-up: the last hits fly into the echo for the silence
        if let Some(b) = &self.build {
            if b.style == BuildStyle::Echo && idx == b.start + b.len - DROP_GAP {
                self.throw = 1.0;
            }
        }
        if let Some((bar, fx, from)) = self.fx_moment {
            if idx == bar * STEPS as u64 + from {
                match fx {
                    Fx::Stutter if self.stutter.is_none() => {
                        self.stutter = Some(idx);
                        self.auto_stutter = true;
                    }
                    Fx::Echo => self.throw = 1.0,
                    _ => {}
                }
            }
        }
        let play_idx = match self.stutter {
            Some(origin) if idx >= origin => origin + (idx - origin) % 2,
            _ => idx,
        };
        let in_bar = idx % STEPS as u64;
        let fill = self.fill.filter(|(b, _)| *b == idx / STEPS as u64).map(|(_, f)| f);
        let step_len = self.step_len() as f32;
        let energy = self.layer_energy;
        // The kick roll of a build-up comes in even when the energy or a breakdown took the kick out
        let kick_by_hand = self.breakdown.as_ref().map_or(self.song.track(Voice::Kick).mute, |b| {
            if b.out.contains(&Voice::Kick) {
                b.before[Voice::Kick.index()]
            } else {
                self.song.track(Voice::Kick).mute
            }
        });
        // A hi-hat can't be open and closed at once: the closed one leaves the open one's steps alone
        let oh = self.song.track(Voice::OpenHat);
        let open_now = !oh.mute && energy >= LAYERS[Voice::OpenHat.index()] && oh.steps[oh.step_at(play_idx)].on;
        for (ti, voice) in Voice::ALL.into_iter().enumerate() {
            let track = self.song.tracks[ti];
            let roll = build.is_some() && voice == Voice::Kick && !kick_by_hand;
            if !roll && (track.mute || energy < LAYERS[ti]) {
                continue;
            }
            if fill == Some(Fill::Gap) && in_bar >= 12 && matches!(voice, Voice::Kick | Voice::Bass) {
                continue;
            }
            // Build-up: the groove itself turns into it (`build_play`), silence in the gap
            let mut track_idx = play_idx;
            let mut lift = 0;
            if let (Some(p), Some(start)) = (build, self.build.as_ref().map(|b| b.start)) {
                if gap {
                    continue;
                }
                match build_play(voice, p, idx, start) {
                    BuildPlay::Pattern => {}
                    BuildPlay::Rest => continue,
                    BuildPlay::Hit(v) => {
                        self.trigger(voice, track.volume * v, v > 0.85, 0, 0, false);
                        continue;
                    }
                    BuildPlay::Loop { at, up } => {
                        track_idx = at;
                        lift = up;
                    }
                }
            }
            if voice == Voice::Hat && open_now && build.is_none() {
                continue;
            }
            let si = track.step_at(track_idx);
            let step = track.steps[si];
            let mut vel = track.volume * if step.accent { 1.0 } else { 0.72 };
            // Energy thins out the percussion (below 45) or adds ghost hats (above 60);
            // the choice changes every two bars
            let perc = matches!(voice, Voice::Hat | Voice::Ride | Voice::Rim | Voice::OpenHat);
            let h = hash01(ti as u64 * 7919 + si as u64 * 104_729 + (play_idx / 32) * 15_485_863);
            let mut on = step.on;
            if on && perc && !step.accent && energy < 0.45 && h < (0.45 - energy) * 1.8 {
                on = false;
            }
            if !step.on && voice == Voice::Hat && energy > 0.6 && h < (energy - 0.6) * 1.5 {
                on = true;
                vel *= 0.5;
            }
            if !on || (step.on && step.chance < 100 && self.rng.below(100) >= step.chance as u32) {
                continue;
            }
            let next = track.steps[(si + 1) % track.len.max(1) as usize];
            let legato = voice == Voice::Acid && next.on && next.slide;
            let gate = if legato { step_len * 1.05 } else { step_len * 0.5 };
            self.trigger(voice, vel, step.accent, step.note + lift, gate as u32, step.slide && step.on);
        }
        // Build-up: a clap roll that gets faster and louder
        if let Some(p) = build.filter(|_| !gap) {
            if p < 0.5 && idx.is_multiple_of(2) || p >= 0.5 {
                let v = 0.35 + 0.55 * p;
                self.clap_hit(v, 0.08);
                self.count_hit(Voice::Clap);
            }
        }
        if build.is_none() {
            self.play_fill(fill, in_bar);
        }
    }

    fn play_fill(&mut self, fill: Option<Fill>, s: u64) {
        let sr = self.sr;
        match fill {
            Some(Fill::Roll) if s >= 8 && (s >= 12 || s.is_multiple_of(2)) => {
                self.clap_hit(0.25 + 0.06 * (s - 8) as f32, 0.07);
                self.count_hit(Voice::Clap);
            }
            Some(Fill::Toms) if s >= 10 => {
                self.rim
                    .trigger(0.55 + 0.05 * (s - 10) as f32, 1.45 - (s - 10) as f32 * 0.1, 3, 1.0);
                self.count_hit(Voice::Rim);
            }
            Some(Fill::OpenHats) if s >= 8 && s.is_multiple_of(2) => {
                let m = &OPEN_HATS[self.song.track(Voice::OpenHat).sound as usize % OPEN_HATS.len()];
                self.open_hat.set_model(m, 1.0, sr);
                self.open_hat.trigger(0.3 + 0.04 * (s - 8) as f32, 0.12 * m.length, sr);
                self.count_hit(Voice::OpenHat);
            }
            Some(Fill::Throw) if s == 12 => {
                self.throw = 1.0;
                self.clap_hit(0.7, 0.15);
                self.count_hit(Voice::Clap);
            }
            _ => {}
        }
    }

    /// A clap for build-ups and fills, in the clap track's sound and tuning
    fn clap_hit(&mut self, vel: f32, length: f32) {
        let t = self.song.track(Voice::Clap);
        let (sound, tune) = (t.sound, t.tune_ratio());
        self.clap.trigger(vel, length, self.sr, sound, tune);
    }

    fn count_hit(&self, v: Voice) {
        self.shared.hits[v.index()].fetch_add(1, Ordering::Relaxed);
    }

    /// Start a sound. `note` in semitones above the root (synths only)
    fn trigger(&mut self, voice: Voice, vel: f32, accent: bool, note: i8, gate: u32, slide: bool) {
        let sr = self.sr;
        let m = self.song.macros;
        let track = *self.song.track(voice);
        let (tune, decay) = (track.tune_ratio(), track.decay_factor());
        let gate = if gate == 0 { self.step_len() as f32 * 0.6 } else { gate as f32 };
        // Bass and synth follow the chord progression, bar by bar
        let sc = self.song.scale;
        let note = if voice.is_synth() {
            sc.shift(note, self.song.prog_degree(self.bar_now))
        } else {
            note
        };
        let pitch = (self.song.root as i32 + note as i32 + self.transpose as i32 + track.tune as i32) as f32;
        let sound = track.sound;
        let pick = |n: usize| sound as usize % n;
        match voice {
            Voice::Kick => {
                let k = &KICKS[pick(KICKS.len())];
                self.kick.set_model(k);
                self.kick.tune = (47.0 + (self.song.root % 12) as f32 * 0.6) * k.tune * tune;
                let hard = if matches!(self.song.vibe, Vibe::Industrial | Vibe::Hard) {
                    1.5
                } else {
                    0.0
                };
                self.kick.drive = (2.5 + hard) * k.drive + k.drive_add;
                self.kick.trigger(vel, (0.11 + m.dark * 0.2) * k.length * decay, sr);
                self.pump_env = 1.0;
            }
            Voice::Clap => self.clap.trigger(vel * 1.3, (0.1 + m.space * 0.15) * decay, sr, sound, tune),
            Voice::Rim => self.rim.trigger(vel * 1.4, tune * if accent { 1.06 } else { 1.0 }, sound, decay),
            Voice::Hat => {
                let h = &HATS[pick(HATS.len())];
                self.hat.set_model(h, tune, sr);
                self.open_hat.choke(sr);
                self.hat.trigger(vel * 1.05, (0.03 + m.energy * 0.035) * h.length * decay, sr);
            }
            Voice::OpenHat => {
                let h = &OPEN_HATS[pick(OPEN_HATS.len())];
                self.open_hat.set_model(h, tune, sr);
                self.open_hat.trigger(vel * 0.85, (0.18 + m.energy * 0.2) * h.length * decay, sr);
            }
            Voice::Ride => {
                let h = &RIDES[pick(RIDES.len())];
                self.ride.set_model(h, tune, sr);
                self.ride.trigger(vel * 0.9, (0.45 + m.space * 0.4) * h.length * decay, sr);
            }
            Voice::Bass => self.bass.trigger(pitch, vel, gate * decay.min(1.5), sr, sound, decay),
            Voice::Acid => {
                // Stab chords: root, third and fifth of the scale on this note
                let base = sc.shift(note, 0) as f32;
                let chord = [0.0, sc.shift(note, 2) as f32 - base, sc.shift(note, 4) as f32 - base];
                let gate = gate * decay.min(1.5);
                if slide && sound < 2 {
                    self.acid.slide(pitch, accent, gate, sr);
                } else {
                    self.acid.trigger(pitch, vel, accent, gate, sr, sound, chord, decay);
                }
            }
        }
        self.count_hit(voice);
    }

    /// Fill an interleaved stereo buffer
    pub fn render(&mut self, out: &mut [f32]) {
        let sr = self.sr;
        let m = self.song.macros;
        // (The energy changes what plays, never how loud)
        let vol: [f32; TRACKS] = std::array::from_fn(|i| if self.song.tracks[i].mute { 0.0 } else { 1.0 });
        let step_len = self.step_len();
        let delay_time = (step_len * 3.0) as f32;
        let space = m.space;
        let dark = m.dark;
        // Break zone: the lower the energy, the more closed and spacious the sound
        let es_coef = 1.0 - (-1.0 / (0.12 * sr)).exp();
        let break_space = 1.0 + ((BREAK_ZONE - self.energy_smooth) / BREAK_ZONE).clamp(0.0, 1.0) * 1.2;
        // Darkness closes the master tone filter – all the way down to a muffled 1.5 kHz
        let tone = 18_000.0 * 0.1f32.powf(dark * dark * 1.08);
        self.tone_l.set(tone, 0.6, sr);
        self.tone_r.set(tone, 0.6, sr);
        // Chord stabs live in the echo (dub)
        let mut delay_send = DELAY_SEND;
        if self.song.track(Voice::Acid).sound == 2 {
            delay_send[Voice::Acid.index()] = 0.75;
        }
        // Auto mode turns the acid knob itself: a slow sweep over 16 bars
        let acid = if self.song.auto && self.playing {
            (m.acid + 0.22 * (self.phase as f32 / 256.0 * std::f32::consts::TAU).sin()).clamp(0.0, 1.0)
        } else {
            m.acid
        };
        // (Stutter works on the steps, in `fire`)
        let [_, echo_on, mut wash_on, crush_on, gate_on, tape_on] = self.fx;
        let (crush_on, gate_on) = (crush_on || self.moment(Fx::Crush), gate_on || self.moment(Fx::Gate));
        // AUTO FX: a breakdown starts in the wash
        if let Some(b) = &self.breakdown {
            if self.song.auto_fx
                && (self.song.auto || self.arrangement.is_some())
                && self.last_fired.is_some_and(|i| i < b.start + STEPS as u64)
            {
                wash_on = true;
            }
        }
        // Pump: the duck lasts a bit more than a 16th
        let pump_coef = (-1.0 / (step_len as f32 * 1.3)).exp();
        let pump = m.pump;
        // Master EQ: ±12 dB per band
        let eq = self.song.eq;
        let db = |v: f32| 10f32.powf((v - 0.5) * 24.0 / 20.0) - 1.0;
        let (g_low, g_mid, g_high) = (db(eq.low), db(eq.mid), db(eq.high));
        let eq_on = g_low.abs() + g_mid.abs() + g_high.abs() > 0.01;
        let band_coef = (-1.0 / (0.03 * sr)).exp();
        let wash_release = (-1.0 / (2.5 * sr)).exp();

        for frame in out.as_chunks_mut::<2>().0 {
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
                // Last quarter of the build-up: 32nd-note rolls (waiting for its bar shows as just begun)
                if self.build.is_some() {
                    let p = self.build_progress(self.phase).unwrap_or(0.0);
                    self.shared.build.store((p * 1000.0).max(1.0) as u32, Ordering::Relaxed);
                    if p > 0.75 && frac >= 0.5 && self.half_fired != Some(idx) && !self.in_drop_gap(self.phase) {
                        self.half_fired = Some(idx);
                        self.clap_hit(0.5 + 0.4 * p, 0.06);
                    }
                }
            }

            // ── Voices ──
            let kick_raw = self.kick.next(sr, &mut self.rng);
            let duck = self.kick.envelope();
            self.pump_env *= pump_coef;
            // Everything but the kick breathes with it
            let pg = 1.0 - pump * 0.85 * self.pump_env.min(1.0).powf(0.6);
            let v = [
                kick_raw * vol[0],
                self.clap.next(sr, &mut self.rng) * vol[1] * pg,
                self.rim.next(sr, &mut self.rng) * vol[2] * pg,
                self.hat.next(sr, &mut self.rng) * vol[3] * pg,
                self.open_hat.next(sr, &mut self.rng) * vol[4] * pg,
                self.ride.next(sr, &mut self.rng) * vol[5] * pg,
                self.bass.next(sr, self.energy_smooth, dark) * vol[6] * 2.0 * (1.0 - duck * 0.7) * pg,
                self.acid.next(sr, acid) * vol[7] * (1.0 - duck * 0.3) * pg,
            ];
            let (mut l, mut r) = (0.0f32, 0.0f32);
            let (mut dl, mut dr, mut rv) = (0.0f32, 0.0f32, 0.0f32);
            let echo_send = self.echo_amount;
            for i in 0..TRACKS {
                let x = v[i];
                let p = PAN[i];
                l += x * (1.0 - p).min(1.0);
                r += x * (1.0 + p).min(1.0);
                let ds = delay_send[i] * space * 2.0 + if i == 0 || i == 6 { 0.0 } else { echo_send };
                dl += x * ds * (1.0 - p * 0.5);
                dr += x * ds * (1.0 + p * 0.5);
                rv += x * VERB_SEND[i] * (0.15 + space * 1.6) * break_space;
            }
            // Rumble and the drop
            let rumble = self.rumble.run(kick_raw * vol[0], duck) * dark * 1.3;
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
            // Build-up: a noise riser sweeping up, fading out in the gap before the drop
            let build = self.build_progress(self.phase).filter(|_| self.playing);
            let riser_want = if build.is_some() && !self.in_drop_gap(self.phase) {
                1.0
            } else {
                0.0
            };
            self.riser_gain += (riser_want - self.riser_gain) * 0.0015;
            match (build, &self.build) {
                (Some(p), Some(b)) => {
                    self.build_cut = p;
                    self.cut_style = b.style;
                }
                // After the drop the build-up filter lets go within a few milliseconds
                _ => self.build_cut *= 0.998,
            }
            if self.riser_gain > 1e-4 {
                let p = self.build_cut;
                self.riser.set(400.0 * 30f32.powf(p), 2.5, sr);
                let noise = self.rng.noise();
                let n = self.riser.run(noise).bp * p * p * 0.5 * self.riser_gain;
                l += n;
                r += n;
                rv += n;
            }

            // ── Sends ──
            self.throw *= 0.99997;
            let echo_want = if echo_on { 0.9 } else { 0.0 } + self.throw * 0.6;
            self.echo_amount += (echo_want - self.echo_amount) * 0.0005;
            let fb = 0.25 + space * 0.4 + self.echo_amount * 0.35;
            let (el, er) = self.delay.run(dl, dr, delay_time, fb.min(0.85));
            // Wash: everything into the reverb, the dry beat steps back
            self.wash += ((if wash_on { 1.0 } else { 0.0 }) - self.wash) * 0.0008;
            self.wash_room = if self.wash > self.wash_room {
                self.wash
            } else {
                self.wash_room * wash_release
            };
            if self.wash > 1e-4 {
                rv += self.wash_hp.run((l + r) * 0.5).hp * self.wash * WASH_SEND;
            }
            let duck_dry = 1.0 - self.wash * 0.55;
            l = l * duck_dry + el * pg;
            r = r * duck_dry + er * pg;
            let size = (0.5 + space * 0.46 + self.wash_room * 0.5).min(0.98);
            let (rl, rr) = self.reverb.run(rv, size, 0.3 + dark * 0.5);
            l += rl * pg;
            r += rr * pg;

            // ── Master ──
            // With no hand on the filter, a breakdown may sweep it open by itself
            let auto_filter = match &self.breakdown {
                Some(b) if b.sweep && self.playing => {
                    let p = ((self.phase - b.start as f64) / b.len as f64).clamp(0.0, 1.0) as f32;
                    -0.75 * (1.0 - p)
                }
                _ => 0.0,
            };
            let target = if self.filter_target.abs() > 0.01 {
                self.filter_target
            } else {
                auto_filter
            };
            self.filter += (target - self.filter) * 0.002;
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
            if self.build_cut > 0.002 {
                // Classic and echo: a high-pass opens upwards; gate: a lighter one;
                // low-pass: the sound closes down to a muffle
                let cut = self.build_cut;
                let (fc, low) = match self.cut_style {
                    BuildStyle::Lowpass => (18_000.0 * 0.025f32.powf(cut), true),
                    BuildStyle::Gate => (20.0 * 80f32.powf(cut * 0.6), false),
                    BuildStyle::Classic | BuildStyle::Echo => (20.0 * 80f32.powf(cut), false),
                };
                self.build_hp_l.set(fc, 0.9, sr);
                self.build_hp_r.set(fc, 0.9, sr);
                let (ol, or) = (self.build_hp_l.run(l), self.build_hp_r.run(r));
                (l, r) = if low { (ol.lp, or.lp) } else { (ol.hp, or.hp) };
            }
            l = self.tone_l.run(l).lp;
            r = self.tone_r.run(r).lp;
            self.energy_smooth += (m.energy - self.energy_smooth) * es_coef;
            let brk = ((BREAK_ZONE - self.energy_smooth) / BREAK_ZONE).clamp(0.0, 1.0);
            if brk > 0.001 {
                let fc = 18_000.0 * 0.12f32.powf(brk);
                self.break_lp_l.set(fc, 0.8, sr);
                self.break_lp_r.set(fc, 0.8, sr);
                l = self.break_lp_l.run(l).lp;
                r = self.break_lp_r.run(r).lp;
            }

            // Crush: hold every few samples and round to fewer bits
            self.crush += ((if crush_on { 1.0 } else { 0.0 }) - self.crush) * 0.002;
            if self.crush > 0.002 {
                self.crush_count += 1.0;
                let hold = 1.0 + self.crush * 9.0;
                if self.crush_count >= hold {
                    self.crush_count -= hold;
                    let q = 2f32.powf(12.0 - self.crush * 8.0);
                    self.crush_hold = ((l * q).round() / q, (r * q).round() / q);
                }
                l += (self.crush_hold.0 * 1.3 - l) * self.crush;
                r += (self.crush_hold.1 * 1.3 - r) * self.crush;
            }
            // Gate: open for the first half of every 16th (a gate build-up chops harder and harder)
            let build_gate = match build {
                Some(p) if self.cut_style == BuildStyle::Gate => (p * 1.3).min(1.0),
                _ => 0.0,
            };
            let gate_want = if gate_on { 1.0 } else { build_gate };
            self.gate += (gate_want - self.gate) * 0.002;
            if self.gate > 0.002 {
                let open = !self.playing || self.phase.fract() < 0.5;
                self.gate_env += ((if open { 1.0 } else { 0.0 }) - self.gate_env) * 0.01;
                let g = 1.0 - self.gate * (1.0 - self.gate_env);
                l *= g;
                r *= g;
            }

            // Master EQ: low shelf, mid bell, high shelf
            if eq_on {
                for (ch, x) in [&mut l, &mut r].into_iter().enumerate() {
                    let lo = self.eq_low[ch].run(*x).lp;
                    let mid = self.eq_mid[ch].run(*x).bp / 0.8;
                    let hi = self.eq_high[ch].run(*x).hp;
                    *x += lo * g_low + mid * g_mid + hi * g_high;
                }
            }
            // Glue: saturation with the DRIVE knob
            let drive = 1.34 + (eq.drive - 0.3) * 1.6;
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

            // Tape stop: read the recent output back slower and slower until it stands still
            let n = self.tape.len() / 2;
            let w = self.tape_w;
            self.tape[2 * w] = l;
            self.tape[2 * w + 1] = r;
            self.tape_w = (w + 1) % n;
            if tape_on {
                if !self.taping {
                    self.taping = true;
                    self.tape_r = w as f64;
                    self.tape_rate = 1.0;
                }
                let i = self.tape_r.floor() as usize % n;
                let j = (i + 1) % n;
                let f = self.tape_r.fract() as f32;
                let fade = (self.tape_rate * 3.0).min(1.0);
                l = (self.tape[2 * i] * (1.0 - f) + self.tape[2 * j] * f) * fade;
                r = (self.tape[2 * i + 1] * (1.0 - f) + self.tape[2 * j + 1] * f) * fade;
                self.tape_r = (self.tape_r + self.tape_rate as f64) % n as f64;
                self.tape_rate = (self.tape_rate - 1.0 / (0.8 * sr)).max(0.0);
            } else {
                self.taping = false;
            }

            self.peak = self.peak.max(l.abs().max(r.abs()));
            frame[0] = l;
            frame[1] = r;

            // Spectrum for the visualizer
            let mono = (l + r) * 0.5;
            for i in 0..BANDS.len() {
                let y = self.band_filters[i].run(mono).bp.abs();
                self.band_env[i] = y.max(self.band_env[i] * band_coef);
                self.band_peak[i] = self.band_peak[i].max(self.band_env[i]);
            }
        }
        for i in 0..BANDS.len() {
            self.shared.bands[i].store(self.band_peak[i].to_bits(), Ordering::Relaxed);
            self.band_peak[i] = 0.0;
        }
        // Peak meter with a slow fall
        let shown = f32::from_bits(self.shared.level.load(Ordering::Relaxed));
        let fallen = (shown * 0.85).max(self.peak);
        self.shared.level.store(fallen.to_bits(), Ordering::Relaxed);
        self.peak = 0.0;
        self.shared.playing.store(self.playing, Ordering::Relaxed);
        self.keep_recording(out);
    }
}

/// Render `bars` bars of the song (plus a reverb tail) into interleaved stereo (tests, demo loops)
#[cfg(test)]
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
    use std::collections::HashSet;

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
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert!(changes.contains(&Change::Breakdown), "{changes:?}");
        assert!(changes.contains(&Change::Build));
        assert!(matches!(changes.last(), Some(Change::Drop | Change::NewChapter)), "{changes:?}");
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
    fn manual_drop_builds_two_bars_from_the_next_bar() {
        let (tx, rx) = std::sync::mpsc::channel();
        let shared = Shared::new();
        let mut e = Engine::new(22_050.0, Song::default(), shared.clone(), Some(tx), 2);
        e.handle(Cmd::Play);
        let step = e.step_len() as usize;
        let mut buf = vec![0.0; 2 * step * 5];
        e.render(&mut buf); // into bar 0
        e.handle(Cmd::Drop);
        assert!(e.build.is_some());
        // Waiting for bar 1: the beat plays on, the button already shows it
        let kicks = |s: &Shared| s.hits[Voice::Kick.index()].load(Ordering::Relaxed);
        let before = kicks(&shared);
        let mut buf = vec![0.0; 2 * step * 10];
        e.render(&mut buf);
        assert!(kicks(&shared) > before, "kick until the bar is over");
        assert!(shared.build_progress().is_some());
        assert_eq!(e.build_progress(e.phase), None);
        // Build-up through bars 1 and 2, the drop on bar 3
        let mut buf = vec![0.0; 2 * step * 32];
        e.render(&mut buf);
        assert!(e.build.is_some(), "still building on step 47");
        let mut buf = vec![0.0; 2 * step * 2];
        e.render(&mut buf);
        assert!(e.build.is_none(), "drop on step 48");
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert_eq!(changes, vec![Change::Build, Change::Drop]);
    }

    #[test]
    fn build_up_turns_the_groove_into_a_roll_and_breathes_before_the_drop() {
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(4));
        song.macros.energy = 0.9;
        for v in Voice::ALL {
            song.track_mut(v).mute = false;
        }
        let shared = Shared::new();
        let mut e = Engine::new(22_050.0, song, shared.clone(), None, 1);
        e.handle(Cmd::Play);
        let step = e.step_len() as usize;
        let hits = |v: Voice| shared.hits[v.index()].load(Ordering::Relaxed);
        // Windows end in the middle of a step, away from the step boundaries
        let mut buf = vec![0.0; 2 * (step * 15 + step / 2)];
        e.render(&mut buf);
        e.handle(Cmd::Drop);
        let count = |e: &mut Engine, steps: usize, v: Voice| {
            let before = hits(v);
            let mut buf = vec![0.0; 2 * step * steps];
            e.render(&mut buf);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            hits(v) - before
        };
        // Kick roll: quarters in the first bar, 8ths, then 16ths – up to the gap
        assert_eq!(count(&mut e, 16, Voice::Kick), 4);
        assert_eq!(count(&mut e, 8, Voice::Kick), 4);
        assert_eq!(count(&mut e, 6, Voice::Kick), 6);
        // The gap: nothing new starts in the last half beat
        let before = Voice::ALL.map(hits);
        let mut buf = vec![0.0; 2 * step * 2];
        e.render(&mut buf);
        assert_eq!(Voice::ALL.map(hits), before, "silence before the drop");
        assert!(count(&mut e, 4, Voice::Kick) > 0 && e.build.is_none(), "the drop");
    }

    #[test]
    fn build_up_loops_the_riff_and_lifts_it() {
        let start = 32;
        assert_eq!(build_play(Voice::Acid, 0.2, 36, start), BuildPlay::Pattern);
        assert_eq!(build_play(Voice::Acid, 0.6, 54, start), BuildPlay::Loop { at: 50, up: 0 });
        assert_eq!(build_play(Voice::Bass, 0.8, 59, start), BuildPlay::Loop { at: 49, up: 12 });
        assert_eq!(build_play(Voice::Rim, 0.8, 59, start), BuildPlay::Loop { at: 49, up: 0 });
        assert_eq!(build_play(Voice::OpenHat, 0.6, 50, start), BuildPlay::Rest);
        assert!(matches!(build_play(Voice::Hat, 0.6, 51, start), BuildPlay::Hit(_)));
    }

    #[test]
    fn the_drop_zone_starts_a_build_up() {
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(4));
        song.macros.energy = 0.5;
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 1);
        e.handle(Cmd::Play);
        let mut buf = vec![0.0; 2 * e.step_len() as usize];
        e.render(&mut buf);
        let set = |e: &mut Engine, v: f32| {
            let mut s = e.song;
            s.macros.energy = v;
            e.handle(Cmd::Song(Box::new(s)));
        };
        set(&mut e, 0.6);
        set(&mut e, 0.85);
        assert!(e.build.is_none(), "below 90 it's just energy");
        set(&mut e, 0.9);
        assert!(e.build.is_some(), "at 90 the build-up starts");
        // Up there it doesn't start again – only after coming back down
        e.build = None;
        set(&mut e, 1.0);
        set(&mut e, 0.95);
        assert!(e.build.is_none());
        set(&mut e, 0.7);
        set(&mut e, 0.92);
        assert!(e.build.is_some());
        // With the auto mode's curve the engine owns the energy – no zone
        song.auto = true;
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 1);
        e.handle(Cmd::Play);
        e.render(&mut buf);
        set(&mut e, 0.5);
        set(&mut e, 0.95);
        assert!(e.build.is_none());
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
    fn auto_mode_varies_and_always_brings_the_beat_back() {
        // Several cycles: different lengths, breakdowns, build-ups, fills, chapters – and
        // outside a breakdown the kick always plays
        let (tx, rx) = std::sync::mpsc::channel();
        let mut song = gen::generate(Vibe::Hypnotic, &Default::default(), &mut Rng::new(8));
        song.auto = true;
        song.auto_bars = 2;
        song.bpm = 180.0;
        let mut e = Engine::new(11_025.0, song, Shared::new(), Some(tx), 4);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar];
        let (mut kinds, mut lens, mut styles) = (HashSet::new(), HashSet::new(), HashSet::new());
        for b in 0..(32 * 10) {
            e.render(&mut buf);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            if e.breakdown.is_none() && e.build.is_none() && b > 0 {
                assert!(!e.song.track(Voice::Kick).mute, "kick plays (bar {b})");
            }
            kinds.insert(e.plan.breakdown_at * 100 + e.plan.out.len() as u64 * 10 + e.plan.sweep as u64);
            lens.insert(e.cycle_len);
            if let Some(b) = &e.build {
                styles.insert(format!("{:?}", b.style));
            }
        }
        assert!(kinds.len() >= 2, "different breakdowns");
        assert!(lens.len() >= 2, "different cycle lengths: {lens:?}");
        assert!(styles.len() >= 2, "different build-ups: {styles:?}");
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert!(changes.iter().any(|c| matches!(c, Change::NewSound(..))), "{changes:?}");
        assert!(changes.iter().filter(|c| matches!(c, Change::Drop | Change::NewChapter)).count() >= 5);
    }

    #[test]
    fn a_drop_by_hand_pushes_the_next_auto_drop_back() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(8));
        song.auto = true;
        song.bpm = 180.0;
        let mut e = Engine::new(11_025.0, song, Shared::new(), Some(tx), 4);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar * 20 + 200];
        e.render(&mut buf); // into bar 20, just before the auto breakdown
        e.handle(Cmd::Drop); // build-up in bars 21–22, drop on 23
        let mut buf = vec![0.0; 2 * bar * 3];
        e.render(&mut buf);
        assert_eq!(e.cycle_start, 23, "the drop starts a new cycle");
        let _ = rx.try_iter().count();
        let mut buf = vec![0.0; 2 * bar * 20];
        e.render(&mut buf);
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert!(!changes.contains(&Change::Breakdown), "no breakdown right after: {changes:?}");
        assert!(!changes.contains(&Change::Drop));
    }

    #[test]
    fn live_effects_stay_clean() {
        let song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(3));
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 1);
        e.handle(Cmd::Play);
        let mut buf = vec![0.0; 2 * 22_050];
        e.render(&mut buf);
        for fx in Fx::ALL {
            e.handle(Cmd::Fx(fx, true));
            e.render(&mut buf);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0), "{fx:?}");
            if fx == Fx::Tape {
                // Fully stopped after a second
                let tail = &buf[buf.len() - 2000..];
                assert!(tail.iter().all(|x| x.abs() < 1e-3), "tape stop ends in silence");
            }
            e.handle(Cmd::Fx(fx, false));
            e.render(&mut buf);
        }
        let (peak, rms) = stats(&buf);
        assert!(peak > 0.1 && rms > 0.02, "back to normal after the effects");
    }

    fn patterns(bpm: f32) -> [Song; 3] {
        [Vibe::Dub, Vibe::Peak, Vibe::Hard].map(|v| {
            let mut s = gen::generate(v, &Default::default(), &mut Rng::new(v as u64 + 1));
            s.bpm = bpm;
            s
        })
    }

    #[test]
    fn a_song_plays_its_parts_with_the_patterns() {
        let (tx, rx) = std::sync::mpsc::channel();
        let [a, b, c] = patterns(180.0);
        let mut e = Engine::new(11_025.0, a, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Arrange(Some(Box::new(Arrangement {
            patterns: [Some(a), Some(b), Some(c), None],
            form: 0,
            fallback: a,
        }))));
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar];
        let mut kick_in_break = None;
        for _ in 0..(FORMS[0].bars() + 2) {
            e.render(&mut buf);
            assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
            if e.breakdown.is_some() {
                kick_in_break = Some(e.song.track(Voice::Kick).mute);
                assert_eq!(
                    e.song.tracks.map(|t| t.steps),
                    b.tracks.map(|t| t.steps),
                    "the break plays pattern B"
                );
            }
        }
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        let parts: Vec<Part> = changes
            .iter()
            .filter_map(|c| if let Change::Part(p, ..) = c { Some(*p) } else { None })
            .collect();
        assert_eq!(parts, vec![Part::Intro, Part::Groove, Part::Break, Part::Peak, Part::Outro]);
        assert_eq!(kick_in_break, Some(true), "no kick in the break");
        let build = changes.iter().position(|c| *c == Change::Build).expect("a build-up");
        let drop = changes.iter().position(|c| *c == Change::Drop).expect("a drop");
        assert!(build < drop);
        assert_eq!(changes.last(), Some(&Change::SongEnd));
        assert!(!e.playing && e.arrangement.is_none(), "the song ends");
    }

    #[test]
    fn a_song_is_recorded_with_its_tail() {
        let (tx, rx) = std::sync::mpsc::channel();
        let [a, b, _] = patterns(180.0);
        let sr = 11_025.0;
        let mut e = Engine::new(sr, a, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Arrange(Some(Box::new(Arrangement {
            patterns: [Some(a), Some(b), None, None],
            form: 0,
            fallback: a,
        }))));
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar];
        for _ in 0..(FORMS[0].bars() + 3) {
            e.render(&mut buf);
        }
        let (mut samples, mut done) = (0, false);
        for ev in rx.try_iter() {
            match ev {
                Event::Audio(piece) => {
                    assert!(!done, "nothing after the end");
                    samples += piece.len();
                }
                Event::Recorded => done = true,
                Event::Song(..) => {}
            }
        }
        assert!(done, "the recording is complete");
        let frames = samples / 2;
        let song = FORMS[0].bars() as usize * bar;
        let tail = (sr * 2.0) as usize;
        assert!(
            frames >= song + tail - bar && frames <= song + tail + bar,
            "{frames} vs {}",
            song + tail
        );
    }

    #[test]
    fn a_jump_goes_to_a_part_on_the_next_bar() {
        let (tx, rx) = std::sync::mpsc::channel();
        let [a, b, _] = patterns(180.0);
        let mut e = Engine::new(11_025.0, a, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Arrange(Some(Box::new(Arrangement {
            patterns: [Some(a), Some(b), None, None],
            form: 1,
            fallback: a,
        }))));
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar * 2 + 100];
        e.render(&mut buf);
        e.handle(Cmd::Jump(4)); // the first peak
        let mut buf = vec![0.0; 2 * bar];
        e.render(&mut buf);
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert!(changes.contains(&Change::Drop), "a peak starts with a drop: {changes:?}");
        // Pattern C is empty: the peak falls back to B
        assert!(changes.contains(&Change::Part(Part::Peak, 4, Some(1))), "{changes:?}");
        assert_eq!(e.section, 4);
        assert!(e.song.macros.energy > 0.9);
    }

    #[test]
    fn a_pattern_picked_while_playing_comes_in_on_the_next_bar() {
        let (tx, rx) = std::sync::mpsc::channel();
        let [a, b, _] = patterns(130.0);
        let mut e = Engine::new(22_050.0, a, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Play);
        let step = e.step_len() as usize;
        let mut buf = vec![0.0; 2 * step * 5];
        e.render(&mut buf);
        e.handle(Cmd::Queue(Box::new(b), 1));
        assert_eq!(e.song.tracks, a.tracks, "not yet");
        assert_eq!(e.fill, Some((0, Fill::Roll)), "a clap roll leads into it");
        let mut buf = vec![0.0; 2 * step * 12];
        e.render(&mut buf);
        assert_eq!(e.song.tracks.map(|t| t.steps), b.tracks.map(|t| t.steps));
        let changes: Vec<Change> = rx.try_iter().filter_map(Event::change).collect();
        assert_eq!(changes, vec![Change::Scene(1)]);
    }

    #[test]
    fn energy_strips_the_groove_and_the_drop_brings_it_back() {
        let mut song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(4));
        song.macros.energy = 0.0;
        let shared = Shared::new();
        let mut e = Engine::new(22_050.0, song, shared.clone(), None, 1);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar];
        e.render(&mut buf);
        let hits = |v: Voice| shared.hits[v.index()].load(Ordering::Relaxed);
        assert_eq!(hits(Voice::Kick), 0, "no kick at zero energy");
        assert!(hits(Voice::Hat) > 0, "hats stay");
        e.handle(Cmd::Drop);
        let mut buf = vec![0.0; 2 * bar * 2 + 400];
        e.render(&mut buf);
        assert!(e.song.macros.energy >= 0.85, "the drop brings the energy back");
        let before = hits(Voice::Kick);
        let mut buf = vec![0.0; 2 * bar];
        e.render(&mut buf);
        assert!(hits(Voice::Kick) >= before + 4);
    }

    #[test]
    fn auto_mode_curve_moves_the_energy_until_you_take_it() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut song = gen::generate(Vibe::Hypnotic, &Default::default(), &mut Rng::new(4));
        song.auto = true;
        song.bpm = 180.0;
        let mut e = Engine::new(11_025.0, song, Shared::new(), Some(tx), 1);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar * 27 + 100];
        e.render(&mut buf); // into the breakdown – the energy glides down over two bars
        assert!(e.song.macros.energy < 0.3, "low in the breakdown: {}", e.song.macros.energy);
        // It glided: no step bigger than the glide between two updates sent to the UI
        let sent: Vec<f32> = rx
            .try_iter()
            .filter_map(|ev| if let Event::Song(s, _) = ev { Some(s.macros.energy) } else { None })
            .collect();
        assert!(sent.iter().any(|&x| x > 0.85), "high after the start: {sent:?}");
        for w in sent.windows(2) {
            assert!((w[1] - w[0]).abs() <= 4.0 * GLIDE + 1e-3, "a jump: {} → {}", w[0], w[1]);
        }
        // The UI can't override it while the curve is on …
        let mut ui_song = e.song;
        ui_song.macros.energy = 0.9;
        e.handle(Cmd::Song(Box::new(ui_song)));
        assert!(e.song.macros.energy < 0.3);
        // … but by hand it can
        ui_song.curve = false;
        e.handle(Cmd::Song(Box::new(ui_song)));
        assert_eq!(e.song.macros.energy, 0.9);
    }

    #[test]
    fn master_section_and_shaping_stay_clean() {
        let mut song = gen::generate(Vibe::Hard, &Default::default(), &mut Rng::new(9));
        song.macros.pump = 1.0;
        song.eq = crate::song::MasterEq {
            low: 1.0,
            mid: 1.0,
            high: 1.0,
            drive: 1.0,
        };
        song.prog = 3;
        for t in song.tracks.iter_mut() {
            t.tune = 12;
            t.decay = 1.0;
        }
        let buf = render_offline(&song, 4, 22_050, 1);
        let (peak, rms) = stats(&buf);
        assert!(buf.iter().all(|x| x.is_finite()) && peak <= 1.0 && rms > 0.05, "{peak} {rms}");
        for t in song.tracks.iter_mut() {
            t.tune = -12;
            t.decay = 0.0;
        }
        song.eq = crate::song::MasterEq {
            low: 0.0,
            mid: 0.0,
            high: 0.0,
            drive: 0.0,
        };
        let buf = render_offline(&song, 4, 22_050, 1);
        let (peak, _) = stats(&buf);
        assert!(buf.iter().all(|x| x.is_finite()) && peak <= 1.0);
    }

    #[test]
    fn spectrum_follows_the_music() {
        let shared = Shared::new();
        let mut e = Engine::new(22_050.0, Song::default(), shared.clone(), None, 1);
        e.handle(Cmd::Play);
        let mut buf = vec![0.0; 2 * 2048];
        e.render(&mut buf);
        let bands = shared.bands();
        assert!(bands.iter().all(|b| b.is_finite()));
        assert!(bands[0] > 0.01, "the kick shows in the lowest band: {bands:?}");
    }

    #[test]
    fn every_sound_plays_cleanly() {
        let base = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(6));
        for v in Voice::ALL {
            for s in 0..v.sounds().len() {
                let mut song = base;
                song.track_mut(v).sound = s as u8;
                song.track_mut(v).mute = false;
                let buf = render_offline(&song, 2, 22_050, 1);
                assert!(buf.iter().all(|x| x.is_finite()));
                let (peak, _) = stats(&buf);
                assert!(peak <= 1.0, "{v:?} {s}: {peak}");
            }
        }
    }

    #[test]
    fn wash_drowns_without_booming_and_rings_out() {
        let song = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(3));
        let mut e = Engine::new(22_050.0, song, Shared::new(), None, 1);
        e.handle(Cmd::Play);
        let bar = (e.step_len() * STEPS as f64) as usize;
        let mut buf = vec![0.0; 2 * bar];
        let db = |b: &[f32]| 20.0 * stats(b).1.log10();
        e.render(&mut buf);
        e.render(&mut buf);
        let dry = db(&buf);
        e.handle(Cmd::Fx(Fx::Wash, true));
        e.render(&mut buf);
        e.render(&mut buf);
        let wet = db(&buf);
        assert!(buf.iter().all(|x| x.is_finite() && x.abs() <= 1.0));
        assert!(wet < dry + 2.5, "the wash doesn't pile up: {dry:.1} → {wet:.1} dB");
        // Let go and stop: the room keeps ringing for a while
        e.handle(Cmd::Fx(Fx::Wash, false));
        e.handle(Cmd::Stop);
        e.render(&mut buf);
        e.render(&mut buf);
        assert!(db(&buf) > -45.0, "the tail rings out: {:.1} dB", db(&buf));
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
        e.handle(Cmd::Fx(Fx::Stutter, true)); // loops steps 5 and 6
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

    /// Prints the loudness of each track – and each of its sounds – on its own, for balancing the mix:
    /// `cargo test --release print_levels -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn print_levels() {
        let base = gen::generate(Vibe::Peak, &Default::default(), &mut Rng::new(11));
        for (i, v) in Voice::ALL.into_iter().enumerate() {
            for sound in 0..v.sounds().len() {
                let mut s = base;
                for (j, t) in s.tracks.iter_mut().enumerate() {
                    t.mute = j != i;
                }
                s.tracks[i].sound = sound as u8;
                s.macros.space = 0.0;
                s.macros.dark = 0.0;
                let buf = render_offline(&s, 4, 44_100, 1);
                let n = buf.len() - 44_100 * 4; // without the tail
                let rms = (buf[..n].iter().map(|x| x * x).sum::<f32>() / n as f32).sqrt();
                let peak = buf.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                println!(
                    "{:10} {:11} rms {:6.1} dB  peak {:6.1} dB",
                    v.name(),
                    v.sounds()[sound],
                    20.0 * rms.log10(),
                    20.0 * peak.log10()
                );
            }
        }
    }
}
