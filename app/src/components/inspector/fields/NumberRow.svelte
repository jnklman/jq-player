<script lang="ts">
  import type { Snippet } from 'svelte';

  import { type CueEdit, isMixed, shown } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import NumberField from '../../fields/NumberField.svelte';
  import FieldRow from '../FieldRow.svelte';

  // A number field of the edited cues as an inspector row; "mixed" when they differ.
  let {
    edit,
    key,
    label,
    unit,
    step = 0.1,
    min,
    max,
    integer = false,
    optional = false,
    placeholder = '',
    width = 'w-28',
    own = false,
    sub = false,
    field,
    children,
  }: {
    edit: CueEdit;
    key: string;
    label: string;
    unit?: string;
    step?: number;
    min?: number;
    max?: number;
    integer?: boolean;
    /** Empty means null (the placeholder says what that stands for). */
    optional?: boolean;
    placeholder?: string;
    width?: string;
    own?: boolean;
    sub?: boolean;
    field?: string;
    /** Hints below the field. */
    children?: Snippet;
  } = $props();

  const value = $derived(edit.value<number | null>(key));
</script>

<FieldRow {label} {own} {sub} {field}>
  <NumberField
    class={width}
    aria-label={label}
    {unit}
    {step}
    {min}
    {max}
    {integer}
    optional={optional || isMixed(value)}
    placeholder={isMixed(value) ? t('field.mixed') : placeholder}
    value={shown(value)}
    onchange={(number) => {
      // Mixed fields left empty keep each cue's value.
      if (number !== null || optional) edit.set({ [key]: number });
    }}
  />
  {@render children?.()}
</FieldRow>
