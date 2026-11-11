<script lang="ts">
  import { Tooltip as TooltipPrimitive } from 'bits-ui';
  import type { ComponentProps } from 'svelte';

  import type { WithoutChildrenOrChild } from '$lib/utils.js';
  import { cn } from '$lib/utils.js';

  import TooltipPortal from './tooltip-portal.svelte';

  let {
    ref = $bindable(null),
    class: className,
    sideOffset = 6,
    side = 'top',
    children,
    portalProps,
    ...restProps
  }: TooltipPrimitive.ContentProps & {
    portalProps?: WithoutChildrenOrChild<ComponentProps<typeof TooltipPortal>>;
  } = $props();
</script>

<TooltipPortal {...portalProps}>
  <TooltipPrimitive.Content
    bind:ref
    data-slot="tooltip-content"
    {sideOffset}
    {side}
    class={cn(
      'z-50 w-fit max-w-xs origin-(--bits-tooltip-content-transform-origin) rounded-md border border-input bg-popover px-2.5 py-1.5 text-xs leading-snug font-normal wrap-break-word whitespace-pre-line text-popover-foreground shadow-xl select-none data-[side=bottom]:slide-in-from-top-2 data-[side=left]:slide-in-from-right-2 data-[side=right]:slide-in-from-left-2 data-[side=top]:slide-in-from-bottom-2 data-[state=delayed-open]:animate-in data-[state=delayed-open]:fade-in-0 data-[state=delayed-open]:zoom-in-95 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
      className,
    )}
    {...restProps}
  >
    {@render children?.()}
  </TooltipPrimitive.Content>
</TooltipPortal>
