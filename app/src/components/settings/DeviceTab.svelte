<script lang="ts">
  import { Input } from '$lib/components/ui/input';

  import type { LocalSettings } from '../../lib/api';
  import type { AudioSetup } from '../../lib/audioSetup.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { withMissing } from '../../lib/options';
  import { positiveOrNull } from '../../lib/settings';
  import { settingsView } from '../../lib/settingsView.svelte';
  import { ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // Host, device, channel count and buffer size.
  let { draft = $bindable(), setup }: { draft: LocalSettings; setup: AudioSetup } = $props();

  const view = $derived(ui.view!);
  const devices = $derived(setup.devices);
  const settings = settingsView();
</script>

<SettingsPanel tab="device" description={t('device.description')}>
  {#if !settings.searching}
    <div
      class={[
        'flex flex-wrap items-center gap-2.5 rounded-lg border bg-panel-2 px-3 py-2.5 text-xs text-fg-2 tabular-nums',
        view.audio_error ? 'border-stop-line' : 'border-line',
      ]}
    >
      <span
        class={[
          'size-2 shrink-0 rounded-full',
          view.audio_error ? 'bg-stop-soft ring-3 ring-stop-soft/15' : 'bg-go ring-3 ring-go/15',
        ]}
      ></span>
      {#if view.audio_error}
        <span class="text-stop-soft">{t('device.unavailable', { error: view.audio_error })}</span>
      {:else}
        {t('device.running')} · {view.audio.sample_rate} Hz · {t('header.channels', { count: view.audio.channels })}
        {#if view.audio_frames !== null}
          · {t('device.frames', { frames: view.audio_frames, latency: setup.latency(view.audio_frames) })}
          {#if view.settings.buffer_frames !== null && view.settings.buffer_frames !== view.audio_frames && view.audio.host !== 'JACK'}
            <span class="text-fade-soft">· {t('device.requested', { frames: view.settings.buffer_frames })}</span>
          {/if}
        {/if}
      {/if}
    </div>
  {/if}
  <SettingGroup>
    <SettingRow label={t('device.host')} keywords={t('device.host_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('device.host')}
        value={draft.host ?? ''}
        options={withMissing(
          [
            { value: '', label: t('device.system_default') },
            ...(devices?.hosts ?? []).map((host) => ({ value: host, label: host })),
          ],
          devices ? draft.host : null,
          t('device.not_available', { name: draft.host ?? '' }),
        )}
        onchange={(host) => setup.setHost(host)}
      />
    </SettingRow>
    <SettingRow label={t('midi_fields.device')} keywords={t('device.device_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('midi_fields.device')}
        value={draft.device ?? ''}
        tip={setup.selectedDevice?.description}
        disabled={setup.loading}
        options={[
          { value: '', label: setup.loading ? t('device.searching') : t('audio.default_device') },
          ...(devices?.devices ?? []).map((device) => ({ value: device.id, label: device.name })),
          ...(setup.unknownDevice && draft.device
            ? [{ value: draft.device, label: t('device.not_found', { name: draft.device }) }]
            : []),
        ]}
        onchange={(device) => (draft.device = device || null)}
      />
    </SettingRow>
    <SettingRow label={t('device.channels')} keywords={t('device.channels_keywords')}>
      <div class="flex flex-wrap items-center gap-2.5">
        <Input
          class="w-24"
          type="number"
          min="2"
          max={setup.mainMax ?? undefined}
          placeholder={t('device.default')}
          aria-label={t('device.channels')}
          value={draft.channels ?? ''}
          oninput={(e) => (draft.channels = positiveOrNull(e.currentTarget.value))}
        />
        {#if setup.tooManyChannels}
          <span class="text-xs text-stop-soft">{t('device.only_channels', { count: setup.mainMax ?? 0 })}</span>
        {:else}
          <span class={hint}>
            {setup.selectedDevice?.default_config ?? t('device.currently', { count: view.audio.channels })}
            {#if setup.mainMax !== null}· {t('device.up_to', { count: setup.mainMax })}{/if}
          </span>
        {/if}
      </div>
    </SettingRow>
    <SettingRow label={t('device.buffer')} keywords={t('device.buffer_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('device.buffer')}
        value={setup.hostSetsBuffer ? '' : String(draft.buffer_frames ?? '')}
        disabled={setup.hostSetsBuffer}
        options={withMissing(
          [
            {
              value: '',
              label:
                setup.defaultFrames === null
                  ? t('device.default')
                  : t('device.default_frames', {
                      frames: setup.defaultFrames,
                      latency: setup.latency(setup.defaultFrames),
                    }),
            },
            ...setup.bufferSizes.map((frames) => ({
              value: String(frames),
              label: `${frames} (${setup.latency(frames)})`,
            })),
          ],
          draft.buffer_frames === null ? null : String(draft.buffer_frames),
          t('device.unsupported', { frames: draft.buffer_frames ?? 0 }),
        )}
        onchange={(frames) => (draft.buffer_frames = frames ? Number(frames) : null)}
      />
      <span class={hint}>
        {setup.hostSetsBuffer ? t('device.jack_buffer') : t('device.buffer_hint')}
      </span>
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
