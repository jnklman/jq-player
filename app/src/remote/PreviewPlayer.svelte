<script lang="ts">
  import Headphones from '~icons/ph/headphones-bold';
  import Pause from '~icons/ph/pause-fill';
  import Play from '~icons/ph/play-fill';
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Slider } from '$lib/components/ui/slider';

  import { fmtClock } from '../lib/format';
  import { fmtNumber, t } from '../lib/i18n/index.svelte';
  import { ui } from '../lib/store.svelte';
  import { remote } from './remote.svelte';

  // A cue's preview on this device: play/pause, the position and the time, nothing else; the browser's own player
  // stays hidden. The host renders the sound while it is played: its length is known from the start, the bar shows
  // the part rendered so far (`remote.progress`), and the position can only be moved within it. Without a connection
  // to the host the page is covered, so the preview pauses (▶ goes on with what has arrived). Played to the end, the
  // player closes.
  let {
    id,
    url,
    duration,
    name,
    onclose,
  }: {
    /** Of the preview (`LocalPreview.id`). */
    id: string;
    url: string;
    /** Seconds. */
    duration: number;
    /** The cue, e.g. "3 Rain". */
    name: string;
    onclose: () => void;
  } = $props();

  /** Moved back from the rendered end by this much, so playback does not run straight into the part not there. */
  const EDGE = 1;
  /** Steps of the position bar over the whole length, whatever it is: fine for a cue of seconds, and few enough
   * for one of minutes (the slider keeps a list of its steps). */
  const STEPS = 1000;
  /** The shown position follows the browser's at once when they are further apart (a move, a stall). */
  const JUMP = 0.25;
  /** Share of a smaller difference made up each frame. */
  const CATCH_UP = 0.08;

  let audio = $state<HTMLAudioElement>();
  let paused = $state(true);
  let current = $state(0);
  /** The browser waits for sound (after a move, or a slow network). */
  let waiting = $state(false);
  let unplayable = $state(false);
  /** Where the thumb is while the operator drags it; the sound moves there once it is let go. */
  let dragged = $state<number | null>(null);
  let holding = false;

  const progress = $derived(remote.progress?.id === id ? remote.progress : null);
  const done = $derived(progress?.done ?? false);
  const rendered = $derived(done ? duration : Math.min(progress?.rendered ?? 0, duration));
  const failed = $derived(unplayable || (progress?.failed ?? false));
  /** The furthest the position may be moved to. */
  const reachable = $derived(done ? duration : Math.max(0, rendered - EDGE));
  const length = $derived(Math.max(duration, 0.001));
  /** Seconds as a step of the bar, and back. */
  const toStep = (secs: number) => Math.round((secs / length) * STEPS);
  const toSecs = (step: number) => (step / STEPS) * length;

  // The browser tells the position only a few times a second, and at the start guesses it from the clock before
  // the sound comes out, then holds it until the sound catches up. So while playing, the bar runs on in real time
  // every frame and makes up a difference to the browser's position gently; a real jump it follows at once.
  $effect(() => {
    const playing = audio;
    if (!playing || paused) return;
    let last = performance.now();
    let frame = requestAnimationFrame(function follow(now) {
      const actual = playing.currentTime;
      const ahead = current + ((now - last) / 1000) * playing.playbackRate;
      last = now;
      const jumped = waiting || Math.abs(actual - ahead) > JUMP;
      current = Math.min(jumped ? actual : Math.max(current, ahead + (actual - ahead) * CATCH_UP), duration);
      frame = requestAnimationFrame(follow);
    });
    return () => cancelAnimationFrame(frame);
  });

  // The connection is lost: the page is covered, and the player with it.
  $effect(() => {
    if (!(ui.remote?.connected ?? true)) audio?.pause();
  });

  // Starts at once: the tap on "Preview" lets it play. A browser that wants a tap on the player itself waits for ▶.
  $effect(() => {
    if (!audio || !url) return;
    void audio.play().catch(() => {});
  });

  function toggle() {
    if (!audio) return;
    if (audio.paused) {
      // Played to the end: from the start again.
      if (audio.ended) audio.currentTime = 0;
      void audio.play().catch(() => {});
    } else audio.pause();
  }

  function seek(step: number) {
    holding = false;
    dragged = null;
    const to = Math.min(toSecs(step), reachable);
    current = to;
    if (audio) audio.currentTime = to;
  }
</script>

<svelte:window onpointerup={() => (holding = false)} onpointercancel={() => (holding = false)} />

<div
  class="flex items-center gap-2 border-t border-line px-2 py-1.5"
  role="group"
  aria-label={t('remote.listen_player', { cue: name })}
>
  <audio
    bind:this={audio}
    bind:paused
    ontimeupdate={() => audio?.paused && (current = audio.currentTime)}
    onended={onclose}
    src={url}
    preload="auto"
    onwaiting={() => (waiting = true)}
    onplaying={() => (waiting = false)}
    oncanplay={() => (waiting = false)}
    onpause={() => (waiting = false)}
    onerror={() => (unplayable = true)}
  ></audio>
  <Button
    variant="ghost"
    size="icon"
    class="shrink-0 text-primary"
    aria-label={paused ? t('remote.listen_play') : t('remote.listen_pause')}
    disabled={failed}
    onclick={toggle}
  >
    {#if paused}<Play class="size-5" />{:else}<Pause class="size-5" />{/if}
  </Button>
  <div class="flex min-w-0 grow flex-col">
    <div class="flex min-w-0 items-baseline gap-2 text-xs">
      <Headphones class="size-3 shrink-0 self-center text-muted-foreground" />
      <span class="min-w-0 grow truncate font-semibold">{name}</span>
      {#if failed}
        <span class="shrink-0 text-stop-soft">{t('remote.listen_failed')}</span>
      {:else}
        {#if waiting && !paused}
          <span class="shrink-0 text-muted-foreground motion-safe:animate-pulse">{t('remote.listen_loading')}</span>
        {:else if !done}
          <span class="shrink-0 text-muted-foreground">
            {t('remote.listen_rendering', { percent: fmtNumber((rendered / length) * 100, 0) })}
          </span>
        {/if}
        <span class="shrink-0 font-mono text-fg-2 tabular-nums">
          {fmtClock(dragged ?? current)} / {fmtClock(duration)}
        </span>
      {/if}
    </div>
    <div class="contents" onpointerdowncapture={() => (holding = true)}>
      <Slider
        type="single"
        class="min-h-6"
        min={0}
        max={STEPS}
        step={1}
        value={toStep(dragged ?? current)}
        loaded={rendered / length}
        format={(step) => fmtClock(toSecs(step))}
        disabled={failed}
        aria-label={t('remote.listen_position')}
        onValueChange={(value) => {
          // Only the operator's drag: the slider also reports a value it snapped to its steps.
          if (holding) dragged = Math.min(toSecs(value), reachable);
        }}
        onValueCommit={seek}
      />
    </div>
  </div>
  <Button variant="ghost" size="icon-sm" class="shrink-0" aria-label={t('remote.listen_close')} onclick={onclose}
    ><XMark /></Button
  >
</div>
