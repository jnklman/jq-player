import { describe, expect, it } from 'vitest';

import { dbToFader, faderSteps, faderToDb, roundFaderDb, SILENCE_DB } from './fader';

const DEFAULT = { max_db: 6, min_db: -60 };

describe('fader taper', () => {
  it('puts unity at three quarters and the limits at the ends', () => {
    expect(dbToFader(0, DEFAULT)).toBeCloseTo(0.75);
    expect(faderToDb(0.75, DEFAULT)).toBeCloseTo(0);
    expect(dbToFader(6, DEFAULT)).toBe(1);
    expect(faderToDb(1, DEFAULT)).toBeCloseTo(6);
    expect(faderToDb(0, DEFAULT)).toBe(SILENCE_DB);
    expect(dbToFader(-60, DEFAULT)).toBe(0);
    expect(dbToFader(-200, DEFAULT)).toBe(0);
  });

  it('clamps positions and levels beyond the travel', () => {
    expect(faderToDb(-1, DEFAULT)).toBe(SILENCE_DB);
    expect(faderToDb(2, DEFAULT)).toBeCloseTo(6);
    expect(dbToFader(50, DEFAULT)).toBe(1);
  });

  it('maps a level back to the position it came from, also between other limits', () => {
    for (const limits of [DEFAULT, { max_db: 12, min_db: -80 }, { max_db: 0, min_db: -40 }]) {
      for (const db of [-35, -12.5, -0.3, limits.max_db]) {
        expect(faderToDb(dbToFader(db, limits), limits)).toBeCloseTo(db, 6);
      }
    }
  });

  it('rounds to the fader resolution', () => {
    expect(roundFaderDb(-3.04)).toBe(-3);
    expect(roundFaderDb(-3.06)).toBe(-3.1);
  });

  it('has a step for every tenth of a dB and the value itself', () => {
    const steps = faderSteps(DEFAULT, -3.333);
    expect(steps).toContain(0);
    expect(steps).toContain(dbToFader(-3.333, DEFAULT));
    expect(steps).toHaveLength(2 + 661);
    expect(Math.max(...steps)).toBe(1);
  });
});
