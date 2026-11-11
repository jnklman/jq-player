<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import { Button } from '$lib/components/ui/button';
  import * as Tooltip from '$lib/components/ui/tooltip';

  import { api } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { type PanelId, storedWindow } from '../../lib/layout.svelte';
  import { mode, ui } from '../../lib/store.svelte';
  import { empty } from '../../lib/ui';
  import { hasKeys, windowSupport } from '../../lib/window.svelte';
  import { setOpacityFromWindow } from '../../lib/windows.svelte';
  import ClashDialog from '../ClashDialog.svelte';
  import KeyboardScope from '../KeyboardScope.svelte';
  import Notices from '../Notices.svelte';
  import QuestionDialog from '../QuestionDialog.svelte';
  import WindowControls from '../WindowControls.svelte';
  import { setPanelId, setWindowChrome } from './context';
  import { panelById } from './panels';
  import PanelWindowMenu from './PanelWindowMenu.svelte';

  // A panel in a window of its own (opened by the backend, see
  // `panel_windows.rs`): the same component as in the main window, whose
  // title row is the window's title bar. The keys work as in the main window;
  // dialogs open there (`lib/windows.svelte.ts`). What kind of window it is
  // and its opacity are read from the layout, which only the main window writes.
  let { id }: { id: PanelId } = $props();

  // Fixed for the window's life (`__JQ_PANEL__`).
  // svelte-ignore state_referenced_locally
  setPanelId(id);
  // svelte-ignore state_referenced_locally
  const def = panelById(id);
  const Panel = def?.component;

  const stored = $derived(storedWindow(ui.view?.settings.layout, mode(), id));
  // Without overlays (Wayland) an overlay of the layout is a normal window.
  const kind = $derived(stored?.kind === 'overlay' && windowSupport.overlay ? 'overlay' : 'window');
  const overlay = $derived(kind === 'overlay');
  // The system's title bar (Wayland): moves the window and has its buttons and window menu.
  const native = windowSupport.native_frame;
  document.documentElement.toggleAttribute('data-native-frame', native);

  // The slider's value until the saved layout has caught up with it.
  let dragged = $state<number | null>(null);
  let sliding = false;
  const opacity = $derived(dragged ?? stored?.opacity ?? 1);
  $effect(() => {
    const saved = stored?.opacity;
    if (!sliding && dragged !== null && saved !== undefined && Math.abs(saved - dragged) < 0.001) dragged = null;
  });

  function setOpacity(value: number, done: boolean) {
    sliding = !done;
    dragged = value;
    setOpacityFromWindow(id, value);
  }

  $effect(() => {
    void getCurrentWindow()
      .setTitle(`${def?.title ?? t('layout.panel')} – jQ Player`)
      .catch(() => {});
  });

  // The overlay's background lets through what lies behind it (`styles.css`).
  $effect(() => {
    const root = document.documentElement;
    root.toggleAttribute('data-overlay', overlay);
    root.style.setProperty('--overlay-opacity', String(overlay ? opacity : 1));
  });

  // No window of the app has the keys: the window edge turns red, as in the main window.
  const focusLost = $derived(!!ui.view && !hasKeys() && ui.view.settings.focus_warning);
  $effect(() => {
    document.documentElement.toggleAttribute('data-focus-lost', focusLost);
    if (document.documentElement.hasAttribute('data-windows')) void api.windowAlert(focusLost).catch(() => {});
  });

  setWindowChrome({ controls: chrome, drag: !native });
</script>

{#snippet chrome()}
  {#if !native}
    <PanelWindowMenu {id} {kind} {opacity} onopacity={setOpacity} />
    <WindowControls {overlay} />
  {/if}
{/snippet}

<KeyboardScope />

<Tooltip.Provider delayDuration={500} ignoreNonKeyboardFocus>
  {#if Panel}
    {#if ui.view}
      <Panel />
    {/if}
  {:else}
    <div class="flex h-full flex-col items-start gap-3 {empty}" data-tauri-drag-region="deep">
      {t('layout.panel_gone')}
      <Button variant="outline" size="sm" onclick={() => getCurrentWindow().close()}>{t('common.close')}</Button>
    </div>
  {/if}
  <!-- Asked from within the panel (e.g. deleting cues); other dialogs open in the main window. -->
  {#if ui.question}
    <QuestionDialog question={ui.question} />
  {/if}
  {#if ui.clashQuestion}
    <ClashDialog question={ui.clashQuestion} />
  {/if}
  <Notices />
</Tooltip.Provider>
