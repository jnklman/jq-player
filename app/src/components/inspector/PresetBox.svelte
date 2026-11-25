<script lang="ts">
  import ArrowCircleUp from '~icons/ph/arrow-circle-up-bold';
  import ArrowCounterClockwise from '~icons/ph/arrow-counter-clockwise-bold';
  import CaretDown from '~icons/ph/caret-down-bold';
  import { Button } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Cue, CueInfo, LibraryPreset, ParamValue } from '../../lib/api';
  import { detachPreset, resetToPreset, setPresetParams } from '../../lib/commands';
  import { libraryIcon } from '../../lib/cueIcons';
  import { errorText } from '../../lib/errors';
  import { t } from '../../lib/i18n/index.svelte';
  import { libraries, loadLibraries, localized, paramProblem, paramValues } from '../../lib/library.svelte';
  import { presetFieldName } from '../../lib/presetFields';
  import PresetParams from '../library/PresetParams.svelte';

  // Where a cue made from a library preset came from: its parameters (to change), the fields that differ from the
  // preset (to reset one by one or all), a newer revision (to update) and detaching. The cue works the same
  // without the library; this box only needs it for the comparison.
  let { cue, info, locked }: { cue: Cue; info: CueInfo | undefined; locked: boolean } = $props();

  const reference = $derived(cue.preset!);
  const status = $derived(info?.preset ?? null);
  $effect(() => {
    void loadLibraries([reference.library]).catch(() => {});
  });
  const preset = $derived<LibraryPreset | undefined>(
    libraries.loaded[reference.library]?.presets.find((p) => p.id === reference.preset),
  );

  // The form's values: the cue's, again whenever the cue or the preset changes.
  let values = $derived<Record<string, ParamValue>>(
    preset ? paramValues(preset, reference.params ?? {}) : { ...(reference.params ?? {}) },
  );
  const paramsChanged = $derived(
    JSON.stringify(values) !== JSON.stringify(preset ? paramValues(preset, reference.params ?? {}) : {}),
  );
  const paramsValid = $derived(!!preset && preset.params.every((p) => !paramProblem(p, values[p.id])));

  const Icon = libraryIcon;
  const title = $derived(status && status.status !== 'missing' ? localized(status.title) : '');
  const libraryName = $derived(status && status.status !== 'missing' ? localized(status.library) : '');
  const modified = $derived(status?.status === 'linked' ? status.modified : []);
  const update = $derived(status?.status === 'linked' ? status.update : null);

  const summary = $derived.by(() => {
    if (!status || status.status === 'missing') return t('preset.missing');
    if (status.status === 'broken') return t('preset.broken');
    if (update !== null) return t('preset.update_available', { revision: update });
    return modified.length ? t('preset.modified', { count: modified.length }) : t('preset.unchanged');
  });
</script>

<details class="group mx-4 mb-2.5 rounded-lg border border-line bg-panel-2 text-xs">
  <summary class="flex cursor-pointer items-center gap-2 px-3 py-1">
    <Icon class="size-3.5 shrink-0 text-muted-foreground" />
    <span class="min-w-0 grow truncate">
      <span class="font-semibold text-fg-2">{t('preset.from')}</span>
      {#if title}<span class="text-fg-2">{title}</span>{/if}
      <span class="text-muted-foreground">· {summary}</span>
    </span>
    {#if update !== null}<ArrowCircleUp class="size-3.5 shrink-0 text-primary" />{/if}
    <CaretDown class="text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none" />
  </summary>
  <div class="flex flex-col gap-3 px-3 pt-1 pb-3">
    {#if !status || status.status === 'missing'}
      <p class="m-0 leading-normal text-fg-2">{t('preset.missing_text')}</p>
    {:else}
      <span class="text-muted-foreground">
        {libraryName} · {t('library.revision', { revision: reference.revision })}
      </span>
      {#if status.status === 'broken'}
        <p class="m-0 leading-normal text-stop-soft">{errorText({ kind: 'preset', error: status.error })}</p>
      {/if}
      {#if preset && preset.params.length}
        <div class="flex flex-col gap-2">
          <PresetParams {preset} bind:values disabled={locked} />
          <div>
            <Button
              size="sm"
              disabled={locked || !paramsChanged || !paramsValid}
              onclick={() => setPresetParams(cue.id, $state.snapshot(values))}
            >
              {t('preset.apply_params')}
            </Button>
          </div>
        </div>
      {/if}
      {#if modified.length}
        <div class="flex flex-col gap-1">
          <span class="font-semibold text-fg-2">{t('preset.changed_fields')}</span>
          <ul class="m-0 flex list-none flex-col gap-0.5 p-0">
            {#each modified as field (field)}
              <li class="flex items-center gap-2">
                <span class="grow text-fg-2">{presetFieldName(field)}</span>
                <Tooltip tip={t('preset.reset_field')}>
                  {#snippet child({ props })}
                    <Button
                      {...props}
                      size="icon-sm"
                      variant="ghost"
                      disabled={locked}
                      aria-label={t('preset.reset_field_named', { field: presetFieldName(field) })}
                      onclick={() => resetToPreset([cue.id], [field])}
                    >
                      <ArrowCounterClockwise />
                    </Button>
                  {/snippet}
                </Tooltip>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
      <div class="flex flex-wrap gap-2">
        {#if update !== null}
          <Button size="sm" variant="default" disabled={locked} onclick={() => resetToPreset([cue.id])}>
            {t('preset.update', { revision: update })}
          </Button>
        {:else if modified.length}
          <Button size="sm" disabled={locked} onclick={() => resetToPreset([cue.id])}>{t('preset.reset_all')}</Button>
        {/if}
        <Tooltip tip={t('preset.detach_tip')}>
          {#snippet child({ props })}
            <Button {...props} size="sm" variant="ghost" disabled={locked} onclick={() => detachPreset([cue.id])}>
              {t('preset.detach')}
            </Button>
          {/snippet}
        </Tooltip>
      </div>
      {#if update !== null}
        <p class="m-0 leading-normal text-muted-foreground">{t('preset.update_text')}</p>
      {/if}
    {/if}
  </div>
</details>
