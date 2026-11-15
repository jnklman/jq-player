// Where the inspector's "+ Mark" and "+ Vamp" put a new mark or vamp of a sound cue: after the last one, but never
// past the end it plays to (`end`: the cue's end, else the file's length; null while unknown).

import type { Vamp } from './api';

/** 1 s after the last mark (or the start), at most `end`; null when there is no room left. */
export function nextMarkAt(marks: number[], start: number, end: number | null): number | null {
  const after = marks.at(-1) ?? start;
  const at = end == null ? after + 1 : Math.min(after + 1, end);
  return at > after ? at : null;
}

/** A 4 s vamp after the last one (or at the start), cut at `end`; null when there is no room left. */
export function nextVamp(vamps: Vamp[], start: number, end: number | null): Vamp | null {
  const from = vamps.at(-1)?.end ?? start;
  const to = end == null ? from + 4 : Math.min(from + 4, end);
  return to > from ? { start: from, end: to } : null;
}
