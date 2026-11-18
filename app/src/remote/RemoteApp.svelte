<script lang="ts">
  import { MediaQuery } from 'svelte/reactivity';

  import Keyboard from '~icons/ph/keyboard-bold';
  import LockSimple from '~icons/ph/lock-simple-bold';
  import LockSimpleOpen from '~icons/ph/lock-simple-open-bold';
  import WifiSlash from '~icons/ph/wifi-slash-bold';
  import { Badge } from '$lib/components/ui/badge';
  import { Button } from '$lib/components/ui/button';
  import * as ToggleGroup from '$lib/components/ui/toggle-group';
  import * as Tooltip from '$lib/components/ui/tooltip';

  import ActiveCues from '../components/active/ActiveCues.svelte';
  import Notices from '../components/Notices.svelte';
  import PlayheadCue from '../components/PlayheadCue.svelte';
  import StopwatchesPanel from '../components/StopwatchesPanel.svelte';
  import Transport from '../components/Transport.svelte';
  import { i18n, t } from '../lib/i18n/index.svelte';
  import { keyboard } from '../lib/keys.svelte';
  import { ui } from '../lib/store.svelte';
  import JoinView from './JoinView.svelte';
  import { onRemoteKey } from './keys';
  import KeysDialog from './KeysDialog.svelte';
  import { remote, setLocked } from './remote.svelte';
  import RemoteCueList from './RemoteCueList.svelte';

  // The remote page: a status bar, then the panels laid out by the screen. A phone (portrait) has the transport on
  // top and the rest in tabs; a tablet in portrait running cues and cue list side by side under it; a wide screen the
  // cue list on the left and the transport, running cues and notes stacked on the right. Without a connection
  // everything is covered: frozen times would mislead.

  type Tab = 'active' | 'cues' | 'playhead' | 'stopwatches';
  let tab = $state<Tab>('active');
  const tabs: { id: Tab; key: 'active.title' | 'cuelist.title' | 'playhead.title' | 'stopwatches.title' }[] = [
    { id: 'active', key: 'active.title' },
    { id: 'cues', key: 'cuelist.title' },
    { id: 'playhead', key: 'playhead.title' },
    { id: 'stopwatches', key: 'stopwatches.title' },
  ];

  const role = $derived(ui.remote?.role ?? 'pending');
  const connected = $derived(ui.remote?.connected ?? false);
  const locked = $derived(ui.remote?.locked ?? false);
  // The device's own keys: only where it operates and has a keyboard.
  let keysOpen = $state(false);
  const keysOffered = $derived(role === 'control' && connected && !!ui.view && keyboard.present);
  $effect(() => {
    if (!keysOffered) keysOpen = false;
  });

  // Tailwind's `lg` and `md`; one layout is rendered, not three hidden ones following every state.
  const wide = new MediaQuery('(min-width: 64rem)');
  const medium = new MediaQuery('(min-width: 48rem)');

  let now = $state(new Date());
  $effect(() => {
    const timer = setInterval(() => (now = new Date()), 1000);
    return () => clearInterval(timer);
  });
  const clock = $derived(
    now.toLocaleTimeString(i18n.locale, { hour: '2-digit', minute: '2-digit', second: '2-digit' }),
  );
</script>

<svelte:window onkeydown={onRemoteKey} />

<Tooltip.Provider delayDuration={500} ignoreNonKeyboardFocus>
  <div class="flex h-dvh touch-manipulation flex-col bg-canvas text-fg [-webkit-tap-highlight-color:transparent]">
    {#if remote.phase === 'starting'}
      <div class="m-auto p-4 text-center text-[13px] text-muted-foreground">
        {remote.unreachable ? t('remote.unreachable') : t('remote.connecting')}
      </div>
    {:else if remote.phase === 'join'}
      <JoinView />
    {:else}
      <header class="flex items-center gap-3 border-b border-line bg-panel px-4 py-2">
        <span class="min-w-0 grow truncate text-sm font-semibold">{ui.view?.name ?? 'jQ Player'}</span>
        <Badge variant="outline" class={connected ? 'text-go' : 'text-stop-soft'}>
          {connected ? t('remote.connected') : t('remote.disconnected_short')}
        </Badge>
        <Badge variant="outline" class="hidden sm:inline-flex">{t(`remote.role.${role}`)}</Badge>
        {#if keysOffered}
          <Button size="sm" variant="outline" aria-label={t('remote.keys_title')} onclick={() => (keysOpen = true)}>
            <Keyboard class="size-3.5" />
            <span class="hidden sm:inline">{t('remote.keys')}</span>
          </Button>
        {/if}
        {#if role === 'control'}
          <Button
            size="sm"
            variant={locked ? 'default' : 'outline'}
            aria-pressed={locked}
            onclick={() => setLocked(!locked)}
          >
            {#if locked}<LockSimple class="size-3.5" />{:else}<LockSimpleOpen class="size-3.5" />{/if}
            <span class="hidden sm:inline">{locked ? t('remote.unlock') : t('remote.lock')}</span>
          </Button>
        {/if}
        <span class="shrink-0 font-mono text-sm text-fg-2 tabular-nums">{clock}</span>
      </header>

      <main class="relative flex min-h-0 flex-1 flex-col">
        {#if role === 'pending'}
          <div class="m-auto max-w-md p-4 text-center">
            <h1 class="mb-2 text-lg font-semibold">{t('remote.waiting_title')}</h1>
            <p class="text-[13px] text-muted-foreground">{t('remote.waiting_text', { name: remote.name })}</p>
          </div>
        {:else if !ui.started}
          <div class="m-auto p-4 text-[13px] text-muted-foreground">{t('remote.connecting')}</div>
        {:else if !ui.view}
          <div class="m-auto p-4 text-[13px] text-muted-foreground">{t('remote.no_show')}</div>
        {:else}
          {#if wide.current}
            <!-- Wide: cue list left, the rest stacked on the right. -->
            <div class="grid min-h-0 flex-1 grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-2 p-2">
              <RemoteCueList />
              <div class="flex min-h-0 flex-col gap-2">
                <div class="flex-none"><Transport /></div>
                <div class="min-h-0 flex-1"><ActiveCues /></div>
                <div class="min-h-0 basis-1/4"><PlayheadCue /></div>
              </div>
            </div>
          {:else if medium.current}
            <!-- Tablet portrait: transport on top, running cues and cue list side by side. -->
            <div class="flex min-h-0 flex-1 flex-col gap-2 p-2">
              <div class="flex-none"><Transport /></div>
              <div class="grid min-h-0 flex-1 grid-cols-2 gap-2">
                <ActiveCues />
                <RemoteCueList />
              </div>
            </div>
          {:else}
            <!-- Phone: transport on top, the rest in tabs. -->
            <div class="flex min-h-0 flex-1 flex-col gap-2 p-2">
              <div class="flex-none"><Transport /></div>
              <ToggleGroup.Root
                type="single"
                value={tab}
                onValueChange={(value) => value && (tab = value as Tab)}
                class="w-full"
                aria-label={t('remote.views')}
              >
                {#each tabs as item (item.id)}
                  <ToggleGroup.Item value={item.id} class="min-w-0 flex-1 truncate px-1 text-xs"
                    >{t(item.key)}</ToggleGroup.Item
                  >
                {/each}
              </ToggleGroup.Root>
              <div class="min-h-0 flex-1">
                {#if tab === 'active'}
                  <ActiveCues />
                {:else if tab === 'cues'}
                  <RemoteCueList />
                {:else if tab === 'playhead'}
                  <PlayheadCue />
                {:else}
                  <StopwatchesPanel />
                {/if}
              </div>
            </div>
          {/if}
        {/if}

        {#if !connected}
          <div
            class="absolute inset-0 z-40 flex flex-col items-center justify-center gap-2 bg-canvas/85 p-4 text-center backdrop-blur-sm"
            role="alert"
          >
            <WifiSlash class="size-10 text-stop-soft" />
            <span class="text-lg font-semibold text-stop-soft">{t('remote.disconnected')}</span>
            <span class="text-[13px] text-muted-foreground">{t('remote.reconnecting')}</span>
          </div>
        {/if}
      </main>
    {/if}
  </div>
  {#if keysOpen}<KeysDialog onclose={() => (keysOpen = false)} />{/if}
  <Notices />
</Tooltip.Provider>
