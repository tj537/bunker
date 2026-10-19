# Architecture

Bunker is one Rust binary: an [egui](https://github.com/emilk/egui) window (eframe 0.31) and a synthesizer engine that runs inside the audio callback ([cpal](https://github.com/RustAudio/cpal)). Nothing is loaded from disk except the saved state – fonts and the icon are compiled in, every sound is computed. The only network access is the optional update check at start (`update.rs`).

```
src/
  main.rs       window setup
  app.rs        BunkerApp: panels (top bar, vibe bar, tracks, sound/auto/live, scenes), keys, MIDI, export, help, persistence
  orbit.rs      the circular sequencer (drawing + mouse) and `Live`, the playback state the UI shows
  pads.rs       pads view: eight pads + step grid
  widgets.rs    knobs, chips, glowing buttons, effect buttons, the spring-back filter slider
  theme.rs      colours, fonts (Space Grotesk, Anton, Phosphor icons), egui style
  song.rs       data model: Song → 8 Tracks (with a sound each) → 16 Steps, vibes, scales, macros (all `Copy`)
  gen.rs        groove generator (vibes, sounds per vibe, per-track dice), `evolve` and `new_chapter` for the auto mode
  engine.rs     sequencer clock, voices, mixer, master and live effects, drop, auto arrangement, scene chain
  synth.rs      the sounds (every voice has several models) and effects (SVF filter, delay, reverb, rumble)
  audio.rs      cpal output: runs the engine in the device callback
  midi.rs       midir input: decode messages, connect every port
  wav.rs        16-bit WAV writer
  rng.rs        PCG32 random numbers (seedable → repeatable tests and exports)
  update.rs     update check: asks the GitHub API for the latest release tag (via the system's curl)
  offscreen.rs  software rasterizer for egui (tests only)
  snapshot.rs   screenshot tests (ignored by default)
```

## Threads and data flow

```
 UI thread (egui)                         audio thread (cpal callback)
 ───────────────                          ────────────────────────────
 BunkerApp.song ── Cmd::Song(Box<Song>) ──▶ Engine.song
 pads / keys    ── Cmd::Hit, Fx, Drop, … ──▶ Engine
 scenes         ── Cmd::Chain(slots)    ──▶ Engine
               ◀── Event::Song(song, Change) ── auto mode, recording, scene chain
               ◀── Shared (atomics): step, hits per track, level, build-up
 MIDI callback  ── Cmd (pads, transpose, filter, echo) ──▶ Engine   (no UI round trip)
                ── Midi ──▶ UI (macros, display)
```

- `Song` is `Copy` (fixed arrays), so it travels whole through the channel. The UI sends it whenever it changed this frame (`song != sent`).
- The engine owns its copy. When it changes the song itself – auto mode, a recorded hit, the drop – it sends it back with a `Change` the UI shows as a caption.
- The UI reads the playhead and hit counters from `Shared` atomics; the smooth beam interpolates between steps with the frame time.

## Engine

`Engine::render` fills an interleaved stereo buffer sample by sample:

1. **Clock** – `phase` counts 16th steps; a step fires when the integer part changes (odd steps wait for the swing offset). Bar starts call `new_bar`: drops, the scene chain (next filled slot every `Chain::bars`), and in auto mode the 32-bar arrangement. Each cycle picks a `Plan` (classic, short and filtered, stripped, filtered): groove with `gen::evolve` every `auto_bars` (2/4/8) and a one-bar `Fill` before every 8-bar phrase, the breakdown (the plan's tracks muted, optionally under a low-pass that opens by itself), 2 bars build-up, drop. 45 % of drops start a `gen::new_chapter` (new bass and synth lines, maybe sounds and key). A breakdown restores the mutes it saved when it ends, also when the music stops, the auto mode is switched off or a chain starts.
2. **Steps** – each track loops over its own length (`Track::step_at`), so lengths below 16 make polyrhythms. `chance` < 100 rolls the dice. Energy gates the percussion live: below 45 it drops unaccented hits, above 60 it adds ghost hats (chosen by a hash of track, step and 2-bar block, so it forms a pattern). Synth steps whose next step slides get a longer gate and glide.
3. **Voices** – `trigger` picks the model from `Track::sound` (`KICKS`, `HATS`, `OPEN_HATS`, `RIDES` tables in `synth.rs`; clap, rim, bass and synth switch on a `model` index). The synth's stab plays the scale's triad on the step's note.
4. **Mixer** (pan, delay send, reverb send per track; stabs get extra echo) → rumble (kick through dark resonant delays, ducked by the kick) → drop boom / crash → build-up riser.
5. **Master** – wash (everything into a huge reverb, dry ducked), spring-back filter (or the breakdown's auto filter), build-up high-pass, darkness tone filter (18 kHz down to ~1.5 kHz), crush (sample-and-hold + fewer bits), gate (16th chop), saturation, a simple limiter, master volume, then tape stop (a 2-second ring buffer read back at a falling rate).

`render_offline` runs the same engine without a device – for WAV export and tests.

Macros act live (energy: percussion density and level, kick drive, saturation; dark: kick length, rumble, tone; acid: filter and resonance – the auto mode sweeps it slowly; space: sends and feedback) and the generator uses Energy for density.

Live effects are `Cmd::Fx(Fx, bool)` – held down / let go. The UI sends a change only when key or mouse state changes (`fx_sent`).

## App state around the song

- **Undo/redo** – at the end of every frame the UI compares `song` with `sent` (the last song it gave the engine). A difference is a user edit: the previous song goes on the undo stack, unless the last edit was less than 0.8 s ago (a knob turn or a painted row is one step). Songs coming from the engine don't count; neither does the restore itself (`restoring`). Undo and slot loads keep the current master volume and auto-mode settings (`keep_live_settings`).
- **Scenes A–D** – `slots: [Option<Song>; 4]` and `slot_now`, saved with the window state. A user edit also writes the song into the lit slot; a shuffle, undo or redo detaches it (`slot_now = None`) so a saved scene is never overwritten by surprise. With the chain on, every slot change sends `Cmd::Chain` again; the engine reports the scene it switched to with `Change::Scene`. Chain and auto mode exclude each other.
- **Sounds** – `Track::sound` indexes `Voice::sounds()`. Changing the synth between 303 / stab / pluck / hoover rolls a line written for the new sound (undo keeps the old one). Changing a sound while stopped plays a preview after the new song went out (`preview`).
- **Update check** – `update::check` runs `curl` against the GitHub releases API on a thread at start (unless switched off in the help window, `check_updates`), compares the tag with `CARGO_PKG_VERSION` and the top bar shows an update button.
- **Solo** – remembers everyone's mute state and puts it back when the solo ends.
- **Devices** – every 3 s the UI checks the number of MIDI ports and the name of the default audio output. New port count → reconnect MIDI; another default output (headphones) or a failed stream (`audio_failed`, set by cpal's error callback) → `restart_audio` builds a new engine on new channels and reconnects MIDI to it.
- **Loaded state is repaired** with `Song::sanitize` (lengths 1–16, finite volumes, tempo 90–180 …), also when the engine receives a song.

## Generator

`gen::generate(vibe, macros, rng)` first picks a sound per track that suits the vibe (`sound_for`), then builds every track from genre rules: kick on 0/4/8/12 plus ghost kicks (electro: broken patterns), claps on 2 and 4, hats as 16ths, off-beats or shakers, Euclidean rims and rides, a bass template that never sits on a kick. The synth line depends on its sound: a 303 line walking through the scale, off-beat chord stabs, a plucked arpeggio or a hoover riff. Bass, stabs and arpeggios follow a two-chord progression derived from key and scale (`progression`), so they agree even when only one track is rolled again. Tests check the rules for every vibe (`every_vibe_makes_a_techno_groove`).

`gen::evolve` changes exactly one thing: a new pattern, a track in or out (at least four stay), a rotation, another sound, new notes on the same rhythm, or a polyrhythm. It never touches the kick.

## UI

Immediate mode: `BunkerApp::ui` draws everything every frame (continuously while playing). The orbit and pads are custom-painted with the egui painter (circles, arcs, a mesh for the beam). `Geometry` in `orbit.rs` maps steps to positions and back for hit testing.

Layout: top bar, vibe bar (ten equal chips + SHUFFLE), tracks on the left (row height adapts to the window), sound knobs / auto / live on the right, the orbit or pads in the middle with the scene strip below. Short windows get smaller effect buttons and DROP. Minimum window size is 1100 × 700.

The UI fonts have no ♯, ◂ or ▸ – use `#` and words instead.

## Checking changes

There is no way to drive the real window from a terminal. Instead:

- `cargo test --release render_ -- --ignored` writes screenshots to `target/snapshots/*.png` (orbit stopped/playing/build-up, chain + update button, pads, small window, help, and `readme_orbit` – the source of the README pictures in `docs/images`) and demo loops for every vibe plus a 68-bar auto-mode run to `target/snapshots/sounds/*.wav`.
- `cargo test --release print_levels -- --ignored --nocapture` prints the loudness of each track and each of its sounds alone – for balancing the mix (sounds of one track should stay within ~3 dB of each other).
