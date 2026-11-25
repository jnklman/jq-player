import { afterEach, describe, expect, it } from 'vitest';

import type { Chord, CommandError, CueProblem, TimingIssue, ValueIssue } from './api';
import { errorText, isCommandError } from './errors';
import { setLanguage } from './i18n/index.svelte';
import { problemText, timingText } from './problems';

type Data<T extends { kind: string }> = { [K in T['kind']]: Omit<Extract<T, { kind: K }>, 'kind'> };

const cue = { number: '1', title: 'Rain' };
const key: Chord = { code: 'F5', label: '', ctrl: false, shift: false, alt: false };
const error = 'disk gone';

/** One command error of every kind (the compiler checks the list is complete). */
const errors: Data<CommandError> = {
  no_show: {},
  no_reply: {},
  backend_locked: {},
  thread_ended: {},
  writer_panicked: {},
  peak_cache_not_cleared: { error },
  media_dir_not_found: { path: '/media' },
  file_manager_failed: { error },
  library_file: { path: '/d/x.jqlibrary', error: { kind: 'too_new', found: 2, current: 1 } },
  library_io: { path: '/d', error },
  no_library_folder: {},
  library_not_found: {},
  preset_not_found: {},
  library_built_in: {},
  preset: { error: { kind: 'param', preset: 'p', param: 'ch', problem: { kind: 'out_of_range', min: 1, max: 32 } } },
  not_from_preset: {},
  show_mode_locked: {},
  media_dir_busy_cue: {},
  media_dir_busy_files: {},
  decide_recovery: {},
  media_dir_unreachable: { error },
  not_a_folder: { path: '/x' },
  show_dir_unreachable: { error },
  media_dir_not_storable: { path: '/x' },
  media_dir_has_show: {},
  name_empty: {},
  save_as_busy_cue: {},
  save_as_busy_files: {},
  save_as_changed: {},
  media_not_copied: { error },
  convert_control_only: {},
  countdown_range: { name: 'Pause' },
  stopwatch_id: { id: 'w' },
  stopwatch_name: {},
  device_id: { id: 'd' },
  device_address: { name: 'Desk' },
  playlist_not_found: {},
  item_not_found: {},
  sound_not_found: {},
  file_unused: {},
  loudness_range: { min: -40, max: -6 },
  search_running: {},
  invalid_file_name: {},
  no_group_in_group: {},
  group_not_found: {},
  output_id: { id: 'o' },
  output_name: {},
  main_output: {},
  output_in_use: { output: 'Monitor', cue },
  level_limits: { min: -100, max: 24 },
  level_gap: {},
  next_item_playlist_only: {},
  scene_main_only: {},
  goto_top_level: {},
  others_fade: { max: 600 },
  others_duck: {},
  hotkey_main_only: {},
  hotkey_fixed: { key },
  hotkey_global: { key, action: 'go' },
  midi_main_only: {},
  midi_global: { action: 'stop_all' },
  no_midi_input: {},
  keyboard_only: { action: 'preview' },
  setting_negative: { setting: 'fade_all' },
  setting_positive: { setting: 'rate_step' },
  jump_times: {},
  keep_alive_level: { min: -90, max: -40 },
  msc_device: {},
  buffer_size: { min: 32, max: 4096 },
  osc_port: {},
  channels_from_one: {},
  min_channels: {},
  settings_show_mode_locked: {},
  audio_device_busy: {},
  device_too_few_channels: { name: 'AG08', max: 8, channels: 10 },
  pair_missing: { output: null, first: 9, second: 10, name: 'AG08', limit: 8, set: false },
  update_check_failed: { error },
  update_install_failed: { error },
  update_busy: {},
  update_locked: {},
  update_installing: {},
  update_unsaved: {},
  update_none_pending: {},
  update_writes_pending: {},
  update_portable: {},
  update_rpm: {},
  update_not_bundled: {},
  cue_not_found: {},
  link_needs_two: {},
  link_not_groups: {},
  link_same_type: {},
  nothing_selected: {},
  group_main_only: {},
  group_not_scenes: {},
  group_not_nested: {},
  file_io: { path: '/a.jqshow', error },
  file_invalid: { path: '/a.jqshow', error },
  file_wrong_format: { path: '/a.jqshow', found: 'other' },
  file_too_new: { path: '/a.jqshow', found: 9, current: 1 },
  invalid_name: { name: 'a/b' },
  exists: { path: '/a' },
  key_missing: { action: 'go' },
  key_fixed: { key },
  key_taken: { key, action: 'go' },
  too_many_keys: { action: 'go', max: 2 },
  audio_host_unavailable: { name: 'JACK', available: 'ALSA' },
  audio_no_default_device: {},
  audio_device_not_found: { name: 'USB' },
  audio_unsupported_format: { format: 'U8' },
  audio_cpal: { error },
  audio_stopped: {},
  audio_channels: {},
  audio_xruns: { count: 3 },
  audio_device_gone: {},
  audio_stream_invalidated: {},
  audio_host_lost: {},
  audio_default_changed: {},
  audio_no_data: {},
  audio_lost_recovering: { reason: { kind: 'audio_device_gone' } },
  audio_lost_last_attempt: {
    reason: { kind: 'audio_no_data' },
    error: { kind: 'audio_device_not_found', name: 'USB' },
  },
  engine_queue_full: {},
  engine_sample_rate_mismatch: { buffer: 44100, engine: 48000 },
  engine_invalid_region: { problem: 'loops_unordered' },
  engine_output_channel: { first: 3, second: 4, channels: 2 },
  media_file: { path: '/m/a.wav', reason: { kind: 'media_corrupt' } },
  media_no_audio_track: {},
  media_unsupported_codec: { codec: 'WMA' },
  media_midi: {},
  media_resample_failed: { message: 'ratio' },
  media_cancelled: {},
  media_read_error: { error },
  media_unsupported: {},
  media_corrupt: {},
  media_seek_failed: {},
  media_too_large: {},
  media_decode_error: {},
  copy_no_file_name: {},
  copy_cancelled: {},
  outside_media: {},
  midi_unavailable: { error },
  midi_no_input: { name: 'Keys' },
  midi_input_error: { name: 'Keys', error },
  midi_no_output: { name: 'Desk' },
  midi_output_error: { name: 'Desk', error },
  midi_msc_no_cue: { number: '4' },
  midi_out_of_range: { what: 'velocity', value: 200 },
  midi_bad_channel: { channel: 17 },
  midi_msc_cue_digits: { text: '1a' },
  osc_sender_stopped: { target: 'desk:9000' },
  osc_address_not_found: { target: 'desk:9000' },
  osc_address_slash: { address: 'go' },
  osc_encode_failed: { error },
  runner_still_copying: {},
  runner_file_not_loadable: { error: { kind: 'media_file', path: '/m/a.wav', reason: 'gone' } },
  runner_no_osc_device: {},
  runner_osc_device_missing: { device: 'weg' },
  runner_osc_no_address: { device: 'X32' },
  runner_osc_unavailable: {},
  runner_no_midi_output: {},
  panels_locked: {},
  panels_not_opened: { panels: ['cues', 'stopwatches'] },
  keylabels_no_display: {},
  keylabels_no_layout: {},
  keylabels_unavailable: {},
};

const problems: Data<CueProblem> = {
  start_from_sound_only: { cue },
  start_from_after_end: { time: 3, cue, end: 2.2 },
  start_from_before_start: { time: 0.5, cue, start: 1 },
  no_stopwatch: {},
  stopwatch_deleted: {},
  target_deleted: {},
  targets_deleted: { gone: 1, total: 2 },
  no_previous: {},
  start_needs_chosen: {},
  output_missing: { output: 'weg' },
  starts_itself: {},
  position_playlist: { cue },
  position_after_end: { time: 99, cue, end: 30 },
  position_before_start: { time: 0.5, cue, start: 1 },
  no_marks: { cue },
  mark_missing: { index: 3, cue, count: 2 },
  hotkey_no_key: {},
  hotkey_outside_main: { key },
  hotkey_reserved: { key },
  hotkey_taken_globally: { key, action: 'stop_all' },
  midi_outside_main: {},
  midi_taken_globally: { action: 'go' },
  release_ignored: {},
  file_unreadable: { item: 2, reason: 'not audio' },
};

const values: Data<ValueIssue> = {
  negative: { field: 'fade_duration', item: null },
  pan_range: { item: 2 },
  rate_range: { item: null, min: 50, max: 150 },
  transpose_range: { item: null, max: 12 },
  output_unknown: { output: 'gone' },
  next_item_only_playlist: {},
};

const timings: Data<TimingIssue> = {
  start_past_end: {},
  end_past_file: {},
  vamp_empty: { index: 0 },
  vamp_before_start: { index: 1 },
  vamp_past_end: { index: 2 },
  vamp_overlap: { index: 3, other: 0 },
  vamp_cut: { index: 4 },
  mark_outside: { index: 5 },
  mark_duplicate: { index: 6 },
  fade_in_too_long: {},
  end_fade_too_long: {},
  loop_overlap_cut: {},
};

const all = <T>(data: Record<string, object>) => Object.entries(data).map(([kind, d]) => ({ kind, ...d }) as T);

afterEach(() => setLanguage({ setting: 'en', resolved: 'en' }));

describe('wording', () => {
  it.each(['en', 'de'])('words every error, problem and timing issue with all its data (%s)', (language) => {
    setLanguage({ setting: language, resolved: language });
    const texts = [
      ...all<CommandError>(errors).map(errorText),
      ...all<CueProblem>(problems).map(problemText),
      ...all<ValueIssue>(values).map(problemText),
      ...all<TimingIssue>(timings).map(timingText),
    ];
    for (const text of texts) expect(text).not.toMatch(/[{}]|^$/);
  });

  it('fills in cues, keys, actions, numbers and seconds', () => {
    expect(errorText({ kind: 'hotkey_global', key, action: 'go' })).toBe('F5 is assigned globally to “GO”');
    expect(errorText({ kind: 'output_in_use', output: 'Monitor', cue })).toBe(
      'Output “Monitor” is still used by cue 1 “Rain”',
    );
    expect(errorText({ kind: 'setting_negative', setting: 'fade_all' })).toBe('FADE ALL time must be a number ≥ 0');
    expect(errorText({ kind: 'pair_missing', ...errors.pair_missing })).toBe(
      'Preview: channels 9/10 do not exist, “AG08” has 8 channels',
    );
    expect(problemText({ kind: 'start_from_after_end', ...problems.start_from_after_end })).toBe(
      'Start from 3.0 s is behind the end of cue 1 “Rain” (2.2 s) – the cue ends at once',
    );
    expect(problemText({ kind: 'pan_range', item: 2 })).toBe('Item 2: Pan is outside −1…+1 – will be limited');
    expect(problemText({ kind: 'negative', field: 'pre_wait', item: null })).toBe('Pre-wait is negative – counts as 0');
    expect(timingText({ kind: 'vamp_overlap', index: 3, other: 0 })).toBe('Vamp 4 overlaps vamp 1 – skipped');
    setLanguage({ setting: 'de', resolved: 'de' });
    expect(problemText({ kind: 'start_from_after_end', ...problems.start_from_after_end })).toContain('3,0 s');
  });

  it('words what the layers below say, nested ones too, and the program’s own devices', () => {
    expect(errorText({ kind: 'media_file', path: '/m/a.wav', reason: { kind: 'media_unsupported' } })).toBe(
      '/m/a.wav: not a supported audio format',
    );
    expect(errorText({ kind: 'audio_lost_recovering', reason: { kind: 'audio_device_gone' } })).toBe(
      'Device no longer available; the audio engine is being recovered',
    );
    expect(errorText({ kind: 'midi_out_of_range', what: 'velocity', value: 200 })).toBe('Velocity 200 is not in 0–127');
    expect(errorText({ kind: 'device_too_few_channels', name: '@default', max: 2, channels: 4 })).toBe(
      '“Default device” only has 2 channels, 4 are set',
    );
  });

  it('shows what is no command error as it is', () => {
    expect(isCommandError({ kind: 'no_show' })).toBe(true);
    expect(isCommandError('refused')).toBe(false);
    expect(errorText('refused')).toBe('refused');
    expect(errorText(new Error('boom'))).toBe('Error: boom');
    expect(errorText({ kind: 'from_a_newer_backend', x: 1 })).toBe('{"kind":"from_a_newer_backend","x":1}');
  });
});
