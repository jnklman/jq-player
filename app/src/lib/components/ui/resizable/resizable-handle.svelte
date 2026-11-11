<script lang="ts">
  import * as ResizablePrimitive from 'paneforge';

  import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    class: className,
    withHandle = false,
    ...restProps
  }: WithoutChildrenOrChild<ResizablePrimitive.PaneResizerProps> & {
    withHandle?: boolean;
  } = $props();
</script>

<ResizablePrimitive.PaneResizer
  bind:ref
  data-slot="resizable-handle"
  class={cn(
    'group/handle relative flex w-4 shrink-0 items-center justify-center bg-transparent outline-none after:absolute after:inset-y-2 after:left-1/2 after:w-0.5 after:-translate-x-1/2 after:rounded-full after:bg-transparent after:transition-colors hover:after:bg-line-strong focus-visible:after:bg-ring data-[active=pointer]:after:bg-primary data-[direction=vertical]:h-4 data-[direction=vertical]:w-full data-[direction=vertical]:after:inset-x-2 data-[direction=vertical]:after:inset-y-auto data-[direction=vertical]:after:top-1/2 data-[direction=vertical]:after:h-0.5 data-[direction=vertical]:after:w-auto data-[direction=vertical]:after:translate-x-0 data-[direction=vertical]:after:-translate-y-1/2',
    className,
  )}
  {...restProps}
>
  {#if withHandle}
    <div class="z-10 flex h-6 w-1 shrink-0 rounded-lg bg-border"></div>
  {/if}
</ResizablePrimitive.PaneResizer>
