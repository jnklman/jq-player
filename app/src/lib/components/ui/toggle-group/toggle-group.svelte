<script lang="ts" module>
  import { getContext, setContext, untrack } from 'svelte';
  import type { VariantProps } from 'tailwind-variants';

  import { toggleVariants } from '$lib/components/ui/toggle/index.js';

  type ToggleVariants = VariantProps<typeof toggleVariants>;

  interface ToggleGroupContext extends ToggleVariants {
    spacing?: number;
    orientation?: 'horizontal' | 'vertical';
  }

  export function setToggleGroupCtx(props: ToggleGroupContext) {
    setContext('toggleGroup', props);
  }

  export function getToggleGroupCtx() {
    return getContext<Required<ToggleGroupContext>>('toggleGroup');
  }
</script>

<script lang="ts">
  import { ToggleGroup as ToggleGroupPrimitive } from 'bits-ui';

  import { cn } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    value = $bindable(),
    class: className,
    size = 'default',
    spacing = 0,
    orientation = 'horizontal',
    variant = 'default',
    children,
    ...restProps
  }: ToggleGroupPrimitive.RootProps &
    ToggleVariants & {
      spacing?: number;
      orientation?: 'horizontal' | 'vertical';
    } = $props();

  setToggleGroupCtx({
    get variant() {
      return variant;
    },
    get size() {
      return size;
    },
    get spacing() {
      return spacing;
    },
    get orientation() {
      return orientation;
    },
  });

  // Single-select groups draw the active highlight as one element that slides between items.
  let indicator = $state<{ left: number; top: number; width: number; height: number }>();
  let animate = $state(false);
  const sliding = $derived(restProps.type === 'single');

  $effect(() => {
    if (!sliding || !ref) return;
    const root = ref;
    void value;
    const measure = () => {
      const item = root.querySelector<HTMLElement>('[data-slot=toggle-group-item][data-state=on]');
      // A hidden group (e.g. in a tab not shown) has no size yet.
      if (!item || item.offsetWidth === 0) {
        indicator = undefined;
        animate = false;
        return;
      }
      // Read untracked: the effect writes `indicator` itself.
      const first = untrack(() => indicator === undefined);
      indicator = { left: item.offsetLeft, top: item.offsetTop, width: item.offsetWidth, height: item.offsetHeight };
      // The first placement does not slide in (on mount, or when the group is shown); later moves do.
      if (first) requestAnimationFrame(() => (animate = indicator !== undefined));
    };
    measure();
    const observer = new ResizeObserver(measure);
    for (const item of root.children) observer.observe(item);
    return () => observer.disconnect();
  });
</script>

<!--
Discriminated Unions + Destructing (required for bindable) do not
get along, so we shut typescript up by casting `value` to `never`.
-->
<ToggleGroupPrimitive.Root
  bind:value={value as never}
  bind:ref
  {orientation}
  data-slot="toggle-group"
  data-variant={variant}
  data-size={size}
  data-spacing={spacing}
  data-sliding={sliding ? '' : undefined}
  style={`--gap: ${spacing}`}
  class={cn(
    'group/toggle-group relative flex w-fit flex-row items-center gap-[--spacing(var(--gap))] rounded-lg data-[variant=outline]:data-[spacing=0]:overflow-hidden data-[variant=outline]:data-[spacing=0]:border data-[variant=outline]:data-[spacing=0]:border-input data-vertical:flex-col data-vertical:items-stretch',
    className,
  )}
  {...restProps}
>
  {#if indicator}
    <span
      aria-hidden="true"
      class={[
        'pointer-events-none absolute rounded-lg bg-mode group-data-[spacing=0]/toggle-group:rounded-none motion-reduce:transition-none',
        animate && 'transition-[left,top,width,height] duration-200 ease-out',
      ]}
      style:left="{indicator.left}px"
      style:top="{indicator.top}px"
      style:width="{indicator.width}px"
      style:height="{indicator.height}px"
    ></span>
  {/if}
  {@render children?.()}
</ToggleGroupPrimitive.Root>
