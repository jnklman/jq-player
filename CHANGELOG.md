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
