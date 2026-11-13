// Rules of the machine settings as the settings dialog edits them.

import type { Chord, ControlAction, KeyBinding, LocalSettings, ShowView } from './api';
import { type AudioSetup, fits } from './audioSetup.svelte';
import { t } from './i18n/index.svelte';
import { ACTION_NAMES, isFixed, keyText, sameChord } from './keys.svelte';

/** The tabs of the settings dialog. */
export type SettingsTab = 'device' | 'routing' | 'show' | 'keys' | 'midi' | 'osc' | 'ui' | 'media';

/** The tabs in the order of the dialog, grouped under a heading each. */
export const SETTINGS_GROUPS: { title: string; tabs: { id: SettingsTab; title: string }[] }[] = [
  {
    get title() {
      return t('settings.group.audio');
    },
    tabs: [
      {
        id: 'device',
        get title() {
          return t('settings.tab.device');
        },
      },
      {
        id: 'routing',
        get title() {
          return t('settings.tab.routing');
        },
      },
    ],
  },
  {
    get title() {
      return t('settings.group.show');
    },
    tabs: [
      {
        id: 'show',
        get title() {
          return t('settings.tab.show');
        },
      },
      {
        id: 'keys',
        get title() {
          return t('settings.tab.keys');
        },
      },
    ],
  },
  {
    get title() {
      return t('settings.group.remote');
    },
    tabs: [
      {
        id: 'midi',
        get title() {
          return t('settings.tab.midi');
        },
      },
      {
        id: 'osc',
        get title() {
          return t('settings.tab.osc');
        },
      },
    ],
  },
  {
    get title() {
      return t('settings.group.program');
    },
    tabs: [
      {
        id: 'ui',
        get title() {
          return t('settings.tab.ui');
        },
      },
      {
        id: 'media',
        get title() {
          return t('settings.tab.media');
        },
      },
    ],
  },
];

export const tabTitle = (tab: SettingsTab) =>
  SETTINGS_GROUPS.flatMap((group) => group.tabs).find((entry) => entry.id === tab)?.title ?? tab;

/** The draft fields each tab edits (the waveform cache applies at once and is not among them). */
const TAB_FIELDS: Record<SettingsTab, (keyof LocalSettings)[]> = {
  device: ['host', 'device', 'channels', 'buffer_frames'],
  routing: ['outputs', 'preview_device', 'preview_channel'],
  show: ['fade_all', 'panic_fade', 'go_lockout', 'end_warning'],
  keys: ['keys', 'jump_back', 'jump_forward', 'jump_end', 'volume_step', 'pan_step', 'rate_step'],
  midi: ['midi_port', 'msc_device', 'midi_out'],
  osc: ['osc_listen', 'osc_hosts'],
  ui: ['language', 'open_last', 'wheel', 'escape', 'focus_warning', 'check_updates'],
  media: ['media_watch'],
};

/** The settings to send: `current` (as the backend has them) with the fields the dialog edits taken from `draft`. */
export function withEdits(current: LocalSettings, draft: LocalSettings): LocalSettings {
  const next = { ...current };
  for (const key of Object.values(TAB_FIELDS).flat()) (next as Record<string, unknown>)[key] = draft[key];
  return next;
}

/** The fields of `draft` that differ from `initial`, by tab. */
export function changedFields(draft: LocalSettings, initial: LocalSettings): Map<SettingsTab, number> {
  const changed = new Map<SettingsTab, number>();
  for (const [tab, fields] of Object.entries(TAB_FIELDS) as [SettingsTab, (keyof LocalSettings)[]][]) {
    const count = fields.filter((key) => JSON.stringify(draft[key]) !== JSON.stringify(initial[key])).length;
    if (count) changed.set(tab, count);
  }
  return changed;
}

/** Tabs with a problem to show in the tab list: a device that failed, channels that do not fit, … */
export function tabProblems(view: ShowView, draft: LocalSettings, setup: AudioSetup): Set<SettingsTab> {
  const problems = new Set<SettingsTab>();
  if (view.audio_error || setup.tooManyChannels) problems.add('device');
  const outputsFit = view.show.outputs.every((output) => fits(draft.outputs[output.id] ?? 1, setup.mainLimit));
  const previewFits = setup.previewMode === 'off' || fits(draft.preview_channel ?? 1, setup.previewLimit);
  if (!outputsFit || !previewFits || setup.previewOverlaps || view.preview.error) problems.add('routing');
  if (view.midi.error || view.midi.out_error) problems.add('midi');
  if (view.osc.error) problems.add('osc');
  return problems;
}

/** The global actions as the shortcut table groups them, by what they act on. */
/** `keywords` are found by the search besides the names (e.g. for the step fields of the group). */
export const ACTION_GROUPS: {
  id: string;
  title: string;
  scope?: string;
  keywords?: string;
  actions: ControlAction[];
}[] = [
  {
    id: 'flow',
    get title() {
      return t('actions.group.flow');
    },
    actions: ['go', 'stop_all', 'panic', 'fade_all'],
  },
  {
    id: 'pause',
    get title() {
      return t('actions.group.pause');
    },
    actions: ['pause_all', 'resume_all', 'toggle_pause_all'],
  },
  {
    id: 'playhead',
    get title() {
      return t('actions.group.playhead');
    },
    actions: ['playhead_next', 'playhead_prev'],
  },
  {
    id: 'preview',
    get title() {
      return t('actions.group.preview');
    },
    actions: ['preview', 'stop_preview'],
  },
  {
    id: 'selected',
    get title() {
      return t('actions.group.selected');
    },
    get scope() {
      return t('actions.scope.keys_only');
    },
    actions: ['toggle_pause_cue', 'stop_cue', 'fade_cue', 'restart_cue', 'exit_loop_cue', 'next_item'],
  },
  {
    id: 'jump',
    get title() {
      return t('actions.group.jump');
    },
    get scope() {
      return t('actions.scope.selected_keys_only');
    },
    get keywords() {
      return t('actions.keywords.jump');
    },
    actions: ['jump_back', 'jump_forward', 'jump_end'],
  },
  {
    id: 'level',
    get title() {
      return t('actions.group.level');
    },
    get scope() {
      return t('actions.scope.selected_keys_only');
    },
    get keywords() {
      return t('actions.keywords.level');
    },
    actions: ['volume_up', 'volume_down', 'pan_left', 'pan_right', 'rate_up', 'rate_down'],
  },
];

/** What keeps the settings from being applied (as a message in the current language); null if nothing does. An
 * emptied number field binds to null, so every number is checked for being one. */
export function settingsProblem(draft: LocalSettings): string | null {
  const isNumber = (value: unknown): value is number => typeof value === 'number' && isFinite(value);
  if (!isNumber(draft.fade_all) || draft.fade_all < 0) {
    return t('settings.problem.not_negative', { what: t('settings.problem.fade_all') });
  }
  if ([draft.jump_back, draft.jump_forward, draft.jump_end].some((jump) => !isNumber(jump) || jump <= 0)) {
    return t('settings.problem.jump_times');
  }
  const times: [unknown, string][] = [
    [draft.end_warning, t('settings.problem.end_warning')],
    [draft.panic_fade, t('settings.problem.panic_fade')],
    [draft.go_lockout, t('settings.problem.go_lockout')],
  ];
  const time = times.find(([secs]) => !isNumber(secs) || secs < 0);
  if (time) return t('settings.problem.not_negative', { what: time[1] });
  const steps: [unknown, string][] = [
    [draft.volume_step, t('settings.problem.volume_step')],
    [draft.pan_step, t('settings.problem.pan_step')],
    [draft.rate_step, t('settings.problem.rate_step')],
  ];
  const step = steps.find(([value]) => !isNumber(value) || !(value > 0));
  if (step) return t('settings.problem.positive', { what: step[1] });
  return null;
}

/** The name of a global action. */
export const actionName = (action: ControlAction) => ACTION_NAMES[action] ?? action;

/** The keys of `action`, in order. */
export const keysOf = (keys: KeyBinding[], action: ControlAction) =>
  keys.filter((binding) => binding.action === action).map((binding) => binding.chord);

/**
 * Sets (or with null removes) key `index` of `action`; a key another action
 * had moves here, and its former binding is returned as `displaced`. A fixed
 * key cannot be bound (`error`).
 */
export function bindKey(
  keys: KeyBinding[],
  action: ControlAction,
  index: number,
  chord: Chord | null,
  fixed: Chord[],
): { keys: KeyBinding[]; error?: string; displaced: KeyBinding[] } {
  if (chord && isFixed(chord, fixed)) {
    return { keys, error: t('keys.fixed', { key: keyText(chord) }), displaced: [] };
  }
  const before = keys.find((binding) => chord && binding.action !== action && sameChord(binding.chord, chord));
  let own = keysOf(keys, action);
  if (chord) {
    own[index] = chord;
    // The chord the action already has in another slot would be a duplicate.
    own = own.filter((key, i) => i === index || !sameChord(key, chord));
  } else own.splice(index, 1);
  const others = keys.filter((binding) => binding.action !== action && !(chord && sameChord(binding.chord, chord)));
  const next = [...others, ...own.map((key) => ({ chord: key, action }))];
  return {
    keys: next,
    displaced: before ? [before] : [],
  };
}

/** Whether the keys of `action` differ from its default keys (order aside). */
export function keysChanged(keys: KeyBinding[], defaults: KeyBinding[], action: ControlAction): boolean {
  const own = keysOf(keys, action);
  const standard = keysOf(defaults, action);
  return own.length !== standard.length || !own.every((chord) => standard.some((other) => sameChord(chord, other)));
}

/**
 * Gives `action` its default keys again; a default key another action has
 * moves here, and those former bindings are returned as `displaced`.
 */
export function resetKeys(
  keys: KeyBinding[],
  defaults: KeyBinding[],
  action: ControlAction,
): { keys: KeyBinding[]; displaced: KeyBinding[] } {
  const standard = keysOf(defaults, action);
  const taken = keys.filter(
    (binding) => binding.action !== action && standard.some((chord) => sameChord(chord, binding.chord)),
  );
  const others = keys.filter((binding) => binding.action !== action && !taken.includes(binding));
  return {
    keys: [...others, ...standard.map((chord) => ({ chord, action }))],
    displaced: taken,
  };
}

/**
 * The note on keys `action` took from other actions, as long as it still has
 * them in `keys` and they are not applied yet (not so in `applied`).
 */
export function displacedText(
  displaced: KeyBinding[],
  action: ControlAction,
  keys: KeyBinding[],
  applied: KeyBinding[],
): string | undefined {
  const holds = (list: KeyBinding[], chord: Chord) =>
    list.some((binding) => binding.action === action && sameChord(binding.chord, chord));
  const open = displaced.filter((binding) => holds(keys, binding.chord) && !holds(applied, binding.chord));
  return open.length
    ? open
        .map((binding) => t('keys.was_bound', { key: keyText(binding.chord), action: actionName(binding.action) }))
        .join(', ')
    : undefined;
}

/** A whole number above 0 from a field; empty (or anything else) = null, e.g. "device default". */
export function positiveOrNull(value: string): number | null {
  if (!/^\s*\d+\s*$/.test(value)) return null;
  const number = parseInt(value, 10);
  return number > 0 ? number : null;
}

/** A whole number from 0 from a field; empty = null (e.g. MSC off, as 0 is a device id). */
export function idOrNull(value: string): number | null {
  if (!/^\s*\d+\s*$/.test(value)) return null;
  return parseInt(value, 10);
}
