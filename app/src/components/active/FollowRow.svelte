<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { buttonVariants } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type FollowState } from '../../lib/api';
  import { fmtTime } from '../../lib/format';
  import { t } from '../../lib/i18n/index.svelte';
  import { cueById } from '../../lib/store.svelte';

  // A cue a chain starts next, dimmed, with the time until then; its button breaks the chain off here.
  let { follow }: { follow: FollowState } = $props();

  const cue = $derived(cueById(follow.cue));
  const after = $derived(cueById(follow.after));
</script>

<div class="flex items-center gap-2.5 border-b border-line py-2 pr-4 pl-8 opacity-70">
  <span class="font-mono font-semibold text-muted-foreground">{cue?.number}</span>
  <span class="min-w-0 grow truncate text-fg-2">{cue?.title}</span>
  <span class="shrink-0 text-xs text-muted-foreground">{t('active.after', { number: after?.number ?? '–' })}</span>
  <Badge variant="outline" class={follow.paused ? 'text-pause-soft' : 'text-muted-foreground'}>
    {follow.paused ? t('active.paused_lower') : t('active.follows')}
    {#if follow.remaining != null}{t('active.in')} <span class="font-mono">{fmtTime(follow.remaining)}</span>{/if}
  </Badge>
  <Tooltip
    tip={t('active.halt_tip', { number: cue?.number ?? '', after: after?.number ?? '' })}
    class={buttonVariants({ class: 'h-6 bg-transparent px-2 text-xs text-fg-2 hover:bg-hover' })}
    aria-label={t('active.halt_label', { number: cue?.number ?? '' })}
    onclick={() => api.action({ action: 'cancel_follow', cue: follow.after })}
  >
    {t('active.halt')}
  </Tooltip>
</div>
