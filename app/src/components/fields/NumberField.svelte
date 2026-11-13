<script lang="ts" generics="Optional extends boolean = false">
  import type { Snippet } from 'svelte';

  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { clamp, parseNumber, plainNumber } from '../../lib/input';
  import { field } from '../../lib/ui';

  // A number input with its label (a grid cell of a form). A change is
  // clamped to `min`/`max` (rounded with `integer`) and sent through
  // `onchange`; an empty field sends null if `optional`, else (as anything
  // that is no number) puts the value back. Without `label`, just the input.
  type Props = {
    label?: string | Snippet;
    value: number | null | undefined;
    step?: number | string;
    min?: number;
    max?: number;
    integer?: boolean;
    placeholder?: string;
    /** Unit shown inside the field, right of the number ("s", "dB"). */
    unit?: string;
    /** Tooltip on the input. */
    tip?: string;
    disabled?: boolean;
    /** Classes of the label (the grid cell), e.g. `col-span-2`, `w-20`; without a label, of the input. */
    class?: string;
    id?: string;
    'aria-label'?: string;
    /** An empty field sends null instead of keeping the value. */
    optional?: Optional;
    onchange: (value: Optional extends true ? number | null : number) => void;
  };

  let {
    label,
    value,
    step = 0.1,
    min,
    max,
    integer = false,
    optional,
    placeholder,
    unit,
    tip,
    disabled,
    class: className = '',
    id,
    'aria-label': ariaLabel,
    onchange,
  }: Props = $props();

  function change(e: Event & { currentTarget: HTMLInputElement }) {
    const input = e.currentTarget;
    const parsed = parseNumber(input.value);
    if (parsed === null) {
      // An unreadable entry ("1-2") reads as empty from a number input: that is no request to clear the value.
      if (optional && !input.validity.badInput && input.value.trim() === '')
        return (onchange as (value: number | null) => void)(null);
      // Not a number: the field shows the value again.
      input.value = value == null ? '' : plainNumber(value);
      return;
    }
    const next = clamp(integer ? Math.round(parsed) : parsed, min, max);
    // A clamped value the field still shows unclamped would stay wrong.
    if (next !== parsed) input.value = plainNumber(next);
    (onchange as (value: number) => void)(next);
  }
</script>

{#snippet input()}
  {#if tip}
    <Tooltip {tip} tabindex={0}>
      {#snippet child({ props })}
        {@render numberInput(props)}
      {/snippet}
    </Tooltip>
  {:else}
    {@render numberInput({})}
  {/if}
{/snippet}

{#snippet numberInput(props: Record<string, unknown>)}
  <Input
    {...props}
    {id}
    class={[unit && 'font-mono text-[13px] tabular-nums', label === undefined && className]}
    type="number"
    {unit}
    {step}
    {min}
    {max}
    {placeholder}
    {disabled}
    aria-label={ariaLabel}
    value={value == null ? '' : plainNumber(value)}
    onchange={change}
  />
{/snippet}

{#if label === undefined}
  {@render input()}
{:else}
  <Label class={[field, className]}>
    {#if typeof label === 'string'}{label}{:else}{@render label()}{/if}
    {@render input()}
  </Label>
{/if}
