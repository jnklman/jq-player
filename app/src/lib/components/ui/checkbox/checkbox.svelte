<script lang="ts">
  import { Checkbox as CheckboxPrimitive } from 'bits-ui';

  import CheckIcon from '~icons/ph/check-bold';
  import MinusIcon from '~icons/ph/minus-bold';
  import { cn, type WithoutChildrenOrChild } from '$lib/utils.js';

  let {
    ref = $bindable(null),
    checked = $bindable(false),
    indeterminate = $bindable(false),
    class: className,
    ...restProps
  }: WithoutChildrenOrChild<CheckboxPrimitive.RootProps> = $props();
</script>

<CheckboxPrimitive.Root
  bind:ref
  data-slot="checkbox"
  class={cn(
    'peer relative flex size-4 shrink-0 items-center justify-center rounded-sm border border-input bg-background after:absolute after:-inset-x-3 after:-inset-y-2 disabled:cursor-default disabled:opacity-60 aria-invalid:border-destructive data-checked:border-primary data-checked:bg-primary data-checked:text-primary-foreground',
    className,
  )}
  bind:checked
  bind:indeterminate
  {...restProps}
>
  {#snippet children({ checked, indeterminate })}
    <div data-slot="checkbox-indicator" class="grid place-content-center text-current transition-none [&>svg]:size-3">
      {#if checked}
        <CheckIcon />
      {:else if indeterminate}
        <MinusIcon />
      {/if}
    </div>
  {/snippet}
</CheckboxPrimitive.Root>
