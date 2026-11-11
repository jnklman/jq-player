# jQ Player

A free, native cue player for Linux and Windows for theatre operation: cue list,
GO/STOP/FADE ALL, sound and control cues, plus **sample-accurate vamping** and
**portable shows with relative media paths**. The program is available in English and German.

## Features

### Cue list and operation

- **GO, STOP ALL, FADE ALL** from the transport bar, keyboard, MIDI or OSC. GO reacts immediately
  and never waits for the UI; a held space bar triggers GO only once. A GO lockout (default 0.3 s)
  discards a second GO shortly after the first, wherever it comes from.
- **Panic (Esc):** fades everything out over the panic time (default 1 s); a second Esc during
  the fade stops immediately. Optionally, Esc closes an open dialog first.
- **Playhead** with a display of the next cue; double-clicking a cue sets the playhead.
  Selection and playhead can be kept apart or coupled (playhead follows selection, selection
  follows playhead, linked).
- **Pre-wait**, **auto-continue** (the next cue starts together with this one) and
  **auto-follow** (the next cue starts when this one has finished).
- **Hotkey per cue:** one character starts the cue regardless of the playhead.
- **Edit and show mode:** In show mode editing is locked; GO and STOP ALL work even while an
  input field has focus. Show mode can be locked in the show; the switch to edit mode is then
  hidden until it is unlocked in the show menu.
- **Editing:** drag and drop, multi-selection (Shift+arrows, Home/End), copy, cut, paste and
  duplicate, undo/redo, "Renumber". New cues get fitting numbers: 3 between 2 and 5, 2.5
  between 2 and 3.
- **Inspector** for the selected cue; with several cues selected, their common properties are
  edited at once.
- **Linked cues:** instances of a cue share all settings except number, title, script
  reference, notes, hotkey and continue mode (e.g. the same jingle in several places). Created
  with "Link", linked duplicate or paste (Ctrl+Shift+V).
- **Running cues** with elapsed and remaining time, live level, loop indicator, playlist
  position and fade progress; stop and fade per cue. For rehearsals: jumps back and forward by
  an adjustable time and to just before the end (skipped vamps count as left).
- **Panels** (cue list, running cues, inspector, playhead cue with script reference and detail
  text) can be moved and resized, with a separate layout for edit and show mode.

### Cue types

| Type | Function |
|---|---|
| **Sound** | One audio file with volume, pan, start and end position, fade in/out (in dB down to the show's minimum level: slow at the top (default), slow at the bottom, S-curve, even; or in amplitude: linear, S-curve), end fade, loop (n times or endless), vamp ranges and marks. Rate 50–150 % with "keep pitch" (default) or varispeed (pitch follows) and transposition ±12 semitones; start, end, vamps and marks stay in file time, fades, duration and remaining time are in real time. |
| **Start/Stop** | Starts target cues without moving the playhead (paused ones continue, sound cues optionally from a position) or stops them including pre-wait and follow-up cues. |
| **Pause** | Pauses, resumes or toggles (pauses if any of them is still playing). |
| **Volume** | Fades target cues to a level, or fades out and stops; fixed duration or the target's fade-out time. |
| **Pan** | Moves the pan over a time (S-curve or linear); the next playlist item takes it over. |
| **Position** | Jumps in running cues: to a position in the file, back/forward, to a mark, to the next or previous mark; adjustable crossfade. A jump back before a left vamp makes it hold again. |
| **Exit loop** | Leaves the next vamp or loop (or all) of a running cue: at the loop end, at the next mark or immediately (50 ms crossfade). |
| **Rate** | Changes rate and/or transposition of running cues over a time (S-curve or linear); whether the pitch follows is determined by the target. The next playlist item takes it over. |
| **Reset** | Brings level, pan and rate of running cues back to their own values over a time. |
| **Stopwatch** | Starts, stops, resets or restarts a named stopwatch of the show. |
| **Wait** | Waits a fixed time, e.g. as a pause in an auto-follow chain. |
| **Group** | Nestable group: all cues start at the same time or one after the other according to their continue settings. Auto-follow waits until all have finished. |
| **Playlist** | Several files in sequence as one cue (house open, intermission), with a gap between items, fades and repeat. |
| **OSC** | Sends one or more OSC messages (int, float, string) to a network device of the show (e.g. a lighting desk). |
| **MIDI** | Program change, control change, note on/off and MIDI Show Control (GO, STOP, RESUME) to the MIDI output. |

### Sound cues, vamping and waveform

- **Sample-accurate vamping:** several vamp ranges per cue, repeated in order until an
  exit-loop cue releases them; transitions with a short crossfade.
- **Exclusive mode** (per show): GO on a sound cue fades out everything running over an
  adjustable time.
- **Normalisation:** loudness (EBU R128) and true peak of each file are measured while idle
  and cached next to the peaks. Sound and playlist cues can get a fixed offset to the show's
  target loudness (default −20 LUFS, true peak at most −1 dBTP), by hand or automatically on
  import; the file stays unchanged.
- **Level limits** (per show): no cue plays louder than the maximum level (default +6 dB),
  levels at the minimum (default −60 dB) are silent.
- **Waveform** in the inspector with draggable marks for start, end and vamp boundaries;
  double-click sets or removes a mark.
- **Waveform editor** (large window): zoom with Ctrl+mouse wheel and a detailed waveform,
  drag fade in/out, create vamps, audition from the cursor, from the start, just before the
  end, or start to end on the preview output (space bar).
- **Audition** (P) of a sound cue on the preview output, e.g. headphones, without touching the
  show. Volume and pan controls act live on running cues.
- **Timing warnings** if start, end, vamps or fades do not fit the file.
- Waveforms come from a peak cache on disk (at most 500 MB, can be emptied in the settings),
  which is precomputed while idle.

### Audio output

- Audio host and device selectable: on Linux PipeWire, JACK and ALSA, on Windows WASAPI and
  optionally ASIO.
- **Named outputs** of the show, mapped to any channel pairs of the device; the channel
  mapping is a machine setting, so the show stays portable.
- **Preview output** on its own channel pair or its own device.
- If the audio device fails (unplugged, driver reset), cues keep running silently and the
  device is reopened as soon as possible.
- Long files are streamed instead of loaded completely; the next cues are preloaded in the
  background so that GO starts without delay.

### Remote control

- **MIDI input** with learn: GO, STOP ALL, panic, FADE ALL, playhead next/previous can be
  assigned to notes or controllers. MIDI is processed in the backend and works even if the UI
  hangs.
- **Incoming OSC** (UDP, port adjustable):

  | Address | Effect |
  |---|---|
  | `/go`, `/stop`, `/panic`, `/fade` | GO, STOP ALL, panic, FADE ALL |
  | `/pause`, `/resume`, `/pausetoggle` | PAUSE ALL, RESUME ALL, toggle |
  | `/playhead/next`, `/playhead/previous` (also `/select/…`), `/select/{no}` | move or set the playhead |
  | `/cue/{no}/start`, `/stop`, `/fade`, `/playhead` | start, stop, fade out a single cue, set the playhead |
  | `/cue/{no}/pause`, `/resume`, `/pausetoggle`, `/restart`, `/exitloop` | pause, resume, restart a cue, exit loop |
  | `/cue/{no}/volume dB [s]`, `/cue/{no}/volume/relative dB [s]` | set or change the level, optionally with a fade time |
  | `/cue/{no}/pan −1…1 [s]`, `/cue/{no}/reset [s]` | set pan, reset level, pan and rate |
  | `/cue/{no}/rate % [s]`, `/cue/{no}/rate/relative points [s]`, `/cue/{no}/transpose st [s]` | set or change the rate, set the transposition |
  | `/cue/{no}/position s`, `/cue/{no}/position/relative s`, `/cue/{no}/mark n` | jump within the cue |
  | `/stopwatch/{name}/start`, `/stop`, `/reset`, `/restart` | control a stopwatch |

  An argument of 0 (key release) is ignored for addresses without a value; for `/volume`, `/pan` and
  `/position`, 0 is a value.

### Shows and media

- A show is a **folder with relative media paths** and can be copied as a whole to another
  computer.
- **Media import:** files from outside are copied into the media folder (in the background,
  with progress; the cue is there at once) or linked by absolute path. For names already taken
  the app asks (rename, use existing, skip) and recognises identical files.
- **Missing files** are marked, differing upper/lower case is detected; "Relink" looks for
  files of the same name in a folder.
- **Changes on disk** (file replaced, deleted) are detected and reported; on Windows the
  media files are write-locked in show mode.
- **Save as** creates a new show, with copied media or media linked to the previous media
  folder. The show folder can be renamed.
- **Clean up media folder:** show unused files and delete them after confirmation.
- **Autosave** with recovery after a crash; a prompt on unsaved changes, running cues or
  running copies when closing.
- Single instance; `.jqshow` files open by double-click in the file manager.

### Operation

- **Stopwatches:** named stopwatches of the show (e.g. performance, intermission) in the header,
  controlled by click, stopwatch cue or OSC. They run only in the session; STOP ALL and PAUSE ALL
  do not stop them.
- **Keep-alive:** computer and screen stay awake while a show is open (runs in the backend, even if
  the UI hangs).
- Adjustable FADE ALL time, jump distances for rehearsals (back, forward, before the end) and
  warning time before the cue end.
- **Last show at start:** off, open immediately or ask. If the app crashes during automatic
  opening, it asks at the next start.
- **Language:** English or German, "Automatic (system language)" by default with English as the
  fallback; chosen in the settings and stored in the machine settings.

### Keyboard shortcuts

| Key | Effect |
|---|---|
| Space | GO |
| Esc | Panic (fade out, twice: stop immediately) |
| F | FADE ALL |
| ↑ / ↓, Home / End | Move the selection (with Shift: select a range, edit mode only) |
| P | Audition the selected sound cue / stop auditioning |
| Del | Edit mode: delete the selected cues; show mode: stop the selected cue |
| Cue hotkey | Start the cue |
| Ctrl+C / X / V | Copy / cut / paste (Ctrl+Shift+V: paste linked) |
| Ctrl+A | Select all cues |
| Ctrl+Z, Ctrl+Y / Ctrl+Shift+Z | Undo, redo |
| Ctrl+S, Ctrl+Shift+S | Save, save as |
| Ctrl+N, Ctrl+O | New show, open show |

## Structure

| Crate | Contents |
|---|---|
| `crates/jq-engine` | Real-time mixer with no notion of cues: it plays voices from RAM or stream buffers and sums them onto channel pairs. Per-sample volume ramps (in dB linear, S-curve, slow at the top or bottom; in amplitude linear, S-curve), pan and live trim with smoothing, several loop/vamp ranges with a crossfade at the jump, devamp (at the loop end, at the next mark, immediately), end fade in the last pass, jumps in a running voice (back before a vamp: it holds again), pan ramps with a curve, stopping fades and pan movements. Playback rate: varispeed via band-limited sinc interpolation in the read head (bit-identical to direct playback at 100 %), pitch separate from rate via Signalsmith Stretch (`crates/jq-stretch`) from a pool allocated up front at start whose size follows from a short CPU measurement; switching without offset, pre-roll spread over several callbacks. Control and audio threads talk only through lock-free queues (`rtrb`); no allocation or locks in the callback, buffers are sent back to be freed. No device dependency, sample-accurately unit-tested. |
| `crates/jq-media` | Decoding (symphonia: WAV, FLAC, MP3, OGG, AAC/M4A …), stereo conversion, resampling to the device rate (rubato), determining lengths. Files up to 30 s are loaded completely into RAM, longer ones are streamed: a reader thread keeps about 14 s in blocks from each playback position, the start points of the upcoming cues and the loop/vamp ranges of running voices loaded. Waveform peaks of a whole file (up to 256 samples per value) and more precisely for excerpts up to 120 s. |
| `crates/jq-show` | Show model (common cue fields with hotkey and link; sound, wait, group, playlist, GoTo, OSC and MIDI cues and the control cues start/stop, pause, volume, pan, position, exit loop, reset and stopwatch with a shared target selection; outputs, network devices, exclusive mode, show mode lock). Show folders with `<name>/<name>.jqshow` (JSON), atomic saving, autosave and renaming. Relative media paths with `/`, checks for missing files and differing case, "Relink", copying into the media folder with progress and name conflicts, copying media for "Save as", finding and deleting unused media. Editing with undo/redo, linking, duplicating, numbering of new cues and renumbering; timing check against the file length. |
| `crates/jq-core` | Show controller (`Runner`) on top of the engine: playhead, GO/STOP ALL/FADE ALL, pre-wait, auto-continue/-follow, the logic of all cue types, exclusive mode, rehearsal jumps, preview. Keyboard, MIDI and OSC all end up as an `Action`. Also: a buffer cache that loads the files of the next cues in the background ("arm"); OSC over UDP (send and receive); MIDI mappings and learn; peak cache on disk (by file content, at most 500 MB, precomputed while idle); watching the media files for changes (polling, also on network drives) and a write lock in show mode (Windows only); machine settings; keep-alive. |
| `crates/jq-midi` | MIDI input and output via midir (ALSA, WinMM). Incoming messages go straight to the show thread, so MIDI control never depends on the UI; MIDI cues send through the output. `cargo run -p jq-midi --example monitor -- "<port>"` shows what a controller sends. |
| `crates/jq-audio` | Device output via cpal: host, device, channel count and buffer size selectable. On Linux PipeWire (with the real channel count of the default device), JACK (only if a server is running), ALSA and PulseAudio (only without PipeWire), on Windows WASAPI and optionally ASIO. A supervisor thread detects a lost device (unplugged, driver reset, callbacks stop), lets the mixer keep running silently in real time meanwhile and reopens the device. A silent output also serves as a fallback if no device opens. |
| `crates/jq-i18n` | Localised texts of the backend (errors, notices, action names): catalogs `locales/en.txt` and `locales/de.txt`, the `t!` macro, the language setting and detection of the system language (English as the fallback). The UI has its own catalogs in `app/src/lib/i18n`. |
| `app/` | Desktop app: Tauri 2 (`app/src-tauri`) and a Svelte 5 UI with Tailwind CSS v4 and shadcn-svelte (`app/src`). In the backend a show thread owns the session (show, editor, runner, audio, preview and MIDI devices, keep-alive, autosave, clipboard) and sends the state to the UI; playback and fades continue if the WebView hangs. Media work (file lengths, copying, searching for missing files, save as, cleaning up the media folder) runs in its own threads. Confirmation before closing, as a native dialog if need be; single instance, file association for `.jqshow`. |
| `crates/jq-stretch` | Thin layer over Signalsmith Stretch (MIT, via the `signalsmith-stretch` binding, C++): time stretch and pitch shift without allocation after creation, cost measurement for the pool size; `cargo run --release -p jq-stretch --example measure` measures latency and CPU. |

**UI ↔ backend interface:** [app/src/lib/api.ts](app/src/lib/api.ts) – all commands,
events (`show`, `state`, `notice`, `close_requested`, `open_requested`, `language_changed`) and types. The UI holds
no show state itself; it displays what the backend sends and sends commands.

## Building

Prerequisites: Rust (via [rustup](https://rustup.rs)) and Node.js. On Linux additionally
the ALSA, JACK and PipeWire development files, clang and the
[WebKitGTK packages for Tauri](https://tauri.app/start/prerequisites/#linux):

```sh
sudo apt install libasound2-dev libjack-jackd2-dev libpipewire-0.3-dev clang pkg-config
cargo test                 # all tests
cargo build --release
```

`jq-engine`, `jq-media`, `jq-show` and `jq-core` need only clang (libclang for bindgen) and a
C++ compiler (Signalsmith Stretch) of these.

On Windows you need Rust with MSVC and LLVM (libclang for Signalsmith Stretch); WASAPI is built in.
ASIO is the feature `asio` and additionally needs the ASIO SDK (`CPAL_ASIO_DIR`).

## Running the app

```sh
cd app
npm install
npm run app                    # development mode with hot reload
npm run tauri build            # installer package
```

`npm run app` removes the GTK variables that a VS Code installed as a snap passes on
to its terminals; without that, WebKitGTK crashes.

On Linux the UI is drawn on the graphics card (DMA-BUF). If the window stays blank or
flickers (once observed with NVIDIA under Wayland), start the app with
`WEBKIT_DISABLE_DMABUF_RENDERER=1`, e.g. `WEBKIT_DISABLE_DMABUF_RENDERER=1 npm run app`,
or put `env WEBKIT_DISABLE_DMABUF_RENDERER=1` in front of the command in the `Exec` line of the
installed app's launcher. The software path needs noticeably more CPU.

More commands in `app/`: `npm run lint` (ESLint), `npm run format` (Prettier, check only;
`npm run format:fix` writes), `npm run check` (types), `npm test` (Vitest).

### Building the Windows installer

On Windows with Rust (MSVC) and LLVM, see above:

```sh
cd app && npm run tauri build
# → target/release/bundle/nsis/jQ Player_<version>_x64-setup.exe
# → target/release/bundle/msi/jQ Player_<version>_x64_en-US.msi and …_de-DE.msi
```

The installers come in English and German (the MSI as one file per language; the NSIS
installer lets you choose).

With ASIO: unpack Steinberg's [ASIO SDK](https://www.steinberg.net/asiosdk) (GPLv3 or
Steinberg's own licence, not in the repo), set `CPAL_ASIO_DIR` to its folder
and build with `npm run tauri build -- --features asio`.

Runs on Windows 10 and 11. The installer works offline: it contains the
WebView2 installer (hence around 210 MB) and runs it only if WebView2 is missing.
It is unsigned (Windows SmartScreen warns).

`.github/workflows/build.yml` builds Windows (with ASIO) and Linux packages on GitHub Actions,
not on every push: a version tag (`git tag v0.2.0 && git push origin v0.2.0`, must match the
version in `tauri.conf.json`) publishes a release with the installers, "Run
workflow" in the Actions tab builds a test version as an artifact (kept for 5 days).

## Shows and settings

A show is a folder: the show file `<show name>.jqshow` (JSON) plus `media/`. After
"Save as" with linked media, the show points to the media folder of the old show by relative
path instead (on another drive by absolute path). Device-specific things
(audio device, outputs and channel mapping, OSC addresses, keep-alive, FADE ALL time, language) do
not belong to the show and are stored in the app's configuration folder (`settings.json`, Linux:
`~/.config/de.jqplayer.app/`, Windows: `%APPDATA%\de.jqplayer.app\`); what concerns only one
show is stored there under its ID, everything else applies to all shows. The peak cache for the
waveforms is in the app's cache folder (Linux: `~/.cache/de.jqplayer.app/peaks/`). Unsaved
changes are continuously backed up by the app in `<show name>.jqshow.autosave`; after a crash it offers
recovery when opening.

## Licence

jQ Player is free software under the **GNU General Public License, version 3 or later**
(see [LICENSE](LICENSE)). The licences of the bundled libraries are listed in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) and in the app's About dialog (app menu →
"About jQ Player"). Both are generated by `cd app && npm run licenses` from the dependencies
(Rust crates, npm packages); run it again after changing them, CI checks it.

Because of the GPL, the Windows version with ASIO may only be built and distributed with the
ASIO SDK version 2.3.4 or later, which Steinberg offers under the GPLv3; older SDK versions
are proprietary and not redistributable. "ASIO" is a trademark of Steinberg Media
Technologies GmbH.
