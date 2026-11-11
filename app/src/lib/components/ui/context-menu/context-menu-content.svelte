<script lang="ts">
  import { ContextMenu as ContextMenuPrimitive } from 'bits-ui';
  import type { ComponentProps } from 'svelte';

  import type { WithoutChildrenOrChild } from '$lib/utils.js';
  import { cn } from '$lib/utils.js';

  import ContextMenuPortal from './context-menu-portal.svelte';

  let {
    ref = $bindable(null),
    portalProps,
    class: className,
    ...restProps
  }: ContextMenuPrimitive.ContentProps & {
    portalProps?: WithoutChildrenOrChild<ComponentProps<typeof ContextMenuPortal>>;
  } = $props();
</script>

<ContextMenuPortal {...portalProps}>
  <ContextMenuPrimitive.Content
    bind:ref
    data-slot="context-menu-content"
    class={cn(
      'z-50 max-h-(--bits-context-menu-content-available-height) min-w-40 origin-(--bits-context-menu-content-transform-origin) overflow-x-hidden overflow-y-auto rounded-lg border border-input bg-popover p-1 text-popover-foreground shadow-2xl duration-100 data-open:animate-in data-open:fade-in-0 data-open:zoom-in-95 data-closed:animate-out data-closed:overflow-hidden data-closed:fade-out-0 data-closed:zoom-out-95',
      className,
    )}
    {...restProps}
  />
</ContextMenuPortal>
