<script lang="ts">
  import type { Component } from 'svelte';

  import ClockCounterClockwise from '~icons/ph/clock-counter-clockwise-bold';
  import MagnifyingGlass from '~icons/ph/magnifying-glass-bold';
  import SquaresFour from '~icons/ph/squares-four-bold';
  import Star from '~icons/ph/star-bold';
  import StarFill from '~icons/ph/star-fill';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { Input } from '$lib/components/ui/input';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type ParamValue } from '../../lib/api';
  import { addPreset, type Place } from '../../lib/commands';
  import { cueIcons, libraryIcon } from '../../lib/cueIcons';
  import { t } from '../../lib/i18n/index.svelte';
  import {
    entriesFor,
    entriesOf,
    keyOf,
    libraries,
    loadLibraries,
    localized,
    offeredIds,
    paramProblem,
    paramValues,
    type PresetEntry,
    presetTitle,
    sameKey,
    search,
    setView,
  } from '../../lib/library.svelte';
  import { attempt, mode, ui } from '../../lib/store.svelte';
  import { empty, hint } from '../../lib/ui';
  import PresetParams from './PresetParams.svelte';

  // The cue libraries' presets to pick one from and insert it at `place`: by library and category, recently used and
  // favorites, with a search over titles, descriptions and keywords. The parameters are filled in on the right.
  let { place, onclose }: { place: Place; onclose: () => void } = $props();

  type Scope =
    { kind: 'all' } | { kind: 'recent' } | { kind: 'favorites' } | { kind: 'library'; id: string; category?: string };

  const ids = $derived(offeredIds());
  let loading = $state(true);
  $effect(() => {
    const wanted = ids;
    loading = true;
    loadLibraries(wanted).finally(() => (loading = false));
  });
  const list = $derived(ids.map((id) => libraries.loaded[id]).filter((l) => !!l));
  const all = $derived(entriesOf(list));
  const prefs = $derived(libraries.view?.prefs ?? { recent: [], favorites: [], disabled: [] });
  const recent = $derived(entriesFor(prefs.recent, list));
  const favorites = $derived(entriesFor(prefs.favorites, list));

  let scope = $state<Scope>({ kind: 'all' });
  // Recently used first, once there are some.
  let scopeChosen = false;
  $effect(() => {
    if (!scopeChosen && recent.length) {
      scopeChosen = true;
      scope = { kind: 'recent' };
    }
  });
  let query = $state('');
  const inScope = $derived.by(() => {
    switch (scope.kind) {
      case 'all':
        return all;
      case 'recent':
        return recent;
      case 'favorites':
        return favorites;
      case 'library': {
        const { id, category } = scope;
        return all.filter((e) => e.library.id === id && (!category || e.preset.category === category));
      }
    }
  });
  // The search looks through everything; the scope only narrows an empty search.
  const shown = $derived(query.trim() ? search(all, query) : inScope);

  let picked = $state<PresetEntry | null>(null);
  let values = $state<Record<string, ParamValue>>({});
  // The first shown entry is picked when the picked one is not shown (any more).
  $effect(() => {
    const first = shown[0] ?? null;
    if (!picked || !shown.some((e) => sameKey(keyOf(e), keyOf(picked!)))) pick(first);
  });

  function pick(entry: PresetEntry | null) {
    picked = entry;
    values = entry ? paramValues(entry.preset) : {};
  }

  const problems = $derived(picked ? picked.preset.params.filter((p) => paramProblem(p, values[p.id])).length : 0);
  const locked = $derived(mode() === 'show');
  let inserting = $state(false);

  async function insert() {
    if (!picked || problems || locked || inserting) return;
    inserting = true;
    const done = await addPreset(picked.library.id, picked.preset.id, $state.snapshot(values), place);
    inserting = false;
    if (done) onclose();
  }

  function isFavorite(entry: PresetEntry) {
    return prefs.favorites.some((key) => sameKey(key, keyOf(entry)));
  }

  async function toggleFavorite(entry: PresetEntry) {
    const view = await attempt(api.setPresetFavorite(entry.library.id, entry.preset.id, !isFavorite(entry)));
    if (view) setView(view);
  }

  let listEl = $state<HTMLElement | null>(null);

  // Up and down move through the list from the search field (and the list), Enter inserts.
  function onkeydown(e: KeyboardEvent) {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Enter') return;
    if (e.key === 'Enter') {
      if ((e.target as HTMLElement).closest('[data-params]')) return;
      e.preventDefault();
      void insert();
      return;
    }
    if (!shown.length) return;
    e.preventDefault();
    const at = picked ? shown.findIndex((entry) => sameKey(keyOf(entry), keyOf(picked!))) : -1;
    const next = shown[Math.max(0, Math.min(shown.length - 1, at + (e.key === 'ArrowDown' ? 1 : -1)))];
    pick(next);
    listEl?.querySelector<HTMLElement>(`[data-key="${next.library.id}/${next.preset.id}"]`)?.scrollIntoView({
      block: 'nearest',
    });
  }

  const scopeIs = (other: Scope) =>
    scope.kind === other.kind &&
    (scope.kind !== 'library' ||
      (other.kind === 'library' && scope.id === other.id && scope.category === other.category));

  const icon = (entry: PresetEntry): Component => cueIcons[entry.preset.cue.type] ?? libraryIcon;
  const categoryName = (entry: PresetEntry) =>
    localized(entry.library.categories.find((c) => c.id === entry.preset.category)?.name);
  const device = $derived.by(() => {
    if (picked?.preset.cue.type !== 'osc') return null;
    const id = picked.preset.cue.device;
    return picked.library.devices.find((d) => d.id === id) ?? null;
  });
  const deviceInShow = $derived(
    !!device &&
      !!picked &&
      (ui.view?.show.devices ?? []).some(
        (d) => d.template?.library === picked!.library.id && d.template.device === device.id,
      ),
  );
</script>

{#snippet scopeButton(target: Scope, label: string, Icon: Component | null, count: number | null, indent = false)}
  <button
    type="button"
    class={[
      'flex w-full items-center gap-2 rounded px-2 py-1.5 text-left text-[13px] hover:bg-hover',
      indent && 'pl-7 text-xs',
      scopeIs(target) && !query.trim() ? 'bg-selected text-fg' : 'text-fg-2',
    ]}
    aria-pressed={scopeIs(target) && !query.trim()}
    onclick={() => {
      scope = target;
      query = '';
    }}
  >
    {#if Icon}<Icon class="size-3.5 shrink-0 text-muted-foreground" />{/if}
    <span class="min-w-0 grow truncate">{label}</span>
    {#if count !== null}<span class="shrink-0 font-mono text-[11px] text-muted-foreground">{count}</span>{/if}
  </button>
{/snippet}

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) onclose();
    }
  }
>
  <Dialog.Content class="h-[min(42rem,calc(100%-2rem))] max-w-264">
    <Dialog.Header>
      <Dialog.Title>{t('library.title')}</Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}>
        <XMark />
      </Dialog.Close>
    </Dialog.Header>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)_19rem] max-md:grid-cols-1" {onkeydown}>
      <nav
        class="flex min-h-0 flex-col gap-0.5 overflow-y-auto border-r border-line p-2"
        aria-label={t('library.scopes')}
      >
        {@render scopeButton({ kind: 'all' }, t('library.all'), SquaresFour, all.length)}
        {@render scopeButton({ kind: 'recent' }, t('library.recent'), ClockCounterClockwise, recent.length)}
        {@render scopeButton({ kind: 'favorites' }, t('library.favorites'), Star, favorites.length)}
        {#each list as library (library.id)}
          <div class="mt-2 border-t border-line pt-2">
            {@render scopeButton(
              { kind: 'library', id: library.id },
              localized(library.name),
              libraryIcon,
              library.presets.length,
            )}
            {#each library.categories as category (category.id)}
              {@render scopeButton(
                { kind: 'library', id: library.id, category: category.id },
                localized(category.name),
                null,
                library.presets.filter((p) => p.category === category.id).length,
                true,
              )}
            {/each}
          </div>
        {/each}
      </nav>

      <div class="flex min-h-0 flex-col">
        <div class="relative shrink-0 border-b border-line p-2">
          <MagnifyingGlass
            class="pointer-events-none absolute top-1/2 left-4.5 size-3.5 -translate-y-1/2 text-muted-foreground"
          />
          <Input class="pl-7" type="search" autofocus placeholder={t('library.search')} bind:value={query} />
        </div>
        <ul
          bind:this={listEl}
          class="m-0 min-h-0 flex-1 list-none overflow-y-auto p-1"
          role="listbox"
          aria-label={t('library.presets')}
        >
          {#each shown as entry (`${entry.library.id}/${entry.preset.id}`)}
            {@const Icon = icon(entry)}
            {@const selected = !!picked && sameKey(keyOf(entry), keyOf(picked))}
            <!-- Keys move through the list from the search field (`onkeydown` above). -->
            <!-- svelte-ignore a11y_click_events_have_key_events -->
            <li
              role="option"
              aria-selected={selected}
              data-key="{entry.library.id}/{entry.preset.id}"
              class={[
                'group flex cursor-default items-center gap-2.5 rounded px-2 py-1.5',
                selected ? 'bg-selected' : 'hover:bg-hover',
              ]}
              onclick={() => pick(entry)}
              ondblclick={() => {
                pick(entry);
                void insert();
              }}
            >
              <Icon class="size-4 shrink-0 text-muted-foreground" />
              <span class="flex min-w-0 grow flex-col">
                <span class="truncate text-[13px] text-fg">{presetTitle(entry.preset)}</span>
                <span class="truncate text-[11px] text-muted-foreground">
                  {[scope.kind === 'library' && !query.trim() ? '' : localized(entry.library.name), categoryName(entry)]
                    .filter(Boolean)
                    .join(' · ')}
                </span>
              </span>
              <Tooltip tip={isFavorite(entry) ? t('library.unfavorite') : t('library.favorite')}>
                {#snippet child({ props })}
                  <button
                    {...props}
                    type="button"
                    class={[
                      'shrink-0 rounded p-1 text-muted-foreground hover:text-fg',
                      !isFavorite(entry) && 'opacity-0 group-hover:opacity-100 focus-visible:opacity-100',
                    ]}
                    aria-label={isFavorite(entry) ? t('library.unfavorite') : t('library.favorite')}
                    aria-pressed={isFavorite(entry)}
                    onclick={(e) => {
                      e.stopPropagation();
                      void toggleFavorite(entry);
                    }}
                  >
                    {#if isFavorite(entry)}<StarFill class="size-3.5 text-primary" />{:else}<Star
                        class="size-3.5"
                      />{/if}
                  </button>
                {/snippet}
              </Tooltip>
            </li>
          {:else}
            <li class={empty}>
              {#if loading}
                {t('library.loading')}
              {:else if !ids.length}
                {t('library.none_enabled')}
              {:else if query.trim()}
                {t('library.nothing_found', { query: query.trim() })}
              {:else if scope.kind === 'recent'}
                {t('library.no_recent')}
              {:else if scope.kind === 'favorites'}
                {t('library.no_favorites')}
              {:else}
                {t('library.empty')}
              {/if}
            </li>
          {/each}
        </ul>
      </div>

      <aside class="flex min-h-0 flex-col border-l border-line max-md:border-t max-md:border-l-0">
        {#if picked}
          <div class="flex min-h-0 flex-1 flex-col gap-3 overflow-y-auto p-4" data-params>
            <div class="flex flex-col gap-1">
              <h3 class="m-0 text-sm font-semibold text-balance">{presetTitle(picked.preset, values)}</h3>
              <span class={hint}>
                {[localized(picked.library.name), categoryName(picked)].filter(Boolean).join(' · ')}
              </span>
            </div>
            {#if localized(picked.preset.description)}
              <p class="m-0 text-[13px] leading-normal text-fg-2">{localized(picked.preset.description)}</p>
            {/if}
            <PresetParams preset={picked.preset} bind:values disabled={locked} onsubmit={insert} />
            {#if device}
              <p class="{hint} m-0">
                {deviceInShow
                  ? t('library.device_used', { name: localized(device.name) })
                  : t('library.device_added', { name: localized(device.name), port: device.port })}
              </p>
            {/if}
          </div>
          <div class="flex shrink-0 items-center justify-end gap-2 border-t border-line p-3">
            <Tooltip tip={locked ? t('library.show_mode') : problems ? t('library.fix_params') : undefined}>
              {#snippet child({ props })}
                <span {...props} class="inline-flex">
                  <Button variant="default" disabled={!!problems || locked || inserting} onclick={insert}>
                    {t('library.insert')}
                  </Button>
                </span>
              {/snippet}
            </Tooltip>
          </div>
        {:else}
          <p class={empty}>{t('library.pick')}</p>
        {/if}
      </aside>
    </div>
  </Dialog.Content>
</Dialog.Root>
