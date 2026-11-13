import { describe, expect, it } from 'vitest';

import { parseNumber, plainNumber } from './input';

describe('parseNumber', () => {
  it('reads decimals with a point or comma and a typographic minus', () => {
    expect(parseNumber(' 1,5 ')).toBe(1.5);
    expect(parseNumber('−2')).toBe(-2);
    expect(parseNumber('.5')).toBe(0.5);
    expect(parseNumber('1e3')).toBe(1000);
  });

  it('rejects what is no plain number', () => {
    for (const bad of ['', ' ', '0x10', '1_0', 'Infinity', '1-2', '1,2,3', '1e', '--1']) {
      expect(parseNumber(bad), bad).toBeNull();
    }
  });
});

describe('plainNumber', () => {
  it('drops floating-point noise', () => {
    expect(plainNumber(0.1 + 0.2)).toBe('0.3');
    expect(plainNumber(12.5)).toBe('12.5');
    expect(plainNumber(-3)).toBe('-3');
  });
});
