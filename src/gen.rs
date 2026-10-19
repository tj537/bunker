//! The groove generator behind *Shuffle* and the auto mode.
//!
//! Techno has strong conventions – four-to-the-floor kick, claps on 2 and 4,
//! off-beat hats, a rolling bass that dodges the kick. The generator starts from
//! those, then varies density with the Energy macro and spices things up with
//! Euclidean rhythms, so every roll sounds like techno but never the same.

use crate::rng::Rng;
use crate::song::{Macros, Scale, Song, Step, Track, Vibe, Voice, STEPS};

/// `k` hits spread as evenly as possible over `n` steps, rotated by `rot`
pub fn euclid(k: usize, n: usize, rot: usize) -> [bool; STEPS] {
    let mut out = [false; STEPS];
    if n == 0 {
        return out;
    }
    let k = k.min(n);
    for i in 0..n {
        // Bresenham-style distribution
        if (i * k) % n < k {
            out[(i + rot) % n] = true;
        }
    }
    out
}

fn from_mask(mask: [bool; STEPS]) -> Track {
    let mut t = Track::default();
    for (s, on) in t.steps.iter_mut().zip(mask) {
        s.on = on;
    }
    t
}

fn from_positions(pos: &[usize]) -> Track {
    let mut t = Track::default();
    for &p in pos {
        t.steps[p % STEPS] = Step::hit();
    }
    t
}

/// Chord changes per half bar, as scale degrees: i–i, i–VI, i–iv, i–VII, i–v
const PROGRESSIONS: [[i32; 2]; 5] = [[0, 0], [0, 5], [0, 3], [0, -1], [0, 4]];

/// The song's chord progression – derived from key and scale, so bass and synth
/// agree even when only one of them is rolled again
fn progression(song: &Song) -> [i32; 2] {
    let k = song.root as usize + song.scale as usize * 5;
    PROGRESSIONS[k % PROGRESSIONS.len()]
}

/// A whole new groove
pub fn generate(vibe: Vibe, m: &Macros, rng: &mut Rng) -> Song {
    let mut song = Song::empty();
    song.vibe = vibe;
    song.macros = *m;
    let (lo, hi) = vibe.bpm();
    song.bpm = (lo + rng.f32() * (hi - lo)).round();
    song.root = *rng.pick(&[36u8, 37, 38, 39, 40, 41, 42, 43]); // C2 … G2
    song.scale = match vibe {
        Vibe::Acid | Vibe::Industrial | Vibe::Hard => *rng.pick(&[Scale::Phrygian, Scale::Minor]),
        Vibe::Minimal | Vibe::Dub => *rng.pick(&[Scale::Dorian, Scale::Minor]),
        Vibe::Melodic => *rng.pick(&[Scale::Minor, Scale::Dorian, Scale::Minor]),
        _ => *rng.pick(&[Scale::Minor, Scale::Minor, Scale::Phrygian, Scale::Dorian]),
    };
    song.swing = match vibe {
        Vibe::Minimal => 0.15 + rng.f32() * 0.2,
        Vibe::Dub | Vibe::Electro => 0.08 + rng.f32() * 0.15,
        Vibe::Hypnotic | Vibe::Melodic => rng.f32() * 0.12,
        _ => 0.0,
    };
    // Sounds first: the synth line is written for the sound that plays it
    for v in Voice::ALL {
        song.track_mut(v).sound = sound_for(v, vibe, rng);
    }
    for v in Voice::ALL {
        let vol = song.track(v).volume;
        *song.track_mut(v) = track_for(v, vibe, m.energy, &song, rng);
        song.track_mut(v).volume = vol;
    }
    song
}

/// A sound that suits the vibe (index into `Voice::sounds()`)
pub fn sound_for(v: Voice, vibe: Vibe, rng: &mut Rng) -> u8 {
    use Vibe::*;
    let options: &[u8] = match v {
        // 909, 808, Hard, Deep, Punch
        Voice::Kick => match vibe {
            Dub => &[3, 1, 3],
            Minimal => &[4, 0, 3],
            Melodic => &[0, 3, 4],
            Hypnotic => &[0, 3, 0],
            Electro => &[1],
            Peak | Acid => &[0, 4, 0],
            Rave => &[2, 0],
            Industrial => &[2, 0, 2],
            Hard => &[2],
        },
        // Clap, Snare, Big clap, Snap
        Voice::Clap => match vibe {
            Dub | Minimal => &[3, 0],
            Melodic => &[2, 0],
            Hypnotic => &[0, 3],
            Electro => &[1],
            Peak => &[0, 2],
            Acid => &[0, 1],
            Rave => &[1, 2],
            Industrial => &[1, 0],
            Hard => &[0, 1, 2],
        },
        // Rim, Cowbell, Clave, Tom, Blip
        Voice::Rim => match vibe {
            Dub => &[0, 4],
            Minimal => &[0, 2, 4],
            Melodic => &[2, 0, 3],
            Hypnotic => &[0, 3],
            Electro => &[1, 2],
            Peak => &[0, 3],
            Acid => &[0, 1],
            Rave => &[1, 0],
            Industrial | Hard => &[3, 0],
        },
        // 909, 808, Shaker, Tick
        Voice::Hat => match vibe {
            Dub => &[2, 1],
            Minimal => &[3, 0, 2],
            Melodic => &[2, 0],
            Electro => &[1],
            Hard | Industrial => &[0],
            _ => &[0, 1, 0],
        },
        // 909, 808, Sizzle
        Voice::OpenHat => match vibe {
            Electro => &[1],
            Dub => &[1, 2],
            Melodic | Minimal => &[0, 2],
            _ => &[0, 0, 1],
        },
        // Ride, Bell, Dark
        Voice::Ride => match vibe {
            Minimal => &[1, 0],
            Dub => &[2],
            Electro => &[1],
            _ => &[0, 2, 0],
        },
        // Rolling, Sub, Reese, Pulse, Donk
        Voice::Bass => match vibe {
            Dub => &[1],
            Minimal => &[1, 3, 0],
            Melodic => &[1, 2],
            Hypnotic => &[0, 2, 0],
            Electro => &[3, 1],
            Peak => &[0, 3],
            Acid => &[0, 1],
            Rave => &[2, 4],
            Industrial => &[0, 2],
            Hard => &[4, 0],
        },
        // 303 saw, 303 square, Stab, Pluck, Hoover
        Voice::Acid => match vibe {
            Dub => &[2],
            Minimal => &[3, 1, 2],
            Melodic => &[3, 3, 2],
            Hypnotic => &[0, 3, 2],
            Electro => &[1, 3],
            Peak => &[2, 0],
            Acid => &[0, 1, 0],
            Rave => &[4, 2],
            Industrial => &[0, 4],
            Hard => &[4, 2],
        },
    };
    *rng.pick(options)
}

/// A fresh pattern for one track that fits the vibe (keeps the track's sound)
pub fn track_for(v: Voice, vibe: Vibe, energy: f32, song: &Song, rng: &mut Rng) -> Track {
    let e = energy.clamp(0.0, 1.0);
    let sound = song.track(v).sound;
    let mut t = match v {
        Voice::Kick => kick(vibe, rng),
        Voice::Clap => clap(vibe, e, rng),
        Voice::Rim => rim(vibe, e, rng),
        Voice::Hat => hat(vibe, e, rng),
        Voice::OpenHat => open_hat(vibe, e, rng),
        Voice::Ride => ride(vibe, e, rng),
        Voice::Bass => bass(vibe, e, song, rng),
        Voice::Acid => synth_line(vibe, e, song, rng),
    };
    t.volume = song.track(v).volume;
    t.sound = sound;
    t.mute = match v {
        // The synth line plays in the vibes made for it – elsewhere it often waits muted for the auto mode
        Voice::Acid => match vibe {
            Vibe::Acid | Vibe::Dub | Vibe::Melodic => false,
            Vibe::Rave => rng.chance(0.25),
            _ => !rng.chance(0.3),
        },
        Voice::Ride => matches!(vibe, Vibe::Minimal | Vibe::Dub) && rng.chance(0.6),
        Voice::Rim => matches!(vibe, Vibe::Hard) && rng.chance(0.4),
        _ => false,
    };
    t
}

fn kick(vibe: Vibe, rng: &mut Rng) -> Track {
    if !vibe.four_on_the_floor() {
        // Electro: broken kick patterns, always on the one
        let mut t = from_positions(rng.pick(&[
            &[0usize, 6, 10][..],
            &[0, 3, 8, 11],
            &[0, 7, 10],
            &[0, 6, 9, 12],
            &[0, 10],
            &[0, 3, 6, 10, 13],
        ]));
        t.steps[0].accent = true;
        if rng.chance(0.4) {
            t.steps[*rng.pick(&[13usize, 14, 15])] = Step { chance: 60, ..Step::hit() };
        }
        return t;
    }
    let mut t = from_positions(&[0, 4, 8, 12]);
    for i in [0, 4, 8, 12] {
        t.steps[i].accent = true;
    }
    // Ghost kicks: zero to three soft extra hits, so every roll of the dice sounds different
    let ghosts: &[usize] = match vibe {
        Vibe::Industrial | Vibe::Hard => &[2, 3, 6, 10, 11, 14, 15],
        Vibe::Minimal | Vibe::Hypnotic | Vibe::Dub => &[3, 7, 10, 11, 15],
        _ => &[7, 10, 14, 15],
    };
    let n = match vibe {
        Vibe::Industrial | Vibe::Hard => rng.range(1, 3),
        Vibe::Rave | Vibe::Peak => rng.range(0, 1),
        _ => rng.range(0, 2),
    };
    for _ in 0..n {
        let at = *rng.pick(ghosts);
        t.steps[at] = Step {
            chance: *rng.pick(&[50u8, 70, 100]),
            ..Step::hit()
        };
    }
    // Sometimes the last beat of the bar rolls
    if matches!(vibe, Vibe::Hard | Vibe::Rave) && rng.chance(0.3) {
        t.steps[14] = Step::hit();
        t.steps[15] = Step { chance: 70, ..Step::hit() };
    }
    t
}

fn clap(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let mut t = match vibe {
        Vibe::Minimal | Vibe::Dub => from_positions(if rng.chance(0.5) { &[12] } else { &[4, 12] }),
        Vibe::Hypnotic => {
            if rng.chance(0.35) {
                Track::default()
            } else {
                from_positions(&[4, 12])
            }
        }
        Vibe::Electro => from_positions(rng.pick(&[&[4usize, 12][..], &[4, 12, 15], &[4, 11, 12]])),
        _ => from_positions(&[4, 12]),
    };
    if e > 0.55 && rng.chance(e - 0.3) {
        t.steps[*rng.pick(&[7usize, 9, 14, 15])] = Step { chance: 70, ..Step::hit() };
    }
    if matches!(vibe, Vibe::Industrial | Vibe::Hard) && rng.chance(0.5) {
        t.steps[13] = Step { chance: 50, ..Step::hit() };
    }
    t
}

fn rim(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let k = match vibe {
        Vibe::Minimal => rng.range(3, 6),
        Vibe::Industrial | Vibe::Electro => rng.range(4, 7),
        Vibe::Dub => rng.range(1, 3),
        _ => rng.range(2, 5),
    } as usize
        + (e * 2.0) as usize;
    let mut t = from_mask(euclid(k, STEPS, rng.below(16) as usize));
    // Rims shouldn't sit on the kick
    for i in [0, 4, 8, 12] {
        t.steps[i].on = false;
    }
    if t.hits() == 0 {
        t.steps[*rng.pick(&[3usize, 6, 10, 13])] = Step::hit();
    }
    for s in t.steps.iter_mut().filter(|s| s.on) {
        s.accent = rng.chance(0.3);
        if rng.chance(0.25) {
            s.chance = 65;
        }
    }
    // A shorter rim loop drifts against the bar – hypnotic
    if matches!(vibe, Vibe::Hypnotic | Vibe::Minimal | Vibe::Dub | Vibe::Melodic) && rng.chance(0.4) {
        t.len = *rng.pick(&[12u8, 14, 15]);
    }
    t
}

fn hat(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let density = e + rng.f32() * 0.3 - 0.15;
    let sixteenths = density > 0.55 || matches!(vibe, Vibe::Industrial | Vibe::Peak | Vibe::Hard | Vibe::Rave);
    let mut t = if sixteenths {
        // 16ths, accents on the off-beats, some gaps when the energy is low
        let mut t = from_positions(&(0..16).collect::<Vec<_>>());
        let gaps = rng.chance(0.3);
        for (i, s) in t.steps.iter_mut().enumerate() {
            s.accent = i % 4 == 2;
            if i % 2 == 1 && rng.chance(0.3 * (1.0 - e)) {
                s.on = false;
            }
            if gaps && i % 4 == 0 {
                s.on = false;
            }
        }
        t
    } else {
        match rng.below(3) {
            0 => from_positions(&[2, 6, 10, 14]),
            1 => from_positions(&[2, 3, 6, 10, 11, 14]),
            _ => {
                let mut t = from_positions(&[1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15]);
                for i in [2, 6, 10, 14] {
                    t.steps[i].accent = true;
                }
                t
            }
        }
    };
    if matches!(vibe, Vibe::Minimal) && rng.chance(0.5) {
        t = from_mask(euclid(rng.range(5, 9) as usize, STEPS, 2));
    }
    if matches!(vibe, Vibe::Dub | Vibe::Melodic) && rng.chance(0.5) {
        // Shaker groove: soft 16ths, louder off-beats
        t = from_positions(&(0..16).collect::<Vec<_>>());
        for (i, s) in t.steps.iter_mut().enumerate() {
            s.accent = i % 4 == 2;
            s.chance = if i % 2 == 1 { 70 } else { 100 };
        }
    }
    t
}

fn open_hat(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let mut t = from_positions(&[2, 6, 10, 14]);
    if matches!(vibe, Vibe::Minimal | Vibe::Dub) || (e < 0.35 && rng.chance(0.5)) {
        t = from_positions(&[*rng.pick(&[6usize, 14])]);
    } else if matches!(vibe, Vibe::Electro) {
        t = from_positions(rng.pick(&[&[2usize, 10][..], &[6, 14], &[14]]));
    } else if rng.chance(0.2) {
        t = from_mask(euclid(3, 8, 2));
        t.len = 8;
    }
    if e > 0.75 && rng.chance(0.3) {
        t.steps[15] = Step { chance: 50, ..Step::hit() };
    }
    t
}

fn ride(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    match vibe {
        Vibe::Peak | Vibe::Industrial | Vibe::Hard | Vibe::Rave => {
            let mut t = if e > 0.5 {
                from_positions(&[0, 2, 4, 6, 8, 10, 12, 14])
            } else {
                from_positions(&[2, 6, 10, 14])
            };
            for i in [2, 6, 10, 14] {
                t.steps[i].accent = true;
            }
            t
        }
        _ => {
            let mut t = from_mask(euclid(rng.range(3, 5) as usize, STEPS, rng.below(4) as usize * 2));
            if rng.chance(0.5) {
                t.len = 12;
            }
            t
        }
    }
}

fn bass(vibe: Vibe, e: f32, song: &Song, rng: &mut Rng) -> Track {
    let scale = song.scale;
    // Rolling bass lives between the kicks
    let template: &[usize] = match vibe {
        Vibe::Hypnotic | Vibe::Industrial => {
            if rng.chance(0.6) {
                &[1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15]
            } else {
                &[2, 3, 6, 7, 10, 11, 14, 15]
            }
        }
        Vibe::Peak => rng.pick(&[
            &[2usize, 6, 10, 14][..],
            &[2, 3, 6, 7, 10, 11, 14, 15][..],
            &[1, 2, 5, 6, 9, 10, 13, 14],
        ]),
        Vibe::Minimal => rng.pick(&[&[3usize, 6, 11, 14][..], &[2, 7, 10, 15][..], &[3, 7, 10, 14]]),
        Vibe::Acid | Vibe::Hard => rng.pick(&[&[2usize, 6, 10, 14][..], &[2, 3, 6, 7, 10, 11, 14, 15]]),
        Vibe::Rave => rng.pick(&[&[2usize, 6, 10, 14][..], &[2, 5, 6, 10, 13, 14]]),
        Vibe::Dub => rng.pick(&[&[2usize, 10][..], &[3, 11], &[2, 7, 10]]),
        Vibe::Melodic => rng.pick(&[&[2usize, 6, 10, 14][..], &[2, 3, 6, 10, 11, 14]]),
        Vibe::Electro => rng.pick(&[&[2usize, 3, 7, 11, 14][..], &[1, 4, 7, 9, 12, 14], &[3, 4, 7, 11, 12, 15]]),
    };
    let mut t = from_positions(template);
    let prog = progression(song);
    let fifth = scale.shift(0, 4);
    let follows_chords = matches!(vibe, Vibe::Dub | Vibe::Melodic | Vibe::Rave) || rng.chance(0.25);
    for (i, s) in t.steps.iter_mut().enumerate() {
        if !s.on {
            continue;
        }
        let home = if follows_chords { scale.shift(0, prog[i / 8]) } else { 0 };
        // Folded back below the root when a chord takes it too high
        let home = if home > 7 { home - 12 } else { home };
        s.note = home;
        // Mostly the root; a few octave jumps and fifths for movement
        if rng.chance(0.12 + e * 0.1) {
            s.note = home + 12;
        } else if rng.chance(0.08) && !follows_chords {
            s.note = fifth;
        }
        s.accent = i % 4 == 2;
    }
    if matches!(vibe, Vibe::Minimal) && rng.chance(0.5) {
        let k = t.steps.iter().filter(|s| s.on).count();
        for s in t.steps.iter_mut().filter(|s| s.on).take(k / 2) {
            s.note = scale.shift(0, -2) + 12;
        }
    }
    // Never on top of a kick: they would fight for the low end
    let kick = song.track(Voice::Kick);
    for (s, k) in t.steps.iter_mut().zip(kick.steps) {
        if k.on && kick.len == 16 {
            s.on = false;
        }
    }
    t
}

/// The synth track – the pattern depends on its sound
fn synth_line(vibe: Vibe, e: f32, song: &Song, rng: &mut Rng) -> Track {
    match song.track(Voice::Acid).sound {
        2 => stabs(vibe, song, rng),
        3 => arp(e, song, rng),
        4 => hoover(song, rng),
        _ => acid(vibe, e, song.scale, rng),
    }
}

/// Chord stabs on the off-beats, one chord per half bar
fn stabs(vibe: Vibe, song: &Song, rng: &mut Rng) -> Track {
    let pos: &[usize] = if matches!(vibe, Vibe::Dub) {
        rng.pick(&[&[3usize, 10][..], &[2, 10], &[3, 6, 11], &[2, 7, 10, 13]])
    } else {
        rng.pick(&[&[2usize, 6, 10, 14][..], &[3, 6, 11, 14], &[2, 5, 10, 13], &[0, 3, 6, 10, 12]])
    };
    let mut t = from_positions(pos);
    let prog = progression(song);
    for (i, s) in t.steps.iter_mut().enumerate().filter(|(_, s)| s.on) {
        s.note = song.scale.shift(0, prog[i / 8]) + 12;
        s.accent = rng.chance(0.3);
    }
    t
}

/// A plucked arpeggio walking up and down the chord
fn arp(e: f32, song: &Song, rng: &mut Rng) -> Track {
    let mut t = Track::default();
    let prog = progression(song);
    let shape: &[i32] = rng.pick(&[&[0, 2, 4, 7][..], &[0, 4, 2, 4], &[0, 2, 4, 2], &[0, 7, 4, 2], &[4, 2, 0, 2]]);
    let eighths = e < 0.45 || rng.chance(0.3);
    let lift = if rng.chance(0.4) { 12 } else { 0 };
    for (i, s) in t.steps.iter_mut().enumerate() {
        if eighths && i % 2 == 1 {
            continue;
        }
        s.on = !(i % 4 == 0 && rng.chance(0.25));
        let k = if eighths { i / 2 } else { i };
        s.note = song.scale.shift(0, prog[i / 8] + shape[k % shape.len()]) + 12 + lift;
        s.accent = i % 4 == 2 || rng.chance(0.15);
    }
    t.steps[0].on = true;
    if rng.chance(0.3) {
        t.len = *rng.pick(&[12u8, 14]);
    }
    t
}

/// A rave hoover riff: few long notes with slides
fn hoover(song: &Song, rng: &mut Rng) -> Track {
    let pos: &[usize] = rng.pick(&[&[0usize, 3, 6, 10][..], &[0, 6, 8, 14], &[2, 6, 10, 14], &[0, 3, 8, 11]]);
    let mut t = from_positions(pos);
    let degrees = [0, 0, 2, -1, 4, 3];
    for s in t.steps.iter_mut().filter(|s| s.on) {
        s.note = song.scale.shift(0, *rng.pick(&degrees)) + 12;
        s.slide = rng.chance(0.3);
        s.accent = rng.chance(0.4);
    }
    t
}

/// A 303-style line: notes from the scale, accents, slides and octave jumps
pub fn acid(vibe: Vibe, e: f32, scale: Scale, rng: &mut Rng) -> Track {
    let mut t = Track::default();
    let density = 0.45 + e * 0.3 + if matches!(vibe, Vibe::Acid) { 0.1 } else { 0.0 };
    let mut degree = 0i32;
    for (i, s) in t.steps.iter_mut().enumerate() {
        s.on = i == 0 || rng.chance(density);
        if !s.on {
            continue;
        }
        degree = (degree + rng.range(-2, 2)).clamp(-2, 6);
        if rng.chance(0.15) {
            degree = 0; // home
        }
        s.note = scale.shift(0, degree) + if rng.chance(0.22) { 12 } else { 0 } + 12;
        s.accent = rng.chance(0.28);
        s.slide = i > 0 && rng.chance(0.22);
    }
    // Acid loves odd lengths
    if rng.chance(0.25) {
        t.len = *rng.pick(&[12u8, 14, 15]);
    }
    t
}

/// What the auto mode just did – shown in the UI
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Change {
    NewPattern(Voice),
    Muted(Voice),
    Unmuted(Voice),
    Shifted(Voice),
    NewSound(Voice, u8),
    NewNotes(Voice),
    Length(Voice, u8),
    Breakdown,
    Build,
    Drop,
    /// After a drop: new bassline and synth, maybe new sounds or a new key
    NewChapter,
    /// The scene chain moved on to scene `0…3`
    Scene(usize),
}

impl Change {
    pub fn describe(self) -> String {
        match self {
            Change::NewPattern(v) => format!("New {} pattern", v.name().to_lowercase()),
            Change::Muted(v) => format!("{} out", v.name()),
            Change::Unmuted(v) => format!("{} in", v.name()),
            Change::Shifted(v) => format!("{} shifted", v.name()),
            Change::NewSound(v, s) => format!("{} sound: {}", v.name(), v.sounds()[s as usize % v.sounds().len()]),
            Change::NewNotes(v) => format!("New {} notes", v.name().to_lowercase()),
            Change::Length(v, n) => format!("{} loops every {n} steps", v.name()),
            Change::Breakdown => "Breakdown".into(),
            Change::Build => "Build-up".into(),
            Change::Drop => "Drop!".into(),
            Change::NewChapter => "Drop! New chapter".into(),
            Change::Scene(i) => format!("Scene {}", ["A", "B", "C", "D"][i % 4]),
        }
    }
}

/// One step of the auto mode: change one thing so the groove keeps moving but
/// stays recognizable. (Breakdowns and drops are arranged by the engine.)
pub fn evolve(song: &mut Song, rng: &mut Rng) -> Change {
    let m = song.macros;
    let roll = rng.f32();
    let voices = [
        Voice::Clap,
        Voice::Rim,
        Voice::Hat,
        Voice::OpenHat,
        Voice::Ride,
        Voice::Bass,
        Voice::Acid,
    ];
    let v = *rng.pick(&voices);
    if roll < 0.28 {
        let fresh = track_for(v, song.vibe, m.energy, song, rng);
        let t = song.track_mut(v);
        let mute = t.mute;
        *t = Track {
            mute,
            volume: t.volume,
            ..fresh
        };
        Change::NewPattern(v)
    } else if roll < 0.5 {
        // Bring in or take out an element; keep at least four running
        let playing = voices.iter().filter(|&&x| !song.track(x).mute).count();
        let v = if !song.track(v).mute && playing <= 4 {
            let muted: Vec<Voice> = voices.iter().copied().filter(|&x| song.track(x).mute).collect();
            if muted.is_empty() {
                v
            } else {
                *rng.pick(&muted)
            }
        } else {
            v
        };
        let t = song.track_mut(v);
        t.mute = !t.mute;
        if t.mute {
            Change::Muted(v)
        } else {
            Change::Unmuted(v)
        }
    } else if roll < 0.62 {
        // Rotate the pattern by a step or two – same notes, new feel
        let t = song.track_mut(v);
        let len = t.len as usize;
        t.steps[..len].rotate_right(*rng.pick(&[1usize, 2, 3]));
        Change::Shifted(v)
    } else if roll < 0.76 {
        // Another sound on a playing track (the synth gets a line made for its new sound)
        let v = *rng.pick(&[Voice::Clap, Voice::Rim, Voice::Hat, Voice::OpenHat, Voice::Bass, Voice::Acid]);
        let n = v.sounds().len() as u8;
        let old = song.track(v).sound;
        let new = (old + 1 + rng.below(n as u32 - 1) as u8) % n;
        song.track_mut(v).sound = new;
        if v == Voice::Acid && !(new < 2 && old < 2) {
            let fresh = track_for(v, song.vibe, m.energy, song, rng);
            let t = song.track_mut(v);
            t.steps = fresh.steps;
            t.len = fresh.len;
        }
        Change::NewSound(v, new)
    } else if roll < 0.88 {
        // Same rhythm, new notes for bass or synth
        let v = *rng.pick(&[Voice::Bass, Voice::Acid]);
        let fresh = track_for(v, song.vibe, m.energy, song, rng);
        let t = song.track_mut(v);
        let mut fresh_notes = fresh.steps.iter().filter(|s| s.on).map(|s| s.note).cycle();
        for s in t.steps.iter_mut().filter(|s| s.on) {
            s.note = fresh_notes.next().unwrap_or(s.note);
        }
        Change::NewNotes(v)
    } else {
        // Polyrhythm: a loop gets shorter (or back to a full bar)
        let v = *rng.pick(&[Voice::Rim, Voice::Ride, Voice::Acid, Voice::OpenHat]);
        let t = song.track_mut(v);
        t.len = if t.len < 16 { 16 } else { *rng.pick(&[12u8, 14, 15]) };
        if t.hits() == 0 {
            t.steps[2] = Step::hit();
        }
        Change::Length(v, t.len)
    }
}

/// A bigger change after a drop: new bassline and synth line, often new sounds,
/// sometimes a key change
pub fn new_chapter(song: &mut Song, rng: &mut Rng) -> Change {
    let vibe = song.vibe;
    if rng.chance(0.3) {
        // Up a fourth or down a whole tone – stays in the bass range
        let shifted = song.root as i32 + *rng.pick(&[5, -2, 3, -5]);
        song.root = if (36..=47).contains(&shifted) {
            shifted as u8
        } else {
            (shifted - 12).clamp(36, 47) as u8
        };
    }
    for v in [Voice::Hat, Voice::Clap, Voice::Bass, Voice::Acid] {
        if rng.chance(0.4) {
            song.track_mut(v).sound = sound_for(v, vibe, rng);
        }
    }
    for v in [Voice::Bass, Voice::Acid, Voice::Rim] {
        let fresh = track_for(v, vibe, song.macros.energy, song, rng);
        let t = song.track_mut(v);
        let mute = if v == Voice::Acid { rng.chance(0.3) } else { t.mute };
        *t = Track {
            mute,
            volume: t.volume,
            ..fresh
        };
    }
    Change::NewChapter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn euclid_spreads_hits() {
        let e = euclid(4, 16, 0);
        assert_eq!(e.iter().filter(|&&b| b).count(), 4);
        assert!(e[0] && e[4] && e[8] && e[12]);
        assert_eq!(euclid(3, 8, 0).iter().filter(|&&b| b).count(), 3);
        assert_eq!(euclid(20, 16, 0).iter().filter(|&&b| b).count(), 16);
        assert_eq!(euclid(5, 16, 3).iter().filter(|&&b| b).count(), 5);
    }

    #[test]
    fn every_vibe_makes_a_techno_groove() {
        let mut rng = Rng::new(42);
        for vibe in Vibe::ALL {
            for _ in 0..50 {
                let s = generate(vibe, &Macros::default(), &mut rng);
                let kick = s.track(Voice::Kick);
                // Four to the floor (electro: at least on the one)
                if vibe.four_on_the_floor() {
                    for i in [0, 4, 8, 12] {
                        assert!(kick.steps[i].on, "{vibe:?}");
                    }
                }
                assert!(kick.steps[0].on);
                let (lo, hi) = vibe.bpm();
                assert!(s.bpm >= lo && s.bpm <= hi);
                // The bass never doubles the kick
                for i in 0..STEPS {
                    assert!(!(kick.steps[i].on && s.track(Voice::Bass).steps[i].on), "{vibe:?} step {i}");
                }
                for (t, v) in s.tracks.iter().zip(Voice::ALL) {
                    assert!((t.sound as usize) < v.sounds().len());
                }
                for t in &s.tracks {
                    assert!((1..=16).contains(&t.len));
                }
                // Synth notes stay in the scale
                for v in [Voice::Bass, Voice::Acid] {
                    for st in s.track(v).steps.iter().filter(|st| st.on) {
                        assert!(s.scale.intervals().contains(&st.note.rem_euclid(12)), "{v:?} {}", st.note);
                    }
                }
            }
        }
    }

    #[test]
    fn evolve_keeps_the_groove_alive() {
        let mut rng = Rng::new(3);
        let mut s = generate(Vibe::Peak, &Macros::default(), &mut rng);
        let before = s;
        let mut changes = 0;
        for _ in 0..200 {
            evolve(&mut s, &mut rng);
            changes += 1;
            // The kick is never touched by the auto mode
            assert_eq!(s.track(Voice::Kick), before.track(Voice::Kick));
            for t in &s.tracks {
                assert!((1..=16).contains(&t.len));
            }
            let playing = s.tracks[1..].iter().filter(|t| !t.mute).count();
            assert!(playing >= 4, "only {playing} tracks left");
        }
        assert_eq!(changes, 200);
        assert_ne!(s, before);
        s.sanitize();
    }

    #[test]
    fn vibes_sound_different() {
        // Each vibe has its own set of sounds – the generator must not pick the same kit everywhere
        let mut rng = Rng::new(8);
        let kits: Vec<[u8; 8]> = Vibe::ALL
            .iter()
            .map(|&v| generate(v, &Macros::default(), &mut rng).tracks.map(|t| t.sound))
            .collect();
        let distinct = kits.iter().filter(|k| kits.iter().filter(|o| o == k).count() == 1).count();
        assert!(distinct >= 7, "{kits:?}");
    }

    #[test]
    fn kick_dice_changes_something() {
        // The dice next to the kick used to roll the same four hits again and again
        let mut rng = Rng::new(12);
        let song = generate(Vibe::Hypnotic, &Macros::default(), &mut rng);
        let rolls: Vec<_> = (0..10)
            .map(|_| track_for(Voice::Kick, Vibe::Hypnotic, 0.6, &song, &mut rng).steps)
            .collect();
        let different = rolls.iter().filter(|r| **r != rolls[0]).count();
        assert!(different >= 4);
    }

    #[test]
    fn new_chapter_stays_in_range() {
        let mut rng = Rng::new(21);
        let mut s = generate(Vibe::Rave, &Macros::default(), &mut rng);
        for _ in 0..100 {
            new_chapter(&mut s, &mut rng);
            assert!((36..=47).contains(&s.root));
            let copy = s;
            s.sanitize();
            assert_eq!(s, copy);
        }
    }
}
