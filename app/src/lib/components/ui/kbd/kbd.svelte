<script lang="ts">
  import type { HTMLAttributes } from 'svelte/elements';

  import { cn, type WithElementRef } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLAttributes<HTMLElement>> = $props();
</script>

<kbd
  bind:this={ref}
  data-slot="kbd"
  class={cn(
    'pointer-events-none inline-flex w-fit items-center justify-center gap-1 rounded border border-input px-1.5 py-px font-mono text-[11px] leading-normal text-foreground select-none in-data-[slot=tooltip-content]:text-fg-2',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</kbd>
