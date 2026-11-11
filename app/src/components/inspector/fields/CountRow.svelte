<script lang="ts">
  import type { Snippet } from 'svelte';

  import * as ToggleGroup from '$lib/components/ui/toggle-group';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Repeat } from '../../../lib/api';
  import { type CueEdit, isMixed } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import { type CountKind, countKind, countOf, type Option } from '../../../lib/options';
  import NumberField from '../../fields/NumberField.svelte';
  import FieldRow from '../FieldRow.svelte';

  // A count of the edited cues (loop passes, repeat GOs): off, a number or endless, as segments with the
  // number next to them.
  let {
    edit,
    key,
    label,
    options,
    unit,
    disabled = false,
    tip,
    own = false,
    field,
    children,
  }: {
    edit: CueEdit;
    key: 'loop' | 'passes';
    label: string;
    options: Option<CountKind>[];
    /** Unit of the number ("×", "GOs"). */
    unit: string;
    disabled?: boolean;
    tip?: string;
    own?: boolean;
    field?: string;
    children?: Snippet;
  } = $props();

  const value = $derived(edit.value<Repeat>(key));
  const kinds = $derived(
    new Set(edit.cues.map((cue) => countKind((cue as unknown as Record<string, Repeat | undefined>)[key] ?? 0))),
  );
  const kind = $derived(kinds.size === 1 ? [...kinds][0] : '');
</script>

<FieldRow {label} {own} {field}>
  <span class="flex flex-wrap items-center gap-2">
    <Tooltip {tip}>
      {#snippet child({ props })}
        <span {...props} class="inline-flex">
          <ToggleGroup.Root
            type="single"
            variant="outline"
            size="sm"
            aria-label={label}
            {disabled}
            bind:value={
              () => kind,
              (next) =>
                next &&
                edit.set((cue) => ({
                  [key]: countOf(next as CountKind, (cue as unknown as Record<string, Repeat>)[key]),
                }))
            }
          >
            {#each options as option (option.value)}
              <ToggleGroup.Item value={option.value}>{option.label}</ToggleGroup.Item>
            {/each}
          </ToggleGroup.Root>
        </span>
      {/snippet}
    </Tooltip>
    {#if kind === 'times' && !disabled}
      <NumberField
        class="w-28"
        aria-label={t('count.number', { label })}
        {unit}
        min={2}
        step={1}
        integer
        optional={isMixed(value)}
        placeholder={isMixed(value) ? t('field.mixed') : ''}
        value={typeof value === 'number' ? value : null}
        onchange={(count) => count !== null && edit.set({ [key]: count })}
      />
    {/if}
  </span>
  {@render children?.()}
</FieldRow>
