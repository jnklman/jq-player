<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';

  import { type ToggleVariants, toggleVariants } from '$lib/components/ui/toggle/index.js';
  import { cn } from '$lib/utils.js';

  import { getToggleGroupCtx } from './toggle-group.svelte';

  let {
    ref = $bindable(null),
    value = $bindable(),
    class: className,
    size,
    variant,
    children: content,
    ...restProps
  }: ToggleGroupPrimitive.ItemProps & ToggleVariants = $props();

  const ctx = getToggleGroupCtx();
</script>

<ToggleGroupPrimitive.Item
  bind:ref
  data-slot="toggle-group-item"
  data-variant={ctx.variant || variant}
  data-size={ctx.size || size}
  data-spacing={ctx.spacing}
  class={cn(
    'relative shrink-0 group-data-[spacing=0]/toggle-group:rounded-none group-data-[spacing=0]/toggle-group:border-0 focus:z-10 focus-visible:z-10 group-data-sliding/toggle-group:data-[state=on]:bg-transparent',
    toggleVariants({
      variant: ctx.variant || variant,
      size: ctx.size || size,
    }),
    className,
  )}
  {value}
  {...restProps}
>
  <!-- An invisible bold copy reserves the width of the active (semibold) label, so toggling does not shift
       the layout. -->
  {#snippet children(state)}
    <span class="grid">
      <span
        aria-hidden="true"
        class="invisible col-start-1 row-start-1 inline-flex items-center justify-center gap-1 font-semibold"
      >
        {@render content?.(state)}
      </span>
      <span class="col-start-1 row-start-1 inline-flex items-center justify-center gap-1">
        {@render content?.(state)}
      </span>
    </span>
  {/snippet}
</ToggleGroupPrimitive.Item>
