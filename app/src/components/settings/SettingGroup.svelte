<script lang="ts">
  import type { Snippet } from 'svelte';

  import { provideSettingsGroup, settingsView } from '../../lib/settingsView.svelte';

  // A titled group of setting rows in a tab; hidden while a search finds none of its rows.
  let {
    title = '',
    badge,
    aside,
    children,
  }: {
    title?: string;
    /** Next to the title, e.g. "This show". */
    badge?: Snippet;
    /** Right of the title: a note or fields that belong to the whole group. */
    aside?: Snippet;
    children: Snippet;
  } = $props();

  const view = settingsView();
  const id = provideSettingsGroup(() => title);
</script>

<section hidden={!view.showsGroup(id)} class="flex flex-col">
  {#if title}
    <div
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line pb-1.5 text-xs font-semibold text-fg-2"
    >
      {title}
      {@render badge?.()}
      {#if aside}
        <div class="ml-auto flex flex-wrap items-center gap-3 font-normal text-muted-foreground">{@render aside()}</div>
      {/if}
    </div>
  {/if}
  {@render children()}
</section>
