<script lang="ts">
  import { t } from '../../lib/i18n/index.svelte';
  import type { InspectorTab, TabDef } from '../../lib/inspector.svelte';

  // The inspector's tab bar. A red dot marks a tab with a notice; the tab list scrolls the
  // selected tab fully into view.
  let {
    tabs,
    active,
    problems,
    onselect,
  }: {
    tabs: TabDef[];
    active: InspectorTab;
    problems: Set<InspectorTab>;
    onselect: (tab: InspectorTab) => void;
  } = $props();

  let list = $state<HTMLElement | null>(null);

  // Keeps the selected tab fully visible, whether it was clicked, chosen with the arrow keys or opened from a notice.
  $effect(() => {
    list?.querySelector<HTMLElement>(`[data-tab="${active}"]`)?.scrollIntoView({ block: 'nearest', inline: 'nearest' });
  });

  // Arrow keys move between the tabs (and open them), as in any tab list.
  function onkeydown(e: KeyboardEvent) {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    const index = tabs.findIndex((tab) => tab.id === active);
    const next = tabs[(index + step + tabs.length) % tabs.length].id;
    onselect(next);
    list?.querySelector<HTMLElement>(`[data-tab="${next}"]`)?.focus();
  }
</script>

<div
  bind:this={list}
  role="tablist"
  aria-label={t('inspector.tabs_label')}
  tabindex="-1"
  class="flex shrink-0 scrollbar-none gap-0.5 overflow-x-auto border-b border-line px-2.5"
  {onkeydown}
>
  {#each tabs as tab (tab.id)}
    {@const selected = tab.id === active}
    <button
      type="button"
      role="tab"
      data-tab={tab.id}
      aria-selected={selected}
      tabindex={selected ? 0 : -1}
      class={[
        '-mb-px flex items-center gap-1.5 border-b-2 px-2 pt-1.5 pb-1.5 text-[13px] whitespace-nowrap outline-none focus-visible:ring-2 focus-visible:ring-ring',
        selected ? 'border-primary text-fg' : 'border-transparent text-muted-foreground hover:text-fg-2',
      ]}
      onclick={() => onselect(tab.id)}
    >
      {tab.label}
      {#if problems.has(tab.id)}
        <span class="size-1.5 rounded-full bg-stop-soft" aria-label={t('inspector.tab_notice')}></span>
      {/if}
    </button>
  {/each}
</div>
