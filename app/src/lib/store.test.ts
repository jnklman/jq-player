import { describe, expect, it, vi } from 'vitest';

import { backend } from '../test/backend';
import { active, group, playlist, runtime, scene, settings, sound, view, wait } from '../test/fixtures';
import { loadShow, settle } from '../test/store';
import { i18n } from './i18n/index.svelte';
import {
  activeFor,
  allCues,
  ask,
  askClashes,
  askUnsaved,
  attempt,
  attemptChange,
  besideNotices,
  cueById,
  dialogEscape,
  dismiss,
  escapeAnswers,
  extendSelection,
  fadingFor,
  gapFor,
  gotoCue,
  importFor,
  initLanguage,
  isRunning,
  isTopLevel,
  mode,
  moveSelection,
  noticeMessage,
  notify,
  previousOf,
  saveShow,
  select,
  selectAll,
  selection,
  selectionMode,
  siblingsOf,
  toggleSelected,
  ui,
  waitingFor,
  whenShown,
} from './store.svelte';

const list = () => [sound('a'), wait('b'), scene('s1'), sound('c'), wait('d'), scene('s2'), sound('e')];

describe('show events', () => {
  it('takes the view of a show event and the runtime state', async () => {
    await loadShow(view([sound('a')]), runtime({ playhead: 'a' }));
    expect(ui.view?.show.cues.map((cue) => cue.id)).toEqual(['a']);
    expect(ui.runtime?.playhead).toBe('a');
    expect(ui.started).toBe(true);
  });

  it('ignores a view older than the one shown', async () => {
    await loadShow(view([sound('new')]));
    const stale = backend.revision - 1;
    const { emit } = await import('@tauri-apps/api/event');
    await emit('show', { revision: stale, view: view([sound('old')]) });
    expect(ui.view?.show.cues[0].id).toBe('new');
  });

  it('forgets selected cues that are gone', async () => {
    await loadShow(view(list()));
    selectAll(['a', 'c', 'e']);
    ui.rangeEnd = 'e';
    backend.show(view([sound('c'), wait('b')]));
    await settle();
    expect(ui.selected).toBe('c');
    expect(ui.marked).toEqual([]);
    expect(ui.rangeEnd).toBeNull();
  });

  it('resolves `whenShown` once the revision is there', async () => {
    await loadShow(view([sound('a')]));
    const revision = backend.show(view([sound('b')]));
    const shown = await whenShown(revision);
    expect(shown?.show.cues[0].id).toBe('b');
  });

  it('`attemptChange` resolves with the changed view, or undefined on failure', async () => {
    await loadShow(view([sound('a')]));
    backend.onEdit(() => view([sound('a'), sound('b')]));
    const { api } = await import('./api');
    const changed = await attemptChange(api.edit({ op: 'renumber' }));
    expect(changed?.show.cues).toHaveLength(2);

    backend.fail('edit', 'refused');
    expect(await attemptChange(api.edit({ op: 'renumber' }))).toBeUndefined();
    expect(ui.notices.at(-1)).toMatchObject({ level: 'error', message: 'refused' });
  });
});

describe('main window events', () => {
  it('shows notices and the update offered at start', async () => {
    await loadShow(view());
    await backend.emit('notice', { level: 'error', kind: 'audio_unavailable', error: 'Device lost' });
    expect(ui.notices.map(noticeMessage)).toContain('Audio output not available: Device lost');

    await backend.emit('update_available', { version: '9.9.9', notes: null });
    expect(ui.update?.version).toBe('9.9.9');
    expect(ui.notices.at(-1)?.message).toContain('9.9.9');
  });

  it('asks before closing and acknowledges the request at once', async () => {
    await loadShow(view());
    const check = { show_mode: false, show_mode_locked: false, dirty: true, running: 0, imports: 0 };
    await backend.emit('close_requested', check);
    expect(ui.leaving).toMatchObject({ kind: 'quit', check });
    expect(backend.invoked('close_ack')).toHaveLength(1);
    await ui.leaving!.proceed(true);
    expect(backend.invoked('confirm_close')).toEqual([{ save: true }]);
  });
});

describe('notices', () => {
  it('keeps the latest five and lets warnings fade out', () => {
    vi.useFakeTimers();
    try {
      for (let i = 0; i < 7; i++) notify({ level: 'error', message: `e${i}` });
      expect(ui.notices.map((n) => n.message)).toEqual(['e2', 'e3', 'e4', 'e5', 'e6']);
      notify({ level: 'warning', message: 'w' });
      expect(ui.notices.at(-1)?.message).toBe('w');
      vi.advanceTimersByTime(8000);
      expect(ui.notices.some((n) => n.message === 'w')).toBe(false);
      dismiss(ui.notices[0].id);
      expect(ui.notices).toHaveLength(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it('shows a repeated notice once, the newer in place of the older', () => {
    vi.useFakeTimers();
    try {
      const running = { level: 'warning', kind: 'already_running', cue: { number: '1', title: '' } } as const;
      notify(running);
      notify({ level: 'error', message: 'refused' });
      vi.advanceTimersByTime(5000);
      notify(running);
      notify({ level: 'error', message: 'refused' });
      expect(ui.notices.map(noticeMessage)).toEqual(['Cue 1 is already running', 'refused']);
      vi.advanceTimersByTime(5000);
      expect(ui.notices).toHaveLength(2);
      notify({ ...running, cue: { number: '2', title: 'Rain' } });
      expect(ui.notices.map(noticeMessage)).toContain('Cue 2 “Rain” is already running');
      expect(ui.notices).toHaveLength(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it('lets a newer notice about the same device or file replace the older one', () => {
    notify({ level: 'error', kind: 'device_lost', device: 'main', name: 'USB', reason: 'unplugged' });
    notify({ level: 'error', kind: 'device_lost', device: 'preview', name: 'Phones', reason: 'gone' });
    notify({ level: 'warning', kind: 'device_back', device: 'main', name: 'USB' });
    notify({ level: 'warning', kind: 'underrun', file: 'a.wav', ms: 10 });
    notify({ level: 'warning', kind: 'underrun', file: 'a.wav', ms: 30 });
    expect(ui.notices.map((n) => [n.level, noticeMessage(n)])).toEqual([
      ['error', expect.stringMatching(/^Preview output “Phones” interrupted: gone\./)],
      ['warning', 'Audio output “USB” reconnected'],
      ['warning', 'a.wav: 30 ms of silence, the file did not come from the disk fast enough'],
    ]);
  });

  it('`attempt` turns a failure into an error notice', async () => {
    expect(await attempt(Promise.resolve(3))).toBe(3);
    expect(await attempt(Promise.reject('nope'))).toBeUndefined();
    expect(ui.notices.at(-1)).toMatchObject({ level: 'error', message: 'nope' });
  });
});

describe('questions', () => {
  it('resolves with the chosen value; a newer question answers the open one with its first choice', async () => {
    const first = ask('T', 'text', [
      { label: 'No', value: 'no' },
      { label: 'Yes', value: 'yes' },
    ]);
    const second = ask('T2', 'text', [
      { label: 'Cancel', value: 'cancel' },
      { label: 'OK', value: 'ok' },
    ]);
    expect(await first).toBe('no');
    expect(ui.question?.title).toBe('T2');
    ui.question!.answer('ok');
    expect(await second).toBe('ok');
    expect(ui.question).toBeNull();
  });

  it('offers to apply unsaved changes only when they can be applied', async () => {
    const withApply = askUnsaved();
    expect(ui.question?.choices.map((c) => c.value)).toEqual(['stay', 'discard', 'apply']);
    expect(ui.question?.escape).toBe('stay');
    ui.question!.answer('apply');
    expect(await withApply).toBe('apply');

    const without = askUnsaved(false);
    expect(ui.question?.choices.map((c) => c.value)).toEqual(['stay', 'discard']);
    ui.question!.answer('discard');
    expect(await without).toBe('discard');
  });

  it('cancels an open clash question when a new one comes', async () => {
    const clash = { path: '/a.wav', existing: 'a.wav', picked: false };
    const first = askClashes([clash]);
    const second = askClashes([clash]);
    expect(await first).toBeNull();
    ui.clashQuestion!.answer({ '/a.wav': 'skip' });
    expect(await second).toEqual({ '/a.wav': 'skip' });
  });
});

describe('Esc behaviour', () => {
  it('closes dialogs only with "close_first"', async () => {
    await loadShow(view([], { settings: { escape: 'stop_all' } }));
    expect(dialogEscape()).toBe('ignore');
    const answer = vi.fn();
    const props = escapeAnswers(answer);
    expect(props.escapeKeydownBehavior).toBe('ignore');
    expect(props['data-escape']).toBeUndefined();
    const e = new KeyboardEvent('keydown', { key: 'Escape', cancelable: true });
    props.onEscapeKeydown(e);
    expect(e.defaultPrevented).toBe(true);
    expect(answer).not.toHaveBeenCalled();

    await loadShow(view([], { settings: { escape: 'close_first' } }));
    expect(dialogEscape()).toBe('close');
    const closing = escapeAnswers(answer);
    expect(closing['data-escape']).toBe('');
    closing.onEscapeKeydown(new KeyboardEvent('keydown'));
    expect(answer).toHaveBeenCalledOnce();
    // A question without an answer for Esc ignores it.
    expect(escapeAnswers().escapeKeydownBehavior).toBe('ignore');
  });
});

describe('saving', () => {
  it('confirms a save briefly', async () => {
    await loadShow(view());
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
    try {
      await saveShow();
      expect(backend.invoked('save_show')).toHaveLength(1);
      expect(ui.justSaved).toBe(true);
      vi.advanceTimersByTime(2000);
      expect(ui.justSaved).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it('does not confirm a failed save', async () => {
    await loadShow(view());
    backend.fail('save_show');
    await saveShow();
    expect(ui.justSaved).toBe(false);
    expect(ui.notices.at(-1)?.level).toBe('error');
  });
});

describe('language', () => {
  it("takes the backend language and follows its changes, rewording the backend's notices", async () => {
    backend.on('get_language', () => ({ setting: 'de', resolved: 'de' }));
    await initLanguage();
    expect(i18n.locale).toBe('de');
    expect(i18n.setting).toBe('de');
    notify({ level: 'error', message: 'old' });
    notify({ level: 'error', kind: 'search_aborted' });
    expect(ui.notices.map(noticeMessage)).toEqual(['old', 'Suche nach fehlenden Dateien abgebrochen']);
    await backend.emit('language_changed', { setting: 'auto', resolved: 'en' });
    expect(i18n.locale).toBe('en');
    expect(ui.notices.map(noticeMessage)).toEqual(['Search for missing files aborted']);
  });
});

describe('selection', () => {
  it('reports mode and selection mode from the settings', async () => {
    await loadShow(view([], { settings: settings({ show_mode: true, selection_mode: 'linked' }) }));
    expect(mode()).toBe('show');
    expect(selectionMode()).toBe('linked');
    ui.view = null;
    expect(mode()).toBe('edit');
    expect(selectionMode()).toBe('separate');
  });

  it('moves the playhead along with a selection it follows', async () => {
    await loadShow(view(list(), { settings: { selection_mode: 'playhead_follows' } }), runtime({ playhead: 'a' }));
    select('a');
    expect(backend.actions).toEqual([]);
    select('c');
    expect(backend.actions).toEqual([{ action: 'set_playhead', cue: 'c' }]);
    expect(ui.selected).toBe('c');
  });

  it('toggles cues in and out of the selection', async () => {
    await loadShow(view(list()));
    select('a');
    toggleSelected('c');
    expect([ui.selected, ui.marked]).toEqual(['c', ['a']]);
    toggleSelected('a');
    expect([ui.selected, ui.marked]).toEqual(['c', []]);
    toggleSelected('c');
    expect([ui.selected, ui.marked]).toEqual([null, []]);
    toggleSelected('e');
    expect(ui.selected).toBe('e');
  });

  it('extends a range from the selected cue, in either direction', async () => {
    await loadShow(view(list()));
    select('c');
    extendSelection('a');
    expect(selection()).toEqual(['a', 'b', 's1', 'c']);
    expect(ui.rangeEnd).toBe('a');
    extendSelection('e');
    expect(selection()).toEqual(['c', 'd', 's2', 'e']);
    // Without a selection, it just selects.
    select(null);
    extendSelection('b');
    expect(selection()).toEqual(['b']);
  });

  it('moves by steps, to the ends and from scene to scene', async () => {
    await loadShow(view(list()));
    moveSelection(1);
    expect(ui.selected).toBe('a');
    moveSelection(-1);
    expect(ui.selected).toBe('a');
    moveSelection(100);
    expect(ui.selected).toBe('e');
    moveSelection('start');
    expect(ui.selected).toBe('a');
    moveSelection('end');
    expect(ui.selected).toBe('e');
    select(null);
    moveSelection(-1);
    expect(ui.selected).toBe('e');

    select('a');
    moveSelection('next_scene');
    expect(ui.selected).toBe('c');
    moveSelection('next_scene');
    expect(ui.selected).toBe('e');
    moveSelection('next_scene');
    expect(ui.selected).toBe('e');
    moveSelection('prev_scene');
    expect(ui.selected).toBe('c');
    moveSelection('prev_scene');
    expect(ui.selected).toBe('a');
    moveSelection('prev_scene');
    expect(ui.selected).toBe('a');
  });

  it('extends with Shift instead of moving', async () => {
    await loadShow(view(list()));
    select('a');
    moveSelection(1, true);
    moveSelection(1, true);
    expect(ui.selected).toBe('a');
    expect(selection()).toEqual(['a', 'b', 's1']);
  });

  it('moves the playhead past scene markers when the selection follows it', async () => {
    await loadShow(view(list(), { settings: { selection_mode: 'selection_follows' } }), runtime({ playhead: 'b' }));
    ui.selected = 'b';
    moveSelection(1);
    expect(backend.actions.at(-1)).toEqual({ action: 'set_playhead', cue: 'c' });
    ui.selected = 'c';
    moveSelection(-1);
    expect(backend.actions.at(-1)).toEqual({ action: 'set_playhead', cue: 'b' });
    // At the end of the list it turns back to the last playable cue.
    await loadShow(view([sound('x'), scene('s')], { settings: { selection_mode: 'linked' } }));
    ui.selected = 'x';
    moveSelection(1);
    expect(backend.actions.at(-1)).toEqual({ action: 'set_playhead', cue: 'x' });
    // An empty list does nothing.
    await loadShow(view([], { settings: { selection_mode: 'linked' } }));
    const sent = backend.actions.length;
    moveSelection(1);
    expect(backend.actions).toHaveLength(sent);
  });

  it('does nothing for scene moves in a list without playable cues', async () => {
    await loadShow(view([scene('only')]));
    moveSelection('next_scene');
    expect(ui.selected).toBeNull();
  });

  it('goes to a cue by its number', async () => {
    const cues = [sound('a', { number: '1' }), scene('s', { number: '2' }), sound('b', { number: '2' })];
    await loadShow(view(cues));
    expect(gotoCue('  ')).toBe(false);
    expect(gotoCue('9')).toBe(false);
    expect(gotoCue(' 1 ')).toBe(true);
    expect(ui.selected).toBe('a');
    // Following the playhead, a scene marker with the number is passed over.
    await loadShow(view(cues, { settings: { selection_mode: 'linked' } }));
    expect(gotoCue('2')).toBe(true);
    expect(backend.actions.at(-1)).toEqual({ action: 'set_playhead', cue: 'b' });
  });
});

describe('cue lookup', () => {
  const show = () => [sound('a'), group('g', [wait('g1'), sound('g2'), wait('g3')]), wait('w'), playlist('p', ['x'])];

  it('finds cues inside groups and their siblings', async () => {
    await loadShow(view(show()));
    expect(allCues().map((cue) => cue.id)).toEqual(['a', 'g', 'g1', 'g2', 'g3', 'w', 'p']);
    expect(cueById('g2')?.type).toBe('sound');
    expect(cueById(null)).toBeUndefined();
    expect(siblingsOf('g3').map((cue) => cue.id)).toEqual(['g1', 'g2', 'g3']);
    expect(siblingsOf('nope')).toEqual([]);
    expect(isTopLevel('g')).toBe(true);
    expect(isTopLevel('g1')).toBe(false);
  });

  it('finds the previous cue that control cues can act on', async () => {
    await loadShow(view(show()));
    expect(previousOf('w')?.id).toBe('g');
    expect(previousOf('g3')?.id).toBe('g2');
    expect(previousOf('g1')).toBeUndefined();
    expect(previousOf('a')).toBeUndefined();
  });

  it('reads the runtime state per cue', async () => {
    const state = runtime({
      active: [active('a')],
      fading: [{ cue: 'w', elapsed: 1, remaining: 1, progress: 0.5, paused: false }],
      gaps: [{ cue: 'p', remaining: 2, next: { index: 1, file: 1, count: 2 }, paused: false }],
      waiting: [{ cue: 'g1', remaining: 3, paused: false, root: 'g1' }],
      imports: [{ file: 'x.wav', progress: 0.4 }],
    });
    await loadShow(view(show()), state);
    expect(activeFor('a')?.elapsed).toBe(10);
    expect(fadingFor('w')?.progress).toBe(0.5);
    expect(gapFor('p')?.remaining).toBe(2);
    expect(waitingFor('g1')).toBe(3);
    expect(importFor('x.wav')).toBe(0.4);
    expect(isRunning(cueById('g')!)).toBe(true);
    expect(isRunning(cueById('g2')!)).toBe(false);
  });
});

describe('besideNotices', () => {
  it('keeps a dialog open for clicks into the notices only', () => {
    document.body.innerHTML = '<div data-notices><button>x</button></div><p>outside</p>';
    const own = vi.fn();
    const outside = besideNotices(own);
    const click = (target: Element) => {
      const event = new Event('pointerup', { cancelable: true }) as PointerEvent;
      Object.defineProperty(event, 'target', { value: target });
      outside(event);
      return event;
    };
    expect(click(document.querySelector('button')!).defaultPrevented).toBe(true);
    expect(own).not.toHaveBeenCalled();
    expect(click(document.querySelector('p')!).defaultPrevented).toBe(false);
    expect(own).toHaveBeenCalledOnce();
    document.body.innerHTML = '';
  });
});
