// Contract between UI and backend. The backend (Rust, app/src-tauri) owns
// all show state; the UI renders what it receives and sends commands.
//
// - Commands: the functions below (Tauri `invoke`).
// - Events:   `show` (ShowView, after every change), `state` (RuntimeState,
//             ~30×/s while a show is open), `notice` (Notice),
//             `close_requested` (CloseCheck, the window's × was clicked),
//             `language_changed` (LanguageInfo),
//             `open_requested` (path of a show file opened from outside,
//             e.g. in the file manager, while the program runs),
//             `panel_window_moved`, `panel_window_closed` (panel windows,
//             see `setPanelWindows`), `app_focus` (boolean).
//             `notice`, `close_requested`, `open_requested` and the panel
//             window events are meant for the main window only; panel
//             windows don't listen to them.
//
// Types mirror the Rust serde types in crates/jq-show/src/model.rs,
// crates/jq-core/src/runner.rs and app/src-tauri/src/session.rs.

import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

export type CueId = string;

/** Whether the next cue (its next sibling) starts by itself, and when. `at` is kept without `start_next`. */
export interface Advance {
  start_next: boolean;
  at: AdvanceAt;
}

/**
 * When the next cue starts. Offer only what suits the cue type; the backend reads a point that does not suit as
 * the nearest one that does (after_fade_in without a fade-in = start; end of an instant cue = start;
 * before_fade_out without an end fade = end). Instant cues: osc, midi, goto, start_stop, pause, position,
 * exit_loop, stopwatch.
 * - start: together with this cue (all types)
 * - after: `secs` after its start, also past its end (post-wait; all types)
 * - after_fade_in: once the fade-in is done (sound; playlist: the first item's)
 * - end: once it has ended (sound, playlist: last item, group: nothing inside runs any more, wait, volume, pan,
 *   reset)
 * - before_fade_out: when the end fade begins (sound with end_fade and a fade-out time)
 * - before_end: `secs` before the end (sound, playlist: last item, wait, volume, pan, reset); at the start if
 *   shorter
 * - loop_exit: when the target actually leaves its loop (exit_loop only): the first target to do so (`of` null) or
 *   the sound cue `of` inside the target (offer sound cues with vamps or an endless loop). Nothing running to
 *   leave = at once.
 */
export type AdvanceAt =
  | { at: 'start' }
  | { at: 'after'; secs: number }
  | { at: 'after_fade_in' }
  | { at: 'end' }
  | { at: 'before_fade_out' }
  | { at: 'before_end'; secs: number }
  | { at: 'loop_exit'; of: CueId | null };

/** Repeat: GOs the playhead stays on a top-level cue for; 0 (and 1) = off, "forever" = until moved on. */
export type Repeat = number | 'forever';

/** In dB, to and from silence via the show's `level_limits.min_db`: decibel evenly, decibel_s with a gentle
 * start and end (smoothstep), decibel_slow_top (default) slow near the louder level (a
 * fade-out holds, then falls; a fade-in settles gently), decibel_slow_bottom slow near the quieter one. linear and
 * s_curve: in amplitude. */
export type FadeCurve = 'decibel' | 'decibel_s' | 'decibel_slow_top' | 'decibel_slow_bottom' | 'linear' | 's_curve';
/** 0 = off, n = n repeats, "forever" = until stopped. */
/** Passes through start–end: 0 (and 1) = off, n ≥ 2 = n passes, "forever" = until stopped or devamped. */
export type LoopMode = number | 'forever';

interface CueBase {
  id: CueId;
  number: string;
  title: string;
  script_ref: string;
  notes: string;
  color: string | null;
  advance: Advance;
  /** Repeat (named so because playlists have `repeat`); only top-level cues. A GO on the cue while it runs with
   * second trigger "ignore" is no pass (`CueInfo.repeat_blocked` if it never ends by itself). */
  passes: Repeat;
  /** Seconds between GO and start. */
  pre_wait: number;
  /** Key that starts the cue regardless of the playhead (top-level cues only); several cues may share one, a press
   * starts them all (`CueInfo.hotkey_shared`). Refused on a fixed key (`ShowView.fixed_keys`) or a global shortcut. */
  hotkey: Chord | null;
  /** MIDI control that starts the cue like its hotkey (top-level cues only; velocity is ignored). Learned with
   * `midiLearn({ kind: "cue", cue })`; refused if bound globally (`settings.midi`). */
  midi_trigger: MidiTrigger | null;
  /** What a GO or direct start (hotkey, MIDI, OSC) does while the cue runs. */
  second_trigger: SecondTrigger;
  /** Releasing the hotkey or MIDI control fires `second_trigger` (send `release_cue` on keyup); with "stop" the cue
   * plays while the key is held. A press while it runs is an ordinary second trigger (while it fades out: a restart). */
  release_trigger: boolean;
  /** Effect on the other running cues when this one starts; null = the show's `others`. Only for cues with media
   * (sound, playlist, group); inside a group only a setting of its own applies. */
  others: OthersEffect | null;
  /** Cues with the same link are instances of each other: the backend keeps everything but
   * id, number, title, script_ref, notes, color, hotkey, midi_trigger, advance, passes, second_trigger,
   * release_trigger and others identical (pre_wait included). Absent = not linked. Only changed by link_cues/unlink_cues, duplicate and paste. */
  link?: string;
}

/** ignore: swallowed silently; pause: pause/resume in turn; fade_out: own fade-out time (FADE ALL time if 0);
 * restart: from the top; exit_loop: like a loop exit cue on it; next_item (playlists only, refused on other cues):
 * like the `next_item` action. A chain starting a running cue is no second trigger (it warns). stop and fade_out also
 * end the cue's post-wait. A GO or direct start on a cue that is fading out to a stop (fade_out, FADE ALL, panic,
 * exclusive) is no second trigger: it breaks the fade off and starts the cue again. */
export type SecondTrigger =
  | { action: 'ignore' }
  | { action: 'pause' }
  | { action: 'stop' }
  | { action: 'fade_out' }
  | { action: 'restart' }
  | { action: 'exit_loop'; exit: ExitLoop }
  | { action: 'next_item' };

/** How a loop is left (loop exit cue, second trigger). innermost: the next loop/vamp not left yet; all: all at once. */
export interface ExitLoop {
  mode: DevampMode;
  level: 'innermost' | 'all';
  /** The cue ends at the exit (10 ms blend) instead of playing on past the loop. */
  stop: boolean;
}

/** none: others play on; fade_stop: they fade out over `fade` and stop (exclusive); duck: they are lowered by
 * `duck_db` (−60…0) over `fade` while this cue runs, then come back (only cues running at its start; the
 * strongest duck wins). scope "output": only cues on the same output. Cues started by the same GO are never
 * affected. */
export interface OthersEffect {
  effect: 'none' | 'fade_stop' | 'duck';
  duck_db: number;
  /** Seconds, 0…600. */
  fade: number;
  scope: 'all' | 'output';
}

export interface SoundFields extends SpeedFields {
  type: 'sound';
  /** Relative to the media folder, `/`-separated. "" = placeholder (`add_placeholder`): a cue without a sound yet;
   * a GO plays nothing (warning „Platzhalter ohne Datei“) and its chain goes on as if it had ended at once. Give it a
   * file with `set_file`. Not `missing`, no `file_duration`. */
  file: string;
  output: string;
  /** A changed volume is capped at `show.level_limits.max_db`; at or below `min_db` silent; −120 = silence. */
  volume_db: number;
  /** Normalization offset added to `volume_db` when playing (not shown on the fader); absent = not normalized. */
  normalize_db?: number | null;
  pan: number;
  start: number;
  end: number | null;
  /** Ignored while the cue has vamps. */
  loop: LoopMode;
  /** Regions (seconds in the file) repeated until a loop exit cue releases them, in order. */
  vamps: Vamp[];
  /** Seconds in the file where a loop exit "next_mark" may leave a vamp; targets of position cues. */
  marks: number[];
  fade_in: number;
  fade_out: number;
  end_fade: boolean;
  curve: FadeCurve;
}

/**
 * How fast and at what pitch a sound or playlist cue plays (fields of `SoundFields` and `PlaylistFields`). Absent
 * fields have their default; the backend leaves defaults out. Start, end, vamps, marks and position cues stay in file
 * time; fades, "n s before the end" and remaining times are real time at the rate.
 */
export interface SpeedFields {
  /** Percent of the original speed, 50–150 (outside: clamped, reported in `checks`). Default 100. */
  rate?: number;
  /** The pitch stays when the rate changes; false: it follows the rate like a tape (varispeed). Default true. */
  preserve_pitch?: boolean;
  /** Semitones up or down, −12…+12, fractions allowed. Default 0. */
  transpose?: number;
}

export interface Vamp {
  start: number;
  end: number;
}

/**
 * What a control cue acts on: cue ids (a group: everything in it), "all" running cues, the nearest previous sibling
 * that can receive changes (a sound, playlist, or group containing one), or every cue running on one output. "all"
 * means all, also the control cue's own group. A start cue starts only chosen cues (or the previous target); "all"
 * and an output start nothing.
 */
export type Targets = CueId[] | 'all' | 'previous' | { output: string };

/** The control cue types (`add_control`, `set_cue_type`). */
export type ControlKind =
  'start_stop' | 'pause' | 'volume' | 'pan' | 'rate' | 'position' | 'exit_loop' | 'reset' | 'stopwatch';

export type VolumeAction = 'fade_to' | 'fade_stop';

/** Fades its targets to a level, or out and stops them. */
export interface VolumeFields {
  type: 'volume';
  targets: Targets;
  action: VolumeAction;
  /** Target level for "fade_to". */
  volume_db: number | null;
  /** null = the target cue's own fade-out time. */
  duration: number | null;
  curve: FadeCurve;
}

/** Starts its targets (not running: with pre-wait and chain, playhead untouched; paused: resumes them; running:
 * plays on with a warning, no second trigger) or stops them at once with what they would still start. */
export interface StartStopFields {
  type: 'start_stop';
  targets: Targets;
  action: 'start' | 'stop';
  /** Start: sound cues play from here (seconds in the file, within start–end) instead of their start. */
  from?: number | null;
}

/** toggle: pauses all targets if anything of them still plays, else resumes them. */
export interface PauseFields {
  type: 'pause';
  targets: Targets;
  action: 'pause' | 'resume' | 'toggle';
}

export type PanCurve = 'linear' | 's_curve';

/** Moves the balance of its targets over `duration` (0 = at once, smoothed); pausing it holds the move. The next
 * playlist item keeps the pan. */
export interface PanFields {
  type: 'pan';
  targets: Targets;
  /** −1 (left) … +1 (right). */
  pan: number;
  duration: number;
  /** Default s_curve. */
  curve: PanCurve;
}

/** Moves rate and transposition of its targets over `duration` (0 = at once, smoothed); pausing it holds the move.
 * Whether the pitch follows the rate stays each target's own `preserve_pitch`. The next playlist item keeps what it
 * set; a reset cue takes it back. A change to a cue keeping its pitch is heard about 0.15 s later (the stretcher's
 * latency). */
export interface RateFields {
  type: 'rate';
  targets: Targets;
  /** Percent; null leaves the rate as it is. Default 100. */
  rate: number | null;
  /** Semitones; null leaves the transposition as it is. Default null. */
  transpose: number | null;
  duration: number;
  /** Default s_curve. */
  curve: PanCurve;
}

/**
 * absolute: seconds in the file (like marks); relative: ahead (negative: back) of the current position; mark: the
 * cue's mark number `index` (from 1); next_mark / previous_mark from the current position (within 0.5 s after a
 * mark, previous goes to the one before). Clamped to start–end. A playlist takes only relative (in its item).
 */
export type PositionTo =
  | { to: 'absolute'; secs: number }
  | { to: 'relative'; secs: number }
  | { to: 'mark'; index: number }
  | { to: 'next_mark' }
  | { to: 'previous_mark' };

/** Jumps within running targets; a paused one stays paused, one not running is left alone. A jump back before a
 * vamp left already holds there again. */
export interface PositionFields {
  type: 'position';
  targets: Targets;
  to: PositionTo;
  /** Crossfade of the jump, ms (default 20; 0 = hard cut with the 10 ms stop blend). */
  crossfade_ms: number;
}

/** Brings level, pan, rate and transposition of its targets back to their cues' own (fade_to, pan and rate cues
 * undone) over `duration`; cues fading out to a stop are left alone. */
export interface ResetFields {
  type: 'reset';
  targets: Targets;
  duration: number;
}

/** A named stopwatch of the show (e.g. performance time, interval). Its time runs for the session only; STOP ALL,
 * FADE ALL, panic and PAUSE ALL leave it running. */
export interface Stopwatch {
  id: string;
  name: string;
  /** Seconds a countdown starts from (> 0, at most 100 h); it runs on past 0 into overtime (`remaining` negative).
   * Absent = counts up. */
  countdown?: number;
}

/** start: runs on from its time; stop: stands still; reset: to 0, standing; restart: from 0, running. */
export type StopwatchAction = 'start' | 'stop' | 'reset' | 'restart';

export interface StopwatchFields {
  type: 'stopwatch';
  /** Id of a show stopwatch; null = not set. */
  watch: string | null;
  action: StopwatchAction;
}

/** loop_end: finish the pass; next_mark: at the next mark; immediate: 50 ms crossfade now. */
export type DevampMode = 'loop_end' | 'next_mark' | 'immediate';

/** Tagged, so a float 1.0 stays a float although JavaScript has no such number. */
export type OscArg = { int: number } | { float: number } | { string: string };

export interface OscMessage {
  /** e.g. "/pb/1/go". */
  address: string;
  args: OscArg[];
}

export interface OscFields {
  type: 'osc';
  /** Id of a show device; null = not set. */
  device: string | null;
  messages: OscMessage[];
}

export type MscCommand = 'go' | 'stop' | 'resume';

/** Channels are 0-based (shown as 1–16), values 0–127. */
export type MidiMessage =
  | { kind: 'program_change'; channel: number; program: number }
  | { kind: 'control_change'; channel: number; controller: number; value: number }
  | { kind: 'note_on'; channel: number; note: number; velocity: number }
  | { kind: 'note_off'; channel: number; note: number }
  /** MIDI Show Control; device 127 = all; cue "" = the receiver's next cue. */
  | { kind: 'msc'; device: number; command: MscCommand; cue: string; list: string };

export interface MidiFields {
  type: 'midi';
  /** Sent to `settings.midi_out`. */
  messages: MidiMessage[];
}

/** Several files one after the other as one cue (walk-in, interval); rate and pitch apply to every item. */
export interface PlaylistFields extends SpeedFields {
  type: 'playlist';
  /** In playing order. */
  items: PlaylistItem[];
  output: string;
  volume_db: number;
  /** −1 (left) … +1 (right), for every item. */
  pan: number;
  fade_in: number;
  fade_out: number;
  /** Silence between items, seconds. */
  gap: number;
  repeat: boolean;
  shuffle: Shuffle;
  curve: FadeCurve;
}

/** One entry of a playlist: its file, its normalization and what it plays with instead of the playlist's settings
 * (e.g. one song quieter). Everything stays with the entry when it is moved; a file listed twice has entries of its
 * own. */
export interface PlaylistItem extends ItemOverrides {
  /** Relative to the media folder, `/`-separated. */
  file: string;
  /** Normalization offset added to the volume; absent = not normalized. Dropped when the entry gets another file
   * (`set_file`, `relink_file`); kept when a search relinks the same file. */
  normalize_db?: number | null;
}

/** What one playlist item plays with instead of the playlist's settings; a field left out (or null) keeps the
 * playlist's. Live edits move the item playing; a pan, rate or fade cue's value goes on to the next item as before.
 * Values out of range are clamped and reported in `checks` („Titel 2: …“). */
export interface ItemOverrides {
  /** dB added to the playlist's `volume_db` like a normalization offset, so the item stays this much louder or quieter
   * when the playlist's volume is changed or faded. Default 0. */
  gain_db?: number;
  /** Start position in the file, seconds (the playlist has none of its own: 0 = file start). Applies on the item's
   * next start, not to the item playing. Default 0. */
  start?: number;
  /** End position in the file, seconds; absent or null = end of file. Like `start`, not live. */
  end?: number | null;
  /** −1 (left) … +1 (right). */
  pan?: number | null;
  /** Percent, 50–150. */
  rate?: number | null;
  preserve_pitch?: boolean | null;
  /** Semitones, −12…+12. */
  transpose?: number | null;
  fade_in?: number | null;
  /** Fade time when the item is faded without an explicit time. */
  fade_out?: number | null;
}

/** Random playing order of a playlist. off: the list's; per_start: drawn anew at every start, a repeating playlist
 * keeps it for all passes; per_pass: drawn anew for every pass (a pass never starts with the item just played),
 * without repeat like per_start. The first item of the next start is drawn ahead, so it is armed. */
export type Shuffle = 'off' | 'per_start' | 'per_pass';

/** all: every cue starts on the group's GO (their advance settings do not apply);
 * sequence: the first starts, the others follow by their advance settings. */
export type GroupMode = 'all' | 'sequence';

/** Cues that run as one, edited in a dialog. Its advance "end" waits until all are done. */
export interface GroupFields {
  type: 'group';
  mode: GroupMode;
  cues: Cue[];
}

/** Network target of OSC cues. */
export interface NetDevice {
  id: string;
  name: string;
  host: string;
  port: number;
}

/** Leaves the loop or vamp of running targets (each one releases the next vamp); none running = no-op. */
export interface ExitLoopFields extends ExitLoop {
  type: 'exit_loop';
  targets: Targets;
}

/** A scene marker: a heading in the main list (act, scene) to structure it, only title, notes and colour count
 * (`add_scene`, no number). Playback passes over it: the playhead never stands on it (`set_playhead`, a go-to, MSC or
 * a GO's move go on to the first cue after it; playhead_prev back to the one before), a chain runs on to the cue
 * after it, starting it does nothing. `update_cue` refuses it inside a group and drops advance, passes, pre_wait,
 * hotkey, midi_trigger, second_trigger, release_trigger, others and link. */
export interface SceneFields {
  type: 'scene';
}

export interface WaitFields {
  type: 'wait';
  /** Seconds; with advance "end" the next cue starts afterwards. */
  duration: number;
}

/** Jumps in the list: selects (start false) or starts its target like a GO on it, the playhead going past the
 * target's chain. Targets only top-level cues (a group as a whole); `update_cue` refuses others. A deleted target
 * stays set and counts as missing (`CueInfo.target_missing`; firing reports „Ziel fehlt“, the chain goes on); so does a target inside a group, reported as
 * such. */
export interface GoToFields {
  type: 'goto';
  target: CueId | null;
  start: boolean;
}

export type SoundCue = CueBase & SoundFields;
export type VolumeCue = CueBase & VolumeFields;
export type ExitLoopCue = CueBase & ExitLoopFields;
export type StartStopCue = CueBase & StartStopFields;
export type PauseCue = CueBase & PauseFields;
export type PanCue = CueBase & PanFields;
export type RateCue = CueBase & RateFields;
export type PositionCue = CueBase & PositionFields;
export type ResetCue = CueBase & ResetFields;
export type StopwatchCue = CueBase & StopwatchFields;
export type WaitCue = CueBase & WaitFields;
export type OscCue = CueBase & OscFields;
export type MidiCue = CueBase & MidiFields;
export type PlaylistCue = CueBase & PlaylistFields;
export type GroupCue = CueBase & GroupFields;
export type GoToCue = CueBase & GoToFields;
export type SceneCue = CueBase & SceneFields;
export type Cue =
  | SoundCue
  | VolumeCue
  | ExitLoopCue
  | StartStopCue
  | PauseCue
  | PanCue
  | RateCue
  | PositionCue
  | ResetCue
  | StopwatchCue
  | WaitCue
  | OscCue
  | MidiCue
  | PlaylistCue
  | GroupCue
  | GoToCue
  | SceneCue;
/** Cues with `targets`. */
export type ControlCue = VolumeCue | ExitLoopCue | StartStopCue | PauseCue | PanCue | RateCue | PositionCue | ResetCue;

export interface Show {
  format: string;
  version: number;
  /** Stable identity; the machine settings for this show are filed under it. */
  id: string;
  /** Name shown in the app, independent of the file and folder names. */
  name: string;
  media_root: string;
  outputs: Output[];
  /** What a top-level cue with media does to the others when it starts, unless it has `others` of its own. */
  others: OthersEffect;
  /** Keeps the show in show mode until unlocked; the mode switch is hidden meanwhile. */
  show_mode_locked: boolean;
  /** Set with `set_level_limits`. */
  level_limits: LevelLimits;
  /** LUFS that `normalize` brings files to (−40…−6). Set with `set_normalization`. */
  loudness_target: number;
  /** New sound and playlist cues are normalized as soon as their files are measured. */
  normalize_on_import: boolean;
  devices: NetDevice[];
  /** Set with `set_stopwatches`. */
  stopwatches: Stopwatch[];
  cues: Cue[];
}

/**
 * Loudest level a cue plays at (volume plus normalization, also fade targets), and the level at or below which a
 * volume is silence; dB. Default +6 / −60.
 */
export interface LevelLimits {
  max_db: number;
  min_db: number;
}

/** What setting new level limits would change; cues inside groups by their own id. */
export interface LevelChanges {
  /** Brought to the new maximum. */
  capped: CueId[];
  /** Audible now, silent then (set to −120). What is silent already stays so, also when the minimum goes down. */
  silenced: CueId[];
}

/** Logical output; which channels it plays on is a machine setting (`LocalSettings.outputs`). */
export interface Output {
  id: string;
  name: string;
}

export interface OutputInfo {
  host: string;
  /** Short device name. */
  device: string;
  /** Full description (manufacturer, type, connection) for tooltips. */
  device_detail: string;
  sample_rate: number;
  channels: number;
  /** Most channels the device takes; null if it cannot tell. Outputs and the preview pair must stay below. */
  max_channels: number | null;
  sample_format: string;
  /** Fixed callback buffer in frames (`settings.buffer_frames`, or the host fallback); null = the host's default. */
  buffer_frames: number | null;
}

export type KeepAliveState = { state: 'off' } | { state: 'active' } | { state: 'failed'; error: string };

/**
 * Machine-specific, stored in the app's config folder, never in the show folder.
 * `outputs` and `osc_hosts` are kept per show, everything else applies to all shows.
 */
export interface LocalSettings {
  keep_awake: boolean;
  /** Editing is locked; edit commands fail. */
  show_mode: boolean;
  /** Audio host ("ALSA", "JACK", "WASAPI" …); null = system default. */
  host: string | null;
  /** Part of the device description or its id; null = system default. */
  device: string | null;
  channels: number | null;
  /** Output id → first channel (1-based). */
  outputs: Record<string, number>;
  /** FADE ALL time in seconds. */
  fade_all: number;
  /** Rehearsal jump steps and the "jump to end" lead time in seconds (> 0 each). */
  jump_back: number;
  jump_forward: number;
  jump_end: number;
  /** Seconds before its end from which a running cue shows the warning colour (≥ 0; 0 = never). */
  end_warning: number;
  /** What the mouse wheel does over the cue list, in edit and show mode alike. */
  wheel: WheelMode;
  /**
   * Fixed callback buffer of the audio device in frames: a power of two, 32–4096, within the device's
   * `buffer_range`; null = the host's default. Changing it opens the device again (refused while cues run).
   * Ignored by JACK, whose server sets the size for all clients.
   */
  buffer_frames: number | null;
  /** What Esc does while a dialog, menu or list is open. */
  escape: EscapeMode;
  /** Fade time of a panic (Esc) in seconds (≥ 0); 0 = stop at once. A second panic during the fade stops at once. */
  panic_fade: number;
  /** Shortest time between two GOs in seconds (≥ 0; 0 = off); a GO sooner is dropped (`go_rejected`). */
  go_lockout: number;
  /** Whether the last show is opened when the app starts. */
  open_last: OpenLast;
  /** The show opened last; set by the backend, `setSettings` leaves it alone. */
  last_show: string | null;
  /** Separate preview device (e.g. headphones); null with a preview_channel = main device. */
  preview_device: string | null;
  /** First channel (1-based) of the preview pair; both null = no preview. */
  preview_channel: number | null;
  /** MIDI input (part of its name); null = no MIDI. */
  midi_port: string | null;
  /** Learned with `midiLearn`; saved by the backend right away. */
  midi: MidiBinding[];
  /** MIDI Show Control device id (0–126) answered to, besides the all call (127); null = MSC off. Received: GO
   * (with a cue number: playhead to it, then GO), STOP (pauses), RESUME (each for one cue or all), ALL_OFF. */
  msc_device: number | null;
  /** Global shortcuts; at most two keys per action, no key twice, no fixed key (`setSettings` refuses). Esc is
   * always panic (fixed); keys bound to `panic` come on top of it. */
  keys: KeyBinding[];
  /** Steps of the +/− actions on the selected cue: dB, pan (−1…1), rate points (> 0 each). */
  volume_step: number;
  pan_step: number;
  rate_step: number;
  /** Show a clear warning while the window has no keyboard focus. */
  focus_warning: boolean;
  /** MIDI output for MIDI cues (part of its name); null = none. */
  midi_out: string | null;
  /**
   * UDP port for incoming OSC; null = off. Addresses: /go, /stop, /panic, /fade, /pause, /resume, /pausetoggle,
   * /playhead/next|previous (also /select/…), /select/{Q#}, /cue/{Q#}/start|stop|fade|playhead|pause|resume|
   * pausetoggle|restart|exitloop, /cue/{Q#}/reset [s], /cue/{Q#}/volume dB [s], /cue/{Q#}/volume/relative dB [s],
   * /cue/{Q#}/pan −1…1 [s], /cue/{Q#}/position s, /cue/{Q#}/position/relative s, /cue/{Q#}/mark n,
   * /cue/{Q#}/rate % [s], /cue/{Q#}/rate/relative points [s], /cue/{Q#}/transpose semitones [s],
   * /stopwatch/{name}/start|stop|reset|restart.
   */
  osc_listen: number | null;
  /** Show device id → "host:port" on this machine, overriding the show. */
  osc_hosts: Record<string, string>;
  /** How changes of media files on disk are noticed. */
  media_watch: WatchMode;
  /**
   * Language of the program: "auto" (the system language, English if that is not offered) or the code of an offered
   * language ("en", "de"). The backend's messages follow it at once; the UI takes `getLanguage` / `language_changed`.
   */
  language: string;
  /** Look for a newer release shortly after the start (`update_available` event); off by default. */
  check_updates: boolean;
  /** Folder the audio file picker starts in; null = the show's media folder. Set with `setAudioDir`. */
  audio_dir: string | null;
  /** Arrangement of the panels per mode (`{ edit, show }`, see `lib/layout.svelte.ts`; with the panel windows), stored as
   * sent; null = default.
   * Set with `setLayout`; `setSettings` leaves it alone. */
  layout: unknown;
  /** Set with `setSelectionMode`. */
  selection_mode: SelectionMode;
}

/** A panel window: a normal window, or an overlay that stays above other programs and is not in the taskbar. */
export type WindowKind = 'window' | 'overlay';

/**
 * A panel of the main view in a window of its own (label `panel-{panel}`), as kept in the layout. Position and size
 * in logical pixels; null: not known yet (the window opens in the middle of the main window's screen).
 */
export interface PanelWindow {
  panel: string;
  kind: WindowKind;
  x: number | null;
  y: number | null;
  width: number | null;
  height: number | null;
}

/**
 * What panel windows can be on this machine. On Wayland no program can keep its window above the others, but the
 * compositor's window menu can ("Always on top"): there panel windows get the system's title bar (`native_frame`)
 * and overlays are not offered (`overlay`); an overlay in the layout opens as a normal window.
 */
export interface WindowSupport {
  overlay: boolean;
  native_frame: boolean;
}

/** Where the operator moved or resized a panel window to. */
export interface PanelWindowRect {
  panel: string;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Mouse wheel over the cue list: scrolls it, moves the selection or moves the playhead. */
export type WheelMode = 'scroll' | 'selection' | 'playhead';

/**
 * Media files changing on disk: the system's file events (`events`, no load while idle, but blind to changes other
 * machines make on a network share) or looking at the files every second (`poll`). Either way `rescanMedia` looks
 * at them on request.
 */
export type WatchMode = 'events' | 'poll';

/** Esc while a dialog, menu or list is open: panics as well (`stop_all`) or only closes it (`close_first`). */
export type EscapeMode = 'stop_all' | 'close_first';

/**
 * On startup: nothing, open the last show right away (`last`), or offer it on the start screen (`ask`, see
 * `startupShow`). A show that crashed the app on the last start is only offered.
 */
export type OpenLast = 'off' | 'last' | 'ask';

/**
 * How the cue list's selection and the playhead go together: each on its own,
 * selecting a cue moves the playhead, the playhead's cue gets selected, or both.
 */
export type SelectionMode = 'separate' | 'playhead_follows' | 'selection_follows' | 'linked';

/** Channels are 0-based. A note is pressed by Note On and released by Note Off; a CC is pressed rising to ≥ 64 and
 * released falling below; a program change is only pressed. */
export type MidiTrigger =
  | { kind: 'note'; channel: number; note: number }
  | { kind: 'cc'; channel: number; controller: number }
  | { kind: 'program'; channel: number; program: number };

/** A key by its physical position (`KeyboardEvent.code`) and modifiers; Cmd counts as Ctrl. `label` is the key's text
 * when recorded (`KeyboardEvent.key`), shown where the current layout cannot be asked. Compared without the label. */
export interface Chord {
  code: string;
  ctrl: boolean;
  shift: boolean;
  alt: boolean;
  label: string;
}

/**
 * Global actions for keys and MIDI. The ones from `preview` on work on the selected cue: keys only (the UI sends the
 * matching `Action` with the selection); `midiLearn` refuses them.
 */
export type ControlAction =
  | 'go'
  | 'stop_all'
  | 'panic'
  | 'fade_all'
  | 'pause_all'
  | 'resume_all'
  | 'toggle_pause_all'
  | 'playhead_next'
  | 'playhead_prev'
  | 'stop_preview'
  | 'preview'
  | 'toggle_pause_cue'
  | 'stop_cue'
  | 'fade_cue'
  | 'restart_cue'
  | 'exit_loop_cue'
  | 'next_item'
  | 'jump_back'
  | 'jump_forward'
  | 'jump_end'
  | 'volume_up'
  | 'volume_down'
  | 'pan_left'
  | 'pan_right'
  | 'rate_up'
  | 'rate_down';

export interface KeyBinding {
  chord: Chord;
  action: ControlAction;
}

export interface MidiBinding {
  trigger: MidiTrigger;
  action: ControlAction;
}

/** What the next MIDI control is learned for: a global action or a cue's `midi_trigger` (an undoable edit). */
export type LearnTarget = { kind: 'action'; action: ControlAction } | { kind: 'cue'; cue: CueId };

export interface DeviceInfo {
  id: string;
  /** Short name for lists. */
  name: string;
  /** Full description (manufacturer, type, connection). */
  description: string;
  /** e.g. "48000 Hz, 2 Kanäle, F32"; null if it cannot be opened. */
  default_config: string | null;
  /** Most channels the device takes; null if it cannot tell. */
  max_channels: number | null;
  /** Smallest and largest callback buffer in frames; null if the host does not tell. */
  buffer_range: [number, number] | null;
}

export interface AudioDevices {
  hosts: string[];
  /** The listed host. */
  host: string;
  devices: DeviceInfo[];
}

export interface CueInfo {
  file_duration: number | null;
  /** Including loops, real time at the cue's rate; null if unknown or endless. */
  duration: number | null;
  missing: boolean;
  /** Per playlist item, in order: its file is missing (empty for other cues). */
  missing_items: boolean[];
  case_mismatch: string | null;
  /** Grows when the file changes on disk; reload views of it (waveform). */
  file_version: number;
  /** Positions and fade times that do not fit the file (sound cues). */
  timing: TimingWarning[];
  /** Per playlist item, in order: its `start`/`end` that do not fit its file (empty for other cues). */
  item_timing: TimingWarning[][];
  /** The file's length is still being read in the background; `file_duration` and `duration` follow. */
  probing: boolean;
  /** The file is still being copied into the media folder (progress: `RuntimeState.imports`); not `missing` meanwhile, it plays once done. */
  importing: boolean;
  /** Volume plus normalization is above `level_limits.max_db`; the cue (or a playlist item) plays at the maximum. */
  level_capped: boolean;
  /** The volume is below `level_limits.min_db` without being set to silence (−120), so the cue is silent; e.g. after
   * editing the show file by hand. */
  level_silenced: boolean;
  /** Further problems as messages: values only a hand-edited show file has (negative times, pan out of range, unknown
   * output), a hotkey or MIDI trigger that never fires (inside a group, fixed key, bound globally) or a release that
   * does nothing, a file that is there but no audio, a start cue's `from` past a target's end
   * or before its start (or on a target that is no sound cue), control-cue targets that act on nothing (none chosen,
   * deleted, no previous entry, unknown output, "all"/output on a start cue, a deleted or unset stopwatch) and start
   * cues starting themselves without a pre-wait, and position jumps a target cannot make (past its end, a mark it
   * lacks, anything but a relative jump on a playlist). */
  problems: string[];
  /** Per file (a playlist's in order); null until measured. */
  loudness: (FileLoudness | null)[];
  /** Some file's loudness is still being measured in the background (waits while cues play). */
  measuring: boolean;
  /** A stored offset is more than 0.5 dB off what the file needs now (file or target changed); `normalize` again. */
  normalize_stale: boolean;
  /** Go-to cue: its target was deleted or moved into a group. Control cue: no target left (all deleted).
   * Stopwatch cue: its stopwatch was removed. Loop exit cue: its loop_exit `of` is no longer in its target (it then
   * waits for the first exit). */
  target_missing: boolean;
  /** Repeat can never apply: second trigger "ignore" and the cue never ends by itself (endless loop, vamps,
   * repeating playlist). */
  repeat_blocked: boolean;
  /** Other top-level cues with the same hotkey (a hint: a press starts them all). */
  hotkey_shared: CueId[];
}

export interface FileLoudness {
  /** Integrated loudness (EBU R128), LUFS; null for a silent file. */
  lufs: number | null;
  /** True peak, dBTP. */
  peak_db: number | null;
  /** Offset that brings it to `show.loudness_target`, keeping the true peak at −1 dBTP; null for a silent file. */
  suggested_db: number | null;
}

/** Playback clamps these on its own; they say what will sound different. */
export type TimingIssue =
  | { kind: 'start_past_end' }
  | { kind: 'end_past_file' }
  | { kind: 'vamp_empty'; index: number }
  | { kind: 'vamp_before_start'; index: number }
  | { kind: 'vamp_past_end'; index: number }
  | { kind: 'vamp_overlap'; index: number; other: number }
  | { kind: 'vamp_cut'; index: number }
  | { kind: 'mark_outside'; index: number }
  | { kind: 'mark_duplicate'; index: number }
  | { kind: 'fade_in_too_long' }
  | { kind: 'end_fade_too_long' };

export type TimingWarning = TimingIssue & { message: string };

export interface ShowView {
  path: string;
  name: string;
  media_dir: string;
  media_dir_exists: boolean;
  show: Show;
  dirty: boolean;
  can_undo: boolean;
  can_redo: boolean;
  /** The clipboard holds cues (`paste`). */
  can_paste: boolean;
  audio: OutputInfo;
  /** Frames per callback as the device runs it (latency = frames / sample_rate); null before the first. */
  audio_frames: number | null;
  audio_error: string | null;
  /**
   * Why the open device stopped playing (unplugged, driver reset, …). Cues
   * run silently meanwhile; the device is opened again as soon as it can be.
   */
  audio_lost: string | null;
  keep_alive: KeepAliveState;
  settings: LocalSettings;
  /** Sound, playlist and group cues have an entry, others only with `target_missing` or `repeat_blocked`. */
  cue_info: Record<CueId, CueInfo>;
  /**
   * Time (ms since 1970) of an autosave holding changes lost in a crash;
   * ask whether to restore it (`recoverAutosave`). Nothing is autosaved
   * until then.
   */
  recovery: number | null;
  osc: {
    /** UDP port incoming OSC is received on. */
    listening: number | null;
    error: string | null;
  };
  preview: {
    /** Name of the separate preview device, if open. */
    device: string | null;
    error: string | null;
    /** Like `audio_lost`, for the preview device. */
    lost: string | null;
  };
  midi: {
    /** Name of the open input port. */
    port: string | null;
    error: string | null;
    /** Waiting for a control to learn for this. */
    learning: LearnTarget | null;
    /** Name of the open output port (MIDI cues). */
    out: string | null;
    out_error: string | null;
  };
  /** Keys the app keeps for itself (Ctrl+S/N/O/Z/Y/C/X/V/A by the letter on the key, Esc for panic, arrows, Home,
   * End, Delete, Tab, Enter); they cannot be bound. */
  fixed_keys: Chord[];
  /** Global shortcuts of a new machine (for "Standard"). */
  default_keys: KeyBinding[];
}

export interface ActiveState {
  cue: CueId;
  elapsed: number;
  remaining: number | null;
  /** Position within start–end, 0…1. */
  progress: number;
  level_db: number | null;
  /** Balance as it plays now (−1 … +1), also while a pan cue moves it. */
  pan: number;
  /** Rate as it plays now, percent (100 = original), also while a rate cue moves it. */
  rate: number;
  /** Transposition as it plays now, semitones (beyond what the rate does to the pitch). */
  transpose: number;
  looping: boolean;
  /** Item of a playlist cue. */
  item: ItemState | null;
  /** A playlist item another one follows (and not already fading out to it, unless paused): `next_item` goes on. */
  has_next: boolean;
  /** Pass (from 1) through the loop or vamp playback is in; `count` null = until devamped or stopped; `vamp`: its
   * number (from 1), null for a loop over the whole range. Null between vamps and without loops. */
  pass: PassState | null;
  /** Held by a pause; elapsed and remaining stand still. */
  paused: boolean;
  /** Waits (at most 2 s) for its streamed file's start to be read, instead of playing silence; elapsed stays 0. */
  loading: boolean;
}

/** A playlist item: `index` its position in the playing order (0-based, counted anew each pass), `file` its place
 * in `files` (0-based; differs from `index` when shuffled). */
export interface ItemState {
  index: number;
  file: number;
  count: number;
}

export interface PassState {
  pass: number;
  count: number | null;
  vamp: number | null;
}

/** A playlist cue in the gap between two items: nothing of it plays, but it still runs. */
export interface GapState {
  cue: CueId;
  /** Seconds until `next` starts. */
  remaining: number;
  /** The item that starts then. */
  next: ItemState;
  paused: boolean;
}

export interface FadingState {
  cue: CueId;
  elapsed: number;
  remaining: number;
  /** 0…1. */
  progress: number;
  paused: boolean;
}

/** A cue waiting to start by a chain; only the next one of each chain. */
export interface FollowState {
  cue: CueId;
  /** Starts once this cue reaches its advance point. */
  after: CueId;
  /** First cue of the chain (the one its GO started); list the row under it. */
  root: CueId;
  /** `after` is paused; `remaining` stands. */
  paused: boolean;
  /** Seconds; null while unknown (open vamp, loop exit). */
  remaining: number | null;
}

export interface RuntimeState {
  playhead: CueId | null;
  active: ActiveState[];
  /** Cues in their pre-wait; `root`: first cue of their chain (the cue itself when its GO started it). */
  waiting: { cue: CueId; remaining: number; paused: boolean; root: CueId }[];
  /** Volume, pan, reset and wait cues in progress; not part of `active`. */
  fading: FadingState[];
  /** Playlist cues between two items; not part of `active`. */
  gaps: GapState[];
  /** The cue on the preview output; `playing` is false while its file loads. `position`: seconds in the file. */
  preview: { cue: CueId; playing: boolean; elapsed: number; progress: number; position: number } | null;
  playhead_armed: boolean;
  /** GOs dropped by the GO lockout so far; flash GO when it changes. */
  go_rejected: number;
  /** Something is paused; the PAUSE ALL toggle then resumes. */
  any_paused: boolean;
  /** GOs taken on the playhead cue if it has Repeat (`count` null = forever); null otherwise. */
  repeat: { done: number; count: number | null } | null;
  following: FollowState[];
  /** Files being copied into the media folder, in copy order; `file` is the stored path the cues use. */
  imports: { file: string; progress: number }[];
  /** A search for missing files (`relink`) is under way; its repairs arrive as a new `show`. */
  searching: boolean;
  /** The show's stopwatches in its order; `elapsed` in seconds since the last reset, `remaining` a countdown's time
   * left (`countdown − elapsed`, negative in overtime; null for one counting up). */
  stopwatches: { id: string; elapsed: number; remaining: number | null; running: boolean }[];
}

/** A newer release than the running version. */
export interface UpdateInfo {
  version: string;
  /** Release notes, if the release has any. */
  notes: string | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number | null;
}

/** Why closing needs a question. */
export interface CloseCheck {
  show_mode: boolean;
  dirty: boolean;
  /** Running cues plus cues in their pre-wait. */
  running: number;
  /** Files still being copied into the media folder; leaving stops them. */
  imports: number;
}

export type Notice = { level: 'warning' | 'error'; message: string };

export type Action =
  | { action: 'go' }
  | { action: 'stop_all' }
  /** Esc: fades everything over `settings.panic_fade`; a second panic during the fade stops at once. */
  | { action: 'panic' }
  | { action: 'fade_all' }
  | { action: 'playhead_next' }
  | { action: 'playhead_prev' }
  | { action: 'set_playhead'; cue: CueId }
  /** Starts a cue (with its pre-wait) without moving the playhead, e.g. by hotkey. */
  | { action: 'start_cue'; cue: CueId }
  | { action: 'stop_cue'; cue: CueId }
  /** Fade-out with the cue's own time (FADE ALL time if 0), then stop. */
  | { action: 'fade_cue'; cue: CueId }
  /** Rehearsal jumps in a running cue by `settings.jump_back` / `jump_forward` seconds. */
  | { action: 'jump_back'; cue: CueId }
  | { action: 'jump_forward'; cue: CueId }
  /** To `settings.jump_end` seconds before the end; vamps jumped over count as left. */
  | { action: 'jump_end'; cue: CueId }
  /** A playlist goes on to its next item: the one playing fades out with its fade-out time (at once without one),
   * then the gap runs; in the gap, the next item starts at once. Paused, the next item starts at once (no fade, no
   * gap) and stays paused at its start. Nothing on the last item (without repeat). */
  | { action: 'next_item'; cue: CueId }
  /** Plays a sound cue on the preview output, or stops it if it is playing there; never touches the show. */
  | { action: 'preview'; cue: CueId }
  | { action: 'stop_preview' }
  /** Holds a running cue (a group: all of it) with its pre-wait and playlist gap; resume plays on. */
  | { action: 'pause_cue'; cue: CueId }
  | { action: 'resume_cue'; cue: CueId }
  /** Pauses if anything of the cue still plays, else resumes. */
  | { action: 'toggle_pause_cue'; cue: CueId }
  | { action: 'pause_all' }
  | { action: 'resume_all' }
  /** Pauses everything if anything still plays, else resumes everything. */
  | { action: 'toggle_pause_all' }
  /** Stops a cue and starts it again from the top. */
  | { action: 'restart_cue'; cue: CueId }
  /** Leaves the innermost loop or vamp of a cue at its end. */
  | { action: 'exit_loop_cue'; cue: CueId }
  /** Brings a cue's level and pan back to its own at once. */
  | { action: 'reset_cue'; cue: CueId }
  /** The cue's hotkey or MIDI control was released: fires its second trigger if it has `release_trigger`. */
  | { action: 'release_cue'; cue: CueId }
  /** Breaks off what a running cue (a group: anything in it) would still start by its advance; it plays on. */
  | { action: 'cancel_follow'; cue: CueId }
  /** Moves a running cue's level, pan or rate by `settings.volume_step` / `pan_step` / `rate_step`. */
  | { action: 'volume_up'; cue: CueId }
  | { action: 'volume_down'; cue: CueId }
  | { action: 'pan_left'; cue: CueId }
  | { action: 'pan_right'; cue: CueId }
  | { action: 'rate_up'; cue: CueId }
  | { action: 'rate_down'; cue: CueId };

/** What to do with a copied file whose name is taken in the media folder. */
export type OnClash = 'rename' | 'use_existing' | 'skip';

/** How picked files are stored. */
export interface Import {
  /** Copy files outside the media folder into it; otherwise linked by absolute path. */
  copy_outside: boolean;
  /** Per picked path, for the clashes `checkImport` found; without a choice the copy is renamed. */
  on_clash?: Record<string, OnClash>;
}

/** A file in the media folder that no cue uses; see `unusedMedia`. */
export interface UnusedFile {
  /** Path relative to the media folder, `/`-separated. */
  file: string;
  bytes: number;
}

export interface CleanedMedia {
  deleted: number;
  /** Files that could not be deleted, each as "path: reason". */
  failed: string[];
}

/** Files of a pick that need a question before they are added. */
export interface ImportCheck {
  /** Picked paths outside the media folder. */
  outside: string[];
  /** Of those, the ones whose name is taken when copied in (in pick order). */
  clashes: ImportClash[];
  /** Of those, the ones whose name already holds the same file; send them as "use_existing". */
  identical: string[];
}

export interface ImportClash {
  path: string;
  /** Stored path of the file that has the name. */
  existing: string;
  /** Taken by an earlier file of the same pick, not by one in the media folder ("use_existing" makes no sense then). */
  picked: boolean;
}

export type EditOp =
  /** Also for cues inside a group. */
  | { op: 'update_cue'; cue: Cue }
  /** Several cues as one undo step (multi-selection in the inspector); all must exist. */
  | { op: 'update_cues'; cues: Cue[] }
  // The add_* ops insert into the group `group` (index within it) if given.
  // Files to copy get their cues at once (`CueInfo.importing`); the copying runs in the background.
  | ({ op: 'add_sounds'; paths: string[]; index: number | null; group?: CueId } & Import)
  /** A control cue with defaults, titled after its type (and its one target, e.g. „Fade-Out Musik“). `targets: null`
   * = the entry before the new cue ("previous"). A stopwatch cue gets the show's first stopwatch. */
  | { op: 'add_control'; kind: ControlKind; index: number | null; targets: CueId[] | null; group?: CueId }
  /** Turns control cues into another control type, one undo step; targets, duration and curve stay where both have
   * them ("all"/output become "previous" for start_stop). Fails for other cues. */
  | { op: 'set_cue_type'; ids: CueId[]; kind: ControlKind }
  /** Ids unique and not empty, names given, countdowns > 0 and at most 100 h. */
  | { op: 'set_stopwatches'; stopwatches: Stopwatch[] }
  /** With advance { start_next: true, at: "end" }. */
  | { op: 'add_wait'; index: number | null; group?: CueId }
  /** Titled „Gehe zu“, without a target. */
  | { op: 'add_goto'; index: number | null; group?: CueId }
  /** Scene marker titled „Szene“, in the main list only, without a number. */
  | { op: 'add_scene'; index: number | null }
  /** Sound cue without a file titled „Platzhalter“ (`file` ""). */
  | { op: 'add_placeholder'; index: number | null; group?: CueId }
  | { op: 'add_group'; index: number | null }
  | { op: 'add_osc'; index: number | null; group?: CueId }
  | { op: 'add_midi'; index: number | null; group?: CueId }
  | ({ op: 'add_playlist'; paths: string[]; index: number | null; group?: CueId } & Import)
  | ({ op: 'add_to_playlist'; id: CueId; paths: string[] } & Import)
  /** Another file for this one sound cue only; `retitle` names the cue after the new file. With `item`, replaces
   * that item of a playlist cue instead (`retitle` is ignored). */
  | ({ op: 'set_file'; id: CueId; path: string; item?: number; retitle?: boolean } & Import)
  | { op: 'set_devices'; devices: NetDevice[] }
  /** Output "main" must stay (renaming is fine); outputs used by cues cannot be removed. */
  | { op: 'set_outputs'; outputs: Output[] }
  /** The show's default effect of a starting cue on the others. */
  | { op: 'set_others'; others: OthersEffect }
  /** Max −100…+24 dB, min at least 6 dB below it. Volumes outside are brought in at once, in the same undo step
   * (see `checkLevelLimits`, to warn first). */
  | { op: 'set_level_limits'; limits: LevelLimits }
  /** Loudness target (−40…−6 LUFS) and automatic normalization of new cues; offsets already set stay. */
  | { op: 'set_normalization'; target: number; on_import: boolean }
  /** Sets the offsets of these cues (and a group's cues) from their measured files, one undo step; files not measured
   * yet are left out (with a notice). To remove an offset, update the cue with `normalize_db` null (playlist: on its items). */
  | { op: 'normalize'; ids: CueId[] }
  /** Locking also enters show mode; the only edit allowed in show mode. */
  | { op: 'lock_show_mode'; locked: boolean }
  /** delete, move (within its list) and duplicate also work inside groups. */
  | { op: 'delete'; ids: CueId[] }
  /** Moves `ids` (siblings) as a block, in list order; `to` is the index among the cues that stay. */
  | { op: 'move'; ids: CueId[]; to: number }
  /** `linked`: the copy is an instance of the original (not for groups). */
  | { op: 'duplicate'; id: CueId; linked?: boolean }
  /** `ids` take the settings of `source` and stay in sync with it; links they are in are merged.
   * Only cues of one type, no groups. */
  | { op: 'link_cues'; source: CueId; ids: CueId[] }
  /** Takes cues out of their links; they keep their settings. */
  | { op: 'unlink_cues'; ids: CueId[] }
  /** Copies onto the backend's clipboard (kept while the show is open); no undo step. */
  | { op: 'copy'; ids: CueId[] }
  /** copy, then delete. */
  | { op: 'cut'; ids: CueId[] }
  /** Inserts the clipboard after `after` (in its list, also a group's), or at the end of the main list.
   * Copies get new ids and no hotkey; outside groups they keep their numbers if those are free.
   * `linked`: each copy is an instance of its original, if that is still in the show. */
  | { op: 'paste'; after: CueId | null; linked?: boolean }
  /** Deletes `ids` and inserts the clipboard where the first of them was (list order), one undo step; copies as with paste.
   * `swap`: the deleted cues are on the clipboard afterwards (as by copy). */
  | { op: 'paste_replace'; ids: CueId[]; swap?: boolean }
  /** Repairs missing files from same-named files found in `dir`. Searches in the background
   * (`RuntimeState.searching`); the repairs arrive as one undo step in a later `show`. */
  | { op: 'relink'; dir: string; copy_outside: boolean }
  /** Replaces every use of one stored media path with the selected file. */
  | ({ op: 'relink_file'; old_path: string; path: string } & Import)
  /** 1, 2, 3 … through the main list; scene markers get no number. */
  | { op: 'renumber' }
  /** Puts cues of the main list (no scene markers or groups: groups don't nest) into a new group in sequence,
   * at the place of the first, so they
   * play as before: the group continues like the last cue and takes the first one's hotkey, MIDI trigger and passes. */
  | { op: 'group'; ids: CueId[] }
  /** Dissolves the groups among `ids`: their cues take each group's place and play as before as far as possible
   * (advance, pre-wait, hotkey; control and go-to cues aiming at the group aim at its cues). */
  | { op: 'ungroup'; ids: CueId[] }
  | { op: 'undo' }
  | { op: 'redo' };

export const SHOW_EXTENSION = 'jqshow';

/** What "save as" does with the media; see `saveShowAs`. */
export type SaveAsMedia = 'copy' | 'link';

/** The language setting and what it resolves to; the language in use is always one of the offered. */
export interface LanguageInfo {
  /** "auto" or a language code, as in `LocalSettings.language`. */
  setting: string;
  resolved: string;
}

export const api = {
  /** The language setting and the language in use; works without a show (start screen). */
  getLanguage: () => invoke<LanguageInfo>('get_language'),
  openShow: (path: string) => invoke<ShowView>('open_show', { path }),
  /** Which cues `set_level_limits` with these limits would change; changes nothing. */
  checkLevelLimits: (limits: LevelLimits) => invoke<LevelChanges>('check_level_limits', { limits }),
  /** The last show to offer on the start screen (`open_last = "ask"`, or after a crash); only the first call gets it. */
  startupShow: () => invoke<string | null>('startup_show'),
  newShow: (dir: string, name: string, createFolder: boolean) =>
    invoke<ShowView>('new_show', { dir, name, createFolder }),
  saveShow: () => invoke<ShowView>('save_show'),
  /** Also sends the current `state` event first; listen to it before calling (on start, after a reload). */
  getShow: () => invoke<ShowView | null>('get_show'),
  /** Fire-and-forget; GO never waits for a reply. */
  action: (action: Action) => invoke<void>('action', { action }),
  /** Fire-and-forget: starts, stops or resets a stopwatch of the show (by id). */
  stopwatch: (id: string, action: StopwatchAction) => invoke<void>('stopwatch', { id, action }),
  edit: (op: EditOp) => invoke<ShowView>('edit', { op }),
  /**
   * Fire-and-forget, for every step of a slider: running cues follow `cue`
   * (volume, pan), but show and undo history stay. End the edit with
   * `update_cue`; until then the next GO plays the cue as sent here.
   * Ignored in show mode; sends no `show` event.
   */
  liveCue: (cue: Cue) => invoke<void>('live_cue', { cue }),
  /**
   * Fire-and-forget: plays a sound cue's file on the preview output from `at`
   * seconds to `until` (default: the file's end), at the cue's level (no
   * fades, the cue's start/end do not apply); while it plays, another call
   * with the same `until` moves it. Stop with `stop_preview`.
   */
  previewAt: (cue: CueId, at: number, until?: number) => invoke<void>('preview_at', { cue, at, until: until ?? null }),
  /** Fails in show mode. */
  setKeepAwake: (enabled: boolean) => invoke<ShowView>('set_keep_awake', { enabled }),
  /** Learns the next MIDI control for `target`; null cancels. Needs an open port; a cue only in edit mode. */
  midiLearn: (target: LearnTarget | null) => invoke<ShowView>('midi_learn', { target }),
  /** Unshifted text of the character keys on the current keyboard layout, by `KeyboardEvent.code` (missing keys:
   * unknown). For webviews without `navigator.keyboard`. */
  keyLabels: () => invoke<Record<string, string>>('key_labels'),
  /** Windows: the window's border in the stop colour (no keyboard focus) or back to the line colour; elsewhere
   * nothing. */
  windowAlert: (on: boolean) => invoke<void>('window_alert', { on }),
  midiPorts: () => invoke<{ inputs: string[]; outputs: string[] }>('midi_ports'),
  /**
   * Peak levels (0…1) of `buckets` (at most 200 000) equal slices of a media
   * file, from the machine's peak cache; a file not cached yet (or changed)
   * is decoded in the background first.
   */
  waveform: (mediaDir: string, file: string, buckets: number) =>
    invoke<{ duration: number; peaks: number[] }>('waveform', { mediaDir, file, buckets }),
  /**
   * Like `waveform` for `from`–`to` seconds only: detail for a zoomed-in
   * view. From the peak cache down to 256 samples per bucket, closer from
   * every sample (then at most 120 s). `duration` is the stretch covered.
   */
  waveformRange: (mediaDir: string, file: string, from: number, to: number, buckets: number) =>
    invoke<{ duration: number; peaks: number[] }>('waveform_range', { mediaDir, file, from, to, buckets }),
  /** Bytes the waveform peak cache takes on this machine (kept under 500 MB). */
  peakCacheSize: () => invoke<number>('peak_cache_size'),
  /** Deletes the cached peaks; returns the bytes left. Waveforms are decoded again when next shown. */
  clearPeakCache: () => invoke<number>('clear_peak_cache'),
  /** Which picked files lie outside the media folder and whose names are taken there; reads the files, may take a moment. */
  checkImport: (mediaDir: string, paths: string[]) => invoke<ImportCheck>('check_import', { mediaDir, paths }),
  /**
   * Files in the media folder that neither the show (as edited or as last saved) nor the clipboard
   * uses, sorted. Names are compared ignoring case; hidden files are left out. Fails in show mode.
   */
  unusedMedia: () => invoke<UnusedFile[]>('unused_media'),
  /**
   * Deletes the given files (from `unusedMedia`) for good, skipping those the show uses again by
   * now, and the folders left empty. Fails in show mode.
   */
  cleanMedia: (files: string[]) => invoke<CleanedMedia>('clean_media', { files }),
  /** Restores the crash autosave as one undo step (`true`) or deletes it. */
  recoverAutosave: (apply: boolean) => invoke<ShowView>('recover_autosave', { apply }),
  /**
   * Saves the show as `<dir>/<name>/<name>.jqshow` and goes on there; the old show stays as last
   * saved. `copy` copies the media files the show uses (undo history and clipboard included) into
   * the new folder's own media folder, `link` points the new show at the current media folder.
   * Fails in show mode, while cues run or files are being copied in, and if the folder exists.
   */
  saveShowAs: (dir: string, name: string, media: SaveAsMedia) => invoke<ShowView>('save_show_as', { dir, name, media }),
  /** Renames the show folder; fails in show mode and while cues run. */
  renameShow: (name: string) => invoke<ShowView>('rename_show', { name }),
  setShowMode: (enabled: boolean) => invoke<ShowView>('set_show_mode', { enabled }),
  /**
   * Saves and applies all settings. A changed host/device/channel count
   * (or a failed device) is reopened; that fails while cues are running.
   */
  setSettings: (settings: LocalSettings) => invoke<ShowView>('set_settings', { settings }),
  /** Remembers the audio picker's folder; changes no other setting, so it also works while cues run. */
  setAudioDir: (dir: string | null) => invoke<ShowView>('set_audio_dir', { dir }),
  /** Changes the show's media folder; refused while cues run or file actions are underway. */
  setMediaDir: (dir: string) => invoke<ShowView>('set_media_dir', { dir }),
  /** Stores the panel layout; changes no other setting, so it also works while cues run. */
  setLayout: (layout: unknown) => invoke<ShowView>('set_layout', { layout }),
  /** Stores the cue list's selection mode; changes no other setting. */
  setSelectionMode: (mode: SelectionMode) => invoke<ShowView>('set_selection_mode', { mode }),
  /**
   * Looks at the media files again, for changes the file watcher missed; missing files are searched anew. Changes
   * found come in as `show` events and warnings.
   */
  rescanMedia: () => invoke<ShowView>('rescan_media'),
  /** Opens the existing media folder in the system file manager. */
  openMediaDir: () => invoke<void>('open_media_dir'),
  /**
   * Must be called right after `close_requested` arrives; otherwise the
   * backend assumes a hung UI and asks with a native dialog.
   */
  closeAck: () => invoke<void>('close_ack'),
  /** Looks for a newer release on GitHub; `null` if this is the latest. Never installs anything. */
  checkUpdate: () => invoke<UpdateInfo | null>('check_update'),
  /**
   * Downloads and installs the update `checkUpdate` found, then restarts (on Windows the installer ends the
   * program, so this may not return). Fails while show mode is on, a cue runs or the show has unsaved changes.
   */
  installUpdate: () => invoke<void>('install_update'),
  /** Closes the window; with `save`, only if saving succeeds. */
  confirmClose: (save: boolean) => invoke<void>('confirm_close', { save }),
  /**
   * The panel windows there should be (the current mode's list in the layout; `[]` closes all): opens missing ones
   * at their stored place (back on a screen if that one is gone), switches the kind of open ones and closes the
   * others. Position and size of open windows stay as the operator left them. Main window only.
   */
  setPanelWindows: (windows: PanelWindow[]) => invoke<void>('set_panel_windows', { windows }),
  windowSupport: () => invoke<WindowSupport>('window_support'),
  /** Whether any window of the app has the keyboard focus (later changes come as `app_focus`). */
  appFocused: () => invoke<boolean>('app_focused'),
  /** Brings the main window to the front. */
  focusMain: () => invoke<void>('focus_main'),
  /** Output devices of `host` (default: system host); may take a moment. */
  audioDevices: (host?: string) => invoke<AudioDevices>('audio_devices', { host: host ?? null }),
};

export const events = {
  /** The language setting changed (`setSettings`); sent to all windows. */
  onLanguageChanged: (handler: (info: LanguageInfo) => void): Promise<UnlistenFn> =>
    listen<LanguageInfo>('language_changed', (e) => handler(e.payload)),
  onShow: (handler: (view: ShowView | null) => void): Promise<UnlistenFn> =>
    listen<ShowView | null>('show', (e) => handler(e.payload)),
  /** Sent only when the state changed (up to 20 times a second), and with `getShow`. */
  onState: (handler: (state: RuntimeState) => void): Promise<UnlistenFn> =>
    listen<RuntimeState>('state', (e) => handler(e.payload)),
  onNotice: (handler: (notice: Notice) => void): Promise<UnlistenFn> =>
    listen<Notice>('notice', (e) => handler(e.payload)),
  onCloseRequested: (handler: (check: CloseCheck) => void): Promise<UnlistenFn> =>
    listen<CloseCheck>('close_requested', (e) => handler(e.payload)),
  /** The start-up check (setting `check_updates`) found a newer release; sent once, never while installing. */
  onUpdateAvailable: (handler: (info: UpdateInfo) => void): Promise<UnlistenFn> =>
    listen<UpdateInfo>('update_available', (e) => handler(e.payload)),
  /** Download progress of `installUpdate`; `total` is null if the server does not tell the size. */
  onUpdateProgress: (handler: (progress: UpdateProgress) => void): Promise<UnlistenFn> =>
    listen<UpdateProgress>('update_progress', (e) => handler(e.payload)),
  /** Open it like `openShow`, asking first; the backend opens nothing itself. */
  onOpenRequested: (handler: (path: string) => void): Promise<UnlistenFn> =>
    listen<string>('open_requested', (e) => handler(e.payload)),
  /** A panel window was moved or resized; may come many times a second while dragging. */
  onPanelWindowMoved: (handler: (rect: PanelWindowRect) => void): Promise<UnlistenFn> =>
    listen<PanelWindowRect>('panel_window_moved', (e) => handler(e.payload)),
  /** The operator closed a panel window (its × or Alt+F4); not sent when `setPanelWindows` closes it. */
  onPanelWindowClosed: (handler: (panel: string) => void): Promise<UnlistenFn> =>
    listen<{ panel: string }>('panel_window_closed', (e) => handler(e.payload.panel)),
  /** Whether any window of the app has the keyboard focus; sent once it settled (not while moving between them). */
  onAppFocus: (handler: (focused: boolean) => void): Promise<UnlistenFn> =>
    listen<boolean>('app_focus', (e) => handler(e.payload)),
};
