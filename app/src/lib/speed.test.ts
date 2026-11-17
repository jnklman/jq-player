import { describe, expect, it } from 'vitest';

import { i18n } from './i18n/index.svelte';
import { clampRate, clampTranspose, rateLabel, speedChanged, speedOf, speedSummary, transposeLabel } from './speed';

describe('speed', () => {
  it('fills in the defaults the backend leaves out', () => {
    expect(speedOf({})).toEqual({ rate: 100, preserve_pitch: true, transpose: 0 });
    const own = { rate: 80, preserve_pitch: false, transpose: -2 };
    expect(speedOf(own)).toEqual(own);
  });

  it('tells a changed speed or pitch', () => {
    expect(speedChanged({})).toBe(false);
    expect(speedChanged({ preserve_pitch: false })).toBe(false);
    expect(speedChanged({ rate: 101 })).toBe(true);
    expect(speedChanged({ transpose: 1 })).toBe(true);
  });

  it('clamps to the backend limits', () => {
    expect(clampRate(10)).toBe(50);
    expect(clampRate(200)).toBe(150);
    expect(clampRate(99)).toBe(99);
    expect(clampTranspose(-20)).toBe(-12);
    expect(clampTranspose(13)).toBe(12);
  });

  it('labels rate and transposition', () => {
    expect(rateLabel(102.54)).toBe('102.5 %');
    expect(transposeLabel(0.001)).toBe('±0 st');
    expect(transposeLabel(3)).toBe('+3 st');
    expect(transposeLabel(-1.5)).toBe('−1.5 st');
    i18n.locale = 'de';
    expect(transposeLabel(-1.5)).toBe('−1,5 HT');
  });

  it('summarises only what differs', () => {
    expect(speedSummary({})).toBe('');
    expect(speedSummary({ rate: 80 })).toBe('80 %');
    expect(speedSummary({ transpose: 3 })).toBe('+3 st');
    expect(speedSummary({ rate: 80, transpose: 3 })).toBe('80 % · +3 st');
  });
});
