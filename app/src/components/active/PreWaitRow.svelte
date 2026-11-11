<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';

  import { fmtTime } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import type { PreWait } from '../../lib/running';
  import { cueById } from '../../lib/store.svelte';
  import CueButtons from './CueButtons.svelte';

  // A cue counting down its pre-wait; it can be paused or stopped like a running one.
  let { wait, nested }: { wait: PreWait; nested: boolean } = $props();

  const cue = $derived(cueById(wait.cue));
</script>

<div
  class={[
    'flex items-center gap-2.5 border-b border-line py-2 pr-4',
    nested ? 'pl-8' : 'pl-4',
    wait.paused && 'bg-row-paused',
  ]}
>
  <span class="font-mono font-semibold text-primary">{cue?.number}</span>
  <span class="min-w-0 grow truncate font-semibold">{cue?.title}</span>
  <Badge variant="outline" class={wait.paused ? 'text-pause-soft' : 'text-primary'}>
    {wait.paused ? t('active.pre_wait_paused') : t('active.pre_wait_left')}
    <span class="font-mono">{fmtTime(wait.remaining)}</span>
  </Badge>
  <CueButtons cue={wait.cue} number={cue?.number} paused={wait.paused} />
</div>
