<script lang="ts">
  import { Badge } from '$lib/components/ui/badge';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import { t } from '../../lib/i18n/index.svelte';

  // Marks settings that behave unlike the rest: saved with the show (`show`) or applied at once (`now`).
  let { kind }: { kind: 'show' | 'now' } = $props();
</script>

<Tooltip tip={kind === 'show' ? t('settings.badge_show_tip') : t('settings.badge_now_tip')} tabindex={0}>
  {#snippet child({ props })}
    <Badge
      {...props}
      variant="outline"
      class={[
        'tracking-wide uppercase',
        kind === 'show' ? 'border-primary/40 bg-primary/10 text-primary' : 'border-fade-line bg-fade/10 text-fade-soft',
      ]}>{kind === 'show' ? t('settings.badge_show') : t('settings.badge_now')}</Badge
    >
  {/snippet}
</Tooltip>
