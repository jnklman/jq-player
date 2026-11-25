// Notices from the backend (`Notice` in api.ts): their wording, and which of them replace each other.

import type { CueName, DeviceRole, Notice } from './api';
import { errorText } from './errors';
import { deviceName } from './format';
import { t } from './i18n/index.svelte';
import { timingText } from './problems';

/** How a notice names a cue: number and title, else the one it has. */
export function cueName({ number, title }: CueName): string {
  if (number && title) return t('notice.cue_both', { number, title });
  return number || t('cue.quoted', { title: title || t('cue.untitled') });
}

function device(role: DeviceRole): string {
  return role === 'main' ? t('notice.device.main') : t('notice.device.preview');
}

/** The text of a notice, in the current language. */
export function noticeText(n: Notice): string {
  switch (n.kind) {
    case 'already_fading':
      return t('notice.already_fading', { cue: cueName(n.cue) });
    case 'underrun':
      return t('notice.underrun', { file: n.file, ms: n.ms });
    case 'too_many_stretchers':
      return t('notice.too_many_stretchers', { cue: cueName(n.cue) });
    case 'too_many_voices':
      return t('notice.too_many_voices');
    case 'already_running':
      return t('notice.already_running', { cue: cueName(n.cue) });
    case 'placeholder':
      return t('notice.placeholder', { cue: cueName(n.cue) });
    case 'no_stopwatch_chosen':
      return t('notice.no_stopwatch_chosen', { cue: cueName(n.cue) });
    case 'stopwatch_missing':
      return t('notice.stopwatch_missing', { cue: cueName(n.cue) });
    case 'cue_failed':
      return n.item === null
        ? t('notice.cue_failed', { cue: cueName(n.cue), error: errorText(n.error) })
        : t('notice.item_failed', { cue: cueName(n.cue), item: n.item, error: errorText(n.error) });
    case 'no_target':
      return t('notice.no_target', { cue: cueName(n.cue) });
    case 'target_in_group':
      return t('notice.target_in_group', { cue: cueName(n.cue), target: cueName(n.target) });
    case 'target_missing':
      return t('notice.target_missing', { cue: cueName(n.cue) });
    case 'goto_loop':
      return t('notice.goto_loop', { cue: cueName(n.cue) });
    case 'start_loop':
      return t('notice.start_loop', { cue: cueName(n.cue) });
    case 'not_preloaded':
      return t('notice.not_preloaded', { cue: cueName(n.cue) });
    case 'fading_no_effect': {
      const what = n.ignores === 'reset' ? t('notice.ignores.reset') : t('notice.ignores.fade_to_level');
      return t('notice.fading_no_effect', { cue: cueName(n.cue), what });
    }
    case 'mark_missing':
      return t('notice.mark_missing', { cue: cueName(n.cue) });
    case 'preview_placeholder':
      return t('notice.preview_placeholder');
    case 'no_preview_output':
      return t('notice.no_preview_output');
    case 'preview_not_loadable':
      return t('notice.preview_not_loadable', { error: errorText(n.error) });
    case 'preview_failed':
      return t('notice.preview_failed', { error: errorText(n.error) });
    case 'engine_failed':
      return t('notice.engine_failed', { error: errorText(n.error) });
    case 'watcher_unavailable':
      return t('notice.watcher_unavailable', { error: errorText(n.error) });
    case 'audio_unavailable':
      return t('notice.audio_unavailable', { error: errorText(n.error) });
    case 'preview_unavailable':
      return t('notice.preview_unavailable', { error: errorText(n.error) });
    case 'stream_error':
      return n.device === 'main'
        ? t('notice.stream_error.main', { error: errorText(n.error) })
        : t('notice.stream_error.preview', { error: errorText(n.error) });
    case 'device_lost':
      return t('notice.device_lost', {
        device: device(n.device),
        name: deviceName(n.name),
        reason: errorText(n.reason),
      });
    case 'device_back':
      return t('notice.device_back', { device: device(n.device), name: deviceName(n.name) });
    case 'device_switched':
      return t('notice.device_switched', { device: device(n.device), name: deviceName(n.name) });
    case 'output_channels':
      return t('notice.output_channels', {
        output: n.output ?? t('notice.preview'),
        first: n.first,
        second: n.second,
        channels: n.channels,
      });
    case 'keep_awake_failed':
      return t('notice.keep_awake_failed', { error: errorText(n.error) });
    case 'osc_listen_failed':
      return t('notice.osc_listen_failed', { port: n.port, error: errorText(n.error) });
    case 'osc_send_unavailable':
      return t('notice.osc_send_unavailable', { error: errorText(n.error) });
    case 'midi_in_failed':
    case 'midi_out_failed':
    case 'msc_failed':
      return errorText(n.error);
    case 'learn_failed':
    case 'import_failed':
      return errorText(n.error);
    case 'settings_not_saved':
      return t('notice.settings_not_saved', { error: errorText(n.error) });
    case 'autosave_failed':
      return t('notice.autosave_failed', { error: errorText(n.error) });
    case 'writes_pending':
      return t('notice.writes_pending');
    case 'settings_ignored':
      return t('notice.settings_ignored', { error: errorText(n.error) });
    case 'link_synced':
      return t('notice.link_synced', { cue: cueName(n.cue) });
    case 'library_device_added':
      return t('notice.library_device_added', { name: n.name, port: n.port });
    case 'autosave_unreadable':
      return t('notice.autosave_unreadable', { error: errorText(n.error) });
    case 'autosave_not_deleted':
      return t('notice.autosave_not_deleted', { error: errorText(n.error) });
    case 'file_missing':
      return t('notice.file_missing', { file: n.file });
    case 'case_mismatch':
      return t('notice.case_mismatch', { file: n.file, actual: n.actual });
    case 'changed_while_playing':
      return t('notice.changed_while_playing', { cue: cueName(n.cue) });
    case 'file_now_missing':
      return t('notice.file_now_missing', { file: n.file });
    case 'file_back':
      return t('notice.file_back', { file: n.file });
    case 'file_changed':
      return t('notice.file_changed', { file: n.file });
    case 'timing':
      return n.item === null
        ? t('notice.timing', { cue: cueName(n.cue), issue: timingText(n.issue) })
        : t('notice.timing_item', { cue: cueName(n.cue), item: n.item, issue: timingText(n.issue) });
    case 'lock_failed':
      return t('notice.lock_failed', { count: n.count, file: n.file, error: errorText(n.error) });
    case 'not_copied':
      return t('notice.not_copied', { file: n.file, error: errorText(n.error) });
    case 'search_aborted':
      return t('notice.search_aborted');
    case 'relinked_some':
      return t('notice.relinked_some', { relinked: n.relinked, missing: n.missing });
    case 'normalize_waiting':
      return t('notice.normalize_waiting', { count: n.count });
    case 'outside_show_folder':
      return t('notice.outside_show_folder', { path: n.path });
    case 'name_taken':
      return t('notice.name_taken', { name: n.name, stored: n.stored });
    case 'save_as_missing':
      return t('notice.save_as_missing', { count: n.count, file: n.file });
    case 'save_as_outside':
      return t('notice.save_as_outside', { count: n.count, file: n.file });
    case 'startup_open_failed':
      return t('notice.startup_open_failed', { path: n.path, error: errorText(n.error) });
    case 'writer_panicked':
      return t('notice.writer_panicked');
    case 'internal_error':
      return t('notice.internal_error');
  }
}

/**
 * Notices with the same key replace each other: the newer one says what is true now. Kinds about one thing that
 * changes (a device, a file, a count) share a key for that thing; any other notice is replaced only by the same one
 * again, so a repeated warning does not pile up.
 */
export function noticeKey(n: Notice): string {
  switch (n.kind) {
    case 'underrun':
      return `underrun:${n.file}`;
    case 'stream_error':
      return `stream_error:${n.device}`;
    case 'device_lost':
    case 'device_back':
    case 'device_switched':
      return `device:${n.device}`;
    case 'file_missing':
    case 'file_now_missing':
    case 'file_back':
    case 'file_changed':
      return `file:${n.file}`;
    case 'output_channels':
      return `output_channels:${n.output ?? ''}`;
    case 'audio_unavailable':
    case 'preview_unavailable':
    case 'engine_failed':
    case 'preview_failed':
    case 'keep_awake_failed':
    case 'osc_listen_failed':
    case 'osc_send_unavailable':
    case 'midi_in_failed':
    case 'midi_out_failed':
    case 'settings_ignored':
    case 'settings_not_saved':
    case 'autosave_failed':
    case 'lock_failed':
    case 'relinked_some':
    case 'normalize_waiting':
    case 'writes_pending':
      return n.kind;
    default:
      return JSON.stringify(n);
  }
}
