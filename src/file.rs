//! `.bunker` files: the groove and the four patterns in one readable text file
//! (RON), to keep or to share. (Older files also carry scene lengths and drops –
//! they're ignored.)

use crate::app::SLOTS;
use crate::song::Song;
use serde::{Deserialize, Serialize};

pub const EXTENSION: &str = "bunker";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BunkerFile {
    /// Raised when the format changes in a way older versions can't read
    pub format: u32,
    pub song: Song,
    /// The patterns A–D
    pub scenes: [Option<Song>; SLOTS],
}

impl Default for BunkerFile {
    fn default() -> Self {
        BunkerFile {
            format: 1,
            song: Song::default(),
            scenes: [None; SLOTS],
        }
    }
}

impl BunkerFile {
    pub fn to_text(&self) -> String {
        let pretty = ron::ser::PrettyConfig::new().depth_limit(3);
        ron::ser::to_string_pretty(self, pretty).unwrap_or_default()
    }

    /// Read a file, repairing anything out of range
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut f: BunkerFile = ron::from_str(text).map_err(|e| format!("not a Bunker file ({e})"))?;
        f.song.sanitize();
        for s in f.scenes.iter_mut().flatten() {
            s.sanitize();
        }
        Ok(f)
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), String> {
        std::fs::write(path, self.to_text()).map_err(|e| e.to_string())
    }

    pub fn load(path: &std::path::Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        Self::from_text(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gen;
    use crate::rng::Rng;
    use crate::song::Vibe;

    #[test]
    fn round_trips() {
        let mut f = BunkerFile {
            song: gen::generate(Vibe::Dub, &Default::default(), &mut Rng::new(1)),
            ..Default::default()
        };
        f.scenes[2] = Some(gen::generate(Vibe::Hard, &Default::default(), &mut Rng::new(2)));
        f.song.tracks[3].tune = -5;
        f.song.tracks[3].lock = true;
        let back = BunkerFile::from_text(&f.to_text()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn repairs_and_rejects() {
        assert!(BunkerFile::from_text("hello").is_err());
        // Missing fields fall back to defaults, broken values are repaired, old fields are ignored
        let f = BunkerFile::from_text("(song: (bpm: 999.0), scene_bars: (0, 8, 8, 200))").unwrap();
        assert_eq!(f.song.bpm, 180.0);
    }
}
