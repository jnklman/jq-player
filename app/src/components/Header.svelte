<script lang="ts">
  import { useNow } from '@ariefsn/svelte-use/time/useNow.svelte';

  import Undo from '~icons/ph/arrow-u-up-left-bold';
  import Redo from '~icons/ph/arrow-u-up-right-bold';
  import CaretDown from '~icons/ph/caret-down-bold';
  import FloppyDisk from '~icons/ph/floppy-disk-bold';
  import GearSix from '~icons/ph/gear-six-bold';
  import LockSimple from '~icons/ph/lock-simple-bold';
  import * as Popover from '$lib/components/ui/popover';
  import { Switch } from '$lib/components/ui/switch';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api } from '../lib/api';
  import { redo, setMode, undo } from '../lib/commands';
  import { clock, shortDevice } from '../lib/format';
  import { fmtNumber, t } from '../lib/i18n/index.svelte';
  import { attempt, type Mode, mode as currentMode, saveShow, ui } from '../lib/store.svelte';
  import { dragFill } from '../lib/ui';
  import AppMenu from './AppMenu.svelte';
  import LayoutMenu from './layout/LayoutMenu.svelte';
  import ShowMenu from './ShowMenu.svelte';
  import Stopwatches from './Stopwatches.svelte';
  import WindowControls from './WindowControls.svelte';

  let now = useNow();

  const view = $derived(ui.view!);
  const keepAlive = $derived(view.keep_alive);

  let menuOpen = $state(false);
</script>

<!-- Doubles as the title bar: everything but controls drags the window, double-click maximizes.
     Row 1 never wraps, so the window buttons stay put in a narrow window. -->
<header class="flex flex-col border-b border-line bg-panel">
  <div class="grid grid-cols-[1fr_auto_1fr] gap-4 px-5 pt-2.5 pb-1.5" data-tauri-drag-region="deep">
    <div class="flex min-w-0 items-center gap-2.5">
      <!-- The menus open in a portal, outside the drag region. -->
      <div class="flex shrink-0" data-tauri-drag-region="false">
        <AppMenu />
      </div>
      <span class="text-muted-foreground">·</span>
      <div class="min-w-0" data-tauri-drag-region="false">
        <Popover.Root bind:open={menuOpen}>
          <Tooltip tip={t('header.show_settings')} side="bottom">
            {#snippet child({ props: tipProps })}
              <Popover.Trigger {...tipProps}>
                {#snippet child({ props })}
                  <button
                    {...props}
                    class="flex max-w-full items-center gap-1 rounded-md px-1.5 py-0.5 font-medium hover:bg-hover"
                  >
                    <span class="truncate">{view.name}</span>
                    <CaretDown class="size-4 shrink-0 text-muted-foreground" />
                  </button>
                {/snippet}
              </Popover.Trigger>
            {/snippet}
          </Tooltip>
          <Popover.Content
            align="start"
            sideOffset={8}
            class="max-h-(--bits-popover-content-available-height) w-80 overflow-y-auto"
            aria-label={t('header.show')}
          >
            <ShowMenu onclose={() => (menuOpen = false)} />
          </Popover.Content>
        </Popover.Root>
      </div>
      {#if !view.settings.show_mode}
        <Tooltip
          tip={t('header.undo')}
          keys="{t('key.ctrl')} + Z"
          side="bottom"
          class="relative grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:not-disabled:bg-hover hover:not-disabled:text-fg disabled:opacity-60"
          disabled={!view.can_undo}
          aria-label={t('header.undo')}
          onclick={undo}
        >
          <Undo />
        </Tooltip>
        <Tooltip
          tip={t('header.redo')}
          keys="{t('key.ctrl')} + Y"
          side="bottom"
          class="relative grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:not-disabled:bg-hover hover:not-disabled:text-fg disabled:opacity-60"
          disabled={!view.can_redo}
          aria-label={t('header.redo')}
          onclick={redo}
        >
          <Redo />
        </Tooltip>
      {/if}
      {#if !view.settings.show_mode || view.dirty}
        <Tooltip
          tip={view.dirty ? t('header.save_dirty') : t('common.save')}
          keys="{t('key.ctrl')} + S"
          side="bottom"
          class="relative grid size-6 shrink-0 place-items-center rounded-md text-muted-foreground hover:bg-hover hover:text-fg"
          aria-label={view.dirty ? t('header.save_dirty_label') : t('common.save')}
          onclick={saveShow}
        >
          <FloppyDisk class="size-4" />
          {#if view.dirty}
            <span class="absolute top-0.5 right-0.5 size-1.5 rounded-full bg-fade ring-2 ring-panel"></span>
          {/if}
        </Tooltip>
      {/if}
      {#if ui.justSaved && !view.dirty}
        <span class="shrink-0 text-xs text-muted-foreground">{t('header.saved')}</span>
      {/if}
      <div class={dragFill} data-tauri-drag-region="deep"></div>
    </div>

    <span
      class="flex items-center font-mono text-xl leading-none font-semibold tabular-nums"
      data-tauri-drag-region="deep">{clock(new Date(now()))}</span
    >
    <div class="flex items-center">
      <div class={dragFill} data-tauri-drag-region="deep"></div>
      <WindowControls />
    </div>
  </div>

  <div class="flex min-h-12.5 flex-wrap items-center gap-4 px-5 pt-1.5 pb-2.5">
    {#if view.show.show_mode_locked}
      <Tooltip tip={t('header.locked_tip')} side="bottom">
        {#snippet child({ props })}
          <span {...props} class="flex min-h-7 items-center gap-1.5 text-xs text-muted-foreground">
            <LockSimple class="size-4" />
            {t('header.locked')}
          </span>
        {/snippet}
      </Tooltip>
    {:else}
      <ToggleGroup.Root
        type="single"
        variant="outline"
        aria-label={t('header.mode')}
        bind:value={() => currentMode(), (mode) => mode && setMode(mode as Mode)}
      >
        <ToggleGroup.Item value="edit">{t('header.edit')}</ToggleGroup.Item>
        <ToggleGroup.Item value="show">{t('header.show')}</ToggleGroup.Item>
      </ToggleGroup.Root>
    {/if}

    <Stopwatches />

    <div class="grow"></div>

    <div class="flex flex-wrap items-center gap-3.5 text-xs text-muted-foreground">
      <Tooltip
        tip={view.audio_error ??
          (view.audio_lost && t('header.audio_lost', { reason: view.audio_lost })) ??
          `${view.audio.device_detail}\n${view.audio.host} · ${view.audio.sample_rate} Hz · ${t('header.channels', { count: view.audio.channels })} · ${view.audio.sample_format}`}
        side="bottom"
      >
        {#snippet child({ props })}
          <span {...props} class="flex max-w-80 min-w-0 items-center gap-1.5">
            <span
              class={['size-2 shrink-0 rounded-full', view.audio_error || view.audio_lost ? 'bg-stop-soft' : 'bg-go']}
            ></span>
            <span class="truncate">{shortDevice(view.audio.device)}</span>
            <span class="shrink-0"
              >· {fmtNumber(view.audio.sample_rate / 1000)} kHz · {view.audio.channels} {t('header.ch')}</span
            >
          </span>
        {/snippet}
      </Tooltip>
      <Tooltip
        tip={keepAlive.state === 'failed'
          ? t('header.keep_awake_error', { error: keepAlive.error ?? '' })
          : t('header.keep_awake_tip')}
        side="bottom"
      >
        {#snippet child({ props })}
          <label
            {...props}
            class="flex min-h-7 cursor-pointer items-center gap-2 rounded-full border border-input bg-secondary py-0 pr-2.5 pl-1.5 text-xs text-fg"
          >
            <Switch
              class={keepAlive.state === 'failed' ? 'data-unchecked:bg-stop' : ''}
              bind:checked={
                () => keepAlive.state === 'active', () => attempt(api.setKeepAwake(keepAlive.state === 'off'))
              }
            />
            {keepAlive.state === 'failed' ? t('header.keep_awake_failed') : t('header.keep_awake')}
          </label>
        {/snippet}
      </Tooltip>
      <LayoutMenu />
      <Tooltip
        tip={t('header.settings')}
        side="bottom"
        class="flex size-7 items-center justify-center rounded-full border border-input bg-secondary text-fg hover:bg-accent"
        aria-label={t('header.settings')}
        onclick={() => (ui.settingsOpen = true)}
      >
        <GearSix class="size-4" />
      </Tooltip>
    </div>
  </div>
</header>
