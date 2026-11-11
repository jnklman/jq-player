// The media folder: picking audio files (cue list, group dialog, inspector),
// repairing missing files and removing unused ones.

import { dirname } from '@tauri-apps/api/path';
import { open } from '@tauri-apps/plugin-dialog';

import { api, type Cue, type Import, type OnClash, type ShowView } from './api';
import { cueLabel } from './cue';
import { fmtMegabytes } from './format';
import { t } from './i18n/index.svelte';
import { ask, askClashes, attempt, ui } from './store.svelte';

const AUDIO = ['wav', 'flac', 'mp3', 'ogg', 'm4a', 'aac', 'aif', 'aiff'];

export interface MissingMediaUse {
  key: string;
  label: string;
}

export interface MissingMediaFile {
  key: string;
  path: string;
  uses: MissingMediaUse[];
}

/** Lists each missing stored media path once, with the cues that use it. */
export function missingMediaFiles(view: ShowView): MissingMediaFile[] {
  const files = new Map<string, MissingMediaFile>();

  function add(path: string, cue: Cue) {
    let file = files.get(path);
    if (!file) {
      file = { key: path, path, uses: [] };
      files.set(path, file);
    }
    if (!file.uses.some((use) => use.key === cue.id)) {
      file.uses.push({
        key: cue.id,
        label: cueLabel(cue),
      });
    }
  }

  function visit(cues: Cue[]) {
    for (const cue of cues) {
      if (cue.type === 'group') {
        visit(cue.cues);
        continue;
      }
      const info = view.cue_info[cue.id];
      if (cue.type === 'sound' && info?.missing) {
        add(cue.file, cue);
      } else if (cue.type === 'playlist') {
        cue.items.forEach((item, i) => {
          if (info?.missing_items[i]) add(item.file, cue);
        });
      }
    }
  }

  visit(view.show.cues);
  return [...files.values()];
}

/** Picks another media folder and stores it in the show. */
export async function chooseMediaFolder(view: ShowView): Promise<boolean> {
  const dir = await open({ directory: true, defaultPath: view.media_dir, title: t('media.choose_folder') });
  if (!dir) return false;
  return (await attempt(api.setMediaDir(dir))) !== undefined;
}

/**
 * Lets the operator pick audio files. Files outside the media folder are
 * offered to be copied in, so the show stays portable; for names taken
 * there, each file gets its own choice. Starts in the folder last picked
 * from (machine setting); a pick from the media folder means "the media
 * folder", so it follows when another show is opened. `null`: cancelled or
 * every file skipped. With `multiple: false`, at most one file is picked.
 */
export async function pickAudio(
  view: ShowView,
  { multiple = true } = {},
): Promise<({ paths: string[] } & Import) | null> {
  const mediaDir = view.media_dir;
  const result = await open({
    multiple,
    defaultPath: view.settings.audio_dir ?? mediaDir,
    filters: [{ name: 'Audio', extensions: AUDIO }],
  });
  const picked = typeof result === 'string' ? [result] : result;
  if (!picked || picked.length === 0) return null;
  const dir = await dirname(picked[0]);
  // Only a convenience: a failure must not stop adding the files.
  api.setAudioDir(dir.startsWith(mediaDir) ? null : dir).catch(() => {});

  const check = await attempt(api.checkImport(mediaDir, picked));
  if (!check) return null;
  if (check.outside.length === 0) return { paths: picked, copy_outside: false };
  // The same file is already there (or picked twice): no second copy.
  const identical: Record<string, OnClash> = Object.fromEntries(check.identical.map((path) => [path, 'use_existing']));
  const answer = await ask(
    t('media.copy_title'),
    `${outsideText(check.outside.length, picked.length)} ${t('media.copy_question')}`,
    [
      { label: t('common.cancel'), value: 'cancel' },
      { label: t('media.dont_copy'), value: 'link' },
      { label: t('media.copy'), value: 'copy', focus: true },
    ],
  );
  if (answer === 'cancel') return null;
  if (answer === 'link') return { paths: picked, copy_outside: false };
  if (check.clashes.length === 0) return { paths: picked, copy_outside: true, on_clash: identical };

  const chosen = await askClashes(check.clashes);
  if (!chosen) return null;
  const on_clash = { ...identical, ...chosen };
  const paths = picked.filter((path) => on_clash[path] !== 'skip');
  return paths.length > 0 ? { paths, copy_outside: true, on_clash } : null;
}

function outsideText(outside: number, all: number): string {
  if (all === 1) return t('media.outside_one');
  if (outside === all) return t('media.outside_all', { count: all });
  return t('media.outside_some', { count: outside, all });
}

/** Looks at the media files again, for changes on disk the file watcher missed (e.g. on a network share). */
export function rescanMedia() {
  void attempt(api.rescanMedia());
}

/** Opens the list of missing files so each one can be relinked individually. */
export function relinkMissing() {
  ui.relinkOpen = true;
}

/** Lists the files no cue uses and deletes them once confirmed. */
export async function cleanMediaFolder() {
  const files = await attempt(api.unusedMedia());
  if (!files) return;
  const ok = [{ label: t('common.ok'), value: 'ok', focus: true }];
  if (files.length === 0) {
    await ask(t('media.clean_title'), t('media.clean_all_used'), ok);
    return;
  }
  const bytes = files.reduce((sum, file) => sum + file.bytes, 0);
  const shown = files.slice(0, 8).map((file) => `• ${file.file}`);
  if (files.length > shown.length) shown.push(t('media.clean_more', { count: files.length - shown.length }));
  const answer = await ask(
    t('media.clean_question'),
    `${t('media.clean_text', { count: files.length, size: fmtMegabytes(bytes) })}\n\n${shown.join('\n')}\n\n${t('media.clean_irreversible')}`,
    [
      { label: t('common.cancel'), value: 'cancel', focus: true },
      { label: t('common.delete'), value: 'delete', danger: true },
    ],
  );
  if (answer !== 'delete') return;
  const result = await attempt(api.cleanMedia(files.map((file) => file.file)));
  if (!result) return;
  const deleted = t('media.clean_deleted', { count: result.deleted });
  const failed = result.failed.length ? `\n\n${t('media.clean_failed')}\n${result.failed.join('\n')}` : '';
  await ask(t('media.clean_done'), deleted + failed, ok);
}
