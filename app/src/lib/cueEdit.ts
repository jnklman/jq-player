// What the inspector edits: one cue (its draft) or several selected cues.
// Its fields read and write through a `CueEdit`, so the same fields serve
// both; with several cues a field shows `MIXED` where their values differ.

import type { Cue } from './api';
import { speedOf } from './speed';

/** A field's value when the edited cues differ in it. */
export const MIXED = Symbol('gemischt');
export type Mixed = typeof MIXED;

export type Patch = object;

export interface CueEdit {
  /** The edited cues: the draft of one cue, or the selected cues. */
  readonly cues: Cue[];
  /** More than one cue: per-cue fields (file, hotkey, …) are left out. */
  readonly multi: boolean;
  /** Show mode: nothing can be changed. */
  readonly locked: boolean;
  /** The value all cues share, or `MIXED`. The caller names the type it expects. */
  value<T>(key: string): T | Mixed;
  /**
   * Writes `patch` (or, per cue, what the function returns) into every cue as one undo step. `live` (a slider
   * being dragged, one cue only): running cues follow without an undo step; the drag ends with `set({})`.
   */
  set(patch: Patch | ((cue: Cue) => Patch), live?: boolean): void;
}

/** A field of `cue`; rate, pitch and transposition with their defaults (the backend leaves them out). */
function read(cue: Cue, key: string): unknown {
  if (cue.type === 'sound' || cue.type === 'playlist') {
    const speed = speedOf(cue) as Record<string, unknown>;
    if (key in speed) return speed[key];
  }
  return (cue as unknown as Record<string, unknown>)[key];
}

/** The value of `key` all `cues` share, or `MIXED`. */
export function shared(cues: Cue[], key: string): unknown {
  if (cues.length === 0) return undefined;
  const first = JSON.stringify(read(cues[0], key));
  return cues.every((cue) => JSON.stringify(read(cue, key)) === first) ? read(cues[0], key) : MIXED;
}

export const isMixed = (value: unknown): value is Mixed => value === MIXED;

/** For an input: the value, or null when mixed. */
export function shown<T>(value: T | Mixed): T | null {
  return value === MIXED ? null : value;
}

/** The patch for `cue`. */
export function patchFor(patch: Patch | ((cue: Cue) => Patch), cue: Cue): Patch {
  return typeof patch === 'function' ? patch(cue) : patch;
}
