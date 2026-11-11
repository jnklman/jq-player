// Derived cue facts shared by the cue list and the running cues.

import type {
  Cue,
  CueInfo,
  FadeCurve,
  ItemOverrides,
  OthersEffect,
  PassState,
  PlaylistCue,
  PlaylistItem,
  SecondTrigger,
} from './api';
import { labels, options, t } from './i18n/index.svelte';

export const curveOptions = options<FadeCurve>({
  decibel_slow_top: 'curve.decibel_slow_top',
  decibel_slow_bottom: 'curve.decibel_slow_bottom',
  decibel_s: 'curve.decibel_s',
  decibel: 'curve.decibel',
  linear: 'curve.linear',
  s_curve: 'curve.s_curve',
});

const easeIn = (t: number) => t * t;
const easeOut = (t: number) => t * (2 - t);
const smooth = (t: number) => t * t * (3 - 2 * t);

/** Share of the way at `t` (0…1) of a fade going up (`rising`) or down, as the engine's shapes. */
const shapes: Record<FadeCurve, (t: number, rising: boolean) => number> = {
  decibel: (t) => t,
  decibel_s: smooth,
  decibel_slow_top: (t, rising) => (rising ? easeOut(t) : easeIn(t)),
  decibel_slow_bottom: (t, rising) => (rising ? easeIn(t) : easeOut(t)),
  linear: (t) => t,
  s_curve: smooth,
};

/** Gain (0…1 of the cue level) over a fade-in and a fade-out, by position 0…1 in the fade; as the engine shapes
 * them. A fade in dB runs between the show minimum `floorDb` and the level, silence beyond. */
export function fadeGains(
  curve: FadeCurve,
  floorDb: number,
): { in: (t: number) => number; out: (t: number) => number } {
  const shape = shapes[curve] ?? shapes.decibel;
  if (curve.startsWith('decibel')) {
    const floor = 10 ** (Math.min(Math.max(floorDb, -120), -6) / 20);
    return {
      in: (t) => (t <= 0 ? 0 : floor ** (1 - shape(t, true))),
      out: (t) => (t >= 1 ? 0 : floor ** shape(t, false)),
    };
  }
  return { in: (t) => shape(t, true), out: (t) => 1 - shape(t, false) };
}

/** Vamp regions as fractions of the played range, for marking progress bars. */
export function vampSpans(cue: Cue | undefined, info: CueInfo | undefined): { left: number; width: number }[] {
  if (cue?.type !== 'sound' || cue.vamps.length === 0) return [];
  const end = cue.end ?? info?.file_duration;
  if (end == null || end <= cue.start) return [];
  const range = end - cue.start;
  return cue.vamps.map((vamp) => {
    const left = Math.max(0, (vamp.start - cue.start) / range);
    const right = Math.min(1, (vamp.end - cue.start) / range);
    return { left, width: Math.max(0, right - left) };
  });
}

/** "Loop 2/3", "Loop 4/∞", "Vamp 1 · 3/∞". */
export function passLabel(pass: PassState): string {
  const of = `${pass.pass}/${pass.count ?? '∞'}`;
  return pass.vamp != null ? `Vamp ${pass.vamp} · ${of}` : `Loop ${of}`;
}

/** "Vamp" for cues with vamps, "Loop" for a loop over the whole range. */
export function loopLabel(cue: Cue | undefined): string {
  return cue?.type === 'sound' && cue.vamps.length > 0 ? 'Vamp' : 'Loop';
}

/** The other cues linked with `cue`, in list order (`all` with groups' cues). */
export function linkedCues(cue: Cue | undefined | null, all: Cue[]): Cue[] {
  if (!cue?.link) return [];
  return all.filter((other) => other.link === cue.link && other.id !== cue.id);
}

/** A sound cue without a file yet. */
export function isPlaceholder(cue: Cue | null | undefined): boolean {
  return cue?.type === 'sound' && cue.file === '';
}

/** How a cue is named in texts: its number, else its title in quotes. */
export function cueLabel(cue: Cue): string {
  return cue.number || t('cue.quoted', { title: cue.title || t('cue.untitled') });
}

/** Second trigger modes as offered in the inspector. */
export const secondTriggerLabels = labels<SecondTrigger['action']>({
  ignore: 'second_trigger.ignore',
  pause: 'second_trigger.pause',
  stop: 'second_trigger.stop',
  fade_out: 'second_trigger.fade_out',
  restart: 'second_trigger.restart',
  exit_loop: 'second_trigger.exit_loop',
  next_item: 'second_trigger.next_item',
});

/** What a starting cue does to the others. */
export const othersLabels = labels<OthersEffect['effect']>({
  none: 'others.none',
  fade_stop: 'others.fade_stop',
  duck: 'others.duck',
});

const overrideKeys = [
  'gain_db',
  'start',
  'end',
  'pan',
  'rate',
  'preserve_pitch',
  'transpose',
  'fade_in',
  'fade_out',
] as const satisfies readonly (keyof ItemOverrides)[];

/** A change of a playlist item; null sets a field back to the playlist's (the offset: not normalized). */
export type ItemPatch = { [K in keyof ItemOverrides | 'normalize_db']?: number | boolean | null };

/** True if a playlist item plays with any setting of its own. */
export function hasOverrides(item: PlaylistItem): boolean {
  return overrideKeys.some((key) => item[key] != null && !((key === 'gain_db' || key === 'start') && item[key] === 0));
}

/** The playlist's items with `patch` applied to item `index` (`null` as the patch: all its overrides back to the
 * playlist's); fields set to null are left out. */
export function withItem(cue: PlaylistCue, index: number, patch: ItemPatch | null): PlaylistItem[] {
  const reset = patch ?? Object.fromEntries(overrideKeys.map((key) => [key, null]));
  return cue.items.map((item, i) => (i === index ? withoutNull({ ...item, ...reset }) : item));
}

/** The playlist's items without their normalization offsets. */
export function withoutNormalization(cue: PlaylistCue): PlaylistItem[] {
  return cue.items.map((item) => withoutNull({ ...item, normalize_db: null }));
}

function withoutNull(item: Record<string, unknown>): PlaylistItem {
  return Object.fromEntries(Object.entries(item).filter(([, value]) => value != null)) as unknown as PlaylistItem;
}
