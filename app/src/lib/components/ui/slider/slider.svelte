<script lang="ts">
  import { Slider as SliderPrimitive } from 'bits-ui';

  import { Tooltip } from '$lib/components/ui/tooltip';
  import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    value = $bindable(),
    orientation = 'horizontal',
    class: className,
    format = (value: number) => value.toLocaleString(),
    reset,
    ...restProps
  }: WithoutChildrenOrChild<SliderPrimitive.RootProps> & {
    /** Tooltip text for a thumb's value. */
    format?: (value: number) => string;
    /** Value a double click puts a single slider back to. */
    reset?: number;
  } = $props();

  let dragging = $state(false);
  let hovering = $state(false);

  // Double click resets. The second press must not reach bits-ui (it listens on the
  // document): it would move the thumb to the pointer and commit that value before the
  // reset, and the show events of both commits make the thumb jump back and forth.
  const DOUBLE_CLICK_MS = 500;
  const DOUBLE_CLICK_PX = 6;
  $effect(() => {
    const node = ref;
    if (!node || reset === undefined) return;
    const resetTo = reset;
    let last: { time: number; x: number; y: number } | null = null;
    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || restProps.disabled) return;
      const second =
        last !== null &&
        e.timeStamp - last.time < DOUBLE_CLICK_MS &&
        Math.hypot(e.clientX - last.x, e.clientY - last.y) < DOUBLE_CLICK_PX;
      last = second ? null : { time: e.timeStamp, x: e.clientX, y: e.clientY };
      if (!second) return;
      e.stopPropagation();
      e.preventDefault();
      value = resetTo as never;
      const single = restProps as { onValueChange?: (value: number) => void; onValueCommit?: (value: number) => void };
      single.onValueChange?.(resetTo);
      single.onValueCommit?.(resetTo);
    };
    node.addEventListener('pointerdown', onDown, { capture: true });
    return () => node.removeEventListener('pointerdown', onDown, { capture: true });
  });
</script>

<svelte:window onpointerup={() => (dragging = false)} onpointercancel={() => (dragging = false)} />

<!--
Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<SliderPrimitive.Root
  bind:ref
  bind:value={value as never}
  data-slot="slider"
  {orientation}
  class={cn(
    'relative flex min-h-8 w-full touch-none items-center select-none data-disabled:opacity-50 data-vertical:h-full data-vertical:min-h-40 data-vertical:w-auto data-vertical:flex-col',
    className,
  )}
  onpointerdown={() => (dragging = true)}
  onpointerenter={() => (hovering = true)}
  onpointerleave={() => (hovering = false)}
  {...restProps}
>
  {#snippet children({ thumbItems })}
    <span
      data-slot="slider-track"
      data-orientation={orientation}
      class={cn(
        'relative grow overflow-hidden rounded-full bg-muted data-horizontal:h-1 data-horizontal:w-full data-vertical:h-full data-vertical:w-1',
      )}
    >
      <SliderPrimitive.Range
        data-slot="slider-range"
        class={cn('absolute bg-primary select-none data-horizontal:h-full data-vertical:w-full')}
      />
    </span>
    {#each thumbItems as thumb (thumb.index)}
      <Tooltip
        tip={(() => {
          const v = Array.isArray(value) ? value[thumb.index] : value;
          return v === undefined ? undefined : format(v);
        })()}
        side="top"
        keepOpen={dragging || hovering}
        data-slot="slider-thumb"
      >
        {#snippet child({ props: tipProps })}
          <SliderPrimitive.Thumb
            {...tipProps}
            index={thumb.index}
            class="relative block size-3 shrink-0 rounded-full bg-fg transition-transform select-none after:absolute after:-inset-2 active:scale-115 disabled:pointer-events-none disabled:opacity-50"
          />
        {/snippet}
      </Tooltip>
    {/each}
  {/snippet}
</SliderPrimitive.Root>
