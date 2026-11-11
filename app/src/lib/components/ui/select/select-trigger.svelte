<script lang="ts">
  import { Select as SelectPrimitive } from 'bits-ui';

  import ChevronDownIcon from '~icons/ph/caret-down-bold';
  import { cn, type WithoutChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    children,
    size = 'default',
    ...restProps
  }: WithoutChild<SelectPrimitive.TriggerProps> & {
    size?: 'sm' | 'default';
  } = $props();
</script>

<SelectPrimitive.Trigger
  bind:ref
  data-slot="select-trigger"
  data-size={size}
  class={cn(
    'flex min-h-8 w-full min-w-0 items-center justify-between gap-1.5 rounded-lg border border-input bg-background pr-2 pl-2.5 text-left text-sm whitespace-nowrap text-foreground select-none disabled:cursor-default disabled:text-fg-2 aria-invalid:border-destructive data-placeholder:text-muted-foreground data-[size=sm]:min-h-7 *:data-[slot=select-value]:line-clamp-1 [&_svg]:pointer-events-none [&_svg]:shrink-0',
    className,
  )}
  {...restProps}
>
  {@render children?.()}
  <ChevronDownIcon class="pointer-events-none size-3.5 text-muted-foreground" />
</SelectPrimitive.Trigger>
