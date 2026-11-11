// The running cues as the "Laufende Cues" panel lists them: each playing cue
// or playlist gap with the cues its chain waits to start under it.

import type { CueId, FollowState, RuntimeState } from './api';

export type PreWait = RuntimeState['waiting'][number];

export interface RunningGroup<T> {
  state: T;
  /** Cues of its chain in their pre-wait. */
  preWaits: PreWait[];
  /** Cues of its chain still to start. */
  following: FollowState[];
}

export interface RunningList {
  active: RunningGroup<RuntimeState['active'][number]>[];
  gaps: RunningGroup<RuntimeState['gaps'][number]>[];
  /** Pre-waits of no running chain (their own GO started them). */
  preWaits: PreWait[];
  /** Waiting cues of no running chain. */
  following: FollowState[];
}

/**
 * A waiting cue is listed under the running cue its chain started with, else
 * under the one it waits for; cues in their pre-wait likewise under their
 * chain's first cue, else on their own.
 */
export function runningList(runtime: RuntimeState | null): RunningList {
  const active = runtime?.active ?? [];
  const gaps = runtime?.gaps ?? [];
  const waiting = runtime?.waiting ?? [];
  const following = runtime?.following ?? [];
  const running = (id: CueId) => active.some((state) => state.cue === id) || gaps.some((gap) => gap.cue === id);
  const anchor = (follow: FollowState) =>
    running(follow.root) ? follow.root : running(follow.after) ? follow.after : null;
  const group = <T extends { cue: CueId }>(state: T): RunningGroup<T> => ({
    state,
    preWaits: waiting.filter((wait) => wait.root === state.cue && wait.cue !== state.cue),
    following: following.filter((follow) => anchor(follow) === state.cue),
  });
  return {
    active: active.map(group),
    gaps: gaps.map(group),
    preWaits: waiting.filter((wait) => wait.root === wait.cue || !running(wait.root)),
    following: following.filter((follow) => anchor(follow) === null),
  };
}
