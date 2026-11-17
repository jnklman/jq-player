import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { settle } from '../test/store';
import { PeakTiles } from './peakTiles.svelte';
import { ui } from './store.svelte';
import {
  audibleRange,
  copyTimes,
  dragHandle,
  fadeSegments,
  levelOf,
  overviewPeaks,
  rulerLabel,
  rulerTicks,
  settleTimes,
  toggleMark,
  trimTimes,
} from './waveform';
import { drawWaveform } from './waveformCanvas';

const times = { start: 0, end: null, vamps: [], marks: [] };

describe('trimTimes', () => {
  it('moves the start outwards to the millisecond', () => {
    expect(trimTimes(times, 'start', { start: 0.12345, end: 9 }, 10)).toEqual({ start: 0.123 });
  });

  it('moves the end outwards, or leaves it open when the sound lasts to the end', () => {
    expect(trimTimes(times, 'end', { start: 0, end: 9.0001 }, 10)).toEqual({ end: 9.001 });
    expect(trimTimes({ ...times, end: 8 }, 'end', { start: 0, end: null }, 10)).toEqual({ end: null });
  });

  it('never lets start and end cross', () => {
    expect(trimTimes({ ...times, end: 2 }, 'start', { start: 5, end: null }, 10)).toEqual({ start: 1.99 });
    expect(trimTimes({ ...times, start: 6 }, 'end', { start: 0, end: 3 }, 10)).toEqual({ end: 6.01 });
  });
});

describe('dragging handles', () => {
  const fresh = () => ({
    start: 1,
    end: 9 as number | null,
    vamps: [{ start: 3, end: 5 }],
    marks: [2, 7],
    fade_in: 0,
    fade_out: 0,
  });

  it('copies without sharing vamps or marks', () => {
    const original = fresh();
    const copy = copyTimes(original);
    copy.vamps[0].start = 0;
    copy.marks.push(1);
    expect(original.vamps[0].start).toBe(3);
    expect(original.marks).toEqual([2, 7]);
  });

  it('keeps start and end apart', () => {
    const t = fresh();
    dragHandle(t, { kind: 'start' }, 20, 10);
    expect(t.start).toBeCloseTo(8.99);
    dragHandle(t, { kind: 'end' }, 0, 10);
    expect(t.end).toBeCloseTo(9);
    const open = { ...fresh(), end: null };
    dragHandle(open, { kind: 'start' }, 4, 10);
    expect(open.start).toBe(4);
  });

  it('sets fades from the edges, to the millisecond', () => {
    const t = fresh();
    dragHandle(t, { kind: 'fade_in' }, 2.50049, 10);
    expect(t.fade_in).toBe(1.5);
    dragHandle(t, { kind: 'fade_in' }, 0, 10);
    expect(t.fade_in).toBe(0);
    dragHandle(t, { kind: 'fade_out' }, 6.25, 10);
    expect(t.fade_out).toBe(2.75);
    dragHandle(t, { kind: 'fade_out' }, 20, 10);
    expect(t.fade_out).toBe(0);
  });

  it('moves vamp edges and marks', () => {
    const t = fresh();
    dragHandle(t, { kind: 'vamp_start', index: 0 }, 6, 10);
    expect(t.vamps[0].start).toBeCloseTo(4.99);
    dragHandle(t, { kind: 'vamp_end', index: 0 }, 1, 10);
    expect(t.vamps[0].end).toBeCloseTo(5);
    dragHandle(t, { kind: 'mark', index: 1 }, 0.5, 10);
    expect(t.marks).toEqual([2, 0.5]);
  });

  it('tidies up after a drag', () => {
    const t = {
      ...fresh(),
      end: 9.95,
      vamps: [
        { start: 5, end: 6 },
        { start: 1, end: 2 },
      ],
      marks: [7, 2],
    };
    settleTimes(t, 10, 0.1);
    expect(t.end).toBeNull();
    expect(t.vamps.map((v) => v.start)).toEqual([1, 5]);
    expect(t.marks).toEqual([2, 7]);
    const kept = { ...fresh(), end: 9 };
    settleTimes(kept, 10, 0.1);
    expect(kept.end).toBe(9);
  });

  it('adds a mark or removes the one near', () => {
    expect(toggleMark([5, 1], 3, 0.1)).toEqual([1, 3, 5]);
    expect(toggleMark([1, 3, 5], 3.05, 0.1)).toEqual([1, 5]);
  });
});

describe('audibleRange', () => {
  it('passes the range on and warns about a silent file', async () => {
    backend.on('audible_range', () => ({ start: 0.5, end: null }));
    expect(await audibleRange('/m', 'a.wav')).toEqual({ start: 0.5, end: null });
    backend.on('audible_range', () => null);
    expect(await audibleRange('/m', 'a.wav')).toBeUndefined();
    expect(ui.notices.at(-1)).toMatchObject({ level: 'warning', message: expect.stringContaining('silent') });
    backend.fail('audible_range');
    expect(await audibleRange('/m', 'a.wav')).toBeUndefined();
    expect(ui.notices.at(-1)?.level).toBe('error');
  });
});

describe('fadeSegments', () => {
  const base = { start: 2, fade_in: 1, fade_out: 3, end_fade: true, curve: 'linear' as const };

  it('places the fade-in after the start and the end fade before the end', () => {
    const segments = fadeSegments(base, 10, -60);
    expect(segments.map(([from, to]) => [from, to])).toEqual([
      [2, 3],
      [7, 10],
    ]);
    expect(segments[0][2](0.5)).toBe(0.5);
  });

  it('leaves out what is off and keeps fades within the range', () => {
    expect(fadeSegments({ ...base, end_fade: false, fade_in: 0 }, 10, -60)).toEqual([]);
    expect(fadeSegments(base, 2, -60)).toEqual([]);
    expect(fadeSegments({ ...base, fade_in: 50 }, 4, -60)[0].slice(0, 2)).toEqual([2, 4]);
  });
});

describe('overviewPeaks', () => {
  it('asks once per file version and forgets a failed answer', async () => {
    backend.on('waveform', () => ({ duration: 3, peaks: [0.5] }));
    const first = overviewPeaks('/m', 'once.wav', 1);
    expect(overviewPeaks('/m', 'once.wav', 1)).toBe(first);
    expect(await first).toEqual({ duration: 3, peaks: [0.5] });
    expect(overviewPeaks('/m', 'once.wav', 2)).not.toBe(first);

    backend.fail('waveform');
    await expect(overviewPeaks('/m', 'bad.wav', 1)).rejects.toBe('waveform failed');
    backend.on('waveform', () => ({ duration: 1, peaks: [] }));
    expect(await overviewPeaks('/m', 'bad.wav', 1)).toEqual({ duration: 1, peaks: [] });
    expect(backend.invoked('waveform')).toHaveLength(4);
  });
});

describe('levelOf', () => {
  const stretch = { from: 0, to: 4, peaks: [0.1, 0.8, 0.3, 0.4] };

  it('takes the loudest peak of a stretch', () => {
    expect(levelOf(stretch, 0, 4)).toBe(0.8);
    expect(levelOf(stretch, 2, 4)).toBe(0.4);
    expect(levelOf({ from: 0, to: 0, peaks: [1] }, 0, 1)).toBe(0);
    expect(levelOf({ from: 0, to: 1, peaks: [] }, 0, 1)).toBe(0);
  });

  it('interpolates between peaks when zoomed in further', () => {
    expect(levelOf(stretch, 1, 1.0001)).toBeCloseTo(0.45, 2);
    expect(levelOf(stretch, -5, -4.9)).toBe(0.1);
  });
});

describe('ruler', () => {
  it('picks a step that leaves room between labels', () => {
    expect(rulerTicks(0, 0, 100)).toEqual({ step: 1, times: [] });
    expect(rulerTicks(100, 0, 300)).toEqual({ step: 1, times: [0, 1, 2, 3] });
    expect(rulerTicks(10000, 500, 600).step).toBe(0.01);
    expect(rulerTicks(0.01, 0, 1).step).toBe(600);
  });

  it('labels with as many decimals as the step needs', () => {
    expect(rulerLabel(12, 1)).toBe('12');
    expect(rulerLabel(65.5, 0.5)).toBe('1:05.5');
    expect(rulerLabel(0.25, 0.05)).toBe('0.25');
    expect(rulerLabel(125, 5)).toBe('2:05');
  });
});

describe('PeakTiles', () => {
  it('loads the tiles around the view, nearest first, and serves the closest level', async () => {
    const asked: number[] = [];
    backend.on('waveform_range', ({ from, to }) => {
      asked.push(from as number);
      return { duration: (to as number) - (from as number), peaks: [0.5] };
    });
    const tiles = new PeakTiles();
    tiles.reset('/m', 'a.wav');
    // Level 10: tiles of 2 s; the view 10–12 s.
    tiles.request(10, 10, 2, 1, 100);
    await settle();
    expect(asked).toEqual([10, 8, 12]);
    expect(tiles.changed).toBe(3);
    expect(tiles.at(10, 11)?.from).toBe(10);
    expect(tiles.at(11, 11)?.from).toBe(10);
    expect(tiles.at(10, 50)).toBeUndefined();

    // Asked again (a tile arrived): the rest of the stretch, one view behind and two ahead (8–16 s).
    tiles.request(10, 10, 2, 1, 100);
    await settle();
    expect(asked).toEqual([10, 8, 12, 14]);
  });

  it('drops answers for a file read anew and marks failed tiles empty', async () => {
    let answer!: () => void;
    backend.on('waveform_range', () => new Promise((resolve) => (answer = () => resolve({ duration: 2, peaks: [1] }))));
    const tiles = new PeakTiles();
    tiles.reset('/m', 'a.wav');
    tiles.request(10, 0, 2, -1, 2);
    tiles.reset('/m', 'b.wav');
    answer();
    await settle();
    expect(tiles.at(10, 1)).toBeUndefined();
    expect(tiles.changed).toBe(0);

    backend.fail('waveform_range');
    tiles.request(10, 0, 2, 1, 2);
    await settle();
    expect(tiles.changed).toBe(1);
    expect(tiles.at(10, 1)).toBeUndefined();
  });

  it('keeps at most 128 tiles', async () => {
    backend.on('waveform_range', ({ from, to }) => ({ duration: (to as number) - (from as number), peaks: [1] }));
    const tiles = new PeakTiles();
    tiles.reset('/m', 'a.wav');
    for (let i = 0; i < 50; i++) {
      tiles.request(10, i * 6, 2, 1, 1000);
      await settle(1);
    }
    expect(tiles.at(10, 1)).toBeUndefined();
    expect(tiles.at(10, 49 * 6 + 1)).toBeDefined();
  });
});

describe('drawWaveform', () => {
  it('draws a column per pixel and the fades over it', () => {
    const calls: string[] = [];
    const ctx = new Proxy(
      {},
      {
        get: (_, name) => (name === 'canvas' ? undefined : () => calls.push(String(name))),
        set: () => true,
      },
    );
    const canvas = document.createElement('canvas');
    vi.spyOn(canvas, 'getContext').mockReturnValue(ctx as CanvasRenderingContext2D);
    const picture = {
      width: 20,
      height: 100,
      ruler: 20,
      pxPerSec: 10,
      scrollLeft: 0,
      overview: { from: 0, to: 2, peaks: [0.5, 1] },
      detail: (secs: number) => (secs > 1 ? { from: 1, to: 2, peaks: [0.2] } : undefined),
      fades: fadeSegments({ start: 0, fade_in: 1, fade_out: 0, end_fade: false, curve: 'linear' }, 2, -60),
    };
    drawWaveform(canvas, picture);
    expect(canvas.width).toBe(20);
    expect(calls.filter((call) => call === 'fillRect')).toHaveLength(20);
    expect(calls).toContain('stroke');
    // The size stays, so the bitmap is not reallocated.
    drawWaveform(canvas, picture);
    expect(calls.filter((call) => call === 'fillRect')).toHaveLength(40);
  });

  it('draws nothing without a context', () => {
    const canvas = document.createElement('canvas');
    vi.spyOn(canvas, 'getContext').mockReturnValue(null);
    expect(() =>
      drawWaveform(canvas, {
        width: 1,
        height: 1,
        ruler: 0,
        pxPerSec: 1,
        scrollLeft: 0,
        overview: { from: 0, to: 1, peaks: [] },
        detail: () => undefined,
        fades: [],
      }),
    ).not.toThrow();
  });
});
