<script lang="ts">
  import SkipForward from '~icons/ph/skip-forward-bold';
  import { buttonVariants } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { api, type CueId } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';

  // Goes on to a playlist's next item; `enabled`: one can follow now (else the button stays, disabled).
  let { cue, number, enabled }: { cue: CueId; number: string | undefined; enabled: boolean } = $props();
</script>

<Tooltip
  tip={t('action.next_item')}
  class={buttonVariants({ class: 'gap-1 bg-transparent px-2 text-xs text-fg-2 hover:bg-hover' })}
  disabled={!enabled}
  aria-label={t('active.next_item_label', { number: number ?? '' })}
  onclick={() => api.action({ action: 'next_item', cue })}><SkipForward class="size-3.5" />{t('active.next')}</Tooltip
>
