// Rate, pitch and transposition of sound and playlist cues (M6): defaults,
// limits and how they are shown.

import type { SpeedFields } from './api';
import { fmtNumber, t } from './i18n/index.svelte';

export type Speed = SpeedFields;

/** Limits as in the backend (`RATE_MIN`, `RATE_MAX`, `TRANSPOSE_MAX`). */
export const RATE_MIN = 50;
export const RATE_MAX = 150;
export const TRANSPOSE_MAX = 12;

/** The speed fields with their defaults filled in (the backend leaves defaults out). */
export function speedOf(cue: SpeedFields): Required<SpeedFields> {
  return { rate: cue.rate ?? 100, preserve_pitch: cue.preserve_pitch ?? true, transpose: cue.transpose ?? 0 };
}

/** True if the cue plays other than at its original speed and pitch. */
export function speedChanged(cue: SpeedFields): boolean {
  const s = speedOf(cue);
  return s.rate !== 100 || s.transpose !== 0;
}

export function clampRate(rate: number): number {
  return Math.max(RATE_MIN, Math.min(RATE_MAX, rate));
}

export function clampTranspose(semitones: number): number {
  return Math.max(-TRANSPOSE_MAX, Math.min(TRANSPOSE_MAX, semitones));
}

/** "80 %", "102,5 %". */
export function rateLabel(rate: number): string {
  return `${fmtNumber(Math.round(rate * 10) / 10)} %`;
}

/** "+3 st", "−1,5 st", "±0 st" (German: HT, for Halbton). */
export function transposeLabel(semitones: number): string {
  const rounded = Math.round(semitones * 100) / 100;
  const unit = t('speed.semitones');
  if (rounded === 0) return `±0 ${unit}`;
  return `${rounded > 0 ? '+' : '−'}${fmtNumber(Math.abs(rounded))} ${unit}`;
}

/** Short summary for the cue list: "80 %", "+3 HT", "80 % · +3 HT"; empty at the original speed and pitch. */
export function speedSummary(cue: SpeedFields): string {
  const s = speedOf(cue);
  return [s.rate !== 100 ? rateLabel(s.rate) : null, s.transpose !== 0 ? transposeLabel(s.transpose) : null]
    .filter((part) => part !== null)
    .join(' · ');
}
