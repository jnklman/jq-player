<script lang="ts" module>
  import { tv, type VariantProps } from 'tailwind-variants';

  export const toggleVariants = tv({
    base: 'text-muted-foreground hover:text-foreground data-[state=on]:bg-mode data-[state=on]:text-foreground inline-flex items-center justify-center gap-1 rounded-lg text-[13px] whitespace-nowrap focus-visible:-outline-offset-2 data-[state=on]:font-semibold disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:shrink-0',
    variants: {
      variant: {
        default: 'bg-transparent',
        outline: 'border-input border bg-transparent',
      },
      size: {
        default: 'min-h-8 min-w-8 px-3.5',
        sm: 'min-h-7 min-w-7 px-2.5 text-xs',
        lg: 'min-h-9 min-w-9 px-4',
      },
    },
    defaultVariants: {
      variant: 'default',
      size: 'default',
    },
  });

  export type ToggleVariant = VariantProps<typeof toggleVariants>['variant'];
  export type ToggleSize = VariantProps<typeof toggleVariants>['size'];
  export type ToggleVariants = VariantProps<typeof toggleVariants>;
</script>

<script lang="ts">
  import { Toggle as TogglePrimitive } from 'bits-ui';

  import { cn } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    pressed = $bindable(false),
    class: className,
    size = 'default',
    variant = 'default',
    ...restProps
  }: TogglePrimitive.RootProps & {
    variant?: ToggleVariant;
    size?: ToggleSize;
  } = $props();
</script>

<TogglePrimitive.Root
  bind:ref
  bind:pressed
  data-slot="toggle"
  class={cn(toggleVariants({ variant, size }), className)}
  {...restProps}
/>
