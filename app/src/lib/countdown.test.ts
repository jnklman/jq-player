import { describe, expect, it } from 'vitest';

import { fmtCountdown } from './format';
import { parseClock } from './input';

describe('countdown times', () => {
  it('reads minutes, m:ss and h:mm:ss', () => {
    expect(parseClock('15')).toBe(900);
    expect(parseClock('2:30')).toBe(150);
    expect(parseClock('1:30:00')).toBe(5400);
    expect(parseClock('1,5')).toBe(90);
    for (const bad of ['', 'x', '1::2', '-3', '1:2:3:4']) expect(parseClock(bad)).toBeNull();
  });

  it('counts down rounded up, then on with a minus', () => {
    expect(fmtCountdown(900)).toBe('15:00');
    expect(fmtCountdown(0.4)).toBe('0:01');
    expect(fmtCountdown(0)).toBe('0:00');
    expect(fmtCountdown(-0.4)).toBe('0:00');
    expect(fmtCountdown(-1.2)).toBe('−0:01');
    expect(fmtCountdown(-3700)).toBe('−1:01:40');
  });
});
