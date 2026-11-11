<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';

  import type { GapState } from '../../lib/api';
  import { fmtTime } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { cueById } from '../../lib/store.svelte';
  import CueButtons from './CueButtons.svelte';
  import NextItemButton from './NextItemButton.svelte';

  // A playlist between two items: nothing plays, but it still runs until the next item.
  let { gap }: { gap: GapState } = $props();

  const cue = $derived(cueById(gap.cue));
</script>

<div class={['flex items-center gap-2.5 border-b border-line px-4 py-3', gap.paused && 'bg-row-paused']}>
  <span class="font-mono font-semibold text-primary">{cue?.number}</span>
  <span class="min-w-0 grow truncate font-semibold">{cue?.title}</span>
  <Badge variant="outline" class="text-primary"
    >{t('active.item', { index: gap.next.index + 1, count: gap.next.count })}</Badge
  >
  <Badge variant="outline" class={gap.paused ? 'text-pause-soft' : 'text-muted-foreground'}>
    {gap.paused ? t('active.gap_paused') : t('active.gap_left')}
    <span class="font-mono">{fmtTime(gap.remaining)}</span>
  </Badge>
  <NextItemButton cue={gap.cue} number={cue?.number} enabled />
  <CueButtons cue={gap.cue} number={cue?.number} paused={gap.paused} />
</div>
