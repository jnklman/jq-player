<script lang="ts">
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { t } from '../lib/i18n/index.svelte';
  import { cueById, cues, ui } from '../lib/store.svelte';
  import { empty, tw } from '../lib/ui';
  import PanelFrame from './layout/PanelFrame.svelte';
  import RichText from './RichText.svelte';

  // The cue at the playhead, read-only and in full: unlike the transport and
  // the cue list, nothing here is truncated, so long titles, script references
  // and detail texts can be read during the show.
  const cue = $derived(cueById(ui.runtime?.playhead));

  const caption = tw`text-[11px] font-semibold tracking-[0.8px] text-muted-foreground uppercase`;
</script>

<PanelFrame title={t('playhead.title')}>
  <ScrollArea class="flex-1 select-text">
    {#if cue}
      <div class="flex flex-col gap-4 px-4 py-3 wrap-break-word">
        <div class="flex items-baseline gap-3">
          <span class="shrink-0 font-mono text-2xl font-semibold text-primary">{cue.number}</span>
          <span class={['min-w-0 text-xl font-semibold', !cue.title && 'text-muted-foreground']}>
            {cue.title || t('group.untitled')}
          </span>
        </div>
        {#if cue.script_ref}
          <div class="flex flex-col gap-1">
            <span class={caption}>{t('playhead.script_ref')}</span>
            <span class="text-[15px] text-fg-2">{cue.script_ref}</span>
          </div>
        {/if}
        {#if cue.notes}
          <div class="flex flex-col gap-1">
            <span class={caption}>{t('inspector.tab.notes')}</span>
            <p class="m-0 text-[15px] whitespace-pre-wrap"><RichText text={cue.notes} /></p>
          </div>
        {/if}
      </div>
    {:else}
      <div class={empty}>{cues().length ? t('playhead.end_of_list') : t('playhead.no_cues')}</div>
    {/if}
  </ScrollArea>
</PanelFrame>
