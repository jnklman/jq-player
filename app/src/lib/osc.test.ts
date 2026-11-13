import { describe, expect, it } from 'vitest';

import { formatOscArgs, parseOscArgs } from './osc';

describe('OSC arguments', () => {
  it('round-trips floats in every notation', () => {
    for (const value of [1, 0.5, -2, 1e-7, 1e21, 1.5e-9]) {
      const text = formatOscArgs([{ float: value }]);
      expect(parseOscArgs(text), text).toEqual([{ float: value }]);
    }
  });

  it('round-trips ints and strings', () => {
    const args = [{ int: 3 }, { string: 'two words' }, { string: '5' }, { string: 'x' }, { string: '' }];
    expect(parseOscArgs(formatOscArgs(args))).toEqual(args);
  });
});
