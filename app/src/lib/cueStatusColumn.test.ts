// The status column for each cue type, warnings and badges (see also cueStatus.test.ts).

import { describe, expect, it } from 'vitest';

import { active, control, goto, group, info, playlist, runtime, sound, view, wait } from '../test/fixtures';
import type { Cue, CueInfo, RuntimeState, ShowView } from './api';
import { cueStatus, cueWarnings, repeatBadge, rowRunning } from './cueStatus';
import { ui } from './store.svelte';

function showing(cues: Cue[], infos: Record<string, Partial<CueInfo>> = {}, extra: Partial<ShowView> = {}) {
  const v = view(cues);
  for (const [id, patch] of Object.entries(infos)) v.cue_info[id] = info(patch);
  ui.view = { ...v, ...extra };
}

describe('a running cue', () => {
  const s = sound('s');

  it('shows its times and what it is doing', () => {
    showing([s]);
    const at = (state: Partial<RuntimeState['active'][number]>) => {
      ui.runtime = runtime({ active: [active('s', state)] });
      return cueStatus(s);
    };
    expect(at({})).toMatchObject({ text: '10.0 s / −50.0 s', progress: 1 / 6, warn: false });
    expect(at({ remaining: 5 }).warn).toBe(true);
    expect(at({ paused: true })).toMatchObject({ info: 'Paused', paused: true });
    expect(at({ loading: true }).info).toBe('loading');
    expect(at({ item: { index: 1, file: 0, count: 3 } }).info).toBe('2/3');
    expect(at({ pass: { pass: 2, count: 4, vamp: null } }).info).toBe('Loop 2/4');
    expect(at({ looping: true })).toMatchObject({ info: 'Loop', vamp: true });
    // Between two loop passes the bar shows the silence.
    expect(at({ progress: 1, loop_gap: { remaining: 1.25, progress: 0.4 } })).toMatchObject({
      info: 'Pause 1.3 s',
      progress: 0.4,
      silent: true,
    });
  });

  it('shows fades, playlist gaps and pre-waits', () => {
    showing([s]);
    ui.runtime = runtime({ fading: [{ cue: 's', elapsed: 1, remaining: 2, progress: 0.3, paused: true }] });
    expect(cueStatus(s)).toMatchObject({ text: '1.0 s / −2.0 s', info: 'Paused', progress: 0.3 });
    ui.runtime = runtime({ gaps: [{ cue: 's', remaining: 2, next: { index: 2, file: 2, count: 4 }, paused: false }] });
    expect(cueStatus(s)).toMatchObject({ text: 'Gap 2.0 s', info: '3/4' });
    ui.runtime = runtime({ gaps: [{ cue: 's', remaining: 2, next: { index: 2, file: 2, count: 4 }, paused: true }] });
    expect(cueStatus(s).info).toBe('Paused');
    ui.runtime = runtime({ waiting: [{ cue: 's', remaining: 1.5, paused: false, root: 's' }] });
    expect(cueStatus(s)).toMatchObject({ text: 'Pre-wait 1.5 s', info: undefined });
    ui.runtime = runtime({ waiting: [{ cue: 's', remaining: 1.5, paused: true, root: 's' }] });
    expect(cueStatus(s).info).toBe('Paused');
  });

  it('is tinted only while it plays, fades or waits in a gap', () => {
    const g = group('g', [s]);
    showing([g]);
    ui.runtime = runtime({ waiting: [{ cue: 's', remaining: 1, paused: false, root: 's' }] });
    expect(rowRunning(s)).toBe(false);
    expect(rowRunning(g)).toBe(true);
    ui.runtime = runtime({ active: [active('s')] });
    expect(rowRunning(s)).toBe(true);
  });
});

describe('a resting cue', () => {
  it('shows nothing without a show', () => {
    expect(cueStatus(sound('s'))).toEqual({ text: '' });
  });

  it('shows a sound cue by its file', () => {
    const cues = [
      sound('ok', { rate: 80 }),
      sound('p', { file: '' }),
      sound('gone'),
      sound('copy'),
      sound('loop', { loop: 'forever', transpose: 2 }),
      sound('vamp', { vamps: [{ start: 1, end: 2 }] }),
      sound('probe'),
      sound('unknown'),
    ];
    showing(cues, {
      gone: { missing: true },
      copy: { importing: true },
      probe: { duration: null, probing: true },
      unknown: { duration: null },
    });
    ui.runtime = runtime({ imports: [{ file: 'copy.wav', progress: 0.25 }] });
    const status = Object.fromEntries(cues.map((cue) => [cue.id, cueStatus(cue, false)]));
    expect(status.ok).toEqual({ text: '1:00', info: '80 %' });
    expect(status.p).toEqual({ text: 'no file', info: 'Placeholder' });
    expect(status.gone).toEqual({ text: 'File missing', bad: true });
    expect(status.copy).toEqual({ text: 'being copied', info: '25 %', progress: 0.25, copying: true });
    expect(status.loop).toEqual({ text: '∞', info: 'Loop · +2 st' });
    expect(status.vamp).toEqual({ text: '∞', info: 'Vamp' });
    expect(status.probe.text).toBe('…');
    expect(status.unknown.text).toBe('—');
  });

  it('shows a copy without progress yet at 0 %', () => {
    showing([sound('c')], { c: { importing: true } });
    expect(cueStatus(sound('c')).info).toBe('0 %');
  });

  it('shows waits, groups and playlists', () => {
    const list = playlist('p', ['a', 'b'], { rate: 90 });
    const g = group('g', [wait('w1'), wait('w2')], { mode: 'sequence' });
    showing([wait('w', { duration: 2.5 }), g, list]);
    expect(cueStatus(wait('w', { duration: 2.5 }))).toEqual({ text: '2.5 s' });
    expect(cueStatus(g)).toMatchObject({ text: '2 cues', info: 'in sequence' });
    expect(cueStatus(list)).toEqual({ text: '1:00', info: '2 items · 90 %' });
    expect(cueStatus({ ...list, repeat: true }).text).toBe('∞');

    showing([list], { p: { missing: true } });
    expect(cueStatus(list)).toEqual({ text: 'File missing', bad: true });
    showing([list], { p: { importing: true } });
    expect(cueStatus(list).text).toBe('being copied');
    showing([list], { p: { duration: null, probing: true } });
    expect(cueStatus(list).text).toBe('…');
    showing([list], { p: { duration: null } });
    expect(cueStatus(list).text).toBe('—');
  });

  it('shows MIDI and OSC messages', () => {
    const midi = (messages: unknown[]) => ({ ...wait('m'), type: 'midi', messages }) as unknown as Cue;
    const osc = (device: string | null) =>
      ({ ...wait('o'), type: 'osc', device, messages: [{ address: '/go', args: [] }] }) as unknown as Cue;
    showing([], {}, {});
    ui.view!.show.devices = [{ id: 'd', name: 'Desk', host: 'x', port: 1 }];
    expect(cueStatus(midi([]))).toEqual({ text: '0 messages', bad: true });
    expect(cueStatus(midi([{ kind: 'program_change', channel: 0, program: 3 }])).text).toBe('PC 3 · Channel 1');
    expect(cueStatus(midi([{}, {}])).text).toBe('2 messages');
    expect(cueStatus(osc('d')).text).toBe('→ Desk · /go');
    expect(cueStatus({ ...osc('d'), messages: [] } as unknown as Cue).text).toBe('→ Desk · –');
    expect(cueStatus(osc(null))).toEqual({ text: 'no device', bad: true });
  });

  it('shows the target of a go-to cue', () => {
    const target = sound('t', { number: '5', title: 'Storm' });
    const inner = sound('i', { number: '7' });
    showing([target, group('g', [inner])]);
    expect(cueStatus(goto('x', null))).toEqual({ text: 'no target', bad: true });
    expect(cueStatus(goto('x', 't'))).toEqual({ text: '→ 5 Storm', info: 'selects' });
    expect(cueStatus(goto('x', 't', { start: true })).info).toBe('starts');
    expect(cueStatus(goto('x', 'i'))).toEqual({ text: '7 in group', bad: true });
    expect(cueStatus(goto('x', 'deleted'))).toEqual({ text: 'target missing', bad: true });
  });

  it('shows the target of a control cue, and nothing for a scene marker', () => {
    const s = sound('s', { number: '1' });
    const fade = control('f', 'volume');
    showing([s, fade], { f: { target_missing: true } });
    expect(cueStatus(fade)).toMatchObject({ text: '→ 1', bad: true });
    expect(cueStatus({ ...wait('x'), type: 'scene' } as Cue)).toEqual({ text: '' });
  });
});

describe('cueWarnings', () => {
  it('lists what the cue cannot play as set', () => {
    const s = sound('s');
    const g = group('g', []);
    const p = playlist('p', []);
    showing([s, g, p], {
      s: {
        missing: true,
        timing: [{ kind: 'end_past_file', message: 'End past file' }],
        problems: ['Unknown output'],
        case_mismatch: 'S.WAV',
        level_silenced: true,
        level_capped: true,
      },
      g: { missing: true },
      p: { level_capped: true },
    });
    expect(cueWarnings(s)).toEqual([
      'File missing',
      'End past file',
      'Unknown output',
      expect.stringContaining('S.WAV'),
      "Level below the show's minimum (−60.0 dB): the cue is silent",
      "Level above the show's maximum: plays at +6.0 dB",
    ]);
    expect(cueWarnings(g)).toEqual(['A file in the group is missing']);
    expect(cueWarnings(p)).toEqual(["Level above the show's maximum: items play at +6.0 dB"]);
    expect(cueWarnings(wait('none'))).toEqual([]);
  });
});

describe('repeatBadge', () => {
  it('counts the passes on the playhead cue, else names how many', () => {
    showing([]);
    expect(repeatBadge(sound('s'))).toBeNull();
    expect(repeatBadge(sound('s', { passes: 1 }))).toBeNull();
    expect(repeatBadge(sound('s', { passes: 3 }))).toBe('×3');
    ui.runtime = runtime({ playhead: 's', repeat: { done: 2, count: null } });
    expect(repeatBadge(sound('s', { passes: 'forever' }))).toBe('2/∞');
    expect(repeatBadge(sound('t', { passes: 'forever' }))).toBe('×∞');
  });
});
