// What the cue list shows of a cue besides its fields: the status column
// (running time, countdowns, duration, target), warnings, chain arrows and
// the repeat count.

import { advanceBadge, effectiveAt } from './advance';
import type { Cue } from './api';
import { controlStatus, isControl } from './control';
import { isPlaceholder, loopLabel, passLabel } from './cue';
import { fmtDb, fmtRunning, fmtSecs, fmtTime, nearEnd } from './format';
import { t } from './i18n/index.svelte';
import { midiSummary } from './midi';
import { speedSummary } from './speed';
import { activeFor, allCues, cues, fadingFor, gapFor, importFor, isRunning, previousOf, ui } from './store.svelte';

export interface CueStatus {
  text: string;
  /** Shown to the right of `text`, e.g. "Paused", "2/5", "Loop". */
  info?: string;
  /** 0…1 for a progress bar. */
  progress?: number;
  /** In a loop or vamp. */
  vamp?: boolean;
  /** Cannot play as set (no file, no target). */
  bad?: boolean;
  /** Close to the end (`settings.end_warning`). */
  warn?: boolean;
  /** The progress bar shows copying, not playback. */
  copying?: boolean;
  paused?: boolean;
}

/** The status column of a cue: while it runs its times, else what it is set to. */
export function cueStatus(cue: Cue): CueStatus {
  return runningStatus(cue) ?? restingStatus(cue);
}

function runningStatus(cue: Cue): CueStatus | undefined {
  const active = activeFor(cue.id);
  if (active) {
    const [elapsed, remaining] = fmtRunning(active.elapsed, active.remaining);
    return {
      text: `${elapsed} / −${remaining}`,
      info: active.paused
        ? t('status.paused')
        : active.loading
          ? t('status.loading')
          : active.item
            ? `${active.item.index + 1}/${active.item.count}`
            : active.pass
              ? passLabel(active.pass)
              : active.looping
                ? loopLabel(cue)
                : undefined,
      progress: active.progress,
      vamp: active.looping,
      warn: nearEnd(active.remaining, ui.view?.settings.end_warning ?? 10),
      paused: active.paused,
    };
  }
  const fading = fadingFor(cue.id);
  if (fading) {
    const [elapsed, remaining] = fmtRunning(fading.elapsed, fading.remaining);
    return {
      text: `${elapsed} / −${remaining}`,
      progress: fading.progress,
      info: fading.paused ? t('status.paused') : undefined,
      paused: fading.paused,
    };
  }
  const gap = gapFor(cue.id);
  if (gap) {
    return {
      text: t('status.gap', { time: fmtSecs(gap.remaining) }),
      info: gap.paused ? t('status.paused') : `${gap.next.index + 1}/${gap.next.count}`,
      paused: gap.paused,
    };
  }
  const wait = ui.runtime?.waiting.find((waiting) => waiting.cue === cue.id);
  if (wait) {
    return {
      text: t('status.pre_wait', { time: fmtSecs(wait.remaining) }),
      info: wait.paused ? t('status.paused') : undefined,
      paused: wait.paused,
    };
  }
  return undefined;
}

function restingStatus(cue: Cue): CueStatus {
  const view = ui.view;
  if (!view) return { text: '' };
  const info = view.cue_info[cue.id];
  switch (cue.type) {
    case 'sound': {
      if (isPlaceholder(cue)) return { text: t('status.no_file'), info: t('status.placeholder') };
      if (info?.missing) return { text: t('status.file_missing'), bad: true };
      if (info?.importing) {
        const progress = importFor(cue.file) ?? 0;
        return { text: t('status.copying'), info: `${Math.round(progress * 100)} %`, progress, copying: true };
      }
      const speed = speedSummary(cue) || undefined;
      if (cue.vamps.length > 0) return { text: '∞', info: speed ? `Vamp · ${speed}` : 'Vamp' };
      if (cue.loop === 'forever') return { text: '∞', info: speed ? `Loop · ${speed}` : 'Loop' };
      return { text: info?.duration != null ? fmtTime(info.duration) : info?.probing ? '…' : '—', info: speed };
    }
    case 'wait':
      return { text: fmtSecs(cue.duration) };
    case 'group': {
      const mode = cue.mode === 'all' ? t('status.simultaneous') : t('status.sequential');
      if (isRunning(cue)) return { text: t('status.running'), info: mode };
      return { text: t('status.cues', { count: cue.cues.length }), info: mode, bad: info?.missing };
    }
    case 'playlist': {
      if (info?.missing) return { text: t('status.file_missing'), bad: true };
      const speed = speedSummary(cue);
      const titles = `${t('status.items', { count: cue.items.length })}${speed ? ` · ${speed}` : ''}`;
      if (info?.importing) return { text: t('status.copying'), info: titles };
      const length = info?.duration != null ? fmtTime(info.duration) : info?.probing ? '…' : '—';
      return { text: cue.repeat ? '∞' : length, info: titles };
    }
    case 'midi': {
      const count = cue.messages.length;
      return { text: count === 1 ? midiSummary(cue.messages[0]) : t('status.messages', { count }), bad: count === 0 };
    }
    case 'osc': {
      const device = view.show.devices.find((other) => other.id === cue.device);
      if (!device) return { text: t('status.no_device'), bad: true };
      return { text: `→ ${device.name} · ${cue.messages[0]?.address ?? '–'}` };
    }
    case 'goto': {
      // The target instead of an arrow, so the list stays calm.
      if (!cue.target) return { text: t('status.no_target'), bad: true };
      const target = cues().find((other) => other.id === cue.target);
      if (!target) {
        const inner = allCues().find((other) => other.id === cue.target);
        return { text: inner ? t('status.in_group', { number: inner.number }) : t('status.target_missing'), bad: true };
      }
      return {
        text: `→ ${target.number} ${target.title}`,
        info: cue.start ? t('status.goto_starts') : t('status.goto_selects'),
      };
    }
  }
  if (isControl(cue)) return controlStatus(cue, previousOf(cue.id), allCues(), view.show, !!info?.target_missing);
  return { text: '' };
}

/**
 * What the cue cannot play as set: a missing or unreadable file, its level
 * against the show's limits, positions and fade times against the file, the
 * file name's case against the disk, hand-edited values.
 */
export function cueWarnings(cue: Cue): string[] {
  const info = ui.view?.cue_info[cue.id];
  const limits = ui.view?.show.level_limits;
  if (!info || !limits) return [];
  const warnings = (info.timing ?? []).map((warning) => warning.message);
  if (info.missing) {
    warnings.unshift(cue.type === 'group' ? t('status.warn.group_file_missing') : t('status.file_missing'));
  }
  warnings.push(...(info.problems ?? []));
  if (info.case_mismatch) warnings.push(t('inspector.notice.case_mismatch', { name: info.case_mismatch }));
  if (info.level_silenced) warnings.push(t('status.warn.level_silenced', { level: fmtDb(limits.min_db) }));
  if (info.level_capped) {
    const key = cue.type === 'playlist' ? 'status.warn.level_capped_items' : 'status.warn.level_capped';
    warnings.push(t(key, { level: fmtDb(limits.max_db) }));
  }
  return warnings;
}

/** The chain arrow from `cue` to `next`, if it starts that one by itself; `waiting` while it is about to. */
export function chainArrow(cue: Cue, next: Cue | undefined): { label: string; waiting: boolean } | null {
  if (!cue.advance.start_next || !next) return null;
  const waiting = !!ui.runtime?.following.some((follow) => follow.after === cue.id && follow.cue === next.id);
  return { label: advanceBadge(effectiveAt(cue)), waiting };
}

/** Repeat: passes so far on the playhead cue ("2/3"), else how many ("×3"); null without repeat. */
export function repeatBadge(cue: Cue): string | null {
  if (cue.passes === 0 || cue.passes === 1) return null;
  const count = cue.passes === 'forever' ? '∞' : String(cue.passes);
  const state = ui.runtime?.repeat;
  return cue.id === ui.runtime?.playhead && state ? `${state.done}/${count}` : `×${count}`;
}

/** True while the cue plays, fades or sits in a playlist gap (a group: anything in it). Unlike `isRunning`, a
 * pre-wait does not count: the row is not tinted as running yet. */
export function rowRunning(cue: Cue): boolean {
  return !!(activeFor(cue.id) || fadingFor(cue.id) || gapFor(cue.id)) || (cue.type === 'group' && isRunning(cue));
}
