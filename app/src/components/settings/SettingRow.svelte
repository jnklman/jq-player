<script lang="ts">
  import type { Snippet } from 'svelte';

  import { useSetting } from '../../lib/settingsView.svelte';

  // One setting: its label left, the control (with hints below it) right.
  // `keywords` are found by the search besides the label ("Latenz" → buffer size).
  let {
    label,
    keywords = '',
    badge,
    children,
  }: { label: string; keywords?: string; badge?: Snippet; children: Snippet } = $props();

  const id = $props.id();
  let el = $state<HTMLElement | null>(null);
  const setting = useSetting(
    () => `${label} ${keywords}`,
    () => el,
  );
</script>

<div
  bind:this={el}
  role="group"
  aria-labelledby={id}
  hidden={!setting.visible}
  class="grid grid-cols-[11rem_minmax(0,1fr)] items-start gap-4 border-b border-row-line py-2.5 last:border-b-0 max-sm:grid-cols-1 max-sm:gap-1.5"
>
  <span {id} class="flex flex-wrap items-center gap-2 pt-1.5 text-[13px] text-fg-2 max-sm:pt-0">
    {label}{@render badge?.()}
  </span>
  <div class="flex min-w-0 flex-col gap-1.5">{@render children()}</div>
</div>
