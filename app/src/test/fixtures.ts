// Builders for the data the backend sends (`api.ts` types), with plain defaults; a test sets only what it is about.

import type {
  Chord,
  ControlKind,
  Cue,
  CueId,
  CueInfo,
  GroupCue,
  LocalSettings,
  PlaylistCue,
  RuntimeState,
  Show,
  ShowView,
  SoundCue,
} from '$lib/api';

type Partial2<T> = { [K in keyof T]?: T[K] extends object ? Partial<T[K]> | T[K] : T[K] };

const base = (id: CueId) => ({
  id,
  number: '',
  title: id,
  script_ref: '',
  notes: '',
  color: null,
  advance: { start_next: false, at: { at: 'end' as const } },
  passes: 0,
  pre_wait: 0,
  hotkey: null,
  midi_trigger: null,
  second_trigger: { action: 'ignore' as const },
  release_trigger: false,
  others: null,
});

export function sound(id: CueId, extra: Partial<SoundCue> = {}): SoundCue {
  return {
    ...base(id),
    type: 'sound',
    file: `${id}.wav`,
    output: 'main',
    volume_db: 0,
    pan: 0,
    start: 0,
    end: null,
    loop: 0,
    vamps: [],
    marks: [],
    fade_in: 0,
    fade_out: 0,
    end_fade: false,
    curve: 'decibel_slow_top',
    ...extra,
  };
}

export function playlist(id: CueId, files: string[], extra: Partial<PlaylistCue> = {}): PlaylistCue {
  return {
    ...base(id),
    type: 'playlist',
    items: files.map((file) => ({ file })),
    output: 'main',
    volume_db: 0,
    pan: 0,
    fade_in: 0,
    fade_out: 0,
    gap: 0,
    repeat: false,
    shuffle: 'off',
    curve: 'decibel_slow_top',
    ...extra,
  };
}

export function group(id: CueId, cues: Cue[], extra: Partial<GroupCue> = {}): GroupCue {
  return { ...base(id), type: 'group', mode: 'all', cues, ...extra };
}

export function wait(id: CueId, extra: object = {}): Cue {
  return { ...base(id), type: 'wait', duration: 5, ...extra } as Cue;
}

export function scene(id: CueId, extra: object = {}): Cue {
  return { ...base(id), type: 'scene', ...extra } as Cue;
}

export function goto(id: CueId, target: CueId | null, extra: object = {}): Cue {
  return { ...base(id), type: 'goto', target, start: false, ...extra } as Cue;
}

/** A control cue of `kind` with the backend's defaults; `extra` overrides them. */
export function control(id: CueId, kind: ControlKind, extra: object = {}): Cue {
  const fields: Record<ControlKind, object> = {
    start_stop: { targets: 'previous', action: 'start', from: null },
    pause: { targets: 'previous', action: 'pause' },
    volume: { targets: 'previous', action: 'fade_to', volume_db: -10, duration: 3, curve: 'decibel_slow_top' },
    pan: { targets: 'previous', pan: 0, duration: 0, curve: 's_curve' },
    rate: { targets: 'previous', rate: 100, transpose: null, duration: 0, curve: 's_curve' },
    position: { targets: 'previous', to: { to: 'absolute', secs: 0 }, crossfade_ms: 20 },
    exit_loop: { targets: 'previous', mode: 'loop_end', level: 'innermost', stop: false },
    reset: { targets: 'previous', duration: 0 },
    stopwatch: { watch: null, action: 'start' },
  };
  return { ...base(id), type: kind, ...fields[kind], ...extra } as Cue;
}

export function chord(code: string, mods: Partial<Chord> = {}): Chord {
  return { code, ctrl: false, shift: false, alt: false, label: code.replace(/^Key/, '').toLowerCase(), ...mods };
}

export function settings(extra: Partial<LocalSettings> = {}): LocalSettings {
  return {
    keep_awake: false,
    show_mode: false,
    host: null,
    device: null,
    channels: null,
    outputs: { main: 1 },
    fade_all: 2,
    jump_back: 5,
    jump_forward: 5,
    jump_end: 10,
    end_warning: 10,
    wheel: 'scroll',
    buffer_frames: null,
    escape: 'stop_all',
    panic_fade: 1,
    go_lockout: 0,
    open_last: 'off',
    last_show: null,
    preview_device: null,
    device_names: {},
    preview_channel: null,
    midi_port: null,
    midi: [],
    msc_device: null,
    keys: [
      { chord: chord('Space', { label: ' ' }), action: 'go' },
      { chord: chord('KeyP'), action: 'preview' },
      { chord: chord('Equal', { label: '=' }), action: 'volume_up' },
      { chord: chord('KeyG', { ctrl: true }), action: 'focus_goto' },
    ],
    volume_step: 1,
    pan_step: 0.1,
    rate_step: 1,
    focus_warning: false,
    midi_out: null,
    osc_listen: null,
    osc_hosts: {},
    media_watch: 'events',
    language: 'auto',
    check_updates: false,
    underrun_warning: false,
    keep_alive_signal: false,
    keep_alive_level: -80,
    audio_dir: null,
    layout: null,
    selection_mode: 'separate',
    ...extra,
  };
}

export function info(extra: Partial<CueInfo> = {}): CueInfo {
  return {
    file_duration: 60,
    duration: 60,
    missing: false,
    missing_items: [],
    case_mismatch: null,
    file_version: 0,
    timing: [],
    item_timing: [],
    probing: false,
    importing: false,
    level_capped: false,
    level_silenced: false,
    problems: [],
    loudness: [],
    measuring: false,
    normalize_stale: false,
    target_missing: false,
    repeat_blocked: false,
    hotkey_shared: [],
    ...extra,
  };
}

export function show(cues: Cue[], extra: Partial<Show> = {}): Show {
  return {
    format: 'jqshow',
    version: 1,
    id: 'show-1',
    name: 'Test Show',
    media_root: 'media',
    outputs: [
      { id: 'main', name: 'Main' },
      { id: 'monitor', name: 'Monitor' },
    ],
    others: { effect: 'none', duck_db: -10, fade: 1, scope: 'all' },
    show_mode_locked: false,
    level_limits: { max_db: 6, min_db: -60 },
    loudness_target: -23,
    normalize_on_import: false,
    devices: [],
    stopwatches: [],
    cues,
    ...extra,
  };
}

/** A show view of `cues`; sound and playlist cues get a `cue_info` entry (found files, 60 s). */
export function view(
  cues: Cue[] = [],
  extra: Partial2<Omit<ShowView, 'show' | 'settings'>> & {
    show?: Partial<Show>;
    settings?: Partial<LocalSettings>;
  } = {},
): ShowView {
  const infos: Record<CueId, CueInfo> = {};
  const walk = (list: Cue[]) =>
    list.forEach((cue) => {
      if (cue.type === 'sound') infos[cue.id] = info();
      if (cue.type === 'playlist') infos[cue.id] = info({ missing_items: cue.items.map(() => false) });
      if (cue.type === 'group') {
        infos[cue.id] = info();
        walk(cue.cues);
      }
    });
  walk(cues);
  const { show: showExtra, settings: settingsExtra, cue_info, ...rest } = extra;
  return {
    path: '/shows/Test Show/Test Show.jqshow',
    name: 'Test Show',
    media_dir: '/shows/Test Show/media',
    media_dir_exists: true,
    show: show(cues, showExtra),
    dirty: false,
    can_undo: false,
    can_redo: false,
    can_paste: false,
    audio: {
      host: 'WASAPI',
      device: 'Speakers',
      device_detail: 'Speakers (Test)',
      sample_rate: 48000,
      channels: 2,
      max_channels: 2,
      sample_format: 'F32',
      buffer_frames: null,
    },
    audio_frames: 480,
    audio_error: null,
    audio_lost: null,
    keep_alive: { state: 'off' },
    settings: settings(settingsExtra),
    cue_info: { ...infos, ...(cue_info as Record<CueId, CueInfo>) },
    recovery: null,
    osc: { listening: null, error: null },
    preview: { device: null, error: null, lost: null },
    midi: { port: null, error: null, learning: null, out: null, out_error: null },
    fixed_keys: [
      chord('KeyS', { ctrl: true, label: 's' }),
      chord('KeyZ', { ctrl: true, label: 'z' }),
      chord('Escape', { label: 'Escape' }),
      chord('ArrowDown'),
      chord('Delete'),
    ],
    default_keys: [{ chord: chord('Space', { label: ' ' }), action: 'go' }],
    ...(rest as Partial<ShowView>),
  };
}

export function runtime(extra: Partial<RuntimeState> = {}): RuntimeState {
  return {
    playhead: null,
    active: [],
    waiting: [],
    fading: [],
    gaps: [],
    preview: null,
    playhead_armed: false,
    go_rejected: 0,
    any_paused: false,
    repeat: null,
    following: [],
    imports: [],
    searching: false,
    stopwatches: [],
    ...extra,
  };
}

/** A running sound cue's state. */
export function active(
  cue: CueId,
  extra: Partial<RuntimeState['active'][number]> = {},
): RuntimeState['active'][number] {
  return {
    cue,
    elapsed: 10,
    remaining: 50,
    progress: 1 / 6,
    level_db: 0,
    pan: 0,
    rate: 100,
    transpose: 0,
    looping: false,
    item: null,
    has_next: false,
    pass: null,
    paused: false,
    loading: false,
    ...extra,
  };
}
