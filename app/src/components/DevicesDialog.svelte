<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import type { NetDevice } from '../lib/api';
  import { edit } from '../lib/commands';
  import { t } from '../lib/i18n/index.svelte';
  import { mode, ui } from '../lib/store.svelte';
  import { field, hint } from '../lib/ui';
  import EditDialog from './EditDialog.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const locked = $derived(mode() === 'show');
  // Edited as a whole and saved as one undo step.
  const initial = JSON.stringify(ui.view!.show.devices);
  let devices = $state<NetDevice[]>(JSON.parse(initial));
  const dirty = $derived(JSON.stringify(devices) !== initial);
  // An emptied number field binds to null.
  const valid = $derived(
    devices.every((device) => Number.isInteger(device.port) && device.port >= 1 && device.port <= 65535),
  );

  function add() {
    devices.push({
      id: crypto.randomUUID().slice(0, 8),
      name: t('devices.default_name'),
      host: '192.168.1.20',
      port: 8000,
    });
  }
</script>

<EditDialog
  title={t('devices.title')}
  class="max-w-140"
  {dirty}
  {valid}
  invalidTip={t('devices.invalid_tip')}
  {locked}
  onapply={async () => !!(await edit({ op: 'set_devices', devices: $state.snapshot(devices) }))}
  {onclose}
>
  <ScrollArea
    ><fieldset class="m-0 flex min-w-0 flex-col gap-3 border-0 p-4" disabled={locked}>
      <p class="{hint} m-0">{t('devices.text')}</p>
      {#each devices as device, i (device.id)}
        <div class="grid grid-cols-[2fr_2fr_1fr_auto] items-end gap-2">
          <Label class={field}>{t('devices.name')}<Input bind:value={device.name} /></Label>
          <Label class={field}>{t('devices.address')}<Input class="font-mono" bind:value={device.host} /></Label>
          <Label class={field}>Port<Input type="number" min="1" max="65535" bind:value={device.port} /></Label>
          <Button
            variant="ghost"
            size="icon-sm"
            class="text-base"
            aria-label={t('devices.remove', { name: device.name })}
            onclick={() => devices.splice(i, 1)}><XMark /></Button
          >
        </div>
      {:else}
        <p class="{hint} m-0">{t('devices.none')}</p>
      {/each}
      <Button class="self-start" onclick={add}>{t('devices.add')}</Button>
    </fieldset></ScrollArea
  >
</EditDialog>
