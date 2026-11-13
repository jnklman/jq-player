// Advance points (when the next cue starts) as the inspector offers them and
// the cue list's chain arrows show them.

import type { AdvanceAt, Cue } from './api';
import { fmtNumber, labels, t } from './i18n/index.svelte';

export type AdvanceKind = AdvanceAt['at'];

/** The points a cue type offers, in the order of the inspector's list. */
export function advancePoints(cue: Cue): AdvanceKind[] {
  switch (cue.type) {
    case 'sound':
      return ['start', 'after', 'after_fade_in', 'end', 'before_fade_out', 'before_end'];
    case 'playlist':
      return ['start', 'after', 'after_fade_in', 'end', 'before_end'];
    case 'group':
      return ['start', 'after', 'end'];
    case 'wait':
    case 'volume':
    case 'pan':
    case 'reset':
      return ['start', 'after', 'end', 'before_end'];
    case 'exit_loop':
      return ['start', 'after', 'loop_exit'];
    default:
      return ['start', 'after'];
  }
}

export const advanceLabels = labels<AdvanceKind>({
  start: 'advance.start',
  after: 'advance.after',
  after_fade_in: 'advance.after_fade_in',
  end: 'advance.end',
  before_fade_out: 'advance.before_fade_out',
  before_end: 'advance.before_end',
  loop_exit: 'advance.loop_exit',
});

/** A point of `kind`, keeping the seconds of `from` where both have some. */
export function advanceAt(kind: AdvanceKind, from?: AdvanceAt): AdvanceAt {
  const secs = from && 'secs' in from ? from.secs : undefined;
  switch (kind) {
    case 'after':
      return { at: kind, secs: secs ?? 5 };
    case 'before_end':
      return { at: kind, secs: secs ?? 2 };
    case 'loop_exit':
      return { at: kind, of: null };
    default:
      return { at: kind };
  }
}

/** Cue types that are over as soon as they fire. */
const INSTANT: Cue['type'][] = ['osc', 'midi', 'exit_loop', 'goto', 'start_stop', 'pause', 'position', 'stopwatch'];

/**
 * The point as the backend reads it for the cue's type (runner.rs `effective_at`); "before_fade_out" stays as it
 * is while the cue has an end fade.
 */
export function effectiveAt(cue: Cue): AdvanceAt {
  const at = cue.advance.at;
  const instant = INSTANT.includes(cue.type);
  switch (at.at) {
    case 'start':
    case 'after':
      return at;
    case 'after_fade_in':
      return (cue.type === 'sound' || cue.type === 'playlist') && cue.fade_in > 0 ? at : { at: 'start' };
    case 'loop_exit':
      return cue.type === 'exit_loop' ? at : instant ? { at: 'start' } : { at: 'end' };
    default:
      if (instant) return { at: 'start' };
      if (at.at === 'before_fade_out') {
        return cue.type === 'sound' && cue.end_fade && cue.fade_out > 0 ? at : { at: 'end' };
      }
      if (at.at === 'before_end' && !['sound', 'playlist', 'wait', 'volume', 'pan', 'reset'].includes(cue.type)) {
        return { at: 'end' };
      }
      return at;
  }
}

const secs = (value: number) => `${fmtNumber(Math.round(value * 10) / 10)} s`;

/** Short label on a chain arrow: "Start", "+5 s", "Fade-In", "End", "−2 s", "Fade-Out", "Vamp". */
export function advanceBadge(at: AdvanceAt): string {
  switch (at.at) {
    case 'start':
      return t('advance.badge_start');
    case 'after':
      return `+${secs(at.secs)}`;
    case 'after_fade_in':
      return t('advance.badge_fade_in');
    case 'end':
      return t('advance.badge_end');
    case 'before_fade_out':
      return t('advance.badge_fade_out');
    case 'before_end':
      return `−${secs(at.secs)}`;
    case 'loop_exit':
      return t('advance.badge_vamp');
  }
}

/** Sound cues inside a loop exit target (the target itself if it is one) whose loop it can leave. */
export function loopCues(target: Cue | undefined): Cue[] {
  if (!target) return [];
  if (target.type === 'group') return target.cues.flatMap(loopCues);
  return target.type === 'sound' && (target.vamps.length > 0 || target.loop === 'forever') ? [target] : [];
}
