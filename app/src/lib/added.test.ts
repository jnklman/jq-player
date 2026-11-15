import { describe, expect, it } from 'vitest';

import { cueAfter, firstAdded } from './added';
import type { Cue } from './api';

const cue = (id: string, cues?: Cue[]) => (cues ? { id, type: 'group', cues } : { id, type: 'wait' }) as unknown as Cue;

describe('firstAdded', () => {
  it('finds the new cue wherever it was inserted', () => {
    const before = [cue('a'), cue('b')];
    expect(firstAdded(before, [cue('a'), cue('n'), cue('b')])).toBe('n');
    expect(firstAdded(before, [cue('n'), cue('m'), cue('a'), cue('b')])).toBe('n');
  });

  it('ignores cues that moved or were removed meanwhile', () => {
    expect(firstAdded([cue('a'), cue('b')], [cue('b'), cue('n')])).toBe('n');
  });

  it('is undefined when nothing was added', () => {
    expect(firstAdded([cue('a')], [cue('a')])).toBeUndefined();
  });
});

describe('cueAfter', () => {
  const list = [cue('a'), cue('g', [cue('x'), cue('y')]), cue('b')];

  it('finds the next cue in the main list and inside groups', () => {
    expect(cueAfter(list, 'a')?.id).toBe('g');
    expect(cueAfter(list, 'x')?.id).toBe('y');
  });

  it('is undefined for the last cue or an unknown one', () => {
    expect(cueAfter(list, 'b')).toBeUndefined();
    expect(cueAfter(list, 'y')).toBeUndefined();
    expect(cueAfter(list, 'nope')).toBeUndefined();
  });
});
