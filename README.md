# ⭕ Bunker

**Techno beats in orbit.** A drum machine for people who have never used one: every sound is a glowing ring, every step a dot, a beam of light circles like a radar. Press play, roll a groove, turn four big knobs – or switch on the auto mode and let Bunker play a whole set with breakdowns and drops.

For **macOS** (Apple Silicon & Intel) and **Windows**. Written in Rust, every sound synthesized live – no samples, no account, no internet.

<p align="center">
  <a href="https://github.com/tj537/bunker/releases/latest/download/Bunker-macos-universal.zip"><img src="https://img.shields.io/badge/%EF%A3%BF%20macOS-Download-ff3b46?style=for-the-badge" alt="Download for macOS" height="44"></a>
  <a href="https://github.com/tj537/bunker/releases/latest/download/Bunker-windows-x64.zip"><img src="https://img.shields.io/badge/%E2%8A%9E%20Windows-Download-c8ff3d?style=for-the-badge&labelColor=333" alt="Download for Windows" height="44"></a>
</p>

![Bunker: the orbit while playing](docs/images/orbit.jpg)

> 🇩🇪 **Kurz auf Deutsch:** Bunker ist eine Techno-Drum-Machine zum Ausprobieren, ganz ohne Vorwissen. Jeder Sound ist ein leuchtender Ring, jeder Punkt ein Schritt – klicken, und es groovt. *Shuffle* würfelt passende Kombis, der **Auto-Modus** spielt selbstständig ein ganzes Set mit Breakdowns und Drops. Dazu Bass & Acid (303-Style), Live-Effekte, WAV-Export und Unterstützung für MIDI-Keyboards. Läuft auf Mac und Windows, alles wird live synthetisiert.

## ✨ What it does

- **The orbit** – eight rings: kick, clap, rim, hat, open hat, ride, bass and acid. Click a dot to switch a step on or off, drag to paint, right-click for an accent. Give a ring fewer steps and it drifts against the others – polyrhythms you can *see*.
- **Pads** – press <kbd>Tab</kbd> for eight big pads and a classic step grid. Play with the mouse or the keys <kbd>A</kbd>–<kbd>K</kbd>, hit **RECORD** and your hits land on the nearest step.
- **Five vibes** – Minimal, Hypnotic, Peak time, Acid, Industrial. Each click rolls a new groove that follows the rules of the genre: four-to-the-floor kick, claps on 2 and 4, off-beat hats, a rolling bass that dodges the kick.
- **Auto mode** – Bunker changes one thing every few bars (a new hat pattern, the acid line coming in, a shifted rim …), and every 32 bars it plays a breakdown, builds up and drops. Leave it running.
- **Four macros** instead of a hundred knobs: **Energy** (busier, harder), **Darkness** (longer kick, rumble, darker sound), **Acid** (the squelch), **Space** (reverb & echo).
- **Live** – a filter that springs back when you let go, **Echo** and **Stutter** to hold, and the **DROP** button: a build-up with a clap roll and a noise riser, then everything hits on the next bar.
- **Bass & acid** – a punchy rolling bass and a 303-style acid line with accents and slides, both in the song's key. Scroll on a dot to change its note.
- **Export WAV** – 4 to 64 bars, 44.1 kHz stereo, auto mode included.
- **MIDI keyboard** – just plug it in (see below).

![Pads and step grid](docs/images/pads.jpg)

## 🎹 MIDI keyboard

Bunker connects to every MIDI keyboard it finds, also when you plug one in later.

| On the keyboard | Does |
|---|---|
| Keys **below middle C** | Play the sounds: C kick, D clap, E rim, F hat, G open hat, A ride, B bass (black keys fill in) |
| Keys **from middle C up** (hold) | Transpose bass and acid live – C4 is the original key |
| Mod wheel | Acid |
| Pitch bend | Filter |
| Sustain pedal | Echo |
| Knobs CC 70–73 (Akai MPK mini default) or CC 74/71/91/93 | Energy, Darkness, Acid, Space |

## ⌨️ Keys

| Key | Does |
|---|---|
| <kbd>Space</kbd> | Play / stop |
| <kbd>Enter</kbd> | Shuffle a new groove |
| <kbd>A</kbd> … <kbd>K</kbd> | The eight pads |
| <kbd>1</kbd> … <kbd>8</kbd> | Mute / unmute a track |
| <kbd>B</kbd> | Build-up and drop |
| <kbd>Q</kbd> / <kbd>W</kbd> (hold) | Stutter / echo |
| <kbd>R</kbd> | Record on / off |
| <kbd>Tab</kbd> | Orbit ↔ pads |
| <kbd>↑</kbd> / <kbd>↓</kbd> | Tempo |

In the orbit: **click** a dot to switch it, **drag** to paint, **right-click** for an accent (on the acid ring the next right-click adds a slide), **shift-click** makes a step play only sometimes (75 → 50 → 25 %), **scroll** on bass and acid dots changes the note.

## ⬇️ Install

| Platform | Download | First start |
|---|---|---|
| macOS 11+ (Apple Silicon & Intel) | [`Bunker-macos-universal.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-macos-universal.zip) | Unzip, move to Applications. Not notarized yet: right-click → *Open*, or *System Settings → Privacy & Security → Open Anyway* |
| Windows 10/11 | [`Bunker-windows-x64.zip`](https://github.com/tj537/bunker/releases/latest/download/Bunker-windows-x64.zip) | Unzip and run `Bunker.exe` – no installation, no admin rights. SmartScreen may ask once: *More info → Run anyway* |

Checksums are in `SHA256SUMS.txt` on every release. Your beat and settings are saved automatically.

## 🛠️ Build it yourself

You need [Rust](https://rustup.rs). On Linux also `libasound2-dev` (and the usual egui libraries).

```bash
cargo run --release             # start Bunker
cargo test --release            # tests
./scripts/build-mac.sh --install   # build Bunker.app and copy it to /Applications
```

How the code fits together: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## Credits

Built with [egui](https://github.com/emilk/egui), [cpal](https://github.com/RustAudio/cpal) and [midir](https://github.com/Boddlnagg/midir). Fonts: [Space Grotesk](https://github.com/floriankarsten/space-grotesk) and [Anton](https://github.com/googlefonts/AntonFont) (SIL Open Font License), icons: [Phosphor](https://phosphoricons.com).

Made by Tom, together with Claude. MIT licensed.
