<script lang="ts">
  import { getCurrentWindow } from '@tauri-apps/api/window';

  import ArrowsInSimple from '~icons/ph/arrows-in-simple-bold';
  import ArrowsOutSimple from '~icons/ph/arrows-out-simple-bold';
  import Minus from '~icons/ph/minus-bold';
  import XMark from '~icons/ph/x-bold';
  import { windowState } from '$lib/window.svelte';

  import { t } from '../lib/i18n/index.svelte';

  // Replaces the system title bar (`decorations: false`). Closing goes
  // through the usual close request, so unsaved changes are still asked about
  // (a panel window's panel goes back to the main window). An overlay is not
  // in the taskbar, so it is never minimized; nor maximized.
  let { overlay = false }: { overlay?: boolean } = $props();

  const appWindow = getCurrentWindow();

  const maximized = $derived(windowState.maximized);

  const windowButtons = () => [
    {
      label: t('window.minimize'),
      colors: 'hover:bg-win-min text-win-min-fg',
      icon: Minus,
      run: () => appWindow.minimize(),
    },
    {
      label: maximized ? t('window.restore') : t('window.maximize'),
      colors: 'hover:bg-win-max text-win-max-fg',
      icon: maximized ? ArrowsInSimple : ArrowsOutSimple,
      run: () => appWindow.toggleMaximize(),
    },
  ];

  const buttons = $derived([
    ...(overlay ? [] : windowButtons()),
    {
      label: t('common.close'),
      colors: 'hover:bg-win-close text-win-close-fg',
      icon: XMark,
      run: () => appWindow.close(),
    },
  ]);
</script>

<div class="ml-4 flex items-center gap-2" role="group" aria-label={t('window.label')}>
  {#each buttons as button, i (i)}
    {@const Icon = button.icon}
    <button
      class="group flex size-4 items-center justify-center rounded-full bg-dim p-0 {button.colors}"
      aria-label={button.label}
      tabindex="-1"
      onclick={button.run}
    >
      <Icon class="size-2.5 opacity-0 group-hover:opacity-100" />
    </button>
  {/each}
</div>
