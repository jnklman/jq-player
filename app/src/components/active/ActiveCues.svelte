<script lang="ts">
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { t } from '../../lib/i18n/index.svelte';
  import { runningList } from '../../lib/running';
  import { stickToBottom } from '../../lib/stick';
  import { ui } from '../../lib/store.svelte';
  import { empty, hint } from '../../lib/ui';
  import PanelFrame from '../layout/PanelFrame.svelte';
  import FollowRow from './FollowRow.svelte';
  import GapRow from './GapRow.svelte';
  import PreWaitRow from './PreWaitRow.svelte';
  import RunningRow from './RunningRow.svelte';

  // Everything that runs: playing cues, playlists between items, pre-waits,
  // and the cues their chains start next under them.
  const list = $derived(runningList(ui.runtime));
  const counts = $derived({
    active: ui.runtime?.active.length ?? 0,
    gaps: ui.runtime?.gaps.length ?? 0,
    preWaits: ui.runtime?.waiting.length ?? 0,
    following: ui.runtime?.following.length ?? 0,
  });
  const idle = $derived(!list.active.length && !list.gaps.length && !list.preWaits.length && !list.following.length);

  let viewport = $state<HTMLElement | null>(null);
  $effect(() => (viewport ? stickToBottom(viewport) : undefined));
</script>

{#snippet count()}
  <span class={hint}
    >{t('active.count_active', { count: counts.active })}{counts.gaps
      ? ` · ${t('active.count_gaps', { count: counts.gaps })}`
      : ''}{counts.preWaits ? ` · ${t('active.count_pre_wait', { count: counts.preWaits })}` : ''}{counts.following
      ? ` · ${t('active.count_following', { count: counts.following })}`
      : ''}</span
  >
{/snippet}

<PanelFrame title={t('active.title')} actions={count}>
  <ScrollArea class="flex-1" bind:viewportRef={viewport}>
    {#each list.active as group (group.state.cue)}
      <RunningRow running={group.state} />
      {#each group.preWaits as wait (wait.cue)}<PreWaitRow {wait} nested />{/each}
      {#each group.following as follow (follow.cue)}<FollowRow {follow} />{/each}
    {/each}
    {#each list.gaps as group (group.state.cue)}
      <GapRow gap={group.state} />
      {#each group.preWaits as wait (wait.cue)}<PreWaitRow {wait} nested />{/each}
      {#each group.following as follow (follow.cue)}<FollowRow {follow} />{/each}
    {/each}
    {#if idle}
      <div class={empty}>{t('active.idle')}</div>
    {/if}
    {#each list.preWaits as wait (wait.cue)}<PreWaitRow {wait} nested={false} />{/each}
    {#each list.following as follow (follow.cue)}<FollowRow {follow} />{/each}
  </ScrollArea>
</PanelFrame>
