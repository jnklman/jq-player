<script lang="ts">
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';

  import { t } from '../lib/i18n/index.svelte';
  import { type Leaving, notify } from '../lib/store.svelte';

  let { leaving, onclose }: { leaving: Leaving; onclose: () => void } = $props();

  const check = $derived(leaving.check);
  let cancel = $state<HTMLElement | null>(null);
  let busy = $state(false);

  const reasons = $derived(
    [
      check.show_mode && t('leave.show_mode'),
      check.running > 0 && t('leave.running', { count: check.running }),
      check.imports > 0 && t('leave.imports', { count: check.imports }),
      check.dirty && t('leave.dirty'),
    ].filter((reason) => typeof reason === 'string'),
  );

  async function leave(save: boolean) {
    busy = true;
    try {
      await leaving.proceed(save);
      onclose();
    } catch (error) {
      // E.g. saving failed: the show stays open.
      notify({ level: 'error', message: String(error) });
    } finally {
      busy = false;
    }
  }
</script>

<AlertDialog.Root open>
  <!-- Enter must not leave by accident: the focus starts on "Abbrechen". -->
  <AlertDialog.Content
    escapeKeydownBehavior="ignore"
    onOpenAutoFocus={(e) => {
      e.preventDefault();
      cancel?.focus();
    }}
  >
    <AlertDialog.Header>
      <AlertDialog.Title>{leaving.kind === 'quit' ? t('leave.title_quit') : t('leave.title_switch')}</AlertDialog.Title>
    </AlertDialog.Header>
    <AlertDialog.Description>
      {#snippet child({ props })}
        <ul {...props} class="m-0 flex list-none flex-col gap-1.5 p-4 text-[13px] text-fg-2">
          {#each reasons as reason (reason)}
            <li>{reason}</li>
          {/each}
        </ul>
      {/snippet}
    </AlertDialog.Description>
    <AlertDialog.Footer>
      <Button bind:ref={cancel} disabled={busy} onclick={onclose}>{t('common.cancel')}</Button>
      {#if check.dirty}
        <Button variant="destructive" disabled={busy} onclick={() => leave(false)}>
          {leaving.kind === 'quit' ? t('leave.discard_quit') : t('leave.discard_switch')}
        </Button>
        <Button variant="default" disabled={busy} onclick={() => leave(true)}>
          {leaving.kind === 'quit' ? t('leave.save_quit') : t('leave.save_switch')}
        </Button>
      {:else}
        <Button variant="destructive" disabled={busy} onclick={() => leave(false)}>
          {leaving.kind === 'quit' ? t('leave.quit') : t('leave.switch')}
        </Button>
      {/if}
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
