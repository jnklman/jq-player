<script lang="ts" generics="T extends string">
  import type { Snippet } from 'svelte';

  import { type CueEdit, isMixed } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import type { Option } from '../../../lib/options';
  import SimpleSelect from '../../SimpleSelect.svelte';
  import FieldRow from '../FieldRow.svelte';

  // A choice of the edited cues as an inspector row; "mixed" when they differ.
  let {
    edit,
    key,
    label,
    options,
    own = false,
    sub = false,
    field,
    onchange,
    children,
  }: {
    edit: CueEdit;
    key: string;
    label: string;
    options: Option<T>[];
    own?: boolean;
    sub?: boolean;
    field?: string;
    /** Instead of setting `key` on every cue. */
    onchange?: (value: T) => void;
    children?: Snippet;
  } = $props();

  const value = $derived(edit.value<T>(key));
</script>

<FieldRow {label} {own} {sub} {field}>
  <SimpleSelect
    aria-label={label}
    value={isMixed(value) ? ('' as T) : value}
    {options}
    placeholder={t('field.mixed')}
    onchange={(next) => (onchange ? onchange(next) : edit.set({ [key]: next }))}
  />
  {@render children?.()}
</FieldRow>
