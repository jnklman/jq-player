// What the backend says went wrong (`Detail` in api.ts): why a command failed, or what a layer below reports (a device,
// a file, MIDI, OSC). A message of a kind is worded here; a text (a system's error, an exception) is shown as it is.

import type { CheckedSetting, CommandError, LayerMessage } from './api';
import { deviceName } from './format';
import { t } from './i18n/index.svelte';
import { keyText } from './keys.svelte';
import { cueName } from './notices';
import { actionName } from './settings';

const SETTINGS: Record<CheckedSetting, () => string> = {
  fade_all: () => t('error.setting.fade_all'),
  end_warning: () => t('error.setting.end_warning'),
  panic_fade: () => t('error.setting.panic_fade'),
  go_lockout: () => t('error.setting.go_lockout'),
  volume_step: () => t('error.setting.volume_step'),
  pan_step: () => t('error.setting.pan_step'),
  rate_step: () => t('error.setting.rate_step'),
};

/** True for a command's typed rejection. */
export function isCommandError(e: unknown): e is CommandError {
  return typeof e === 'object' && e !== null && typeof (e as { kind?: unknown }).kind === 'string';
}

/** What went wrong, in the current language. */
export function errorText(e: unknown): string {
  return isCommandError(e) ? commandErrorText(e) : String(e);
}

function commandErrorText(e: CommandError): string {
  switch (e.kind) {
    case 'no_show':
      return t('error.no_show');
    case 'no_reply':
      return t('error.no_reply');
    case 'backend_locked':
      return t('error.backend_locked');
    case 'thread_ended':
      return t('error.thread_ended');
    case 'writer_panicked':
      return t('notice.writer_panicked');
    case 'peak_cache_not_cleared':
      return t('error.peak_cache_not_cleared', { error: e.error });
    case 'media_dir_not_found':
      return t('error.media_dir_not_found', { path: e.path });
    case 'file_manager_failed':
      return t('error.file_manager_failed', { error: e.error });
    case 'show_mode_locked':
      return t('error.show_mode_locked');
    case 'media_dir_busy_cue':
      return t('error.media_dir_busy_cue');
    case 'media_dir_busy_files':
      return t('error.media_dir_busy_files');
    case 'decide_recovery':
      return t('error.decide_recovery');
    case 'media_dir_unreachable':
      return t('error.media_dir_unreachable', { error: e.error });
    case 'not_a_folder':
      return t('error.not_a_folder', { path: e.path });
    case 'show_dir_unreachable':
      return t('error.show_dir_unreachable', { error: e.error });
    case 'media_dir_not_storable':
      return t('error.media_dir_not_storable', { path: e.path });
    case 'media_dir_has_show':
      return t('error.media_dir_has_show');
    case 'name_empty':
      return t('error.name_empty');
    case 'save_as_busy_cue':
      return t('error.save_as_busy_cue');
    case 'save_as_busy_files':
      return t('error.save_as_busy_files');
    case 'save_as_changed':
      return t('error.save_as_changed');
    case 'media_not_copied':
      return t('error.media_not_copied', { error: errorText(e.error) });
    case 'convert_control_only':
      return t('error.convert_control_only');
    case 'countdown_range':
      return t('error.countdown_range', { name: e.name });
    case 'stopwatch_id':
      return t('error.stopwatch_id', { id: e.id });
    case 'stopwatch_name':
      return t('error.stopwatch_name');
    case 'device_id':
      return t('error.device_id', { id: e.id });
    case 'device_address':
      return t('error.device_address', { name: e.name });
    case 'playlist_not_found':
      return t('error.playlist_not_found');
    case 'item_not_found':
      return t('error.item_not_found');
    case 'sound_not_found':
      return t('error.sound_not_found');
    case 'file_unused':
      return t('error.file_unused');
    case 'loudness_range':
      return t('error.loudness_range', { min: e.min, max: e.max });
    case 'search_running':
      return t('error.search_running');
    case 'invalid_file_name':
      return t('error.invalid_file_name');
    case 'no_group_in_group':
      return t('error.no_group_in_group');
    case 'group_not_found':
      return t('error.group_not_found');
    case 'output_id':
      return t('error.output_id', { id: e.id });
    case 'output_name':
      return t('error.output_name');
    case 'main_output':
      return t('error.main_output');
    case 'output_in_use':
      return t('error.output_in_use', { output: e.output, cue: cueName(e.cue) });
    case 'level_limits':
      return t('error.level_limits', { min: e.min, max: e.max });
    case 'level_gap':
      return t('error.level_gap');
    case 'next_item_playlist_only':
      return t('error.next_item_playlist_only');
    case 'scene_main_only':
      return t('error.scene_main_only');
    case 'goto_top_level':
      return t('error.goto_top_level');
    case 'others_fade':
      return t('error.others_fade', { max: e.max });
    case 'others_duck':
      return t('error.others_duck');
    case 'hotkey_main_only':
      return t('error.hotkey_main_only');
    case 'hotkey_fixed':
    case 'key_fixed':
      return t('error.key_fixed', { key: keyText(e.key) });
    case 'hotkey_global':
      return t('error.hotkey_global', { key: keyText(e.key), action: actionName(e.action) });
    case 'midi_main_only':
      return t('error.midi_main_only');
    case 'midi_global':
      return t('error.midi_global', { action: actionName(e.action) });
    case 'no_midi_input':
      return t('error.no_midi_input');
    case 'keyboard_only':
      return t('error.keyboard_only', { action: actionName(e.action) });
    case 'setting_negative':
      return t('error.setting_negative', { what: SETTINGS[e.setting]() });
    case 'setting_positive':
      return t('error.setting_positive', { what: SETTINGS[e.setting]() });
    case 'jump_times':
      return t('error.jump_times');
    case 'keep_alive_level':
      return t('error.keep_alive_level', { min: e.min, max: e.max });
    case 'msc_device':
      return t('error.msc_device');
    case 'buffer_size':
      return t('error.buffer_size', { min: e.min, max: e.max });
    case 'osc_port':
      return t('error.osc_port');
    case 'channels_from_one':
      return t('error.channels_from_one');
    case 'min_channels':
      return t('error.min_channels');
    case 'settings_show_mode_locked':
      return t('error.settings_show_mode_locked');
    case 'audio_device_busy':
      return t('error.audio_device_busy');
    case 'device_too_few_channels':
      return t('error.device_too_few_channels', { name: deviceName(e.name), max: e.max, channels: e.channels });
    case 'pair_missing': {
      const params = {
        output: e.output ?? t('notice.preview'),
        first: e.first,
        second: e.second,
        name: deviceName(e.name),
        limit: e.limit,
      };
      return e.set ? t('error.pair_missing_set', params) : t('error.pair_missing_has', params);
    }
    case 'update_check_failed':
      return t('error.update_check_failed', { error: e.error });
    case 'update_install_failed':
      return t('error.update_install_failed', { error: e.error });
    case 'update_busy':
      return t('error.update_busy');
    case 'update_locked':
      return t('error.update_locked');
    case 'update_installing':
      return t('error.update_installing');
    case 'update_unsaved':
      return t('error.update_unsaved');
    case 'update_none_pending':
      return t('error.update_none_pending');
    case 'update_writes_pending':
      return t('error.update_writes_pending');
    case 'update_portable':
      return t('error.update_portable');
    case 'update_rpm':
      return t('error.update_rpm');
    case 'update_not_bundled':
      return t('error.update_not_bundled');
    case 'cue_not_found':
      return t('error.cue_not_found');
    case 'link_needs_two':
      return t('error.link_needs_two');
    case 'link_not_groups':
      return t('error.link_not_groups');
    case 'link_same_type':
      return t('error.link_same_type');
    case 'nothing_selected':
      return t('error.nothing_selected');
    case 'group_main_only':
      return t('error.group_main_only');
    case 'group_not_scenes':
      return t('error.group_not_scenes');
    case 'group_not_nested':
      return t('error.group_not_nested');
    case 'file_io':
      return `${e.path}: ${e.error}`;
    case 'file_invalid':
      return t('error.file_invalid', { path: e.path, error: e.error });
    case 'file_wrong_format':
      return t('error.file_wrong_format', { path: e.path, found: e.found });
    case 'file_too_new':
      return t('error.file_too_new', { path: e.path, found: e.found, current: e.current });
    case 'invalid_name':
      return t('error.invalid_name', { name: e.name });
    case 'exists':
      return t('error.exists', { path: e.path });
    case 'key_missing':
      return t('error.key_missing', { action: actionName(e.action) });
    case 'key_taken':
      return t('error.key_taken', { key: keyText(e.key), action: actionName(e.action) });
    case 'too_many_keys':
      return t('error.too_many_keys', { action: actionName(e.action), max: e.max });
    default:
      return layerText(e);
  }
}

const MIDI_VALUES: Record<Extract<LayerMessage, { kind: 'midi_out_of_range' }>['what'], () => string> = {
  program: () => t('midi.what.program'),
  controller: () => t('midi.what.controller'),
  value: () => t('midi.what.value'),
  note: () => t('midi.what.note'),
  velocity: () => t('midi.what.velocity'),
  msc_device: () => t('midi.what.msc_device'),
};

const REGION_PROBLEMS: Record<Extract<LayerMessage, { kind: 'engine_invalid_region' }>['problem'], () => string> = {
  start_after_end: () => t('error.engine_region.start_after_end'),
  loop_backwards: () => t('error.engine_region.loop_backwards'),
  loop_past_end: () => t('error.engine_region.loop_past_end'),
  loops_unordered: () => t('error.engine_region.loops_unordered'),
  marks_unsorted: () => t('error.engine_region.marks_unsorted'),
};

function layerText(e: LayerMessage): string {
  switch (e.kind) {
    case 'audio_host_unavailable':
      return t('error.audio_host_unavailable', { name: e.name, available: e.available });
    case 'audio_no_default_device':
      return t('error.audio_no_default_device');
    case 'audio_device_not_found':
      return t('error.audio_device_not_found', { name: e.name });
    case 'audio_unsupported_format':
      return t('error.audio_unsupported_format', { format: e.format });
    case 'audio_cpal':
      return t('error.audio_cpal', { error: e.error });
    case 'audio_stopped':
      return t('error.audio_stopped');
    case 'audio_channels':
      return t('error.audio_channels');
    case 'audio_xruns':
      return t('error.audio_xruns', { count: e.count });
    case 'audio_device_gone':
      return t('error.audio_device_gone');
    case 'audio_stream_invalidated':
      return t('error.audio_stream_invalidated');
    case 'audio_host_lost':
      return t('error.audio_host_lost');
    case 'audio_default_changed':
      return t('error.audio_default_changed');
    case 'audio_no_data':
      return t('error.audio_no_data');
    case 'audio_lost_recovering':
      return t('error.audio_lost_recovering', { reason: errorText(e.reason) });
    case 'audio_lost_last_attempt':
      return t('error.audio_lost_last_attempt', { reason: errorText(e.reason), error: errorText(e.error) });
    case 'engine_queue_full':
      return t('error.engine_queue_full');
    case 'engine_sample_rate_mismatch':
      return t('error.engine_sample_rate_mismatch', { buffer: e.buffer, engine: e.engine });
    case 'engine_invalid_region':
      return t('error.engine_invalid_region', { problem: REGION_PROBLEMS[e.problem]() });
    case 'engine_output_channel':
      return t('error.engine_output_channel', { first: e.first, second: e.second, channels: e.channels });
    case 'media_file':
      return t('error.media_file', { path: e.path, reason: errorText(e.reason) });
    case 'media_no_audio_track':
      return t('error.media_no_audio_track');
    case 'media_unsupported_codec':
      return t('error.media_unsupported_codec', { codec: e.codec });
    case 'media_midi':
      return t('error.media_midi');
    case 'media_resample_failed':
      return t('error.media_resample_failed', { message: e.message });
    case 'media_cancelled':
      return t('error.media_cancelled');
    case 'media_read_error':
      return t('error.media_read_error', { error: e.error });
    case 'media_unsupported':
      return t('error.media_unsupported');
    case 'media_corrupt':
      return t('error.media_corrupt');
    case 'media_seek_failed':
      return t('error.media_seek_failed');
    case 'media_too_large':
      return t('error.media_too_large');
    case 'media_decode_error':
      return t('error.media_decode_error');
    case 'copy_no_file_name':
      return t('error.copy_no_file_name');
    case 'copy_cancelled':
      return t('error.copy_cancelled');
    case 'outside_media':
      return t('error.outside_media');
    case 'midi_unavailable':
      return t('error.midi_unavailable', { error: e.error });
    case 'midi_no_input':
      return t('error.midi_no_input', { name: e.name });
    case 'midi_input_error':
      return t('error.midi_input_error', { name: e.name, error: e.error });
    case 'midi_no_output':
      return t('error.midi_no_output', { name: e.name });
    case 'midi_output_error':
      return t('error.midi_output_error', { name: e.name, error: e.error });
    case 'midi_msc_no_cue':
      return t('error.midi_msc_no_cue', { number: e.number });
    case 'midi_out_of_range':
      return t('error.midi_out_of_range', { what: MIDI_VALUES[e.what](), value: e.value });
    case 'midi_bad_channel':
      return t('error.midi_bad_channel', { channel: e.channel });
    case 'midi_msc_cue_digits':
      return t('error.midi_msc_cue_digits', { text: e.text });
    case 'osc_sender_stopped':
      return t('error.osc_sender_stopped', { target: e.target });
    case 'osc_address_not_found':
      return t('error.osc_address_not_found', { target: e.target });
    case 'osc_address_slash':
      return t('error.osc_address_slash', { address: e.address });
    case 'osc_encode_failed':
      return t('error.osc_encode_failed', { error: e.error });
    case 'runner_still_copying':
      return t('error.runner_still_copying');
    case 'runner_file_not_loadable':
      return t('error.runner_file_not_loadable', { error: errorText(e.error) });
    case 'runner_no_osc_device':
      return t('error.runner_no_osc_device');
    case 'runner_osc_device_missing':
      return t('error.runner_osc_device_missing', { device: e.device });
    case 'runner_osc_unavailable':
      return t('error.runner_osc_unavailable');
    case 'runner_no_midi_output':
      return t('error.runner_no_midi_output');
    case 'panels_locked':
      return t('error.panels_locked');
    case 'panels_not_opened':
      return t('error.panels_not_opened', { panels: e.panels.join(', ') });
    case 'keylabels_no_display':
      return t('error.keylabels_no_display');
    case 'keylabels_no_layout':
      return t('error.keylabels_no_layout');
    case 'keylabels_unavailable':
      return t('error.keylabels_unavailable');
    default:
      // A kind this UI does not know yet (a newer backend): its data says the most.
      return JSON.stringify(e);
  }
}
