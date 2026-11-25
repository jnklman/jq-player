import { afterEach, describe, expect, it } from 'vitest';

import type { Notice, NoticeKind } from './api';
import { setLanguage } from './i18n/index.svelte';
import { cueName, noticeKey, noticeText } from './notices';

const cue = { number: '1', title: 'Rain' };
const error = 'disk gone';

/** One notice of every kind (the compiler checks the list is complete). */
const every: { [K in NoticeKind['kind']]: Omit<Extract<NoticeKind, { kind: K }>, 'kind'> } = {
  already_fading: { cue },
  underrun: { file: 'rain.wav', ms: 20 },
  too_many_stretchers: { cue },
  too_many_voices: {},
  already_running: { cue },
  placeholder: { cue },
  no_stopwatch_chosen: { cue },
  stopwatch_missing: { cue },
  cue_failed: { cue, item: 2, error },
  no_target: { cue },
  target_in_group: { cue, target: { number: '2a', title: '' } },
  target_missing: { cue },
  goto_loop: { cue },
  start_loop: { cue },
  not_preloaded: { cue },
  fading_no_effect: { cue, ignores: 'reset' },
  mark_missing: { cue },
  preview_placeholder: {},
  no_preview_output: {},
  preview_not_loadable: { error },
  preview_failed: { error },
  engine_failed: { error },
  watcher_unavailable: { error },
  audio_unavailable: { error },
  preview_unavailable: { error },
  stream_error: { device: 'preview', error },
  device_lost: { device: 'main', name: 'USB', reason: 'unplugged' },
  device_back: { device: 'main', name: 'USB' },
  device_switched: { device: 'preview', name: 'Speakers' },
  output_channels: { output: null, first: 3, second: 4, channels: 2 },
  keep_awake_failed: { error },
  osc_listen_failed: { port: 53000, error },
  osc_send_unavailable: { error },
  midi_in_failed: { error },
  midi_out_failed: { error },
  msc_failed: { error },
  learn_failed: { error: { kind: 'show_mode_locked' } },
  writes_pending: {},
  settings_ignored: { error },
  link_synced: { cue },
  library_device_added: { name: 'X32', port: 10023 },
  autosave_unreadable: { error },
  autosave_not_deleted: { error },
  file_missing: { file: 'rain.wav' },
  case_mismatch: { file: 'rain.wav', actual: 'Rain.WAV' },
  changed_while_playing: { cue },
  file_now_missing: { file: 'rain.wav' },
  file_back: { file: 'rain.wav' },
  file_changed: { file: 'rain.wav' },
  timing: { cue, item: null, issue: { kind: 'end_past_file' } },
  lock_failed: { count: 2, file: 'rain.wav', error },
  not_copied: { file: 'rain.wav', error },
  search_aborted: {},
  import_failed: { error: 'far.wav: gone' },
  relinked_some: { relinked: 1, missing: 2 },
  normalize_waiting: { count: 1 },
  outside_show_folder: { path: '/far/rain.wav' },
  name_taken: { name: 'rain.wav', stored: 'rain 2.wav' },
  save_as_missing: { count: 1, file: 'rain.wav' },
  save_as_outside: { count: 3, file: 'rain.wav' },
  startup_open_failed: { path: '/shows/a.jqshow', error: { kind: 'file_too_new', path: 'a', found: 9, current: 1 } },
  settings_not_saved: { error },
  autosave_failed: { error },
  writer_panicked: {},
  internal_error: {},
};

const notices = Object.entries(every).map(([kind, data]) => ({ level: 'warning', kind, ...data }) as Notice);

afterEach(() => setLanguage({ setting: 'en', resolved: 'en' }));

describe('noticeText', () => {
  it.each(['en', 'de'])('words every kind with all its data (%s)', (language) => {
    setLanguage({ setting: language, resolved: language });
    for (const notice of notices) {
      const text = noticeText(notice);
      expect(text, notice.kind).not.toMatch(/[{}]|^$/);
    }
  });

  it('names a cue by number and title, else by the one it has', () => {
    expect(cueName(cue)).toBe('1 “Rain”');
    expect(cueName({ number: '1', title: '' })).toBe('1');
    expect(cueName({ number: '', title: 'Rain' })).toBe('“Rain”');
    expect(noticeText({ level: 'error', kind: 'cue_failed', cue, item: null, error })).toBe('Cue 1 “Rain”: disk gone');
  });
});

describe('noticeKey', () => {
  const key = (notice: NoticeKind) => noticeKey({ level: 'warning', ...notice } as Notice);

  it('is the same for the same notice and for news about the same device or file', () => {
    expect(key({ kind: 'already_running', cue })).toBe(key({ kind: 'already_running', cue }));
    expect(key({ kind: 'already_running', cue })).not.toBe(
      key({ kind: 'already_running', cue: { ...cue, number: '2' } }),
    );
    expect(key({ kind: 'device_lost', device: 'main', name: 'A', reason: 'x' })).toBe(
      key({ kind: 'device_back', device: 'main', name: 'A' }),
    );
    expect(key({ kind: 'device_back', device: 'main', name: 'A' })).not.toBe(
      key({ kind: 'device_back', device: 'preview', name: 'A' }),
    );
    expect(key({ kind: 'file_now_missing', file: 'a.wav' })).toBe(key({ kind: 'file_back', file: 'a.wav' }));
    expect(key({ kind: 'normalize_waiting', count: 1 })).toBe(key({ kind: 'normalize_waiting', count: 3 }));
  });
});
