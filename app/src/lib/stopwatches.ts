// The show's stopwatches and countdowns as the header and the panel show them.

import type { Stopwatch } from './api';
import { fmtClock, fmtCountdown } from './format';
import { ui } from './store.svelte';

export interface WatchView extends Stopwatch {
  running: boolean;
  /** The time to show: elapsed, or a countdown's time left ("−0:12" in overtime). */
  time: string;
  /** A countdown that ran out. */
  overtime: boolean;
}

/** The show's stopwatches in its order, with their time; they run in the backend, also without the UI. */
export function watchViews(): WatchView[] {
  return (ui.view?.show.stopwatches ?? []).map((watch) => {
    const state = ui.runtime?.stopwatches?.find((s) => s.id === watch.id);
    const remaining = state?.remaining ?? watch.countdown ?? null;
    return {
      ...watch,
      running: state?.running ?? false,
      time: remaining != null ? fmtCountdown(remaining) : fmtClock(state?.elapsed ?? 0),
      overtime: remaining != null && Math.ceil(remaining - 1e-6) < 0,
    };
  });
}
