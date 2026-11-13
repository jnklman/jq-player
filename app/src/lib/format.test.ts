import { describe, expect, it } from 'vitest';

import { fmtDb, fmtLufs, fmtMegabytes, fmtTime } from './format';

describe('fmtTime', () => {
  it('turns over to the next minute instead of printing 60', () => {
    expect(fmtTime(119.7)).toBe('2:00');
    expect(fmtTime(59.96)).toBe('1:00');
    expect(fmtTime(59.94)).toBe('59.9 s');
    expect(fmtTime(59.5)).toBe('59.5 s');
    expect(fmtTime(60)).toBe('1:00');
    expect(fmtTime(60.4)).toBe('1:00');
    expect(fmtTime(3599.6)).toBe('60:00');
  });

  it('shows plain values and the unset ones', () => {
    expect(fmtTime(0)).toBe('0.0 s');
    expect(fmtTime(-3)).toBe('0.0 s');
    expect(fmtTime(65)).toBe('1:05');
    expect(fmtTime(null)).toBe('∞');
  });
});

describe('rounded units', () => {
  it('carries a rounded size into the whole numbers', () => {
    expect(fmtMegabytes(9.96 * (1 << 20))).toBe('10 MB');
    expect(fmtMegabytes(5 * (1 << 20))).toBe('5.0 MB');
  });

  it('shows no sign for a level that rounds to zero', () => {
    expect(fmtDb(-0.04)).toBe('0.0 dB');
    expect(fmtDb(-3)).toBe('−3.0 dB');
    expect(fmtDb(0.5)).toBe('+0.5 dB');
    expect(fmtLufs(-0.01)).toBe('0.0 LUFS');
  });
});
