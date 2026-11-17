import { describe, expect, it } from 'vitest';

import { midiKinds, midiMessageOf, midiSummary } from './midi';

describe('midiSummary', () => {
  it('shows the raw program 0 … 127 and channels from 1', () => {
    expect(midiSummary({ kind: 'program_change', channel: 0, program: 0 })).toBe('PC 0 · Channel 1');
    expect(midiSummary({ kind: 'program_change', channel: 9, program: 127 })).toBe('PC 127 · Channel 10');
  });
});

describe('midiMessageOf', () => {
  it('starts each kind with defaults, keeping the channel', () => {
    const from = { kind: 'note_on', channel: 4, note: 1, velocity: 2 } as const;
    expect(midiMessageOf('program_change', from)).toEqual({ kind: 'program_change', channel: 4, program: 0 });
    expect(midiMessageOf('control_change')).toEqual({ kind: 'control_change', channel: 0, controller: 0, value: 127 });
    expect(midiMessageOf('note_on', from)).toEqual({ kind: 'note_on', channel: 4, note: 60, velocity: 127 });
    expect(midiMessageOf('note_off', from)).toEqual({ kind: 'note_off', channel: 4, note: 60 });
    expect(midiMessageOf('msc', from)).toEqual({ kind: 'msc', device: 127, command: 'go', cue: '', list: '' });
    expect(midiKinds.map((k) => k.value)).toHaveLength(5);
  });

  it('summarises every kind', () => {
    expect(midiSummary({ kind: 'control_change', channel: 0, controller: 7, value: 100 })).toBe('CC 7=100 · Channel 1');
    expect(midiSummary({ kind: 'note_on', channel: 1, note: 60, velocity: 1 })).toBe('Note 60 on · Channel 2');
    expect(midiSummary({ kind: 'note_off', channel: 1, note: 60 })).toBe('Note 60 off · Channel 2');
    expect(midiSummary({ kind: 'msc', device: 1, command: 'go', cue: '12.5', list: '' })).toBe('MSC GO 12.5');
    expect(midiSummary({ kind: 'msc', device: 1, command: 'stop', cue: '', list: '' })).toBe('MSC STOP (next)');
  });
});
