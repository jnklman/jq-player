<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';

  import { cn, type WithElementRef, type WithoutChildren } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    value = $bindable(),
    class: className,
    'data-slot': dataSlot = 'textarea',
    ...restProps
  }: WithoutChildren<WithElementRef<HTMLTextareaAttributes>> = $props();
</script>

<textarea
  bind:this={ref}
  data-slot={dataSlot}
  class={cn(
    'min-h-16 w-full min-w-0 resize-y rounded-lg border border-input bg-background px-2.5 py-2 text-sm text-foreground select-text placeholder:text-dim disabled:text-fg-2 aria-invalid:border-destructive',
    className,
  )}
  bind:value
  {...restProps}></textarea>
