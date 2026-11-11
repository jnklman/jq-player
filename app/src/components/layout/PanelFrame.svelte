<script lang="ts">
  import type { Snippet } from 'svelte';

  import DotsSixVertical from '~icons/ph/dots-six-vertical-bold';
  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';
  import { dragFill, panelHead, panelTitle } from '../../lib/ui';
  import { getArrange, getPanelId, getWindowChrome } from './context';
  import { fitHeights } from './fit.svelte';

  // The frame of a panel: title row (optionally with actions on the right)
  // over the content, which handles its own scrolling. In the workspace the
  // title doubles as the handle for moving the panel. The title keeps its
  // width (up to 3/5 of the row); the actions wrap instead. In a panel
  // window the title row is the window's title bar (`PanelWindow.svelte`).
  // With `fit`, the content keeps its natural height and the layout gives
  // the panel at least that much room (`fit.svelte.ts`). Where even that is
  // not there, it scrolls in a ScrollArea the frame brings along.
  let {
    title,
    label = title,
    fit = false,
    actions,
    children,
  }: { title: string; label?: string; fit?: boolean; actions?: Snippet; children: Snippet } = $props();

  const arrange = getArrange();
  const id = getPanelId();
  const movable = $derived(arrange !== undefined && id !== undefined && arrange.enabled());
  const chrome = getWindowChrome();

  let headHeight = $state(0);
  let contentHeight = $state(0);
  $effect(() => {
    if (!fit || id === undefined) return;
    // Plus the frame's border.
    fitHeights.set(id, headHeight + contentHeight + 2);
    return () => fitHeights.delete(id);
  });
</script>

<section
  class={[
    'flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-panel',
    chrome ? 'rounded-[10px] in-data-native-frame:rounded-none' : 'rounded-xl border border-line',
  ]}
  data-slot="panel-frame"
  aria-label={label}
>
  <div class={panelHead} data-tauri-drag-region={chrome?.drag ? 'deep' : undefined} bind:offsetHeight={headHeight}>
    {#if movable}
      <!-- Handlers go on the Tooltip, which merges them with the tooltip's own. -->
      <Tooltip tip={t('layout.drag_panel')} side="bottom" onpointerdown={(e) => arrange!.start(id!, e)}>
        {#snippet child({ props })}
          <div
            {...props}
            class="-ml-1.5 flex max-w-3/5 min-w-0 shrink-0 cursor-grab touch-none items-center gap-1 active:cursor-grabbing"
            role="presentation"
          >
            <DotsSixVertical class="shrink-0 text-dim" />
            <h2 class="{panelTitle} truncate">{title}</h2>
          </div>
        {/snippet}
      </Tooltip>
    {:else}
      <h2 class="{panelTitle} max-w-3/5 shrink-0 truncate">{title}</h2>
    {/if}
    {#if chrome}
      {#if chrome.drag}
        <div class={dragFill} data-tauri-drag-region="deep"></div>
      {/if}
      <div class="flex min-w-0 items-center justify-end gap-3">
        {@render actions?.()}
        {@render chrome.controls()}
      </div>
    {:else}
      {@render actions?.()}
    {/if}
  </div>
  {#if fit}
    <!-- Scrolls where the room is short anyway: a low panel window, a very small main window. -->
    <ScrollArea class="flex-1">
      <div bind:offsetHeight={contentHeight}>{@render children()}</div>
    </ScrollArea>
  {:else}
    {@render children()}
  {/if}
</section>
