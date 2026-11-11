<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import type { ComponentProps } from 'svelte';

  import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

  import ContextMenuPortal from './context-menu-portal.svelte';

  let {
    ref = $bindable(null),
    class: className,
    align = 'start',
    alignOffset = -3,
    portalProps,
    ...restProps
  }: ContextMenuPrimitive.SubContentProps & {
    portalProps?: WithoutChildrenOrChild<ComponentProps<typeof ContextMenuPortal>>;
  } = $props();
</script>

<ContextMenuPortal {...portalProps}>
  <ContextMenuPrimitive.SubContent
    bind:ref
    data-slot="context-menu-sub-content"
    {align}
    {alignOffset}
    class={cn(
      'z-50 min-w-40 origin-(--bits-context-menu-content-transform-origin) overflow-hidden rounded-lg border border-input bg-popover p-1 text-popover-foreground shadow-2xl duration-100 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:fade-out-0 data-closed:zoom-out-95',
      className,
    )}
    {...restProps}
  />
</ContextMenuPortal>
