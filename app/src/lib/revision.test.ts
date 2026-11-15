import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { Revisions } from './revision';

describe('Revisions', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  /** Whether `promise` has settled by now (after pending microtasks). */
  async function settled(promise: Promise<unknown>): Promise<boolean> {
    let done = false;
    void promise.then(() => (done = true));
    await vi.advanceTimersByTimeAsync(0);
    return done;
  }

  it('takes newer views and the same one again, drops older ones', () => {
    const revisions = new Revisions();
    expect(revisions.accept(0)).toBe(true);
    expect(revisions.accept(3)).toBe(true);
    expect(revisions.accept(3)).toBe(true);
    expect(revisions.accept(2)).toBe(false);
    expect(revisions.current).toBe(3);
  });

  it('resolves at once when the view is already shown (event before reply)', async () => {
    const revisions = new Revisions();
    revisions.accept(5);
    expect(await settled(revisions.reached(5, 2000))).toBe(true);
    expect(await settled(revisions.reached(4, 2000))).toBe(true);
  });

  it('waits for the view when the reply comes first', async () => {
    const revisions = new Revisions();
    revisions.accept(1);
    const waiting = revisions.reached(2, 2000);
    expect(await settled(waiting)).toBe(false);
    revisions.accept(2);
    expect(await settled(waiting)).toBe(true);
  });

  it('a later view also counts (several changes in a row)', async () => {
    const revisions = new Revisions();
    const first = revisions.reached(2, 2000);
    const second = revisions.reached(3, 2000);
    revisions.accept(3);
    expect(await settled(first)).toBe(true);
    expect(await settled(second)).toBe(true);
  });

  it('wakes only those whose view has come', async () => {
    const revisions = new Revisions();
    const early = revisions.reached(2, 2000);
    const late = revisions.reached(4, 2000);
    revisions.accept(2);
    expect(await settled(early)).toBe(true);
    expect(await settled(late)).toBe(false);
    revisions.accept(4);
    expect(await settled(late)).toBe(true);
  });

  it('never hangs when the view does not come', async () => {
    const revisions = new Revisions();
    const waiting = revisions.reached(9, 2000);
    await vi.advanceTimersByTimeAsync(1999);
    expect(await settled(waiting)).toBe(false);
    await vi.advanceTimersByTimeAsync(1);
    expect(await settled(waiting)).toBe(true);
    // Given up on: a view coming later changes nothing for it, and no timer is left behind.
    revisions.accept(9);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('clears the timer of a wait that was answered', async () => {
    const revisions = new Revisions();
    const waiting = revisions.reached(2, 2000);
    revisions.accept(2);
    await waiting;
    expect(vi.getTimerCount()).toBe(0);
  });
});
