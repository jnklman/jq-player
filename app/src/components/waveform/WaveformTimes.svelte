<script lang="ts">
  import Crosshair from '~icons/ph/crosshair-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Checkbox } from '$lib/components/ui/checkbox';
  import { Label } from '$lib/components/ui/label';
  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { Vamp } from '../../lib/api';
  import { curveOptions } from '../../lib/cue';
  import { t } from '../../lib/i18n/index.svelte';
  import { roundMs } from '../../lib/input';
  import { check, field } from '../../lib/ui';
  import type { Times } from '../../lib/waveform';
  import NumberField from '../fields/NumberField.svelte';
  import MarkList from '../MarkList.svelte';
  import SimpleSelect from '../SimpleSelect.svelte';

  // The waveform dialog's fields: start, end, fades, vamps and marks, each one
  // also settable to `here` (where the preview plays, else the cursor).
  let {
    times,
    here,
    end,
    locked,
    oncommit,
  }: {
    times: Times;
    here: number | null;
    /** Where the cue ends: its end, else the file's. */
    end: number;
    locked: boolean;
    oncommit: (changes: Partial<Times>) => void;
  } = $props();

  const byStart = (vamps: Vamp[]) => vamps.sort((left, right) => left.start - right.start);

  function setVamp(index: number, edge: 'start' | 'end', value: number) {
    oncommit({
      vamps: byStart(times.vamps.map((vamp, i) => (i === index ? { ...vamp, [edge]: value } : { ...vamp }))),
    });
  }

  /** At the position (or after the last vamp), 4 s long. */
  function addVamp() {
    const from = here ?? times.vamps.at(-1)?.end ?? times.start;
    oncommit({
      vamps: byStart([...times.vamps.map((vamp) => ({ ...vamp })), { start: from, end: roundMs(from + 4) }]),
    });
  }
</script>

<!-- Sets a value to the position. -->
{#snippet atHere(tip: string, label: string, apply: (secs: number) => void)}
  <Tooltip
    {tip}
    class={buttonVariants({ size: 'icon' })}
    aria-label={label}
    disabled={here == null || locked}
    onclick={() => here != null && apply(here)}><Crosshair /></Tooltip
  >
{/snippet}

<!-- Seconds to the millisecond, with a button that sets them from the position. -->
{#snippet secs(
  label: string,
  id: string,
  value: number | null,
  set: (secs: number | null) => void,
  toHere: { tip: string; label: string; apply: (secs: number) => void },
)}
  <div class={field}>
    <Label for={id}>{label}</Label>
    <div class="flex gap-1.5">
      <NumberField {id} step={0.01} min={0} optional value={value == null ? null : roundMs(value)} onchange={set} />
      {@render atHere(toHere.tip, toHere.label, toHere.apply)}
    </div>
  </div>
{/snippet}

<fieldset class="m-0 grid min-w-0 grid-cols-2 gap-6 border-0 p-0" disabled={locked}>
  <div class="grid grid-cols-2 content-start gap-3">
    {@render secs(t('wave.start_s'), 'wf-start', times.start, (start) => oncommit({ start: roundMs(start ?? 0) }), {
      tip: t('wave.set_start'),
      label: t('wave.start_to_pos'),
      apply: (at) => oncommit({ start: Math.min(at, end - 0.01) }),
    })}
    {@render secs(
      t('wave.end_s'),
      'wf-end',
      times.end,
      (secs) => oncommit({ end: secs === null ? null : roundMs(secs) }),
      {
        tip: t('wave.set_end'),
        label: t('wave.end_to_pos'),
        apply: (at) => oncommit({ end: Math.max(at, times.start + 0.01) }),
      },
    )}
    {@render secs(
      t('wave.fade_in_s'),
      'wf-fade-in',
      times.fade_in,
      (secs) => oncommit({ fade_in: roundMs(secs ?? 0) }),
      {
        tip: t('wave.set_fade_in'),
        label: t('wave.fade_in_to_pos'),
        apply: (at) => oncommit({ fade_in: roundMs(Math.max(0, at - times.start)) }),
      },
    )}
    {@render secs(
      t('wave.fade_out_s'),
      'wf-fade-out',
      times.fade_out,
      (secs) => oncommit({ fade_out: roundMs(secs ?? 0) }),
      {
        tip: t('wave.set_fade_out'),
        label: t('wave.fade_out_from_pos'),
        apply: (at) => oncommit({ fade_out: roundMs(Math.max(0, end - at)), end_fade: true }),
      },
    )}
    <Label class="{check} self-end pb-2">
      <Checkbox checked={times.end_fade} onCheckedChange={(on) => oncommit({ end_fade: on })} />
      {t('wave.end_fade')}
    </Label>
    <Label class={field}>
      {t('field.curve')}
      <SimpleSelect value={times.curve} options={curveOptions} onchange={(curve) => oncommit({ curve })} />
    </Label>
  </div>

  <!-- The padding keeps the focus rings inside the scrolling box, which clips them. -->
  <ScrollArea class="-m-1 max-h-56 min-w-0"
    ><div class="flex flex-col gap-3 p-1">
      <div class="flex flex-col gap-1.5" role="group" aria-labelledby="wf-vamps-label">
        <span id="wf-vamps-label" class="text-xs text-muted-foreground">{t('wave.vamps_s')}</span>
        {#each times.vamps as vamp, i (i)}
          <div class="flex items-center gap-1.5">
            <span class="w-14 shrink-0 text-xs text-vamp-soft">{t('sound_main.vamp', { number: i + 1 })}</span>
            <NumberField
              aria-label={t('sound_main.vamp_start', { number: i + 1 })}
              step={0.01}
              min={0}
              value={roundMs(vamp.start)}
              onchange={(secs) => setVamp(i, 'start', roundMs(secs))}
            />
            {@render atHere(t('wave.set_vamp_start'), t('wave.vamp_start_to_pos', { number: i + 1 }), (at) =>
              setVamp(i, 'start', at),
            )}
            <span class="text-muted-foreground">–</span>
            <NumberField
              aria-label={t('sound_main.vamp_end', { number: i + 1 })}
              step={0.01}
              min={0}
              value={roundMs(vamp.end)}
              onchange={(secs) => setVamp(i, 'end', roundMs(secs))}
            />
            {@render atHere(t('wave.set_vamp_end'), t('wave.vamp_end_to_pos', { number: i + 1 }), (at) =>
              setVamp(i, 'end', at),
            )}
            <Button
              variant="ghost"
              size="icon-sm"
              class="shrink-0 text-base"
              aria-label={t('sound_main.vamp_remove', { number: i + 1 })}
              onclick={() => oncommit({ vamps: times.vamps.filter((_, index) => index !== i) })}><XMark /></Button
            >
          </div>
        {/each}
        <Tooltip tip={t('wave.add_vamp_tip')} class="{buttonVariants()} self-start" onclick={addVamp}
          >{t('sound_main.add_vamp')}</Tooltip
        >
      </div>
      <div class="flex flex-col gap-1.5" role="group" aria-labelledby="wf-marks-label">
        <span id="wf-marks-label" class="text-xs text-muted-foreground">
          {t('wave.marks_for')}
        </span>
        <MarkList
          marks={times.marks}
          next={here}
          nextTip={t('wave.mark_tip')}
          disabled={locked}
          onchange={(marks) => oncommit({ marks })}
        />
      </div>
    </div></ScrollArea
  >
</fieldset>
