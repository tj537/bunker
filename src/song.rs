//! The song: eight tracks of up to 16 steps, tempo, key and the four macros.
//! Everything is `Copy`, so the UI and the audio thread can swap whole songs
//! without allocating.

use serde::{Deserialize, Serialize};

pub const TRACKS: usize = 8;
pub const STEPS: usize = 16;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Voice {
    Kick,
    Clap,
    Rim,
    Hat,
    OpenHat,
    Ride,
    Bass,
    Acid,
}

impl Voice {
    pub const ALL: [Voice; TRACKS] = [
        Voice::Kick,
        Voice::Clap,
        Voice::Rim,
        Voice::Hat,
        Voice::OpenHat,
        Voice::Ride,
        Voice::Bass,
        Voice::Acid,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Voice::Kick => "Kick",
            Voice::Clap => "Clap",
            Voice::Rim => "Rim",
            Voice::Hat => "Hat",
            Voice::OpenHat => "Open hat",
            Voice::Ride => "Ride",
            Voice::Bass => "Bass",
            Voice::Acid => "Acid",
        }
    }

    /// Synth tracks play notes; drum tracks ignore `Step::note`.
    pub fn is_synth(self) -> bool {
        matches!(self, Voice::Bass | Voice::Acid)
    }

    pub fn index(self) -> usize {
        Voice::ALL.iter().position(|&v| v == self).unwrap_or(0)
    }

    /// The sounds each track can switch between. The first one is the classic
    /// default, so saved songs from before sounds existed play as they did.
    pub fn sounds(self) -> &'static [&'static str] {
        match self {
            Voice::Kick => &["909", "808", "Hard", "Deep", "Punch"],
            Voice::Clap => &["Clap", "Snare", "Big clap", "Snap"],
            Voice::Rim => &["Rim", "Cowbell", "Clave", "Tom", "Blip"],
            Voice::Hat => &["909", "808", "Shaker", "Tick"],
            Voice::OpenHat => &["909", "808", "Sizzle"],
            Voice::Ride => &["Ride", "Bell", "Dark"],
            Voice::Bass => &["Rolling", "Sub", "Reese", "Pulse", "Donk"],
            Voice::Acid => &["303 saw", "303 square", "Stab", "Pluck", "Hoover"],
        }
    }

    /// One line about what the track is for – shown when hovering its name
    pub fn blurb(self) -> &'static str {
        match self {
            Voice::Kick => "The heartbeat: four on the floor",
            Voice::Clap => "The backbeat on 2 and 4",
            Voice::Rim => "Percussion that bounces around the beat",
            Voice::Hat => "Closed hi-hat: drives the groove forward",
            Voice::OpenHat => "Open hi-hat: the off-beat \"tsss\"",
            Voice::Ride => "Ride cymbal: shimmer on top",
            Voice::Bass => "Bassline in the song's key",
            Voice::Acid => "Synth line: 303 acid, chord stabs, plucks or a rave hoover",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Step {
    pub on: bool,
    /// Louder, and for the acid line: brighter and snappier
    pub accent: bool,
    /// Semitones above the song's root (synth tracks only)
    pub note: i8,
    /// Glide into this note without retriggering (acid only)
    pub slide: bool,
    /// Probability in percent that the step plays
    pub chance: u8,
}

impl Default for Step {
    fn default() -> Self {
        Step {
            on: false,
            accent: false,
            note: 0,
            slide: false,
            chance: 100,
        }
    }
}

impl Step {
    pub fn hit() -> Self {
        Step {
            on: true,
            ..Default::default()
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Track {
    pub steps: [Step; STEPS],
    /// 1…16 – shorter tracks loop earlier (polyrhythms)
    pub len: u8,
    pub mute: bool,
    /// 0…1
    pub volume: f32,
    /// Index into `Voice::sounds()`
    pub sound: u8,
}

impl Default for Track {
    fn default() -> Self {
        Track {
            steps: [Step::default(); STEPS],
            len: STEPS as u8,
            mute: false,
            volume: 0.8,
            sound: 0,
        }
    }
}

impl Track {
    pub fn hits(&self) -> usize {
        self.steps[..self.len as usize].iter().filter(|s| s.on).count()
    }

    /// Index of the step that plays at global step `pos`
    pub fn step_at(&self, pos: u64) -> usize {
        (pos % self.len.max(1) as u64) as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Vibe {
    Dub,
    Minimal,
    Melodic,
    Hypnotic,
    Electro,
    Peak,
    Acid,
    Rave,
    Industrial,
    Hard,
}

impl Vibe {
    /// From calm to hard – the order of the chips in the vibe bar
    pub const ALL: [Vibe; 10] = [
        Vibe::Dub,
        Vibe::Minimal,
        Vibe::Melodic,
        Vibe::Hypnotic,
        Vibe::Electro,
        Vibe::Peak,
        Vibe::Acid,
        Vibe::Rave,
        Vibe::Industrial,
        Vibe::Hard,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Vibe::Dub => "Dub",
            Vibe::Minimal => "Minimal",
            Vibe::Melodic => "Melodic",
            Vibe::Hypnotic => "Hypnotic",
            Vibe::Electro => "Electro",
            Vibe::Peak => "Peak time",
            Vibe::Acid => "Acid",
            Vibe::Rave => "Rave",
            Vibe::Industrial => "Industrial",
            Vibe::Hard => "Hard",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Vibe::Dub => "Deep chord stabs drowning in echo",
            Vibe::Minimal => "Sparse, clicky, lots of air",
            Vibe::Melodic => "Plucked arpeggios over a warm sub",
            Vibe::Hypnotic => "Rolling bass, deep and steady",
            Vibe::Electro => "Broken 808 kicks, snares and cowbells",
            Vibe::Peak => "Big claps, rides, full energy",
            Vibe::Acid => "Squelchy 303 lines on top",
            Vibe::Rave => "90s hoovers, stabs and a reese bass",
            Vibe::Industrial => "Fast, dark and distorted",
            Vibe::Hard => "Distorted kicks and a donk bass, 150 BPM",
        }
    }

    /// Typical tempo range
    pub fn bpm(self) -> (f32, f32) {
        match self {
            Vibe::Dub => (118.0, 124.0),
            Vibe::Minimal => (124.0, 128.0),
            Vibe::Melodic => (122.0, 126.0),
            Vibe::Hypnotic => (128.0, 132.0),
            Vibe::Electro => (126.0, 132.0),
            Vibe::Peak => (132.0, 138.0),
            Vibe::Acid => (130.0, 136.0),
            Vibe::Rave => (138.0, 145.0),
            Vibe::Industrial => (138.0, 146.0),
            Vibe::Hard => (148.0, 156.0),
        }
    }

    /// Everything but electro keeps the kick on every beat
    pub fn four_on_the_floor(self) -> bool {
        self != Vibe::Electro
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Scale {
    Minor,
    Phrygian,
    Dorian,
}

impl Scale {
    pub fn intervals(self) -> [i8; 7] {
        match self {
            Scale::Minor => [0, 2, 3, 5, 7, 8, 10],
            Scale::Phrygian => [0, 1, 3, 5, 7, 8, 10],
            Scale::Dorian => [0, 2, 3, 5, 7, 9, 10],
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Scale::Minor => "minor",
            Scale::Phrygian => "phrygian",
            Scale::Dorian => "dorian",
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Scale::Minor => "dark and classic",
            Scale::Phrygian => "darker, a bit eastern",
            Scale::Dorian => "minor, but warmer",
        }
    }

    /// Closest note of the scale at or below `semis`, then `steps` scale degrees further
    pub fn shift(self, semis: i8, steps: i32) -> i8 {
        let iv = self.intervals();
        let octave = (semis as i32).div_euclid(12);
        let within = (semis as i32).rem_euclid(12) as i8;
        let degree = iv.iter().rposition(|&i| i <= within).unwrap_or(0) as i32;
        let d = degree + steps;
        let (o, k) = (d.div_euclid(7), d.rem_euclid(7));
        ((octave + o) * 12 + iv[k as usize] as i32).clamp(-24, 36) as i8
    }
}

/// The four big knobs, each 0…1
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Macros {
    pub energy: f32,
    pub dark: f32,
    pub acid: f32,
    pub space: f32,
}

impl Default for Macros {
    fn default() -> Self {
        Macros {
            energy: 0.6,
            dark: 0.45,
            acid: 0.5,
            space: 0.35,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Song {
    pub tracks: [Track; TRACKS],
    pub bpm: f32,
    /// 0…1: how late the off-16ths land
    pub swing: f32,
    /// MIDI note of the bass root (e.g. 41 = F2)
    pub root: u8,
    pub scale: Scale,
    pub vibe: Vibe,
    pub macros: Macros,
    pub master: f32,
    /// Auto mode: the machine keeps changing the groove
    pub auto: bool,
    /// Bars between two auto changes (2, 4 or 8)
    pub auto_bars: u8,
}

impl Default for Song {
    fn default() -> Self {
        let mut song = crate::gen::generate(Vibe::Hypnotic, &Macros::default(), &mut crate::rng::Rng::new(909));
        // The first beat anyone hears should have a clap on 2 and 4
        let clap = song.track_mut(Voice::Clap);
        clap.steps[4] = Step::hit();
        clap.steps[12] = Step::hit();
        song
    }
}

impl Song {
    pub fn empty() -> Self {
        let mut tracks = [Track::default(); TRACKS];
        tracks[Voice::Kick.index()].volume = 0.9;
        tracks[Voice::Ride.index()].volume = 0.55;
        tracks[Voice::Rim.index()].volume = 0.65;
        Song {
            tracks,
            bpm: 130.0,
            swing: 0.0,
            root: 41,
            scale: Scale::Minor,
            vibe: Vibe::Hypnotic,
            macros: Macros::default(),
            master: 0.8,
            auto: false,
            auto_bars: 8,
        }
    }

    pub fn track(&self, v: Voice) -> &Track {
        &self.tracks[v.index()]
    }

    pub fn track_mut(&mut self, v: Voice) -> &mut Track {
        &mut self.tracks[v.index()]
    }

    /// Repair values that would break the app (old or hand-edited saved state)
    pub fn sanitize(&mut self) {
        for (t, v) in self.tracks.iter_mut().zip(Voice::ALL) {
            t.len = t.len.clamp(1, STEPS as u8);
            if t.sound as usize >= v.sounds().len() {
                t.sound = 0;
            }
            t.volume = if t.volume.is_finite() { t.volume.clamp(0.0, 1.0) } else { 0.8 };
            for s in &mut t.steps {
                s.chance = s.chance.min(100);
                s.note = s.note.clamp(-24, 36);
            }
        }
        let fix = |v: f32, lo: f32, hi: f32, def: f32| if v.is_finite() { v.clamp(lo, hi) } else { def };
        self.bpm = fix(self.bpm, 90.0, 180.0, 130.0);
        self.swing = fix(self.swing, 0.0, 1.0, 0.0);
        self.master = fix(self.master, 0.0, 1.0, 0.8);
        let m = &mut self.macros;
        m.energy = fix(m.energy, 0.0, 1.0, 0.6);
        m.dark = fix(m.dark, 0.0, 1.0, 0.45);
        m.acid = fix(m.acid, 0.0, 1.0, 0.5);
        m.space = fix(m.space, 0.0, 1.0, 0.35);
        self.root = self.root.clamp(36, 47);
        if ![2, 4, 8].contains(&self.auto_bars) {
            self.auto_bars = if self.auto_bars > 8 { 8 } else { 4 };
        }
    }

    pub fn note_name(&self) -> &'static str {
        // "#" rather than "♯": the UI fonts have no sharp sign
        const NAMES: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
        NAMES[self.root as usize % 12]
    }

    pub fn root_name(&self) -> String {
        format!("{} {}", self.note_name(), self.scale.name())
    }

    pub fn sound_name(&self, v: Voice) -> &'static str {
        v.sounds()[self.track(v).sound as usize % v.sounds().len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_shift_stays_in_scale() {
        let s = Scale::Minor;
        assert_eq!(s.shift(0, 1), 2);
        assert_eq!(s.shift(0, 2), 3);
        assert_eq!(s.shift(0, 7), 12);
        assert_eq!(s.shift(0, -1), -2);
        assert_eq!(s.shift(1, 0), 0); // C♯ is not in C minor → snaps down to C
        for n in -12..24 {
            for d in -3..4 {
                let r = s.shift(n, d).rem_euclid(12);
                assert!(s.intervals().contains(&r), "{n} {d} → {r}");
            }
        }
    }

    #[test]
    fn sanitize_repairs_broken_values() {
        let mut s = Song::default();
        s.tracks[2].len = 0;
        s.tracks[3].volume = f32::NAN;
        s.bpm = 9999.0;
        s.root = 3;
        s.auto_bars = 16;
        s.tracks[7].sound = 99;
        s.sanitize();
        assert_eq!(s.tracks[2].len, 1);
        assert_eq!(s.tracks[3].volume, 0.8);
        assert_eq!(s.bpm, 180.0);
        assert_eq!(s.root, 36);
        assert_eq!(s.auto_bars, 8);
        assert_eq!(s.tracks[7].sound, 0);
        let mut ok = Song::default();
        let before = ok;
        ok.sanitize();
        assert_eq!(ok, before);
    }

    #[test]
    fn song_round_trips_through_json_shape() {
        // serde derives must cover every field (saved state)
        let song = Song::default();
        let copy = song;
        assert_eq!(song, copy);
        assert_eq!(Voice::ALL.len(), TRACKS);
        for (i, v) in Voice::ALL.iter().enumerate() {
            assert_eq!(v.index(), i);
        }
    }
}
