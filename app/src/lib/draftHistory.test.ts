import { describe, expect, it } from 'vitest';

import { DraftHistory } from './draftHistory';

describe('DraftHistory', () => {
  it('steps back and forward through recorded changes', () => {
    const history = new DraftHistory<{ start: number }>();
    history.record({ start: 0 }, { start: 1 });
    history.record({ start: 1 }, { start: 2 });
    expect(history.undo({ start: 2 })).toEqual({ start: 1 });
    expect(history.undo({ start: 1 })).toEqual({ start: 0 });
    expect(history.undo({ start: 0 })).toBeNull();
    expect(history.redo({ start: 0 })).toEqual({ start: 1 });
    expect(history.redo({ start: 1 })).toEqual({ start: 2 });
    expect(history.redo({ start: 2 })).toBeNull();
  });

  it('records no step for an unchanged draft', () => {
    const history = new DraftHistory<{ start: number }>();
    history.record({ start: 1 }, { start: 1 });
    expect(history.canUndo).toBe(false);
  });

  it('drops redo with a new change', () => {
    const history = new DraftHistory<{ start: number }>();
    history.record({ start: 0 }, { start: 1 });
    history.undo({ start: 1 });
    expect(history.canRedo).toBe(true);
    history.record({ start: 0 }, { start: 5 });
    expect(history.canRedo).toBe(false);
    expect(history.undo({ start: 5 })).toEqual({ start: 0 });
  });

  it('hands out copies', () => {
    const history = new DraftHistory<{ marks: number[] }>();
    const before = { marks: [1] };
    history.record(before, { marks: [1, 2] });
    before.marks.push(3);
    expect(history.undo({ marks: [1, 2] })).toEqual({ marks: [1] });
  });

  it('keeps at most `limit` steps', () => {
    const history = new DraftHistory<number>(2);
    history.record(0, 1);
    history.record(1, 2);
    history.record(2, 3);
    expect(history.undo(3)).toBe(2);
    expect(history.undo(2)).toBe(1);
    expect(history.undo(1)).toBeNull();
  });
});
