# Changelog

All changes to jQ Player that users notice, newest first. The text of each
version is also its GitHub release text.

<!--
New entries go under "Unreleased", sorted into "### Added", "### Changed",
"### Removed" and "### Fixed", one line per change, written for users (what is
different in the app, not how the code changed). The Release workflow turns
"Unreleased" into the new version's section (see scripts/changelog.mjs).
-->

## Unreleased

### Added

- Web remote (Settings › Web remote): tablets, phones and other computers on the show network open the
  show in a browser, without installing anything, and see the next cue, the running cues with their
  times, the cue list, the notes and the stopwatches. A new device asks to join by name; on the jQ
  Player computer it is let in to watch or to operate (GO, stops, playhead, stopping
  cues, stopwatches), and can be changed or removed at any time. Nothing can be edited remotely. The
  page adapts to phones and tablets, says clearly when the connection is lost, has a lock against
  operating by accident and takes the global keys of a connected keyboard, or keys of its own set
  on the device (kept on the jQ Player computer, which can reset them). Devices the jQ Player
  computer allows to can preview a sound cue on themselves, sounding as the preview there (by
  default only while no cue runs). Off by default; the address is shown with a QR code. The connection is not encrypted: use it only on a trusted network.

## 0.3.0 – 2026-11-17

### Added

- "Keep device awake" (Settings › Device) plays an inaudible signal with an adjustable level, so
  notebook speakers and other devices that switch off in silence no longer cut off the start of a cue.
- Clicking the audio device in the header opens a menu to change the host, device, channel count and
  buffer size directly; each change takes effect at once. In a locked show mode it only shows the
  device; it can then be changed only in the settings.
- Audio files dragged from Explorer or a file manager onto the cue list become sound cues where they
  are dropped (in edit mode), as if picked with "+ Sound".
- WAV files that hold MP3 or MP2 audio (as broadcast software and some audio editors write them) play
  like any other file instead of being refused as an unsupported format.
- "Add audio" also offers MP2, CAF and Ogg FLAC (.oga) files and the sound of MP4 and MOV videos.
- WAV files larger than 4 GB (RF64/BW64, as field recorders and broadcast software write them) and
  Wave64 files (.w64) play instead of being refused.
- Opus files (.opus, and .ogg files holding Opus) play, also in 5.1.
- A button next to the audio device list (Settings › Device and the header's audio menu) searches
  for devices again.

### Changed

- A file in a format that cannot be played (WMA, AC-3, DTS, HE-AAC or surround AAC, WavPack and
  others) is refused when it is added, naming the format, instead of failing only when the cue loads;
  MIDI files are explained.
- Disabled buttons, fields and menu entries show a "not allowed" mouse pointer.
- The opacity of an overlay panel now fades its text and controls too, not only its background. It
  goes down to 5 % in 1 % steps, and the panel turns solid while the mouse is over it or over a menu it opened.

### Fixed

- An audio or preview device that is unplugged shows in the device list by its name instead of a
  cryptic id like "wasapi:{0.0.0.00000000}.{…}" (once it has been used since this update).
- Clicking a disabled input field no longer puts a blinking text cursor in it.
- Windows: an audio device plugged in while the program runs is listed without restarting the program.
- Windows: with "Default device", the output moves to the new system default device as soon as it
  changes, and the header names the new device. If the device in use is disconnected, it is reported
  as lost and the output moves on about a second later. Before, opening the default device could
  fail with "the thread mode cannot be changed after it is set" until the program was restarted.
- An audio device that is disconnected or stops working is reported in one message, not also with a
  technical error such as "Failed to get current padding".
- The chain line drawn through a scene marker no longer stays on screen while the marker sticks at the
  top of the cue list after the chain has scrolled away.

## 0.2.1 – 2026-11-15

### Added

- Copies installed from the deb or MSI package update themselves like the AppImage and the setup
  ("Install update" in the About dialog; for the deb it asks for the password).

### Changed

- Updates can be installed in show mode as long as no cue is running; only a locked show mode
  prevents them.

### Fixed

- The update found at start was not announced when the program started in show mode, and the About
  dialog did not know about it either. The notice now also comes in show mode, and the About dialog
  offers the update right away.

## 0.2.0 – 2026-11-15

### Added

- Portable Windows version (`-portable.exe`): one program file that keeps its settings and caches in a
  "jQ Player Data" folder next to itself. It does not update itself; replace the file instead.
- "Trim start" and "Trim end" in the waveform dialog set the start or end to where the sound becomes
  audible or falls silent.
- Volume, pan, rate and transposition in the inspector can switch from the slider to a number field
  for exact values.
- The waveform editor has its own undo and redo (Ctrl+Z / Ctrl+Y) for the changes made in it.
- A shortcut that jumps to the "Go to" field (Ctrl+G by default, under "Cue list" in the shortcut
  settings).

### Changed

- "Exit loop" on a cue with a counted loop now ends the loop early: the current pass plays to its end
  and the cue ends normally. A later jump back resumes the loop with the passes it had left.
- MIDI program numbers are shown as 0–127 everywhere (triggers, outgoing messages, cue list).
- Learned MIDI controls are shown in a short form ("C4 · Ch 1"), the full text in a tooltip.
- The warning about files not loaded in time (silence or a late start from a slow disk) is now a
  setting under Settings → Audio device and off by default; then it is only reported when a cue
  started by GO starts late. With it on, cues started by hotkey or OSC are reported too. Dropouts of
  the audio device are always reported.
- The Esc setting (Settings → Operation) now applies to all menus, lists and popovers, not only to
  dialogs. When Esc closes a question, it answers it with its "Cancel".
- "Save as" is not available in show mode.
- The "Go to" field sits in the same place in edit and show mode.

### Fixed

- Playback
  - Streamed MP3, AAC and Ogg Vorbis files no longer click about 2.5 s into the cue or after a jump,
    and Ogg Vorbis files no longer play slightly late after a jump.
  - A position cue jumping far into a streamed file no longer lands on a part not yet loaded and plays
    silence.
  - A chain of cues that loops back on itself with no delay (follow plus GoTo or Start) no longer
    freezes the show.
  - A fade to a level on a cue that was edited to silence is no longer too loud later.
  - A playlist whose last item cannot be played still goes on to the cue that follows it.
  - The time left is right for cues started from a position.
  - Stopwatches and other running state survive reopening the audio device.
  - Cues with a changed rate or transposition: a seek back after their end, seeks while paused and
    stopping while a pause fades out work correctly.
  - A jump on the last pass of a counted loop keeps that pass.
  - Mono audio devices work, and the output recovers after a stall.
  - On Windows a streamed file that is not playing is closed, so it can be replaced or deleted.
- Editing
  - Shortcuts no longer act on the cue list behind an open dialog or menu; Delete in the group dialog
    removes the cue selected there, and undo/redo in other dialogs no longer change the show behind
    them.
  - Text being typed is kept when saving (Ctrl+S, without the focus flickering), when the selection
    moves to another cue, when closing the program or a panel window and when opening or creating
    another show; the question about unsaved changes counts it.
  - Number fields: an unreadable entry ("1-2") is reset instead of staying, it no longer clears an
    optional field, and fields that take no negative values ignore the minus key.
  - Numbers are formatted for the chosen language (decimal comma in German), and times never show as
    "1:60".
  - New marks and vamps stay inside the file; "+ Vamp" is disabled when there is no room left.
  - Learning a MIDI trigger is cancelled when another cue is selected.
  - The preview button's tooltip shows the key actually assigned.
  - Normalizing a newly added file no longer adds an undo step of its own or clears redo, and levels
    no longer change during a performance.
  - Cleaning up the media folder keeps files that undo could still bring back.
  - Problem checks pick the cue that "previous" refers to the same way playback does.
- Operation
  - In show mode, GO and panic work while the waveform dialog is open.
  - The pan slider is locked in show mode like the other controls.
  - The cue list no longer scrolls back to the playhead on every change while you look elsewhere.
  - Buttons reached with Tab take Space, also in the waveform dialog; tooltips no longer take buttons
    out of the Tab order.
  - Drags of panels and list items no longer get stuck when the window loses focus (Alt+Tab).
  - The media menu closes after opening or choosing a media folder.
  - Settings can be changed while cues run after the audio device was lost.
  - Long settings tab names no longer push the tab markers out of view.
  - Several texts that stayed English in the German interface are translated.
- Files and system
  - Settings and autosaves are written in the background, so saving no longer holds up the program;
    a failed write is reported.
  - A settings file that cannot be read is backed up before it is replaced.
  - A show that cannot be opened at startup is reported instead of only showing the start screen.
  - On Windows, folders and files are stored with plain paths (no `\\?\` prefix).
  - "Save as" and installing an update check again that the show is not in use before they go
    ahead.

## 0.1.0 – 2026-11-11

First release.
