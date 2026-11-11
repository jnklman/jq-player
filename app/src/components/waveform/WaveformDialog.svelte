<script lang="ts">
  import { onDestroy } from 'svelte';

  import ZoomAll from '~icons/ph/arrows-out-line-horizontal-bold';
  import ZoomOut from '~icons/ph/magnifying-glass-minus-bold';
  import ZoomIn from '~icons/ph/magnifying-glass-plus-bold';
  import MouseScroll from '~icons/ph/mouse-scroll-bold';
  import Play from '~icons/ph/play-fill';
  import SkipBack from '~icons/ph/skip-back-fill';
  import SkipForward from '~icons/ph/skip-forward-fill';
  import SpeakerHigh from '~icons/ph/speaker-high-fill';
  import Stop from '~icons/ph/stop-fill';
  import { buttonVariants } from '$lib/components/ui/button';
  import Kbd from '$lib/components/ui/kbd/kbd.svelte';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type Cue, type CueId } from '../../lib/api';
  import { edit } from '../../lib/commands';
  import { fmtPosition } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { roundMs } from '../../lib/input';
  import { isTextField } from '../../lib/shortcuts';
  import { rateLabel, speedOf } from '../../lib/speed';
  import { cueById, mode, ui } from '../../lib/store.svelte';
  import { hint } from '../../lib/ui';
  import { copyTimes, type Times } from '../../lib/waveform';
  import EditDialog from '../EditDialog.svelte';
  import WaveformTimeline from './WaveformTimeline.svelte';
  import WaveformTimes from './WaveformTimes.svelte';

  // Big, zoomable waveform of one sound cue for setting start, end, fades,
  // vamps and marks by ear: the preview plays the file from any position.
  // Edits stay in the dialog until "Fertig" applies them as one undo step.
  let { cueId, onclose }: { cueId: CueId; onclose: () => void } = $props();

  /** "Before the end" plays from this many seconds before the end. */
  const END_LEAD = 5;

  const cue = $derived.by(() => {
    const found = cueById(cueId);
    return found?.type === 'sound' ? found : undefined;
  });
  const locked = $derived(mode() === 'show');
  const info = $derived(ui.view?.cue_info[cueId]);
  const previewSet = $derived(ui.view?.settings.preview_channel != null || ui.view?.settings.preview_device != null);

  $effect(() => {
    // A cue deleted meanwhile (undo, another edit) closes the dialog.
    if (!cue) onclose();
  });

  // --- Editing

  // The cue's values when the dialog opened, and the edited ones.
  // svelte-ignore state_referenced_locally
  const initial: Times = copyTimes({
    start: cue?.start ?? 0,
    end: cue?.end ?? null,
    vamps: cue?.vamps ?? [],
    marks: cue?.marks ?? [],
    fade_in: cue?.fade_in ?? 0,
    fade_out: cue?.fade_out ?? 0,
    end_fade: cue?.end_fade ?? false,
    curve: cue?.curve ?? 'decibel_slow_top',
  });
  let draft = $state<Times>($state.snapshot(initial));
  const dirty = $derived(!locked && JSON.stringify(draft) !== JSON.stringify(initial));
  // While a handle is dragged on the waveform, the dragged copy.
  let dragged = $state<Times | null>(null);
  const shown: Times = $derived(dragged ?? draft);

  /** Into the draft only; "Fertig" sends it. */
  function commit(changes: Partial<Times>) {
    if (!locked) draft = { ...$state.snapshot(draft), ...changes };
  }

  /** "Fertig": the draft onto the cue as it is now, as one undo step. */
  async function apply(): Promise<boolean> {
    if (!cue || !dirty) return true;
    return !!(await edit({ op: 'update_cue', cue: { ...$state.snapshot(cue), ...$state.snapshot(draft) } as Cue }));
  }

  // --- Timeline

  let timeline = $state<WaveformTimeline>();
  let zoom = $state(1);
  let maxZoom = $state(1);
  let duration = $state(0);
  /** Click position in the waveform; playing starts here. */
  let cursor = $state<number | null>(null);
  const end = $derived(Math.min(shown.end ?? duration, duration));

  // --- Preview

  const preview = $derived(ui.runtime?.preview?.cue === cueId ? ui.runtime.preview : null);
  const playhead = $derived(preview?.playing ? preview.position : null);
  /** What the position buttons take: where the preview plays, else the cursor. */
  const here = $derived(playhead != null ? roundMs(playhead) : cursor);

  function playFrom(secs: number) {
    void api.previewAt(cueId, Math.max(0, secs));
  }

  function togglePlay() {
    if (preview) void api.action({ action: 'stop_preview' });
    else playFrom(cursor ?? shown.start);
  }

  /** Whether Space goes to the player: not in fields, which keep their keys. */
  function playerKey(e: KeyboardEvent): boolean {
    return e.key === ' ' && !e.defaultPrevented && !e.ctrlKey && !e.metaKey && !e.altKey && !isTextField(e.target);
  }

  // Space works the player here instead of GO (the shortcuts leave it alone
  // while this dialog is open).
  function onKeydown(e: KeyboardEvent) {
    if (!playerKey(e)) return;
    e.preventDefault();
    if (!e.repeat && previewSet) togglePlay();
  }

  // A focused button clicks on keyup, which the keydown above does not stop
  // (WebKit): Space would also close the dialog or zoom again.
  function onKeyup(e: KeyboardEvent) {
    if (playerKey(e)) e.preventDefault();
  }

  onDestroy(() => {
    if (ui.runtime?.preview?.cue === cueId) void api.action({ action: 'stop_preview' });
  });

  const zoomButton = buttonVariants({ variant: 'ghost', size: 'icon-sm' });
</script>

<svelte:window onkeydown={onKeydown} onkeyup={onKeyup} />

{#snippet title()}
  {t('wave.title', { number: cue?.number ?? '' })} · {cue?.title || t('group.untitled')}
  {#if locked}<span class="ml-2 font-normal text-muted-foreground">{t('wave.locked')}</span>{/if}
{/snippet}

<EditDialog
  {title}
  class="h-[min(820px,calc(100%-2rem))] max-w-7xl"
  {dirty}
  applyLabel={t('common.done')}
  focusDialog
  onapply={apply}
  {onclose}
>
  {#if cue && ui.view}
    <div class="flex min-h-0 flex-1 flex-col gap-3 p-4">
      <div class="flex flex-wrap items-center gap-1.5">
        <Tooltip
          tip={previewSet ? t('wave.play_tip') : t('runner.no_preview_output')}
          keys={previewSet ? t('key.space') : undefined}
          class={buttonVariants({ variant: 'default' })}
          aria-pressed={preview != null}
          disabled={!previewSet}
          onclick={togglePlay}
        >
          {#if preview}<Stop class="size-3.5" /> {t('type.stop')}{:else}<Play class="size-3.5" /> {t('wave.play')}{/if}
        </Tooltip>
        <Tooltip
          tip={t('wave.from_start_tip')}
          class={buttonVariants()}
          disabled={!previewSet}
          onclick={() => playFrom(shown.start)}><SkipBack class="size-3.5" /> {t('wave.from_start')}</Tooltip
        >
        <Tooltip
          tip={t('wave.before_end_tip', { secs: END_LEAD })}
          class={buttonVariants()}
          disabled={!previewSet || duration <= 0}
          onclick={() => playFrom(end - END_LEAD)}><SkipForward class="size-3.5" /> {t('shortcuts.before_end')}</Tooltip
        >
        <Tooltip
          tip={t('wave.range_tip')}
          class={buttonVariants({ variant: 'ghost' })}
          disabled={!previewSet || duration <= 0}
          onclick={() => api.previewAt(cueId, shown.start, end)}
          ><SpeakerHigh class="size-4" /> {t('wave.range')}</Tooltip
        >
        <span class="ml-3 font-mono text-[13px] text-fg-2 tabular-nums">
          {#if playhead != null}<span class="text-go">▶ {fmtPosition(playhead)}</span>
          {:else if cursor != null}{t('wave.cursor', { time: fmtPosition(cursor) })}
          {:else}<span class="text-muted-foreground">{t('wave.cursor_hint')}</span>{/if}
        </span>
        <span class="ml-auto flex items-center gap-1">
          <Tooltip
            tip={t('wave.zoom_out')}
            class={zoomButton}
            aria-label={t('wave.zoom_out')}
            disabled={zoom <= 1}
            onclick={() => timeline?.setZoom(zoom / 2)}><ZoomOut /></Tooltip
          >
          <Tooltip
            tip={t('wave.zoom_in')}
            class={zoomButton}
            aria-label={t('wave.zoom_in')}
            disabled={zoom >= maxZoom}
            onclick={() => timeline?.setZoom(zoom * 2)}><ZoomIn /></Tooltip
          >
          <Tooltip
            tip={t('wave.whole_file')}
            class={zoomButton}
            aria-label={t('wave.whole_file')}
            disabled={zoom <= 1}
            onclick={() => timeline?.setZoom(1)}><ZoomAll /></Tooltip
          >
        </span>
      </div>

      <WaveformTimeline
        bind:this={timeline}
        mediaDir={ui.view.media_dir}
        file={cue.file}
        version={info?.file_version ?? 0}
        fileDuration={info?.file_duration}
        times={draft}
        {locked}
        {playhead}
        bind:cursor
        bind:zoom
        bind:maxZoom
        bind:duration
        oncommit={commit}
        ondrag={(times) => (dragged = times)}
        onscrub={(secs) => preview && playFrom(secs)}
      />
      <span class={hint}>
        {t('wave.hint_double')} ·
        <Kbd>{t('key.ctrl')} +<MouseScroll class="inline size-3 align-[-2px]" /></Kbd>
        {t('wave.hint_zoom')} · <Kbd>{t('key.space')}</Kbd>
        {t('wave.hint_play')}
      </span>
      {#if speedOf(cue).rate !== 100}
        <span class="{hint} text-fade">{t('wave.rate_hint', { rate: rateLabel(speedOf(cue).rate) })}</span>
      {/if}

      <WaveformTimes times={shown} {here} {end} {locked} oncommit={commit} />
    </div>
  {/if}
</EditDialog>
