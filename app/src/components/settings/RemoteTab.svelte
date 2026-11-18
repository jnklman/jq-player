<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import { api, type KeyBinding, type LocalSettings, type RemoteAddress, type RemoteClient } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { liveNumber } from '../../lib/input';
  import { ACTION_NAMES, keyText } from '../../lib/keys.svelte';
  import { remoteRoleOptions } from '../../lib/options';
  import { positiveOrNull } from '../../lib/settings';
  import { attempt, ui } from '../../lib/store.svelte';
  import { check, hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import SettingGroup from './SettingGroup.svelte';
  import SettingRow from './SettingRow.svelte';
  import SettingsPanel from './SettingsPanel.svelte';

  // The web remote: whether the server runs and on which port (in the draft, applied with the dialog), where devices
  // find it, and the devices themselves; letting them in, their roles and removing them apply at once, also in show
  // mode (it is part of operating).
  let { draft = $bindable() }: { draft: LocalSettings } = $props();

  const status = $derived(ui.remoteStatus);
  const listening = $derived(status?.listening ?? null);
  const clients = $derived(status?.clients ?? []);
  const known = $derived(clients.filter((client) => client.role !== 'pending'));
  const requests = $derived(clients.filter((client) => client.role === 'pending'));

  let addresses = $state<RemoteAddress[]>([]);
  $effect(() => {
    if (listening === null) {
      addresses = [];
      return;
    }
    void attempt(api.remoteInfo()).then((found) => (addresses = found ?? []));
  });

  function seen(client: RemoteClient): string {
    if (client.connected) return t('remote_tab.connected', { count: client.connected });
    if (client.last_seen === null) return t('remote_tab.not_seen');
    return t('remote_tab.last_seen', { time: new Date(client.last_seen).toLocaleString() });
  }

  /** A device's own keys, e.g. "GO F1 · Stop all F2"; set on the device, reset here. */
  function ownKeys(keys: KeyBinding[]): string {
    if (!keys.length) return t('remote_tab.own_keys_none');
    const list = keys.map((binding) => `${ACTION_NAMES[binding.action]} ${keyText(binding.chord)}`).join(' · ');
    return t('remote_tab.own_keys', { keys: list });
  }
</script>

<SettingsPanel tab="remote">
  <SettingGroup title={t('remote_tab.server')}>
    <SettingRow label={t('remote_tab.enabled')} keywords={t('remote_tab.enabled_keywords')}>
      <Label class="{check} pt-1.5">
        <Checkbox bind:checked={draft.remote.enabled} />
        {t('remote_tab.enable')}
      </Label>
      <span class="{hint} {status?.error ? 'text-stop-soft' : listening ? 'text-go' : ''}">
        {status?.error ?? (listening ? t('remote_tab.listening', { port: listening }) : t('remote_tab.not_listening'))}
      </span>
    </SettingRow>
    <SettingRow label={t('midi_tab.port')} keywords={t('remote_tab.port_keywords')}>
      <Input
        class="w-24"
        type="number"
        min="1"
        max="65535"
        aria-label={t('remote_tab.port_label')}
        value={draft.remote.port}
        {...liveNumber(
          () => draft.remote.port,
          (text) => (draft.remote.port = positiveOrNull(text) ?? draft.remote.port),
        )}
      />
    </SettingRow>
    {#if addresses.length}
      <SettingRow label={t('remote_tab.address')} keywords={t('remote_tab.address_keywords')}>
        <div class="flex flex-wrap gap-4 pt-1">
          {#each addresses as address (address.url)}
            <figure class="m-0 flex flex-col items-center gap-1.5">
              <img
                class="size-40 rounded-md bg-white"
                src="data:image/svg+xml,{encodeURIComponent(address.qr_svg)}"
                alt={t('remote_tab.qr_of', { url: address.url })}
              />
              <figcaption class="flex flex-col items-center text-xs">
                <span class="font-mono text-fg select-text">{address.url}</span>
                <span class="text-muted-foreground">{address.interface}</span>
              </figcaption>
            </figure>
          {/each}
        </div>
      </SettingRow>
    {/if}
    <SettingRow label={t('remote_tab.security')} keywords={t('remote_tab.security_keywords')}>
      <span class="{hint} pt-1.5">{t('remote_tab.security_hint')}</span>
    </SettingRow>
  </SettingGroup>

  {#if requests.length}
    <SettingGroup title={t('remote_tab.requests')}>
      {#each requests as client (client.id)}
        <SettingRow label={client.name} keywords={t('remote_tab.device_keywords')}>
          <div class="flex flex-wrap items-center gap-2">
            <Button size="sm" onclick={() => attempt(api.setRemoteClient(client.id, 'view'))}
              >{t('remote_tab.let_view')}</Button
            >
            <Button size="sm" onclick={() => attempt(api.setRemoteClient(client.id, 'control'))}
              >{t('remote_tab.let_control')}</Button
            >
            <Button size="sm" variant="destructive" onclick={() => attempt(api.removeRemoteClient(client.id))}
              >{t('remote_tab.decline')}</Button
            >
          </div>
          <span class={hint}>
            {client.address ?? ''}{#if client.known_name}
              · {t('remote_tab.known_name')}{/if}
          </span>
        </SettingRow>
      {/each}
    </SettingGroup>
  {/if}

  <SettingGroup title={t('remote_tab.devices')}>
    {#snippet aside()}{t('remote_tab.devices_hint')}{/snippet}
    {#each known as client (client.id)}
      <SettingRow label={client.name} keywords={t('remote_tab.device_keywords')}>
        <div class="flex flex-wrap items-center gap-2">
          <SimpleSelect
            class="w-40"
            aria-label={t('remote_tab.role_of', { name: client.name })}
            value={client.role as 'view' | 'control'}
            options={remoteRoleOptions}
            onchange={(role) => attempt(api.setRemoteClient(client.id, role))}
          />
          {#if client.keys}
            <Button
              size="sm"
              aria-label={t('remote_tab.host_keys_of', { name: client.name })}
              onclick={() => attempt(api.setRemoteKeys(client.id, null))}>{t('remote_tab.host_keys')}</Button
            >
          {/if}
          <Button size="sm" variant="destructive" onclick={() => attempt(api.removeRemoteClient(client.id))}
            >{t('remote_tab.remove')}</Button
          >
        </div>
        <span class={[hint, client.connected && 'text-go']}>{seen(client)}</span>
        {#if client.keys}<span class={hint}>{ownKeys(client.keys)}</span>{/if}
      </SettingRow>
    {:else}
      <p class="{hint} py-2.5">{t('remote_tab.no_devices')}</p>
    {/each}
  </SettingGroup>
</SettingsPanel>
