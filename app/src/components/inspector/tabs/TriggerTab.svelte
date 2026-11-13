<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type Cue, type CueInfo, type SecondTrigger } from '../../../lib/api';
  import { cueLabel } from '../../../lib/cue';
  import type { CueEdit } from '../../../lib/cueEdit';
  import { fmtMidi } from '../../../lib/format';
  import { t } from '../../../lib/i18n/index.svelte';
  import { playsMedia } from '../../../lib/inspector.svelte';
  import { secondTriggerOptions } from '../../../lib/options';
  import { attempt, cueById, isTopLevel, ui } from '../../../lib/store.svelte';
  import { hint } from '../../../lib/ui';
  import KeyCapture from '../../KeyCapture.svelte';
  import SimpleSelect from '../../SimpleSelect.svelte';
  import { getLinked } from '../context';
  import ExitLoopFields from '../ExitLoopFields.svelte';
  import FieldGroup from '../FieldGroup.svelte';
  import FieldRow from '../FieldRow.svelte';
  import SwitchRow from '../fields/SwitchRow.svelte';
  import OwnMark from '../OwnMark.svelte';

  // "Trigger": hotkey and MIDI trigger (in the look of the settings' shortcut table), triggering on release, and
  // what a trigger does while the cue runs.
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const view = $derived(ui.view!);
  const linked = getLinked();
  const single = $derived(edit.multi ? null : edit.cues[0]);
  // Hotkeys, MIDI triggers and release apply to top-level cues only.
  const topLevel = $derived(!!single && isTopLevel(single.id));
  const plays = $derived(edit.cues.every(playsMedia));
  const learning = $derived(!!single && view.midi.learning?.kind === 'cue' && view.midi.learning.cue === single.id);
  const actions = $derived(new Set(edit.cues.map((cue) => cue.second_trigger.action)));
  const action = $derived(actions.size === 1 ? [...actions][0] : null);
  // "Next item" exists for playlists only.
  const options = $derived(
    edit.cues.every((cue) => cue.type === 'playlist')
      ? secondTriggerOptions
      : secondTriggerOptions.filter((option) => option.value !== 'next_item'),
  );

  function setSecondTrigger(next: SecondTrigger['action']) {
    edit.set({
      second_trigger:
        next === 'exit_loop'
          ? { action: next, exit: { mode: 'loop_end', level: 'innermost', stop: false } }
          : { action: next },
    });
  }

  // Leaving the cue (another cue, another tab) ends a pending learn for it, so the next control does not bind to a
  // cue no longer shown.
  const learningFor = $derived(learning ? single?.id : undefined);
  $effect(() => {
    const id = learningFor;
    if (!id) return;
    return () => {
      const pending = ui.view?.midi.learning;
      if (pending?.kind === 'cue' && pending.cue === id) void attempt(api.midiLearn(null));
    };
  });

  function releaseHint(cue: Cue): string {
    const second = cue.second_trigger.action;
    if (second === 'stop') return t('trigger.release_stop');
    if (second === 'ignore') return t('trigger.release_ignore');
    return t('trigger.release_like');
  }
</script>

{#if single && topLevel}
  <FieldGroup title={t('trigger.keys')}>
    {#snippet aside()}
      {#if linked()}<OwnMark label={t('trigger.own')} />{/if}
    {/snippet}
    <div
      class="grid grid-cols-[minmax(0,1fr)_minmax(0,1fr)] items-center gap-x-2 gap-y-1 border-b border-row-line py-2 text-[13px]"
    >
      <span class="text-[11px] tracking-[0.06em] text-muted-foreground uppercase">{t('cuerow.hotkey')}</span>
      <span class="text-[11px] tracking-[0.06em] text-muted-foreground uppercase">MIDI</span>
      <KeyCapture
        value={single.hotkey}
        label={t('cuerow.hotkey')}
        placeholder={t('trigger.unassigned')}
        clearable={false}
        onchange={(hotkey) => edit.set({ hotkey })}
      />
      <div class="flex min-w-0 items-center gap-1">
        {#if learning}
          <span class="truncate text-xs text-primary motion-safe:animate-pulse">{t('trigger.press_controller')}</span>
          <Button variant="ghost" size="sm" onclick={() => attempt(api.midiLearn(null))}>{t('common.cancel')}</Button>
        {:else if single.midi_trigger}
          <span class="truncate font-mono text-[11.5px] text-fg-2">{fmtMidi(single.midi_trigger)}</span>
          <Button
            variant="ghost"
            size="icon-xs"
            aria-label={t('trigger.remove_midi')}
            onclick={() => edit.set({ midi_trigger: null })}><XMark /></Button
          >
        {:else}
          <Tooltip tip={view.midi.port ? undefined : t('trigger.no_midi_input')}>
            {#snippet child({ props })}
              <span {...props} class="inline-flex">
                <Button
                  size="sm"
                  disabled={!view.midi.port}
                  onclick={() => attempt(api.midiLearn({ kind: 'cue', cue: single.id }))}>{t('trigger.learn')}</Button
                >
              </span>
            {/snippet}
          </Tooltip>
        {/if}
      </div>
      <span class="{hint} col-span-2">{t('trigger.hint', { del: t('key.delete') })}</span>
      {#if info?.hotkey_shared.length}
        <span class="{hint} col-span-2">
          {t('trigger.shared', {
            cues: info.hotkey_shared
              .map((id) => cueById(id))
              .filter((other) => other !== undefined)
              .map(cueLabel)
              .join(', '),
          })}
        </span>
      {/if}
    </div>
    {#if plays}
      <SwitchRow {edit} key="release_trigger" label={t('trigger.release')} text={t('trigger.release_text')} own>
        {#if single.release_trigger}<span class={hint}>{releaseHint(single)}</span>{/if}
      </SwitchRow>
    {/if}
  </FieldGroup>
{/if}

{#if plays}
  <FieldGroup title={t('trigger.second')}>
    <FieldRow label={t('trigger.while_running')} own>
      <SimpleSelect
        aria-label={t('trigger.second')}
        value={(action ?? '') as SecondTrigger['action']}
        placeholder={t('field.mixed')}
        {options}
        onchange={setSecondTrigger}
      />
      <span class={hint}>{t('trigger.second_hint')}</span>
    </FieldRow>
    {#if single?.second_trigger.action === 'exit_loop'}
      <ExitLoopFields
        sub
        exit={single.second_trigger.exit}
        onchange={(exit) => edit.set({ second_trigger: { action: 'exit_loop', exit } })}
      />
    {/if}
  </FieldGroup>
{/if}
