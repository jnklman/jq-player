import { afterEach, describe, expect, it } from 'vitest';

import x32Text from '../../src-tauri/libraries/x32.jqlibrary?raw';
import type { Library, LibraryError } from './api';
import { errorText } from './errors';
import { setLanguage } from './i18n/index.svelte';
import {
  entriesOf,
  fillText,
  insertableAsIs,
  localized,
  mapValue,
  normalize,
  paramProblem,
  presetTitle,
  search,
} from './library.svelte';

const x32 = JSON.parse(x32Text) as Library;
const preset = (id: string) => x32.presets.find((p) => p.id === id)!;

afterEach(() => setLanguage({ setting: 'en', resolved: 'en' }));

describe('library texts', () => {
  it('falls back to English, then to the first language', () => {
    expect(localized({ en: 'Hello', de: 'Hallo' }, 'de')).toBe('Hallo');
    expect(localized({ en: 'Hello', de: 'Hallo' }, 'fr')).toBe('Hello');
    expect(localized({ fr: 'Bonjour' }, 'de')).toBe('Bonjour');
    expect(localized(undefined)).toBe('');
  });

  it('fills placeholders as the backend does', () => {
    const fader = preset('ch-fader');
    expect(fillText('/ch/{ch:02}/mix/fader {level} {level:raw}', fader, { ch: 5, level: -10 })).toBe(
      '/ch/05/mix/fader 0.5 -10',
    );
    expect(fillText('{ch+1} {ch-1} {{x}}', fader, { ch: 5, level: 0 })).toBe('6 4 {x}');
    expect(presetTitle(preset('ch-mute'), { ch: 3, state: 1 })).toBe('Ch 3: Unmute');
    expect(presetTitle(preset('ch-mute'), { ch: 3, state: 0 }, 'de')).toBe('Kanal 3: Stummschalten');
    expect(
      mapValue(
        [
          [0, 0],
          [100, 1],
        ],
        40,
      ),
    ).toBe(0.4);
  });

  it('checks parameters as the backend does', () => {
    const [ch] = preset('ch-mute').params;
    expect(paramProblem(ch, 0)).toBe('range');
    expect(paramProblem(ch, 1.5)).toBe('number');
    expect(paramProblem(ch, 32)).toBeNull();
    const [, name] = preset('ch-name').params;
    expect(paramProblem(name, '')).toBe('required');
    expect(paramProblem(name, 'thirteen chars')).toBe('too_long');
    expect(insertableAsIs(preset('ch-name'))).toBe(false);
    expect(insertableAsIs(preset('scene'))).toBe(true);
  });

  it('searches titles, descriptions and keywords in the language and English, ignoring accents', () => {
    const entries = entriesOf([x32]);
    expect(search(entries, '').length).toBe(x32.presets.length);
    expect(search(entries, 'dca fader').map((e) => e.preset.id)).toEqual(['dca-fader']);
    setLanguage({ setting: 'de', resolved: 'de' });
    expect(search(entries, 'lautstarke', 'de').map((e) => e.preset.id)).toContain('ch-fader');
    expect(search(entries, 'scene', 'de').map((e) => e.preset.id)).toContain('scene');
    expect(normalize('Ärger')).toBe('arger');
  });

  it('words every library error', () => {
    const errors: LibraryError[] = [
      { kind: 'too_large', max: 5 * 1024 * 1024 },
      { kind: 'invalid', error: 'eof' },
      { kind: 'wrong_format', found: 'jqshow' },
      { kind: 'too_new', found: 2, current: 1 },
      { kind: 'duplicate', part: 'preset', id: 'p' },
      { kind: 'unknown_category', preset: 'p', category: 'c' },
      { kind: 'param', preset: 'p', param: 'x', problem: { kind: 'map' } },
      { kind: 'template', preset: 'p', error: { kind: 'syntax', text: '/a/{' } },
      { kind: 'cue', preset: 'p', problem: { kind: 'unknown_device', device: 'd' } },
      { kind: 'cue', preset: 'p', problem: { kind: 'media' } },
      { kind: 'preset_not_found', preset: 'p' },
    ];
    for (const language of ['en', 'de']) {
      setLanguage({ setting: language, resolved: language });
      for (const error of errors) {
        expect(errorText({ kind: 'library_file', path: '/x', error })).not.toMatch(/[{}]\w|^$/);
      }
    }
  });
});
