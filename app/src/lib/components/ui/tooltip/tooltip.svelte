<script lang="ts" module>
  import { Tooltip as TooltipPrimitive } from 'bits-ui';

  export type TooltipProps = TooltipPrimitive.TriggerProps & {
    /** Tooltip text; `\n` breaks the line. Empty: no tooltip. */
    tip: string | null | undefined;
    /** Shortcut shown after the text as a key, e.g. "Strg+S". */
    keys?: string;
    side?: 'top' | 'right' | 'bottom' | 'left';
    /** Keeps the tooltip open regardless of pointer and focus. */
    keepOpen?: boolean;
  };
</script>

<script lang="ts">
  import { Kbd } from '../kbd';
  import TooltipContent from './tooltip-content.svelte';
  import TooltipTrigger from './tooltip-trigger.svelte';

  // Tooltips instead of `title`, so they look the same in every webview.
  // By default the trigger is a <button> taking the props (style it with
  // `buttonVariants`); for any other element pass a `child` snippet and
  // spread its `props` onto that element:
  //   <Tooltip tip="…">{#snippet child({ props })}<span {...props}>…</span>{/snippet}</Tooltip>
  // A disabled button gets no pointer events; wrap it in a span to explain why
  // it is disabled. Such wrappers are no tab stops of their own: without `tabindex` the trigger's tabindex is
  // dropped from the props handed to `child`, so a real button or menu trigger stays reachable with Tab and a
  // span stays out of the tab order. Pass `tabindex={0}` to make a wrapper focusable on purpose.
  let { tip, keys, side = 'top', children, child, tabindex, keepOpen = false, ...restProps }: TooltipProps = $props();
  let open = $state(false);
</script>

<!-- The trigger's props say type="button", which would turn an <input> into a button. -->
{#snippet untyped({ props }: { props: Record<string, unknown> })}
  {@render child?.({ props: { ...props, type: undefined, ...(tabindex === undefined && { tabindex: undefined }) } })}
{/snippet}

<TooltipPrimitive.Root bind:open={() => open || keepOpen, (v) => (open = v)}>
  <TooltipTrigger {tabindex} child={child && untyped} {...restProps}>
    {@render children?.()}
  </TooltipTrigger>
  {#if tip}
    <TooltipContent {side} class="text-center">
      {tip}
      {#if keys}<Kbd class="ml-1.5">{keys}</Kbd>{/if}
    </TooltipContent>
  {/if}
</TooltipPrimitive.Root>
