import { describe, expect, it, vi } from 'vitest';

import { SettingsView } from './settingsView.svelte';

function filled() {
  const view = new SettingsView();
  const ids = { volume: Symbol(), fade: Symbol(), port: Symbol() };
  const group = Symbol();
  const el = document.createElement('div');
  el.innerHTML = '<button disabled></button><input />';
  document.body.append(el);
  view.register(ids.volume, { tab: 'keys', group, text: 'Volume step', el: null });
  view.register(ids.fade, { tab: 'show', group: null, text: 'FADE ALL time', el });
  view.register(ids.port, { tab: 'osc', group: null, text: 'OSC port', el: null });
  return { view, ids, group, el };
}

describe('SettingsView', () => {
  it('shows everything and the open tab without a search', () => {
    const { view, ids, group } = filled();
    expect(view.searching).toBe(false);
    expect(view.shows(ids.port)).toBe(true);
    expect(view.showsGroup(group)).toBe(true);
    expect(view.showsPanel('device')).toBe(true);
    expect(view.showsPanel('osc')).toBe(false);
  });

  it('filters rows, groups and tabs by every word, ignoring case', () => {
    const { view, ids, group } = filled();
    view.query = '  fade   TIME ';
    expect(view.searching).toBe(true);
    expect(view.shows(ids.fade)).toBe(true);
    expect(view.shows(ids.volume)).toBe(false);
    expect(view.shows(Symbol())).toBe(false);
    expect(view.showsGroup(group)).toBe(false);
    expect(view.hits('show')).toBe(1);
    expect(view.showsPanel('show')).toBe(true);
    expect(view.showsPanel('device')).toBe(false);
    expect(view.found()).toBe(true);
    view.query = 'nothing';
    expect(view.found()).toBe(false);
  });

  it('forgets rows that unregister', () => {
    const { view, ids } = filled();
    view.unregister(ids.port);
    view.query = 'osc';
    expect(view.found()).toBe(false);
  });

  it('opens the first hit and focuses its first usable control', async () => {
    const { view, el } = filled();
    el.scrollIntoView = vi.fn();
    view.query = 'fade';
    await view.openFirstHit();
    expect(view.tab).toBe('show');
    expect(view.query).toBe('');
    expect(el.scrollIntoView).toHaveBeenCalled();
    expect(document.activeElement).toBe(el.querySelector('input'));
  });

  it('stays without a hit', async () => {
    const { view } = filled();
    view.query = 'nothing';
    await view.openFirstHit();
    expect(view.tab).toBe('device');
  });
});
