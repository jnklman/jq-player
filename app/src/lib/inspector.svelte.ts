// The inspector's tabs (which a cue type has, in a fixed order) and its
// notices (every problem of a cue, each pointing to the tab and field it
// concerns). The fields themselves are in components/inspector.

import type { Cue, CueInfo } from './api';
import { isControl } from './control';
import { fmtDb } from './format';
import { t, type TextKey } from './i18n/index.svelte';

export type InspectorTab = 'main' | 'sound' | 'flow' | 'trigger' | 'notes';

export interface TabDef {
  id: InspectorTab;
  label: string;
}

/** The tab chosen last; kept while other cues are selected, and used wherever the cue has it. */
export const inspectorView = $state({ tab: 'main' as InspectorTab });

/** Cues that play media: they have "Other cues on start" and a second trigger. */
export const playsMedia = (cue: Cue) => cue.type === 'sound' || cue.type === 'playlist' || cue.type === 'group';
/** Cues with level, normalization and speed (the "Sound" tab). */
export const hasAudio = (cue: Cue) => cue.type === 'sound' || cue.type === 'playlist';

const mainLabels: Partial<Record<Cue['type'], TextKey>> = {
  sound: 'inspector.tab.playback',
  playlist: 'inspector.tab.playback',
  group: 'inspector.tab.group',
  wait: 'inspector.tab.wait',
  goto: 'inspector.tab.goto',
  midi: 'inspector.tab.messages',
  osc: 'inspector.tab.messages',
};

/** Types whose own fields can be edited for several cues at once (`EffectRows`). */
export const MULTI_TYPES: Cue['type'][] = ['volume', 'pan', 'rate', 'reset', 'pause', 'exit_loop', 'wait', 'group'];

/** Name of the first tab, which holds what the cue type itself does; null: several cues share nothing there. */
function mainLabel(cues: Cue[]): string | null {
  const type = cues[0].type;
  const same = cues.every((cue) => cue.type === type);
  if (cues.length === 1 || (same && MULTI_TYPES.includes(type))) {
    const key = mainLabels[type] ?? (isControl(cues[0]) ? 'inspector.tab.action' : null);
    return key && t(key);
  }
  return cues.every(playsMedia) ? t('inspector.tab.playback') : null;
}

/** The tabs for the edited cues, always in the same order. `topLevel`: the (single) cue is not inside a group. */
export function inspectorTabs(cues: Cue[], topLevel: boolean): TabDef[] {
  if (cues.length === 0) return [];
  // A scene marker is only a heading: title, colour and script in the head, the detail text below.
  if (cues.some((cue) => cue.type === 'scene')) return [{ id: 'notes', label: t('inspector.tab.notes') }];
  const tabs: TabDef[] = [];
  const main = mainLabel(cues);
  if (main) tabs.push({ id: 'main', label: main });
  if (cues.every(hasAudio)) tabs.push({ id: 'sound', label: t('inspector.tab.sound') });
  tabs.push({ id: 'flow', label: t('inspector.tab.flow') });
  // Hotkey and MIDI trigger: one top-level cue; the second trigger: cues with media.
  if ((cues.length === 1 && topLevel) || cues.every(playsMedia)) {
    tabs.push({ id: 'trigger', label: t('inspector.tab.trigger') });
  }
  tabs.push({ id: 'notes', label: t('inspector.tab.notes') });
  return tabs;
}

/** The tab to show: the one chosen last if the cues have it, else the first. */
export function shownTab(tabs: TabDef[]): InspectorTab {
  return tabs.some((tab) => tab.id === inspectorView.tab) ? inspectorView.tab : (tabs[0]?.id ?? 'main');
}

export interface InspectorNotice {
  text: string;
  /** Where it can be fixed; none for the backend's general problems. */
  tab?: InspectorTab;
  /** `data-field` of the row to reveal. */
  field?: string;
}

/** Everything wrong with `cue`, for the notice box at the top of the inspector. */
export function noticesOf(cue: Cue, info: CueInfo | undefined, maxDb: number): InspectorNotice[] {
  if (!info) return [];
  const notices: InspectorNotice[] = info.problems.map((text) => ({ text }));
  if (info.missing) notices.push({ text: t('inspector.notice.missing'), tab: 'main', field: 'file' });
  if (info.case_mismatch) {
    notices.push({
      text: t('inspector.notice.case_mismatch', { name: info.case_mismatch }),
      tab: 'main',
      field: 'file',
    });
  }
  for (const warning of info.timing) notices.push({ text: warning.message, tab: 'main', field: 'range' });
  if (info.normalize_stale) {
    notices.push({
      text: t('inspector.notice.normalize_stale'),
      tab: 'sound',
      field: 'normalize',
    });
  }
  if (info.level_silenced) {
    notices.push({ text: t('inspector.notice.level_silenced'), tab: 'sound', field: 'volume' });
  }
  if (info.level_capped) {
    notices.push({
      text: t('inspector.notice.level_capped', { level: fmtDb(maxDb) }),
      tab: 'sound',
      field: 'volume',
    });
  }
  if (info.target_missing) {
    if (cue.type === 'goto') notices.push({ text: t('inspector.notice.goto_target'), tab: 'main', field: 'target' });
    else if (cue.type === 'stopwatch')
      notices.push({ text: t('inspector.notice.stopwatch_deleted'), tab: 'main', field: 'watch' });
    else if (cue.type === 'exit_loop') {
      notices.push({
        text: t('inspector.notice.exit_not_target'),
        tab: 'flow',
        field: 'next',
      });
    }
  }
  if (info.repeat_blocked) {
    notices.push({
      text: t('inspector.notice.repeat_blocked'),
      tab: 'flow',
      field: 'repeat',
    });
  }
  return notices;
}
