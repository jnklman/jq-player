// Display formatting; numbers follow the language (decimal comma in German, point in English).

import type { LevelLimits, MidiTrigger } from './api';
import { fmtNumber, i18n, t } from './i18n/index.svelte';

export function fmtTime(secs: number | null | undefined): string {
  if (secs == null || !isFinite(secs)) return '∞';
  const total = Math.max(0, secs);
  const minutes = Math.floor(total / 60);
  const rest = total - minutes * 60;
  return minutes > 0 ? `${minutes}:${rest.toFixed(0).padStart(2, '0')}` : `${fmtNumber(rest, 1)} s`;
}

/**
 * Elapsed and remaining time of a running cue, as `fmtTime`, turning over
 * together. Rounded each on its own, they would tick apart by the fraction
 * of a step in the cue's length. The remaining time counts down (rounded up,
 * it reaches 0 at the end); the elapsed time is rounded down on a grid
 * shifted by that fraction, so it ticks with the remaining time and is at
 * most one step (0.1 s, from a minute 1 s) ahead.
 */
export function fmtRunning(elapsed: number, remaining: number | null | undefined): [string, string] {
  // Displayed steps per second, as `fmtTime` shows the value.
  const perSec = (secs: number) => (secs < 60 ? 10 : 1);
  // Absorbs floating-point noise at the turnover.
  const EPS = 1e-6;
  const steps = perSec(elapsed);
  let shift = 0;
  if (remaining != null && isFinite(remaining)) {
    const total = (elapsed + remaining) * steps;
    shift = Math.ceil(total - EPS) - total;
  }
  const shownElapsed = Math.floor(elapsed * steps + shift + EPS) / steps;
  if (remaining == null || !isFinite(remaining)) return [fmtTime(shownElapsed), fmtTime(remaining)];
  const down = perSec(remaining);
  return [fmtTime(shownElapsed), fmtTime(Math.ceil(remaining * down - EPS) / down)];
}

/** A running cue shows the warning colour from `warning` seconds (`settings.end_warning`; 0 = never) before its end. */
export function nearEnd(remaining: number | null | undefined, warning: number): boolean {
  return remaining != null && warning > 0 && remaining <= warning;
}

/** Position in a file to the millisecond: "1:05,250", "12,500 s" (German). */
export function fmtPosition(secs: number): string {
  const total = Math.round(Math.max(0, secs) * 1000) / 1000;
  const minutes = Math.floor(total / 60);
  const rest = fmtNumber(total - minutes * 60, 3);
  return minutes > 0 ? `${minutes}:${rest.padStart(6, '0')}` : `${rest} s`;
}

export function fmtSecs(secs: number): string {
  return `${fmtNumber(secs, 1)} s`;
}

/** Size on disk: "0,4 MB", "312 MB". */
export function fmtMegabytes(bytes: number): string {
  const megabytes = bytes / (1 << 20);
  return `${megabytes < 10 ? fmtNumber(megabytes, 1) : Math.round(megabytes)} MB`;
}

export function fmtDb(db: number | null | undefined): string {
  if (db == null || !isFinite(db) || db <= -120) return '−∞ dB';
  return `${db < 0 ? '−' : db > 0 ? '+' : ''}${fmtNumber(Math.abs(db), 1)} dB`;
}

/** Loudness: "−18,3 LUFS"; silence "−∞ LUFS". */
export function fmtLufs(lufs: number | null | undefined): string {
  if (lufs == null || !isFinite(lufs)) return '−∞ LUFS';
  return `${lufs < 0 ? '−' : ''}${fmtNumber(Math.abs(lufs), 1)} LUFS`;
}

export function fmtPercent(percent: number | null | undefined): string {
  if (percent == null || !isFinite(percent)) return '∞ %';
  return `${(percent * 100).toFixed()} %`;
}

/** Meter width for a level: -60 dB … 0 dB → 0 … 100 %. */
export function meterPercent(db: number | null | undefined, limits: LevelLimits): number {
  if (db == null || !isFinite(db) || db <= limits.min_db) return 0;
  const range = limits.max_db - limits.min_db;
  return Math.max(0, Math.min(100, ((db - limits.min_db) / range) * 100));
}

/** Device name without the adapter in parentheses:
 * "Lautsprecher (Realtek High Definition Audio)" → "Lautsprecher". */
export function shortDevice(name: string): string {
  return name.replace(/\s*\([^()]*\)$/, '') || name;
}

export function clock(date: Date): string {
  return date.toLocaleTimeString(i18n.locale, { hour12: false });
}

/** Note names of the octave; German calls the B natural "H". */
const noteNames = () => ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', t('midi.note_b')];

/** e.g. "Note C4 · Channel 1", "CC 64 · Channel 10", "Program 5 · Channel 2". */
export function fmtMidi(trigger: MidiTrigger): string {
  const channel = t('midi.channel', { channel: trigger.channel + 1 });
  if (trigger.kind === 'cc') return `CC ${trigger.controller} · ${channel}`;
  if (trigger.kind === 'program') return `${t('midi.program')} ${trigger.program + 1} · ${channel}`;
  const name = noteNames()[trigger.note % 12] + (Math.floor(trigger.note / 12) - 1);
  return `${t('midi.note')} ${name} · ${channel}`;
}

/** Stopwatch time: "0:07", "12:34", "1:02:03" (whole seconds, rounded down). */
export function fmtClock(secs: number): string {
  const total = Math.max(0, Math.floor(secs));
  const [h, m, s] = [Math.floor(total / 3600), Math.floor(total / 60) % 60, total % 60];
  const pad = (n: number) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

/** A countdown's time left as `fmtClock`, rounded up (it shows 0:00 for the second it runs out); in overtime with a
 * minus: "−0:12". */
export function fmtCountdown(remaining: number): string {
  const secs = Math.ceil(remaining - 1e-6);
  return secs < 0 ? `−${fmtClock(-secs)}` : fmtClock(secs);
}
