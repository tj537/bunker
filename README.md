<p align="center">
  <img src="assets/icon.png" alt="" width="128">
</p>

<h1 align="center">BUNKER</h1>

<p align="center">
  <b>Generative techno from another planet.</b><br>
  A drum machine for people who have never used one – press play, pick a vibe, hit DROP.
</p>

<p align="center">
  <a href="https://github.com/tj537/bunker/releases/latest"><img src="https://img.shields.io/github/v/release/tj537/bunker?style=flat-square&color=c8ff3d&labelColor=17171b&label=latest" alt="Latest release"></a>
  <img src="https://img.shields.io/badge/macOS%20%C2%B7%20Windows-17171b?style=flat-square" alt="macOS and Windows">
  <img src="https://img.shields.io/badge/100%25%20synthesized-no%20samples-ff3b46?style=flat-square&labelColor=17171b" alt="100% synthesized">
  <img src="https://img.shields.io/badge/Rust-egui-8c7dff?style=flat-square&labelColor=17171b" alt="Rust and egui">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-45c8ff?style=flat-square&labelColor=17171b" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://github.com/tj537/bunker/releases/latest/download/Bunker-macos-universal.zip"><img src="https://img.shields.io/badge/%EF%A3%BF%20macOS-Download-ff3b46?style=for-the-badge" alt="Download for macOS" height="44"></a>
  &nbsp;
  <a href="https://github.com/tj537/bunker/releases/latest/download/Bunker-windows-x64.zip"><img src="https://img.shields.io/badge/%E2%8A%9E%20Windows-Download-c8ff3d?style=for-the-badge&labelColor=333" alt="Download for Windows" height="44"></a>
</p>

![Bunker playing: the orbit in the middle, tracks and mood on the left, live controls on the right, favorites below](docs/images/orbit.jpg)

Bunker is a techno drum machine written in Rust. Every sound is a glowing ring, every step a dot, and a beam of light circles like a radar. Kick, clap, hats, bass and a 303-style synth are synthesized live: no samples, no plug-ins, no account, no tracking. It's a download of a few megabytes for macOS and Windows.

## Why Bunker

**You don't need to know anything about music to use it.**

- **Grooves that already work.** Pick a vibe and Bunker rolls a pattern that follows the genre's rules: four on the floor, claps on 2 and 4, a bassline that dodges the kick, bass and synth on the same chord. It sounds like techno from the first second.
- **Words instead of theory.** You don't pick a scale or a chord progression. You pick a mood from a dropdown next to the knobs: *Dark, Hypnotic, Deep, Sad, Euphoric, Dreamy, Epic, Sunny, Mysterious* – or leave it on *Auto* and every shuffle picks one that suits the vibe. Each brings its own chords, so the bars move instead of repeating; a mood you pick stays through shuffles and the auto mode. Tempo and swing come with the vibe. Or pick *Custom* and start from an empty pattern.
- **Nothing can go off-beat.** Switching favorites, drops and song parts wait for the next bar, and every loop fits the bar.
- **One fader from intro to drop.** The energy fader is the arrangement, not the volume: low is a break, the middle is the groove, 90 and up is a drop.
- **A song without a timeline.** Save your favorite grooves, pick AUTO SONG from the arrow next to the record button, and Bunker plays a whole track with them: intro, groove, break, build-up, drop, outro.
- **Nothing to break.** <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes every change, a shuffle too. Your work is saved automatically.

**And if you do know what you're doing,** you still get per-step accents, slides and probability, tune and decay per track, a master EQ, MIDI in and WAV recording.

## What it does

- **Ten vibes:** Dub, Minimal, Melodic, Hypnotic, Electro, Peak time, Acid, Rave, Industrial, Hard. Each has its own tempo, sounds and pattern rules. The shuffle button next to it (<kbd>Enter</kbd>) rolls a new groove – always clearly different from the one before, sometimes busier, sometimes sparser. **Custom** starts from an empty pattern for you to paint.
- **Eight tracks, 34 sounds:** five kicks (909, 808, Hard, Deep, Punch), clap, snare and snap, rim, cowbell, clave and toms, closed and open hats, rides, five basses (rolling, sub, reese, pulse, donk), and a synth that plays 303 saw or square, chord stabs, plucks or a rave hoover. Click a track's name to hear its sound. Every track also has tune, decay, solo, mute, a dice for a new rhythm, and a lock that keeps shuffles away from it.
- **Planets**, the main view: the groove as a little solar system seen from an angle. Every track is an orbit round the sun, every step that plays a planet on it – accents wear a ring like Saturn, half behind the planet. A radar beam sweeps round once a bar and every planet it passes lights up and sends out a ring. Bass and synth planets float on stalks – the higher the note, the higher they float. The sun is the play button.
- **Editing:** click to switch a step, drag to paint, right-click a step for its menu – on, accent, slide, how often it plays and, on bass and synth, the note from the scale. Shift-click and the mouse wheel are shortcuts for how often and the note. In the planets view you can simply grab a bass or synth planet and drag it up or down: it moves through the scale, and a ladder beside it shows the degree – 0 is the root, then +1, +2 … up or -1, -2 … down – and the note's name.
- **PLANETS | CLASSIC** at the top (or <kbd>Tab</kbd>) shows the same groove as eight pads and a step grid. The older **Radar** view (flat rings of dots with a sweeping beam) can be switched on in the settings as a legacy view.
- **Energy and drop:** the energy fader changes what plays, never how loud. Under 30 it's a break: no kick, no bass, the sound closes and opens into the reverb. Going up, kick, bass, open hat and ride come in on the beat; a track the energy holds out says **OUT** next to its name (in the auto mode with AUTO FX, Bunker mutes it instead – the mute button shows it). **DROP** turns the groove itself into a two-bar build-up (a kick roll, running hats, the riff looping and lifting an octave, a gate chopping harder and harder), then half a beat of silence, then everything hits. Pushing the fader into its red **drop zone** (90 and up) does the same – so you can take a track from intro to drop with one slider.
- **Favorites and songs:** twelve slots in two rows to keep grooves, each with a name you give it (right-click → Rename). A click switches on the next bar with a clap roll; the lit one is exactly what plays and goes dark once the auto mode changes the groove. **AUTO SONG** (the arrow next to the record button) turns favorite 1 into a whole track – *Short*, *Club* or *Hypnotic*: it starts quietly with the hats under a closed filter, brings the tracks in one by one, keeps the grooves moving with a gentle change every 8 bars, eases into each break, and drops two or three times – the first time after about half a minute. Favorite 2 takes over after the first drop and favorite 1 returns for the last (with only one favorite, every drop is a new chapter of it); the outro winds down track by track. The song steers the energy; the bar shows where it is. Click a part in the song strip to jump there. A song records itself; when it ends, save it as a WAV.
- **Record your set:** the red record button in the top bar (or <kbd>R</kbd>) records everything you hear – whatever plays, the auto mode, songs, effects, pauses included – with a running timer, until you click it again. Then a save dialog asks where the WAV should go.
- **Auto mode:** Bunker plays an endless set on its own. It thinks in 8-bar phrases: a bigger change opens a phrase – a pattern, a sound, new notes, taking turns between drums and bass/synth – small touches come in between (every 1 to 16 bars – a slider behind the arrow on the AUTO MODE button), and a fill closes it, a bigger one every 16 bars. Every few drops a new chapter brings one or two bigger changes, most likely after the peak. The **AUTO FX** switch, in the same menu and on by default, decides who arranges:
  - **On:** Bunker does everything. Its energy glides through cycles of 32 to 64 bars with breakdowns of four kinds (or none – a rolling cycle), build-ups and drops, climbing a step every phrase. Like a DJ set it has an arc: each cycle runs a little hotter than the last until a short, stripped-down peak, then the set breathes out into a longer, calmer cycle and climbs again. Every build-up comes with a gate that chops harder and harder. Whatever it takes out it mutes, so you always see it on the mute buttons. It also plays with the mix a little – track volumes, DARK, SPACE and the master EQ wander gently around where you set them, and a knob you touch is yours. The energy then shows as a level meter in the log.
  - **Off:** the energy bar is yours and decides what plays – low is a break, 85 is everything, 90 drops. The auto mode keeps changing patterns, sounds and notes but never takes a track out and never breaks or drops by itself.
- **The log** under the mood types out, like an old terminal, everything Bunker says and does – and in the auto mode where the set is, when the breakdown and the drop come, and how hot the set runs. Now and then it adds where the set stands (minutes, drops), what plays (vibe, key, mood, tempo) or a tip.
- **Live effects (hold to use):** stutter, echo, wash, crush, gate and tape stop, plus a spring-back filter. When Bunker uses one by itself – the gate in every build-up – its button lights up and flickers with the chop. Nine knobs sit at the top right (a click on SOUND shows or folds them): DARK, ACID, SPACE, PUMP, PITCH (how high bass and synth sit), DRIVE and the master EQ LOW, MID, HIGH – drag up or scroll, double-click resets.
- **Two visualizers** for the big screen with no text, just a small energy bar at the bottom. LIGHTS goes fullscreen; FACE opens in a window of its own, so you can drag it to a projector and keep playing (double-click or <kbd>F</kbd> for fullscreen):
  - **LIGHTS** (<kbd>P</kbd>): a spectrum ring, kick shockwaves and a star field.
  - **FACE** (<kbd>O</kbd>): a low-poly head in space that nods in time, with empty eye holes like a mask's – you see the stars through them – and a black slit for a mouth that now and then falls open, corners down. They glow with the kick, its edges flicker with the hats. In a build-up it warps, glitches, splits into red and blue, nods twice as fast and its mouth starts to tear open; on the drop it rushes at you, tears its mouth open and screams with lasers blazing from its eyes, cracks glow through its face, and it bursts into shards that fly out of the picture and come back together.

  All keys keep working, so <kbd>←</kbd> <kbd>→</kbd> take the set from break to drop.
- **Files:** `.bunker` files (plain text) keep the groove and all twelve favorites with their names.
- **Settings (the gear):** volume, the sound output, the MIDI keyboard and the update check (Bunker asks GitHub once at start whether there is a new version – you can switch that off). Headphones and MIDI keyboards are picked up while Bunker runs. The output level runs as a thin line along the bottom of the space window.

## Under the hood

- **Rust 2021**, about 9k lines with no `unsafe` code. The UI is immediate-mode [egui](https://github.com/emilk/egui)/eframe 0.31. Audio goes out through [cpal](https://github.com/RustAudio/cpal), MIDI comes in through [midir](https://github.com/Boddlnagg/midir), and files are [RON](https://github.com/ron-rs/ron) via serde.
- **Real-time audio thread.** The engine renders inside the device callback without locks, blocking or file I/O. The UI sends commands over an `mpsc` channel and reads the playhead, hit lights, levels and spectrum from atomics. When the engine changes the song itself (auto mode, song mode, recording), it sends the new song back as an event.
- **Copyable song model.** A `Song` is plain fixed-size data with no `Vec` or `String`, so it can be swapped between threads cheaply. New fields use `#[serde(default)]`, so older saves and files keep loading.
- **All DSP is hand-written:** TPT state-variable filters, PolyBLEP saws, a 303-style voice with accent and slide, 808-style metallic hats from six detuned squares, a Freeverb-style reverb, a ping-pong delay, a kick rumble, saturation and a limiter. Every sound is synthesized from scratch.
- **Recording without stalls.** While recording, the audio thread hands what it renders to the UI in 32k-sample pieces over the same channel; the UI keeps them as 16-bit and writes the WAV on a worker thread.
- **Deterministic and tested.** Randomness comes from a seedable RNG, so patterns and renders are repeatable. The test suite checks the genre rules for every vibe, timing, drops, songs, clipping (peak ≤ 1.0) and the file format. An offscreen renderer draws the real UI to PNG for visual checks without touching the desktop.
- **CI and releases** run on GitHub Actions. They check fmt, `clippy -D warnings` and the tests on Linux, macOS and Windows. A tag builds a universal (arm64 + x86_64) macOS app and a Windows exe and publishes them with SHA-256 checksums.

More detail is in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Keys

- <kbd>Space</kbd>: play / stop
- <kbd>Enter</kbd>: shuffle a new groove
- <kbd>A</kbd>–<kbd>K</kbd>: the eight pads
- <kbd>1</kbd>–<kbd>8</kbd>: mute / unmute a track
- <kbd>←</kbd> <kbd>→</kbd>: energy down / up
- <kbd>↑</kbd> <kbd>↓</kbd>: tempo
- <kbd>B</kbd>: build-up and drop
- <kbd>Q</kbd> <kbd>W</kbd> <kbd>E</kbd> <kbd>X</kbd> <kbd>C</kbd> <kbd>V</kbd> (hold): stutter, echo, wash, crush, gate, tape stop
- <kbd>R</kbd>: record your set on / off
- <kbd>Tab</kbd>: planets → classic (→ radar, when it's on)
- <kbd>P</kbd>: LIGHTS visualizer
- <kbd>O</kbd>: FACE visualizer
- <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>Z</kbd>: undo (add <kbd>Shift</kbd> to redo)
- <kbd>Esc</kbd>: close a window or leave the visualizer

## MIDI

Bunker connects to every MIDI keyboard it finds, including ones you plug in later.

- **Keys below middle C** play the sounds: C kick, D clap, E rim, F hat, G open hat, A ride, B bass (the black keys fill in).
- **Keys from middle C up** (hold) transpose bass and synth live.
- **Mod wheel:** acid. **Pitch bend:** filter. **Sustain pedal:** echo.
- **Knobs:** CC 70–73 (the Akai MPK mini default) or CC 74/71/91/93 control energy, dark, acid and space.

## Install

**macOS 11+** (Apple Silicon and Intel): download [`Bunker-macos-universal.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-macos-universal.zip), unzip it and move Bunker to Applications. The app isn't notarized yet, so on first launch right-click it and choose *Open*, or use *System Settings → Privacy & Security → Open Anyway*.

**Windows 10/11:** download [`Bunker-windows-x64.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-windows-x64.zip), unzip it and run `Bunker.exe`. There is no installer and no admin rights are needed. SmartScreen may ask once: *More info → Run anyway*.

Every release has a `SHA256SUMS.txt`.

## Build it yourself

You need [Rust](https://rustup.rs). On Linux you also need `libasound2-dev` and `libgtk-3-dev`.

```bash
cargo run --release
```

```bash
cargo test --release
```

```bash
cargo test --release render_ -- --ignored
```

The last command writes UI screenshots and demo loops (WAV) to `target/snapshots/`. On a Mac, `./scripts/build-mac.sh --install` builds `Bunker.app` and copies it to Applications.

## Credits

Built with [egui](https://github.com/emilk/egui), [cpal](https://github.com/RustAudio/cpal) and [midir](https://github.com/Boddlnagg/midir). The fonts are [Space Grotesk](https://github.com/floriankarsten/space-grotesk) and [Anton](https://github.com/googlefonts/AntonFont) (SIL Open Font License), and the icons are from [Phosphor](https://phosphoricons.com).

Made by Tom, together with Claude. MIT licensed.
