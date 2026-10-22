// On Windows, don't open a console window next to the app (release builds only)
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod controls;
mod engine;
mod face;
mod file;
mod form;
mod gen;
mod log;
mod midi;
#[cfg(test)]
mod offscreen;
mod orbit;
mod pads;
mod planets;
mod rng;
#[cfg(test)]
mod snapshot;
mod song;
mod synth;
mod theme;
mod update;
mod visual;
mod wav;
mod widgets;

use eframe::egui;

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([1360.0, 860.0])
        .with_min_inner_size([1100.0, 700.0])
        .with_title("Solarum");
    // Icon for the window, taskbar and Dock (when started without the .app bundle)
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")) {
        viewport = viewport.with_icon(icon);
    }
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    // (The storage keeps its old name, "Bunker", so saved grooves and settings survive the rename)
    eframe::run_native("Bunker", options, Box::new(|cc| Ok(Box::new(app::SolarumApp::new(cc)))))
}
