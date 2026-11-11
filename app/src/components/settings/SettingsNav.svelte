<script lang="ts">
  import type { Component } from 'svelte';

  import ArrowsSplit from '~icons/ph/arrows-split';
  import Broadcast from '~icons/ph/broadcast';
  import CursorClick from '~icons/ph/cursor-click';
  import Keyboard from '~icons/ph/keyboard';
  import MagnifyingGlass from '~icons/ph/magnifying-glass';
  import PianoKeys from '~icons/ph/piano-keys';
  import Play from '~icons/ph/play';
  import SpeakerHifi from '~icons/ph/speaker-hifi';
  import Waveform from '~icons/ph/waveform';
  import { Input } from '$lib/components/ui/input';
  import { Kbd } from '$lib/components/ui/kbd';

  import { t } from '../../lib/i18n/index.svelte';
  import { SETTINGS_GROUPS, type SettingsTab } from '../../lib/settings';
  import { settingsView } from '../../lib/settingsView.svelte';

  // Search field and tab list of the settings dialog. A tab shows a dot when
  // it was changed (blue) or has a problem (red), while searching its hits.
  let {
    changed,
    problems,
    search = $bindable(null),
  }: {
    changed: Map<SettingsTab, number>;
    problems: Set<SettingsTab>;
    /** The search field, for Ctrl+F. */
    search?: HTMLInputElement | null;
  } = $props();

  const view = settingsView();
  const icons: Record<SettingsTab, Component> = {
    device: SpeakerHifi,
    routing: ArrowsSplit,
    show: Play,
    keys: Keyboard,
    midi: PianoKeys,
    osc: Broadcast,
    ui: CursorClick,
    media: Waveform,
  };
  const order = SETTINGS_GROUPS.flatMap((group) => group.tabs.map((tab) => tab.id));
  let list = $state<HTMLElement | null>(null);

  // Arrow keys move between the tabs (and open them), as in any tab list.
  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowDown: 1, ArrowRight: 1, ArrowUp: -1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    const next = order[(order.indexOf(view.tab) + step + order.length) % order.length];
    view.open(next);
    list?.querySelector<HTMLElement>(`#settings-tab-${next}`)?.focus();
  }
</script>

<nav
  aria-label={t('settings.sections')}
  class="flex min-h-0 flex-col border-r border-line bg-panel-2 max-sm:border-r-0 max-sm:border-b"
>
  <div class="relative p-2.5">
    <MagnifyingGlass
      class="pointer-events-none absolute top-1/2 left-5 -translate-y-1/2 text-sm text-muted-foreground"
    />
    <Input
      bind:ref={search}
      type="search"
      class="pr-14 pl-7"
      placeholder={t('common.search')}
      aria-label={t('settings.search')}
      autocomplete="off"
      bind:value={view.query}
      onkeydown={(e) => {
        if (e.key === 'Enter') void view.openFirstHit();
        else if (e.key === 'Escape' && view.query) {
          // Clears the search instead of closing the dialog.
          e.stopPropagation();
          e.preventDefault();
          view.query = '';
        }
      }}
    />
    {#if !view.query}
      <Kbd class="absolute top-1/2 right-4.5 -translate-y-1/2 max-sm:hidden">{t('key.ctrl')} + F</Kbd>
    {/if}
  </div>
  <div
    bind:this={list}
    role="tablist"
    aria-orientation="vertical"
    tabindex={-1}
    class="flex min-h-0 flex-col gap-px overflow-y-auto px-2 pb-3 max-sm:flex-row max-sm:overflow-x-auto"
    {onkeydown}
  >
    {#each SETTINGS_GROUPS as group, index (group.title)}
      <div
        class={[
          'px-2 pb-1 text-[10.5px] font-semibold tracking-[0.07em] text-muted-foreground uppercase max-sm:hidden',
          index === 0 ? 'pt-0.5' : 'pt-3',
        ]}
      >
        {group.title}
      </div>
      {#each group.tabs as tab (tab.id)}
        {@const Icon = icons[tab.id]}
        {@const selected = !view.searching && view.tab === tab.id}
        {@const hits = view.searching ? view.hits(tab.id) : null}
        <button
          id="settings-tab-{tab.id}"
          role="tab"
          aria-selected={selected}
          aria-controls="settings-panel-{tab.id}"
          tabindex={view.tab === tab.id ? 0 : -1}
          class={[
            'flex shrink-0 cursor-pointer items-center gap-2.5 rounded-md px-2 py-1.5 text-left text-[13px] font-medium whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-ring',
            selected ? 'bg-mode text-fg' : 'text-fg-2 hover:bg-hover',
            hits === 0 && 'opacity-40',
          ]}
          onclick={() => view.open(tab.id)}
        >
          <Icon class={['shrink-0 text-base', selected ? 'text-primary' : 'text-muted-foreground']} />
          {tab.title}
          <span class="ml-auto flex items-center gap-1">
            {#if hits}
              <span class="text-[11px] text-primary tabular-nums">{hits}</span>
            {/if}
            {#if problems.has(tab.id)}
              <span class="size-1.75 rounded-full bg-stop-soft" aria-label={t('settings.problem')}></span>
            {/if}
            {#if changed.has(tab.id) && hits === null}
              <span class="size-1.75 rounded-full bg-primary" aria-label={t('settings.changed')}></span>
            {/if}
          </span>
        </button>
      {/each}
    {/each}
  </div>
</nav>
