<script lang="ts">
  import type { Snippet } from 'svelte';

  import OwnMark from './OwnMark.svelte';

  // One field of the inspector: its label left, the control (with hints below it) right; in a narrow panel
  // the label goes on top. `sub` indents a field that depends on the one above it. Without a label the
  // control takes the whole row.
  let {
    label,
    own = false,
    sub = false,
    field,
    children,
  }: {
    label?: string;
    /** A linked cue keeps this field for itself (see `OwnMark`). */
    own?: boolean;
    sub?: boolean;
    /** `data-field`, for revealing the row from a notice. */
    field?: string;
    children: Snippet;
  } = $props();

  const id = $props.id();
</script>

<div
  role="group"
  aria-labelledby={label ? id : undefined}
  data-field={field}
  class={[
    'grid items-start gap-x-3 gap-y-1 border-b border-row-line py-2 last:border-b-0',
    label === undefined
      ? 'grid-cols-1'
      : sub
        ? 'grid-cols-[calc(7rem-0.875rem)_minmax(0,1fr)] pl-3.5 @max-[22rem]:grid-cols-1'
        : 'grid-cols-[7rem_minmax(0,1fr)] @max-[22rem]:grid-cols-1',
  ]}
>
  {#if label !== undefined}
    <span {id} class="pt-1.5 text-[13px] text-fg-2 @max-[22rem]:pt-0">
      {#if own}<OwnMark {label} />{:else}{label}{/if}
    </span>
  {/if}
  <div class="flex min-w-0 flex-col gap-1.5">{@render children()}</div>
</div>
