import type { LevelLimits } from './api';

// Fader taper like a mixing desk: unity at three quarters of the travel,
// fine steps around it, the lower range compressed towards silence.

/** dB at or below which a cue is silent (`SILENCE_DB` in the runner). */
export const SILENCE_DB = -120;

/** Fader position (0…1) → dB at the default limits (+6 / −60); linear in between. */
const TAPER: [number, number][] = [
  [0, -60],
  [0.15, -40],
  [0.25, -30],
  [0.375, -20],
  [0.5, -10],
  [0.75, 0],
  [1, 6],
];

/** Scale marks to print next to a fader, from the top. */
export const FADER_MARKS = [6, 0, -10, -20, -30, -40, -60];

/**
 * The taper between the show's limits: the minimum (silence) at the bottom of the travel, the maximum at the top.
 * Every position above the bottom is an audible level, so a level maps back to the position it came from.
 */
function taper({ min_db, max_db }: LevelLimits): [number, number][] {
  const points: [number, number][] = [
    [position(min_db), min_db],
    ...TAPER.filter(([, db]) => db > min_db && db < max_db),
    [position(max_db), max_db],
  ];
  // Stretched so the limits sit at the ends of the travel.
  const [bottom, top] = [points[0][0], points[points.length - 1][0]];
  return points.map(([p, db]) => [(p - bottom) / (top - bottom), db]);
}

/** Where `db` sits on the default taper; beyond its ends the end segments go on in a straight line. */
function position(db: number): number {
  const last = TAPER.length - 1;
  const i = db <= TAPER[0][1] ? 1 : db >= TAPER[last][1] ? last : TAPER.findIndex(([, d]) => d >= db);
  const [[p0, db0], [p1, db1]] = [TAPER[i - 1], TAPER[i]];
  return p0 + ((db - db0) / (db1 - db0)) * (p1 - p0);
}

/** Fader position → dB within the show's limits; the bottom is silence (`SILENCE_DB`). */
export function faderToDb(position: number, limits: LevelLimits): number {
  const points = taper(limits);
  const p = Math.max(0, Math.min(1, position));
  if (p === 0) return SILENCE_DB;
  for (let i = 1; i < points.length; i++) {
    const [p1, db1] = points[i];
    if (p <= p1) {
      const [p0, db0] = points[i - 1];
      return db0 + ((p - p0) / (p1 - p0)) * (db1 - db0);
    }
  }
  return limits.max_db;
}

export function dbToFader(db: number, limits: LevelLimits): number {
  if (db <= limits.min_db) return 0;
  const points = taper(limits);
  const d = Math.min(limits.max_db, db);
  for (let i = 1; i < points.length; i++) {
    const [p1, db1] = points[i];
    if (d <= db1) {
      const [p0, db0] = points[i - 1];
      return p0 + ((d - db0) / (db1 - db0)) * (p1 - p0);
    }
  }
  return 1;
}

/** Resolution of a level set on a fader: steps per dB. */
const STEPS_PER_DB = 10;

/** `db` rounded to the fader's resolution. */
export function roundFaderDb(db: number): number {
  return Math.round(db * STEPS_PER_DB) / STEPS_PER_DB;
}

/**
 * Slider steps (fader positions) for every level on the fader's resolution within the limits, plus `db` itself.
 * A slider snaps its value to its steps; on a grid of positions a level would move to the next grid position and
 * read (and be stored) a little off, e.g. 0 dB as +0,1 dB when the limits shift the taper.
 */
export function faderSteps(limits: LevelLimits, db: number): number[] {
  const steps = [0, dbToFader(db, limits)];
  const [lo, hi] = [Math.ceil(limits.min_db * STEPS_PER_DB), Math.floor(limits.max_db * STEPS_PER_DB)];
  for (let n = lo; n <= hi; n++) steps.push(dbToFader(n / STEPS_PER_DB, limits));
  return steps;
}
