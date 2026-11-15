<script lang="ts">
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { MidiTrigger } from '../lib/api';
  import { fmtMidi, fmtMidiShort } from '../lib/format';

  // A learned MIDI control in a narrow column: the short form, the full one in the tooltip.
  let { trigger }: { trigger: MidiTrigger } = $props();
</script>

<Tooltip tip={fmtMidi(trigger)}>
  {#snippet child({ props })}
    <span {...props} class="min-w-0 truncate font-mono text-[11.5px] text-fg-2" aria-label={fmtMidi(trigger)}
      >{fmtMidiShort(trigger)}</span
    >
  {/snippet}
</Tooltip>
