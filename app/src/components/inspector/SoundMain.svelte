<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { CueInfo, SoundCue } from '../../lib/api';
  import { openWaveform, replaceSoundFile } from '../../lib/commands';
  import { isPlaceholder } from '../../lib/cue';
  import { type CueEdit, isMixed, shown } from '../../lib/cueEdit';
  import { fmtTime } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { roundMs } from '../../lib/input';
  import { loopOptions } from '../../lib/options';
  import { importFor, ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import NumberField from '../fields/NumberField.svelte';
  import MarkList from '../MarkList.svelte';
  import Waveform from '../waveform/Waveform.svelte';
  import FadeRows from './FadeRows.svelte';
  import FieldGroup from './FieldGroup.svelte';
  import FieldRow from './FieldRow.svelte';
  import CountRow from './fields/CountRow.svelte';

  // The first tab of sound cues: file and waveform, range and fades, loop, vamps and marks. With several
  // sound cues only what they all can share (range, fades, loop).
  let { edit, info }: { edit: CueEdit; info: CueInfo | undefined } = $props();

  const cue = $derived(edit.multi ? null : (edit.cues[0] as SoundCue));
  const start = $derived(edit.value<number>('start'));
  const end = $derived(edit.value<number | null>('end'));
  const vamps = $derived(edit.cues.some((other) => other.type === 'sound' && other.vamps.length > 0));
  // Previewing never touches the show, so it works in show mode too.
  const preview = $derived(cue && ui.runtime?.preview?.cue === cue.id ? ui.runtime.preview : null);

  /** Where "+ Mark" puts a mark: where the preview plays, else 1 s after the last mark (or the start). */
  const nextMark = $derived(
    cue ? (preview?.playing ? roundMs(preview.position) : (cue.marks.at(-1) ?? cue.start) + 1) : null,
  );

  /** Adds a vamp after the last one (or at the start), 4 s long. */
  function addVamp(cue: SoundCue) {
    const from = cue.vamps.at(-1)?.end ?? cue.start;
    edit.set({ vamps: [...cue.vamps, { start: from, end: from + 4 }] });
  }

  function setVamp(cue: SoundCue, index: number, edge: 'start' | 'end', value: number) {
    const next = cue.vamps.map((vamp, i) => (i === index ? { ...vamp, [edge]: value } : vamp));
    edit.set({ vamps: next.sort((left, right) => left.start - right.start) });
  }
</script>

{#if cue}
  <FieldGroup title={t('sound_main.file')}>
    {#snippet aside()}
      {#if info?.file_duration != null}<span class="font-mono tabular-nums">{fmtTime(info.file_duration)}</span>{/if}
    {/snippet}
    <FieldRow field="file">
      {#if isPlaceholder(cue)}
        <div class="flex items-center gap-2.5">
          <span class="min-w-0 grow text-xs text-muted-foreground">{t('sound_main.placeholder')}</span>
          <Button onclick={() => replaceSoundFile(cue)}>{t('cuemenu.choose_file')}</Button>
        </div>
      {:else}
        <div class="flex gap-1.5">
          <Tooltip tip={cue.file} tabindex={0}>
            {#snippet child({ props })}
              <Input
                {...props}
                class="min-w-0 grow font-mono text-xs"
                aria-label={t('sound_main.file')}
                value={cue.file}
                readonly
              />
            {/snippet}
          </Tooltip>
          <Button onclick={() => replaceSoundFile(cue)}>{t('sound_main.replace')}</Button>
        </div>
        {#if info?.missing}<span class="text-xs text-stop-soft">{t('inspector.notice.missing')}</span>{/if}
        {#if info?.importing}
          <span class={hint}>
            {t('sound_main.copying')}
            {Math.round((importFor(cue.file) ?? 0) * 100)} %
          </span>
        {/if}
        {#if info?.case_mismatch}
          <span class="text-xs text-stop-soft">{t('inspector.notice.case_mismatch', { name: info.case_mismatch })}</span
          >
        {/if}
        {#if !info?.missing && !info?.importing}
          <Waveform
            {cue}
            version={info?.file_version ?? 0}
            locked={edit.locked}
            onexpand={() => openWaveform(cue.id)}
            onchange={(times) => edit.set(times)}
          />
        {/if}
      {/if}
    </FieldRow>
  </FieldGroup>
{/if}

<FieldGroup title={t('sound_main.range_fades')}>
  <FieldRow label={t('sound_main.range')} field="range">
    <span class="flex flex-wrap items-center gap-2">
      <NumberField
        class="w-28"
        aria-label={t('check.field.start')}
        unit="s"
        min={0}
        optional={isMixed(start)}
        placeholder={isMixed(start) ? t('field.mixed') : ''}
        value={shown(start)}
        onchange={(value) => value !== null && edit.set({ start: value })}
      />
      <span class={hint}>{t('sound_main.to')}</span>
      <NumberField
        class="w-28"
        aria-label={t('overrides.end')}
        unit="s"
        min={0}
        optional
        placeholder={isMixed(end) ? t('field.mixed') : t('overrides.file_end')}
        value={shown(end)}
        onchange={(value) => edit.set({ end: value })}
      />
    </span>
    {#each info?.timing ?? [] as warning (warning.message)}
      <span class="text-xs text-stop-soft">{warning.message}</span>
    {/each}
  </FieldRow>
  <FadeRows {edit} />
</FieldGroup>

<FieldGroup title={cue ? t('sound_main.loop_vamps') : 'Loop'}>
  <CountRow
    {edit}
    key="loop"
    label="Loop"
    options={loopOptions}
    unit="×"
    disabled={vamps}
    tip={vamps ? t('sound_main.vamps_no_loop_tip') : undefined}
  >
    {#if vamps && cue}<span class={hint}>{t('sound_main.vamps_no_loop')}</span>{/if}
  </CountRow>
  {#if cue}
    <FieldRow label="Vamps">
      {#each cue.vamps as vamp, i (i)}
        <div class="flex items-center gap-1.5">
          <span class="w-14 shrink-0 text-xs text-vamp-soft">{t('sound_main.vamp', { number: i + 1 })}</span>
          <NumberField
            class="w-24"
            aria-label={t('sound_main.vamp_start', { number: i + 1 })}
            unit="s"
            min={0}
            value={vamp.start}
            onchange={(secs) => setVamp(cue, i, 'start', secs)}
          />
          <span class="text-muted-foreground">–</span>
          <NumberField
            class="w-24"
            aria-label={t('sound_main.vamp_end', { number: i + 1 })}
            unit="s"
            min={0}
            value={vamp.end}
            onchange={(secs) => setVamp(cue, i, 'end', secs)}
          />
          <Button
            variant="ghost"
            size="icon-sm"
            class="shrink-0 text-base"
            aria-label={t('sound_main.vamp_remove', { number: i + 1 })}
            onclick={() => edit.set({ vamps: cue.vamps.filter((_, index) => index !== i) })}><XMark /></Button
          >
        </div>
      {/each}
      <Button size="sm" class="self-start" onclick={() => addVamp(cue)}>{t('sound_main.add_vamp')}</Button>
      <span class={hint}>{t('sound_main.vamp_hint')}</span>
    </FieldRow>
    <FieldRow label={t('sound_main.marks')}>
      <MarkList
        marks={cue.marks}
        next={nextMark}
        nextTip={t('sound_main.next_mark_tip')}
        disabled={edit.locked}
        onchange={(marks) => edit.set({ marks })}
      />
      <span class={hint}>{t('sound_main.marks_hint')}</span>
    </FieldRow>
  {/if}
</FieldGroup>
