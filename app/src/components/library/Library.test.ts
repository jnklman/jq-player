// The cue library dialogs: picking and inserting a preset, and installing a library file.

import { screen, within } from '@testing-library/svelte';
import { afterEach, beforeEach, describe, expect, it, type MockInstance, vi } from 'vitest';

import x32Text from '../../../src-tauri/libraries/x32.jqlibrary?raw';
import type { EditOp, LibrariesView, Library } from '../../lib/api';
import { libraries, setView } from '../../lib/library.svelte';
import { backend } from '../../test/backend';
import { runtime, view, wait } from '../../test/fixtures';
import { renderUi } from '../../test/render';
import { loadShow, settle } from '../../test/store';
import LibraryDialog from './LibraryDialog.svelte';
import LibraryImportDialog from './LibraryImportDialog.svelte';

const x32 = JSON.parse(x32Text) as Library;

const librariesView = (extra: Partial<LibrariesView['prefs']> = {}): LibrariesView => ({
  libraries: [
    {
      id: x32.id,
      source: 'bundled',
      file: 'x32.jqlibrary',
      error: null,
      name: x32.name,
      description: x32.description,
      author: x32.author,
      homepage: '',
      revision: 1,
      presets: x32.presets.length,
      languages: ['de', 'en'],
      enabled: true,
      active: true,
    },
  ],
  prefs: { disabled: [], recent: [], favorites: [], ...extra },
  folder: '/config/libraries',
});

const lastEdit = <K extends EditOp['op']>(op: K) =>
  backend.edits.filter((edit): edit is Extract<EditOp, { op: K }> => edit.op === op).at(-1);

let errors: MockInstance;
beforeEach(() => {
  errors = vi.spyOn(console, 'error').mockImplementation(() => {});
  backend.on('library_presets', () => ({ [x32.id]: x32 }));
  libraries.loaded = {};
  setView(librariesView());
});
afterEach(() => {
  expect(errors).not.toHaveBeenCalled();
  vi.restoreAllMocks();
});

async function openDialog() {
  await loadShow(view([wait('w')]), runtime());
  const onclose = vi.fn();
  const result = await renderUi(LibraryDialog, { place: { index: 1 }, onclose });
  await settle(5);
  return { ...result, dialog: screen.getByRole('dialog'), onclose };
}

describe('LibraryDialog', () => {
  it('finds a preset, takes its parameters and inserts it at the place', async () => {
    const { user, dialog, onclose } = await openDialog();
    const list = within(dialog).getByRole('listbox');
    expect(within(list).getAllByRole('option')).toHaveLength(x32.presets.length);
    await user.type(within(dialog).getByRole('searchbox'), 'dca');
    await settle();
    expect(within(list).getAllByRole('option').length).toBeLessThan(x32.presets.length);
    await user.click(within(list).getByText('DCA 1: Mute'));
    await settle();
    // The picked preset's title follows the parameters.
    const dca = within(dialog).getByRole('spinbutton', { name: 'DCA' });
    await user.clear(dca);
    await user.type(dca, '4');
    await user.tab();
    await settle();
    expect(within(dialog).getByRole('heading', { name: 'DCA 4: Mute' })).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Insert' }));
    await settle();
    expect(lastEdit('add_preset')).toEqual({
      op: 'add_preset',
      library: x32.id,
      preset: 'dca-mute',
      params: { n: 4, state: 0 },
      index: 1,
    });
    expect(onclose).toHaveBeenCalled();
  });

  it('cannot insert while a required parameter is empty', async () => {
    const { user, dialog } = await openDialog();
    await user.type(within(dialog).getByRole('searchbox'), 'scribble');
    await settle();
    expect(within(dialog).getByRole('button', { name: 'Insert' })).toBeDisabled();
    expect(within(dialog).getByText('Required')).toBeInTheDocument();
    await user.type(within(dialog).getByRole('textbox', { name: 'Name' }), 'Vox');
    await settle();
    expect(within(dialog).getByRole('button', { name: 'Insert' })).toBeEnabled();
  });

  it('shows recently used presets first and marks favorites', async () => {
    setView(librariesView({ recent: [{ library: x32.id, preset: 'scene' }] }));
    backend.on('set_preset_favorite', ({ favorite }) =>
      librariesView({ favorites: favorite ? [{ library: x32.id, preset: 'scene' }] : [] }),
    );
    const { user, dialog } = await openDialog();
    const options = within(within(dialog).getByRole('listbox')).getAllByRole('option');
    expect(options).toHaveLength(1);
    expect(options[0]).toHaveTextContent('Recall scene 00');
    await user.click(within(options[0]).getByRole('button', { name: 'Add to favorites' }));
    await settle();
    expect(backend.invoked('set_preset_favorite')).toEqual([{ library: x32.id, preset: 'scene', favorite: true }]);
    await user.click(within(dialog).getByRole('button', { name: /Favorites/ }));
    await settle();
    expect(within(within(dialog).getByRole('listbox')).getAllByRole('option')).toHaveLength(1);
  });

  it('says so when no library is switched on', async () => {
    setView({ ...librariesView(), prefs: { disabled: [x32.id], recent: [], favorites: [] } });
    setView({
      ...librariesView(),
      libraries: librariesView().libraries.map((l) => ({ ...l, enabled: false })),
    });
    const { dialog } = await openDialog();
    expect(within(dialog).getByText(/No cue library is switched on/)).toBeInTheDocument();
  });
});

describe('LibraryImportDialog', () => {
  it('shows what a library file holds and installs it', async () => {
    backend.on('inspect_library', () => ({ library: x32, comparison: 'newer', installed: 0 }));
    backend.on('install_library', () => librariesView());
    const onclose = vi.fn();
    const { user } = await renderUi(LibraryImportDialog, { path: '/downloads/x32.jqlibrary', onclose });
    await settle(5);
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByText('Behringer X32 / Midas M32')).toBeInTheDocument();
    expect(within(dialog).getByText(/Newer than the installed revision 0/)).toBeInTheDocument();
    expect(within(dialog).getByText('Ch 1: Mute')).toBeInTheDocument();
    await user.click(within(dialog).getByRole('button', { name: 'Replace' }));
    await settle();
    expect(backend.invoked('install_library')).toEqual([{ path: '/downloads/x32.jqlibrary' }]);
    expect(onclose).toHaveBeenCalled();
  });

  it('explains why a file cannot be installed', async () => {
    backend.on('inspect_library', () => {
      throw { kind: 'library_file', path: '/d/bad.jqlibrary', error: { kind: 'wrong_format', found: 'jqshow' } };
    });
    await renderUi(LibraryImportDialog, { path: '/d/bad.jqlibrary', onclose: vi.fn() });
    await settle(5);
    const dialog = screen.getByRole('dialog');
    expect(within(dialog).getByRole('alert')).toHaveTextContent('bad.jqlibrary cannot be installed');
    expect(within(dialog).getByRole('alert')).toHaveTextContent('format “jqshow”');
    expect(within(dialog).getByRole('button', { name: 'Install' })).toBeDisabled();
  });
});
