//! The log under the mood: what Bunker says (every caption), where the auto mode is and when
//! the drop comes, and now and then a line from the bunker – typed in like on an old terminal

use crate::engine::{Shared, NO_BREAK};
use crate::rng::Rng;
use crate::song::Song;
use crate::theme::*;
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Rect, Sense, Ui};
use std::collections::VecDeque;
use std::sync::atomic::Ordering;

/// Characters typed per second
const TYPE_SPEED: f64 = 48.0;
const KEEP: usize = 40;

/// Tips, one now and then while the music plays (in turn with what's going on in the set)
const TIPS: [&str; 14] = [
    "Tip: click a track's name to hear its sound",
    "Tip: drag a bass or synth planet up or down for another note",
    "Tip: right-click a planet – accent, slide, how often it plays",
    "Tip: lock a track and shuffle and the auto mode leave it alone",
    "Tip: B builds up for two bars and drops on the next",
    "Tip: hold WASH (E) through the last bar before a drop",
    "Tip: hold ECHO (W) on the last clap of a phrase",
    "Tip: R records everything you hear – save it as a WAV",
    "Tip: keep a groove in a favorite – it comes in on the next bar",
    "Tip: AUTO SONG sits in the arrow next to record",
    "Tip: scroll on a sound to flip through the sounds",
    "Tip: S solos a track – click again to bring the rest back",
    "Tip: double-click a knob to reset it",
    "Tip: the spring-back filter snaps home when you let go",
];

/// What the auto mode is doing, read from `Shared`
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Where {
    /// Bar within the cycle and its length
    pub bar: u64,
    pub len: u64,
    /// Bar of the breakdown in the cycle (None: a roller without one)
    pub brk: Option<u64>,
    pub heat: f32,
}

impl Where {
    pub fn read(shared: &Shared, bar_now: u64) -> Self {
        let start = shared.cycle_start.load(Ordering::Relaxed);
        let brk = shared.cycle_break.load(Ordering::Relaxed);
        Where {
            bar: bar_now.saturating_sub(start),
            len: shared.cycle_len.load(Ordering::Relaxed).max(1) as u64,
            brk: (brk != NO_BREAK).then_some(brk as u64),
            heat: shared.heat.load(Ordering::Relaxed) as f32 / 1000.0,
        }
    }

    /// Bars until the drop lands
    pub fn bars_to_drop(&self) -> u64 {
        self.len.saturating_sub(self.bar)
    }

    pub fn phase(&self) -> &'static str {
        if self.bar + 2 >= self.len {
            "BUILD-UP"
        } else if self.brk.is_some_and(|b| self.bar >= b) {
            "BREAKDOWN"
        } else if self.brk.is_none() {
            "ROLLER"
        } else {
            "GROOVE"
        }
    }

    fn heat_word(&self) -> &'static str {
        match self.heat {
            h if h >= 0.9 => "peak",
            h if h >= 0.7 => "hot",
            h if h >= 0.5 => "warm",
            _ => "cool",
        }
    }
}

/// What plays, as the log's status line sees it
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Stopped,
    /// A song (song mode) – it says what it does itself
    Song,
    /// The auto mode with AUTO FX: it arranges, so there's a drop to count down to
    Auto(Where),
    /// The auto mode without AUTO FX, or no auto mode: just the bar
    Bar(u64),
}

pub struct Log {
    /// Text and when it came
    lines: VecDeque<(String, f64)>,
    last_bar: Option<u64>,
    /// When the next fact or tip comes, and which kind it is (a fact about the set, the
    /// groove, or a tip – in turn)
    next_line: f64,
    kind: u32,
    last_tip: usize,
    /// Drops so far and seconds played, for the facts
    drops: u32,
    played: f64,
    last_tick: Option<f64>,
    rng: Rng,
}

impl Log {
    pub fn new() -> Self {
        Log {
            lines: VecDeque::new(),
            last_bar: None,
            next_line: 30.0,
            kind: 0,
            last_tip: usize::MAX,
            drops: 0,
            played: 0.0,
            last_tick: None,
            rng: Rng::new(1984),
        }
    }

    pub fn push(&mut self, text: impl Into<String>, now: f64) {
        let text = text.into();
        // The same line twice in a row is once
        if self.lines.back().is_some_and(|(t, _)| *t == text) {
            return;
        }
        self.lines.push_back((text, now));
        while self.lines.len() > KEEP {
            self.lines.pop_front();
        }
    }

    #[cfg(test)]
    pub fn lines(&self) -> impl Iterator<Item = &str> {
        self.lines.iter().map(|(t, _)| t.as_str())
    }

    /// A drop happened (for the facts)
    pub fn note_drop(&mut self) {
        self.drops += 1;
    }

    /// Once per frame: countdowns on new bars, and now and then a fact about the set or a tip
    pub fn tick(&mut self, state: State, now: f64, song: &Song) {
        let bar = match state {
            State::Auto(w) => Some(w.bar),
            State::Bar(b) => Some(b),
            _ => None,
        };
        if bar != self.last_bar {
            self.last_bar = bar;
            if let State::Auto(w) = state {
                self.countdown(w, now);
            }
        }
        let dt = self.last_tick.map_or(0.0, |t| (now - t).clamp(0.0, 1.0));
        self.last_tick = Some(now);
        if state == State::Stopped {
            self.next_line = self.next_line.max(now + 20.0);
            return;
        }
        self.played += dt;
        if now >= self.next_line {
            self.next_line = now + 30.0 + self.rng.f32() as f64 * 20.0;
            let line = self.next_fact(song);
            self.push(line, now);
        }
    }

    fn next_fact(&mut self, song: &Song) -> String {
        self.kind += 1;
        match self.kind % 3 {
            0 => {
                let mins = (self.played / 60.0) as u32;
                let time = if mins == 0 {
                    "Under a minute in".to_string()
                } else {
                    format!("{mins} min in")
                };
                match self.drops {
                    0 => format!("{time} · no drop yet"),
                    1 => format!("{time} · 1 drop"),
                    n => format!("{time} · {n} drops"),
                }
            }
            1 => {
                let mood = song.mood().map_or("", |k| crate::song::MOODS[k].0);
                format!("{} · {} · {mood} · {:.0} BPM", song.vibe.name(), song.root_name(), song.bpm)
            }
            _ => {
                let mut i = self.rng.below(TIPS.len() as u32) as usize;
                if i == self.last_tip {
                    i = (i + 1) % TIPS.len();
                }
                self.last_tip = i;
                TIPS[i].to_string()
            }
        }
    }

    fn countdown(&mut self, w: Where, now: f64) {
        if w.bar == 0 {
            self.push(format!("New cycle · {} bars · the set runs {}", w.len, w.heat_word()), now);
            if w.brk.is_none() {
                self.push("No breakdown this time – it just rolls on", now);
            }
        }
        if let Some(b) = w.brk.filter(|&b| b > w.bar) {
            let n = b - w.bar;
            if n == 8 || n == 4 {
                self.push(format!("Breakdown in {n} bars"), now);
            }
        }
        let n = w.bars_to_drop();
        if matches!(n, 16 | 8) && w.brk.is_none_or(|b| w.bar >= b || n == 16) {
            self.push(format!("Drop in {n} bars"), now);
        }
    }

    /// The log: a status line, then the lines, the newest at the bottom being typed
    pub fn ui(&self, ui: &mut Ui, state: State, energy: Option<f32>, now: f64) {
        let w = ui.available_width();
        let h = ui.available_height().max(70.0);
        let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
        let p = ui.painter_at(rect);
        // A little darker than the concrete around it, with a faint picture noise
        p.rect_filled(rect, 10.0, Color32::from_black_alpha(70));
        p.rect_stroke(rect, 10.0, stroke(1.0, mix(PANEL, ACID, 0.25)), egui::StrokeKind::Inside);
        let frame = (now * 14.0) as u32;
        for i in 0..(rect.area() / 90.0) as u32 {
            let h = |k: u32| {
                let mut x = i.wrapping_mul(0x9E37_79B9) ^ frame.wrapping_mul(0x85EB_CA6B) ^ k.wrapping_mul(0xC2B2_AE35);
                x ^= x >> 15;
                x = x.wrapping_mul(0x2C1B_3C6D);
                x ^= x >> 12;
                (x & 0xFFFF) as f32 / 65535.0
            };
            let at = pos2(
                rect.left() + 2.0 + h(1) * (rect.width() - 4.0),
                rect.top() + 2.0 + h(2) * (rect.height() - 4.0),
            );
            // Exact premultiplied greys: a speck of light, never a wash
            let v = (4.0 + h(3) * 10.0) as u8;
            p.rect_filled(
                Rect::from_min_size(at, vec2(1.5, 1.5)),
                0.0,
                Color32::from_rgba_premultiplied(v, v, v, v),
            );
        }
        let mono = |size: f32| FontId::monospace(size);
        let inner = rect.shrink2(vec2(10.0, 8.0));

        // Status line
        let status = match state {
            State::Stopped => "STOPPED".to_string(),
            State::Song => "SONG".to_string(),
            State::Auto(w) => {
                let drop = match w.bars_to_drop() {
                    0 | 1 => "DROP NOW".to_string(),
                    n => format!("DROP IN {n}"),
                };
                format!("{} · {}/{} · {drop}", w.phase(), w.bar + 1, w.len)
            }
            State::Bar(b) => format!("BAR {}", b + 1),
        };
        p.text(inner.left_top(), Align2::LEFT_TOP, &status, mono(11.0), ACID);
        let mut top = inner.top() + 18.0;
        // The energy the auto mode steers: a row of cells, like an old level meter
        if let Some(e) = energy {
            p.text(pos2(inner.left(), top), Align2::LEFT_TOP, "ENERGY", mono(10.0), alpha(ACID, 0.7));
            p.text(
                pos2(inner.right(), top),
                Align2::RIGHT_TOP,
                format!("{:.0}", e * 100.0),
                mono(10.0),
                ACID,
            );
            let (x0, x1) = (inner.left() + 50.0, inner.right() - 24.0);
            let n = 24;
            let cw = (x1 - x0) / n as f32;
            for i in 0..n {
                let on = e >= (i as f32 + 0.5) / n as f32;
                let r = Rect::from_min_size(pos2(x0 + i as f32 * cw, top + 2.0), vec2(cw - 2.0, 8.0));
                p.rect_filled(r, 1.0, if on { ACID } else { alpha(ACID, 0.12) });
            }
            top += 16.0;
        }
        // Some room above and below the line
        top += 4.0;
        p.line_segment(
            [pos2(inner.left(), top - 4.0), pos2(inner.right(), top - 4.0)],
            stroke(1.0, alpha(ACID, 0.25)),
        );
        top += 4.0;

        // The lines, from the newest up – clipped below the status line, fading out before it
        let p = p.with_clip_rect(Rect::from_min_max(pos2(rect.left(), top), rect.max));
        let font = mono(10.5);
        let mut bottom = inner.bottom();
        for (k, (text, at)) in self.lines.iter().rev().enumerate() {
            if bottom <= top {
                break;
            }
            let newest = k == 0;
            let typed = if newest {
                let chars = ((now - at) * TYPE_SPEED).max(0.0) as usize;
                text.chars().take(chars).collect::<String>()
            } else {
                text.clone()
            };
            let fade = if newest { 1.0 } else { (0.75 - 0.08 * k as f32).max(0.25) };
            let galley = p.layout(format!("> {typed}"), font.clone(), ACID, inner.width() - 8.0);
            let gh = galley.size().y;
            let pos = pos2(inner.left(), bottom - gh);
            let edge = ((pos.y - top) / 30.0).clamp(0.0, 1.0);
            // A blinking block where the typing is
            if newest && (now * 2.0).fract() < 0.6 {
                let end = galley.rows.last().map_or(pos, |r| pos + vec2(r.rect.right() + 2.0, r.rect.top()));
                p.rect_filled(Rect::from_min_size(end, vec2(6.0, 11.0)), 0.0, alpha(ACID, 0.8));
            }
            p.galley_with_override_text_color(pos, galley, alpha(ACID, fade * edge));
            bottom -= gh + 4.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(bar: u64, brk: Option<u64>) -> State {
        State::Auto(Where {
            bar,
            len: 32,
            brk,
            heat: 0.6,
        })
    }

    #[test]
    fn the_log_counts_down_to_breakdown_and_drop() {
        let mut log = Log::new();
        for bar in 0..32 {
            log.tick(at(bar, Some(24)), bar as f64, &Song::default());
        }
        let lines: Vec<&str> = log.lines().collect();
        assert!(lines[0].starts_with("New cycle · 32 bars"), "{lines:?}");
        for want in ["Drop in 16 bars", "Breakdown in 8 bars", "Breakdown in 4 bars", "Drop in 8 bars"] {
            assert!(lines.contains(&want), "{want}: {lines:?}");
        }
        // Each said once, in time order
        assert_eq!(lines.iter().filter(|l| l.starts_with("Drop in")).count(), 2);
    }

    #[test]
    fn a_roller_says_so_and_facts_and_tips_come_in_turn() {
        let song = Song::default();
        let mut log = Log::new();
        log.tick(at(0, None), 0.0, &song);
        assert!(log.lines().any(|l| l.contains("No breakdown")));
        // Stopped: nothing comes; playing: a line every half minute or so, in turn
        log.tick(State::Stopped, 100.0, &song);
        let n = log.lines().count();
        log.tick(State::Bar(3), 119.0, &song);
        assert_eq!(log.lines().count(), n);
        log.note_drop();
        for t in 0..300 {
            log.tick(State::Bar(3), 120.0 + t as f64, &song);
        }
        let lines: Vec<&str> = log.lines().skip(n).collect();
        assert!((6..=11).contains(&lines.len()), "{lines:?}");
        assert!(lines.iter().any(|l| l.ends_with("1 drop")), "{lines:?}");
        assert!(lines.iter().any(|l| l.contains("BPM")), "{lines:?}");
        assert!(lines.iter().any(|l| l.starts_with("Tip:")), "{lines:?}");
    }
}
