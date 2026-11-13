<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { buttonVariants } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { type ActiveState, api } from '../../lib/api';
  import { panLabel } from '../../lib/control';
  import { loopLabel, passLabel, vampSpans } from '../../lib/cue';
  import { fmtDb, fmtRunning, meterPercent, nearEnd } from '../../lib/format';
  import { fmtNumber, t } from '../../lib/i18n/index.svelte';
  import { rateLabel, transposeLabel } from '../../lib/speed';
  import { cueById, ui } from '../../lib/store.svelte';
  import { bar, tw } from '../../lib/ui';
  import CueButtons from './CueButtons.svelte';
  import NextItemButton from './NextItemButton.svelte';
  import PanMeter from './PanMeter.svelte';

  // A playing cue: times and progress, level and pan as they play now, rehearsal jumps and its buttons.
  let { running }: { running: ActiveState } = $props();

  const cue = $derived(cueById(running.cue));
  const warn = $derived(nearEnd(running.remaining, ui.view?.settings.end_warning ?? 10));
  const times = $derived(fmtRunning(running.elapsed, running.remaining));
  // Rounded to whole percent, so a standing pan does not touch the DOM.
  const pan = $derived(Math.round(Math.max(-1, Math.min(1, running.pan ?? 0)) * 100) / 100);
  // Rate in whole percent and transposition in tenths, as they play now (rate cues move them).
  const rate = $derived(Math.round(running.rate ?? 100));
  const transpose = $derived(Math.round((running.transpose ?? 0) * 10) / 10);
  const speed = $derived(
    [rate !== 100 ? rateLabel(rate) : null, transpose !== 0 ? transposeLabel(transpose) : null]
      .filter(Boolean)
      .join(' · '),
  );

  const secs = (value: number | undefined) => fmtNumber(value ?? 10);
  const back = $derived(secs(ui.view?.settings.jump_back));
  const forward = $derived(secs(ui.view?.settings.jump_forward));
  const toEnd = $derived(secs(ui.view?.settings.jump_end));
  const jumpBtn = tw`bg-transparent px-2 text-xs text-fg-2 hover:bg-hover`;
</script>

<div
  class={[
    'flex flex-col gap-2 border-b border-line px-4 py-3',
    running.paused ? 'bg-row-paused' : running.looping && 'bg-row-vamp',
  ]}
>
  <div class="flex items-center gap-2.5">
    <span class="font-mono font-semibold text-primary">{cue?.number}</span>
    <span class="min-w-0 grow truncate font-semibold">{cue?.title}</span>
    {#if running.item}
      <Badge variant="outline" class="text-primary">
        {t('active.item', { index: running.item.index + 1, count: running.item.count })}
      </Badge>
    {/if}
    {#if running.pass}
      <Badge variant="outline" class={running.looping ? 'text-vamp-soft' : 'text-muted-foreground'}>
        {passLabel(running.pass)}</Badge
      >
    {:else if running.looping}
      <Badge variant="outline" class="text-vamp-soft">{loopLabel(cue)}</Badge>
    {/if}
    {#if speed}<Badge variant="outline" class="text-fade">{speed}</Badge>{/if}
    {#if running.loading}<Badge variant="outline" class="text-muted-foreground">{t('status.loading')}</Badge>{/if}
    {#if running.paused}<Badge variant="outline" class="text-pause-soft">{t('status.paused')}</Badge>{/if}
  </div>
  <div class="flex items-center gap-3">
    <span class="min-w-13 font-mono text-[13px]">{times[0]}</span>
    <span class="{bar} relative grow">
      <span
        class={[
          'block h-full w-full origin-left',
          running.paused ? 'bg-pause' : running.looping ? 'bg-vamp' : warn ? 'bg-fade' : 'bg-progress',
        ]}
        style:transform="scaleX({running.progress})"
      ></span>
      {#each vampSpans(cue, ui.view?.cue_info[running.cue]) as span, i (i)}
        <span
          class="absolute inset-y-0 bg-vamp-soft/35"
          style:left="{span.left * 100}%"
          style:width="{span.width * 100}%"
        ></span>
      {/each}
    </span>
    <span
      class={['min-w-13 text-right font-mono text-[13px]', warn ? 'font-semibold text-fade' : 'text-muted-foreground']}
    >
      −{times[1]}
    </span>
  </div>
  <div class="flex items-center gap-2.5">
    <span class="w-13 shrink-0 text-[11px] text-muted-foreground">{fmtDb(running.level_db)}</span>
    <span class="h-1 min-w-0 flex-1 overflow-hidden rounded-sm bg-line">
      <span
        class="block h-full w-full origin-left bg-go"
        style:transform="scaleX({meterPercent(running.level_db, ui.view!.show.level_limits) / 100})"
      ></span>
    </span>
    <PanMeter {pan} label={t('active.pan_label', { number: cue?.number ?? '' })} />
    <span class="w-13 shrink-0 text-right text-[11px] text-muted-foreground">{panLabel(pan)}</span>
  </div>
  <div class="flex flex-wrap items-center gap-1.5">
    <span class="flex" role="group" aria-label={t('active.jump_group')}>
      <Tooltip
        tip={t('active.jump_back_tip', { secs: back })}
        class={buttonVariants({ class: [jumpBtn, 'rounded-r-none'] })}
        aria-label={t('active.jump_back_label', { number: cue?.number ?? '', secs: back })}
        onclick={() => api.action({ action: 'jump_back', cue: running.cue })}
        >−{back}
      </Tooltip>
      <Tooltip
        tip={t('active.jump_forward_tip', { secs: forward })}
        class={buttonVariants({ class: [jumpBtn, 'rounded-none border-x-0'] })}
        aria-label={t('active.jump_forward_label', { number: cue?.number ?? '', secs: forward })}
        onclick={() => api.action({ action: 'jump_forward', cue: running.cue })}
      >
        +{forward}
      </Tooltip>
      <Tooltip
        tip={t('active.jump_end_tip', { secs: toEnd })}
        class={buttonVariants({ class: [jumpBtn, 'rounded-l-none'] })}
        aria-label={t('active.jump_end_label', { number: cue?.number ?? '', secs: toEnd })}
        onclick={() => api.action({ action: 'jump_end', cue: running.cue })}
      >
        {t('active.end')}
      </Tooltip>
    </span>
    {#if running.item}
      <NextItemButton cue={running.cue} number={cue?.number} enabled={running.has_next} />
    {/if}
    <span class="grow"></span>
    <CueButtons cue={running.cue} number={cue?.number} paused={running.paused} fade />
  </div>
</div>
