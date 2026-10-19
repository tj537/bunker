//! Tests only: screenshots of the whole window, rendered without a window.
//!
//!     cargo test --release render_ -- --ignored    → target/snapshots/*.png

#[cfg(test)]
mod tests {
    use crate::app::{BunkerApp, View};
    use crate::offscreen::Offscreen;
    use crate::orbit::Live;
    use crate::song::{Song, TRACKS};
    use std::path::Path;

    fn render(name: &str, w: f32, h: f32, setup: impl Fn(&mut BunkerApp)) {
        let mut snap = Offscreen::retina(w, h, 2.0);
        crate::theme::install(&snap.ctx);
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        setup(&mut app);
        let mut shapes = Vec::new();
        // A few frames so layouts and fonts settle
        for t in 0..4 {
            shapes = snap.frame(10.0 + t as f64 * 0.016, |ctx| app.ui(ctx));
        }
        snap.save(&shapes, Path::new(&format!("target/snapshots/{name}.png")));
    }

    /// The face over time: frames at these seconds, a build-up (or none) in each
    fn render_face(name: &str, frames: &[(f64, Option<f32>)]) {
        let mut snap = Offscreen::retina(1360.0, 860.0, 2.0);
        crate::theme::install(&snap.ctx);
        let mut app = BunkerApp::headless(Song::default(), View::Orbit, false);
        app.visual = true;
        app.face = true;
        app.song.macros.energy = 0.8;
        let mut shapes = Vec::new();
        for &(t, build) in frames {
            app.fake_live = Some(Live {
                build,
                caption: None,
                ..playing(t * 8.0, &[0, 3])
            });
            shapes = snap.frame(t, |ctx| app.ui(ctx));
        }
        snap.save(&shapes, Path::new(&format!("target/snapshots/{name}.png")));
    }

    #[test]
    #[ignore]
    fn render_face_frames() {
        render_face("face", &[(0.5, None), (0.52, None)]);
        render_face(
            "face_gape",
            &[(crate::face::gape_at(), None), (crate::face::gape_at() + 0.02, None)],
        );
        render_face("face_build", &[(10.0, Some(0.85)), (10.02, Some(0.85))]);
        render_face("face_scream", &[(10.0, Some(0.99)), (10.02, None), (10.35, None)]);
        render_face("face_laser", &[(10.0, Some(0.99)), (10.02, None), (10.9, None)]);
        render_face("face_crack", &[(10.0, Some(0.99)), (10.02, None), (11.7, None)]);
        render_face("face_burst", &[(10.0, Some(0.99)), (10.02, None), (12.15, None)]);
        render_face("face_back", &[(10.0, Some(0.99)), (10.02, None), (13.0, None)]);
    }

    fn playing(pos: f64, recent: &[usize]) -> Live {
        let mut hit_age = [2.0; TRACKS];
        for &t in recent {
            hit_age[t] = 0.03;
        }
        Live {
            playing: true,
            pos,
            hit_age,
            build: None,
            bar: (pos / 16.0) as u64,
            part: None,
            caption: Some(("New hat pattern".into(), 0.5)),
            bands: [0.22, 0.12, 0.05, 0.03, 0.035, 0.02, 0.015, 0.008],
            level: 0.7,
        }
    }

    #[test]
    #[ignore]
    fn render_window() {
        render("orbit_stopped", 1360.0, 860.0, |_| {});
        render("orbit_playing", 1360.0, 860.0, |app| {
            app.song.auto = true;
            app.slots[0] = Some(app.song);
            app.slots[1] = Some(crate::gen::generate(
                crate::song::Vibe::Acid,
                &Default::default(),
                &mut crate::rng::Rng::new(4),
            ));
            app.slot_now = Some(0);
            app.fake_live = Some(playing(36.3, &[0, 3, 6]));
        });
        render("orbit_chain_update", 1360.0, 860.0, |app| {
            for (i, vibe) in [crate::song::Vibe::Dub, crate::song::Vibe::Rave, crate::song::Vibe::Melodic]
                .into_iter()
                .enumerate()
            {
                app.slots[i] = Some(crate::gen::generate(
                    vibe,
                    &Default::default(),
                    &mut crate::rng::Rng::new(i as u64 + 30),
                ));
            }
            app.song = app.slots[1].unwrap();
            app.slot_now = Some(1);
            app.queued_slot = Some(2);
            app.song_form = Some(1);
            app.capture = true;
            app.new_version = Some(crate::update::Release { version: "0.3.0".into() });
            app.fake_live = Some(Live {
                part: Some((4, 9.4)),
                ..playing(20.3, &[0, 7])
            });
        });
        render("orbit_build", 1360.0, 860.0, |app| {
            app.fake_live = Some(Live {
                build: Some(0.6),
                ..playing(50.5, &[1])
            });
        });
        render("orbit_small", 1100.0, 700.0, |app| {
            app.slots[0] = Some(app.song);
            app.slot_now = Some(0);
            app.fake_live = Some(playing(4.2, &[0]));
        });
        render("break_small", 1100.0, 700.0, |app| {
            app.song.macros.energy = 0.15;
            app.fake_live = Some(playing(4.2, &[3]));
        });
        render("pads", 1360.0, 860.0, |app| {
            app.view = View::Pads;
            app.fake_live = Some(playing(9.4, &[2, 7]));
        });
        render("pads_small", 1100.0, 700.0, |app| {
            app.view = View::Pads;
        });
        render("help_small", 1100.0, 700.0, |app| {
            app.help = true;
        });
        render("visualizer", 1360.0, 860.0, |app| {
            app.visual = true;
            app.song.macros.energy = 0.8;
            app.fake_live = Some(Live {
                caption: Some(("Drop!".into(), 0.4)),
                ..playing(66.2, &[0, 1, 3, 6])
            });
        });
        render("master_tab_small", 1100.0, 700.0, |app| {
            app.master_tab = true;
            app.song.auto = true;
            app.song.tracks[0].lock = true;
            app.song.tracks[6].lock = true;
            app.song.prog = 3;
            app.fake_live = Some(playing(12.5, &[0]));
        });
        // The README picture: docs/images/orbit.jpg (1600 px wide JPEG of this)
        render("readme_orbit", 1360.0, 860.0, |app| {
            use crate::song::{Vibe, Voice};
            let mut rng = crate::rng::Rng::new(77);
            for (i, vibe) in [Vibe::Dub, Vibe::Acid, Vibe::Rave].into_iter().enumerate() {
                let mut s = crate::gen::generate(vibe, &Default::default(), &mut rng);
                s.track_mut(Voice::Acid).mute = false;
                app.slots[i] = Some(s);
            }
            app.song = app.slots[1].unwrap();
            app.song.auto = true;
            app.song.auto_bars = 4;
            app.song.prog = 3;
            app.song.macros.pump = 0.4;
            app.song.tracks[0].lock = true;
            app.song.tracks[3].tune = 2;
            app.slot_now = Some(1);
            app.fx_key[2] = true;
            app.fake_live = Some(Live {
                caption: Some(("Hat sound: 808".into(), 0.5)),
                ..playing(37.3, &[0, 3, 6, 7])
            });
        });
    }
}
