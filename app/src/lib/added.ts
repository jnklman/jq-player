// Finding the cues an edit created, from the show before and after it, so a command can select them
// whatever else changed in between.

import type { Cue, CueId } from './api';

/** The first cue of `after` that is not in `before` (by id): the first one an add created. */
export function firstAdded(before: readonly Cue[], after: readonly Cue[]): CueId | undefined {
  const known = new Set(before.map((cue) => cue.id));
  return after.find((cue) => !known.has(cue.id))?.id;
}

/** The cue right after `id` in its list, also inside groups: where a duplicate puts its copy. */
export function cueAfter(list: readonly Cue[], id: CueId): Cue | undefined {
  const at = list.findIndex((cue) => cue.id === id);
  if (at >= 0) return list[at + 1];
  for (const cue of list) {
    if (cue.type !== 'group') continue;
    const inner = cueAfter(cue.cues, id);
    if (inner) return inner;
  }
}
