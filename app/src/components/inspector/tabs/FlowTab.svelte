<script lang="ts">
  import {
    advanceAt,
    type AdvanceKind,
    advanceLabels,
    advancePoints,
    effectiveAt,
    loopCues,
  } from '../../../lib/advance';
  import type { Cue, CueInfo } from '../../../lib/api';
  import { type CueEdit, MIXED } from '../../../lib/cueEdit';
  import { t } from '../../../lib/i18n/index.svelte';
  import { repeatOptions, withMissing } from '../../../lib/options';
  import { cueById, isTopLevel, previousOf } from '../../../lib/store.svelte';
  import { hint } from '../../../lib/ui';
  import NumberField from '../../fields/NumberField.svelte';
  import SimpleSelect from '../../SimpleSelect.svelte';
  import FieldGroup from '../FieldGroup.svelte';
  import FieldRow from '../FieldRow.svelte';
  import CountRow from '../fields/CountRow.svelte';
  import NumberRow from '../fields/NumberRow.svelte';
  import SwitchRow from '../fields/SwitchRow.svelte';

  // "Flow": pre-wait, when the next cue starts, and how many GOs the playhead stays.
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const cues = $derived(edit.cues);
  const single = $derived(edit.multi ? null : cues[0]);

  const startNext = $derived(new Set(cues.map((cue) => cue.advance.start_next)));
  const on = $derived(startNext.size === 1 && startNext.has(true));
  // The points every cue offers, and the one they share.
  const points = $derived(
    cues.reduce<AdvanceKind[]>(
      (common, cue) => common.filter((kind) => advancePoints(cue).includes(kind)),
      advancePoints(cues[0]),
    ),
  );
  const kinds = $derived(new Set(cues.map((cue) => cue.advance.at.at)));
  const kind = $derived(kinds.size === 1 ? [...kinds][0] : null);
  const secs = $derived.by(() => {
    const values = new Set(cues.map((cue) => ('secs' in cue.advance.at ? cue.advance.at.secs : null)));
    return values.size === 1 ? [...values][0] : null;
  });

  // The loop exits a loop exit cue's next cue can wait for.
  const exitCues = $derived.by(() => {
    if (single?.type !== 'exit_loop') return [];
    if (Array.isArray(single.targets)) return single.targets.flatMap((target) => loopCues(cueById(target)));
    return single.targets === 'previous' ? loopCues(previousOf(single.id)) : [];
  });

  // A point the cue type reads otherwise (e.g. after the fade-in without one).
  const readAs = $derived.by(() => {
    if (!single?.advance.start_next) return null;
    const effective = effectiveAt(single);
    return effective.at === single.advance.at.at ? null : advanceLabels[effective.at];
  });

  // Repeat applies to top-level cues only.
  const topLevel = $derived(cues.every((cue) => isTopLevel(cue.id)));

  function setAt(at: (cue: Cue) => Cue['advance']['at']) {
    edit.set((cue) => ({ advance: { ...cue.advance, at: at(cue) } }));
  }
</script>

<FieldGroup title={t('type.start')}>
  <NumberRow {edit} key="pre_wait" label={t('check.field.pre_wait')} unit="s" min={0} />
</FieldGroup>

<FieldGroup title={t('flow.then')}>
  <SwitchRow
    {edit}
    label={t('flow.follow_cue')}
    text={t('flow.start_next')}
    own
    field="next"
    checked={startNext.size > 1 ? MIXED : on}
    onchange={(start_next) => edit.set((cue) => ({ advance: { ...cue.advance, start_next } }))}
  />
  {#if on}
    <FieldRow label={t('flow.point')} own sub>
      <span class="flex flex-wrap items-center gap-2">
        <SimpleSelect
          class="min-w-44 grow"
          aria-label={t('flow.point')}
          value={kind ?? ''}
          placeholder={t('field.mixed')}
          options={points.map((point) => ({ value: point, label: advanceLabels[point] }))}
          onchange={(next) => setAt((cue) => advanceAt(next as AdvanceKind, cue.advance.at))}
        />
        {#if kind === 'after' || kind === 'before_end'}
          <NumberField
            class="w-28"
            aria-label={kind === 'after' ? t('flow.secs_after_start') : t('flow.secs_before_end')}
            unit="s"
            min={0}
            optional={secs === null}
            placeholder={secs === null ? t('field.mixed') : ''}
            value={secs}
            onchange={(value) => value !== null && setAt((cue) => ({ ...cue.advance.at, secs: value }) as never)}
          />
        {/if}
      </span>
      {#if kind === 'after' || kind === 'before_end'}
        <span class={hint}>{kind === 'after' ? t('flow.after_start') : t('flow.before_end')}</span>
      {/if}
      {#if readAs}<span class={hint}>{t('flow.read_as', { point: readAs })}</span>{/if}
    </FieldRow>
    {#if single && single.advance.at.at === 'loop_exit'}
      {@const at = single.advance.at}
      <FieldRow label={t('flow.exit_of')} sub>
        <SimpleSelect
          aria-label={t('flow.exit_of')}
          value={at.of ?? ''}
          options={withMissing(
            [
              { value: '', label: t('flow.first_exit') },
              ...exitCues.map((exit) => ({ value: exit.id, label: `${exit.number} · ${exit.title}` })),
            ],
            at.of,
            t('flow.not_in_target'),
          )}
          onchange={(of) => setAt(() => ({ at: 'loop_exit', of: of || null }))}
        />
        {#if info?.target_missing}
          <span class="text-xs text-stop-soft">{t('flow.exit_gone')}</span>
        {/if}
      </FieldRow>
    {/if}
  {/if}
</FieldGroup>

{#if topLevel}
  <FieldGroup title={t('actions.group.playhead')}>
    <CountRow {edit} key="passes" label="Repeat" options={repeatOptions} unit="GOs" own field="repeat">
      {#if info?.repeat_blocked}
        <span class="text-xs text-stop-soft">{t('flow.repeat_blocked')}</span>
      {:else if single && single.passes !== 0 && single.second_trigger.action === 'ignore'}
        <span class={hint}>{t('flow.repeat_ignored')}</span>
      {/if}
    </CountRow>
  </FieldGroup>
{/if}
