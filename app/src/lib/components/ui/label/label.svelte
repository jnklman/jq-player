<script lang="ts">
  import type { HTMLLabelAttributes } from 'svelte/elements';

  import { cn, type WithElementRef } from '$lib/utils.js';

  // A plain <label> instead of bits-ui's Label: that one prevents the default
  // of every double-click (to keep the label text from being selected), also
  // on a field inside it, whose word then can't be selected. The text here is
  // `select-none` anyway.
  let {
    ref = $bindable(null),
    class: className,
    children,
    ...restProps
  }: WithElementRef<HTMLLabelAttributes> = $props();
</script>

<label
  bind:this={ref}
  data-slot="label"
  class={cn(
    'flex items-center gap-2 text-[13px] select-none group-data-[disabled=true]:pointer-events-none peer-disabled:cursor-default',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
</label>
