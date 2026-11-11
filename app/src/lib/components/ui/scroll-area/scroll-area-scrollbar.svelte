<script lang="ts">
  import { ScrollArea as ScrollAreaPrimitive } from 'bits-ui';
  import type { Snippet } from 'svelte';

  import { cn, type WithoutChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    orientation = 'vertical',
    thumb,
    children,
    ...restProps
  }: WithoutChild<ScrollAreaPrimitive.ScrollbarProps> & { thumb?: Snippet } = $props();
</script>

<ScrollAreaPrimitive.Scrollbar
  bind:ref
  data-slot="scroll-area-scrollbar"
  data-orientation={orientation}
  {orientation}
  class={cn(
    'z-2 flex touch-none p-px transition-colors select-none data-[state=hidden]:animate-out data-[state=hidden]:duration-200 data-[state=hidden]:fade-out-0 data-horizontal:h-2.5 data-horizontal:flex-col data-horizontal:border-t data-horizontal:border-t-transparent data-vertical:h-full data-vertical:w-2.5 data-vertical:border-l data-vertical:border-l-transparent',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
  {#if thumb}
    {@render thumb()}
  {:else}
    <ScrollAreaPrimitive.Thumb
      data-slot="scroll-area-thumb"
      class="relative flex-1 rounded-full bg-dim/40 transition-colors hover:bg-dim/70"
    />
  {/if}
</ScrollAreaPrimitive.Scrollbar>
