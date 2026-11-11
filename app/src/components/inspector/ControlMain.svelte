<script lang="ts">
  import MagnifyingGlass from '~icons/ph/magnifying-glass';
  import { Button } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import type { ControlKind } from '../../lib/api';
  import { type AnyControl, targetable, targetMode, targetModeLabels, targetModes } from '../../lib/control';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { outputOptions, withMissing } from '../../lib/options';
  import { allCues, previousOf, ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import SimpleSelect from '../SimpleSelect.svelte';
  import EffectRows from './EffectRows.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';

  // The first tab of a control cue: what it acts on, then what it does. A stopwatch cue instead picks its
  // stopwatch. The type is changed in the inspector's head.
  let { edit, cue, targetMissing }: { edit: CueEdit; cue: AnyControl; targetMissing: boolean } = $props();

  const view = $derived(ui.view!);
  const kind = $derived(cue.type as ControlKind);
  let filter = $state('');
  // What the cue can act on: everything but itself, by type.
  const candidates = $derived(allCues().filter((other) => other.id !== cue.id && targetable(kind, other)));
  const shownCandidates = $derived.by(() => {
    const words = filter.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return candidates.filter((other) => {
      const text = `${other.number} ${other.title}`.toLowerCase();
      return words.every((word) => text.includes(word));
    });
  });
  const outputs = $derived(outputOptions(view));

  function setTargetMode(mode: 'previous' | 'cues' | 'all' | 'output') {
    edit.set({
      targets:
        mode === 'all' || mode === 'previous' ? mode : mode === 'cues' ? [] : { output: outputs[0]?.value ?? 'main' },
    });
  }

  function toggleTarget(id: string, on: boolean) {
    if (cue.type === 'stopwatch' || !Array.isArray(cue.targets)) return;
    edit.set({ targets: on ? [...cue.targets, id] : cue.targets.filter((target) => target !== id) });
  }
</script>

{#if cue.type === 'stopwatch'}
  <FieldGroup title={t('stopwatches.watch')}>
    {#snippet aside()}
      <Button variant="link" class="min-h-0 p-0 text-xs" onclick={() => (ui.stopwatchesOpen = true)}
        >{t('control_main.stopwatches')}</Button
      >
    {/snippet}
    <FieldRow label={t('stopwatches.watch')} field="watch">
      <SimpleSelect
        aria-label={t('stopwatches.watch')}
        value={cue.watch ?? ''}
        options={withMissing(
          [
            { value: '', label: t('control_main.none') },
            ...view.show.stopwatches.map((watch) => ({ value: watch.id, label: watch.name })),
          ],
          cue.watch,
          t('control_main.deleted'),
        )}
        onchange={(watch) => edit.set({ watch: watch || null })}
      />
      {#if targetMissing}<span class="text-xs text-stop-soft">{t('control_main.watch_deleted')}</span>{/if}
    </FieldRow>
    <FieldRow label={t('field.action')}>
      <SimpleSelect
        aria-label={t('field.action')}
        value={cue.action}
        options={[
          { value: 'start', label: t('control_main.watch_start') },
          { value: 'stop', label: t('control_main.watch_stop') },
          { value: 'reset', label: t('control_main.watch_reset') },
          { value: 'restart', label: t('control_main.watch_restart') },
        ]}
        onchange={(action) => edit.set({ action })}
      />
      <span class={hint}>{t('control_main.watch_hint')}</span>
    </FieldRow>
  </FieldGroup>
{:else}
  <FieldGroup title={t('control_main.target')}>
    <FieldRow label={t('control_main.acts_on')} field="targets">
      <SimpleSelect
        aria-label={t('control_main.target')}
        value={targetMode(cue.targets)}
        options={targetModes(kind).map((value) => ({ value, label: targetModeLabels[value] }))}
        onchange={setTargetMode}
      />
      {#if cue.targets === 'previous'}
        {@const target = previousOf(cue.id)}
        {#if target}
          <span class={hint}>
            {target.number} – {target.title}
          </span>
        {/if}
      {/if}
    </FieldRow>
    {#if Array.isArray(cue.targets)}
      {@const targets = cue.targets}
      <FieldRow label={t('control_main.cues')} sub>
        <div class="flex flex-col overflow-hidden rounded-lg border border-line">
          <div class="relative border-b border-line">
            <MagnifyingGlass class="pointer-events-none absolute top-1/2 left-2.5 -translate-y-1/2 text-xs text-dim" />
            <Input
              type="search"
              class="min-h-7 rounded-none border-0 bg-transparent pl-7 text-xs"
              placeholder={t('control_main.filter')}
              aria-label={t('control_main.filter_cues')}
              bind:value={filter}
            />
          </div>
          <!-- The padding keeps the checkboxes' focus ring inside the scrolling box, which clips it. -->
          <ScrollArea class="max-h-44">
            <div class="flex flex-col gap-1.5 p-2">
              {#each shownCandidates as other (other.id)}
                <Label class="font-normal text-fg-2">
                  <Checkbox bind:checked={() => targets.includes(other.id), (on) => toggleTarget(other.id, on)} />
                  <span class="font-mono text-primary">{other.number}</span>
                  <span class="truncate">{other.title}</span>
                </Label>
              {:else}
                <span class={hint}
                  >{candidates.length ? t('control_main.no_match') : t('control_main.no_candidates')}</span
                >
              {/each}
            </div>
          </ScrollArea>
        </div>
        {#if targets.length === 0}
          <span class="text-xs text-stop-soft">{t('control_main.no_target')}</span>
        {:else}
          <span class={hint}>{t('control_main.chosen', { count: targets.length })}</span>
        {/if}
      </FieldRow>
    {:else if cue.targets !== 'all' && cue.targets !== 'previous'}
      <FieldRow label={t('field.output')} sub>
        <SimpleSelect
          aria-label={t('field.output')}
          value={cue.targets.output}
          options={outputs}
          onchange={(output) => edit.set({ targets: { output } })}
        />
      </FieldRow>
    {/if}
  </FieldGroup>
  <FieldGroup title={t('control_main.effect')}>
    <EffectRows {edit} />
  </FieldGroup>
{/if}
