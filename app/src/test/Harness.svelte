<script lang="ts">
  // Mounts a component as the app does: inside the tooltip provider (`App.svelte`, `PanelWindow.svelte`).
  import type { Component } from 'svelte';

  import * as Tooltip from '$lib/components/ui/tooltip';

  // eslint-disable-next-line @typescript-eslint/no-explicit-any -- any component with any props
  let { component: Inner, props = {} }: { component: Component<any>; props?: Record<string, unknown> } = $props();
</script>

<Tooltip.Provider delayDuration={0}>
  <Inner {...props} />
</Tooltip.Provider>
