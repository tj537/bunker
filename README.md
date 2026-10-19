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

![Bunker playing: the orbit, ten vibes on top, sounds per track on the left, live effects on the right, scenes below](docs/images/orbit.jpg)

Every sound is a glowing ring, every step a dot, a beam of light circles like a radar. Pick one of **ten vibes**, give every track **its own sound**, keep grooves in **scenes** and chain them into a song – or switch on the **auto mode** and let Bunker play a whole set with fills, breakdowns and drops while you play the **live effects**.

Written in Rust, every sound synthesized live – no samples, no account, no tracking.

> 🇩🇪 **Kurz auf Deutsch:** Bunker ist eine Techno-Drum-Machine zum Ausprobieren, ganz ohne Vorwissen. Jeder Sound ist ein leuchtender Ring, jeder Punkt ein Schritt – klicken, und es groovt. Oben wählst du einen von zehn **Vibes** (Dub bis Hard Techno), jede Spur hat mehrere **Sounds** zur Auswahl (909, 808, Snare, Cowbell, Reese, Stab, Hoover …). **Szenen** speichern Grooves und spielen sie als **Kette** nacheinander ab. Der **Auto-Modus** spielt selbstständig ein ganzes Set – mit Fills, verschiedenen Breakdowns, Drops und neuen Kapiteln. Dazu sechs **Live-Effekte** (Stutter, Echo, Wash, Crush, Gate, Tape Stop), Filter, DROP-Button, WAV-Export und MIDI-Keyboards. Läuft auf Mac und Windows, alles wird live synthetisiert, und Bunker sagt Bescheid, wenn es ein Update gibt.

---

## ✨ What's inside

<table>
<tr>
<td width="50%" valign="top">

### 🎛️ Ten vibes
**Dub · Minimal · Melodic · Hypnotic · Electro · Peak time · Acid · Rave · Industrial · Hard** – one click rolls a new groove that follows the rules of the genre: four on the floor (or broken electro kicks), claps on 2 and 4, a bassline that dodges the kick, chords that agree with the bass. Each vibe brings its own tempo, key, swing and sounds. **SHUFFLE** (<kbd>Enter</kbd>) rolls again.

### 🥁 A sound for every track
Click the coloured pill under a track name (or scroll on it):

| Track | Sounds |
|---|---|
| Kick | 909 · 808 · Hard · Deep · Punch |
| Clap | Clap · Snare · Big clap · Snap |
| Rim | Rim · Cowbell · Clave · Tom · Blip |
| Hat / Open hat | 909 · 808 · Shaker · Tick / Sizzle |
| Ride | Ride · Bell · Dark |
| Bass | Rolling · Sub · Reese · Pulse · Donk |
| Synth | 303 saw · 303 square · Stab · Pluck · Hoover |

**S** solos a track, the speaker mutes it, the 🎲 rolls a new rhythm for just that track.

</td>
<td width="50%" valign="top">
<img src="docs/images/tracks.jpg" alt="Track list with sound pills, solo, mute and dice" width="100%">
</td>
</tr>
<tr>
<td width="50%" valign="top">
<img src="docs/images/live.jpg" alt="Sound knobs, auto mode and the six live effects" width="100%">
</td>
<td width="50%" valign="top">

### ♾️ Auto mode
Bunker plays a whole set on its own. Every **2, 4 or 8 bars** it changes one thing – a new pattern, another sound, new notes, a polyrhythm, a track in or out. Every 8 bars a **fill** (clap roll, falling toms, a kick gap, an echo throw). Every 32 bars a **breakdown** – a different kind each time – a build-up and the drop. After some drops a **new chapter** starts: new bassline, new sounds, sometimes a new key.

### 🔥 Live effects
Hold them, combine them:
**STUTTER** <kbd>Q</kbd> · **ECHO** <kbd>W</kbd> · **WASH** <kbd>E</kbd> · **CRUSH** <kbd>X</kbd> · **GATE** <kbd>C</kbd> · **TAPE STOP** <kbd>V</kbd> – plus a **filter** that springs back and the **DROP** button: build-up, clap roll, noise riser, then everything hits on the next bar.

### 🎚️ Four sound knobs
**ENERGY** thins out or fills up the hats and drives the kick · **DARK** closes the sound and adds rumble · **ACID** squeezes the synth filter · **SPACE** reverb and echo.

</td>
</tr>
</table>

### 🎬 Scenes & chain

![Scenes A–D under the orbit and the CHAIN button](docs/images/scenes.jpg)

Four memory slots under the orbit. Click **+** to keep the groove you hear, click a full scene to switch to it – the lit one keeps your changes. **CHAIN** plays the full scenes one after another (A → B → C → D) every 4, 8 or 16 bars, like a song. Right-click a scene to overwrite or clear it.

### And also

- **The orbit** – click a dot to switch a step, drag to paint, right-click for an accent (on the synth: slide), shift-click for chance, scroll on bass and synth dots to change the note. Give a ring fewer steps and it drifts against the others – polyrhythms you can *see*.
- **Pads** – <kbd>Tab</kbd> for eight big pads and a classic step grid. Play with the mouse or <kbd>A</kbd>–<kbd>K</kbd>, hit **RECORD** and your hits land on the nearest step.
- **Key & swing** – the arrows move bass and synth up or down, the scale button changes the mood (minor, phrygian, dorian), swing adds bounce.
- **Undo everything** – <kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>Z</kbd> takes back any change, a shuffle too.
- **Export WAV** – 4 to 64 bars, 44.1 kHz stereo, auto mode included.
- **Plug and play** – switch to headphones or plug in a MIDI keyboard while it runs; Bunker follows within a few seconds.
- **Update check** – Bunker asks GitHub once at start whether there is a new version and shows a green button if there is. Nothing else is sent; switch it off in the help window (<kbd>?</kbd>).

![Pads and step grid](docs/images/pads.jpg)

## 🎹 MIDI keyboard

Bunker connects to every MIDI keyboard it finds, also when you plug one in later.

| On the keyboard | Does |
|---|---|
| Keys **below middle C** | Play the sounds: C kick, D clap, E rim, F hat, G open hat, A ride, B bass (black keys fill in) |
| Keys **from middle C up** (hold) | Transpose bass and synth live – C4 is the original key |
| Mod wheel | Acid |
| Pitch bend | Filter |
| Sustain pedal | Echo |
| Knobs CC 70–73 (Akai MPK mini default) or CC 74/71/91/93 | Energy, Dark, Acid, Space |

## ⌨️ Keys

| Key | Does | | Key | Does |
|---|---|---|---|---|
| <kbd>Space</kbd> | Play / stop | | <kbd>Q</kbd> <kbd>W</kbd> <kbd>E</kbd> (hold) | Stutter, echo, wash |
| <kbd>Enter</kbd> | Shuffle a new groove | | <kbd>X</kbd> <kbd>C</kbd> <kbd>V</kbd> (hold) | Crush, gate, tape stop |
| <kbd>A</kbd> … <kbd>K</kbd> | The eight pads | | <kbd>B</kbd> | Build-up and drop |
| <kbd>1</kbd> … <kbd>8</kbd> | Mute / unmute a track | | <kbd>R</kbd> | Record on / off |
| <kbd>↑</kbd> / <kbd>↓</kbd> | Tempo | | <kbd>Tab</kbd> | Orbit ↔ pads |
| <kbd>⌘</kbd>/<kbd>Ctrl</kbd> + <kbd>Z</kbd> | Undo (with <kbd>Shift</kbd>: redo) | | <kbd>Esc</kbd> | Close a window |

## ⬇️ Install

| Platform | Download | First start |
|---|---|---|
| macOS 11+ (Apple Silicon & Intel) | [`Bunker-macos-universal.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-macos-universal.zip) | Unzip, move to Applications. Not notarized yet: right-click → *Open*, or *System Settings → Privacy & Security → Open Anyway* |
| Windows 10/11 | [`Bunker-windows-x64.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-windows-x64.zip) | Unzip and run `Bunker.exe` – no installation, no admin rights. SmartScreen may ask once: *More info → Run anyway* |

Checksums are in `SHA256SUMS.txt` on every release. Your beat, scenes and settings are saved automatically.

## 🛠️ Build it yourself

You need [Rust](https://rustup.rs). On Linux also `libasound2-dev` and `libgtk-3-dev`.

```bash
cargo run --release                       # start Bunker
cargo test --release                      # tests
cargo test --release render_ -- --ignored # screenshots + demo loops in target/snapshots/
./scripts/build-mac.sh --install          # build Bunker.app and copy it to /Applications
```

How the code fits together: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 🙏 Credits

Built with [egui](https://github.com/emilk/egui), [cpal](https://github.com/RustAudio/cpal) and [midir](https://github.com/Boddlnagg/midir). Fonts: [Space Grotesk](https://github.com/floriankarsten/space-grotesk) and [Anton](https://github.com/googlefonts/AntonFont) (SIL Open Font License), icons: [Phosphor](https://phosphoricons.com).

Made by Tom, together with Claude. MIT licensed.
