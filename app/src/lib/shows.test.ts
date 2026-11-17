import { describe, expect, it } from 'vitest';

import { backend } from '../test/backend';
import { active, runtime, sound, view } from '../test/fixtures';
import { loadShow, settle } from '../test/store';
import { createShow, newShow, openShow, openShowFile, saveShowAs } from './shows';
import { ui } from './store.svelte';

const other = () => view([sound('x')], { path: '/shows/Other/Other.jqshow', name: 'Other' });

/** The backend opens `other()` for `open_show` and `new_show`. */
function opensOther() {
  backend.on('open_show', () => backend.show(other()));
  backend.on('new_show', () => backend.show(other()));
}

function dialogOptions(cmd: 'open' | 'save') {
  return backend.invoked(`plugin:dialog|${cmd}`).map((args) => args.options as Record<string, unknown>);
}

describe('openShow', () => {
  it('starts the picker next to the show folder and opens the pick', async () => {
    await loadShow(view());
    opensOther();
    backend.on('plugin:dialog|open', () => '/shows/Other/Other.jqshow');
    await openShow();
    expect(dialogOptions('open')[0].defaultPath).toBe('/shows');
    expect(backend.invoked('open_show')).toEqual([{ path: '/shows/Other/Other.jqshow' }]);
    expect(ui.view?.name).toBe('Other');
    expect(ui.openingShow).toBe(false);
  });

  it('starts in the folder of a show file not in a folder of its name', async () => {
    await loadShow(view([], { path: '/shows/loose.jqshow' }));
    await openShow();
    expect(dialogOptions('open')[0].defaultPath).toBe('/shows');
  });

  it('does nothing when cancelled', async () => {
    await loadShow(view());
    await openShow();
    expect(backend.invoked('open_show')).toEqual([]);
  });

  it('runs once at a time', async () => {
    await loadShow(view());
    let finish!: (path: string | null) => void;
    backend.on('plugin:dialog|open', () => new Promise((resolve) => (finish = resolve)));
    const first = openShow();
    await settle();
    await openShow();
    expect(dialogOptions('open')).toHaveLength(1);
    finish(null);
    await first;
  });

  it('asks before replacing a show with unsaved changes, and saves when asked to', async () => {
    await loadShow(view([], { dirty: true }));
    opensOther();
    await openShowFile('/shows/Other/Other.jqshow');
    expect(backend.invoked('open_show')).toEqual([]);
    expect(ui.leaving).toMatchObject({ kind: 'switch', check: { dirty: true, running: 0 } });
    await ui.leaving!.proceed(true);
    expect(backend.invoked('save_show')).toHaveLength(1);
    expect(backend.invoked('open_show')).toHaveLength(1);
    expect(ui.openingShow).toBe(false);
  });

  it('counts running cues and copies for the question', async () => {
    await loadShow(
      view([], { settings: { show_mode: true }, show: { show_mode_locked: true } }),
      runtime({ active: [active('a')], imports: [{ file: 'x', progress: 0 }] }),
    );
    await openShowFile('/x.jqshow');
    expect(ui.leaving?.check).toEqual({
      show_mode: true,
      show_mode_locked: true,
      dirty: false,
      running: 1,
      imports: 1,
    });
  });

  it('opens nothing from outside while the program is closing', async () => {
    await loadShow(view());
    ui.leaving = { kind: 'quit', check: {} as never, proceed: async () => {} };
    await openShowFile('/x.jqshow');
    expect(backend.invoked('open_show')).toEqual([]);
  });

  it('opens directly without a show and reports a failure', async () => {
    await loadShow(null);
    backend.fail('open_show', 'not a show');
    await openShowFile('/x.jqshow');
    expect(ui.notices.at(-1)?.message).toBe('not a show');
  });
});

describe('newShow', () => {
  it('creates the show in the picked folder', async () => {
    await loadShow(null);
    opensOther();
    backend.on('plugin:dialog|open', () => '/shows');
    await newShow('Gala', false);
    expect(dialogOptions('open')[0]).toMatchObject({ directory: true, defaultPath: undefined });
    expect(backend.invoked('new_show')).toEqual([{ dir: '/shows', name: 'Gala', createFolder: false }]);
    expect(ui.view?.name).toBe('Other');
  });

  it('does nothing when no folder is picked', async () => {
    await loadShow(null);
    await newShow('Gala');
    expect(backend.invoked('new_show')).toEqual([]);
  });
});

describe('createShow', () => {
  it('asks where to put the files and creates the show there', async () => {
    await loadShow(view());
    opensOther();
    backend.on('plugin:dialog|save', () => '/shows/Gala.jqshow');
    const done = createShow();
    await settle();
    expect(dialogOptions('save')[0].defaultPath).toBe('/shows/New show');
    ui.question!.answer('folder');
    await done;
    expect(backend.invoked('new_show')).toEqual([{ dir: '/shows', name: 'Gala', createFolder: true }]);
  });

  it('suggests the plain name without a show, and stops when cancelled', async () => {
    await loadShow(null);
    backend.on('plugin:dialog|save', () => '/shows/Gala');
    const done = createShow();
    await settle();
    expect(dialogOptions('save')[0].defaultPath).toBe('New show');
    ui.question!.answer('cancel');
    await done;
    expect(backend.invoked('new_show')).toEqual([]);
  });

  it('stops when the save dialog is cancelled', async () => {
    await loadShow(null);
    await createShow();
    expect(ui.question).toBeNull();
  });
});

describe('saveShowAs', () => {
  it('suggests a copy next to the show and saves with the chosen media', async () => {
    await loadShow(view());
    backend.on('plugin:dialog|save', () => '/backup/Test Show copy.jqshow');
    const done = saveShowAs();
    await settle();
    expect(dialogOptions('save')[0].defaultPath).toBe('/shows/Test Show copy');
    ui.question!.answer('link');
    await settle();
    expect(ui.savingAs).toBe(false);
    await done;
    expect(backend.invoked('save_show_as')).toEqual([{ dir: '/backup', name: 'Test Show copy', media: 'link' }]);
  });

  it('does nothing without a show, when cancelled, or when the question is cancelled', async () => {
    await loadShow(null);
    await saveShowAs();
    await loadShow(view());
    await saveShowAs();
    backend.on('plugin:dialog|save', () => '/backup/x');
    const done = saveShowAs();
    await settle();
    ui.question!.answer('cancel');
    await done;
    expect(backend.invoked('save_show_as')).toEqual([]);
  });

  it('is shown as under way while it runs', async () => {
    await loadShow(view());
    backend.on('plugin:dialog|save', () => '/backup/x');
    let finish!: () => void;
    backend.on('save_show_as', () => new Promise<number>((resolve) => (finish = () => resolve(backend.show(view())))));
    const done = saveShowAs();
    await settle();
    ui.question!.answer('copy');
    await settle();
    expect(ui.savingAs).toBe(true);
    finish();
    await done;
    expect(ui.savingAs).toBe(false);
  });
});
