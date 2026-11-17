import { describe, expect, it } from 'vitest';

import { group, info, playlist, scene, sound, wait } from '../test/fixtures';
import {
  canLink,
  cueLabel,
  curveOptions,
  fadeGains,
  hasOverrides,
  isPlaceholder,
  linkedCues,
  loopLabel,
  passLabel,
  vampSpans,
  withItem,
  withoutNormalization,
} from './cue';

describe('fadeGains', () => {
  it('runs every curve from silence to the level and back', () => {
    for (const { value } of curveOptions) {
      const gains = fadeGains(value, -60);
      expect(gains.in(0), value).toBeCloseTo(0, 2);
      expect(gains.in(1), value).toBeCloseTo(1);
      expect(gains.out(0), value).toBeCloseTo(1);
      expect(gains.out(1), value).toBe(0);
    }
  });

  it('shapes the decibel curves between the show minimum and the level', () => {
    const top = fadeGains('decibel_slow_top', -60);
    const bottom = fadeGains('decibel_slow_bottom', -60);
    // Slow near the louder end: the fade-out holds at first, the slow-bottom one drops faster.
    expect(top.out(0.25)).toBeGreaterThan(bottom.out(0.25));
    expect(fadeGains('decibel', -60).in(0.5)).toBeCloseTo(10 ** (-30 / 20));
    // A floor above −6 dB is held at −6.
    expect(fadeGains('decibel', 0).in(0.5)).toBeCloseTo(10 ** (-3 / 20));
  });

  it('fades linear curves in amplitude', () => {
    expect(fadeGains('linear', -60).in(0.5)).toBe(0.5);
    expect(fadeGains('s_curve', -60).out(0.5)).toBe(0.5);
    expect(fadeGains('nope' as never, -60).in(0.5)).toBe(0.5);
  });
});

describe('vampSpans', () => {
  it('places vamps within the played range', () => {
    const cue = sound('s', {
      start: 10,
      end: 30,
      vamps: [
        { start: 15, end: 20 },
        { start: 5, end: 40 },
      ],
    });
    expect(vampSpans(cue, info())).toEqual([
      { left: 0.25, width: 0.25 },
      { left: 0, width: 1 },
    ]);
  });

  it('takes the file length without an end, and gives nothing without a range', () => {
    const cue = sound('s', { vamps: [{ start: 30, end: 45 }] });
    expect(vampSpans(cue, info({ file_duration: 60 }))).toEqual([{ left: 0.5, width: 0.25 }]);
    expect(vampSpans(cue, info({ file_duration: null }))).toEqual([]);
    expect(vampSpans(sound('s', { start: 5, end: 5, vamps: [{ start: 1, end: 2 }] }), info())).toEqual([]);
    expect(vampSpans(sound('s'), info())).toEqual([]);
    expect(vampSpans(wait('w'), info())).toEqual([]);
  });
});

describe('labels', () => {
  it('names passes, loops and vamps', () => {
    expect(passLabel({ pass: 2, count: 3, vamp: null })).toBe('Loop 2/3');
    expect(passLabel({ pass: 4, count: null, vamp: 1 })).toBe('Vamp 1 · 4/∞');
    expect(loopLabel(sound('s', { vamps: [{ start: 1, end: 2 }] }))).toBe('Vamp');
    expect(loopLabel(sound('s'))).toBe('Loop');
    expect(loopLabel(undefined)).toBe('Loop');
  });

  it('names a cue by number, else by title', () => {
    expect(cueLabel(sound('s', { number: '4' }))).toBe('4');
    expect(cueLabel(sound('s', { title: 'Rain' }))).toBe('“Rain”');
    expect(cueLabel(sound('s', { title: '' }))).toBe('“untitled”');
  });
});

describe('links', () => {
  it('lists the other instances of a cue', () => {
    const a = sound('a', { link: 'L' });
    const b = sound('b', { link: 'L' });
    const c = sound('c', { link: 'M' });
    expect(linkedCues(a, [a, b, c]).map((cue) => cue.id)).toEqual(['b']);
    expect(linkedCues(sound('x'), [a, b])).toEqual([]);
    expect(linkedCues(null, [a])).toEqual([]);
  });

  it('cannot link groups and scene markers', () => {
    expect(canLink(sound('s'))).toBe(true);
    expect(canLink(group('g', []))).toBe(false);
    expect(canLink(scene('s'))).toBe(false);
    expect(canLink(undefined)).toBe(false);
  });

  it('tells placeholders', () => {
    expect(isPlaceholder(sound('s', { file: '' }))).toBe(true);
    expect(isPlaceholder(sound('s'))).toBe(false);
    expect(isPlaceholder(wait('w'))).toBe(false);
  });
});

describe('playlist items', () => {
  it('tells items with settings of their own; 0 dB gain and a 0 start are none', () => {
    expect(hasOverrides({ file: 'a' })).toBe(false);
    expect(hasOverrides({ file: 'a', gain_db: 0, start: 0 })).toBe(false);
    expect(hasOverrides({ file: 'a', gain_db: -3 })).toBe(true);
    expect(hasOverrides({ file: 'a', preserve_pitch: false })).toBe(true);
  });

  it('patches one item, leaving out fields set back to the playlist', () => {
    const cue = playlist('p', ['a', 'b']);
    cue.items[1] = { file: 'b', gain_db: -3, pan: 0.5, normalize_db: -2 };
    expect(withItem(cue, 1, { pan: null, rate: 90 })).toEqual([
      { file: 'a' },
      { file: 'b', gain_db: -3, rate: 90, normalize_db: -2 },
    ]);
    // `null`: all overrides back, the normalization stays.
    expect(withItem(cue, 1, null)[1]).toEqual({ file: 'b', normalize_db: -2 });
    expect(withoutNormalization(cue)[1]).toEqual({ file: 'b', gain_db: -3, pan: 0.5 });
  });
});
