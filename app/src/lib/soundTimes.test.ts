import { describe, expect, it } from 'vitest';

import { nextMarkAt, nextVamp } from './soundTimes';

describe('nextMarkAt', () => {
  it('puts a mark 1 s after the last one, or after the start', () => {
    expect(nextMarkAt([], 2, null)).toBe(3);
    expect(nextMarkAt([4, 7], 0, 20)).toBe(8);
  });

  it('stays within the end', () => {
    expect(nextMarkAt([9.5], 0, 10)).toBe(10);
    expect(nextMarkAt([10], 0, 10)).toBeNull();
  });
});

describe('nextVamp', () => {
  it('adds 4 s after the last vamp, or at the start', () => {
    expect(nextVamp([], 1, null)).toEqual({ start: 1, end: 5 });
    expect(nextVamp([{ start: 2, end: 6 }], 0, 30)).toEqual({ start: 6, end: 10 });
  });

  it('is cut at the end, and none fits behind it', () => {
    expect(nextVamp([{ start: 2, end: 8 }], 0, 10)).toEqual({ start: 8, end: 10 });
    expect(nextVamp([{ start: 2, end: 10 }], 0, 10)).toBeNull();
  });
});
