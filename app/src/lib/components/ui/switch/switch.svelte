<script lang="ts">
  import { Switch as SwitchPrimitive } from 'bits-ui';

  import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    checked = $bindable(false),
    size = 'default',
    ...restProps
  }: WithoutChildrenOrChild<SwitchPrimitive.RootProps> & {
    size?: 'sm' | 'default';
  } = $props();
</script>

<SwitchPrimitive.Root
  bind:ref
  bind:checked
  data-slot="switch"
  data-size={size}
  class={cn(
    'peer group/switch relative inline-flex h-4 w-7 shrink-0 items-center rounded-full p-0.5 transition-[background-color] duration-200 data-checked:bg-go data-unchecked:bg-line-strong data-disabled:cursor-default data-disabled:opacity-45',
    className,
  )}
  {...restProps}
>
  <SwitchPrimitive.Thumb
    data-slot="switch-thumb"
    class={cn(
      'pointer-events-none block size-3 rounded-full transition duration-200 data-checked:translate-x-full data-checked:bg-go-fg data-unchecked:bg-muted-foreground',
    )}
  />
</SwitchPrimitive.Root>
