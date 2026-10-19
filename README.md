<p align="center">
  <img src="assets/icon.png" alt="" width="128">
</p>

<h1 align="center">BUNKER</h1>

<p align="center">
  <b>Techno beats in orbit.</b><br>
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
- **Words instead of theory.** You don't pick a scale or a chord progression. You pick a mood: *Dark, Hypnotic, Deep, Sad, Euphoric, Dreamy, Epic, Sunny, Mysterious*. Each brings its own chords, so the bars move instead of repeating. Tempo and swing come with the vibe. Or pick *Custom* and start from an empty pattern.
- **Nothing can go off-beat.** Switching favorites, drops and song parts wait for the next bar, and every loop fits the bar.
- **One fader from intro to drop.** The energy fader is the arrangement, not the volume: low is a break, the middle is the groove, 90 and up is a drop.
- **A song without a timeline.** Save your favorite grooves, press AUTO SONG, and Bunker plays a whole track with them: intro, groove, break, build-up, drop, outro.
- **Nothing to break.** <kbd>⌘</kbd>/<kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes every change, a shuffle too. Your work is saved automatically.

**And if you do know what you're doing,** you still get per-step accents, slides and probability, tune and decay per track, a master EQ, MIDI in and WAV recording.

## What it does

- **Ten vibes:** Dub, Minimal, Melodic, Hypnotic, Electro, Peak time, Acid, Rave, Industrial, Hard. Each has its own tempo, sounds and pattern rules. **SHUFFLE** (<kbd>Enter</kbd>) rolls a new groove. **Custom** starts from an empty pattern for you to paint.
- **Eight tracks, 34 sounds:** five kicks (909, 808, Hard, Deep, Punch), clap, snare and snap, rim, cowbell, clave and toms, closed and open hats, rides, five basses (rolling, sub, reese, pulse, donk), and a synth that plays 303 saw or square, chord stabs, plucks or a rave hoover. Every track also has tune, decay, solo, mute, a dice for a new rhythm, and a lock that keeps shuffles away from it.
- **The orbit:** click to switch a step, drag to paint, right-click a step (or a tile in the Classic view) for its menu – on, accent, slide, how often it plays and, on bass and synth, the note from the scale. Shift-click and the mouse wheel are shortcuts for how often and the note. **ORBIT | CLASSIC** at the top (or <kbd>Tab</kbd>) switches to the Classic view: eight pads and a step grid.
- **Energy and drop:** the energy fader changes what plays, never how loud. Under 30 it's a break: no kick, no bass, the sound closes and opens into the reverb. Going up, kick, bass, open hat and ride come in on the beat; a track the energy holds out says **OUT** next to its name. **DROP** turns the groove itself into a two-bar build-up (a kick roll, running hats, the riff looping and lifting an octave, a gate chopping harder and harder), then half a beat of silence, then everything hits. Pushing the fader into its red **drop zone** (90 and up) does the same – so you can take a track from intro to drop with one slider.
- **Favorites and songs:** twelve slots in two rows to keep grooves, each with a name you give it (right-click → Rename). A click switches on the next bar with a clap roll; the lit one is exactly what plays and goes dark once the auto mode changes the groove. **AUTO SONG** turns favorite 1 into a whole track – *Short*, *Club* or *Hypnotic*: it starts quietly with the hats under a closed filter, brings the tracks in one by one, keeps the grooves moving with a gentle change every 8 bars, eases into each break, and drops two or three times – the first time after about half a minute. Favorite 2 takes over after the first drop and favorite 1 returns for the last (with only one favorite, every drop is a new chapter of it); the outro winds down track by track. The song steers the energy; the bar shows where it is. Click a part in the song strip to jump there. A song records itself; when it ends, save it as a WAV.
- **Record your set:** the red record button in the top bar (or <kbd>R</kbd>) records everything you hear – whatever plays in the orbit, the auto mode, songs, effects, pauses included – with a running timer, until you click it again. Then a save dialog asks where the WAV should go.
- **Auto mode:** Bunker plays an endless set on its own. It changes one thing every 2, 4 or 8 bars – a pattern, a sound, the notes – and plays fills; every few drops a new chapter brings one or two bigger changes. The **AUTO FX** switch decides who arranges:
  - **On:** Bunker does everything. Its energy glides through cycles of 32 to 64 bars with breakdowns of four kinds, build-ups and drops, every build-up with a gate that chops harder and harder. The energy bar shrinks to a slim display.
  - **Off:** the energy bar is yours and decides what plays – low is a break, 85 is everything, 90 drops. The auto mode keeps changing patterns, sounds and notes but never takes a track out and never breaks or drops by itself.
- **Live effects (hold to use):** stutter, echo, wash, crush, gate and tape stop, plus a spring-back filter. When Bunker uses one by itself – the gate in every build-up – its button lights up and flickers with the chop. The sound sliders are DARK, ACID, SPACE and PUMP; the master page has LOW, MID, HIGH and DRIVE.
- **Two visualizers** for the big screen, fullscreen with no text, just a small energy bar at the bottom:
  - **LIGHTS** (<kbd>P</kbd>): a spectrum ring, kick shockwaves and a star field.
  - **FACE** (<kbd>O</kbd>): a low-poly head in space that nods in time, with slit-pupilled eyes that dart around and stare at you. They glow with the kick, its edges flicker with the hats. In a build-up it warps, glitches, splits into red and blue, nods twice as fast and its mouth starts to tear open; on the drop it rushes at you, tears its mouth open and screams with lasers blazing from its eyes, cracks glow through its face, and it bursts into shards that fly out of the picture and come back together.

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
- <kbd>Tab</kbd>: orbit ↔ classic
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
