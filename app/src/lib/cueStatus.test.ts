import { afterEach, describe, expect, it } from 'vitest';

import type { Cue, PassState, RuntimeState, ShowView } from './api';
import { canLink, loopLabel, passLabel } from './cue';
import { chainArrow, chainWaiting, cueStatus } from './cueStatus';
import { fmtNumber, i18n } from './i18n/index.svelte';
import { isRunning, selection, ui } from './store.svelte';

const wait = (id: string, extra: object = {}) =>
  ({ id, type: 'wait', duration: 5, advance: { start_next: false, at: { at: 'end' } }, ...extra }) as unknown as Cue;
const group = (id: string, cues: Cue[]) => ({ id, type: 'group', mode: 'all', cues }) as unknown as Cue;
const view = (cues: Cue[]) => ({ show: { cues, devices: [] }, cue_info: {}, settings: {} }) as unknown as ShowView;
const runtime = (state: Partial<RuntimeState>) =>
  ({ active: [], fading: [], gaps: [], waiting: [], following: [], imports: [], ...state }) as unknown as RuntimeState;

afterEach(() => {
  ui.view = null;
  ui.runtime = null;
  ui.selected = null;
  ui.marked = [];
  i18n.locale = 'en';
});

describe('selection', () => {
  it('lists the selected cues in list order, leaving out unknown ones', () => {
    ui.view = view([wait('a'), wait('b'), wait('c')]);
    ui.selected = 'c';
    ui.marked = ['x', 'a'];
    expect(selection()).toEqual(['a', 'c']);
  });
});

describe('cue status', () => {
  it('reads the runtime state only for a live cue', () => {
    const inner = wait('w');
    const g = group('g', [inner]);
    ui.view = view([g]);
    ui.runtime = runtime({ active: [{ cue: 'w' }] as unknown as RuntimeState['active'] });
    expect(isRunning(g)).toBe(true);
    expect(cueStatus(g).text).toBe('running');
    // A row passes `live: false` only when `isRunning` says so; then the resting text.
    expect(cueStatus(g, false).text).toBe('1 cue');
  });

  it('splits the chain arrow label from its waiting state', () => {
    const a = wait('a', { advance: { start_next: true, at: { at: 'start' } } });
    const b = wait('b');
    expect(chainArrow(a, b)).toBe('Start');
    expect(chainArrow(b, a)).toBeNull();
    expect(chainWaiting(a, b)).toBe(false);
    ui.runtime = runtime({ following: [{ after: 'a', cue: 'b' }] as unknown as RuntimeState['following'] });
    expect(chainWaiting(a, b)).toBe(true);
  });
});

describe('loop labels', () => {
  it('come from the catalogs', () => {
    const vamped = { id: 's', type: 'sound', vamps: [{ start: 0, end: 1 }] } as unknown as Cue;
    expect(loopLabel(vamped)).toBe('Vamp');
    expect(loopLabel(wait('w'))).toBe('Loop');
    expect(passLabel({ pass: 2, count: 3 } as PassState)).toBe('Loop 2/3');
    expect(passLabel({ pass: 4, count: null, vamp: 1 } as unknown as PassState)).toBe('Vamp 1 · 4/∞');
  });
});

describe('linking', () => {
  it('excludes groups and scene markers, as the backend does', () => {
    expect(canLink(wait('w'))).toBe(true);
    expect(canLink(group('g', []))).toBe(false);
    expect(canLink({ id: 's', type: 'scene' } as unknown as Cue)).toBe(false);
    expect(canLink(undefined)).toBe(false);
  });
});

describe('fmtNumber', () => {
  it('follows the language with its cached formatters', () => {
    expect(fmtNumber(2.5)).toBe('2.5');
    expect(fmtNumber(2.5, 2)).toBe('2.50');
    i18n.locale = 'de';
    expect(fmtNumber(2.5)).toBe('2,5');
    expect(fmtNumber(1234.5, 1)).toBe('1234,5');
  });
});
