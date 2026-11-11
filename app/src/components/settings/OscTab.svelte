<script lang="ts">
  import { Input } from '$lib/components/ui/input';
  import { Kbd } from '$lib/components/ui/kbd';

  import type { LocalSettings } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { positiveOrNull } from '../../lib/settings';
  import { ui } from '../../lib/store.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsBadge from './SettingsBadge.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // The OSC port this machine listens on, and its own addresses of the show's devices.
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  const view = $derived(ui.view!);
  const commands = [
    '/go',
    '/stop',
    '/panic',
    '/fade',
    '/playhead/next',
    '/playhead/previous',
    '/cue/{No.}/start|stop|fade|playhead',
  ];
</script>

<SettingsPanel tab="osc">
  <SettingGroup title={t('osc_tab.receive')}>
    <SettingRow label={t('midi_tab.port')} keywords={t('osc_tab.port_keywords')}>
      <div class="flex flex-wrap items-center gap-2.5">
        <Input
          class="w-24"
          type="number"
          min="1"
          max="65535"
          placeholder={t('common.off')}
          aria-label={t('osc_tab.port_label')}
          value={draft.osc_listen ?? ''}
          oninput={(e) => (draft.osc_listen = positiveOrNull(e.currentTarget.value))}
        />
        <span
          class={[
            'text-xs',
            view.osc.error ? 'text-stop-soft' : view.osc.listening ? 'text-go' : 'text-muted-foreground',
          ]}
        >
          {view.osc.error ??
            (view.osc.listening ? t('osc_tab.listening', { port: view.osc.listening }) : t('osc_tab.not_listening'))}
        </span>
      </div>
    </SettingRow>
    <SettingRow label={t('osc_tab.commands')} keywords={t('osc_tab.commands_keywords')}>
      <div class="flex flex-wrap gap-1.5 pt-1">
        {#each commands as command (command)}<Kbd>{command}</Kbd>{/each}
      </div>
    </SettingRow>
  </SettingGroup>

  {#if view.show.devices.length}
    <SettingGroup title={t('osc_tab.device_addresses')}>
      {#snippet badge()}<SettingsBadge kind="show" />{/snippet}
      {#snippet aside()}{t('osc_tab.empty_hint')}{/snippet}
      {#each view.show.devices as device (device.id)}
        <SettingRow label={device.name} keywords={t('osc_tab.device_keywords')}>
          <Input
            class="w-full max-w-90 font-mono"
            aria-label={t('osc_tab.address_on_machine', { name: device.name })}
            placeholder={t('osc_tab.from_show', { address: `${device.host}:${device.port}` })}
            value={draft.osc_hosts[device.id] ?? ''}
            oninput={(e) => {
              const target = e.currentTarget.value.trim();
              if (target) draft.osc_hosts[device.id] = target;
              else delete draft.osc_hosts[device.id];
            }}
          />
        </SettingRow>
      {/each}
    </SettingGroup>
  {/if}
</SettingsPanel>
