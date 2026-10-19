//! The groove generator behind *Shuffle* and the auto mode.
//!
//! Techno has strong conventions – four-to-the-floor kick, claps on 2 and 4,
//! off-beat hats, a rolling bass that dodges the kick. The generator starts from
//! those, then varies density with the Energy macro and spices things up with
//! Euclidean rhythms, so every roll sounds like techno but never the same.

use crate::form::Part;
use crate::rng::Rng;
use crate::song::{Macros, Scale, Song, Step, Track, Vibe, Voice, PROGRESSIONS, STEPS, TRACKS};

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
const HALF_BAR_CHORDS: [[i32; 2]; 5] = [[0, 0], [0, 5], [0, 3], [0, -1], [0, 4]];

/// The song's chord progression – derived from key and scale, so bass and synth
/// agree even when only one of them is rolled again
fn progression(song: &Song) -> [i32; 2] {
    // A song-wide progression moves bar by bar already – then the bar itself stays on one chord
    if song.prog != 0 {
        return [0, 0];
    }
    let k = song.root as usize + song.scale as usize * 5;
    HALF_BAR_CHORDS[k % HALF_BAR_CHORDS.len()]
}

/// A whole new groove
pub fn generate(vibe: Vibe, m: &Macros, rng: &mut Rng) -> Song {
    generate_keeping(vibe, m, rng, &[None; TRACKS])
}

/// A whole new groove that keeps the given tracks (the locked ones) as they are
pub fn generate_keeping(vibe: Vibe, m: &Macros, rng: &mut Rng, keep: &[Option<Track>; TRACKS]) -> Song {
    if vibe == Vibe::Custom {
        let mut song = generate_keeping(vibe.rules(), m, rng, keep);
        song.vibe = vibe;
        return song;
    }
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
    song.prog = *rng.pick(match vibe {
        Vibe::Melodic | Vibe::Dub | Vibe::Rave => &[1u8, 3, 4, 1, 0][..],
        Vibe::Hypnotic | Vibe::Custom | Vibe::Minimal => &[0, 0, 0, 2, 6],
        Vibe::Peak => &[0, 1, 3],
        Vibe::Electro => &[0, 2, 5],
        Vibe::Acid | Vibe::Industrial | Vibe::Hard => &[0, 0, 6],
    }) % PROGRESSIONS.len() as u8;
    // Sounds first: the synth line is written for the sound that plays it
    for v in Voice::ALL {
        song.track_mut(v).sound = sound_for(v, vibe, rng);
    }
    for (i, v) in Voice::ALL.into_iter().enumerate() {
        if let Some(t) = keep[i] {
            *song.track_mut(v) = t;
            continue;
        }
        let vol = song.track(v).volume;
        *song.track_mut(v) = track_for(v, vibe, m.energy, &song, rng);
        song.track_mut(v).volume = vol;
    }
    song
}

/// A sound that suits the vibe (index into `Voice::sounds()`)
pub fn sound_for(v: Voice, vibe: Vibe, rng: &mut Rng) -> u8 {
    *rng.pick(sounds_for(v, vibe))
}

/// The sounds that suit the vibe on a track (some twice: more likely)
fn sounds_for(v: Voice, vibe: Vibe) -> &'static [u8] {
    use Vibe::*;
    let vibe = vibe.rules();
    match v {
        // 909, 808, Hard, Deep, Punch
        Voice::Kick => match vibe {
            Dub => &[3, 1, 3],
            Minimal => &[4, 0, 3],
            Melodic => &[0, 3, 4],
            Hypnotic | Custom => &[0, 3, 0],
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
            Hypnotic | Custom => &[0, 3],
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
            Hypnotic | Custom => &[0, 3],
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
            Hypnotic | Custom => &[0, 2, 0],
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
            Hypnotic | Custom => &[0, 3, 2],
            Electro => &[1, 3],
            Peak => &[2, 0],
            Acid => &[0, 1, 0],
            Rave => &[4, 2],
            Industrial => &[0, 4],
            Hard => &[4, 2],
        },
    }
}

/// A fresh pattern for one track that fits the vibe (keeps the track's sound)
pub fn track_for(v: Voice, vibe: Vibe, energy: f32, song: &Song, rng: &mut Rng) -> Track {
    let vibe = vibe.rules();
    let e = energy.clamp(0.0, 1.0);
    let old = *song.track(v);
    let sound = old.sound;
    let mut t = match v {
        Voice::Kick => kick(vibe, rng),
        Voice::Clap => clap(vibe, e, rng),
        Voice::Rim => rim(vibe, e, song, rng),
        Voice::Hat => hat(vibe, e, rng),
        Voice::OpenHat => open_hat(vibe, e, rng),
        Voice::Ride => ride(vibe, e, rng),
        Voice::Bass => bass(vibe, e, song, rng),
        Voice::Acid => synth_line(vibe, e, song, rng),
    };
    t.volume = old.volume;
    t.sound = sound;
    t.tune = old.tune;
    t.decay = old.decay;
    t.lock = old.lock;
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
            t.steps[*rng.pick(&[13usize, 14, 15])] = Step::hit();
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
    // They always play (soft, unaccented) – a kick that only sometimes comes sounds like a stumble
    for _ in 0..n {
        t.steps[*rng.pick(ghosts)] = Step::hit();
    }
    // Sometimes the last beat of the bar rolls
    if matches!(vibe, Vibe::Hard | Vibe::Rave) && rng.chance(0.3) {
        t.steps[14] = Step::hit();
        t.steps[15] = Step::hit();
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
        t.steps[*rng.pick(&[7usize, 9, 14, 15])] = Step::hit();
    }
    if matches!(vibe, Vibe::Industrial | Vibe::Hard) && rng.chance(0.5) {
        t.steps[13] = Step::hit();
    }
    t
}

fn rim(vibe: Vibe, e: f32, song: &Song, rng: &mut Rng) -> Track {
    let k = match vibe {
        Vibe::Minimal => rng.range(3, 6),
        Vibe::Industrial | Vibe::Electro => rng.range(4, 7),
        Vibe::Dub => rng.range(1, 3),
        _ => rng.range(2, 5),
    } as usize
        + (e * 2.0) as usize;
    let mut t = from_mask(euclid(k, STEPS, rng.below(16) as usize));
    // Rims shouldn't sit on the kick or the clap
    let clap = song.track(Voice::Clap);
    for i in 0..STEPS {
        if i % 4 == 0 || (clap.len == 16 && clap.steps[i].on) {
            t.steps[i].on = false;
        }
    }
    if t.hits() == 0 {
        t.steps[*rng.pick(&[3usize, 6, 10, 13])] = Step::hit();
    }
    for s in t.steps.iter_mut().filter(|s| s.on) {
        s.accent = rng.chance(0.3);
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
        t.steps[15] = Step::hit();
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
        _ => from_mask(euclid(rng.range(3, 5) as usize, STEPS, rng.below(4) as usize * 2)),
    }
}

fn bass(vibe: Vibe, e: f32, song: &Song, rng: &mut Rng) -> Track {
    let scale = song.scale;
    // Rolling bass lives between the kicks
    let template: &[usize] = match vibe {
        Vibe::Hypnotic | Vibe::Custom | Vibe::Industrial => {
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
    // The bass plays the root of the chord the synths play over it, so they always agree
    let prog = progression(song);
    // Folded back below the root when a chord takes it too high
    let fold = |n: i8| if n > 7 { n - 12 } else { n };
    for (i, s) in t.steps.iter_mut().enumerate() {
        if !s.on {
            continue;
        }
        let home = fold(scale.shift(0, prog[i / 8]));
        s.note = home;
        // Mostly the root; a few octave jumps and fifths for movement
        if rng.chance(0.12 + e * 0.1) {
            s.note = home + 12;
        } else if rng.chance(0.08) {
            s.note = fold(scale.shift(0, prog[i / 8] + 4));
        }
        s.accent = i % 4 == 2;
    }
    if matches!(vibe, Vibe::Minimal) && rng.chance(0.5) {
        // Answer phrase: the first half an octave up on the chord's sixth below
        for (i, s) in t.steps.iter_mut().enumerate().take(8).filter(|(_, s)| s.on) {
            s.note = fold(scale.shift(0, prog[i / 8] - 2)) + 12;
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
        _ => acid(vibe, e, song, rng),
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
    t
}

/// A rave hoover riff: few long notes with slides
fn hoover(song: &Song, rng: &mut Rng) -> Track {
    let pos: &[usize] = rng.pick(&[&[0usize, 3, 6, 10][..], &[0, 6, 8, 14], &[2, 6, 10, 14], &[0, 3, 8, 11]]);
    let mut t = from_positions(pos);
    let degrees = [0, 0, 2, -1, 4, 3];
    let prog = progression(song);
    for (i, s) in t.steps.iter_mut().enumerate().filter(|(_, s)| s.on) {
        s.note = song.scale.shift(0, prog[i / 8] + *rng.pick(&degrees)) + 12;
        s.slide = rng.chance(0.3);
        s.accent = rng.chance(0.4);
    }
    t
}

/// A 303-style line: notes from the scale, accents, slides and octave jumps. On every
/// beat it lands on a note of the chord (root, third or fifth), in between it wanders.
pub fn acid(vibe: Vibe, e: f32, song: &Song, rng: &mut Rng) -> Track {
    let mut t = Track::default();
    let scale = song.scale;
    let prog = progression(song);
    let density = 0.45 + e * 0.3 + if matches!(vibe, Vibe::Acid) { 0.1 } else { 0.0 };
    let mut degree = 0i32;
    for (i, s) in t.steps.iter_mut().enumerate() {
        s.on = i == 0 || rng.chance(density);
        if !s.on {
            continue;
        }
        degree = if i % 4 == 0 {
            *rng.pick(&[0, 0, 2, 4])
        } else {
            (degree + rng.range(-2, 2)).clamp(-2, 6)
        };
        s.note = scale.shift(0, prog[i / 8] + degree) + if rng.chance(0.22) { 12 } else { 0 } + 12;
        s.accent = rng.chance(0.28);
        s.slide = i > 0 && rng.chance(0.22);
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
    /// Another pattern `0…3` came in (picked during the bar, it waited for the next one)
    Scene(usize),
    /// Song mode moved on to section `usize` of its form, playing pattern `Option<usize>`
    Part(Part, usize, Option<usize>),
    /// Song mode played its last bar
    SongEnd,
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
            Change::Scene(i) => format!("Favorite {}", i + 1),
            Change::Part(Part::Peak, ..) => "Drop! Peak".into(),
            Change::Part(p, _, slot) => match slot {
                Some(i) => format!("{} · favorite {}", p.name(), i + 1),
                None => p.name().into(),
            },
            Change::SongEnd => "The end".into(),
        }
    }
}

/// One step of the auto mode: change one thing so the groove keeps moving but
/// stays recognizable. Locked tracks are left alone; `None` when everything is locked.
/// (Breakdowns and drops are arranged by the engine.)
/// With `mutes` false it never takes a track out or brings one in (the energy bar decides).
pub fn evolve(song: &mut Song, rng: &mut Rng, mutes: bool) -> Option<Change> {
    let m = song.macros;
    let all = [
        Voice::Clap,
        Voice::Rim,
        Voice::Hat,
        Voice::OpenHat,
        Voice::Ride,
        Voice::Bass,
        Voice::Acid,
    ];
    let free = |song: &Song, list: &[Voice]| -> Vec<Voice> { list.iter().copied().filter(|&v| !song.track(v).lock).collect() };
    let voices = free(song, &all);
    if voices.is_empty() {
        return None;
    }
    let roll = rng.f32();
    let v = *rng.pick(&voices);
    let synths = free(song, &[Voice::Bass, Voice::Acid]);
    // Shorter loops only for percussion – a shorter synth line would miss the chord changes
    let loops = free(song, &[Voice::Rim, Voice::Ride, Voice::OpenHat]);
    if roll < 0.28 {
        let fresh = track_for(v, song.vibe, m.energy, song, rng);
        let t = song.track_mut(v);
        let mute = t.mute;
        *t = Track {
            mute,
            volume: t.volume,
            ..fresh
        };
        Some(Change::NewPattern(v))
    } else if roll < 0.5 && !mutes {
        Some(rotate(song, v, rng))
    } else if roll < 0.5 {
        // Bring in or take out an element; keep at least four running
        let playing = all.iter().filter(|&&x| !song.track(x).mute).count();
        let v = if !song.track(v).mute && playing <= 4 {
            let muted: Vec<Voice> = voices.iter().copied().filter(|&x| song.track(x).mute).collect();
            if muted.is_empty() {
                return Some(rotate(song, v, rng));
            }
            *rng.pick(&muted)
        } else {
            v
        };
        let t = song.track_mut(v);
        t.mute = !t.mute;
        Some(if t.mute { Change::Muted(v) } else { Change::Unmuted(v) })
    } else if roll < 0.62 {
        Some(rotate(song, v, rng))
    } else if roll < 0.76 {
        // Another sound that suits the vibe (the synth gets a line made for its new sound)
        let v = if v == Voice::Ride { voices[0] } else { v };
        let old = song.track(v).sound;
        let options: Vec<u8> = sounds_for(v, song.vibe).iter().copied().filter(|&s| s != old).collect();
        if options.is_empty() {
            return Some(rotate(song, v, rng));
        }
        let new = *rng.pick(&options);
        song.track_mut(v).sound = new;
        if v == Voice::Acid && !(new < 2 && old < 2) {
            let fresh = track_for(v, song.vibe, m.energy, song, rng);
            let t = song.track_mut(v);
            t.steps = fresh.steps;
            t.len = fresh.len;
        }
        Some(Change::NewSound(v, new))
    } else if roll < 0.88 && !synths.is_empty() {
        // Same rhythm, new notes for bass or synth
        let v = *rng.pick(&synths);
        let fresh = track_for(v, song.vibe, m.energy, song, rng);
        let t = song.track_mut(v);
        let mut fresh_notes = fresh.steps.iter().filter(|s| s.on).map(|s| s.note).cycle();
        for s in t.steps.iter_mut().filter(|s| s.on) {
            s.note = fresh_notes.next().unwrap_or(s.note);
        }
        Some(Change::NewNotes(v))
    } else if !loops.is_empty() {
        // A loop gets half a bar long (or back to a full bar) – always in step with the beat
        let v = *rng.pick(&loops);
        let t = song.track_mut(v);
        t.len = if t.len < 16 { 16 } else { 8 };
        if t.hits() == 0 {
            t.steps[2] = Step::hit();
        }
        Some(Change::Length(v, t.len))
    } else {
        Some(rotate(song, v, rng))
    }
}

/// New notes on the same rhythm for bass and synth (not when locked), e.g. after the
/// mood changed, so they follow the new scale and chords
pub fn refit_notes(song: &mut Song, rng: &mut Rng) {
    for v in [Voice::Bass, Voice::Acid] {
        if song.track(v).lock {
            continue;
        }
        let fresh = track_for(v, song.vibe, song.macros.energy, song, rng);
        let t = song.track_mut(v);
        let mut fresh_notes = fresh.steps.iter().filter(|s| s.on).map(|s| s.note).cycle();
        for s in t.steps.iter_mut().filter(|s| s.on) {
            s.note = fresh_notes.next().unwrap_or(s.note);
        }
    }
}

/// Rotate a pattern by a step or two – same notes, new feel
fn rotate(song: &mut Song, v: Voice, rng: &mut Rng) -> Change {
    let t = song.track_mut(v);
    let len = t.len as usize;
    t.steps[..len].rotate_right(*rng.pick(&[1usize, 2, 3]));
    Change::Shifted(v)
}

/// A new chapter after a drop: one or two bigger changes – a new bassline, a new synth
/// line, another sound, other chords or (rarely) another key – not everything at once
pub fn new_chapter(song: &mut Song, rng: &mut Rng) -> Change {
    let vibe = song.vibe;
    // What can change; a key change is the rarest
    let kinds = [0u8, 1, 1, 2, 2, 3, 3, 4];
    let first = *rng.pick(&kinds);
    let second = rng.chance(0.4).then(|| *rng.pick(&kinds)).filter(|&k| k != first);
    for kind in std::iter::once(first).chain(second) {
        match kind {
            0 => {
                // Up a fourth or down a whole tone – stays in the bass range
                let shifted = song.root as i32 + *rng.pick(&[5, -2, 3, -5]);
                song.root = if (36..=47).contains(&shifted) {
                    shifted as u8
                } else {
                    (shifted - 12).clamp(36, 47) as u8
                };
            }
            1 | 2 => {
                let v = if kind == 1 { Voice::Bass } else { Voice::Acid };
                if song.track(v).lock {
                    continue;
                }
                let fresh = track_for(v, vibe, song.macros.energy, song, rng);
                let t = song.track_mut(v);
                // The synth sometimes comes in with its new line
                let mute = t.mute && !(v == Voice::Acid && rng.chance(0.4));
                *t = Track {
                    mute,
                    volume: t.volume,
                    sound: t.sound,
                    tune: t.tune,
                    decay: t.decay,
                    ..fresh
                };
            }
            3 => {
                let v = *rng.pick(&[Voice::Hat, Voice::Clap, Voice::Bass, Voice::Acid]);
                if !song.track(v).lock {
                    song.track_mut(v).sound = sound_for(v, vibe, rng);
                    if v == Voice::Acid {
                        // A line written for the new sound
                        let fresh = track_for(v, vibe, song.macros.energy, song, rng);
                        let t = song.track_mut(v);
                        t.steps = fresh.steps;
                        t.len = fresh.len;
                    }
                }
            }
            _ => {
                song.prog = rng.below(PROGRESSIONS.len() as u32) as u8;
                refit_notes(song, rng);
            }
        }
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
    fn the_tracks_fit_together() {
        let mut rng = Rng::new(77);
        for vibe in Vibe::ALL {
            for _ in 0..40 {
                let s = generate(vibe, &Macros::default(), &mut rng);
                let prog = progression(&s);
                // The bass plays the chord's root or fifth (in some octave) under every half bar
                for (i, st) in s.track(Voice::Bass).steps.iter().enumerate().filter(|(_, st)| st.on) {
                    let root = s.scale.shift(0, prog[i / 8]).rem_euclid(12);
                    let fifth = s.scale.shift(0, prog[i / 8] + 4).rem_euclid(12);
                    let sixth = s.scale.shift(0, prog[i / 8] - 2).rem_euclid(12);
                    let n = st.note.rem_euclid(12);
                    assert!(n == root || n == fifth || n == sixth, "{vibe:?} bass step {i}: {}", st.note);
                }
                // A 303 line lands on a chord note on every beat
                let synth = s.track(Voice::Acid);
                if synth.sound < 2 {
                    for i in (0..STEPS).step_by(4).filter(|&i| synth.steps[i].on) {
                        let chord = [0, 2, 4].map(|d| s.scale.shift(0, prog[i / 8] + d).rem_euclid(12));
                        assert!(chord.contains(&synth.steps[i].note.rem_euclid(12)), "{vibe:?} synth step {i}");
                    }
                    assert_eq!(synth.len, 16);
                }
                // Rims stay off the kick and the clap
                for i in 0..STEPS {
                    let rim = s.track(Voice::Rim).steps[i].on;
                    assert!(!(rim && (i % 4 == 0 || s.track(Voice::Clap).steps[i].on)), "{vibe:?} rim step {i}");
                }
                // Loops are a full bar or half a bar – always in step with the beat
                for t in &s.tracks {
                    assert!([8, 16].contains(&t.len), "{vibe:?} {}", t.len);
                }
                // Every generated step always plays – "sometimes" is only for you to set (shift-click)
                for t in &s.tracks {
                    assert!(t.steps.iter().all(|st| st.chance == 100), "{vibe:?}");
                }
            }
        }
    }

    #[test]
    fn every_mood_is_its_own_and_its_notes_fit() {
        use crate::song::MOODS;
        let mut rng = Rng::new(9);
        for (i, a) in MOODS.iter().enumerate() {
            for b in &MOODS[i + 1..] {
                assert!((a.2, a.3) != (b.2, b.3), "{} and {} are the same", a.0, b.0);
            }
            let mut s = generate(Vibe::Hypnotic, &Macros::default(), &mut rng);
            s.scale = a.2;
            s.prog = a.3;
            refit_notes(&mut s, &mut rng);
            assert_eq!(s.mood(), Some(i));
            for v in [Voice::Bass, Voice::Acid] {
                for st in s.track(v).steps.iter().filter(|st| st.on) {
                    assert!(s.scale.intervals().contains(&st.note.rem_euclid(12)), "{} {v:?} {}", a.0, st.note);
                }
            }
        }
    }

    #[test]
    fn auto_mode_picks_sounds_of_the_vibe() {
        let mut rng = Rng::new(5);
        for vibe in Vibe::ALL {
            let mut s = generate(vibe, &Macros::default(), &mut rng);
            for _ in 0..300 {
                if let Some(Change::NewSound(v, n)) = evolve(&mut s, &mut rng, true) {
                    assert!(sounds_for(v, vibe).contains(&n), "{vibe:?} {v:?} {n}");
                }
                assert_eq!(s.track(Voice::Acid).len, 16);
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
            assert!(evolve(&mut s, &mut rng, true).is_some());
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
    fn locked_tracks_stay_as_they_are() {
        let mut rng = Rng::new(31);
        let mut s = generate(Vibe::Acid, &Macros::default(), &mut rng);
        for v in [Voice::Bass, Voice::Hat, Voice::Clap] {
            s.track_mut(v).lock = true;
        }
        let before = s;
        for _ in 0..300 {
            evolve(&mut s, &mut rng, true);
            new_chapter(&mut s, &mut rng);
        }
        for v in [Voice::Bass, Voice::Hat, Voice::Clap] {
            assert_eq!(s.track(v), before.track(v), "{v:?}");
        }
        // Everything locked: the auto mode has nothing to do
        for t in s.tracks.iter_mut() {
            t.lock = true;
        }
        assert_eq!(evolve(&mut s, &mut rng, true), None);
        // Shuffle keeps locked tracks
        let mut keep = [None; TRACKS];
        keep[Voice::Bass.index()] = Some(*before.track(Voice::Bass));
        let fresh = generate_keeping(Vibe::Dub, &Macros::default(), &mut rng, &keep);
        assert_eq!(fresh.track(Voice::Bass), before.track(Voice::Bass));
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
