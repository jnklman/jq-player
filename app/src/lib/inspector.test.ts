import { describe, expect, it } from 'vitest';

import { control, goto, group, info, playlist, scene, sound, wait } from '../test/fixtures';
import type { Cue } from './api';
import { inspectorTabs, inspectorView, noticesOf, shownTab } from './inspector.svelte';

const ids = (cues: Cue[], topLevel = true) => inspectorTabs(cues, topLevel).map((tab) => tab.id);
const labels = (cues: Cue[]) => inspectorTabs(cues, true).map((tab) => tab.label);

describe('inspectorTabs', () => {
  it('has none without cues and only notes for a scene marker', () => {
    expect(ids([])).toEqual([]);
    expect(ids([scene('s'), sound('a')])).toEqual(['notes']);
  });

  it('gives a sound cue every tab, the trigger only at the top level', () => {
    expect(ids([sound('a')])).toEqual(['main', 'sound', 'flow', 'trigger', 'notes']);
    expect(ids([wait('w')], false)).toEqual(['main', 'flow', 'notes']);
    expect(labels([sound('a')])[0]).toBe('Playback');
  });

  it('names the first tab after what the type does', () => {
    expect(labels([control('v', 'volume')])[0]).toBe('Action');
    expect(labels([goto('g', null)])[0]).not.toBe('Action');
    expect(ids([control('v', 'stopwatch')])).toContain('main');
  });

  it('keeps for several cues only what they share', () => {
    expect(ids([wait('a'), wait('b')])).toEqual(['main', 'flow', 'notes']);
    expect(ids([sound('a'), playlist('p', [])])).toEqual(['main', 'sound', 'flow', 'trigger', 'notes']);
    expect(ids([sound('a'), group('g', [])])).toEqual(['main', 'flow', 'trigger', 'notes']);
    expect(ids([sound('a'), wait('w')])).toEqual(['flow', 'notes']);
    expect(ids([goto('a', null), goto('b', null)])).toEqual(['flow', 'notes']);
  });
});

describe('shownTab', () => {
  it('keeps the tab chosen last where the cues have it', () => {
    const tabs = inspectorTabs([sound('a')], true);
    inspectorView.tab = 'sound';
    expect(shownTab(tabs)).toBe('sound');
    expect(shownTab(inspectorTabs([wait('w')], true))).toBe('main');
    expect(shownTab([])).toBe('main');
    inspectorView.tab = 'main';
  });
});

describe('noticesOf', () => {
  it('lists every problem with the field to fix it in', () => {
    const notices = noticesOf(
      sound('a'),
      info({
        problems: [{ kind: 'output_unknown', output: 'gone' }],
        missing: true,
        case_mismatch: 'Rain.WAV',
        timing: [{ kind: 'start_past_end' }],
        normalize_stale: true,
        level_silenced: true,
        level_capped: true,
        repeat_blocked: true,
      }),
      6,
    );
    expect(notices.map((n) => [n.tab, n.field])).toEqual([
      [undefined, undefined],
      ['main', 'file'],
      ['main', 'file'],
      ['main', 'range'],
      ['sound', 'normalize'],
      ['sound', 'volume'],
      ['sound', 'volume'],
      ['flow', 'repeat'],
    ]);
    expect(notices[0].text).toBe('Output “gone” does not exist in the show – plays on channels 1/2');
    expect(notices[2].text).toContain('Rain.WAV');
    expect(notices[6].text).toContain('+6.0 dB');
  });

  it('points a missing target to the field of the type', () => {
    const missing = info({ target_missing: true });
    expect(noticesOf(goto('g', 'x'), missing, 6)[0].field).toBe('target');
    expect(noticesOf(control('w', 'stopwatch'), missing, 6)[0].field).toBe('watch');
    expect(noticesOf(control('e', 'exit_loop'), missing, 6)[0]).toMatchObject({ tab: 'flow', field: 'next' });
    expect(noticesOf(control('v', 'volume'), missing, 6)).toEqual([]);
  });

  it('has nothing to say without info', () => {
    expect(noticesOf(sound('a'), undefined, 6)).toEqual([]);
  });
});
