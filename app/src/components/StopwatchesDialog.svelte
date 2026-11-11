<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';
  import { Label } from '$lib/components/ui/label';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import type { Stopwatch } from '../lib/api';
  import { edit } from '../lib/commands';
  import { fmtClock } from '../lib/format';
  import { t } from '../lib/i18n/index.svelte';
  import { parseClock } from '../lib/input';
  import { mode, ui } from '../lib/store.svelte';
  import { field, hint } from '../lib/ui';
  import EditDialog from './EditDialog.svelte';
  import SimpleSelect from './SimpleSelect.svelte';

  let { onclose }: { onclose: () => void } = $props();

  const locked = $derived(mode() === 'show');
  // Edited as a whole and saved as one undo step.
  const initial = JSON.stringify(ui.view!.show.stopwatches);
  let watches = $state<Stopwatch[]>(JSON.parse(initial));
  const dirty = $derived(JSON.stringify(watches) !== initial);
  const valid = $derived(watches.every((watch) => watch.name.trim() !== ''));

  /** Longest countdown, as the backend allows (100 h). */
  const MAX_COUNTDOWN = 360_000;
  /** Start time of a new countdown: 15 minutes, an interval. */
  const DEFAULT_COUNTDOWN = 900;

  const kindOptions = $derived([
    { value: 'up', label: t('stopwatches.kind_up') },
    { value: 'down', label: t('stopwatches.kind_down') },
  ] as const);

  function setKind(watch: Stopwatch, kind: 'up' | 'down') {
    if (kind === 'up') delete watch.countdown;
    else watch.countdown ??= DEFAULT_COUNTDOWN;
  }

  /** A typed start time; anything unreadable, 0 or too long puts the old one back. */
  function setCountdown(watch: Stopwatch, input: HTMLInputElement) {
    const secs = parseClock(input.value);
    if (secs !== null && secs > 0 && secs <= MAX_COUNTDOWN) watch.countdown = secs;
    input.value = fmtClock(watch.countdown ?? DEFAULT_COUNTDOWN);
  }

  function add() {
    const names = [t('stopwatches.name_show'), t('stopwatches.name_break'), t('stopwatches.name_act')];
    const name =
      names.find((n) => !watches.some((watch) => watch.name === n)) ??
      t('stopwatches.name_numbered', { number: watches.length + 1 });
    watches.push({ id: crypto.randomUUID().slice(0, 8), name });
  }
</script>

<EditDialog
  title={t('stopwatches.title')}
  class="max-w-110"
  {dirty}
  {valid}
  invalidTip={t('stopwatches.invalid_tip')}
  {locked}
  onapply={async () => !!(await edit({ op: 'set_stopwatches', stopwatches: $state.snapshot(watches) }))}
  {onclose}
>
  <ScrollArea
    ><fieldset class="m-0 flex min-w-0 flex-col gap-3 border-0 p-4" disabled={locked}>
      <p class="{hint} m-0">{t('stopwatches.text')}</p>
      {#each watches as watch, i (watch.id)}
        <div class="grid grid-cols-[1fr_8rem_6rem_auto] items-end gap-2">
          <Label class={field}>{t('devices.name')}<Input bind:value={watch.name} /></Label>
          <Label class={field}
            >{t('stopwatches.kind')}<SimpleSelect
              aria-label={t('stopwatches.kind_of', { name: watch.name })}
              value={watch.countdown != null ? 'down' : 'up'}
              options={[...kindOptions]}
              onchange={(kind) => setKind(watch, kind)}
            /></Label
          >
          {#if watch.countdown != null}
            <Label class={field}
              >{t('stopwatches.from')}<Input
                class="font-mono tabular-nums"
                aria-label={t('stopwatches.countdown_from', { name: watch.name })}
                value={fmtClock(watch.countdown)}
                onchange={(e) => setCountdown(watch, e.currentTarget)}
              /></Label
            >
          {:else}
            <span></span>
          {/if}
          <Button
            variant="ghost"
            size="icon-sm"
            class="text-base"
            aria-label={t('devices.remove', { name: watch.name })}
            onclick={() => watches.splice(i, 1)}><XMark /></Button
          >
        </div>
      {:else}
        <p class="{hint} m-0">{t('stopwatches.empty')}</p>
      {/each}
      <Button class="self-start" onclick={add}>{t('stopwatches.add')}</Button>
    </fieldset></ScrollArea
  >
</EditDialog>
