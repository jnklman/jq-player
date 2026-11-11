<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  import type { MidiCue } from '../../lib/api';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { midiMessageOf } from '../../lib/midi';
  import { ui } from '../../lib/store.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';
  import MidiMessageFields from './MidiMessageFields.svelte';

  // The messages of a MIDI cue.
  let { cue, edit }: { cue: MidiCue; edit: CueEdit } = $props();

  const out = $derived(ui.view?.midi.out);
</script>

<FieldGroup title={t('inspector.tab.messages')}>
  {#snippet aside()}{out ? t('midi_fields.to', { name: out }) : t('midi_fields.choose_output')}{/snippet}
  <FieldRow>
    {#each cue.messages as message, i (i)}
      <MidiMessageFields
        {message}
        index={i}
        onchange={(changed) => edit.set({ messages: cue.messages.map((other, j) => (j === i ? changed : other)) })}
        onremove={() => edit.set({ messages: cue.messages.filter((_, j) => j !== i) })}
      />
    {/each}
    <Button
      size="sm"
      class="self-start"
      onclick={() => edit.set({ messages: [...cue.messages, midiMessageOf('program_change', cue.messages.at(-1))] })}
      >{t('midi_fields.add')}</Button
    >
  </FieldRow>
</FieldGroup>
