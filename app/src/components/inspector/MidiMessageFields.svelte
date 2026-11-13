<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';

  import type { MidiMessage, MscCommand } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { midiKinds, midiMessageOf } from '../../lib/midi';
  import type { Option } from '../../lib/options';
  import { field } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';

  // One outgoing message of a MIDI cue: its kind and that kind's values, in a box.
  let {
    message,
    index,
    onchange,
    onremove,
  }: { message: MidiMessage; index: number; onchange: (message: MidiMessage) => void; onremove: () => void } = $props();

  const kindOptions = midiKinds;
  const mscCommands: Option<MscCommand>[] = [
    { value: 'go', label: 'GO' },
    { value: 'stop', label: 'STOP' },
    { value: 'resume', label: 'RESUME' },
  ];

  function set(patch: Record<string, number | string>) {
    onchange({ ...message, ...patch } as MidiMessage);
  }
</script>

{#snippet byte(label: string, key: string, value: number, min = 0)}
  <NumberField
    class="w-20"
    {label}
    step={1}
    {min}
    max={127}
    integer
    {value}
    onchange={(byteValue) => set({ [key]: byteValue })}
  />
{/snippet}

<div class="flex flex-wrap items-end gap-2 rounded-lg border border-line p-2">
  <Label class="{field} min-w-40 flex-1">
    {t('layout.kind')}
    <SimpleSelect
      value={message.kind}
      options={kindOptions}
      onchange={(kind) => onchange(midiMessageOf(kind, message))}
    />
  </Label>
  {#if message.kind === 'msc'}
    <NumberField
      class="w-20"
      label={t('midi_fields.device')}
      tip={t('midi_fields.all')}
      step={1}
      min={0}
      max={127}
      integer
      value={message.device}
      onchange={(device) => set({ device })}
    />
    <Label class="{field} w-24">
      {t('midi_fields.command')}
      <SimpleSelect value={message.command} options={mscCommands} onchange={(command) => set({ command })} />
    </Label>
    <Label class="{field} w-20">
      {t('midi_fields.cue')}
      <Input
        placeholder={t('midi_fields.next')}
        value={message.cue}
        onchange={(e) => set({ cue: e.currentTarget.value.trim() })}
      />
    </Label>
    <Label class="{field} w-16">
      {t('midi_fields.list')}
      <Input value={message.list} onchange={(e) => set({ list: e.currentTarget.value.trim() })} />
    </Label>
  {:else}
    <NumberField
      class="w-16"
      label={t('midi_fields.channel')}
      step={1}
      min={1}
      max={16}
      integer
      value={message.channel + 1}
      onchange={(channel) => set({ channel: channel - 1 })}
    />
    {#if message.kind === 'program_change'}
      <!-- Shown 1 … 128 like a learned program trigger (`fmtMidi`); sent as 0 … 127. -->
      <NumberField
        class="w-20"
        label={t('midi.program')}
        step={1}
        min={1}
        max={128}
        integer
        value={message.program + 1}
        onchange={(program) => set({ program: program - 1 })}
      />
    {:else if message.kind === 'control_change'}
      {@render byte(t('midi.what.controller'), 'controller', message.controller)}
      {@render byte(t('midi.what.value'), 'value', message.value)}
    {:else}
      {@render byte(t('midi.note'), 'note', message.note)}
      {#if message.kind === 'note_on'}
        {@render byte(t('midi_fields.velocity'), 'velocity', message.velocity, 1)}
      {/if}
    {/if}
  {/if}
  <Button
    variant="ghost"
    size="icon-sm"
    class="shrink-0 text-base"
    aria-label={t('midi_fields.remove', { number: index + 1 })}
    onclick={onremove}><XMark /></Button
  >
</div>
