<script lang="ts">
  import { tick } from 'svelte';

  import CrosshairSimple from '~icons/ph/crosshair-simple-bold';
  import Play from '~icons/ph/play-fill';
  import Warning from '~icons/ph/warning-bold';
  import { Button } from '$lib/components/ui/button';

  import PanelFrame from '../components/layout/PanelFrame.svelte';
  import { api, type Cue, type CueId } from '../lib/api';
  import { typeColors, typeLabel } from '../lib/control';
  import { chainArrow, cueStatus, cueWarnings, rowRunning } from '../lib/cueStatus';
  import { t } from '../lib/i18n/index.svelte';
  import { plainText } from '../lib/richText';
  import { canControl, cueById, cues, isRunning, ui } from '../lib/store.svelte';
  import { bar, empty } from '../lib/ui';

  // The cue list for a touch screen: two lines a cue, the playhead marked, following it unless the operator
  // scrolls. Tapping a cue only picks it here (never on the host); what to do with it is in the bar below, only for
  // a device that operates, so a stray tap starts nothing.

  const list = $derived(cues());
  const playhead = $derived(ui.runtime?.playhead ?? null);
  const headIndex = $derived(list.findIndex((cue) => cue.id === playhead));

  let picked = $state<CueId | null>(null);
  const pickedCue = $derived(cueById(picked));
  // A picked cue that is gone (deleted on the host) is forgotten.
  $effect(() => {
    if (picked && !pickedCue) picked = null;
  });
  const operates = $derived(ui.remote?.role === 'control');

  let scroller = $state<HTMLElement>();
  /** The list keeps the playhead in view until the operator scrolls it. */
  let following = $state(true);

  async function reveal(id: CueId | null, behavior: ScrollBehavior = 'smooth') {
    await tick();
    if (!id || !scroller) return;
    scroller.querySelector<HTMLElement>(`[data-remote-cue="${id}"]`)?.scrollIntoView({ block: 'center', behavior });
  }

  $effect(() => {
    if (following) void reveal(playhead);
  });

  function follow() {
    following = true;
    void reveal(playhead);
  }

  function rowClass(cue: Cue, index: number) {
    const status = cueStatus(cue, isRunning(cue));
    return [
      'w-full border-b border-line px-3 py-2 text-left',
      cue.id === playhead && 'outline-2 -outline-offset-2 outline-primary',
      status.vamp ? 'bg-row-vamp' : rowRunning(cue) ? 'bg-row-running' : cue.id === picked ? 'bg-selected' : '',
      headIndex >= 0 && index < headIndex && 'text-muted-foreground',
    ];
  }

  function act(action: 'set_playhead' | 'start_cue' | 'stop_cue') {
    if (!picked) return;
    void api.action({ action, cue: picked }).catch(() => {});
  }
</script>

<PanelFrame title={t('cuelist.title')}>
  {#snippet actions()}
    {#if !following}
      <!-- No higher than the title line, so the header keeps its height. -->
      <Button size="xs" variant="ghost" class="-my-0.5" onclick={follow}>
        <CrosshairSimple class="size-3.5" />{t('remote.to_playhead')}
      </Button>
    {/if}
  {/snippet}
  <div
    bind:this={scroller}
    class="min-h-0 flex-1 overflow-y-auto overscroll-contain"
    role="listbox"
    tabindex="-1"
    aria-label={t('cuelist.title')}
    onwheel={() => (following = false)}
    ontouchmove={() => (following = false)}
  >
    {#each list as cue, index (cue.id)}
      {#if cue.type === 'scene'}
        <div
          class="sticky top-0 z-1 border-b border-line-strong bg-panel-2 px-3 py-1.5 text-[13px] font-semibold"
          style:box-shadow={cue.color ? `inset 6px 0 0 ${cue.color}` : undefined}
          data-remote-cue={cue.id}
        >
          {cue.title || t('group.untitled')}
        </div>
      {:else}
        {@const status = cueStatus(cue, isRunning(cue))}
        {@const arrow = chainArrow(cue, list[index + 1])}
        {@const warnings = cueWarnings(cue)}
        <button
          type="button"
          class={rowClass(cue, index)}
          style:box-shadow={cue.color ? `inset 6px 0 0 ${cue.color}` : undefined}
          data-remote-cue={cue.id}
          role="option"
          aria-selected={cue.id === picked}
          onclick={() => (picked = picked === cue.id ? null : cue.id)}
        >
          <span class="flex min-w-0 items-center gap-2">
            <span class="w-4 shrink-0 text-primary">
              {#if cue.id === playhead}<Play class="size-3.5" />{/if}
            </span>
            <span class="shrink-0 font-mono font-semibold">{cue.number}</span>
            <span class="min-w-0 grow truncate font-medium">{cue.title || t('group.untitled')}</span>
            {#if warnings.length}<Warning class="size-3.5 shrink-0 text-stop-soft" />{/if}
            <span class="shrink-0 text-xs font-semibold {typeColors[cue.type]}">{typeLabel(cue)}</span>
          </span>
          <span class="mt-0.5 flex min-w-0 items-center gap-2 pl-6 text-xs text-fg-2">
            <span
              class={[
                'min-w-0 truncate font-mono',
                status.bad && 'text-stop-soft',
                status.warn && 'font-semibold text-fade',
              ]}>{status.text}</span
            >
            {#if status.info}<span class="shrink-0 font-semibold text-primary">{status.info}</span>{/if}
            <span class="grow"></span>
            {#if arrow}<span class="shrink-0 text-[11px] text-muted-foreground">↓ {arrow}</span>{/if}
          </span>
          {#if status.progress != null}
            <span class="{bar} mt-1 ml-6">
              <span
                class={['block h-full w-full origin-left', status.paused ? 'bg-pause' : 'bg-progress']}
                style:transform="scaleX({status.progress})"
              ></span>
            </span>
          {/if}
          {#if cue.notes}
            <span class="mt-0.5 block truncate pl-6 text-xs text-muted-foreground"
              >{plainText(cue.notes.split('\n')[0])}</span
            >
          {/if}
        </button>
      {/if}
    {:else}
      <div class={empty}>{t('transport.no_cues')}</div>
    {/each}
  </div>
  {#if pickedCue && operates}
    <div class="flex flex-wrap items-center gap-2 border-t border-line px-3 py-2">
      <span class="min-w-0 grow truncate text-[13px] font-semibold">
        <span class="font-mono">{pickedCue.number}</span>
        {pickedCue.title || t('group.untitled')}
      </span>
      <Button size="sm" disabled={!canControl()} onclick={() => act('set_playhead')}>{t('remote.set_next')}</Button>
      <Button size="sm" variant="go" disabled={!canControl()} onclick={() => act('start_cue')}
        >{t('remote.start')}</Button
      >
      <Button size="sm" variant="stop" disabled={!canControl()} onclick={() => act('stop_cue')}>{t('type.stop')}</Button
      >
    </div>
  {/if}
</PanelFrame>
