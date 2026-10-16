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

/// A whole new groove
pub fn generate(vibe: Vibe, m: &Macros, rng: &mut Rng) -> Song {
    let mut song = Song::empty();
    song.vibe = vibe;
    song.macros = *m;
    let (lo, hi) = vibe.bpm();
    song.bpm = (lo + rng.f32() * (hi - lo)).round();
    song.root = *rng.pick(&[36u8, 38, 39, 40, 41, 43]); // C2 … G2
    song.scale = match vibe {
        Vibe::Acid | Vibe::Industrial => *rng.pick(&[Scale::Phrygian, Scale::Minor]),
        Vibe::Minimal => *rng.pick(&[Scale::Dorian, Scale::Minor]),
        _ => *rng.pick(&[Scale::Minor, Scale::Minor, Scale::Phrygian, Scale::Dorian]),
    };
    song.swing = match vibe {
        Vibe::Minimal => 0.15 + rng.f32() * 0.2,
        Vibe::Hypnotic => rng.f32() * 0.12,
        _ => 0.0,
    };
    for v in Voice::ALL {
        let vol = song.track(v).volume;
        *song.track_mut(v) = track_for(v, vibe, m.energy, &song, rng);
        song.track_mut(v).volume = vol;
    }
    song
}

/// A fresh pattern for one track that fits the vibe
pub fn track_for(v: Voice, vibe: Vibe, energy: f32, song: &Song, rng: &mut Rng) -> Track {
    let e = energy.clamp(0.0, 1.0);
    let mut t = match v {
        Voice::Kick => kick(vibe, rng),
        Voice::Clap => clap(vibe, e, rng),
        Voice::Rim => rim(vibe, e, rng),
        Voice::Hat => hat(vibe, e, rng),
        Voice::OpenHat => open_hat(vibe, e, rng),
        Voice::Ride => ride(vibe, e, rng),
        Voice::Bass => bass(vibe, e, song.scale, rng),
        Voice::Acid => acid(vibe, e, song.scale, rng),
    };
    t.volume = song.track(v).volume;
    t.mute = match v {
        // The acid line only screams in acid tracks – elsewhere it waits muted for the auto mode
        Voice::Acid => !matches!(vibe, Vibe::Acid) && !rng.chance(0.2),
        Voice::Ride => matches!(vibe, Vibe::Minimal) && rng.chance(0.6),
        _ => false,
    };
    t
}

fn kick(vibe: Vibe, rng: &mut Rng) -> Track {
    let mut t = from_positions(&[0, 4, 8, 12]);
    for i in [0, 4, 8, 12] {
        t.steps[i].accent = true;
    }
    match vibe {
        Vibe::Industrial => {
            if rng.chance(0.6) {
                t.steps[14] = Step::hit();
            }
            if rng.chance(0.4) {
                t.steps[15] = Step { chance: 60, ..Step::hit() };
            }
        }
        Vibe::Minimal | Vibe::Hypnotic => {
            if rng.chance(0.3) {
                t.steps[*rng.pick(&[7usize, 10, 15])] = Step { chance: 70, ..Step::hit() };
            }
        }
        _ => {
            if rng.chance(0.15) {
                t.steps[15] = Step { chance: 50, ..Step::hit() };
            }
        }
    }
    t
}

fn clap(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let mut t = match vibe {
        Vibe::Minimal => from_positions(if rng.chance(0.5) { &[12] } else { &[4, 12] }),
        Vibe::Hypnotic => {
            if rng.chance(0.35) {
                Track::default()
            } else {
                from_positions(&[4, 12])
            }
        }
        _ => from_positions(&[4, 12]),
    };
    if e > 0.6 && rng.chance(e - 0.4) {
        t.steps[*rng.pick(&[7usize, 14, 15])] = Step { chance: 70, ..Step::hit() };
    }
    if matches!(vibe, Vibe::Industrial) && rng.chance(0.5) {
        t.steps[13] = Step { chance: 50, ..Step::hit() };
    }
    t
}

fn rim(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let k = match vibe {
        Vibe::Minimal => rng.range(3, 6),
        Vibe::Industrial => rng.range(4, 7),
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
    if matches!(vibe, Vibe::Hypnotic | Vibe::Minimal) && rng.chance(0.4) {
        t.len = *rng.pick(&[12u8, 14, 15]);
    }
    t
}

fn hat(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let density = e + rng.f32() * 0.3 - 0.15;
    let mut t = if density > 0.55 || matches!(vibe, Vibe::Industrial | Vibe::Peak) {
        // 16ths, accents on the off-beats
        let mut t = from_positions(&(0..16).collect::<Vec<_>>());
        for (i, s) in t.steps.iter_mut().enumerate() {
            s.accent = i % 4 == 2;
            if i % 2 == 1 && rng.chance(0.3 * (1.0 - e)) {
                s.on = false;
            }
        }
        t
    } else {
        from_positions(&[2, 6, 10, 14])
    };
    if matches!(vibe, Vibe::Minimal) && rng.chance(0.5) {
        t = from_mask(euclid(rng.range(5, 9) as usize, STEPS, 2));
    }
    t
}

fn open_hat(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    let mut t = from_positions(&[2, 6, 10, 14]);
    if matches!(vibe, Vibe::Minimal) || (e < 0.35 && rng.chance(0.5)) {
        t = from_positions(&[*rng.pick(&[6usize, 14])]);
    }
    if e > 0.75 && rng.chance(0.3) {
        t.steps[15] = Step { chance: 50, ..Step::hit() };
    }
    t
}

fn ride(vibe: Vibe, e: f32, rng: &mut Rng) -> Track {
    match vibe {
        Vibe::Peak | Vibe::Industrial => {
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

fn bass(vibe: Vibe, e: f32, scale: Scale, rng: &mut Rng) -> Track {
    // Rolling bass lives between the kicks
    let template: &[usize] = match vibe {
        Vibe::Hypnotic | Vibe::Industrial => {
            if rng.chance(0.6) {
                &[1, 2, 3, 5, 6, 7, 9, 10, 11, 13, 14, 15]
            } else {
                &[2, 3, 6, 7, 10, 11, 14, 15]
            }
        }
        Vibe::Peak => rng.pick(&[&[2usize, 6, 10, 14][..], &[2, 3, 6, 7, 10, 11, 14, 15][..]]),
        Vibe::Minimal => rng.pick(&[&[3usize, 6, 11, 14][..], &[2, 7, 10, 15][..]]),
        Vibe::Acid => &[2, 6, 10, 14],
    };
    let mut t = from_positions(template);
    let fifth = scale.shift(0, 4);
    for (i, s) in t.steps.iter_mut().enumerate() {
        if !s.on {
            continue;
        }
        s.note = 0;
        // Mostly the root; a few octave jumps and fifths for movement
        if rng.chance(0.12 + e * 0.1) {
            s.note = 12;
        } else if rng.chance(0.08) {
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
    Breakdown,
    Build,
    Drop,
}

impl Change {
    pub fn describe(self) -> String {
        match self {
            Change::NewPattern(v) => format!("New {} pattern", v.name().to_lowercase()),
            Change::Muted(v) => format!("{} out", v.name()),
            Change::Unmuted(v) => format!("{} in", v.name()),
            Change::Shifted(v) => format!("{} shifted", v.name()),
            Change::Breakdown => "Breakdown".into(),
            Change::Build => "Build-up".into(),
            Change::Drop => "Drop!".into(),
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
    if roll < 0.45 {
        let fresh = track_for(v, song.vibe, m.energy, song, rng);
        let t = song.track_mut(v);
        let mute = t.mute;
        *t = Track {
            mute,
            volume: t.volume,
            ..fresh
        };
        Change::NewPattern(v)
    } else if roll < 0.75 {
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
    } else {
        // Rotate the pattern by a step or two – same notes, new feel
        let t = song.track_mut(v);
        let len = t.len as usize;
        t.steps[..len].rotate_right(*rng.pick(&[1usize, 2, 3]));
        Change::Shifted(v)
    }
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
                // Four to the floor, always
                for i in [0, 4, 8, 12] {
                    assert!(kick.steps[i].on, "{vibe:?}");
                }
                let (lo, hi) = vibe.bpm();
                assert!(s.bpm >= lo && s.bpm <= hi);
                // The bass never doubles the kick on the beat
                for i in [0, 4, 8, 12] {
                    assert!(!s.track(Voice::Bass).steps[i].on);
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
    }
}
