<script lang="ts">
  import ArrowsOut from '~icons/ph/arrows-out-simple-bold';
  import { buttonVariants } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { SoundCue } from '../../lib/api';
  import { fmtPosition } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { activeFor, ui } from '../../lib/store.svelte';
  import {
    copyTimes,
    dragHandle,
    fadeSegments,
    type Handle,
    overviewPeaks,
    type Peaks,
    type Positions,
    settleTimes,
    toggleMark,
  } from '../../lib/waveform';

  // The inspector's waveform of a sound cue: start, end, vamps and marks can be
  // dragged, a double-click sets or removes a mark. Fades show but are set in the fields.
  let {
    cue,
    version,
    locked,
    onchange,
    onexpand,
  }: {
    cue: SoundCue;
    version: number;
    locked: boolean;
    onchange: (positions: Positions) => void;
    /** Opens the big waveform editor. */
    onexpand?: () => void;
  } = $props();

  let data = $state<Peaks | null>(null);
  let error = $state<string | null>(null);
  let area = $state<HTMLElement>();
  let areaWidth = $state<number>(0);
  // While dragging, the edited copy; otherwise the cue's own values.
  let dragging = $state<{ handle: Handle; positions: Positions } | null>(null);
  const shown: Positions = $derived(dragging?.positions ?? cue);

  // Primitive, so edits that leave the file alone do not reload.
  const mediaDir = $derived(ui.view?.media_dir);
  const file = $derived(cue.file);

  $effect(() => {
    if (!mediaDir) return;
    data = null;
    error = null;
    // A result for a file that has since been replaced is dropped.
    let stale = false;
    overviewPeaks(mediaDir, file, version).then(
      (peaks) => {
        if (!stale) data = peaks;
      },
      (e) => {
        if (!stale) error = String(e);
      },
    );
    return () => {
      stale = true;
    };
  });

  // One vertical stroke per peak, mirrored around the middle.
  const path = $derived(
    data
      ? data.peaks
          .map((peak, i) => {
            const x = ((i + 0.5) / data!.peaks.length) * 1000;
            const half = Math.max(0.5, peak * 48);
            return `M${x.toFixed(1)} ${(50 - half).toFixed(1)}V${(50 + half).toFixed(1)}`;
          })
          .join('')
      : '',
  );

  const duration = $derived(data?.duration ?? 0);
  const pct = (secs: number) => (duration > 0 ? Math.max(0, Math.min(100, secs / duration)) : 0);
  const pctString = (secs: number) => `${pct(secs) * 100}%`;
  const end = $derived(Math.min(shown.end ?? duration, duration));

  // Fade-in and end fade as a gain envelope over start–end; what the fade takes away is shaded.
  const fades = $derived.by(() => {
    if (duration <= 0) return { area: '', line: '' };
    const segments = fadeSegments({ ...cue, start: shown.start }, end, ui.view?.show.level_limits.min_db ?? -60);
    const x = (secs: number) => ((secs / duration) * 1000).toFixed(1);
    let area = '';
    let line = '';
    for (const [from, to, gain] of segments) {
      const points = Array.from({ length: 33 }, (_, i) => {
        const pos = i / 32;
        return `${x(from + pos * (to - from))} ${(100 - gain(pos) * 100).toFixed(1)}`;
      }).join('L');
      line += `M${points}`;
      area += `M${x(from)} 0L${points}L${x(to)} 0Z`;
    }
    return { area, line };
  });

  // Where the running cue is, mapped from its progress within start–end.
  const position = $derived.by(() => {
    const running = activeFor(cue.id);
    return running ? shown.start + running.progress * (end - shown.start) : null;
  });

  function timeAt(clientX: number): number {
    if (!area || duration <= 0) return 0;
    const rect = area.getBoundingClientRect();
    const secs = Math.max(0, Math.min(1, (clientX - rect.left) / rect.width)) * duration;
    return Math.round(secs * 100) / 100;
  }

  function grab(e: PointerEvent, handle: Handle) {
    if (locked) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    dragging = { handle, positions: copyTimes(cue) };
  }

  function move(e: PointerEvent) {
    if (dragging) dragHandle(dragging.positions, dragging.handle, timeAt(e.clientX), duration);
  }

  function drop() {
    if (!dragging) return;
    const { start, end, vamps, marks } = dragging.positions;
    const positions = { start, end, vamps, marks };
    dragging = null;
    settleTimes(positions, duration, 0.005);
    // A plain click on a handle moves nothing: no update, no undo step.
    const before: Positions = { start: cue.start, end: cue.end, vamps: cue.vamps, marks: cue.marks };
    if (JSON.stringify(positions) === JSON.stringify(before)) return;
    onchange(positions);
  }

  function onMarkClick(e: MouseEvent) {
    if (locked || duration <= 0) return;
    // A mark within a few pixels is removed instead.
    const near = (8 / (area?.clientWidth || 1)) * duration;
    const { start, end, vamps } = cue;
    onchange({ start, end, vamps, marks: toggleMark(cue.marks, timeAt(e.clientX), near) });
  }

  // Pointer only, no tab stops: the fields below the waveform set the same values.
  const handle = 'absolute inset-y-0 w-3 -translate-x-1/2 touch-none';
</script>

<div class="flex flex-col gap-1">
  <div
    bind:this={area}
    bind:offsetWidth={areaWidth}
    class="relative h-20 overflow-hidden rounded-lg border border-line bg-canvas select-none"
    role="presentation"
    onpointermove={move}
    onpointerup={drop}
    onpointercancel={drop}
    ondblclick={onMarkClick}
  >
    {#if data}
      <svg class="absolute inset-0 h-full w-full" viewBox="0 0 1000 100" preserveAspectRatio="none" aria-hidden="true">
        <path d={path} class="stroke-fg-2" stroke-width="1" vector-effect="non-scaling-stroke" />
        <path d={fades.area} class="fill-black/45" />
        <path d={fades.line} class="fill-none stroke-fade" stroke-width="1.5" vector-effect="non-scaling-stroke" />
      </svg>
      {#each shown.vamps as vamp, i (i)}
        <span
          class="absolute inset-y-0 border-x border-vamp bg-vamp/20"
          style:left={pctString(vamp.start)}
          style:width="calc({pctString(vamp.end)} - {pctString(vamp.start)})"
        ></span>
      {/each}
      <!-- Outside start–end is dimmed. -->
      <span class="absolute inset-y-0 left-0 bg-black/55" style:width={pctString(shown.start)}></span>
      <span class="absolute inset-y-0 right-0 bg-black/55" style:left={pctString(end)}></span>
      {#each shown.marks as mark, i (i)}
        <span class="absolute inset-y-0 w-px bg-primary/80" style:left={pctString(mark)}></span>
      {/each}
      {#if position != null}
        <span class="absolute inset-y-0 left-0 w-0.5 bg-go" style:transform="translateX({pct(position) * areaWidth}px)"
        ></span>
      {/if}

      {#if !locked}
        <Tooltip
          tabindex={-1}
          class="{handle} cursor-ew-resize"
          style="left: {pctString(shown.start)}"
          tip={t('wave.drag_start')}
          aria-label={t('check.field.start')}
          onpointerdown={(e) => grab(e, { kind: 'start' })}
          ><span class="mx-auto block h-full w-0.5 bg-go"></span></Tooltip
        >
        <Tooltip
          tabindex={-1}
          class="{handle} cursor-ew-resize"
          style="left: {pctString(end)}"
          tip={t('wave.drag_end')}
          aria-label={t('overrides.end')}
          onpointerdown={(e) => grab(e, { kind: 'end' })}
          ><span class="mx-auto block h-full w-0.5 bg-stop-soft"></span></Tooltip
        >
        {#each shown.vamps as vamp, i (i)}
          <Tooltip
            tabindex={-1}
            class="{handle} cursor-ew-resize"
            style="left: {pctString(vamp.start)}"
            tip={t('sound_main.vamp_start', { number: i + 1 })}
            aria-label={t('sound_main.vamp_start', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'vamp_start', index: i })}
          ></Tooltip>
          <Tooltip
            tabindex={-1}
            class="{handle} cursor-ew-resize"
            style="left: {pctString(vamp.end)}"
            tip={t('sound_main.vamp_end', { number: i + 1 })}
            aria-label={t('sound_main.vamp_end', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'vamp_end', index: i })}
          ></Tooltip>
        {/each}
        {#each shown.marks as mark, i (i)}
          <Tooltip
            tabindex={-1}
            class="{handle} top-auto h-3 cursor-ew-resize"
            style="left: {pctString(mark)}"
            tip={t('wave.mark_at', { time: fmtPosition(mark) })}
            aria-label={t('wave.mark', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'mark', index: i })}
            ><span class="mx-auto block size-2 rotate-45 bg-primary"></span></Tooltip
          >
        {/each}
      {/if}
    {:else if error}
      <span class="absolute inset-0 grid place-items-center text-xs text-stop-soft">{error}</span>
    {:else}
      <span class="absolute inset-0 grid place-items-center text-xs text-muted-foreground">{t('wave.computing')}</span>
    {/if}
  </div>
  {#if !locked && data}
    <div class="flex items-center justify-between gap-2">
      <span class="text-[11px] text-muted-foreground">
        {t('wave.hint')}
      </span>
      {#if onexpand}
        <Tooltip tip={t('wave.expand_tip')} class="{buttonVariants({ size: 'xs' })} shrink-0" onclick={onexpand}
          ><ArrowsOut /> {t('wave.expand')}</Tooltip
        >
      {/if}
    </div>
  {/if}
</div>
