<script lang="ts">
  import { untrack } from 'svelte';

  import WarningIcon from '~icons/ph/warning-fill';
  import * as Tooltip from '$lib/components/ui/tooltip';

  import AboutDialog from './components/AboutDialog.svelte';
  import ClashDialog from './components/ClashDialog.svelte';
  import DevicesDialog from './components/DevicesDialog.svelte';
  import Footer from './components/Footer.svelte';
  import GroupDialog from './components/GroupDialog.svelte';
  import Header from './components/Header.svelte';
  import KeyboardScope from './components/KeyboardScope.svelte';
  import Workspace from './components/layout/Workspace.svelte';
  import LeaveDialog from './components/LeaveDialog.svelte';
  import Notices from './components/Notices.svelte';
  import OutputsDialog from './components/OutputsDialog.svelte';
  import QuestionDialog from './components/QuestionDialog.svelte';
  import RecoveryDialog from './components/RecoveryDialog.svelte';
  import RelinkDialog from './components/RelinkDialog.svelte';
  import Settings from './components/settings/Settings.svelte';
  import StopwatchesDialog from './components/StopwatchesDialog.svelte';
  import WaveformDialog from './components/waveform/WaveformDialog.svelte';
  import Welcome from './components/Welcome.svelte';
  import { api } from './lib/api';
  import { t } from './lib/i18n/index.svelte';
  import { select, selectionMode, ui } from './lib/store.svelte';
  import { hasKeys } from './lib/window.svelte';

  // Selection follows the playhead (also when linked): whenever it moves (or the mode is chosen),
  // its cue gets selected; past the end, nothing is. Both are primitives, so
  // new show or state events alone don't take a selection made by hand away.
  const playhead = $derived(ui.runtime?.playhead ?? null);
  const following = $derived(['selection_follows', 'linked'].includes(selectionMode()));
  $effect(() => {
    if (!following) return;
    const id = playhead;
    untrack(() => select(id));
  });

  // Keys only reach the app while one of its windows has the focus.
  const focusLost = $derived(!!ui.view && !hasKeys() && ui.view.settings.focus_warning);
  $effect(() => {
    document.documentElement.toggleAttribute('data-focus-lost', focusLost);
    // Windows draws the window edge itself.
    if (document.documentElement.hasAttribute('data-windows')) void api.windowAlert(focusLost).catch(() => {});
  });
</script>

<KeyboardScope />

<Tooltip.Provider delayDuration={500} ignoreNonKeyboardFocus>
  {#if ui.view}
    <div class="flex h-full flex-col">
      <Header />
      <Workspace />
      <Footer />
    </div>
    {#if ui.view.recovery != null}
      <RecoveryDialog savedAt={ui.view.recovery} />
    {/if}
    {#if ui.relinkOpen}
      <RelinkDialog onclose={() => (ui.relinkOpen = false)} />
    {/if}
    {#if ui.groupOpen}
      <GroupDialog groupId={ui.groupOpen} onclose={() => (ui.groupOpen = null)} />
    {/if}
    <!-- Opened from the inspector, also the one in the group dialog, so after it. -->
    {#if ui.waveformOpen}
      <WaveformDialog cueId={ui.waveformOpen} onclose={() => (ui.waveformOpen = null)} />
    {/if}
    {#if ui.devicesOpen}
      <DevicesDialog onclose={() => (ui.devicesOpen = false)} />
    {/if}
    {#if ui.outputsOpen}
      <OutputsDialog onclose={() => (ui.outputsOpen = false)} />
    {/if}
    {#if ui.stopwatchesOpen}
      <StopwatchesDialog onclose={() => (ui.stopwatchesOpen = false)} />
    {/if}
    {#if ui.settingsOpen}
      <Settings onclose={() => (ui.settingsOpen = false)} />
    {/if}
  {:else if ui.started}
    <Welcome />
  {/if}
  {#if ui.aboutOpen}
    <AboutDialog onclose={() => (ui.aboutOpen = false)} />
  {/if}
  {#if ui.leaving}
    <LeaveDialog leaving={ui.leaving} onclose={() => (ui.leaving = null)} />
  {/if}
  <!-- Asked from within other dialogs (e.g. the group dialog), so above them. -->
  {#if ui.question}
    <QuestionDialog question={ui.question} />
  {/if}
  {#if ui.clashQuestion}
    <ClashDialog question={ui.clashQuestion} />
  {/if}
  <Notices />
  {#if focusLost}
    <!-- Keys (GO!) do nothing while another window has the focus. Laid over everything, so nothing moves; the red
    window edge comes with it (`data-focus-lost` in styles.css). -->
    <div
      class="pointer-events-none fixed bottom-5 left-1/2 z-9999 flex -translate-x-1/2 flex-col items-center gap-2 rounded-lg border border-stop bg-stop-bg px-4 py-2 font-semibold whitespace-nowrap text-fg shadow-lg"
      role="alert"
    >
      <div class="flex items-center gap-2 text-2xl font-extrabold">
        <WarningIcon class="size-6 text-stop-soft " />
        {t('app.no_focus')}
      </div>
      <div class="text-sm font-semibold">{t('app.no_focus_text')}</div>
    </div>
  {/if}
</Tooltip.Provider>
