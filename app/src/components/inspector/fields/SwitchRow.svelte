<script lang="ts">
  import type { Snippet } from 'svelte';

  import { Label } from '$lib/components/ui/label';
  import { Switch } from '$lib/components/ui/switch';

  import { type CueEdit, isMixed, type Mixed } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import FieldRow from '../FieldRow.svelte';

  // A yes/no field of the edited cues as an inspector row: a switch with a sentence. With `key` it reads and
  // sets that field; else `checked` and `onchange` do.
  let {
    edit,
    key,
    checked,
    onchange,
    label,
    text,
    own = false,
    sub = false,
    field,
    children,
  }: {
    edit: CueEdit;
    key?: string;
    checked?: boolean | Mixed;
    onchange?: (on: boolean) => void;
    label: string;
    /** What "on" means, next to the switch. */
    text: string;
    own?: boolean;
    sub?: boolean;
    field?: string;
    children?: Snippet;
  } = $props();

  const value = $derived(key ? edit.value<boolean>(key) : checked);
  const mixed = $derived(isMixed(value));
</script>

<FieldRow {label} {own} {sub} {field}>
  <Label class="min-h-8 font-normal text-fg-2">
    <Switch
      aria-label={label}
      checked={value === true}
      onCheckedChange={(on) => (onchange ? onchange(on) : edit.set({ [key!]: on }))}
    />
    {text}
    {#if mixed}<span class="text-xs text-muted-foreground">({t('field.mixed')})</span>{/if}
  </Label>
  {@render children?.()}
</FieldRow>
