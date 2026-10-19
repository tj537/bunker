//! Song forms: ready-made arrangements (intro, groove, break, peak, outro) that the
//! engine plays with the saved patterns A–D, steering energy, breaks and drops itself.

/// A part of a song
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Part {
    /// Starts thin, the energy climbs
    Intro,
    /// The groove at full steam
    Groove,
    /// Kick and bass out, energy low; the last two bars build up into the next part
    Break,
    /// Everything in, the energy at the top – it starts with a drop
    Peak,
    /// The energy sinks, then the song ends
    Outro,
}

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Intro => "Intro",
            Part::Groove => "Groove",
            Part::Break => "Break",
            Part::Peak => "Peak",
            Part::Outro => "Outro",
        }
    }

    /// The energy `bar` bars into a part of `bars` bars
    pub fn energy(self, bar: u64, bars: u64) -> f32 {
        let t = bar as f32 / bars.max(1) as f32;
        match self {
            // From hats under a closed filter, one track after the other comes in
            Part::Intro => 0.05 + 0.7 * t,
            Part::Groove => 0.75 + 0.1 * t,
            // Deep in the break zone, rising through the build-up
            Part::Break => {
                if bar + 2 >= bars {
                    0.45
                } else {
                    0.15
                }
            }
            Part::Peak => 0.97 - 0.1 * t,
            // The tracks leave one after the other, down to the hats
            Part::Outro => 0.75 * (1.0 - t),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Section {
    pub part: Part,
    pub bars: u8,
    /// The pattern it plays: 0…3 = A…D (an empty slot falls back to another one)
    pub pattern: u8,
}

const fn s(part: Part, bars: u8, pattern: u8) -> Section {
    Section { part, bars, pattern }
}

pub struct Form {
    pub name: &'static str,
    pub blurb: &'static str,
    pub sections: &'static [Section],
}

impl Form {
    pub fn bars(&self) -> u32 {
        self.sections.iter().map(|s| s.bars as u32).sum()
    }
}

use Part::*;

/// Built around favorite 1; favorite 2 (if there is one) takes over after the first drop.
/// With one favorite only, Bunker makes the part after the drop a new chapter of it.
pub const FORMS: [Form; 3] = [
    Form {
        name: "Short",
        blurb: "One break, one drop",
        sections: &[s(Intro, 16, 0), s(Groove, 16, 0), s(Break, 8, 0), s(Peak, 16, 1), s(Outro, 16, 1)],
    },
    Form {
        name: "Club",
        blurb: "Two breaks, two drops – a full club track",
        sections: &[
            s(Intro, 16, 0),
            s(Groove, 16, 0),
            s(Groove, 16, 0),
            s(Break, 8, 0),
            s(Peak, 32, 1),
            s(Groove, 16, 1),
            s(Break, 8, 1),
            s(Peak, 16, 1),
            s(Outro, 16, 1),
        ],
    },
    Form {
        name: "Hypnotic",
        blurb: "Long and rolling, one big drop",
        sections: &[
            s(Intro, 32, 0),
            s(Groove, 32, 0),
            s(Groove, 32, 0),
            s(Break, 16, 0),
            s(Peak, 32, 1),
            s(Outro, 32, 1),
        ],
    },
];

/// The slot a section's pattern comes from: its own if it's full, else the nearest full
/// one before it, else the first full one; None when every slot is empty
pub fn pattern_slot(want: u8, full: [bool; 4]) -> Option<usize> {
    let want = (want as usize).min(3);
    (0..=want).rev().find(|&i| full[i]).or_else(|| (0..4).find(|&i| full[i]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forms_make_sense() {
        for f in &FORMS {
            let parts: Vec<Part> = f.sections.iter().map(|s| s.part).collect();
            assert_eq!(parts.first(), Some(&Intro), "{}", f.name);
            assert_eq!(parts.last(), Some(&Outro), "{}", f.name);
            // Every break builds up into a peak
            for (i, p) in parts.iter().enumerate() {
                if *p == Break {
                    assert_eq!(parts.get(i + 1), Some(&Peak), "{}", f.name);
                }
            }
            for s in f.sections {
                assert!(s.bars >= 4 && s.pattern < 4);
            }
        }
    }

    #[test]
    fn empty_slots_fall_back() {
        assert_eq!(pattern_slot(2, [true, true, true, false]), Some(2));
        assert_eq!(pattern_slot(3, [true, false, true, false]), Some(2));
        assert_eq!(pattern_slot(1, [false, false, true, false]), Some(2));
        assert_eq!(pattern_slot(0, [false; 4]), None);
    }

    #[test]
    fn energy_climbs_and_sinks() {
        assert!(Intro.energy(0, 16) < Intro.energy(15, 16));
        assert!(Outro.energy(0, 16) > Outro.energy(15, 16));
        assert!(Peak.energy(0, 16) > 0.9);
        assert!(Break.energy(2, 8) < 0.35);
    }
}
