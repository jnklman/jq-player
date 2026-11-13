import { describe, expect, it } from 'vitest';

import { midiSummary } from './midi';

describe('midiSummary', () => {
  it('counts programs and channels from 1, like a learned trigger', () => {
    expect(midiSummary({ kind: 'program_change', channel: 0, program: 0 })).toBe('PC 1 · Channel 1');
    expect(midiSummary({ kind: 'program_change', channel: 9, program: 127 })).toBe('PC 128 · Channel 10');
  });
});
