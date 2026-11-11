<script lang="ts">
  import { Button } from '$lib/components/ui/button';

  import { api, type CueId } from '../../lib/api';
  import { t } from '../../lib/i18n/index.svelte';
  import { tw } from '../../lib/ui';

  // Pause or resume, fade (optional) and stop of one running cue.
  let {
    cue,
    number,
    paused,
    fade = false,
  }: { cue: CueId; number: string | undefined; paused: boolean; fade?: boolean } = $props();

  const small = tw`bg-transparent px-3 text-xs font-semibold hover:bg-hover`;
</script>

{#if fade}
  <Button
    class="{small} border-fade-line text-fade"
    aria-label={t('active.fade_cue', { number: number ?? '' })}
    onclick={() => api.action({ action: 'fade_cue', cue })}>{t('type.fade')}</Button
  >
{/if}
<Button
  class="{small} min-w-17 border-pause-line text-pause-soft"
  aria-label={t(paused ? 'active.resume_cue' : 'active.pause_cue', { number: number ?? '' })}
  onclick={() => api.action({ action: 'toggle_pause_cue', cue })}
  >{paused ? t('active.resume') : t('control.pause')}</Button
>
<Button
  class="{small} border-stop-line text-stop-soft"
  aria-label={t('active.stop_cue', { number: number ?? '' })}
  onclick={() => api.action({ action: 'stop_cue', cue })}>{t('type.stop')}</Button
>
