// Localisation of the UI. The texts live in `en.ts` (the master: its keys define `MessageKey`) and `de.ts` (must have
// every key). English is the fallback for a language that is not offered and for a text missing in one.
//
// `t(key, params)` reads the reactive `i18n.locale`, so a text in a template or a `$derived` follows a change of the
// language by itself. Constants outside of components hold keys or lazy labels (`labels`), never finished texts.
//
// A text with plural forms has the keys `key.one` and `key.other`; `t('key', { count })` picks the form.

import de from './de';
import en, { type MessageKey } from './en';

/** The languages the UI is offered in; each is shown by its own name. The first is the fallback. */
export const LANGUAGES = [
  { code: 'en', name: 'English' },
  { code: 'de', name: 'Deutsch' },
] as const;

export type Locale = (typeof LANGUAGES)[number]['code'];

/** The setting value for "follow the system language". */
export const AUTO = 'auto';

const FALLBACK: Locale = 'en';

const catalogs: Record<Locale, Record<string, string>> = { en, de };

type Singular = Exclude<MessageKey, `${string}.one` | `${string}.other`>;
type PluralBase<K> = K extends `${infer Base}.one` | `${infer Base}.other` ? Base : never;
/** A key `t` takes: a text, or the base of one with plural forms (then `count` is a parameter). */
export type TextKey = Singular | PluralBase<MessageKey>;

export type Params = Record<string, string | number>;

/** The offered language for a code like "de", "de-AT" or "de_AT.UTF-8"; null if it is not offered. */
export function localeOf(code: string): Locale | null {
  const primary = code.split(/[-_.@]/)[0]?.toLowerCase();
  return LANGUAGES.find((language) => language.code === primary)?.code ?? null;
}

/** The first of the preferred languages that is offered, else English. */
export function detectLocale(preferred: readonly string[]): Locale {
  for (const code of preferred) {
    const locale = localeOf(code);
    if (locale) return locale;
  }
  return FALLBACK;
}

/** Until the backend has answered (`getLanguage`, see `initLanguage`), the browser's language is the best guess. */
function initialLocale(): Locale {
  return detectLocale(typeof navigator === 'undefined' ? [] : navigator.languages);
}

/** `setting`: what the user chose ("auto" or a code); `locale`: the language in use. */
export const i18n = $state({ setting: AUTO as string, locale: initialLocale() });

/** Takes over what the backend resolved (`getLanguage`, event `language_changed`). */
export function setLanguage(info: { setting: string; resolved: string }) {
  i18n.setting = info.setting;
  i18n.locale = localeOf(info.resolved) ?? FALLBACK;
  document.documentElement.lang = i18n.locale;
}

const pluralRules: Partial<Record<Locale, Intl.PluralRules>> = {};

function pluralForm(locale: Locale, count: number): string {
  const rules = (pluralRules[locale] ??= new Intl.PluralRules(locale));
  return rules.select(count);
}

function lookup(locale: Locale, key: string): string | undefined {
  return catalogs[locale][key] ?? catalogs[FALLBACK][key];
}

/** The text of `key` in `locale` with `{name}` filled in from `params`. */
export function translate(locale: Locale, key: TextKey, params?: Params): string {
  let template: string | undefined;
  if (params && typeof params.count === 'number') {
    const form = pluralForm(locale, params.count);
    template = lookup(locale, `${key}.${form}`) ?? lookup(locale, `${key}.other`);
  }
  template ??= lookup(locale, key);
  if (template === undefined) return key;
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match));
}

/** The text of `key` in every offered language (e.g. to recognise a default title made in another language). */
export function translations(key: TextKey): string[] {
  return LANGUAGES.map((language) => translate(language.code, key));
}

/** The text of `key` in the current language. */
export function t(key: TextKey, params?: Params): string {
  return translate(i18n.locale, key, params);
}

/**
 * An object whose values are texts that follow the language: each is read (and so translated) when it is used.
 * For constants outside of components, e.g. `labels({ fade: 'cue.others.fade' })`.
 */
export function labels<K extends string>(keys: Record<K, TextKey>): Record<K, string> {
  const result = {} as Record<K, string>;
  for (const name of Object.keys(keys) as K[]) {
    Object.defineProperty(result, name, { get: () => t(keys[name]), enumerable: true });
  }
  return result;
}

/** Choices for a select whose labels follow the language: `value` → key of the label, in this order. */
export function options<T extends string>(keys: Record<T, TextKey>): { value: T; label: string }[] {
  return (Object.keys(keys) as T[]).map((value) => ({
    value,
    get label() {
      return t(keys[value]);
    },
  }));
}

/** Formatters of `fmtNumber` by language and digits; a plain cache, nothing reacts to it. */
// eslint-disable-next-line svelte/prefer-svelte-reactivity
const numberFormats = new Map<string, Intl.NumberFormat>();

/** A number as text in the current language ("2,5" / "2.5"), with `digits` decimals (as many as needed if omitted). */
export function fmtNumber(value: number, digits?: number): string {
  // Called for every running time ~30×/s; building a formatter each time is the costly part.
  const locale = i18n.locale;
  const key = `${locale}:${digits ?? ''}`;
  let format = numberFormats.get(key);
  if (!format) {
    format = new Intl.NumberFormat(locale, {
      useGrouping: false,
      minimumFractionDigits: digits ?? 0,
      maximumFractionDigits: digits ?? 20,
    });
    numberFormats.set(key, format);
  }
  return format.format(value);
}
