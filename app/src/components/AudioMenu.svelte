<script lang="ts">
  import LockSimple from '~icons/ph/lock-simple-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import { api, type LocalSettings } from '../lib/api';
  import { AudioSetup } from '../lib/audioSetup.svelte';
  import { t } from '../lib/i18n/index.svelte';
  import { liveNumber } from '../lib/input';
  import { positiveOrNull } from '../lib/settings';
  import { attemptChange, cuesRunning, ui } from '../lib/store.svelte';
  import { field, hint } from '../lib/ui';
  import ReloadDevices from './ReloadDevices.svelte';
  import SimpleSelect from './SimpleSelect.svelte';

  // Content of the popover on the audio status in the header: the device settings of the settings dialog's
  // device tab (host, device, channels, buffer). Each change is applied at once (the device is opened again);
  // everything else is in the dialog.
  let { onclose }: { onclose: () => void } = $props();

  const view = $derived(ui.view!);
  let draft = $state<LocalSettings>($state.snapshot(ui.view!.settings));
  const setup = new AudioSetup(
    () => draft,
    () => ui.view!,
  );
  // One reopen at a time: the controls wait meanwhile.
  let applying = $state(false);
  // The backend opens the device again only while no cue runs.
  const busy = $derived(cuesRunning());
  // A locked show mode keeps the device as it is here; the settings dialog still changes it, deliberately.
  const showLocked = $derived(view.show.show_mode_locked);
  const locked = $derived(applying || busy);
  const fieldsLocked = $derived(locked || showLocked);

  /** Applies the draft's device fields; a failed device is opened again even unchanged. Channels that do not fit
   * stay in the draft with their message, unapplied. */
  async function apply() {
    if (locked || !setup.channelsValid || (!setup.audioChanged && !view.audio_error)) return;
    applying = true;
    const current = $state.snapshot(ui.view!.settings);
    const { host, device, channels, buffer_frames } = $state.snapshot(draft);
    const applied = await attemptChange(api.setSettings({ ...current, host, device, channels, buffer_frames }));
    applying = false;
    // What the backend runs now, also after a refusal (e.g. while cues run); a copy, the view is the store's.
    draft = structuredClone((applied ?? ui.view!).settings);
  }

  function openSettings() {
    onclose();
    ui.settingsOpen = true;
  }
</script>

{#if showLocked}
  <span class="flex items-center gap-1.5 text-xs text-muted-foreground">
    <LockSimple class="size-4 shrink-0" />
    {t('header.audio_show_locked')}
  </span>
{/if}

<div class="flex flex-col gap-1">
  <span class="font-medium">{t('header.audio_title')}</span>
  {#if view.audio_error}
    <span class="text-xs text-stop-soft">{t('header.audio_unavailable', { error: view.audio_error })}</span>
  {:else}
    <span class="text-xs wrap-break-word text-fg-2">{view.audio.device_detail}</span>
    <span class="{hint} tabular-nums">
      {view.audio.host} · {view.audio.sample_rate} Hz · {t('header.channels', { count: view.audio.channels })}
      {#if view.audio_frames !== null}· {t('device.frames', {
          frames: view.audio_frames,
          latency: setup.latency(view.audio_frames),
        })}{/if}
    </span>
    {#if view.audio_lost}
      <span class="text-xs whitespace-pre-line text-stop-soft"
        >{t('header.audio_lost', { reason: view.audio_lost })}</span
      >
    {/if}
  {/if}
</div>

<fieldset class="m-0 flex min-w-0 flex-col gap-3 border-0 p-0" disabled={fieldsLocked}>
  <Label class={field}>
    {t('device.host')}
    <SimpleSelect
      class="w-full"
      aria-label={t('device.host')}
      value={draft.host ?? ''}
      disabled={fieldsLocked}
      options={setup.hostOptions}
      onchange={(host) => {
        setup.setHost(host);
        void apply();
      }}
    />
  </Label>
  <Label class={field}>
    {t('midi_fields.device')}
    <div class="flex items-center gap-1">
      <SimpleSelect
        class="min-w-0 grow"
        aria-label={t('midi_fields.device')}
        value={draft.device ?? ''}
        tip={setup.selectedDevice?.description}
        disabled={fieldsLocked || setup.loading}
        options={setup.deviceOptions}
        onchange={(device) => {
          draft.device = device || null;
          void apply();
        }}
      />
      <ReloadDevices {setup} disabled={fieldsLocked} />
    </div>
  </Label>
  <div class="grid grid-cols-[6rem_minmax(0,1fr)] gap-2">
    <Label class={field}>
      {t('device.channels')}
      <Input
        type="number"
        min="2"
        max={setup.mainMax ?? undefined}
        placeholder={t('device.default')}
        aria-label={t('device.channels')}
        disabled={fieldsLocked}
        value={draft.channels ?? ''}
        {...liveNumber(
          () => draft.channels,
          (text) => (draft.channels = positiveOrNull(text)),
        )}
        onchange={() => void apply()}
      />
    </Label>
    <Label class={field}>
      {t('device.buffer')}
      <SimpleSelect
        class="w-full"
        aria-label={t('device.buffer')}
        value={setup.bufferValue}
        disabled={fieldsLocked || setup.hostSetsBuffer}
        options={setup.bufferOptions}
        onchange={(frames) => {
          setup.setBuffer(frames);
          void apply();
        }}
      />
    </Label>
  </div>
  {#if busy}
    <span class="text-xs text-fade-soft">{t('header.audio_busy')}</span>
  {:else if setup.tooManyChannels}
    <span class="text-xs text-stop-soft">{t('device.only_channels', { count: setup.mainMax ?? 0 })}</span>
  {:else if !setup.channelsValid}
    <span class="text-xs text-stop-soft">{t('settings.channels_invalid')}</span>
  {:else}
    <span class={hint}>{t('device.description')}</span>
  {/if}
</fieldset>

<div class="flex items-center justify-between gap-2">
  <Button variant="ghost" size="sm" onclick={openSettings}>{t('header.audio_more')}</Button>
  {#if view.audio_error}
    <Button size="sm" disabled={locked} onclick={apply}>{t('header.audio_retry')}</Button>
  {/if}
</div>
