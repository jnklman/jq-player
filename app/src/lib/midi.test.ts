import { describe, expect, it } from 'vitest';

import { midiSummary } from './midi';

describe('midiSummary', () => {
  it('shows the raw program 0 … 127 and channels from 1', () => {
    expect(midiSummary({ kind: 'program_change', channel: 0, program: 0 })).toBe('PC 0 · Channel 1');
    expect(midiSummary({ kind: 'program_change', channel: 9, program: 127 })).toBe('PC 127 · Channel 10');
  });
});
