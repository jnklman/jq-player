<script lang="ts">
  import { ScrollArea as ScrollAreaPrimitive } from 'bits-ui';
  import type { Snippet } from 'svelte';

  import { cn, type WithoutChild } from '$lib/utils.js';

  import Scrollbar from './scroll-area-scrollbar.svelte';

  // Root and viewport are a flex column, so the area also shrinks inside a
  // parent that only has a max-height (dialogs), where a `size-full` viewport
  // would grow with its content instead of scrolling. Styles for the scrolling
  // element itself (scroll padding, a fixed content height) go in
  // `viewportClass`; the content sits in a wrapper of bits-ui that grows to
  // the viewport's height.
  let {
    ref = $bindable(null),
    viewportRef = $bindable(null),
    class: className,
    viewportClass = '',
    orientation = 'vertical',
    scrollbarXClasses = '',
    scrollbarYClasses = '',
    scrollbarXThumb,
    children,
    ...restProps
  }: WithoutChild<ScrollAreaPrimitive.RootProps> & {
    orientation?: 'vertical' | 'horizontal' | 'both' | undefined;
    viewportClass?: string | undefined;
    scrollbarXClasses?: string | undefined;
    scrollbarYClasses?: string | undefined;
    scrollbarXThumb?: Snippet;
    viewportRef?: HTMLElement | null;
  } = $props();
</script>

<ScrollAreaPrimitive.Root
  bind:ref
  data-slot="scroll-area"
  class={cn('relative flex min-h-0 flex-col overflow-hidden', className)}
  scrollHideDelay={100}
  {...restProps}
>
  <ScrollAreaPrimitive.Viewport
    bind:ref={viewportRef}
    data-slot="scroll-area-viewport"
    class={cn('min-h-0 w-full flex-1 rounded-[inherit]', viewportClass)}
  >
    {@render children?.()}
  </ScrollAreaPrimitive.Viewport>
  {#if orientation === 'vertical' || orientation === 'both'}
    <Scrollbar orientation="vertical" class={scrollbarYClasses} />
  {/if}
  {#if orientation === 'horizontal' || orientation === 'both'}
    <Scrollbar orientation="horizontal" class={scrollbarXClasses} thumb={scrollbarXThumb} />
  {/if}
  <ScrollAreaPrimitive.Corner />
</ScrollAreaPrimitive.Root>
