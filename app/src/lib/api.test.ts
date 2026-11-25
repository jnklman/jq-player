import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { settle } from '../test/store';
import { api, editsAnswered, events } from './api';

describe('commands', () => {
  it('each command reaches the backend under its name, with its arguments', async () => {
    const calls: [() => Promise<unknown>, string, object][] = [
      [() => api.getLanguage(), 'get_language', {}],
      [() => api.openShow('/a'), 'open_show', { path: '/a' }],
      [() => api.checkLevelLimits({ max_db: 0, min_db: -60 }), 'check_level_limits', {}],
      [() => api.startupShow(), 'startup_show', {}],
      [() => api.newShow('/d', 'n', true), 'new_show', { dir: '/d', name: 'n', createFolder: true }],
      [() => api.saveShow(), 'save_show', {}],
      [() => api.getShow(), 'get_show', {}],
      [() => api.action({ action: 'go' }), 'action', { action: { action: 'go' } }],
      [() => api.stopwatch('w', 'start'), 'stopwatch', { id: 'w', action: 'start' }],
      [() => api.edit({ op: 'undo' }), 'edit', { op: { op: 'undo' } }],
      [() => api.liveCue({ id: 'c' } as never), 'live_cue', { cue: { id: 'c' } }],
      [() => api.previewAt('c', 1), 'preview_at', { cue: 'c', at: 1, until: null }],
      [() => api.previewAt('c', 1, 2), 'preview_at', { until: 2 }],
      [() => api.setKeepAwake(true), 'set_keep_awake', { enabled: true }],
      [() => api.midiLearn(null), 'midi_learn', { target: null }],
      [() => api.keyLabels(), 'key_labels', {}],
      [() => api.windowAlert(true), 'window_alert', { on: true }],
      [() => api.midiPorts(), 'midi_ports', {}],
      [() => api.waveform('/m', 'f', 10), 'waveform', { mediaDir: '/m', file: 'f', buckets: 10 }],
      [() => api.waveformRange('/m', 'f', 1, 2, 3), 'waveform_range', { from: 1, to: 2, buckets: 3 }],
      [() => api.audibleRange('/m', 'f'), 'audible_range', { mediaDir: '/m', file: 'f' }],
      [() => api.peakCacheSize(), 'peak_cache_size', {}],
      [() => api.clearPeakCache(), 'clear_peak_cache', {}],
      [() => api.checkImport('/m', ['/x']), 'check_import', { paths: ['/x'] }],
      [() => api.unusedMedia(), 'unused_media', {}],
      [() => api.cleanMedia(['a']), 'clean_media', { files: ['a'] }],
      [() => api.recoverAutosave(false), 'recover_autosave', { apply: false }],
      [() => api.saveShowAs('/d', 'n', 'copy'), 'save_show_as', { dir: '/d', name: 'n', media: 'copy' }],
      [() => api.renameShow('n'), 'rename_show', { name: 'n' }],
      [() => api.setShowMode(true), 'set_show_mode', { enabled: true }],
      [() => api.setSettings({} as never), 'set_settings', { settings: {} }],
      [() => api.setAudioDir(null), 'set_audio_dir', { dir: null }],
      [() => api.setMediaDir('/m'), 'set_media_dir', { dir: '/m' }],
      [() => api.setLayout({ a: 1 }), 'set_layout', { layout: { a: 1 } }],
      [() => api.setSelectionMode('linked'), 'set_selection_mode', { mode: 'linked' }],
      [() => api.rescanMedia(), 'rescan_media', {}],
      [() => api.openMediaDir(), 'open_media_dir', {}],
      [() => api.closeAck(), 'close_ack', {}],
      [() => api.fieldsCommitted(), 'fields_committed', {}],
      [() => api.checkUpdate(), 'check_update', {}],
      [() => api.installUpdate(), 'install_update', {}],
      [() => api.confirmClose(false), 'confirm_close', { save: false }],
      [() => api.setPanelWindows([]), 'set_panel_windows', { windows: [] }],
      [() => api.windowSupport(), 'window_support', {}],
      [() => api.appFocused(), 'app_focused', {}],
      [() => api.focusMain(), 'focus_main', {}],
      [() => api.audioDevices(), 'audio_devices', { host: null }],
      [() => api.audioDevices('JACK'), 'audio_devices', { host: 'JACK' }],
      [() => api.startupLibrary(), 'startup_library', {}],
      [() => api.libraries(), 'libraries', {}],
      [() => api.library('l'), 'library', { id: 'l' }],
      [() => api.libraryPresets(['l']), 'library_presets', { ids: ['l'] }],
      [() => api.inspectLibrary('/x.jqlibrary'), 'inspect_library', { path: '/x.jqlibrary' }],
      [() => api.installLibrary('/x.jqlibrary'), 'install_library', { path: '/x.jqlibrary' }],
      [() => api.removeLibrary('l'), 'remove_library', { id: 'l' }],
      [() => api.removeLibraryFile('x.jqlibrary'), 'remove_library_file', { file: 'x.jqlibrary' }],
      [() => api.rescanLibraries(), 'rescan_libraries', {}],
      [() => api.setLibraryEnabled('l', false), 'set_library_enabled', { id: 'l', enabled: false }],
      [
        () => api.setPresetFavorite('l', 'p', true),
        'set_preset_favorite',
        { library: 'l', preset: 'p', favorite: true },
      ],
      [() => api.openLibraryFolder(), 'open_library_folder', {}],
    ];
    for (const [call, cmd, args] of calls) {
      backend.calls = [];
      await call();
      expect(backend.invoked(cmd), cmd).toEqual([expect.objectContaining(args)]);
    }
    // Every command of the contract is listed here.
    expect(new Set(calls.map(([, cmd]) => cmd)).size).toBe(Object.keys(api).length);
  });
});

describe('edits and saves', () => {
  it('a save waits for the edits sent before it', async () => {
    const order: string[] = [];
    let answer!: () => void;
    backend.on('edit', () => new Promise<number>((resolve) => (answer = () => resolve(backend.show(null)))));
    backend.on('save_show', () => {
      order.push('save');
      return backend.show(null);
    });
    const edit = api.edit({ op: 'renumber' }).then(() => order.push('edit'));
    const save = api.saveShow();
    await settle();
    expect(order).toEqual([]);
    answer();
    await Promise.all([edit, save]);
    expect(order).toEqual(['edit', 'save']);
  });

  it('lists the revisions of the edits that succeeded', async () => {
    backend.on('edit', ({ op }) => {
      if ((op as { op: string }).op === 'undo') throw 'nothing to undo';
      return 41;
    });
    const ok = api.edit({ op: 'renumber' });
    const failed = api.edit({ op: 'undo' }).catch(() => undefined);
    expect(await editsAnswered()).toEqual([41]);
    await Promise.all([ok, failed]);
    expect(await editsAnswered()).toEqual([]);
  });
});

describe('events', () => {
  it('each event hands its payload to the handler', async () => {
    const cases: [string, (handler: (payload: unknown) => void) => Promise<() => void>, unknown, unknown][] = [
      ['language_changed', events.onLanguageChanged, { setting: 'de', resolved: 'de' }, undefined],
      ['show', events.onShow, { revision: 1, view: null }, undefined],
      ['state', events.onState, { playhead: null }, undefined],
      ['notice', events.onNotice, { level: 'warning', message: 'x' }, undefined],
      ['close_requested', events.onCloseRequested, { dirty: true }, undefined],
      ['update_available', events.onUpdateAvailable, { version: '1', notes: null }, undefined],
      ['update_progress', events.onUpdateProgress, { downloaded: 1, total: null }, undefined],
      ['open_requested', events.onOpenRequested, '/x.jqshow', undefined],
      ['panel_window_moved', events.onPanelWindowMoved, { panel: 'p', x: 0, y: 0, width: 1, height: 1 }, undefined],
      ['panel_window_closed', events.onPanelWindowClosed, { panel: 'p' }, 'p'],
      ['app_focus', events.onAppFocus, false, undefined],
    ];
    for (const [event, on, payload, expected] of cases) {
      const handler = vi.fn();
      const unlisten = await on(handler);
      await backend.emit(event, payload);
      expect(handler, event).toHaveBeenCalledWith(expected === undefined ? payload : expected);
      unlisten();
    }
    const committed = vi.fn();
    await events.onCommitFields(committed);
    await backend.emit('commit_fields', null);
    expect(committed).toHaveBeenCalledOnce();
  });
});
