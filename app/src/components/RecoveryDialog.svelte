<script lang="ts">
  import * as AlertDialog from '$lib/components/ui/alert-dialog';
  import { Button } from '$lib/components/ui/button';

  import { api } from '../lib/api';
  import { i18n, t } from '../lib/i18n/index.svelte';
  import { attempt } from '../lib/store.svelte';

  let { savedAt }: { savedAt: number } = $props();

  let busy = $state(false);
  const when = $derived(new Date(savedAt).toLocaleString(i18n.locale, { dateStyle: 'medium', timeStyle: 'short' }));

  async function decide(apply: boolean) {
    busy = true;
    await attempt(api.recoverAutosave(apply));
    busy = false;
  }
</script>

<AlertDialog.Root open>
  <AlertDialog.Content escapeKeydownBehavior="ignore">
    <AlertDialog.Header>
      <AlertDialog.Title>{t('recovery.title')}</AlertDialog.Title>
    </AlertDialog.Header>
    <AlertDialog.Description>
      {t('recovery.text', { when, undo: `${t('key.ctrl')}+Z` })}
    </AlertDialog.Description>
    <AlertDialog.Footer>
      <Button variant="destructive" disabled={busy} onclick={() => decide(false)}>{t('unsaved.discard')}</Button>
      <Button variant="default" disabled={busy} onclick={() => decide(true)}>{t('recovery.restore')}</Button>
    </AlertDialog.Footer>
  </AlertDialog.Content>
</AlertDialog.Root>
