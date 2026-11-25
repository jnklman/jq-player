<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import type { LibraryPreset, ParamValue } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { localized, paramProblem, type ParamProblemKind } from '../../lib/library.svelte';
  import { field, hint } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';

  // The parameters of a library preset as a form: numbers, texts and choices, each checked as the backend checks it.
  let {
    preset,
    values = $bindable(),
    disabled = false,
    onsubmit,
  }: {
    preset: LibraryPreset;
    values: Record<string, ParamValue>;
    disabled?: boolean;
    /** Enter in a text field. */
    onsubmit?: () => void;
  } = $props();

  // A new object each time, so a parent holding the values in a `$derived` sees the change.
  function set(id: string, value: ParamValue) {
    values = { ...values, [id]: value };
  }

  function problemText(kind: ParamProblemKind, param: LibraryPreset['params'][number]): string {
    switch (kind) {
      case 'number':
        return t('library.param.number');
      case 'range':
        return param.type === 'int' || param.type === 'float'
          ? t('library.param.range', { min: param.min, max: param.max })
          : '';
      case 'required':
        return t('library.param.required');
      case 'too_long':
        return param.type === 'text' ? t('library.param.too_long', { max: param.max_len ?? 0 }) : '';
      case 'pattern':
        return t('library.param.pattern');
      case 'choice':
        return t('library.param.choice');
    }
  }
</script>

{#if preset.params.length}
  <div class="grid grid-cols-2 gap-x-3 gap-y-2.5">
    {#each preset.params as param (param.id)}
      {@const label = localized(param.label)}
      {@const problem = paramProblem(param, values[param.id])}
      <div class={['flex flex-col gap-1', param.type === 'text' && 'col-span-2']}>
        {#if param.type === 'int' || param.type === 'float'}
          <NumberField
            {label}
            {disabled}
            value={typeof values[param.id] === 'number' ? (values[param.id] as number) : null}
            min={param.min}
            max={param.max}
            integer={param.type === 'int'}
            step={param.type === 'float' ? (param.step ?? 0.1) : 1}
            unit={param.unit}
            onchange={(v) => set(param.id, v)}
          />
        {:else if param.type === 'text'}
          <Label class={field}>
            {label}
            <Input
              {disabled}
              value={typeof values[param.id] === 'string' ? (values[param.id] as string) : ''}
              maxlength={param.max_len}
              aria-invalid={problem ? 'true' : undefined}
              oninput={(e) => set(param.id, e.currentTarget.value)}
              onkeydown={(e) => {
                if (e.key === 'Enter' && !problem) onsubmit?.();
              }}
            />
          </Label>
        {:else}
          {@const choices = param.values.map((choice, i) => ({ value: String(i), label: localized(choice.label) }))}
          <Label class={field}>
            {label}
            <SimpleSelect
              {disabled}
              aria-label={label}
              options={choices}
              value={String(param.values.findIndex((choice) => choice.value === values[param.id]))}
              onchange={(i) => set(param.id, param.values[Number(i)].value)}
            />
          </Label>
        {/if}
        {#if problem}
          <span class="{hint} text-stop-soft">{problemText(problem, param)}</span>
        {/if}
      </div>
    {/each}
  </div>
{:else}
  <p class="{hint} m-0">{t('library.no_params')}</p>
{/if}
