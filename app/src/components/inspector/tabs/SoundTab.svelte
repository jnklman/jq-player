<script lang="ts">
  import Slider from '$lib/components/ui/slider/slider.svelte';

  import type { CueInfo } from '../../../lib/api';
  import { withoutNormalization } from '../../../lib/cue';
  import { type CueEdit, isMixed } from '../../../lib/cueEdit';
  import { dbToFader, faderSteps, faderToDb, roundFaderDb } from '../../../lib/fader';
  import { fmtDb, fmtPercent } from '../../../lib/format';
  import { t } from '../../../lib/i18n/index.svelte';
  import { clamp } from '../../../lib/input';
  import { clampRate, RATE_MAX, RATE_MIN, rateLabel, TRANSPOSE_MAX } from '../../../lib/speed';
  import { ui } from '../../../lib/store.svelte';
  import { hint } from '../../../lib/ui';
  import FieldGroup from '../FieldGroup.svelte';
  import FieldRow from '../FieldRow.svelte';
  import NumberRow from '../fields/NumberRow.svelte';
  import SwitchRow from '../fields/SwitchRow.svelte';
  import Normalization from '../Normalization.svelte';

  // "Sound" of sound and playlist cues: level and pan, normalization, rate and pitch. One cue gets sliders
  // that running cues follow while dragged; several cues get number fields.
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const view = $derived(ui.view!);
  const limits = $derived(view.show.level_limits);
  const single = $derived(edit.multi ? null : edit.cues[0]);
  const rate = $derived(edit.value<number>('rate'));
</script>

<FieldGroup title={t('sound_tab.level_pan')}>
  {#if single?.type === 'sound' || single?.type === 'playlist'}
    {@const cue = single}
    <FieldRow label={t('control.volume')} field="volume">
      <Slider
        type="single"
        aria-label={t('control.volume')}
        step={faderSteps(limits, cue.volume_db)}
        min={0}
        max={1}
        value={dbToFader(cue.volume_db, limits)}
        disabled={edit.locked}
        format={(fader) => fmtDb(faderToDb(fader, limits))}
        onValueChange={(fader) => edit.set({ volume_db: roundFaderDb(faderToDb(fader, limits)) }, true)}
        onValueCommit={() => edit.set({})}
        reset={dbToFader(0, limits)}
      />
      {#if info?.level_silenced}
        <span class="text-xs text-stop-soft">{t('sound_tab.silenced')}</span>
      {/if}
      {#if info?.level_capped}
        <span class="text-xs text-stop-soft">{t('sound_tab.capped', { level: fmtDb(limits.max_db) })}</span>
      {/if}
    </FieldRow>
    <FieldRow label={t('control.pan')}>
      <Slider
        type="single"
        aria-label={t('control.pan')}
        step={0.01}
        min={-1}
        max={1}
        format={(pan) => fmtPercent(pan)}
        disabled={edit.locked}
        value={cue.pan}
        onValueChange={(pan) => edit.set({ pan: clamp(pan, -1, 1) }, true)}
        onValueCommit={() => edit.set({})}
        reset={0}
      />
    </FieldRow>
  {:else}
    <NumberRow {edit} key="volume_db" label={t('control.volume')} unit="dB" step={0.5} max={limits.max_db} />
    <NumberRow {edit} key="pan" label={t('control.pan')} min={-1} max={1} placeholder="−1 … +1" />
  {/if}
</FieldGroup>

{#if single?.type === 'sound'}
  <FieldGroup title={t('showmenu.normalization')}>
    <Normalization cue={single} {info} onreset={() => edit.set({ normalize_db: null })} />
  </FieldGroup>
{:else if single?.type === 'playlist'}
  <FieldGroup title={t('showmenu.normalization')}>
    <Normalization cue={single} {info} onreset={() => edit.set({ items: withoutNormalization(single) })} />
  </FieldGroup>
{/if}

<FieldGroup title={t('sound_tab.tempo_pitch')}>
  {#if single && !isMixed(rate)}
    <FieldRow label={t('control.rate')}>
      <Slider
        type="single"
        aria-label={t('control.rate')}
        step={1}
        min={RATE_MIN}
        max={RATE_MAX}
        format={rateLabel}
        disabled={edit.locked}
        value={rate}
        onValueChange={(next) => edit.set({ rate: clampRate(Math.round(next)) }, true)}
        onValueCommit={() => edit.set({})}
        reset={100}
      />
    </FieldRow>
  {:else}
    <NumberRow {edit} key="rate" label={t('control.rate')} unit="%" step={1} min={RATE_MIN} max={RATE_MAX} />
  {/if}
  <NumberRow
    {edit}
    key="transpose"
    label={t('effect.transposition')}
    unit={t('speed.semitones')}
    step={0.5}
    min={-TRANSPOSE_MAX}
    max={TRANSPOSE_MAX}
  />
  <SwitchRow {edit} key="preserve_pitch" label={t('sound_tab.pitch')} text={t('sound_tab.preserve_pitch')} />
  {#if !isMixed(rate) && rate !== 100}
    <FieldRow>
      <span class={hint}>{t('sound_tab.rate_hint', { rate: rateLabel(rate) })}</span>
    </FieldRow>
  {/if}
</FieldGroup>
