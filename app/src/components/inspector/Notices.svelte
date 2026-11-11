<script lang="ts">
  import CaretDown from '~icons/ph/caret-down-bold';
  import Warning from '~icons/ph/warning-bold';

  import { t } from '../../lib/i18n/index.svelte';
  import type { InspectorNotice, TabDef } from '../../lib/inspector.svelte';

  // Every problem of the cue in one box; a notice with a place jumps there.
  let {
    notices,
    tabs,
    onreveal,
  }: { notices: InspectorNotice[]; tabs: TabDef[]; onreveal: (notice: InspectorNotice) => void } = $props();

  const tabLabel = (notice: InspectorNotice) => tabs.find((tab) => tab.id === notice.tab)?.label;
</script>

{#if notices.length}
  <details class="group mx-4 mb-2.5 rounded-lg border border-stop-line bg-panel-2 text-xs" open>
    <summary class="flex cursor-pointer items-center gap-2 px-3 py-1 text-stop-soft">
      <Warning class="size-3.5 shrink-0" />
      <span class="grow font-semibold">{t('inspector.notices', { count: notices.length })}</span>
      <CaretDown
        class="text-muted-foreground transition-transform group-open:rotate-180 motion-reduce:transition-none"
      />
    </summary>
    <ul class="m-0 flex list-none flex-col gap-1 px-3 pt-0 pb-2 pl-8.5">
      {#each notices as notice, i (i)}
        {@const label = tabLabel(notice)}
        <li>
          {#if label}
            <button
              type="button"
              class="flex w-full gap-2 text-left text-stop-soft hover:underline"
              onclick={() => onreveal(notice)}
            >
              <span class="grow">{notice.text}</span>
              <span class="shrink-0 text-muted-foreground">→ {label}</span>
            </button>
          {:else}
            <span class="text-stop-soft">{notice.text}</span>
          {/if}
        </li>
      {/each}
    </ul>
  </details>
{/if}
