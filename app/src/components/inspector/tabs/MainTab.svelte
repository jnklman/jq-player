<script lang="ts">
  import type { CueInfo } from '../../../lib/api';
  import { isControl } from '../../../lib/control';
  import type { CueEdit } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import { hasAudio, playsMedia } from '../../../lib/inspector.svelte';
  import { outputOptions } from '../../../lib/options';
  import { ui } from '../../../lib/store.svelte';
  import ControlMain from '../ControlMain.svelte';
  import EffectRows from '../EffectRows.svelte';
  import FadeRows from '../FadeRows.svelte';
  import FieldGroup from '../FieldGroup.svelte';
  import SelectRow from '../fields/SelectRow.svelte';
  import GoToFields from '../GoToFields.svelte';
  import GroupFields from '../GroupFields.svelte';
  import MidiFields from '../MidiFields.svelte';
  import OscFields from '../OscFields.svelte';
  import OthersGroup from '../OthersGroup.svelte';
  import PlaylistMain from '../PlaylistMain.svelte';
  import SoundMain from '../SoundMain.svelte';

  // The first tab: what the cue type itself does. With several cues: what they share of it (all sound cues,
  // all playlists, or several cues of one type).
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const cues = $derived(edit.cues);
  const first = $derived(cues[0]);
  const type = $derived(cues.every((cue) => cue.type === first.type) ? first.type : null);
</script>

{#if type === 'sound'}
  <SoundMain {edit} {info} />
{:else if type === 'playlist'}
  <PlaylistMain {edit} {info} />
{:else if cues.every(hasAudio)}
  <FieldGroup title="Fades"><FadeRows {edit} /></FieldGroup>
{:else if type === 'group'}
  <GroupFields {edit} />
{:else if !edit.multi && isControl(first)}
  <ControlMain {edit} cue={first} targetMissing={!!info?.target_missing} />
{:else if !edit.multi && first.type === 'midi'}
  <MidiFields {edit} cue={first} />
{:else if !edit.multi && first.type === 'osc'}
  <OscFields {edit} cue={first} />
{:else if !edit.multi && first.type === 'goto'}
  <GoToFields {edit} cue={first} {info} />
{:else if type === 'wait'}
  <FieldGroup><EffectRows {edit} /></FieldGroup>
{:else if type}
  <FieldGroup title={t('control_main.effect')}><EffectRows {edit} /></FieldGroup>
{/if}

{#if cues.every(hasAudio)}
  <FieldGroup title={t('field.output')}>
    <SelectRow {edit} key="output" label={t('field.output')} options={outputOptions(ui.view!)} />
  </FieldGroup>
{/if}

{#if cues.every(playsMedia)}
  <OthersGroup {edit} />
{/if}
