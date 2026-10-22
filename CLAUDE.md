# Solarum – notes for working on the code

Techno drum machine: orbit sequencer, pads, bass & acid, auto mode, live effects, WAV export, MIDI input. Rust + eframe/egui 0.31, cpal for audio, midir for MIDI. Architecture: `docs/ARCHITECTURE.md`; user docs: `README.md`. Keep both up to date with every feature.

## Commands

| Command | What it does |
|---|---|
| `cargo test --release` | all tests (fast) |
| `cargo clippy --release --all-targets -- -D warnings` | must stay free of warnings |
| `cargo fmt` | CI checks `cargo fmt --check` (`max_width = 140`) |
| `cargo test --release render_ -- --ignored` | screenshots to `target/snapshots/*.png` and demo loops to `target/snapshots/sounds/*.wav` – look at (and listen to) them after UI or sound changes |
| `cargo test --release print_levels -- --ignored --nocapture` | loudness of every track alone |
| `./scripts/build-mac.sh --install` | builds `dist/Solarum.app` (universal), replaces `/Applications/Solarum.app` and removes the build copy; `NATIVE=1` builds for this Mac only |
| `./scripts/build-mac.sh --zip` | builds `dist/Bunker-macos-universal.zip` (release asset) |

Commit only when `cargo test` exits 0.

## Conventions

- **Language:** UI, README, docs, release notes, commit messages and code comments in English.
- **egui pins:** egui/eframe 0.31 with `egui-phosphor` 0.9 – upgrade them together (same as Mindful).
- **Audio thread:** `Engine::render` runs in the device callback – no locks, no blocking, no file I/O there. Talk to it with `Cmd`, read state from `Shared`.
- **Song is `Copy`:** keep it that way (fixed arrays, no `Vec`/`String`) so it can be swapped between threads cheaply.
- **New `Song` / `Track` / `Step` fields:** add `#[serde(default)]` so saved state from older versions still loads.
- **Randomness:** use `rng::Rng` (seedable), so tests and exports are repeatable.
- **Visual checks:** verify UI with the offscreen renderer (`src/offscreen.rs`, tests in `src/snapshot.rs`, `SolarumApp::headless` + `fake_live`). Don't take screenshots of the user's desktop.
- **Sound checks:** render the demo loops and check `print_levels`; nothing may clip (tests assert peak ≤ 1).

## Releases

Asset names never change (`Bunker-macos-universal.zip`, `Bunker-windows-x64.zip`) so `releases/latest/download/…` links keep working. They keep the old name from before the rename to Solarum (the zips hold `Solarum.app` / `Solarum.exe`); the app also keeps its storage under "Bunker" (`main.rs`) so nothing saved gets lost.

1. Raise `version` in `Cargo.toml`, commit.
2. Write `.github/release-notes/vX.Y.Z.md` (English).
3. `git tag -a vX.Y.Z -m "Solarum X.Y.Z" && git push origin vX.Y.Z` – `.github/workflows/release.yml` builds both platforms, adds `SHA256SUMS.txt` and publishes the release.

## Platform notes

- Windows: `build.rs` embeds `assets/icon.ico` (only when building on Windows); `main.rs` sets the GUI subsystem.
- Linux builds need `libasound2-dev` and `libgtk-3-dev` (CI installs them); Linux isn't released.
- The Mac bundle is ad-hoc signed only (no notarization yet), so first launch needs *Open Anyway*.
- Icons: the logo is drawn by `widgets::logo` (the step ring – top bar, start screen); `cargo test --release render_icon -- --ignored` renders it to `target/snapshots/icon.png` (1024 px), and `icon.png` (512), `icon.icns` (sips + iconutil) and `icon.ico` (PNG entries) come from that. `assets/icon.svg` is the same drawing as a vector.
