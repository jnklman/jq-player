<script lang="ts">
  import Link from '~icons/ph/link-bold';
  import XMark from '~icons/ph/x-bold';
  import { Button, buttonVariants } from '$lib/components/ui/button';
  import * as Dialog from '$lib/components/ui/dialog';
  import { ScrollArea } from '$lib/components/ui/scroll-area';

  import { edit } from '../lib/commands';
  import { t } from '../lib/i18n/index.svelte';
  import type { MissingMediaFile } from '../lib/media';
  import { missingMediaFiles, pickAudio } from '../lib/media';
  import { dialogEscape, mode, ui } from '../lib/store.svelte';

  let { onclose }: { onclose: () => void } = $props();
  let busyKey = $state<string | null>(null);

  const missingFiles = $derived(ui.view ? missingMediaFiles(ui.view) : []);

  const canRelink = $derived(mode() === 'edit' && !ui.runtime?.searching);

  async function chooseFile(file: MissingMediaFile) {
    if (!ui.view || !canRelink || busyKey) return;
    busyKey = file.path;
    try {
      const picked = await pickAudio(ui.view, { multiple: false });
      if (!picked) return;
      const { paths, ...storage } = picked;
      await edit({ op: 'relink_file', old_path: file.path, path: paths[0], ...storage });
    } finally {
      busyKey = null;
    }
  }
</script>

<Dialog.Root
  bind:open={
    () => true,
    (open) => {
      if (!open) onclose();
    }
  }
>
  <Dialog.Content class="max-w-150" escapeKeydownBehavior={dialogEscape()}>
    <Dialog.Header>
      <Dialog.Title>{t('relink.title')}</Dialog.Title>
      <Dialog.Close class={buttonVariants({ variant: 'ghost', size: 'icon-sm' })} aria-label={t('common.close')}
        ><XMark /></Dialog.Close
      >
    </Dialog.Header>
    <Dialog.Description class="m-0 px-4 pt-3">
      {#if missingFiles.length}
        {t('relink.text')}
      {:else}
        {t('relink.all_done')}
      {/if}
    </Dialog.Description>
    <ScrollArea class="max-h-[60vh] min-h-0">
      <div class="flex flex-col gap-2 p-4">
        {#each missingFiles as file (file.key)}
          <div class="flex min-w-0 items-center gap-3 rounded-md border border-line p-3">
            <div class="min-w-0 grow">
              <p class="m-0 font-mono text-xs break-all text-muted-foreground">{file.path}</p>
              <p class="m-0 text-xs text-muted-foreground">
                {t('relink.used_in', { cues: file.uses.map((use) => use.label).join(', ') })}
              </p>
            </div>
            <Button
              variant="outline"
              class="shrink-0"
              disabled={!canRelink || busyKey !== null}
              onclick={() => chooseFile(file)}
            >
              <Link class="size-4" />
              {busyKey === file.path ? t('relink.linking') : t('relink.choose')}
            </Button>
          </div>
        {:else}
          <p class="m-0 p-2 text-muted-foreground">{t('relink.none')}</p>
        {/each}
      </div>
    </ScrollArea>
    <Dialog.Footer>
      <Button onclick={onclose}>{t('common.close')}</Button>
    </Dialog.Footer>
  </Dialog.Content>
</Dialog.Root>
