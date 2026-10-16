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
            caption: Some(("New hat pattern".into(), 0.5)),
        }
    }

    #[test]
    #[ignore]
    fn render_window() {
        render("orbit_stopped", 1360.0, 860.0, |_| {});
        render("orbit_playing", 1360.0, 860.0, |app| {
            app.song.auto = true;
            app.fake_live = Some(playing(36.3, &[0, 3, 6]));
        });
        render("orbit_build", 1360.0, 860.0, |app| {
            app.fake_live = Some(Live {
                build: Some(0.6),
                ..playing(50.5, &[1])
            });
        });
        render("orbit_small", 1100.0, 700.0, |app| {
            app.fake_live = Some(playing(4.2, &[0]));
        });
        render("pads", 1360.0, 860.0, |app| {
            app.view = View::Pads;
            app.fake_live = Some(playing(9.4, &[2, 7]));
        });
        render("pads_small", 1100.0, 700.0, |app| {
            app.view = View::Pads;
        });
    }
}
