// Cue libraries in the UI: the backend's list (`LibrariesView`, kept current by the `libraries` event), the full
// libraries of the enabled ones loaded on demand, and the helpers to show and search their presets. Texts come in
// several languages (`LocalizedText`): the current one, else English, else the first.

import {
  api,
  events,
  type LibrariesView,
  type Library,
  LIBRARY_EXTENSION,
  type LibraryParam,
  type LibraryPreset,
  type LocalizedText,
  type ParamValue,
  type PresetKey,
} from './api';
import { i18n, t } from './i18n/index.svelte';
import { ui } from './store.svelte';

export const libraries = $state({
  view: null as LibrariesView | null,
  /** Full libraries by id, as last loaded (see `loadLibraries`). */
  loaded: {} as Record<string, Library>,
});

/** Takes the libraries list from the backend and follows its changes; works without a show. */
export function initLibraries() {
  void events.onLibraries((view) => setView(view));
  api.libraries().then(setView, () => {});
}

/** Shows a list the backend sent (also as a command's answer). */
export function setView(view: LibrariesView) {
  libraries.view = view;
  // A library installed anew may have other presets: loaded again when next needed.
  for (const summary of view.libraries) {
    const loaded = summary.id && libraries.loaded[summary.id];
    if (loaded && summary.active && loaded.revision !== summary.revision) delete libraries.loaded[summary.id!];
  }
}

/** The ids of the libraries offered when adding cues: readable, in use, enabled. */
export function offeredIds(view: LibrariesView | null = libraries.view): string[] {
  return (view?.libraries ?? []).filter((l) => l.id && l.active && l.enabled).map((l) => l.id!);
}

/** Loads the full libraries with these ids that are not loaded yet. */
export async function loadLibraries(ids: string[]): Promise<void> {
  const missing = ids.filter((id) => !libraries.loaded[id]);
  if (!missing.length) return;
  const found = await api.libraryPresets(missing);
  Object.assign(libraries.loaded, found);
}

/** `text` in the current language, else English, else its first language; "" if it has none. */
export function localized(text: LocalizedText | undefined, lang: string = i18n.locale): string {
  if (!text) return '';
  return text[lang] ?? text.en ?? Object.values(text)[0] ?? '';
}

/** A parameter's default value. */
export function paramDefault(param: LibraryParam): ParamValue {
  return param.default;
}

/** The preset's parameters: `given` where set, defaults for the rest. */
export function paramValues(preset: LibraryPreset, given: Record<string, ParamValue> = {}): Record<string, ParamValue> {
  return Object.fromEntries(preset.params.map((p) => [p.id, given[p.id] ?? paramDefault(p)]));
}

/** `x` through a piecewise-linear map (as the backend's `map_value`). */
export function mapValue(map: [number, number][] | undefined, x: number): number {
  if (!map?.length) return x;
  if (x <= map[0][0]) return map[0][1];
  const last = map[map.length - 1];
  if (x >= last[0]) return last[1];
  for (let i = 1; i < map.length; i++) {
    const [x0, y0] = map[i - 1];
    const [x1, y1] = map[i];
    if (x <= x1) return x1 === x0 ? y1 : y0 + ((y1 - y0) * (x - x0)) / (x1 - x0);
  }
  return last[1];
}

function numberText(n: number): string {
  return Number.isInteger(n) ? String(n) : String(Number(n.toFixed(6)));
}

/** A preset text with its placeholders filled in (as the backend fills titles); unknown ones stay as they are. */
export function fillText(
  text: string,
  preset: LibraryPreset,
  values: Record<string, ParamValue>,
  lang: string = i18n.locale,
): string {
  const filled = text.replace(
    /\{\{|\}\}|\{([A-Za-z0-9_]+)([+-]\d+)?(?::(label|raw|0\d+))?\}/g,
    (whole, name, offset, spec) => {
      if (whole === '{{') return '{';
      if (whole === '}}') return '}';
      const param = preset.params.find((p) => p.id === name);
      const raw = values[name];
      if (!param || raw === undefined) return whole;
      if (spec === 'label') {
        const choice = param.type === 'choice' ? param.values.find((c) => c.value === raw) : undefined;
        return choice ? localized(choice.label, lang) : whole;
      }
      if (typeof raw === 'string') return raw;
      const map = param.type === 'int' || param.type === 'float' ? param.map : undefined;
      const value = (spec === 'raw' ? raw : mapValue(map, raw)) + Number(offset ?? 0);
      if (spec && spec !== 'raw') {
        const width = Number(spec.slice(1));
        const n = Math.round(value);
        return (n < 0 ? '-' : '') + String(Math.abs(n)).padStart(width, '0');
      }
      return numberText(value);
    },
  );
  return filled.split(/\s+/).filter(Boolean).join(' ');
}

/** A preset's title with `values` (default: its defaults) in the current language. */
export function presetTitle(preset: LibraryPreset, values?: Record<string, ParamValue>, lang: string = i18n.locale) {
  return fillText(localized(preset.title, lang), preset, values ?? paramValues(preset), lang);
}

/** Why a parameter value cannot be used; null if it can. */
export function paramProblem(param: LibraryParam, value: ParamValue | undefined): ParamProblemKind | null {
  switch (param.type) {
    case 'int':
    case 'float':
      if (typeof value !== 'number' || !Number.isFinite(value)) return 'number';
      if (param.type === 'int' && !Number.isInteger(value)) return 'number';
      return value < param.min || value > param.max ? 'range' : null;
    case 'text': {
      const text = typeof value === 'string' ? value : '';
      if (!text) return param.required ? 'required' : null;
      if (param.max_len !== undefined && [...text].length > param.max_len) return 'too_long';
      if (param.pattern && !new RegExp(`^(?:${param.pattern})$`, 'u').test(text)) return 'pattern';
      return null;
    }
    case 'choice':
      return param.values.some((c) => c.value === value) ? null : 'choice';
  }
}

export type ParamProblemKind = 'number' | 'range' | 'required' | 'too_long' | 'pattern' | 'choice';

/** Whether a preset can be inserted with its defaults (no text to fill in first). */
export function insertableAsIs(preset: LibraryPreset): boolean {
  return preset.params.every((p) => !paramProblem(p, paramDefault(p)));
}

/** A preset of a loaded library, with the library. */
export interface PresetEntry {
  library: Library;
  preset: LibraryPreset;
}

export const keyOf = (entry: PresetEntry): PresetKey => ({ library: entry.library.id, preset: entry.preset.id });
export const sameKey = (a: PresetKey, b: PresetKey) => a.library === b.library && a.preset === b.preset;

/** Lower case, without accents, so "Lautstarke" finds "Lautstärke". */
export function normalize(text: string): string {
  return text.normalize('NFD').replace(/\p{M}/gu, '').toLowerCase();
}

/** What the search looks through for a preset: titles, descriptions, keywords, category and library name, in the
 * current language and English. */
function searchText(entry: PresetEntry, lang: string): string {
  const { library, preset } = entry;
  const category = library.categories.find((c) => c.id === preset.category);
  const parts: string[] = [];
  for (const l of new Set([lang, 'en'])) {
    parts.push(
      presetTitle(preset, undefined, l),
      localized(preset.title, l),
      localized(preset.description, l),
      ...(preset.keywords[l] ?? []),
      localized(category?.name, l),
      localized(library.name, l),
    );
  }
  parts.push(preset.cue.type);
  return normalize(parts.join(' '));
}

/** The entries every word of `query` is found in (all with an empty query). */
export function search(entries: PresetEntry[], query: string, lang: string = i18n.locale): PresetEntry[] {
  const words = normalize(query).split(/\s+/).filter(Boolean);
  if (!words.length) return entries;
  return entries.filter((entry) => {
    const text = searchText(entry, lang);
    return words.every((word) => text.includes(word));
  });
}

/** Every preset of the given libraries, in library and file order. */
export function entriesOf(list: Library[]): PresetEntry[] {
  return list.flatMap((library) => library.presets.map((preset) => ({ library, preset })));
}

/** The entries for `keys` that are loaded, in the order of `keys`. */
export function entriesFor(keys: PresetKey[], list: Library[]): PresetEntry[] {
  return keys.flatMap((key) => {
    const library = list.find((l) => l.id === key.library);
    const preset = library?.presets.find((p) => p.id === key.preset);
    return library && preset ? [{ library, preset }] : [];
  });
}

/** Picks a library file and shows it to be installed (`LibraryImportDialog`). */
export async function pickLibraryFile(): Promise<void> {
  const { open } = await import('@tauri-apps/plugin-dialog');
  const path = await open({
    multiple: false,
    directory: false,
    filters: [{ name: t('library.file_type'), extensions: [LIBRARY_EXTENSION] }],
  });
  if (typeof path === 'string') ui.libraryImport = path;
}
