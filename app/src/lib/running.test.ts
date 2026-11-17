import { describe, expect, it } from 'vitest';

import { active, runtime } from '../test/fixtures';
import { runningList } from './running';

describe('runningList', () => {
  it('is empty without a state', () => {
    expect(runningList(null)).toEqual({ active: [], gaps: [], preWaits: [], following: [] });
  });

  it('lists waiting cues under their chain, else under what they wait for, else alone', () => {
    const list = runningList(
      runtime({
        active: [active('a'), active('b')],
        gaps: [{ cue: 'p', remaining: 1, next: { index: 1, file: 1, count: 2 }, paused: false }],
        waiting: [
          { cue: 'a2', remaining: 1, paused: false, root: 'a' },
          { cue: 'own', remaining: 2, paused: false, root: 'own' },
          { cue: 'orphan', remaining: 2, paused: false, root: 'gone' },
        ],
        following: [
          { cue: 'f1', after: 'x', root: 'a', paused: false, remaining: 3 },
          { cue: 'f2', after: 'b', root: 'gone', paused: false, remaining: null },
          { cue: 'f3', after: 'p', root: 'p', paused: false, remaining: 1 },
          { cue: 'f4', after: 'nothing', root: 'gone', paused: false, remaining: 1 },
        ],
      }),
    );
    const groups = list.active.map((g) => [g.state.cue, g.preWaits.map((w) => w.cue), g.following.map((f) => f.cue)]);
    expect(groups).toEqual([
      ['a', ['a2'], ['f1']],
      ['b', [], ['f2']],
    ]);
    expect(list.gaps[0].following.map((f) => f.cue)).toEqual(['f3']);
    expect(list.preWaits.map((w) => w.cue)).toEqual(['own', 'orphan']);
    expect(list.following.map((f) => f.cue)).toEqual(['f4']);
  });
});
