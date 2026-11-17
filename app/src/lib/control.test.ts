import { describe, expect, it } from 'vitest';

import { control, group, playlist, scene, show, sound, wait } from '../test/fixtures';
import type { AnyControl } from './control';
import {
  addGroups,
  controlStatus,
  isControl,
  mainListOnly,
  panLabel,
  targetable,
  targetMode,
  targetModes,
  typeLabel,
} from './control';
import { i18n } from './i18n/index.svelte';

describe('types', () => {
  it('tells control cues', () => {
    expect(isControl(control('v', 'volume'))).toBe(true);
    expect(isControl(control('w', 'stopwatch'))).toBe(true);
    expect(isControl(sound('s'))).toBe(false);
    expect(isControl(null)).toBe(false);
  });

  it('labels the type column', () => {
    expect(typeLabel(control('a', 'start_stop'))).toBe('Start');
    expect(typeLabel(control('b', 'start_stop', { action: 'stop' }))).toBe('Stop');
    expect(typeLabel(control('c', 'volume'))).toBe('Fade');
    expect(typeLabel(control('d', 'stopwatch'))).toBe('Clock');
    expect(typeLabel(sound('s'))).toBe('Sound');
    for (const kind of ['exit_loop', 'reset', 'pause', 'pan', 'rate', 'position'] as const) {
      expect(typeLabel(control(kind, kind))).not.toBe('');
    }
    for (const cue of [playlist('p', []), wait('w'), group('g', []), scene('s')]) {
      expect(typeLabel(cue)).not.toBe('');
    }
  });

  it('builds the add menu in the language shown', () => {
    const kinds = addGroups.flatMap((g) => g.items.map((item) => item.kind));
    expect(kinds).toContain('sound');
    expect(kinds).toContain('volume');
    expect(kinds.filter((kind) => kind === 'stopwatch')).toHaveLength(1);
    expect(addGroups[1].label).toBe('Control');
    expect(addGroups[1].items[2].label).toBe('Volume');
    i18n.locale = 'de';
    expect(addGroups[1].label).toBe('Steuerung');
    expect(addGroups[1].items[2].label).not.toBe('Volume');
  });

  it('keeps groups and scene markers in the main list', () => {
    expect(mainListOnly({ label: '', kind: 'group' })).toBe(true);
    expect(mainListOnly({ label: '', kind: 'scene' })).toBe(true);
    expect(mainListOnly({ label: '', kind: 'wait' })).toBe(false);
  });
});

describe('targets', () => {
  it('tells which cues a type can act on', () => {
    expect(targetable('start_stop', wait('w'))).toBe(true);
    expect(targetable('pause', scene('s'))).toBe(true);
    expect(targetable('exit_loop', sound('s'))).toBe(true);
    expect(targetable('exit_loop', playlist('p', []))).toBe(false);
    expect(targetable('volume', playlist('p', []))).toBe(true);
    expect(targetable('volume', wait('w'))).toBe(false);
  });

  it('offers chosen cues only for a start cue', () => {
    expect(targetModes('start_stop')).toEqual(['previous', 'cues']);
    expect(targetModes('volume')).toEqual(['previous', 'cues', 'all', 'output']);
    expect(targetMode('all')).toBe('all');
    expect(targetMode('previous')).toBe('previous');
    expect(targetMode(['a'])).toBe('cues');
    expect(targetMode({ output: 'main' })).toBe('output');
  });
});

describe('panLabel', () => {
  it('names the side and the amount', () => {
    expect(panLabel(0.001)).toBe('Center');
    expect(panLabel(-0.5)).toBe('L 50 %');
    expect(panLabel(1)).toBe('R 100 %');
  });
});

describe('controlStatus', () => {
  const a = sound('a', { number: '1', title: 'Music' });
  const b = sound('b', { number: '2' });
  const untitled = sound('c', { number: '' });
  const all = [a, b, untitled];
  const theShow = show(all, { stopwatches: [{ id: 'w1', name: 'Interval' }] });
  const status = (cue: ReturnType<typeof control>, previous: typeof a | null = a, missing = false) =>
    controlStatus(cue as AnyControl, previous ?? undefined, all, theShow, missing);

  it('names the targets', () => {
    expect(status(control('x', 'pause', { targets: 'all' })).text).toBe('→ all');
    expect(status(control('x', 'pause')).text).toBe('→ 1');
    expect(status(control('x', 'pause'), null)).toMatchObject({ text: 'no target', bad: true });
    expect(status(control('x', 'pause', { targets: { output: 'monitor' } })).text).toBe('→ Monitor');
    expect(status(control('x', 'pause', { targets: { output: 'gone' } })).text).toBe('→ gone');
    expect(status(control('x', 'pause', { targets: ['a'] })).text).toBe('→ 1 Music');
    expect(status(control('x', 'pause', { targets: ['a', 'c'] })).text).toBe('→ 1, ?');
    expect(status(control('x', 'pause', { targets: ['deleted'] }))).toMatchObject({ bad: true });
    expect(status(control('x', 'pause', { targets: ['a'] }), a, true).bad).toBe(true);
  });

  it('describes what each type does', () => {
    expect(status(control('x', 'start_stop', { from: 65 })).info).toBe('from 1:05');
    expect(status(control('x', 'start_stop')).info).toBeUndefined();
    expect(status(control('x', 'pause', { action: 'toggle' })).info).toBe('Toggle');
    expect(status(control('x', 'volume')).info).toBe('to −10.0 dB in 3.0 s');
    expect(status(control('x', 'volume', { duration: null })).info).toBe('to −10.0 dB');
    expect(status(control('x', 'volume', { action: 'fade_stop' })).info).toBe('out in 3.0 s');
    expect(status(control('x', 'volume', { action: 'fade_stop', duration: null })).info).toBe('target fade');
    expect(status(control('x', 'pan', { pan: -1, duration: 2 })).info).toBe('L 100 % in 2.0 s');
    expect(status(control('x', 'pan')).info).toBe('Center');
    expect(status(control('x', 'rate', { rate: 80, transpose: 2, duration: 1 })).info).toBe('80 %, +2 st in 1.0 s');
    expect(status(control('x', 'rate', { rate: null })).info).toBe('no change');
    expect(status(control('x', 'position')).info).toBe('to 0.0 s');
    expect(status(control('x', 'position', { to: { to: 'relative', secs: -5 } })).info).toBe('−5.0 s');
    expect(status(control('x', 'position', { to: { to: 'relative', secs: 5 } })).info).toBe('+5.0 s');
    expect(status(control('x', 'position', { to: { to: 'mark', index: 2 } })).info).toBe('mark 2');
    expect(status(control('x', 'position', { to: { to: 'next_mark' } })).info).toBe('next mark');
    expect(status(control('x', 'position', { to: { to: 'previous_mark' } })).info).toBe('previous mark');
    expect(status(control('x', 'exit_loop', { level: 'all' })).info).toBe('Loop end, all');
    expect(status(control('x', 'exit_loop', { mode: 'immediate' })).info).toBe('immediately');
    expect(status(control('x', 'reset')).info).toBe('at once');
    expect(status(control('x', 'reset', { duration: 4 })).info).toBe('in 4.0 s');
  });

  it('names the stopwatch of a stopwatch cue', () => {
    expect(status(control('x', 'stopwatch'))).toMatchObject({ text: 'no stopwatch', bad: true });
    expect(status(control('x', 'stopwatch', { watch: 'w1', action: 'reset' }))).toEqual({
      text: 'Interval',
      info: 'Reset',
    });
    expect(status(control('x', 'stopwatch', { watch: 'gone' }))).toMatchObject({
      text: 'Stopwatch missing',
      bad: true,
    });
  });
});
