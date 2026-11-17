import { describe, expect, it } from 'vitest';

import { sound, wait } from '../test/fixtures';
import { isMixed, MIXED, patchFor, shared, shown } from './cueEdit';

describe('shared values', () => {
  it('gives the common value or MIXED', () => {
    expect(shared([], 'volume_db')).toBeUndefined();
    expect(shared([sound('a', { volume_db: -3 }), sound('b', { volume_db: -3 })], 'volume_db')).toBe(-3);
    expect(shared([sound('a', { volume_db: -3 }), sound('b')], 'volume_db')).toBe(MIXED);
    expect(shared([sound('a', { vamps: [{ start: 1, end: 2 }] }), sound('b')], 'vamps')).toBe(MIXED);
  });

  it('reads rate and pitch with their defaults', () => {
    expect(shared([sound('a'), sound('b', { rate: 100 })], 'rate')).toBe(100);
    expect(shared([sound('a'), sound('b', { preserve_pitch: false })], 'preserve_pitch')).toBe(MIXED);
    expect(shared([wait('w')], 'duration')).toBe(5);
  });

  it('shows MIXED as an empty field', () => {
    expect(isMixed(MIXED)).toBe(true);
    expect(isMixed(null)).toBe(false);
    expect(shown(MIXED)).toBeNull();
    expect(shown(4)).toBe(4);
  });

  it('builds the patch per cue', () => {
    const cue = sound('a', { volume_db: -6 });
    expect(patchFor({ pan: 1 }, cue)).toEqual({ pan: 1 });
    expect(patchFor((c) => ({ title: `${c.id}!` }), cue)).toEqual({ title: 'a!' });
  });
});
