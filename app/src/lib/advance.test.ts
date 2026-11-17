import { describe, expect, it } from 'vitest';

import { control, group, playlist, sound, wait } from '../test/fixtures';
import { advanceAt, advanceBadge, advancePoints, effectiveAt, loopCues } from './advance';
import type { AdvanceAt, Cue } from './api';
import { i18n } from './i18n/index.svelte';

const at = (cue: Cue, point: AdvanceAt): Cue => ({ ...cue, advance: { start_next: true, at: point } }) as Cue;

describe('advance points', () => {
  it('offers what suits each type', () => {
    expect(advancePoints(sound('s'))).toContain('before_fade_out');
    expect(advancePoints(playlist('p', []))).not.toContain('before_fade_out');
    expect(advancePoints(group('g', []))).toEqual(['start', 'after', 'end']);
    expect(advancePoints(wait('w'))).toEqual(['start', 'after', 'end', 'before_end']);
    expect(advancePoints(control('x', 'exit_loop'))).toContain('loop_exit');
    expect(advancePoints(control('x', 'pause'))).toEqual(['start', 'after']);
  });

  it('keeps the seconds of the previous point where both have some', () => {
    expect(advanceAt('after')).toEqual({ at: 'after', secs: 5 });
    expect(advanceAt('before_end')).toEqual({ at: 'before_end', secs: 2 });
    expect(advanceAt('before_end', { at: 'after', secs: 7 })).toEqual({ at: 'before_end', secs: 7 });
    expect(advanceAt('loop_exit')).toEqual({ at: 'loop_exit', of: null });
    expect(advanceAt('end', { at: 'after', secs: 7 })).toEqual({ at: 'end' });
  });
});

describe('effectiveAt', () => {
  it('keeps start and after as they are', () => {
    expect(effectiveAt(at(control('x', 'pause'), { at: 'after', secs: 3 }))).toEqual({ at: 'after', secs: 3 });
  });

  it('reads a fade-in point without a fade-in as the start', () => {
    expect(effectiveAt(at(sound('s', { fade_in: 2 }), { at: 'after_fade_in' }))).toEqual({ at: 'after_fade_in' });
    expect(effectiveAt(at(sound('s'), { at: 'after_fade_in' }))).toEqual({ at: 'start' });
    expect(effectiveAt(at(wait('w'), { at: 'after_fade_in' }))).toEqual({ at: 'start' });
  });

  it('reads a loop exit point on other cues as start or end', () => {
    const exit = { at: 'loop_exit', of: null } as const;
    expect(effectiveAt(at(control('x', 'exit_loop'), exit))).toEqual(exit);
    expect(effectiveAt(at(control('x', 'pause'), exit))).toEqual({ at: 'start' });
    expect(effectiveAt(at(sound('s'), exit))).toEqual({ at: 'end' });
  });

  it('reads end points of instant cues as the start', () => {
    expect(effectiveAt(at(control('x', 'position'), { at: 'end' }))).toEqual({ at: 'start' });
  });

  it('needs an end fade for "before fade-out"', () => {
    const fade = sound('s', { end_fade: true, fade_out: 2 });
    expect(effectiveAt(at(fade, { at: 'before_fade_out' }))).toEqual({ at: 'before_fade_out' });
    expect(effectiveAt(at(sound('s'), { at: 'before_fade_out' }))).toEqual({ at: 'end' });
  });

  it('reads "before end" as the end where the type has no such point', () => {
    expect(effectiveAt(at(group('g', []), { at: 'before_end', secs: 2 }))).toEqual({ at: 'end' });
    expect(effectiveAt(at(wait('w'), { at: 'before_end', secs: 2 }))).toEqual({ at: 'before_end', secs: 2 });
  });
});

describe('advanceBadge', () => {
  it('labels each point', () => {
    expect(advanceBadge({ at: 'start' })).toBe('Start');
    expect(advanceBadge({ at: 'after', secs: 5.04 })).toBe('+5 s');
    expect(advanceBadge({ at: 'after_fade_in' })).toBe('Fade-in');
    expect(advanceBadge({ at: 'end' })).toBe('End');
    expect(advanceBadge({ at: 'before_fade_out' })).toBe('Fade-out');
    expect(advanceBadge({ at: 'before_end', secs: 2.5 })).toBe('−2.5 s');
    expect(advanceBadge({ at: 'loop_exit', of: null })).toBe('Vamp');
  });

  it('follows the language', () => {
    i18n.locale = 'de';
    expect(advanceBadge({ at: 'after', secs: 2.5 })).toBe('+2,5 s');
  });
});

describe('loopCues', () => {
  it('finds sound cues with vamps or an endless loop, also in groups', () => {
    const vamped = sound('v', { vamps: [{ start: 1, end: 2 }] });
    const endless = sound('e', { loop: 'forever' });
    expect(loopCues(undefined)).toEqual([]);
    expect(loopCues(sound('plain'))).toEqual([]);
    const found = loopCues(group('g', [vamped, sound('plain'), group('h', [endless])]));
    expect(found.map((c) => c.id)).toEqual(['v', 'e']);
  });
});
