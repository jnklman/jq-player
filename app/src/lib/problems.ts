// What the backend finds wrong with a cue (`CueInfo.problems`, `timing`, `item_timing`): its wording.

import type { CueProblem, Problem, TimeField, TimingIssue, ValueIssue } from './api';
import { errorText } from './errors';
import { fmtSecs } from './format';
import { t } from './i18n/index.svelte';
import { keyText } from './keys.svelte';
import { cueName } from './notices';
import { actionName } from './settings';

const FIELDS: Record<TimeField, () => string> = {
  pre_wait: () => t('check.field.pre_wait'),
  follow_time: () => t('check.field.follow_time'),
  start: () => t('check.field.start'),
  fade_in: () => t('check.field.fade_in'),
  fade_out: () => t('check.field.fade_out'),
  gap: () => t('check.field.gap'),
  duration: () => t('check.field.duration'),
  fade_duration: () => t('check.field.fade_duration'),
  start_from: () => t('check.field.start_from'),
  crossfade: () => t('check.field.crossfade'),
  wait: () => t('check.field.wait'),
};

/** A position or fade time that does not fit the file. */
export function timingText(issue: TimingIssue): string {
  switch (issue.kind) {
    case 'start_past_end':
      return t('timing.start_past_end');
    case 'end_past_file':
      return t('timing.end_past_file');
    case 'vamp_empty':
      return t('timing.vamp_empty', { number: issue.index + 1 });
    case 'vamp_before_start':
      return t('timing.vamp_before_start', { number: issue.index + 1 });
    case 'vamp_past_end':
      return t('timing.vamp_past_end', { number: issue.index + 1 });
    case 'vamp_overlap':
      return t('timing.vamp_overlap', { number: issue.index + 1, other: issue.other + 1 });
    case 'vamp_cut':
      return t('timing.vamp_cut', { number: issue.index + 1 });
    case 'mark_outside':
      return t('timing.mark_outside', { number: issue.index + 1 });
    case 'mark_duplicate':
      return t('timing.mark_duplicate', { number: issue.index + 1 });
    case 'fade_in_too_long':
      return t('timing.fade_in_too_long');
    case 'end_fade_too_long':
      return t('timing.end_fade_too_long');
    case 'loop_overlap_cut':
      return t('timing.loop_overlap_cut');
  }
}

function valueText(issue: ValueIssue): string {
  const text = valueIssueText(issue);
  const item = 'item' in issue ? issue.item : null;
  return item === null ? text : t('value.item', { number: item, issue: text });
}

function valueIssueText(issue: ValueIssue): string {
  switch (issue.kind) {
    case 'negative':
      return t('value.negative', { name: FIELDS[issue.field]() });
    case 'pan_range':
      return t('value.pan_range');
    case 'rate_range':
      return t('value.rate_range', { min: issue.min, max: issue.max });
    case 'transpose_range':
      return t('value.transpose_range', { max: issue.max });
    case 'output_unknown':
      return t('value.output_unknown', { output: issue.output });
    case 'next_item_only_playlist':
      return t('value.next_item_only_playlist');
  }
}

function cueProblemText(p: CueProblem): string {
  switch (p.kind) {
    case 'start_from_sound_only':
      return t('problem.start_from_sound_only', { cue: cueName(p.cue) });
    case 'start_from_after_end':
      return t('problem.start_from_after_end', { time: fmtSecs(p.time), cue: cueName(p.cue), end: fmtSecs(p.end) });
    case 'start_from_before_start':
      return t('problem.start_from_before_start', {
        time: fmtSecs(p.time),
        cue: cueName(p.cue),
        start: fmtSecs(p.start),
      });
    case 'no_stopwatch':
      return t('problem.no_stopwatch');
    case 'stopwatch_deleted':
      return t('problem.stopwatch_deleted');
    case 'target_deleted':
      return t('problem.target_deleted');
    case 'targets_deleted':
      return t('problem.targets_deleted', { gone: p.gone, total: p.total });
    case 'no_previous':
      return t('problem.no_previous');
    case 'start_needs_chosen':
      return t('problem.start_needs_chosen');
    case 'output_missing':
      return t('problem.output_missing', { output: p.output });
    case 'starts_itself':
      return t('problem.starts_itself');
    case 'position_playlist':
      return t('problem.position_playlist', { cue: cueName(p.cue) });
    case 'position_after_end':
      return t('problem.position_after_end', { time: fmtSecs(p.time), cue: cueName(p.cue), end: fmtSecs(p.end) });
    case 'position_before_start':
      return t('problem.position_before_start', {
        time: fmtSecs(p.time),
        cue: cueName(p.cue),
        start: fmtSecs(p.start),
      });
    case 'no_marks':
      return t('problem.no_marks', { cue: cueName(p.cue) });
    case 'mark_missing':
      return t('problem.mark_missing', { index: p.index, cue: cueName(p.cue), count: p.count });
    case 'hotkey_no_key':
      return t('problem.hotkey_no_key');
    case 'hotkey_outside_main':
      return t('problem.hotkey_outside_main', { key: keyText(p.key) });
    case 'hotkey_reserved':
      return t('problem.hotkey_reserved', { key: keyText(p.key) });
    case 'hotkey_taken_globally':
      return t('problem.hotkey_taken_globally', { key: keyText(p.key), action: actionName(p.action) });
    case 'midi_outside_main':
      return t('problem.midi_outside_main');
    case 'midi_taken_globally':
      return t('problem.midi_taken_globally', { action: actionName(p.action) });
    case 'release_ignored':
      return t('problem.release_ignored');
    case 'file_unreadable':
      return p.item === null
        ? t('problem.file_unreadable', { reason: errorText(p.reason) })
        : t('problem.item_unreadable', { item: p.item, reason: errorText(p.reason) });
  }
}

const VALUE_KINDS = new Set<string>([
  'negative',
  'pan_range',
  'rate_range',
  'transpose_range',
  'output_unknown',
  'next_item_only_playlist',
]);

/** A further problem of a cue, in the current language. */
export function problemText(p: Problem): string {
  return VALUE_KINDS.has(p.kind) ? valueText(p as ValueIssue) : cueProblemText(p as CueProblem);
}
