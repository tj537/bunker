# Architecture

Bunker is one Rust binary: an [egui](https://github.com/emilk/egui) window (eframe 0.31) and a synthesizer engine that runs inside the audio callback ([cpal](https://github.com/RustAudio/cpal)). Nothing is loaded from disk except the saved state – fonts and the icon are compiled in, every sound is computed. The only network access is the optional update check at start (`update.rs`).

```
src/
  main.rs       window setup
  app.rs        BunkerApp: panels (top bar with the vibe dropdown, tracks, mood, sound|master/auto/live, patterns and song strip, volume on the speaker), keys, MIDI, files, song recording, help, persistence
  visual.rs     the visualizer: spectrum ring, kick shockwaves, star field – drawn from `Live` only
  file.rs       .bunker files (RON): song and patterns
  form.rs       song forms (Short, Club, Hypnotic): parts, lengths, patterns, energy per part
  orbit.rs      the circular sequencer (drawing + mouse) and `Live`, the playback state the UI shows
  pads.rs       pads view: eight pads + step grid
  widgets.rs    knobs, chips, glowing buttons, effect buttons, the energy fader, the spring-back filter slider
  theme.rs      colours, fonts (Space Grotesk, Anton, Phosphor icons), egui style
  song.rs       data model: Song → 8 Tracks (sound, tune, decay, lock) → 16 Steps, vibes, scales, chord progressions, moods, macros, master EQ (all `Copy`)
  gen.rs        groove generator (vibes, sounds per vibe, per-track dice), `evolve` and `new_chapter` for the auto mode
  engine.rs     sequencer clock, voices, mixer, master and live effects, drop, auto arrangement, song mode, patterns on the next bar
  synth.rs      the sounds (every voice has several models) and effects (SVF filter, delay, reverb, rumble)
  audio.rs      cpal output: runs the engine in the device callback
  midi.rs       midir input: decode messages, connect every port
  wav.rs        16-bit WAV writer
  rng.rs        PCG32 random numbers (seedable → repeatable tests and renders)
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
 patterns, song ── Cmd::Queue, Arrange, Jump ──▶ Engine
               ◀── Event::Song(song, Change) ── auto mode, recording, song mode
               ◀── Shared (atomics): step, hits per track, level, build-up, spectrum bands
 MIDI callback  ── Cmd (pads, transpose, filter, echo) ──▶ Engine   (no UI round trip)
                ── Midi ──▶ UI (macros, display)
```

- `Song` is `Copy` (fixed arrays), so it travels whole through the channel. The UI sends it whenever it changed this frame (`song != sent`).
- The engine owns its copy. When it changes the song itself – auto mode, a recorded hit, the drop – it sends it back with a `Change` the UI shows as a caption.
- The UI reads the playhead and hit counters from `Shared` atomics; the smooth beam interpolates between steps with the frame time.

## Engine

`Engine::render` fills an interleaved stereo buffer sample by sample:

1. **Clock** – `phase` counts 16th steps; a step fires when the integer part changes (odd steps wait for the swing offset). Bar starts call `new_bar`: drops, a pattern waiting from `Cmd::Queue` (it comes in now; a `Fill::Roll` led into it), song mode (`arrange_bar`), and in auto mode the arrangement. It runs in cycles (`cycle_start`, `cycle_len`: 32 bars, in the calm vibes also 48 or 64); every drop – also one by hand or from the drop zone – starts the next cycle (`new_cycle`), so an automatic drop never follows a manual one closely. Each cycle picks a `Plan` (classic, short and filtered, stripped, filtered): groove with `gen::evolve` every `auto_bars` (2/4/8) and a one-bar `Fill` before every 8-bar phrase, the breakdown (the plan's tracks muted, optionally under a low-pass that opens by itself), 2 bars build-up, drop. Every few drops (`drops_since_chapter` ≥ 2, then a coin flip) start a `gen::new_chapter`: one or two of a new bassline, a new synth line, another sound, other chords (refitted notes) or, rarely, another key. Breakdowns, build-ups and drops only come from the curve (`Song::curve`); with the energy by hand the auto mode just evolves the groove and fills. A breakdown restores the mutes it saved when it ends, also when the music stops, the auto mode is switched off or a song starts. Every build-up (DROP, drop zone, auto mode, song mode) starts on a bar and lasts two bars and is made from the groove itself (`build_play`): the kick rolls (quarters, 8ths, 16ths – also when energy or a breakdown took it out, not when it's muted by hand), the hats run in 16ths, the open hat rests, bass, synth, rim and ride loop the second bar's first beat, then half a beat, the synths an octave up for the last quarter. The last `DROP_GAP` steps are silent, then `drop_now` lands on the bar after. On top, a `BuildStyle`: DROP by hand is always `Classic` (a high-pass opens upwards); with `Song::auto_fx` the auto mode and songs pick `Lowpass` (closes to a muffle, the drop throws it open), `Gate` (the gate chops harder with the progress) or `Echo` (classic, and the last hits are thrown into the echo for the silence). The filter lets go in the same style after the drop (`cut_style`, `build_cut`). AUTO FX also turns some phrase ends (`phrase_end`) into an effect moment (`fx_moment`: stutter, gate, crush from step 8 or 12, or an echo throw) and opens breakdowns in the wash.
2. **Steps** – each track loops over its own length (`Track::step_at`), so lengths below 16 make polyrhythms. The generator and the auto mode only use 16 and 8, which stay in step with the bar. `chance` < 100 rolls the dice. A closed hat never plays on a step where the open hat plays. Energy (`layer_energy`, taken on every beat) gates the percussion live: below 45 it drops unaccented hits, above 60 it adds ghost hats (chosen by a hash of track, step and 2-bar block, so it forms a pattern). Synth steps whose next step slides get a longer gate and glide.
3. **Voices** – `trigger` applies the track's `tune` and `decay` and, for bass and synth, the chord progression (`Song::prog_degree` of the bar, as scale degrees), then picks the model from `Track::sound` (`KICKS`, `HATS`, `OPEN_HATS`, `RIDES` tables in `synth.rs`; clap, rim, bass and synth switch on a `model` index). The synth's stab plays the scale's triad on the step's note.
4. **Mixer** (pan, delay send, reverb send per track; stabs get extra echo; pump ducks everything but the kick and the returns with an envelope the kick restarts) → rumble (kick through dark resonant delays, ducked by the kick) → drop boom / crash → build-up riser (fades out in the gap before the drop).
5. **Master** – wash (everything above ~280 Hz into a huge reverb, dry ducked; the room shrinks slowly after it, so the tail rings out), spring-back filter (or the breakdown's auto filter), build-up high-pass (lets go within a few ms after the drop instead of jumping), darkness tone filter (18 kHz down to ~1.5 kHz), master EQ (SVF low shelf / bell / high shelf, ±12 dB), crush (sample-and-hold + fewer bits), gate (16th chop), saturation, a simple limiter, master volume, then tape stop (a 2-second ring buffer read back at a falling rate). Last, eight band-passes measure the spectrum for the visualizer (`Shared::bands`).

**Energy** is the arrangement, never the volume: a track plays only while `layer_energy ≥ LAYERS[track]` (hats and synth always, then rim, clap, kick at 0.3, bass, open hat, ride), checked on every beat so tracks come and go in time. Below `BREAK_ZONE` (0.3) it's a break: kick and bass out, a low-pass closes (`break_lp`) and the reverb sends grow, following `energy_smooth`. Track volumes, kick drive and the master saturation no longer depend on it. A drop lifts the energy to at least 0.85. Energy by hand that reaches `DROP_ZONE` (0.9) starts a build-up (`watch_drop_zone`); it arms again below `DROP_REARM` (0.88). In auto mode with `Song::curve` on, the engine owns the energy: `curve_target` (or `Part::energy` in song mode) sets `energy_goal` every bar (high after the drop, rising to the breakdown, low in it, climbing through the build-up) and the energy glides there by at most `GLIDE` per step; a `Cmd::Song` from the UI can't change it. The UI's fader sets `curve = false` (by hand) – then the UI's value counts again.

**Song mode** (`Arrangement`: the patterns, a form from `form::FORMS`, and a fallback song for when no pattern is saved): `arrange_bar` moves to the next section when one is over or on a `Cmd::Jump`, and `enter_section` loads the section's pattern (`form::pattern_slot`: an empty slot falls back to the nearest full one before it), sets `curve` and the part's energy (`Part::energy`, every bar after), mutes kick and bass for a `Break` (a `Breakdown`, so a pattern change keeps them out) and drops into every `Peak`. A break builds up over its last two bars; a fill leads into a section with another pattern, plus random fills every 8 bars in grooves and peaks. After the last section the engine stops and sends `Change::SongEnd`. The section and the bar it started go to `Shared::part` / `part_start` for the strip.

`render_offline` runs the same engine without a device – for tests and the demo loops.

**Song recording**: from a song's first section, `keep_recording` appends every rendered buffer and sends `Event::Audio` pieces of `REC_CHUNK` samples; when the song ends or stops, `end_recording` adds two seconds of tail, then `finish_recording` sends the rest and `Event::Recorded`. The UI keeps the pieces as 16-bit (`wav::to_i16`), offers *Save WAV* and writes it on a thread (`wav::write_i16`).

Macros act live (energy: the layers, percussion density, hat length, bass brightness and the break zone; dark: kick length, rumble, tone; acid: filter and resonance – the auto mode sweeps it slowly; space: sends and feedback) and the generator uses Energy for density.

Live effects are `Cmd::Fx(Fx, bool)` – held down / let go. The UI sends a change only when key or mouse state changes (`fx_sent`).

## App state around the song

- **Undo/redo** – at the end of every frame the UI compares `song` with `sent` (the last song it gave the engine). A difference is a user edit: the previous song goes on the undo stack, unless the last edit was less than 0.8 s ago (a knob turn or a painted row is one step). Songs coming from the engine don't count; neither does the restore itself (`restoring`). Undo and slot loads keep the current master volume and auto-mode settings (`keep_live_settings`).
- **Locks** – `Track::lock`: shuffle uses `gen::generate_keeping` with the locked tracks, the dice is disabled, `gen::evolve` and `gen::new_chapter` skip them.
- **Files** – *File* menu: `.bunker` save/open (also by drag and drop). A loaded file is an ordinary edit, so undo goes back.
- **Visualizer** – `visual: bool`; the window then shows only `visual::visual` in fullscreen, keys keep working, Esc leaves.
- **Patterns A–D** – `slots: [Option<Song>; 4]` and `slot_now`, saved with the window state. A user edit also writes the song into the lit slot; a shuffle, undo or redo detaches it (`slot_now = None`) so a saved pattern is never overwritten by surprise. A click while playing sends `Cmd::Queue` and marks `queued_slot` (it blinks) until the engine reports `Change::Scene`; stopped, it switches at once. **Song** – `song_form` and `song_fallback`; every slot change sends `Cmd::Arrange` again (same form = patterns only), `Change::Part` tells the UI which pattern plays. Song and auto mode exclude each other; stopping ends the song.
- **Mood** – `song::MOODS`, nine of them (scale + progression); a new mood refits the bass and synth notes to it (`gen::refit_notes`). Swing comes from the vibe. **Custom** (`Vibe::Custom`) clears every unlocked track; shuffle and the auto mode then follow hypnotic rules (`Vibe::rules`).
- **Orbit** – a loop whose length divides 16 is drawn repeated around its ring (`Geometry::shown`), so its light turns with the beam; a click on any copy edits the step it repeats.
- **Sounds** – `Track::sound` indexes `Voice::sounds()`. Changing the synth between 303 / stab / pluck / hoover rolls a line written for the new sound (undo keeps the old one). Changing a sound while stopped plays a preview after the new song went out (`preview`).
- **Update check** – `update::check` runs `curl` against the GitHub releases API on a thread at start (unless switched off in the help window, `check_updates`), compares the tag with `CARGO_PKG_VERSION` and the top bar shows an update button.
- **Solo** – remembers everyone's mute state and puts it back when the solo ends.
- **Devices** – every 3 s the UI checks the number of MIDI ports and the name of the default audio output. New port count → reconnect MIDI; another default output (headphones) or a failed stream (`audio_failed`, set by cpal's error callback) → `restart_audio` builds a new engine on new channels and reconnects MIDI to it.
- **Loaded state is repaired** with `Song::sanitize` (lengths 1–16, finite volumes, tempo 90–180 …), also when the engine receives a song.

## Generator

`gen::generate(vibe, macros, rng)` first picks a sound per track that suits the vibe (`sound_for`), then builds every track from genre rules: kick on 0/4/8/12 plus ghost kicks that always play (electro: broken patterns), claps on 2 and 4, hats as 16ths, off-beats or shakers, Euclidean rims and rides that stay off kick and clap, a bass template that never sits on a kick. The synth line depends on its sound: a 303 line that lands on a chord note on every beat and wanders in between, off-beat chord stabs, a plucked arpeggio or a hoover riff. Bass and every synth line follow a two-chord progression derived from key and scale (`progression`) – the bass always plays the chord's root (or its fifth) – so they agree even when only one track is rolled again. Shorter loops are 8 or 12 steps and only on percussion. Tests check the rules for every vibe (`every_vibe_makes_a_techno_groove`, `the_tracks_fit_together`).

`gen::evolve` changes exactly one thing: a new pattern, a track in or out (at least four stay), a rotation, another sound from the vibe's set (`sounds_for`), new notes on the same rhythm, or a shorter percussion loop. It never touches the kick.

## UI

Immediate mode: `BunkerApp::ui` draws everything every frame (continuously while playing). The orbit and pads are custom-painted with the egui painter (circles, arcs, a mesh for the beam). `Geometry` in `orbit.rs` maps steps to positions and back for hit testing.

Layout: top bar (with the vibe dropdown and SHUFFLE), tracks on the left (row height adapts to the window; the sound pill opens sounds + tune/decay), SOUND|MASTER knobs / auto / LIVE with the AUTO FX switch, energy, filter, effects, drop on the right, the orbit or pads in the middle with the pattern strip below (and the song strip above it while a song plays). No status bar: the shortcuts are in the help, the sound status is an icon in the top bar. Short windows get smaller effect buttons and DROP. Minimum window size is 1100 × 700.

The UI fonts have no ♯, ◂ or ▸ – use `#` and words instead.

## Checking changes

There is no way to drive the real window from a terminal. Instead:

- `cargo test --release render_ -- --ignored` writes screenshots to `target/snapshots/*.png` (orbit stopped/playing/build-up, a song with a waiting pattern + update button, pads, small window with master tab and locks, help, visualizer, and `readme_orbit` – the source of `docs/images/orbit.jpg`, the README's only picture) and demo loops for every vibe plus a 68-bar auto-mode run to `target/snapshots/sounds/*.wav`.
- `cargo test --release print_levels -- --ignored --nocapture` prints the loudness of each track and each of its sounds alone – for balancing the mix (sounds of one track should stay within ~3 dB of each other).
