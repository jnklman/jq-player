<script lang="ts" module>
  import type { HTMLAnchorAttributes, HTMLButtonAttributes } from 'svelte/elements';
  import { tv, type VariantProps } from 'tailwind-variants';

  import { tw } from '$lib/ui';
  import { cn, type WithElementRef } from '$lib/utils.js';

  // Styled after the app's buttons. Without a variant a button is "outline",
  // the plain grey button used throughout; "default" keeps shadcn's meaning
  // (primary colour) for components added later.
  export const buttonVariants = tv({
    base: 'aria-invalid:border-destructive group/button inline-flex shrink-0 items-center justify-center gap-1.5 rounded-lg border border-transparent text-[13px] whitespace-nowrap select-none disabled:pointer-events-none disabled:opacity-45 [&_svg]:pointer-events-none [&_svg]:shrink-0',
    variants: {
      variant: {
        default: tw`bg-primary font-semibold text-primary-foreground hover:bg-primary/85`,
        outline: tw`border-input bg-secondary text-foreground hover:bg-accent aria-expanded:bg-accent`,
        secondary: tw`bg-secondary text-secondary-foreground hover:bg-accent`,
        ghost: tw`text-muted-foreground hover:bg-accent hover:text-foreground aria-expanded:bg-accent`,
        transparent: `text-muted-foreground bg-transparent hover:text-foreground`,
        destructive: tw`border-input bg-secondary text-destructive hover:bg-accent`,
        link: tw`rounded-xs text-primary underline-offset-4 hover:underline`,
        // Transport: big and coloured, sitting on a solid ledge in a darker
        // shade (`*-bg`) that the button sinks onto while pressed.
        go: tw`bg-go text-go-fg shadow-[0_4px_0_var(--color-go-bg)] transition-[translate,box-shadow] duration-100 active:translate-y-1 active:shadow-[0_0_0_var(--color-go-bg)]`,
        fade: tw`bg-fade text-fade-fg shadow-[0_4px_0_var(--color-fade-bg)] transition-[translate,box-shadow] duration-100 active:translate-y-1 active:shadow-[0_0_0_var(--color-fade-bg)]`,
        pause: tw`bg-pause text-pause-fg shadow-[0_4px_0_var(--color-pause-bg)] transition-[translate,box-shadow] duration-100 active:translate-y-1 active:shadow-[0_0_0_var(--color-pause-bg)]`,
        stop: tw`bg-stop text-stop-fg shadow-[0_4px_0_var(--color-stop-bg)] transition-[translate,box-shadow] duration-100 active:translate-y-1 active:shadow-[0_0_0_var(--color-stop-bg)]`,
      },
      size: {
        default: tw`min-h-8 px-3`,
        xs: tw`min-h-6 gap-1 rounded-md px-2 text-xs [&_svg:not([class*='size-'])]:size-3`,
        sm: tw`min-h-7 gap-1 rounded-md px-2.5 text-xs`,
        lg: tw`min-h-10 px-4`,
        icon: tw`size-8 text-base`,
        'icon-xs': tw`size-6 rounded-md text-sm`,
        'icon-sm': tw`size-7 rounded-md text-base`,
        'icon-lg': tw`size-9 text-lg`,
        transport: tw`min-h-24 flex-col gap-0.5 rounded-xl border-0 font-bold`,
      },
    },
    defaultVariants: {
      variant: 'outline',
      size: 'default',
    },
  });

  export type ButtonVariant = VariantProps<typeof buttonVariants>['variant'];
  export type ButtonSize = VariantProps<typeof buttonVariants>['size'];

  export type ButtonProps = WithElementRef<HTMLButtonAttributes> &
    WithElementRef<HTMLAnchorAttributes> & {
      variant?: ButtonVariant;
      size?: ButtonSize;
    };
</script>

<script lang="ts">
  let {
    class: className,
    variant = 'outline',
    size = 'default',
    ref = $bindable(null),
    href = undefined,
    type = 'button',
    disabled,
    children,
    ...restProps
  }: ButtonProps = $props();
</script>

{#if href}
  <a
    bind:this={ref}
    data-slot="button"
    class={cn(buttonVariants({ variant, size }), className)}
    href={disabled ? undefined : href}
    aria-disabled={disabled}
    role={disabled ? 'link' : undefined}
    tabindex={disabled ? -1 : undefined}
    {...restProps}
  >
    {@render children?.()}
  </a>
{:else}
  <button
    bind:this={ref}
    data-slot="button"
    class={cn(buttonVariants({ variant, size }), className)}
    {type}
    {disabled}
    {...restProps}
  >
    {@render children?.()}
  </button>
{/if}
