<script lang="ts">
  import Slider from '$lib/components/ui/slider/slider.svelte';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';

  import type { PositionTo } from '../../lib/api';
  import { panCurveOptions, panLabel, positionLabels } from '../../lib/control';
  import { curveOptions } from '../../lib/cue';
  import type { CueEdit } from '../../lib/cueEdit';
  import { t } from '../../lib/i18n/index.svelte';
  import { clamp } from '../../lib/input';
  import {
    devampModeOptions,
    exitLevelOptions,
    groupModeOptions,
    optionsOf,
    pauseActionOptions,
    volumeActionOptions,
  } from '../../lib/options';
  import { RATE_MAX, RATE_MIN, TRANSPOSE_MAX } from '../../lib/speed';
  import { hint } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';
  import ExitLoopFields from './ExitLoopFields.svelte';
  import FieldRow from './FieldRow.svelte';
  import NumberRow from './fields/NumberRow.svelte';
  import SelectRow from './fields/SelectRow.svelte';

  // What a control, wait or group cue does, for one cue or several of the same type (see `MULTI_TYPES`).
  let { edit }: { edit: CueEdit } = $props();

  const first = $derived(edit.cues[0]);
  const single = $derived(edit.multi ? null : first);
  const allFadeTo = $derived(edit.cues.every((cue) => cue.type === 'volume' && cue.action === 'fade_to'));

  function setPositionKind(to: PositionTo['to']) {
    if (single?.type !== 'position') return;
    const secs = 'secs' in single.to ? single.to.secs : 0;
    const next: PositionTo =
      to === 'absolute' || to === 'relative' ? { to, secs } : to === 'mark' ? { to, index: 1 } : { to };
    edit.set({ to: next });
  }
</script>

{#if first.type === 'start_stop' && single?.type === 'start_stop'}
  <FieldRow label={t('field.action')}>
    <ToggleGroup.Root
      type="single"
      variant="outline"
      size="sm"
      aria-label={t('field.action')}
      bind:value={() => single.action, (action) => action && edit.set({ action })}
    >
      <ToggleGroup.Item value="start">{t('effect.start')}</ToggleGroup.Item>
      <ToggleGroup.Item value="stop">{t('effect.stop')}</ToggleGroup.Item>
    </ToggleGroup.Root>
    {#if single.action === 'start'}<span class={hint}>{t('effect.paused_resume')}</span>{/if}
  </FieldRow>
  {#if single.action === 'start'}
    <NumberRow
      {edit}
      key="from"
      label={t('effect.start_from')}
      unit="s"
      min={0}
      optional
      placeholder={t('effect.start_point')}
      sub
    />
  {/if}
{:else if first.type === 'pause'}
  <SelectRow {edit} key="action" label={t('field.action')} options={pauseActionOptions} />
{:else if first.type === 'volume'}
  <SelectRow {edit} key="action" label={t('field.action')} options={volumeActionOptions} />
  {#if allFadeTo}
    <NumberRow {edit} key="volume_db" label={t('effect.target_level')} unit="dB" step={0.5} sub />
  {/if}
  <NumberRow
    {edit}
    key="duration"
    label={t('field.duration')}
    unit="s"
    min={0}
    optional
    placeholder={t('control.target_fade')}
  />
  <SelectRow {edit} key="curve" label={t('field.curve')} options={curveOptions} />
{:else if first.type === 'pan'}
  {#if single?.type === 'pan'}
    <FieldRow label={t('control.pan')}>
      <Slider
        type="single"
        aria-label={t('control.pan')}
        step={0.01}
        min={-1}
        max={1}
        format={panLabel}
        disabled={edit.locked}
        value={single.pan}
        onValueCommit={(pan) => edit.set({ pan: clamp(pan, -1, 1) })}
        reset={0}
      />
    </FieldRow>
  {:else}
    <NumberRow {edit} key="pan" label={t('control.pan')} min={-1} max={1} placeholder="−1 … +1" />
  {/if}
  <NumberRow {edit} key="duration" label={t('field.duration')} unit="s" min={0} />
  <SelectRow {edit} key="curve" label={t('field.curve')} options={panCurveOptions} />
{:else if first.type === 'rate'}
  <NumberRow
    {edit}
    key="rate"
    label={t('control.rate')}
    unit="%"
    step={1}
    min={RATE_MIN}
    max={RATE_MAX}
    optional
    placeholder={t('effect.unchanged')}
  />
  <NumberRow
    {edit}
    key="transpose"
    label={t('effect.transposition')}
    unit={t('speed.semitones')}
    step={0.5}
    min={-TRANSPOSE_MAX}
    max={TRANSPOSE_MAX}
    optional
    placeholder={t('effect.unchanged')}
  />
  <NumberRow {edit} key="duration" label={t('field.duration')} unit="s" min={0} />
  <SelectRow {edit} key="curve" label={t('field.curve')} options={panCurveOptions} />
  <FieldRow>
    <span class={hint}>{t('effect.rate_hint')}</span>
  </FieldRow>
{:else if first.type === 'position' && single?.type === 'position'}
  {@const to = single.to}
  <FieldRow label={t('effect.jump')}>
    <SimpleSelect
      aria-label={t('effect.jump')}
      value={to.to}
      options={optionsOf(positionLabels)}
      onchange={setPositionKind}
    />
    <span class={hint}>{t('effect.jump_playlists')}</span>
  </FieldRow>
  {#if to.to === 'absolute' || to.to === 'relative'}
    <FieldRow label={to.to === 'absolute' ? t('effect.at') : t('effect.by')} sub>
      <NumberField
        class="w-28"
        aria-label={to.to === 'absolute' ? t('effect.secs_in_file') : t('effect.secs_negative')}
        unit="s"
        min={to.to === 'absolute' ? 0 : undefined}
        value={to.secs}
        onchange={(secs) => edit.set({ to: { to: to.to, secs } })}
      />
      {#if to.to === 'relative'}<span class={hint}>{t('effect.negative_back')}</span>{/if}
    </FieldRow>
  {:else if to.to === 'mark'}
    <FieldRow label={t('effect.mark_no')} sub>
      <NumberField
        class="w-28"
        aria-label={t('effect.mark_no')}
        step={1}
        min={1}
        integer
        value={to.index}
        onchange={(index) => edit.set({ to: { to: 'mark', index } })}
      />
    </FieldRow>
  {/if}
  <NumberRow {edit} key="crossfade_ms" label={t('check.field.crossfade')} unit="ms" step={5} min={0} />
{:else if first.type === 'exit_loop'}
  {#if single?.type === 'exit_loop'}
    <ExitLoopFields
      exit={{ mode: single.mode, level: single.level, stop: single.stop }}
      onchange={(exit) => edit.set(exit)}
    />
  {:else}
    <SelectRow {edit} key="mode" label={t('exitloop.exit')} options={devampModeOptions} />
    <SelectRow {edit} key="level" label={t('exitloop.level')} options={exitLevelOptions} />
  {/if}
{:else if first.type === 'reset'}
  <NumberRow {edit} key="duration" label={t('field.duration')} unit="s" min={0}>
    <span class={hint}>{t('effect.reset_hint')}</span>
  </NumberRow>
{:else if first.type === 'wait'}
  <NumberRow {edit} key="duration" label={t('field.duration')} unit="s" min={0}>
    <span class={hint}>{t('effect.wait_hint')}</span>
  </NumberRow>
{:else if first.type === 'group'}
  <SelectRow {edit} key="mode" label={t('type.start')} options={groupModeOptions} />
{/if}
