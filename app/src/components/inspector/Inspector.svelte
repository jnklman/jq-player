<script lang="ts">
  import { untrack } from 'svelte';

  import Lock from '~icons/ph/lock-bold';
  import SpeakerHigh from '~icons/ph/speaker-high-fill';
  import Stop from '~icons/ph/stop-fill';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type CueId } from '../../lib/api';
  import { isPlaceholder } from '../../lib/cue';
  import { t } from '../../lib/i18n/index.svelte';
  import { boundKeys } from '../../lib/keys.svelte';
  import { cueById, mode, selection, ui } from '../../lib/store.svelte';
  import { empty, hint } from '../../lib/ui';
  import PanelFrame from '../layout/PanelFrame.svelte';
  import InspectorBody from './InspectorBody.svelte';

  // The selected cue (or the selected cues) in the inspector. Without `cueId` the inspector follows the
  // selection in the cue list.
  let { cueId }: { cueId?: CueId | null } = $props();

  const locked = $derived(mode() === 'show');

  // Several cues selected in the main list: their shared fields instead of one cue.
  const cues = $derived.by(() => {
    if (cueId === undefined && selection().length > 1) {
      const many = selection()
        .map(cueById)
        .filter((cue) => cue !== undefined);
      if (many.length > 1) return many;
    }
    const cue = cueById(cueId === undefined ? ui.selected : cueId);
    return cue ? [cue] : [];
  });
  // A new body for other cues or another type (see `InspectorBody`).
  const key = $derived(cues.map((cue) => `${cue.id}:${cue.type}`).join());

  // Before the body goes (the selection moved on, e.g. by a GO from MIDI or OSC that the selection follows), a
  // field being typed in loses the focus, so it commits to the cue it was made for: a removed input sends no
  // `change`.
  let frame = $state<HTMLElement | null>(null);
  $effect.pre(() => {
    void key;
    untrack(() => {
      const active = document.activeElement;
      if (active instanceof HTMLElement && frame?.contains(active)) active.blur();
    });
  });

  const single = $derived(cues.length === 1 ? cues[0] : null);
  // Previewing never touches the show, so it works in show mode too.
  const preview = $derived(single && ui.runtime?.preview?.cue === single.id ? ui.runtime.preview : null);
</script>

{#snippet headActions()}
  <span class="flex items-center gap-3">
    {#if locked}
      <Tooltip tip={t('inspector.locked_tip')}>
        {#snippet child({ props })}
          <span {...props} class="{hint} flex items-center gap-1.5 whitespace-nowrap">
            <Lock />
            {t('inspector.locked')}
          </span>
        {/snippet}
      </Tooltip>
    {/if}
    {#if single?.type === 'sound' && !isPlaceholder(single)}
      {@const id = single.id}
      <Tooltip
        tip={t('inspector.preview_tip')}
        keys={boundKeys(ui.view?.settings.keys ?? [], 'preview') || undefined}
        class={buttonVariants()}
        aria-pressed={preview != null}
        onclick={() => api.action({ action: 'preview', cue: id })}
      >
        {#if preview}<Stop class="size-3.5" /> {t('type.stop')}{:else}<SpeakerHigh class="size-4" />
          {t('action.preview')}{/if}
      </Tooltip>
    {/if}
  </span>
{/snippet}

<PanelFrame title={t('inspector.title')} actions={headActions}>
  {#if cues.length === 0}
    <div class={empty}>{t('inspector.click_cue')}</div>
  {:else}
    <div class="contents" bind:this={frame}>
      {#key key}
        <!-- A failing field must not take the panel (or the rest of the window) with it. -->
        <svelte:boundary onerror={(error) => console.error('Inspector:', error)}>
          <InspectorBody {cues} {locked} />
          {#snippet failed(error, reset)}
            <div class="{empty} flex flex-col items-center gap-2">
              {t('inspector.failed')}
              <span class="{hint} font-mono">{String(error)}</span>
              <Button size="sm" onclick={reset}>{t('inspector.retry')}</Button>
            </div>
          {/snippet}
        </svelte:boundary>
      {/key}
    </div>
  {/if}
</PanelFrame>
