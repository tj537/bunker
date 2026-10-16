# Architecture

Bunker is one Rust binary: an [egui](https://github.com/emilk/egui) window (eframe 0.31) and a synthesizer engine that runs inside the audio callback ([cpal](https://github.com/RustAudio/cpal)). Nothing is loaded from disk except the saved state – fonts and the icon are compiled in, every sound is computed.

```
src/
  main.rs       window setup
  app.rs        BunkerApp: panels, keys, MIDI, export window, help, persistence
  orbit.rs      the circular sequencer (drawing + mouse) and `Live`, the playback state the UI shows
  pads.rs       pads view: eight pads + step grid
  widgets.rs    knobs, chips, glowing buttons, the spring-back filter slider
  theme.rs      colours, fonts (Space Grotesk, Anton, Phosphor icons), egui style
  song.rs       data model: Song → 8 Tracks → 16 Steps, vibes, scales, macros (all `Copy`)
  gen.rs        groove generator (Shuffle, per-track dice) and `evolve` for the auto mode
  engine.rs     sequencer clock, voices, mixer, master effects, drop, auto arrangement
  synth.rs      the sounds and effects (SVF filter, kick, clap, rim, hats, bass, acid, delay, reverb, rumble)
  audio.rs      cpal output: runs the engine in the device callback
  midi.rs       midir input: decode messages, connect every port
  wav.rs        16-bit WAV writer
  rng.rs        PCG32 random numbers (seedable → repeatable tests and exports)
  offscreen.rs  software rasterizer for egui (tests only)
  snapshot.rs   screenshot tests (ignored by default)
```

## Threads and data flow

```
 UI thread (egui)                         audio thread (cpal callback)
 ───────────────                          ────────────────────────────
 BunkerApp.song ── Cmd::Song(Box<Song>) ──▶ Engine.song
 pads / keys    ── Cmd::Hit, Drop, …    ──▶ Engine
               ◀── Event::Song(song, Change) ── auto mode, recording
               ◀── Shared (atomics): step, hits per track, level, build-up
 MIDI callback  ── Cmd (pads, transpose, filter, echo) ──▶ Engine   (no UI round trip)
                ── Midi ──▶ UI (macros, display)
```

- `Song` is `Copy` (fixed arrays), so it travels whole through the channel. The UI sends it whenever it changed this frame (`song != sent`).
- The engine owns its copy. When it changes the song itself – auto mode, a recorded hit, the drop – it sends it back with a `Change` the UI shows as a caption.
- The UI reads the playhead and hit counters from `Shared` atomics; the smooth beam interpolates between steps with the frame time.

## Engine

`Engine::render` fills an interleaved stereo buffer sample by sample:

1. **Clock** – `phase` counts 16th steps; a step fires when the integer part changes (odd steps wait for the swing offset). Bar starts call `new_bar`: drops, and in auto mode the 32-bar arrangement (24 bars groove with `gen::evolve` every `auto_bars`, 6 bars breakdown, 2 bars build-up, drop).
2. **Steps** – each track loops over its own length (`Track::step_at`), so lengths below 16 make polyrhythms. `chance` < 100 rolls the dice. Acid steps whose next step slides get a longer gate and glide.
3. **Voices** → mixer (pan, delay send, reverb send per track) → rumble (kick through dark resonant delays, ducked by the kick) → drop boom / crash → build-up riser.
4. **Master** – spring-back filter (low-pass/high-pass), build-up high-pass, darkness tone filter, saturation, a simple limiter, master volume.

`render_offline` runs the same engine without a device – for WAV export and tests.

Macros act live (energy: hat/ride level, kick drive; dark: kick length, rumble, tone; acid: filter and resonance; space: sends) and the generator uses Energy for density.

## Generator

`gen::generate(vibe, macros, rng)` builds every track from genre rules: kick on 0/4/8/12 (+ ghost kicks per vibe), claps on 4/12, hats as 16ths or off-beats, Euclidean rims and rides, a bass template that avoids the kick, a 303 line walking through the scale with accents, slides and octave jumps. Tests check the rules for every vibe (`every_vibe_makes_a_techno_groove`).

`gen::evolve` changes exactly one thing: a new pattern, a track in or out (at least four stay), or a pattern rotated by a few steps. It never touches the kick.

## UI

Immediate mode: `BunkerApp::ui` draws everything every frame (continuously while playing). The orbit and pads are custom-painted with the egui painter (circles, arcs, a mesh for the beam). `Geometry` in `orbit.rs` maps steps to positions and back for hit testing.

Short windows (< 760 px of panel height) put the four knobs in one row. Minimum window size is 1100 × 700.

## Checking changes

There is no way to drive the real window from a terminal. Instead:

- `cargo test --release render_ -- --ignored` writes screenshots to `target/snapshots/*.png` (orbit stopped/playing/build-up, pads, small window) and demo loops for every vibe plus a 36-bar auto-mode run to `target/snapshots/sounds/*.wav`.
- `cargo test --release print_levels -- --ignored --nocapture` prints the loudness of each track alone – for balancing the mix.
