<script lang="ts">
  import { untrack } from 'svelte';

  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api } from '../lib/api';
  import { controlStatus, isControl, typeColors, typeLabel } from '../lib/control';
  import { fmtSecs } from '../lib/format';
  import { t } from '../lib/i18n/index.svelte';
  import { boundKeys } from '../lib/keys.svelte';
  import { allCues, cueById, cues, previousOf, ui } from '../lib/store.svelte';
  import { tw } from '../lib/ui';
  import PanelFrame from './layout/PanelFrame.svelte';

  // GO, the next cue and the global stops, as a panel like any other: it can
  // be moved, taken into a window of its own (e.g. as an overlay above
  // another program) or switched off.

  const next = $derived(cueById(ui.runtime?.playhead));
  const armed = $derived(ui.runtime?.playhead_armed ?? true);
  const fadeAll = $derived(ui.view?.settings.fade_all ?? 5);
  // Like the backend's toggle: pauses while anything still plays, else resumes.
  const playing = $derived(
    !!ui.runtime &&
      [...ui.runtime.active, ...ui.runtime.waiting, ...ui.runtime.fading, ...ui.runtime.gaps].some(
        (running) => !running.paused,
      ),
  );
  const resumes = $derived(!playing && !!ui.runtime?.any_paused);

  // Show "loading" only once arming takes noticeably long, so stepping through
  // cues doesn't flash the badge; the delay restarts with every new playhead.
  const LOADING_DELAY_MS = 400;
  const nextId = $derived(next?.id);
  let slowLoad = $state(false);
  $effect(() => {
    slowLoad = false;
    if (nextId === undefined || armed) return;
    const timer = setTimeout(() => (slowLoad = true), LOADING_DELAY_MS);
    return () => clearTimeout(timer);
  });

  // A GO dropped by the GO lockout flashes the button; the count only grows.
  const REJECTED_FLASH_MS = 400;
  const rejectedCount = $derived(ui.runtime?.go_rejected ?? 0);
  // Starts at the count of the moment the panel opens, so a panel mounted later does not flash for old rejections.
  let seenRejected = untrack(() => rejectedCount);
  let rejected = $state(false);
  $effect(() => {
    const count = rejectedCount;
    if (count <= seenRejected) {
      seenRejected = count;
      return;
    }
    seenRejected = count;
    rejected = true;
    const timer = setTimeout(() => (rejected = false), REJECTED_FLASH_MS);
    return () => clearTimeout(timer);
  });

  /** What the next cue acts on, if it is a control cue: "Fade → 3 Musik". */
  const controls = $derived.by(() => {
    if (!isControl(next) || !ui.view) return null;
    const status = controlStatus(next, previousOf(next.id), allCues(), ui.view.show, false);
    return { text: `${typeLabel(next)} ${status.text}`, color: typeColors[next.type] };
  });

  // The keys as set in the settings. The pause button toggles, so a key that only pauses or only resumes counts
  // when it fits; the stop button stops at once, so without a key of its own it shows the panic key (Esc).
  const keys = $derived(ui.view?.settings.keys ?? []);
  const goKeys = $derived(boundKeys(keys, 'go'));
  const fadeKeys = $derived(boundKeys(keys, 'fade_all'));
  const pauseKeys = $derived(boundKeys(keys, 'toggle_pause_all', resumes ? 'resume_all' : 'pause_all'));
  const stopKeys = $derived(boundKeys(keys, 'stop_all', 'panic'));

  const sub = tw`text-xs font-medium tracking-normal`;
  // Equally wide; in a narrow panel the label wraps instead of overflowing. Stacked (portrait) they are lower, so
  // the panel doesn't take the whole height.
  const stopButton = tw`min-h-16 min-w-0 px-2 text-center text-[17px] leading-tight whitespace-normal @[40rem]:min-h-24 @[60rem]:min-w-30`;
</script>

<!-- Laid out by the panel's width: in one row where all fits; else the next cue on top and all buttons
     in a row below it; in a narrow (portrait) panel the next cue and each button in a row of its own. -->
<PanelFrame title={t('transport.title')} fit>
  <div class="@container">
    <div
      class="grid grid-cols-1 gap-3 px-4 py-3 @[40rem]:grid-cols-[minmax(200px,2fr)_repeat(3,minmax(0,1fr))] @[60rem]:grid-cols-[auto_minmax(0,1fr)_repeat(3,auto)]"
    >
      <Button
        variant="go"
        size="transport"
        class={['text-[34px] tracking-[2px] @[60rem]:min-w-50', rejected && 'ring-4 ring-stop-soft']}
        disabled={!next}
        onclick={() => api.action({ action: 'go' })}
      >
        GO<span class={sub}>{rejected ? t('transport.too_fast') : goKeys || '\u00a0'}</span>
      </Button>

      <!-- Fixed-height rows that never wrap, so stepping through cues doesn't shift the layout. -->
      <div
        class="order-first flex min-w-0 flex-col justify-center gap-1.5 rounded-xl border border-input bg-secondary px-4.5 py-3.5 @[40rem]:col-span-4 @[60rem]:order-none @[60rem]:col-span-1"
      >
        <div class="flex h-5 min-w-0 items-center gap-2">
          <span class="shrink-0 text-[11px] font-semibold tracking-[1px] text-muted-foreground uppercase"
            >{t('transport.next_cue')}</span
          >
          {#if controls}
            <Tooltip tip={controls.text}>
              {#snippet child({ props })}
                <Badge {...props} variant="outline" class="block min-w-0 truncate {controls.color}"
                  >{controls.text}</Badge
                >
              {/snippet}
            </Tooltip>
          {/if}
          {#if next && !armed && slowLoad}
            <Tooltip tip={t('transport.loading_tip')}>
              {#snippet child({ props })}
                <Badge {...props} variant="outline" class="text-stop-soft">{t('transport.loading')}</Badge>
              {/snippet}
            </Tooltip>
          {/if}
        </div>
        <div class="flex h-9 min-w-0 items-baseline gap-3.5 leading-9">
          {#if next}
            <span class="shrink-0 font-mono text-[28px] font-semibold text-primary">{next.number}</span>
            <Tooltip tip={next.title}>
              {#snippet child({ props })}
                <span {...props} class="min-w-0 truncate text-2xl font-semibold"
                  >{next.title || t('group.untitled')}</span
                >
              {/snippet}
            </Tooltip>
          {:else}
            <span class="truncate text-2xl font-semibold text-muted-foreground">
              {cues().length ? t('transport.end_of_list') : t('transport.no_cues')}
            </span>
          {/if}
        </div>
        <div class="h-5 truncate text-[15px] leading-5 text-fg-2">
          {next?.script_ref ?? ''}
        </div>
      </div>

      <Button variant="fade" size="transport" class={stopButton} onclick={() => api.action({ action: 'fade_all' })}>
        {t('action.fade_all')}<span class={sub}>{fadeKeys ? `${fadeKeys} · ` : ''}{fmtSecs(fadeAll)}</span>
      </Button>
      <Button
        variant="pause"
        size="transport"
        class={stopButton}
        onclick={() => api.action({ action: 'toggle_pause_all' })}
      >
        {resumes ? t('action.resume_all') : t('action.pause_all')}<span class={sub}>{pauseKeys || '\u00a0'}</span>
      </Button>
      <Button variant="stop" size="transport" class={stopButton} onclick={() => api.action({ action: 'stop_all' })}>
        {t('action.stop_all')}<span class={sub}>{stopKeys || '\u00a0'}</span>
      </Button>
    </div>
  </div>
</PanelFrame>
