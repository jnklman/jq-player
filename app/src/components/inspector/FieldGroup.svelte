<script lang="ts">
  import type { Snippet } from 'svelte';

  // A titled group of rows in an inspector tab, in the look of the settings' groups.
  let { title, aside, children }: { title?: string; aside?: Snippet; children: Snippet } = $props();
</script>

<section class="mt-4 flex flex-col first:mt-3">
  {#if title}
    <div
      class="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line pb-1.5 text-xs font-semibold text-fg-2"
    >
      {title}
      {#if aside}
        <div class="ml-auto flex flex-wrap items-center gap-2 font-normal text-muted-foreground">{@render aside()}</div>
      {/if}
    </div>
  {/if}
  {@render children()}
</section>
