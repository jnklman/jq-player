<script lang="ts">
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { api, type LocalSettings } from '../../lib/api';
  import { AudioSetup } from '../../lib/audioSetup.svelte';
  import { t } from '../../lib/i18n/index.svelte';
  import { changedFields, settingsProblem, tabProblems, withEdits } from '../../lib/settings';
  import { provideSettingsView, SettingsView } from '../../lib/settingsView.svelte';
  import { attempt, notify, ui } from '../../lib/store.svelte';
  import EditDialog from '../EditDialog.svelte';
  import DeviceTab from './DeviceTab.svelte';
  import MediaTab from './MediaTab.svelte';
  import MidiTab from './MidiTab.svelte';
  import OscTab from './OscTab.svelte';
  import RoutingTab from './RoutingTab.svelte';
  import SettingsNav from './SettingsNav.svelte';
  import ShortcutsTab from './ShortcutsTab.svelte';
  import ShowTab from './ShowTab.svelte';
  import UiTab from './UiTab.svelte';

  // The machine settings in tabs. Edited as a draft and sent on "Apply", which keeps the dialog open; the
  // backend saves and applies them. Only the fields the dialog edits are sent, the rest as the backend has them.
  let { onclose }: { onclose: () => void } = $props();

  // What was last applied (the baseline of "changed").
  let initial = $state<LocalSettings>($state.snapshot(ui.view!.settings));
  // svelte-ignore state_referenced_locally
  let draft = $state<LocalSettings>($state.snapshot(initial));
  // MIDI bindings are not part of the draft (they are learned and saved at once, see `ShortcutsTab`).
  const changed = $derived(changedFields(draft, initial));
  const changes = $derived([...changed.values()].reduce((sum, count) => sum + count, 0));
  const setup = new AudioSetup(
    () => draft,
    () => ui.view!,
  );
  const problems = $derived(tabProblems(ui.view!, draft, setup));

  const view = new SettingsView();
  provideSettingsView(view);
  let search = $state<HTMLInputElement | null>(null);
  let scroller = $state<HTMLElement | null>(null);

  // A new tab starts at its top.
  $effect(() => {
    void view.tab;
    if (scroller) scroller.scrollTop = 0;
  });

  async function apply(): Promise<boolean> {
    const problem = settingsProblem(draft);
    if (problem) {
      notify({ level: 'error', message: problem });
      return false;
    }
    const applied = await attempt(
      api.setSettings(withEdits($state.snapshot(ui.view!.settings), $state.snapshot(draft))),
    );
    if (!applied) return false;
    // The backend's version is the new baseline (and draft), so nothing counts as changed.
    initial = applied.settings;
    draft = structuredClone(applied.settings);
    return true;
  }

  // For MIDI learn: opens the input chosen in the draft, applying nothing else.
  async function openMidiPort(): Promise<boolean> {
    const port = draft.midi_port;
    const applied = await attempt(api.setSettings({ ...$state.snapshot(ui.view!.settings), midi_port: port }));
    if (!applied) return false;
    initial.midi_port = port;
    if (applied.midi.port) return true;
    notify({ level: 'error', message: applied.midi.error ?? t('settings.midi_failed') });
    return false;
  }

  function onkeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      search?.focus();
      search?.select();
    }
  }
</script>

<EditDialog
  title={t('header.settings')}
  class="h-[min(44rem,calc(100%-2rem))] max-w-240"
  dirty={changes > 0}
  valid={setup.channelsValid}
  invalidTip={t('settings.channels_invalid')}
  closeOnApply={false}
  onapply={apply}
  {onclose}
>
  {#snippet status()}
    <span class="text-xs text-muted-foreground">
      {#if changes}
        <span class="font-semibold text-primary">{changes}</span>
        {t('settings.changes', { count: changes })}
      {:else}
        {t('settings.machine_only')}
      {/if}
    </span>
  {/snippet}
  <!-- svelte-ignore a11y_no_static_element_interactions -->
  <div class="grid min-h-0 flex-1 grid-cols-[13rem_minmax(0,1fr)] max-sm:grid-cols-1" {onkeydown}>
    <SettingsNav {changed} {problems} bind:search />
    <ScrollArea bind:viewportRef={scroller} viewportClass="scroll-pt-3">
      <DeviceTab bind:draft {setup} />
      <RoutingTab bind:draft {setup} />
      <ShowTab bind:draft />
      <ShortcutsTab bind:draft {openMidiPort} />
      <MidiTab bind:draft />
      <OscTab bind:draft />
      <UiTab bind:draft />
      <MediaTab bind:draft />
      {#if view.searching && !view.found()}
        <p class="m-0 px-5 py-12 text-center text-[13px] text-muted-foreground">
          {t('settings.nothing_found', { query: view.query.trim() })}
        </p>
      {/if}
    </ScrollArea>
  </div>
</EditDialog>
