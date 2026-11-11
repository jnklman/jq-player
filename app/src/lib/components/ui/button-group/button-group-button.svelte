<script lang="ts">
  import { Button, type ButtonProps } from '$lib/components/ui/button/index.js';
  import { Tooltip, type TooltipProps } from '$lib/components/ui/tooltip/index.js';

  // A button of a group with its own tooltip. The tooltip trigger renders
  // the button itself (no wrapper), so the group's rounding and borders apply.
  let {
    ref = $bindable(null),
    tip,
    keys,
    side,
    tabindex,
    children,
    ...restProps
  }: ButtonProps & Pick<TooltipProps, 'keys' | 'side'> & { tip?: TooltipProps['tip'] } = $props();
</script>

{#if tip}
  <Tooltip {tip} {keys} {side}>
    {#snippet child({ props })}
      <Button bind:ref {...props} {...restProps} tabindex={tabindex ?? 0}>
        {@render children?.()}
      </Button>
    {/snippet}
  </Tooltip>
{:else}
  <Button bind:ref {tabindex} {...restProps}>
    {@render children?.()}
  </Button>
{/if}
