<script lang="ts">
  import type { PanelId } from '../../lib/layout.svelte';
  import { getPanelHosts, setPanelId } from './context';
  import { panelById } from './panels';

  // One panel of the workspace, rendered once and moved into its layout slot (`hosts.ts`).
  let { id }: { id: PanelId } = $props();

  // Keyed by id in the workspace, so the id never changes.
  // svelte-ignore state_referenced_locally
  setPanelId(id);
  // svelte-ignore state_referenced_locally
  const Panel = panelById(id)?.component;
  const hosts = getPanelHosts();
</script>

<div class="h-full min-h-0 min-w-0" data-panel={id} {@attach hosts.host(id)}>
  <Panel />
</div>
