import { describe, expect, it } from 'vitest';

import { backend } from '../test/backend';
import { group, playlist, sound, view } from '../test/fixtures';
import { settle } from '../test/store';
import type { ImportClash } from './api';
import {
  chooseMediaFolder,
  cleanMediaFolder,
  isAudioFile,
  isInsideDir,
  missingMediaFiles,
  pickAudio,
  relinkMissing,
  rescanMedia,
} from './media';
import { ui } from './store.svelte';

describe('isInsideDir', () => {
  it('matches the folder and what is below it, not siblings', () => {
    expect(isInsideDir('/shows/a/media', '/shows/a/media')).toBe(true);
    expect(isInsideDir('/shows/a/media/sub', '/shows/a/media/')).toBe(true);
    expect(isInsideDir('/shows/a/media2', '/shows/a/media')).toBe(false);
  });

  it('ignores case and separators of Windows paths', () => {
    expect(isInsideDir('c:\\Shows\\A\\MEDIA\\x', 'C:\\shows\\a\\media')).toBe(true);
    expect(isInsideDir('C:/shows/a/media2', 'C:\\shows\\a\\media')).toBe(false);
  });
});

describe('isAudioFile', () => {
  it('goes by the extensions the picker offers, in any case', () => {
    expect(isAudioFile('C:\\Music\\Rain.WAV')).toBe(true);
    expect(isAudioFile('/music/a.b/song.opus')).toBe(true);
    expect(isAudioFile('/music/notes.txt')).toBe(false);
    expect(isAudioFile('/music/wav')).toBe(false);
    expect(isAudioFile('/music/.wav/readme')).toBe(false);
  });
});

describe('missingMediaFiles', () => {
  it('lists each missing path once with the cues using it, also in groups and playlists', () => {
    const v = view([
      sound('a', { number: '1', file: 'rain.wav' }),
      sound('b', { title: 'Rain again', file: 'rain.wav' }),
      group('g', [sound('c', { number: '3', file: 'thunder.wav' })]),
      playlist('p', ['x.wav', 'y.wav'], { number: '4' }),
      sound('ok', { file: 'ok.wav' }),
    ]);
    for (const id of ['a', 'b', 'c']) v.cue_info[id].missing = true;
    v.cue_info.p.missing_items = [false, true];
    expect(missingMediaFiles(v)).toEqual([
      {
        key: 'rain.wav',
        path: 'rain.wav',
        uses: [
          { key: 'a', label: '1' },
          { key: 'b', label: '“Rain again”' },
        ],
      },
      { key: 'thunder.wav', path: 'thunder.wav', uses: [{ key: 'c', label: '3' }] },
      { key: 'y.wav', path: 'y.wav', uses: [{ key: 'p', label: '4' }] },
    ]);
  });

  it('lists a cue once for a file it uses twice', () => {
    const v = view([playlist('p', ['x.wav', 'x.wav'])]);
    v.cue_info.p.missing_items = [true, true];
    expect(missingMediaFiles(v)[0].uses).toHaveLength(1);
  });
});

describe('media folder', () => {
  it('stores another media folder once picked', async () => {
    const v = view();
    expect(await chooseMediaFolder(v)).toBe(false);
    backend.on('plugin:dialog|open', () => '/elsewhere');
    expect(await chooseMediaFolder(v)).toBe(true);
    expect(backend.invoked('set_media_dir')).toEqual([{ dir: '/elsewhere' }]);
    backend.fail('set_media_dir');
    expect(await chooseMediaFolder(v)).toBe(false);
  });

  it('rescans and opens the relink list', async () => {
    rescanMedia();
    relinkMissing();
    await settle();
    expect(backend.invoked('rescan_media')).toHaveLength(1);
    expect(ui.relinkOpen).toBe(true);
  });
});

describe('pickAudio', () => {
  const MEDIA = '/shows/Test Show/media';
  const pick = (...paths: string[]) => backend.on('plugin:dialog|open', () => paths);
  const check = (outside: string[], clashes: ImportClash[] = [], identical: string[] = []) =>
    backend.on('check_import', () => ({ outside, clashes, identical }));

  /** Starts a pick and answers the copy question with `answer`. */
  async function pickAnswering(answer: string) {
    const result = pickAudio(view());
    await settle();
    expect(ui.question?.title).toBe('Copy files?');
    ui.question!.answer(answer);
    return result;
  }

  it('returns null when cancelled', async () => {
    expect(await pickAudio(view())).toBeNull();
    pick();
    expect(await pickAudio(view())).toBeNull();
  });

  it('starts in the folder last picked from and remembers a folder outside the media folder', async () => {
    const v = view([], { settings: { audio_dir: '/music' } });
    pick('/music/a.wav');
    check([]);
    expect(await pickAudio(v)).toEqual({ paths: ['/music/a.wav'], copy_outside: false });
    const [{ options }] = backend.invoked('plugin:dialog|open') as { options: { defaultPath: string } }[];
    expect(options.defaultPath).toBe('/music');
    expect(backend.invoked('set_audio_dir')).toEqual([{ dir: '/music' }]);
  });

  it('takes a single pick as a list', async () => {
    backend.on('plugin:dialog|open', () => `${MEDIA}/a.wav`);
    expect(await pickAudio(view(), { multiple: false })).toEqual({ paths: [`${MEDIA}/a.wav`], copy_outside: false });
  });

  it('adds nothing when the check fails', async () => {
    pick('/x/a.wav');
    backend.fail('check_import');
    expect(await pickAudio(view())).toBeNull();
  });

  it('asks to copy files from outside, naming how many', async () => {
    pick('/x/a.wav', `${MEDIA}/b.wav`);
    check(['/x/a.wav'], [], ['/x/a.wav']);
    const result = pickAudio(view());
    await settle();
    expect(ui.question?.text).toContain('One of the 2 files is outside the show folder.');
    ui.question!.answer('copy');
    expect(await result).toEqual({
      paths: ['/x/a.wav', `${MEDIA}/b.wav`],
      copy_outside: true,
      on_clash: { '/x/a.wav': 'use_existing' },
    });
  });

  it('links or cancels as answered', async () => {
    pick('/x/a.wav');
    check(['/x/a.wav']);
    expect(await pickAnswering('link')).toEqual({ paths: ['/x/a.wav'], copy_outside: false });
    expect(await pickAnswering('cancel')).toBeNull();
  });

  it('asks per clashing file and leaves out skipped ones', async () => {
    pick('/x/a.wav', '/x/b.wav');
    const clashes = [{ path: '/x/a.wav', existing: 'a.wav', picked: false }];
    check(['/x/a.wav', '/x/b.wav'], clashes);
    const result = pickAudio(view());
    await settle();
    expect(ui.question?.text).toContain('All 2 files are outside');
    ui.question!.answer('copy');
    await settle();
    expect(ui.clashQuestion?.clashes).toEqual(clashes);
    ui.clashQuestion!.answer({ '/x/a.wav': 'skip' });
    expect(await result).toEqual({ paths: ['/x/b.wav'], copy_outside: true, on_clash: { '/x/a.wav': 'skip' } });
  });

  it('adds nothing when every file is skipped or the clashes are cancelled', async () => {
    pick('/x/a.wav');
    check(['/x/a.wav'], [{ path: '/x/a.wav', existing: 'a.wav', picked: false }]);
    let result = pickAnswering('copy');
    await settle(6);
    ui.clashQuestion!.answer({ '/x/a.wav': 'skip' });
    expect(await result).toBeNull();

    result = pickAnswering('copy');
    await settle(6);
    ui.clashQuestion!.answer(null);
    expect(await result).toBeNull();
  });

  it('says so for a single file', async () => {
    pick('/x/a.wav');
    check(['/x/a.wav']);
    const result = pickAudio(view());
    await settle();
    expect(ui.question?.text).toContain('The file is outside the show folder.');
    ui.question!.answer('cancel');
    await result;
  });
});

describe('cleanMediaFolder', () => {
  it('says when every file is used', async () => {
    const done = cleanMediaFolder();
    await settle();
    expect(ui.question?.text).toBe('All files in the media folder are used by cues.');
    ui.question!.answer('ok');
    await done;
    expect(backend.invoked('clean_media')).toEqual([]);
  });

  it('lists up to eight files, deletes them once confirmed and reports failures', async () => {
    const files = Array.from({ length: 10 }, (_, i) => ({ file: `f${i}.wav`, bytes: 1 << 20 }));
    backend.on('unused_media', () => files);
    backend.on('clean_media', () => ({ deleted: 9, failed: ['f9.wav: in use'] }));
    const done = cleanMediaFolder();
    await settle();
    expect(ui.question?.text).toContain('10 files are used by no cue and will be deleted for good (10 MB):');
    expect(ui.question?.text).toContain('• f7.wav');
    expect(ui.question?.text).not.toContain('• f8.wav');
    expect(ui.question?.text).toContain('… and 2 more');
    ui.question!.answer('delete');
    await settle();
    expect(backend.invoked('clean_media')).toEqual([{ files: files.map((f) => f.file) }]);
    expect(ui.question?.text).toBe('9 files deleted.\n\nNot deleted:\nf9.wav: in use');
    ui.question!.answer('ok');
    await done;
  });

  it('deletes nothing when cancelled or when listing fails', async () => {
    backend.on('unused_media', () => [{ file: 'a.wav', bytes: 10 }]);
    const done = cleanMediaFolder();
    await settle();
    expect(ui.question?.text).toContain('1 file is used by no cue');
    ui.question!.answer('cancel');
    await done;
    backend.fail('unused_media');
    await cleanMediaFolder();
    expect(backend.invoked('clean_media')).toEqual([]);
  });

  it('stops when deleting fails', async () => {
    backend.on('unused_media', () => [{ file: 'a.wav', bytes: 10 }]);
    backend.fail('clean_media');
    const done = cleanMediaFolder();
    await settle();
    ui.question!.answer('delete');
    await done;
    expect(ui.question).toBeNull();
    expect(ui.notices).toHaveLength(1);
  });
});
