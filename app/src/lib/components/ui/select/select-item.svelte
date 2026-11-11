<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';

  import CheckIcon from '~icons/ph/check-bold';
  import { cn, type WithoutChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    value,
    label,
    children: childrenProp,
    ...restProps
  }: WithoutChild<SelectPrimitive.ItemProps> = $props();
</script>

<SelectPrimitive.Item
  bind:ref
  {value}
  {label}
  data-slot="select-item"
  class={cn(
    'relative flex w-full cursor-default items-center gap-1.5 rounded-md py-1.5 pr-8 pl-3 text-[13px] outline-hidden select-none data-highlighted:bg-accent data-highlighted:text-accent-foreground data-disabled:pointer-events-none data-disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:shrink-0',
    className,
  )}
  {...restProps}
>
  {#snippet children({ selected, highlighted })}
    <span class="pointer-events-none absolute right-2 flex size-4 items-center justify-center">
      {#if selected}
        <CheckIcon class="pointer-events-none size-3.5 text-primary" />
      {/if}
    </span>
    <span class="flex min-w-0 flex-1 gap-2 truncate">
      {#if childrenProp}
        {@render childrenProp({ selected, highlighted })}
      {:else}
        {label || value}
      {/if}
    </span>
  {/snippet}
</SelectPrimitive.Item>
