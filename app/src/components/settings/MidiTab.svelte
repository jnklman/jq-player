<script lang="ts">
  import { Input } from '$lib/components/ui/input';

  import { api, type LocalSettings } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { withMissing } from '../../lib/options';
  import { idOrNull } from '../../lib/settings';
  import { settingsView } from '../../lib/settingsView.svelte';
  import { attempt, ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // MIDI input and output, MSC. The learned bindings are in the shortcut table.
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  const view = $derived(ui.view!);
  const settings = settingsView();

  let inputs = $state<string[]>([]);
  let outputs = $state<string[]>([]);
  attempt(api.midiPorts()).then((ports) => {
    inputs = ports?.inputs ?? [];
    outputs = ports?.outputs ?? [];
  });

  const portOptions = (ports: string[], none: string, current: string | null) =>
    withMissing(
      [{ value: '', label: none }, ...ports.map((port) => ({ value: port, label: port }))],
      current,
      t('device.not_found', { name: current ?? '' }),
    );
</script>

<SettingsPanel tab="midi">
  {#if !settings.searching}
    <p class="{hint} -mt-3 mb-0">
      {t('midi_tab.bindings_before')}
      <button class="cursor-pointer text-primary hover:underline" onclick={() => settings.open('keys')}
        >{t('settings.tab.keys')}</button
      >.
    </p>
  {/if}
  <SettingGroup title={t('midi_tab.input')}>
    <SettingRow label={t('midi_tab.port')} keywords={t('midi_tab.input_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('midi_tab.input_label')}
        value={draft.midi_port ?? ''}
        options={portOptions(inputs, t('midi_tab.no_midi'), draft.midi_port)}
        onchange={(port) => (draft.midi_port = port || null)}
      />
      {#if view.midi.error}
        <span class="text-xs text-stop-soft">{t('midi_tab.error_retry', { error: view.midi.error })}</span>
      {:else if draft.midi_port !== view.settings.midi_port}
        <span class={hint}>{t('midi_tab.opens_on_apply')}</span>
      {:else if view.midi.port}
        <span class="text-xs text-go">{t('midi_tab.connected', { name: view.midi.port })}</span>
      {/if}
    </SettingRow>
    <SettingRow label={t('midi_tab.msc_id')} keywords="MIDI Show Control">
      <Input
        class="w-24"
        type="number"
        min="0"
        max="126"
        placeholder={t('common.off')}
        aria-label={t('midi_tab.msc_id')}
        value={draft.msc_device ?? ''}
        oninput={(e) => (draft.msc_device = idOrNull(e.currentTarget.value))}
      />
      <span class={hint}>{t('midi_tab.msc_hint')}</span>
    </SettingRow>
  </SettingGroup>

  <SettingGroup title={t('midi_tab.output')}>
    <SettingRow label={t('midi_tab.out_port')} keywords={t('midi_tab.output_keywords')}>
      <SimpleSelect
        class="w-full max-w-90"
        aria-label={t('midi_tab.output_label')}
        value={draft.midi_out ?? ''}
        options={portOptions(outputs, t('midi_tab.no_output'), draft.midi_out)}
        onchange={(port) => (draft.midi_out = port || null)}
      />
      {#if view.midi.out_error}
        <span class="text-xs text-stop-soft">{t('midi_tab.error_retry', { error: view.midi.out_error })}</span>
      {/if}
    </SettingRow>
  </SettingGroup>
</SettingsPanel>
