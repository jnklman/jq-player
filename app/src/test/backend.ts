// A fake of the Tauri backend for tests: every `invoke` lands here (installed once per test file by
// `lib/test-setup.ts`), is recorded, and is answered by a handler of the test or a plain default. Events go through
// Tauri's own mock event bus, so `events.on*` listeners in the store get them as in the app.

import { emit as tauriEmit } from '@tauri-apps/api/event';
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks';
import { tick } from 'svelte';

import type { Action, EditOp, Revision, RuntimeState, ShowView } from '$lib/api';

type Args = Record<string, unknown>;
type Handler = (args: Args) => unknown;

export interface Call {
  cmd: string;
  args: Args;
}

const lastSegment = (path: string) =>
  path
    .replace(/[\\/]+$/, '')
    .split(/[\\/]/)
    .at(-1) ?? '';

/** Commands answered with a new revision of the current view (the backend's `show` event comes first). */
const CHANGES = new Set([
  'open_show',
  'new_show',
  'save_show',
  'save_show_as',
  'rename_show',
  'set_show_mode',
  'set_settings',
  'set_audio_dir',
  'set_media_dir',
  'set_layout',
  'set_selection_mode',
  'set_keep_awake',
  'midi_learn',
  'rescan_media',
  'recover_autosave',
]);

class Backend {
  calls: Call[] = [];
  view: ShowView | null = null;
  revision: Revision = 0;
  private handlers = new Map<string, Handler>();

  /** Forgets calls and handlers and closes the show (the revision keeps counting, as in the app). */
  reset() {
    this.calls = [];
    this.handlers.clear();
    this.view = null;
  }

  /** Answers `cmd` with `handler` (a thrown error or rejected promise makes the command fail). */
  on(cmd: string, handler: Handler) {
    this.handlers.set(cmd, handler);
    return this;
  }

  /** `cmd` fails with `message`. */
  fail(cmd: string, message = `${cmd} failed`) {
    return this.on(cmd, () => {
      throw message;
    });
  }

  /** Each `edit` op gets the view `next` returns (`undefined`: the view stays) as a new revision. */
  onEdit(next: (op: EditOp, view: ShowView | null) => ShowView | undefined | void) {
    return this.on('edit', ({ op }) => {
      const changed = next(op as EditOp, this.view);
      return this.show(changed ?? this.view);
    });
  }

  /** Sends a new view as the `show` event, as the backend does after a change; returns its revision. */
  show(view: ShowView | null): Revision {
    this.view = view;
    this.revision += 1;
    void tauriEmit('show', { revision: this.revision, view });
    return this.revision;
  }

  /** Sends the `state` event and waits for the UI to render it. */
  async state(state: RuntimeState | null) {
    await tauriEmit('state', state);
    await tick();
  }

  /** Sends any event (`notice`, `close_requested`, …). */
  async emit(event: string, payload?: unknown) {
    await tauriEmit(event, payload);
    await tick();
  }

  /** The args of every call of `cmd`, in order. */
  invoked(cmd: string): Args[] {
    return this.calls.filter((call) => call.cmd === cmd).map((call) => call.args);
  }

  /** The actions sent (`api.action`). */
  get actions(): Action[] {
    return this.invoked('action').map((args) => args.action as Action);
  }

  /** The edit ops sent (`api.edit`). */
  get edits(): EditOp[] {
    return this.invoked('edit').map((args) => args.op as EditOp);
  }

  async handle(cmd: string, args: Args): Promise<unknown> {
    this.calls.push({ cmd, args });
    const handler = this.handlers.get(cmd);
    if (handler) return handler(args);
    if (cmd === 'get_show') return { revision: this.revision, view: this.view };
    if (cmd === 'edit' || CHANGES.has(cmd)) return this.show(this.view);
    switch (cmd) {
      case 'get_language':
        return { setting: 'auto', resolved: 'en' };
      case 'plugin:path|basename': {
        const name = lastSegment(args.path as string);
        const ext = args.ext as string | undefined;
        return ext && name.endsWith(`.${ext}`) ? name.slice(0, -ext.length - 1) : name;
      }
      case 'plugin:path|dirname':
        return (args.path as string).replace(/[\\/]+$/, '').replace(/[\\/][^\\/]*$/, '') || '/';
      case 'plugin:path|join':
        return (args.paths as string[]).join('/').replace(/\/+/g, '/');
      case 'app_focused':
        return true;
      case 'window_support':
        return { overlay: true, native_frame: false };
      case 'key_labels':
        return {};
      case 'midi_ports':
        return { inputs: [], outputs: [] };
      case 'unused_media':
        return [];
      case 'check_import':
        return { outside: [], clashes: [], identical: [] };
      case 'check_level_limits':
        return { capped: [], silenced: [] };
      case 'peak_cache_size':
      case 'clear_peak_cache':
        return 0;
      case 'audio_devices':
        return { hosts: ['WASAPI'], host: 'WASAPI', devices: [] };
      case 'check_update':
        return null;
      default:
        // Fire-and-forget commands (action, stopwatch, live_cue, …), file dialogs (cancelled), window events.
        return null;
    }
  }
}

export const backend = new Backend();

/** Installs the fake for this test file; called once by the setup file, before any app module is imported. */
export function installBackend() {
  mockWindows('main');
  mockIPC((cmd, args) => backend.handle(cmd, (args ?? {}) as Args), { shouldMockEvents: true });
  // The mock keeps event calls to itself; they are recorded too, so a test sees what a window sent (`emit`, and
  // `emit_to` for another window, which goes nowhere here).
  const internals = (window as unknown as { __TAURI_INTERNALS__: { invoke: (...args: unknown[]) => unknown } })
    .__TAURI_INTERNALS__;
  const invoke = internals.invoke;
  internals.invoke = (cmd, args, options) => {
    if (cmd === 'plugin:event|emit' || cmd === 'plugin:event|emit_to') {
      backend.calls.push({ cmd: cmd as string, args: args as Args });
    }
    return invoke(cmd, args, options);
  };
}
