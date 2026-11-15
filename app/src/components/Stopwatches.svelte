<script lang="ts">
  import ArrowCounterClockwise from '~icons/ph/arrow-counter-clockwise-bold';
  import ClockCountdown from '~icons/ph/clock-countdown-bold';
  import Pause from '~icons/ph/pause-fill';
  import Play from '~icons/ph/play-fill';
  import Timer from '~icons/ph/timer-bold';
  import { Button } from '$lib/components/ui/button';
  import * as Popover from '$lib/components/ui/popover';

  import { api } from '../lib/api';
  import { t } from '../lib/i18n/index.svelte';
  import { watchViews } from '../lib/stopwatches';
  import { attempt } from '../lib/store.svelte';

  // The show's stopwatches and countdowns in the header: name and time; a
  // click offers start/stop and reset. They run in the backend, also without the UI.
  const watches = $derived(watchViews());
</script>

{#each watches as watch (watch.id)}
  <Popover.Root>
    <Popover.Trigger>
      {#snippet child({ props })}
        <button
          {...props}
          class={[
            'flex min-h-7 items-center gap-1.5 rounded-full border border-input bg-secondary px-2.5 text-xs hover:bg-accent',
            watch.running ? 'text-fg' : 'text-muted-foreground',
          ]}
          aria-label={t(watch.countdown != null ? 'stopwatches.countdown_named' : 'stopwatches.watch_named', {
            name: watch.name,
          })}
        >
          {#if watch.countdown != null}
            <ClockCountdown class={['size-3.5', watch.running && 'text-go']} />
          {:else}
            <Timer class={['size-3.5', watch.running && 'text-go']} />
          {/if}
          <span class="max-w-28 truncate">{watch.name}</span>
          <span class={['font-mono font-semibold tabular-nums', watch.overtime && 'text-stop-soft']}>{watch.time}</span>
        </button>
      {/snippet}
    </Popover.Trigger>
    <Popover.Content
      align="start"
      sideOffset={8}
      class="flex w-auto gap-1.5 p-2"
      aria-label={t('stopwatches.watch_named', { name: watch.name })}
    >
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
      <Button variant="ghost" onclick={() => attempt(api.stopwatch(watch.id, 'reset'))}
        ><ArrowCounterClockwise class="size-3.5" /> {t('common.reset')}</Button
      >
    </Popover.Content>
  </Popover.Root>
{/each}
