<script lang="ts">
  import LinkSimpleBreak from '~icons/ph/link-simple-break-bold';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';
  import { getLinked } from './context';

  // Marks a field that a linked cue keeps for itself: a label text followed by the mark.
  let { label }: { label: string } = $props();
  const linked = getLinked();
</script>

<span class="flex items-center gap-1">
  {label}
  {#if linked()}
    <Tooltip tip={t('inspector.own_mark')}>
      {#snippet child({ props })}
        <span {...props} class="flex text-dim"><LinkSimpleBreak class="size-3" /></span>
      {/snippet}
    </Tooltip>
  {/if}
</span>
