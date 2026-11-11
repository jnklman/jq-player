// Control cues (start/stop, pause, volume, pan, rate, position, loop exit,
// reset, stopwatch): names, the add menu and what the cue list shows of them.

import type { ControlCue, ControlKind, Cue, PanCurve, PositionTo, Show, StopwatchCue, Targets } from './api';
import { fmtDb, fmtSecs, fmtTime } from './format';
import { labels, options, t, type TextKey } from './i18n/index.svelte';
import { rateLabel, transposeLabel } from './speed';

export type AnyControl = ControlCue | StopwatchCue;

/** Control types in menu order, with their name. */
export const controlLabels = labels<ControlKind>({
  start_stop: 'control.start_stop',
  pause: 'control.pause',
  volume: 'control.volume',
  pan: 'control.pan',
  rate: 'control.rate',
  position: 'control.position',
  exit_loop: 'control.exit_loop',
  reset: 'control.reset',
  stopwatch: 'control.stopwatch',
});

export function isControl(cue: Cue | null | undefined): cue is AnyControl {
  return !!cue && cue.type in controlLabels;
}

/** Short type name in the list's type column. */
export function typeLabel(cue: Cue): string {
  switch (cue.type) {
    case 'start_stop':
      return cue.action === 'start' ? t('type.start') : t('type.stop');
    case 'volume':
      return t('type.fade');
    case 'exit_loop':
      return t('type.exit_loop');
    case 'reset':
      return t('type.reset');
    case 'stopwatch':
      return t('type.stopwatch');
    default:
      return otherLabels[cue.type];
  }
}

const otherLabels = labels({
  sound: 'type.sound',
  playlist: 'type.playlist',
  wait: 'type.wait',
  osc: 'type.osc',
  midi: 'type.midi',
  group: 'type.group',
  goto: 'type.goto',
  scene: 'type.scene',
  pause: 'type.pause',
  pan: 'type.pan',
  rate: 'type.rate',
  position: 'type.position',
});

/** Inspector title per type. */
export const typeTitles = labels<Cue['type']>({
  sound: 'type_title.sound',
  playlist: 'type_title.playlist',
  group: 'type_title.group',
  wait: 'type_title.wait',
  osc: 'type_title.osc',
  midi: 'type_title.midi',
  goto: 'type_title.goto',
  scene: 'type_title.scene',
  start_stop: 'type_title.start_stop',
  pause: 'type_title.pause',
  volume: 'type_title.volume',
  pan: 'type_title.pan',
  rate: 'type_title.rate',
  position: 'type_title.position',
  exit_loop: 'type_title.exit_loop',
  reset: 'type_title.reset',
  stopwatch: 'type_title.stopwatch',
});

export const typeColors: Record<Cue['type'], string> = {
  sound: 'text-primary',
  playlist: 'text-primary',
  wait: 'text-fg-2',
  osc: 'text-go',
  midi: 'text-go',
  group: 'text-fg',
  goto: 'text-fg-2',
  scene: 'text-fg',
  start_stop: 'text-go',
  pause: 'text-pause',
  volume: 'text-fade',
  pan: 'text-fade',
  rate: 'text-fade',
  position: 'text-progress',
  exit_loop: 'text-vamp',
  reset: 'text-fade',
  stopwatch: 'text-fg-2',
};

/** The add menu's groups; `kind` names what to add. */
export type AddItem =
  | { label: string; kind: ControlKind }
  | {
      label: string;
      kind: 'sound' | 'placeholder' | 'playlist' | 'wait' | 'group' | 'goto' | 'scene' | 'osc' | 'midi';
    };

/** Add-menu kinds that only go into the main list, never into a group. */
export function mainListOnly(item: AddItem): boolean {
  return item.kind === 'group' || item.kind === 'scene';
}

/** An entry of the add menu; its label follows the language. */
function addItem<K extends AddItem['kind']>(key: TextKey, kind: K): { label: string; kind: K } {
  return {
    get label() {
      return t(key);
    },
    kind,
  };
}

export const addGroups: { label: string; items: AddItem[] }[] = [
  {
    get label() {
      return t('add.audio');
    },
    items: [
      addItem('add.sound', 'sound'),
      addItem('add.placeholder', 'placeholder'),
      addItem('add.playlist', 'playlist'),
    ],
  },
  {
    get label() {
      return t('add.control');
    },
    items: (Object.keys(controlLabels) as ControlKind[])
      .filter((kind) => kind !== 'stopwatch')
      .map((kind) => ({
        get label() {
          return controlLabels[kind];
        },
        kind,
      })),
  },
  {
    get label() {
      return t('add.flow');
    },
    items: [
      addItem('add.scene', 'scene'),
      addItem('add.wait', 'wait'),
      addItem('add.group', 'group'),
      addItem('add.goto', 'goto'),
      addItem('add.stopwatch', 'stopwatch'),
    ],
  },
  {
    get label() {
      return t('add.external');
    },
    items: [addItem('add.osc', 'osc'), addItem('add.midi', 'midi')],
  },
];

/** Which cues a control type can target (besides itself). */
export function targetable(kind: ControlKind, cue: Cue): boolean {
  switch (kind) {
    case 'start_stop':
    case 'pause':
      return true;
    case 'exit_loop':
      return cue.type === 'sound' || cue.type === 'group';
    default:
      return cue.type === 'sound' || cue.type === 'playlist' || cue.type === 'group';
  }
}

/** Target modes a control type offers. A start needs chosen cues. */
export function targetModes(kind: ControlKind): ('previous' | 'cues' | 'all' | 'output')[] {
  return kind === 'start_stop' ? ['previous', 'cues'] : ['previous', 'cues', 'all', 'output'];
}

export const targetModeLabels = labels<'previous' | 'cues' | 'all' | 'output'>({
  previous: 'target_mode.previous',
  cues: 'target_mode.cues',
  all: 'target_mode.all',
  output: 'target_mode.output',
});

export function targetMode(targets: Targets): 'previous' | 'cues' | 'all' | 'output' {
  if (targets === 'all' || targets === 'previous') return targets;
  return Array.isArray(targets) ? 'cues' : 'output';
}

export const panCurveOptions = options<PanCurve>({ s_curve: 'pan_curve.s_curve', linear: 'pan_curve.linear' });

/** "L 50 %", "Center", "R 100 %". */
export function panLabel(pan: number): string {
  if (Math.abs(pan) < 0.005) return t('pan.center');
  return `${pan < 0 ? 'L' : 'R'} ${Math.round(Math.abs(pan) * 100)} %`;
}

export const positionLabels = labels<PositionTo['to']>({
  absolute: 'position.absolute',
  relative: 'position.relative',
  mark: 'position.mark',
  next_mark: 'position.next_mark',
  previous_mark: 'position.previous_mark',
});

function positionInfo(to: PositionTo): string {
  switch (to.to) {
    case 'absolute':
      return t('position.info_absolute', { time: fmtTime(to.secs) });
    case 'relative':
      return `${to.secs < 0 ? '−' : '+'}${fmtSecs(Math.abs(to.secs))}`;
    case 'mark':
      return t('position.info_mark', { index: to.index });
    case 'next_mark':
      return t('position.info_next_mark');
    case 'previous_mark':
      return t('position.info_previous_mark');
  }
}

const exitKeys = {
  loop_end: 'exit.loop_end',
  next_mark: 'exit.next_mark',
  immediate: 'exit.immediate',
} as const satisfies Record<string, TextKey>;

/** "→ 3", "→ 3, 5", "→ all", "→ Main", "→ 2" (previous entry); `bad` without a target. */
function targetsText(targets: Targets, previous: Cue | undefined, all: Cue[], show: Show) {
  if (targets === 'all') return { text: t('target.all') };
  if (targets === 'previous') {
    return previous ? { text: `→ ${previous.number || previous.title}` } : noTarget();
  }
  if (!Array.isArray(targets)) {
    return { text: `→ ${show.outputs.find((o) => o.id === targets.output)?.name ?? targets.output}` };
  }
  const named = targets.map((id) => all.find((cue) => cue.id === id)).filter((cue) => cue !== undefined);
  if (named.length === 0) return noTarget();
  if (named.length === 1) return { text: `→ ${named[0].number} ${named[0].title}`.trimEnd() };
  return { text: `→ ${named.map((cue) => cue.number || '?').join(', ')}` };
}

const noTarget = () => ({ text: t('status.no_target'), bad: true });

/** What the cue list shows for a control cue that is not running. */
export function controlStatus(
  cue: AnyControl,
  previous: Cue | undefined,
  all: Cue[],
  show: Show,
  targetMissing: boolean,
): { text: string; info?: string; bad?: boolean } {
  if (cue.type === 'stopwatch') {
    const watch = show.stopwatches.find((w) => w.id === cue.watch);
    const action = t(`stopwatch_action.${cue.action}`);
    if (!cue.watch) return { text: t('status.no_stopwatch'), bad: true };
    return watch ? { text: watch.name, info: action } : { text: t('status.stopwatch_missing'), bad: true };
  }
  const target = targetsText(cue.targets, previous, all, show);
  const bad = ('bad' in target && target.bad) || targetMissing;
  const info = (() => {
    switch (cue.type) {
      case 'start_stop':
        return cue.action === 'start' && cue.from != null ? t('control.from', { time: fmtTime(cue.from) }) : undefined;
      case 'pause':
        return t(`pause_action.${cue.action}`);
      case 'volume':
        if (cue.action === 'fade_stop') {
          return cue.duration != null
            ? t('control.fade_out_in', { time: fmtSecs(cue.duration) })
            : t('control.target_fade');
        }
        return cue.duration != null
          ? t('control.to_level_in', { level: fmtDb(cue.volume_db), time: fmtSecs(cue.duration) })
          : t('control.to_level', { level: fmtDb(cue.volume_db) });
      case 'pan':
        return `${panLabel(cue.pan)}${cue.duration > 0 ? ` ${t('control.in', { time: fmtSecs(cue.duration) })}` : ''}`;
      case 'rate': {
        const parts = [
          cue.rate != null ? rateLabel(cue.rate) : null,
          cue.transpose != null ? transposeLabel(cue.transpose) : null,
        ].filter((part) => part !== null);
        const what = parts.length > 0 ? parts.join(', ') : t('control.no_change');
        return `${what}${cue.duration > 0 ? ` ${t('control.in', { time: fmtSecs(cue.duration) })}` : ''}`;
      }
      case 'position':
        return positionInfo(cue.to);
      case 'exit_loop':
        return `${t(exitKeys[cue.mode])}${cue.level === 'all' ? t('control.exit_all') : ''}`;
      case 'reset':
        return cue.duration > 0 ? t('control.in', { time: fmtSecs(cue.duration) }) : t('control.at_once');
    }
  })();
  return { text: target.text, info, bad };
}
