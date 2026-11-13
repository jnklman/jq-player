// Opening and creating shows, shared by the start screen and the show menu.

import { basename, dirname, join } from '@tauri-apps/api/path';
import { open, save } from '@tauri-apps/plugin-dialog';

import { api, type CloseCheck, type SaveAsMedia, SHOW_EXTENSION } from './api';
import { t } from './i18n/index.svelte';
import { ask, attempt, mode, ui } from './store.svelte';

/** A useful start directory for show pickers. */
async function showsDir(): Promise<string | undefined> {
  if (!ui.view) return undefined;
  const folder = await dirname(ui.view.path);
  const file = await basename(ui.view.path);
  return file === `${await basename(folder)}.${SHOW_EXTENSION}` ? dirname(folder) : folder;
}

async function pickShowFile(): Promise<string | null> {
  return open({
    defaultPath: await showsDir(),
    filters: [{ name: t('shows.file_filter'), extensions: [SHOW_EXTENSION] }],
  });
}

async function showName(path: string): Promise<string> {
  return (await basename(path)).replace(new RegExp(`\\.${SHOW_EXTENSION}$`, 'i'), '');
}

async function pickShowDir(createFolder: boolean): Promise<string | null> {
  return open({
    directory: true,
    defaultPath: await showsDir(),
    title: createFolder ? t('shows.pick_dir_folder') : t('shows.pick_dir_files'),
  });
}

/**
 * Runs `task` unless another one is under way, so a slow load (e.g. files
 * coming from a cloud folder) cannot be started twice.
 */
async function exclusive(task: () => Promise<unknown>) {
  if (ui.openingShow) return;
  ui.openingShow = true;
  try {
    await task();
  } finally {
    ui.openingShow = false;
  }
}

/** Picks a show file and opens it in place of the current show. */
export function openShow(): Promise<void> {
  return exclusive(async () => {
    const path = await pickShowFile();
    if (path) await replaceShow(() => api.openShow(path));
  });
}

/**
 * Opens a show file chosen outside the program (file manager, second start)
 * in place of the current show. Left alone while another show is being
 * opened or the program is about to close.
 */
export function openShowFile(path: string): Promise<void> {
  if (ui.leaving) return Promise.resolve();
  return exclusive(() => replaceShow(() => api.openShow(path)));
}

/** Picks a folder and creates the show `name` in it or directly there. */
export function newShow(name: string, createFolder = true): Promise<void> {
  return exclusive(async () => {
    const dir = await pickShowDir(createFolder);
    if (dir) await replaceShow(() => api.newShow(dir, name, createFolder));
  });
}

/**
 * Asks for the new show folder's place and name in one save dialog and
 * creates the show there.
 */
export function createShow(): Promise<void> {
  return exclusive(async () => {
    const dir = await showsDir();
    const path = await save({
      title: t('shows.new_title'),
      defaultPath: dir ? await join(dir, t('shows.new_name')) : t('shows.new_name'),
    });
    if (!path) return;
    const placement = await ask<'folder' | 'here' | 'cancel'>(t('shows.new_title'), t('shows.new_where'), [
      { value: 'cancel', label: t('common.cancel') },
      { value: 'here', label: t('shows.new_here') },
      { value: 'folder', label: t('shows.new_folder'), focus: true },
    ]);
    if (placement !== 'cancel') {
      await replaceShow(async () => api.newShow(await dirname(path), await showName(path), placement === 'folder'));
    }
  });
}

/**
 * Saves the open show in a new show folder, asked for like a new show, and
 * asks whether its media are copied along or the current media folder is
 * linked.
 */
export function saveShowAs(): Promise<void> {
  return exclusive(async () => {
    const view = ui.view;
    if (!view) return;
    const dir = await showsDir();
    const suggested = t('shows.copy_name', { name: view.name });
    const path = await save({
      title: t('shows.save_as_title'),
      defaultPath: dir ? await join(dir, suggested) : suggested,
    });
    if (!path) return;
    const media = await ask<SaveAsMedia | 'cancel'>(t('shows.media_title'), t('shows.media_text'), [
      { value: 'cancel', label: t('common.cancel') },
      { value: 'link', label: t('shows.media_link') },
      { value: 'copy', label: t('shows.media_copy'), focus: true },
    ]);
    if (media === 'cancel') return;
    const name = await showName(path);
    ui.savingAs = true;
    try {
      await attempt(api.saveShowAs(await dirname(path), name, media));
    } finally {
      ui.savingAs = false;
    }
  });
}

/**
 * Replaces the open show with `load`. Asks first in show mode, while cues
 * run (they stop) or with unsaved changes, like closing the window.
 */
async function replaceShow(load: () => Promise<unknown>) {
  const check: CloseCheck = {
    show_mode: mode() === 'show',
    dirty: ui.view?.dirty ?? false,
    running: (ui.runtime?.active.length ?? 0) + (ui.runtime?.waiting.length ?? 0) + (ui.runtime?.gaps.length ?? 0),
    imports: ui.runtime?.imports.length ?? 0,
  };
  if (!ui.view || (!check.show_mode && !check.dirty && check.running === 0 && check.imports === 0)) {
    await attempt(load());
    return;
  }
  ui.leaving = {
    kind: 'switch',
    check,
    // Confirmed after `exclusive` has returned, so it locks again itself.
    proceed: async (save) => {
      ui.openingShow = true;
      try {
        if (save) await api.saveShow();
        await load();
      } finally {
        ui.openingShow = false;
      }
    },
  };
}
