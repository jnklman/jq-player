<script lang="ts">
  import ArrowClockwise from '~icons/ph/arrow-clockwise-bold';
  import { Button } from '$lib/components/ui/button';
  import { Tooltip } from '$lib/components/ui/tooltip';

  import type { AudioSetup } from '../lib/audioSetup.svelte';
  import { t } from '../lib/i18n/index.svelte';

  // Next to a device select: lists the devices anew (one plugged in since).
  let { setup, disabled = false }: { setup: AudioSetup; disabled?: boolean } = $props();
</script>

<Tooltip tip={t('device.reload_tip')}>
  {#snippet child({ props })}
    <Button
      {...props}
      variant="ghost"
      size="icon-sm"
      class="shrink-0"
      aria-label={t('device.reload')}
      disabled={disabled || setup.loading}
      onclick={() => setup.reload()}
    >
      <ArrowClockwise />
    </Button>
  {/snippet}
</Tooltip>
