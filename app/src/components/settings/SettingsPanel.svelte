<script lang="ts">
  import type { Snippet } from 'svelte';

  import { type SettingsTab, tabTitle } from '../../lib/settings';
  import { provideSettingsTab, settingsView } from '../../lib/settingsView.svelte';
  import { hint } from '../../lib/ui';

  // The panel of a settings tab. Stays mounted while another tab is open, so
  // the search finds its rows; while searching it shows with its hits only.
  let { tab, description, children }: { tab: SettingsTab; description?: string; children: Snippet } = $props();

  const view = settingsView();
  // svelte-ignore state_referenced_locally
  provideSettingsTab({ id: tab, title: () => tabTitle(tab) });
</script>

<div
  id="settings-panel-{tab}"
  role="tabpanel"
  aria-labelledby="settings-tab-{tab}"
  hidden={!view.showsPanel(tab)}
  class={['flex flex-col gap-5 px-5.5', view.searching ? 'pt-4 pb-2' : 'pt-4.5 pb-7']}
>
  <header class="flex flex-col gap-1">
    <h3 class={['m-0 font-semibold text-balance', view.searching ? 'text-[13px] text-muted-foreground' : 'text-base']}>
      {tabTitle(tab)}
    </h3>
    {#if description && !view.searching}<p class="{hint} m-0">{description}</p>{/if}
  </header>
  {@render children()}
</div>
