//! All sounds of Bunker, synthesized sample by sample – no recordings.
//!
//! Drums follow the classic analog recipes (808/909): a sine with a falling
//! pitch for the kick, filtered noise bursts for the clap, six detuned square
//! waves for the metallic hats and ride. Bass and acid are a saw wave through a
//! resonant filter with an envelope, the acid one 303-style with accent and slide.

use crate::rng::Rng;
use std::f32::consts::{PI, TAU};

pub fn midi_hz(note: f32) -> f32 {
    440.0 * 2f32.powf((note - 69.0) / 12.0)
}

/// Exponential decay factor per sample for a time constant in seconds
fn decay_coef(seconds: f32, sr: f32) -> f32 {
    (-1.0 / (seconds.max(1e-4) * sr)).exp()
}

/// Soft clipper
pub fn sat(x: f32) -> f32 {
    x.tanh()
}

/// State-variable filter (Simper/Cytomic TPT): stable at any cutoff and resonance
#[derive(Clone, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
    k: f32,
}

pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    pub fn new(cutoff: f32, q: f32, sr: f32) -> Self {
        let mut f = Svf::default();
        f.set(cutoff, q, sr);
        f
    }

    pub fn set(&mut self, cutoff: f32, q: f32, sr: f32) {
        let fc = cutoff.clamp(10.0, sr * 0.45);
        let g = (PI * fc / sr).tan();
        self.k = 1.0 / q.max(0.05);
        self.a1 = 1.0 / (1.0 + g * (g + self.k));
        self.a2 = g * self.a1;
        self.a3 = g * self.a2;
    }

    pub fn run(&mut self, x: f32) -> SvfOut {
        let v3 = x - self.ic2;
        let v1 = self.a1 * self.ic1 + self.a2 * v3;
        let v2 = self.ic2 + self.a2 * self.ic1 + self.a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        // Guard against denormals / blow-ups from extreme settings
        if !self.ic1.is_finite() || !self.ic2.is_finite() {
            self.ic1 = 0.0;
            self.ic2 = 0.0;
        }
        SvfOut {
            lp: v2,
            bp: v1,
            hp: x - self.k * v1 - v2,
        }
    }
}

/// Band-limited saw (PolyBLEP) – less harsh aliasing in the high notes
#[derive(Clone, Default)]
struct Saw {
    phase: f32,
}

impl Saw {
    fn next(&mut self, hz: f32, sr: f32) -> f32 {
        let dt = (hz / sr).min(0.5);
        let mut y = 2.0 * self.phase - 1.0;
        let t = self.phase;
        if t < dt {
            let t = t / dt;
            y -= t + t - t * t - 1.0;
        } else if t > 1.0 - dt {
            let t = (t - 1.0) / dt;
            y -= t * t + t + t + 1.0;
        }
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        y
    }
}

// ───────────────────────────── Drums ─────────────────────────────

#[derive(Clone)]
pub struct Kick {
    phase: f32,
    t: f32,
    amp: f32,
    env: f32,
    decay: f32,
    click: f32,
    pub tune: f32,
    pub drive: f32,
}

impl Kick {
    pub fn new() -> Self {
        Kick {
            phase: 0.0,
            t: 10.0,
            amp: 0.0,
            env: 0.0,
            decay: 1.0,
            click: 0.0,
            tune: 48.0,
            drive: 1.6,
        }
    }

    /// `length` in seconds (0.2 … 0.7)
    pub fn trigger(&mut self, vel: f32, length: f32, sr: f32) {
        self.t = 0.0;
        self.phase = 0.0;
        self.amp = vel;
        self.env = 1.0;
        self.decay = decay_coef(length, sr);
        self.click = 1.0;
    }

    /// Envelope of the last hit – used to duck the bass and the rumble
    pub fn envelope(&self) -> f32 {
        self.env
    }

    pub fn next(&mut self, sr: f32, rng: &mut Rng) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        let t = self.t / sr;
        let hz = self.tune + self.tune * 2.8 * (-t / 0.026).exp() + 40.0 * (-t / 0.004).exp();
        self.phase = (self.phase + hz / sr) % 1.0;
        let attack = (t / 0.0015).min(1.0);
        let body = (self.phase * TAU).sin() * self.env * attack;
        let click = rng.noise() * self.click * 0.35;
        self.click *= 0.985;
        self.env *= self.decay;
        self.t += 1.0;
        sat((body + click) * self.drive) / sat(self.drive) * self.amp
    }
}

#[derive(Clone)]
pub struct Clap {
    t: f32,
    amp: f32,
    bp: Svf,
    tail: f32,
    tail_decay: f32,
    burst_decay: f32,
}

impl Clap {
    pub fn new(sr: f32) -> Self {
        Clap {
            t: 1e9,
            amp: 0.0,
            bp: Svf::new(1150.0, 1.3, sr),
            tail: 0.0,
            tail_decay: decay_coef(0.12, sr),
            burst_decay: 0.0,
        }
    }

    pub fn trigger(&mut self, vel: f32, length: f32, sr: f32) {
        self.t = 0.0;
        self.amp = vel;
        self.tail = 1.0;
        self.tail_decay = decay_coef(length, sr);
        self.burst_decay = decay_coef(0.006, sr);
    }

    pub fn next(&mut self, sr: f32, rng: &mut Rng) -> f32 {
        let ms = self.t / sr * 1000.0;
        if ms > 1500.0 {
            return 0.0;
        }
        // Three quick bursts (hands not clapping exactly together), then the tail
        let env = if ms < 27.0 {
            let local = (ms % 9.0) / 1000.0 * sr;
            self.burst_decay.powf(local)
        } else {
            self.tail *= self.tail_decay;
            self.tail * 0.8
        };
        self.t += 1.0;
        let n = rng.noise();
        self.bp.run(n).bp * env * self.amp * 2.6
    }
}

#[derive(Clone)]
pub struct Rim {
    t: f32,
    amp: f32,
    hp: Svf,
    pitch: f32,
}

impl Rim {
    pub fn new(sr: f32) -> Self {
        Rim {
            t: 1e9,
            amp: 0.0,
            hp: Svf::new(350.0, 0.7, sr),
            pitch: 1.0,
        }
    }

    pub fn trigger(&mut self, vel: f32, pitch: f32) {
        self.t = 0.0;
        self.amp = vel;
        self.pitch = pitch;
    }

    pub fn next(&mut self, sr: f32, rng: &mut Rng) -> f32 {
        let t = self.t / sr;
        if t > 0.25 {
            return 0.0;
        }
        self.t += 1.0;
        let tone = (TAU * 1720.0 * self.pitch * t).sin() * 0.55 + (TAU * 495.0 * self.pitch * t).sin() * 0.7;
        let x = tone * (-t / 0.024).exp() + rng.noise() * (-t / 0.003).exp() * 0.6;
        self.hp.run(x).hp * self.amp
    }
}

/// Six square waves at inharmonic ratios – the 808's metallic source
#[derive(Clone)]
struct Metal {
    phases: [f32; 6],
    freqs: [f32; 6],
}

impl Metal {
    fn new(scale: f32) -> Self {
        Metal {
            phases: [0.0, 0.13, 0.31, 0.57, 0.71, 0.89],
            freqs: [205.3, 304.4, 369.6, 522.7, 540.0, 800.0].map(|f| f * scale),
        }
    }

    fn next(&mut self, sr: f32) -> f32 {
        let mut s = 0.0;
        for (p, f) in self.phases.iter_mut().zip(self.freqs) {
            *p = (*p + f / sr) % 1.0;
            s += if *p < 0.5 { 1.0 } else { -1.0 };
        }
        s / 6.0
    }
}

#[derive(Clone)]
pub struct Hat {
    metal: Metal,
    bp: Svf,
    hp: Svf,
    env: f32,
    decay: f32,
    amp: f32,
    noise_mix: f32,
}

impl Hat {
    /// `open`: longer and a bit darker; `ride`: other ratios, lower band
    pub fn new(sr: f32, kind: HatKind) -> Self {
        let (scale, band, hp, noise) = match kind {
            HatKind::Closed => (1.0, 10_000.0, 7_000.0, 0.45),
            HatKind::Open => (1.0, 9_000.0, 6_500.0, 0.4),
            HatKind::Ride => (1.37, 6_200.0, 3_800.0, 0.15),
        };
        Hat {
            metal: Metal::new(scale),
            bp: Svf::new(band, 1.1, sr),
            hp: Svf::new(hp, 0.7, sr),
            env: 0.0,
            decay: 0.0,
            amp: 0.0,
            noise_mix: noise,
        }
    }

    pub fn trigger(&mut self, vel: f32, length: f32, sr: f32) {
        self.env = 1.0;
        self.amp = vel;
        self.decay = decay_coef(length, sr);
    }

    /// An open hat is cut off by the closed one
    pub fn choke(&mut self, sr: f32) {
        self.decay = self.decay.min(decay_coef(0.01, sr));
    }

    pub fn next(&mut self, sr: f32, rng: &mut Rng) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        let src = self.metal.next(sr) * (1.0 - self.noise_mix) + rng.noise() * self.noise_mix;
        let x = self.bp.run(src).bp;
        let y = self.hp.run(x).hp;
        self.env *= self.decay;
        y * self.env * self.amp * 2.2
    }
}

#[derive(Clone, Copy)]
pub enum HatKind {
    Closed,
    Open,
    Ride,
}

// ───────────────────────────── Synths ─────────────────────────────

/// Short, punchy filtered saw for the rolling techno bass
#[derive(Clone)]
pub struct Bass {
    saw: Saw,
    sub: f32,
    lp: Svf,
    hz: f32,
    amp: f32,
    env: f32,
    fenv: f32,
    gate: f32,
    adecay: f32,
    fdecay: f32,
}

impl Bass {
    pub fn new(sr: f32) -> Self {
        Bass {
            saw: Saw::default(),
            sub: 0.0,
            lp: Svf::new(300.0, 0.9, sr),
            hz: 55.0,
            amp: 0.0,
            env: 0.0,
            fenv: 0.0,
            gate: 0.0,
            adecay: 0.0,
            fdecay: 0.0,
        }
    }

    /// `gate` in samples
    pub fn trigger(&mut self, note: f32, vel: f32, gate: f32, sr: f32) {
        self.hz = midi_hz(note);
        self.amp = vel;
        self.env = 1.0;
        self.fenv = 1.0;
        self.gate = gate;
        self.adecay = decay_coef(0.22, sr);
        self.fdecay = decay_coef(0.055, sr);
    }

    /// `bright` 0…1 (energy), `dark` 0…1 lowers the filter
    pub fn next(&mut self, sr: f32, bright: f32, dark: f32) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        if self.gate > 0.0 {
            self.gate -= 1.0;
            self.env *= self.adecay;
        } else {
            self.env *= 0.995; // quick release
        }
        self.fenv *= self.fdecay;
        let cutoff = 90.0 + (1.0 - dark) * 160.0 + self.fenv * (350.0 + bright * 1100.0);
        self.lp.set(cutoff, 1.4, sr);
        self.sub = (self.sub + self.hz / sr) % 1.0;
        let x = self.saw.next(self.hz, sr) * 0.8 + (self.sub * TAU).sin() * 0.5;
        sat(self.lp.run(x).lp * 1.8) * self.env * self.amp
    }
}

/// 303-style acid voice: saw → resonant low-pass with envelope, accent and slide
#[derive(Clone)]
pub struct Acid {
    saw: Saw,
    lp: Svf,
    lp2: Svf,
    hz: f32,
    target_hz: f32,
    glide: f32,
    amp: f32,
    env: f32,
    fenv: f32,
    fdecay: f32,
    gate: f32,
    accent: f32,
}

impl Acid {
    pub fn new(sr: f32) -> Self {
        Acid {
            saw: Saw::default(),
            lp: Svf::new(800.0, 4.0, sr),
            lp2: Svf::new(800.0, 0.7, sr),
            hz: 110.0,
            target_hz: 110.0,
            glide: 0.0,
            amp: 0.0,
            env: 0.0,
            fenv: 0.0,
            fdecay: 0.0,
            gate: 0.0,
            accent: 0.0,
        }
    }

    pub fn trigger(&mut self, note: f32, vel: f32, accent: bool, gate: f32, sr: f32) {
        self.hz = midi_hz(note);
        self.target_hz = self.hz;
        self.amp = vel;
        self.env = 1.0;
        self.fenv = 1.0;
        self.accent = if accent { 1.0 } else { 0.0 };
        self.fdecay = decay_coef(if accent { 0.09 } else { 0.2 }, sr);
        self.gate = gate;
    }

    /// Slide: change pitch smoothly, keep the envelopes running
    pub fn slide(&mut self, note: f32, accent: bool, gate: f32, sr: f32) {
        self.target_hz = midi_hz(note);
        self.glide = decay_coef(0.045, sr);
        self.gate = gate;
        if accent {
            self.accent = 1.0;
            self.fenv = self.fenv.max(0.7);
        }
        if self.env < 0.05 {
            self.trigger(note, self.amp.max(0.7), accent, gate, sr);
        }
    }

    /// `acid` 0…1: filter opening and resonance
    pub fn next(&mut self, sr: f32, acid: f32) -> f32 {
        if self.env < 1e-4 {
            return 0.0;
        }
        self.hz = self.target_hz + (self.hz - self.target_hz) * self.glide;
        if self.gate > 0.0 {
            self.gate -= 1.0;
        } else {
            self.env *= 0.993;
        }
        self.fenv *= self.fdecay;
        let a = acid.clamp(0.0, 1.0);
        let base = 140.0 + a * a * 1800.0;
        let depth = (500.0 + a * 3600.0) * (1.0 + self.accent * 0.8);
        let cutoff = base + depth * self.fenv;
        let q = 2.5 + a * 9.0 + self.accent * 2.0;
        self.lp.set(cutoff, q, sr);
        self.lp2.set(cutoff * 1.6, 0.6, sr);
        let x = self.saw.next(self.hz, sr);
        let y = self.lp2.run(self.lp.run(x).lp).lp;
        let level = self.amp * (0.75 + self.accent * 0.35);
        sat(y * (1.6 + a * 1.6)) * self.env * level * 0.8
    }
}

/// A long low "boom" for the drop
#[derive(Clone, Default)]
pub struct Boom {
    phase: f32,
    t: f32,
    on: bool,
}

impl Boom {
    pub fn trigger(&mut self) {
        self.t = 0.0;
        self.phase = 0.0;
        self.on = true;
    }

    pub fn next(&mut self, sr: f32) -> f32 {
        if !self.on {
            return 0.0;
        }
        let t = self.t / sr;
        if t > 3.0 {
            self.on = false;
            return 0.0;
        }
        self.t += 1.0;
        let hz = 34.0 + 60.0 * (-t / 0.08).exp();
        self.phase = (self.phase + hz / sr) % 1.0;
        (self.phase * TAU).sin() * (-t / 0.9).exp() * (t / 0.002).min(1.0) * 0.9
    }
}

// ───────────────────────────── Effects ─────────────────────────────

/// Stereo ping-pong delay with a filtered feedback path
pub struct Delay {
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
    pos: usize,
    tone_l: Svf,
    tone_r: Svf,
}

impl Delay {
    pub fn new(sr: f32) -> Self {
        let n = (sr * 2.0) as usize;
        Delay {
            buf_l: vec![0.0; n],
            buf_r: vec![0.0; n],
            pos: 0,
            tone_l: Svf::new(2800.0, 0.6, sr),
            tone_r: Svf::new(2800.0, 0.6, sr),
        }
    }

    /// `time` in samples; input mono-ish (l, r); returns the wet signal
    pub fn run(&mut self, l: f32, r: f32, time: f32, feedback: f32) -> (f32, f32) {
        let n = self.buf_l.len();
        let d = (time as usize).clamp(1, n - 1);
        let read = (self.pos + n - d) % n;
        let (dl, dr) = (self.buf_l[read], self.buf_r[read]);
        let fl = self.tone_l.run(dr).lp;
        let fr = self.tone_r.run(dl).lp;
        // Ping-pong: left feeds right and vice versa
        self.buf_l[self.pos] = l + fl * feedback;
        self.buf_r[self.pos] = r * 0.3 + fr * feedback;
        self.pos = (self.pos + 1) % n;
        (dl, dr)
    }
}

/// A compact Freeverb: parallel combs into series all-passes, per channel
pub struct Reverb {
    combs: Vec<(Vec<f32>, usize, f32)>,
    allpasses: Vec<(Vec<f32>, usize)>,
}

impl Reverb {
    pub fn new(sr: f32) -> Self {
        let scale = sr / 44_100.0;
        let mut combs = Vec::new();
        for (i, &len) in [1116usize, 1188, 1277, 1356, 1422, 1491, 1557, 1617].iter().enumerate() {
            // Right channel uses slightly longer lines for width
            for ch in 0..2 {
                let l = ((len + ch * 23 + i) as f32 * scale) as usize;
                combs.push((vec![0.0; l], 0, 0.0));
            }
        }
        let mut allpasses = Vec::new();
        for &len in &[556usize, 441, 341, 225] {
            for ch in 0..2 {
                allpasses.push((vec![0.0; ((len + ch * 23) as f32 * scale) as usize], 0));
            }
        }
        Reverb { combs, allpasses }
    }

    /// `size` 0…1, `damp` 0…1
    pub fn run(&mut self, input: f32, size: f32, damp: f32) -> (f32, f32) {
        let fb = 0.7 + size * 0.28;
        let mut out = [0.0f32; 2];
        for (i, (buf, pos, store)) in self.combs.iter_mut().enumerate() {
            let y = buf[*pos];
            *store = y * (1.0 - damp) + *store * damp;
            buf[*pos] = input + *store * fb;
            *pos = (*pos + 1) % buf.len();
            out[i % 2] += y;
        }
        for (i, (buf, pos)) in self.allpasses.iter_mut().enumerate() {
            let ch = i % 2;
            let b = buf[*pos];
            buf[*pos] = out[ch] + b * 0.5;
            out[ch] = b - out[ch];
            *pos = (*pos + 1) % buf.len();
        }
        (out[0] * 0.06, out[1] * 0.06)
    }
}

/// The techno rumble: the kick smeared through dark resonating delays,
/// ducked whenever the kick hits
pub struct Rumble {
    lines: Vec<(Vec<f32>, usize)>,
    lp: Svf,
    out_lp: Svf,
}

impl Rumble {
    pub fn new(sr: f32) -> Self {
        let lines = [0.027f32, 0.039, 0.053, 0.071]
            .iter()
            .map(|s| (vec![0.0; (s * sr) as usize], 0))
            .collect();
        Rumble {
            lines,
            lp: Svf::new(160.0, 0.7, sr),
            out_lp: Svf::new(110.0, 0.8, sr),
        }
    }

    pub fn run(&mut self, kick: f32, duck: f32) -> f32 {
        let x = self.lp.run(kick).lp;
        let mut sum = 0.0;
        for (buf, pos) in self.lines.iter_mut() {
            let y = buf[*pos];
            buf[*pos] = x + y * 0.8;
            *pos = (*pos + 1) % buf.len();
            sum += y;
        }
        let d = (1.0 - duck * 1.5).max(0.0);
        self.out_lp.run(sat(sum * 0.5)).lp * d * d
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f32 = 44_100.0;

    fn peak_and_finite(mut f: impl FnMut() -> f32, n: usize) -> f32 {
        let mut peak = 0.0f32;
        for _ in 0..n {
            let x = f();
            assert!(x.is_finite());
            peak = peak.max(x.abs());
        }
        peak
    }

    #[test]
    fn voices_make_sound_and_stay_bounded() {
        let mut rng = Rng::new(1);
        let mut kick = Kick::new();
        kick.trigger(1.0, 0.5, SR);
        let p = peak_and_finite(|| kick.next(SR, &mut rng), 20_000);
        assert!(p > 0.5 && p <= 1.05, "kick {p}");

        let mut clap = Clap::new(SR);
        clap.trigger(1.0, 0.15, SR);
        let p = peak_and_finite(|| clap.next(SR, &mut rng), 20_000);
        assert!(p > 0.1 && p < 3.0, "clap {p}");

        for kind in [HatKind::Closed, HatKind::Open, HatKind::Ride] {
            let mut hat = Hat::new(SR, kind);
            hat.trigger(1.0, 0.2, SR);
            let p = peak_and_finite(|| hat.next(SR, &mut rng), 20_000);
            assert!(p > 0.05 && p < 3.0, "hat {p}");
        }

        let mut bass = Bass::new(SR);
        bass.trigger(41.0, 1.0, 4000.0, SR);
        let p = peak_and_finite(|| bass.next(SR, 0.5, 0.5), 20_000);
        assert!(p > 0.2 && p <= 1.0, "bass {p}");

        let mut acid = Acid::new(SR);
        acid.trigger(53.0, 1.0, true, 4000.0, SR);
        let p = peak_and_finite(|| acid.next(SR, 1.0), 20_000);
        assert!(p > 0.2 && p <= 1.2, "acid {p}");
    }

    #[test]
    fn voices_fall_silent() {
        let mut rng = Rng::new(2);
        let mut kick = Kick::new();
        kick.trigger(1.0, 0.3, SR);
        for _ in 0..(SR as usize * 4) {
            kick.next(SR, &mut rng);
        }
        assert_eq!(kick.next(SR, &mut rng), 0.0);
    }

    #[test]
    fn svf_lowpass_passes_dc_and_blocks_highs() {
        let mut f = Svf::new(200.0, 0.7, SR);
        let mut y = 0.0;
        for _ in 0..10_000 {
            y = f.run(1.0).lp;
        }
        assert!((y - 1.0).abs() < 0.01);
        let mut f = Svf::new(200.0, 0.7, SR);
        let mut peak = 0.0f32;
        for i in 0..10_000 {
            let x = if i % 2 == 0 { 1.0 } else { -1.0 }; // Nyquist
            peak = peak.max(f.run(x).lp.abs());
        }
        assert!(peak < 0.01);
    }
}
