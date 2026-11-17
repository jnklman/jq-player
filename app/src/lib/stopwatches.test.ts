import { describe, expect, it } from 'vitest';

import { runtime, view } from '../test/fixtures';
import { watchViews } from './stopwatches';
import { ui } from './store.svelte';

describe('watchViews', () => {
  it('is empty without a show', () => {
    expect(watchViews()).toEqual([]);
  });

  it('shows elapsed time, a countdown and its overtime', () => {
    ui.view = view([], {
      show: {
        stopwatches: [
          { id: 'up', name: 'Show' },
          { id: 'down', name: 'Interval', countdown: 900 },
          { id: 'over', name: 'Over', countdown: 60 },
          { id: 'idle', name: 'Idle', countdown: 300 },
        ],
      },
    });
    ui.runtime = runtime({
      stopwatches: [
        { id: 'up', elapsed: 3725, remaining: null, running: true },
        { id: 'down', elapsed: 10, remaining: 890, running: true },
        { id: 'over', elapsed: 72, remaining: -12, running: false },
      ],
    });
    expect(watchViews().map(({ id, running, time, overtime }) => ({ id, running, time, overtime }))).toEqual([
      { id: 'up', running: true, time: '1:02:05', overtime: false },
      { id: 'down', running: true, time: '14:50', overtime: false },
      { id: 'over', running: false, time: '−0:12', overtime: true },
      { id: 'idle', running: false, time: '5:00', overtime: false },
    ]);
  });
});
