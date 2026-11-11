# jQ Player

A free, native cue player for theatre sound on Linux and Windows: cue list, GO/STOP ALL/FADE ALL,
sound and control cues, **sample-accurate vamping** and **portable show folders**. The app comes
in English and German.

## Features

### Operation

- **GO, STOP ALL, FADE ALL, PAUSE ALL** from the transport bar, keyboard, MIDI or OSC. GO reacts
  immediately and never waits for the UI; a GO lockout drops a second GO that comes too soon.
- **Panic (Esc):** fades everything out, a second Esc stops at once.
- **Edit and show mode:** show mode locks editing and can itself be locked in the show.
- **Playhead** with a next-cue display; selection and playhead separate or coupled.
- **Flow:** pre-wait and a follow cue that starts the next cue at a chosen point (with the start,
  after a time, after the fade-in, before the fade-out or end, after the end, on the vamp exit).
- **Triggers per cue:** hotkey (optionally re-triggering on release, e.g. "play while held"),
  MIDI trigger, and a second-trigger action while the cue runs (pause, stop, fade out, restart,
  exit loop, next item).
- **Other running cues** on start: leave alone, fade out (exclusive) or duck; set per show and per cue.
- **Configurable keys:** up to two keys and a MIDI command per action (transport, playhead,
  selected cue, jumps, level/pan/rate steps).
- **Running cues** panel with elapsed/remaining time, level, loops and fades; rehearsal jumps
  back, forward and to just before the end.
- **Stopwatches and countdowns** of the show in the header, controlled by click, cue or OSC.
- **Panels** can be moved and resized, placed in their own window or as an overlay, with separate
  layouts for edit and show mode.
- Keep-alive (no sleep or screen saver while a show is open), focus warning, autosave with crash
  recovery, single instance, `.jqshow` file association, optional update check.

### Editing

- Drag and drop, multi-selection, copy/cut/paste, duplicate, undo/redo, renumber; new cues get
  fitting numbers. Scene markers structure the list.
- **Inspector** for one or several cues (common properties are edited together).
- **Linked cues** share their settings (e.g. the same jingle in several places).
- **Waveform** in the inspector and a large waveform editor: drag start, end, fades, vamps and
  marks, audition on the preview output.

### Cue types

| Type | Function |
|---|---|
| **Sound** | One file: level, pan, range, fades with several curves, loop, vamps and marks, rate 50–150 % with preserved or following pitch, transposition. |
| **Playlist** | Several files in a row (in order or shuffled) with gap, fades, repeat and per-item overrides. |
| **Group** | Nested cues, all at once or one after another. |
| **Start/Stop, Pause** | Start, stop, pause or resume target cues. |
| **Volume, Pan, Rate, Reset** | Change level, pan or rate of running cues over a time, or bring them back. |
| **Position** | Jump within running cues: to a time, by a time, or to a mark. |
| **Exit loop** | Release the vamp or loop of a running cue: at the loop end, the next mark or at once. |
| **GoTo** | Move the playhead to another cue, optionally starting it. |
| **Wait** | A timed pause in a chain. |
| **Stopwatch** | Start, stop, reset or restart a stopwatch. |
| **OSC** | Send OSC messages to a network device of the show. |
| **MIDI** | Program change, control change, notes and MIDI Show Control. |
| **Scene** | A heading in the cue list; playback passes over it. |

### Audio

- Linux: PipeWire, JACK, ALSA; Windows: WASAPI and optionally ASIO.
- **Named outputs** of the show, mapped to channel pairs per computer; a **preview output** on its
  own channels or device.
- Normalisation to a target loudness (EBU R128, true peak) and per-show level limits; the files
  stay unchanged.
- Long files are streamed and upcoming cues preloaded. If the device fails, cues keep running
  silently and the device is reopened.

### Shows and media

A show is a folder (`<name>.jqshow` plus `media/`) with relative paths and can be copied to another
computer. Imported files are copied into the media folder (or linked). Missing files and changes
on disk are detected; "Relink", "Save as" (copy or link media) and "Clean up media folder" help keep
it tidy. Machine-specific settings (audio device, channel mapping, keys, MIDI/OSC, language) live
in the app's config folder (`~/.config/de.jqplayer.app/`, `%APPDATA%\de.jqplayer.app\`), not in
the show.

### Remote control

- **MIDI in:** learnable commands per action and triggers per cue, plus MSC (GO, STOP, RESUME,
  ALL_OFF). Handled in the backend, so it works even if the UI hangs.
- **OSC in** (UDP, port adjustable):

  | Address | Effect |
  |---|---|
  | `/go`, `/stop`, `/panic`, `/fade`, `/pause`, `/resume`, `/pausetoggle` | Global transport |
  | `/playhead/next`, `/playhead/previous`, `/select/{no}` | Move or set the playhead |
  | `/cue/{no}/start`, `stop`, `fade`, `pause`, `resume`, `pausetoggle`, `restart`, `exitloop`, `playhead` | Act on one cue |
  | `/cue/{no}/volume dB [s]`, `pan`, `rate`, `transpose`, `reset` (`…/relative` for volume and rate) | Change a running cue |
  | `/cue/{no}/position s`, `/cue/{no}/position/relative s`, `/cue/{no}/mark n` | Jump within a cue |
  | `/stopwatch/{name}/start`, `stop`, `reset`, `restart` | Control a stopwatch |

  A 0 argument (button release) is ignored for addresses without a value.

### Default keys

Space GO · Esc panic · F FADE ALL · Pause toggle pause all · K pause selected cue · P preview ·
↑/↓/Home/End selection (Ctrl+↑/↓ by scene) · Del delete (edit) / stop (show) ·
Ctrl+C/X/V/A, Ctrl+Z/Y, Ctrl+S/Shift+S, Ctrl+N/O as usual. Everything except the fixed keys can be
changed in the settings.

## Code structure

| Crate | Contents |
|---|---|
| `crates/jq-engine` | Real-time mixer of voices (no cue logic): ramps, loops/vamps, varispeed and pitch shift, lock-free control. Sample-accurately tested. |
| `crates/jq-stretch` | Wrapper around Signalsmith Stretch (C++) for time stretch and pitch shift. |
| `crates/jq-media` | Decoding (symphonia), resampling, streaming readers, waveform peaks. |
| `crates/jq-show` | Show model, show folders, media handling, editing with undo/redo. |
| `crates/jq-core` | Show controller (`Runner`) with all cue logic, preload cache, OSC, MIDI mappings, peak cache, file watching, machine settings. |
| `crates/jq-audio` | Device output via cpal with device-loss recovery. |
| `crates/jq-midi` | MIDI in/out via midir. |
| `crates/jq-i18n` | Backend texts (`t!`) and language setting. |
| `app/` | Tauri 2 backend (`src-tauri`) and Svelte 5 UI (`src`). The UI ↔ backend contract is [app/src/lib/api.ts](app/src/lib/api.ts). |

## Building

Prerequisites: Rust ([rustup](https://rustup.rs)) and Node.js. On Linux additionally:

```sh
sudo apt install libasound2-dev libjack-jackd2-dev libpipewire-0.3-dev clang pkg-config
```

plus the [WebKitGTK packages for Tauri](https://tauri.app/start/prerequisites/#linux). On Windows:
Rust with MSVC and LLVM; for ASIO also the [ASIO SDK](https://www.steinberg.net/asiosdk) in
`CPAL_ASIO_DIR` and the feature `asio` (`npm run tauri build -- --features asio`).

```sh
cargo test                     # Rust tests
cd app && npm install
npm run app                    # development mode
npm run tauri build            # installers (Windows: NSIS + MSI, Linux: deb + AppImage)
```

UI checks: `npm run lint`, `npm run format`, `npm run check`, `npm test`.

If the window stays blank or flickers on Linux, start with `WEBKIT_DISABLE_DMABUF_RENDERER=1`.

## Licence

jQ Player is free software under the **GNU General Public License, version 3 or later**
([LICENSE](LICENSE)). Bundled libraries are listed in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)
and in the About dialog; both are generated by `cd app && npm run licenses`.

The ASIO build requires the ASIO SDK 2.3.4 or later, which Steinberg offers under the GPLv3.
"ASIO" is a trademark of Steinberg Media Technologies GmbH.
