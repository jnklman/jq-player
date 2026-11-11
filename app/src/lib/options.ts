// Choices offered in more than one place (inspector, multi-edit, control
// cues, show menu), so the same setting reads the same everywhere.

import type {
  Cue,
  DevampMode,
  ExitLoop,
  GroupMode,
  OthersEffect,
  PauseFields,
  Repeat,
  SecondTrigger,
  ShowView,
  Shuffle,
  VolumeAction,
} from './api';
import { othersLabels, secondTriggerLabels } from './cue';
import { options } from './i18n/index.svelte';

/** An entry of a `SimpleSelect`. */
export interface Option<T extends string = string> {
  value: T;
  label: string;
  disabled?: boolean;
}

/** Options from a record of labels (see `labels`), in its order; the labels are read when shown. */
export function optionsOf<T extends string>(labels: Record<T, string>): Option<T>[] {
  return (Object.keys(labels) as T[]).map((value) => ({
    value,
    get label() {
      return labels[value];
    },
  }));
}

/** How often something repeats, as a choice: not, a number of times (a second field), or forever. */
export type CountKind = 'off' | 'times' | 'forever';

/** The kind of a loop or repeat count; a count of 0 or 1 is "off". */
export function countKind(count: Repeat): CountKind {
  return count === 'forever' ? 'forever' : count > 1 ? 'times' : 'off';
}

/** The count a newly chosen kind starts with; `current` is kept if it already is a number of times. */
export function countOf(kind: CountKind, current?: Repeat): Repeat {
  if (kind === 'forever') return 'forever';
  if (kind === 'off') return 0;
  return current !== undefined && countKind(current) === 'times' ? current : 2;
}

/** A sound cue's loop kind; other cues don't loop. */
export function loopKind(cue: Cue): CountKind {
  return cue.type === 'sound' ? countKind(cue.loop) : 'off';
}

export const loopOptions = options<CountKind>({
  off: 'options.off',
  times: 'options.loop.times',
  forever: 'options.loop.forever',
});

export const repeatOptions = options<CountKind>({
  off: 'options.off',
  times: 'options.repeat.times',
  forever: 'options.repeat.forever',
});

export const groupModeOptions = options<GroupMode>({
  all: 'options.group_mode.all',
  sequence: 'options.group_mode.sequence',
});

export const shuffleOptions = options<Shuffle>({
  off: 'options.off',
  per_start: 'options.shuffle.per_start',
  per_pass: 'options.shuffle.per_pass',
});

export const volumeActionOptions = options<VolumeAction>({
  fade_stop: 'options.volume_action.fade_stop',
  fade_to: 'options.volume_action.fade_to',
});

export const pauseActionOptions = options<PauseFields['action']>({
  pause: 'options.pause_action.pause',
  resume: 'options.pause_action.resume',
  toggle: 'options.pause_action.toggle',
});

export const devampModeOptions = options<DevampMode>({
  loop_end: 'options.devamp.loop_end',
  next_mark: 'options.devamp.next_mark',
  immediate: 'options.devamp.immediate',
});

export const exitLevelOptions = options<ExitLoop['level']>({
  innermost: 'options.exit_level.innermost',
  all: 'options.exit_level.all',
});

export const othersScopeOptions = options<OthersEffect['scope']>({
  all: 'options.others_scope.all',
  output: 'options.others_scope.output',
});

export const othersOptions = optionsOf<OthersEffect['effect']>(othersLabels);
export const secondTriggerOptions = optionsOf<SecondTrigger['action']>(secondTriggerLabels);

/** The show's outputs. */
export function outputOptions(view: ShowView): Option[] {
  return view.show.outputs.map((output) => ({ value: output.id, label: output.name }));
}

/** `options`, plus `value` as its own entry `label` if none of them has it (a value gone from the show). */
export function withMissing<T extends string>(options: Option<T>[], value: T | null | undefined, label: string) {
  return value && !options.some((option) => option.value === value) ? [...options, { value, label }] : options;
}
