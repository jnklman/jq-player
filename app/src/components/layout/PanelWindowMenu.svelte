<script lang="ts">
  import ArrowSquareIn from '~icons/ph/arrow-square-in-bold';
  import SlidersHorizontal from '~icons/ph/sliders-horizontal-bold';
  import { Button } from '$lib/components/ui/button';
  import * as Popover from '$lib/components/ui/popover';
  import { Slider } from '$lib/components/ui/slider';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { WindowKind } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { MIN_OPACITY, type PanelId } from '../../lib/layout.svelte';
  import { windowSupport } from '../../lib/window.svelte';
  import { placeFromWindow } from '../../lib/windows.svelte';

  // A panel window's own settings: window or overlay, the overlay's opacity,
  // back to the main window. The main window changes the layout for them.
  // Without overlays (Wayland) only the way back: the title bar's window menu
  // keeps the window on top there.
  let {
    id,
    kind,
    opacity,
    onopacity,
  }: { id: PanelId; kind: WindowKind; opacity: number; onopacity: (opacity: number, done: boolean) => void } = $props();
</script>

<Popover.Root>
  <Tooltip tip={t('window.label')} side="bottom">
    {#snippet child({ props: tipProps })}
      <Popover.Trigger {...tipProps}>
        {#snippet child({ props })}
          <button
            {...props}
            class="grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-hover hover:text-fg"
            aria-label={t('window.label')}
          >
            <SlidersHorizontal class="size-4" />
          </button>
        {/snippet}
      </Popover.Trigger>
    {/snippet}
  </Tooltip>
  <Popover.Content align="end" sideOffset={8} class="flex w-64 flex-col gap-3" aria-label={t('window.label')}>
    {#if windowSupport.overlay}
      <ToggleGroup.Root
        type="single"
        variant="outline"
        size="sm"
        aria-label={t('layout.kind')}
        bind:value={() => kind, (next) => next && placeFromWindow(id, next as WindowKind)}
      >
        <ToggleGroup.Item value="window">{t('layout.short_window')}</ToggleGroup.Item>
        <ToggleGroup.Item value="overlay">{t('layout.short_overlay')}</ToggleGroup.Item>
      </ToggleGroup.Root>
    {/if}
    {#if kind === 'overlay'}
      <div class="flex flex-col gap-1.5">
        <span class="text-xs text-muted-foreground">{t('layout.opacity')} · {Math.round(opacity * 100)} %</span>
        <Slider
          type="single"
          aria-label={t('layout.opacity')}
          step={0.05}
          min={MIN_OPACITY}
          max={1}
          value={opacity}
          format={(value) => `${Math.round(value * 100)} %`}
          onValueChange={(value) => onopacity(value, false)}
          onValueCommit={(value) => onopacity(value, true)}
          reset={1}
        />
      </div>
    {/if}
    <Button variant="outline" size="sm" onclick={() => placeFromWindow(id, 'main')}>
      <ArrowSquareIn class="size-3.5" />
      {t('layout.back_to_main')}
    </Button>
  </Popover.Content>
</Popover.Root>
