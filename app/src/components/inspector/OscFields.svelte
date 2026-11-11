<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';

  import type { OscCue, OscMessage } from '../../lib/api';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { formatOscArgs, parseOscArgs } from '../../lib/osc';
  import { ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';

  // The device and messages of an OSC cue.
  let { cue, edit }: { cue: OscCue; edit: CueEdit } = $props();

  const devices = $derived(ui.view?.show.devices ?? []);

  function setMessage(index: number, patch: Partial<OscMessage>) {
    edit.set({ messages: cue.messages.map((message, i) => (i === index ? { ...message, ...patch } : message)) });
  }
</script>

<FieldGroup title={t('midi_fields.device')}>
  <FieldRow label={t('osc_fields.send_to')}>
    <SimpleSelect
      aria-label={t('midi_fields.device')}
      value={cue.device ?? ''}
      options={[
        { value: '', label: t('osc_fields.no_device') },
        ...devices.map((device) => ({ value: device.id, label: `${device.name} (${device.host}:${device.port})` })),
      ]}
      onchange={(device) => edit.set({ device: device || null })}
    />
    {#if devices.length === 0}
      <Button variant="link" class="min-h-0 self-start p-0 text-xs" onclick={() => (ui.devicesOpen = true)}>
        {t('osc_fields.create_devices')}
      </Button>
    {/if}
  </FieldRow>
</FieldGroup>
<FieldGroup title={t('inspector.tab.messages')}>
  <FieldRow>
    <span class={hint}>{t('osc_fields.hint')}</span>
    {#each cue.messages as message, i (i)}
      <div class="flex items-center gap-2">
        <Input
          class="font-mono"
          aria-label={t('osc_fields.address', { number: i + 1 })}
          placeholder="/pb/1/go"
          value={message.address}
          onchange={(e) => setMessage(i, { address: e.currentTarget.value.trim() })}
        />
        <Input
          class="font-mono"
          aria-label={t('osc_fields.args', { number: i + 1 })}
          placeholder={t('osc_fields.args_placeholder')}
          value={formatOscArgs(message.args)}
          onchange={(e) => setMessage(i, { args: parseOscArgs(e.currentTarget.value) })}
        />
        <Button
          variant="ghost"
          size="icon-sm"
          class="shrink-0 text-base"
          aria-label={t('midi_fields.remove', { number: i + 1 })}
          onclick={() => edit.set({ messages: cue.messages.filter((_, j) => j !== i) })}><XMark /></Button
        >
      </div>
    {/each}
    <Button
      size="sm"
      class="self-start"
      onclick={() => edit.set({ messages: [...cue.messages, { address: '/', args: [] }] })}
      >{t('midi_fields.add')}</Button
    >
  </FieldRow>
</FieldGroup>
