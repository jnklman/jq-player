<script lang="ts">
  import XMark from '~icons/ph/x-bold';
  import { Button } from '$lib/components/ui/button';

  import { t } from '../lib/i18n/index.svelte';
  import { dismiss, ui } from '../lib/store.svelte';
</script>

<!-- Above dialogs (z-50), and clickable while one is open (it turns off pointer events on the body):
     an error often comes from an action in a dialog. -->
<div
  class="pointer-events-auto fixed right-5 bottom-14 z-60 flex w-[min(420px,calc(100%-40px))] flex-col gap-2"
  aria-live="polite"
>
  {#each ui.notices as notice (notice.id)}
    <div
      class={[
        'flex items-start gap-2.5 rounded-[10px] border bg-panel-2 px-3 py-2.5 text-[13px] select-text',
        notice.level === 'warning' ? 'border-fade-line text-fade-soft' : 'border-stop-line text-stop-soft',
      ]}
    >
      <!-- Long paths break anywhere, so the close button stays inside. -->
      <span class="min-w-0 grow wrap-break-word">{notice.message}</span>
      <Button
        variant="ghost"
        size="icon-xs"
        class="shrink-0 text-current hover:text-current"
        aria-label={t('notices.dismiss')}
        onclick={() => dismiss(notice.id)}
      >
        <XMark />
      </Button>
    </div>
  {/each}
</div>
