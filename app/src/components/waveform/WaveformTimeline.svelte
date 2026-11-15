<script lang="ts">
  import { tick, untrack } from 'svelte';

  import { ScrollArea } from '$lib/components/ui/scroll-area';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api } from '../../lib/api';
  import { fmtPosition } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { roundMs } from '../../lib/input';
  import { PeakTiles } from '../../lib/peakTiles.svelte';
  import { ui } from '../../lib/store.svelte';
  import { tw } from '../../lib/ui';
  import {
    copyTimes,
    dragHandle,
    fadeSegments,
    type Handle,
    type Peaks,
    rulerLabel,
    rulerTicks,
    settleTimes,
    type Times,
    toggleMark,
  } from '../../lib/waveform';
  import { drawWaveform } from '../../lib/waveformCanvas';

  // The waveform dialog's zoomable waveform: a time ruler, the waveform with
  // start, end, fades, vamps and marks as handles, the click cursor and the
  // preview position. Edits go out through `oncommit`; while a handle is
  // dragged, `ondrag` gets the dragged copy as it changes.
  let {
    mediaDir,
    file,
    version,
    fileDuration,
    times,
    locked,
    playhead,
    cursor = $bindable(null),
    zoom = $bindable(1),
    maxZoom = $bindable(1),
    duration = $bindable(0),
    oncommit,
    ondrag,
    onscrub,
  }: {
    mediaDir: string;
    file: string;
    /** Grows when the file changes on disk: read anew. */
    version: number;
    /** Known length of the file, for the overview's resolution. */
    fileDuration: number | null | undefined;
    times: Times;
    locked: boolean;
    /** Where the preview plays; null while it does not. */
    playhead: number | null;
    /** Click position in the waveform. */
    cursor?: number | null;
    zoom?: number;
    /** Closest zoom for this file (out). */
    maxZoom?: number;
    /** Length of the file once read (out). */
    duration?: number;
    oncommit: (times: Partial<Times>) => void;
    /** The dragged copy of `times` while a handle moves; null once it is let go. */
    ondrag?: (times: Times | null) => void;
    /** The cursor was placed (a click or a scrub ended). */
    onscrub?: (secs: number) => void;
  } = $props();

  /** Height of the time ruler above the waveform in px (`top-5`, `h-5` in the markup). */
  const RULER = 20;
  /** Closest zoom. */
  const MAX_PX_PER_SEC = 2000;
  /** Peaks requested per second of audio; the backend has about 190 (one per 256 frames at 48 kHz). */
  const PEAKS_PER_SEC = 200;
  const viewportId = $props.id();

  // --- Peaks

  // Raw: the arrays are large, and a deep proxy would slow down drawing.
  let data = $state.raw<Peaks | null>(null);
  let error = $state<string | null>(null);
  const tiles = new PeakTiles();

  $effect(() => {
    void version;
    // The length only sets the resolution; it arriving later must not reload.
    const seconds = untrack(() => fileDuration) ?? 600;
    const buckets = Math.round(Math.min(200_000, Math.max(4000, seconds * PEAKS_PER_SEC)));
    data = null;
    error = null;
    tiles.reset(mediaDir, file);
    let current = true;
    api.waveform(mediaDir, file, buckets).then(
      (peaks) => current && (data = peaks),
      (e) => current && (error = String(e)),
    );
    return () => {
      current = false;
    };
  });

  // --- Geometry

  let scroller = $state<HTMLElement | null>(null);
  let scrollbar = $state<HTMLElement | null>(null);
  let content = $state<HTMLElement>();
  let canvas = $state<HTMLCanvasElement>();
  let viewWidth = $state(0);
  let viewHeight = $state(0);
  let scrollLeft = $state(0);
  let scrollbarDragging = false;
  let scrollbarDragX = 0;
  let scrollbarDragLeft = 0;

  // The dragged copy, deeply reactive here; `ondrag` gets snapshots of it.
  let drag = $state<Times | null>(null);
  $effect(() => {
    ondrag?.(drag ? $state.snapshot(drag) : null);
  });
  const shown = $derived(drag ?? times);
  $effect(() => {
    duration = data?.duration ?? 0;
  });
  const contentWidth = $derived(viewWidth * zoom);
  const scrollRange = $derived(Math.max(0, contentWidth - viewWidth));
  const thumbWidth = $derived(viewWidth > 0 ? Math.min(viewWidth, Math.max(18, viewWidth / zoom)) : 0);
  const thumbLeft = $derived(
    scrollRange > 0 ? (Math.min(scrollRange, Math.max(0, scrollLeft)) / scrollRange) * (viewWidth - thumbWidth) : 0,
  );
  const pxPerSec = $derived(duration > 0 ? contentWidth / duration : 0);
  $effect(() => {
    maxZoom = Math.max(1, (duration * MAX_PX_PER_SEC) / Math.max(1, viewWidth));
  });
  const end = $derived(Math.min(shown.end ?? duration, duration));
  const px = (secs: number) => `${Math.max(0, Math.min(contentWidth, secs * pxPerSec))}px`;

  function timeAt(clientX: number): number {
    if (!content || pxPerSec <= 0) return 0;
    const secs = (clientX - content.getBoundingClientRect().left) / pxPerSec;
    return roundMs(Math.max(0, Math.min(duration, secs)));
  }

  /** Zooms around `clientX` (default: the middle of the view). */
  export async function setZoom(next: number, clientX?: number) {
    if (!scroller) return;
    const offset = clientX == null ? viewWidth / 2 : clientX - scroller.getBoundingClientRect().left;
    const fraction = contentWidth > 0 ? (scroller.scrollLeft + offset) / contentWidth : 0;
    zoom = Math.max(1, Math.min(maxZoom, next));
    await tick();
    scroller.scrollLeft = fraction * contentWidth - offset;
    scrollLeft = scroller.scrollLeft;
  }

  function scrollToThumbPosition(clientX: number) {
    if (!scroller || !scrollbar) return;
    const trackLeft = scrollbar.getBoundingClientRect().left;
    const travel = viewWidth - thumbWidth;
    if (travel <= 0) return;
    const thumbCenter = Math.max(0, Math.min(travel, clientX - trackLeft - thumbWidth / 2));
    scroller.scrollLeft = (thumbCenter / travel) * scrollRange;
  }

  function onScrollbarPointerDown(e: PointerEvent) {
    if (e.button !== 0 || !scroller) return;
    e.stopPropagation();
    if (e.target === e.currentTarget) {
      e.preventDefault();
      scrollToThumbPosition(e.clientX);
    }
    e.preventDefault();
    scrollbarDragging = true;
    scrollbarDragX = e.clientX;
    scrollbarDragLeft = scroller.scrollLeft;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function moveThumb(e: PointerEvent) {
    if (!scrollbarDragging || !scroller) return;
    e.stopPropagation();
    const travel = viewWidth - thumbWidth;
    if (travel > 0) {
      scroller.scrollLeft = scrollbarDragLeft + ((e.clientX - scrollbarDragX) / travel) * scrollRange;
    }
  }

  function endThumbDrag(e: PointerEvent) {
    e.stopPropagation();
    scrollbarDragging = false;
  }

  function onScrollbarKeydown(e: KeyboardEvent) {
    if (!scroller) return;
    const step = e.shiftKey ? viewWidth : 40;
    switch (e.key) {
      case 'ArrowLeft':
        scroller.scrollLeft -= step;
        break;
      case 'ArrowRight':
        scroller.scrollLeft += step;
        break;
      case 'PageUp':
        scroller.scrollLeft -= viewWidth;
        break;
      case 'PageDown':
        scroller.scrollLeft += viewWidth;
        break;
      case 'Home':
        scroller.scrollLeft = 0;
        break;
      case 'End':
        scroller.scrollLeft = scrollRange;
        break;
      default:
        return;
    }
    e.preventDefault();
  }

  // The viewport of the scroll area is the scrolling element.
  $effect(() => {
    const element = scroller;
    if (!element) return;
    const resize = new ResizeObserver(() => {
      viewWidth = element.clientWidth;
      viewHeight = element.clientHeight;
    });
    resize.observe(element);
    const scroll = () => (scrollLeft = element.scrollLeft);
    element.addEventListener('scroll', scroll, { passive: true });
    // Not passive, so Ctrl+wheel does not also zoom the page.
    const wheel = (e: WheelEvent) => {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        void setZoom(zoom * Math.exp(-e.deltaY * 0.002), e.clientX);
      } else if (zoom > 1 && Math.abs(e.deltaY) > Math.abs(e.deltaX)) {
        e.preventDefault();
        element.scrollLeft += e.deltaY;
      }
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => {
      resize.disconnect();
      element.removeEventListener('scroll', scroll);
      element.removeEventListener('wheel', wheel);
    };
  });

  /** Detail level for the zoom: peaks per second 2^level, at least one per pixel; null while the overview suffices. */
  const level = $derived(
    data && pxPerSec > 0 && data.peaks.length < contentWidth ? Math.ceil(Math.log2(pxPerSec)) : null,
  );
  /** Direction of the last scroll (1 = later), so more is fetched ahead than behind. */
  let heading = 1;
  let lastScroll = 0;
  $effect(() => {
    if (scrollLeft !== lastScroll) heading = scrollLeft > lastScroll ? 1 : -1;
    lastScroll = scrollLeft;
  });

  // Runs again whenever a tile arrives, for the next ones.
  $effect(() => {
    void tiles.changed;
    if (level === null || viewWidth <= 0) return;
    tiles.request(level, scrollLeft / pxPerSec, viewWidth / pxPerSec, heading, duration);
  });

  const ticks = $derived(rulerTicks(pxPerSec, scrollLeft, scrollLeft + viewWidth));

  // Only the visible part is drawn: the canvas is as wide as the view and
  // stays put while the content under it scrolls.
  $effect(() => {
    if (!canvas || !data || viewWidth <= 0 || viewHeight <= RULER) return;
    void tiles.changed;
    const detailLevel = level;
    drawWaveform(canvas, {
      width: viewWidth,
      height: viewHeight,
      ruler: RULER,
      pxPerSec,
      scrollLeft,
      overview: { from: 0, to: duration, peaks: data.peaks },
      detail: (secs) => (detailLevel === null ? undefined : tiles.at(detailLevel, secs)),
      fades: fadeSegments(shown, end, ui.view?.show.level_limits.min_db ?? -60),
    });
  });

  // Zoomed in, the view follows the preview once it runs out of sight.
  $effect(() => {
    if (playhead == null || !scroller || drag || zoom <= 1) return;
    const at = playhead * pxPerSec;
    const left = untrack(() => scrollLeft);
    if (at < left || at > left + viewWidth - 24) scroller.scrollLeft = at - viewWidth * 0.1;
  });

  // --- Pointer

  let scrubbing = false;
  let handle: Handle | null = null;

  function press(e: PointerEvent) {
    if (e.button !== 0 || duration <= 0) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    scrubbing = true;
    cursor = timeAt(e.clientX);
  }

  function grab(e: PointerEvent, grabbed: Handle) {
    if (locked || e.button !== 0) return;
    e.preventDefault();
    e.stopPropagation();
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    handle = grabbed;
    drag = copyTimes($state.snapshot(times));
  }

  function move(e: PointerEvent) {
    if (scrubbing) cursor = timeAt(e.clientX);
    else if (drag && handle) dragHandle(drag, handle, timeAt(e.clientX), duration);
  }

  function release() {
    if (scrubbing) {
      scrubbing = false;
      if (cursor != null) onscrub?.(cursor);
      return;
    }
    if (!drag) return;
    const result = $state.snapshot(drag);
    drag = null;
    handle = null;
    settleTimes(result, duration, 0.0005);
    // A click on a handle moves nothing: no commit, no undo step.
    const positions = (p: Times) =>
      JSON.stringify([p.start, p.end, p.vamps, p.marks, p.fade_in, p.fade_out, p.end_fade]);
    if (positions(result) === positions($state.snapshot(times))) return;
    oncommit(result);
  }

  function onMarkClick(e: MouseEvent) {
    if (locked || pxPerSec <= 0) return;
    // A mark within a few pixels is removed instead.
    oncommit({ marks: toggleMark(shown.marks, timeAt(e.clientX), 8 / pxPerSec) });
  }

  // Pointer only, no tab stops: the fields below set the same values.
  const handleClass = tw`absolute top-5 bottom-0 w-3 -translate-x-1/2 cursor-ew-resize touch-none`;
  const fadeHandleClass = tw`absolute top-5 grid size-4 -translate-x-1/2 cursor-ew-resize touch-none place-items-center`;
</script>

<ScrollArea
  bind:viewportRef={scroller}
  orientation="horizontal"
  type="hover"
  class="min-h-48 flex-1 rounded-lg border border-line bg-canvas select-none"
>
  {#snippet scrollbarXThumb()}
    <div
      bind:this={scrollbar}
      role="scrollbar"
      aria-controls={viewportId}
      aria-label={t('wave.scroll')}
      aria-orientation="horizontal"
      aria-valuemin={0}
      aria-valuemax={Math.ceil(scrollRange)}
      aria-valuenow={Math.round(Math.min(scrollRange, Math.max(0, scrollLeft)))}
      tabindex="0"
      class="absolute inset-0 touch-none"
      onpointerdown={onScrollbarPointerDown}
      onpointermove={moveThumb}
      onpointerup={endThumbDrag}
      onpointercancel={endThumbDrag}
      onlostpointercapture={endThumbDrag}
      onkeydown={onScrollbarKeydown}
    >
      <span
        class="absolute top-px bottom-px left-px rounded-full bg-dim/40 transition-colors hover:bg-dim/70"
        style:width="{thumbWidth}px"
        style:transform="translateX({thumbLeft}px)"
      ></span>
    </div>
  {/snippet}
  {#if data}
    <div
      bind:this={content}
      class="relative h-full overflow-clip"
      style:width={zoom > 1 ? `${contentWidth}px` : '100%'}
      role="presentation"
      onpointerdown={press}
      onpointermove={move}
      onpointerup={release}
      onpointercancel={release}
      ondblclick={onMarkClick}
    >
      <canvas bind:this={canvas} class="sticky left-0 block h-full" style:width="{viewWidth}px"></canvas>

      <span class="absolute inset-x-0 top-0 h-5 border-b border-line bg-panel"></span>
      {#each ticks.times as time (time)}
        <span
          class="absolute top-0 h-5 border-l border-line-strong pl-1 font-mono text-[10px] leading-5 text-muted-foreground"
          style:left={px(time)}>{rulerLabel(time, ticks.step)}</span
        >
      {/each}

      {#each shown.vamps as vamp, i (i)}
        <span
          class="absolute top-5 bottom-0 border-x border-vamp bg-vamp/15"
          style:left={px(vamp.start)}
          style:width="{Math.max(0, vamp.end - vamp.start) * pxPerSec}px"
        ></span>
        <span class="absolute top-5 ml-1.5 text-[10px] text-vamp-soft" style:left={px(vamp.start)}
          >{t('sound_main.vamp', { number: i + 1 })}</span
        >
      {/each}
      <!-- Outside start–end is dimmed. -->
      <span class="absolute top-5 bottom-0 left-0 bg-black/55" style:width={px(shown.start)}></span>
      <span class="absolute top-5 right-0 bottom-0 bg-black/55" style:left={px(end)}></span>
      {#each shown.marks as mark, i (i)}
        <span class="absolute top-5 bottom-0 w-px bg-primary/70" style:left={px(mark)}></span>
      {/each}
      {#if cursor != null}
        <span class="absolute inset-y-0 w-px bg-fg" style:left={px(cursor)}></span>
      {/if}
      {#if playhead != null}
        <span class="absolute inset-y-0 w-0.5 -translate-x-1/2 bg-go" style:left={px(playhead)}></span>
      {/if}

      {#if !locked}
        <Tooltip
          tabindex={-1}
          class={handleClass}
          style="left: {px(shown.start)}"
          tip={t('wave.drag_start')}
          aria-label={t('check.field.start')}
          onpointerdown={(e) => grab(e, { kind: 'start' })}
        >
          <span class="mx-auto block h-full w-0.5 bg-go"></span>
          <span class="absolute bottom-1 left-2 rounded-sm bg-go px-1 text-[10px] text-go-fg"
            >{t('check.field.start')}</span
          >
        </Tooltip>
        <Tooltip
          tabindex={-1}
          class={handleClass}
          style="left: {px(end)}"
          tip={t('wave.drag_end')}
          aria-label={t('overrides.end')}
          onpointerdown={(e) => grab(e, { kind: 'end' })}
        >
          <span class="mx-auto block h-full w-0.5 bg-stop-soft"></span>
          <span class="absolute right-2 bottom-1 rounded-sm bg-stop-soft px-1 text-[10px] text-stop-fg"
            >{t('overrides.end')}</span
          >
        </Tooltip>
        {#each shown.vamps as vamp, i (i)}
          <Tooltip
            tabindex={-1}
            class={handleClass}
            style="left: {px(vamp.start)}"
            tip={t('wave.drag_vamp_start', { number: i + 1 })}
            aria-label={t('sound_main.vamp_start', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'vamp_start', index: i })}
          ></Tooltip>
          <Tooltip
            tabindex={-1}
            class={handleClass}
            style="left: {px(vamp.end)}"
            tip={t('wave.drag_vamp_end', { number: i + 1 })}
            aria-label={t('sound_main.vamp_end', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'vamp_end', index: i })}
          ></Tooltip>
        {/each}
        {#each shown.marks as mark, i (i)}
          <Tooltip
            tabindex={-1}
            class="{handleClass} top-auto h-4"
            style="left: {px(mark)}"
            tip={t('wave.mark_at', { time: fmtPosition(mark) })}
            aria-label={t('wave.mark', { number: i + 1 })}
            onpointerdown={(e) => grab(e, { kind: 'mark', index: i })}
            ><span class="mx-auto block size-2.5 rotate-45 bg-primary"></span></Tooltip
          >
        {/each}
        <!-- Fade handles last, so they win over start and end where they meet. -->
        <Tooltip
          tabindex={-1}
          class={fadeHandleClass}
          style="left: {px(Math.min(shown.start + shown.fade_in, end))}"
          tip={t('wave.drag_fade_in')}
          aria-label={t('check.field.fade_in')}
          onpointerdown={(e) => grab(e, { kind: 'fade_in' })}
          ><span class="block size-2.5 rounded-sm bg-fade"></span></Tooltip
        >
        {#if shown.end_fade}
          <Tooltip
            tabindex={-1}
            class={fadeHandleClass}
            style="left: {px(Math.max(end - shown.fade_out, shown.start))}"
            tip={t('wave.drag_fade_out')}
            aria-label={t('check.field.fade_out')}
            onpointerdown={(e) => grab(e, { kind: 'fade_out' })}
            ><span class="block size-2.5 rounded-sm bg-fade"></span></Tooltip
          >
        {/if}
      {/if}
    </div>
  {:else if error}
    <span class="absolute inset-0 grid place-items-center text-xs text-stop-soft">{error}</span>
  {:else}
    <span class="absolute inset-0 grid place-items-center text-xs text-muted-foreground">{t('wave.computing')}</span>
  {/if}
</ScrollArea>
