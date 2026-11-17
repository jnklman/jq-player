import { describe, expect, it } from 'vitest';

import { chord as key, settings, view } from '../test/fixtures';
import type { Chord, ControlAction, KeyBinding } from './api';
import type { AudioSetup } from './audioSetup.svelte';
import {
  ACTION_GROUPS,
  actionName,
  bindKey,
  changedFields,
  displacedText,
  idOrNull,
  keysChanged,
  keysOf,
  positiveOrNull,
  resetKeys,
  SETTINGS_GROUPS,
  settingsProblem,
  tabProblems,
  tabTitle,
  withEdits,
} from './settings';

const chord = (code: string): Chord => ({ code, ctrl: false, shift: false, alt: false, label: code });
const bind = (code: string, action: string): KeyBinding => ({ chord: chord(code), action: action as ControlAction });

describe('positiveOrNull', () => {
  it('takes whole numbers above 0 only', () => {
    expect(positiveOrNull('5')).toBe(5);
    expect(positiveOrNull(' 12 ')).toBe(12);
    expect(positiveOrNull('')).toBeNull();
    expect(positiveOrNull('0')).toBeNull();
    expect(positiveOrNull('12abc')).toBeNull();
    expect(positiveOrNull('1.5')).toBeNull();
  });
});

describe('idOrNull', () => {
  it('takes 0 and whole numbers only', () => {
    expect(idOrNull('0')).toBe(0);
    expect(idOrNull('7x')).toBeNull();
    expect(idOrNull('')).toBeNull();
  });
});

describe('bindKey', () => {
  it('does not duplicate a chord the action has in another slot', () => {
    const keys = [bind('KeyA', 'go'), bind('KeyB', 'go')];
    const { keys: next } = bindKey(keys, 'go' as ControlAction, 1, chord('KeyA'), []);
    expect(keysOf(next, 'go' as ControlAction).map((c) => c.code)).toEqual(['KeyA']);
  });

  it('moves a chord of another action here and reports it', () => {
    const keys = [bind('KeyA', 'go'), bind('KeyB', 'stop')];
    const { keys: next, displaced } = bindKey(keys, 'go' as ControlAction, 1, chord('KeyB'), []);
    expect(keysOf(next, 'go' as ControlAction).map((c) => c.code)).toEqual(['KeyA', 'KeyB']);
    expect(keysOf(next, 'stop' as ControlAction)).toEqual([]);
    expect(displaced).toHaveLength(1);
  });

  it('removes a key with null and refuses fixed keys', () => {
    const keys = [bind('KeyA', 'go'), bind('KeyB', 'go')];
    expect(keysOf(bindKey(keys, 'go', 0, null, []).keys, 'go').map((c) => c.code)).toEqual(['KeyB']);
    const refused = bindKey(keys, 'go', 0, key('Delete'), [key('Delete')]);
    expect(refused.keys).toBe(keys);
    expect(refused.error).toBe('Del is reserved and cannot be assigned');
  });
});

describe('default keys', () => {
  const defaults = [bind('Space', 'go'), bind('KeyS', 'stop_all')];

  it('tells keys that differ from the defaults, order aside', () => {
    expect(keysChanged([bind('Space', 'go')], defaults, 'go')).toBe(false);
    expect(keysChanged([bind('Enter', 'go')], defaults, 'go')).toBe(true);
    expect(keysChanged([], defaults, 'go')).toBe(true);
  });

  it('brings back the defaults, taking them from other actions', () => {
    const { keys, displaced } = resetKeys([bind('Space', 'panic'), bind('Enter', 'go')], defaults, 'go');
    expect(keys).toEqual([bind('Space', 'go')]);
    expect(displaced).toEqual([bind('Space', 'panic')]);
  });

  it('notes taken keys until they are applied', () => {
    const displaced = [bind('Space', 'panic')];
    const keys = [bind('Space', 'go')];
    expect(displacedText(displaced, 'go', keys, [])).toBe('Space was assigned to “Panic”');
    expect(displacedText(displaced, 'go', keys, keys)).toBeUndefined();
    expect(displacedText(displaced, 'go', [], [])).toBeUndefined();
  });
});

describe('the settings dialog', () => {
  it('names groups, tabs and action groups in the language shown', () => {
    expect(SETTINGS_GROUPS.flatMap((g) => g.tabs.map((tab) => tab.id))).toEqual(
      expect.arrayContaining(['device', 'routing', 'show', 'keys', 'midi', 'osc', 'ui', 'media']),
    );
    for (const group of SETTINGS_GROUPS) {
      expect(group.title).toBeTruthy();
      for (const tab of group.tabs) expect(tab.title).toBe(tabTitle(tab.id));
    }
    for (const group of ACTION_GROUPS) {
      expect(group.title).toBeTruthy();
      void group.scope;
      void group.keywords;
    }
    expect(ACTION_GROUPS.flatMap((g) => g.actions)).toContain('focus_goto');
    expect(actionName('go')).toBe('GO');
    expect(actionName('nope' as ControlAction)).toBe('nope');
  });

  it('sends only the fields the dialog edits', () => {
    const current = settings({ layout: { x: 1 }, fade_all: 2 });
    const draft = settings({ layout: null, fade_all: 5, last_show: '/x' });
    const next = withEdits(current, draft);
    expect(next.fade_all).toBe(5);
    expect(next.layout).toEqual({ x: 1 });
    expect(next.last_show).toBeNull();
  });

  it('counts the changed fields per tab', () => {
    const initial = settings();
    const draft = settings({ fade_all: 9, panic_fade: 0, outputs: { main: 3 }, language: 'de' });
    expect(Object.fromEntries(changedFields(draft, initial))).toEqual({ show: 2, routing: 1, ui: 1 });
    expect(changedFields(initial, initial).size).toBe(0);
  });

  it('marks the tabs with problems', () => {
    const setup = (extra: Partial<AudioSetup> = {}) =>
      ({
        tooManyChannels: false,
        mainLimit: 2,
        previewLimit: 2,
        previewMode: 'off',
        previewOverlaps: false,
        ...extra,
      }) as AudioSetup;
    expect(tabProblems(view(), settings(), setup()).size).toBe(0);
    const broken = view([], {
      audio_error: 'gone',
      midi: { error: 'busy' },
      osc: { error: 'port in use' },
      preview: { error: 'no device' },
    });
    expect([...tabProblems(broken, settings(), setup())].sort()).toEqual(['device', 'midi', 'osc', 'routing']);
    // Outputs beyond the channels, a preview pair that does not fit.
    expect(tabProblems(view(), settings({ outputs: { main: 3 } }), setup()).has('routing')).toBe(true);
    const preview = settings({ preview_channel: 2 });
    expect(tabProblems(view(), preview, setup({ previewMode: 'main' })).has('routing')).toBe(true);
    expect(tabProblems(view(), settings(), setup({ tooManyChannels: true })).has('device')).toBe(true);
  });
});

describe('settingsProblem', () => {
  it('accepts the defaults', () => {
    expect(settingsProblem(settings())).toBeNull();
  });

  it('names the first field that is out of range or empty', () => {
    expect(settingsProblem(settings({ fade_all: -1 }))).toBe('FADE ALL time must be a number ≥ 0');
    expect(settingsProblem(settings({ jump_end: 0 }))).toBe('Jump times must be greater than 0');
    expect(settingsProblem(settings({ jump_back: null as unknown as number }))).toBe(
      'Jump times must be greater than 0',
    );
    expect(settingsProblem(settings({ go_lockout: -0.1 }))).toBe('GO lockout must be a number ≥ 0');
    expect(settingsProblem(settings({ pan_step: 0 }))).toBe('Pan step must be greater than 0');
    expect(settingsProblem(settings({ rate_step: NaN }))).toBe('Rate step must be greater than 0');
  });
});
