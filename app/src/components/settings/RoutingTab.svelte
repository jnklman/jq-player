<script lang="ts">
  import { Input } from '$lib/components/ui/input';

  import type { LocalSettings } from '../../lib/api';
  import { type AudioSetup, fits } from '../../lib/audioSetup.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { withMissing } from '../../lib/options';
  import { positiveOrNull } from '../../lib/settings';
  import { ui } from '../../lib/store.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsBadge from './SettingsBadge.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // On which channels the show's outputs and the preview play.
  let { draft = $bindable(), setup }: { draft: LocalSettings; setup: AudioSetup } = $props();

  const view = $derived(ui.view!);
</script>

<!-- The first channel of a pair and whether the pair exists. -->
{#snippet pair(label: string, first: number, limit: number | null, set: (first: number) => void)}
  <div class="flex flex-wrap items-center gap-2.5">
    <Input
      class="w-24"
      type="number"
      min="1"
      max={limit !== null ? limit - 1 : undefined}
      aria-label={label}
      value={first}
      oninput={(e) => {
        // An emptied field waits for the next digit instead of writing 1 back.
        const channel = positiveOrNull(e.currentTarget.value);
        if (channel !== null) set(channel);
      }}
      onblur={(e) => (e.currentTarget.value = String(first))}
    />
    <span class={['text-xs', fits(first, limit) ? 'text-muted-foreground' : 'text-stop-soft']}>
      {t('routing.pair', { first, second: first + 1 })}{fits(first, limit)
        ? ''
        : ` ${t('routing.missing', { count: limit ?? 0 })}`}
    </span>
  </div>
{/snippet}

<SettingsPanel tab="routing" description={t('routing.description')}>
  <SettingGroup title={t('outputs.title')}>
    {#snippet badge()}<SettingsBadge kind="show" />{/snippet}
    {#snippet aside()}{t('routing.first_of_pair')}{/snippet}
    {#each view.show.outputs as output (output.id)}
      <SettingRow label={output.name} keywords={t('routing.output_keywords')}>
        {@render pair(
          t('routing.first_channel_of', { name: output.name }),
          draft.outputs[output.id] ?? 1,
          setup.mainLimit,
          (first) => (draft.outputs[output.id] = first),
        )}
      </SettingRow>
    {:else}
      <p class="m-0 py-2.5 text-xs text-muted-foreground">{t('routing.no_outputs')}</p>
    {/each}
  </SettingGroup>

  <SettingGroup title={t('action.preview')}>
    <SettingRow label={t('field.output')} keywords={t('routing.preview_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('routing.preview_output')}
        value={setup.previewMode}
        options={[
          { value: 'off', label: t('common.off') },
          { value: 'main', label: t('routing.pair_on_device') },
          { value: 'device', label: t('routing.own_device') },
        ]}
        onchange={(mode) => setup.setPreviewMode(mode)}
      />
      {#if view.preview.error}
        <span class="text-xs text-stop-soft">{view.preview.error}</span>
      {/if}
    </SettingRow>
    {#if setup.previewMode === 'device'}
      <SettingRow label={t('midi_fields.device')} keywords={t('routing.preview_keywords')}>
        <SimpleSelect
          class="w-full max-w-90"
          aria-label={t('routing.preview_device')}
          value={draft.preview_device ?? ''}
          options={withMissing(
            (setup.devices?.devices ?? []).map((device) => ({ value: device.id, label: device.name })),
            draft.preview_device,
            t('device.not_found', { name: draft.preview_device ?? '' }),
          )}
          onchange={(device) => (draft.preview_device = device)}
        />
      </SettingRow>
    {/if}
    {#if setup.previewMode !== 'off'}
      <SettingRow label={t('routing.first_channel')} keywords={t('routing.preview_keywords')}>
        {@render pair(
          t('routing.preview_first_channel'),
          draft.preview_channel ?? 1,
          setup.previewLimit,
          (first) => (draft.preview_channel = first),
        )}
        {#if setup.previewOverlaps}
          <span class="text-xs text-stop-soft">{t('routing.overlap')}</span>
        {/if}
      </SettingRow>
    {/if}
  </SettingGroup>
</SettingsPanel>
