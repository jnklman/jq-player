<script lang="ts">
  import ArrowCounterClockwise from '~icons/ph/arrow-counter-clockwise-bold';
  import Pause from '~icons/ph/pause-fill';
  import Play from '~icons/ph/play-fill';
  import { Button } from '$lib/components/ui/button';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { watchViews } from '../lib/stopwatches';
  import { attempt, ui } from '../lib/store.svelte';
  import { empty } from '../lib/ui';
  import PanelFrame from './layout/PanelFrame.svelte';

  // The show's stopwatches and countdowns large, e.g. as an overlay on the
  // second screen; the header shows them small. They run in the backend, also without the UI.
  const watches = $derived(watchViews());
</script>

<PanelFrame title={t('stopwatches.title')}>
  <ScrollArea class="flex-1">
    {#if watches.length}
      <div class="flex flex-col gap-3 px-4 py-3">
        {#each watches as watch (watch.id)}
          <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
            <div class="flex min-w-0 flex-1 flex-col">
              <span class={['truncate text-[13px] font-semibold', !watch.running && 'text-muted-foreground']}
                >{watch.name}{#if watch.overtime}<span class="font-normal text-stop-soft">
                    · {t('stopwatches.overtime')}</span
                  >{/if}</span
              >
              <span
                class={[
                  'font-mono text-4xl leading-tight font-semibold tabular-nums',
                  watch.overtime ? 'text-stop-soft' : watch.running ? 'text-fg' : 'text-muted-foreground',
                ]}>{watch.time}</span
              >
            </div>
            <div class="flex gap-1.5">
              {#if watch.running}
                <Button onclick={() => attempt(api.stopwatch(watch.id, 'stop'))}
                  ><Pause class="size-3.5" /> {t('stopwatch_action.stop')}</Button
                >
              {:else}
                <Button onclick={() => attempt(api.stopwatch(watch.id, 'start'))}
                  ><Play class="size-3.5" /> {t('stopwatch_action.start')}</Button
                >
              {/if}
              <Button onclick={() => attempt(api.stopwatch(watch.id, 'restart'))}>{t('stopwatches.restart')}</Button>
              <Button variant="ghost" onclick={() => attempt(api.stopwatch(watch.id, 'reset'))}>
                <ArrowCounterClockwise class="size-3.5" />
                {t('common.reset')}
              </Button>
            </div>
          </div>
        {/each}
      </div>
    {:else}
      <div class={empty}>
        {t('stopwatches.none')}
        <Button variant="link" class="min-h-0 p-0 text-[13px]" onclick={() => (ui.stopwatchesOpen = true)}
          >{t('stopwatches.create')}</Button
        >
      </div>
    {/if}
  </ScrollArea>
</PanelFrame>
