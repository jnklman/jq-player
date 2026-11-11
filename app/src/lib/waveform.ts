// Positions of a sound cue on its waveform, shared by the inspector's small
// waveform and the waveform dialog: what dragging a handle or double-clicking
// does, the fade envelope and the peaks of a file.

import { api, type FadeCurve, type SoundFields } from './api';
import { fadeGains } from './cue';

/** The positions in a file: start, end, vamps and marks. */
export type Positions = Pick<SoundFields, 'start' | 'end' | 'vamps' | 'marks'>;
/** The positions and the fades over them. */
export type Times = Positions & Pick<SoundFields, 'fade_in' | 'fade_out' | 'end_fade' | 'curve'>;

/** A draggable handle on the waveform. */
export type Handle =
  { kind: 'start' | 'end' | 'fade_in' | 'fade_out' } | { kind: 'vamp_start' | 'vamp_end' | 'mark'; index: number };

/** Shortest start–end and vamp, so the edges never cross. */
const MIN_LENGTH = 0.01;

/** A copy that can be changed without touching `times` (or the cue it came from). */
export function copyTimes<T extends Positions>(times: T): T {
  return { ...times, vamps: times.vamps.map((vamp) => ({ ...vamp })), marks: [...times.marks] };
}

/** Moves `handle` of `times` (changed in place) to `pos` seconds in a file `duration` long. */
export function dragHandle(times: Positions & Partial<Times>, handle: Handle, pos: number, duration: number) {
  const last = Math.min(times.end ?? duration, duration);
  const ms = (secs: number) => Math.round(secs * 1000) / 1000;
  switch (handle.kind) {
    case 'start':
      times.start = Math.min(pos, last - MIN_LENGTH);
      break;
    case 'end':
      times.end = Math.max(pos, times.start + MIN_LENGTH);
      break;
    case 'fade_in':
      times.fade_in = ms(Math.max(0, Math.min(pos, last) - times.start));
      break;
    case 'fade_out':
      times.fade_out = ms(Math.max(0, last - Math.max(pos, times.start)));
      break;
    case 'vamp_start':
      times.vamps[handle.index].start = Math.min(pos, times.vamps[handle.index].end - MIN_LENGTH);
      break;
    case 'vamp_end':
      times.vamps[handle.index].end = Math.max(pos, times.vamps[handle.index].start + MIN_LENGTH);
      break;
    case 'mark':
      times.marks[handle.index] = pos;
      break;
  }
}

/** Tidies `times` (in place) after a drag: an end within `tolerance` of the file's end means "to the end"; vamps and
 * marks in order. */
export function settleTimes(times: Positions, duration: number, tolerance: number) {
  if (times.end != null && duration > 0 && times.end >= duration - tolerance) times.end = null;
  times.vamps.sort((left, right) => left.start - right.start);
  times.marks.sort((left, right) => left - right);
}

/** Adds a mark at `pos`, or removes the one within `near` seconds of it; sorted. */
export function toggleMark(marks: number[], pos: number, near: number): number[] {
  const hit = marks.findIndex((mark) => Math.abs(mark - pos) <= near);
  const next = hit >= 0 ? marks.filter((_, i) => i !== hit) : [...marks, pos];
  return next.sort((left, right) => left - right);
}

/** A stretch of a fade: from and to (seconds) and the gain (0…1) by position 0…1 in it. */
export type FadeSegment = [from: number, to: number, gain: (pos: number) => number];

/**
 * The fade-in and the end fade over start–`end` as gain curves, shaped as the
 * engine shapes them; fades in dB run from the show minimum `floorDb`.
 */
export function fadeSegments(
  times: Pick<Times, 'start' | 'fade_in' | 'fade_out' | 'end_fade'> & { curve: FadeCurve },
  end: number,
  floorDb: number,
): FadeSegment[] {
  const length = end - times.start;
  if (length <= 0) return [];
  const gains = fadeGains(times.curve, floorDb);
  const parts: FadeSegment[] = [];
  const fadeIn = Math.min(times.fade_in, length);
  if (fadeIn > 0) parts.push([times.start, times.start + fadeIn, gains.in]);
  const fadeOut = times.end_fade ? Math.min(times.fade_out, length) : 0;
  if (fadeOut > 0) parts.push([end - fadeOut, end, gains.out]);
  return parts;
}

// --- Peaks

export type Peaks = { duration: number; peaks: number[] };

// The small waveform's peaks per media file; decoding happens once per version
// of the file (it grows when the file changes on disk).
const overviews = new Map<string, { version: number; peaks: Promise<Peaks> }>();

/** Peaks of the whole file at the small waveform's resolution, cached per file version. */
export function overviewPeaks(mediaDir: string, file: string, version: number): Promise<Peaks> {
  const key = `${mediaDir}\n${file}`;
  let entry = overviews.get(key);
  if (entry?.version !== version) {
    const peaks = api.waveform(mediaDir, file, 1500);
    entry = { version, peaks };
    peaks.catch(() => {
      if (overviews.get(key) === entry) overviews.delete(key);
    });
    overviews.set(key, entry);
  }
  return entry.peaks;
}

/** Peaks spread evenly over `from`–`to` seconds. */
export type Stretch = { from: number; to: number; peaks: number[] };

/** Loudest peak of `stretch` between two times; between two peaks (zoomed in further than they reach) interpolated. */
export function levelOf(stretch: Stretch, since: number, until: number): number {
  const peaks = stretch.peaks;
  const length = stretch.to - stretch.from;
  if (peaks.length === 0 || length <= 0) return 0;
  const perSec = peaks.length / length;
  const first = (since - stretch.from) * perSec;
  const last = (until - stretch.from) * perSec;
  if (last - first >= 1) {
    let peak = 0;
    for (let i = Math.max(0, Math.floor(first)); i < Math.min(peaks.length, Math.ceil(last)); i++) {
      peak = Math.max(peak, peaks[i]);
    }
    return peak;
  }
  const pos = Math.max(0, Math.min(peaks.length - 1, (first + last) / 2 - 0.5));
  const below = Math.floor(pos);
  const above = Math.min(peaks.length - 1, below + 1);
  return peaks[below] + (peaks[above] - peaks[below]) * (pos - below);
}

// --- Ruler

/** Ruler steps in seconds; the first that leaves enough room between labels is used. */
const STEPS = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];

/** The ruler's marks in the visible part (`from`–`to` px of content `pxPerSec` wide per second). */
export function rulerTicks(pxPerSec: number, from: number, to: number): { step: number; times: number[] } {
  if (pxPerSec <= 0) return { step: 1, times: [] };
  const step = STEPS.find((candidate) => candidate * pxPerSec >= 80) ?? STEPS.at(-1)!;
  const times: number[] = [];
  for (let i = Math.max(0, Math.floor(from / pxPerSec / step)); i <= Math.ceil(to / pxPerSec / step); i++) {
    times.push(i * step);
  }
  return { step, times };
}

/** A ruler label with as many decimals as the step needs: "12", "1:05,5", "0,25". */
export function rulerLabel(secs: number, step: number): string {
  const digits = step < 0.1 ? 2 : step < 1 ? 1 : 0;
  const total = Math.round(secs * 100) / 100;
  const minutes = Math.floor(total / 60);
  const rest = (total - minutes * 60).toFixed(digits).replace('.', ',');
  return minutes > 0 ? `${minutes}:${rest.padStart(digits ? 3 + digits : 2, '0')}` : rest;
}
