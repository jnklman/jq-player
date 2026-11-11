import { describe, expect, it } from 'vitest';

import de from './de';
import en from './en';
import { detectLocale, labels, localeOf, translate } from './index.svelte';

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe('catalogs', () => {
  it('have the same keys and placeholders', () => {
    expect(Object.keys(de).sort()).toEqual(Object.keys(en).sort());
    for (const [key, text] of Object.entries(en)) {
      expect(placeholders((de as Record<string, string>)[key]), key).toEqual(placeholders(text));
    }
  });

  it('give every plural text both forms', () => {
    for (const key of Object.keys(en)) {
      if (key.endsWith('.one')) expect(en, key).toHaveProperty(key.replace(/\.one$/, '.other'));
    }
  });
});

describe('locale detection', () => {
  it('maps codes to offered languages', () => {
    expect(localeOf('de-AT')).toBe('de');
    expect(localeOf('en_US.UTF-8')).toBe('en');
    expect(localeOf('fr-FR')).toBeNull();
  });

  it('takes the first offered preference and falls back to English', () => {
    expect(detectLocale(['fr-FR', 'de-DE', 'en'])).toBe('de');
    expect(detectLocale(['fr-FR', 'it'])).toBe('en');
    expect(detectLocale([])).toBe('en');
  });
});

describe('translate', () => {
  it('fills in parameters and picks plural forms', () => {
    expect(translate('en', 'leave.running', { count: 1 })).toBe('One cue is still running.');
    expect(translate('en', 'leave.running', { count: 3 })).toBe('3 cues are still running.');
    expect(translate('de', 'leave.running', { count: 3 })).toBe('Es laufen noch 3 Cues.');
    expect(translate('de', 'about.version', { version: '1.2' })).toBe('Version 1.2');
  });

  it('labels follow the language when read', () => {
    const names = labels({ go: 'action.go', toggle: 'action.toggle_pause_all' });
    expect(names.go).toBe('GO');
  });
});
