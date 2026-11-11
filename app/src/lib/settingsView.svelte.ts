// Navigation of the settings dialog: the open tab and the search over all
// tabs. Every setting row registers its text here (see `useSetting`), so the
// search can filter rows, groups and tabs and count the hits per tab.

import { getContext, setContext, tick } from 'svelte';
import { SvelteMap } from 'svelte/reactivity';

import type { SettingsTab } from './settings';

type Entry = { tab: SettingsTab; group: symbol | null; text: string; el: HTMLElement | null };

export class SettingsView {
  tab = $state<SettingsTab>('device');
  query = $state('');
  private readonly entries = new SvelteMap<symbol, Entry>();

  private readonly words = $derived(this.query.trim().toLowerCase().split(/\s+/).filter(Boolean));
  readonly searching = $derived(this.words.length > 0);

  private matches(entry: Entry): boolean {
    return this.words.every((word) => entry.text.includes(word));
  }

  register(id: symbol, entry: Entry) {
    this.entries.set(id, { ...entry, text: entry.text.toLowerCase() });
  }

  unregister(id: symbol) {
    this.entries.delete(id);
  }

  /** Whether the row is shown: always, unless a search leaves it out. */
  shows(id: symbol): boolean {
    if (!this.searching) return true;
    const entry = this.entries.get(id);
    return !!entry && this.matches(entry);
  }

  /** Whether a group is shown: unless a search leaves out all its rows. */
  showsGroup(group: symbol): boolean {
    if (!this.searching) return true;
    for (const entry of this.entries.values()) if (entry.group === group && this.matches(entry)) return true;
    return false;
  }

  /** Rows of `tab` the search finds. */
  hits(tab: SettingsTab): number {
    let count = 0;
    for (const entry of this.entries.values()) if (entry.tab === tab && this.matches(entry)) count++;
    return count;
  }

  /** Whether the search finds anything at all. */
  found(): boolean {
    for (const entry of this.entries.values()) if (this.matches(entry)) return true;
    return false;
  }

  /** Whether a tab's panel is shown: the open one, or while searching every one with hits. */
  showsPanel(tab: SettingsTab): boolean {
    return this.searching ? this.hits(tab) > 0 : this.tab === tab;
  }

  /** Opens `tab` and ends the search. */
  open(tab: SettingsTab) {
    this.query = '';
    this.tab = tab;
  }

  /** Opens the tab of the first hit and focuses its first control. */
  async openFirstHit() {
    const hit = [...this.entries.values()].find((entry) => this.matches(entry));
    if (!hit) return;
    this.open(hit.tab);
    await tick();
    hit.el?.scrollIntoView({ block: 'center' });
    hit.el?.querySelector<HTMLElement>('input, button:not(:disabled), [tabindex="0"]')?.focus();
  }
}

const VIEW = Symbol('settings-view');
const TAB = Symbol('settings-tab');
const GROUP = Symbol('settings-group');

type TabContext = { id: SettingsTab; title: () => string };
type GroupContext = { id: symbol; title: () => string };

export const provideSettingsView = (view: SettingsView) => setContext(VIEW, view);
export const settingsView = () => getContext<SettingsView>(VIEW);
export const provideSettingsTab = (tab: TabContext) => setContext(TAB, tab);

/** Opens a group of rows; returns its id for `SettingsView.showsGroup`. */
export function provideSettingsGroup(title: () => string): symbol {
  const id = Symbol();
  setContext<GroupContext>(GROUP, { id, title });
  return id;
}

/**
 * Registers a setting row for the search, with its searchable text (label,
 * keywords, current values). Call during component init; `visible` tells
 * whether the search leaves it in.
 */
export function useSetting(text: () => string, el: () => HTMLElement | null) {
  const view = settingsView();
  const tab = getContext<TabContext>(TAB);
  const group = getContext<GroupContext | undefined>(GROUP);
  const id = Symbol();
  $effect(() => {
    view.register(id, {
      tab: tab.id,
      group: group?.id ?? null,
      text: [tab.title(), group?.title() ?? '', text()].join(' '),
      el: el(),
    });
    return () => view.unregister(id);
  });
  return {
    get visible() {
      return view.shows(id);
    },
  };
}
