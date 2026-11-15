import { describe, expect, it } from 'vitest';

import { trimTimes } from './waveform';

const times = { start: 0, end: null, vamps: [], marks: [] };

describe('trimTimes', () => {
  it('moves the start outwards to the millisecond', () => {
    expect(trimTimes(times, 'start', { start: 0.12345, end: 9 }, 10)).toEqual({ start: 0.123 });
  });

  it('moves the end outwards, or leaves it open when the sound lasts to the end', () => {
    expect(trimTimes(times, 'end', { start: 0, end: 9.0001 }, 10)).toEqual({ end: 9.001 });
    expect(trimTimes({ ...times, end: 8 }, 'end', { start: 0, end: null }, 10)).toEqual({ end: null });
  });

  it('never lets start and end cross', () => {
    expect(trimTimes({ ...times, end: 2 }, 'start', { start: 5, end: null }, 10)).toEqual({ start: 1.99 });
    expect(trimTimes({ ...times, start: 6 }, 'end', { start: 0, end: 3 }, 10)).toEqual({ end: 6.01 });
  });
});
